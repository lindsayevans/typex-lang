use typex_ast::*;
use typex_lexer::{Lexer, Token, TokenKind};
use typex_span::{Diagnostic, FileId, Level, Span};

// ------------------------------------------------------------------
// Parser
// ------------------------------------------------------------------

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Token navigation
    // ------------------------------------------------------------------

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_token(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, kind: &TokenKind) -> Option<&Token> {
        if self.peek() == kind {
            Some(self.advance())
        } else {
            let span = self.peek_token().span;
            self.diagnostics.push(Diagnostic {
                level: Level::Error,
                span,
                message: format!("expected {:?}, got {:?}", kind, self.peek()),
            });
            None
        }
    }

    fn current_span(&self) -> Span {
        self.peek_token().span
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    // ------------------------------------------------------------------
    // Top level
    // ------------------------------------------------------------------

    pub fn parse_module(&mut self) -> Module {
        let start = self.current_span();
        let mut items = Vec::new();
        while self.peek() != &TokenKind::Eof {
            if let Some(item) = self.parse_item() {
                items.push(item);
            }
        }
        let span = start.to(self.current_span());
        Module { items, span }
    }

    fn parse_item(&mut self) -> Option<Item> {
        match self.peek() {
            TokenKind::Function => Some(Item::Function(self.parse_function(false)?)),
            TokenKind::Type => Some(Item::TypeAlias(self.parse_type_alias()?)),
            TokenKind::Enum => Some(Item::Enum(self.parse_enum()?)),
            TokenKind::Export => self.parse_export_item(),
            TokenKind::Import => Some(Item::Import(self.parse_import()?)),
            TokenKind::Const => Some(Item::Const(self.parse_const()?)),
            TokenKind::Let => Some(Item::Let(self.parse_let()?)),
            _ => {
                let span = self.current_span();
                self.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    span,
                    message: format!("unexpected token at top level: {:?}", self.peek()),
                });
                self.advance();
                None
            }
        }
    }

    // ------------------------------------------------------------------
    // Functions
    // ------------------------------------------------------------------

    fn parse_function(&mut self, exported: bool) -> Option<FunctionDef> {
        let start = self.current_span();
        self.expect(&TokenKind::Function)?;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params();
        self.expect(&TokenKind::RParen)?;
        let return_type = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let span = start.to(self.current_span());
        Some(FunctionDef {
            name,
            params,
            return_type,
            body,
            span,
            exported,
        })
    }

    fn parse_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        while self.peek() != &TokenKind::RParen && self.peek() != &TokenKind::Eof {
            if let Some(p) = self.parse_param() {
                params.push(p);
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        params
    }

    fn parse_param(&mut self) -> Option<Param> {
        let start = self.current_span();
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type_expr()?;
        let span = start.to(self.current_span());
        Some(Param { name, ty, span })
    }

    // ------------------------------------------------------------------
    // Types
    // ------------------------------------------------------------------

    fn parse_type_expr(&mut self) -> Option<TypeExpr> {
        let base = self.parse_base_type()?;
        // union types: string | null
        if self.eat(&TokenKind::Pipe) {
            let mut variants = vec![base];
            loop {
                if let Some(ty) = self.parse_base_type() {
                    variants.push(ty);
                }
                if !self.eat(&TokenKind::Pipe) {
                    break;
                }
            }
            Some(TypeExpr::Union(variants))
        } else {
            Some(base)
        }
    }

    fn parse_base_type(&mut self) -> Option<TypeExpr> {
        let span = self.current_span();
        // Allow keywords that are valid type names
        let name = match self.peek().clone() {
            TokenKind::Null => {
                self.advance();
                Ident {
                    name: "null".to_string(),
                    span,
                }
            }
            _ => self.parse_ident()?,
        };
        // Generic: Array<string>, Result<int, string>
        if self.eat(&TokenKind::Lt) {
            let mut args = Vec::new();
            while self.peek() != &TokenKind::Gt && self.peek() != &TokenKind::Eof {
                if let Some(ty) = self.parse_type_expr() {
                    args.push(ty);
                }
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
            self.expect(&TokenKind::Gt)?;
            Some(TypeExpr::Generic(name, args))
        } else {
            Some(TypeExpr::Named(name))
        }
    }

    fn parse_type_alias(&mut self) -> Option<TypeAlias> {
        let start = self.current_span();
        self.expect(&TokenKind::Type)?;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Assign)?;
        let ty = self.parse_type_expr()?;
        self.expect(&TokenKind::Semicolon)?;
        let span = start.to(self.current_span());
        Some(TypeAlias { name, ty, span })
    }

    // ------------------------------------------------------------------
    // Enums
    // ------------------------------------------------------------------

    fn parse_enum(&mut self) -> Option<EnumDef> {
        let start = self.current_span();
        self.expect(&TokenKind::Enum)?;
        let name = self.parse_ident()?;
        self.expect(&TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            if let Some(v) = self.parse_enum_variant() {
                variants.push(v);
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        let span = start.to(self.current_span());
        Some(EnumDef {
            name,
            variants,
            span,
        })
    }

    fn parse_enum_variant(&mut self) -> Option<EnumVariant> {
        let start = self.current_span();
        let name = self.parse_ident()?;
        let value = if self.eat(&TokenKind::Assign) {
            match self.peek().clone() {
                TokenKind::Int(n) => {
                    let n = n;
                    self.advance();
                    Some(EnumValue::Int(n))
                }
                TokenKind::Str(s) => {
                    let s = s.clone();
                    self.advance();
                    Some(EnumValue::Str(s))
                }
                TokenKind::Char(c) => {
                    let c = c;
                    self.advance();
                    Some(EnumValue::Char(c))
                }
                _ => None,
            }
        } else {
            None
        };
        let span = start.to(self.current_span());
        Some(EnumVariant { name, value, span })
    }

    // ------------------------------------------------------------------
    // Imports / Exports
    // ------------------------------------------------------------------

    fn parse_import(&mut self) -> Option<Import> {
        let start = self.current_span();
        self.expect(&TokenKind::Import)?;
        self.expect(&TokenKind::LBrace)?;
        let mut names = Vec::new();
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            if let Some(name) = self.parse_ident() {
                names.push(name);
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        self.expect(&TokenKind::From)?;
        let from = match self.peek().clone() {
            TokenKind::Str(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                let span = self.current_span();
                self.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    span,
                    message: "expected module path string".to_string(),
                });
                return None;
            }
        };
        self.expect(&TokenKind::Semicolon)?;
        let span = start.to(self.current_span());
        Some(Import { names, from, span })
    }

    fn parse_export_item(&mut self) -> Option<Item> {
        self.expect(&TokenKind::Export)?;
        match self.peek() {
            TokenKind::Function => Some(Item::Function(self.parse_function(true)?)),
            TokenKind::LBrace => Some(Item::Export(self.parse_export_names()?)),
            _ => {
                let span = self.current_span();
                self.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    span,
                    message: format!(
                        "expected function or {{ after export, got {:?}",
                        self.peek()
                    ),
                });
                None
            }
        }
    }

    fn parse_export_names(&mut self) -> Option<Export> {
        let start = self.current_span();
        self.expect(&TokenKind::LBrace)?;
        let mut names = Vec::new();
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            if let Some(name) = self.parse_ident() {
                names.push(name);
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        self.expect(&TokenKind::Semicolon)?;
        let span = start.to(self.current_span());
        Some(Export { names, span })
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn parse_block(&mut self) -> Option<Block> {
        let start = self.current_span();
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            if let Some(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            }
        }
        self.expect(&TokenKind::RBrace)?;
        let span = start.to(self.current_span());
        Some(Block { stmts, span })
    }

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.peek() {
            TokenKind::Let => Some(Stmt::Let(self.parse_let()?)),
            TokenKind::Const => Some(Stmt::Const(self.parse_const()?)),
            TokenKind::Return => self.parse_return(),
            TokenKind::If => Some(Stmt::If(self.parse_if()?)),
            TokenKind::Switch => Some(Stmt::Switch(self.parse_switch()?)),
            TokenKind::For => Some(Stmt::For(self.parse_for()?)),
            TokenKind::Match => Some(Stmt::Match(self.parse_match()?)),
            _ => {
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::Semicolon)?;
                Some(Stmt::Expr(expr))
            }
        }
    }

    fn parse_let(&mut self) -> Option<LetDef> {
        let start = self.current_span();
        self.expect(&TokenKind::Let)?;
        let name = self.parse_ident()?;
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let value = if self.eat(&TokenKind::Assign) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Semicolon)?;
        let span = start.to(self.current_span());
        Some(LetDef {
            name,
            ty,
            value,
            span,
        })
    }

    fn parse_const(&mut self) -> Option<ConstDef> {
        let start = self.current_span();
        self.expect(&TokenKind::Const)?;
        let name = self.parse_ident()?;
        let ty = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Assign)?;
        let value = self.parse_expr()?;
        self.expect(&TokenKind::Semicolon)?;
        let span = start.to(self.current_span());
        Some(ConstDef {
            name,
            ty,
            value,
            span,
        })
    }

    fn parse_return(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.expect(&TokenKind::Return)?;
        let value = if self.peek() != &TokenKind::Semicolon {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Semicolon)?;
        let span = start.to(self.current_span());
        Some(Stmt::Return(value, span))
    }

    // ------------------------------------------------------------------
    // Control flow
    // ------------------------------------------------------------------

    fn parse_if(&mut self) -> Option<IfStmt> {
        let start = self.current_span();
        self.expect(&TokenKind::If)?;
        self.expect(&TokenKind::LParen)?;
        let condition = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        let then_block = self.parse_block()?;
        let mut else_if = Vec::new();
        let mut else_block = None;
        while self.eat(&TokenKind::Else) {
            if self.eat(&TokenKind::If) {
                self.expect(&TokenKind::LParen)?;
                let cond = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                let block = self.parse_block()?;
                else_if.push((cond, block));
            } else {
                else_block = Some(self.parse_block()?);
                break;
            }
        }
        let span = start.to(self.current_span());
        Some(IfStmt {
            condition,
            then_block,
            else_if,
            else_block,
            span,
        })
    }

    fn parse_switch(&mut self) -> Option<SwitchStmt> {
        let start = self.current_span();
        self.expect(&TokenKind::Switch)?;
        self.expect(&TokenKind::LParen)?;
        let value = self.parse_expr()?;
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::LBrace)?;
        let mut cases = Vec::new();
        let mut default = None;
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            if self.eat(&TokenKind::Default) {
                self.expect(&TokenKind::Colon)?;
                let body = self.parse_case_body();
                default = Some(body);
            } else {
                self.expect(&TokenKind::Case)?;
                let case_start = self.current_span();
                let case_value = self.parse_expr()?;
                self.expect(&TokenKind::Colon)?;
                let body = self.parse_case_body();
                let case_span = case_start.to(self.current_span());
                cases.push(SwitchCase {
                    value: case_value,
                    body,
                    span: case_span,
                });
            }
        }
        self.expect(&TokenKind::RBrace)?;
        let span = start.to(self.current_span());
        Some(SwitchStmt {
            value,
            cases,
            default,
            span,
        })
    }

    fn parse_case_body(&mut self) -> Block {
        let start = self.current_span();
        let mut stmts = Vec::new();
        while self.peek() != &TokenKind::Case
            && self.peek() != &TokenKind::Default
            && self.peek() != &TokenKind::RBrace
            && self.peek() != &TokenKind::Eof
        {
            if let Some(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            }
        }
        let span = start.to(self.current_span());
        Block { stmts, span }
    }

    // ------------------------------------------------------------------
    // Loops
    // ------------------------------------------------------------------

    fn parse_for(&mut self) -> Option<ForStmt> {
        let start = self.current_span();
        self.expect(&TokenKind::For)?;
        self.expect(&TokenKind::LParen)?;
        self.expect(&TokenKind::Let)?;
        self.expect(&TokenKind::LBrace)?;

        // collect destructured field names
        let mut fields: Vec<String> = Vec::new();
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            if let Some(ident) = self.parse_ident() {
                fields.push(ident.name);
            }
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;

        // in = array/string, of = object
        let is_array = self.eat(&TokenKind::In);
        if !is_array {
            self.expect(&TokenKind::Of)?;
        }

        let iter = Box::new(self.parse_expr()?);
        self.expect(&TokenKind::RParen)?;
        let body = self.parse_block()?;
        let span = start.to(self.current_span());

        let get = |fields: &Vec<String>, name: &str| -> Option<Ident> {
            if fields.contains(&name.to_string()) {
                Some(Ident {
                    name: name.to_string(),
                    span,
                })
            } else {
                None
            }
        };

        if is_array {
            // could be string iteration (index, offset, value) or array (index, value)
            let offset = get(&fields, "offset");
            if offset.is_some() {
                Some(ForStmt::Str {
                    index: get(&fields, "index"),
                    offset,
                    value: get(&fields, "value"),
                    iter,
                    body,
                    span,
                })
            } else {
                Some(ForStmt::Array {
                    index: get(&fields, "index"),
                    value: get(&fields, "value"),
                    iter,
                    body,
                    span,
                })
            }
        } else {
            Some(ForStmt::Object {
                key: get(&fields, "key"),
                value: get(&fields, "value"),
                iter,
                body,
                span,
            })
        }
    }

    // ------------------------------------------------------------------
    // Match
    // ------------------------------------------------------------------

    fn parse_match(&mut self) -> Option<MatchExpr> {
        let start = self.current_span();
        self.expect(&TokenKind::Match)?;
        let value = Box::new(self.parse_expr()?);
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            if let Some(arm) = self.parse_match_arm() {
                arms.push(arm);
            }
            self.eat(&TokenKind::Comma);
        }
        self.expect(&TokenKind::RBrace)?;
        let span = start.to(self.current_span());
        Some(MatchExpr { value, arms, span })
    }

    fn parse_match_arm(&mut self) -> Option<MatchArm> {
        let start = self.current_span();
        let pattern = self.parse_pattern()?;
        self.expect(&TokenKind::Arrow)?;
        let body = self.parse_expr()?;
        let span = start.to(self.current_span());
        Some(MatchArm {
            pattern,
            body,
            span,
        })
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        let name = self.parse_ident()?;
        match name.name.as_str() {
            "Ok" => {
                self.expect(&TokenKind::LParen)?;
                let binding = self.parse_ident()?;
                self.expect(&TokenKind::RParen)?;
                Some(Pattern::Ok(binding))
            }
            "Err" => {
                self.expect(&TokenKind::LParen)?;
                let binding = self.parse_ident()?;
                self.expect(&TokenKind::RParen)?;
                Some(Pattern::Err(binding))
            }
            "_" => Some(Pattern::Wildcard),
            _ => {
                // enum variant with optional binding
                let binding = if self.eat(&TokenKind::LParen) {
                    let b = self.parse_ident();
                    self.expect(&TokenKind::RParen)?;
                    b
                } else {
                    None
                };
                Some(Pattern::EnumVariant(name, binding))
            }
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------
    fn parse_expr(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let expr = self.parse_ternary()?;

        // check for assignment
        if self.eat(&TokenKind::Assign) {
            let rhs = self.parse_expr()?;
            let span = start.to(self.current_span());
            return Some(Expr::Assign(Box::new(expr), Box::new(rhs), span));
        }

        Some(expr)
    }

    fn parse_ternary(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let cond = self.parse_or()?;
        if self.eat(&TokenKind::Question) {
            let then = self.parse_expr()?;
            self.expect(&TokenKind::Colon)?;
            let else_ = self.parse_expr()?;
            let span = start.to(self.current_span());
            Some(Expr::Ternary(
                Box::new(cond),
                Box::new(then),
                Box::new(else_),
                span,
            ))
        } else {
            Some(cond)
        }
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_and()?;
        while self.eat(&TokenKind::Or) {
            let right = self.parse_and()?;
            let span = start.to(self.current_span());
            left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right), span);
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_equality()?;
        while self.eat(&TokenKind::And) {
            let right = self.parse_equality()?;
            let span = start.to(self.current_span());
            left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(right), span);
        }
        Some(left)
    }

    fn parse_equality(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                TokenKind::Eq => BinOp::Eq,
                TokenKind::NotEq => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            let span = start.to(self.current_span());
            left = Expr::BinOp(Box::new(left), op, Box::new(right), span);
        }
        Some(left)
    }

    fn parse_comparison(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Lte => BinOp::Lte,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Gte => BinOp::Gte,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = start.to(self.current_span());
            left = Expr::BinOp(Box::new(left), op, Box::new(right), span);
        }
        Some(left)
    }

    fn parse_additive(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = start.to(self.current_span());
            left = Expr::BinOp(Box::new(left), op, Box::new(right), span);
        }
        Some(left)
    }

    fn parse_multiplicative(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            let span = start.to(self.current_span());
            left = Expr::BinOp(Box::new(left), op, Box::new(right), span);
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let start = self.current_span();
        match self.peek() {
            TokenKind::Bang => {
                self.advance();
                let expr = self.parse_unary()?;
                let span = start.to(self.current_span());
                Some(Expr::UnaryOp(UnaryOp::Not, Box::new(expr), span))
            }
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_unary()?;
                let span = start.to(self.current_span());
                Some(Expr::UnaryOp(UnaryOp::Neg, Box::new(expr), span))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.advance();
                    let field = self.parse_ident()?;
                    let span = start.to(self.current_span());
                    expr = Expr::Field(Box::new(expr), field, span);
                }
                TokenKind::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek() != &TokenKind::RParen && self.peek() != &TokenKind::Eof {
                        if let Some(arg) = self.parse_expr() {
                            args.push(arg);
                        }
                        if !self.eat(&TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(&TokenKind::RParen)?;
                    let span = start.to(self.current_span());
                    expr = Expr::Call(Box::new(expr), args, span);
                }
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(&TokenKind::RBracket)?;
                    let span = start.to(self.current_span());
                    expr = Expr::Index(Box::new(expr), Box::new(index), span);
                }
                _ => break,
            }
        }
        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let start = self.current_span();
        match self.peek().clone() {
            TokenKind::Int(n) => {
                let n = n;
                self.advance();
                Some(Expr::Lit(Lit::Int(n, start)))
            }
            TokenKind::Float(f) => {
                let f = f;
                self.advance();
                Some(Expr::Lit(Lit::Float(f, start)))
            }
            TokenKind::Bool(b) => {
                let b = b;
                self.advance();
                Some(Expr::Lit(Lit::Bool(b, start)))
            }
            TokenKind::Str(s) => {
                let s = s.clone();
                self.advance();
                Some(Expr::Lit(Lit::Str(s, start)))
            }
            TokenKind::Char(c) => {
                let c = c;
                self.advance();
                Some(Expr::Lit(Lit::Char(c, start)))
            }
            TokenKind::Null => {
                self.advance();
                Some(Expr::Lit(Lit::Null(start)))
            }
            TokenKind::LParen => {
                // lookahead: is this an arrow function or a grouped expression?
                // arrow function: ( ident : type ... ) : type =>
                // grouped expr:   ( expr )
                if self.is_arrow_fn() {
                    Some(Expr::Arrow(self.parse_arrow_fn()?))
                } else {
                    self.advance();
                    let expr = self.parse_expr()?;
                    self.expect(&TokenKind::RParen)?;
                    Some(expr)
                }
            }
            TokenKind::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                while self.peek() != &TokenKind::RBracket && self.peek() != &TokenKind::Eof {
                    if let Some(e) = self.parse_expr() {
                        elements.push(e);
                    }
                    if !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RBracket)?;
                let span = start.to(self.current_span());
                Some(Expr::Array(elements, span))
            }
            TokenKind::Match => Some(Expr::Match(self.parse_match()?)),
            TokenKind::Ident(_) => {
                let ident = self.parse_ident()?;
                Some(Expr::Ident(ident))
            }
            TokenKind::Panic => {
                let span = self.current_span();
                self.advance();
                Some(Expr::Ident(Ident {
                    name: "panic".to_string(),
                    span,
                }))
            }
            TokenKind::LBrace => {
                if self.is_record_literal() {
                    Some(self.parse_record_literal()?)
                } else {
                    // block expression - not supported in this position
                    let span = self.current_span();
                    self.diagnostics.push(Diagnostic {
                        level: Level::Error,
                        span,
                        message: "unexpected block in expression position".to_string(),
                    });
                    None
                }
            }
            other => {
                let span = self.current_span();
                self.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    span,
                    message: format!("unexpected token in expression: {:?}", other),
                });
                self.advance();
                None
            }
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn parse_ident(&mut self) -> Option<Ident> {
        let span = self.current_span();
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Some(Ident { name, span })
            }
            other => {
                self.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    span,
                    message: format!("expected identifier, got {:?}", other),
                });
                None
            }
        }
    }

    fn is_arrow_fn(&self) -> bool {
        // peek ahead to see if this looks like (ident: type, ...) : type =>
        // simple heuristic: look for pattern ( ident : or ( )
        let i = self.pos + 1; // skip the (
        if i >= self.tokens.len() {
            return false;
        }
        // empty params: () =>
        if matches!(self.tokens[i].kind, TokenKind::RParen) {
            // check for ) : type => or ) =>
            let j = i + 1;
            if j >= self.tokens.len() {
                return false;
            }
            if matches!(self.tokens[j].kind, TokenKind::Arrow) {
                return true;
            }
            if matches!(self.tokens[j].kind, TokenKind::Colon) {
                // ) : type =>
                let k = j + 2; // skip type name
                if k >= self.tokens.len() {
                    return false;
                }
                return matches!(self.tokens[k].kind, TokenKind::Arrow);
            }
            return false;
        }
        // ( ident : ... ) => pattern
        if matches!(self.tokens[i].kind, TokenKind::Ident(_)) {
            let j = i + 1;
            if j >= self.tokens.len() {
                return false;
            }
            return matches!(self.tokens[j].kind, TokenKind::Colon);
        }
        false
    }

    fn parse_arrow_fn(&mut self) -> Option<ArrowFn> {
        let start = self.current_span();
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params();
        self.expect(&TokenKind::RParen)?;
        let return_type = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(&TokenKind::Arrow)?;
        // block or expression body
        let body = if self.peek() == &TokenKind::LBrace {
            ArrowBody::Block(self.parse_block()?)
        } else {
            ArrowBody::Expr(Box::new(self.parse_expr()?))
        };
        let span = start.to(self.current_span());
        Some(ArrowFn {
            params,
            return_type,
            body,
            span,
        })
    }

    fn is_record_literal(&self) -> bool {
        // lookahead: { ident : ... } is a record literal
        // { } is an empty record
        let i = self.pos + 1; // skip {
        if i >= self.tokens.len() {
            return false;
        }
        // empty record: {}
        if matches!(self.tokens[i].kind, TokenKind::RBrace) {
            return true;
        }
        // { ident : value }
        if matches!(self.tokens[i].kind, TokenKind::Ident(_)) {
            let j = i + 1;
            if j >= self.tokens.len() {
                return false;
            }
            return matches!(self.tokens[j].kind, TokenKind::Colon);
        }
        false
    }

    fn parse_record_literal(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while self.peek() != &TokenKind::RBrace && self.peek() != &TokenKind::Eof {
            let key = self.parse_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((key, value));
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        let span = start.to(self.current_span());
        Some(Expr::Record(fields, span))
    }
}

