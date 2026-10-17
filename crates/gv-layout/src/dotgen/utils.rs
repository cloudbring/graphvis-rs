//! Ports of the helpers dotgen uses from `lib/common/utils.c` (attribute
//! readers with C defaults, union-find) and `lib/util` (`scale_clamp`).

use super::Dot;
use super::types::*;

/// `strtol(p, &end, 10)` prefix parse: `None` when no digits were consumed.
pub fn strtol_prefix(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let start = i;
    let mut v: i64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        v = v.saturating_mul(10).saturating_add((b[i] - b'0') as i64);
        i += 1;
    }
    if i == start {
        return None;
    }
    Some(if neg { -v } else { v })
}

/// `strtod` prefix parse: `None` when nothing was consumed.
pub fn strtod_prefix(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let lower = t[i..].to_ascii_lowercase();
    if lower.starts_with("inf") || lower.starts_with("nan") {
        let neg = t.starts_with('-');
        let v = if lower.starts_with("nan") {
            f64::NAN
        } else {
            f64::INFINITY
        };
        return Some(if neg { -v } else { v });
    }
    let mut digits = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    let mant_end = i;
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let ds = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > ds {
            i = j;
        }
    }
    let txt = &t[..i];
    txt.parse::<f64>()
        .ok()
        .or_else(|| t[..mant_end].parse::<f64>().ok())
}

/// `atof`
pub fn atof(s: &str) -> f64 {
    strtod_prefix(s).unwrap_or(0.0)
}

/// `atoi`
pub fn atoi(s: &str) -> i32 {
    strtol_prefix(s).map(|v| v as i32).unwrap_or(0)
}

/// `late_int` on an attribute value (`None` = attribute absent).
pub fn late_int(v: Option<&str>, default: i32, minimum: i32) -> i32 {
    let Some(p) = v else { return default };
    if p.is_empty() {
        return default;
    }
    let Some(rv) = strtol_prefix(p) else {
        return default;
    };
    if rv > i32::MAX as i64 {
        return default;
    }
    if rv < minimum as i64 {
        return minimum;
    }
    rv as i32
}

/// `late_double`
pub fn late_double(v: Option<&str>, default: f64, minimum: f64) -> f64 {
    let Some(p) = v else { return default };
    if p.is_empty() {
        return default;
    }
    let Some(rv) = strtod_prefix(p) else {
        return default;
    };
    if rv < minimum {
        return minimum;
    }
    rv
}

/// `mapBool`
pub fn map_bool(p: Option<&str>, default: bool) -> bool {
    let Some(p) = p else { return default };
    if p.is_empty() {
        return default;
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
    default
}

/// `mapbool`
pub fn mapbool(p: Option<&str>) -> bool {
    map_bool(p, false)
}

/// `scale_clamp` (lib/util/gv_math.h)
pub fn scale_clamp(original: i32, scale: f64) -> i32 {
    if scale < 0.0 {
        return 0;
    }
    if scale > 1.0 && (original as f64) > (i32::MAX as f64) / scale {
        return i32::MAX;
    }
    (original as f64 * scale) as i32
}

/// `ROUND(f)` from lib/common/macros.h.
#[inline]
pub fn round_c(f: f64) -> i32 {
    if f >= 0.0 {
        (f + 0.5) as i32
    } else {
        (f - 0.5) as i32
    }
}

/// `POINTS(a_inches)` = `ROUND(a * 72)`.
#[inline]
pub fn points(inches: f64) -> i32 {
    round_c(inches * 72.0)
}

impl Dot<'_> {
    /// `agget(g, name)`
    pub fn agget(&self, g: G, name: &str) -> Option<&str> {
        self.graphs[g].attrs.get(name).map(String::as_str)
    }

    /// `agxget(n, sym)` for a node.
    pub fn node_attr(&self, n: N, name: &str) -> Option<&str> {
        let r = self.nodes[n].real?;
        self.input.nodes[r].attrs.get(name).map(String::as_str)
    }

    /// `agxget(e, sym)` for an edge.
    pub fn edge_attr(&self, e: E, name: &str) -> Option<&str> {
        let r = self.edges[e].real?;
        self.input.edges[r].attrs.get(name).map(String::as_str)
    }

    /// `is_a_cluster(g)`
    pub fn is_a_cluster(&self, g: G) -> bool {
        if g == 0 {
            return true;
        }
        let name = &self.graphs[g].name;
        (name.len() >= 7 && name[..7].eq_ignore_ascii_case("cluster"))
            || mapbool(self.agget(g, "cluster"))
    }

    /// `nonconstraint_edge(e)` (class1.c)
    pub fn nonconstraint_edge(&self, e: E) -> bool {
        if let Some(c) = self.edge_attr(e, "constraint")
            && !c.is_empty()
            && !mapbool(Some(c))
        {
            return true;
        }
        false
    }

    pub fn uf_find(&mut self, mut n: N) -> N {
        while let Some(p) = self.nodes[n].uf_parent {
            if p == n {
                break;
            }
            if let Some(pp) = self.nodes[p].uf_parent {
                self.nodes[n].uf_parent = Some(pp);
            }
            n = self.nodes[n].uf_parent.unwrap();
        }
        n
    }

    pub fn uf_union(&mut self, mut u: N, mut v: N) -> N {
        if u == v {
            return u;
        }
        if self.nodes[u].uf_parent.is_none() {
            self.nodes[u].uf_parent = Some(u);
            self.nodes[u].uf_size = 1;
        } else {
            u = self.uf_find(u);
        }
        if self.nodes[v].uf_parent.is_none() {
            self.nodes[v].uf_parent = Some(v);
            self.nodes[v].uf_size = 1;
        } else {
            v = self.uf_find(v);
        }
        if u == v {
            return u;
        }
        // ND_id is never set in dot, so `ND_id(u) > ND_id(v)` is always false.
        if self.nodes[u].id > self.nodes[v].id {
            self.nodes[u].uf_parent = Some(v);
            self.nodes[v].uf_size += self.nodes[u].uf_size;
            v
        } else {
            self.nodes[v].uf_parent = Some(u);
            self.nodes[u].uf_size += self.nodes[v].uf_size;
            u
        }
    }

    pub fn uf_singleton(&mut self, u: N) {
        self.nodes[u].uf_size = 1;
        self.nodes[u].uf_parent = None;
        self.nodes[u].ranktype = NORMAL;
    }

    pub fn uf_setname(&mut self, u: N, v: N) {
        self.nodes[u].uf_parent = Some(v);
        self.nodes[v].uf_size += self.nodes[u].uf_size;
    }
}
