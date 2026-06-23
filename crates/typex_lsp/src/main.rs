use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use typex_parser::parse;
use typex_span::SourceMap;

// ------------------------------------------------------------------
// Backend
// ------------------------------------------------------------------

#[derive(Debug, Clone)]
struct SymbolInfo {
    name: String,
    kind: SymbolKind,
    ty: String,
    _span: typex_span::Span,
}

#[derive(Debug, Clone)]
enum SymbolKind {
    Function,
    Variable,
    Param,
    Type,
    Builtin,
}

struct Backend {
    client: Client,
    documents: Arc<Mutex<HashMap<String, String>>>,
    symbols: Arc<Mutex<HashMap<String, Vec<SymbolInfo>>>>, // uri -> symbols
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
            symbols: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn analyze(&self, uri: &str, text: &str) {
        let mut sm = SourceMap::new();
        let file = sm.add(uri.to_string(), text.to_string());

        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let mut symbol_list: Vec<SymbolInfo> = Vec::new();

        // add builtins
        for name in &["print", "println", "panic"] {
            symbol_list.push(SymbolInfo {
                name: name.to_string(),
                kind: SymbolKind::Builtin,
                ty: "builtin function".to_string(),
                _span: typex_span::Span::point(
                    typex_span::FileId(0),
                    typex_span::Pos::new(0, 0, 0),
                ),
            });
        }

        // add stdlib
        for (module, fns) in &[
            (
                "tx:fs",
                vec!["readFile", "writeFile", "exists", "deleteFile"],
            ),
            ("tx:io", vec!["readLine", "readLines"]),
            (
                "tx:math",
                vec![
                    "sqrt", "abs", "pow", "floor", "ceil", "round", "min", "max", "clamp",
                ],
            ),
            ("tx:process", vec!["exec", "exit"]),
            ("tx:env", vec!["getenv", "setenv", "args", "cwd"]),
        ] {
            for f in fns {
                symbol_list.push(SymbolInfo {
                    name: f.to_string(),
                    kind: SymbolKind::Builtin,
                    ty: format!("stdlib fn from {}", module),
                    _span: typex_span::Span::point(
                        typex_span::FileId(0),
                        typex_span::Pos::new(0, 0, 0),
                    ),
                });
            }
        }

        // Parse
        let (module, parse_diags) = parse(text, file);
        for diag in &parse_diags {
            if let Some(d) = to_lsp_diagnostic(diag) {
                diagnostics.push(d);
            }
        }

        // always collect symbols from whatever we could parse
        collect_symbols(&module, &mut symbol_list);

        if parse_diags
            .iter()
            .all(|d| d.level != typex_span::Level::Error)
        {
            // Resolve
            let resolve_diags = typex_resolve::resolve(&module);
            for diag in &resolve_diags {
                if let Some(d) = to_lsp_diagnostic(diag) {
                    diagnostics.push(d);
                }
            }

            // Typecheck
            let type_diags = typex_typecheck::typecheck(&module);
            for diag in &type_diags {
                let already_reported = resolve_diags.iter().any(|r| {
                    r.span.start.line == diag.span.start.line
                        && r.span.start.col == diag.span.start.col
                });
                if !already_reported {
                    if let Some(d) = to_lsp_diagnostic(diag) {
                        diagnostics.push(d);
                    }
                }
            }
        }

        // store symbols
        self.symbols
            .lock()
            .await
            .insert(uri.to_string(), symbol_list);

        self.client
            .publish_diagnostics(
                Url::parse(uri).unwrap_or_else(|_| Url::parse("file:///unknown").unwrap()),
                diagnostics,
                None,
            )
            .await;
    }
}

