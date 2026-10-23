//! Attribute parsing helpers with C semantics (`lib/common/utils.c`):
//! `strtol` prefix parsing, `late_*`, `mapbool` (`strtod` is `graphviz_pure_core::strtod`).

use graphviz_pure_core::AttrMap;

/// `strtol(p, &end, 10)` / `atoi` / `sscanf("%d")`.
pub fn parse_leading_int(s: &str) -> Option<i64> {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == ds {
        return None;
    }
    t[..i].parse::<i64>().ok()
}

/// `atoi`: 0 when nothing parses.
pub fn atoi(s: &str) -> i64 {
    parse_leading_int(s).unwrap_or(0)
}

/// `mapBool(p, dflt)`.
pub fn map_bool(p: Option<&str>, dflt: bool) -> bool {
    let Some(p) = p else { return dflt };
    if p.is_empty() {
        return dflt;
    }
    if p.eq_ignore_ascii_case("false") || p.eq_ignore_ascii_case("no") {
        return false;
    }
    if p.eq_ignore_ascii_case("true") || p.eq_ignore_ascii_case("yes") {
        return true;
    }
    if p.as_bytes()[0].is_ascii_digit() {
        return atoi(p) != 0;
    }
    dflt
}

/// `mapbool(p)`.
pub fn mapbool(p: Option<&str>) -> bool {
    map_bool(p, false)
}

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
    if d == 0.0 { 72.0 } else { d }
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
pub fn late_int(attrs: &AttrMap, key: &str, def: i64, low: i64) -> i64 {
    match attrs.get(key) {
        None => def,
        Some(s) if s.is_empty() => def,
        Some(s) => match parse_leading_int(s) {
            None => def,
            Some(v) if v > i64::from(i32::MAX) => def,
            Some(v) if v < low => low,
            Some(v) => v,
        },
    }
}

/// `late_nnstring`: non-empty value or the default.
pub fn late_nnstring<'a>(attrs: &'a AttrMap, key: &str, def: &'a str) -> &'a str {
    match attrs.get(key) {
        Some(s) if !s.is_empty() => s,
        _ => def,
    }
}

/// `ROUND(f)` from `arith.h`: round half away from zero to an int.
pub fn c_round(f: f64) -> i64 {
    if f >= 0.0 {
        (f + 0.5) as i64
    } else {
        (f - 0.5) as i64
    }
}

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
