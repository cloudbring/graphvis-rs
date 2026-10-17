#include <stdio.h>
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
        printf("node %s coord %.17g %.17g w %.17g h %.17g lw %.17g ht %.17g\n", agnameof(n), ND_coord(n).x, ND_coord(n).y, ND_width(n), ND_height(n), ND_lw(n), ND_ht(n));
        polygon_t *p = ND_shape_info(n);
        if (p && ND_shape(n)->polygon) for (size_t i = 0; i < p->sides * (p->peripheries ? p->peripheries : 1); i++) printf("  v %.17g %.17g\n", p->vertices[i].x, p->vertices[i].y);
    }
    return 0;
}
