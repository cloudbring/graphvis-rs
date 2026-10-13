//! Geometry, node shape sizing & boundary clipping, record/HTML table box layout,
//! arrowhead geometry, and Bezier spline routing (`gv-geom`).

#![forbid(unsafe_code)]

pub mod shapes;
pub mod splines;

pub use shapes::{
    DEF_POINT_SIZE, DEFAULT_MARGIN_X, DEFAULT_MARGIN_Y, DEFAULT_NODE_HEIGHT, DEFAULT_NODE_WIDTH,
    HtmlCell, HtmlTable, MIN_NODE_HEIGHT, MIN_NODE_WIDTH, MIN_POINT_SIZE, PERIPHERY_GAP_PTS,
    POINTS_PER_INCH, RecordField, ShapeKind, clip_ray_to_node_boundary, compute_node_dimensions,
    compute_node_dimensions_with_rankdir, html_table_layout, is_html_label, parse_graphviz_bool,
    parse_html_table, parse_node_margin, record_layout, resolve_port_box_and_compass,
    resolve_port_offset, shape_polygon_vertices,
};
pub use splines::{
    ArrowKind, ArrowSpec, DEFAULT_ARROW_HALF_WIDTH, DEFAULT_ARROW_LENGTH, EdgeLabelPositions,
    PORT_LABEL_ANGLE_DEG, PORT_LABEL_DISTANCE, SELF_EDGE_SIZE, arrow_polygon_points,
    compute_arrow_length, compute_edge_label_positions, eval_cubic_bezier, parse_arrow_specs,
    resolve_edge_arrow_lengths, route_edge_spline, route_edge_spline_with_offset,
    segment_intersects_box, spline_midpoint,
};

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

#[cfg(test)]
mod tests {
    use super::*;
    use gv_core::{AttrMap, BoundingBox, EdgeData, EdgeId, NodeData, NodeId, Port};

    fn make_node(id: u32, name: &str, pos: Point, attrs: &[(&str, &str)]) -> NodeData {
        let mut map = AttrMap::new();
        map.insert("label".to_string(), "\\N".to_string());
        for &(k, v) in attrs {
            map.insert(k.to_string(), v.to_string());
        }
        let mut node = NodeData {
            id: NodeId(id),
            name: name.to_string(),
            attrs: map,
            pos,
            width: 0.75,
            height: 0.5,
            rank: 0,
            order: id as usize,
        };
        compute_node_dimensions(&mut node, "G");
        node
    }

    fn make_edge(id: u32, tail: NodeId, head: NodeId, attrs: &[(&str, &str)]) -> EdgeData {
        let mut map = AttrMap::new();
        for &(k, v) in attrs {
            map.insert(k.to_string(), v.to_string());
        }
        EdgeData {
            id: EdgeId(id),
            tail,
            head,
            tail_port: Port::default(),
            head_port: Port::default(),
            attrs: map,
            spline: Spline::default(),
            label_pos: None,
        }
    }

