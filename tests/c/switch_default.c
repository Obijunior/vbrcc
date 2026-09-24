// expect: 37
int f(int x) {
    int r = 0;
    switch (x) {
    default: r = 7;
    case 1: r += 10; break;
    case 2: r = 20; break;
    }
    return r;
}
int g(int x) {
    int r = 3;
    switch (x) { case 1: r = 100; }
    return r;
}
int main() { return f(9) + f(2) - f(1) + g(5) + 7; }
