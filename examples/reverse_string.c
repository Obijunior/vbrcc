#include <stdio.h>

void string_reverse(char string[]) {
    int len = 0;
    while (string[len] != '\0') len++;
    char reversed_string[256];
    for (int i =0; i < len; i++) {
        reversed_string[i] = string[len - 1 - i];
    }
    reversed_string[len] = '\0';
    printf("Original string: [ %s ]\nReversed string: [ %s ]", string, reversed_string);
}

void main() {
    string_reverse("hello world");
}