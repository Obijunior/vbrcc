# Example programs

These C files exercise the compiler. The table gives the backend each one needs and the
result you get when it works.

| File | Backend | Result |
|---|---|---|
| [`return42.c`](return42.c) | default | exits with code `42` |
| [`input.c`](input.c) | default | prints `hello world - sum: 52` |
| [`multiple_functions.c`](multiple_functions.c) | default | prints `hello world`, then `hello world 5` |
| [`reverse_string.c`](reverse_string.c) | default | prints a string and its reverse |
| [`matrix_test.c`](matrix_test.c) | — | **does not compile yet**. See below. |

None of these need an external toolchain. The default backend encodes the machine code
and writes the PE itself, and the PE has an import table. Each `printf` call therefore
resolves against `msvcrt.dll` at load time.

## `return42.c`

This is the smallest program the compiler handles. Use it as the first check that a
build works. It calls nothing, so the output has no import table:

```console
$ vbrcc examples/return42.c -o ret
[ SUCCESS ] :: Wrote assembly to "ret.s"
[ SUCCESS ] :: Created Windows Executable: "ret.exe"
  - .text size: 34 bytes
  - .data size: 0 bytes
  - .idata size: 0 bytes
[ SUCCESS ] :: Compiled binary to "ret.exe"

$ ./ret.exe; echo $?
42
```

## `input.c`

This file uses most of the supported language at once: `#include <stdio.h>`, `for`,
`if` and `else`, `%`, the compound assignments `*=`, `+=`, and `-=`, post-increment, the
comparison operators, logical `&&`, and a block comment.

```console
$ vbrcc examples/input.c -o input
$ ./input.exe
hello world - sum: 52
```

## `multiple_functions.c`

This file has two functions. `main` calls `idk` before the file defines `idk`, which
checks that a forward reference resolves. The call also passes a format argument.

`idk` has no prototype. The type checker allows a call to a name it has never seen,
because C89 permits it, and because it keeps programs written before `#include` worked
compiling. The type checker does check the argument count of a call to a function that
has a prototype.

```console
$ vbrcc examples/multiple_functions.c -o multi
$ ./multi.exe
hello world
hello world 5
```

## `reverse_string.c`

This file walks a `char` array with a `while` loop, writes to it by index, and passes
two `%s` arguments to `printf`. It is the closest thing here to an ordinary C program.

```console
$ vbrcc examples/reverse_string.c -o rev
$ ./rev.exe
Original string: [ hello world ]
Reversed string: [ dlrow olleh ]
```

## `matrix_test.c`

**This file does not compile yet.** It is a target for the next phase of work, not a
working sample. It needs two features that do not exist: multi-dimensional arrays, and
brace initialiser lists.

Today it fails in the parser:

```console
$ vbrcc examples/matrix_test.c
error: expected `;`, found `[`
  --> examples/matrix_test.c:4:18
   |
 4 |     int matrix[3][3] = {{1,2,3}, {4,5,6}, {7,8,9}};
   |                  ^ expected `;` here
```

## When you still need `--lld-link`

The import table of the default backend covers **one DLL**, `msvcrt.dll`. That covers
the C runtime functions these examples call. A program that also imports from
`kernel32`, `user32`, or the UCRT builds, but those symbols do not resolve at load time.
Use `--lld-link` for such a program.

`--lld-link` needs LLVM and the Windows SDK. See
[`../docs/CONTRIBUTING.md`](../docs/CONTRIBUTING.md) for the setup.

## A note on `#include`

Several of these files start with `#include <stdio.h>`, and the line does real work. The
bundled `stdio.h` declares `printf`, and that declaration lets the type checker verify
the call. A small header set ships inside the binary: `stdio.h`, `stdlib.h`, `string.h`,
`stdbool.h`, `stddef.h`, `stdint.h`, and `limits.h`. The `-I <dir>` flag adds a search
directory, and a header there replaces a bundled header of the same name.

`vbrcc file.c -E` prints the preprocessed source and exits. This is the fastest way to
see what macro expansion produced.
