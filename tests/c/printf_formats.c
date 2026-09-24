// expect: 0
// expect-stdout: n=42 s=hi c=Z\nneg=-7 line two
#include <stdio.h>
int main() {
    printf("n=%d s=%s c=%c\n", 42, "hi", 'Z');
    printf("neg=%d line two", -7);
    return 0;
}
