// expect: 3
int main() { int a[5]; int *p = a; int *q = a + 3; return q - p; }
