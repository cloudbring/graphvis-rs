//! Edge spline routing, obstacle-avoiding waypoint fitting, self-loop tear-drop curves,
//! arrowhead geometry/clipping, and edge label placement ported from Graphviz 16.1.0
//! (`lib/common/splines.c`, `lib/common/arrows.c`, `lib/common/routespl.c`, and `lib/pathplan/`).

use crate::shapes::{POINTS_PER_INCH, clip_ray_to_node_boundary, resolve_port_offset};
use gv_core::font::{
    DEFAULT_FONT_NAME, DEFAULT_FONT_SIZE, DEFAULT_LABEL_FONT_SIZE, LabelSubstContext,
    measure_multiline_label_with_context,
};
use gv_core::{BezierSegment, BoundingBox, EdgeData, NodeData, Point, Port, Spline};

/// Standard Graphviz arrowhead length in points (`10.0`, `ARROW_LENGTH` in `arrows.c`).
pub const DEFAULT_ARROW_LENGTH: f64 = 10.0;

/// Standard Graphviz arrowhead half-width in points (`3.5`, `0.35 * ARROW_LENGTH` in `arrows.c`).
pub const DEFAULT_ARROW_HALF_WIDTH: f64 = 3.5;

/// Default self-edge loop extension in points (`18.0`, `SELF_EDGE_SIZE` in `const.h`).
pub const SELF_EDGE_SIZE: f64 = 18.0;

/// Default port label distance in points (`10.0`, `PORT_LABEL_DISTANCE` in `const.h`).
pub const PORT_LABEL_DISTANCE: f64 = 10.0;

/// Default port label angle in degrees (`-25.0`, `PORT_LABEL_ANGLE` in `const.h`).
pub const PORT_LABEL_ANGLE_DEG: f64 = -25.0;

/// Obstacle clearance padding in points when routing splines around intermediate nodes.
const OBSTACLE_CLEARANCE: f64 = 8.0;

/// Classification of Graphviz primitive arrowhead shapes (`lib/common/arrows.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArrowKind {
    None,
    #[default]
    Normal,
    Inv,
    Crow,
    Vee,
    Tee,
    Box,
    Diamond,
    Dot,
    Curve,
    Icurve,
}

impl ArrowKind {
    /// Returns the length factor relative to `DEFAULT_ARROW_LENGTH` (`10.0` pt) from `Arrowtypes[]` in `arrows.c`.
    #[inline]
    pub const fn len_factor(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Normal
            | Self::Inv
            | Self::Crow
            | Self::Vee
            | Self::Box
            | Self::Curve
            | Self::Icurve => 1.0,
            Self::Tee => 0.5,
            Self::Diamond => 1.2,
            Self::Dot => 0.8,
        }
    }
}

/// Parsed arrowhead specification including open/half modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrowSpec {
    pub kind: ArrowKind,
    pub open: bool,
    pub left_only: bool,
    pub right_only: bool,
}

impl Default for ArrowSpec {
    fn default() -> Self {
        Self {
            kind: ArrowKind::Normal,
            open: false,
            left_only: false,
            right_only: false,
        }
    }
}

