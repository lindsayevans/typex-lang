use std::collections::HashMap;
use typex_ast::*;
use typex_span::{Diagnostic, Level, Span};

// ------------------------------------------------------------------
// Symbol
// ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Const,
    Let,
    Param,
    Type,
    Enum,
    Import,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
}

// ------------------------------------------------------------------
// Scope
// ------------------------------------------------------------------

#[derive(Debug)]
pub struct Scope {
    symbols: HashMap<String, Symbol>,
}

impl Scope {
    fn new() -> Self {
        Self {
            symbols: HashMap::new(),
        }
    }

    fn define(&mut self, symbol: Symbol) -> Option<Symbol> {
        self.symbols.insert(symbol.name.clone(), symbol)
    }

    fn get(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
}

// ------------------------------------------------------------------
// Resolver
// ------------------------------------------------------------------

pub struct Resolver {
    scopes: Vec<Scope>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolver {
    pub fn new() -> Self {
        let mut r = Self {
            scopes: Vec::new(),
            diagnostics: Vec::new(),
        };
        r.push_scope(); // global scope
        r.define_builtins();
        r
    }

    fn define_builtins(&mut self) {
        // builtin functions
        for name in &["print", "println", "panic"] {
            self.define(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Function,
                span: Span::point(typex_span::FileId(0), typex_span::Pos::new(0, 0, 0)),
            });
        }
        // builtin types
        for name in &[
            "int", "uint", "float", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32",
            "uint64", "float32", "float64", "string", "char", "boolean", "null", "Array", "Result",
            "Ok", "Err", "Date", "Time", "DateTime",
        ] {
            self.define(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Type,
                span: Span::point(typex_span::FileId(0), typex_span::Pos::new(0, 0, 0)),
            });
        }
    }

