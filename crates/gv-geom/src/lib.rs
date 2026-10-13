//! Geometry, path planning, spline fitting, R-tree label placement, and polyomino packing (`gv-geom`).

use gv_core::{BezierSegment, Point, Spline};

/// Computes a straight cubic Bezier segment between two endpoints.
pub fn straight_spline(from: Point, to: Point) -> Spline {
    let p1 = Point::new(
        from.x + (to.x - from.x) / 3.0,
        from.y + (to.y - from.y) / 3.0,
    );
    let p2 = Point::new(
        from.x + 2.0 * (to.x - from.x) / 3.0,
        from.y + 2.0 * (to.y - from.y) / 3.0,
    );
    Spline {
        segments: vec![BezierSegment {
            points: vec![from, p1, p2, to],
            start_arrow: None,
            end_arrow: Some(to),
        }],
    }
}
