/* Prints what the C library's strtod and sscanf("%lf") make of each input line, for the
 * table in crates/graphviz-pure-core/src/cnum.rs (tests). One output line per input line:
 *
 *   <strtod bits, hex> <strtod bytes consumed> <sscanf count> <sscanf bytes consumed>
 *   [<sscanf value bits>] <sscanf("%f") float bits, or "-">
 *
 * NaN is printed as its sign and "nan" (the payload is not observable in Graphviz output).
 *
 *   cc scripts/strtod-probe.c -o /tmp/strtod-probe && /tmp/strtod-probe < inputs.txt
 */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

static void show(double v) {
    if (isnan(v)) {
        printf("%snan", signbit(v) ? "-" : "+");
        return;
    }
    uint64_t bits;
    memcpy(&bits, &v, sizeof bits);
    printf("%016llx", (unsigned long long)bits);
}

int main(void) {
    char line[4096];
    while (fgets(line, sizeof line, stdin)) {
        line[strcspn(line, "\n")] = 0;
        char *end;
        double v = strtod(line, &end);
        show(v);
        printf(" %ld", (long)(end - line));
        double w = 0;
        int n = -1;
        int k = sscanf(line, "%lf%n", &w, &n);
        printf(" %d %d", k, k == 1 ? n : 0);
        if (k == 1) {
            printf(" ");
            show(w);
        }
        float f = 0;
        if (sscanf(line, "%f", &f) == 1) {
            uint32_t fb;
            memcpy(&fb, &f, sizeof fb);
            if (isnan(f))
                printf(" %snan", signbit(f) ? "-" : "+");
            else
                printf(" %08x", fb);
        } else
            printf(" -");
        printf("\n");
    }
    return 0;
}
