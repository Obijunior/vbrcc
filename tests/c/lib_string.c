// expect: 245
#include <string.h>
#include <stdlib.h>
int main() {
    char buf[32];
    memset(buf, 0, 32);
    memcpy(buf, "abcdef", 6);
    memmove(buf + 2, buf, 4);
    int r = 0;
    if (memcmp(buf, "ababcd", 6) == 0) r += 1;
    if (strchr(buf, 'c') == buf + 4) r += 2;
    if (strrchr(buf, 'b') == buf + 3) r += 4;
    if (strstr(buf, "bcd") == buf + 3) r += 8;
    r += strspn("aaab", "a") * 16;
    r += strcspn("hello", "l");
    char t[16];
    strncpy(t, "xyz", 16);
    strncat(t, "12345", 2);
    if (strcmp(t, "xyz12") == 0) r += 64;
    if (strncmp("abcX", "abcY", 3) == 0) r += 100;
    char *d = strdup("dup");
    if (strcmp(d, "dup") == 0) r += 10;
    free(d);
    char words[20];
    strcpy(words, "a,bb,ccc");
    char *w = strtok(words, ",");
    while (w) {
        r += strlen(w);
        w = strtok(0, ",");
    }
    return r;
}
