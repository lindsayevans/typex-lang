use std::collections::HashMap;
use typex_ast::*;
use typex_span::{Diagnostic, Level, Span};

// ------------------------------------------------------------------
// Types
// ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    // Primitives
    Int8,
    Int16,
    Int32,
    Int64,
    Uint8,
    Uint16,
    Uint32,
    Uint64,
    Float32,
    Float64,
    Bool,
    Char,
    String,
    Null,

    // Compound
    Array(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Generic(String, Vec<Ty>),

    // User defined
    Named(String),

    // Special
    Void,    // function returns nothing
    Unknown, // error recovery - suppresses further errors
}

impl Ty {
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Ty::Int8
                | Ty::Int16
                | Ty::Int32
                | Ty::Int64
                | Ty::Uint8
                | Ty::Uint16
                | Ty::Uint32
                | Ty::Uint64
                | Ty::Float32
                | Ty::Float64
        )
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown)
    }
}

impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ty::Int8 => write!(f, "int8"),
            Ty::Int16 => write!(f, "int16"),
            Ty::Int32 => write!(f, "int32"),
            Ty::Int64 => write!(f, "int64"),
            Ty::Uint8 => write!(f, "uint8"),
            Ty::Uint16 => write!(f, "uint16"),
            Ty::Uint32 => write!(f, "uint32"),
            Ty::Uint64 => write!(f, "uint64"),
            Ty::Float32 => write!(f, "float32"),
            Ty::Float64 => write!(f, "float64"),
            Ty::Bool => write!(f, "boolean"),
            Ty::Char => write!(f, "char"),
            Ty::String => write!(f, "string"),
            Ty::Null => write!(f, "null"),
            Ty::Array(inner) => write!(f, "Array<{}>", inner),
            Ty::Result(ok, err) => write!(f, "Result<{}, {}>", ok, err),
            Ty::Generic(name, args) => {
                let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                write!(f, "{}<{}>", name, args.join(", "))
            }
            Ty::Named(name) => write!(f, "{}", name),
            Ty::Void => write!(f, "void"),
            Ty::Unknown => write!(f, "unknown"),
        }
    }
}

// ------------------------------------------------------------------
// Function signature
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

// ------------------------------------------------------------------
// Type environment
// ------------------------------------------------------------------

#[derive(Debug)]
pub struct TypeEnv {
    vars: HashMap<String, Ty>,
    fns: HashMap<String, FnSig>,
    current_return: Option<Ty>,
}

impl TypeEnv {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
            fns: HashMap::new(),
            current_return: None,
        }
    }

    fn define_var(&mut self, name: String, ty: Ty) {
        self.vars.insert(name, ty);
    }

    fn lookup_var(&self, name: &str) -> Option<&Ty> {
        self.vars.get(name)
    }

    fn define_fn(&mut self, name: String, sig: FnSig) {
        self.fns.insert(name, sig);
    }

    fn lookup_fn(&self, name: &str) -> Option<&FnSig> {
        self.fns.get(name)
    }
}

// ------------------------------------------------------------------
// Typechecker
// ------------------------------------------------------------------

pub struct Typechecker {
    env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
}

impl Typechecker {
    pub fn new() -> Self {
        let mut tc = Self {
            env: TypeEnv::new(),
            diagnostics: Vec::new(),
        };
        tc.register_builtins();
        tc
    }