// ------------------------------------------------------------------
// Public API
// ------------------------------------------------------------------

pub fn parse(src: &str, file: FileId) -> (Module, Vec<Diagnostic>) {
    let mut lexer = Lexer::new(src, file);
    let tokens = lexer.tokenize();
    let mut diagnostics = lexer.diagnostics;
    let mut parser = Parser::new(tokens);
    let module = parser.parse_module();
    diagnostics.extend(parser.diagnostics);
    (module, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use typex_span::SourceMap;

    fn parse_src(src: &str) -> (Module, Vec<Diagnostic>) {
        let mut sm = SourceMap::new();
        let file = sm.add("test.tx".to_string(), src.to_string());
        parse(src, file)
    }

    #[test]
    fn test_parse_const() {
        let (module, diags) = parse_src("const x = 42;");
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        assert_eq!(module.items.len(), 1);
        assert!(matches!(module.items[0], Item::Const(_)));
    }

    #[test]
    fn test_parse_function() {
        let src = r#"
            function add(a: int, b: int): int {
                return a + b;
            }
        "#;
        let (module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        assert_eq!(module.items.len(), 1);
        if let Item::Function(f) = &module.items[0] {
            assert_eq!(f.name.name, "add");
            assert_eq!(f.params.len(), 2);
        } else {
            panic!("expected function");
        }
    }

    #[test]
    fn test_parse_if_else() {
        let src = r#"
            function main() {
                if (x == 1) {
                    return 1;
                } else {
                    return 0;
                }
            }
        "#;
        let (module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        assert_eq!(module.items.len(), 1);
    }

    #[test]
    fn test_parse_match() {
        let src = r#"
            function main() {
                match divide(10, 0) {
                    Ok(result) => result,
                    Err(error) => error,
                }
            }
        "#;
        let (_module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }

    #[test]
    fn test_parse_enum() {
        let src = r##"
            enum Colour {
                Red = "#f00",
                Green = "#0f0",
                Blue = "#00f",
            }
        "##;
        let (module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        if let Item::Enum(e) = &module.items[0] {
            assert_eq!(e.name.name, "Colour");
            assert_eq!(e.variants.len(), 3);
        } else {
            panic!("expected enum");
        }
    }

    #[test]
    fn test_parse_import() {
        let src = r#"import { foo, bar } from "./mymodule";"#;
        let (module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        if let Item::Import(i) = &module.items[0] {
            assert_eq!(i.names.len(), 2);
            assert_eq!(i.from, "./mymodule");
        } else {
            panic!("expected import");
        }
    }

    #[test]
    fn test_parse_type_alias() {
        let src = r#"type Status = string | null;"#;
        let (module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        if let Item::TypeAlias(t) = &module.items[0] {
            assert_eq!(t.name.name, "Status");
            assert!(matches!(t.ty, TypeExpr::Union(_)));
        } else {
            panic!("expected type alias");
        }
    }

    #[test]
    fn test_parse_ternary() {
        let src = r#"const status = age >= 18 ? "Adult" : "Minor";"#;
        let (module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        if let Item::Const(c) = &module.items[0] {
            assert!(matches!(c.value, Expr::Ternary(_, _, _, _)));
        } else {
            panic!("expected const");
        }
    }

    #[test]
    fn test_parse_divide_example() {
        let src = r#"
            function divide(numerator: int, denominator: int): Result<int, string> {
                if (denominator == 0) {
                    return Err("Cannot divide by zero!");
                }
                return Ok(numerator / denominator);
            }
            function main() {
                match divide(10, 0) {
                    Ok(result) => println("Success!"),
                    Err(error) => println("Error occurred"),
                }
            }
        "#;
        let (module, diags) = parse_src(src);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
        assert_eq!(module.items.len(), 2);
    }
}
