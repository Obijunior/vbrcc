//! The macro table.

use std::collections::HashMap;

use crate::diagnostic::Span;
use crate::lexer::{SpannedToken, Token};

/// Macros whose value depends on where they are used, so they cannot be stored
/// as a fixed token body.
#[derive(Clone, Copy, Debug)]
pub enum Builtin {
    File,
    Line,
}

#[derive(Clone, Debug)]
pub enum MacroKind {
    Object { body: Vec<SpannedToken> },
    Builtin(Builtin),
}

#[derive(Clone, Debug)]
pub struct MacroDef {
    pub kind: MacroKind,
    pub name_span: Span,
}

#[derive(Default)]
pub struct MacroTable {
    map: HashMap<String, MacroDef>,
}

impl MacroTable {
    pub fn new() -> MacroTable {
        MacroTable { map: HashMap::new() }
    }

    /// Insert a definition, returning the previous one if the name was taken.
    pub fn define(&mut self, name: &str, def: MacroDef) -> Option<MacroDef> {
        self.map.insert(name.to_string(), def)
    }

    /// Remove a definition. Returns whether it existed.
    pub fn undef(&mut self, name: &str) -> bool {
        self.map.remove(name).is_some()
    }

    pub fn get(&self, name: &str) -> Option<&MacroDef> {
        self.map.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }

    /// A table seeded with the predefined macros.
    ///
    /// `__DATE__` and `__TIME__` are deliberately absent: producing them without
    /// a dependency means hand-rolling civil-calendar arithmetic, and nothing
    /// needs them yet. Add them when something does.
    ///
    /// `__STDC_VERSION__` is `199901` rather than the standard's `199901L`
    /// because integer suffixes are roadmap item 22 and the lexer cannot
    /// produce one yet.
    pub fn with_predefined() -> MacroTable {
        let mut t = MacroTable::new();

        for (name, value) in [
            ("__STDC__", 1i64),
            ("__STDC_VERSION__", 199901),
            // Bundled headers branch on the target; omitting these would make
            // them lie about the platform we actually emit for.
            ("_WIN32", 1),
            ("_WIN64", 1),
        ] {
            t.define(
                name,
                MacroDef {
                    kind: MacroKind::Object {
                        body: vec![SpannedToken {
                            token: Token::IntLiteral(value),
                            span: Span::dummy(),
                        }],
                    },
                    name_span: Span::dummy(),
                },
            );
        }

        for (name, builtin) in [("__FILE__", Builtin::File), ("__LINE__", Builtin::Line)] {
            t.define(
                name,
                MacroDef { kind: MacroKind::Builtin(builtin), name_span: Span::dummy() },
            );
        }

        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(tokens: Vec<Token>) -> MacroDef {
        MacroDef {
            kind: MacroKind::Object {
                body: tokens
                    .into_iter()
                    .map(|t| SpannedToken { token: t, span: Span::dummy() })
                    .collect(),
            },
            name_span: Span::dummy(),
        }
    }

    #[test]
    fn define_then_get_round_trips() {
        let mut t = MacroTable::new();
        assert!(t.define("N", obj(vec![Token::IntLiteral(10)])).is_none());
        assert!(t.contains("N"));
        match &t.get("N").unwrap().kind {
            MacroKind::Object { body } => {
                assert_eq!(body.len(), 1);
                assert_eq!(body[0].token, Token::IntLiteral(10));
            }
            other => panic!("expected an object macro, got {other:?}"),
        }
    }

    #[test]
    fn with_predefined_seeds_the_standard_set() {
        let t = MacroTable::with_predefined();
        for name in ["__STDC__", "__STDC_VERSION__", "_WIN32", "_WIN64", "__FILE__", "__LINE__"] {
            assert!(t.contains(name), "{name} should be predefined");
        }
        assert!(matches!(t.get("__FILE__").unwrap().kind, MacroKind::Builtin(Builtin::File)));
        assert!(matches!(t.get("__LINE__").unwrap().kind, MacroKind::Builtin(Builtin::Line)));
    }

    #[test]
    fn redefining_returns_the_previous_definition() {
        let mut t = MacroTable::new();
        t.define("N", obj(vec![Token::IntLiteral(1)]));
        let prev = t.define("N", obj(vec![Token::IntLiteral(2)]));
        assert!(prev.is_some(), "redefinition must surface the old body so the \
                                 caller can decide whether to warn");
    }

    #[test]
    fn undef_removes_and_reports_whether_it_existed() {
        let mut t = MacroTable::new();
        t.define("N", obj(vec![Token::IntLiteral(1)]));
        assert!(t.undef("N"));
        assert!(!t.contains("N"));
        assert!(!t.undef("N"), "undef of an unknown macro is legal but reports false");
    }

    #[test]
    fn unknown_macro_is_absent() {
        let t = MacroTable::new();
        assert!(!t.contains("NOPE"));
        assert!(t.get("NOPE").is_none());
    }
}