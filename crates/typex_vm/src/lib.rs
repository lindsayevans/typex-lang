use std::collections::HashMap;
use typex_ast::*;
use typex_parser;
use typex_runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value, format_string};
use typex_span;
use typex_std::StdRegistry;

// ------------------------------------------------------------------
// Control flow signals
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Signal {
    Return(Value),
    Panic(String),
}

// ------------------------------------------------------------------
// Environment
// ------------------------------------------------------------------

#[derive(Debug)]
pub struct Env {
    scopes: Vec<HashMap<String, Value>>,
}

impl Env {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: String, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, value);
        }
    }

    fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }

    fn set(&mut self, name: &str, value: Value) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return true;
            }
        }
        false
    }
}

// ------------------------------------------------------------------
// VM
// ------------------------------------------------------------------

pub struct Vm {
    env: Env,
    functions: HashMap<String, FunctionDef>,
    std: StdRegistry,
    imports: HashMap<String, String>, // name -> module e.g. "readFile" -> "tx:fs"
    source_path: Option<std::path::PathBuf>,
}

impl Vm {
    pub fn new() -> Self {
        Self {
            env: Env::new(),
            functions: HashMap::new(),
            std: StdRegistry::new(),
            imports: HashMap::new(),
            source_path: None,
        }
    }

    pub fn with_path(path: impl Into<std::path::PathBuf>) -> Self {
        let mut vm = Self::new();
        vm.source_path = Some(path.into());
        vm
    }

    // ------------------------------------------------------------------
    // Module
    // ------------------------------------------------------------------

    pub fn run_module(&mut self, module: &Module, argv: Vec<String>) -> RuntimeResult<i64> {
        // process imports first
        for item in &module.items {
            if let Item::Import(i) = item {
                if self.std.has_module(&i.from) {
                    // stdlib import
                    for name in &i.names {
                        self.imports.insert(name.name.clone(), i.from.clone());
                    }
                } else if i.from.starts_with("./") || i.from.starts_with("../") {
                    // file import
                    self.load_file_import(i)?;
                }
            }
        }

        // hoist all function definitions
        for item in &module.items {
            if let Item::Function(f) = item {
                self.functions.insert(f.name.name.clone(), f.clone());
            }
        }

        // run top-level const/let
        for item in &module.items {
            match item {
                Item::Const(c) => {
                    let val = self.eval_expr(&c.value)?;
                    self.env.define(c.name.name.clone(), val);
                }
                Item::Let(l) => {
                    let val = if let Some(expr) = &l.value {
                        self.eval_expr(expr)?
                    } else {
                        Value::Null
                    };
                    self.env.define(l.name.name.clone(), val);
                }
                _ => {}
            }
        }

        // call main if present
        if let Some(main_fn) = self.functions.get("main").cloned() {
            let args = if main_fn.params.is_empty() {
                vec![]
            } else {
                vec![Value::Array(
                    argv.iter().map(|s| Value::Str(s.clone())).collect(),
                )]
            };
            match self.call_function(&main_fn, args)? {
                Value::Int(code) => return Ok(code),
                Value::Void => return Ok(0),
                _ => return Ok(0),
            }
        }

        Ok(0)
    }

    fn load_file_import(&mut self, import: &Import) -> RuntimeResult<()> {
        // resolve path relative to current source file
        let import_path = if let Some(source) = &self.source_path {
            let dir = source.parent().unwrap_or(std::path::Path::new("."));
            dir.join(&import.from)
        } else {
            std::path::PathBuf::from(&import.from)
        };

        // read source
        let src = std::fs::read_to_string(&import_path).map_err(|e| {
            RuntimeError::new(format!("failed to load module '{}': {}", import.from, e))
        })?;

        // parse
        let mut sm = typex_span::SourceMap::new();
        let file = sm.add(import_path.to_string_lossy().to_string(), src.clone());
        let (module, diags) = typex_parser::parse(&src, file);

        if diags.iter().any(|d| d.level == typex_span::Level::Error) {
            return Err(RuntimeError::new(format!(
                "parse errors in module '{}'",
                import.from
            )));
        }

        // extract exported functions that are requested
        let requested: std::collections::HashSet<String> =
            import.names.iter().map(|n| n.name.clone()).collect();

        for item in &module.items {
            if let Item::Function(f) = item {
                if f.exported && requested.contains(&f.name.name) {
                    self.functions.insert(f.name.name.clone(), f.clone());
                }
            }
        }

        Ok(())
    }

