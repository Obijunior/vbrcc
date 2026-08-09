//! Stage 0: the preprocessor.
//!
//! Owns the read loop. Walks logical lines, dispatches `#` lines as directives,
//! and hands everything else to a per-file [`Lexer`]. Produces the token stream
//! the parser consumes.

pub mod normalize;
pub mod macros;
pub mod include;

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use crate::diagnostic::{CompileError, FileId, Span, SourceMap};
use crate::lexer::{Lexer, SpannedToken, Token};
use include::{IncludeResolver, Resolved};
use macros::{Builtin, MacroTable, MacroDef, MacroKind};
use normalize::normalize;

/// How many files may be open at once. Guards against a cyclic `#include`
/// that no include guard breaks.
const MAX_INCLUDE_DEPTH: usize = 64;

/// One open `#if`, `#ifdef`, or `#ifndef`.
struct Cond {
    active: bool,
    /// True if a branch of this conditional was taken. `#else` reads this.
    taken: bool,
    seen_else: bool,
    /// The opening directive. The "unterminated" error points here.
    span: Span,
}

/// One open file: its normalized text, how far we have read, and a lexer
/// positioned over it.
struct FileState {
    file: FileId,
    chars: Vec<char>,
    cursor: usize,
    lexer: Lexer,
    /// Open conditionals, innermost last. A conditional must close in the file
    /// that opens it, so this belongs to the file.
    conds: Vec<Cond>,
    /// The directory holding this file. `#include "name"` searches here first.
    /// `None` for a bundled header and for text that came from memory.
    dir: Option<PathBuf>,
    /// The canonical path, for `#pragma once`.
    path: Option<PathBuf>,
}

impl FileState {
    fn new(file: FileId, text: &str) -> FileState {
        let chars = normalize(text);
        let lexer = Lexer::from_chars(chars.clone(), file);
        FileState {
            file,
            chars,
            cursor: 0,
            lexer,
            conds: Vec::new(),
            dir: None,
            path: None,
        }
    }

    fn emitting(&self) -> bool {
        self.conds.iter().all(|c| c.active)
    }

