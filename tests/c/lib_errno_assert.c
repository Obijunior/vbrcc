// expect: 42
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <assert.h>
int main() {
    int r = 0;
    errno = 0;
    FILE *f = fopen("vbrcc_no_such_dir/missing.txt", "r");
    if (f == 0 && errno == ENOENT) r += 10;
    errno = 0;
    strtol("99999999999999999999", 0, 10);
    if (errno == ERANGE) r += 20;
    assert(r == 30);
    r += 12;
#define NDEBUG
#include <assert.h>
    assert(r == 0);
    return r;
}
