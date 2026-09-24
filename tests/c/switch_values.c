// expect: 6
enum Mode { OFF, ON = 5, AUTO };
int kind(long long v) {
    switch (v) {
    case -1: return 1;
    case 5000000000: return 2;
    case ON: return 3;
    }
    return 0;
}
int main() {
    enum Mode m = AUTO;
    int r = 0;
    switch (m) { case OFF: r = 50; break; case AUTO: r = 0; break; }
    return kind(-1) + kind(5000000000) + kind(5) + kind(4) + r;
}
