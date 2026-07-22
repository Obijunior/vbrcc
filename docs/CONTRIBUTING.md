# Contributing to VBRCC

Thanks for your interest in the project.

## Project status: issues welcome, pull requests are not

VBRCC is a personal learning project. Until it reaches C99 compliance at **v1.0.0**, the
internals move too fast for outside patches to be practical. The type system, the code
generator's calling convention, and the assembler's instruction set are all still being
reshaped between releases.

**What is very welcome:**

- **Bug reports**, especially miscompilations
- **Questions** about how any part of the compiler works
- **Ideas** and feature suggestions
- **Corrections** to the documentation

Please [open an issue](https://github.com/obijunior/vbrcc/issues).

**Pull requests are not being accepted at this time.** This will change at v1.0.0. If
you have opened one already, thank you. Please convert it into an issue describing the
problem, and I will read it.

## Reporting a bug

The most valuable report is a **miscompilation**: a program VBRCC accepts but compiles
to the wrong behaviour. These are harder to find than crashes, because the compiler
reports success and you only notice when the program misbehaves.

A good report has three things:

1. **A minimal C file.** Cut it down until removing one more line makes the bug vanish.
2. **What you expected**, and what happened instead: exit code, printed output, or a
   crash.
3. **The backend you used**: default, `--lld-link`, or `--gcc`.

Generated assembly helps a lot. Attach it with:

```sh
vbrcc bug.c --keep-artifacts     # leaves bug.s next to the source
```

If you can, compare against GCC. A diff between the two disassemblies usually points at
the bad instruction:

```sh
vbrcc bug.c -o mine
vbrcc bug.c --gcc -o theirs
objdump -d -M intel mine.exe   > mine.asm
objdump -d -M intel theirs.exe > theirs.asm
diff mine.asm theirs.asm
```

For a **crash** or a **rejected valid program**, the compiler's own diagnostic plus the
source file is usually enough.

## Building from source

```sh
git clone https://github.com/obijunior/vbrcc
cd vbrcc
cargo build
```

Only a stable Rust toolchain is needed to build the compiler itself.

### Backend prerequisites

The default backend needs nothing. VBRCC encodes the machine code and writes the PE
executable itself. The other two shell out:

| Backend | Needs | Getting it |
|---|---|---|
| *(default)* | nothing | — |
| `--lld-link` | `lld-link`, `llvm-dlltool`, Windows SDK | [LLVM releases](https://github.com/llvm/llvm-project/releases), or `winget install LLVM.LLVM` |
| `--gcc` | MinGW-w64 GCC | [MSYS2](https://www.msys2.org/), then `pacman -S mingw-w64-x86_64-gcc` |

Note that VBRCC only emits **Windows PE/COFF** binaries. It builds and runs on Linux and
macOS, but the executables it produces will not run there natively.

## Running the tests

```sh
cargo test                       # everything
cargo test --lib assembler       # just the assembler unit tests
cargo test --test full_compilation
```

Integration tests live in `tests/`, split roughly by stage: `lexer_pipeline.rs`,
`parser_pipeline.rs`, `diagnostics.rs`, `pointers.rs`, `assembler_test.rs`,
`full_compilation.rs`, and `lld_link_test.rs`.

The `lld_link_test.rs` suite requires LLVM to be installed and will fail without it.

> **Careful with spans in tests.** `Span` implements `PartialEq` to compare equal to
> *every* other `Span`, so AST nodes can be compared structurally. This means
> `assert_eq!(span_a, span_b)` **always passes** and tests nothing. Assert on
> `span.start` and `span.end` individually instead.

## Debugging the compiler

Three environment variables dump intermediate state to stderr:

```sh
DUMP_TOKENS=1 vbrcc input.c     # token stream from the lexer
DUMP_AST=1    vbrcc input.c     # parsed AST, pretty-printed
DUMP_ASM=1    vbrcc input.c     # generated assembly text
```

Working backwards from the symptom is usually fastest: if the assembly looks right, the
bug is in the assembler; if it looks wrong, dump the AST and check whether the type
checker assigned what you expected.

Useful external tools:

```sh
objdump -d -M intel prog.exe                 # disassemble the output
hexdump -v -e '1/1 "%02x "' prog.exe         # raw image bytes
gcc -S -masm=intel -O0 -fno-asynchronous-unwind-tables -fno-ident in.c
```

That last one produces assembly much closer in shape to VBRCC's output than a plain
`gcc -S`, which makes diffing practical.

## Code layout

Read [`architecture.md`](architecture.md) first. It walks through all five stages, the
data that moves between them, the code generator's register discipline, and step-by-step
recipes for adding an instruction, an operator, or a statement.

API documentation is on [docs.rs](https://docs.rs/vbrcc), or build it locally:

```sh
cargo doc --no-deps --open
```

## License

VBRCC is licensed under the GPL-3.0-or-later. Contributions, when they open at v1.0.0,
will be under the same terms.
