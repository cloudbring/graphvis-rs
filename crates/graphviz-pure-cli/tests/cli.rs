use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn run(args: &[&str], stdin: &str) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_graphviz-pure"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A run rejected on its arguments may exit before reading stdin.
    if let Err(e) = child.stdin.take().unwrap().write_all(stdin.as_bytes()) {
        assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe, "{e}");
    }
    let out = child.wait_with_output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn undirected_graph_has_no_arrowhead() {
    let (out, _, ok) = run(&["-Tsvg"], "graph{a--b}");
    assert!(ok);
    assert_eq!(out.matches("<polygon").count(), 1, "only the background");
    let (out, _, _) = run(&["-Tsvg"], "digraph{a->b}");
    assert_eq!(out.matches("<polygon").count(), 2);
}

#[test]
fn unknown_flag_is_rejected() {
    let (_, err, ok) = run(&["-Z"], "graph{}");
    assert!(!ok);
    assert!(err.contains("option -Z unrecognized"), "{err}");
}

#[test]
fn global_attrs_output_file_and_separate_args() {
    let path = tmp("cli_out.svg");
    let (_, _, ok) = run(
        &[
            "-T",
            "svg",
            "-o",
            path.to_str().unwrap(),
            "-Nshape=box",
            "-Gbgcolor=yellow",
            "-Ecolor=red",
        ],
        "digraph G{a->b}\ndigraph H{c->d}",
    );
    assert!(ok);
    let svg = std::fs::read_to_string(&path).unwrap();
    assert_eq!(svg.matches("<svg").count(), 2, "every graph is rendered");
    assert!(!svg.contains("<ellipse"));
    assert!(svg.contains("fill=\"yellow\""));
    assert!(svg.contains("stroke=\"red\""));
}

#[test]
fn multiple_input_files_are_each_rendered() {
    let a = tmp("cli_a.gv");
    let b = tmp("cli_b.gv");
    std::fs::write(&a, "digraph{x->y}").unwrap();
    std::fs::write(&b, "graph{p--q}").unwrap();
    let (out, _, ok) = run(&["-Tsvg", a.to_str().unwrap(), b.to_str().unwrap()], "");
    assert!(ok);
    assert_eq!(out.matches("<svg").count(), 2);
}

#[test]
fn version_and_help_exit_successfully() {
    let (_, err, ok) = run(&["-V"], "");
    assert!(ok && err.contains("version"));
    let (out, _, ok) = run(&["-?"], "");
    assert!(ok && out.contains("Usage:"));
}

#[test]
fn no_layout_mode_keeps_positions() {
    let laid = "digraph{a [pos=\"127,118\"]; b [pos=\"127,18\"]; a -> b [pos=\"e,127,36 127,100 127,80 127,60 127,46\"]}";
    let (out, _, ok) = run(&["-Kneato", "-n2", "-Tdot"], laid);
    assert!(ok);
    // translated so the bounding box starts at the origin
    assert!(out.contains("pos=\"27,118\""), "{out}");
    assert!(
        out.contains("pos=\"e,27,36 27,100 27,80 27,60 27,46\""),
        "{out}"
    );
    let (_, err, ok) = run(&["-Kneato", "-n0", "-Tdot"], laid);
    assert!(!ok && err.contains("Invalid parameter"));
    let (out, _, ok) = run(&["-Knop2", "-Tplain"], laid);
    assert!(ok && out.starts_with("graph 1 "), "{out}");
}