    #[test]
    fn test_shape_classification_and_sizing() {
        assert_eq!(ShapeKind::parse("ellipse"), ShapeKind::Ellipse);
        assert_eq!(ShapeKind::parse("oval"), ShapeKind::Ellipse);
        assert_eq!(ShapeKind::parse("box"), ShapeKind::Box);
        assert_eq!(ShapeKind::parse("rectangle"), ShapeKind::Box);
        assert_eq!(ShapeKind::parse("doublecircle"), ShapeKind::DoubleCircle);
        assert_eq!(ShapeKind::parse("Mrecord"), ShapeKind::Mrecord);
        assert_eq!(ShapeKind::parse("star"), ShapeKind::Star);
        assert_eq!(ShapeKind::parse("cylinder"), ShapeKind::Cylinder);

        // Point shape defaults to 0.05 inches
        let pt_node = make_node(0, "p", Point::new(0.0, 0.0), &[("shape", "point")]);
        assert!((pt_node.width - 0.05).abs() < 1e-6);
        assert!((pt_node.height - 0.05).abs() < 1e-6);

        // Circle and doublecircle enforce width == height
        let circle = make_node(
            1,
            "very_wide_circle_label",
            Point::new(0.0, 0.0),
            &[("shape", "circle")],
        );
        assert!((circle.width - circle.height).abs() < 1e-9);
        assert!(circle.width > 0.75);

        let dbl_circle = make_node(
            2,
            "very_wide_circle_label",
            Point::new(0.0, 0.0),
            &[("shape", "doublecircle")],
        );
        assert!((dbl_circle.width - dbl_circle.height).abs() < 1e-9);
        // Doublecircle has an extra periphery gap of 2 * 4pt = 8pt = 8/72 in
        assert!((dbl_circle.width - circle.width - 8.0 / 72.0).abs() < 1e-6);

        // Fixedsize respects explicit width and height
        let fixed = make_node(
            3,
            "very_long_label_that_overflows",
            Point::new(0.0, 0.0),
            &[
                ("shape", "box"),
                ("width", "1.25"),
                ("height", "0.8"),
                ("fixedsize", "true"),
            ],
        );
        assert!((fixed.width - 1.25).abs() < 1e-9);
        assert!((fixed.height - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_record_nested_layout_and_ports() {
        let rec_node = make_node(
            0,
            "rec",
            Point::new(100.0, 200.0),
            &[
                ("shape", "record"),
                ("label", "{ a | { b | <p1> c | d } | <p2> e }"),
            ],
        );
        let layout = record_layout(&rec_node, "G", "TB").expect("record layout should succeed");
        assert!(layout.bbox.width() >= 54.0);
        assert!(layout.bbox.height() >= 36.0);

        let p1 = layout.find_port("p1").expect("port p1 should exist");
        let p2 = layout.find_port("p2").expect("port p2 should exist");
        assert_eq!(p1.text, "c");
        assert_eq!(p2.text, "e");
        assert!(p1.bbox.width() > 0.0 && p1.bbox.height() > 0.0);

        // Clipping to <p1>:e should land on the right edge of p1's sub-box
        let port_p1_e = Port {
            name: Some("p1".to_string()),
            compass: Some("e".to_string()),
        };
        let clipped = clip_ray_to_node_boundary(&rec_node, &port_p1_e, Point::new(300.0, 200.0));
        assert!((clipped.x - (rec_node.pos.x + p1.bbox.ur.x)).abs() < 1e-6);
        let p1_cy = rec_node.pos.y + (p1.bbox.ll.y + p1.bbox.ur.y) * 0.5;
        assert!((clipped.y - p1_cy).abs() < 1e-6);
    }

    #[test]
    fn test_html_table_layout_and_ports() {
        let html_str = r#"<<TABLE BORDER="0" CELLBORDER="1" CELLSPACING="0" CELLPADDING="4">
            <TR><TD ROWSPAN="3">hello<BR/>world</TD><TD COLSPAN="3">b</TD></TR>
            <TR><TD>c</TD><TD PORT="here">d</TD><TD>e</TD></TR>
            <TR><TD COLSPAN="3">f</TD></TR>
        </TABLE>>"#;
        let node = make_node(
            0,
            "struct3",
            Point::new(50.0, 60.0),
            &[("shape", "plaintext"), ("label", html_str)],
        );
        let tbl = html_table_layout(&node, "G").expect("HTML table should parse");
        assert_eq!(tbl.row_count, 3);
        assert_eq!(tbl.col_count, 4);

        let here_bb = tbl.find_port_box("here").expect("port 'here' should exist");
        assert!(here_bb.width() > 0.0 && here_bb.height() > 0.0);

        let port_here = Port {
            name: Some("here".to_string()),
            compass: Some("s".to_string()),
        };
        let pt = clip_ray_to_node_boundary(&node, &port_here, Point::new(50.0, 0.0));
        let expected_x = node.pos.x + (here_bb.ll.x + here_bb.ur.x) * 0.5;
        let expected_y = node.pos.y + here_bb.ll.y;
        assert!((pt.x - expected_x).abs() < 1e-6);
        assert!((pt.y - expected_y).abs() < 1e-6);
    }

    #[test]
    fn test_boundary_clipping_ellipse_box_diamond_compass() {
        // Ellipse at (0, 0) with width=2.0 in (rx=72 pt), height=1.0 in (ry=36 pt)
        let ellipse = make_node(
            0,
            "e",
            Point::new(0.0, 0.0),
            &[
                ("shape", "ellipse"),
                ("width", "2.0"),
                ("height", "1.0"),
                ("fixedsize", "true"),
            ],
        );
        let hit_right =
            clip_ray_to_node_boundary(&ellipse, &Port::default(), Point::new(200.0, 0.0));
        assert!((hit_right.x - 72.0).abs() < 1e-6);
        assert!(hit_right.y.abs() < 1e-6);

        let hit_top = clip_ray_to_node_boundary(&ellipse, &Port::default(), Point::new(0.0, 200.0));
        assert!(hit_top.x.abs() < 1e-6);
        assert!((hit_top.y - 36.0).abs() < 1e-6);

        // Check ellipse equation (x/72)^2 + (y/36)^2 == 1 for arbitrary angle
        let hit_diag =
            clip_ray_to_node_boundary(&ellipse, &Port::default(), Point::new(120.0, 95.0));
        let eq = (hit_diag.x / 72.0).powi(2) + (hit_diag.y / 36.0).powi(2);
        assert!((eq - 1.0).abs() < 1e-6);

        // Box at (10, 20) with rx=36, ry=18
        let box_node = make_node(
            1,
            "b",
            Point::new(10.0, 20.0),
            &[
                ("shape", "box"),
                ("width", "1.0"),
                ("height", "0.5"),
                ("fixedsize", "true"),
            ],
        );
        let compass_ne = Port {
            name: None,
            compass: Some("ne".to_string()),
        };
        let ne_pt = clip_ray_to_node_boundary(&box_node, &compass_ne, Point::new(100.0, 100.0));
        assert!((ne_pt.x - 46.0).abs() < 1e-6);
        assert!((ne_pt.y - 38.0).abs() < 1e-6);

        // Diamond polygon clipping
        let diamond = make_node(
            2,
            "d",
            Point::new(0.0, 0.0),
            &[
                ("shape", "diamond"),
                ("width", "1.0"),
                ("height", "1.0"),
                ("fixedsize", "true"),
            ],
        );
        // For a 72x72 diamond with vertices at (+-36, 0) and (0, +-36), ray along (1, 1) hits (18, 18)
        let d_hit = clip_ray_to_node_boundary(&diamond, &Port::default(), Point::new(100.0, 100.0));
        assert!((d_hit.x + d_hit.y - 36.0).abs() < 1e-5);
    }

    #[test]
    fn test_route_edge_spline_arrows_self_loop_and_obstacles() {
        let tail = make_node(0, "a", Point::new(0.0, 100.0), &[("shape", "box")]);
        let head = make_node(1, "b", Point::new(0.0, 0.0), &[("shape", "box")]);

        // 1. Standard forward edge with label
        let edge = make_edge(0, tail.id, head.id, &[("label", "step")]);
        let (spline, label_pos) = route_edge_spline(&tail, &head, &edge, &[], "TB");
        assert_eq!(spline.segments.len(), 1);
        let seg = &spline.segments[0];
        assert_eq!(seg.points.len(), 4);
        assert!(seg.start_arrow.is_none());
        let tip = seg.end_arrow.expect("forward edge should have end_arrow");
        // Head top boundary is at y = 0 + 18 = 18.0
        assert!((tip.y - 18.0).abs() < 1e-6);
        // Curve endpoint is shortened by DEFAULT_ARROW_LENGTH (10.0 pt) -> y = 28.0
        assert!((seg.points[3].y - 28.0).abs() < 1e-6);
        assert!(label_pos.is_some());

        // 2. Bidirectional edge (`dir=both`)
        let both_edge = make_edge(1, tail.id, head.id, &[("dir", "both")]);
        let (both_sp, _) = route_edge_spline(&tail, &head, &both_edge, &[], "TB");
        assert!(both_sp.segments[0].start_arrow.is_some());
        assert!(both_sp.segments[0].end_arrow.is_some());

        // 3. Self-loop (`tail.id == head.id`)
        let self_edge = make_edge(2, tail.id, tail.id, &[("label", "loop")]);
        let (self_sp, self_lbl) = route_edge_spline(&tail, &tail, &self_edge, &[], "TB");
        let self_seg = &self_sp.segments[0];
        assert_eq!(self_seg.points.len(), 4);
        // Control points extend to the right of tail's right boundary
        let tail_right = tail.pos.x + tail.width * 36.0;
        assert!(self_seg.points[1].x > tail_right);
        assert!(self_seg.points[2].x > tail_right);
        assert!(self_lbl.is_some());

        // 4. Obstacle avoidance around an intermediate box at (0, 50)
        let obstacle = BoundingBox::new(-27.0, 32.0, 27.0, 68.0);
        let (detour_sp, _) = route_edge_spline(&tail, &head, &edge, &[obstacle], "TB");
        let mid = spline_midpoint(&detour_sp).unwrap();
        // Midpoint of the detoured spline must bypass the obstacle horizontally
        assert!(mid.x.abs() > 27.0);
    }
}
