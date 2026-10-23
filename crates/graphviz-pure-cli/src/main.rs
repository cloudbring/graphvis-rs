use graphviz_pure::{GlobalAttrs, LayoutEngine, LayoutOptions, OutputFormat};
use graphviz_pure_core::diag::{AgErrLevel, agerrorf, agseterr, fprintf_stderr};
use std::borrow::Cow;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

const VERSION: &str = concat!(
    "graphviz-pure - graphviz-pure version ",
    env!("CARGO_PKG_VERSION"),
    " (Rust port of Graphviz 16.1.0)"
);

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
 -q[l]       - Set level of message suppression
 -n[v]       - No layout mode 'v' (=1)
 -s[v], -x, -y, -lv - accepted, ignored
";

struct Options {
    engine: String,
    format: String,
    output: Option<OsString>,
    auto_output: bool,
    globals: GlobalAttrs,
    layout: LayoutOptions,
    inputs: Vec<OsString>,
}

enum Parsed {
    Run(Options),
    Exit(ExitCode),
}

fn usage(argv0: &str) -> String {
    format!("Usage: {argv0} [-Vv?] [-(GNEA)name=val] [-(KTo)<val>] <dot files>\n\n{USAGE}")
}

/// An argument as text. C's `main` gets each argument as bytes, so the bytes of one that is
/// not valid UTF-8 are kept as raw-byte chars (`graphviz_pure_core::charset`): an attribute
/// set with `-G` carries them into the output, and an error message names them as C does.
fn arg_text(arg: &OsStr) -> String {
    bytes_text(&arg_bytes(arg))
}

/// [`arg_text`] of some of an argument's bytes.
fn bytes_text(bytes: &[u8]) -> String {
    use graphviz_pure_core::charset::raw_char;
    // As `charset::decode`, but a char of the raw-byte range itself is kept as raw bytes
    // too, so `charset::encode` gives back exactly the argument's bytes.
    let mut text = String::new();
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            if c >= raw_char(0) {
                text.extend(c.encode_utf8(&mut [0; 4]).bytes().map(raw_char));
            } else {
                text.push(c);
            }
        }
        text.extend(chunk.invalid().iter().map(|&b| raw_char(b)));
    }
    text
}

/// `strerror(errno)` for an I/O error: its text without Rust's ` (os error N)`.
fn strerror(e: &io::Error) -> String {
    let text = e.to_string();
    match (e.raw_os_error(), text.rfind(" (os error ")) {
        (Some(_), Some(at)) => text[..at].to_string(),
        _ => text,
    }
}

/// `atoi(s)`: C's white space, a sign, then digits; what it gives an `int` beyond its range.
fn atoi(s: &str) -> i32 {
    let s = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    // `atoi` is `(int)strtol(s, NULL, 10)`: strtol saturates at the `long` range.
    let mut v: i64 = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        v = v.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    let v = if neg { v.saturating_neg() } else { v };
    v as i32
}

/// The bytes of an argument, as C's `argv` holds them (UTF-8 where the OS has UTF-16).
/// Flags are split on these, not on [`arg_text`], which does not round-trip a char of the
/// raw-byte range.
fn arg_bytes(arg: &OsStr) -> Cow<'_, [u8]> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Cow::Borrowed(arg.as_bytes())
    }
    #[cfg(not(unix))]
    {
        Cow::Owned(arg.to_string_lossy().into_owned().into_bytes())
    }
}

/// The file name after a two-letter flag (`-ofile`), with the argument's own bytes.
fn os_tail(arg: &OsStr) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        OsStr::from_bytes(&arg.as_bytes()[2..]).to_os_string()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        OsString::from_wide(&arg.encode_wide().skip(2).collect::<Vec<_>>())
    }
    #[cfg(not(any(unix, windows)))]
    {
        OsString::from(String::from_utf8_lossy(&arg_bytes(arg)[2..]).into_owned())
    }
}

