//! Port of `gv_postprocess` / `dotneato_postprocess` (lib/common/postproc.c):
//! cluster label placement, room for the root graph label, and the final
//! translation/rotation from rank-direction layout space into drawing space.
//! (`addXLabels` is phase 3.)

use super::Dot;
use super::init::{ccwrotatepf, pad};
use super::types::*;
use graphviz_pure_core::Point;

impl Dot<'_> {
    fn map_point(&self, p: Point, rankdir: i32, offset: Point) -> Point {
        let q = ccwrotatepf(p, rankdir * 90);
        Point::new(q.x - offset.x, q.y - offset.y)
    }

    fn translate_bb(&mut self, g: G, rankdir: i32, offset: Point) {
        let bb = self.graphs[g].bb;
        let new_bb = if rankdir == RANKDIR_LR || rankdir == RANKDIR_BT {
            BoxF {
                ll: self.map_point(Point::new(bb.ll.x, bb.ur.y), rankdir, offset),
                ur: self.map_point(Point::new(bb.ur.x, bb.ll.y), rankdir, offset),
            }
        } else {
            BoxF {
                ll: self.map_point(bb.ll, rankdir, offset),
                ur: self.map_point(bb.ur, rankdir, offset),
            }
        };
        self.graphs[g].bb = new_bb;
        if let Some(l) = self.graphs[g].label {
            self.labels[l].pos = self.map_point(self.labels[l].pos, rankdir, offset);
        }
        for c in self.graphs[g].clust.clone() {
            self.translate_bb(c, rankdir, offset);
        }
    }

    /// `map_edge(e)`.
    fn map_edge(&mut self, e: E, rankdir: i32, offset: Point) {
        let Some(mut s) = self.edges[e].spl.take() else {
            if !self.concentrate && self.edges[e].edge_type != IGNORED {
                let (t, h) = (self.edges[e].tail, self.edges[e].head);
                // postproc.c:104: both names share agnameof's static buffer when anonymous.
                let [t, h] = graphviz_pure_core::agnameof_buffer([
                    self.node_agnameof(t),
                    self.node_agnameof(h),
                ]);
                graphviz_pure_core::diag::agerrorf(&format!("lost {t} {h} edge\n"));
            }
            return;
        };
        for bz in &mut s {
            for p in &mut bz.list {
                *p = self.map_point(*p, rankdir, offset);
            }
            if bz.sflag != 0 {
                bz.sp = self.map_point(bz.sp, rankdir, offset);
            }
            if bz.eflag != 0 {
                bz.ep = self.map_point(bz.ep, rankdir, offset);
            }
        }
        self.edges[e].spl = Some(s);
        let ed = &self.edges[e];
        for l in [ed.label, ed.xlabel, ed.head_label, ed.tail_label]
            .into_iter()
            .flatten()
        {
            self.labels[l].pos = self.map_point(self.labels[l].pos, rankdir, offset);
        }
    }

    /// `translate_drawing(g)`.
    fn translate_drawing(&mut self, rankdir: i32, offset: Point) {
        let shift = offset.x != 0.0 || offset.y != 0.0;
        if !shift && rankdir == 0 {
            return;
        }
        for n in self.nodes_of(0) {
            self.nodes[n].coord = self.map_point(self.nodes[n].coord, rankdir, offset);
            if let Some(l) = self.nodes[n].xlabel {
                self.labels[l].pos = self.map_point(self.labels[l].pos, rankdir, offset);
            }
            if self.splines_done {
                for e in self.out_edges(0, n) {
                    self.map_edge(e, rankdir, offset);
                }
            }
        }
        self.translate_bb(0, rankdir, offset);
    }

    fn place_root_label(&mut self, d: Point) {
        let g = 0;
        let bb = self.graphs[g].bb;
        let lp = self.graphs[g].label_pos;
        let x = if lp & LABEL_AT_RIGHT != 0 {
            bb.ur.x - d.x / 2.0
        } else if lp & LABEL_AT_LEFT != 0 {
            bb.ll.x + d.x / 2.0
        } else {
            (bb.ll.x + bb.ur.x) / 2.0
        };
        let y = if lp & LABEL_AT_TOP != 0 {
            bb.ur.y - d.y / 2.0
        } else {
            bb.ll.y + d.y / 2.0
        };
        let l = self.graphs[g].label.unwrap();
        self.labels[l].pos = Point::new(x, y);
        self.labels[l].set = true;
    }

    fn place_flip_graph_label(&mut self, g: G) {
        if g != 0
            && let Some(l) = self.graphs[g].label
            && !self.labels[l].set
        {
            let bb = self.graphs[g].bb;
            let lp = self.graphs[g].label_pos;
            let (d, x) = if lp & LABEL_AT_TOP != 0 {
                let d = self.graphs[g].border[RIGHT_IX];
                (d, bb.ur.x - d.x / 2.0)
            } else {
                let d = self.graphs[g].border[LEFT_IX];
                (d, bb.ll.x + d.x / 2.0)
            };
            let y = if lp & LABEL_AT_RIGHT != 0 {
                bb.ll.y + d.y / 2.0
            } else if lp & LABEL_AT_LEFT != 0 {
                bb.ur.y - d.y / 2.0
            } else {
                (bb.ll.y + bb.ur.y) / 2.0
            };
            self.labels[l].pos = Point::new(x, y);
            self.labels[l].set = true;
        }
        for c in self.graphs[g].clust.clone() {
            self.place_flip_graph_label(c);
        }
    }

    pub fn place_graph_label(&mut self, g: G) {
        if g != 0
            && let Some(l) = self.graphs[g].label
            && !self.labels[l].set
        {
            let bb = self.graphs[g].bb;
            let lp = self.graphs[g].label_pos;
            let (d, y) = if lp & LABEL_AT_TOP != 0 {
                let d = self.graphs[g].border[TOP_IX];
                (d, bb.ur.y - d.y / 2.0)
            } else {
                let d = self.graphs[g].border[BOTTOM_IX];
                (d, bb.ll.y + d.y / 2.0)
            };
            let x = if lp & LABEL_AT_RIGHT != 0 {
                bb.ur.x - d.x / 2.0
            } else if lp & LABEL_AT_LEFT != 0 {
                bb.ll.x + d.x / 2.0
            } else {
                (bb.ll.x + bb.ur.x) / 2.0
            };
            self.labels[l].pos = Point::new(x, y);
            self.labels[l].set = true;
        }
        for c in self.graphs[g].clust.clone() {
            self.place_graph_label(c);
        }
    }

    /// `gv_postprocess(g, 1)`
    pub fn dotneato_postprocess(&mut self) {
        let g = 0;
        let rankdir = self.graphs[g].rankdir;
        let flip = self.graphs[g].flip();
        if flip {
            self.place_flip_graph_label(g);
        } else {
            self.place_graph_label(g);
        }
        self.add_xlabels();
        let mut dimen = Point::new(0.0, 0.0);
        if let Some(l) = self.graphs[g].label
            && !self.labels[l].set
        {
            dimen = pad(self.labels[l].dimen);
            let lp = self.graphs[g].label_pos;
            let bb = &mut self.graphs[g].bb;
            if flip {
                if lp & LABEL_AT_TOP != 0 {
                    bb.ur.x += dimen.y;
                } else {
                    bb.ll.x -= dimen.y;
                }
                if dimen.x > bb.ur.y - bb.ll.y {
                    let diff = (dimen.x - (bb.ur.y - bb.ll.y)) / 2.0;
                    bb.ll.y -= diff;
                    bb.ur.y += diff;
                }
            } else {
                if lp & LABEL_AT_TOP != 0 {
                    if rankdir == RANKDIR_TB {
                        bb.ur.y += dimen.y;
                    } else {
                        bb.ll.y -= dimen.y;
                    }
                } else if rankdir == RANKDIR_TB {
                    bb.ll.y -= dimen.y;
                } else {
                    bb.ur.y += dimen.y;
                }
                if dimen.x > bb.ur.x - bb.ll.x {
                    let diff = (dimen.x - (bb.ur.x - bb.ll.x)) / 2.0;
                    bb.ll.x -= diff;
                    bb.ur.x += diff;
                }
            }
        }
        let bb = self.graphs[g].bb;
        let offset = match rankdir {
            RANKDIR_LR => Point::new(-bb.ur.y, bb.ll.x),
            RANKDIR_BT => Point::new(bb.ll.x, -bb.ur.y),
            RANKDIR_RL => Point::new(bb.ll.y, bb.ll.x),
            _ => bb.ll,
        };
        self.translate_drawing(rankdir, offset);
        if let Some(l) = self.graphs[g].label
            && !self.labels[l].set
        {
            self.place_root_label(dimen);
        }
    }

    /// `addXLabels(gp)`, gated on `GD_has_labels` and `EdgeLabelsDone` as in C.
    fn add_xlabels(&mut self) {
        let hl = self.graphs[0].has_labels;
        if hl & (NODE_XLABEL | EDGE_XLABEL | TAIL_LABEL | HEAD_LABEL) == 0
            && (hl & EDGE_LABEL == 0 || self.splines_done)
        {
            return;
        }
        crate::common::xlabels::add_xlabels(self);
    }

    /// `removeFill`: delete the newrank fill nodes before splines.
    pub fn remove_fill(&mut self) {
        let Some(sg) = self.graphs[0]
            .subgs
            .iter()
            .copied()
            .find(|&s| self.graphs[s].name == "_new_rank")
        else {
            return;
        };
        for n in self.nodes_of(sg) {
            self.delete_fast_node(0, n);
            let r = self.nodes[n].rank;
            let cnt = self.rank_n(0, r);
            let mut j = 0;
            while j < cnt {
                if self.rv(0, r, j) == Some(n) {
                    let mut k = j + 1;
                    while k < cnt {
                        let v = self.rv(0, r, k);
                        self.set_rv(0, r, k - 1, v);
                        k += 1;
                    }
                    self.rk_mut(0, r).n -= 1;
                    break;
                }
                j += 1;
            }
            self.agdelnode(0, n);
        }
        self.cg_delsubg(0, sg);
    }
}
