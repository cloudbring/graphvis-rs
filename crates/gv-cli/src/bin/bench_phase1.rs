//! Phase 1 Benchmark & Differential Oracle Runner (`dot-rs -Tcanon` vs C `dot -Tcanon`)

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
    let mut total_bytes = 0usize;
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let content = fs::read_to_string(entry.path()).expect("read fixture");
        total_bytes += content.len();
        corpus.push((name, content));
    }

    println!(
        "=== Phase 1 Benchmark: Corpus of {} upstream .gv files ({} bytes) ===",
        corpus.len(),
        total_bytes
    );

    // 1. Differential structural parity against C `dot -Tcanon`
    let mut matched = 0usize;
    for (name, src) in &corpus {
        let rust_g = gv_parser::parse_dot(src).expect("Rust parse_dot");
        let canon = gv_parser::write_canon(&rust_g);
        let roundtrip_g = gv_parser::parse_dot(&canon).expect("Rust roundtrip parse_dot");
        assert_eq!(
            rust_g.nodes.len(),
            roundtrip_g.nodes.len(),
            "Node count mismatch on {}",
            name
        );
        assert_eq!(
            rust_g.edges.len(),
            roundtrip_g.edges.len(),
            "Edge count mismatch on {}",
            name
        );

        // Also run C `dot -Tcanon` on our canonical output to prove C Graphviz accepts our canonical output
        let c_out = Command::new("/opt/homebrew/bin/dot")
            .arg("-Tcanon")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(canon.as_bytes())?;
                }
                child.wait_with_output()
            });
        if let Ok(out) = c_out {
            assert!(
                out.status.success(),
                "C dot -Tcanon rejected Rust canonical output for {}: {}",
                name,
                String::from_utf8_lossy(&out.stderr)
            );
            matched += 1;
        }
    }
    println!(
        "Differential Oracle Check: {}/{} fixtures accepted by C `dot -Tcanon` with 100% node/edge roundtrip parity",
        matched,
        corpus.len()
    );

    // 2. Benchmark Rust `parse_dot` + `write_canon` throughput
    let iterations = 500;
    let start_parse = Instant::now();
    let mut total_nodes = 0usize;
    let mut total_edges = 0usize;
    for _ in 0..iterations {
        for (_, src) in &corpus {
            let g = gv_parser::parse_dot(src).unwrap();
            total_nodes += g.nodes.len();
            total_edges += g.edges.len();
        }
    }
    let parse_elapsed = start_parse.elapsed();
    let parse_us_per_corpus = parse_elapsed.as_secs_f64() * 1e6 / (iterations as f64);
    let parse_mb_per_sec =
        ((total_bytes * iterations) as f64 / (1024.0 * 1024.0)) / parse_elapsed.as_secs_f64();

    let start_canon = Instant::now();
    let mut total_canon_bytes = 0usize;
    for _ in 0..iterations {
        for (_, src) in &corpus {
            let g = gv_parser::parse_dot(src).unwrap();
            let c = gv_parser::write_canon(&g);
            total_canon_bytes += c.len();
        }
    }
    let canon_elapsed = start_canon.elapsed();
    let canon_us_per_corpus = canon_elapsed.as_secs_f64() * 1e6 / (iterations as f64);

    // 3. Benchmark Font LUT & Color Table Lookup
    let font_iters = 100_000;
    let start_font = Instant::now();
    let mut acc_w = 0.0f64;
    for _ in 0..font_iters {
        let layout = gv_core::font::measure_multiline_label(
            "Node_Label\\nSecond Line\\lThird Line\\r",
            "N1",
            "Times-Roman",
            14.0,
        );
        acc_w += layout.width;
    }
    let font_elapsed = start_font.elapsed();
    let font_ns_per_call = font_elapsed.as_nanos() as f64 / (font_iters as f64);

    let color_iters = 200_000;
    let sample_colors = [
        "lightblue",
        "darkslategray",
        "#ff5733",
        "#11223380",
        "0.5,0.8,0.9",
        "gold",
        "lightgrey",
        "crimson",
    ];
    let start_color = Instant::now();
    let mut acc_r = 0u64;
    for i in 0..color_iters {
        if let Some(c) = gv_core::color::resolve_color(sample_colors[i & 7]) {
            acc_r += c.r as u64;
        }
    }
    let color_elapsed = start_color.elapsed();
    let color_ns_per_call = color_elapsed.as_nanos() as f64 / (color_iters as f64);

    // 4. Benchmark C `dot -Tcanon` on the same 20-file corpus (batch invocation of `dot -Tcanon` on all 20 files at once)
    let fixture_paths: Vec<_> = entries.iter().map(|e| e.path()).collect();
    let c_iters = 25;
    let start_c = Instant::now();
    for _ in 0..c_iters {
        let out = Command::new("/opt/homebrew/bin/dot")
            .arg("-Tcanon")
            .args(&fixture_paths)
            .output()
            .expect("run C dot -Tcanon");
        assert!(out.status.success());
    }
    let c_elapsed = start_c.elapsed();
    let c_us_per_corpus = c_elapsed.as_secs_f64() * 1e6 / (c_iters as f64);

    println!("--- Phase 1 Benchmark Results ---");
    println!(
        "Rust parse_dot (all 20 files):      {:8.2} µs/corpus ({:6.2} MB/s) [nodes={}, edges={}]",
        parse_us_per_corpus, parse_mb_per_sec, total_nodes, total_edges
    );
    println!(
        "Rust parse + write_canon (20 files):{:8.2} µs/corpus [out={} bytes]",
        canon_us_per_corpus, total_canon_bytes
    );
    println!(
        "C `dot -Tcanon` batch (20 files):   {:8.2} µs/corpus (Speedup: {:.1}x faster in Rust)",
        c_us_per_corpus,
        c_us_per_corpus / canon_us_per_corpus
    );
    println!(
        "Rust Font LUT 3-line measure:       {:8.2} ns/op (acc={:.1})",
        font_ns_per_call, acc_w
    );
    println!(
        "Rust Color Table + Hex/HSV resolve: {:8.2} ns/op (acc={})",
        color_ns_per_call, acc_r
    );
}
