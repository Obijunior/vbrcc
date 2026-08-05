//! Stage 0: the preprocessor.
//!
//! Owns the read loop. Walks logical lines, dispatches `#` lines as directives,
//! and hands everything else to a per-file [`Lexer`]. Produces the token stream
//! the parser consumes.

pub mod normalize;
pub mod macros;

use std::collections::{HashSet, VecDeque};

use crate::diagnostic::{CompileError, FileId, Span, SourceMap};
use crate::lexer::{Lexer, SpannedToken, Token};
use macros::{Builtin, MacroTable, MacroDef, MacroKind};
use normalize::normalize;

/// One open file: its normalized text, how far we have read, and a lexer
/// positioned over it.
struct FileState {
    file: FileId,
    chars: Vec<char>,
    cursor: usize,
    lexer: Lexer,
}

impl FileState {
    fn new(file: FileId, text: &str) -> FileState {
        let chars = normalize(text);
        let lexer = Lexer::from_chars(chars.clone(), file);
        FileState { file, chars, cursor: 0, lexer }
    }

    /// Consume one logical line, returning `[start, end)` without the newline.
    fn next_line(&mut self) -> (usize, usize) {
        let start = self.cursor;
        let mut end = start;
        while end < self.chars.len() && self.chars[end] != '\n' {
            end += 1;
        }
        self.cursor = if end < self.chars.len() { end + 1 } else { end };
        (start, end)
    }

    /// Offset of the first non-blank char in `[start, end)`, if any.
    fn first_non_blank(&self, start: usize, end: usize) -> Option<usize> {
        (start..end).find(|&i| !self.chars[i].is_whitespace())
    }

    /// 1-based line number containing `offset`, for `__LINE__`.
    ///
    /// Normalization preserves newlines, so counting them in the normalized
    /// buffer gives the same answer as counting them in the original file.
    fn line_of(&self, offset: usize) -> i64 {
        1 + self.chars[..offset.min(self.chars.len())]
            .iter()
            .filter(|&&c| c == '\n')
            .count() as i64
    }
}

/// An item queued for the parser. The sentinel marks where a macro's expansion
/// ends, so the recursion guard un-paints the name at exactly the right moment.
enum PendingItem {
    Tok(SpannedToken),
    EndExpansion(String),
}

pub struct Preprocessor<'a> {
    map: &'a mut SourceMap,
    macros: MacroTable,
    stack: Vec<FileState>,
    pending: VecDeque<PendingItem>,
    active: HashSet<String>,
    out: Vec<SpannedToken>,
    entry: FileId,
}

