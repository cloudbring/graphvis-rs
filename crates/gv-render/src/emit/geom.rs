//! Geometry helpers used while emitting: `dotneato_closest`, `Bezier`, `splitBSpline`,
//! `computeoffset_p`, `computeoffset_qr` (`lib/common/emit.c`, `utils.c`),
//! `get_gradient_points` (`utils.c`) and `taper` (`lib/common/taper.c`).

use super::arrows::bezier_split;
use super::attrs::Attrs;
use super::model::Bz;
use gv_core::Point;

fn dist2(a: Point, b: Point) -> f64 {
    (a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)
}

fn dist(a: Point, b: Point) -> f64 {
    dist2(a, b).sqrt()
}

/// `dotneato_closest`.
pub(crate) fn dotneato_closest(spl: &[Bz], pt: Point) -> Point {
    let (mut besti, mut bestj, mut best) = (usize::MAX, usize::MAX, f64::MAX);
    for (i, bz) in spl.iter().enumerate() {
        for (j, b) in bz.list.iter().enumerate() {
            let d2 = dist2(*b, pt);
            if bestj == usize::MAX || d2 < best {
                besti = i;
                bestj = j;
                best = d2;
            }
        }
    }
    let bz = &spl[besti];
    if bestj == bz.list.len() - 1 {
        bestj -= 1;
    }
    let j = 3 * (bestj / 3);
    let c = [bz.list[j], bz.list[j + 1], bz.list[j + 2], bz.list[j + 3]];
    let (mut low, mut high) = (0.0, 1.0);
    let mut dlow2 = dist2(c[0], pt);
    let mut dhigh2 = dist2(c[3], pt);
    let mut pt2;
    loop {
        let t = (low + high) / 2.0;
        pt2 = bezier_split(&c, t).0;
        if (dlow2 - dhigh2).abs() < 1.0 {
            break;
        }
        if (high - low).abs() < 0.00001 {
            break;
        }
        if dlow2 < dhigh2 {
            high = t;
            dhigh2 = dist2(pt2, pt);
        } else {
            low = t;
            dlow2 = dist2(pt2, pt);
        }
    }
    pt2
}

fn approx_len(p: &[Point]) -> f64 {
    dist(p[0], p[1]) + dist(p[1], p[2]) + dist(p[2], p[3])
}

/// `splitBSpline`: `(left, right)`.
pub(crate) fn split_bspline(list: &[Point], t: f64) -> (Vec<Point>, Vec<Point>) {
    let cnt = (list.len() - 1) / 3;
    if cnt == 1 {
        let c = [list[0], list[1], list[2], list[3]];
        let (_, l, r) = bezier_split(&c, t);
        return (l.to_vec(), r.to_vec());
    }
    let lens: Vec<f64> = (0..cnt).map(|i| approx_len(&list[3 * i..])).collect();
    let sum: f64 = lens.iter().sum();
    let len = t * sum;
    let mut s = 0.0;
    let mut i = 0;
    while i < cnt {
        s += lens[i];
        if s >= len {
            break;
        }
        i += 1;
    }
    let i = i.min(cnt - 1);
    let mut left: Vec<Point> = list[..3 * (i + 1) + 1].to_vec();
    let k = left.len() - 4;
    let mut right: Vec<Point> = list[k..k + 3 * (cnt - i) + 1].to_vec();
    let last = lens[i];
    let r = (len - (s - last)) / last;
    let c = [list[3 * i], list[3 * i + 1], list[3 * i + 2], list[3 * i + 3]];
    let (_, l, rr) = bezier_split(&c, r);
    left[3 * i..3 * i + 4].copy_from_slice(&l);
    right[..4].copy_from_slice(&rr);
    (left, right)
}

/// `computeoffset_p`.
pub(crate) fn computeoffset_p(p: Point, q: Point, d: f64) -> Point {
    let (x, y) = (p.x - q.x, p.y - q.y);
    let d = d / (x * x + y * y + 0.0001).sqrt();
    Point::new(y * d, -x * d)
}

