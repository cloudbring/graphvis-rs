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
    let c = [
        list[3 * i],
        list[3 * i + 1],
        list[3 * i + 2],
        list[3 * i + 3],
    ];
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

/// `taper` (`lib/common/taper.c`): the outline of a tapered edge.
pub(crate) fn taper(list: &[Point], dir: TaperDir, initwid: f64) -> Vec<Point> {
    use std::f64::consts::PI;
    let d2r = |d: f64| PI * d / 180.0;
    let myatan = |y: f64, x: f64| {
        if x == 0.0 && y == 0.0 {
            return 0.0;
        }
        let v = y.atan2(x);
        if v >= 0.0 { v } else { v + 2.0 * PI }
    };
    let mymod = |o: f64, m: f64| {
        if o < 0.0 || o >= m {
            -(o / m).floor() * m + o
        } else {
            o
        }
    };
    let radfunc = |cur: f64, total: f64| match dir {
        TaperDir::Forward => (1.0 - cur / total) * initwid / 2.0,
        TaperDir::Back => cur / total * initwid / 2.0,
        TaperDir::None => initwid / 2.0,
        TaperDir::Both => {
            let fr = cur / total;
            if fr <= 0.5 {
                fr * initwid
            } else {
                (1.0 - fr) * initwid
            }
        }
    };
    // pathtolines
    let mut pts: Vec<(Point, f64)> = vec![(list[0], 0.0)];
    let mut linelen = 0.0;
    let mut v3 = list[0];
    let mut i = 0;
    while i + 3 < list.len() {
        let v = [v3, list[i + 1], list[i + 2], list[i + 3]];
        let mut p0 = v[0];
        for step in 1..=20 {
            let p1 = bezier_split(&v, f64::from(step) / 20.0).0;
            linelen += (p0.x - p1.x).hypot(p0.y - p1.y);
            pts.push((p1, linelen));
            p0 = p1;
        }
        v3 = v[3];
        i += 3;
    }
    let n = pts.len();
    let total = pts[n - 1].1;
    // (dir, lout, bevel, dir2)
    let mut info: Vec<(f64, f64, bool, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        let l = if i == 0 { n - 1 } else { i - 1 };
        let nx = (i + 1) % n;
        let (p, dist) = pts[i];
        let ndir = myatan(pts[nx].0.y - p.y, pts[nx].0.x - p.x);
        let ldir = myatan(pts[l].0.y - p.y, pts[l].0.x - p.x);
        let linerad = radfunc(dist, total);
        let (mut bevel, mut direction, direction2, lineout);
        bevel = false;
        if i == 0 || i == n - 1 {
            lineout = linerad;
            direction = if i == 0 {
                ndir + d2r(90.0)
            } else {
                ldir - d2r(90.0)
            };
            direction2 = direction;
        } else {
            let mut theta = ndir - ldir;
            if theta < 0.0 {
                theta += d2r(360.0);
            }
            let phi = d2r(90.0) - theta / 2.0;
            let mut lo = if phi.cos() == 0.0 {
                0.0
            } else {
                linerad / phi.cos()
            };
            direction = ndir + d2r(90.0) + phi;
            if lo > 10.0 * linerad {
                bevel = true;
                lo = linerad;
                direction = mymod(ldir - d2r(90.0), d2r(360.0));
                direction2 = mymod(ndir + d2r(90.0), d2r(360.0));
            } else {
                direction2 = direction;
            }
            lineout = lo;
        }
        info.push((direction, lineout, bevel, direction2));
    }
    let mut out = Vec::new();
    let (mut x, mut y, mut direction, mut lineout) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        x = pts[i].0.x;
        y = pts[i].0.y;
        let (d, lo, bevel, d2) = info[i];
        direction = d;
        lineout = lo;
        out.push(Point::new(x + d.cos() * lo, y + d.sin() * lo));
        if bevel {
            // drawbevel (C uses x for both coordinates)
            out.push(Point::new(x + lo * d2.cos(), x + lo * d2.sin()));
        }
    }
    direction += d2r(180.0);
    out.push(Point::new(
        x + direction.cos() * lineout,
        y + direction.sin() * lineout,
    ));
    for i in (0..n.saturating_sub(1)).rev() {
        let (px, py) = (pts[i].0.x, pts[i].0.y);
        let (d, lo, bevel, d2) = info[i];
        let dir = d + d2r(180.0);
        let dir2 = d2 + d2r(180.0);
        out.push(Point::new(px + dir2.cos() * lo, py + dir2.sin() * lo));
        if bevel {
            out.push(Point::new(px + lo * dir.cos(), px + lo * dir.sin()));
        }
    }
    out
}

