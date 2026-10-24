use crate::math;
use serde::{Deserialize, Serialize};

/// 2D point in Graphviz points (72 points = 1 inch).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn dist(self, other: Self) -> f64 {
        math::hypot(self.x - other.x, self.y - other.y)
    }

    /// Safe vector normalization avoiding `NaN` on zero-length vectors (Fixes GV-BUG-006).
    #[inline]
    pub fn normalized_or_zero(self) -> Self {
        let len = math::hypot(self.x, self.y);
        if len <= 1e-12 {
            Self::new(0.0, 0.0)
        } else {
            Self::new(self.x / len, self.y / len)
        }
    }
}

/// Axis-aligned 2D bounding box `(ll, ur)`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct BoundingBox {
    pub ll: Point,
    pub ur: Point,
}

impl BoundingBox {
    #[inline]
    pub const fn new(llx: f64, lly: f64, urx: f64, ury: f64) -> Self {
        Self {
            ll: Point::new(llx, lly),
            ur: Point::new(urx, ury),
        }
    }

    #[inline]
    pub fn width(&self) -> f64 {
        (self.ur.x - self.ll.x).max(0.0)
    }

    #[inline]
    pub fn height(&self) -> f64 {
        (self.ur.y - self.ll.y).max(0.0)
    }

    #[inline]
    pub fn expand_to_include(&mut self, pt: Point) {
        if pt.x < self.ll.x {
            self.ll.x = pt.x;
        }
        if pt.y < self.ll.y {
            self.ll.y = pt.y;
        }
        if pt.x > self.ur.x {
            self.ur.x = pt.x;
        }
        if pt.y > self.ur.y {
            self.ur.y = pt.y;
        }
    }
}

/// Cubic Bezier spline chain representing a routed edge (C's `bezier`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(from = "BezierSegmentData")]
pub struct BezierSegment {
    /// Control points: `3n + 1` points for `n` cubic Bezier segments.
    pub points: Vec<Point>,
    /// `bz.sp` and `bz.ep`: the tips of the start and end arrowheads, set where `sflag` and
    /// `eflag` are not zero.
    pub start_arrow: Option<Point>,
    pub end_arrow: Option<Point>,
    /// `bz.sflag` and `bz.eflag`: the arrowhead types (`crate::arrows`) the layout clipped
    /// the spline for, as `arrow_clip` (splines.c) leaves them: after `swap_ends`, `ARR_NONE`
    /// at a concentrator merge point, with an opposing edge's types merged under
    /// `concentrate`. Renderers draw an arrowhead only where its flag is not zero, as
    /// `emit_edge_graphics` does, whatever `start_arrow`/`end_arrow` hold.
    pub sflag: u32,
    pub eflag: u32,
}

/// [`BezierSegment`] as serialized, also by releases before 0.2.0, which wrote no arrow types:
/// an arrowhead of such data is read as `normal`.
#[derive(Deserialize)]
struct BezierSegmentData {
    points: Vec<Point>,
    start_arrow: Option<Point>,
    end_arrow: Option<Point>,
    #[serde(default)]
    sflag: Option<u32>,
    #[serde(default)]
    eflag: Option<u32>,
}

impl From<BezierSegmentData> for BezierSegment {
    fn from(d: BezierSegmentData) -> Self {
        let legacy = |arrow: Option<Point>| match arrow {
            Some(_) => crate::arrows::ARR_TYPE_NORM,
            None => crate::arrows::ARR_TYPE_NONE,
        };
        BezierSegment {
            sflag: d.sflag.unwrap_or_else(|| legacy(d.start_arrow)),
            eflag: d.eflag.unwrap_or_else(|| legacy(d.end_arrow)),
            points: d.points,
            start_arrow: d.start_arrow,
            end_arrow: d.end_arrow,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Spline {
    pub segments: Vec<BezierSegment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_without_arrow_types_reads_as_normal_arrowheads() {
        // As serialized before 0.2.0: no `sflag`/`eflag`.
        let old = r#"{"points":[],"start_arrow":null,"end_arrow":{"x":1.0,"y":2.0}}"#;
        let s: BezierSegment = serde_json::from_str(old).unwrap();
        assert_eq!((s.sflag, s.eflag), (0, crate::arrows::ARR_TYPE_NORM));
        let new = BezierSegment {
            end_arrow: Some(Point::new(1.0, 2.0)),
            eflag: crate::arrows::ARR_TYPE_DOT,
            ..Default::default()
        };
        let back: BezierSegment =
            serde_json::from_str(&serde_json::to_string(&new).unwrap()).unwrap();
        assert_eq!(back, new);
    }
}
