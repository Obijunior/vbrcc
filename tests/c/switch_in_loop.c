// expect: 7
int main() {
    int sum = 0;
    for (int i = 0; i < 10; i++) {
        switch (i % 3) {
        case 0: continue;
        case 1: sum += i; break;
        default: sum += 1;
        }
        sum += 100;
        if (sum > 400) break;
    }
    return sum - 400;
}