/// Parses a Graphviz arrowhead specification string (which may be a compound of up to 4 primitive
/// shapes such as `"normal"`, `"inv"`, `"odot"`, `"invodot"`, `"vee"`, `"diamond"`, `"none"`),
/// returning the list of parsed [`ArrowSpec`] primitives from tip to base.
pub fn parse_arrow_specs(spec_str: &str) -> Vec<ArrowSpec> {
    let mut rest = spec_str.trim();
    if rest.is_empty() {
        return vec![ArrowSpec::default()];
    }
    if rest.eq_ignore_ascii_case("none") {
        return Vec::new();
    }

    let mut result = Vec::new();
    while !rest.is_empty() && result.len() < 4 {
        // Synonym check first (`"invempty"` in `Arrowsynonyms[]`).
        if let Some(after) = strip_prefix_ci(rest, "invempty") {
            result.push(ArrowSpec {
                kind: ArrowKind::Inv,
                open: true,
                left_only: false,
                right_only: false,
            });
            rest = after;
            continue;
        }
        if let Some(after) = strip_prefix_ci(rest, "open") {
            result.push(ArrowSpec {
                kind: ArrowKind::Vee,
                open: false,
                left_only: false,
                right_only: false,
            });
            rest = after;
            continue;
        }
        if let Some(after) = strip_prefix_ci(rest, "empty") {
            result.push(ArrowSpec {
                kind: ArrowKind::Normal,
                open: true,
                left_only: false,
                right_only: false,
            });
            rest = after;
            continue;
        }
        if let Some(after) = strip_prefix_ci(rest, "halfopen") {
            result.push(ArrowSpec {
                kind: ArrowKind::Vee,
                open: false,
                left_only: true,
                right_only: false,
            });
            rest = after;
            continue;
        }

        let mut open = false;
        let mut left_only = false;
        let mut right_only = false;

        loop {
            if let Some(after) = strip_prefix_ci(rest, "half") {
                left_only = true;
                rest = after;
            } else if let Some(after) = strip_prefix_ci(rest, "o") {
                open = true;
                rest = after;
            } else if let Some(after) = strip_prefix_ci(rest, "e") {
                open = true;
                rest = after;
            } else if let Some(after) = strip_prefix_ci(rest, "l") {
                left_only = true;
                rest = after;
            } else if let Some(after) = strip_prefix_ci(rest, "r") {
                right_only = true;
                rest = after;
            } else {
                break;
            }
        }

        let prims: &[(&str, ArrowKind)] = &[
            ("normal", ArrowKind::Normal),
            ("crow", ArrowKind::Crow),
            ("tee", ArrowKind::Tee),
            ("box", ArrowKind::Box),
            ("diamond", ArrowKind::Diamond),
            ("dot", ArrowKind::Dot),
            ("none", ArrowKind::None),
            ("inv", ArrowKind::Inv),
            ("vee", ArrowKind::Vee),
            ("icurve", ArrowKind::Icurve),
            ("curve", ArrowKind::Curve),
        ];

        let mut matched = false;
        for &(name, kind) in prims {
            if let Some(after) = strip_prefix_ci(rest, name) {
                if kind != ArrowKind::None {
                    result.push(ArrowSpec {
                        kind,
                        open,
                        left_only,
                        right_only,
                    });
                }
                rest = after;
                matched = true;
                break;
            }
        }

        if !matched {
            if open || left_only || right_only {
                result.push(ArrowSpec {
                    kind: ArrowKind::Normal,
                    open,
                    left_only,
                    right_only,
                });
            } else if result.is_empty() {
                // Unknown arrow name: default to normal.
                result.push(ArrowSpec::default());
            }
            break;
        }
    }

    result
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Computes the total arrowhead length in points for an arrow specification string and `arrowsize` multiplier.
pub fn compute_arrow_length(spec_str: &str, arrowsize: f64) -> f64 {
    if arrowsize <= 0.0 {
        return 0.0;
    }
    let specs = parse_arrow_specs(spec_str);
    let total_factor: f64 = specs.iter().map(|s| s.kind.len_factor()).sum();
    total_factor * arrowsize * DEFAULT_ARROW_LENGTH
}

/// Computes the 3 triangle vertices `[tip, base_left, base_right]` for a standard arrowhead
/// whose tip is at `tip` and whose base is at `stem_end` (or along direction `dir_to_tip`).
pub fn arrow_polygon_points(tip: Point, stem_end: Point, arrowsize: f64) -> [Point; 3] {
    let scale = arrowsize.max(0.0);
    let len = DEFAULT_ARROW_LENGTH * scale;
    let half_w = DEFAULT_ARROW_HALF_WIDTH * scale;

    let dir = Point::new(tip.x - stem_end.x, tip.y - stem_end.y).normalized_or_zero();
    let dir = if dir.x == 0.0 && dir.y == 0.0 {
        Point::new(0.0, -1.0)
    } else {
        dir
    };

    let base_center = Point::new(tip.x - dir.x * len, tip.y - dir.y * len);
    let perp = Point::new(-dir.y * half_w, dir.x * half_w);
    [
        tip,
        Point::new(base_center.x + perp.x, base_center.y + perp.y),
        Point::new(base_center.x - perp.x, base_center.y - perp.y),
    ]
}

/// Resolves whether an edge has a start arrow (`arrowtail`) and/or end arrow (`arrowhead`),
/// returning `(start_arrow_len, end_arrow_len)` in points.
pub fn resolve_edge_arrow_lengths(edge: &EdgeData) -> (f64, f64) {
    let default_dir = if edge.attrs.get("_undirected").is_some_and(|v| v == "true") {
        "none"
    } else {
        "forward"
    };

    let dir = edge.attrs.get("dir").map_or(default_dir, |s| s.trim());

    let (mut has_start, mut has_end) = match dir.to_ascii_lowercase().as_str() {
        "forward" => (false, true),
        "back" => (true, false),
        "both" => (true, true),
        "none" => (false, false),
        _ => (false, default_dir == "forward"),
    };

    let arrowsize = edge
        .attrs
        .get("arrowsize")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(1.0)
        .max(0.0);

    let arrowhead_str = edge.attrs.get("arrowhead").map_or("normal", |s| s.trim());
    let arrowtail_str = edge.attrs.get("arrowtail").map_or("normal", |s| s.trim());

    if arrowhead_str.eq_ignore_ascii_case("none") {
        has_end = false;
    }
    if arrowtail_str.eq_ignore_ascii_case("none") {
        has_start = false;
    }

    let start_len = if has_start {
        compute_arrow_length(arrowtail_str, arrowsize)
    } else {
        0.0
    };
    let end_len = if has_end {
        compute_arrow_length(arrowhead_str, arrowsize)
    } else {
        0.0
    };

    (start_len, end_len)
}

fn effective_ports(edge: &EdgeData) -> (Port, Port) {
    let mut tail_port = edge.tail_port.clone();
    let mut head_port = edge.head_port.clone();

    if tail_port == Port::default()
        && let Some(tp) = edge.attrs.get("tailport")
    {
        tail_port = parse_port_attr(tp);
    }
    if head_port == Port::default()
        && let Some(hp) = edge.attrs.get("headport")
    {
        head_port = parse_port_attr(hp);
    }
    (tail_port, head_port)
}

fn parse_port_attr(s: &str) -> Port {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Port::default();
    }
    if let Some((p1, p2)) = trimmed.split_once(':') {
        Port {
            name: Some(p1.to_string()),
            compass: Some(p2.to_string()),
        }
    } else {
        Port {
            name: Some(trimmed.to_string()),
            compass: None,
        }
    }
}

