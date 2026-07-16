use std::ops::Deref;

#[derive(Clone, Debug, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl PartialEq for Span {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
impl Eq for Span {}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub fn dummy() -> Self {
        Span { start: 0, end: 0 }
    }

    /// Join two spans: start of `self`, end of `other`.
    pub fn to(self, other: Span) -> Span {
        Span { start: self.start, end: other.end }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Spanned { node, span }
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.node
    }
}

#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
    pub label: Option<String>,
}

impl CompileError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        CompileError { message: message.into(), span, label: None }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

// ANSI SGR helper
fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    } else {
        text.to_string()
    }
}

/// Render a diagnostic as a rustc-style frame ending in a newline.
pub fn render(filename: &str, source: &str, err: &CompileError, use_color: bool) -> String {
    let chars: Vec<char> = source.chars().collect();
    let start = err.span.start.min(chars.len());

    let mut line_start = 0;
    let mut line_no = 1;
    for i in 0..start {
        if chars[i] == '\n' {
            line_no += 1;
            line_start = i + 1;
        }
    }

    let mut line_end = start;
    while line_end < chars.len() && chars[line_end] != '\n' {
        line_end += 1;
    }

    let col = start - line_start + 1;
    let line_text: String = chars[line_start..line_end].iter().collect();

    let span_end = err.span.end.min(line_end);
    let caret_len = span_end.saturating_sub(start).max(1);

    let num_str = line_no.to_string();
    let pad = " ".repeat(num_str.len());

    let rail = |s: &str| paint(s, "1;34", use_color);
    let red = |s: &str| paint(s, "1;31", use_color);

    let mut out = String::new();

    // error: <message>
    out.push_str(&red("error:"));
    out.push(' ');
    out.push_str(&err.message);
    out.push('\n');

    // ` <pad>--> file:line:col`
    out.push_str(&format!(" {pad}"));
    out.push_str(&rail("-->"));
    out.push_str(&format!(" {filename}:{line_no}:{col}\n"));

    // ` <pad> |`
    out.push_str(&format!(" {pad} "));
    out.push_str(&rail("|"));
    out.push('\n');

    // ` <num> | <source line>`
    out.push_str(&format!(" {num_str} "));
    out.push_str(&rail("|"));
    out.push_str(&format!(" {line_text}\n"));

    // ` <pad> | <indent>^^^ label`
    out.push_str(&format!(" {pad} "));
    out.push_str(&rail("|"));
    out.push(' ');
    out.push_str(&" ".repeat(col - 1));
    let mut caret = "^".repeat(caret_len);
    if let Some(label) = &err.label {
        caret.push(' ');
        caret.push_str(label);
    }
    out.push_str(&red(&caret));
    out.push('\n');

    out
}

// Tests 

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_always_compare_equal() {
        // AST equality must ignore span differences.
        assert_eq!(Span::new(0, 3), Span::new(10, 42));
    }

    #[test]
    fn spanned_equality_ignores_span() {
        let a = Spanned::new(7i32, Span::new(0, 1));
        let b = Spanned::new(7i32, Span::new(99, 100));
        assert_eq!(a, b);
        let c = Spanned::new(8i32, Span::dummy());
        assert_ne!(a, c);
    }

    #[test]
    fn span_to_joins_endpoints() {
        let joined = Span::new(3, 5).to(Span::new(9, 14));
        assert_eq!(joined.start, 3);
        assert_eq!(joined.end, 14);
    }

    #[test]
    fn deref_reaches_inner_node() {
        let s = Spanned::new(vec![1, 2, 3], Span::dummy());
        assert_eq!(s.len(), 3); // via Deref
    }

    #[test]
    fn error_builder_sets_fields() {
        let e = CompileError::new("boom", Span::new(2, 4)).with_label("here");
        assert_eq!(e.message, "boom");
        assert_eq!(e.span.start, 2);
        assert_eq!(e.label.as_deref(), Some("here"));
    }
     #[test]
    fn render_plain_frame_points_at_column() {
        let source = "int main() { return 42 }";
        let brace = source.find('}').unwrap(); // offset 23
        let err = CompileError::new("expected `;`, found `}`", Span::new(brace, brace + 1))
            .with_label("expected `;` here");
        let out = render("prog.c", source, &err, false);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "error: expected `;`, found `}`");
        assert_eq!(lines[1], "  --> prog.c:1:24");
        assert_eq!(lines[2], "   |");
        assert_eq!(lines[3], " 1 | int main() { return 42 }");
        // The caret must sit directly under the `}` on the source line above it.
        let caret_line = lines[4];
        assert_eq!(
            caret_line.find('^'),
            lines[3].find('}'),
            "caret misaligned:\n{}\n{}",
            lines[3],
            caret_line
        );
        assert!(caret_line.ends_with("^ expected `;` here"));
    }

    #[test]
    fn render_reports_correct_line_and_column_on_later_line() {
        let source = "int main() {\n    return x + y;\n}";
        // `y` is at offset 25 (line 2). Compute line/col dynamically for the assert.
        let y_off = source.find('y').unwrap();
        let err = CompileError::new("undefined variable `y`", Span::new(y_off, y_off + 1))
            .with_label("not found in this scope");
        let out = render("prog.c", source, &err, false);
        assert!(out.contains("--> prog.c:2:16"), "got:\n{out}");
        assert!(out.contains("    return x + y;"), "got:\n{out}");
        assert!(out.contains("^ not found in this scope"), "got:\n{out}");
    }

    #[test]
    fn render_color_wraps_in_ansi_but_plain_does_not() {
        let source = "x";
        let err = CompileError::new("bad", Span::new(0, 1));
        assert!(!render("f", source, &err, false).contains('\u{1b}'));
        assert!(render("f", source, &err, true).contains('\u{1b}'));
    }
}