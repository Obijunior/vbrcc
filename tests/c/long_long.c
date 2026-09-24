// expect: 42
int main() {
    long long big = 1;
    for (int i = 0; i < 40; i++) big = big * 2;
    long long back = big / 1099511627776;
    return (int)back + 41;
}