/// Evaluates a single cubic Bezier curve `(p0, p1, p2, p3)` at parameter `t` in `[0.0, 1.0]`.
#[inline]
pub fn eval_cubic_bezier(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;
    Point::new(
        uuu * p0.x + 3.0 * uu * t * p1.x + 3.0 * u * tt * p2.x + ttt * p3.x,
        uuu * p0.y + 3.0 * uu * t * p1.y + 3.0 * u * tt * p2.y + ttt * p3.y,
    )
}

/// Evaluates the midpoint (`t = 0.5`) of a [`Spline`].
pub fn spline_midpoint(spline: &Spline) -> Option<Point> {
    let seg = spline.segments.first()?;
    if seg.points.len() < 4 {
        return seg.points.first().copied();
    }
    let num_cubics = (seg.points.len() - 1) / 3;
    if num_cubics == 0 {
        return seg.points.first().copied();
    }
    let mid_cubic = num_cubics / 2;
    let base = mid_cubic * 3;
    let t = if num_cubics % 2 == 1 { 0.5 } else { 0.0 };
    Some(eval_cubic_bezier(
        seg.points[base],
        seg.points[base + 1],
        seg.points[base + 2],
        seg.points[base + 3],
        t,
    ))
}

/// Computed positions for all four Graphviz edge labels (`label`, `xlabel`, `headlabel`, `taillabel`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeLabelPositions {
    pub label_pos: Option<Point>,
    pub xlabel_pos: Option<Point>,
    pub headlabel_pos: Option<Point>,
    pub taillabel_pos: Option<Point>,
}

