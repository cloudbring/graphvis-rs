//! Phase 4 Visual & Geometric Differential Harness (`visual_diff`).
//!
//! 1. Measures the **C Graphviz 16.1.0 Noise Floor** by rendering every fixture and every engine
//!    (`dot`, `neato`, `fdp`, `sfdp`, `twopi`, `circo`, `osage`, `patchwork`) multiple times against
//!    itself and checking SVG byte determinism, pixel RMSE, and ink overlap.
//! 2. Measures **C vs. Rust (`gv-engine`) Layout & Visual Fidelity** across all 20 `.gv` fixtures:
//!    - Geometric metrics from `-Tjson0`: node width/height accuracy, bounding-box similarity,
//!      rank direction concordance, raw & bbox-normalized node center RMSE.
//!    - Visual raster metrics from `-Tsvg` rasterized via `rsvg-convert`: pixel similarity
//!      (`1 - MAE/255`), 8x8 block SSIM, soft blurred ink Dice (`sigma = 3px`), and composite score.
//! 3. Optionally exports side-by-side PNG renders, diff heatmaps, and a structured JSON report.

use gv_core::math;
use gv_engine::render_dot;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const C_DOT_BIN: &str = "/opt/homebrew/bin/dot";
const RSVG_CONVERT_BIN: &str = "/opt/homebrew/bin/rsvg-convert";
const SIPS_BIN: &str = "/usr/bin/sips";

#[derive(Debug, Clone, Serialize)]
struct NoiseFloorEntry {
    engine: String,
    fixture: String,
    svg_bytes_identical: bool,
    svg_len_run1: usize,
    svg_len_run2: usize,
    pixel_rmse: f64,
    soft_ink_dice: f64,
}

#[derive(Debug, Clone, Serialize)]
struct FixtureDiffMetrics {
    fixture: String,
    engine: String,
    node_count_c: usize,
    node_count_rs: usize,
    edge_count_c: usize,
    edge_count_rs: usize,
    c_bb: [f64; 2],
    rs_bb: [f64; 2],
    /// 1.0 - mean relative error in node (width, height)
    node_size_similarity: f64,
    /// 1.0 - mean relative error in graph bounding box (width, height)
    bb_similarity: f64,
    /// Fraction of directed edges with matching rank direction (sign of dy/dx along rankdir)
    rank_concordance: f64,
    /// Raw Euclidean node center RMSE in points
    raw_pos_rmse_pt: f64,
    /// 1.0 - raw_pos_rmse_pt / c_diag_pt
    raw_pos_similarity: f64,
    /// 1.0 - bbox-normalized node center RMSE / sqrt(2)
    norm_pos_similarity: f64,
    /// 1.0 - MAE / 255.0 on common canvas
    pixel_similarity: f64,
    /// 8x8 block Structural Similarity Index (SSIM) on luminance
    ssim_8x8: f64,
    /// Soft Dice overlap on blurred ink darkness (r = 4px box blur)
    soft_ink_dice: f64,
    /// Exact unblurred ink Dice overlap
    exact_ink_dice: f64,
    /// Weighted composite fidelity score (0.0 .. 100.0)
    composite_score: f64,
}

#[derive(Debug, Clone, Serialize)]
struct FullVisualDiffReport {
    noise_floor: Vec<NoiseFloorEntry>,
    fixtures: Vec<FixtureDiffMetrics>,
    summary: SummaryMetrics,
}

#[derive(Debug, Clone, Serialize)]
struct SummaryMetrics {
    fixture_count: usize,
    mean_node_size_similarity: f64,
    mean_bb_similarity: f64,
    mean_rank_concordance: f64,
    mean_raw_pos_similarity: f64,
    mean_norm_pos_similarity: f64,
    mean_pixel_similarity: f64,
    mean_ssim_8x8: f64,
    mean_soft_ink_dice: f64,
    mean_composite_score: f64,
}

#[derive(Debug, Clone)]
struct RgbImage {
    width: usize,
    height: usize,
    /// Row-major RGB bytes (`width * height * 3`)
    data: Vec<u8>,
}

impl RgbImage {
    fn luma_at(&self, x: usize, y: usize) -> f64 {
        let idx = (y * self.width + x) * 3;
        let r = f64::from(self.data[idx]);
        let g = f64::from(self.data[idx + 1]);
        let b = f64::from(self.data[idx + 2]);
        0.299 * r + 0.587 * g + 0.114 * b
    }

