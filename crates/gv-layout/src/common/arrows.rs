//! Arrowhead flags and lengths for edge clipping, ported from
//! `lib/common/arrows.c` (`arrow_flags`, `arrow_length`, `arrowStartClip`,
//! `arrowEndClip` and the penwidth-aware length helpers).

use super::splines::bezier_clip;
use super::utils::late_double;
use gv_core::{AttrMap, Point};

const ARROW_LENGTH: f64 = 10.0;
const NUMB_OF_ARROW_HEADS: usize = 4;
const BITS_PER_ARROW: u32 = 8;
const BITS_PER_ARROW_TYPE: u32 = 4;

pub const ARR_TYPE_NONE: u32 = 0;
pub const ARR_TYPE_NORM: u32 = 1;
const ARR_TYPE_CROW: u32 = 2;
const ARR_TYPE_TEE: u32 = 3;
const ARR_TYPE_BOX: u32 = 4;
const ARR_TYPE_DIAMOND: u32 = 5;
const ARR_TYPE_DOT: u32 = 6;
const ARR_TYPE_CURVE: u32 = 7;
const ARR_TYPE_GAP: u32 = 8;
const ARR_MOD_OPEN: u32 = 1 << BITS_PER_ARROW_TYPE;
const ARR_MOD_INV: u32 = 1 << (BITS_PER_ARROW_TYPE + 1);
const ARR_MOD_LEFT: u32 = 1 << (BITS_PER_ARROW_TYPE + 2);
const ARR_MOD_RIGHT: u32 = 1 << (BITS_PER_ARROW_TYPE + 3);

const SYNONYMS: &[(&str, u32)] = &[("invempty", ARR_TYPE_NORM | ARR_MOD_INV | ARR_MOD_OPEN)];
const MODS: &[(&str, u32)] = &[
    ("o", ARR_MOD_OPEN),
    ("r", ARR_MOD_RIGHT),
    ("l", ARR_MOD_LEFT),
    ("e", ARR_MOD_OPEN),
    ("half", ARR_MOD_LEFT),
];
const NAMES: &[(&str, u32)] = &[
    ("normal", ARR_TYPE_NORM),
    ("crow", ARR_TYPE_CROW),
    ("tee", ARR_TYPE_TEE),
    ("box", ARR_TYPE_BOX),
    ("diamond", ARR_TYPE_DIAMOND),
    ("dot", ARR_TYPE_DOT),
    ("none", ARR_TYPE_GAP),
    ("inv", ARR_TYPE_NORM | ARR_MOD_INV),
    ("vee", ARR_TYPE_CROW | ARR_MOD_INV),
    ("pen", ARR_TYPE_CROW | ARR_MOD_INV),
    ("mpty", ARR_TYPE_NORM),
    ("curve", ARR_TYPE_CURVE),
    ("icurve", ARR_TYPE_CURVE | ARR_MOD_INV),
];

fn match_frag<'s>(name: &'s str, table: &[(&str, u32)], flag: &mut u32) -> &'s str {
    for (n, t) in table {
        if let Some(rest) = name.strip_prefix(n) {
            *flag |= t;
            return rest;
        }
    }
    name
}

fn match_shape(name: &str) -> (u32, &str) {
    let mut f = ARR_TYPE_NONE;
    let mut rest = match_frag(name, SYNONYMS, &mut f);
    if rest.len() == name.len() {
        loop {
            let next = rest;
            rest = match_frag(next, MODS, &mut f);
            if rest.len() == next.len() {
                break;
            }
        }
        rest = match_frag(rest, NAMES, &mut f);
    }
    if f != 0 && f & ((1 << BITS_PER_ARROW_TYPE) - 1) == 0 {
        f |= ARR_TYPE_NORM;
    }
    (f, rest)
}

