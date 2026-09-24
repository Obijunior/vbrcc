// expect: 31
#include <string.h>
#include <stdlib.h>
#include <ctype.h>
/* Win64 defines only the low bits of an int or char return value. The
   caller must extend them before a 64-bit compare. */
int main() {
    int r = 0;
    if (strcmp("a", "b") < 0) r += 1;
    if (atoi("-5") < 0) r += 2;
    if (atoi("-5") == -5) r += 4;
    if (tolower('A') == 'a') r += 8;
    if (strncmp("ab", "aa", 2) > 0) r += 16;
    return r;
}
