use gv_engine::{GlobalAttrs, LayoutEngine, LayoutOptions, OutputFormat};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

const VERSION: &str = "dot-rs - graphvis-rs version 0.1.0 (partial port of Graphviz 16.1.0)";

const USAGE: &str = " -V          - Print version and exit
 -v[n]       - Enable verbose mode (accepted, ignored)
 -Gname=val  - Set graph attribute 'name' to 'val'
 -Nname=val  - Set node attribute 'name' to 'val'
 -Ename=val  - Set edge attribute 'name' to 'val'
 -Aname=val  - Set attribute 'name' to 'val' for graph, node, and edge
 -Tv         - Set output format to 'v'
 -Kv         - Set layout engine to 'v'
 -ofile      - Write output to 'file'
 -O          - Write each input's output to <input>.<format>
 -q[l]       - Set level of message suppression (accepted, ignored)
 -n[v]       - No layout mode 'v' (=1)
 -s[v], -x, -y, -lv - accepted, ignored
";

struct Options {
    engine: String,
    format: String,
    output: Option<String>,
    auto_output: bool,
    globals: GlobalAttrs,
    layout: LayoutOptions,
    inputs: Vec<String>,
}

enum Parsed {
    Run(Options),
    Exit(ExitCode),
}

fn usage(argv0: &str) -> String {
    format!("Usage: {argv0} [-Vv?] [-(GNEA)name=val] [-(KTo)<val>] <dot files>\n\n{USAGE}")
}

/// Mirrors `getFlagOpt`: the text after the flag letter, else the next argument unless it
/// starts with `-`.
fn flag_value(args: &[String], i: &mut usize) -> Option<String> {
    let arg = &args[*i];
    if arg.len() > 2 {
        return Some(arg[2..].to_string());
    }
    let next = args.get(*i + 1)?;
    if !next.is_empty() && !next.starts_with('-') {
        *i += 1;
        return Some(next.clone());
    }
    None
}

/// Mirrors `global_def`: `name=value`, or a bare `name` meaning `true`.
fn global_def(spec: &str) -> (String, String) {
    match spec.split_once('=') {
        Some((name, value)) => (name.to_string(), value.to_string()),
        None => (spec.to_string(), "true".to_string()),
    }
}

fn parse_args(argv0: &str, args: &[String]) -> Parsed {
    let mut opts = Options {
        engine: "dot".to_string(),
        format: "dot".to_string(),
        output: None,
        auto_output: false,
        globals: GlobalAttrs::default(),
        layout: LayoutOptions::default(),
        inputs: Vec::new(),
    };
    let missing = |flag: char| {
        eprintln!("Missing argument for -{flag} flag");
        eprint!("{}", usage(argv0));
        Parsed::Exit(ExitCode::FAILURE)
    };

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg.starts_with("-V") || arg == "--version" {
            eprintln!("{VERSION}");
            return Parsed::Exit(ExitCode::SUCCESS);
        } else if arg.starts_with("-?") || arg == "--help" {
            print!("{}", usage(argv0));
            return Parsed::Exit(ExitCode::SUCCESS);
        } else if arg.starts_with('-') && arg.len() > 1 {
            let flag = arg.as_bytes()[1] as char;
            let rest = &arg[2..];
            match flag {
                'G' | 'N' | 'E' | 'A' => {
                    if rest.is_empty() {
                        return missing(flag);
                    }
                    let (name, value) = global_def(rest);
                    if matches!(flag, 'G' | 'A') {
                        opts.globals.graph.insert(name.clone(), value.clone());
                    }
                    if matches!(flag, 'N' | 'A') {
                        opts.globals.node.insert(name.clone(), value.clone());
                    }
                    if matches!(flag, 'E' | 'A') {
                        opts.globals.edge.insert(name, value);
                    }
                }
                'T' => match flag_value(args, &mut i) {
                    Some(v) => opts.format = v,
                    None => return missing('T'),
                },
                'K' => match flag_value(args, &mut i) {
                    Some(v) => opts.engine = v,
                    None => return missing('K'),
                },
                'o' => match flag_value(args, &mut i) {
                    Some(v) => opts.output = Some(v),
                    None => return missing('o'),
                },
                'l' => {
                    if flag_value(args, &mut i).is_none() {
                        return missing('l');
                    }
                }
                'O' => opts.auto_output = true,
                // neato_extra_args: `-n` is `-n1`; `-nN` needs N > 0 (atoi semantics).
                'n' => {
                    if rest.is_empty() {
                        opts.layout.nop = 1;
                    } else {
                        let digits: String = rest
                            .trim_start()
                            .chars()
                            .enumerate()
                            .take_while(|(i, c)| c.is_ascii_digit() || (*i == 0 && *c == '+'))
                            .map(|(_, c)| c)
                            .collect();
                        match digits.parse::<i64>() {
                            Ok(n) if n > 0 => opts.layout.nop = n.min(255) as u8,
                            _ => {
                                eprintln!("Error: Invalid parameter \"{rest}\" for -n flag");
                                eprint!("{}", usage(argv0));
                                return Parsed::Exit(ExitCode::FAILURE);
                            }
                        }
                    }
                }
                'v' | 'q' | 's' | 'x' | 'y' | 'P' | 'c' | 'L' => {}
                _ => {
                    eprintln!("{argv0}: option -{flag} unrecognized\n");
                    eprint!("{}", usage(argv0));
                    return Parsed::Exit(ExitCode::FAILURE);
                }
            }
        } else {
            opts.inputs.push(args[i].clone());
        }
        i += 1;
    }
    Parsed::Run(opts)
}

