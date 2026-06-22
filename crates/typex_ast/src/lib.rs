use typex_span::Span;

/// A named identifier e.g. `foo`, `myVar`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// Top level - a single .tx file
#[derive(Debug, Clone)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

/// Top level items in a module
#[derive(Debug, Clone)]
pub enum Item {
    Function(FunctionDef),
    TypeAlias(TypeAlias),
    Enum(EnumDef),
    Import(Import),
    Export(Export),
    Const(ConstDef),
    Let(LetDef),
}

// ------------------------------------------------------------------
// Functions
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
    pub exported: bool,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

// ------------------------------------------------------------------
// Types
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(Ident),                  // e.g. `string`, `boolean`, `MyType`
    Generic(Ident, Vec<TypeExpr>), // e.g. `Array<string>`, `Result<int, string>`
    Union(Vec<TypeExpr>),          // e.g. `string | null`
    Nullable(Box<TypeExpr>),       // e.g. `string | null` sugar
}

#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub name: Ident,
    pub ty: TypeExpr,
    pub span: Span,
}

// ------------------------------------------------------------------
// Enums
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: Ident,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: Ident,
    pub value: Option<EnumValue>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum EnumValue {
    Int(i64),
    Str(String),
    Char(char),
}

// ------------------------------------------------------------------
// Imports / Exports
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Import {
    pub names: Vec<Ident>,
    pub from: String, // module path e.g. "./foo"
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Export {
    pub names: Vec<Ident>,
    pub span: Span,
}

// ------------------------------------------------------------------
// Statements
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(LetDef),
    Const(ConstDef),
    Expr(Expr),
    Return(Option<Expr>, Span),
    For(ForStmt),
    If(IfStmt),
    Switch(SwitchStmt),
    Match(MatchExpr),
}

#[derive(Debug, Clone)]
pub struct LetDef {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ConstDef {
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

// ------------------------------------------------------------------
// Control flow
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_block: Block,
    pub else_if: Vec<(Expr, Block)>,
    pub else_block: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SwitchStmt {
    pub value: Expr,
    pub cases: Vec<SwitchCase>,
    pub default: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct SwitchCase {
    pub value: Expr,
    pub body: Block,
    pub span: Span,
}

// ------------------------------------------------------------------
// Loops
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum ForStmt {
    // for (let {key, value} of object)
    Object {
        key: Option<Ident>,
        value: Option<Ident>,
        iter: Box<Expr>,
        body: Block,
        span: Span,
    },
    // for (let {index, value} in array)
    Array {
        index: Option<Ident>,
        value: Option<Ident>,
        iter: Box<Expr>,
        body: Block,
        span: Span,
    },
    // for (let {index, offset, value} in str)
    Str {
        index: Option<Ident>,
        offset: Option<Ident>,
        value: Option<Ident>,
        iter: Box<Expr>,
        body: Block,
        span: Span,
    },
}

// ------------------------------------------------------------------
// Match
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub value: Box<Expr>,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Ok(Ident),                         // Ok(x)
    Err(Ident),                        // Err(e)
    EnumVariant(Ident, Option<Ident>), // Variant(binding)
    Wildcard,                          // _
}

// ------------------------------------------------------------------
// Expressions
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    Lit(Lit),
    Ident(Ident),
    BinOp(Box<Expr>, BinOp, Box<Expr>, Span),
    UnaryOp(UnaryOp, Box<Expr>, Span),
    Call(Box<Expr>, Vec<Expr>, Span),               // fn(args)
    Index(Box<Expr>, Box<Expr>, Span),              // a[b]
    Field(Box<Expr>, Ident, Span),                  // a.b
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>, Span), // cond ? a : b
    Match(MatchExpr),
    Arrow(ArrowFn),
    Array(Vec<Expr>, Span),           // [a, b, c]
    Record(Vec<(Ident, Expr)>, Span), // { key: value }
    Destructure(Destructure),
    Assign(Box<Expr>, Box<Expr>, Span), // lhs = rhs
}

#[derive(Debug, Clone)]
pub enum Lit {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    Char(char, Span),
    Null(Span),
}

#[derive(Debug, Clone)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg, // -x
    Not, // !x
}

#[derive(Debug, Clone)]
pub struct ArrowFn {
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: ArrowBody,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ArrowBody {
    Block(Block),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Destructure {
    Array(Vec<Ident>, Span),                   // let [a, b] = ...
    Object(Vec<(Ident, Option<Ident>)>, Span), // let {a, b} = ...
}

#[cfg(test)]
mod tests {
    use super::*;
    use typex_span::{FileId, Pos, Span};

    fn dummy_span() -> Span {
        let id = FileId(0);
        let pos = Pos::new(1, 1, 0);
        Span::point(id, pos)
    }

    fn dummy_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: dummy_span(),
        }
    }

    #[test]
    fn test_function_def() {
        let f = FunctionDef {
            name: dummy_ident("main"),
            params: vec![],
            return_type: None,
            body: Block {
                stmts: vec![],
                span: dummy_span(),
            },
            span: dummy_span(),
            exported: false,
        };
        assert_eq!(f.name.name, "main");
        assert!(f.params.is_empty());
    }

    #[test]
    fn test_enum_def() {
        let e = EnumDef {
            name: dummy_ident("Colour"),
            variants: vec![
                EnumVariant {
                    name: dummy_ident("Red"),
                    value: Some(EnumValue::Str("#f00".to_string())),
                    span: dummy_span(),
                },
                EnumVariant {
                    name: dummy_ident("Green"),
                    value: Some(EnumValue::Str("#0f0".to_string())),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.name.name, "Colour");
    }

    #[test]
    fn test_union_type() {
        let ty = TypeExpr::Union(vec![
            TypeExpr::Named(dummy_ident("string")),
            TypeExpr::Named(dummy_ident("null")),
        ]);
        if let TypeExpr::Union(variants) = ty {
            assert_eq!(variants.len(), 2);
        } else {
            panic!("expected union type");
        }
    }

    #[test]
    fn test_literal_expr() {
        let expr = Expr::Lit(Lit::Int(42, dummy_span()));
        if let Expr::Lit(Lit::Int(n, _)) = expr {
            assert_eq!(n, 42);
        } else {
            panic!("expected int literal");
        }
    }

    #[test]
    fn test_match_expr() {
        let m = MatchExpr {
            value: Box::new(Expr::Ident(dummy_ident("result"))),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Ok(dummy_ident("val")),
                    body: Expr::Ident(dummy_ident("val")),
                    span: dummy_span(),
                },
                MatchArm {
                    pattern: Pattern::Err(dummy_ident("e")),
                    body: Expr::Ident(dummy_ident("e")),
                    span: dummy_span(),
                },
            ],
            span: dummy_span(),
        };
        assert_eq!(m.arms.len(), 2);
    }
}
