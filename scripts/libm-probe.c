// Measures libm results for docs/porting/libm.md: Apple libm vs MPFR (correctly rounded).
//
// Build (macOS, `brew install mpfr`):
//   clang -O2 -ffp-contract=off -o /tmp/libm-probe scripts/libm-probe.c \
//     -I/opt/homebrew/opt/mpfr/include -I/opt/homebrew/opt/gmp/include \
//     -L/opt/homebrew/opt/mpfr/lib -L/opt/homebrew/opt/gmp/lib -lmpfr -lgmp
// Usage:
//   libm-probe gen  <fn> <n> <out.bin>                  random inputs (pairs of f64)
//   libm-probe eval <fn> <in.bin> <apple.bin> <cr.bin>  system and correctly rounded results
// Input files are pairs of little-endian f64 (y = 0 for one-argument functions), as written
// by the `math-trace` feature of gv-core (`GV_MATH_TRACE=dir`). fn is one of
// hypot pow atan2 sin cos tan asin acos exp log log2 cbrt.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <stdint.h>
#include <mpfr.h>

static uint64_t s = 0x9e3779b97f4a7c15ull;
static uint64_t rnd(void) { s ^= s << 13; s ^= s >> 7; s ^= s << 17; return s; }
static double u01(void) { return (rnd() >> 11) * 0x1p-53; }
static double logu(double lo, double hi) { return exp2(lo + (hi - lo) * u01()); }
static double sgn(double x) { return (rnd() & 1) ? -x : x; }

static int fid(const char *n) {
  const char *names[] = {"hypot","pow","atan2","sin","cos","tan","asin","acos","exp","log","log2","cbrt"};
  for (int i = 0; i < 12; i++) if (!strcmp(n, names[i])) return i;
  fprintf(stderr, "bad fn %s\n", n); exit(2);
}
static double apple(int f, double x, double y) {
  switch (f) {
  case 0: return hypot(x, y); case 1: return pow(x, y); case 2: return atan2(x, y);
  case 3: return sin(x); case 4: return cos(x); case 5: return tan(x); case 6: return asin(x);
  case 7: return acos(x); case 8: return exp(x); case 9: return log(x); case 10: return log2(x);
  default: return cbrt(x);
  }
}
static double cr(int f, double x, double y) {
  mpfr_t a, b, r; mpfr_inits2(53, a, b, r, (mpfr_ptr)0);
  mpfr_set_d(a, x, MPFR_RNDN); mpfr_set_d(b, y, MPFR_RNDN);
  int t;
  switch (f) {
  case 0: t = mpfr_hypot(r, a, b, MPFR_RNDN); break;
  case 1: t = mpfr_pow(r, a, b, MPFR_RNDN); break;
  case 2: t = mpfr_atan2(r, a, b, MPFR_RNDN); break;
  case 3: t = mpfr_sin(r, a, MPFR_RNDN); break;
  case 4: t = mpfr_cos(r, a, MPFR_RNDN); break;
  case 5: t = mpfr_tan(r, a, MPFR_RNDN); break;
  case 6: t = mpfr_asin(r, a, MPFR_RNDN); break;
  case 7: t = mpfr_acos(r, a, MPFR_RNDN); break;
  case 8: t = mpfr_exp(r, a, MPFR_RNDN); break;
  case 9: t = mpfr_log(r, a, MPFR_RNDN); break;
  case 10: t = mpfr_log2(r, a, MPFR_RNDN); break;
  default: t = mpfr_cbrt(r, a, MPFR_RNDN); break;
  }
  t = mpfr_subnormalize(r, t, MPFR_RNDN);
  double d = mpfr_get_d(r, MPFR_RNDN);
  mpfr_clears(a, b, r, (mpfr_ptr)0);
  return d;
}
static void gen(int f, double *x, double *y) {
  *y = 0;
  switch (f) {
  case 0: *x = sgn(logu(-40, 40)); *y = (rnd() & 3) ? sgn(*x * logu(-30, 30)) : sgn(logu(-40, 40)); break;
  case 1: *x = logu(-30, 30); *y = (rnd() & 1) ? (u01() * 6 - 3) : sgn(logu(-10, 6)); break;
  case 2: *x = sgn(logu(-30, 30)); *y = sgn(logu(-30, 30)); break;
  case 3: case 4: case 5: *x = (rnd() & 3) ? (u01() * 20 - 10) : sgn(logu(-30, 20)); break;
  case 6: case 7: *x = (rnd() & 1) ? (u01() * 2 - 1) : sgn(logu(-30, 0)); break;
  case 8: *x = (rnd() & 1) ? (u01() * 40 - 20) : sgn(logu(-30, 9)); break;
  case 9: case 10: *x = logu(-60, 60); break;
  default: *x = sgn(logu(-60, 60)); break;
  }
}
int main(int argc, char **argv) {
  mpfr_set_emin(-1073); mpfr_set_emax(1024);
  if (!strcmp(argv[1], "gen")) {
    int f = fid(argv[2]); long n = atol(argv[3]); FILE *o = fopen(argv[4], "wb");
    for (long i = 0; i < n; i++) { double v[2]; gen(f, &v[0], &v[1]); fwrite(v, 8, 2, o); }
    fclose(o); return 0;
  }
  int f = fid(argv[2]);
  FILE *in = fopen(argv[3], "rb"), *oa = fopen(argv[4], "wb"), *oc = fopen(argv[5], "wb");
  double v[2];
  while (fread(v, 8, 2, in) == 2) {
    double a = apple(f, v[0], v[1]), c = cr(f, v[0], v[1]);
    fwrite(&a, 8, 1, oa); fwrite(&c, 8, 1, oc);
  }
  fclose(oa); fclose(oc); return 0;
}