    // ------------------------------------------------------------------
    // Scope management
    // ------------------------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, symbol: Symbol) {
        if let Some(scope) = self.scopes.last_mut() {
            if let Some(existing) = scope.define(symbol.clone()) {
                self.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    span: symbol.span,
                    message: format!(
                        "duplicate declaration '{}', previously declared at {}:{}",
                        symbol.name, existing.span.start.line, existing.span.start.col
                    ),
                });
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.get(name) {
                return Some(sym);
            }
        }
        None
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            level: Level::Error,
            span,
            message: message.into(),
        });
    }

    // ------------------------------------------------------------------
    // Resolution
    // ------------------------------------------------------------------

    pub fn resolve_module(&mut self, module: &Module) {
        // First pass: collect top-level declarations so order doesn't matter
        for item in &module.items {
            self.hoist_item(item);
        }
        // Second pass: resolve bodies
        for item in &module.items {
            self.resolve_item(item);
        }
    }

    fn hoist_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => {
                self.define(Symbol {
                    name: f.name.name.clone(),
                    kind: SymbolKind::Function,
                    span: f.span,
                });
            }
            Item::TypeAlias(t) => {
                self.define(Symbol {
                    name: t.name.name.clone(),
                    kind: SymbolKind::Type,
                    span: t.span,
                });
            }
            Item::Enum(e) => {
                self.define(Symbol {
                    name: e.name.name.clone(),
                    kind: SymbolKind::Enum,
                    span: e.span,
                });
            }
            Item::Import(i) => {
                for name in &i.names {
                    self.define(Symbol {
                        name: name.name.clone(),
                        kind: SymbolKind::Import,
                        span: name.span,
                    });
                }
            }
            Item::Const(c) => {
                self.define(Symbol {
                    name: c.name.name.clone(),
                    kind: SymbolKind::Const,
                    span: c.span,
                });
            }
            Item::Let(l) => {
                self.define(Symbol {
                    name: l.name.name.clone(),
                    kind: SymbolKind::Let,
                    span: l.span,
                });
            }
            Item::Export(_) => {} // exports reference existing names, resolved in second pass
        }
    }

    fn resolve_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => self.resolve_function(f),
            Item::TypeAlias(t) => self.resolve_type_expr(&t.ty),
            Item::Enum(_) => {}   // enum variants are self-contained
            Item::Import(_) => {} // already hoisted
            Item::Export(e) => self.resolve_export(e),
            Item::Const(c) => self.resolve_expr(&c.value),
            Item::Let(l) => {
                if let Some(val) = &l.value {
                    self.resolve_expr(val);
                }
            }
        }
    }

    fn resolve_function(&mut self, f: &FunctionDef) {
        self.push_scope();
        for param in &f.params {
            self.define(Symbol {
                name: param.name.name.clone(),
                kind: SymbolKind::Param,
                span: param.span,
            });
            self.resolve_type_expr(&param.ty);
        }
        if let Some(ret) = &f.return_type {
            self.resolve_type_expr(ret);
        }
        self.resolve_block(&f.body);
        self.pop_scope();
    }

    fn resolve_block(&mut self, block: &Block) {
        self.push_scope();
        for stmt in &block.stmts {
            self.resolve_stmt(stmt);
        }
        self.pop_scope();
    }

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(l) => {
                if let Some(val) = &l.value {
                    self.resolve_expr(val);
                }
                if let Some(ty) = &l.ty {
                    self.resolve_type_expr(ty);
                }
                self.define(Symbol {
                    name: l.name.name.clone(),
                    kind: SymbolKind::Let,
                    span: l.span,
                });
            }
            Stmt::Const(c) => {
                self.resolve_expr(&c.value);
                if let Some(ty) = &c.ty {
                    self.resolve_type_expr(ty);
                }
                self.define(Symbol {
                    name: c.name.name.clone(),
                    kind: SymbolKind::Const,
                    span: c.span,
                });
            }
            Stmt::Return(expr, _) => {
                if let Some(e) = expr {
                    self.resolve_expr(e);
                }
            }
            Stmt::Expr(e) => self.resolve_expr(e),
            Stmt::If(i) => self.resolve_if(i),
            Stmt::Switch(s) => self.resolve_switch(s),
            Stmt::For(f) => self.resolve_for(f),
            Stmt::Match(m) => self.resolve_match(m),
        }
    }

    fn resolve_if(&mut self, i: &IfStmt) {
        self.resolve_expr(&i.condition);
        self.resolve_block(&i.then_block);
        for (cond, block) in &i.else_if {
            self.resolve_expr(cond);
            self.resolve_block(block);
        }
        if let Some(else_block) = &i.else_block {
            self.resolve_block(else_block);
        }
    }

    fn resolve_switch(&mut self, s: &SwitchStmt) {
        self.resolve_expr(&s.value);
        for case in &s.cases {
            self.resolve_expr(&case.value);
            self.resolve_block(&case.body);
        }
        if let Some(default) = &s.default {
            self.resolve_block(default);
        }
    }

    fn resolve_for(&mut self, f: &ForStmt) {
        self.push_scope();
        match f {
            ForStmt::Array {
                index,
                value,
                iter,
                body,
                ..
            } => {
                self.resolve_expr(iter);
                if let Some(i) = index {
                    self.define(Symbol {
                        name: i.name.clone(),
                        kind: SymbolKind::Let,
                        span: i.span,
                    });
                }
                if let Some(v) = value {
                    self.define(Symbol {
                        name: v.name.clone(),
                        kind: SymbolKind::Let,
                        span: v.span,
                    });
                }
                self.resolve_block(body);
            }
            ForStmt::Object {
                key,
                value,
                iter,
                body,
                ..
            } => {
                self.resolve_expr(iter);
                if let Some(k) = key {
                    self.define(Symbol {
                        name: k.name.clone(),
                        kind: SymbolKind::Let,
                        span: k.span,
                    });
                }
                if let Some(v) = value {
                    self.define(Symbol {
                        name: v.name.clone(),
                        kind: SymbolKind::Let,
                        span: v.span,
                    });
                }
                self.resolve_block(body);
            }
            ForStmt::Str {
                index,
                offset,
                value,
                iter,
                body,
                ..
            } => {
                self.resolve_expr(iter);
                for opt_ident in &[index, offset, value] {
                    if let Some(i) = opt_ident {
                        self.define(Symbol {
                            name: i.name.clone(),
                            kind: SymbolKind::Let,
                            span: i.span,
                        });
                    }
                }
                self.resolve_block(body);
            }
        }
        self.pop_scope();
    }

    fn resolve_match(&mut self, m: &MatchExpr) {
        self.resolve_expr(&m.value);
        for arm in &m.arms {
            self.push_scope();
            match &arm.pattern {
                Pattern::Ok(binding) | Pattern::Err(binding) => {
                    self.define(Symbol {
                        name: binding.name.clone(),
                        kind: SymbolKind::Let,
                        span: binding.span,
                    });
                }
                Pattern::EnumVariant(_, Some(binding)) => {
                    self.define(Symbol {
                        name: binding.name.clone(),
                        kind: SymbolKind::Let,
                        span: binding.span,
                    });
                }
                Pattern::EnumVariant(_, None) | Pattern::Wildcard => {}
            }
            self.resolve_expr(&arm.body);
            self.pop_scope();
        }
    }

    fn resolve_export(&mut self, e: &Export) {
        for name in &e.names {
            if self.lookup(&name.name).is_none() {
                self.error(name.span, format!("export '{}' is not defined", name.name));
            }
        }
    }

    fn resolve_type_expr(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Named(name) => {
                if self.lookup(&name.name).is_none() {
                    self.error(name.span, format!("unknown type '{}'", name.name));
                }
            }
            TypeExpr::Generic(name, args) => {
                if self.lookup(&name.name).is_none() {
                    self.error(name.span, format!("unknown type '{}'", name.name));
                }
                for arg in args {
                    self.resolve_type_expr(arg);
                }
            }
            TypeExpr::Union(variants) => {
                for v in variants {
                    self.resolve_type_expr(v);
                }
            }
            TypeExpr::Nullable(inner) => {
                self.resolve_type_expr(inner);
            }
        }
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Lit(_) => {}
            Expr::Ident(i) => {
                if self.lookup(&i.name).is_none() {
                    self.error(i.span, format!("undefined identifier '{}'", i.name));
                }
            }
            Expr::BinOp(l, _, r, _) => {
                self.resolve_expr(l);
                self.resolve_expr(r);
            }
            Expr::UnaryOp(_, e, _) => self.resolve_expr(e),
            Expr::Call(f, args, _) => {
                self.resolve_expr(f);
                for arg in args {
                    self.resolve_expr(arg);
                }
            }
            Expr::Field(e, _, _) => self.resolve_expr(e),
            Expr::Index(e, i, _) => {
                self.resolve_expr(e);
                self.resolve_expr(i);
            }
            Expr::Ternary(cond, then, else_, _) => {
                self.resolve_expr(cond);
                self.resolve_expr(then);
                self.resolve_expr(else_);
            }
            Expr::Match(m) => self.resolve_match(m),
            Expr::Arrow(a) => {
                self.push_scope();
                for param in &a.params {
                    self.define(Symbol {
                        name: param.name.name.clone(),
                        kind: SymbolKind::Param,
                        span: param.span,
                    });
                }
                match &a.body {
                    ArrowBody::Expr(e) => self.resolve_expr(e),
                    ArrowBody::Block(b) => self.resolve_block(b),
                }
                self.pop_scope();
            }
            Expr::Array(elements, _) => {
                for e in elements {
                    self.resolve_expr(e);
                }
            }
            Expr::Record(fields, _) => {
                for (_, v) in fields {
                    self.resolve_expr(v);
                }
            }
            Expr::Destructure(_) => {}
            Expr::Assign(lhs, rhs, _) => {
                self.resolve_expr(lhs);
                self.resolve_expr(rhs);
            }
        }
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------
// Public API
// ------------------------------------------------------------------

