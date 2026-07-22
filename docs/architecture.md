# VBRCC Architecture

This document describes how VBRCC compiles a C source file into an executable.
It describes each stage of the pipeline, the data that moves between the stages,
and the main source files. Read this document to learn how the parts fit together
before you change the code.

For building the project, backend prerequisites, running the tests, and debugging
techniques, see [CONTRIBUTING.md](CONTRIBUTING.md). For per-module API documentation,
see [docs.rs/vbrcc](https://docs.rs/vbrcc) or run `cargo doc --no-deps --open`.

VBRCC uses no external compiler libraries. The lexer, the parser, the type
checker, the code generator, and the assembler are all hand-written in Rust.

## The pipeline

VBRCC processes a C file in five stages. Each stage gives its output to the next
stage. `src/main.rs` runs the stages in order.

```
C source
   │
   ▼
 Lexer          →  tokens (each token has a span)
   │
   ▼
 Parser         →  AST (a Program of functions)
   │
   ▼
 Type checker   →  the same AST, with a type on each expression
   │
   ▼
 Code generator →  Intel x86-64 assembly text
   │
   ▼
 Assembler      →  machine bytes, then a PE executable or a COFF object
```

If a stage finds an error, the compiler prints a diagnostic and stops. The
compiler stops at the first error. It does not report more than one error.

## Stage 1 — Lexer

**File:** `src/lexer.rs`

The lexer reads the source text. It makes a list of tokens. A token is one word
or one symbol of the language. A token is a keyword, an identifier, a number, a
string, or an operator.

Each token has a span. A span records the start position and the end position of
the token in the source text. The compiler uses the span later. It uses the span
to show the location of an error.

The `tokenize` method returns a `Vec<SpannedToken>`. A `SpannedToken` holds a
`Token` value and its `Span`. The `Token` enum lists all token kinds. The
`tokenize` method returns a `Result`. If the lexer finds an unknown character, it
returns a `CompileError` with the position of the character.

Note: the lexer emits `Token::Assign` for `=` and `Token::Equals` for `==`. These
are different tokens. The parser needs this difference to tell an assignment from
an equality test.

## Stage 2 — Parser

**File:** `src/parser.rs`

The parser reads the tokens. It builds an Abstract Syntax Tree (AST). The AST is a
tree of Rust enums. Each node is one construct of the language.

The parser is a recursive-descent parser. It uses one method for each grammar
rule. For expressions, it uses precedence climbing. Each method calls the method
for the next-higher precedence level.

The parser produces these top-level types (see `src/ast.rs`):

- `Program` — a list of functions.
- `Function` — a name, typed parameters, a return type, and a body of statements.
- `Stmt` — a statement, for example `Return`, `VarDecl`, `If`, `While`, or `For`.
- `Expr` — an expression, for example a literal, a variable, a binary operation,
  an assignment, an address-of, a dereference, an index, or a cast.

The parser wraps each statement in a `Spanned<Stmt>`. It wraps each expression in
a `TypedExpr`. A `TypedExpr` holds an `Expr`, a `Span`, and a `Type`. The parser
sets the type to `Type::Unknown`. The type checker sets the correct type later.

The parser does not check types. For example, the parser accepts any expression
on the left of `=`. The type checker rejects an invalid target later.

## Stage 3 — Type checker

**File:** `src/typeck.rs`

The type checker walks the AST. It gives a type to each expression. It writes the
type into the `ty` field of each `TypedExpr`.

The type checker also finds these errors:

- A variable that is not declared.
- A dereference of a value that is not a pointer.
- An index of a value that is not a pointer or an array.
- An assignment to a target that is not an lvalue. An lvalue is a variable, a
  dereference, or an index.

The type checker keeps a scope. The scope is a map from a name to a `Type`. The
scope is flat. The type checker does not use a separate scope for each block yet.

The `Type` enum holds the type kinds: `Int`, `Char`, `Long`, `Void`, a `Pointer`
to a type, and an `Array` of a type and a length. The `Type::size` method and the
`Type::align` method give the size and the alignment of a type. These two methods
are the single place that controls sizes. A later phase can change the sizes in
one place.

Note: VBRCC uses loose sizing at present. Every scalar type and every pointer is
8 bytes. An array is 8 bytes for each element. A later phase adds the true sizes
(`char` = 1, `int` = 4, `long` and pointer = 8).

## Stage 4 — Code generator

**File:** `src/codegen.rs`

The code generator walks the typed AST. It emits Intel x86-64 assembly as text.
The code generator reads the `ty` field of each expression. It uses the type to
scale pointer arithmetic and to decay an array to a pointer.

The code generator follows these rules:

- A result always goes into `rax`.
- For a binary operation, the generator evaluates the left side first. It pushes
  the result. It evaluates the right side. It pops the left side back. The left
  side goes to `rax`. The right side goes to `rcx`. Then the generator emits the
  operation.
- The generator stores a variable on the stack. It uses a negative offset from
  `rbp`. The `variables` map holds the offset for each name.
- The generator uses numbered labels for control flow, for example `loop_0_start`
  and `if_0_end`.

The generator has two expression methods:

- `gen_expr` computes the *value* of an expression into `rax`.
- `gen_lvalue_addr` computes the *address* of an lvalue into `rax`. The generator
  uses this method for `&x`, for a store through a pointer, and for an index.

The compiler stores the assembly text in a `.s` file. It also passes the text to
the assembler.

## Stage 5 — Assembler

**Directory:** `src/assembler/`

The assembler turns the assembly text into machine bytes. The assembler has two
layers:

1. **The text parser** (`instruction.rs`). It reads one line of Intel-syntax
   assembly. It returns an `Instruction` value or a directive.
2. **The encoder** (`encoder.rs`). It turns an `Instruction` value into raw bytes.
   It also gives the length of each instruction. The assembler needs the length to
   compute jump and call offsets.

The assembler supports two output formats:

- **A PE executable** (`pe.rs`, the default). The assembler writes a complete
  Windows PE32+ executable. The file has a DOS header, a COFF header, a section
  table, and an import table. The assembler resolves an external call, for example
  `printf`, through the Import Address Table.

  Note: the import path is not finished. A program with no external calls builds and
  runs correctly. A program that calls `printf` also builds, and the compiler reports
  success, but the resulting image fails to load (exit code 127). Use the `--lld-link`
  mode for such programs until this is fixed.
- **A COFF object** (`coff.rs`, for the `--lld-link` path). The assembler writes a
  relocatable object file. The file has a symbol table and relocations. The linker
  `lld-link` resolves the relocations.

`relocation.rs` holds the relocation types. `register.rs` holds the register
enums and helpers.

## The driver

**File:** `src/assembler_driver.rs`

The driver connects the assembler to the linker. It has three modes:

- **CustomPe** (default) — the assembler writes the PE executable directly. This
  mode needs no external tool.
- **LldLink** (`--lld-link`) — the assembler writes a COFF object. Then the driver
  calls `lld-link`. This mode needs LLVM and the Windows SDK.
- **Gcc** (`--gcc`) — the system `gcc` assembles and links the `.s` file. This
  mode needs MinGW-w64 GCC.

## Diagnostics

**File:** `src/diagnostic.rs`

Every stage returns a `Result`. On an error, a stage returns a `CompileError`. A
`CompileError` holds a message, a span, and an optional label.

The `render` function makes a rustc-style error frame. The frame shows the
message, the file name, the line and the column, the source line, and a caret
under the span. The frame uses color on a terminal.

Note: `Span` compares equal to any other `Span`. Do not use a `Span` as a map
key. In a test, check `err.span.start` and `err.span.end`. Do not use
`assert_eq!` on two spans.

## Key files

| File | Responsibility |
|---|---|
| `src/main.rs` | Parses the CLI flags. Runs the pipeline stages. |
| `src/lexer.rs` | Turns source text into tokens with spans. |
| `src/parser.rs` | Turns tokens into the AST. |
| `src/ast.rs` | Holds the AST enums and the `Type` enum. |
| `src/typeck.rs` | Gives a type to each expression. Finds type errors. |
| `src/codegen.rs` | Turns the AST into Intel x86-64 assembly text. |
| `src/assembler/instruction.rs` | Parses assembly text into `Instruction` values. |
| `src/assembler/encoder.rs` | Encodes `Instruction` values into bytes. |
| `src/assembler/pe.rs` | Writes a PE executable. |
| `src/assembler/coff.rs` | Writes a COFF object. |
| `src/assembler_driver.rs` | Selects the output mode. Calls the linker. |
| `src/diagnostic.rs` | Holds `CompileError`, `Span`, and `render`. |

## How to extend the compiler

### Add an assembler instruction

Do these steps in order:

1. Add a variant to the `Instruction` enum in `instruction.rs`.
2. Add a match arm in `parse_intel_line` in `instruction.rs`.
3. Add an arm in `encoded_len` and an arm in `encode` in `encoder.rs`.
4. Add a test. Run `cargo test --lib assembler`.

### Add a C operator

Do these steps in order:

1. Add a token to the `Token` enum in `lexer.rs`, if the operator needs one.
2. Add a variant to `BinaryOp` or `UnaryOp` in `ast.rs`.
3. Parse the operator at the correct precedence level in `parser.rs`.
4. Give the result a type in `typeck.rs`.
5. Emit the assembly for the operator in `codegen.rs`.

### Add a statement

Do these steps in order:

1. Add a variant to the `Stmt` enum in `ast.rs`.
2. Parse the statement in `parse_statement` in `parser.rs`.
3. Check the statement in `typeck.rs`.
4. Emit the assembly in `gen_statement` in `codegen.rs`.