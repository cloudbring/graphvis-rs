use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut engine = "dot".to_string();
    let mut format = "dot".to_string();
    let mut input_path: Option<String> = None;

    for arg in env::args().skip(1) {
        if arg == "-V" || arg == "--version" {
            eprintln!("dot-rs - graphvis-rs version 0.1.0 (Graphviz 16.1.0 pure-Rust port)");
            return ExitCode::SUCCESS;
        } else if let Some(fmt) = arg.strip_prefix("-T") {
            format = fmt.to_string();
        } else if let Some(eng) = arg.strip_prefix("-K") {
            engine = eng.to_string();
        } else if !arg.starts_with('-') {
            input_path = Some(arg);
        }
    }

    let source = match input_path {
        Some(path) => match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {}: {}", path, e);
                return ExitCode::FAILURE;
            }
        },
        None => {
            let mut buf = String::new();
            if let Err(e) = io::stdin().read_to_string(&mut buf) {
                eprintln!("Error reading stdin: {}", e);
                return ExitCode::FAILURE;
            }
            buf
        }
    };

    match gv_engine::render_dot(&source, &engine, &format) {
        Ok(out) => {
            print!("{}", out);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}
