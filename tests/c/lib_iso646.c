// expect: 13
#include <iso646.h>
int main() {
    int a = 6, b = 3, r = 0;
    if (a > 1 and b > 1) r += 1;
    if (not (a == b)) r += 2;
    if (a not_eq b) r += 4;
    r += a bitand b;
    if (a xor b) r += 4;
    if (0 or compl 0) r = r bitor 0;
    return r;
}