    fn register_builtins(&mut self) {
        // print/println accept any single string arg
        for name in &["print", "println"] {
            self.env.define_fn(
                name.to_string(),
                FnSig {
                    params: vec![Ty::String],
                    ret: Ty::Void,
                },
            );
        }
        // panic
        self.env.define_fn(
            "panic".to_string(),
            FnSig {
                params: vec![Ty::String],
                ret: Ty::Void,
            },
        );
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            level: Level::Error,
            span,
            message: message.into(),
        });
    }

    fn warning(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            level: Level::Warning,
            span,
            message: message.into(),
        });
    }

    // ------------------------------------------------------------------
    // Type expression -> Ty
    // ------------------------------------------------------------------

    fn resolve_type(&self, ty: &TypeExpr) -> Ty {
        match ty {
            TypeExpr::Named(name) => self.resolve_named_type(&name.name),
            TypeExpr::Generic(name, args) => {
                let resolved_args: Vec<Ty> = args.iter().map(|a| self.resolve_type(a)).collect();
                match name.name.as_str() {
                    "Array" if resolved_args.len() == 1 => {
                        Ty::Array(Box::new(resolved_args.into_iter().next().unwrap()))
                    }
                    "Result" if resolved_args.len() == 2 => {
                        let mut iter = resolved_args.into_iter();
                        Ty::Result(
                            Box::new(iter.next().unwrap()),
                            Box::new(iter.next().unwrap()),
                        )
                    }
                    _ => Ty::Generic(name.name.clone(), resolved_args),
                }
            }
            TypeExpr::Union(variants) => {
                // For now treat string | null as nullable string
                // Full union types come in v2
                if variants.len() == 2 {
                    let has_null = variants
                        .iter()
                        .any(|v| matches!(v, TypeExpr::Named(n) if n.name == "null"));
                    if has_null {
                        let other = variants
                            .iter()
                            .find(|v| !matches!(v, TypeExpr::Named(n) if n.name == "null"));
                        if let Some(inner) = other {
                            return self.resolve_type(inner);
                        }
                    }
                }
                Ty::Unknown
            }
            TypeExpr::Nullable(inner) => self.resolve_type(inner),
        }
    }

    fn resolve_named_type(&self, name: &str) -> Ty {
        match name {
            "int8" => Ty::Int8,
            "int16" => Ty::Int16,
            "int32" => Ty::Int32,
            "int64" => Ty::Int64,
            "int" => Ty::Int64,
            "uint8" => Ty::Uint8,
            "uint16" => Ty::Uint16,
            "uint32" => Ty::Uint32,
            "uint64" => Ty::Uint64,
            "uint" => Ty::Uint64,
            "float32" => Ty::Float32,
            "float64" => Ty::Float64,
            "float" => Ty::Float64,
            "boolean" => Ty::Bool,
            "char" => Ty::Char,
            "string" => Ty::String,
            "null" => Ty::Null,
            other => Ty::Named(other.to_string()),
        }
    }

    // ------------------------------------------------------------------
    // Module
    // ------------------------------------------------------------------

    pub fn check_module(&mut self, module: &Module) {
        // First pass: collect function signatures
        for item in &module.items {
            if let Item::Function(f) = item {
                self.hoist_function(f);
            }
        }
        // Second pass: check bodies
        for item in &module.items {
            self.check_item(item);
        }
    }

    fn hoist_function(&mut self, f: &FunctionDef) {
        let params: Vec<Ty> = f.params.iter().map(|p| self.resolve_type(&p.ty)).collect();
        let ret = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t))
            .unwrap_or(Ty::Void);
        self.env
            .define_fn(f.name.name.clone(), FnSig { params, ret });
    }

    fn check_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => self.check_function(f),
            Item::Const(c) => {
                let ty =
                    c.ty.as_ref()
                        .map(|t| self.resolve_type(t))
                        .unwrap_or_else(|| {
                            // allow arrow functions without explicit type annotation
                            if matches!(&c.value, Expr::Arrow(_)) {
                                return Ty::Unknown;
                            }
                            self.error(
                                c.span,
                                format!(
                                    "const '{}' must have an explicit type annotation",
                                    c.name.name
                                ),
                            );
                            Ty::Unknown
                        });
                let val_ty = self.check_expr(&c.value);
                if ty != Ty::Unknown {
                    self.check_assignable(&ty, &val_ty, c.span);
                }
                self.env.define_var(c.name.name.clone(), val_ty);
            }
            Item::Let(l) => {
                let ty =
                    l.ty.as_ref()
                        .map(|t| self.resolve_type(t))
                        .unwrap_or_else(|| {
                            self.error(
                                l.span,
                                format!(
                                    "let '{}' must have an explicit type annotation",
                                    l.name.name
                                ),
                            );
                            Ty::Unknown
                        });
                if let Some(val) = &l.value {
                    let val_ty = self.check_expr(val);
                    self.check_assignable(&ty, &val_ty, l.span);
                }
                self.env.define_var(l.name.name.clone(), ty);
            }
            Item::TypeAlias(_) | Item::Enum(_) | Item::Import(_) | Item::Export(_) => {}
        }
    }

    fn check_function(&mut self, f: &FunctionDef) {
        let ret_ty = f
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t))
            .unwrap_or(Ty::Void);

        // save/restore return type context
        let prev_return = self.env.current_return.clone();
        self.env.current_return = Some(ret_ty.clone());

        // define params in a new scope (we use a flat env for v1)
        let mut saved: Vec<(String, Option<Ty>)> = Vec::new();
        for param in &f.params {
            let ty = self.resolve_type(&param.ty);
            let prev = self.env.vars.remove(&param.name.name);
            saved.push((param.name.name.clone(), prev));
            self.env.define_var(param.name.name.clone(), ty);
        }

        self.check_block(&f.body);

        // restore scope
        for (name, prev) in saved {
            match prev {
                Some(ty) => self.env.define_var(name, ty),
                None => {
                    self.env.vars.remove(&name);
                }
            }
        }
        self.env.current_return = prev_return;
    }

    fn check_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.check_stmt(stmt);
        }
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(l) => {
                let ty =
                    l.ty.as_ref()
                        .map(|t| self.resolve_type(t))
                        .unwrap_or_else(|| {
                            self.error(
                                l.span,
                                format!(
                                    "let '{}' must have an explicit type annotation",
                                    l.name.name
                                ),
                            );
                            Ty::Unknown
                        });
                if let Some(val) = &l.value {
                    let val_ty = self.check_expr(val);
                    self.check_assignable(&ty, &val_ty, l.span);
                }
                self.env.define_var(l.name.name.clone(), ty);
            }
            Stmt::Const(c) => {
                let ty =
                    c.ty.as_ref()
                        .map(|t| self.resolve_type(t))
                        .unwrap_or_else(|| {
                            if matches!(&c.value, Expr::Arrow(_)) {
                                return Ty::Unknown;
                            }
                            self.error(
                                c.span,
                                format!(
                                    "const '{}' must have an explicit type annotation",
                                    c.name.name
                                ),
                            );
                            Ty::Unknown
                        });
                let val_ty = self.check_expr(&c.value);
                if ty != Ty::Unknown {
                    self.check_assignable(&ty, &val_ty, c.span);
                }
                self.env.define_var(c.name.name.clone(), val_ty);
            }
            Stmt::Return(expr, span) => {
                let expected = self.env.current_return.clone().unwrap_or(Ty::Void);
                match expr {
                    Some(e) => {
                        let ty = self.check_expr(e);
                        // void return type means unconstrained - skip check
                        if expected != Ty::Void {
                            self.check_assignable(&expected, &ty, *span);
                        }
                    }
                    None => {
                        if expected != Ty::Void {
                            self.error(
                                *span,
                                format!("expected return type '{}', got void", expected),
                            );
                        }
                    }
                }
            }
            Stmt::Expr(e) => {
                self.check_expr(e);
            }
            Stmt::If(i) => self.check_if(i),
            Stmt::Switch(s) => self.check_switch(s),
            Stmt::For(f) => self.check_for(f),
            Stmt::Match(m) => {
                self.check_match(m);
            }
        }
    }

    fn check_if(&mut self, i: &IfStmt) {
        let cond_ty = self.check_expr(&i.condition);
        if !cond_ty.is_unknown() && cond_ty != Ty::Bool {
            self.error(
                i.span,
                format!("if condition must be boolean, got '{}'", cond_ty),
            );
        }
        self.check_block(&i.then_block);
        for (cond, block) in &i.else_if {
            let ty = self.check_expr(cond);
            if !ty.is_unknown() && ty != Ty::Bool {
                self.error(
                    i.span,
                    format!("else if condition must be boolean, got '{}'", ty),
                );
            }
            self.check_block(block);
        }
        if let Some(else_block) = &i.else_block {
            self.check_block(else_block);
        }
    }

    fn check_switch(&mut self, s: &SwitchStmt) {
        self.check_expr(&s.value);
        for case in &s.cases {
            self.check_expr(&case.value);
            self.check_block(&case.body);
        }
        if let Some(default) = &s.default {
            self.check_block(default);
        }
    }

    fn check_for(&mut self, f: &ForStmt) {
        match f {
            ForStmt::Array {
                value,
                index,
                iter,
                body,
                ..
            } => {
                let iter_ty = self.check_expr(iter);
                let elem_ty = match &iter_ty {
                    Ty::Array(inner) => *inner.clone(),
                    Ty::String => Ty::String, // string iteration
                    Ty::Unknown => Ty::Unknown,
                    other => {
                        self.error(
                            body.span,
                            format!("'in' loop expects Array<T>, got '{}'", other),
                        );
                        Ty::Unknown
                    }
                };
                let saved_index = index
                    .as_ref()
                    .map(|i| (i.name.clone(), self.env.vars.remove(&i.name)));
                let saved_value = value
                    .as_ref()
                    .map(|v| (v.name.clone(), self.env.vars.remove(&v.name)));

                if let Some(i) = index {
                    self.env.define_var(i.name.clone(), Ty::Uint64);
                }
                if let Some(v) = value {
                    self.env.define_var(v.name.clone(), elem_ty);
                }
                self.check_block(body);

                // restore
                if let Some((name, prev)) = saved_index {
                    match prev {
                        Some(ty) => self.env.define_var(name, ty),
                        None => {
                            self.env.vars.remove(&name);
                        }
                    }
                }
                if let Some((name, prev)) = saved_value {
                    match prev {
                        Some(ty) => self.env.define_var(name, ty),
                        None => {
                            self.env.vars.remove(&name);
                        }
                    }
                }
            }
            ForStmt::Object {
                key,
                value,
                iter,
                body,
                ..
            } => {
                self.check_expr(iter);
                let saved_key = key
                    .as_ref()
                    .map(|k| (k.name.clone(), self.env.vars.remove(&k.name)));
                let saved_val = value
                    .as_ref()
                    .map(|v| (v.name.clone(), self.env.vars.remove(&v.name)));

                if let Some(k) = key {
                    self.env.define_var(k.name.clone(), Ty::String);
                }
                if let Some(v) = value {
                    self.env.define_var(v.name.clone(), Ty::Unknown);
                }
                self.check_block(body);

                if let Some((name, prev)) = saved_key {
                    match prev {
                        Some(ty) => self.env.define_var(name, ty),
                        None => {
                            self.env.vars.remove(&name);
                        }
                    }
                }
                if let Some((name, prev)) = saved_val {
                    match prev {
                        Some(ty) => self.env.define_var(name, ty),
                        None => {
                            self.env.vars.remove(&name);
                        }
                    }
                }
            }
            ForStmt::Str {
                index,
                offset,
                value,
                iter,
                body,
                ..
            } => {
                let iter_ty = self.check_expr(iter);
                if !iter_ty.is_unknown() && iter_ty != Ty::String {
                    self.error(
                        body.span,
                        format!("string 'in' loop expects string, got '{}'", iter_ty),
                    );
                }

                let saved: Vec<(String, Option<Ty>)> = [
                    index.as_ref().map(|i| i.name.clone()),
                    offset.as_ref().map(|o| o.name.clone()),
                    value.as_ref().map(|v| v.name.clone()),
                ]
                .iter()
                .flatten()
                .map(|name| (name.clone(), self.env.vars.remove(name)))
                .collect();

                if let Some(i) = index {
                    self.env.define_var(i.name.clone(), Ty::Uint64);
                }
                if let Some(o) = offset {
                    self.env.define_var(o.name.clone(), Ty::Uint64);
                }
                if let Some(v) = value {
                    self.env.define_var(v.name.clone(), Ty::String);
                }
                self.check_block(body);

                for (name, prev) in saved {
                    match prev {
                        Some(ty) => self.env.define_var(name, ty),
                        None => {
                            self.env.vars.remove(&name);
                        }
                    }
                }
            }
        }
    }

    fn check_match(&mut self, m: &MatchExpr) -> Ty {
        let val_ty = self.check_expr(&m.value);
        let mut arm_types: Vec<Ty> = Vec::new();

        for arm in &m.arms {
            // bind pattern variables
            let saved = match &arm.pattern {
                Pattern::Ok(binding) => {
                    let inner = match &val_ty {
                        Ty::Result(ok, _) => *ok.clone(),
                        _ => Ty::Unknown,
                    };
                    let prev = self.env.vars.remove(&binding.name);
                    self.env.define_var(binding.name.clone(), inner);
                    vec![(binding.name.clone(), prev)]
                }
                Pattern::Err(binding) => {
                    let inner = match &val_ty {
                        Ty::Result(_, err) => *err.clone(),
                        _ => Ty::Unknown,
                    };
                    let prev = self.env.vars.remove(&binding.name);
                    self.env.define_var(binding.name.clone(), inner);
                    vec![(binding.name.clone(), prev)]
                }
                Pattern::EnumVariant(_, Some(binding)) => {
                    let prev = self.env.vars.remove(&binding.name);
                    self.env.define_var(binding.name.clone(), Ty::Unknown);
                    vec![(binding.name.clone(), prev)]
                }
                Pattern::EnumVariant(_, None) | Pattern::Wildcard => vec![],
            };

            let arm_ty = self.check_expr(&arm.body);
            arm_types.push(arm_ty);

            // restore bindings
            for (name, prev) in saved {
                match prev {
                    Some(ty) => self.env.define_var(name, ty),
                    None => {
                        self.env.vars.remove(&name);
                    }
                }
            }
        }

        // all arms should return same type
        let is_result_match = matches!(&val_ty, Ty::Result(_, _));

        if !is_result_match {
            if let Some(first) = arm_types.first() {
                for ty in arm_types.iter().skip(1) {
                    if !ty.is_unknown()
                        && !first.is_unknown()
                        && *ty != Ty::Void
                        && *first != Ty::Void
                        && ty != first
                    {
                        self.error(
                            m.span,
                            format!(
                                "match arms have incompatible types: '{}' and '{}'",
                                first, ty
                            ),
                        );
                        break;
                    }
                }
            }
        }

        // for Result matches return the Ok arm type (first arm)
        // for other matches return the first arm type
        arm_types.first().cloned().unwrap_or(Ty::Void)
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn check_expr(&mut self, expr: &Expr) -> Ty {
        match expr {
            Expr::Lit(lit) => self.check_lit(lit),
            Expr::Ident(i) => {
                match self.env.lookup_var(&i.name) {
                    Some(ty) => ty.clone(),
                    None => {
                        // might be a function reference — that's ok
                        if self.env.lookup_fn(&i.name).is_some() {
                            Ty::Named(i.name.clone())
                        } else {
                            self.error(i.span, format!("undefined variable '{}'", i.name));
                            Ty::Unknown
                        }
                    }
                }
            }
            Expr::BinOp(l, op, r, span) => self.check_binop(l, op, r, *span),
            Expr::UnaryOp(op, e, span) => self.check_unary(op, e, *span),
            Expr::Call(f, args, span) => self.check_call(f, args, *span),
            Expr::Field(e, field, span) => self.check_field(e, field, *span),
            Expr::Index(e, i, span) => self.check_index(e, i, *span),
            Expr::Ternary(cond, then, else_, span) => self.check_ternary(cond, then, else_, *span),
            Expr::Match(m) => self.check_match(m),
            Expr::Arrow(a) => self.check_arrow(a),
            Expr::Array(elements, span) => self.check_array(elements, *span),
            Expr::Record(_, _) => Ty::Unknown, // v2
            Expr::Destructure(_) => Ty::Unknown,
            Expr::Assign(lhs, rhs, span) => {
                let rhs_ty = self.check_expr(rhs);
                match lhs.as_ref() {
                    Expr::Ident(i) => {
                        if let Some(lhs_ty) = self.env.lookup_var(&i.name).cloned() {
                            self.check_assignable(&lhs_ty, &rhs_ty, *span);
                        }
                    }
                    _ => self.error(*span, "invalid assignment target"),
                }
                rhs_ty
            }
        }
    }

    fn check_lit(&self, lit: &Lit) -> Ty {
        match lit {
            Lit::Int(_, _) => Ty::Int64,
            Lit::Float(_, _) => Ty::Float64,
            Lit::Bool(_, _) => Ty::Bool,
            Lit::Str(_, _) => Ty::String,
            Lit::Char(_, _) => Ty::Char,
            Lit::Null(_) => Ty::Null,
        }
    }

    fn check_binop(&mut self, l: &Expr, op: &BinOp, r: &Expr, span: Span) -> Ty {
        let lt = self.check_expr(l);
        let rt = self.check_expr(r);

        if lt.is_unknown() || rt.is_unknown() {
            return Ty::Unknown;
        }

        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                // allow string + string or string + numeric
                if matches!(op, BinOp::Add) && lt == Ty::String {
                    return Ty::String;
                }
                if !lt.is_numeric() {
                    self.error(
                        span,
                        format!("operator requires numeric type, got '{}'", lt),
                    );
                    return Ty::Unknown;
                }
                if lt != rt {
                    self.error(span, format!("type mismatch: '{}' and '{}'", lt, rt));
                    return Ty::Unknown;
                }
                lt
            }
            BinOp::Eq | BinOp::NotEq => {
                // allow mixed int/uint comparison without warning
                let mixed_numeric =
                    (lt == Ty::Uint64 && rt == Ty::Int64) || (lt == Ty::Int64 && rt == Ty::Uint64);
                if !mixed_numeric && lt != rt {
                    self.warning(
                        span,
                        format!("comparing '{}' and '{}' will always be unequal", lt, rt),
                    );
                }
                Ty::Bool
            }
            BinOp::Lt | BinOp::Lte | BinOp::Gt | BinOp::Gte => {
                if !lt.is_numeric() {
                    self.error(
                        span,
                        format!("comparison requires numeric type, got '{}'", lt),
                    );
                }
                Ty::Bool
            }
            BinOp::And | BinOp::Or => {
                if lt != Ty::Bool {
                    self.error(
                        span,
                        format!("logical operator requires boolean, got '{}'", lt),
                    );
                }
                if rt != Ty::Bool {
                    self.error(
                        span,
                        format!("logical operator requires boolean, got '{}'", rt),
                    );
                }
                Ty::Bool
            }
        }
    }

    fn check_unary(&mut self, op: &UnaryOp, e: &Expr, span: Span) -> Ty {
        let ty = self.check_expr(e);
        match op {
            UnaryOp::Neg => {
                if !ty.is_numeric() && !ty.is_unknown() {
                    self.error(
                        span,
                        format!("unary '-' requires numeric type, got '{}'", ty),
                    );
                    Ty::Unknown
                } else {
                    ty
                }
            }
            UnaryOp::Not => {
                if ty != Ty::Bool && !ty.is_unknown() {
                    self.error(span, format!("unary '!' requires boolean, got '{}'", ty));
                    Ty::Unknown
                } else {
                    Ty::Bool
                }
            }
        }
    }

    fn check_call(&mut self, f: &Expr, args: &[Expr], span: Span) -> Ty {
        let arg_types: Vec<Ty> = args.iter().map(|a| self.check_expr(a)).collect();

        // get function name for lookup
        let fn_name = match f {
            Expr::Ident(i) => Some(i.name.clone()),
            Expr::Field(_, field, _) => Some(field.name.clone()),
            _ => None,
        };

        if let Some(name) = fn_name {
            // builtins: print/println/panic are variadic, skip arity check
            if matches!(name.as_str(), "print" | "println" | "panic") {
                return Ty::Void;
            }
            // Ok/Err constructors
            if name == "Ok" {
                if let Some(ty) = arg_types.first() {
                    let err_ty = match &self.env.current_return {
                        Some(Ty::Result(_, e)) => *e.clone(),
                        _ => Ty::Unknown,
                    };
                    return Ty::Result(Box::new(ty.clone()), Box::new(err_ty));
                }
            }
            if name == "Err" {
                if let Some(ty) = arg_types.first() {
                    let ok_ty = match &self.env.current_return {
                        Some(Ty::Result(ok, _)) => *ok.clone(),
                        _ => Ty::Unknown,
                    };
                    return Ty::Result(Box::new(ok_ty), Box::new(ty.clone()));
                }
            }

            if let Some(sig) = self.env.lookup_fn(&name).cloned() {
                // check arity
                if arg_types.len() != sig.params.len() {
                    self.error(
                        span,
                        format!(
                            "function '{}' expects {} arguments, got {}",
                            name,
                            sig.params.len(),
                            arg_types.len()
                        ),
                    );
                    return sig.ret.clone();
                }
                // check param types
                for (i, (arg_ty, param_ty)) in arg_types.iter().zip(sig.params.iter()).enumerate() {
                    if !arg_ty.is_unknown() && arg_ty != param_ty {
                        self.error(
                            span,
                            format!(
                                "argument {} of '{}': expected '{}', got '{}'",
                                i + 1,
                                name,
                                param_ty,
                                arg_ty
                            ),
                        );
                    }
                }
                return sig.ret.clone();
            }
        }

        // unknown function — already caught by resolver
        Ty::Unknown
    }

    fn check_field(&mut self, e: &Expr, field: &Ident, _span: Span) -> Ty {
        let ty = self.check_expr(e);
        match (&ty, field.name.as_str()) {
            (Ty::Array(_), "length") => Ty::Uint64,
            (Ty::String, "length") => Ty::Uint64,
            _ => {
                // field access type resolution requires full type info
                Ty::Unknown
            }
        }
    }

    fn check_index(&mut self, e: &Expr, i: &Expr, span: Span) -> Ty {
        let ty = self.check_expr(e);
        self.check_expr(i);
        match ty {
            Ty::Array(inner) => *inner,
            Ty::Unknown => Ty::Unknown,
            other => {
                self.error(
                    span,
                    format!("index operator requires Array<T>, got '{}'", other),
                );
                Ty::Unknown
            }
        }
    }

    fn check_ternary(&mut self, cond: &Expr, then: &Expr, else_: &Expr, span: Span) -> Ty {
        let cond_ty = self.check_expr(cond);
        if !cond_ty.is_unknown() && cond_ty != Ty::Bool {
            self.error(
                span,
                format!("ternary condition must be boolean, got '{}'", cond_ty),
            );
        }
        let then_ty = self.check_expr(then);
        let else_ty = self.check_expr(else_);
        if !then_ty.is_unknown() && !else_ty.is_unknown() && then_ty != else_ty {
            self.error(
                span,
                format!(
                    "ternary branches have incompatible types: '{}' and '{}'",
                    then_ty, else_ty
                ),
            );
        }
        then_ty
    }

    fn check_arrow(&mut self, a: &ArrowFn) -> Ty {
        let params: Vec<Ty> = a.params.iter().map(|p| self.resolve_type(&p.ty)).collect();
        let ret = a
            .return_type
            .as_ref()
            .map(|t| self.resolve_type(t))
            .unwrap_or(Ty::Void);

        // save/restore env
        let mut saved: Vec<(String, Option<Ty>)> = Vec::new();
        for (param, ty) in a.params.iter().zip(params.iter()) {
            let prev = self.env.vars.remove(&param.name.name);
            saved.push((param.name.name.clone(), prev));
            self.env.define_var(param.name.name.clone(), ty.clone());
        }

        let prev_return = self.env.current_return.clone();
        self.env.current_return = Some(ret.clone());

        match &a.body {
            ArrowBody::Expr(e) => {
                self.check_expr(e);
            }
            ArrowBody::Block(b) => {
                self.check_block(b);
            }
        }

        self.env.current_return = prev_return;
        for (name, prev) in saved {
            match prev {
                Some(ty) => self.env.define_var(name, ty),
                None => {
                    self.env.vars.remove(&name);
                }
            }
        }

        Ty::Named(format!(
            "fn({}) -> {}",
            params
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            ret
        ))
    }

    fn check_array(&mut self, elements: &[Expr], span: Span) -> Ty {
        if elements.is_empty() {
            return Ty::Array(Box::new(Ty::Unknown));
        }
        let first_ty = self.check_expr(&elements[0]);
        for elem in elements.iter().skip(1) {
            let ty = self.check_expr(elem);
            if !ty.is_unknown() && !first_ty.is_unknown() && ty != first_ty {
                self.error(
                    span,
                    format!(
                        "array elements must be the same type: '{}' and '{}'",
                        first_ty, ty
                    ),
                );
                return Ty::Array(Box::new(Ty::Unknown));
            }
        }
        Ty::Array(Box::new(first_ty))
    }

    // ------------------------------------------------------------------
    // Type compatibility
    // ------------------------------------------------------------------

    fn check_assignable(&mut self, expected: &Ty, actual: &Ty, span: Span) {
        if expected.is_unknown() || actual.is_unknown() {
            return;
        }
        // allow empty array (Array<Unknown>) to be assigned to any Array<T>
        if let (Ty::Array(_), Ty::Array(inner)) = (expected, actual) {
            if inner.is_unknown() {
                return;
            }
        }
        // numeric widening: allow assigning int8 to int64 etc
        if expected.is_numeric() && actual.is_numeric() {
            return;
        }
        if expected != actual {
            self.error(
                span,
                format!("type mismatch: expected '{}', got '{}'", expected, actual),
            );
        }
    }
}

