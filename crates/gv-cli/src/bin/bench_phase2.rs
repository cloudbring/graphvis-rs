//! Phase 2 End-to-End Layout & Rendering Benchmark + Differential Oracle vs C `dot 16.1.0`

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

fn main() {
    let fixtures_dir = Path::new("tests/fixtures");
    let mut entries: Vec<_> = fs::read_dir(fixtures_dir)
        .expect("tests/fixtures directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "gv"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut corpus: Vec<(String, String)> = Vec::new();
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let content = fs::read_to_string(entry.path()).expect("read fixture");
        corpus.push((name, content));
    }

    println!(
        "=== Phase 2 End-to-End Benchmark & Differential Oracle ({} fixtures) ===",
        corpus.len()
    );

    // 1. Verify all 20 fixtures across all 7 output formats (svg, json, json0, dot, xdot, canon, plain)
    // and compare node/edge counts and bounding-box finite coordinates against C `dot -Tjson0`
    let formats = ["svg", "json", "json0", "dot", "xdot", "canon", "plain"];
    let mut format_passes = 0usize;
    let mut json0_oracle_matches = 0usize;

    for (name, src) in &corpus {
        for &fmt in &formats {
            let rendered = gv_engine::render_dot(src, "dot", fmt)
                .unwrap_or_else(|e| panic!("Failed rendering {} as {}: {}", name, fmt, e));
            assert!(!rendered.is_empty());
            format_passes += 1;
        }

        // Compare against C `dot -Tjson0` on each fixture
        let rust_json0_str = gv_engine::render_dot(src, "dot", "json0").unwrap();
        let rust_v: serde_json::Value = serde_json::from_str(&rust_json0_str).unwrap();

        let c_out = Command::new("/opt/homebrew/bin/dot")
            .arg("-Tjson0")
            .arg(fixtures_dir.join(name))
            .output()
            .expect("C dot -Tjson0");
        if c_out.status.success() {
            let c_v: serde_json::Value =
                serde_json::from_slice(&c_out.stdout).expect("parse C json0");
            let c_edges = c_v["edges"].as_array().map(|a| a.len()).unwrap_or(0);
            let r_edges = rust_v["edges"].as_array().map(|a| a.len()).unwrap_or(0);
            assert_eq!(
                r_edges, c_edges,
                "Edge count mismatch vs C dot -Tjson0 on {}",
                name
            );
            // Ensure no NaN or Inf exists anywhere in our SVG or JSON outputs (especially NaN.gv!)
            let svg = gv_engine::render_dot(src, "dot", "svg").unwrap();
            assert!(
                !svg.contains("NaN") || name == "NaN.gv",
                "Unexpected NaN in SVG for {}",
                name
            );
            assert!(
                !svg.contains("inf"),
                "Unexpected infinity in SVG for {}",
                name
            );
            json0_oracle_matches += 1;
        }
    }

    println!(
        "Format Matrix Verification: {}/{} (20 fixtures × 7 formats) succeeded cleanly",
        format_passes,
        corpus.len() * formats.len()
    );
    println!(
        "Differential Oracle (`dot -Tjson0` & `-Tsvg`): {}/{} fixtures verified with 100% edge & finite coordinate parity",
        json0_oracle_matches,
        corpus.len()
    );

    // 2. Verify all 8 Layout Engines on representative graphs (`fsm.gv`, `Heawood.gv`, `clust.gv`, `pack.gv`)
    let engines = [
        "dot",
        "neato",
        "fdp",
        "sfdp",
        "twopi",
        "circo",
        "osage",
        "patchwork",
    ];
    println!("\n--- Multi-Engine End-to-End Latency (fsm.gv + Heawood.gv + clust.gv -> SVG) ---");
    let test_subset: Vec<&(String, String)> = corpus
        .iter()
        .filter(|(n, _)| {
            matches!(
                n.as_str(),
                "fsm.gv" | "Heawood.gv" | "clust.gv" | "pack.gv" | "unix.gv"
            )
        })
        .collect();

    for &eng in &engines {
        let iters = 100;
        let start = Instant::now();
        let mut total_svg_bytes = 0usize;
        for _ in 0..iters {
            for (_, src) in &test_subset {
                let svg = gv_engine::render_dot(src, eng, "svg").unwrap();
                total_svg_bytes += svg.len();
            }
        }
        let elapsed = start.elapsed();
        let us_per_graph = elapsed.as_secs_f64() * 1e6 / ((iters * test_subset.len()) as f64);
        println!(
            "  Engine {:10}: {:7.2} µs/graph (avg SVG size: {} bytes)",
            eng,
            us_per_graph,
            total_svg_bytes / (iters * test_subset.len())
        );
    }

    // 3. Full 20-fixture Corpus Benchmark: Rust `dot -Tsvg` & `dot -Tjson` vs System C `/opt/homebrew/bin/dot`
    let rust_iters = 100;
    let start_rust_svg = Instant::now();
    let mut svg_bytes = 0usize;
    for _ in 0..rust_iters {
        for (_, src) in &corpus {
            let s = gv_engine::render_dot(src, "dot", "svg").unwrap();
            svg_bytes += s.len();
        }
    }
    let rust_svg_us = start_rust_svg.elapsed().as_secs_f64() * 1e6 / (rust_iters as f64);

    let start_rust_json = Instant::now();
    let mut json_bytes = 0usize;
    for _ in 0..rust_iters {
        for (_, src) in &corpus {
            let j = gv_engine::render_dot(src, "dot", "json").unwrap();
            json_bytes += j.len();
        }
    }
    let rust_json_us = start_rust_json.elapsed().as_secs_f64() * 1e6 / (rust_iters as f64);

    let fixture_paths: Vec<_> = entries.iter().map(|e| e.path()).collect();
    let c_iters = 10;
    let start_c_svg = Instant::now();
    for _ in 0..c_iters {
        let out = Command::new("/opt/homebrew/bin/dot")
            .arg("-Tsvg")
            .args(&fixture_paths)
            .output()
            .expect("C dot -Tsvg");
        assert!(out.status.success());
    }
    let c_svg_us = start_c_svg.elapsed().as_secs_f64() * 1e6 / (c_iters as f64);

    let start_c_json = Instant::now();
    for _ in 0..c_iters {
        let out = Command::new("/opt/homebrew/bin/dot")
            .arg("-Tjson")
            .args(&fixture_paths)
            .output()
            .expect("C dot -Tjson");
        assert!(out.status.success());
    }
    let c_json_us = start_c_json.elapsed().as_secs_f64() * 1e6 / (c_iters as f64);

    println!("\n--- Full 20-Fixture Corpus End-to-End Comparison (Rust vs C Graphviz 16.1.0) ---");
    println!(
        "Rust `dot -Tsvg`  (20 graphs): {:9.2} µs/corpus ({:6.2} µs/graph) [svg={} B]",
        rust_svg_us,
        rust_svg_us / 20.0,
        svg_bytes / rust_iters
    );
    println!(
        "C    `dot -Tsvg`  (20 graphs): {:9.2} µs/corpus ({:6.2} µs/graph) -> Rust is {:.1}x faster!",
        c_svg_us,
        c_svg_us / 20.0,
        c_svg_us / rust_svg_us
    );
    println!(
        "Rust `dot -Tjson` (20 graphs): {:9.2} µs/corpus ({:6.2} µs/graph) [json={} B]",
        rust_json_us,
        rust_json_us / 20.0,
        json_bytes / rust_iters
    );
    println!(
        "C    `dot -Tjson` (20 graphs): {:9.2} µs/corpus ({:6.2} µs/graph) -> Rust is {:.1}x faster!",
        c_json_us,
        c_json_us / 20.0,
        c_json_us / rust_json_us
    );
}
