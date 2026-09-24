// expect: 21
int hits = 0;
int touch(int v) { hits = hits + 1; return v; }
int main() {
    int r = 0;
    if (touch(0) && touch(1)) r = 100;
    if (touch(1) || touch(1)) r = r + 10;
    int t = touch(0) ? touch(5) : touch(9);
    return r + hits + t - 4 + 2;
}