/// Computes positions for `label`, `xlabel`, `headlabel`, and `taillabel` along `spline`,
/// following `addEdgeLabels` and `place_portlabel` in `lib/common/labels.c`.
pub fn compute_edge_label_positions(
    edge: &EdgeData,
    spline: &Spline,
    graph_name: &str,
) -> EdgeLabelPositions {
    let mut positions = EdgeLabelPositions::default();
    let Some(seg) = spline.segments.first() else {
        return positions;
    };
    if seg.points.len() < 4 {
        return positions;
    }

    let font_name = edge
        .attrs
        .get("labelfontname")
        .or_else(|| edge.attrs.get("fontname"))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_FONT_NAME);
    let font_size = edge
        .attrs
        .get("fontsize")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(DEFAULT_FONT_SIZE)
        .max(1.0);
    let label_font_size = edge
        .attrs
        .get("labelfontsize")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(DEFAULT_LABEL_FONT_SIZE)
        .max(1.0);

    let mid = spline_midpoint(spline).unwrap_or(seg.points[0]);
    let p_first = seg.start_arrow.unwrap_or(seg.points[0]);
    let p_last = seg.end_arrow.unwrap_or(*seg.points.last().unwrap());
    let chord = Point::new(p_last.x - p_first.x, p_last.y - p_first.y);
    let unit_dir = chord.normalized_or_zero();
    let perp = if unit_dir.x == 0.0 && unit_dir.y == 0.0 {
        Point::new(1.0, 0.0)
    } else {
        Point::new(-unit_dir.y, unit_dir.x)
    };

    let ctx = LabelSubstContext {
        node_name: "",
        graph_name,
        tail_name: "",
        head_name: "",
        is_edge: true,
        is_directed: true,
        tail_port: edge.tail_port.name.as_deref(),
        head_port: edge.head_port.name.as_deref(),
        label_text: "",
    };

    if let Some(lbl) = edge.attrs.get("label").filter(|s| !s.is_empty()) {
        let measured = measure_multiline_label_with_context(lbl, &ctx, font_name, font_size);
        let offset = (measured.width.min(measured.height) * 0.25).clamp(4.0, 12.0);
        positions.label_pos = Some(Point::new(mid.x + perp.x * offset, mid.y + perp.y * offset));
    }

    if let Some(xlbl) = edge.attrs.get("xlabel").filter(|s| !s.is_empty()) {
        let measured = measure_multiline_label_with_context(xlbl, &ctx, font_name, font_size);
        let offset = (measured.width.min(measured.height) * 0.25).clamp(4.0, 12.0);
        positions.xlabel_pos = Some(Point::new(mid.x - perp.x * offset, mid.y - perp.y * offset));
    }

    let label_dist = edge
        .attrs
        .get("labeldistance")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(1.0)
        .max(0.0)
        * PORT_LABEL_DISTANCE;
    let label_angle_rad = edge
        .attrs
        .get("labelangle")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(PORT_LABEL_ANGLE_DEG)
        .to_radians();

    if let Some(hlbl) = edge.attrs.get("headlabel").filter(|s| !s.is_empty()) {
        let _ = measure_multiline_label_with_context(hlbl, &ctx, font_name, label_font_size);
        let n = seg.points.len();
        let in_vec = Point::new(
            seg.points[n - 2].x - p_last.x,
            seg.points[n - 2].y - p_last.y,
        )
        .normalized_or_zero();
        let base_dir = if in_vec.x == 0.0 && in_vec.y == 0.0 {
            Point::new(-unit_dir.x, -unit_dir.y)
        } else {
            in_vec
        };
        let cos_a = label_angle_rad.cos();
        let sin_a = label_angle_rad.sin();
        let rot = Point::new(
            base_dir.x * cos_a - base_dir.y * sin_a,
            base_dir.x * sin_a + base_dir.y * cos_a,
        );
        positions.headlabel_pos = Some(Point::new(
            p_last.x + rot.x * label_dist,
            p_last.y + rot.y * label_dist,
        ));
    }

    if let Some(tlbl) = edge.attrs.get("taillabel").filter(|s| !s.is_empty()) {
        let _ = measure_multiline_label_with_context(tlbl, &ctx, font_name, label_font_size);
        let out_vec = Point::new(seg.points[1].x - p_first.x, seg.points[1].y - p_first.y)
            .normalized_or_zero();
        let base_dir = if out_vec.x == 0.0 && out_vec.y == 0.0 {
            unit_dir
        } else {
            out_vec
        };
        let neg_angle = -label_angle_rad;
        let cos_a = neg_angle.cos();
        let sin_a = neg_angle.sin();
        let rot = Point::new(
            base_dir.x * cos_a - base_dir.y * sin_a,
            base_dir.x * sin_a + base_dir.y * cos_a,
        );
        positions.taillabel_pos = Some(Point::new(
            p_first.x + rot.x * label_dist,
            p_first.y + rot.y * label_dist,
        ));
    }

    positions
}

