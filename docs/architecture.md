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
 Preprocessor   →  resolves directives, expands macros; drives the lexer
   │                (reads other files into a SourceMap)
   ▼
 tokens (each token has a span tagged with the file it came from)
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

## Stage 1 — Preprocessor

**Directory:** `src/preprocessor/`

The preprocessor owns the read loop. It reads one logical line at a time. It
handles a line that starts with `#` as a directive. It gives every other line to
the lexer. Its output is the token stream that the parser reads.

The lexer is **not** a stage of its own. It is a service that the preprocessor
calls, one line at a time. This order is important. It lets a dead `#if` branch
hold text that does not lex, because the preprocessor skips the line first.

| File | Responsibility |
|---|---|
| `mod.rs` | The line loop, the directives, the conditional stack, the include stack |
| `normalize.rs` | Changes comments and `\`-newline pairs to spaces |
| `macros.rs` | The macro table and parameter substitution |
| `include.rs` | The `#include` search path and the bundled headers |
| `cond_expr.rs` | The `#if` constant-expression evaluator |

The `normalize` function keeps the length of the text. A comment becomes spaces.
It does not disappear. An offset into the new buffer is therefore also an offset
into the original file. This is why a span from a header points at the correct
text, and why the compiler needs no offset arithmetic.

An `#include` pushes a new file onto a stack. The line loop always reads from the
top of the stack. It pops a file when that file ends. Nesting needs no special
case.

### The lexer

**File:** `src/lexer.rs`

The lexer turns source text into tokens. A token is one word or one symbol of the
language. A token is a keyword, an identifier, a number, a string, or an operator.

Each token has a span. A span records the start position, the end position, and
the **file** of the token. The `Span.file` field indexes a `SourceMap`. The
`SourceMap` holds every file the compiler has read. The compiler uses the span
later to show the location of an error in the correct file.

The `Lexer::for_region(text, file, start, end)` method reads one window of a file.
It writes that file id into each span. The `position` field stays an absolute
index into the whole file, so each span is already in file coordinates. The
`tokenize_region` method does not add a `Token::EOF`. The preprocessor adds one
`EOF` at the end of the last file.

Two notes:

- The lexer emits `Token::Assign` for `=` and `Token::Equals` for `==`. These are
  different tokens. The parser needs the difference to tell an assignment from an
  equality test.
- A `#` that reaches the lexer is an error. The preprocessor consumes every
  directive line first. A `#` here is a stray one in the middle of a line, or a
  preprocessor bug.

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

Sizes are the real C widths: `char` is 1 byte, `int` is 4, and `long`, pointer,
and `void` are 8. An array is its element size times its length. Because these two
methods are the single source of truth, the code generator scales pointer arithmetic
and array indexing, sizes each stack slot, and picks the load and store width all
from the same numbers.

## Stage 4 — Code generator

**File:** `src/codegen.rs`

The code generator walks the typed AST. It emits Intel x86-64 assembly as text.
The code generator reads the `ty` field of each expression. It uses the type to
scale pointer arithmetic and to decay an array to a pointer.

The code generator follows these rules:

- A result always goes into `rax`.
- The stack pointer `rsp` does not move after the prologue. To keep an
  intermediate value, the generator saves it to a frame slot with `spill_rax`. It
  does not use `push`.
- For a binary operation, the generator evaluates the left side first and saves
  the result to a frame slot. It evaluates the right side into `rax`. It moves the
  right side to `rcx` and loads the left side back into `rax`. Then it emits the
  operation.
- The generator stores a variable on the stack. It uses a negative offset from
  `rbp`. The `variables` map holds the offset for each name.
- The generator uses numbered labels for control flow, for example `loop_0_start`
  and `if_0_end`.

The rule about `push` is a correctness rule, not a style rule. A callee measures
its 32 bytes of shadow space from `rsp`. A `push` moves `rsp` down by 8, so the
next call writes over the value that the `push` saved. A `push` also breaks the
16-byte stack alignment that Win64 requires at a `call`.

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

  Note: the import table covers one DLL. The `build_import_section` function in
  `assembler/mod.rs` uses the fixed name `msvcrt.dll`. A libc call such as
  `printf` therefore runs through the default backend with no external linker. A
  call into `kernel32`, `user32`, or the UCRT does not resolve. Use `--lld-link`
  for a program that imports from more than one DLL.
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
| `src/preprocessor/` | Owns the read loop. Handles directives, macros, and `#include`. Calls the lexer. |
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