// ------------------------------------------------------------------
// LSP trait implementation
// ------------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "typex_lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "TypeX LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = params.text_document.text.clone();
        self.documents
            .lock()
            .await
            .insert(uri.clone(), text.clone());
        self.analyze(&uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().last() {
            let text = change.text.clone();
            self.documents
                .lock()
                .await
                .insert(uri.clone(), text.clone());
            self.analyze(&uri, &text).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(text) = self.documents.lock().await.get(&uri).cloned() {
            self.analyze(&uri, &text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.documents.lock().await.remove(&uri);
        // clear diagnostics on close
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let pos = params.text_document_position_params.position;

        let documents = self.documents.lock().await;
        let text = match documents.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(documents);

        // find the word under the cursor
        let word = word_at_position(&text, pos.line as usize, pos.character as usize);
        let word = match word {
            Some(w) => w,
            None => return Ok(None),
        };

        // look up in symbol table
        let symbols = self.symbols.lock().await;
        let symbol_list = match symbols.get(&uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };
        drop(symbols);

        for sym in &symbol_list {
            if sym.name == word {
                let kind_str = match sym.kind {
                    SymbolKind::Function => "function",
                    SymbolKind::Variable => "const/let",
                    SymbolKind::Param => "param",
                    SymbolKind::Type => "type",
                    SymbolKind::Builtin => "builtin",
                };
                let contents = format!("**{}** `{}`\n\n_{}_", word, sym.ty, kind_str);
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: contents,
                    }),
                    range: None,
                }));
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();

        let symbols = self.symbols.lock().await;
        let symbol_list = match symbols.get(&uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };
        drop(symbols);

        let items: Vec<CompletionItem> = symbol_list
            .iter()
            .map(|sym| {
                let kind = match sym.kind {
                    SymbolKind::Function => CompletionItemKind::FUNCTION,
                    SymbolKind::Variable => CompletionItemKind::VARIABLE,
                    SymbolKind::Param => CompletionItemKind::VARIABLE,
                    SymbolKind::Type => CompletionItemKind::CLASS,
                    SymbolKind::Builtin => CompletionItemKind::FUNCTION,
                };
                CompletionItem {
                    label: sym.name.clone(),
                    kind: Some(kind),
                    detail: Some(sym.ty.clone()),
                    insert_text: Some(sym.name.clone()),
                    filter_text: Some(sym.name.clone()),
                    ..Default::default()
                }
            })
            .collect();

        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        })))
    }
}

// ------------------------------------------------------------------
// Diagnostic conversion
// ------------------------------------------------------------------

fn to_lsp_diagnostic(diag: &typex_span::Diagnostic) -> Option<Diagnostic> {
    let severity = match diag.level {
        typex_span::Level::Error => DiagnosticSeverity::ERROR,
        typex_span::Level::Warning => DiagnosticSeverity::WARNING,
        typex_span::Level::Note => DiagnosticSeverity::INFORMATION,
    };

    let start_line = diag.span.start.line.saturating_sub(1);
    let start_char = diag.span.start.col.saturating_sub(1);
    let end_line = diag.span.end.line.saturating_sub(1);
    let end_char = diag.span.end.col.saturating_sub(1);

    let start = Position {
        line: start_line,
        character: start_char,
    };

    // if end is on a different line, clamp to end of start line
    // by using a reasonable token length instead
    let end = if end_line > start_line {
        // clamp to same line, extend by token length or reasonable default
        let token_len = diag.span.end.offset.saturating_sub(diag.span.start.offset);
        let clamped_end = if token_len > 0 && token_len < 80 {
            start_char + token_len
        } else {
            start_char + 1
        };
        Position {
            line: start_line,
            character: clamped_end,
        }
    } else if end_char <= start_char {
        Position {
            line: start_line,
            character: start_char + 1,
        }
    } else {
        Position {
            line: end_line,
            character: end_char,
        }
    };

    Some(Diagnostic {
        range: Range { start, end },
        severity: Some(severity),
        message: diag.message.clone(),
        source: Some("typex".to_string()),
        ..Default::default()
    })
}

