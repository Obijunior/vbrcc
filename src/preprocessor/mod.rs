//! Stage 0: the preprocessor.
//!
//! Owns the read loop. Walks logical lines, dispatches `#` lines as directives,
//! and hands everything else to a per-file [`Lexer`]. Produces the token stream
//! the parser consumes.

pub mod normalize;
pub mod macros;

use std::collections::VecDeque;

use crate::diagnostic::{CompileError, FileId, Span, SourceMap};
use crate::lexer::{Lexer, SpannedToken, Token};
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
}

pub struct Preprocessor<'a> {
    #[allow(dead_code)]
    map: &'a mut SourceMap,
    stack: Vec<FileState>,
    pending: VecDeque<SpannedToken>,
    out: Vec<SpannedToken>,
    entry: FileId,
}

impl<'a> Preprocessor<'a> {
    pub fn new(map: &'a mut SourceMap) -> Preprocessor<'a> {
        Preprocessor {
            map,
            stack: Vec::new(),
            pending: VecDeque::new(),
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
            self.out.push(tok);
        }

        self.out.push(SpannedToken {
            token: Token::EOF,
            span: Span::in_file(self.entry, 0, 0),
        });
        Ok(self.out)
    }

    /// The next token, before macro expansion. Runs the line loop as needed.
    fn next_raw(&mut self) -> Result<Option<SpannedToken>, CompileError> {
        loop {
            if let Some(tok) = self.pending.pop_front() {
                return Ok(Some(tok));
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
                    self.pending.extend(toks);
                }
            }
        }
    }

    /// Handle a directive body running from `from` to `end` (the `#` is consumed).
    fn directive(&mut self, _from: usize, _end: usize) -> Result<(), CompileError> {
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
}