fn match_name(name: &str) -> u32 {
    let mut flag = 0;
    let mut rest = name;
    let mut i = 0;
    while !rest.is_empty() && i < NUMB_OF_ARROW_HEADS {
        let (mut f, r) = match_shape(rest);
        rest = r;
        if f == ARR_TYPE_NONE {
            return flag;
        }
        if f == ARR_TYPE_GAP && i == NUMB_OF_ARROW_HEADS - 1 {
            f = ARR_TYPE_NONE;
        }
        if f == ARR_TYPE_GAP && i == 0 && rest.is_empty() {
            f = ARR_TYPE_NONE;
        }
        if f != ARR_TYPE_NONE {
            flag |= f << (i as u32 * BITS_PER_ARROW);
            i += 1;
        }
    }
    flag
}

/// `arrow_flags`: start and end arrow flags of an edge.
pub fn arrow_flags(attrs: &AttrMap, directed: bool) -> (u32, u32) {
    let mut sflag = ARR_TYPE_NONE;
    let mut eflag = if directed {
        ARR_TYPE_NORM
    } else {
        ARR_TYPE_NONE
    };
    if let Some(d) = attrs.get("dir").filter(|s| !s.is_empty()) {
        match d.as_str() {
            "forward" => (sflag, eflag) = (ARR_TYPE_NONE, ARR_TYPE_NORM),
            "back" => (sflag, eflag) = (ARR_TYPE_NORM, ARR_TYPE_NONE),
            "both" => (sflag, eflag) = (ARR_TYPE_NORM, ARR_TYPE_NORM),
            "none" => (sflag, eflag) = (ARR_TYPE_NONE, ARR_TYPE_NONE),
            _ => {}
        }
    }
    if eflag == ARR_TYPE_NORM
        && let Some(a) = attrs.get("arrowhead").filter(|s| !s.is_empty())
    {
        eflag = match_name(a);
    }
    if sflag == ARR_TYPE_NORM
        && let Some(a) = attrs.get("arrowtail").filter(|s| !s.is_empty())
    {
        sflag = match_name(a);
    }
    (sflag, eflag)
}

struct Tri {
    p: [Point; 3],
}

