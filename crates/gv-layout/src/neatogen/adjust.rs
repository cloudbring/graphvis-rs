//! Node overlap removal and spacing factors, ported from
//! `lib/neatogen/adjust.c`: `graphAdjustMode`/`getAdjustMode`,
//! `removeOverlapWith`, `normalize`, `simpleScale`, `sepFactor`,
//! `esepFactor`, and the scaling adjusters `scAdjust`.
//!
//! Graphviz 16.1.0 on Homebrew is built with GTS and SFDP, so
//! `overlap=false` selects the prism algorithm (`AM_PRISM`).

use crate::common::types::{LGraph, Sub};
use crate::common::utils::{map_bool, mapbool, parse_f64_rest};

pub const DFLT_MARGIN: f64 = 4.0;
pub const SEPFACT: f64 = 0.8;

/// `expand_t`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Expand {
    pub x: f64,
    pub y: f64,
    /// If true, `x` and `y` are additive margins in points; else scale factors.
    pub do_add: bool,
}

/// `adjust_mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdjustMode {
    None,
    Vor,
    Scale,
    NScale,
    ScaleXY,
    Ortho,
    OrthoYX,
    OrthoXY,
    OrthoYXrev,
    POrtho,
    POrthoYX,
    POrthoXY,
    POrthoYXrev,
    Compress,
    Vpsc,
    Ipsep,
    Prism,
}

/// `adjust_data`.
#[derive(Debug, Clone, Copy)]
pub struct AdjustData {
    pub mode: AdjustMode,
    pub value: i32,
    pub scaling: f64,
}

/// `sscanf(s, "%lf,%lf", &x, &y)` conversion count and values.
fn scan2(s: &str) -> (usize, f64, f64) {
    let Some((x, rest)) = parse_f64_rest(s) else {
        return (0, 0.0, 0.0);
    };
    if let Some(r) = rest.strip_prefix(',')
        && let Some((y, _)) = parse_f64_rest(r)
    {
        return (2, x, y);
    }
    (1, x, x)
}

/// `parseFactor(s, pp, sepfact, dflt)`.
fn parse_factor(s: &str, sepfact: f64, dflt: f64) -> Option<Expand> {
    let s = s.trim_start();
    let (do_add, s) = match s.strip_prefix('+') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (i, x, y) = scan2(s);
    if i == 0 {
        return None;
    }
    Some(if do_add {
        if sepfact > 1.0 {
            Expand {
                x: dflt.min(x / sepfact),
                y: dflt.min(y / sepfact),
                do_add,
            }
        } else if sepfact < 1.0 {
            Expand {
                x: dflt.max(x / sepfact),
                y: dflt.max(y / sepfact),
                do_add,
            }
        } else {
            Expand { x, y, do_add }
        }
    } else {
        Expand {
            x: 1.0 + x / sepfact,
            y: 1.0 + y / sepfact,
            do_add,
        }
    })
}

/// `sepFactor(g)`: node separation for overlap removal.
pub fn sep_factor(g: &LGraph<'_>) -> Expand {
    if let Some(e) = g.gattr("sep").and_then(|s| parse_factor(s, 1.0, 0.0)) {
        return e;
    }
    if let Some(e) = g
        .gattr("esep")
        .and_then(|s| parse_factor(s, SEPFACT, DFLT_MARGIN))
    {
        return e;
    }
    Expand {
        x: DFLT_MARGIN,
        y: DFLT_MARGIN,
        do_add: true,
    }
}

/// `esepFactor(g)`: edge separation used to grow obstacles for routing.
pub fn esep_factor(g: &LGraph<'_>) -> Expand {
    if let Some(e) = g.gattr("esep").and_then(|s| parse_factor(s, 1.0, 0.0)) {
        return e;
    }
    if let Some(e) = g
        .gattr("sep")
        .and_then(|s| parse_factor(s, 1.0 / SEPFACT, SEPFACT * DFLT_MARGIN))
    {
        return e;
    }
    Expand {
        x: SEPFACT * DFLT_MARGIN,
        y: SEPFACT * DFLT_MARGIN,
        do_add: true,
    }
}

const TABLE: &[(AdjustMode, &str)] = &[
    (AdjustMode::Prism, "prism"),
    (AdjustMode::Vor, "voronoi"),
    (AdjustMode::NScale, "scale"),
    (AdjustMode::Compress, "compress"),
    (AdjustMode::Vpsc, "vpsc"),
    (AdjustMode::Ipsep, "ipsep"),
    (AdjustMode::Scale, "oscale"),
    (AdjustMode::ScaleXY, "scalexy"),
    (AdjustMode::Ortho, "ortho"),
    (AdjustMode::OrthoYX, "ortho_yx"),
    (AdjustMode::OrthoXY, "orthoxy"),
    (AdjustMode::OrthoYXrev, "orthoyx"),
    (AdjustMode::POrtho, "portho"),
    (AdjustMode::POrthoYX, "portho_yx"),
    (AdjustMode::POrthoXY, "porthoxy"),
    (AdjustMode::POrthoYXrev, "porthoyx"),
];

fn set_prism_values(g: &LGraph<'_>, s: &str, dp: &mut AdjustData) {
    dp.value = match crate::common::utils::parse_leading_int(s) {
        Some(v) if v >= 0 => v as i32,
        _ => 1000,
    };
    dp.scaling =
        crate::common::utils::late_double(&g.src.graph_attrs, "overlap_scaling", -4.0, -1.0e10);
}

/// `graphAdjustMode(G, dp, dflt)`.
pub fn graph_adjust_mode(g: &LGraph<'_>, dflt: Option<&str>) -> AdjustData {
    let s = g.gattr("overlap").or(dflt).unwrap_or("");
    get_adjust_mode(g, s)
}