/// Mirrors `getFlagOpt`: the text after the flag letter, else the next argument unless it
/// starts with `-`.
fn flag_value(args: &[OsString], i: &mut usize) -> Option<String> {
    let arg = arg_bytes(&args[*i]);
    if arg.len() > 2 {
        return Some(bytes_text(&arg[2..]));
    }
    let next = arg_text(args.get(*i + 1)?);
    if !next.is_empty() && !next.starts_with('-') {
        *i += 1;
        return Some(next);
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

/// Mirrors `dotneato_args_initialize`. File names (`-o` and the inputs) are kept as the OS
/// gave them, so a name that is not valid UTF-8 still opens.
fn parse_args(argv0: &str, os_args: &[OsString]) -> Parsed {
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
        fprintf_stderr(&format!("Missing argument for -{flag} flag\n"));
        fprintf_stderr(&usage(argv0));
        Parsed::Exit(ExitCode::FAILURE)
    };

    // `neato_extra_args` (`args.c`) takes `-n` and `-x` out of the arguments before
    // `dotneato_args_initialize` reads the rest, so its error comes before any `-q` takes
    // effect, and `-o -n2 file` names `file`.
    let mut kept: Vec<OsString> = Vec::with_capacity(os_args.len());
    for os_arg in os_args {
        let bytes = arg_bytes(os_arg);
        if bytes.len() < 2 || bytes[0] != b'-' || !matches!(bytes[1], b'n' | b'x') {
            kept.push(os_arg.clone());
            continue;
        }
        if bytes[1] == b'x' {
            continue;
        }
        let rest = bytes_text(&bytes[2..]);
        if rest.is_empty() {
            opts.layout.nop = 1;
            continue;
        }
        // `Nop = atoi(arg + 2)`, an error when not positive.
        match atoi(&rest) {
            n if n > 0 => opts.layout.nop = n.min(255) as u8,
            _ => {
                agerrorf(&format!("Invalid parameter \"{rest}\" for -n flag\n"));
                fprintf_stderr(&usage(argv0));
                return Parsed::Exit(ExitCode::FAILURE);
            }
        }
    }
    let os_args = kept.as_slice();
    let args: Vec<String> = os_args.iter().map(|a| arg_text(a)).collect();
    let args = args.as_slice();

    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        if arg.starts_with("-V") || arg == "--version" {
            fprintf_stderr(&format!("{VERSION}\n"));
            return Parsed::Exit(ExitCode::SUCCESS);
        } else if arg.starts_with("-?") || arg == "--help" {
            print!("{}", usage(argv0));
            return Parsed::Exit(ExitCode::SUCCESS);
        } else if arg.starts_with('-') && arg.len() > 1 {
            // `dotneato_args_initialize` switches on the byte `arg[1]`, which need not start
            // a char: split the argument's bytes there.
            let bytes = arg_bytes(&os_args[i]);
            let flag = match bytes[1] {
                b if b.is_ascii() => b as char,
                b => graphviz_pure_core::charset::raw_char(b),
            };
            let rest = bytes_text(&bytes[2..]);
            let rest = rest.as_str();
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
                'T' => match flag_value(os_args, &mut i) {
                    Some(v) => opts.format = v,
                    None => return missing('T'),
                },
                'K' => match flag_value(os_args, &mut i) {
                    Some(v) => opts.engine = v,
                    None => return missing('K'),
                },
                'o' => {
                    let at = i;
                    match flag_value(os_args, &mut i) {
                        Some(_) if i > at => opts.output = Some(os_args[i].clone()),
                        Some(_) => opts.output = Some(os_tail(&os_args[i])),
                        None => return missing('o'),
                    }
                }
                'l' => {
                    if flag_value(os_args, &mut i).is_none() {
                        return missing('l');
                    }
                }
                'O' => opts.auto_output = true,
                // `-q` is `-q1`: warnings are dropped; `-q2` and up drop errors too.
                'q' => {
                    if rest.is_empty() {
                        agseterr(AgErrLevel::Err);
                    } else {
                        match atoi(rest) {
                            ..=0 => fprintf_stderr(&format!(
                                "Invalid parameter \"{rest}\" for -q flag - ignored\n"
                            )),
                            1 => {
                                agseterr(AgErrLevel::Err);
                            }
                            _ => {
                                agseterr(AgErrLevel::Max);
                            }
                        }
                    }
                }
                'v' | 's' | 'y' | 'P' | 'c' | 'L' => {}
                _ => {
                    agerrorf(&format!("{argv0}: option -{flag} unrecognized\n\n"));
                    fprintf_stderr(&usage(argv0));
                    return Parsed::Exit(ExitCode::FAILURE);
                }
            }
        } else {
            opts.inputs.push(os_args[i].clone());
        }
        i += 1;
    }
    Parsed::Run(opts)
}

/// Reads a DOT file. C reads bytes; bytes that are not part of valid UTF-8 are
/// kept as raw-byte chars (`graphviz_pure_core::charset`), so labels are converted once,
/// where C converts them, and the output carries C's bytes.
fn read_input(path: Option<&OsStr>) -> io::Result<String> {
    let bytes = match path {
        Some(p) => fs::read(p)?,
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };
    Ok(graphviz_pure_core::charset::decode(bytes))
}

/// `agerrorf` of an input's error, as C's bytes. A syntax error names the file `aagerror`
/// knows: the input, or the one a `# N "file"` line directive named.
fn report_error(name: &str, e: &graphviz_pure_core::GraphvizError) {
    let name = match e {
        graphviz_pure_core::GraphvizError::SyntaxError {
            file: Some(file), ..
        } => file.as_str(),
        _ => name,
    };
    agerrorf(&format!("{name}: {e}\n"));
}