    // ------------------------------------------------------------------
    // Function calls
    // ------------------------------------------------------------------

    fn call_function(&mut self, f: &FunctionDef, args: Vec<Value>) -> RuntimeResult<Value> {
        // save current env and start fresh for this call
        let saved_env = std::mem::replace(&mut self.env, Env::new());

        // push scope for params
        self.env.push();
        for (param, val) in f.params.iter().zip(args.into_iter()) {
            self.env.define(param.name.name.clone(), val);
        }

        // execute body directly without extra block scope
        let mut result = Ok(Value::Void);
        for stmt in &f.body.stmts {
            match self.exec_stmt(stmt) {
                Ok(_) => {}
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }

        self.env.pop();

        // restore previous env
        self.env = saved_env;

        match result {
            Ok(_) => Ok(Value::Void),
            Err(e) if is_return(&e) => Ok(extract_return(e)),
            Err(e) if is_panic(&e) => Err(e),
            Err(e) => Err(e),
        }
    }

    fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> RuntimeResult<Value> {
        match name {
            "print" | "println" => {
                let output = if args.is_empty() {
                    String::new()
                } else if let Value::Str(template) = &args[0] {
                    format_string(template, &args[1..])
                } else {
                    args[0].to_string()
                };
                if name == "println" {
                    println!("{}", output);
                } else {
                    print!("{}", output);
                }
                Ok(Value::Void)
            }
            "panic" => {
                let msg = if args.is_empty() {
                    "explicit panic".to_string()
                } else if let Value::Str(template) = &args[0] {
                    format_string(template, &args[1..])
                } else {
                    args[0].to_string()
                };
                Err(RuntimeError::new_panic(format!("__panic__{}", msg)))
            }
            _ => Err(RuntimeError::new(format!("unknown builtin '{}'", name))),
        }
    }

    // ------------------------------------------------------------------
    // Blocks & statements
    // ------------------------------------------------------------------

    fn exec_block(&mut self, block: &Block) -> RuntimeResult<Value> {
        self.env.push();
        for stmt in &block.stmts {
            self.exec_stmt(stmt)?;
        }
        self.env.pop();
        Ok(Value::Void)
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> RuntimeResult<Value> {
        match stmt {
            Stmt::Let(l) => {
                let val = if let Some(expr) = &l.value {
                    self.eval_expr(expr)?
                } else {
                    Value::Null
                };
                self.env.define(l.name.name.clone(), val);
                Ok(Value::Void)
            }
            Stmt::Const(c) => {
                let val = self.eval_expr(&c.value)?;
                self.env.define(c.name.name.clone(), val);
                Ok(Value::Void)
            }
            Stmt::Return(expr, _) => {
                let val = if let Some(e) = expr {
                    self.eval_expr(e)?
                } else {
                    Value::Void
                };
                Err(RuntimeError::new_return(val))
            }
            Stmt::Expr(e) => {
                self.eval_expr(e)?;
                Ok(Value::Void)
            }
            Stmt::If(i) => self.exec_if(i),
            Stmt::Switch(s) => self.exec_switch(s),
            Stmt::For(f) => self.exec_for(f),
            Stmt::Match(m) => {
                self.eval_match(m)?;
                Ok(Value::Void)
            }
        }
    }

    fn exec_if(&mut self, i: &IfStmt) -> RuntimeResult<Value> {
        let cond = self.eval_expr(&i.condition)?;
        if cond.is_truthy() {
            return self.exec_block(&i.then_block);
        }
        for (cond_expr, block) in &i.else_if {
            let cond = self.eval_expr(cond_expr)?;
            if cond.is_truthy() {
                return self.exec_block(block);
            }
        }
        if let Some(else_block) = &i.else_block {
            return self.exec_block(else_block);
        }
        Ok(Value::Void)
    }

    fn exec_switch(&mut self, s: &SwitchStmt) -> RuntimeResult<Value> {
        let val = self.eval_expr(&s.value)?;
        for case in &s.cases {
            let case_val = self.eval_expr(&case.value)?;
            if values_equal(&val, &case_val) {
                return self.exec_block(&case.body);
            }
        }
        if let Some(default) = &s.default {
            return self.exec_block(default);
        }
        Ok(Value::Void)
    }

    fn exec_for(&mut self, f: &ForStmt) -> RuntimeResult<Value> {
        match f {
            ForStmt::Array {
                index,
                value,
                iter,
                body,
                ..
            } => {
                let iter_val = self.eval_expr(iter)?;
                let elements = match iter_val {
                    Value::Array(els) => els,
                    other => {
                        return Err(RuntimeError::new(format!(
                            "expected Array, got {}",
                            other.type_name()
                        )));
                    }
                };
                for (i, elem) in elements.into_iter().enumerate() {
                    self.env.push();
                    if let Some(idx) = index {
                        self.env.define(idx.name.clone(), Value::Uint(i as u64));
                    }
                    if let Some(val) = value {
                        self.env.define(val.name.clone(), elem);
                    }
                    match self.exec_block(body) {
                        Ok(_) => {}
                        Err(e) if is_return(&e) => {
                            self.env.pop();
                            return Err(e);
                        }
                        Err(e) => {
                            self.env.pop();
                            return Err(e);
                        }
                    }
                    self.env.pop();
                }
                Ok(Value::Void)
            }
            ForStmt::Object {
                key,
                value,
                iter,
                body,
                ..
            } => {
                let iter_val = self.eval_expr(iter)?;
                let fields = match iter_val {
                    Value::Record(f) => f,
                    other => {
                        return Err(RuntimeError::new(format!(
                            "expected Record, got {}",
                            other.type_name()
                        )));
                    }
                };
                for (k, v) in fields.into_iter() {
                    self.env.push();
                    if let Some(key_ident) = key {
                        self.env.define(key_ident.name.clone(), Value::Str(k));
                    }
                    if let Some(val_ident) = value {
                        self.env.define(val_ident.name.clone(), v);
                    }
                    match self.exec_block(body) {
                        Ok(_) => {}
                        Err(e) if is_return(&e) => {
                            self.env.pop();
                            return Err(e);
                        }
                        Err(e) => {
                            self.env.pop();
                            return Err(e);
                        }
                    }
                    self.env.pop();
                }
                Ok(Value::Void)
            }
            ForStmt::Str {
                index,
                offset,
                value,
                iter,
                body,
                ..
            } => {
                let iter_val = self.eval_expr(iter)?;
                let s = match iter_val {
                    Value::Str(s) => s,
                    other => {
                        return Err(RuntimeError::new(format!(
                            "expected string, got {}",
                            other.type_name()
                        )));
                    }
                };
                let mut byte_offset = 0u64;
                for (i, ch) in s.chars().enumerate() {
                    self.env.push();
                    if let Some(idx) = index {
                        self.env.define(idx.name.clone(), Value::Uint(i as u64));
                    }
                    if let Some(off) = offset {
                        self.env.define(off.name.clone(), Value::Uint(byte_offset));
                    }
                    if let Some(val) = value {
                        self.env
                            .define(val.name.clone(), Value::Str(ch.to_string()));
                    }
                    byte_offset += ch.len_utf8() as u64;
                    match self.exec_block(body) {
                        Ok(_) => {}
                        Err(e) if is_return(&e) => {
                            self.env.pop();
                            return Err(e);
                        }
                        Err(e) => {
                            self.env.pop();
                            return Err(e);
                        }
                    }
                    self.env.pop();
                }
                Ok(Value::Void)
            }
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn eval_expr(&mut self, expr: &Expr) -> RuntimeResult<Value> {
        match expr {
            Expr::Lit(lit) => self.eval_lit(lit),
            Expr::Ident(i) => self.eval_ident(i),
            Expr::BinOp(l, op, r, _) => self.eval_binop(l, op, r),
            Expr::UnaryOp(op, e, _) => self.eval_unary(op, e),
            Expr::Call(f, args, _) => self.eval_call(f, args),
            Expr::Field(e, field, _) => self.eval_field(e, field),
            Expr::Index(e, i, _) => self.eval_index(e, i),
            Expr::Ternary(c, t, e, _) => self.eval_ternary(c, t, e),
            Expr::Match(m) => self.eval_match(m),
            Expr::Array(els, _) => self.eval_array(els),
            Expr::Record(fields, _) => self.eval_record(fields),
            Expr::Arrow(_) => Ok(Value::Void),
            Expr::Destructure(_) => Ok(Value::Void),
            Expr::Assign(lhs, rhs, _) => {
                let val = self.eval_expr(rhs)?;
                match lhs.as_ref() {
                    Expr::Ident(i) => {
                        if !self.env.set(&i.name, val.clone()) {
                            return Err(RuntimeError::new(format!(
                                "undefined variable '{}'",
                                i.name
                            )));
                        }
                        Ok(val)
                    }
                    _ => Err(RuntimeError::new("invalid assignment target")),
                }
            }
        }
    }

    fn eval_lit(&self, lit: &Lit) -> RuntimeResult<Value> {
        Ok(match lit {
            Lit::Int(n, _) => Value::Int(*n),
            Lit::Float(f, _) => Value::Float(*f),
            Lit::Bool(b, _) => Value::Bool(*b),
            Lit::Str(s, _) => Value::Str(s.clone()),
            Lit::Char(c, _) => Value::Char(*c),
            Lit::Null(_) => Value::Null,
        })
    }

    fn eval_ident(&self, i: &Ident) -> RuntimeResult<Value> {
        match self.env.get(&i.name) {
            Some(val) => Ok(val.clone()),
            None => Err(RuntimeError::new(format!(
                "undefined variable '{}'",
                i.name
            ))),
        }
    }

    fn eval_binop(&mut self, l: &Expr, op: &BinOp, r: &Expr) -> RuntimeResult<Value> {
        let lv = self.eval_expr(l)?;
        let rv = self.eval_expr(r)?;
        match op {
            BinOp::Add => match (&lv, &rv) {
                (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
                (Value::Str(a), Value::Int(b)) => Ok(Value::Str(format!("{}{}", a, b))),
                (Value::Str(a), Value::Float(b)) => Ok(Value::Str(format!("{}{}", a, b))),
                _ => numeric_binop(
                    &lv,
                    &rv,
                    |a, b| Value::Int(a + b),
                    |a, b| Value::Float(a + b),
                ),
            },
            BinOp::Sub => numeric_binop(
                &lv,
                &rv,
                |a, b| Value::Int(a - b),
                |a, b| Value::Float(a - b),
            ),
            BinOp::Mul => numeric_binop(
                &lv,
                &rv,
                |a, b| Value::Int(a * b),
                |a, b| Value::Float(a * b),
            ),
            BinOp::Div => {
                if matches!(rv, Value::Int(0)) {
                    return Err(RuntimeError::new("division by zero"));
                }
                numeric_binop(
                    &lv,
                    &rv,
                    |a, b| Value::Int(a / b),
                    |a, b| Value::Float(a / b),
                )
            }
            BinOp::Mod => numeric_binop(
                &lv,
                &rv,
                |a, b| Value::Int(a % b),
                |a, b| Value::Float(a % b),
            ),
            BinOp::Eq => Ok(Value::Bool(values_equal(&lv, &rv))),
            BinOp::NotEq => Ok(Value::Bool(!values_equal(&lv, &rv))),
            BinOp::Lt => numeric_cmp(&lv, &rv, |a, b| a < b, |a, b| a < b),
            BinOp::Lte => numeric_cmp(&lv, &rv, |a, b| a <= b, |a, b| a <= b),
            BinOp::Gt => numeric_cmp(&lv, &rv, |a, b| a > b, |a, b| a > b),
            BinOp::Gte => numeric_cmp(&lv, &rv, |a, b| a >= b, |a, b| a >= b),
            BinOp::And => Ok(Value::Bool(lv.is_truthy() && rv.is_truthy())),
            BinOp::Or => Ok(Value::Bool(lv.is_truthy() || rv.is_truthy())),
        }
    }

    fn eval_unary(&mut self, op: &UnaryOp, e: &Expr) -> RuntimeResult<Value> {
        let val = self.eval_expr(e)?;
        match op {
            UnaryOp::Neg => match val {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                other => Err(RuntimeError::new(format!(
                    "cannot negate {}",
                    other.type_name()
                ))),
            },
            UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
        }
    }

    fn eval_call(&mut self, f: &Expr, args: &[Expr]) -> RuntimeResult<Value> {
        let arg_vals: Vec<Value> = args
            .iter()
            .map(|a| self.eval_expr(a))
            .collect::<RuntimeResult<Vec<_>>>()?;

        match f {
            Expr::Ident(i) => {
                // builtins
                if matches!(i.name.as_str(), "print" | "println" | "panic") {
                    return self.call_builtin(&i.name, arg_vals);
                }
                // Ok/Err constructors
                if i.name == "Ok" {
                    return Ok(Value::Ok(Box::new(
                        arg_vals.into_iter().next().unwrap_or(Value::Void),
                    )));
                }
                if i.name == "Err" {
                    return Ok(Value::Err(Box::new(
                        arg_vals.into_iter().next().unwrap_or(Value::Void),
                    )));
                }
                // stdlib imports
                if let Some(module) = self.imports.get(&i.name).cloned() {
                    if let Some(f) = self.std.get_fn(&module, &i.name) {
                        return f(arg_vals);
                    }
                }
                // user function
                if let Some(func) = self.functions.get(&i.name).cloned() {
                    return self.call_function(&func, arg_vals);
                }
                Err(RuntimeError::new(format!(
                    "undefined function '{}'",
                    i.name
                )))
            }
            Expr::Field(obj, method, _) => {
                let obj_val = self.eval_expr(obj)?;
                self.call_method(obj_val, &method.name, arg_vals)
            }
            _ => Err(RuntimeError::new("cannot call non-function")),
        }
    }

    fn call_method(&mut self, obj: Value, method: &str, args: Vec<Value>) -> RuntimeResult<Value> {
        match (&obj, method) {
            (Value::Array(_els), "filter") => {
                // array.filter not yet implemented - requires closures
                Err(RuntimeError::new("Array.filter not yet implemented"))
            }
            (Value::Array(_els), "map") => Err(RuntimeError::new("Array.map not yet implemented")),
            (Value::Array(els), "length") => Ok(Value::Uint(els.len() as u64)),
            (Value::Array(a), "equals") => {
                if let Some(Value::Array(b)) = args.first() {
                    Ok(Value::Bool(a == b))
                } else {
                    Ok(Value::Bool(false))
                }
            }
            (Value::Str(s), "length") => Ok(Value::Uint(s.chars().count() as u64)),
            _ => Err(RuntimeError::new(format!(
                "no method '{}' on {}",
                method,
                obj.type_name()
            ))),
        }
    }

    fn eval_field(&mut self, e: &Expr, field: &Ident) -> RuntimeResult<Value> {
        let val = self.eval_expr(e)?;
        match &val {
            Value::Record(fields) => fields
                .get(&field.name)
                .cloned()
                .ok_or_else(|| RuntimeError::new(format!("no field '{}'", field.name))),
            Value::Array(els) if field.name == "length" => Ok(Value::Uint(els.len() as u64)),
            Value::Str(s) if field.name == "length" => Ok(Value::Uint(s.chars().count() as u64)),
            Value::Ok(inner) if field.name == "value" => Ok(*inner.clone()),
            Value::Err(inner) if field.name == "message" => Ok(*inner.clone()),
            _ => Err(RuntimeError::new(format!(
                "no field '{}' on {}",
                field.name,
                val.type_name()
            ))),
        }
    }

    fn eval_index(&mut self, e: &Expr, i: &Expr) -> RuntimeResult<Value> {
        let val = self.eval_expr(e)?;
        let idx = self.eval_expr(i)?;
        match (&val, &idx) {
            (Value::Array(els), Value::Int(i)) => els
                .get(*i as usize)
                .cloned()
                .ok_or_else(|| RuntimeError::new(format!("index {} out of bounds", i))),
            (Value::Array(els), Value::Uint(i)) => els
                .get(*i as usize)
                .cloned()
                .ok_or_else(|| RuntimeError::new(format!("index {} out of bounds", i))),
            (Value::Record(fields), Value::Str(key)) => fields
                .get(key)
                .cloned()
                .ok_or_else(|| RuntimeError::new(format!("no key '{}'", key))),
            _ => Err(RuntimeError::new(format!(
                "cannot index {} with {}",
                val.type_name(),
                idx.type_name()
            ))),
        }
    }

    fn eval_ternary(&mut self, cond: &Expr, then: &Expr, else_: &Expr) -> RuntimeResult<Value> {
        let cond_val = self.eval_expr(cond)?;
        if cond_val.is_truthy() {
            self.eval_expr(then)
        } else {
            self.eval_expr(else_)
        }
    }

    fn eval_match(&mut self, m: &MatchExpr) -> RuntimeResult<Value> {
        let val = self.eval_expr(&m.value)?;

        for arm in &m.arms {
            if let Some(bindings) = match_pattern(&arm.pattern, &val) {
                self.env.push();
                for (name, value) in bindings {
                    self.env.define(name, value);
                }
                let result = self.eval_expr(&arm.body)?;
                self.env.pop();
                return Ok(result);
            }
        }
        Err(RuntimeError::new("non-exhaustive match"))
    }

    fn eval_array(&mut self, els: &[Expr]) -> RuntimeResult<Value> {
        let vals: Vec<Value> = els
            .iter()
            .map(|e| self.eval_expr(e))
            .collect::<RuntimeResult<Vec<_>>>()?;
        Ok(Value::Array(vals))
    }

    fn eval_record(&mut self, fields: &[(Ident, Expr)]) -> RuntimeResult<Value> {
        let mut map = HashMap::new();
        for (key, val) in fields {
            map.insert(key.name.clone(), self.eval_expr(val)?);
        }
        Ok(Value::Record(map))
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------
// Pattern matching
// ------------------------------------------------------------------

fn match_pattern(pattern: &Pattern, val: &Value) -> Option<Vec<(String, Value)>> {
    match pattern {
        Pattern::Ok(binding) => {
            if let Value::Ok(inner) = val {
                Some(vec![(binding.name.clone(), *inner.clone())])
            } else {
                None
            }
        }
        Pattern::Err(binding) => {
            if let Value::Err(inner) = val {
                Some(vec![(binding.name.clone(), *inner.clone())])
            } else {
                None
            }
        }
        Pattern::Wildcard => Some(vec![]),
        Pattern::EnumVariant(name, binding) => match val {
            Value::Str(s) if s == &name.name => Some(vec![]),
            Value::Int(n) if name.name == n.to_string() => Some(vec![]),
            _ => {
                if let Some(b) = binding {
                    Some(vec![(b.name.clone(), val.clone())])
                } else {
                    None
                }
            }
        },
    }
}

// ------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------

fn numeric_binop(
    l: &Value,
    r: &Value,
    int_op: impl Fn(i64, i64) -> Value,
    float_op: impl Fn(f64, f64) -> Value,
) -> RuntimeResult<Value> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(int_op(*a, *b)),
        (Value::Float(a), Value::Float(b)) => Ok(float_op(*a, *b)),
        (Value::Int(a), Value::Float(b)) => Ok(float_op(*a as f64, *b)),
        (Value::Float(a), Value::Int(b)) => Ok(float_op(*a, *b as f64)),
        (Value::Uint(a), Value::Uint(b)) => Ok(Value::Uint((*a).wrapping_add(*b))),
        _ => Err(RuntimeError::new(format!(
            "cannot apply numeric op to {} and {}",
            l.type_name(),
            r.type_name()
        ))),
    }
}

fn numeric_cmp(
    l: &Value,
    r: &Value,
    int_cmp: impl Fn(i64, i64) -> bool,
    float_cmp: impl Fn(f64, f64) -> bool,
) -> RuntimeResult<Value> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Bool(int_cmp(*a, *b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(float_cmp(*a, *b))),
        (Value::Int(a), Value::Float(b)) => Ok(Value::Bool(float_cmp(*a as f64, *b))),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Bool(float_cmp(*a, *b as f64))),
        (Value::Uint(a), Value::Uint(b)) => Ok(Value::Bool(int_cmp(*a as i64, *b as i64))),
        _ => Err(RuntimeError::new(format!(
            "cannot compare {} and {}",
            l.type_name(),
            r.type_name()
        ))),
    }
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Uint(a), Value::Uint(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Char(a), Value::Char(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}

fn is_return(e: &RuntimeError) -> bool {
    e.is_return()
}

fn is_panic(e: &RuntimeError) -> bool {
    e.is_panic()
}

fn extract_return(e: RuntimeError) -> Value {
    match e.kind {
        RuntimeErrorKind::Return(v) => v,
        _ => Value::Void,
    }
}

// ------------------------------------------------------------------
// Public API
// ------------------------------------------------------------------

pub fn run(module: &Module, argv: Vec<String>) -> RuntimeResult<i64> {
    let mut vm = Vm::new();
    vm.run_module(module, argv)
}

pub fn run_with_path(module: &Module, argv: Vec<String>, path: &str) -> RuntimeResult<i64> {
    let mut vm = Vm::with_path(path);
    vm.run_module(module, argv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use typex_parser::parse;
    use typex_span::SourceMap;

    fn run_src(src: &str) -> RuntimeResult<i64> {
        let mut sm = SourceMap::new();
        let file = sm.add("test.tx".to_string(), src.to_string());
        let (module, diags) = parse(src, file);
        assert!(diags.is_empty(), "parse errors: {:?}", diags);
        run(&module, vec!["test.tx".to_string()])
    }

    #[test]
    fn test_main_returns_zero() {
        let src = r#"
            function main(): int {
                return 0;
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 0);
    }

    #[test]
    fn test_main_returns_code() {
        let src = r#"
            function main(): int {
                return 42;
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 42);
    }

    #[test]
    fn test_arithmetic() {
        let src = r#"
            function main(): int {
                const x: int = 10 + 5;
                const y: int = x * 2;
                return y;
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 30);
    }

    #[test]
    fn test_if_else() {
        let src = r#"
            function main(): int {
                if (1 == 1) {
                    return 1;
                } else {
                    return 0;
                }
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 1);
    }

    #[test]
    fn test_function_call() {
        let src = r#"
            function add(a: int, b: int): int {
                return a + b;
            }
            function main(): int {
                return add(3, 4);
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 7);
    }

    #[test]
    fn test_match_ok() {
        let src = r#"
        function divide(a: int, b: int): Result<int, string> {
            if (b == 0) {
                return Err("division by zero");
            }
            return Ok(a / b);
        }
        function main(): int {
            const val: int = match divide(10, 2) {
                Ok(n) => n,
                Err(e) => 0,
            };
            return val;
        }
    "#;
        assert_eq!(run_src(src).unwrap(), 5);
    }

    #[test]
    fn test_match_err() {
        let src = r#"
        function divide(a: int, b: int): Result<int, string> {
            if (b == 0) {
                return Err("division by zero");
            }
            return Ok(a / b);
        }
        function main(): int {
            const val: int = match divide(10, 0) {
                Ok(n) => n,
                Err(e) => -1,
            };
            return val;
        }
    "#;
        assert_eq!(run_src(src).unwrap(), -1);
    }

    #[test]
    fn test_ternary() {
        let src = r#"
            function main(): int {
                const x: int = 1 == 1 ? 42 : 0;
                return x;
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 42);
    }

    #[test]
    fn test_array_index() {
        let src = r#"
            function main(): int {
                const nums: Array<int> = [10, 20, 30];
                return nums[1];
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 20);
    }

    #[test]
    fn test_no_main() {
        let src = r#"
            function foo(): int {
                return 1;
            }
        "#;
        assert_eq!(run_src(src).unwrap(), 0);
    }
}