/// `computeoffset_qr`.
pub(crate) fn computeoffset_qr(p: Point, q: Point, r: Point, s: Point, d: f64) -> Point {
    let (mut x, mut y) = (q.x - r.x, q.y - r.y);
    let mut len = x.hypot(y);
    if len < 0.0001 {
        x = p.x - s.x;
        y = p.y - s.y;
        len = (x * x + y * y + 0.0001).sqrt();
    }
    let d = d / len;
    Point::new(y * d, -x * d)
}

/// `get_gradient_points`.
pub(crate) fn gradient_points(a: &[Point], angle: f64, flags: i32) -> [Point; 2] {
    let radial = flags & 1 != 0;
    let rhs = flags & 2 != 0;
    let (min, max) = if a.len() == 2 {
        let rx = a[1].x - a[0].x;
        let ry = a[1].y - a[0].y;
        (
            Point::new(a[0].x - rx, a[0].y - ry),
            Point::new(a[0].x + rx, a[0].y + ry),
        )
    } else {
        let mut min = a[0];
        let mut max = a[0];
        for p in a {
            min.x = if p.x < min.x { p.x } else { min.x };
            min.y = if p.y < min.y { p.y } else { min.y };
            max.x = if p.x > max.x { p.x } else { max.x };
            max.y = if p.y > max.y { p.y } else { max.y };
        }
        (min, max)
    };
    let center = Point::new(min.x + (max.x - min.x) / 2.0, min.y + (max.y - min.y) / 2.0);
    let mut g = [Point::default(); 2];
    if radial {
        let outer_r = (center.x - min.x).hypot(center.y - min.y);
        let inner_r = outer_r / 4.0;
        g[0].y = if rhs { center.y } else { -center.y };
        g[0].x = center.x;
        g[1].x = inner_r;
        g[1].y = outer_r;
    } else {
        let half_x = max.x - center.x;
        let half_y = max.y - center.y;
        let (sina, cosa) = (angle.sin(), angle.cos());
        if rhs {
            g[0].y = center.y - half_y * sina;
            g[1].y = center.y + half_y * sina;
        } else {
            g[0].y = -center.y + (max.y - center.y) * angle.sin();
            g[1].y = -center.y - (center.y - min.y) * angle.sin();
        }
        g[0].x = center.x - half_x * cosa;
        g[1].x = center.x + half_x * cosa;
    }
    g
}

/// Width profile of a tapered edge (`taperfun`).
#[derive(Clone, Copy)]
pub(crate) enum TaperDir {
    Forward,
    Back,
    Both,
    None,
}

pub(crate) fn taper_dir(a: Attrs<'_>, directed: bool) -> TaperDir {
    match a.nonempty("dir") {
        Some("forward") => TaperDir::Forward,
        Some("back") => TaperDir::Back,
        Some("both") => TaperDir::Both,
        Some("none") => TaperDir::None,
        _ if directed => TaperDir::Forward,
        _ => TaperDir::None,
    }
}

/// `taper`: the outline of a tapered edge. Approximation: the control polygon offset by the
/// local half width (C flattens the Bézier first; not yet ported exactly).
pub(crate) fn taper(list: &[Point], dir: TaperDir, initwid: f64) -> Vec<Point> {
    let n = list.len();
    if n < 2 {
        return list.to_vec();
    }
    let mut cum = vec![0.0; n];
    for i in 1..n {
        cum[i] = cum[i - 1] + dist(list[i - 1], list[i]);
    }
    let total = cum[n - 1].max(1e-9);
    let rad = |cur: f64| match dir {
        TaperDir::Forward => (1.0 - cur / total) * initwid / 2.0,
        TaperDir::Back => cur / total * initwid / 2.0,
        TaperDir::None => initwid / 2.0,
        TaperDir::Both => {
            let fr = cur / total;
            if fr <= 0.5 { fr * initwid } else { (1.0 - fr) * initwid }
        }
    };
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = if i + 1 < n {
            (list[i], list[i + 1])
        } else {
            (list[i - 1], list[i])
        };
        let o = computeoffset_p(a, b, rad(cum[i]));
        left.push(Point::new(list[i].x + o.x, list[i].y + o.y));
        right.push(Point::new(list[i].x - o.x, list[i].y - o.y));
    }
    right.reverse();
    left.extend(right);
    left
}
