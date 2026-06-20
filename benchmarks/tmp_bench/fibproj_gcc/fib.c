/*
 * fib.c — Compute-heavy C benchmark program
 *
 * Calculates the 45th Fibonacci number recursively (2^45 calls)
 * to stress-test runtime performance of the compiled binary.
 *
 * Usage:
 *   gcc -O2 -o fib fib.c
 *   time ./fib
 */

#include <stdio.h>
#include <stdlib.h>
#include <time.h>

static long long fib(int n) {
    if (n <= 1) return n;
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    printf("=== Fibonacci Benchmark ===\n");
    printf("Computing fib(45)...\n");
    fflush(stdout);

    clock_t start = clock();
    long long result = fib(45);
    clock_t end = clock();

    double elapsed = (double)(end - start) / CLOCKS_PER_SEC;
    printf("fib(45) = %lld\n", result);
    printf("Time: %.3f seconds\n", elapsed);

    return 0;
}
