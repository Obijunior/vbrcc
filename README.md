# VBRCC - Very Basic Rust C Compiler

A hobby C compiler and assembler written in Rust targeting x86-64 (Intel syntax)

## Usage

```sh
cargo run -- <input.c> [-o <output_file>] [--lld-link] [--gcc]
```

| Flag | Pipeline | External dependencies |
|---|---|---|
| *(none)* | Custom assembler emits a complete PE executable directly | None |
| `--lld-link` | Custom assembler emits COFF `.obj`, then `lld-link` links it | LLVM (`lld-link`, `llvm-dlltool`) + Windows SDK |
| `--gcc` | System `gcc` assembles and links | MinGW-w64 GCC |

The default path is fully self-contained — no external tools needed. Use `--lld-link` when you need C standard library functions (e.g., `printf`) resolved via `msvcrt.dll`.

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
| Logical AND/OR | `&&`, `||` |

### Not yet supported

- Multiple types (all variables implicitly `int` for now)
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

  
### Output formats

The assembler supports two output modes:

- **PE executable** (default): Encodes instructions into raw machine bytes and produces a complete Windows PE32+ executable with DOS header, COFF header, section table, and import table. External calls (e.g., `printf`) are resolved via IAT.
- **COFF `.obj`** (`--coff` flag): Emits a relocatable COFF object file with symbol table and `IMAGE_REL_AMD64_REL32` relocations for cross-section and external references. Designed to be linked by `lld-link`.

## Contributing / next steps
- C99 compliance
- Emit ELF64 output (currently Windows PE/COFF only).
- Proper x86-64 calling convention compliance (stack alignment, prologue/epilogue).
- Write a custom linker to replace `lld-link` dependency.

## Roadmap to C99
- add support for types
- add support for pointers
- add support for multiple functions and function parameters
- support for more C functionality: switch statements, break/continue, do while ...