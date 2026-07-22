# Example programs

Sample C files for exercising the compiler. Each is listed with the backend it needs and
what it should do when it works.

| File | Backend | Result |
|---|---|---|
| [`test_return.c`](test_return.c) | default | exits with code `42` |
| [`input.c`](input.c) | `--lld-link` | prints `hello world - sum: 52` |
| [`test.c`](test.c) | `--lld-link` | prints `hello world` then `hello world 5` |
| [`matrix_test.c`](matrix_test.c) | — | **does not compile yet** (see below) |

## `test_return.c`

The smallest program the compiler handles, and the best first check that a build works.
It calls nothing, so the fully self-contained backend can build it:

```console
$ vbrcc examples/test_return.c -o ret
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

Exercises most of the supported language at once: `for`, `if`/`else`, `%`, compound
assignment (`*=`, `+=`, `-=`), post-increment, comparison operators, and logical `&&`.
Calls `printf`, so it needs `--lld-link`:

```console
$ vbrcc examples/input.c --lld-link -o input
$ ./input.exe
hello world - sum: 52
```

## `test.c`

Two functions, with `main` calling `idk` before `idk` is defined, which checks that
forward references resolve. Also uses a format argument. Needs `--lld-link`:

```console
$ vbrcc examples/test.c --lld-link -o test
$ ./test.exe
hello world
hello world 5
```

## `matrix_test.c`

**This one is aspirational and does not compile yet.** It is kept as a target for the
next phase of work, not as a working sample. It needs two features that do not exist:
multi-dimensional arrays, and brace initialiser lists.

Currently it fails in the parser:

```console
$ vbrcc examples/matrix_test.c
error: expected `;`, found `[`
  --> examples/matrix_test.c:4:18
   |
 4 |     int matrix[3][3] = {{1,2,3}, {4,5,6}, {7,8,9}};
   |                  ^ expected `;` here
```

## Why `printf` needs `--lld-link`

The default backend writes a complete PE executable itself, including an import table.
That path is still being developed: a program calling `printf` will build, but the
resulting executable currently fails to start (exit code `127`, no output). Until that is
fixed, use `--lld-link` for anything touching the C standard library, and reserve the
default backend for self-contained programs like `test_return.c`.

`--lld-link` requires LLVM and the Windows SDK. See
[`../docs/CONTRIBUTING.md`](../docs/CONTRIBUTING.md) for setup.

## A note on `#include`

Several of these files begin with `#include <stdio.h>`. VBRCC has no preprocessor, and
the lexer discards any line starting with `#`. The include is therefore decorative: it is
neither processed nor rejected. `printf` links anyway because the linker resolves the
symbol from `msvcrt.dll`, not because a header declared it.
