// expect: 28
int sum(int *p, int n) { int s = 0; for (int i = 0; i < n; i++) s += p[i]; return s; }
int main() {
    int a[5] = {2, 4, 6, 8, 10};
    int *p = a + 1;
    *p = 1;
    p++;
    return sum(a, 5) + *p - a[1] - 6 + 2;
}
