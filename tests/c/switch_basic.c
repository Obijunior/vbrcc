// expect: 42
int pick(int x) {
    switch (x) {
    case 1: return 10;
    case 2: return 20;
    case 3: return 30;
    }
    return 99;
}
int main() { return pick(2) + pick(1) + pick(3) - pick(7) + 81; }
