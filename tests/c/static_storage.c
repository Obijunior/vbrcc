// expect: 42
static int total = 5;
static int helper(int x) { return x * 2; }
int counter() {
    static int calls;
    calls++;
    return calls;
}
int once() {
    static int seed = 100;
    seed = seed + 1;
    return seed;
}
int other() {
    static int calls = 7;
    return calls;
}
int fill() {
    static int buf[3];
    buf[0] = buf[0] + 1;
    return buf[0] + buf[2];
}
int main() {
    counter(); counter();
    int c = counter();
    once();
    int o = once();
    fill();
    int f = fill();
    return c + (o - 100) + other() + f + helper(total) + 18;
}
