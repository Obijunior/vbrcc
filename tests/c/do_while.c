// expect: 36
int main() {
    int once = 0;
    do { once++; } while (0);
    int i = 0, s = 0;
    do { i++; if (i % 2) continue; s += i; } while (i < 6);
    int k = 0;
    do { k++; if (k == 4) break; } while (1);
    int n = 0;
    do n += 5; while (n < 20);
    return once + s + k + n - 1;
}