/// `getAdjustMode(g, s, dp)`.
pub fn get_adjust_mode(g: &LGraph<'_>, s: &str) -> AdjustData {
    let mut dp = AdjustData {
        mode: AdjustMode::None,
        value: 0,
        scaling: 0.0,
    };
    if s.is_empty() {
        return dp;
    }
    for &(mode, attrib) in TABLE {
        let matches = s.eq_ignore_ascii_case(attrib)
            || (mode == AdjustMode::Prism
                && s.len() >= attrib.len()
                && s[..attrib.len()].eq_ignore_ascii_case(attrib));
        if matches {
            dp.mode = mode;
            if mode == AdjustMode::Prism {
                set_prism_values(g, &s[attrib.len()..], &mut dp);
            }
            return dp;
        }
    }
    let mut v = mapbool(Some(s));
    if v != map_bool(Some(s), true) {
        v = false;
    }
    if v {
        dp.mode = AdjustMode::None;
    } else {
        dp.mode = AdjustMode::Prism;
        set_prism_values(g, "", &mut dp);
    }
    dp
}

/// `angleSet(g, &phi)`.
fn angle_set(g: &LGraph<'_>) -> Option<f64> {
    let a = g.gattr("normalize").filter(|s| !s.is_empty())?;
    let mut ang = match parse_f64_rest(a) {
        Some((v, _)) => v,
        None => {
            if mapbool(Some(a)) {
                0.0
            } else {
                return None;
            }
        }
    };
    while ang > 180.0 {
        ang -= 360.0;
    }
    while ang <= -180.0 {
        ang += 360.0;
    }
    Some(ang.to_radians())
}

/// `normalize(g)`: translate the first node to the origin and rotate so the
/// first edge has the angle given by `normalize`.
pub fn normalize(g: &mut LGraph<'_>, sub: &Sub) -> bool {
    let Some(mut phi) = angle_set(g) else {
        return false;
    };
    let Some(&v0) = sub.nodes.first() else {
        return false;
    };
    let p = (g.nodes[v0].pos[0], g.nodes[v0].pos[1]);
    for &v in &sub.nodes {
        g.nodes[v].pos[0] -= p.0;
        g.nodes[v].pos[1] -= p.1;
    }
    let ret = p.0 != 0.0 || p.1 != 0.0;
    let mut e = None;
    for &v in &sub.nodes {
        if let Some(x) = g.outs(sub, v).next() {
            e = Some(x);
            break;
        }
    }
    let Some(e) = e else { return ret };
    let (t, h) = (g.edges[e].tail, g.edges[e].head);
    phi -= (g.nodes[h].pos[1] - g.nodes[t].pos[1]).atan2(g.nodes[h].pos[0] - g.nodes[t].pos[0]);
    if phi != 0.0 {
        let orig = (g.nodes[t].pos[0], g.nodes[t].pos[1]);
        let (cosv, sinv) = (phi.cos(), phi.sin());
        for &v in &sub.nodes {
            let px = g.nodes[v].pos[0] - orig.0;
            let py = g.nodes[v].pos[1] - orig.1;
            g.nodes[v].pos[0] = px.mul_add(cosv, -(py * sinv)) + orig.0;
            g.nodes[v].pos[1] = px.mul_add(sinv, py * cosv) + orig.1;
        }
        return true;
    }
    ret
}

/// `simpleScale(g)`: the `scale` attribute.
fn simple_scale(g: &mut LGraph<'_>, sub: &Sub) -> bool {
    let Some(p) = g.gattr("scale") else {
        return false;
    };
    let (i, sx, mut sy) = scan2(p);
    if i == 0 {
        return false;
    }
    let iszero = |d: f64| d.abs() < 0.000_000_001;
    if iszero(sx) {
        return false;
    }
    if i == 1 {
        sy = sx;
    } else if iszero(sy) {
        return false;
    }
    if sy == 1.0 && sx == 1.0 {
        return false;
    }
    for &n in &sub.nodes {
        g.nodes[n].pos[0] *= sx;
        g.nodes[n].pos[1] *= sy;
    }
    true
}

/// `removeOverlapWith(G, am)`; returns whether nodes moved.
pub fn remove_overlap_with(g: &mut LGraph<'_>, sub: &Sub, am: &AdjustData) -> bool {
    if sub.nodes.len() < 2 {
        return false;
    }
    let mut nret = normalize(g, sub);
    nret |= simple_scale(g, sub);
    if am.mode == AdjustMode::None {
        return nret;
    }
    let ret = match am.mode {
        AdjustMode::NScale => super::scadjust::sc_adjust(g, sub, 1),
        AdjustMode::ScaleXY => super::scadjust::sc_adjust(g, sub, 0),
        AdjustMode::Compress => super::scadjust::sc_adjust(g, sub, -1),
        AdjustMode::Ortho
        | AdjustMode::OrthoYX
        | AdjustMode::OrthoXY
        | AdjustMode::OrthoYXrev
        | AdjustMode::POrtho
        | AdjustMode::POrthoYX
        | AdjustMode::POrthoXY
        | AdjustMode::POrthoYXrev => {
            super::scadjust::c_adjust(g, sub, am.mode);
            false
        }
        AdjustMode::Prism => {
            super::overlap::fdp_adjust(g, sub, am);
            false
        }
        AdjustMode::Ipsep => return nret,
        AdjustMode::Vpsc => super::scadjust::vpsc_adjust(g, sub),
        AdjustMode::Vor | AdjustMode::Scale => super::voronoi::adjust(g, sub, am.mode),
        AdjustMode::None => false,
    };
    nret || ret
}
