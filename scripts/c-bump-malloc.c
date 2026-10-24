// A bump allocator to interpose into C Graphviz `dot` on macOS: every allocation lies at a
// higher address than the one before, and free never reuses memory, so objects are ordered
// by address in creation order. With it (and address randomisation off), C's output on
// inputs whose result depends on pointer order (VPSC's ties, #225) is deterministic, and is
// what the port's creation order gives. Used by scripts/vpsc-ties-measure.py:
//
//   cc -O2 -dynamiclib -o target/c-bump-malloc.dylib scripts/c-bump-malloc.c
//
// It covers malloc, calloc, realloc, free, posix_memalign, aligned_alloc, valloc,
// malloc_size, and the malloc_type_* entry points libc++'s operator new calls.
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <malloc/malloc.h>
#include <stdint.h>
#define REGION (8ULL<<30)
static char *base, *cur;
static void init(void){ if(!base){ base=mmap(0,REGION,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON|MAP_NORESERVE,-1,0); cur=base; } }
static int mine(void*p){ return base && (char*)p>=base && (char*)p<base+REGION; }
static void *balloc(size_t n,size_t al){ init(); if(al<16)al=16; uintptr_t c=(uintptr_t)cur+16; c=(c+al-1)&~(uintptr_t)(al-1); ((size_t*)c)[-1]=n; cur=(char*)(c+n); return (void*)c; }
static void *my_malloc(size_t n){ return balloc(n,16); }
static void *my_calloc(size_t a,size_t b){ return balloc(a*b,16); }
static void my_free(void*p){ if(!p||mine(p))return; free(p); }
static size_t my_size(const void*p){ if(mine((void*)p)) return ((size_t*)p)[-1]; return malloc_size(p); }
static void *my_realloc(void*p,size_t n){ if(!p) return balloc(n,16); size_t o=my_size(p); void*q=balloc(n,16); memcpy(q,p,o<n?o:n); my_free(p); return q; }
static int my_pm(void**r,size_t al,size_t n){ *r=balloc(n,al); return 0; }
static void *my_aa(size_t al,size_t n){ return balloc(n,al); }
static void *my_valloc(size_t n){ return balloc(n,16384); }
#define I(a,b) __attribute__((used)) static struct{const void*n;const void*o;} _i_##b __attribute__((section("__DATA,__interpose")))={(const void*)a,(const void*)b};
I(my_malloc,malloc) I(my_calloc,calloc) I(my_free,free) I(my_realloc,realloc) I(my_pm,posix_memalign) I(my_aa,aligned_alloc) I(my_valloc,valloc) I(my_size,malloc_size)
typedef unsigned long long mtid;
extern void *malloc_type_malloc(size_t,mtid); extern void *malloc_type_calloc(size_t,size_t,mtid);
extern void *malloc_type_realloc(void*,size_t,mtid); extern void *malloc_type_aligned_alloc(size_t,size_t,mtid);
extern int malloc_type_posix_memalign(void**,size_t,size_t,mtid); extern void *malloc_type_valloc(size_t,mtid);
static void *t_malloc(size_t n,mtid t){ return balloc(n,16); }
static void *t_calloc(size_t a,size_t b,mtid t){ return balloc(a*b,16); }
static void *t_realloc(void*p,size_t n,mtid t){ return my_realloc(p,n); }
static void *t_aa(size_t al,size_t n,mtid t){ return balloc(n,al); }
static int t_pm(void**r,size_t al,size_t n,mtid t){ *r=balloc(n,al); return 0; }
static void *t_valloc(size_t n,mtid t){ return balloc(n,16384); }
I(t_malloc,malloc_type_malloc) I(t_calloc,malloc_type_calloc) I(t_realloc,malloc_type_realloc) I(t_aa,malloc_type_aligned_alloc) I(t_pm,malloc_type_posix_memalign) I(t_valloc,malloc_type_valloc)
#include <stdio.h>
__attribute__((destructor)) static void fin(void){ if(getenv("BUMPDBG")) fprintf(stderr,"bump used %zu bytes\n",(size_t)(cur-base)); }
