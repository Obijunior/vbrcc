// expect: 0
// expect-stdout: 1 2 3 4 5 6\nsix=91 mix=ok nested=ok\n
#include <stdio.h>

int weigh6(int a, int b, int c, int d, int e, int f) {
    return a * 1 + b * 2 + c * 3 + d * 4 + e * 5 + f * 6;
}

/* Every slot past the fourth carries a different type, so a wrong offset or a
   4-byte load from an 8-byte slot shows up. */
long long mix8(char a, int b, long long c, int *p, char d, int e, long long f, int g) {
    return a + b + c + *p + d + e + f + g;
}

struct Big { int x, y, z, w; };      /* 16 bytes: passed by hidden pointer */
struct Small { int lo, hi; };        /* 8 bytes: passed in the slot itself */

int structs(int a, int b, int c, int d, struct Big big, struct Small small) {
    return a + b + c + d + big.x + big.w + small.lo * small.hi;
}

/* A memory-class return takes rcx for the hidden pointer, so the fourth real
   parameter is the fifth argument and travels on the stack. */
struct Big make(int a, int b, int c, int d) {
    struct Big r;
    r.x = a; r.y = b; r.z = c; r.w = d;
    return r;
}

int sum5_down(int n, int a, int b, int c, int acc) {
    if (n == 0) return acc + a + b + c;
    return sum5_down(n - 1, a, b, c, acc + n);
}

/* A five-argument function that makes its own six-argument call: its incoming
   stack arguments and its outgoing area must not overlap. */
int relay(int a, int b, int c, int d, int e) {
    int inner = weigh6(e, d, c, b, a, 1);
    return inner + e;
}

int main() {
    printf("%d %d %d %d %d %d\n", 1, 2, 3, 4, 5, 6);

    int six = weigh6(1, 2, 3, 4, 5, 6);

    int seven = 7;
    long long big = 5000000000;
    long long m = mix8(1, 2, big, &seven, 4, 5, big, 6);
    int mix_ok = (m == 10000000025);

    /* Inner calls write their own stack arguments into the same outgoing area,
       so the outer call must fill its slots only after both are done. */
    int nested = weigh6(weigh6(1, 1, 1, 1, 1, 1), 2, 3, weigh6(0, 0, 0, 0, 0, 1), 5, 6);
    int nested_ok = (nested == 21 + 4 + 9 + 24 + 25 + 36);

    struct Big b4; b4.x = 10; b4.y = 0; b4.z = 0; b4.w = 20;
    struct Small s2; s2.lo = 3; s2.hi = 4;
    if (structs(1, 2, 3, 4, b4, s2) != 52) return 1;

    struct Big r = make(5, 6, 7, 8);
    if (r.x != 5 || r.w != 8) return 2;

    if (sum5_down(4, 1, 2, 3, 100) != 116) return 3;
    if (relay(1, 2, 3, 4, 5) != 5 + 8 + 9 + 8 + 5 + 6 + 5) return 4;

    printf("six=%d mix=%s nested=%s\n", six, mix_ok ? "ok" : "BAD", nested_ok ? "ok" : "BAD");
    return 0;
}
