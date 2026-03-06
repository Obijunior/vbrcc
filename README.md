# VBRCC - Very Basic Rust C Compiler

This is a small hobby C compiler written in Rust. It currently targets x86-64 (Intel syntax) and includes a tiny custom assembler implemented as a subcrate. You can still use `gcc` as the assembler/linker by passing the `-gcc` flag to the compiler.

## Usage

Run the compiler with Cargo:

```sh
cargo run -- <input.c> [-o <output_base>] [-gcc]
```

- Default behavior: uses the custom assembler (Intel x86-64 syntax).
- Pass `-gcc` to use the system `gcc` to assemble/link instead.

## Tests

Run the test suite with:

```sh
cargo test
```

## Compiler: C features

The compiler front-end (AST) currently supports the following operators and expressions:

| AST node | Example |
| --- | --- |
<<<<<<< HEAD
| Add | `a + b` |
| Sub | `a - b` |
| Mul | `a * b` |
| Div | `a / b` |
| Negate | `-a` |
| BitNot | `~a` |
| LogNot | `!a` |

## Assembler: currently supported features

The custom assembler (subcrate `src/assembler`) currently supports a small subset of Intel x86-64 instructions and registers. Important notes:

- Syntax: Intel syntax (the assembler accepts `.intel_syntax noprefix`).
- Registers: all 64-bit general-purpose registers are recognised (RAX, RBX, RCX, RDX, RSI, RDI, RBP, RSP, R8-R15).
- Supported instructions (textual forms accepted by the assembler):
  - `ret`
  - `push <reg>`
  - `pop <reg>`
  - `mov <reg>, <reg>` and `mov <reg>, <imm>` (imm is a 64-bit integer)
  - `add <reg>, <reg>`
  - `sub <reg>, <reg>`

### Notes and limitations

- The assembler currently encodes instructions into raw machine bytes and writes them to the output file. It does not emit a full object file format (ELF/COFF). Because of that, passing the produced file directly to `gcc` as an object file will usually fail. The project contains an `assembler_driver` that attempts to run the assembler and then call `gcc` to link; that driver is a work-in-progress and may require changes to produce linkable object files.
- Labels, relocations and multi-section object support are not implemented yet — those are planned enhancements.
- The assembler code includes an encoder for `imul`, but the textual parser does not accept `imul` at the moment.

## Contributing / next steps

- Improve the assembler to emit proper object files or emit GAS-compatible assembly.
- Add more instructions and addressing modes to the assembler.
- Extend the code generator to support more C features and proper ABI handling.enerator to support more C features and proper ABI handling.
 emit COFF/PE object files on Windows so `gcc` can link them).
=======
| Add | + |
| Sub | - |
| Mul | * |
| Div | / |
| Negate | - |
| BitNot | ~ |
| LogNot | ! |

## Instruction Set (currently) supported by assembler

- mov
- ret
>>>>>>> 3c37ce23f8992ea75b8b983087e5579f60575a1e
