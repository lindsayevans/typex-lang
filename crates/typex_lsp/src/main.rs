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

struct Backend {
    client: Client,
    documents: Arc<Mutex<HashMap<String, String>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn analyze(&self, uri: &str, text: &str) {
        let mut sm = SourceMap::new();
        let file = sm.add(uri.to_string(), text.to_string());

        let mut diagnostics: Vec<Diagnostic> = Vec::new();

        // Parse
        let (module, parse_diags) = parse(text, file);
        for diag in &parse_diags {
            if let Some(d) = to_lsp_diagnostic(diag) {
                diagnostics.push(d);
            }
        }

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

            // Typecheck - skip if resolver already caught it
            let type_diags = typex_typecheck::typecheck(&module);
            for diag in &type_diags {
                // skip if resolver already reported same location
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