fn collect_symbols(module: &typex_ast::Module, symbols: &mut Vec<SymbolInfo>) {
    for item in &module.items {
        match item {
            typex_ast::Item::Function(f) => {
                // add function itself
                let params: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| format!("{}: {}", p.name.name, type_expr_to_string(&p.ty)))
                    .collect();
                let ret = f
                    .return_type
                    .as_ref()
                    .map(|t| type_expr_to_string(t))
                    .unwrap_or("void".to_string());
                symbols.push(SymbolInfo {
                    name: f.name.name.clone(),
                    kind: SymbolKind::Function,
                    ty: format!("function({}) -> {}", params.join(", "), ret),
                    _span: f.span,
                });
                // add params
                for param in &f.params {
                    symbols.push(SymbolInfo {
                        name: param.name.name.clone(),
                        kind: SymbolKind::Param,
                        ty: type_expr_to_string(&param.ty),
                        _span: param.span,
                    });
                }
                // collect from body
                collect_block_symbols(&f.body, symbols);
            }
            typex_ast::Item::Const(c) => {
                let ty =
                    c.ty.as_ref()
                        .map(|t| type_expr_to_string(t))
                        .unwrap_or("unknown".to_string());
                symbols.push(SymbolInfo {
                    name: c.name.name.clone(),
                    kind: SymbolKind::Variable,
                    ty,
                    _span: c.span,
                });
            }
            typex_ast::Item::Let(l) => {
                let ty =
                    l.ty.as_ref()
                        .map(|t| type_expr_to_string(t))
                        .unwrap_or("unknown".to_string());
                symbols.push(SymbolInfo {
                    name: l.name.name.clone(),
                    kind: SymbolKind::Variable,
                    ty,
                    _span: l.span,
                });
            }
            typex_ast::Item::TypeAlias(t) => {
                symbols.push(SymbolInfo {
                    name: t.name.name.clone(),
                    kind: SymbolKind::Type,
                    ty: type_expr_to_string(&t.ty),
                    _span: t.span,
                });
            }
            _ => {}
        }
    }
}

fn collect_block_symbols(block: &typex_ast::Block, symbols: &mut Vec<SymbolInfo>) {
    for stmt in &block.stmts {
        match stmt {
            typex_ast::Stmt::Let(l) => {
                let ty =
                    l.ty.as_ref()
                        .map(|t| type_expr_to_string(t))
                        .unwrap_or("unknown".to_string());
                symbols.push(SymbolInfo {
                    name: l.name.name.clone(),
                    kind: SymbolKind::Variable,
                    ty,
                    _span: l.span,
                });
            }
            typex_ast::Stmt::Const(c) => {
                let ty =
                    c.ty.as_ref()
                        .map(|t| type_expr_to_string(t))
                        .unwrap_or("unknown".to_string());
                symbols.push(SymbolInfo {
                    name: c.name.name.clone(),
                    kind: SymbolKind::Variable,
                    ty,
                    _span: c.span,
                });
            }
            typex_ast::Stmt::If(i) => {
                collect_block_symbols(&i.then_block, symbols);
                for (_, block) in &i.else_if {
                    collect_block_symbols(block, symbols);
                }
                if let Some(else_block) = &i.else_block {
                    collect_block_symbols(else_block, symbols);
                }
            }
            typex_ast::Stmt::For(f) => match f {
                typex_ast::ForStmt::Array { body, .. } => collect_block_symbols(body, symbols),
                typex_ast::ForStmt::Object { body, .. } => collect_block_symbols(body, symbols),
                typex_ast::ForStmt::Str { body, .. } => collect_block_symbols(body, symbols),
            },
            _ => {}
        }
    }
}

fn word_at_position(text: &str, line: usize, col: usize) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line_text = lines.get(line)?;
    let chars: Vec<char> = line_text.chars().collect();

    if col >= chars.len() {
        return None;
    }

    // find word boundaries
    let mut start = col;
    let mut end = col;

    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    if start == end {
        return None;
    }

    Some(chars[start..end].iter().collect())
}

fn type_expr_to_string(ty: &typex_ast::TypeExpr) -> String {
    match ty {
        typex_ast::TypeExpr::Named(n) => n.name.clone(),
        typex_ast::TypeExpr::Generic(n, args) => {
            let args: Vec<String> = args.iter().map(type_expr_to_string).collect();
            format!("{}<{}>", n.name, args.join(", "))
        }
        typex_ast::TypeExpr::Union(variants) => variants
            .iter()
            .map(type_expr_to_string)
            .collect::<Vec<_>>()
            .join(" | "),
        typex_ast::TypeExpr::Nullable(inner) => {
            format!("{} | null", type_expr_to_string(inner))
        }
    }
}

// ------------------------------------------------------------------
// Entry point
// ------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
