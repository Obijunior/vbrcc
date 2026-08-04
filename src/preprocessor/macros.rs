//! The macro table.

use std::collections::HashMap;

use crate::diagnostic::Span;
use crate::lexer::SpannedToken;

#[derive(Clone, Debug)]
pub enum MacroKind {
    Object { body: Vec<SpannedToken> },
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Token;

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
        }
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