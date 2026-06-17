# VBRCC - Very Basic Rust C Compiler

A hobby C compiler written in Rust targeting x86-64 (Intel syntax), with a custom assembler implemented as a subcrate. Every stage is hand-rolled — no LLVM, no Cranelift, no parser generators.

## Usage

Run the compiler with Cargo:

```sh
cargo run -- <input.c> [-o <output_file>] [--gcc]
```

- Default behavior: uses the custom assembler (Intel x86-64 syntax), while still using `gcc` as a linker.
- Pass `--gcc` to use the system `gcc` to assemble and link instead.

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
| Single line comment | `// this is a comment` |

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
- Registers: all 64-bit general-purpose registers (RAX–R15) and 8-bit sub-registers (AL, BL, CL, DL).
- Supported instructions (textual forms accepted by the assembler):
  - `ret`, `syscall`, `cqo`
  - `push <reg>`, `pop <reg>`
  - `neg <reg>`, `not <reg>`, `idiv <reg>`
  - `mov <reg>, <reg>` / `mov <reg>, <imm64>` / `mov <reg>, [reg +/- disp]` / `mov [reg +/- disp], <reg>`
  - `movzx <reg64>, <reg8>`
  - `add <reg>, <reg|imm32>`, `sub <reg>, <reg|imm32>`
  - `imul <reg>, <reg|imm32>`
  - `and <reg>, <reg|imm32>`, `cmp <reg>, <reg|imm32>`
  - `sete`, `setne`, `setl`, `setle`, `setg`, `setge` (8-bit register operand)
  - `jmp <label>`, `je <label>`, `jne <label>`, `jl <label>`, `jle <label>`, `jg <label>`, `jge <label>`
  - `lea <reg>, [rip + label]`
  - `call <label>`

### Fun tests you can run

- if you run `objdump -d -M intel <executable file>` you can see the disassembled assembly contained in the executable. Doing a comparison between the output from my assembler and from when the --gcc flag is passed produces some pretty interesting results
- running `hexdump -v -e '1/1 "%02x "' <executable file>` will give the raw hex from the executable in a big chunk
- running `gcc -S -masm=intel <c code>` will show you how `gcc` compiles the inputted c code 
- Similar: `gcc -S -masm=intel -O0 -fno-asynchronous-unwind-tables -fno-ident input.c` but without optimizations or the `.seh_*` directives

### Notes and limitations

- The assembler encodes instructions into raw machine bytes and produces Windows PE executables. It handles labels within `.text` and `.data` sections, as well as external function calls via IAT.
- Control flow (loops, conditionals) is fully supported via `jmp`/`jcc` jump instructions, `setcc` conditional byte-set, and `movzx` zero-extension.
- No ELF output — currently Windows PE only.

## Contributing / next steps

- Emit ELF64 output (currently Windows PE only).
- Extend the C frontend: function parameters, multiple types, `break`/`continue`, `switch`.
- Proper x86-64 calling convention compliance (stack alignment, prologue/epilogue).
