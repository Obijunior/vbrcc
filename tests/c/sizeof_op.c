// expect: 0
#include <stdlib.h>
struct S { char c; int i; char d; };
struct Node { int v; struct Node *next; };
typedef struct { long long a; int b; } Pair;
char s[] = "hello";
int main() {
    int a[10];
    int m[3][4];
    struct Node *p = 0;
    int i = 0;
    if (sizeof(char) != 1) return 1;
    if (sizeof(int) != 4) return 2;
    if (sizeof(long) != 4) return 3;
    if (sizeof(long long) != 8) return 4;
    if (sizeof(int *) != 8) return 5;
    if (sizeof(struct S) != 12) return 6;
    if (sizeof(struct Node) != 16) return 7;
    if (sizeof(Pair) != 16) return 8;
    if (sizeof a != 40) return 9;
    if (sizeof(a[0]) != 4) return 10;
    if (sizeof m != 48 || sizeof m[0] != 16) return 11;
    if (sizeof s != 6) return 12;
    if (sizeof *p != 16 || sizeof p->next != 8) return 13;
    if (sizeof(int[4]) != 16) return 14;
    if (sizeof(i++) != 4 || i != 0) return 15;
    if (sizeof a / sizeof a[0] != 10) return 16;
    if (sizeof "abc" != 4) return 17;
    int *buf = malloc(5 * sizeof(int));
    buf[4] = 9;
    int r = buf[4];
    free(buf);
    return r - 9;
}
