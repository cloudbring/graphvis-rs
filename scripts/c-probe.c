/* Prints C Graphviz's exact (%.17g) node geometry after neato -n2 for a laid-out
 * DOT file, to compare floating-point values with the port.
 *
 *   I=~/src/gv-ref/build/inst
 *   cc -I$I/include/graphviz scripts/c-probe.c -L$I/lib -lgvc -lcgraph -lcdt -o /tmp/c-probe
 *   DYLD_LIBRARY_PATH=$I/lib GVBINDIR=$I/lib/graphviz /tmp/c-probe laid.dot [node]
 */
#include <stdio.h>
#include <string.h>
#include <gvc.h>
#include <types.h>

int main(int argc, char **argv) {
    GVC_t *gvc = gvContext();
    FILE *f = fopen(argv[1], "r");
    graph_t *g = agread(f, 0);
    gvLayout(gvc, g, "nop2");
    printf("bb %.17g %.17g %.17g %.17g\n", GD_bb(g).LL.x, GD_bb(g).LL.y, GD_bb(g).UR.x, GD_bb(g).UR.y);
    for (node_t *n = agfstnode(g); n; n = agnxtnode(g, n)) {
        if (argc > 2 && strcmp(agnameof(n), argv[2])) continue;
        printf("node %s coord %.17g %.17g w %.17g h %.17g lw %.17g ht %.17g\n", agnameof(n),
               ND_coord(n).x, ND_coord(n).y, ND_width(n), ND_height(n), ND_lw(n), ND_ht(n));
        if (!ND_shape(n)->polygon) {
            field_t *fl = ND_shape_info(n);
            printf("  rec b %.17g %.17g %.17g %.17g\n", fl->b.LL.x, fl->b.LL.y, fl->b.UR.x, fl->b.UR.y);
            continue;
        }
        polygon_t *p = ND_shape_info(n);
        for (size_t i = 0; i < p->sides * (p->peripheries ? p->peripheries : 1); i++)
            printf("  v %.17g %.17g\n", p->vertices[i].x, p->vertices[i].y);
    }
    return 0;
}