/// Routes a cubic Bezier [`Spline`] and optional `label_pos` between `tail` and `head`
/// around `obstacles`, clipping endpoints to node/port boundaries and shortening for arrowheads.
pub fn route_edge_spline(
    tail: &NodeData,
    head: &NodeData,
    edge: &EdgeData,
    obstacles: &[BoundingBox],
    rankdir: &str,
) -> (Spline, Option<Point>) {
    let parallel_index = edge
        .attrs
        .get("_multi_idx")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let parallel_count = edge
        .attrs
        .get("_multi_cnt")
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    route_edge_spline_with_offset(
        tail,
        head,
        edge,
        obstacles,
        rankdir,
        parallel_index,
        parallel_count,
    )
}

/// Routes a cubic Bezier [`Spline`] with explicit `parallel_index` (`0..parallel_count`)
/// for multi-edge bundles between the same pair of nodes.
pub fn route_edge_spline_with_offset(
    tail: &NodeData,
    head: &NodeData,
    edge: &EdgeData,
    obstacles: &[BoundingBox],
    rankdir: &str,
    parallel_index: usize,
    parallel_count: usize,
) -> (Spline, Option<Point>) {
    let (tail_port, head_port) = effective_ports(edge);
    let (start_arrow_len, end_arrow_len) = resolve_edge_arrow_lengths(edge);

    // 1. Self-loop (`tail.id == head.id` or identical positions with same id).
    if tail.id == head.id {
        return route_self_loop(
            tail,
            edge,
            &tail_port,
            &head_port,
            rankdir,
            parallel_index,
            start_arrow_len,
            end_arrow_len,
        );
    }

    let tail_offset = resolve_port_offset(tail, &tail_port);
    let head_offset = resolve_port_offset(head, &head_port);
    let p_tail_origin = Point::new(tail.pos.x + tail_offset.x, tail.pos.y + tail_offset.y);
    let p_head_origin = Point::new(head.pos.x + head_offset.x, head.pos.y + head_offset.y);

    // Compute parallel multi-edge perpendicular offset if applicable.
    let explicit_offset = edge
        .attrs
        .get("_parallel_offset")
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    let multi_offset = if explicit_offset.abs() > 1e-6 {
        explicit_offset
    } else if parallel_count > 1 {
        let center_idx = ((parallel_count - 1) as f64) / 2.0;
        ((parallel_index as f64) - center_idx) * 18.0
    } else {
        0.0
    };

    // 2. Check for intermediate obstacle collisions along `(p_tail_origin, p_head_origin)`.
    let detour_waypoints = if multi_offset.abs() <= 1e-6 && !obstacles.is_empty() {
        compute_obstacle_detour(p_tail_origin, p_head_origin, tail, head, obstacles)
    } else {
        Vec::new()
    };

    let mut raw_points: Vec<Point> = if !detour_waypoints.is_empty() {
        build_spline_through_waypoints(tail, head, &tail_port, &head_port, &detour_waypoints)
    } else if multi_offset.abs() > 1e-6 {
        let chord = Point::new(
            p_head_origin.x - p_tail_origin.x,
            p_head_origin.y - p_tail_origin.y,
        );
        let dir = chord.normalized_or_zero();
        let perp = Point::new(-dir.y, dir.x);
        let mid = Point::new(
            (p_tail_origin.x + p_head_origin.x) * 0.5 + perp.x * multi_offset,
            (p_tail_origin.y + p_head_origin.y) * 0.5 + perp.y * multi_offset,
        );
        let p0 = clip_ray_to_node_boundary(tail, &tail_port, mid);
        let p3 = clip_ray_to_node_boundary(head, &head_port, mid);
        let c1 = Point::new(
            p0.x + (p3.x - p0.x) / 3.0 + perp.x * multi_offset,
            p0.y + (p3.y - p0.y) / 3.0 + perp.y * multi_offset,
        );
        let c2 = Point::new(
            p0.x + 2.0 * (p3.x - p0.x) / 3.0 + perp.x * multi_offset,
            p0.y + 2.0 * (p3.y - p0.y) / 3.0 + perp.y * multi_offset,
        );
        vec![p0, c1, c2, p3]
    } else {
        let p0 = clip_ray_to_node_boundary(tail, &tail_port, p_head_origin);
        let p3 = clip_ray_to_node_boundary(head, &head_port, p_tail_origin);
        let c1 = Point::new(p0.x + (p3.x - p0.x) / 3.0, p0.y + (p3.y - p0.y) / 3.0);
        let c2 = Point::new(
            p0.x + 2.0 * (p3.x - p0.x) / 3.0,
            p0.y + 2.0 * (p3.y - p0.y) / 3.0,
        );
        vec![p0, c1, c2, p3]
    };

    // 3. Apply arrowhead shortening at start and end (`arrowStartClip` / `arrowEndClip` in `arrows.c`).
    let (start_arrow, end_arrow) =
        clip_spline_arrowheads(&mut raw_points, start_arrow_len, end_arrow_len);

    let spline = Spline {
        segments: vec![BezierSegment {
            points: raw_points,
            start_arrow,
            end_arrow,
        }],
    };

    let label_pos = resolve_primary_edge_label_pos(edge, &spline);
    (spline, label_pos)
}

