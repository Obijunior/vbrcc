# VBRCC - Very Basic Rust C Compiler

This is a small hobby C compiler written in Rust. It currently targets x86-64 (Intel syntax) and includes a tiny custom assembler implemented as a subcrate. You can still use `gcc` as the assembler/linker by passing the `-gcc` flag to the compiler.

## Usage

Run the compiler with Cargo:

```sh
cargo run -- <input.c> [-o <output_file>] [--gcc]
```

- Default behavior: uses the custom assembler (Intel x86-64 syntax), while still using `gcc` as a linker.
- Pass `--gcc` to use the system `gcc` to assemble/link instead.

## Tests

Run the test suite with:

```sh
cargo test
```

## Compiler: C features

The compiler front-end (AST) currently supports the following operators and expressions:

| AST node | Example |
| --- | --- |
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

### Fun side notes

- if you run `objdump -d -M intel <output file>` you can see the disassembled assembly contained in the executeable. Doing a comparison between the output from my assembler and from when the --gcc flag is passed produces some pretty interesting results
- running `hexdump -v -e '1/1 "%02x "' <output file>` will give the raw hex from the executeable in a big chunk

### Notes and limitations

- The assembler currently encodes instructions into raw machine bytes and writes them to the output file. It does not emit a full object file format (ELF/COFF). Because of that, passing the produced file directly to `gcc` as an object file will usually fail. The project contains an `assembler_driver` that attempts to run the assembler and then call `gcc` to link; that driver is a work-in-progress and may require changes to produce linkable object files.
- Labels, relocations and multi-section object support are not implemented yet — those are planned enhancements.
- The assembler code includes an encoder for `imul`, but the textual parser does not accept `imul` at the moment.

## Contributing / next steps

- Improve the assembler to emit proper object files or emit GAS-compatible assembly.
- Add more instructions and addressing modes to the assembler.
- Extend the code generator to support more C features and proper ABI handling.enerator to support more C features and proper ABI handling.