fn miter_shape(base_left: Point, p: Point, base_right: Point, penwidth: f64) -> Tri {
    use std::f64::consts::PI;
    if (base_left.x == p.x && base_left.y == p.y) || (base_right.x == p.x && base_right.y == p.y) {
        return Tri { p: [p, p, p] };
    }
    let dxa = p.x - base_left.x;
    let dya = p.y - base_left.y;
    let hypota = dxa.hypot(dya);
    let cos_alpha = dxa / hypota;
    let sin_alpha = dya / hypota;
    let alpha = if dya > 0.0 {
        cos_alpha.acos()
    } else {
        -cos_alpha.acos()
    };
    let p1 = Point::new(
        p.x - penwidth / 2.0 * sin_alpha,
        p.y + penwidth / 2.0 * cos_alpha,
    );
    let dxb = base_right.x - p.x;
    let dyb = base_right.y - p.y;
    let hypotb = dxb.hypot(dyb);
    let cos_beta = dxb / hypotb;
    let beta = if dyb > 0.0 {
        cos_beta.acos()
    } else {
        -cos_beta.acos()
    };
    let beta_rev = beta - PI;
    let theta = beta_rev - alpha
        + if beta_rev - alpha <= -PI {
            2.0 * PI
        } else {
            0.0
        };
    let normalized_miter_length = 1.0 / (theta / 2.0).sin();
    let sin_beta = dyb / hypotb;
    let p2 = Point::new(
        p.x + penwidth / 2.0 * -sin_beta,
        p.y - penwidth / 2.0 * -cos_beta,
    );
    if normalized_miter_length > 4.0 {
        let pb = Point::new((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
        return Tri { p: [pb, p1, p2] };
    }
    let l = penwidth / 2.0 / (theta / 2.0).tan();
    let p3 = Point::new(p1.x + l * cos_alpha, p1.y + l * sin_alpha);
    Tri { p: [p3, p1, p2] }
}

fn side_delta(
    base_left: Point,
    p: Point,
    base_right: Point,
    penwidth: f64,
    idx: usize,
    cos_phi: f64,
    sin_phi: f64,
    phi: f64,
) -> Point {
    let t = miter_shape(base_left, p, base_right, penwidth);
    let q = t.p[idx];
    let dx = q.x - p.x;
    let dy = q.y - p.y;
    let h = dx.hypot(dy);
    let ca = dx / h;
    let alpha = if dy > 0.0 { ca.acos() } else { -ca.acos() };
    let len = h * (alpha - phi).cos();
    Point::new(len * cos_phi, len * sin_phi)
}

fn arrow_type_normal0(
    mut p: Point,
    u: Point,
    penwidth: f64,
    flag: u32,
    a: &mut [Point; 5],
) -> Point {
    let mut arrowwidth = 0.35;
    if penwidth > 4.0 {
        arrowwidth *= penwidth / 4.0;
    }
    let v = Point::new(-u.y * arrowwidth, u.x * arrowwidth);
    let mut q = Point::new(p.x + u.x, p.y + u.y);
    let origin = Point::new(0.0, 0.0);
    let v_inv = Point::new(-v.x, -v.y);
    let normal_left = if flag & ARR_MOD_RIGHT != 0 {
        origin
    } else {
        v_inv
    };
    let normal_right = if flag & ARR_MOD_LEFT != 0 { origin } else { v };
    let inv = flag & ARR_MOD_INV != 0;
    let base_left = if inv { normal_right } else { normal_left };
    let base_right = if inv { normal_left } else { normal_right };
    let pp = if inv { u } else { Point::new(-u.x, -u.y) };
    let mut delta_tip = origin;
    let mut delta_base = origin;
    if u.x != 0.0 || u.y != 0.0 {
        let cos_phi = pp.x / pp.x.hypot(pp.y);
        let sin_phi = pp.y / pp.x.hypot(pp.y);
        let phi = if pp.y > 0.0 {
            cos_phi.acos()
        } else {
            -cos_phi.acos()
        };
        if flag & ARR_MOD_LEFT != 0 {
            delta_tip = side_delta(
                base_left, pp, base_right, penwidth, 1, cos_phi, sin_phi, phi,
            );
        } else if flag & ARR_MOD_RIGHT != 0 {
            delta_tip = side_delta(
                base_left, pp, base_right, penwidth, 2, cos_phi, sin_phi, phi,
            );
        } else {
            let t = miter_shape(base_left, pp, base_right, penwidth);
            delta_tip = Point::new(t.p[0].x - pp.x, t.p[0].y - pp.y);
        }
        delta_base = Point::new(penwidth / 2.0 * cos_phi, penwidth / 2.0 * sin_phi);
    }
    if inv {
        p.x += delta_base.x;
        p.y += delta_base.y;
        q.x += delta_base.x;
        q.y += delta_base.y;
        a[0] = p;
        a[4] = p;
        a[1] = Point::new(p.x - v.x, p.y - v.y);
        a[2] = q;
        a[3] = Point::new(p.x + v.x, p.y + v.y);
        q.x += delta_tip.x;
        q.y += delta_tip.y;
    } else {
        p.x -= delta_tip.x;
        p.y -= delta_tip.y;
        q.x -= delta_tip.x;
        q.y -= delta_tip.y;
        a[0] = q;
        a[4] = q;
        a[1] = Point::new(q.x - v.x, q.y - v.y);
        a[2] = p;
        a[3] = Point::new(q.x + v.x, q.y + v.y);
        q.x -= delta_base.x;
        q.y -= delta_base.y;
    }
    q
}

fn arrow_type_crow0(
    mut p: Point,
    u: Point,
    arrowsize: f64,
    penwidth: f64,
    flag: u32,
    a: &mut [Point; 9],
) -> Point {
    let inv = flag & ARR_MOD_INV != 0;
    let mut arrowwidth = 0.45;
    if penwidth > 4.0 * arrowsize && inv {
        arrowwidth *= penwidth / (4.0 * arrowsize);
    }
    let mut shaftwidth = 0.0;
    if penwidth > 1.0 && inv {
        shaftwidth = 0.05 * (penwidth - 1.0) / arrowsize;
    }
    let v = Point::new(-u.y * arrowwidth, u.x * arrowwidth);
    let w = Point::new(-u.y * shaftwidth, u.x * shaftwidth);
    let mut q = Point::new(p.x + u.x, p.y + u.y);
    let m = Point::new(p.x + u.x * 0.5, p.y + u.y * 0.5);
    let origin = Point::new(0.0, 0.0);
    let v_inv = Point::new(-v.x, -v.y);
    let normal_left = if flag & ARR_MOD_RIGHT != 0 { origin } else { v };
    let normal_right = if flag & ARR_MOD_LEFT != 0 {
        origin
    } else {
        v_inv
    };
    let base_left = if inv { normal_right } else { normal_left };
    let base_right = if inv { normal_left } else { normal_right };
    let pp = if inv { Point::new(-u.x, -u.y) } else { u };
    let mut delta_tip = origin;
    let mut delta_base = origin;
    if u.x != 0.0 || u.y != 0.0 {
        let cos_phi = pp.x / pp.x.hypot(pp.y);
        let sin_phi = pp.y / pp.x.hypot(pp.y);
        let phi = if pp.y > 0.0 {
            cos_phi.acos()
        } else {
            -cos_phi.acos()
        };
        let left = flag & ARR_MOD_LEFT != 0;
        let right = flag & ARR_MOD_RIGHT != 0;
        if (left && inv) || (right && !inv) {
            delta_tip = side_delta(
                base_left, pp, base_right, penwidth, 2, cos_phi, sin_phi, phi,
            );
        } else if (left && !inv) || (right && inv) {
            delta_tip = side_delta(
                base_left, pp, base_right, penwidth, 1, cos_phi, sin_phi, phi,
            );
        } else {
            let t = miter_shape(base_left, pp, base_right, penwidth);
            delta_tip = Point::new(t.p[0].x - pp.x, t.p[0].y - pp.y);
        }
        if inv {
            delta_base = Point::new(penwidth / 2.0 * cos_phi, penwidth / 2.0 * sin_phi);
        } else {
            let toe_base_left = Point::new(m.x - q.x + w.x, m.y - q.y + w.y);
            let toe_p = Point::new(v.x - u.x, v.y - u.y);
            let d = side_delta(
                toe_base_left,
                toe_p,
                origin,
                penwidth,
                1,
                cos_phi,
                sin_phi,
                phi,
            );
            delta_base = Point::new(-d.x, -d.y);
        }
    }
    if inv {
        p.x -= delta_tip.x;
        p.y -= delta_tip.y;
        q.x -= delta_tip.x;
        q.y -= delta_tip.y;
        a[0] = p;
        a[8] = p;
        a[1] = Point::new(q.x - v.x, q.y - v.y);
        a[2] = Point::new(m.x - w.x, m.y - w.y);
        a[3] = Point::new(q.x - w.x, q.y - w.y);
        a[4] = q;
        a[5] = Point::new(q.x + w.x, q.y + w.y);
        a[6] = Point::new(m.x + w.x, m.y + w.y);
        a[7] = Point::new(q.x + v.x, q.y + v.y);
        q.x -= delta_base.x;
        q.y -= delta_base.y;
    } else {
        p.x += delta_base.x;
        p.y += delta_base.y;
        q.x += delta_base.x;
        q.y += delta_base.y;
        a[0] = q;
        a[8] = q;
        a[1] = Point::new(p.x - v.x, p.y - v.y);
        a[2] = Point::new(m.x - w.x, m.y - w.y);
        a[3] = Point::new(p.x + delta_base.x, p.y + delta_base.y);
        a[4] = a[3];
        a[5] = a[3];
        a[6] = Point::new(m.x + w.x, m.y + w.y);
        a[7] = Point::new(p.x + v.x, p.y + v.y);
        q.x += delta_tip.x;
        q.y += delta_tip.y;
    }
    q
}

fn arrow_type_diamond0(
    mut p: Point,
    u: Point,
    penwidth: f64,
    flag: u32,
    a: &mut [Point; 5],
) -> Point {
    let v = Point::new(-u.y / 3.0, u.x / 3.0);
    let mut r = Point::new(p.x + u.x / 2.0, p.y + u.y / 2.0);
    let mut q = Point::new(p.x + u.x, p.y + u.y);
    let origin = Point::new(0.0, 0.0);
    let unmod_left = Point::new(-0.5 * u.x - v.x, -0.5 * u.y - v.y);
    let unmod_right = Point::new(-0.5 * u.x + v.x, -0.5 * u.y + v.y);
    let base_left = if flag & ARR_MOD_RIGHT != 0 {
        origin
    } else {
        unmod_left
    };
    let base_right = if flag & ARR_MOD_LEFT != 0 {
        origin
    } else {
        unmod_right
    };
    let tip = Point::new(-u.x, -u.y);
    let t = miter_shape(base_left, tip, base_right, penwidth);
    let delta = Point::new(t.p[0].x - tip.x, t.p[0].y - tip.y);
    p = Point::new(p.x - delta.x, p.y - delta.y);
    r = Point::new(r.x - delta.x, r.y - delta.y);
    q = Point::new(q.x - delta.x, q.y - delta.y);
    a[0] = q;
    a[4] = q;
    a[1] = Point::new(r.x + v.x, r.y + v.y);
    a[2] = p;
    a[3] = Point::new(r.x - v.x, r.y - v.y);
    Point::new(q.x - delta.x, q.y - delta.y)
}

fn len_for(t: u32, lenfact: f64, arrowsize: f64, penwidth: f64, flag: u32) -> f64 {
    let nominal = lenfact * arrowsize * ARROW_LENGTH;
    let u = Point::new(nominal, 0.0);
    let o = Point::new(0.0, 0.0);
    match t {
        ARR_TYPE_NORM => {
            let mut a = [o; 5];
            let q = arrow_type_normal0(o, u, penwidth, flag, &mut a);
            let full_length = q.x;
            let nominal_length = (a[1].x - a[2].x).abs();
            let nominal_base_width = a[3].y - a[1].y;
            let full_base_width = nominal_base_width * full_length / nominal_length;
            let overlap = if flag & ARR_MOD_INV != 0 {
                full_length * penwidth / full_base_width
            } else {
                penwidth / 2.0
            };
            full_length - overlap
        }
        ARR_TYPE_CROW => {
            let mut a = [o; 9];
            let q = arrow_type_crow0(o, u, arrowsize, penwidth, flag, &mut a);
            let full_length = q.x;
            let fl_ws = full_length - (a[1].x - a[3].x);
            let nominal_length = (a[1].x - a[0].x).abs();
            let nominal_base_width = a[7].y - a[1].y;
            let full_base_width = nominal_base_width * fl_ws / nominal_length;
            let overlap = if flag & ARR_MOD_INV != 0 {
                penwidth / 2.0
            } else {
                fl_ws * penwidth / full_base_width
            };
            full_length - overlap
        }
        ARR_TYPE_TEE => {
            let mut length = nominal;
            let start = penwidth / 2.0 - (1.0 - 0.6) * nominal;
            if start > 0.0 {
                length += start;
            }
            let end = penwidth / 2.0 - 0.2 * nominal;
            if start > 0.0 {
                length += end;
            }
            length
        }
        ARR_TYPE_BOX | ARR_TYPE_CURVE => nominal + penwidth / 2.0,
        ARR_TYPE_DIAMOND => {
            let mut a = [o; 5];
            let q = arrow_type_diamond0(o, u, penwidth, flag, &mut a);
            let full_length = q.x / 2.0;
            let nominal_length = (a[3].x - a[2].x).abs();
            let nominal_base_width = a[1].y - a[3].y;
            let full_base_width = nominal_base_width * full_length / nominal_length;
            2.0 * full_length - full_length * penwidth / full_base_width
        }
        ARR_TYPE_DOT => nominal + penwidth,
        _ => nominal,
    }
}

fn lenfact(t: u32) -> Option<f64> {
    Some(match t {
        ARR_TYPE_NORM | ARR_TYPE_CROW | ARR_TYPE_BOX | ARR_TYPE_CURVE => 1.0,
        ARR_TYPE_TEE | ARR_TYPE_GAP => 0.5,
        ARR_TYPE_DIAMOND => 1.2,
        ARR_TYPE_DOT => 0.8,
        _ => return None,
    })
}

/// `arrow_length(e, flag)`.
pub fn arrow_length(attrs: &AttrMap, flag: u32) -> f64 {
    let penwidth = late_double(attrs, "penwidth", 1.0, 0.0);
    let arrowsize = late_double(attrs, "arrowsize", 1.0, 0.0);
    if arrowsize == 0.0 {
        return 0.0;
    }
    let mut length = 0.0;
    for i in 0..NUMB_OF_ARROW_HEADS as u32 {
        let f = (flag >> (i * BITS_PER_ARROW)) & ((1 << BITS_PER_ARROW_TYPE) - 1);
        if let Some(lf) = lenfact(f) {
            let af = (flag >> (i * BITS_PER_ARROW)) & ((1 << BITS_PER_ARROW) - 1);
            length += len_for(f, lf, arrowsize, penwidth, af);
        }
    }
    length
}

use super::geom::dist;

/// `arrowEndClip`; returns the new `endp` and sets `eflag`/`ep` on the bezier.
pub fn arrow_end_clip(
    attrs: &AttrMap,
    ps: &mut [Point],
    startp: usize,
    mut endp: usize,
    spl: &mut super::types::Bezier,
    eflag: u32,
) -> usize {
    let elen = arrow_length(attrs, eflag);
    spl.eflag = eflag;
    spl.ep = ps[endp + 3];
    if endp > startp && dist(ps[endp], ps[endp + 3]) < elen {
        endp -= 3;
    }
    let mut sp = [spl.ep, ps[endp + 2], ps[endp + 1], ps[endp]];
    if elen > 0.0 {
        let c = sp[0];
        bezier_clip(&mut |p| dist(p, c) <= elen, &mut sp, true);
    }
    ps[endp] = sp[3];
    ps[endp + 1] = sp[2];
    ps[endp + 2] = sp[1];
    ps[endp + 3] = sp[0];
    endp
}

/// `arrowStartClip`; returns the new `startp` and sets `sflag`/`sp`.
pub fn arrow_start_clip(
    attrs: &AttrMap,
    ps: &mut [Point],
    mut startp: usize,
    endp: usize,
    spl: &mut super::types::Bezier,
    sflag: u32,
) -> usize {
    let slen = arrow_length(attrs, sflag);
    spl.sflag = sflag;
    spl.sp = ps[startp];
    if endp > startp && dist(ps[startp], ps[startp + 3]) < slen {
        startp += 3;
    }
    let mut sp = [ps[startp + 3], ps[startp + 2], ps[startp + 1], spl.sp];
    if slen > 0.0 {
        let c = sp[3];
        bezier_clip(&mut |p| dist(p, c) <= slen, &mut sp, false);
    }
    ps[startp] = sp[3];
    ps[startp + 1] = sp[2];
    ps[startp + 2] = sp[1];
    ps[startp + 3] = sp[0];
    startp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_arrow_length() {
        let a = AttrMap::new();
        let l = arrow_length(&a, ARR_TYPE_NORM);
        // Value computed by the C arrow_length_normal for penwidth 1.
        assert!((l - 11.513_544_292_886_934).abs() < 1e-12, "{l}");
        assert_eq!(arrow_flags(&a, true), (0, 1));
        assert_eq!(arrow_flags(&a, false), (0, 0));
    }
}