#[allow(clippy::too_many_arguments)]
fn route_self_loop(
    node: &NodeData,
    edge: &EdgeData,
    tail_port: &Port,
    head_port: &Port,
    rankdir: &str,
    parallel_index: usize,
    start_arrow_len: f64,
    end_arrow_len: f64,
) -> (Spline, Option<Point>) {
    let rx = (node.width * POINTS_PER_INCH / 2.0).max(9.0);
    let ry = (node.height * POINTS_PER_INCH / 2.0).max(9.0);
    let step = SELF_EDGE_SIZE * ((parallel_index + 1) as f64);
    let is_lr = matches!(rankdir.trim().to_ascii_uppercase().as_str(), "LR" | "RL");

    let mut raw_points = if is_lr {
        // Tear-drop loop above the top of the node (`selfTop` in `splines.c`).
        let spread_x = (rx * 0.45).max(6.0);
        let aim_out = Point::new(node.pos.x - spread_x, node.pos.y + ry + step);
        let aim_in = Point::new(node.pos.x + spread_x, node.pos.y + ry + step);
        let p0 = clip_ray_to_node_boundary(node, tail_port, aim_out);
        let p3 = clip_ray_to_node_boundary(node, head_port, aim_in);
        let peak_y = p0.y.max(p3.y) + step;
        let c1 = Point::new(p0.x - spread_x * 0.6, peak_y);
        let c2 = Point::new(p3.x + spread_x * 0.6, peak_y);
        vec![p0, c1, c2, p3]
    } else {
        // Tear-drop loop to the right of the node (`selfRight` in `splines.c`).
        let spread_y = (ry * 0.45).max(6.0);
        let aim_out = Point::new(node.pos.x + rx + step, node.pos.y + spread_y);
        let aim_in = Point::new(node.pos.x + rx + step, node.pos.y - spread_y);
        let p0 = clip_ray_to_node_boundary(node, tail_port, aim_out);
        let p3 = clip_ray_to_node_boundary(node, head_port, aim_in);
        let peak_x = p0.x.max(p3.x) + step;
        let c1 = Point::new(peak_x, p0.y + spread_y * 0.6);
        let c2 = Point::new(peak_x, p3.y - spread_y * 0.6);
        vec![p0, c1, c2, p3]
    };

    let (start_arrow, end_arrow) =
        clip_spline_arrowheads(&mut raw_points, start_arrow_len, end_arrow_len);

    let spline = Spline {
        segments: vec![BezierSegment {
            points: raw_points,
            start_arrow,
            end_arrow,
        }],
    };

    let label_pos = if has_any_edge_label(edge) {
        let apex = spline_midpoint(&spline).unwrap_or(node.pos);
        if is_lr {
            Some(Point::new(apex.x, apex.y + 8.0))
        } else {
            Some(Point::new(apex.x + 12.0, apex.y))
        }
    } else {
        None
    };

    (spline, label_pos)
}

fn has_any_edge_label(edge: &EdgeData) -> bool {
    ["label", "xlabel", "headlabel", "taillabel"]
        .iter()
        .any(|k| edge.attrs.get(*k).is_some_and(|v| !v.is_empty()))
}

fn resolve_primary_edge_label_pos(edge: &EdgeData, spline: &Spline) -> Option<Point> {
    if !has_any_edge_label(edge) {
        return None;
    }
    let positions = compute_edge_label_positions(edge, spline, "");
    positions
        .label_pos
        .or(positions.xlabel_pos)
        .or(positions.headlabel_pos)
        .or(positions.taillabel_pos)
        .or_else(|| spline_midpoint(spline))
}