/// `ellipticWedge` (`lib/common/ellipse.c`): a closed Bézier path from the centre around
/// the arc from `angle0` to `angle1` and back.
pub(crate) fn elliptic_wedge(ctr: Point, a: f64, b: f64, lambda1: f64, lambda2: f64) -> Vec<Point> {
    use std::f64::consts::PI;
    let twopi = 2.0 * PI;
    let eta1 = (lambda1.sin() / b).atan2(lambda1.cos() / a);
    let mut eta2 = (lambda2.sin() / b).atan2(lambda2.cos() / a);
    eta2 -= twopi * ((eta2 - eta1) / twopi).floor();
    if lambda2 - lambda1 > PI && eta2 - eta1 < PI {
        eta2 += twopi;
    }
    const LOW: [[[f64; 4]; 4]; 2] = [
        [
            [3.85268, -21.229, -0.330434, 0.0127842],
            [-1.61486, 0.706564, 0.225945, 0.263682],
            [-0.910164, 0.388383, 0.00551445, 0.00671814],
            [-0.630184, 0.192402, 0.0098871, 0.0102527],
        ],
        [
            [-0.162211, 9.94329, 0.13723, 0.0124084],
            [-0.253135, 0.00187735, 0.0230286, 0.01264],
            [-0.0695069, -0.0437594, 0.0120636, 0.0163087],
            [-0.0328856, -0.00926032, -0.00173573, 0.00527385],
        ],
    ];
    const HIGH: [[[f64; 4]; 4]; 2] = [
        [
            [0.0899116, -19.2349, -4.11711, 0.183362],
            [0.138148, -1.45804, 1.32044, 1.38474],
            [0.230903, -0.450262, 0.219963, 0.414038],
            [0.0590565, -0.101062, 0.0430592, 0.0204699],
        ],
        [
            [0.0164649, 9.89394, 0.0919496, 0.00760802],
            [0.0191603, -0.0322058, 0.0134667, -0.0825018],
            [0.0156192, -0.017535, 0.00326508, -0.228157],
            [-0.0236752, 0.0405821, -0.0173086, 0.176187],
        ],
    ];
    const SAFETY: [f64; 4] = [0.001, 4.98, 0.207, 0.0067];
    let rf = |x: f64, c: &[f64; 4]| (x * (x * c[0] + c[1]) + c[2]) / (x + c[3]);
    let estimate = |eta_a: f64, eta_b: f64| {
        let eta = 0.5 * (eta_a + eta_b);
        let x = b / a;
        let d_eta = eta_b - eta_a;
        let (cos2, cos4, cos6) = ((2.0 * eta).cos(), (4.0 * eta).cos(), (6.0 * eta).cos());
        let c = if x < 0.25 { &LOW } else { &HIGH };
        let c0 = rf(x, &c[0][0])
            + cos2 * rf(x, &c[0][1])
            + cos4 * rf(x, &c[0][2])
            + cos6 * rf(x, &c[0][3]);
        let c1 = rf(x, &c[1][0])
            + cos2 * rf(x, &c[1][1])
            + cos4 * rf(x, &c[1][2])
            + cos6 * rf(x, &c[1][3]);
        rf(x, &SAFETY) * a * (c0 + c1 * d_eta).exp()
    };
    let mut found = false;
    let mut n: i32 = 1;
    while !found && n < 1024 {
        let diff = (eta2 - eta1) / f64::from(n);
        if diff <= 0.5 * PI {
            let mut eta_one = eta1;
            found = true;
            let mut i = 0;
            while found && i < n {
                let eta_a = eta_one;
                eta_one += diff;
                found = estimate(eta_a, eta_one) <= 0.00001;
                i += 1;
            }
        }
        n <<= 1;
    }
    let d_eta = (eta2 - eta1) / f64::from(n);
    let mut eta_b = eta1;
    let (mut xb, mut yb) = (ctr.x + a * eta_b.cos(), ctr.y + b * eta_b.sin());
    let (mut xbd, mut ybd) = (-a * eta_b.sin(), b * eta_b.cos());
    let mut path = vec![ctr];
    let line_to = |path: &mut Vec<Point>, x: f64, y: f64| {
        let c = *path.last().unwrap();
        path.push(c);
        path.push(Point::new(x, y));
        path.push(Point::new(x, y));
    };
    line_to(&mut path, xb, yb);
    let t = (0.5 * d_eta).tan();
    let alpha = d_eta.sin() * ((4.0 + 3.0 * t * t).sqrt() - 1.0) / 3.0;
    for _ in 0..n {
        let (xa, ya, xad, yad) = (xb, yb, xbd, ybd);
        eta_b += d_eta;
        let (c, s) = (eta_b.cos(), eta_b.sin());
        xb = ctr.x + a * c;
        yb = ctr.y + b * s;
        xbd = -a * s;
        ybd = b * c;
        path.push(Point::new(xa + alpha * xad, ya + alpha * yad));
        path.push(Point::new(xb - alpha * xbd, yb - alpha * ybd));
        path.push(Point::new(xb, yb));
    }
    let p0 = path[0];
    line_to(&mut path, p0.x, p0.y);
    path
}
