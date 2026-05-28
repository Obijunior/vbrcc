# VBRCC - Very Basic Rust C Compiler

A hobby C compiler written in Rust targeting x86-64 (Intel syntax), with a custom assembler implemented as a subcrate. Every stage is hand-rolled — no LLVM, no Cranelift, no parser generators.

## Usage

Run the compiler with Cargo:

```sh
cargo run -- <input.c> [-o <output_file>] [--gcc]
```

- Default behavior: uses the custom assembler (Intel x86-64 syntax), while still using `gcc` as a linker.
- Pass `--gcc` to use the system `gcc` to assemble and link instead. Required for programs that use control flow (for, while, if/else), since the custom assembler does not yet support labels and jumps.

## Tests

Run the test suite with:

```sh
cargo test
```

## Compiler: C features

### Expressions

| Feature | Example |
| --- | --- |
| Integer literals | `42` |
| String literals | `"hello\n"` |
| Variables | `x`, `sum` |
| Addition | `a + b` |
| Subtraction | `a - b` |
| Multiplication | `a * b` |
| Division | `a / b` |
| Modulo | `a % b` |
| Negate | `-a` |
| Bitwise NOT | `~a` |
| Logical NOT | `!a` |
| Comparison | `<`, `<=`, `>`, `>=` |
| Assignment | `x = 5` |
| Compound assignment | `+=`, `-=`, `*=`, `/=`, `%=` |
| Post-increment/decrement | `i++`, `i--` |
| Function calls | `printf("hello")` |

### Statements and control flow

| Feature | Example |
| --- | --- |
| Return | `return expr;` |
| Variable declaration | `int x = 0;` |
| For loops | `for (int i = 0; i < 10; i++) { ... }` |
| While loops | `while (cond) { ... }` |
| If/else | `if (cond) { ... } else { ... }` |

### Not yet supported

- Multiple types (only `int` for now)
- Function definitions with parameters
- Arrays, pointers, structs
- `switch`, `do-while`, `break`, `continue`
- Preprocessor directives (`#include`, `#define`)

## Assembler: currently supported features

The custom assembler (subcrate `src/assembler`) supports a small subset of Intel x86-64 instructions and registers:

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

- if you run `objdump -d -M intel <output file>` you can see the disassembled assembly contained in the executable. Doing a comparison between the output from my assembler and from when the --gcc flag is passed produces some pretty interesting results
- running `hexdump -v -e '1/1 "%02x "' <output file>` will give the raw hex from the executable in a big chunk

### Notes and limitations

- The assembler currently encodes instructions into raw machine bytes and writes them to the output file. It does not emit a full object file format (ELF/COFF). Because of that, passing the produced file directly to `gcc` as an object file will usually fail. The project contains an `assembler_driver` that attempts to run the assembler and then call `gcc` to link; that driver is a work-in-progress and may require changes to produce linkable object files.
- Labels, relocations and multi-section object support are not implemented yet — those are planned enhancements.
- The assembler code includes an encoder for `imul`, but the textual parser does not accept `imul` at the moment.
- Programs using control flow (loops, conditionals) must be compiled with `--gcc` until the custom assembler gains label and jump support.

## Contributing / next steps

- Emit proper ELF64/COFF object files from the custom assembler (biggest gap — needed for clean `gcc` linking).
- Add labels, relocations, and jump instructions to the assembler.
- Add more instructions and addressing modes to the assembler (`cmp`, `setcc`, memory operands `[reg + disp]`, etc.).
- Extend the C frontend: function parameters, multiple types, `break`/`continue`, `switch`.
- Proper x86-64 calling convention compliance (stack alignment, prologue/epilogue).
