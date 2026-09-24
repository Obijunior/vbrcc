// expect: 111
int main() {
    int r = 0;
    switch (2) {
    case 1: r += 1000;
    case 2: r += 100;
    case 3: r += 10;
    case 4: r += 1; break;
    case 5: r += 5000;
    }
    return r;
}