fn clip_spline_arrowheads(
    points: &mut [Point],
    start_arrow_len: f64,
    end_arrow_len: f64,
) -> (Option<Point>, Option<Point>) {
    let n = points.len();
    if n < 4 {
        return (None, None);
    }

    let orig_start = points[0];
    let orig_end = points[n - 1];

    let start_arrow = if start_arrow_len > 0.0 {
        let next_pt = if points[1].dist(orig_start) > 1e-6 {
            points[1]
        } else {
            points[3]
        };
        let seg_dist = orig_start.dist(points[3]);
        let eff_len = start_arrow_len.min(seg_dist * 0.85);
        let dir =
            Point::new(next_pt.x - orig_start.x, next_pt.y - orig_start.y).normalized_or_zero();
        let new_p0 = Point::new(
            orig_start.x + dir.x * eff_len,
            orig_start.y + dir.y * eff_len,
        );
        points[0] = new_p0;
        if n == 4 && end_arrow_len <= 0.0 {
            points[1] = Point::new(
                new_p0.x + (points[3].x - new_p0.x) / 3.0,
                new_p0.y + (points[3].y - new_p0.y) / 3.0,
            );
        }
        Some(orig_start)
    } else {
        None
    };

    let end_arrow = if end_arrow_len > 0.0 {
        let prev_pt = if points[n - 2].dist(orig_end) > 1e-6 {
            points[n - 2]
        } else {
            points[n - 4]
        };
        let seg_dist = orig_end.dist(points[n - 4]);
        let eff_len = end_arrow_len.min(seg_dist * 0.85);
        let dir = Point::new(prev_pt.x - orig_end.x, prev_pt.y - orig_end.y).normalized_or_zero();
        let new_end = Point::new(orig_end.x + dir.x * eff_len, orig_end.y + dir.y * eff_len);
        points[n - 1] = new_end;
        if n == 4 {
            let p0 = points[0];
            let chord_x = new_end.x - p0.x;
            let chord_y = new_end.y - p0.y;
            let is_collinear = ((points[1].x - p0.x) * chord_y - (points[1].y - p0.y) * chord_x)
                .abs()
                < 1e-3
                && ((points[2].x - p0.x) * chord_y - (points[2].y - p0.y) * chord_x).abs() < 1e-3;
            if is_collinear {
                points[1] = Point::new(p0.x + chord_x / 3.0, p0.y + chord_y / 3.0);
                points[2] = Point::new(p0.x + 2.0 * chord_x / 3.0, p0.y + 2.0 * chord_y / 3.0);
            }
        }
        Some(orig_end)
    } else {
        None
    };

    (start_arrow, end_arrow)
}

fn box_contains_point(b: BoundingBox, p: Point, pad: f64) -> bool {
    p.x >= b.ll.x - pad && p.x <= b.ur.x + pad && p.y >= b.ll.y - pad && p.y <= b.ur.y + pad
}

