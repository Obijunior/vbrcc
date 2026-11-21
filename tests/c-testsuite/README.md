# c-testsuite

A copy of the `single-exec` suite from <https://github.com/c-testsuite/c-testsuite>,
commit `5c7275656d751de0e68b2d340a95b5681858ed07` (2020-03-09).

Each test is `NNNNN.c`. A test passes when it exits with `0` and its stdout is the same
as `NNNNN.c.expected`. `NNNNN.c.tags` gives the C standard and needs (for example
`needs-libc`). `NNNNN.c.otags` gives the origin of the test.

`tests/c_testsuite.rs` runs the suite. `passing.txt` lists the tests that VBRCC passes.
The test fails when a listed test fails, or when a test that is not listed passes. When
a fix makes a test pass, add its name to `passing.txt`.

## Licenses

- `LICENSE`: MIT. It applies to the upstream runner software, not to the tests.
- Each test has the license of its origin, in `NNNNN.c.otags`:
  - `www.simple-cc.org` (scc): ISC.
  - `bellard.org/tcc` (tinycc): LGPL-2.1-or-later.

This directory is not in the published crate (`exclude` in `Cargo.toml`).
