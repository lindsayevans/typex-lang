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
    span: typex_span::Span,
    doc: Option<String>,
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
    builtin_symbols: Vec<SymbolInfo>,
}

impl Backend {
    fn new(client: Client) -> Self {
        let builtin_symbols = build_builtin_symbols();
        Self {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
            symbols: Arc::new(Mutex::new(HashMap::new())),
            builtin_symbols,
        }
    }

    async fn analyze(&self, uri: &str, text: &str) {
        // skip empty files
        if text.trim().is_empty() {
            return;
        }

        let mut sm = SourceMap::new();
        let file = sm.add(uri.to_string(), text.to_string());

        // start with pre-built builtins
        let mut symbol_list = self.builtin_symbols.clone();

        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // add builtins
        for name in &["print", "println", "panic"] {
            symbol_list.push(SymbolInfo {
                name: name.to_string(),
                kind: SymbolKind::Builtin,
                ty: "builtin function".to_string(),
                span: typex_span::Span::point(typex_span::FileId(0), typex_span::Pos::new(0, 0, 0)),
                doc: None,
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
                    span: typex_span::Span::point(
                        typex_span::FileId(0),
                        typex_span::Pos::new(0, 0, 0),
                    ),
                    doc: None,
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

    async fn complete_import(
        &self,
        text: &str,
        uri: &str,
        pos: Position,
    ) -> Option<Vec<CompletionItem>> {
        let lines: Vec<&str> = text.lines().collect();
        let line = lines.get(pos.line as usize)?;
        let col = pos.character as usize;

        // check if we're on an import line
        let trimmed = line.trim();
        if !trimmed.starts_with("import") {
            return None;
        }

        // check if cursor is inside the from "..." string
        let before_cursor = &line[..col.min(line.len())];

        // are we inside the from string?
        if !before_cursor.contains("from \"") {
            return None;
        }

        // extract what's been typed so far in the string
        let from_pos = before_cursor.rfind("from \"")?;
        let typed = &before_cursor[from_pos + 6..]; // after 'from "'

        if typed.starts_with("tx:") || typed.is_empty() {
            // stdlib completions
            return Some(self.stdlib_import_completions());
        }

        if typed.starts_with('.') || typed.starts_with('/') {
            // file completions
            return Some(self.file_import_completions(uri, typed).await);
        }

        // show both
        let mut items = self.stdlib_import_completions();
        items.extend(self.file_import_completions(uri, typed).await);
        Some(items)
    }

    fn stdlib_import_completions(&self) -> Vec<CompletionItem> {
        let modules = vec![
            ("tx:fs", "readFile, writeFile, exists, deleteFile"),
            ("tx:io", "readLine, readLines"),
            (
                "tx:math",
                "sqrt, abs, pow, floor, ceil, round, min, max, clamp",
            ),
            ("tx:process", "exec, exit"),
            ("tx:env", "getenv, setenv, args, cwd"),
        ];

        modules
            .iter()
            .map(|(module, fns)| {
                CompletionItem {
                    label: module.to_string(),
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(fns.to_string()),
                    insert_text: Some(module.to_string()),
                    filter_text: Some(module.to_string()),
                    // also insert the closing quote
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                }
            })
            .collect()
    }

    async fn file_import_completions(&self, uri: &str, typed: &str) -> Vec<CompletionItem> {
        // resolve current file's directory
        let file_path = uri.trim_start_matches("file://");
        let current_dir = std::path::Path::new(file_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));

        // resolve the typed path prefix
        let search_dir = if typed.is_empty() || typed == "." || typed == "./" {
            current_dir.to_path_buf()
        } else {
            let prefix = typed.trim_end_matches(|c| c != '/' && c != '\\');
            current_dir.join(prefix)
        };

        let mut items = Vec::new();

        let entries = match std::fs::read_dir(&search_dir) {
            Ok(e) => e,
            Err(_) => return items,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };

            // skip hidden files
            if name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                // directory — add with trailing /
                let label = format!("./{}/", name);
                items.push(CompletionItem {
                    label: label.clone(),
                    kind: Some(CompletionItemKind::FOLDER),
                    detail: Some("directory".to_string()),
                    insert_text: Some(label.clone()),
                    filter_text: Some(label),
                    ..Default::default()
                });
            } else if name.ends_with(".tx") {
                // .tx file
                let label = format!("./{}", name);
                items.push(CompletionItem {
                    label: label.clone(),
                    kind: Some(CompletionItemKind::FILE),
                    detail: Some("TypeX module".to_string()),
                    insert_text: Some(label.clone()),
                    filter_text: Some(label),
                    ..Default::default()
                });
            }
        }

        // sort: directories first, then files
        items.sort_by(|a, b| {
            let a_is_dir = matches!(a.kind, Some(CompletionItemKind::FOLDER));
            let b_is_dir = matches!(b.kind, Some(CompletionItemKind::FOLDER));
            b_is_dir.cmp(&a_is_dir).then(a.label.cmp(&b.label))
        });

        items
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
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
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
        self.symbols.lock().await.remove(&uri);
        // clear diagnostics
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

                let mut contents = format!("**{}** `{}`\n\n_{}_", word, sym.ty, kind_str);

                if let Some(doc) = &sym.doc {
                    contents.push_str(&format!("\n\n---\n\n{}", doc));
                }

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
        let pos = params.text_document_position.position;

        let documents = self.documents.lock().await;
        let text = match documents.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(documents);

        // check for import context first
        if let Some(items) = self.complete_import(&text, &uri, pos).await {
            return Ok(Some(CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items,
            })));
        }