/// Tests whether line segment `(a, b)` intersects `bbox` expanded by `pad` using Liang-Barsky clipping.
pub fn segment_intersects_box(a: Point, b: Point, bbox: BoundingBox, pad: f64) -> bool {
    let min_x = bbox.ll.x - pad;
    let max_x = bbox.ur.x + pad;
    let min_y = bbox.ll.y - pad;
    let max_y = bbox.ur.y + pad;

    let dx = b.x - a.x;
    let dy = b.y - a.y;

    let p = [-dx, dx, -dy, dy];
    let q = [a.x - min_x, max_x - a.x, a.y - min_y, max_y - a.y];

    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;

    for i in 0..4 {
        if p[i].abs() <= 1e-12 {
            if q[i] < 0.0 {
                return false;
            }
        } else {
            let r = q[i] / p[i];
            if p[i] < 0.0 {
                if r > t1 {
                    return false;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return false;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }

    t0 <= t1
}

fn compute_obstacle_detour(
    p_start: Point,
    p_end: Point,
    tail: &NodeData,
    head: &NodeData,
    obstacles: &[BoundingBox],
) -> Vec<Point> {
    let blocking: Vec<BoundingBox> = obstacles
        .iter()
        .copied()
        .filter(|b| {
            !box_contains_point(*b, tail.pos, 1.0)
                && !box_contains_point(*b, head.pos, 1.0)
                && !box_contains_point(*b, p_start, 1.0)
                && !box_contains_point(*b, p_end, 1.0)
                && segment_intersects_box(p_start, p_end, *b, OBSTACLE_CLEARANCE)
        })
        .collect();

    if blocking.is_empty() {
        return Vec::new();
    }

    // Union the blocking obstacle bounds along the path to form a clean bypass envelope.
    let mut env = blocking[0];
    for b in &blocking[1..] {
        env.expand_to_include(b.ll);
        env.expand_to_include(b.ur);
    }

    let dx = p_end.x - p_start.x;
    let dy = p_end.y - p_start.y;

    if dy.abs() >= dx.abs() {
        // Primarily vertical edge: detour either left (`env.ll.x - pad`) or right (`env.ur.x + pad`).
        let mid_y = (env.ll.y + env.ur.y) * 0.5;
        let left_wp = Point::new(env.ll.x - OBSTACLE_CLEARANCE - 6.0, mid_y);
        let right_wp = Point::new(env.ur.x + OBSTACLE_CLEARANCE + 6.0, mid_y);
        let d_left = p_start.dist(left_wp) + left_wp.dist(p_end);
        let d_right = p_start.dist(right_wp) + right_wp.dist(p_end);
        if d_right <= d_left {
            vec![right_wp]
        } else {
            vec![left_wp]
        }
    } else {
        // Primarily horizontal edge: detour either above (`env.ur.y + pad`) or below (`env.ll.y - pad`).
        let mid_x = (env.ll.x + env.ur.x) * 0.5;
        let top_wp = Point::new(mid_x, env.ur.y + OBSTACLE_CLEARANCE + 6.0);
        let bot_wp = Point::new(mid_x, env.ll.y - OBSTACLE_CLEARANCE - 6.0);
        let d_top = p_start.dist(top_wp) + top_wp.dist(p_end);
        let d_bot = p_start.dist(bot_wp) + bot_wp.dist(p_end);
        if d_top <= d_bot {
            vec![top_wp]
        } else {
            vec![bot_wp]
        }
    }
}

fn build_spline_through_waypoints(
    tail: &NodeData,
    head: &NodeData,
    tail_port: &Port,
    head_port: &Port,
    waypoints: &[Point],
) -> Vec<Point> {
    let first_wp = waypoints[0];
    let last_wp = *waypoints.last().unwrap();
    let p0 = clip_ray_to_node_boundary(tail, tail_port, first_wp);
    let p_end = clip_ray_to_node_boundary(head, head_port, last_wp);

    if waypoints.len() == 1 {
        let wp = waypoints[0];
        // Fit a single smooth cubic Bezier arch whose midpoint passes through `wp`:
        // For a symmetric cubic Bezier, B(0.5) = 0.125*(P0 + P3) + 0.375*(C1 + C2) = wp.
        let mid_chord = Point::new((p0.x + p_end.x) * 0.5, (p0.y + p_end.y) * 0.5);
        let pull = Point::new(
            (wp.x - mid_chord.x) * (4.0 / 3.0),
            (wp.y - mid_chord.y) * (4.0 / 3.0),
        );
        let c1 = Point::new(
            p0.x + (p_end.x - p0.x) / 3.0 + pull.x,
            p0.y + (p_end.y - p0.y) / 3.0 + pull.y,
        );
        let c2 = Point::new(
            p0.x + 2.0 * (p_end.x - p0.x) / 3.0 + pull.x,
            p0.y + 2.0 * (p_end.y - p0.y) / 3.0 + pull.y,
        );
        return vec![p0, c1, c2, p_end];
    }

    // General Catmull-Rom to cubic Bezier chain across `[p0, waypoints..., p_end]`.
    let mut poly = Vec::with_capacity(waypoints.len() + 2);
    poly.push(p0);
    poly.extend_from_slice(waypoints);
    poly.push(p_end);

    let mut bz = Vec::with_capacity((poly.len() - 1) * 3 + 1);
    bz.push(poly[0]);
    for i in 0..(poly.len() - 1) {
        let prev = if i == 0 { poly[0] } else { poly[i - 1] };
        let cur = poly[i];
        let next = poly[i + 1];
        let after = if i + 2 < poly.len() {
            poly[i + 2]
        } else {
            poly[i + 1]
        };
        let c1 = Point::new(
            cur.x + (next.x - prev.x) / 6.0,
            cur.y + (next.y - prev.y) / 6.0,
        );
        let c2 = Point::new(
            next.x - (after.x - cur.x) / 6.0,
            next.y - (after.y - cur.y) / 6.0,
        );
        bz.push(c1);
        bz.push(c2);
        bz.push(next);
    }
    bz
}