fn main() -> ExitCode {
    // `args_os`: `args` panics on an argument that is not valid UTF-8.
    let raw: Vec<OsString> = env::args_os().collect();
    let argv0 = raw
        .first()
        .and_then(|a| Path::new(a).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("graphviz-pure")
        .to_string();
    let opts = match parse_args(&argv0, &raw[1..]) {
        Parsed::Run(o) => o,
        Parsed::Exit(code) => return code,
    };

    let raster = graphviz_pure_raster::RasterFormat::parse(&opts.format);
    if raster.is_none() && OutputFormat::parse(&opts.format).is_err() {
        // `dotneato_args_initialize` writes these two with `fprintf`, not `agerrorf`.
        fprintf_stderr(&format!("Format: \"{}\" not recognized.\n", opts.format));
        return ExitCode::FAILURE;
    }
    if LayoutEngine::parse(&opts.engine).is_err() {
        fprintf_stderr(&format!(
            "There is no layout engine support for \"{}\"\n",
            opts.engine
        ));
        return ExitCode::FAILURE;
    }

    let mut shared_out: Option<Box<dyn Write>> = match (&opts.output, opts.auto_output) {
        (Some(path), false) => match fs::File::create(path) {
            Ok(f) => Some(Box::new(f)),
            Err(e) => {
                // gvdevice.c `gvdevice_initialize`.
                let path = arg_text(path);
                agerrorf(&format!(
                    "Could not open \"{path}\" for writing : {}\n",
                    strerror(&e)
                ));
                return ExitCode::FAILURE;
            }
        },
        _ => None,
    };

    let inputs: Vec<Option<&OsStr>> = if opts.inputs.is_empty() {
        vec![None]
    } else {
        opts.inputs.iter().map(|s| Some(s.as_os_str())).collect()
    };

    let mut failed = false;
    for input in inputs {
        let name = input.map_or_else(|| "<stdin>".to_string(), arg_text);
        let name = name.as_str();
        let source = match read_input(input) {
            Ok(s) => s,
            Err(e) => {
                // input.c `gvNextInputGraph`.
                agerrorf(&format!("{argv0}: can't open {name}: {}\n", strerror(&e)));
                failed = true;
                continue;
            }
        };
        // Raster formats are drawn from the SVG (graphviz-pure-raster); the rest are text.
        let render = || match raster {
            Some(fmt) => {
                let (svgs, mut err) = graphviz_pure::render_svg_all_partial(
                    &source,
                    &opts.engine,
                    &opts.globals,
                    opts.layout,
                );
                let mut images = Vec::with_capacity(svgs.len());
                for (svg, explicit_dpi) in svgs {
                    match graphviz_pure_raster::rasterize(&svg, fmt, explicit_dpi) {
                        Ok(b) => images.push(b),
                        Err(e) => {
                            err = Some(graphviz_pure_core::GraphvizError::RenderError(e));
                            break;
                        }
                    }
                }
                (images, err)
            }
            None => {
                let (texts, err) = graphviz_pure::render_dot_all_partial(
                    &source,
                    &opts.engine,
                    &opts.format,
                    &opts.globals,
                    opts.layout,
                );
                let bytes = texts
                    .iter()
                    .map(|t| graphviz_pure_core::charset::encode(t).into_owned())
                    .collect();
                (bytes, err)
            }
        };
        // `agread` on the file: read through `fgets`, named as `agconcat`'s `filename`.
        let (outputs, err): (Vec<Vec<u8>>, _) = graphviz_pure_parser::reading_file(name, render);
        if let Some(e) = &err {
            failed = true;
            if outputs.is_empty() {
                report_error(name, e);
                continue;
            }
        }

        let mut per_input: Option<Box<dyn Write>> = None;
        if let (true, Some(path)) = (opts.auto_output, input) {
            let mut target = path.to_os_string();
            target.push(format!(".{}", opts.format));
            match fs::File::create(&target) {
                Ok(f) => per_input = Some(Box::new(f)),
                Err(e) => {
                    let target = arg_text(&target);
                    agerrorf(&format!(
                        "Could not open \"{target}\" for writing : {}\n",
                        strerror(&e)
                    ));
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
        for bytes in outputs {
            if let Err(e) = out.write_all(&bytes) {
                fprintf_stderr(&format!("{argv0}: write error: {e}\n"));
                return ExitCode::FAILURE;
            }
        }
        // The graphs read before a syntax error are written, then the error reported.
        if let Some(e) = err {
            let _ = out.flush();
            report_error(name, &e);
        }
    }

    if let Some(w) = shared_out.as_mut() {
        let _ = w.flush();
    }
    // dot's main returns `graphviz_errors + agerrors()`: an error reported during layout
    // or rendering fails the run even though the output was written.
    if failed || graphviz_pure_core::diag::error_count() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
