// expect: 33
int cell(int a, int b) {
    switch (a) {
    case 0:
        switch (b) { case 0: return 1; case 1: return 2; }
        return 3;
    case 1: {
        int t = b * 10;
        switch (b) { default: t += 5; }
        return t;
    }
    }
    return 0;
}
int main() { return cell(0, 0) + cell(0, 1) + cell(0, 9) + cell(1, 2) + cell(5, 5) + 2; }
