// expect: 2
int main() { int a[3]; a[0] = 1; a[1] = 2; a[2] = 3; int *p = 1 + a; return *p; }
