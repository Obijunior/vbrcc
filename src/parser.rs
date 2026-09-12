//! Stage 2: the parser.
//!
//! [`Parser::parse_program`] reads the tokens from the preprocessor and builds a
//! [`crate::ast::Program`]. A program holds a list of function definitions and a list
//! of prototypes. A definition has typed parameters, a return type, and a body. A
//! prototype has no body, and a header supplies it.
//!
//! A definition and a prototype start the same way, so one method parses both. The
//! token after the parameter list decides which it is: `;` makes a prototype, and `{`
//! makes a definition.
//!
//! The parser is recursive descent, with one method for each grammar rule. It parses
//! expressions by precedence climbing: each level calls the level above it and combines
//! the results as it returns.
//!
//! # What this stage does not do
//!
//! The parser checks shape, not meaning. It accepts an assignment whose left side can
//! never be an lvalue, and [`crate::typeck`] rejects it later. This split lets a type
//! error carry a useful message instead of appearing as a syntax error.
//!
//! Every expression therefore goes into a `TypedExpr` with the type `Type::Unknown`.
//! The type checker fills the type in later. Every statement goes into a
//! `Spanned<Stmt>`, which keeps the source location for diagnostics.

use std::collections::HashMap;

use crate::lexer::{Token, SpannedToken};
use crate::ast::*;
use crate::diagnostic::{CompileError, Span, Spanned};

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    typedefs: HashMap<String, Type>,
    structs: HashMap<String, Type>,
    enums: HashMap<String, Type>,
    /// Every enumerator seen so far, with its value. An enumeration constant
    /// has type `int` in C, so the parser substitutes the value at each use and
    /// no later stage ever sees the name.
    enum_constants: HashMap<String, i64>,
}

