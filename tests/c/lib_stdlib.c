// expect: 100
#include <stdlib.h>
int main() {
    int r = 0;
    r += atoi("42");
    r += atol("-2");
    char *end;
    long v = strtol("  123xyz", &end, 10);
    if (v == 123 && *end == 'x') r += 10;
    if (strtol("ff", 0, 16) == 255) r += 5;
    int *z = calloc(4, 4);
    if (z[0] == 0 && z[3] == 0) r += 1;
    z[0] = 9;
    z = realloc(z, 64);
    if (z[0] == 9) r += 1;
    free(z);
    div_t q = div(17, 5);
    r += q.quot * 10 + q.rem;
    ldiv_t lq = ldiv(-17, 5);
    r += lq.quot + lq.rem;
    r += labs(-6);
    srand(7);
    int a = rand();
    srand(7);
    int b = rand();
    if (a == b && a >= 0 && a <= RAND_MAX) r += 3;
    if (getenv("PATH") != 0) r += 7;
    return r;
}
