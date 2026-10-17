//! `gv-diff`: score the port against C Graphviz golden layouts and guard against regressions.
//!
//! ```text
//! gv-diff [--golden DIR] [--baseline FILE] [--update] [--filter SUBSTR] [--worst N] CORPUS_DIR...
//! ```
//!
//! Exits non-zero when the total score or the number of exactly matching files drops
//! below the baseline. `--update` rewrites the baseline with the current results.

use gv_diff::{Baseline, FileReport, compare_file};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Mutex;

fn main() -> ExitCode {
    let mut golden = PathBuf::from("tests/golden");
    let mut baseline_path: Option<PathBuf> = None;
    let mut update = false;
    let mut filter: Option<String> = None;
    let mut worst = 15usize;
    let mut corpora: Vec<PathBuf> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--golden" => golden = args.next().expect("--golden DIR").into(),
            "--baseline" => baseline_path = Some(args.next().expect("--baseline FILE").into()),
            "--update" => update = true,
            "--filter" => filter = args.next(),
            "--worst" => worst = args.next().and_then(|s| s.parse().ok()).unwrap_or(15),
            _ => corpora.push(a.into()),
        }
    }
    if corpora.is_empty() {
        corpora = vec!["tests/wild".into(), "tests/fixtures".into()];
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for dir in &corpora {
        let Ok(rd) = std::fs::read_dir(dir) else {
            eprintln!("cannot read {}", dir.display());
            return ExitCode::FAILURE;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "gv") {
                files.push(p);
            }
        }
    }
    files.sort();
    if let Some(f) = &filter {
        files.retain(|p| p.to_string_lossy().contains(f.as_str()));
    }

    let results = run_all(&files, &golden);
    let current = Baseline { files: results };
    print_summary(&current, worst);

    let Some(bpath) = baseline_path else {
        return ExitCode::SUCCESS;
    };
    if update {
        let json = serde_json::to_string_pretty(&current).expect("serialize baseline");
        std::fs::write(&bpath, json + "\n").expect("write baseline");
        println!("baseline updated: {}", bpath.display());
        return ExitCode::SUCCESS;
    }
    let base: Baseline = match std::fs::read_to_string(&bpath) {
        Ok(s) => serde_json::from_str(&s).expect("parse baseline"),
        Err(_) => {
            println!("no baseline at {}; run with --update", bpath.display());
            return ExitCode::FAILURE;
        }
    };
    check_against(&base, &current, filter.is_some())
}

fn run_all(files: &[PathBuf], golden: &Path) -> BTreeMap<String, FileReport> {
    // Silence panic messages from the port; they are captured in the report.
    std::panic::set_hook(Box::new(|_| {}));
    let out = Mutex::new(BTreeMap::new());
    let next = Mutex::new(0usize);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = {
                        let mut n = next.lock().unwrap();
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(path) = files.get(i) else { break };
                    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                    let gpath = golden.join(format!("{stem}.dot"));
                    let report = match (
                        std::fs::read_to_string(path),
                        std::fs::read_to_string(&gpath),
                    ) {
                        (Ok(src), Ok(gold)) => compare_file(&src, &gold),
                        _ => FileReport {
                            error: Some(format!("missing golden {}", gpath.display())),
                            ..FileReport::default()
                        },
                    };
                    out.lock().unwrap().insert(stem, report);
                }
            });
        }
    });
    let _ = std::panic::take_hook();
    out.into_inner().unwrap()
}

fn print_summary(b: &Baseline, worst: usize) {
    let n = b.files.len();
    let sum = |f: fn(&FileReport) -> usize| b.files.values().map(f).sum::<usize>();
    let (nodes, nodes_ok) = (sum(|r| r.nodes), sum(|r| r.nodes_ok));
    let (edges, edges_ok) = (sum(|r| r.edges), sum(|r| r.edges_ok));
    let (cl, cl_ok) = (sum(|r| r.clusters), sum(|r| r.clusters_ok));
    let errors = b.files.values().filter(|r| r.error.is_some()).count();
    let pct = |a: usize, t: usize| {
        if t == 0 {
            100.0
        } else {
            100.0 * a as f64 / t as f64
        }
    };
    println!("files:     {n}  (errors: {errors})");
    println!(
        "exact:     {} / {n}  ({:.1}%)",
        b.exact_count(),
        pct(b.exact_count(), n)
    );
    println!(
        "score:     {:.3} / {n}  ({:.1}%)",
        b.total_score(),
        100.0 * b.total_score() / n.max(1) as f64
    );
    println!(
        "nodes ok:  {nodes_ok} / {nodes}  ({:.1}%)",
        pct(nodes_ok, nodes)
    );
    println!(
        "edges ok:  {edges_ok} / {edges}  ({:.1}%)",
        pct(edges_ok, edges)
    );
    println!("clusters:  {cl_ok} / {cl}  ({:.1}%)", pct(cl_ok, cl));
    let bb_ok = b.files.values().filter(|r| r.bb_ok).count();
    println!("bb ok:     {bb_ok} / {n}  ({:.1}%)", pct(bb_ok, n));
    let mut v: Vec<_> = b.files.iter().collect();
    v.sort_by(|a, b| a.1.score().total_cmp(&b.1.score()));
    if worst > 0 {
        println!("\nlowest scores:");
        for (name, r) in v.iter().take(worst) {
            println!(
                "  {:.3}  {name}  nodes {}/{} edges {}/{} node_err {:.1} edge_err {:.1}{}",
                r.score(),
                r.nodes_ok,
                r.nodes,
                r.edges_ok,
                r.edges,
                r.node_err,
                r.edge_err,
                r.error
                    .as_deref()
                    .map(|e| format!("  [{e}]"))
                    .unwrap_or_default()
            );
        }
    }
}

fn check_against(base: &Baseline, cur: &Baseline, filtered: bool) -> ExitCode {
    let mut regressions = Vec::new();
    let mut improvements = 0usize;
    for (name, r) in &cur.files {
        let Some(old) = base.files.get(name) else {
            continue;
        };
        let (a, b) = (old.score(), r.score());
        if b + 1e-9 < a || (old.exact() && !r.exact()) {
            regressions.push(format!("  {name}: {a:.3} -> {b:.3}"));
        } else if b > a + 1e-9 {
            improvements += 1;
        }
    }
    let old_subset = Baseline {
        files: base
            .files
            .iter()
            .filter(|(k, _)| cur.files.contains_key(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };
    let (os, cs) = (old_subset.total_score(), cur.total_score());
    let (oe, ce) = (old_subset.exact_count(), cur.exact_count());
    println!(
        "\nvs baseline: score {os:.3} -> {cs:.3}, exact {oe} -> {ce}, {improvements} files improved, {} regressed",
        regressions.len()
    );
    for r in regressions.iter().take(40) {
        println!("{r}");
    }
    let failed = cs + 1e-6 < os || ce < oe;
    if failed {
        println!("FAIL: aggregate fidelity dropped below the baseline");
        ExitCode::FAILURE
    } else {
        if !filtered && (cs > os + 1e-6 || ce > oe) {
            println!("note: results beat the baseline; refresh it with --update");
        }
        ExitCode::SUCCESS
    }
}
