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
}