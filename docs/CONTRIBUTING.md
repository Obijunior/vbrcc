# Contributing to VBRCC

Thank you for your interest in the project.

## Project status: issues are welcome, pull requests are not

VBRCC is a personal learning project. The internals change too fast for an outside
patch to be practical. This stays true until the compiler reaches C99 compliance at
**v1.0.0**. The type system, the calling convention in the code generator, and the
instruction set of the assembler all change between releases.

These are very welcome:

- **Bug reports**, and most of all miscompilations
- **Questions** about how any part of the compiler works
- **Ideas** and feature suggestions
- **Corrections** to the documentation

Please [open an issue](https://github.com/obijunior/vbrcc/issues).

**Pull requests are not accepted at this time.** This changes at v1.0.0. If you have
opened one already, thank you. Please make it an issue that describes the problem, and
I will read it.

## How to report a bug

The most useful report is a **miscompilation**. This is a program that VBRCC accepts and
then compiles to the wrong behaviour. A miscompilation is harder to find than a crash,
because the compiler reports success. You see the problem only when the program runs.

A good report has three parts:

1. **A minimal C file.** Remove lines until one more removal hides the bug.
2. **What you expected, and what happened.** Give the exit code, the printed output, or
   the crash.
3. **The backend you used**: the default, `--lld-link`, or `--gcc`.

The generated assembly helps. To keep it:

```sh
vbrcc bug.c --keep-artifacts     # writes bug.s next to the source
```

A comparison against GCC helps more. The difference between the two disassemblies
usually shows the bad instruction:

```sh
vbrcc bug.c -o mine
vbrcc bug.c --gcc -o theirs
objdump -d -M intel mine.exe   > mine.asm
objdump -d -M intel theirs.exe > theirs.asm
diff mine.asm theirs.asm
```

For a **crash**, or for a **valid program that the compiler rejects**, the diagnostic
and the source file are usually enough.

## How to build from source

```sh
git clone https://github.com/obijunior/vbrcc
cd vbrcc
cargo build
```

The compiler itself needs only a stable Rust toolchain.

### Backend prerequisites

The default backend needs nothing. VBRCC encodes the machine code and writes the PE
executable itself. The other two backends call an external program:

| Backend | Needs | Where to get it |
|---|---|---|
| *(default)* | nothing | — |
| `--lld-link` | `lld-link`, `llvm-dlltool`, Windows SDK | [LLVM releases](https://github.com/llvm/llvm-project/releases), or `winget install LLVM.LLVM` |
| `--gcc` | MinGW-w64 GCC | [MSYS2](https://www.msys2.org/), then `pacman -S mingw-w64-x86_64-gcc` |

VBRCC emits **Windows PE/COFF** only. It builds and runs on Linux and macOS, but the
executables it writes do not run there.

## How to run the tests

```sh
cargo test                       # everything
cargo test --lib assembler       # the assembler unit tests only
cargo test --test full_compilation
```

The integration tests are in `tests/`, in one file for each area:
`lexer_pipeline.rs`, `parser_pipeline.rs`, `diagnostics.rs`, `pointers.rs`,
`increment.rs`, `call_args.rs`, `preprocessor.rs`, `entry_point.rs`,
`assembler_test.rs`, `full_compilation.rs`, and `lld_link_test.rs`.

The `lld_link_test.rs` suite needs LLVM. It fails without it.

Several suites compile a C file and then run the binary. They report a skip and pass
when the host cannot run a PE, which keeps the Linux CI job green.

> **Take care with spans in a test.** `Span` implements `PartialEq` so that it compares
> equal to *every* other `Span`. This lets a test compare AST nodes by structure. It
> also means that `assert_eq!(span_a, span_b)` **always passes** and tests nothing.
> Compare `span.start` and `span.end` one at a time.

## How to debug the compiler

Three environment variables write the intermediate state to stderr:

```sh
DUMP_TOKENS=1 vbrcc input.c     # the token stream from the lexer
DUMP_AST=1    vbrcc input.c     # the AST, pretty-printed
DUMP_ASM=1    vbrcc input.c     # the generated assembly text
```

`vbrcc input.c -E` prints the preprocessed source and exits. Use it to see what macro
expansion produced.

To find a bug, work backwards from the symptom. If the assembly is correct, the bug is
in the assembler. If the assembly is wrong, dump the AST and check the type that the
type checker assigned.

These external tools help:

```sh
objdump -d -M intel prog.exe                 # disassemble the output
hexdump -v -e '1/1 "%02x "' prog.exe         # the raw image bytes
gcc -S -masm=intel -O0 -fno-asynchronous-unwind-tables -fno-ident in.c
```

The last command produces assembly much closer in shape to VBRCC's output than a plain
`gcc -S`. This makes a diff practical.

## Code layout

Read [`architecture.md`](architecture.md) first. It describes all five stages, the data
that moves between them, the rules the code generator follows, and recipes for adding an
instruction, an operator, or a statement.

The API documentation is on [docs.rs](https://docs.rs/vbrcc). To build it locally:

```sh
cargo doc --no-deps --open
```

## Documentation style

Write documentation in **ASD-STE100 Simplified Technical English**. Keep one idea in one
sentence. Use the active voice and the simple present tense. Use one word for one
meaning. Keep a descriptive sentence to 25 words and a procedural sentence to 20.

Do not over-document the code. A comment must say why, not what. Delete a comment that
restates the line below it. Keep a comment that explains non-obvious hardware behaviour,
such as `idiv` leaving the remainder in `rdx`.

## License

VBRCC uses the GPL-3.0-or-later license. Contributions, when they open at v1.0.0, use
the same terms.
