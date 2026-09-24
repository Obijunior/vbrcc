// expect: 94
int main() {
    int a = 0;
    for (int i = 0; i < 10; i++) { if (i % 2) continue; a += i; }
    int b = 0;
    while (1) { b++; if (b == 7) break; }
    int c = 0;
    for (int i = 0; i < 3; i++) for (int j = 0; j < 5; j++) { if (j == 2) break; c++; }
    int d = 0;
    for (;;) { d += 3; if (d > 20) break; }
    return a + b + c + d + 40;
}
