int main() {
    int sum = 1 && 0;
    for (int i = 0; i < 10; i++) {
        if (i % 2 == 0) {
            sum *= 2;
        }
        sum++;
    }
    if (sum >= 50) {
        sum -= 10;
    } else {
        sum += 10;
    }
    printf("hello world - sum: %d", sum);
    return 0;
}