        let symbols = self.symbols.lock().await;
        let symbol_list = match symbols.get(&uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };
        drop(symbols);

        let existing_imports = get_existing_imports(&text);

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

                // check if this is a stdlib function that needs importing
                let additional_edits = if let SymbolKind::Builtin = sym.kind {
                    if sym.ty.starts_with("stdlib fn from ") {
                        let module = sym.ty.trim_start_matches("stdlib fn from ");
                        make_import_edit(&text, &existing_imports, module, &sym.name)
                    } else {
                        None
                    }
                } else {
                    None
                };

                CompletionItem {
                    label: sym.name.clone(),
                    kind: Some(kind),
                    detail: Some(sym.ty.clone()),
                    insert_text: Some(sym.name.clone()),
                    filter_text: Some(sym.name.clone()),
                    sort_text: Some(sym.name.clone()),
                    additional_text_edits: additional_edits,
                    ..Default::default()
                }
            })
            .collect();

        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items,
        })))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
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

        let word = match word_at_position(&text, pos.line as usize, pos.character as usize) {
            Some(w) => w,
            None => return Ok(None),
        };

        let symbols = self.symbols.lock().await;
        let symbol_list = match symbols.get(&uri) {
            Some(s) => s.clone(),
            None => return Ok(None),
        };
        drop(symbols);

        // prefer function/type declarations over variables
        let decl_priority = |kind: &SymbolKind| match kind {
            SymbolKind::Function => 0,
            SymbolKind::Type => 1,
            SymbolKind::Variable => 2,
            SymbolKind::Param => 3,
            SymbolKind::Builtin => 99,
        };

        let mut best: Option<&SymbolInfo> = None;
        for sym in &symbol_list {
            if sym.name == word && !matches!(sym.kind, SymbolKind::Builtin) {
                match best {
                    None => best = Some(sym),
                    Some(b) => {
                        if decl_priority(&sym.kind) < decl_priority(&b.kind) {
                            best = Some(sym);
                        }
                    }
                }
            }
        }

        if let Some(sym) = best {
            let start = Position {
                line: sym.span.start.line.saturating_sub(1),
                character: sym.span.start.col.saturating_sub(1),
            };
            let end = Position {
                line: sym.span.end.line.saturating_sub(1),
                character: sym.span.end.col.saturating_sub(1),
            };
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: Url::parse(&uri).unwrap(),
                range: Range { start, end },
            })));
        }

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;

        let documents = self.documents.lock().await;
        let text = match documents.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(documents);

        let word = match word_at_position(&text, pos.line as usize, pos.character as usize) {
            Some(w) => w,
            None => return Ok(None),
        };

        // find all occurrences of the word in the document
        let mut locations: Vec<Location> = Vec::new();
        let parsed_uri = Url::parse(&uri).unwrap();

        for (line_num, line) in text.lines().enumerate() {
            let mut col = 0;
            let chars: Vec<char> = line.chars().collect();
            let mut in_string = false;

            while col < chars.len() {
                // skip line comments
                if !in_string && col + 1 < chars.len() && chars[col] == '/' && chars[col + 1] == '/'
                {
                    break; // rest of line is a comment
                }

                if chars[col] == '"' {
                    in_string = !in_string;
                    col += 1;
                    continue;
                }

                if in_string {
                    col += 1;
                    continue;
                }

                let remaining: String = chars[col..].iter().collect();
                if remaining.starts_with(&word) {
                    let before_ok =
                        col == 0 || !(chars[col - 1].is_alphanumeric() || chars[col - 1] == '_');
                    let after_pos = col + word.len();
                    let after_ok = after_pos >= chars.len()
                        || !(chars[after_pos].is_alphanumeric() || chars[after_pos] == '_');

                    if before_ok && after_ok {
                        let start = Position {
                            line: line_num as u32,
                            character: col as u32,
                        };
                        let end = Position {
                            line: line_num as u32,
                            character: (col + word.len()) as u32,
                        };
                        locations.push(Location {
                            uri: parsed_uri.clone(),
                            range: Range { start, end },
                        });
                    }
                    col += word.len();
                } else {
                    col += 1;
                }
            }
        }

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;
        let new_name = params.new_name;

        let documents = self.documents.lock().await;
        let text = match documents.get(&uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        drop(documents);

        let word = match word_at_position(&text, pos.line as usize, pos.character as usize) {
            Some(w) => w,
            None => return Ok(None),
        };

        let mut edits: Vec<TextEdit> = Vec::new();

        for (line_num, line) in text.lines().enumerate() {
            let mut col = 0;
            let chars: Vec<char> = line.chars().collect();
            let mut in_string = false;

            while col < chars.len() {
                // skip line comments
                if !in_string && col + 1 < chars.len() && chars[col] == '/' && chars[col + 1] == '/'
                {
                    break; // rest of line is a comment
                }

                // track string boundaries
                if chars[col] == '"' {
                    in_string = !in_string;
                    col += 1;
                    continue;
                }

                // skip matches inside strings
                if in_string {
                    col += 1;
                    continue;
                }

                let remaining: String = chars[col..].iter().collect();
                if remaining.starts_with(&word) {
                    let before_ok =
                        col == 0 || !(chars[col - 1].is_alphanumeric() || chars[col - 1] == '_');
                    let after_pos = col + word.len();
                    let after_ok = after_pos >= chars.len()
                        || !(chars[after_pos].is_alphanumeric() || chars[after_pos] == '_');

                    if before_ok && after_ok {
                        let start = Position {
                            line: line_num as u32,
                            character: col as u32,
                        };
                        let end = Position {
                            line: line_num as u32,
                            character: (col + word.len()) as u32,
                        };
                        edits.push(TextEdit {
                            range: Range { start, end },
                            new_text: new_name.clone(),
                        });
                    }
                    col += word.len();
                } else {
                    col += 1;
                }
            }
        }

        if edits.is_empty() {
            return Ok(None);
        }

        let mut changes = HashMap::new();
        changes.insert(Url::parse(&uri).unwrap(), edits);

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }))
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
                    span: f.span,
                    doc: f.doc_comment.clone(),
                });
                // add params
                for param in &f.params {
                    symbols.push(SymbolInfo {
                        name: param.name.name.clone(),
                        kind: SymbolKind::Param,
                        ty: type_expr_to_string(&param.ty),
                        span: param.span,
                        doc: None,
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
                    span: c.span,
                    doc: None,
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
                    span: l.span,
                    doc: None,
                });
            }
            typex_ast::Item::TypeAlias(t) => {
                symbols.push(SymbolInfo {
                    name: t.name.name.clone(),
                    kind: SymbolKind::Type,
                    ty: type_expr_to_string(&t.ty),
                    span: t.span,
                    doc: None,
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
                    span: l.span,
                    doc: None,
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
                    span: c.span,
                    doc: None,
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

fn get_existing_imports(text: &str) -> HashMap<String, Vec<String>> {
    // returns map of module -> [imported names]
    // e.g. "tx:math" -> ["sqrt", "abs"]
    let mut imports: HashMap<String, Vec<String>> = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("import {") {
            continue;
        }
        // parse: import { a, b, c } from "module";
        if let Some(from_pos) = line.find("} from \"") {
            let names_part = &line[8..from_pos]; // after "import { "
            let module_part = &line[from_pos + 8..]; // after "} from \""
            let module = module_part.trim_end_matches("\";").trim_end_matches('"');
            let names: Vec<String> = names_part
                .split(',')
                .map(|n| n.trim().to_string())
                .filter(|n| !n.is_empty())
                .collect();
            imports.insert(module.to_string(), names);
        }
    }
    imports
}

fn make_import_edit(
    text: &str,
    existing_imports: &HashMap<String, Vec<String>>,
    module: &str,
    name: &str,
) -> Option<Vec<TextEdit>> {
    // already imported - no edit needed
    if let Some(names) = existing_imports.get(module) {
        if names.contains(&name.to_string()) {
            return None;
        }
        // module imported but name missing - update existing import line
        let mut new_names = names.clone();
        new_names.push(name.to_string());
        new_names.sort();

        // find the line to replace
        for (i, line) in text.lines().enumerate() {
            if line.trim().contains(&format!("from \"{}\"", module)) {
                let new_line =
                    format!("import {{ {} }} from \"{}\";", new_names.join(", "), module);
                return Some(vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: i as u32,
                            character: 0,
                        },
                        end: Position {
                            line: i as u32,
                            character: line.len() as u32,
                        },
                    },
                    new_text: new_line,
                }]);
            }
        }
    }

    // find where to insert new import line
    let mut insert_line: u32 = 0;
    for (i, line) in text.lines().enumerate() {
        if line.trim().starts_with("import {") {
            insert_line = i as u32 + 1;
        }
    }

    let new_import = format!("import {{ {} }} from \"{}\";\n", name, module);

    Some(vec![TextEdit {
        range: Range {
            start: Position {
                line: insert_line,
                character: 0,
            },
            end: Position {
                line: insert_line,
                character: 0,
            },
        },
        new_text: new_import,
    }])
}

fn build_builtin_symbols() -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    let dummy_span = typex_span::Span::point(typex_span::FileId(0), typex_span::Pos::new(0, 0, 0));

    for name in &["print", "println", "panic"] {
        symbols.push(SymbolInfo {
            name: name.to_string(),
            kind: SymbolKind::Builtin,
            ty: "builtin function".to_string(),
            span: dummy_span,
            doc: None,
        });
    }

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
            symbols.push(SymbolInfo {
                name: f.to_string(),
                kind: SymbolKind::Builtin,
                ty: format!("stdlib fn from {}", module),
                span: dummy_span,
                doc: None,
            });
        }
    }
    symbols
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