/// What one top-level item turned out to be.
enum TopLevel {
    Function(Function),
    Decl(FuncDecl),
    /// One declaration may declare several globals: `int a, b = 5;`.
    Global(Vec<GlobalVar>),
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser {
            tokens, pos: 0,
            typedefs: HashMap::new(), structs: HashMap::new(),
            enums: HashMap::new(), enum_constants: HashMap::new(),
        }
    }

    // --- Token navigation ---

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).map(|st| &st.token).unwrap_or(&Token::EOF)
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|st| st.span)
            .unwrap_or_else(|| self.tokens.last().map(|st| st.span).unwrap_or(Span::dummy()))
    }

    fn previous_span(&self) -> Span {
        if self.pos == 0 {
            Span::dummy()
        } else {
            self.tokens.get(self.pos - 1).map(|st| st.span).unwrap_or(Span::dummy())
        }
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).map(|st| &st.token).unwrap_or(&Token::EOF);
        self.pos += 1;
        tok
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos + 1).map(|st| &st.token).unwrap_or(&Token::EOF)
    }

    fn peek2(&self) -> &Token {
        self.tokens.get(self.pos + 2).map(|st| &st.token).unwrap_or(&Token::EOF)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), CompileError> {
        let span = self.current_span();
        let tok = self.advance().clone();
        if tok == *expected {
            Ok(())
        } else {
            Err(CompileError::new(
                format!("expected {}, found {}", expected.describe(), tok.describe()),
                span,
            )
            .with_label(format!("expected {} here", expected.describe())))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, CompileError> {
        let span = self.current_span();
        match self.advance().clone() {
            Token::Ident(s) => Ok(s),
            other => Err(CompileError::new(
                format!("expected {what}, found {}", other.describe()),
                span,
            )),
        }
    }

    /// Assign each member a byte offset. A member sits at the next offset that
    /// is a multiple of its own alignment. The struct's alignment is the
    /// largest member alignment (at least 1); its size is the end of the last
    /// member rounded up to that alignment.
    fn layout_struct(members: &[(Type, String)]) -> (Vec<StructField>, usize, usize) {
        let mut fields = Vec::with_capacity(members.len());
        let mut offset = 0usize;
        let mut align = 1usize;
        for (ty, name) in members {
            let a = ty.align().max(1);
            align = align.max(a);
            offset = (offset + a - 1) & !(a - 1);
            fields.push(StructField { name: name.clone(), ty: ty.clone(), offset });
            offset += ty.size();
        }
        let size = (offset + align - 1) & !(align - 1);
        (fields, size.max(align), align)
    }


    // --- Grammar rules ---

    pub fn parse_program(&mut self) -> Result<Program, CompileError> {
        let mut functions = Vec::new();
        let mut decls = Vec::new();
        let mut globals = Vec::new();
        while self.current() != &Token::EOF {
            if self.current() == &Token::Typedef {
                self.parse_typedef()?;
                continue;
            }
            if self.current() == &Token::Enum {
                // `enum E { ... };` may stand alone or carry a declarator after
                // the closing brace, exactly like a struct definition.
                let looks_like_def = self.peek() == &Token::LBrace
                    || (matches!(self.peek(), Token::Ident(_)) && self.peek2() == &Token::LBrace);
                if looks_like_def {
                    let start = self.current_span();
                    let ty = self.parse_enum_type()?;   // registers the constants
                    if self.current() == &Token::Semicolon {
                        self.advance();
                        continue;
                    }
                    match self.parse_declarator_tail(start, ty)? {
                        TopLevel::Function(f) => functions.push(f),
                        TopLevel::Decl(d) => decls.push(d),
                        TopLevel::Global(g) => globals.extend(g),
                    }
                    continue;
                }
            }
            if self.current() == &Token::Struct {
                // A `{`-bodied struct type here is a definition. It may stand
                // alone (`struct T { ... };`) or carry a declarator right
                // after the closing brace (`struct T { ... } name;`).
                // (peek past an optional tag to a `{`)
                let looks_like_def = self.peek() == &Token::LBrace
                    || (matches!(self.peek(), Token::Ident(_)) && self.peek2() == &Token::LBrace);
                if looks_like_def {
                    let start = self.current_span();
                    let ty = self.parse_struct_type()?;   // registers it in self.structs
                    if self.current() == &Token::Semicolon {
                        self.advance();
                        continue;
                    }
                    match self.parse_declarator_tail(start, ty)? {
                        TopLevel::Function(f) => functions.push(f),
                        TopLevel::Decl(d) => decls.push(d),
                        TopLevel::Global(g) => globals.extend(g),
                    }
                    continue;
                }
            }

            match self.parse_top_level()? {
                TopLevel::Function(f) => functions.push(f),
                TopLevel::Decl(d) => decls.push(d),
                TopLevel::Global(g) => globals.extend(g),
            }
        }
        Ok(Program { functions, decls, globals })
    }

    fn is_type_start(&self, tok: &Token) -> bool {
        matches!(tok, Token::Int | Token::Char | Token::Bool | Token::Long | Token::Void | Token::Const | Token::Struct | Token::Enum)
            || matches!(tok, Token::Ident(name) if self.typedefs.contains_key(name))
    }

    fn parse_block(&mut self) -> Result<Vec<Spanned<Stmt>>, CompileError> {
        if self.current() == &Token::LBrace {
            // Braced body: consume statements until the matching RBrace.
            self.advance(); // consume '{'
            let mut stmts = Vec::new();
            while self.current() != &Token::RBrace && self.current() != &Token::EOF {
                self.parse_statement_into(&mut stmts)?;
            }
            self.expect(&Token::RBrace)?;
            return Ok(stmts);
        }
        // Brace-less body: exactly one statement.
        return Ok(vec![self.parse_statement()?]);
    }

    /// One statement, appended to `out`. A declaration may add more than one
    /// [`Stmt::VarDecl`], so this appends instead of returning one statement.
    fn parse_statement_into(&mut self, out: &mut Vec<Spanned<Stmt>>) -> Result<(), CompileError> {
        if self.is_type_start(self.current()) {
            out.extend(self.parse_decl()?);
            return Ok(());
        }
        out.push(self.parse_statement()?);
        Ok(())
    }

    /// The declaration specifiers: `const`, then `struct { ... }`, a type
    /// keyword, or a typedef name. Stops at the first `*`.
    ///
    /// A `*` belongs to the declarator, not to the specifiers. In `int *a, b;`
    /// only `a` is a pointer. A declarator list therefore reads the base type
    /// once here, then calls [`Parser::parse_pointer_suffix`] for each name.
    fn parse_base_type(&mut self) -> Result<Type, CompileError> {
        // `const` carries nothing this compiler acts on. Accept it and drop it,
        // so a header written for a real compiler still parses.
        while self.current() == &Token::Const {
            self.advance();
        }
        let span = self.current_span();
        if self.current() == &Token::Struct {
            return self.parse_struct_type();
        }
        if self.current() == &Token::Enum {
            return self.parse_enum_type();
        }

        let ty = match self.advance().clone() {
            Token::Int => Type::Int,
            Token::Char => Type::Char,
            Token::Bool => Type::Bool,
            // `long long` is one type, spelled with two tokens.
            Token::Long => {
                if self.current() == &Token::Long {
                    self.advance();
                    Type::LongLong
                } else {
                    Type::Long
                }
            }
            Token::Void => Type::Void,
            Token::Ident(name) => match self.typedefs.get(&name) {
                Some(t) => t.clone(),
                None => return Err(CompileError::new(
                    format!("unknown type `{}`", name),
                    span,
                )),
            }
            other => {
                return Err(CompileError::new(
                    format!("expected a type, found {}", other.describe()),
                    span,
                )
                .with_label("expected `int`, `char`, `long`, or `void`"));
            }
        };
        Ok(ty)
    }

    /// The `*`s one declarator adds to the base type, and any `const` between
    /// them. This cannot fail, so it returns a `Type` and not a `Result`.
    fn parse_pointer_suffix(&mut self, mut ty: Type) -> Type {
        loop {
            if self.current() == &Token::Star {
                self.advance();
                ty = Type::Pointer(Box::new(ty));
            } else if self.current() == &Token::Const {
                self.advance();
            } else {
                return ty;
            }
        }
    }

    /// The `[N]` groups after a declarator name, or `base` unchanged when no
    /// `[` follows. Dimensions fold from the right: `int x[3][4]` is "array 3
    /// of (array 4 of int)", so the last `[N]` is the innermost element type.
    ///
    /// `allow_unsized_first` permits one leading `[]`, whose length the
    /// initializer supplies. The returned flag reports that case. Only the
    /// first dimension may be empty; C needs every inner dimension to compute
    /// a row stride.
    fn parse_array_suffix(
        &mut self,
        base: Type,
        allow_unsized_first: bool,
    ) -> Result<(Type, bool), CompileError> {
        let mut dims: Vec<usize> = Vec::new();
        let mut unsized_first = false;

        while self.current() == &Token::LBracket {
            self.advance(); // '['

            if self.current() == &Token::RBracket {
                if !(dims.is_empty() && allow_unsized_first) {
                    return Err(CompileError::new(
                        "only the first array dimension may be empty",
                        self.current_span(),
                    )
                    .with_label("an inner dimension sets the row stride"));
                }
                unsized_first = true;
                dims.push(0);
                self.advance(); // ']'
                continue;
            }

            let tok = self.advance().clone();
            let len = match &tok {
                Token::IntLiteral(n) if *n >= 0 => *n as usize,
                // `int a[MAX];` where MAX is an enumerator.
                Token::Ident(name) => match self.enum_constants.get(name) {
                    Some(&value) if value >= 0 => value as usize,
                    _ => {
                        return Err(CompileError::new(
                            format!("expected array length, found {}", tok.describe()),
                            self.previous_span(),
                        ));
                    }
                },
                other => {
                    return Err(CompileError::new(
                        format!("expected array length, found {}", other.describe()),
                        self.previous_span(),
                    ));
                }
            };
            self.expect(&Token::RBracket)?;
            dims.push(len);
        }

        let ty = dims
            .into_iter()
            .rev()
            .fold(base, |elem, len| Type::Array(Box::new(elem), len));
        Ok((ty, unsized_first))
    }

    /// An initializer: a brace-enclosed list, or a single expression.
    ///
    /// Lists nest, so a `{` inside a list starts a sub-list. A trailing comma
    /// before `}` is legal. The leaf calls [`Parser::parse_expr`], which stops
    /// at a comma, so commas stay list separators. The type checker matches the
    /// shape of the result against the declared type; the parser does not.
    fn parse_initializer(&mut self) -> Result<TypedExpr, CompileError> {
        if self.current() != &Token::LBrace {
            return self.parse_expr();
        }
        let start = self.current_span();
        self.advance(); // '{'

        let mut elems = Vec::new();
        while self.current() != &Token::RBrace {
            elems.push(self.parse_initializer()?);
            if self.current() != &Token::Comma {
                break;
            }
            self.advance(); // ','
        }
        self.expect(&Token::RBrace)?;

        let span = start.to(self.previous_span());
        Ok(TypedExpr::new(Expr::InitList(elems), span))
    }

    /// One complete type: specifiers plus one declarator's pointer suffix.
    /// Correct where exactly one declarator can appear, which is casts,
    /// parameters, and typedefs.
    fn parse_type(&mut self) -> Result<Type, CompileError> {
        let base = self.parse_base_type()?;
        Ok(self.parse_pointer_suffix(base))
    }

    /// One top-level item. A prototype and a definition share a head, so both
    /// are parsed here and told apart by the token after the parameter list.
    fn parse_top_level(&mut self) -> Result<TopLevel, CompileError> {
        let start = self.current_span();
        let base = self.parse_base_type()?;
        self.parse_declarator_tail(start, base)
    }

    /// The rest of a top-level item once its type is already in hand: a
    /// name, then either a global tail or a parameter list and a body/`;`.
    fn parse_declarator_tail(&mut self, start: Span, base_type: Type) -> Result<TopLevel, CompileError> {
        // `base_type` is the shared specifier type.
        let return_type = self.parse_pointer_suffix(base_type.clone());
        let name = match self.advance().clone() {
            Token::Ident(s) => s,
            other => {
                return Err(CompileError::new(
                    format!("expected function name, found {}", other.describe()),
                    self.previous_span(),
                ));
            }
        };

        if self.current() != &Token::LParen {
            return self.parse_global_tail(start, base_type, return_type, name);
        }

        self.expect(&Token::LParen)?;
        let (params, variadic) = self.parse_param_list()?;
        self.expect(&Token::RParen)?;

        if self.current() == &Token::Semicolon {
            self.advance();
            let span = start.to(self.previous_span());
            return Ok(TopLevel::Decl(FuncDecl { name, return_type, params, variadic, span }));
        }

        if variadic {
            return Err(CompileError::new(
                format!("cannot define the variadic function `{name}`"),
                start.to(self.previous_span()),
            )
            .with_label("`va_arg` is not supported; declare it instead"));
        }

        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while self.current() != &Token::RBrace && self.current() != &Token::EOF {
            self.parse_statement_into(&mut body)?;
        }
        self.expect(&Token::RBrace)?;

        let span = start.to(self.previous_span());
        Ok(TopLevel::Function(Function { name, params, return_type, body, span }))
    }


    /// The tail of a top-level declaration once a `(` is ruled out. Reads the
    /// rest of the init-declarator list:
    /// `[ '[' N? ']' ] [ '=' initializer ] { ',' declarator }* ';'`.
    ///
    /// `base_type` is the shared specifier type, used to restart each later
    /// declarator. `ty` and `name` are the first declarator, which
    /// [`Parser::parse_declarator_tail`] already read to rule out a `(`.
    fn parse_global_tail(
        &mut self,
        start: Span,
        base_type: Type,
        ty: Type,
        name: String,
    ) -> Result<TopLevel, CompileError> {
        let mut globals = Vec::new();
        let mut decl_start = start;
        let mut ty = ty;
        let mut name = name;

        loop {
            let unsized_array;
            (ty, unsized_array) = self.parse_array_suffix(ty, true)?;

            let init = if self.current() == &Token::Assign {
                self.advance(); // '='
                Some(self.parse_initializer()?)
            } else {
                None
            };

            // Each declarator stands alone: `int a[] = "x", b[];` rejects only `b`.
            if unsized_array && init.is_none() {
                return Err(CompileError::new(
                    "array size missing",
                    decl_start.to(self.previous_span()),
                )
                .with_label("an unsized array needs a string or brace initializer"));
            }

            let span = decl_start.to(self.previous_span());
            globals.push(GlobalVar { ty, name, init, span });

            if self.current() != &Token::Comma {
                break;
            }
            self.advance(); // ','
            decl_start = self.current_span();
            ty = self.parse_pointer_suffix(base_type.clone());
            name = self.expect_ident("a variable name")?;
        }

        self.expect(&Token::Semicolon)?;
        Ok(TopLevel::Global(globals))
    }

    /// Parameters between `(` and `)`, with the `)` left unconsumed.
    fn parse_param_list(&mut self) -> Result<(Vec<(Type, String)>, bool), CompileError> {
        let mut params = Vec::new();

        // `(void)` means no parameters, not one parameter of type void.
        if self.current() == &Token::Void && self.peek() == &Token::RParen {
            self.advance();
            return Ok((params, false));
        }

        while self.current() != &Token::RParen {
            if self.current() == &Token::Ellipsis {
                self.advance();
                if self.current() != &Token::RParen {
                    return Err(CompileError::new(
                        "`...` must be the last parameter",
                        self.previous_span(),
                    ));
                }
                return Ok((params, true));
            }

            let ptype = self.parse_type()?;
            let pname = match self.current().clone() {
                Token::Ident(s) => {
                    self.advance();
                    s
                }
                _ => String::new(),
            };
            let ptype = if self.current() == &Token::LBracket {
                self.advance();
                if matches!(self.current(), Token::IntLiteral(_)) {
                    self.advance();
                }
                self.expect(&Token::RBracket)?;
                Type::Pointer(Box::new(ptype))
            } else {
                ptype
            };
            params.push((ptype, pname));
            if self.current() == &Token::Comma {
                self.advance();
            }
        }
        Ok((params, false))
    }

    fn parse_statement(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        let node = match self.current().clone() {
            Token::Return => {
                self.advance(); // consume 'return'
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Stmt::Return(expr)
            }

            // Reached from a brace-less body and from a `for` initializer,
            // neither of which has anywhere to put a second declarator.
            tok if self.is_type_start(&tok) => {
                let mut decls = self.parse_decl()?;
                if decls.len() > 1 {
                    return Err(CompileError::new(
                        "a declaration with several names is not supported here",
                        decls[1].span,
                    )
                    .with_label("declare one name at a time in a `for` initializer"));
                }
                return Ok(decls.remove(0));
            }
            Token::For => return self.parse_for(),
            Token::While => return self.parse_while(),
            Token::If => return self.parse_if(),
            _ => {
                let expr = self.parse_expr()?;
                self.expect(&Token::Semicolon)?;
                Stmt::Expr(expr)
            }
        };
        Ok(Spanned::new(node, start.to(self.previous_span())))
    }

    // --- Expression parsing with precedence climbing ---
    //
    // Precedence (low to high):
    //   1. + -          (additive)
    //   2. * /          (multiplicative)
    //   3. unary - ~ !  (unary)
    //   4. literals, identifiers, ( expr )

    fn parse_expr(&mut self) -> Result<TypedExpr, CompileError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<TypedExpr, CompileError> {
        let lhs = self.parse_ternary()?;
        let start_span = lhs.span;

        // postfix ++ / --
        if self.current() == &Token::PlusPlus || self.current() == &Token::MinusMinus {
            let op = if self.current() == &Token::PlusPlus { IncDec::Inc } else { IncDec::Dec };
            self.advance();
            let span = start_span.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::PostIncDec(op, Box::new(lhs)), span));
        }

        let assign_op = match self.current() {
            Token::Assign => Some(None),
            Token::PlusEquals => Some(Some(BinaryOp::Add)),
            Token::MinusEquals => Some(Some(BinaryOp::Sub)),
            Token::StarEquals => Some(Some(BinaryOp::Mul)),
            Token::SlashEquals => Some(Some(BinaryOp::Div)),
            Token::ModuloEquals => Some(Some(BinaryOp::Mod)),
            Token::AmpersandEquals => Some(Some(BinaryOp::BitAnd)),
            Token::PipeEquals => Some(Some(BinaryOp::BitOr)),
            Token::CaretEquals => Some(Some(BinaryOp::BitXor)),
            Token::ShlEquals => Some(Some(BinaryOp::Shl)),
            Token::ShrEquals => Some(Some(BinaryOp::Shr)),
            _ => None,
        };
        let Some(assign_op) = assign_op else { return Ok(lhs); };
        self.advance(); // consume the assignment operator
        let rhs = self.parse_assignment()?;
        let span = start_span.to(self.previous_span());
        let value = match assign_op {
            None => rhs,
            Some(op) => TypedExpr::new(
                Expr::BinaryOp(op, Box::new(lhs.clone()), Box::new(rhs)),
                span,
            ),
        };
        Ok(TypedExpr::new(Expr::Assign(Box::new(lhs), Box::new(value)), span))
    }

    fn parse_logical_or(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_logical_and()?;
        while let Token::LogicalOr = self.current() {
            self.advance();
            let right = self.parse_logical_and()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(BinaryOp::LogicalOr, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_bit_or()?;
        while let Token::LogicalAnd = self.current() {
            self.advance();
            let right = self.parse_bit_or()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(BinaryOp::LogicalAnd, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_bit_or(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_bit_xor()?;
        while self.current() == &Token::Pipe {
            self.advance();
            let right = self.parse_bit_xor()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(BinaryOp::BitOr, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_bit_and()?;
        while self.current() == &Token::Caret {
            self.advance();
            let right = self.parse_bit_and()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(BinaryOp::BitXor, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_comparison()?;
        while self.current() == &Token::Ampersand {
            self.advance();
            let right = self.parse_comparison()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(BinaryOp::BitAnd, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_additive()?;
        loop {
            let op = match self.current() {
                Token::Shl => BinaryOp::Shl,
                Token::Shr => BinaryOp::Shr,
                _ => break,
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }


    fn parse_comparison(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_shift()?;
        loop {
            let op = match self.current() {
                Token::LessThan => BinaryOp::Lt,
                Token::LessThanEquals => BinaryOp::Lte,
                Token::GreaterThan => BinaryOp::Gt,
                Token::GreaterThanEquals => BinaryOp::Gte,
                Token::Equals => BinaryOp::Eq,
                Token::NotEquals => BinaryOp::Neq,
                _ => break,
            };
            self.advance();
            let right = self.parse_shift()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.current() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<TypedExpr, CompileError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.current() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Modulo => BinaryOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            let span = left.span.to(right.span);
            left = TypedExpr::new(Expr::BinaryOp(op, Box::new(left), Box::new(right)), span);
        }
        Ok(left)
    }

    /// `typedef <type> <name> [ '[' N ']' ] ;` — resolves `name` to a real
    /// `Type` here. Nothing past the parser ever learns a typedef existed.
    fn parse_typedef(&mut self) -> Result<(), CompileError> {
        self.expect(&Token::Typedef)?;
        let base = self.parse_base_type()?;
        loop {
            let ty = self.parse_pointer_suffix(base.clone());
            let name = self.expect_ident("a typedef name")?;
            let name_span = self.previous_span();
            let (ty, _) = self.parse_array_suffix(ty, false)?;

            if let Some(existing) = self.typedefs.get(&name) {
                if *existing != ty {
                    return Err(CompileError::new(
                        format!("`{name}` redefined as `{}`, previously `{}`",
                                ty.describe(), existing.describe()),
                        name_span,
                    ));
                }
            } else {
                self.typedefs.insert(name, ty);
            }

            if self.current() != &Token::Comma {
                break;
            }
            self.advance(); // ','
        }
        self.expect(&Token::Semicolon)?;
        Ok(())
    }

    fn parse_for(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'for'
        self.expect(&Token::LParen)?;

        let init = Box::new(self.parse_statement()?);
        let cond = self.parse_expr()?;
        self.expect(&Token::Semicolon)?;
        let update_start = self.current_span();
        let update_expr = self.parse_expr()?;
        let update = Box::new(Spanned::new(
            Stmt::Expr(update_expr),
            update_start.to(self.previous_span()),
        ));

        self.expect(&Token::RParen)?;
        let body = self.parse_block()?;

        Ok(Spanned::new(Stmt::For { init, cond, update, body }, start.to(self.previous_span())))
    }

    fn parse_while(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'while'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;
        let body = self.parse_block()?;
        Ok(Spanned::new(Stmt::While { cond, body }, start.to(self.previous_span())))
    }

    fn parse_if(&mut self) -> Result<Spanned<Stmt>, CompileError> {
        let start = self.current_span();
        self.advance(); // 'if'
        self.expect(&Token::LParen)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::RParen)?;

        let then_branch = self.parse_block()?;

        let else_branch = if self.current() == &Token::Else {
            self.advance();
            self.parse_block()?
        } else {
            Vec::new()
        };

        Ok(Spanned::new(Stmt::If { cond, then_branch, else_branch }, start.to(self.previous_span())))
    }

    /// A local declaration, which may name several variables:
    /// `type declarator [ '=' init ] { ',' declarator [ '=' init ] }* ';'`.
    ///
    /// Returns one [`Stmt::VarDecl`] per declarator, which the caller splices
    /// into the enclosing statement list. Scope is flat, so a spliced sibling
    /// statement and a second declarator mean the same thing to the type
    /// checker.
    fn parse_decl(&mut self) -> Result<Vec<Spanned<Stmt>>, CompileError> {
        let mut decl_start = self.current_span();
        let base = self.parse_base_type()?;
        let mut out = Vec::new();

        loop {
            let ty = self.parse_pointer_suffix(base.clone());
            let name = self.expect_ident("a variable name")?;
            let (ty, _) = self.parse_array_suffix(ty, false)?;
            let init = if self.current() == &Token::Assign {
                self.advance();
                Some(self.parse_initializer()?)
            } else {
                None
            };
            out.push(Spanned::new(
                Stmt::VarDecl { ty, name, init },
                decl_start.to(self.previous_span()),
            ));

            if self.current() != &Token::Comma {
                break;
            }
            self.advance(); // ','
            decl_start = self.current_span();
        }

        self.expect(&Token::Semicolon)?;
        Ok(out)
    }

    fn parse_unary(&mut self) -> Result<TypedExpr, CompileError> {
        let start = self.current_span();

        // cast: ( type ) unary
        if self.current() == &Token::LParen && self.is_type_start(self.peek()) {
            self.advance(); // (
            let ty = self.parse_type()?;
            self.expect(&Token::RParen)?;
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::Cast(ty, Box::new(operand)), span));
        }

        if let Some(op) = match self.current() {
            Token::Minus => Some(UnaryOp::Negate),
            Token::Tilde => Some(UnaryOp::BitNot),
            Token::Bang => Some(UnaryOp::LogNot),
            _ => None,
        } {
            self.advance();
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::UnaryOp(op, Box::new(operand)), span));
        }

        if self.current() == &Token::Ampersand {
            self.advance();
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::AddressOf(Box::new(operand)), span));
        }
        if self.current() == &Token::Star {
            self.advance();
            let operand = self.parse_unary()?;
            let span = start.to(self.previous_span());
            return Ok(TypedExpr::new(Expr::Deref(Box::new(operand)), span));
        }

        self.parse_postfix()
    }

    fn parse_ternary(&mut self) -> Result<TypedExpr, CompileError> {
        let cond = self.parse_logical_or()?;
        if self.current() != &Token::Question {
            return Ok(cond);
        }
        self.advance(); // '?'
        let then_branch = self.parse_expr()?;      // a full expression sits between ? and :
        self.expect(&Token::Colon)?;
        let else_branch = self.parse_ternary()?;   // right-associative
        let span = cond.span.to(else_branch.span);
        Ok(TypedExpr::new(
            Expr::Ternary(Box::new(cond), Box::new(then_branch), Box::new(else_branch)),
            span,
        ))
    }


    fn parse_postfix(&mut self) -> Result<TypedExpr, CompileError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.current() {
                Token::LBracket => {
                    let start = expr.span;
                    self.advance(); // [
                    let idx = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    let span = start.to(self.previous_span());
                    expr = TypedExpr::new(Expr::Index(Box::new(expr), Box::new(idx)), span);
                }
                Token::Dot => {
                    let start = expr.span;
                    self.advance();
                    let field = self.expect_ident("a member name")?;
                    let span = start.to(self.previous_span());
                    expr = TypedExpr::new(Expr::Member(Box::new(expr), field), span);
                }
                Token::Arrow => {
                    let start = expr.span;
                    self.advance();
                    let field = self.expect_ident("a member name")?;
                    let span = start.to(self.previous_span());
                    let deref = TypedExpr::new(Expr::Deref(Box::new(expr)), span);
                    expr = TypedExpr::new(Expr::Member(Box::new(deref), field), span);
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<TypedExpr, CompileError> {
        let start = self.current_span();
        let node = match self.advance().clone() {
            Token::IntLiteral(n) => Expr::IntLiteral(n),
            Token::CharLiteral(n) => Expr::IntLiteral(n),
            Token::StringLiteral(s) => Expr::StringLiteral(s),
            Token::Ident(name) => {
                if self.current() == &Token::LParen {
                    self.advance();
                    let mut args = Vec::new();
                    while self.current() != &Token::RParen {
                        args.push(self.parse_expr()?);
                        if self.current() == &Token::Comma {
                            self.advance();
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Expr::FunctionCall { name, args }
                } else if let Some(&value) = self.enum_constants.get(&name) {
                    Expr::IntLiteral(value)
                } else {
                    Expr::Var(name)
                }
            }
            Token::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                // Re-span the parenthesized expression to include the parens.
                return Ok(TypedExpr::new(inner.node, start.to(self.previous_span())));
            }
            other => {
                return Err(CompileError::new(
                    format!("unexpected {} in expression", other.describe()),
                    self.previous_span(),
                ));
            }
        };
        Ok(TypedExpr::new(node, start.to(self.previous_span())))
    }

    /// Parse an `enum` type reference or definition. Current token is `Token::Enum`.
    ///
    /// C gives every enumeration constant type `int`, and an enum type is
    /// compatible with `int`. So this method records each value in
    /// `enum_constants` and [`Self::parse_primary`] substitutes it at each use.
    /// No later stage sees an enumerator name, and the code generator needs no
    /// enum support at all.
    fn parse_enum_type(&mut self) -> Result<Type, CompileError> {
        self.advance(); // the `enum` keyword
        let tag = match self.current().clone() {
            Token::Ident(name) => { self.advance(); Some(name) }
            _ => None,
        };

        if self.current() != &Token::LBrace {
            // A bare reference: `enum Tag`. Must already be defined.
            let name = tag.ok_or_else(|| CompileError::new(
                "anonymous enum needs a body", self.current_span(),
            ))?;
            return self.enums.get(&name).cloned().ok_or_else(|| {
                CompileError::new(format!("unknown enum `{name}`"), self.previous_span())
            });
        }

        self.advance(); // the opening brace
        // The counter starts at 0. An explicit value moves it, and the next
        // enumerator continues from there. `{ A = 5, B, C = 1, D }` is 5, 6, 1, 2.
        let mut next: i64 = 0;
        let mut count = 0usize;
        while self.current() != &Token::RBrace {
            let name_span = self.current_span();
            let name = self.expect_ident("an enumerator name")?;
            if self.current() == &Token::Assign {
                self.advance();
                let value = self.parse_ternary()?;
                match crate::constfold::eval_const(&value)? {
                    crate::constfold::ConstValue::Int(n) => next = n,
                    crate::constfold::ConstValue::Bytes(_) => {
                        return Err(CompileError::new(
                            "an enumerator needs an integer constant", value.span,
                        ));
                    }
                }
            }
            if self.enum_constants.insert(name.clone(), next).is_some() {
                return Err(CompileError::new(
                    format!("enumerator `{name}` is already defined"), name_span,
                ));
            }
            next += 1;
            count += 1;
            if self.current() != &Token::Comma {
                break;
            }
            self.advance(); // a trailing comma before the closing brace is allowed
        }
        self.expect(&Token::RBrace)?;

        if count == 0 {
            return Err(CompileError::new("an empty enum is not supported", self.previous_span()));
        }

        let ty = Type::Enum { tag: tag.clone() };
        if let Some(name) = tag {
            self.enums.insert(name, ty.clone());
        }
        Ok(ty)
    }

    /// Parse a `struct` type reference or definition. Current token is `Token::Struct`.
    fn parse_struct_type(&mut self) -> Result<Type, CompileError> {
        self.advance(); // 'struct'
        let tag = match self.current().clone() {
            Token::Ident(name) => { self.advance(); Some(name) }
            _ => None,
        };

        if self.current() != &Token::LBrace {
            // A bare reference: `struct Tag`. Must already be defined.
            let name = tag.ok_or_else(|| CompileError::new(
                "anonymous struct needs a body", self.current_span(),
            ))?;
            return self.structs.get(&name).cloned().ok_or_else(|| {
                CompileError::new(format!("unknown struct `{name}`"), self.previous_span())
            });
        }

        self.advance(); // '{'
        let mut members: Vec<(Type, String)> = Vec::new();
        while self.current() != &Token::RBrace {
            // One line of members shares a base type but each name carries its
            // own `*`s and `[N]`: `int a, *p, r[4];`.
            let base = self.parse_base_type()?;
            loop {
                let mty = self.parse_pointer_suffix(base.clone());
                let mname = self.expect_ident("a member name")?;
                let (mty, _) = self.parse_array_suffix(mty, false)?;
                members.push((mty, mname));
                if self.current() != &Token::Comma {
                    break;
                }
                self.advance(); // ','
            }
            self.expect(&Token::Semicolon)?;
        }
        self.expect(&Token::RBrace)?;

        if members.is_empty() {
            return Err(CompileError::new("an empty struct is not supported", self.previous_span()));
        }

        let (fields, size, align) = Self::layout_struct(&members);
        let ty = Type::Struct { tag: tag.clone(), fields, size, align };
        if let Some(name) = tag {
            self.structs.insert(name, ty.clone());
        }
        Ok(ty)
    }

}



/* ===================================== */
//                                       //
//        Unit tests for the parser      //
//                                       // 
/* ===================================== */

#[cfg(test)]
mod tests {
    use super::*;
    
    // Attach dummy spans so tests can keep writing bare Token vectors.
    fn parser(tokens: Vec<Token>) -> Parser {
        use crate::lexer::SpannedToken;
        use crate::diagnostic::Span;
        let spanned = tokens.into_iter().map(|t| SpannedToken { token: t, span: Span::dummy() }).collect();
        Parser::new(spanned)
    }

    use crate::diagnostic::{Span, Spanned};
    fn e(x: Expr) -> TypedExpr { TypedExpr::new(x, Span::dummy()) }
    fn s(x: Stmt) -> Spanned<Stmt> { Spanned::new(x, Span::dummy()) }

    /// Lex and parse real C text. Prototypes are easier to read as source than
    /// as a hand-written token vector.
    fn parse_src(src: &str) -> Result<Program, CompileError> {
        let tokens = crate::lexer::Lexer::new(src).tokenize().unwrap();
        Parser::new(tokens).parse_program()
    }

    #[test]
    fn a_prototype_parses_without_a_body() {
        let p = parse_src("int printf(const char *fmt, ...);\nint main() { return 0; }").unwrap();
        assert_eq!(p.decls.len(), 1);
        assert_eq!(p.functions.len(), 1);
        assert_eq!(p.decls[0].name, "printf");
        assert!(p.decls[0].variadic);
        assert_eq!(p.decls[0].params.len(), 1);
    }

    #[test]
    fn a_void_parameter_list_means_no_parameters() {
        let p = parse_src("int getchar(void);\nint main() { return 0; }").unwrap();
        assert!(p.decls[0].params.is_empty());
    }

    #[test]
    fn a_void_return_type_is_not_confused_with_a_void_parameter_list() {
        let p = parse_src("void exit(int code);\nint main() { return 0; }").unwrap();
        assert_eq!(p.decls[0].return_type, Type::Void);
        assert_eq!(p.decls[0].params.len(), 1);
    }

    #[test]
    fn const_is_parsed_and_dropped() {
        let p = parse_src("long strlen(const char *s);\nint main() { return 0; }").unwrap();
        assert_eq!(p.decls[0].params[0].0, Type::Pointer(Box::new(Type::Char)));
    }

    #[test]
    fn a_parameter_may_be_unnamed() {
        let p = parse_src("int strcmp(const char *, const char *);\nint main() { return 0; }")
            .unwrap();
        assert_eq!(p.decls[0].params.len(), 2);
    }

    #[test]
    fn a_pointer_return_type_survives() {
        let p = parse_src("void *malloc(long size);\nint main() { return 0; }").unwrap();
        assert_eq!(p.decls[0].return_type, Type::Pointer(Box::new(Type::Void)));
    }

    #[test]
    fn an_enumerator_counts_up_and_an_explicit_value_resets_the_counter() {
        // A = 5, B = 6, C = 1, D = 2.
        let p = parse_src(
            "enum E { A = 5, B, C = 1, D };\n\
             int w = A; int x = B; int y = C; int z = D;\n\
             int main() { return 0; }",
        )
        .unwrap();
        let values: Vec<i64> = p
            .globals
            .iter()
            .map(|g| match &g.init {
                Some(TypedExpr { node: Expr::IntLiteral(n), .. }) => *n,
                other => panic!("`{}` did not fold to a constant: {other:?}", g.name),
            })
            .collect();
        assert_eq!(values, vec![5, 6, 1, 2]);
    }

    #[test]
    fn an_enumerator_use_becomes_a_literal_not_a_variable() {
        // The parser substitutes the value, so no later stage sees the name.
        // This is why the code generator needs no enum support.
        let p = parse_src("enum { HI = 9 };\nint main() { return HI; }").unwrap();
        match &p.functions[0].body[0].node {
            Stmt::Return(value) => assert_eq!(value.node, Expr::IntLiteral(9)),
            other => panic!("expected a return, got {other:?}"),
        }
    }

    #[test]
    fn a_definition_still_parses_as_a_function() {
        let p = parse_src("int add(int a, int b) { return a + b; }").unwrap();
        assert_eq!(p.functions.len(), 1);
        assert!(p.decls.is_empty());
    }

    #[test]
    fn ellipsis_must_come_last() {
        let err = parse_src("int f(..., int a);\nint main() { return 0; }").unwrap_err();
        assert!(err.message.contains("last"), "got: {}", err.message);
    }

    #[test]
    fn a_variadic_function_cannot_be_defined_here() {
        // va_arg does not exist, so a body would compile to something that
        // cannot read its own arguments.
        let err = parse_src("int f(int a, ...) { return a; }").unwrap_err();
        assert!(err.message.contains("variadic"), "got: {}", err.message);
    }

    #[test]
    fn const_is_accepted_on_a_local_declaration() {
        let p = parse_src("int main() { const int x = 5; return x; }").unwrap();
        assert_eq!(p.functions[0].body.len(), 2);
    }

    #[test]
    fn parse_unary_negation_expression() {
        let mut p = parser(vec![Token::Minus, Token::IntLiteral(7), Token::EOF]);
        let expr = p.parse_unary().unwrap();
        assert_eq!(expr, e(Expr::UnaryOp(UnaryOp::Negate, Box::new(e(Expr::IntLiteral(7))))));
    }

    #[test]
    fn parse_parenthesized_primary_expression() {
        let mut p = parser(vec![Token::LParen, Token::IntLiteral(9), Token::RParen, Token::EOF]);
        let expr = p.parse_primary().unwrap();
        assert_eq!(expr, e(Expr::IntLiteral(9)));
    }

    #[test]
    fn parse_expression_statement() {
        let mut p = parser(vec![Token::IntLiteral(7), Token::Semicolon, Token::EOF]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::IntLiteral(7)))));
    }

    #[test]
    fn parse_var_decl_with_init() {
        let mut p = parser(vec![
            Token::Int, Token::Ident("x".into()), Token::Assign,
            Token::IntLiteral(5), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { ty: Type::Int, name: "x".into(), init: Some(e(Expr::IntLiteral(5))) }));
    }

    #[test]
    fn parse_var_decl_without_init() {
        let mut p = parser(vec![
            Token::Int, Token::Ident("x".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { ty: Type::Int, name: "x".into(), init: None }));
    }

    #[test]
    fn parse_assignment() {
        let mut p = parser(vec![
            Token::Ident("x".into()), Token::Assign,
            Token::IntLiteral(10), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(Box::new(e(Expr::Var("x".into()))), Box::new(e(Expr::IntLiteral(10))))))));
    }

    #[test]
    fn parse_compound_assignment() {
        let mut p = parser(vec![
            Token::Ident("x".into()), Token::PlusEquals,
            Token::IntLiteral(3), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(
            Box::new(e(Expr::Var("x".into()))),
            Box::new(e(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(e(Expr::Var("x".into()))),
                Box::new(e(Expr::IntLiteral(3))),
            ))),
        )))));
    }

    #[test]
    fn parse_post_increment() {
        // Not desugared to `i = i + 1`: that form evaluates to the *new* value,
        // so `x = i++` would be off by one. See `Expr::PostIncDec`.
        let mut p = parser(vec![
            Token::Ident("i".into()), Token::PlusPlus, Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::PostIncDec(
            IncDec::Inc,
            Box::new(e(Expr::Var("i".into()))),
        )))));
    }

    #[test]
    fn parse_post_decrement() {
        let mut p = parser(vec![
            Token::Ident("i".into()), Token::MinusMinus, Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::PostIncDec(
            IncDec::Dec,
            Box::new(e(Expr::Var("i".into()))),
        )))));
    }

    #[test]
    fn parse_comparison_less_than() {
        let mut p = parser(vec![
            Token::Ident("i".into()), Token::LessThan, Token::IntLiteral(10), Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::BinaryOp(
            BinaryOp::Lt,
            Box::new(e(Expr::Var("i".into()))),
            Box::new(e(Expr::IntLiteral(10))),
        )));
    }

    #[test]
    fn parse_comparison_binds_looser_than_additive() {
        let mut p = parser(vec![
            Token::Ident("i".into()), Token::LessThan,
            Token::IntLiteral(3), Token::Plus, Token::IntLiteral(1), Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::BinaryOp(
            BinaryOp::Lt,
            Box::new(e(Expr::Var("i".into()))),
            Box::new(e(Expr::BinaryOp(
                BinaryOp::Add,
                Box::new(e(Expr::IntLiteral(3))),
                Box::new(e(Expr::IntLiteral(1))),
            ))),
        )));
    }

        #[test]
    fn parse_char_var_decl() {
        let mut p = parser(vec![
            Token::Char, Token::Ident("c".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl { ty: Type::Char, name: "c".into(), init: None }));
    }

    #[test]
    fn parse_long_var_decl_with_init() {
        let mut p = parser(vec![
            Token::Long, Token::Ident("n".into()), Token::Assign,
            Token::IntLiteral(7), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Long, name: "n".into(), init: Some(e(Expr::IntLiteral(7))),
        }));
    }

    #[test]
    fn parse_pointer_var_decl() {
        let mut p = parser(vec![
            Token::Int, Token::Star, Token::Ident("p".into()), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Pointer(Box::new(Type::Int)), name: "p".into(), init: None,
        }));
    }

    #[test]
    fn parse_array_var_decl() {
        let mut p = parser(vec![
            Token::Int, Token::Ident("a".into()), Token::LBracket,
            Token::IntLiteral(10), Token::RBracket, Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Array(Box::new(Type::Int), 10), name: "a".into(), init: None,
        }));
    }

    #[test]
    fn parse_flat_initializer_list() {
        // int a[3] = {1, 2, 3};
        let mut p = parser(vec![
            Token::Int, Token::Ident("a".into()),
            Token::LBracket, Token::IntLiteral(3), Token::RBracket,
            Token::Assign,
            Token::LBrace,
            Token::IntLiteral(1), Token::Comma,
            Token::IntLiteral(2), Token::Comma,
            Token::IntLiteral(3),
            Token::RBrace, Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Array(Box::new(Type::Int), 3),
            name: "a".into(),
            init: Some(e(Expr::InitList(vec![
                e(Expr::IntLiteral(1)),
                e(Expr::IntLiteral(2)),
                e(Expr::IntLiteral(3)),
            ]))),
        }));
    }

    #[test]
    fn parse_nested_initializer_list_with_trailing_comma() {
        // int a[2][2] = {{1, 2}, {3, 4},};
        let mut p = parser(vec![
            Token::Int, Token::Ident("a".into()),
            Token::LBracket, Token::IntLiteral(2), Token::RBracket,
            Token::LBracket, Token::IntLiteral(2), Token::RBracket,
            Token::Assign,
            Token::LBrace,
            Token::LBrace, Token::IntLiteral(1), Token::Comma, Token::IntLiteral(2), Token::RBrace,
            Token::Comma,
            Token::LBrace, Token::IntLiteral(3), Token::Comma, Token::IntLiteral(4), Token::RBrace,
            Token::Comma,
            Token::RBrace, Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Array(Box::new(Type::Array(Box::new(Type::Int), 2)), 2),
            name: "a".into(),
            init: Some(e(Expr::InitList(vec![
                e(Expr::InitList(vec![e(Expr::IntLiteral(1)), e(Expr::IntLiteral(2))])),
                e(Expr::InitList(vec![e(Expr::IntLiteral(3)), e(Expr::IntLiteral(4))])),
            ]))),
        }));
    }

    /// A global brace initializer used to be a hard parser error.
    #[test]
    fn parse_global_initializer_list() {
        let program = parse_src("int g[2] = {1, 2};").unwrap();
        match &program.globals[0].init {
            Some(init) => match &init.node {
                Expr::InitList(elems) => assert_eq!(elems.len(), 2),
                other => panic!("expected InitList, got {other:?}"),
            },
            None => panic!("expected an initializer"),
        }
    }

    #[test]
    fn parse_two_dimensional_array_decl() {
        // int a[3][4]; is "array 3 of (array 4 of int)".
        let mut p = parser(vec![
            Token::Int, Token::Ident("a".into()),
            Token::LBracket, Token::IntLiteral(3), Token::RBracket,
            Token::LBracket, Token::IntLiteral(4), Token::RBracket,
            Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::VarDecl {
            ty: Type::Array(Box::new(Type::Array(Box::new(Type::Int), 4)), 3),
            name: "a".into(),
            init: None,
        }));
    }

        #[test]
    fn parse_address_of() {
        let mut p = parser(vec![Token::Ampersand, Token::Ident("x".into()), Token::EOF]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::AddressOf(Box::new(e(Expr::Var("x".into()))))));
    }

    #[test]
    fn parse_deref() {
        let mut p = parser(vec![Token::Star, Token::Ident("p".into()), Token::EOF]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::Deref(Box::new(e(Expr::Var("p".into()))))));
    }

    #[test]
    fn parse_index() {
        let mut p = parser(vec![
            Token::Ident("a".into()), Token::LBracket, Token::IntLiteral(2), Token::RBracket, Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::Index(
            Box::new(e(Expr::Var("a".into()))),
            Box::new(e(Expr::IntLiteral(2))),
        )));
    }

    #[test]
    fn parse_cast() {
        let mut p = parser(vec![
            Token::LParen, Token::Char, Token::Star, Token::RParen, Token::Ident("p".into()), Token::EOF,
        ]);
        let expr = p.parse_expr().unwrap();
        assert_eq!(expr, e(Expr::Cast(
            Type::Pointer(Box::new(Type::Char)),
            Box::new(e(Expr::Var("p".into()))),
        )));
    }

    #[test]
    fn parse_deref_assignment() {
        let mut p = parser(vec![
            Token::Star, Token::Ident("p".into()), Token::Assign,
            Token::IntLiteral(5), Token::Semicolon, Token::EOF,
        ]);
        let stmt = p.parse_statement().unwrap();
        assert_eq!(stmt, s(Stmt::Expr(e(Expr::Assign(
            Box::new(e(Expr::Deref(Box::new(e(Expr::Var("p".into())))))),
            Box::new(e(Expr::IntLiteral(5))),
        )))));
    }

    #[test]
    fn a_scalar_global_with_initializer_parses() {
        let p = parse_src("int counter = 5;\nint main() { return 0; }").unwrap();
        assert_eq!(p.globals.len(), 1);
        assert_eq!(p.globals[0].name, "counter");
        assert_eq!(p.globals[0].ty, Type::Int);
        match &p.globals[0].init {
            Some(e) => assert_eq!(e.node, Expr::IntLiteral(5)),
            None => panic!("expected an initializer"),
        }
    }

    #[test]
    fn a_global_without_initializer_parses() {
        let p = parse_src("long n;\nint main() { return 0; }").unwrap();
        assert_eq!(p.globals[0].ty, Type::Long);
        assert!(p.globals[0].init.is_none());
    }

    #[test]
    fn a_sized_global_array_parses() {
        let p = parse_src("char buf[100];\nint main() { return 0; }").unwrap();
        assert_eq!(p.globals[0].ty, Type::Array(Box::new(Type::Char), 100));
        assert!(p.globals[0].init.is_none());
    }

    #[test]
    fn an_unsized_char_array_with_a_string_parses() {
        let p = parse_src("char s[] = \"hi\";\nint main() { return 0; }").unwrap();
        assert_eq!(p.globals[0].ty, Type::Array(Box::new(Type::Char), 0));
        assert!(matches!(p.globals[0].init, Some(_)));
    }

    #[test]
    fn a_function_after_a_global_still_parses() {
        let p = parse_src("int g = 1;\nint main() { return g; }").unwrap();
        assert_eq!(p.globals.len(), 1);
        assert_eq!(p.functions.len(), 1);
    }

    #[test]
    fn an_unsized_global_array_without_initializer_is_rejected() {
        let err = parse_src("int a[];\nint main() { return 0; }").unwrap_err();
        assert!(err.message.contains("array size"), "got: {}", err.message);
    }

    // --- declarator lists: `int a, b;` ---

    /// The seam the whole declarator-list feature rests on: the specifiers stop
    /// at the `*`, so each declarator can add its own pointer depth.
    #[test]
    fn a_base_type_stops_at_the_star() {
        let mut p = parser(vec![
            Token::Int, Token::Star, Token::Ident("p".into()), Token::Semicolon, Token::EOF,
        ]);
        assert_eq!(p.parse_base_type().unwrap(), Type::Int);
        assert_eq!(p.current(), &Token::Star);
    }

    #[test]
    fn struct_members_may_share_one_base_type() {
        let p = parse_src("struct V { int f, t, p; } v;").unwrap();
        match &p.globals[0].ty {
            Type::Struct { fields, size, .. } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(
                    fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
                    ["f", "t", "p"],
                );
                assert_eq!(
                    fields.iter().map(|f| f.offset).collect::<Vec<_>>(),
                    [0, 4, 8],
                );
                assert_eq!(*size, 12);
            }
            other => panic!("got {other:?}"),
        }
    }

    /// `char c, *p, n;` gives one pointer and two chars, not three pointers.
    #[test]
    fn a_star_binds_to_its_own_struct_member() {
        let p = parse_src("struct S { char c, *p, n; } s;").unwrap();
        match &p.globals[0].ty {
            Type::Struct { fields, .. } => {
                assert_eq!(fields[0].ty, Type::Char);
                assert_eq!(fields[1].ty, Type::Pointer(Box::new(Type::Char)));
                assert_eq!(fields[2].ty, Type::Char);
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_struct_member_list_mixes_arrays_and_scalars() {
        let p = parse_src("struct L { int m[4], c; } l;").unwrap();
        match &p.globals[0].ty {
            Type::Struct { fields, .. } => {
                assert_eq!(fields[0].ty, Type::Array(Box::new(Type::Int), 4));
                assert_eq!(fields[1].ty, Type::Int);
                assert_eq!(fields[1].offset, 16);
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_global_declaration_may_name_several_variables() {
        let p = parse_src("int a, b = 5;\nint main() { return 0; }").unwrap();
        assert_eq!(p.globals.len(), 2);
        assert_eq!(p.globals[0].name, "a");
        assert!(p.globals[0].init.is_none());
        assert_eq!(p.globals[1].name, "b");
        assert!(p.globals[1].init.is_some());
    }

    #[test]
    fn a_star_binds_to_its_own_global_declarator() {
        let p = parse_src("int *a, b;\nint main() { return 0; }").unwrap();
        assert_eq!(p.globals[0].ty, Type::Pointer(Box::new(Type::Int)));
        assert_eq!(p.globals[1].ty, Type::Int);
    }

    /// The unsized-array check is per declarator, so only `b` is at fault.
    #[test]
    fn each_global_declarator_needs_its_own_array_size() {
        let err = parse_src("char a[] = \"hi\", b[];\nint main() { return 0; }").unwrap_err();
        assert!(err.message.contains("array size"), "got: {}", err.message);
    }

    #[test]
    fn a_local_declaration_may_name_several_variables() {
        let p = parse_src("int main() { int i = 0, n; return i; }").unwrap();
        let body = &p.functions[0].body;
        assert!(
            matches!(&body[0].node, Stmt::VarDecl { name, init: Some(_), .. } if name == "i"),
            "got {:?}", body[0].node,
        );
        assert!(
            matches!(&body[1].node, Stmt::VarDecl { name, init: None, .. } if name == "n"),
            "got {:?}", body[1].node,
        );
        assert!(matches!(body[2].node, Stmt::Return(_)));
    }

    #[test]
    fn a_star_binds_to_its_own_local_declarator() {
        let p = parse_src("int main() { int *a, b; return b; }").unwrap();
        match &p.functions[0].body[0].node {
            Stmt::VarDecl { ty, .. } => assert_eq!(*ty, Type::Pointer(Box::new(Type::Int))),
            other => panic!("got {other:?}"),
        }
        match &p.functions[0].body[1].node {
            Stmt::VarDecl { ty, .. } => assert_eq!(*ty, Type::Int),
            other => panic!("got {other:?}"),
        }
    }

    /// `Stmt::For` holds a single init statement, so a list has nowhere to go.
    #[test]
    fn a_for_initializer_rejects_several_names() {
        let err = parse_src("int main() { for (int i = 0, n = 5; i < n; i++) { } return 0; }")
            .unwrap_err();
        assert!(err.message.contains("several names"), "got: {}", err.message);
    }

    #[test]
    fn parse_struct_definition_and_variable() {
        // struct Point { int x; int y; } p;
        let src = "struct Point { int x; int y; } p;";
        let tokens = crate::lexer::Lexer::new(src).tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        assert_eq!(program.globals.len(), 1);
        match &program.globals[0].ty {
            Type::Struct { tag, fields, size, align } => {
                assert_eq!(tag.as_deref(), Some("Point"));
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].offset, 0);
                assert_eq!(fields[1].offset, 4);
                assert_eq!(*size, 8);
                assert_eq!(*align, 4);
            }
            other => panic!("expected Type::Struct, got {other:?}"),
        }
    }

    #[test]
    fn parse_struct_layout_pads_for_alignment() {
        // struct S { char c; int n; }  -> c@0, n@4, size 8, align 4
        let src = "struct S { char c; int n; } s;";
        let tokens = crate::lexer::Lexer::new(src).tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        match &program.globals[0].ty {
            Type::Struct { fields, size, align, .. } => {
                assert_eq!(fields[0].offset, 0);
                assert_eq!(fields[1].offset, 4);
                assert_eq!(*size, 8);
                assert_eq!(*align, 4);
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parse_member_and_arrow_in_postfix() {
        // a.b and p->c
        let dot = {
            let toks = crate::lexer::Lexer::new("a.b;").tokenize().unwrap();
            Parser::new(toks).parse_statement().unwrap()
        };
        match dot.node {
            Stmt::Expr(e) => match e.node {
                Expr::Member(base, f) => {
                    assert!(matches!(base.node, Expr::Var(ref n) if n == "a"));
                    assert_eq!(f, "b");
                }
                other => panic!("got {other:?}"),
            },
            other => panic!("got {other:?}"),
        }

        let arrow = {
            let toks = crate::lexer::Lexer::new("p->c;").tokenize().unwrap();
            Parser::new(toks).parse_statement().unwrap()
        };
        match arrow.node {
            Stmt::Expr(e) => match e.node {
                Expr::Member(base, f) => {
                    assert!(matches!(base.node, Expr::Deref(_)));
                    assert_eq!(f, "c");
                }
                other => panic!("got {other:?}"),
            },
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_typedef_list_binds_each_name_separately() {
        // `A` is `int *`, `B` is `int`, `R` is `int[4]`.
        let p = parse_src(
            "typedef int *A, B, R[4];\nA a; B b; R r;\nint main() { return 0; }",
        )
        .unwrap();
        assert_eq!(p.globals[0].ty, Type::Pointer(Box::new(Type::Int)));
        assert_eq!(p.globals[1].ty, Type::Int);
        assert_eq!(p.globals[2].ty, Type::Array(Box::new(Type::Int), 4));
    }

    #[test]
    fn parse_typedef_anonymous_struct() {
        // typedef struct { int x; } Wrap;  then  Wrap w;
        let src = "typedef struct { int x; } Wrap; Wrap w;";
        let tokens = crate::lexer::Lexer::new(src).tokenize().unwrap();
        let program = Parser::new(tokens).parse_program().unwrap();
        match &program.globals[0].ty {
            Type::Struct { tag, fields, .. } => {
                assert!(tag.is_none());
                assert_eq!(fields[0].name, "x");
            }
            other => panic!("got {other:?}"),
        }
    }

    // Helper: parse a bare expression from a statement.
    fn expr_of(src: &str) -> Expr {
        let toks = crate::lexer::Lexer::new(src).tokenize().unwrap();
        match Parser::new(toks).parse_statement().unwrap().node {
            Stmt::Expr(e) => e.node,
            other => panic!("expected Stmt::Expr, got {other:?}"),
        }
    }

    #[test]
    fn bitwise_and_binds_tighter_than_xor_and_or() {
        // a | b ^ c & d  ==  a | (b ^ (c & d))
        match expr_of("a | b ^ c & d;") {
            Expr::BinaryOp(BinaryOp::BitOr, _, rhs) => match rhs.node {
                Expr::BinaryOp(BinaryOp::BitXor, _, r2) => {
                    assert!(matches!(r2.node, Expr::BinaryOp(BinaryOp::BitAnd, _, _)));
                }
                other => panic!("expected BitXor, got {other:?}"),
            },
            other => panic!("expected BitOr at the root, got {other:?}"),
        }
    }

    #[test]
    fn equality_binds_tighter_than_bitwise_and() {
        // a & b == c  ==  a & (b == c)
        match expr_of("a & b == c;") {
            Expr::BinaryOp(BinaryOp::BitAnd, _, rhs) => {
                assert!(matches!(rhs.node, Expr::BinaryOp(BinaryOp::Eq, _, _)));
            }
            other => panic!("expected BitAnd at the root, got {other:?}"),
        }
    }

    #[test]
    fn shift_binds_between_relational_and_additive() {
        // 1 + 2 << 3  ==  (1 + 2) << 3
        match expr_of("1 + 2 << 3;") {
            Expr::BinaryOp(BinaryOp::Shl, lhs, _) => {
                assert!(matches!(lhs.node, Expr::BinaryOp(BinaryOp::Add, _, _)));
            }
            other => panic!("expected Shl at the root, got {other:?}"),
        }
        // 1 < 2 << 3  ==  1 < (2 << 3)
        match expr_of("1 < 2 << 3;") {
            Expr::BinaryOp(BinaryOp::Lt, _, rhs) => {
                assert!(matches!(rhs.node, Expr::BinaryOp(BinaryOp::Shl, _, _)));
            }
            other => panic!("expected Lt at the root, got {other:?}"),
        }
    }

    #[test]
    fn bitwise_is_left_associative() {
        // a ^ b ^ c  ==  (a ^ b) ^ c
        match expr_of("a ^ b ^ c;") {
            Expr::BinaryOp(BinaryOp::BitXor, lhs, _) => {
                assert!(matches!(lhs.node, Expr::BinaryOp(BinaryOp::BitXor, _, _)));
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn parse_simple_ternary() {
        match expr_of("a ? b : c;") {
            Expr::Ternary(c, t, e) => {
                assert!(matches!(c.node, Expr::Var(ref n) if n == "a"));
                assert!(matches!(t.node, Expr::Var(ref n) if n == "b"));
                assert!(matches!(e.node, Expr::Var(ref n) if n == "c"));
            }
            other => panic!("expected Ternary, got {other:?}"),
        }
    }

    #[test]
    fn ternary_is_right_associative() {
        // a ? b : c ? d : e  ==  a ? b : (c ? d : e)
        match expr_of("a ? b : c ? d : e;") {
            Expr::Ternary(_, _, els) => {
                assert!(matches!(els.node, Expr::Ternary(_, _, _)));
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn ternary_binds_looser_than_logical_or() {
        // a || b ? c : d  ==  (a || b) ? c : d
        match expr_of("a || b ? c : d;") {
            Expr::Ternary(c, _, _) => {
                assert!(matches!(c.node, Expr::BinaryOp(BinaryOp::LogicalOr, _, _)));
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn compound_bitwise_assignment_desugars() {
        // x |= 1  ==  x = x | 1
        match expr_of("x |= 1;") {
            Expr::Assign(lhs, value) => {
                assert!(matches!(lhs.node, Expr::Var(ref n) if n == "x"));
                match value.node {
                    Expr::BinaryOp(BinaryOp::BitOr, l, r) => {
                        assert!(matches!(l.node, Expr::Var(ref n) if n == "x"));
                        assert!(matches!(r.node, Expr::IntLiteral(1)));
                    }
                    other => panic!("expected BitOr value, got {other:?}"),
                }
            }
            other => panic!("expected Assign, got {other:?}"),
        }
    }



}