/// Reads a DOT file. C reads bytes; input that is not UTF-8 (Latin-1 graphs)
/// is decoded byte for byte as ISO-8859-1, which is what C's `latin1ToUTF8`
/// (charset=latin1) and its invalid-UTF-8 fallback measure.
fn read_input(path: Option<&str>) -> io::Result<String> {
    let bytes = match path {
        Some(p) => fs::read(p)?,
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };
    Ok(String::from_utf8(bytes)
        .unwrap_or_else(|e| e.into_bytes().iter().map(|&b| char::from(b)).collect()))
}

fn main() -> ExitCode {
    let raw: Vec<String> = env::args().collect();
    let argv0 = raw
        .first()
        .and_then(|a| Path::new(a).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("dot-rs")
        .to_string();
    let opts = match parse_args(&argv0, &raw[1..]) {
        Parsed::Run(o) => o,
        Parsed::Exit(code) => return code,
    };

    if OutputFormat::parse(&opts.format).is_err() {
        eprintln!("Format: \"{}\" not recognized.", opts.format);
        return ExitCode::FAILURE;
    }
    if LayoutEngine::parse(&opts.engine).is_err() {
        eprintln!("There is no layout engine support for \"{}\"", opts.engine);
        return ExitCode::FAILURE;
    }

    let mut shared_out: Option<Box<dyn Write>> = match (&opts.output, opts.auto_output) {
        (Some(path), false) => match fs::File::create(path) {
            Ok(f) => Some(Box::new(f)),
            Err(e) => {
                eprintln!("{argv0}: can't open {path}: {e}");
                return ExitCode::FAILURE;
            }
        },
        _ => None,
    };

    let inputs: Vec<Option<&str>> = if opts.inputs.is_empty() {
        vec![None]
    } else {
        opts.inputs.iter().map(|s| Some(s.as_str())).collect()
    };

    let mut failed = false;
    for input in inputs {
        let name = input.unwrap_or("<stdin>");
        let source = match read_input(input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{argv0}: can't open {name}: {e}");
                failed = true;
                continue;
            }
        };
        let outputs = match gv_engine::render_dot_all_with_options(
            &source,
            &opts.engine,
            &opts.format,
            &opts.globals,
            opts.layout,
        ) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Error: {name}: {e}");
                failed = true;
                continue;
            }
        };

        let mut per_input: Option<Box<dyn Write>> = None;
        if let (true, Some(path)) = (opts.auto_output, input) {
            let target = format!("{path}.{}", opts.format);
            match fs::File::create(&target) {
                Ok(f) => per_input = Some(Box::new(f)),
                Err(e) => {
                    eprintln!("{argv0}: can't open {target}: {e}");
                    failed = true;
                    continue;
                }
            }
        }
        let mut stdout = io::stdout();
        let out: &mut dyn Write = match (per_input.as_mut(), shared_out.as_mut()) {
            (Some(w), _) | (None, Some(w)) => w.as_mut(),
            (None, None) => &mut stdout,
        };
        for text in outputs {
            if let Err(e) = out.write_all(text.as_bytes()) {
                eprintln!("{argv0}: write error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    if let Some(w) = shared_out.as_mut() {
        let _ = w.flush();
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