impl Default for Typechecker {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------
// Public API
// ------------------------------------------------------------------

pub fn typecheck(module: &Module) -> Vec<Diagnostic> {
    let mut tc = Typechecker::new();
    tc.check_module(module);
    tc.diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use typex_parser::parse;
    use typex_span::SourceMap;

    fn check_src(src: &str) -> Vec<Diagnostic> {
        let mut sm = SourceMap::new();
        let file = sm.add("test.tx".to_string(), src.to_string());
        let (module, parse_diags) = parse(src, file);
        assert!(parse_diags.is_empty(), "parse errors: {:?}", parse_diags);
        typecheck(&module)
    }

    #[test]
    fn test_clean_divide() {
        let src = r#"
            function divide(numerator: int, denominator: int): Result<int, string> {
                if (denominator == 0) {
                    return Err("Cannot divide by zero!");
                }
                return Ok(numerator / denominator);
            }
            function main() {
                match divide(10, 0) {
                    Ok(result) => result,
                    Err(error) => error,
                }
            }
        "#;
        let diags = check_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }

    #[test]
    fn test_type_mismatch_return() {
        let src = r#"
            function foo(): int {
                return "hello";
            }
        "#;
        let diags = check_src(src);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("type mismatch"));
    }

    #[test]
    fn test_wrong_arg_count() {
        let src = r#"
            function add(a: int, b: int): int {
                return a + b;
            }
            function main() {
                add(1, 2, 3);
            }
        "#;
        let diags = check_src(src);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("expects 2 arguments, got 3"));
    }

    #[test]
    fn test_wrong_arg_type() {
        let src = r#"
            function double(n: int): int {
                return n + n;
            }
            function main() {
                double("hello");
            }
        "#;
        let diags = check_src(src);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("expected 'int64'"));
    }

    #[test]
    fn test_non_bool_condition() {
        let src = r#"
            function main() {
                if (42) {
                    return 1;
                }
            }
        "#;
        let diags = check_src(src);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("boolean"));
    }

    #[test]
    fn test_missing_type_annotation() {
        let src = r#"
            const x = 42;
        "#;
        let diags = check_src(src);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("explicit type annotation"));
    }

    #[test]
    fn test_array_type() {
        let src = r#"
            const nums: Array<int> = [1, 2, 3];
        "#;
        let diags = check_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }

    #[test]
    fn test_ternary_type_mismatch() {
        let src = r#"
            function main() {
                const x: int = 1 == 1 ? 42 : "hello";
            }
        "#;
        let diags = check_src(src);
        assert!(!diags.is_empty());
        assert!(diags.iter().any(|d| d.message.contains("incompatible")));
    }
}
