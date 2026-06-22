/// Uniquely identifies a source file within the SourceMap
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub u32);

/// A position in source code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,   // 1-based
    pub col: u32,    // 1-based
    pub offset: u32, // byte offset from start of file
}

impl Pos {
    pub fn new(line: u32, col: u32, offset: u32) -> Self {
        Self { line, col, offset }
    }
}

/// A range in source code, referencing a specific file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub file: FileId,
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    pub fn new(file: FileId, start: Pos, end: Pos) -> Self {
        Self { file, start, end }
    }

    /// Create a zero-length span at a single position
    pub fn point(file: FileId, pos: Pos) -> Self {
        Self {
            file,
            start: pos,
            end: pos,
        }
    }

    /// Merge two spans into one that covers both
    pub fn to(&self, other: Span) -> Span {
        Span {
            file: self.file,
            start: self.start,
            end: other.end,
        }
    }
}

/// A single source file
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub name: String, // e.g. "foo.tx"
    pub src: String,  // full source text
}

/// Stores all source files for the current compilation
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceFile {
    pub fn line(&self, line_number: u32) -> Option<&str> {
        // lines are 1-based
        self.src.lines().nth((line_number - 1) as usize)
    }
}

impl SourceMap {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Add a file and return its FileId
    pub fn add(&mut self, name: String, src: String) -> FileId {
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile { id, name, src });
        id
    }

    /// Look up a file by id
    pub fn get(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    /// Get source text for a span
    pub fn snippet(&self, span: Span) -> Option<&str> {
        let file = self.get(span.file)?;
        let start = span.start.offset as usize;
        let end = span.end.offset as usize;
        file.src.get(start..end)
    }

    pub fn render_diagnostic(&self, diag: &Diagnostic) -> String {
        let file = match self.get(diag.span.file) {
            Some(f) => f,
            None => return format!("[{:?}] {}", diag.level, diag.message),
        };

        let line_num = diag.span.start.line;
        let col = diag.span.start.col;
        let level = match diag.level {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Note => "note",
        };

        let mut out = String::new();

        // header
        out.push_str(&format!(
            "[{}] {}:{}:{} — {}\n",
            level, file.name, line_num, col, diag.message
        ));

        // source line
        if let Some(line) = file.line(line_num) {
            let line_num_str = line_num.to_string();
            let line_prefix = format!("{} | ", line_num_str);
            let blank_prefix = format!("{} | ", " ".repeat(line_num_str.len()));

            out.push_str(&format!("{}\n", " ".repeat(line_num_str.len() + 2)));
            out.push_str(&format!("{}{}\n", line_prefix, line));

            let span_len = if diag.span.end.col > diag.span.start.col {
                (diag.span.end.col - diag.span.start.col) as usize
            } else {
                1
            };
            let indent = " ".repeat((col as usize).saturating_sub(1));
            let line_rest = line.len().saturating_sub((col - 1) as usize);
            let pointer = "^".repeat(span_len.min(line_rest).max(1));
            out.push_str(&format!("{}{}{}", blank_prefix, indent, pointer));
            out.push_str("\n");
        }

        out
    }
}

/// Severity level of a diagnostic
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    Note,
}

/// A compiler diagnostic - an error, warning, or note attached to a span
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: Level,
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            span,
            message: message.into(),
        }
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self {
            level: Level::Warning,
            span,
            message: message.into(),
        }
    }

    pub fn note(span: Span, message: impl Into<String>) -> Self {
        Self {
            level: Level::Note,
            span,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_map_add_and_get() {
        let mut sm = SourceMap::new();
        let id = sm.add("foo.tx".to_string(), "let x = 42;".to_string());
        assert_eq!(id, FileId(0));
        let file = sm.get(id).unwrap();
        assert_eq!(file.name, "foo.tx");
    }

    #[test]
    fn test_snippet() {
        let mut sm = SourceMap::new();
        let id = sm.add("foo.tx".to_string(), "let x = 42;".to_string());
        let start = Pos::new(1, 1, 0);
        let end = Pos::new(1, 4, 3);
        let span = Span::new(id, start, end);
        assert_eq!(sm.snippet(span), Some("let"));
    }

    #[test]
    fn test_span_merge() {
        let id = FileId(0);
        let a = Span::new(id, Pos::new(1, 1, 0), Pos::new(1, 4, 3));
        let b = Span::new(id, Pos::new(1, 5, 4), Pos::new(1, 6, 5));
        let merged = a.to(b);
        assert_eq!(merged.start.offset, 0);
        assert_eq!(merged.end.offset, 5);
    }

    #[test]
    fn test_diagnostic() {
        let id = FileId(0);
        let span = Span::point(id, Pos::new(1, 1, 0));
        let d = Diagnostic::error(span, "unexpected token");
        assert_eq!(d.level, Level::Error);
        assert_eq!(d.message, "unexpected token");
    }
}