    fn rgb_at(&self, x: usize, y: usize) -> [u8; 3] {
        let idx = (y * self.width + x) * 3;
        [self.data[idx], self.data[idx + 1], self.data[idx + 2]]
    }
}

fn run_c_graphviz(dot_src: &str, engine: &str, format: &str) -> Result<String, String> {
    let mut child = Command::new(C_DOT_BIN)
        .arg(format!("-K{engine}"))
        .arg(format!("-T{format}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn {C_DOT_BIN}: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(dot_src.as_bytes())
            .map_err(|e| format!("Failed to write stdin: {e}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for {C_DOT_BIN}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "C dot failed ({engine}, {format}): {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Rasterizes an SVG string to an `RgbImage` using `rsvg-convert` and `sips -s format bmp`.
/// If `target_dims` is `Some((w, h))`, scales the SVG to `(w, h)` pixels.
fn rasterize_svg(
    svg_str: &str,
    tag: &str,
    target_dims: Option<(usize, usize)>,
    save_png_path: Option<&Path>,
) -> Result<RgbImage, String> {
    let pid = std::process::id();
    let tmp_png = PathBuf::from(format!("/tmp/gv_vdiff_{pid}_{tag}.png"));
    let tmp_bmp = PathBuf::from(format!("/tmp/gv_vdiff_{pid}_{tag}.bmp"));

    let mut cmd = Command::new(RSVG_CONVERT_BIN);
    cmd.arg("-b").arg("white");
    if let Some((w, h)) = target_dims {
        cmd.arg("-w")
            .arg(w.to_string())
            .arg("-h")
            .arg(h.to_string());
    }
    cmd.arg("-o").arg(&tmp_png);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn {RSVG_CONVERT_BIN}: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(svg_str.as_bytes())
            .map_err(|e| format!("Failed to write SVG to rsvg-convert: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for rsvg-convert: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "rsvg-convert failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    if let Some(dest) = save_png_path {
        let _ = fs::copy(&tmp_png, dest);
    }

    let sips_out = Command::new(SIPS_BIN)
        .arg("-s")
        .arg("format")
        .arg("bmp")
        .arg(&tmp_png)
        .arg("--out")
        .arg(&tmp_bmp)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run sips: {e}"))?;
    if !sips_out.status.success() {
        let _ = fs::remove_file(&tmp_png);
        return Err(format!(
            "sips failed: {}",
            String::from_utf8_lossy(&sips_out.stderr)
        ));
    }

    let bmp_bytes = fs::read(&tmp_bmp).map_err(|e| format!("Failed to read BMP: {e}"))?;
    let _ = fs::remove_file(&tmp_png);
    let _ = fs::remove_file(&tmp_bmp);

    decode_bmp(&bmp_bytes)
}

fn decode_bmp(bytes: &[u8]) -> Result<RgbImage, String> {
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        return Err("Invalid BMP header".to_string());
    }
    let pixel_offset = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    let width_i32 = i32::from_le_bytes(bytes[18..22].try_into().unwrap());
    let height_i32 = i32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let bpp = u16::from_le_bytes(bytes[28..30].try_into().unwrap());

    let width = width_i32.unsigned_abs() as usize;
    let height = height_i32.unsigned_abs() as usize;
    let top_down = height_i32 < 0;
    let bytes_per_pixel = (bpp / 8) as usize;
    if bytes_per_pixel < 3 || width == 0 || height == 0 {
        return Err(format!(
            "Unsupported BMP bpp={bpp} or dimensions {width}x{height}"
        ));
    }
    let row_stride = (width * bytes_per_pixel + 3) & !3;
    let mut data = vec![255u8; width * height * 3];

    for y in 0..height {
        let src_row = if top_down { y } else { height - 1 - y };
        let row_start = pixel_offset + src_row * row_stride;
        if row_start + width * bytes_per_pixel > bytes.len() {
            break;
        }
        for x in 0..width {
            let p = row_start + x * bytes_per_pixel;
            let b = bytes[p];
            let g = bytes[p + 1];
            let r = bytes[p + 2];
            let dst = (y * width + x) * 3;
            data[dst] = r;
            data[dst + 1] = g;
            data[dst + 2] = b;
        }
    }

    Ok(RgbImage {
        width,
        height,
        data,
    })
}

fn write_bmp_and_convert_to_png(img: &RgbImage, png_path: &Path) -> Result<(), String> {
    let width = img.width;
    let height = img.height;
    let row_stride = (width * 3 + 3) & !3;
    let pixel_bytes = row_stride * height;
    let file_size = 54 + pixel_bytes;

    let mut bmp = Vec::with_capacity(file_size);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&54u32.to_le_bytes());
    // DIB header (BITMAPINFOHEADER, 40 bytes)
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(width as i32).to_le_bytes());
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes()); // top-down
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
    bmp.extend_from_slice(&2835i32.to_le_bytes());
    bmp.extend_from_slice(&2835i32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());

    let pad = [0u8; 3];
    let pad_len = row_stride - width * 3;
    for y in 0..height {
        for x in 0..width {
            let [r, g, b] = img.rgb_at(x, y);
            bmp.push(b);
            bmp.push(g);
            bmp.push(r);
        }
        bmp.extend_from_slice(&pad[..pad_len]);
    }

    let tmp_bmp = png_path.with_extension("tmp.bmp");
    fs::write(&tmp_bmp, &bmp).map_err(|e| format!("Failed writing temp BMP: {e}"))?;
    let status = Command::new(SIPS_BIN)
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(&tmp_bmp)
        .arg("--out")
        .arg(png_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed running sips: {e}"))?;
    let _ = fs::remove_file(&tmp_bmp);
    if !status.success() {
        return Err("sips BMP->PNG conversion failed".to_string());
    }
    Ok(())
}

/// Box-blurs a 2D scalar buffer (`width x height`) with radius `r`.
fn box_blur(buf: &[f64], width: usize, height: usize, r: usize) -> Vec<f64> {
    if r == 0 || width == 0 || height == 0 {
        return buf.to_vec();
    }
    let mut temp = vec![0.0f64; width * height];
    let mut out = vec![0.0f64; width * height];
    let r_i = r as isize;

    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            let mut cnt = 0.0;
            for dx in -r_i..=r_i {
                let nx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                sum += buf[y * width + nx];
                cnt += 1.0;
            }
            temp[y * width + x] = sum / cnt;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            let mut cnt = 0.0;
            for dy in -r_i..=r_i {
                let ny = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                sum += temp[ny * width + x];
                cnt += 1.0;
            }
            out[y * width + x] = sum / cnt;
        }
    }
    out
}

/// Computes visual similarity metrics between two `RgbImage`s (already scaled to identical dimensions
/// or padded to common dimensions), plus an optional diff heatmap image.
fn compare_images(
    img_a: &RgbImage,
    img_b: &RgbImage,
    build_diff_image: bool,
) -> (f64, f64, f64, f64, f64, Option<RgbImage>) {
    let w = img_a.width.max(img_b.width).max(1);
    let h = img_a.height.max(img_b.height).max(1);
    let n = (w * h) as f64;

    let mut ink_a = vec![0.0f64; w * h];
    let mut ink_b = vec![0.0f64; w * h];
    let mut sum_abs_diff = 0.0f64;
    let mut sum_sq_diff = 0.0f64;

    let mut diff_data = if build_diff_image {
        vec![255u8; w * h * 3]
    } else {
        Vec::new()
    };

    for y in 0..h {
        for x in 0..w {
            let la = if x < img_a.width && y < img_a.height {
                img_a.luma_at(x, y)
            } else {
                255.0
            };
            let lb = if x < img_b.width && y < img_b.height {
                img_b.luma_at(x, y)
            } else {
                255.0
            };
            let ia = (255.0 - la).max(0.0);
            let ib = (255.0 - lb).max(0.0);
            ink_a[y * w + x] = ia;
            ink_b[y * w + x] = ib;

            let d = (la - lb).abs();
            sum_abs_diff += d;
            sum_sq_diff += d * d;

            if build_diff_image {
                let dst = (y * w + x) * 3;
                // Blue = C reference only, Red = Rust only, Dark Green/Grey = overlap match!
                let overlap = ia.min(ib);
                let only_a = (ia - overlap).max(0.0);
                let only_b = (ib - overlap).max(0.0);
                let r = (255.0 - overlap * 0.7 - only_a * 0.85).clamp(0.0, 255.0) as u8;
                let g =
                    (255.0 - overlap * 0.5 - only_a * 0.75 - only_b * 0.85).clamp(0.0, 255.0) as u8;
                let b = (255.0 - overlap * 0.7 - only_b * 0.85).clamp(0.0, 255.0) as u8;
                diff_data[dst] = r;
                diff_data[dst + 1] = g;
                diff_data[dst + 2] = b;
            }
        }
    }

    let mae = sum_abs_diff / n;
    let rmse = (sum_sq_diff / n).sqrt();
    let pixel_sim = (1.0 - mae / 255.0).clamp(0.0, 1.0);

    // Exact ink Dice
    let mut exact_inter = 0.0f64;
    let mut exact_sum = 0.0f64;
    for i in 0..(w * h) {
        exact_inter += ink_a[i].min(ink_b[i]);
        exact_sum += ink_a[i] + ink_b[i];
    }
    let exact_ink_dice = if exact_sum <= 1e-6 {
        1.0
    } else {
        (2.0 * exact_inter / exact_sum).clamp(0.0, 1.0)
    };

    // Soft blurred ink Dice (2 passes of radius-4 box blur ~ Gaussian sigma=3.5px)
    let blur_a = box_blur(&box_blur(&ink_a, w, h, 4), w, h, 4);
    let blur_b = box_blur(&box_blur(&ink_b, w, h, 4), w, h, 4);
    let mut soft_inter = 0.0f64;
    let mut soft_sum = 0.0f64;
    for i in 0..(w * h) {
        soft_inter += blur_a[i].min(blur_b[i]);
        soft_sum += blur_a[i] + blur_b[i];
    }
    let soft_ink_dice = if soft_sum <= 1e-6 {
        1.0
    } else {
        (2.0 * soft_inter / soft_sum).clamp(0.0, 1.0)
    };

    // 8x8 block SSIM on luminance
    let c1 = (0.01f64 * 255.0).powi(2);
    let c2 = (0.03f64 * 255.0).powi(2);
    let block = 8usize;
    let mut ssim_sum = 0.0f64;
    let mut ssim_cnt = 0.0f64;

    for by in (0..h).step_by(block) {
        for bx in (0..w).step_by(block) {
            let bw = block.min(w - bx);
            let bh = block.min(h - by);
            let bn = (bw * bh) as f64;
            let mut mean_a = 0.0;
            let mut mean_b = 0.0;
            for yy in by..(by + bh) {
                for xx in bx..(bx + bw) {
                    mean_a += 255.0 - ink_a[yy * w + xx];
                    mean_b += 255.0 - ink_b[yy * w + xx];
                }
            }
            mean_a /= bn;
            mean_b /= bn;

            let mut var_a = 0.0;
            let mut var_b = 0.0;
            let mut cov = 0.0;
            for yy in by..(by + bh) {
                for xx in bx..(bx + bw) {
                    let da = (255.0 - ink_a[yy * w + xx]) - mean_a;
                    let db = (255.0 - ink_b[yy * w + xx]) - mean_b;
                    var_a += da * da;
                    var_b += db * db;
                    cov += da * db;
                }
            }
            var_a /= bn;
            var_b /= bn;
            cov /= bn;

            let num = (2.0 * mean_a * mean_b + c1) * (2.0 * cov + c2);
            let den = (mean_a * mean_a + mean_b * mean_b + c1) * (var_a + var_b + c2);
            ssim_sum += (num / den).clamp(0.0, 1.0);
            ssim_cnt += 1.0;
        }
    }
    let ssim_8x8 = if ssim_cnt > 0.0 {
        ssim_sum / ssim_cnt
    } else {
        1.0
    };

    let diff_img = if build_diff_image {
        Some(RgbImage {
            width: w,
            height: h,
            data: diff_data,
        })
    } else {
        None
    };

    (
        rmse,
        pixel_sim,
        ssim_8x8,
        soft_ink_dice,
        exact_ink_dice,
        diff_img,
    )
}

#[derive(Debug, Clone)]
struct ParsedJsonLayout {
    bb_w: f64,
    bb_h: f64,
    /// `name -> (x, y, width_in, height_in)`
    nodes: BTreeMap<String, (f64, f64, f64, f64)>,
    /// `(tail_name, head_name)`
    edges: Vec<(String, String)>,
}

fn parse_json0_layout(json_str: &str) -> Result<ParsedJsonLayout, String> {
    let val: Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON0 output: {e}"))?;
    let bb_str = val.get("bb").and_then(Value::as_str).unwrap_or("0,0,54,36");
    let bb_nums: Vec<f64> = bb_str
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    let (bb_w, bb_h) = if bb_nums.len() == 4 {
        (
            (bb_nums[2] - bb_nums[0]).abs().max(1.0),
            (bb_nums[3] - bb_nums[1]).abs().max(1.0),
        )
    } else {
        (54.0, 36.0)
    };

    let sg_cnt = val
        .get("_subgraph_cnt")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut gvid_to_name = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    if let Some(objs) = val.get("objects").and_then(Value::as_array) {
        for obj in objs.iter().skip(sg_cnt) {
            if obj.get("nodes").is_some() || obj.get("subgraphs").is_some() {
                continue;
            }
            let Some(name) = obj.get("name").and_then(Value::as_str) else {
                continue;
            };
            if let Some(gvid) = obj.get("_gvid").and_then(Value::as_u64) {
                gvid_to_name.insert(gvid, name.to_string());
            }
            let pos_str = obj.get("pos").and_then(Value::as_str).unwrap_or("0,0");
            let mut coords = pos_str
                .split(',')
                .filter_map(|s| s.trim().parse::<f64>().ok());
            let x = coords.next().unwrap_or(0.0);
            let y = coords.next().unwrap_or(0.0);
            let w = obj
                .get("width")
                .and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .or_else(|| v.as_f64())
                })
                .unwrap_or(0.75);
            let h = obj
                .get("height")
                .and_then(|v| {
                    v.as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .or_else(|| v.as_f64())
                })
                .unwrap_or(0.5);
            nodes.insert(name.to_string(), (x, y, w, h));
        }
    }

    let mut edges = Vec::new();
    if let Some(edgs) = val.get("edges").and_then(Value::as_array) {
        for e in edgs {
            let tail_id = e.get("tail").and_then(Value::as_u64);
            let head_id = e.get("head").and_then(Value::as_u64);
            if let (Some(t), Some(h)) = (tail_id, head_id)
                && let (Some(tn), Some(hn)) = (gvid_to_name.get(&t), gvid_to_name.get(&h))
            {
                edges.push((tn.clone(), hn.clone()));
            }
        }
    }

    Ok(ParsedJsonLayout {
        bb_w,
        bb_h,
        nodes,
        edges,
    })
}

fn evaluate_fixture(
    fixture_name: &str,
    dot_src: &str,
    engine_str: &str,
    out_dir: Option<&Path>,
) -> Result<FixtureDiffMetrics, String> {
    let c_json_str = run_c_graphviz(dot_src, engine_str, "json0")?;
    let c_svg_str = run_c_graphviz(dot_src, engine_str, "svg")?;

    let rs_json_str = render_dot(dot_src, engine_str, "json0")
        .map_err(|e| format!("Rust render Json0 failed on {fixture_name}: {e}"))?;
    let rs_svg_str = render_dot(dot_src, engine_str, "svg")
        .map_err(|e| format!("Rust render Svg failed on {fixture_name}: {e}"))?;

    let c_layout = parse_json0_layout(&c_json_str)?;
    let rs_layout = parse_json0_layout(&rs_json_str)?;

    // 1. Node size similarity
    let mut size_err_sum = 0.0f64;
    let mut shared_count = 0usize;
    let mut sq_dist_raw = 0.0f64;
    let mut sq_dist_norm = 0.0f64;

    for (name, &(cx, cy, cw, ch)) in &c_layout.nodes {
        if let Some(&(rx, ry, rw, rh)) = rs_layout.nodes.get(name) {
            shared_count += 1;
            let w_err = (rw - cw).abs() / cw.max(0.01);
            let h_err = (rh - ch).abs() / ch.max(0.01);
            size_err_sum += 0.5 * (w_err.min(1.0) + h_err.min(1.0));

            let dx = rx - cx;
            let dy = ry - cy;
            sq_dist_raw += dx * dx + dy * dy;

            let ncx = cx / c_layout.bb_w.max(1.0);
            let ncy = cy / c_layout.bb_h.max(1.0);
            let nrx = rx / rs_layout.bb_w.max(1.0);
            let nry = ry / rs_layout.bb_h.max(1.0);
            sq_dist_norm += (nrx - ncx).powi(2) + (nry - ncy).powi(2);
        }
    }

    let node_size_similarity = if shared_count > 0 {
        (1.0 - size_err_sum / (shared_count as f64)).clamp(0.0, 1.0)
    } else {
        1.0
    };

    // 2. Bounding box similarity
    let w_rel_err =
        (rs_layout.bb_w - c_layout.bb_w).abs() / c_layout.bb_w.max(rs_layout.bb_w).max(1.0);
    let h_rel_err =
        (rs_layout.bb_h - c_layout.bb_h).abs() / c_layout.bb_h.max(rs_layout.bb_h).max(1.0);
    let bb_similarity = (1.0 - 0.5 * (w_rel_err + h_rel_err)).clamp(0.0, 1.0);

    // 3. Rank concordance
    let mut rank_matches = 0usize;
    let mut rank_total = 0usize;
    for (tn, hn) in &c_layout.edges {
        if tn == hn {
            continue;
        }
        if let (Some(&(ctx, cty, _, _)), Some(&(chx, chy, _, _))) =
            (c_layout.nodes.get(tn), c_layout.nodes.get(hn))
            && let (Some(&(rtx, rty, _, _)), Some(&(rhx, rhy, _, _))) =
                (rs_layout.nodes.get(tn), rs_layout.nodes.get(hn))
        {
            rank_total += 1;
            let c_dy = chy - cty;
            let c_dx = chx - ctx;
            let r_dy = rhy - rty;
            let r_dx = rhx - rtx;
            let sign_y_match = (c_dy.abs() < 2.0 && r_dy.abs() < 2.0) || (c_dy * r_dy > 0.0);
            let sign_x_match = (c_dx.abs() < 2.0 && r_dx.abs() < 2.0) || (c_dx * r_dx > 0.0);
            let is_lr = dot_src.contains("rankdir=LR")
                || dot_src.contains("rankdir = LR")
                || dot_src.contains("rankdir=\"LR\"")
                || dot_src.contains("rankdir=RL");
            if is_lr {
                if c_dx.abs() >= 2.0 {
                    if sign_x_match {
                        rank_matches += 1;
                    }
                } else if sign_y_match {
                    rank_matches += 1;
                }
            } else if c_dy.abs() >= 2.0 {
                if sign_y_match {
                    rank_matches += 1;
                }
            } else if sign_x_match {
                rank_matches += 1;
            }
        }
    }
    let rank_concordance = if rank_total > 0 {
        (rank_matches as f64) / (rank_total as f64)
    } else {
        1.0
    };

    // 4. Node position RMSE & similarity
    let c_diag = math::hypot(c_layout.bb_w, c_layout.bb_h).max(1.0);
    let raw_pos_rmse_pt = if shared_count > 0 {
        (sq_dist_raw / (shared_count as f64)).sqrt()
    } else {
        0.0
    };
    let raw_pos_similarity = (1.0 - raw_pos_rmse_pt / c_diag).clamp(0.0, 1.0);

    let norm_pos_rmse = if shared_count > 0 {
        (sq_dist_norm / (shared_count as f64)).sqrt()
    } else {
        0.0
    };
    let norm_pos_similarity = (1.0 - norm_pos_rmse / std::f64::consts::SQRT_2).clamp(0.0, 1.0);

    // 5. Rasterize both SVGs to a common target canvas (capped at max 900px on longest side for fast, consistent comparison)
    let scale = (600.0 / c_layout.bb_w.max(c_layout.bb_h).max(1.0)).clamp(0.5, 2.0);
    let target_w = ((c_layout.bb_w * scale).round() as usize).clamp(64, 1024);
    let target_h = ((c_layout.bb_h * scale).round() as usize).clamp(64, 1024);

    let c_png_out = out_dir.map(|d| d.join(format!("{fixture_name}_{engine_str}_c.png")));
    let rs_png_out = out_dir.map(|d| d.join(format!("{fixture_name}_{engine_str}_rs.png")));

    let img_c = rasterize_svg(
        &c_svg_str,
        &format!("{fixture_name}_c"),
        Some((target_w, target_h)),
        c_png_out.as_deref(),
    )?;
    let img_rs = rasterize_svg(
        &rs_svg_str,
        &format!("{fixture_name}_rs"),
        Some((target_w, target_h)),
        rs_png_out.as_deref(),
    )?;

    let (_rmse, pixel_similarity, ssim_8x8, soft_ink_dice, exact_ink_dice, diff_img) =
        compare_images(&img_c, &img_rs, out_dir.is_some());

    if let (Some(d), Some(diff)) = (out_dir, diff_img) {
        let diff_path = d.join(format!("{fixture_name}_{engine_str}_diff.png"));
        let _ = write_bmp_and_convert_to_png(&diff, &diff_path);
    }

    let composite_score = 100.0
        * (0.20 * node_size_similarity
            + 0.15 * bb_similarity
            + 0.25 * norm_pos_similarity
            + 0.20 * ssim_8x8
            + 0.20 * soft_ink_dice);

    Ok(FixtureDiffMetrics {
        fixture: fixture_name.to_string(),
        engine: engine_str.to_string(),
        node_count_c: c_layout.nodes.len(),
        node_count_rs: rs_layout.nodes.len(),
        edge_count_c: c_layout.edges.len(),
        edge_count_rs: rs_layout.edges.len(),
        c_bb: [c_layout.bb_w, c_layout.bb_h],
        rs_bb: [rs_layout.bb_w, rs_layout.bb_h],
        node_size_similarity,
        bb_similarity,
        rank_concordance,
        raw_pos_rmse_pt,
        raw_pos_similarity,
        norm_pos_similarity,
        pixel_similarity,
        ssim_8x8,
        soft_ink_dice,
        exact_ink_dice,
        composite_score,
    })
}

fn measure_c_noise_floor(fixtures_dir: &Path) -> Result<Vec<NoiseFloorEntry>, String> {
    let mut entries = Vec::new();
    let engines = [
        ("dot", "fsm.gv"),
        ("dot", "clust4.gv"),
        ("dot", "record.gv"),
        ("neato", "ER.gv"),
        ("fdp", "Heawood.gv"),
        ("sfdp", "Petersen.gv"),
        ("twopi", "unix.gv"),
        ("circo", "Petersen.gv"),
        ("osage", "pack.gv"),
        ("patchwork", "clust.gv"),
    ];

    for (engine, fixture_file) in engines {
        let path = fixtures_dir.join(fixture_file);
        let src = fs::read_to_string(&path)
            .map_err(|e| format!("Failed reading {}: {e}", path.display()))?;
        let svg1 = run_c_graphviz(&src, engine, "svg")?;
        let svg2 = run_c_graphviz(&src, engine, "svg")?;
        let identical = svg1 == svg2;

        let (pixel_rmse, soft_ink_dice) = if identical {
            (0.0, 1.0)
        } else {
            let img1 = rasterize_svg(&svg1, "nf1", Some((400, 400)), None)?;
            let img2 = rasterize_svg(&svg2, "nf2", Some((400, 400)), None)?;
            let (rmse, _, _, dice, _, _) = compare_images(&img1, &img2, false);
            (rmse, dice)
        };

        entries.push(NoiseFloorEntry {
            engine: engine.to_string(),
            fixture: fixture_file.trim_end_matches(".gv").to_string(),
            svg_bytes_identical: identical,
            svg_len_run1: svg1.len(),
            svg_len_run2: svg2.len(),
            pixel_rmse,
            soft_ink_dice,
        });
    }
    Ok(entries)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut out_dir: Option<PathBuf> = None;
    let mut json_out: Option<PathBuf> = None;
    let mut custom_dir: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--out-dir" if i + 1 < args.len() => {
                out_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--json" if i + 1 < args.len() => {
                json_out = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--dir" if i + 1 < args.len() => {
                custom_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    if let Some(ref d) = out_dir {
        let _ = fs::create_dir_all(d);
    }

    let default_fixtures_dir = Path::new("tests/fixtures");
    let fixtures_dir = custom_dir.as_deref().unwrap_or(default_fixtures_dir);
    println!("=== 1. C GRAPHVIZ 16.1.0 SELF-NOISE FLOOR (C vs. C) ===");
    let noise_floor =
        measure_c_noise_floor(default_fixtures_dir).expect("Noise floor check failed");
    println!(
        "{:<10} {:<12} {:<12} {:<12} {:<12}",
        "Engine", "Fixture", "BytesIdent", "PixelRMSE", "SoftInkDice"
    );
    for nf in &noise_floor {
        println!(
            "{:<10} {:<12} {:<12} {:<12.4} {:<12.4}",
            nf.engine, nf.fixture, nf.svg_bytes_identical, nf.pixel_rmse, nf.soft_ink_dice
        );
    }

    println!("\n=== 2. C vs. RUST (gv-engine) DIFFERENTIAL FIDELITY ACROSS ALL 20 FIXTURES ===");
    let mut fixture_paths: Vec<PathBuf> = fs::read_dir(fixtures_dir)
        .expect("Failed reading tests/fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "gv"))
        .collect();
    fixture_paths.sort();

    println!(
        "{:<12} {:<14} {:<14} {:<8} {:<8} {:<8} {:<8} {:<8} {:<8} {:<8}",
        "Fixture",
        "C_BB(wxh)",
        "RS_BB(wxh)",
        "NodeSz%",
        "BB_Sim%",
        "Rank%",
        "PosSim%",
        "SSIM%",
        "InkDice%",
        "SCORE"
    );

    let mut results = Vec::new();
    for path in &fixture_paths {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        let src = fs::read_to_string(path).expect("Failed to read fixture");
        match evaluate_fixture(stem, &src, "dot", out_dir.as_deref()) {
            Ok(m) => {
                println!(
                    "{:<12} {:>6.0}x{:<6.0} {:>6.0}x{:<6.0} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}%",
                    m.fixture,
                    m.c_bb[0],
                    m.c_bb[1],
                    m.rs_bb[0],
                    m.rs_bb[1],
                    m.node_size_similarity * 100.0,
                    m.bb_similarity * 100.0,
                    m.rank_concordance * 100.0,
                    m.norm_pos_similarity * 100.0,
                    m.ssim_8x8 * 100.0,
                    m.soft_ink_dice * 100.0,
                    m.composite_score
                );
                results.push(m);
            }
            Err(e) => {
                eprintln!("ERROR on {stem}: {e}");
            }
        }
    }

    let n = results.len().max(1) as f64;
    let summary = SummaryMetrics {
        fixture_count: results.len(),
        mean_node_size_similarity: results.iter().map(|r| r.node_size_similarity).sum::<f64>() / n,
        mean_bb_similarity: results.iter().map(|r| r.bb_similarity).sum::<f64>() / n,
        mean_rank_concordance: results.iter().map(|r| r.rank_concordance).sum::<f64>() / n,
        mean_raw_pos_similarity: results.iter().map(|r| r.raw_pos_similarity).sum::<f64>() / n,
        mean_norm_pos_similarity: results.iter().map(|r| r.norm_pos_similarity).sum::<f64>() / n,
        mean_pixel_similarity: results.iter().map(|r| r.pixel_similarity).sum::<f64>() / n,
        mean_ssim_8x8: results.iter().map(|r| r.ssim_8x8).sum::<f64>() / n,
        mean_soft_ink_dice: results.iter().map(|r| r.soft_ink_dice).sum::<f64>() / n,
        mean_composite_score: results.iter().map(|r| r.composite_score).sum::<f64>() / n,
    };

    println!(
        "\n{:<12} {:<14} {:<14} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}%",
        "MEAN (20)",
        "-",
        "-",
        summary.mean_node_size_similarity * 100.0,
        summary.mean_bb_similarity * 100.0,
        summary.mean_rank_concordance * 100.0,
        summary.mean_norm_pos_similarity * 100.0,
        summary.mean_ssim_8x8 * 100.0,
        summary.mean_soft_ink_dice * 100.0,
        summary.mean_composite_score
    );

    if let Some(jpath) = json_out {
        let report = FullVisualDiffReport {
            noise_floor,
            fixtures: results,
            summary,
        };
        if let Some(parent) = jpath.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(&jpath, serde_json::to_string_pretty(&report).unwrap())
            .expect("Failed writing JSON report");
        println!("Saved JSON report to {}", jpath.display());
    }
}
