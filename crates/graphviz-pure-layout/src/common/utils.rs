//! Attribute parsing helpers with C semantics (`lib/common/utils.c`):
//! `strtol` prefix parsing, `late_*`, `mapbool` (`strtod` is `graphviz_pure_core::strtod`).

use graphviz_pure_core::AttrMap;
use graphviz_pure_core::cnum;
use std::collections::HashMap;

/// `strtol(p, &end, 10)` or `sscanf("%ld")`: `None` when there are no digits; saturates at
/// the range of `long`.
pub fn parse_leading_int(s: &str) -> Option<i64> {
    let (v, n) = cnum::strtol(s);
    (n > 0).then_some(v)
}

pub use graphviz_pure_core::cnum::{atoi, map_bool, mapbool, scan_d};

thread_local! {
    /// `PSinputscale` as `dot -s` set it (0 when not set).
    static PS_INPUTSCALE: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Sets `PSinputscale` (`dot -s`) for the layouts that follow on this thread.
pub fn set_ps_inputscale(scale: f64) {
    PS_INPUTSCALE.with(|s| s.set(scale));
}

/// `get_inputscale(g)` (`lib/common/utils.c`): `dot -s` when set, else the `inputscale`
/// attribute (low 0), 72 for 0, -1 when absent.
pub fn get_inputscale(attrs: &AttrMap) -> f64 {
    let ps = PS_INPUTSCALE.with(std::cell::Cell::get);
    if ps > 0.0 {
        return ps;
    }
    let d = late_double(attrs, "inputscale", -1.0, 0.0);
    // is_exactly_zero: `-0` is kept, and then does not scale.
    if cnum::is_exactly_zero(d) { 72.0 } else { d }
}

/// `late_double` on an attribute map (absent or empty means "use default").
pub fn late_double(attrs: &AttrMap, key: &str, def: f64, low: f64) -> f64 {
    match attrs.get(key) {
        None => def,
        Some(s) if s.is_empty() => def,
        Some(s) => match graphviz_pure_core::strtod::strtod_rest(s) {
            None => def,
            Some((v, _)) if v < low => low,
            Some((v, _)) => v,
        },
    }
}

/// `late_int` on an attribute map.
pub fn late_int(attrs: &AttrMap, key: &str, def: i32, low: i32) -> i32 {
    cnum::late_int(attrs.get(key).map(String::as_str), def, low)
}

/// `late_nnstring`: non-empty value or the default.
pub fn late_nnstring<'a>(attrs: &'a AttrMap, key: &str, def: &'a str) -> &'a str {
    match attrs.get(key) {
        Some(s) if !s.is_empty() => s,
        _ => def,
    }
}

pub use graphviz_pure_core::cnum::c_round;

/// `compute_bb(g)` for a subgraph view: nodes, edge control points and
/// positioned labels, top-level cluster boxes and (for the root) the graph
/// label if already placed. Stores the result in `sub.bb` (and the root's
/// `g.bb` when `is_root`). Like C, leaves the box untouched when the graph
/// has no nodes and no clusters.
pub fn compute_bb(g: &mut super::types::LGraph<'_>, sub: &mut super::types::Sub, is_root: bool) {
    use super::geom::{expandbb, expandbp};
    use super::splines::add_label_bb;
    use graphviz_pure_core::{BoundingBox, Point};
    if sub.nodes.is_empty() && sub.clusters.is_empty() {
        return;
    }
    let m = f64::from(i32::MAX);
    let mut bb = BoundingBox::new(m, m, -m, -m);
    for &n in &sub.nodes {
        let nd = &g.nodes[n];
        let c = coord(g, n);
        let s2 = Point::new((nd.lw + nd.rw) / 2.0, nd.ht / 2.0);
        let b = BoundingBox::new(c.x - s2.x, c.y - s2.y, c.x + s2.x, c.y + s2.y);
        expandbb(&mut bb, &b);
        if let Some(l) = nd.xlabel.as_ref().filter(|l| l.set) {
            add_label_bb(&mut bb, l, false);
        }
        for e in g.outs(sub, n) {
            let ed = &g.edges[e];
            let Some(spl) = ed.spl.as_ref() else { continue };
            for b in spl {
                for p in &b.list {
                    expandbp(&mut bb, *p);
                }
            }
            for l in [&ed.label, &ed.head_label, &ed.tail_label, &ed.xlabel]
                .into_iter()
                .flatten()
            {
                if l.set {
                    add_label_bb(&mut bb, l, false);
                }
            }
        }
    }
    for &c in &sub.clusters {
        expandbb(&mut bb, &g.clusters[c].bb);
    }
    if is_root && let Some(l) = g.label.as_ref().filter(|l| l.set) {
        add_label_bb(&mut bb, l, false);
    }
    sub.bb = bb;
    if is_root {
        g.bb = bb;
    }
}

/// `coord(n)`: `ND_pos` scaled to points.
pub fn coord(g: &super::types::LGraph<'_>, n: usize) -> graphviz_pure_core::Point {
    let p = &g.nodes[n].pos;
    graphviz_pure_core::Point::new(72.0 * p[0], 72.0 * p[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strtol_prefixes_and_mapbool() {
        assert_eq!(parse_leading_int("12,3"), Some(12));
        assert!(mapbool(Some("TRUE")));
        assert!(mapbool(Some("2")));
        assert!(!mapbool(Some("x")));
    }
}

/// `mkClustMap`'s dictionary: cluster name → the first cluster with that name.
///
/// `agnameof` of an anonymous cluster (`%<id>`) is one static buffer, and `fillMap` stores
/// that pointer as the key. Every later anonymous cluster matches the first one, and once the
/// map is built the key reads as the last anonymous name `fillMap` saw: `findCluster` finds the
/// first anonymous cluster by that name only. (C's buffer also moves when a warning names a
/// node whose `%`-name was renumbered; the port keeps fillMap's last name.)
#[derive(Debug)]
pub struct ClustMap<T> {
    named: HashMap<String, T>,
    anon: Option<(String, T)>,
}

impl<T: Copy> ClustMap<T> {
    /// `findCluster(map, name)`.
    pub fn get(&self, name: &str) -> Option<T> {
        match &self.anon {
            Some((last, cl)) if last == name => Some(*cl),
            _ => self.named.get(name).copied(),
        }
    }
}

/// `mkClustMap` / `fillMap`, from the clusters' `agnameof` names in `fillMap`'s preorder. The
/// first cluster of a name wins; each later one warns. A name in agnameof's static buffer is
/// the one key every anonymous cluster shares. In a pack component the clusters are
/// `projectG`'s copies, whose `%<id>` names are strings of their own
/// ([`graphviz_pure_core::Graph::local_subgraph_names_kept`]).
pub fn mk_clust_map<'a, T>(
    clusters: impl IntoIterator<Item = (graphviz_pure_core::AgName<'a>, T)>,
) -> ClustMap<T> {
    let mut map = ClustMap {
        named: HashMap::new(),
        anon: None,
    };
    for (agname, cl) in clusters {
        let buffered = agname.buffer;
        let name = agname.name.into_owned();
        let taken = if buffered {
            match &mut map.anon {
                Some((last, _)) => {
                    last.clone_from(&name);
                    true
                }
                None => {
                    map.anon = Some((name.clone(), cl));
                    false
                }
            }
        } else if map.named.contains_key(&name) {
            true
        } else {
            map.named.insert(name.clone(), cl);
            false
        };
        if taken {
            graphviz_pure_core::diag::agwarningf(&format!(
                "Two clusters named {name} - the second will be ignored\n"
            ));
        }
    }
    map
}
