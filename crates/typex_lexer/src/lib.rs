use typex_span::{Diagnostic, FileId, Level, Pos, Span};

// ------------------------------------------------------------------
// Tokens
// ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    DocComment(String), // /** ... */

    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),

    // Identifiers & keywords
    Ident(String),

    // Keywords
    Let,
    Const,
    Function,
    Return,
    If,
    Else,
    Switch,
    Case,
    Default,
    For,
    In,
    Of,
    Match,
    Enum,
    Type,
    Import,
    Export,
    From,
    Null,
    Panic,

    // Symbols
    LParen,    // (
    RParen,    // )
    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    Comma,     // ,
    Semicolon, // ;
    Colon,     // :
    Dot,       // .
    Arrow,     // =>
    FatArrow,  // =>  (same, alias for clarity in parser)
    Question,  // ?
    Pipe,      // |

    // Operators
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /
    Percent, // %
    Bang,    // !
    Eq,      // ==
    NotEq,   // !=
    Assign,  // =
    Lt,      //
    Lte,     // <=
    Gt,      // >
    Gte,     // >=
    And,     // &&
    Or,      // ||

    // Special
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

// ------------------------------------------------------------------
// Lexer
// ------------------------------------------------------------------

pub struct Lexer<'a> {
    src: &'a str,
    file: FileId,
    pos: usize, // current byte offset
    line: u32,
    col: u32,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str, file: FileId) -> Self {
        Self {
            src,
            file,
            pos: 0,
            line: 1,
            col: 1,
            diagnostics: Vec::new(),
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    fn current_pos(&self) -> Pos {
        Pos::new(self.line, self.col, self.pos as u32)
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.peek() {
            self.advance();
            if ch == '\n' {
                break;
            }
        }
    }

    fn skip_block_comment(&mut self) {
        loop {
            match self.advance() {
                None => break, // unterminated block comment - just stop
                Some('*') => {
                    if self.peek() == Some('/') {
                        self.advance(); // consume closing /
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn make_span(&self, start: Pos) -> Span {
        Span::new(self.file, start, self.current_pos())
    }
    fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let start = self.current_pos();

        let ch = match self.peek() {
            None => return Token::new(TokenKind::Eof, self.make_span(start)),
            Some(ch) => ch,
        };

        self.advance(); // consume ch — exactly once

        let kind = match ch {
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semicolon,
            ':' => TokenKind::Colon,
            '.' => TokenKind::Dot,
            '?' => TokenKind::Question,
            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    TokenKind::Or
                } else {
                    TokenKind::Pipe
                }
            }
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '%' => TokenKind::Percent,
            '/' => {
                if self.peek() == Some('/') {
                    self.skip_line_comment();
                    return self.next_token();
                } else if self.peek() == Some('*') {
                    self.advance(); // consume *
                    // check for doc comment /**
                    if self.peek() == Some('*') {
                        self.advance(); // consume second *
                        let content = self.lex_doc_comment();
                        return Token::new(TokenKind::DocComment(content), self.make_span(start));
                    }
                    self.skip_block_comment();
                    return self.next_token();
                } else {
                    TokenKind::Slash
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Lte
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Gte
                } else {
                    TokenKind::Gt
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::NotEq
                } else {
                    TokenKind::Bang
                }
            }
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Eq
                } else if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::Arrow
                } else {
                    TokenKind::Assign
                }
            }
            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    TokenKind::And
                } else {
                    let span = self.make_span(start);
                    self.diagnostics.push(Diagnostic {
                        level: Level::Error,
                        span,
                        message: "unexpected character '&', did you mean '&&'?".to_string(),
                    });
                    return self.next_token();
                }
            }
            '"' => self.lex_string(start),
            '\'' => self.lex_char(start),
            c if c.is_ascii_digit() => self.lex_number(c, start),
            c if c.is_alphabetic() || c == '_' => self.lex_ident_or_keyword(c),
            other => {
                let span = self.make_span(start);
                self.diagnostics.push(Diagnostic {
                    level: Level::Error,
                    span,
                    message: format!("unexpected character '{}'", other),
                });
                return self.next_token();
            }
        };

        Token::new(kind, self.make_span(start))
    }

    fn lex_doc_comment(&mut self) -> String {
        let mut content = String::new();
        loop {
            match self.advance() {
                None => break,
                Some('*') => {
                    if self.peek() == Some('/') {
                        self.advance(); // consume /
                        break;
                    }
                    content.push('*');
                }
                Some(c) => content.push(c),
            }
        }
        // clean up leading * on each line
        content
            .lines()
            .map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with("* ") {
                    trimmed[2..].to_string()
                } else if trimmed.starts_with('*') {
                    trimmed[1..].to_string()
                } else {
                    trimmed.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }

    fn lex_string(&mut self, _start: Pos) -> TokenKind {
        let mut s = String::new();
        loop {
            match self.advance() {
                None | Some('\n') => {
                    // unterminated string - we'll add a diagnostic in a later pass
                    break;
                }
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some(c) => s.push(c),
                    None => break,
                },
                Some(c) => s.push(c),
            }
        }
        TokenKind::Str(s)
    }

    fn lex_char(&mut self, start: Pos) -> TokenKind {
        let ch = match self.advance() {
            Some('\\') => match self.advance() {
                Some('n') => '\n',
                Some('t') => '\t',
                Some('\\') => '\\',
                Some('\'') => '\'',
                Some(c) => c,
                None => ' ',
            },
            Some(c) => c,
            None => ' ',
        };
        // consume closing '
        if self.peek() == Some('\'') {
            self.advance();
        } else {
            let span = self.make_span(start);
            self.diagnostics.push(Diagnostic {
                level: Level::Error,
                span,
                message: "unterminated char literal".to_string(),
            });
        }
        TokenKind::Char(ch)
    }

    fn lex_number(&mut self, first: char, _start: Pos) -> TokenKind {
        let mut s = String::new();
        s.push(first);
        let mut is_float = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else if c == '.' && !is_float {
                is_float = true;
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            TokenKind::Float(s.parse().unwrap_or(0.0))
        } else {
            TokenKind::Int(s.parse().unwrap_or(0))
        }
    }

    fn lex_ident_or_keyword(&mut self, first: char) -> TokenKind {
        let mut s = String::new();
        s.push(first);
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        match s.as_str() {
            "let" => TokenKind::Let,
            "const" => TokenKind::Const,
            "function" => TokenKind::Function,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "switch" => TokenKind::Switch,
            "case" => TokenKind::Case,
            "default" => TokenKind::Default,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "of" => TokenKind::Of,
            "match" => TokenKind::Match,
            "enum" => TokenKind::Enum,
            "type" => TokenKind::Type,
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "from" => TokenKind::From,
            "null" => TokenKind::Null,
            "panic" => TokenKind::Panic,
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            _ => TokenKind::Ident(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typex_span::SourceMap;

    fn lex(src: &str) -> Vec<TokenKind> {
        let mut sm = SourceMap::new();
        let file = sm.add("test.tx".to_string(), src.to_string());
        let mut lexer = Lexer::new(src, file);
        lexer.tokenize().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn test_basic_tokens() {
        let tokens = lex("let x = 42;");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".to_string()),
                TokenKind::Assign,
                TokenKind::Int(42),
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_operators() {
        let tokens = lex("== != <= >= && ||");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Eq,
                TokenKind::NotEq,
                TokenKind::Lte,
                TokenKind::Gte,
                TokenKind::And,
                TokenKind::Or,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_string_literal() {
        let tokens = lex(r#""hello world""#);
        assert_eq!(
            tokens,
            vec![TokenKind::Str("hello world".to_string()), TokenKind::Eof,]
        );
    }

    #[test]
    fn test_char_literal() {
        let tokens = lex("'a'");
        assert_eq!(tokens, vec![TokenKind::Char('a'), TokenKind::Eof,]);
    }

    #[test]
    fn test_float() {
        let tokens = lex("3.14");
        assert_eq!(tokens, vec![TokenKind::Float(3.14), TokenKind::Eof,]);
    }

    #[test]
    fn test_keywords() {
        let tokens = lex("function return if else match");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Function,
                TokenKind::Return,
                TokenKind::If,
                TokenKind::Else,
                TokenKind::Match,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_line_comment() {
        let tokens = lex("let x = 1; // this is a comment\nlet y = 2;");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".to_string()),
                TokenKind::Assign,
                TokenKind::Int(1),
                TokenKind::Semicolon,
                TokenKind::Let,
                TokenKind::Ident("y".to_string()),
                TokenKind::Assign,
                TokenKind::Int(2),
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_arrow() {
        let tokens = lex("=>");
        assert_eq!(tokens, vec![TokenKind::Arrow, TokenKind::Eof,]);
    }

    #[test]
    fn test_boolean() {
        let tokens = lex("true false");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Bool(true),
                TokenKind::Bool(false),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_block_comment() {
        let tokens = lex("/* hello */ let x = 1;");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".to_string()),
                TokenKind::Assign,
                TokenKind::Int(1),
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_block_comment_multiline() {
        let tokens = lex("/* multi\n * line\n * comment\n */\nlet x = 1;");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".to_string()),
                TokenKind::Assign,
                TokenKind::Int(1),
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_block_comment_inline() {
        let tokens = lex("let /* comment */ x = 1;");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".to_string()),
                TokenKind::Assign,
                TokenKind::Int(1),
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_block_comment_unterminated() {
        // should not hang or panic, just return eof
        let tokens = lex("let x = /* unterminated");
        assert!(tokens.last() == Some(&TokenKind::Eof));
    }
}