impl<'a> Preprocessor<'a> {
    pub fn new(map: &'a mut SourceMap) -> Preprocessor<'a> {
        Preprocessor {
            map,
            macros: MacroTable::with_predefined(),
            stack: Vec::new(),
            pending: VecDeque::new(),
            active: HashSet::new(),
            out: Vec::new(),
            entry: 0,
        }
    }

    /// Preprocess `entry` and everything it pulls in.
    pub fn run(mut self, entry: FileId) -> Result<Vec<SpannedToken>, CompileError> {
        self.entry = entry;
        let text = self.map.file(entry).text.clone();
        self.stack.push(FileState::new(entry, &text));

        while let Some(tok) = self.next_raw()? {
            if let Token::Ident(name) = &tok.token {
                let name = name.clone();
                if self.try_expand(&name, tok.span) {
                    continue;
                }
            }
            self.out.push(tok);
        }

        self.out.push(SpannedToken {
            token: Token::EOF,
            span: Span::in_file(self.entry, 0, 0),
        });
        Ok(self.out)
    }

    /// Expand `name` if it is a macro that is not already being expanded.
    /// Returns whether anything was pushed.
    fn try_expand(&mut self, name: &str, use_site: Span) -> bool {
        if self.active.contains(name) {
            return false;
        }
        let body = match self.macros.get(name) {
            None => return false,
            Some(def) => match &def.kind {
                MacroKind::Object { body } => body.clone(),
                MacroKind::Builtin(b) => {
                    // Computed at the point of use, not stored.
                    let st = self.stack.last().expect("stack non-empty");
                    let token = match b {
                        Builtin::File => {
                            Token::StringLiteral(self.map.file(st.file).name.clone())
                        }
                        Builtin::Line => Token::IntLiteral(st.line_of(use_site.start)),
                    };
                    vec![SpannedToken { token, span: use_site }]
                }
            },
        };

        self.active.insert(name.to_string());

        // Push the body then the sentinel onto the FRONT, preserving order, so
        // the result is rescanned before any input that follows it.
        self.pending.push_front(PendingItem::EndExpansion(name.to_string()));
        for tok in body.into_iter().rev() {
            // Expanded tokens report at the call site, not at the #define.
            self.pending.push_front(PendingItem::Tok(SpannedToken {
                token: tok.token,
                span: use_site,
            }));
        }
        true
    }

    /// The next token, before macro expansion. Runs the line loop as needed.
    fn next_raw(&mut self) -> Result<Option<SpannedToken>, CompileError> {
        loop {
            while let Some(item) = self.pending.pop_front() {
                match item {
                    PendingItem::EndExpansion(name) => {
                        self.active.remove(&name);
                    }
                    PendingItem::Tok(tok) => return Ok(Some(tok)),
                }
            }

            let (start, end, at_eof) = match self.stack.last_mut() {
                None => return Ok(None),
                Some(st) => {
                    if st.cursor >= st.chars.len() {
                        (0, 0, true)
                    } else {
                        let (s, e) = st.next_line();
                        (s, e, false)
                    }
                }
            };

            if at_eof {
                self.stack.pop();
                continue;
            }

            let st = self.stack.last().expect("stack non-empty");
            match st.first_non_blank(start, end) {
                None => continue, // blank line
                Some(i) if st.chars[i] == '#' => {
                    self.directive(i + 1, end)?;
                }
                Some(_) => {
                    let st = self.stack.last_mut().expect("stack non-empty");
                    st.lexer.retarget(start, end);
                    let toks = st.lexer.tokenize_region()?;
                    self.pending.extend(toks.into_iter().map(PendingItem::Tok));
                }
            }
        }
    }

    fn directive(&mut self, from: usize, end: usize) -> Result<(), CompileError> {
        let st = self.stack.last().expect("stack non-empty");
        let file = st.file;

        let mut i = from;
        while i < end && st.chars[i].is_whitespace() {
            i += 1;
        }
        let name_start = i;
        while i < end && (st.chars[i].is_alphanumeric() || st.chars[i] == '_') {
            i += 1;
        }
        let name: String = st.chars[name_start..i].iter().collect();

        match name.as_str() {
            // A `#` on a line by itself is the null directive: legal, ignored.
            "" => Ok(()),
            "define" => self.define(i, end),
            "undef" => self.undef(i, end),
            // TEMPORARY (Phase 4 implements this): skipping preserves the
            // behaviour the lexer had before the preprocessor existed, so the
            // examples that `#include <stdio.h>` keep compiling.
            "include" => Ok(()),
            "if" | "ifdef" | "ifndef" | "elif" | "else" | "endif" | "error"
            | "warning" | "pragma" | "line" => Err(CompileError::new(
                format!("`#{name}` is not yet supported"),
                Span::in_file(file, name_start, i),
            )),
            other => Err(CompileError::new(
                format!("invalid preprocessing directive `#{other}`"),
                Span::in_file(file, name_start, i),
            )),
        }
    }

    /// `#define NAME body…`  (object-like only in this phase)
    fn define(&mut self, from: usize, end: usize) -> Result<(), CompileError> {
        let (file, name, name_start, name_end) = {
            let st = self.stack.last().expect("stack non-empty");
            let mut i = from;
            while i < end && st.chars[i].is_whitespace() {
                i += 1;
            }
            let start = i;
            while i < end && (st.chars[i].is_alphanumeric() || st.chars[i] == '_') {
                i += 1;
            }
            (st.file, st.chars[start..i].iter().collect::<String>(), start, i)
        };

        if name.is_empty() {
            return Err(CompileError::new(
                "macro name missing",
                Span::in_file(file, from.saturating_sub(1), end.max(from)),
            ));
        }

        // A `(` touching the name means function-like — Phase 2.
        let st = self.stack.last().expect("stack non-empty");
        if name_end < end && st.chars[name_end] == '(' {
            return Err(CompileError::new(
                "function-like macros are not yet supported",
                Span::in_file(file, name_start, name_end),
            ));
        }

        let body = {
            let st = self.stack.last_mut().expect("stack non-empty");
            st.lexer.retarget(name_end, end);
            st.lexer.tokenize_region()?
        };

        self.macros.define(
            &name,
            MacroDef {
                kind: MacroKind::Object { body },
                name_span: Span::in_file(file, name_start, name_end),
            },
        );
        Ok(())
    }

    /// `#undef NAME`
    fn undef(&mut self, from: usize, end: usize) -> Result<(), CompileError> {
        let (file, name, start, stop) = {
            let st = self.stack.last().expect("stack non-empty");
            let mut i = from;
            while i < end && st.chars[i].is_whitespace() {
                i += 1;
            }
            let s = i;
            while i < end && (st.chars[i].is_alphanumeric() || st.chars[i] == '_') {
                i += 1;
            }
            (st.file, st.chars[s..i].iter().collect::<String>(), s, i)
        };

        if name.is_empty() {
            return Err(CompileError::new(
                "macro name missing",
                Span::in_file(file, start, stop.max(start + 1)),
            ));
        }
        self.macros.undef(&name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::SourceMap;
    use crate::lexer::{Lexer, Token};

    /// Preprocess a string and return the resulting tokens.
    pub(crate) fn pp(src: &str) -> Result<Vec<crate::lexer::SpannedToken>, CompileError> {
        let mut map = SourceMap::single("test.c", src);
        Preprocessor::new(&mut map).run(0)
    }

    fn kinds(src: &str) -> Vec<Token> {
        pp(src).unwrap().into_iter().map(|t| t.token).collect()
    }

    #[test]
    fn plain_source_matches_the_bare_lexer() {
        let src = "int main() { return 42; }";
        let direct: Vec<Token> = Lexer::new(src).tokenize().unwrap()
            .into_iter().map(|t| t.token).collect();
        assert_eq!(kinds(src), direct);
    }

    #[test]
    fn multi_line_block_comment_survives_the_line_loop() {
        // The regression this phase exists to avoid: a comment spanning lines
        // must not reach the lexer as an unterminated fragment.
        let src = "int main() {\n    /*\n      hello\n    */\n    return 1;\n}";
        assert_eq!(kinds(src), kinds("int main() {\n    return 1;\n}"));
    }

    #[test]
    fn spans_still_point_at_the_original_text() {
        let src = "int  x;";
        let toks = pp(src).unwrap();
        assert_eq!(toks[1].token, Token::Ident("x".to_string()));
        assert_eq!(toks[1].span.start, src.find('x').unwrap());
        assert_eq!(toks[1].span.file, 0);
    }

    #[test]
    fn output_ends_with_exactly_one_eof() {
        let toks = pp("int x;\nint y;\n").unwrap();
        assert_eq!(toks.last().unwrap().token, Token::EOF);
        assert_eq!(toks.iter().filter(|t| t.token == Token::EOF).count(), 1);
    }

    #[test]
    fn lexer_errors_are_reported_with_a_real_span() {
        let src = "int main() { int x = @; }";
        let err = pp(src).unwrap_err();
        assert!(err.message.contains('@'), "got: {}", err.message);
        assert_eq!(err.span.start, src.find('@').unwrap());
    }

    #[test]
    fn object_macro_is_substituted() {
        assert_eq!(kinds("#define N 10\nint a[N];"), kinds("int a[10];"));
    }

    #[test]
    fn macro_with_a_multi_token_body_expands_in_order() {
        assert_eq!(kinds("#define P 1 + 2\nint x = P;"), kinds("int x = 1 + 2;"));
    }

    #[test]
    fn macro_with_an_empty_body_vanishes() {
        assert_eq!(kinds("#define NOTHING\nint NOTHING x;"), kinds("int x;"));
    }

    #[test]
    fn undefined_after_undef() {
        assert_eq!(kinds("#define N 10\n#undef N\nint N;"), kinds("int N;"));
    }

    #[test]
    fn expansion_is_rescanned() {
        // B expands to A, which must then expand to 7.
        assert_eq!(kinds("#define A 7\n#define B A\nint x = B;"), kinds("int x = 7;"));
    }

    #[test]
    fn self_referential_macro_terminates() {
        // C says `#define foo foo` expands once and is then left alone.
        assert_eq!(kinds("#define foo foo\nint foo;"), kinds("int foo;"));
    }

    #[test]
    fn mutually_recursive_macros_terminate() {
        let out = kinds("#define X Y\n#define Y X\nint X;");
        assert_eq!(out, kinds("int X;"));
    }

    #[test]
    fn a_macro_name_inside_a_string_is_not_expanded() {
        let toks = pp("#define N 10\nchar *s = \"N\";").unwrap();
        assert!(toks.iter().any(|t| t.token == Token::StringLiteral("N".to_string())),
                "string literal must survive verbatim: {toks:?}");
    }

    #[test]
    fn a_longer_identifier_containing_a_macro_name_is_not_expanded() {
        assert_eq!(kinds("#define MAX 1\nint MAXIMUM;"), kinds("int MAXIMUM;"));
    }

    #[test]
    fn expanded_tokens_carry_the_use_site_span() {
        let src = "#define N 10\nint a = N;";
        let toks = pp(src).unwrap();
        let ten = toks.iter().find(|t| t.token == Token::IntLiteral(10)).unwrap();
        assert_eq!(ten.span.start, src.rfind('N').unwrap(),
                   "an error in an expansion must point at the call, not the #define");
    }

    #[test]
    fn multi_line_macro_body_via_continuation() {
        assert_eq!(kinds("#define P 1 + \\\n    2\nint x = P;"), kinds("int x = 1 + 2;"));
    }

    #[test]
    fn function_like_define_is_rejected_for_now() {
        let err = pp("#define F(x) x\nint y;").unwrap_err();
        assert!(err.message.contains("function-like"), "got: {}", err.message);
    }

    #[test]
    fn define_without_a_name_is_an_error() {
        let err = pp("#define\n").unwrap_err();
        assert!(err.message.contains("macro name"), "got: {}", err.message);
    }

    #[test]
    fn include_is_still_skipped_silently() {
        // Phase 4 implements this; until then it must behave as it does today.
        assert_eq!(kinds("#include <stdio.h>\nint x;"), kinds("int x;"));
    }

    // --- Task 5: predefined macros ---

    #[test]
    fn stdc_macros_are_predefined() {
        assert_eq!(kinds("int x = __STDC__;"), kinds("int x = 1;"));
        assert_eq!(kinds("int x = __STDC_VERSION__;"), kinds("int x = 199901;"));
    }

    #[test]
    fn windows_target_macros_are_predefined() {
        assert_eq!(kinds("int x = _WIN32;"), kinds("int x = 1;"));
        assert_eq!(kinds("int x = _WIN64;"), kinds("int x = 1;"));
    }

    #[test]
    fn file_macro_expands_to_the_current_file_name() {
        let toks = pp("char *f = __FILE__;").unwrap();
        assert!(toks.iter().any(|t| t.token == Token::StringLiteral("test.c".to_string())),
                "got {toks:?}");
    }

    #[test]
    fn line_macro_expands_to_the_line_it_appears_on() {
        let toks = pp("int a;\nint b;\nint c = __LINE__;").unwrap();
        assert!(toks.iter().any(|t| t.token == Token::IntLiteral(3)), "got {toks:?}");
    }

    #[test]
    fn predefined_macros_can_be_undefined() {
        // Asserted directly rather than against a reference string: any string
        // mentioning __STDC__ would expand it, so a `kinds(..) == kinds(..)`
        // comparison can never hold once the macro actually works.
        let toks = kinds("#undef __STDC__\nint x = __STDC__;");
        assert!(
            toks.contains(&Token::Ident("__STDC__".to_string())),
            "after #undef the name must survive verbatim: {toks:?}"
        );
        assert!(
            !toks.contains(&Token::IntLiteral(1)),
            "the macro should no longer expand: {toks:?}"
        );
    }
}