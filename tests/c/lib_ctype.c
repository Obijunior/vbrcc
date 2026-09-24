// expect: 105
#include <ctype.h>
int main() {
    char *s = "Hello, World 42!\t";
    int a = 0, d = 0, sp = 0, u = 0, p = 0;
    for (int i = 0; s[i]; i++) {
        if (isalpha(s[i])) a++;
        if (isdigit(s[i])) d++;
        if (isspace(s[i])) sp++;
        if (isupper(s[i])) u++;
        if (ispunct(s[i])) p++;
    }
    int r = a + d * 10 + sp * 20 + u * 5 + p;
    r += (toupper('a') == 'A') + (tolower('Q') == 'q') + (isxdigit('f') != 0);
    return r;
}