    /// True if the region that contains the innermost conditional is live.
    ///
    /// This separates "the branch was not taken" from "the whole region is
    /// dead". An unsupported directive gets a diagnostic in the first case
    /// only.
    fn outer_live(&self) -> bool {
        self.conds.iter().rev().skip(1).all(|c| c.active)
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
    /// While true, `next_raw` drains `pending` only.
    sealed: bool,
    out: Vec<SpannedToken>,
    entry: FileId,
    includes: IncludeResolver,
    /// Files marked by `#pragma once`, keyed on the canonical path.
    once: HashSet<PathBuf>,
    /// One entry per open `#include`, for the diagnostic chain.
    chain: Vec<Span>,
}

impl<'a> Preprocessor<'a> {
    pub fn new(map: &'a mut SourceMap) -> Preprocessor<'a> {
        Preprocessor::with_search_path(map, Vec::new())
    }

    /// `search` holds the `-I` directories, in command-line order.
    pub fn with_search_path(map: &'a mut SourceMap, search: Vec<PathBuf>) -> Preprocessor<'a> {
        Preprocessor {
            map,
            macros: MacroTable::with_predefined(),
            stack: Vec::new(),
            pending: VecDeque::new(),
            active: HashSet::new(),
            sealed: false,
            out: Vec::new(),
            entry: 0,
            includes: IncludeResolver::new(search),
            once: HashSet::new(),
            chain: Vec::new(),
        }
    }

    /// Preprocess `entry` and everything it pulls in.
    pub fn run(mut self, entry: FileId) -> Result<Vec<SpannedToken>, CompileError> {
        self.entry = entry;
        let text = self.map.file(entry).text.clone();
        let mut state = FileState::new(entry, &text);
        let path = PathBuf::from(&self.map.file(entry).name);
        state.dir = path.parent().map(|d| d.to_path_buf());
        state.path = path.canonicalize().ok();
        self.stack.push(state);

        loop {
            let tok = match self.next_raw() {
                Err(e) => return Err(self.with_chain(e)),
                Ok(None) => break,
                Ok(Some(t)) => t,
            };
            if let Token::Ident(name) = &tok.token {
                let name = name.clone();
                match self.try_expand(&name, tok.span) {
                    Err(e) => return Err(self.with_chain(e)),
                    Ok(true) => continue,
                    Ok(false) => {}
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

    /// Attach the open `#include` chain, innermost first, so a diagnostic
    /// inside a header names the line that pulled it in.
    fn with_chain(&self, mut err: CompileError) -> CompileError {
        for span in self.chain.iter().rev() {
            err = err.with_note("in file included from", Some(*span));
        }
        err
    }

    /// Expand `name` if it is a macro that is not already being expanded.
    /// Returns whether anything was pushed.
    fn try_expand(&mut self, name: &str, use_site: Span) -> Result<bool, CompileError> {
        if self.active.contains(name) {
            return Ok(false);
        }
        let kind = match self.macros.get(name) {
            None => return Ok(false),
            Some(def) => def.kind.clone(),
        };

        let body = match kind {
            MacroKind::Object { body } => body,
            MacroKind::Builtin(b) => {
                let st = self.stack.last().expect("stack non-empty");
                let token = match b {
                    Builtin::File => Token::StringLiteral(self.map.file(st.file).name.clone()),
                    Builtin::Line => Token::IntLiteral(st.line_of(use_site.start)),
                };
                vec![SpannedToken { token, span: use_site }]
            }
            MacroKind::Function { params, body } => {
                match self.next_raw()? {
                    None => return Ok(false),
                    Some(tok) => {
                        if tok.token != Token::LParen {
                            self.pending.push_front(PendingItem::Tok(tok));
                            return Ok(false);
                        }
                    }
                }

                let raw_args = self.collect_args(name, use_site)?;
                // C11 6.10.3.1 expands each argument before substitution, in a
                // context where the enclosing macro is not yet painted. `name`
                // enters `active` below, so this is that context.
                let mut args = Vec::with_capacity(raw_args.len());
                for arg in raw_args {
                    args.push(self.expand_list(arg)?);
                }
                if args.is_empty() && params.len() == 1 {
                    args.push(Vec::new());
                }
                if args.len() != params.len() {
                    return Err(CompileError::new(
                        format!(
                            "macro `{name}` requires {} argument{}, but {} given",
                            params.len(),
                            if params.len() == 1 { "" } else { "s" },
                            args.len()
                        ),
                        use_site,
                    ));
                }
                macros::substitute(&params, &args, &body, use_site)
            }
        };

        self.active.insert(name.to_string());
        self.pending.push_front(PendingItem::EndExpansion(name.to_string()));
        for tok in body.into_iter().rev() {
            self.pending.push_front(PendingItem::Tok(SpannedToken {
                token: tok.token,
                span: use_site,
            }));
        }
        Ok(true)
    }

    /// Expand a closed token list.
    ///
    /// `pending` changes to a private queue and `sealed` becomes true, so the
    /// expansion cannot read past the end of the list.
    fn expand_list(
        &mut self,
        tokens: Vec<SpannedToken>,
    ) -> Result<Vec<SpannedToken>, CompileError> {
        let saved_pending = std::mem::replace(
            &mut self.pending,
            tokens.into_iter().map(PendingItem::Tok).collect(),
        );
        let was_sealed = self.sealed;
        self.sealed = true;

        let mut out = Vec::new();
        let result = loop {
            match self.next_raw() {
                Err(e) => break Err(e),
                Ok(None) => break Ok(()),
                Ok(Some(tok)) => {
                    if let Token::Ident(name) = &tok.token {
                        let name = name.clone();
                        match self.try_expand(&name, tok.span) {
                            Err(e) => break Err(e),
                            Ok(true) => continue,
                            Ok(false) => {}
                        }
                    }
                    out.push(tok);
                }
            }
        };

        self.sealed = was_sealed;
        self.pending = saved_pending;
        result.map(|()| out)
    }

    /// Collect a function-like macro's arguments.
    fn collect_args(
        &mut self,
        name: &str,
        use_site: Span,
    ) -> Result<Vec<Vec<SpannedToken>>, CompileError> {
        let mut args: Vec<Vec<SpannedToken>> = Vec::new();
        let mut current: Vec<SpannedToken> = Vec::new();
        let mut depth = 1usize;

        loop {
            let tok = match self.next_raw()? {
                Some(t) => t,
                None => {
                    return Err(CompileError::new(
                        format!("unterminated argument list invoking macro `{name}`"),
                        use_site,
                    ));
                }
            };
            match tok.token {
                Token::LParen => {
                    depth += 1;
                    current.push(tok);
                }
                Token::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        if !(current.is_empty() && args.is_empty()) {
                            args.push(current);
                        }
                        return Ok(args);
                    }
                    current.push(tok);
                }
                Token::Comma if depth == 1 => {
                    args.push(std::mem::take(&mut current));
                }
                _ => current.push(tok),
            }
        }
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

            if self.sealed {
                return Ok(None);
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
                if let Some(st) = self.stack.last() {
                    if let Some(c) = st.conds.last() {
                        return Err(CompileError::new(
                            "unterminated conditional directive",
                            c.span,
                        )
                        .with_label("this conditional is never closed"));
                    }
                }
                self.stack.pop();
                self.chain.pop();
                continue;
            }

            let st = self.stack.last().expect("stack non-empty");
            match st.first_non_blank(start, end) {
                None => continue, // blank line
                Some(i) if st.chars[i] == '#' => {
                    self.directive(i + 1, end)?;
                }
                Some(_) => {
                    // A dead branch can hold text that does not lex. Skip it
                    // before the lexer sees it.
                    if !self.emitting() {
                        continue;
                    }
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

        // Conditional directives always run, even in a dead branch, so that
        // nesting and `#endif` pairing stay correct.
        match name.as_str() {
            "ifdef" => return self.if_defined(i, end, true, name_start),
            "ifndef" => return self.if_defined(i, end, false, name_start),
            "else" => return self.directive_else(file, name_start, i),
            "endif" => return self.directive_endif(file, name_start, i),
            "if" => {
                let live = self.stack.last().expect("stack non-empty").emitting();
                if live {
                    return Err(CompileError::new(
                        "`#if` is not yet supported",
                        Span::in_file(file, name_start, i),
                    ));
                }
                let st = self.stack.last_mut().expect("stack non-empty");
                st.conds.push(Cond {
                    active: false,
                    taken: true,
                    seen_else: false,
                    span: Span::in_file(file, name_start, i),
                });
                return Ok(());
            }
            "elif" => {
                let outer_live = self.stack.last().expect("stack non-empty").outer_live();
                if outer_live {
                    return Err(CompileError::new(
                        "`#elif` is not yet supported",
                        Span::in_file(file, name_start, i),
                    ));
                }
                return Ok(());
            }
            _ => {}
        }

        // Dead code gets no diagnostics. A skipped block may hold anything.
        if !self.emitting() {
            return Ok(());
        }

        match name.as_str() {
            // A `#` on a line by itself is the null directive: legal, ignored.
            "" => Ok(()),
            "define" => self.define(i, end),
            "undef" => self.undef(i, end),
            "include" => self.include(i, end, name_start),
            "error" => self.directive_error(file, i, end, true, name_start),
            "warning" => self.directive_error(file, i, end, false, name_start),
            "pragma" => self.pragma(i, end),
            "line" => Err(CompileError::new(
                format!("`#{name}` is not yet supported"),
                Span::in_file(file, name_start, i),
            )),
            other => Err(CompileError::new(
                format!("invalid preprocessing directive `#{other}`"),
                Span::in_file(file, name_start, i),
            )),
        }
    }

    fn emitting(&self) -> bool {
        self.stack.last().is_none_or(|st| st.emitting())
    }

    /// `#ifdef NAME` when `want_defined`, otherwise `#ifndef NAME`.
    fn if_defined(
        &mut self,
        from: usize,
        end: usize,
        want_defined: bool,
        directive_start: usize,
    ) -> Result<(), CompileError> {
        let (file, name, name_start, name_end) = {
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

        if name.is_empty() && self.emitting() {
            return Err(CompileError::new(
                "macro name missing after the conditional directive",
                Span::in_file(file, name_start, name_end.max(name_start + 1)),
            ));
        }

        let active = self.macros.contains(&name) == want_defined;
        let st = self.stack.last_mut().expect("stack non-empty");
        st.conds.push(Cond {
            active,
            taken: active,
            seen_else: false,
            span: Span::in_file(file, directive_start, end),
        });
        Ok(())
    }

    fn directive_else(
        &mut self,
        file: FileId,
        name_start: usize,
        name_end: usize,
    ) -> Result<(), CompileError> {
        let st = self.stack.last_mut().expect("stack non-empty");
        match st.conds.last_mut() {
            None => Err(CompileError::new(
                "`#else` without `#if`",
                Span::in_file(file, name_start, name_end),
            )),
            Some(c) => {
                if c.seen_else {
                    return Err(CompileError::new(
                        "`#else` after `#else`",
                        Span::in_file(file, name_start, name_end),
                    ));
                }
                c.seen_else = true;
                c.active = !c.taken;
                c.taken = true;
                Ok(())
            }
        }
    }

    fn directive_endif(
        &mut self,
        file: FileId,
        name_start: usize,
        name_end: usize,
    ) -> Result<(), CompileError> {
        let st = self.stack.last_mut().expect("stack non-empty");
        if st.conds.pop().is_none() {
            return Err(CompileError::new(
                "`#endif` without `#if`",
                Span::in_file(file, name_start, name_end),
            ));
        }
        Ok(())
    }

    /// `#error message` stops the build. `#warning message` does not.
    ///
    /// The message is raw text, not tokens. It often contains prose that would
    /// not lex.
    fn directive_error(
        &mut self,
        file: FileId,
        from: usize,
        end: usize,
        fatal: bool,
        name_start: usize,
    ) -> Result<(), CompileError> {
        let text = {
            let st = self.stack.last().expect("stack non-empty");
            st.chars[from.min(end)..end].iter().collect::<String>().trim().to_string()
        };

        if fatal {
            let message = if text.is_empty() {
                "`#error` directive".to_string()
            } else {
                format!("`#error`: {text}")
            };
            return Err(CompileError::new(message, Span::in_file(file, name_start, end)));
        }

        if text.is_empty() {
            eprintln!("warning: `#warning` directive");
        } else {
            eprintln!("warning: {text}");
        }
        Ok(())
    }

    /// `#include <name>` or `#include "name"`.
    ///
    /// The header becomes a new [`FileState`] on the stack. The line loop needs
    /// no other change: it always reads from the top of the stack, and pops a
    /// file when the file runs out.
    fn include(
        &mut self,
        from: usize,
        end: usize,
        directive_start: usize,
    ) -> Result<(), CompileError> {
        let (name, angled, span) = {
            let st = self.stack.last().expect("stack non-empty");
            let span = Span::in_file(st.file, directive_start, end);
            let mut i = from;
            while i < end && st.chars[i].is_whitespace() {
                i += 1;
            }
            let (close, angled) = match (i < end).then(|| st.chars[i]) {
                Some('<') => ('>', true),
                Some('"') => ('"', false),
                _ => {
                    return Err(CompileError::new(
                        "expected `<name>` or `\"name\"` after `#include`",
                        span,
                    ));
                }
            };
            i += 1;
            let start = i;
            while i < end && st.chars[i] != close {
                i += 1;
            }
            if i >= end {
                return Err(CompileError::new(
                    format!("missing closing `{close}` in `#include`"),
                    span,
                ));
            }
            (st.chars[start..i].iter().collect::<String>(), angled, span)
        };

        if self.stack.len() >= MAX_INCLUDE_DEPTH {
            return Err(CompileError::new("`#include` nested too deeply", span)
                .with_label("a header probably includes itself"));
        }

        let from_dir = self.stack.last().and_then(|st| st.dir.clone());
        let (display, text, path, dir) = match self.includes.resolve(&name, angled, from_dir.as_deref()) {
            None => {
                return Err(CompileError::new(format!("'{name}' file not found"), span)
                    .with_note(
                        format!("searched: {}", self.includes.searched_description()),
                        None,
                    ));
            }
            Some(Resolved::Bundled(text)) => (name.clone(), text.to_string(), None, None),
            Some(Resolved::File(p)) => {
                let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
                if self.once.contains(&canonical) {
                    return Ok(());
                }
                let text = std::fs::read_to_string(&p).map_err(|e| {
                    CompileError::new(format!("cannot read '{}': {e}", p.display()), span)
                })?;
                let dir = p.parent().map(|d| d.to_path_buf());
                (p.display().to_string(), text, Some(canonical), dir)
            }
        };

        let id = self.map.add(display, text.clone());
        let mut state = FileState::new(id, &text);
        state.dir = dir;
        state.path = path;
        self.chain.push(span);
        self.stack.push(state);
        Ok(())
    }

    /// Only `#pragma once` is understood. Every other pragma is ignored, which
    /// is what the standard permits for an unrecognised pragma.
    fn pragma(&mut self, from: usize, end: usize) -> Result<(), CompileError> {
        let once = {
            let st = self.stack.last().expect("stack non-empty");
            let mut i = from;
            while i < end && st.chars[i].is_whitespace() {
                i += 1;
            }
            let s = i;
            while i < end && (st.chars[i].is_alphanumeric() || st.chars[i] == '_') {
                i += 1;
            }
            let word: String = st.chars[s..i].iter().collect();
            if word == "once" { st.path.clone() } else { None }
        };
        if let Some(path) = once {
            self.once.insert(path);
        }
        Ok(())
    }

    /// `#define NAME body…` or `#define NAME(a, b) body…`
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

        let is_function_like = {
            let st = self.stack.last().expect("stack non-empty");
            name_end < end && st.chars[name_end] == '('
        };

        let (params, body_start) = if is_function_like {
            self.parse_params(file, name_end + 1, end)?
        } else {
            (Vec::new(), name_end)
        };

        let body = {
            let st = self.stack.last_mut().expect("stack non-empty");
            st.lexer.retarget(body_start, end);
            st.lexer.tokenize_region()?
        };

        let kind = if is_function_like {
            MacroKind::Function { params, body }
        } else {
            MacroKind::Object { body }
        };

        self.macros.define(
            &name,
            MacroDef { kind, name_span: Span::in_file(file, name_start, name_end) },
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

    /// Read a parameter list starting just past the `(`.
    ///
    /// Returns the parameter names and the offset just past the `)`, which is
    /// where the macro body begins.
    fn parse_params(
        &self,
        file: FileId,
        from: usize,
        end: usize,
    ) -> Result<(Vec<String>, usize), CompileError> {
        let st = self.stack.last().expect("stack non-empty");
        let mut params: Vec<String> = Vec::new();
        let mut i = from;

        loop {
            while i < end && st.chars[i].is_whitespace() {
                i += 1;
            }
            if i >= end {
                return Err(CompileError::new(
                    "missing `)` in macro parameter list",
                    Span::in_file(file, from.saturating_sub(1), end),
                ));
            }
            if st.chars[i] == ')' {
                // Only legal here when the list is empty: `#define NOW() 42`.
                if params.is_empty() {
                    return Ok((params, i + 1));
                }
                return Err(CompileError::new(
                    "expected a parameter name after `,`",
                    Span::in_file(file, i, i + 1),
                ));
            }

            let start = i;
            while i < end && (st.chars[i].is_alphanumeric() || st.chars[i] == '_') {
                i += 1;
            }
            if i == start {
                return Err(CompileError::new(
                    "expected a parameter name",
                    Span::in_file(file, i, i + 1),
                ));
            }
            let param: String = st.chars[start..i].iter().collect();
            if params.contains(&param) {
                return Err(CompileError::new(
                    format!("duplicate macro parameter `{param}`"),
                    Span::in_file(file, start, i),
                ));
            }
            params.push(param);

            while i < end && st.chars[i].is_whitespace() {
                i += 1;
            }
            if i >= end {
                return Err(CompileError::new(
                    "missing `)` in macro parameter list",
                    Span::in_file(file, from.saturating_sub(1), end),
                ));
            }
            match st.chars[i] {
                ',' => i += 1,
                ')' => return Ok((params, i + 1)),
                other => {
                    return Err(CompileError::new(
                        format!("expected `,` or `)` in macro parameter list, found `{other}`"),
                        Span::in_file(file, i, i + 1),
                    ));
                }
            }
        }
    }    
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::SourceMap;
    use crate::lexer::{Lexer, Token};
    
    // helper functions ---
    pub(crate) fn pp(src: &str) -> Result<Vec<crate::lexer::SpannedToken>, CompileError> {
        let mut map = SourceMap::single("test.c", src);
        Preprocessor::new(&mut map).run(0)
    }
    
    fn kinds(src: &str) -> Vec<Token> {
        pp(src).unwrap().into_iter().map(|t| t.token).collect()
    }
    
    fn table_after(src: &str) -> MacroTable {
        let mut map = SourceMap::single("test.c", src);
        let mut pp = Preprocessor::new(&mut map);
        pp.entry = 0;
        let text = pp.map.file(0).text.clone();
        pp.stack.push(FileState::new(0, &text));
        while pp.next_raw().unwrap().is_some() {}
        pp.macros
    }
    
    fn params_of(t: &MacroTable, name: &str) -> Vec<String> {
        match &t.get(name).expect("macro should be defined").kind {
            MacroKind::Function { params, .. } => params.clone(),
            other => panic!("expected a function-like macro, got {other:?}"),
        }
    }
    // end helper functions ---

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
    fn define_without_a_name_is_an_error() {
        let err = pp("#define\n").unwrap_err();
        assert!(err.message.contains("macro name"), "got: {}", err.message);
    }

    #[test]
    fn a_macro_nested_in_its_own_argument_is_expanded() {
        assert_eq!(kinds("#define ADD(a, b) ((a) + (b))\nint x = ADD(ADD(1, 2), 39);"),
                   kinds("int x = ((((1) + (2))) + (39));"));
    }

    #[test]
    fn a_simple_self_nested_macro_is_expanded() {
        assert_eq!(kinds("#define F(x) x\nint a = F(F(1));"), kinds("int a = 1;"));
    }

    #[test]
    fn three_deep_self_nesting() {
        assert_eq!(kinds("#define F(x) x\nint a = F(F(F(2)));"), kinds("int a = 2;"));
    }

    #[test]
    fn object_macro_in_an_argument_is_still_expanded() {
        assert_eq!(kinds("#define N 5\n#define ID(x) x\nint a = ID(N);"), kinds("int a = 5;"));
    }

    #[test]
    fn a_directly_recursive_macro_still_terminates() {
        assert_eq!(kinds("#define F(x) F(x)\nint a = F(1);"), kinds("int a = F(1);"));
    }

    #[test]
    fn a_bare_function_macro_name_ending_an_argument_is_left_alone() {
        // The peek for `(` must stop at the end of the argument.
        assert_eq!(kinds("#define G(x) x\n#define ID(y) y\nint a = ID(G) ;"),
                   kinds("int a = G ;"));
    }

    #[test]
    fn ifdef_on_an_undefined_name_skips_the_body() {
        assert_eq!(kinds("#ifdef NOPE\nint dead;\n#endif\nint live;"), kinds("int live;"));
    }

    #[test]
    fn ifdef_on_a_defined_name_keeps_the_body() {
        assert_eq!(kinds("#define YES 1\n#ifdef YES\nint a;\n#endif"), kinds("int a;"));
    }

    #[test]
    fn ifndef_is_the_inverse_of_ifdef() {
        assert_eq!(kinds("#ifndef NOPE\nint a;\n#endif"), kinds("int a;"));
        assert_eq!(kinds("#define YES 1\n#ifndef YES\nint dead;\n#endif\nint b;"), kinds("int b;"));
    }

    #[test]
    fn else_takes_the_other_branch() {
        assert_eq!(kinds("#ifdef NOPE\nint dead;\n#else\nint live;\n#endif"), kinds("int live;"));
        assert_eq!(kinds("#define Y 1\n#ifdef Y\nint live;\n#else\nint dead;\n#endif"),
                   kinds("int live;"));
    }

    #[test]
    fn a_dead_branch_may_contain_anything_at_all() {
        let src = "#ifdef NOPE\nthis is ' not | valid @@ C at all\n#endif\nint ok;";
        assert_eq!(kinds(src), kinds("int ok;"));
    }

    #[test]
    fn a_define_inside_a_dead_branch_does_not_take_effect() {
        assert_eq!(kinds("#ifdef NOPE\n#define X 9\n#endif\nint a = X;"), kinds("int a = X;"));
    }

    #[test]
    fn an_unknown_directive_inside_a_dead_branch_is_not_diagnosed() {
        assert_eq!(kinds("#ifdef NOPE\n#nonsense whatever\n#endif\nint a;"), kinds("int a;"));
    }

    #[test]
    fn an_unsupported_if_inside_a_dead_branch_is_not_diagnosed() {
        assert_eq!(kinds("#ifdef NOPE\n#if 1\nint dead;\n#endif\n#endif\nint a;"),
                   kinds("int a;"));
    }

    #[test]
    fn nested_conditionals_pair_correctly() {
        let src = "#define Y 1\n#ifdef Y\nint outer;\n#ifdef NOPE\nint dead;\n#endif\nint after;\n#endif";
        assert_eq!(kinds(src), kinds("int outer; int after;"));
    }

    #[test]
    fn a_live_conditional_nested_in_a_dead_one_stays_dead() {
        let src = "#define Y 1\n#ifdef NOPE\n#ifdef Y\nint dead;\n#endif\n#endif\nint a;";
        assert_eq!(kinds(src), kinds("int a;"));
    }

    #[test]
    fn include_guard_pattern_works() {
        let src = "#ifndef GUARD_H\n#define GUARD_H\nint once;\n#endif\n\
                   #ifndef GUARD_H\nint twice;\n#endif";
        assert_eq!(kinds(src), kinds("int once;"));
    }

    #[test]
    fn endif_without_an_opener_is_an_error() {
        let err = pp("int a;\n#endif\n").unwrap_err();
        assert!(err.message.contains("without"), "got: {}", err.message);
    }

    #[test]
    fn else_without_an_opener_is_an_error() {
        let err = pp("int a;\n#else\n").unwrap_err();
        assert!(err.message.contains("without"), "got: {}", err.message);
    }

    #[test]
    fn an_unterminated_conditional_is_reported_at_its_opening_directive() {
        let src = "#ifdef NOPE\nint dead;\n";
        let err = pp(src).unwrap_err();
        assert!(err.message.contains("unterminated"), "got: {}", err.message);
        assert_eq!(err.span.start, src.find("ifdef").unwrap());
    }

    #[test]
    fn error_directive_reports_its_message() {
        let err = pp("#error something went wrong\nint a;").unwrap_err();
        assert!(err.message.contains("something went wrong"), "got: {}", err.message);
    }

    #[test]
    fn error_directive_with_no_message() {
        let err = pp("#error\n").unwrap_err();
        assert!(err.message.contains("#error"), "got: {}", err.message);
    }

    #[test]
    fn error_inside_a_dead_branch_does_not_fire() {
        assert_eq!(kinds("#ifdef NOPE\n#error nope\n#endif\nint a;"), kinds("int a;"));
    }

    #[test]
    fn warning_directive_does_not_stop_compilation() {
        assert_eq!(kinds("#warning just so you know\nint a;"), kinds("int a;"));
    }

    // --- Task 3: #include ---

    #[test]
    fn include_brings_in_a_bundled_header() {
        assert_eq!(kinds("#include <limits.h>\nint x = INT_MAX;"),
                   kinds("int x = 2147483647;"));
    }

    #[test]
    fn text_after_an_include_still_reaches_the_output() {
        // The header must pop and hand control back to the includer.
        assert_eq!(kinds("#include <limits.h>\nint after;"), kinds("int after;"));
    }

    #[test]
    fn a_missing_header_is_an_error() {
        let err = pp("#include <nope.h>\n").unwrap_err();
        assert!(err.message.contains("file not found"), "got: {}", err.message);
        assert!(err.notes.iter().any(|n| n.message.contains("bundled")),
                "the error should list what was searched: {:?}", err.notes);
    }

    #[test]
    fn an_include_guard_stops_a_second_inclusion() {
        // limits.h guards itself, so the second read defines nothing new and
        // emits nothing.
        assert_eq!(kinds("#include <limits.h>\n#include <limits.h>\nint x = INT_MAX;"),
                   kinds("int x = 2147483647;"));
    }

    #[test]
    fn a_malformed_include_is_an_error() {
        let err = pp("#include stdio.h\n").unwrap_err();
        assert!(err.message.contains("expected"), "got: {}", err.message);
    }

    #[test]
    fn an_unclosed_include_bracket_is_an_error() {
        let err = pp("#include <stdio.h\n").unwrap_err();
        assert!(err.message.contains("missing closing"), "got: {}", err.message);
    }

    #[test]
    fn an_include_inside_a_dead_branch_is_not_read() {
        assert_eq!(kinds("#ifdef NOPE\n#include <nope.h>\n#endif\nint a;"), kinds("int a;"));
    }

    #[test]
    fn an_error_inside_a_header_names_the_including_line() {
        // stddef.h defines NULL as `((void *)0)`; nothing there fails. Use a
        // header that does: limits.h is fine, so provoke the failure with a
        // deliberately bad quoted include instead.
        let dir = std::env::temp_dir().join("vbrcc_pp_chain");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.h"), "int x = @;\n").unwrap();

        let mut map = SourceMap::single("test.c", "#include \"bad.h\"\nint a;");
        let err = Preprocessor::with_search_path(&mut map, vec![dir])
            .run(0)
            .unwrap_err();
        assert!(err.message.contains('@'), "got: {}", err.message);
        assert!(err.notes.iter().any(|n| n.message.contains("included from")),
                "the chain is missing: {:?}", err.notes);
    }

    #[test]
    fn a_quoted_include_finds_a_file_beside_its_includer() {
        let dir = std::env::temp_dir().join("vbrcc_pp_relative");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("local.h"), "#define LOCAL 7\n").unwrap();
        let main = dir.join("main.c");
        std::fs::write(&main, "#include \"local.h\"\nint x = LOCAL;\n").unwrap();

        let mut map = SourceMap::single(main.to_str().unwrap(),
                                        "#include \"local.h\"\nint x = LOCAL;\n");
        let toks: Vec<Token> = Preprocessor::new(&mut map).run(0).unwrap()
            .into_iter().map(|t| t.token).collect();
        assert!(toks.contains(&Token::IntLiteral(7)), "got {toks:?}");
    }

    #[test]
    fn pragma_once_stops_a_second_inclusion() {
        let dir = std::env::temp_dir().join("vbrcc_pp_once");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("once.h"), "#pragma once\nint marker;\n").unwrap();

        let mut map = SourceMap::single("test.c", "#include <once.h>\n#include <once.h>\n");
        let toks: Vec<Token> = Preprocessor::with_search_path(&mut map, vec![dir])
            .run(0).unwrap().into_iter().map(|t| t.token).collect();
        assert_eq!(toks.iter().filter(|t| **t == Token::Ident("marker".to_string())).count(), 1,
                   "got {toks:?}");
    }

    #[test]
    fn an_unknown_pragma_is_ignored() {
        assert_eq!(kinds("#pragma pack(1)\nint a;"), kinds("int a;"));
    }

    #[test]
    fn a_cyclic_include_hits_the_depth_cap() {
        let dir = std::env::temp_dir().join("vbrcc_pp_cycle");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("loop.h"), "#include <loop.h>\n").unwrap();

        let mut map = SourceMap::single("test.c", "#include <loop.h>\n");
        let err = Preprocessor::with_search_path(&mut map, vec![dir]).run(0).unwrap_err();
        assert!(err.message.contains("too deeply"), "got: {}", err.message);
    }

    #[test]
    fn a_conditional_must_close_in_the_file_that_opens_it() {
        let dir = std::env::temp_dir().join("vbrcc_pp_unbalanced");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("open.h"), "#ifdef NOPE\n").unwrap();

        let mut map = SourceMap::single("test.c", "#include <open.h>\n#endif\n");
        let err = Preprocessor::with_search_path(&mut map, vec![dir]).run(0).unwrap_err();
        assert!(err.message.contains("unterminated"), "got: {}", err.message);
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


    #[test]
    fn function_like_define_records_its_parameters() {
        let t = table_after("#define ADD(a, b) a + b\n");
        assert_eq!(params_of(&t, "ADD"), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn function_like_define_with_no_parameters() {
        let t = table_after("#define NOW() 42\n");
        assert_eq!(params_of(&t, "NOW"), Vec::<String>::new());
    }

    #[test]
    fn function_like_define_body_is_tokenized() {
        let t = table_after("#define ADD(a, b) a + b\n");
        match &t.get("ADD").unwrap().kind {
            MacroKind::Function { body, .. } => {
                let kinds: Vec<Token> = body.iter().map(|t| t.token.clone()).collect();
                assert_eq!(kinds, vec![
                    Token::Ident("a".to_string()),
                    Token::Plus,
                    Token::Ident("b".to_string()),
                ]);
            }
            other => panic!("expected function-like, got {other:?}"),
        }
    }

    #[test]
    fn a_space_before_the_paren_means_object_like() {
        // `#define F (x)` defines F as the token sequence `( x )`, not a macro
        // taking a parameter. The space is the entire difference.
        let t = table_after("#define F (x)\n");
        assert!(matches!(t.get("F").unwrap().kind, MacroKind::Object { .. }));
    }

    #[test]
    fn duplicate_parameter_names_are_rejected() {
        let err = pp("#define BAD(a, a) a\n").unwrap_err();
        assert!(err.message.contains("duplicate"), "got: {}", err.message);
    }

    #[test]
    fn unterminated_parameter_list_is_rejected() {
        let err = pp("#define BAD(a, b\nint x;").unwrap_err();
        assert!(err.message.contains("missing `)`"), "got: {}", err.message);
    }

    #[test]
    fn a_missing_parameter_name_is_rejected() {
        let err = pp("#define BAD(a, ) a\n").unwrap_err();
        assert!(err.message.contains("parameter name"), "got: {}", err.message);
    }

    #[test]
    fn function_like_macro_body_may_use_a_continuation() {
        let t = table_after("#define ADD(a, b) a + \\\n    b\n");
        assert_eq!(params_of(&t, "ADD"), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn function_macro_substitutes_its_arguments() {
        assert_eq!(kinds("#define ADD(a, b) a + b\nint x = ADD(1, 2);"),
                   kinds("int x = 1 + 2;"));
    }

    #[test]
    fn arguments_may_be_arbitrary_expressions() {
        assert_eq!(kinds("#define SQ(x) ((x) * (x))\nint y = SQ(3 + 1);"),
                   kinds("int y = ((3 + 1) * (3 + 1));"));
    }

    #[test]
    fn a_parameter_used_twice_is_substituted_twice() {
        assert_eq!(kinds("#define TWICE(x) x + x\nint y = TWICE(7);"),
                   kinds("int y = 7 + 7;"));
    }

    #[test]
    fn commas_inside_parentheses_do_not_split_arguments() {
        assert_eq!(kinds("#define ONE(x) x\nint y = ONE(f(1, 2));"),
                   kinds("int y = f(1, 2);"));
    }

    #[test]
    fn arguments_may_span_lines() {
        // The whole reason the driver is a pull source rather than a line filter.
        assert_eq!(kinds("#define ADD(a, b) a + b\nint x = ADD(1,\n            2);"),
                   kinds("int x = 1 + 2;"));
    }

    #[test]
    fn a_function_macro_name_without_parens_is_left_alone() {
        // C is explicit about this: no `(` means no invocation.
        assert_eq!(kinds("#define F(x) x\nint F;"), kinds("int F;"));
    }

    #[test]
    fn the_token_after_a_bare_macro_name_is_not_swallowed() {
        // Regression guard for the peek-and-pushback: `;` must survive.
        assert_eq!(kinds("#define F(x) x\nint F; int y;"), kinds("int F; int y;"));
    }

    #[test]
    fn zero_parameter_macro_invocation() {
        assert_eq!(kinds("#define NOW() 42\nint t = NOW();"), kinds("int t = 42;"));
    }

    #[test]
    fn one_empty_argument_is_legal() {
        // `F()` for a one-parameter macro passes a single empty argument.
        assert_eq!(kinds("#define E(x) [x]\nint a E() ;"), kinds("int a [] ;"));
    }

    #[test]
    fn nested_function_macro_expansion() {
        assert_eq!(kinds("#define SQ(x) ((x) * (x))\n#define ADD(a, b) a + b\nint y = ADD(SQ(2), 3);"),
                   kinds("int y = ((2) * (2)) + 3;"));
    }

    #[test]
    fn too_few_arguments_is_an_error() {
        let err = pp("#define ADD(a, b) a + b\nint x = ADD(1);").unwrap_err();
        assert!(err.message.contains("requires 2"), "got: {}", err.message);
    }

    #[test]
    fn too_many_arguments_is_an_error() {
        let err = pp("#define ADD(a, b) a + b\nint x = ADD(1, 2, 3);").unwrap_err();
        assert!(err.message.contains("requires 2"), "got: {}", err.message);
    }

    #[test]
    fn unterminated_argument_list_is_an_error() {
        let err = pp("#define ADD(a, b) a + b\nint x = ADD(1, 2;").unwrap_err();
        assert!(err.message.contains("unterminated argument list"), "got: {}", err.message);
    }
}