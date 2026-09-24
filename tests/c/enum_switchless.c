// expect: 17
enum Color { RED, GREEN = 5, BLUE };
enum Color pick(int i) { return i ? BLUE : RED; }
int main() { enum Color c = pick(1); return c * 2 + GREEN - RED; }