pub fn resolve(module: &Module) -> Vec<Diagnostic> {
    let mut resolver = Resolver::new();
    resolver.resolve_module(module);
    resolver.diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use typex_parser::parse;
    use typex_span::SourceMap;

    fn resolve_src(src: &str) -> Vec<Diagnostic> {
        let mut sm = SourceMap::new();
        let file = sm.add("test.tx".to_string(), src.to_string());
        let (module, parse_diags) = parse(src, file);
        assert!(parse_diags.is_empty(), "parse errors: {:?}", parse_diags);
        resolve(&module)
    }

    #[test]
    fn test_resolve_clean() {
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
        let diags = resolve_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }

    #[test]
    fn test_undefined_variable() {
        let src = r#"
            function main() {
                return foo;
            }
        "#;
        let diags = resolve_src(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("undefined identifier 'foo'"));
    }

    #[test]
    fn test_duplicate_declaration() {
        let src = r#"
            function foo() {}
            function foo() {}
        "#;
        let diags = resolve_src(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("duplicate declaration 'foo'"));
    }

    #[test]
    fn test_undefined_type() {
        let src = r#"
            function foo(x: MyUnknownType): int {
                return x;
            }
        "#;
        let diags = resolve_src(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unknown type 'MyUnknownType'"));
    }

    #[test]
    fn test_match_bindings_scoped() {
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
        let diags = resolve_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }

    #[test]
    fn test_let_scoping() {
        let src = r#"
            function main() {
                let x: int = 1;
                let y: int = x;
            }
        "#;
        let diags = resolve_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }

    #[test]
    fn test_undefined_export() {
        let src = r#"
            export { doesNotExist };
        "#;
        let diags = resolve_src(src);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains("export 'doesNotExist' is not defined")
        );
    }
}
