// expect: 17
int main() {
    int a = -7 / 2;
    int b = -7 % 2;
    int c = 7 / -2;
    int d = 7 % -2;
    return (a + 3) * 1 + (b + 1) + (c + 3) + (d - 1) + 17;
}
