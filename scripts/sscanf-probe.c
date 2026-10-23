/* Prints what the C library's sscanf makes of each input line with the formats Graphviz
 * uses for positions and sizes, for the table in crates/graphviz-pure-core/src/strtod_tests.rs.
 * One output line per input line and format:
 *
 *   <format> <count> [<consumed (%n), or -1>] <values as bits> [<%c as a number, or -1>]
 *
 * where <format> is 2 ("%lf,%lf%n"), 2c ("%lf,%lf%c"), 3c ("%lf,%lf,%lf%c"),
 * 4 ("%lf,%lf,%lf,%lf") or 5 ("%lf,%lf,%lf,%lf,%lf"), and a NaN prints as its sign and "nan".
 *
 *   cc scripts/sscanf-probe.c -o /tmp/sscanf-probe && /tmp/sscanf-probe < inputs.txt
 */
#include <stdio.h>
#include <math.h>
#include <stdint.h>
#include <string.h>

/* A double as its bits in hex, or "nan" with its sign. */
static const char *bits(double v, char *buf) {
    uint64_t b;
    if (isnan(v))
        return signbit(v) ? "-nan" : "+nan";
    memcpy(&b, &v, sizeof b);
    snprintf(buf, 20, "%016llx", (unsigned long long)b);
    return buf;
}

int main(void) {
    char line[4096];
    while (fgets(line, sizeof line, stdin)) {
        line[strcspn(line, "\n")] = 0;
        char b0[20], b1[20], b2[20], b3[20];
        double v[4] = {0, 0, 0, 0};
        char c = 0;
        int n = -1, k;
        k = sscanf(line, "%lf,%lf%n", &v[0], &v[1], &n);
        printf("2 %d %d %s %s\n", k, n, bits(v[0], b0), bits(v[1], b1));
        memset(v, 0, sizeof v), n = -1, c = 0;
        k = sscanf(line, "%lf,%lf%c", &v[0], &v[1], &c);
        printf("2c %d %s %s %d\n", k, bits(v[0], b0), bits(v[1], b1), k == 3 ? c : -1);
        memset(v, 0, sizeof v), n = -1, c = 0;
        k = sscanf(line, "%lf,%lf,%lf%c", &v[0], &v[1], &v[2], &c);
        printf("3c %d %s %s %s %d\n", k, bits(v[0], b0), bits(v[1], b1), bits(v[2], b2), k == 4 ? c : -1);
        memset(v, 0, sizeof v), n = -1;
        k = sscanf(line, "%lf,%lf,%lf,%lf", &v[0], &v[1], &v[2], &v[3]);
        printf("4 %d %s %s %s %s\n", k, bits(v[0], b0), bits(v[1], b1), bits(v[2], b2), bits(v[3], b3));
        double w[5] = {0, 0, 0, 0, 0};
        char b4[20];
        k = sscanf(line, "%lf,%lf,%lf,%lf,%lf", &w[0], &w[1], &w[2], &w[3], &w[4]);
        printf("5 %d %s %s %s %s %s\n", k, bits(w[0], b0), bits(w[1], b1), bits(w[2], b2), bits(w[3], b3),
               bits(w[4], b4));
    }
    return 0;
}
