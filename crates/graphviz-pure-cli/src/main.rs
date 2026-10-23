use graphviz_pure::{
    DEVICE_PLUGINS, FdpParms, GlobalAttrs, GraphJobs, GraphStream, LAYOUT_PLUGINS, LayoutEngine,
    LayoutJobs, LayoutOptions, OutputFormat,
};
use graphviz_pure_core::diag::{AgErrLevel, agerrorf, agseterr, agwarningf, fprintf_stderr};
use std::borrow::Cow;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

const VERSION: &str = concat!(
    "graphviz-pure - graphviz-pure version ",
    env!("CARGO_PKG_VERSION"),
    " (Rust port of Graphviz 16.1.0)"
);

// `lib/common/input.c`: `dotneato_usage`'s texts.
const USAGE_FMT: &str = " [-Vv?] [-(GNEA)name=val] [-(KTlso)<val>] <dot files>\n";
const NEATO_FLAGS: &str = "(additional options for neato)    [-x] [-n<v>]\n";
const FDP_FLAGS: &str = "(additional options for fdp)      [-L(gO)] [-L(nUCT)<val>]\n";
const CONFIG_FLAGS: &str = "(additional options for config)  [-cv]\n";
// The trailing spaces of three lines are C's.
const GENERIC_ITEMS: &str = concat!(
    "\n",
    " -V          - Print version and exit\n",
    " -v[n]       - Enable verbose mode at level 1, or set to the level 'n' if provided\n",
    " -Gname=val  - Set graph attribute 'name' to 'val'\n",
    " -Nname=val  - Set node attribute 'name' to 'val'\n",
    " -Ename=val  - Set edge attribute 'name' to 'val'\n",
    " -Aname=val  - Set attribute 'name' to 'val' for graph, node, and edge\n",
    " -Tv         - Set output format to 'v'\n",
    " -Kv         - Set layout engine to 'v' (overrides default based on command name)\n",
    " -lv         - Use external library 'v'\n",
    " -ofile      - Write output to 'file'\n",
    " -O          - Automatically generate an output filename based on the input filename \
     with a .'format' appended. (Causes all -ofile options to be ignored.) \n",
    " -P          - Internally generate a graph of the current plugins. \n",
    " -q[l]       - Set level of message suppression (=1)\n",
    " -s[v]       - Scale input by 'v' (=72)\n",
    " -y          - Invert y coordinate in output\n",
);
const NEATO_ITEMS: &str = concat!(
    "\n",
    " -n[v]       - No layout mode 'v' (=1)\n",
    " -x          - Reduce graph\n",
);
const FDP_ITEMS: &str = concat!(
    "\n",
    " -Lg         - Don't use grid\n",
    " -LO         - Use old attractive force\n",
    " -Lm<l>      - Set m-limit to l\n",
    " -Ln<i>      - Set number of iterations to i\n",
    " -LU<i>      - Set unscaled factor to i\n",
    " -LC<v>      - Set overlap expansion factor to v\n",
    " -LT[*]<v>   - Set temperature (temperature factor) to v\n",
);
const CONFIG_ITEMS: &str = concat!(
    "\n",
    " -c          - Configure plugins (Writes $prefix/lib/graphviz/config \n",
    "               with available plugin information.  Needs write privilege.)\n",
    " -?          - Print usage and exit\n",
);

/// One `-T`/`-o` job (`GVJ_t`): the output format and file, and the file once opened.
struct Job {
    /// `output_langname`: the `-T` value as given (`svg`, `png:cairo`).
    langname: Option<String>,
    /// `output_filename`; `None` writes to stdout.
    filename: Option<OsString>,
    /// `output_file` while the job's device is open.
    file: Option<Sink>,
}

enum Sink {
    Stdout,
    File(io::BufWriter<fs::File>),
}

impl Sink {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Sink::Stdout => io::stdout().write_all(bytes),
            Sink::File(f) => f.write_all(bytes),
        }
    }
}

struct Options {
    /// The command name (`gvc->common.cmdname`), for messages.
    cmdname: String,
    engine: LayoutEngine,
    jobs: Vec<Job>,
    auto_output: bool,
    globals: GlobalAttrs,
    layout: LayoutOptions,
    y_invert: bool,
    inputs: Vec<OsString>,
}

enum Parsed {
    Run(Box<Options>),
    Exit(ExitCode),
}

/// `dotneato_usage(argv0, exval)`: the usage text, on stdout for `-?` and on stderr after
/// an error, which then ends the run with status 1.
fn usage(argv0: &str, exval: u8) -> ExitCode {
    let text = format!(
        "Usage: {argv0}{USAGE_FMT}{NEATO_FLAGS}{FDP_FLAGS}{CONFIG_FLAGS}{GENERIC_ITEMS}\
         {NEATO_ITEMS}{FDP_ITEMS}{CONFIG_ITEMS}"
    );
    if exval > 0 {
        fprintf_stderr(&text);
    } else {
        print!("{text}");
    }
    ExitCode::from(exval)
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

/// `strtol(s, &end, 10)`: the value, or `None` when no digits were read (`end == s`).
fn strtol(s: &str) -> Option<i64> {
    let s = s.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    if !digits.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    // strtol saturates at the `long` range.
    let mut v: i64 = 0;
    for b in digits.bytes().take_while(u8::is_ascii_digit) {
        v = v.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    Some(if neg { v.saturating_neg() } else { v })
}

/// `atoi(s)`: C's white space, a sign, then digits; what it gives an `int` beyond its range.
fn atoi(s: &str) -> i32 {
    // `atoi` is `(int)strtol(s, NULL, 10)`.
    strtol(s).unwrap_or(0) as i32
}

/// `strtod(s, &end)`: the value, or `None` when nothing was read (`end == s`).
fn strtod(s: &str) -> Option<f64> {
    match graphviz_pure_geom::node_size::strtod_prefix(s) {
        (_, 0) => None,
        (v, _) => Some(v),
    }
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

/// `dotneato_basename`: the command name in an `argv[0]`.
fn basename(argv0: &str) -> &str {
    let trimmed = argv0.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return &argv0[..argv0.len().min(1)];
    }
    match trimmed.rfind(['/', '\\']) {
        Some(at) => &trimmed[at + 1..],
        None => trimmed,
    }
}

/// `setFDPAttr`: one `-L` flag (`arg` is what follows `-L`). `Err` after `bad value`, which
/// ends the run with the usage text.
fn set_fdp_attr(fdp: &mut FdpParms, arg: &str) -> Result<(), ()> {
    let mut chars = arg.chars();
    let flag = chars.next();
    let val = chars.as_str();
    // `setDouble`/`setInt`: the error names the flag, `-L` and all.
    let bad = || agerrorf(&format!("bad value in flag -L{arg} - ignored\n"));
    let double = |v: &str| strtod(v).ok_or_else(bad);
    let int = |v: &str| strtol(v).map(|i| i64::from(i as i32)).ok_or_else(bad);
    match flag {
        Some('g') => fdp.use_grid = false,
        Some('O') => fdp.use_new = false,
        Some('m') => fdp.mlimit = double(val)?,
        Some('n') => fdp.num_iters = int(val)?,
        Some('U') => fdp.unscaled = int(val)?,
        Some('C') => fdp.c = double(val)?,
        Some('T') => match val.strip_prefix('*') {
            Some(v) => fdp.tfact = double(v)?,
            None => fdp.t0 = double(val)?,
        },
        _ => agwarningf(&format!("unknown flag -L{arg} - ignored\n")),
    }
    Ok(())
}

/// `gvjobs_output_langname`/`gvjobs_output_filename`: the `n`th `-T` and the `n`th `-o`
/// fill the `n`th job.
fn job_at(jobs: &mut Vec<Job>, n: usize) -> &mut Job {
    while jobs.len() <= n {
        jobs.push(Job {
            langname: None,
            filename: None,
            file: None,
        });
    }
    &mut jobs[n]
}

/// Mirrors `gvParseArgs`: `neato_extra_args`, `fdp_extra_args` and `config_extra_args` take
/// their flags out of the arguments in turn, then `dotneato_args_initialize` reads the rest.
/// File names (`-o` and the inputs) are kept as the OS gave them, so a name that is not valid
/// UTF-8 still opens.
fn parse_args(argv0: &str, os_args: &[OsString]) -> Parsed {
    let cmdname = basename(argv0).to_string();
    let mut layout = LayoutOptions::default();
    let mut auto_output = false;
    let mut config = false;

    // `neato_extra_args`: `-n` and `-x`.
    let mut kept: Vec<OsString> = Vec::with_capacity(os_args.len());
    for os_arg in os_args {
        let bytes = arg_bytes(os_arg);
        if bytes.len() < 2 || bytes[0] != b'-' || !matches!(bytes[1], b'n' | b'x') {
            kept.push(os_arg.clone());
            continue;
        }
        if bytes[1] == b'x' {
            layout.reduce = true;
            continue;
        }
        let rest = bytes_text(&bytes[2..]);
        if rest.is_empty() {
            layout.nop = 1;
            continue;
        }
        // `Nop = atoi(arg + 2)`, an error when not positive.
        match atoi(&rest) {
            n if n > 0 => layout.nop = n.min(255) as u8,
            _ => {
                agerrorf(&format!("Invalid parameter \"{rest}\" for -n flag\n"));
                return Parsed::Exit(usage(argv0, 1));
            }
        }
    }
    // `fdp_extra_args`: `-L`.
    let os_args = std::mem::take(&mut kept);
    for os_arg in os_args {
        let bytes = arg_bytes(&os_arg);
        if bytes.len() < 2 || bytes[0] != b'-' || bytes[1] != b'L' {
            kept.push(os_arg.clone());
            continue;
        }
        if set_fdp_attr(&mut layout.fdp, &bytes_text(&bytes[2..])).is_err() {
            return Parsed::Exit(usage(argv0, 1));
        }
    }
    // `config_extra_args`: `-v`, `-O` and `-c`.
    let os_args = std::mem::take(&mut kept);
    for os_arg in os_args {
        let bytes = arg_bytes(&os_arg);
        match (bytes.first(), bytes.get(1)) {
            // Verbose mode is accepted; the port has no verbose messages.
            (Some(b'-'), Some(b'v')) => {}
            (Some(b'-'), Some(b'O')) => auto_output = true,
            (Some(b'-'), Some(b'c')) => config = true,
            _ => kept.push(os_arg.clone()),
        }
    }

    // `dotneato_args_initialize`.
    if env::var_os("GV_FILE_PATH").is_some() {
        fprintf_stderr(
            "$GV_FILE_PATH environment variable set; exiting\n\n\
             This sandboxing mechanism is no longer supported\n",
        );
        return Parsed::Exit(ExitCode::FAILURE);
    }
    // `dot -c` writes the plugin configuration and exits; the port's plugins are built in.
    if config {
        return Parsed::Exit(ExitCode::SUCCESS);
    }
    let os_args = kept.as_slice();
    let mut opts = Options {
        cmdname,
        engine: LayoutEngine::Dot,
        jobs: Vec::new(),
        auto_output,
        globals: GlobalAttrs::default(),
        layout,
        y_invert: false,
        inputs: Vec::new(),
    };
    let (mut n_lang, mut n_file) = (0, 0);
    let mut kflag = false;
    let missing = |flag: char| {
        fprintf_stderr(&format!("Missing argument for -{flag} flag\n"));
        Parsed::Exit(usage(argv0, 1))
    };

    let mut i = 0;
    while i < os_args.len() {
        let arg = arg_text(&os_args[i]);
        let bytes = arg_bytes(&os_args[i]);
        if arg.starts_with("-V") || arg == "--version" {
            fprintf_stderr(&format!("{VERSION}\n"));
            return Parsed::Exit(ExitCode::SUCCESS);
        } else if arg.starts_with("-?") || arg == "--help" {
            return Parsed::Exit(usage(argv0, 0));
        } else if arg.starts_with("--filepath=") {
            // `Gvfilepath`, the directories image files are looked up in: the port reads
            // images by the name the graph gives.
        } else if bytes.first() == Some(&b'-') {
            // `dotneato_args_initialize` switches on the byte `arg[1]`, which need not start
            // a char (and is the terminating NUL for `-` alone).
            let flag = match bytes.get(1) {
                None => '\0',
                Some(&b) if b.is_ascii() => b as char,
                Some(&b) => graphviz_pure_core::charset::raw_char(b),
            };
            let rest = bytes_text(bytes.get(2..).unwrap_or_default());
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
                'T' => {
                    let Some(v) = flag_value(os_args, &mut i) else {
                        return missing('T');
                    };
                    job_at(&mut opts.jobs, n_lang).langname = Some(v.clone());
                    n_lang += 1;
                    if graphviz_pure::plugin_load(&DEVICE_PLUGINS, &v).is_none() {
                        // `dotneato_args_initialize` writes these with `fprintf`.
                        fprintf_stderr(&format!(
                            "Format: \"{v}\" not recognized. Use one of:{}\n",
                            graphviz_pure::plugin_list(&DEVICE_PLUGINS, &v)
                        ));
                        return Parsed::Exit(ExitCode::FAILURE);
                    }
                }
                'K' => {
                    let Some(v) = flag_value(os_args, &mut i) else {
                        return missing('K');
                    };
                    match graphviz_pure::layout_select(&v) {
                        Some(e) => opts.engine = e,
                        None => {
                            fprintf_stderr(&format!(
                                "There is no layout engine support for \"{v}\"\n Use one of:{}\n",
                                graphviz_pure::plugin_list(&LAYOUT_PLUGINS, &v)
                            ));
                            return Parsed::Exit(ExitCode::FAILURE);
                        }
                    }
                    kflag = true;
                }
                // `-P` renders the graph of the plugins instead of the inputs; the port
                // has no plugin graph.
                'P' => {}
                'l' => {
                    if flag_value(os_args, &mut i).is_none() {
                        return missing('l');
                    }
                }
                'o' => {
                    let at = i;
                    let name = match flag_value(os_args, &mut i) {
                        Some(_) if i > at => os_args[i].clone(),
                        Some(_) => os_tail(&os_args[i]),
                        None => return missing('o'),
                    };
                    if !opts.auto_output {
                        job_at(&mut opts.jobs, n_file).filename = Some(name);
                        n_file += 1;
                    }
                }
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
                // `PSinputscale`: `-s` alone and `-s0` are 72.
                's' => {
                    let scale = if rest.is_empty() {
                        72.0
                    } else {
                        strtod(rest).unwrap_or(0.0)
                    };
                    if scale < 0.0 {
                        fprintf_stderr(&format!("Invalid parameter \"{rest}\" for -s flag\n"));
                        return Parsed::Exit(usage(argv0, 1));
                    }
                    opts.layout.inputscale = if scale == 0.0 { 72.0 } else { scale };
                }
                'y' => opts.y_invert = true,
                _ => {
                    agerrorf(&format!(
                        "{}: option -{flag} unrecognized\n\n",
                        opts.cmdname
                    ));
                    return Parsed::Exit(usage(argv0, 1));
                }
            }
        } else {
            opts.inputs.push(os_args[i].clone());
        }
        i += 1;
    }

    // Without `-K`, the layout is the command's name (`dot` for any other name).
    if !kflag {
        opts.engine = graphviz_pure::layout_select(&opts.cmdname).unwrap_or(LayoutEngine::Dot);
    }
    // Without `-T`, the first job writes `dot`; a job with no `-T` repeats the one before
    // it (`gvjobs_next`).
    let first = job_at(&mut opts.jobs, 0);
    if first.langname.is_none() {
        first.langname = Some("dot".to_string());
    }
    for n in 1..opts.jobs.len() {
        if opts.jobs[n].langname.is_none() {
            opts.jobs[n].langname = opts.jobs[n - 1].langname.clone();
        }
    }
    Parsed::Run(Box::new(opts))
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

/// `auto_output_filename`: `<input>[.<graph number>].<format>`, with the parts of a
/// `format:renderer:package` name in reverse order (`noname.gv` for stdin).
fn auto_output_filename(input: Option<&OsStr>, graph_index: usize, langname: &str) -> OsString {
    let mut name = input.map_or_else(|| OsString::from("noname.gv"), OsStr::to_os_string);
    if graph_index > 0 {
        name.push(format!(".{}", graph_index + 1));
    }
    let parts: Vec<&str> = langname.split(':').collect();
    let mut suffix = String::new();
    for p in parts.iter().rev() {
        suffix.push('.');
        suffix.push_str(p);
    }
    name.push(suffix);
    name
}

/// The output of one job for one graph: text formats from their writer, raster formats
/// drawn from the SVG (graphviz-pure-raster).
fn render_job(
    jobs: &mut GraphJobs<'_>,
    graph: &graphviz_pure::Graph,
    langname: &str,
) -> Result<Vec<u8>, graphviz_pure_core::GraphvizError> {
    let typ = langname.split(':').next().unwrap_or("");
    if let Some(fmt) = graphviz_pure_raster::RasterFormat::parse(typ) {
        // Graphviz draws raster formats at 96 dpi unless the graph sets `dpi` (or
        // `resolution`), which then scales the drawing.
        let explicit_dpi = ["dpi", "resolution"].iter().any(|k| {
            graph
                .graph_attrs
                .get(*k)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .is_some_and(|d| d > 0.0)
        });
        let svg = jobs.render(OutputFormat::Svg, "svg")?;
        return graphviz_pure_raster::rasterize(&svg, fmt, explicit_dpi)
            .map_err(graphviz_pure_core::GraphvizError::RenderError);
    }
    let text = jobs.render(OutputFormat::parse(typ)?, langname)?;
    Ok(graphviz_pure_core::charset::encode(&text).into_owned())
}

/// The run's state across graphs: `gvRenderJobs`'s jobs and active job, and `dot`'s
/// error counts.
struct Run<'a> {
    opts: &'a mut Options,
    layout: LayoutJobs,
    /// `gvc->active_jobs`: the job whose device is open.
    active: Option<usize>,
    /// `rc` in `dot`'s `main`: the highest error level of a graph.
    rc: i32,
    /// `graphviz_errors`: inputs that could not be opened.
    graphviz_errors: i32,
}

impl Run<'_> {
    /// `gvrender_end_job`: the device of `n` is finalized; a file is closed, and the job
    /// then writes to stdout (`gvdevice_close` forgets its file name).
    fn end_job(&mut self, n: usize) -> io::Result<()> {
        let job = &mut self.opts.jobs[n];
        match job.file.take() {
            Some(Sink::File(mut f)) => {
                job.filename = None;
                f.flush()?;
            }
            Some(Sink::Stdout) => {
                io::stdout().flush()?;
                job.file = Some(Sink::Stdout);
            }
            None => {}
        }
        Ok(())
    }

    /// `gvRenderJobs` for one graph. `Err` is a write error.
    fn render_jobs(
        &mut self,
        graph: &graphviz_pure::Graph,
        input: Option<&OsStr>,
        graph_index: usize,
        name: &str,
    ) -> io::Result<()> {
        let mut graph_jobs = GraphJobs::new(graph);
        for n in 0..self.opts.jobs.len() {
            // Every job's device writes its own output (no format of the port does pages),
            // so the device open before it is finalized.
            if let Some(active) = self.active.take() {
                self.end_job(active)?;
            }
            // `gvrender_begin_job` → `gvdevice_initialize`: open the job's file unless
            // it still is.
            let job = &mut self.opts.jobs[n];
            let langname = job.langname.clone().unwrap_or_default();
            if job.file.is_none() {
                if self.opts.auto_output {
                    job.filename = Some(auto_output_filename(input, graph_index, &langname));
                }
                job.file = Some(match &job.filename {
                    Some(path) => match fs::File::create(path) {
                        Ok(f) => Sink::File(io::BufWriter::new(f)),
                        Err(e) => {
                            agerrorf(&format!(
                                "Could not open \"{}\" for writing : {}\n",
                                arg_text(path),
                                strerror(&e)
                            ));
                            continue;
                        }
                    },
                    None => Sink::Stdout,
                });
            }
            self.active = Some(n);
            match render_job(&mut graph_jobs, graph, &langname) {
                Ok(bytes) => {
                    let sink = self.opts.jobs[n].file.as_mut().expect("opened above");
                    sink.write_all(&bytes)?;
                }
                Err(e) => report_error(name, &e),
            }
        }
        Ok(())
    }

    /// One input (`gvNextInputGraph` over a file): each graph is read, laid out and
    /// rendered before the next is read. `Ok(false)` when a layout failed where C exits or
    /// crashes, which ends the run; `Err` is a write error.
    fn input(&mut self, input: Option<&OsStr>) -> io::Result<bool> {
        let name = input.map_or_else(|| "<stdin>".to_string(), arg_text);
        let source = match read_input(input) {
            Ok(s) => s,
            Err(e) => {
                agerrorf(&format!(
                    "{}: can't open {name}: {}\n",
                    self.opts.cmdname,
                    strerror(&e)
                ));
                self.graphviz_errors += 1;
                return Ok(true);
            }
        };
        // `agread` on the file: read through `fgets`, named as `agconcat`'s `filename`.
        graphviz_pure_parser::reading_file(&name, || {
            let globals = self.opts.globals.clone();
            let mut stream = GraphStream::new(&source, &globals);
            let mut graph_index = 0;
            while let Some(read) = stream.next_graph(|e| report_error(&name, e)) {
                let Ok(mut graph) = read else { break };
                match self.layout.layout(&mut graph) {
                    Ok(true) => self.render_jobs(&graph, input, graph_index, &name)?,
                    Ok(false) => agerrorf("Layout was not done.  Missing layout plugins? \n"),
                    // A layout the port stops where C exits (`graphviz_exit`) or crashes.
                    Err(e) => {
                        report_error(&name, &e);
                        return Ok(false);
                    }
                }
                graph_index += 1;
                // `agreseterrors`: the graph's errors set the status.
                let errors = graphviz_pure_core::diag::error_count();
                graphviz_pure_core::diag::reset_error_count();
                self.rc = self.rc.max(i32::from(errors > 0));
            }
            Ok(true)
        })
    }
}

fn main() -> ExitCode {
    // `args_os`: `args` panics on an argument that is not valid UTF-8.
    let raw: Vec<OsString> = env::args_os().collect();
    let argv0 = raw
        .first()
        .map_or_else(|| "dot".to_string(), |a| arg_text(a));
    let mut opts = match parse_args(&argv0, raw.get(1..).unwrap_or_default()) {
        Parsed::Run(o) => o,
        Parsed::Exit(code) => return code,
    };
    graphviz_pure::set_y_invert(opts.y_invert);
    let inputs: Vec<Option<OsString>> = if opts.inputs.is_empty() {
        vec![None]
    } else {
        opts.inputs.iter().cloned().map(Some).collect()
    };
    let layout = LayoutJobs::new(opts.engine, opts.layout);
    let mut run = Run {
        opts: &mut opts,
        layout,
        active: None,
        rc: 0,
        graphviz_errors: 0,
    };
    let mut written = Ok(());
    let mut stopped = false;
    for input in &inputs {
        match run.input(input.as_deref()) {
            Ok(true) => {}
            Ok(false) => {
                stopped = true;
                break;
            }
            Err(e) => {
                written = Err(e);
                break;
            }
        }
    }
    // `gvFinalize`: the last active job is finalized.
    if written.is_ok()
        && let Some(active) = run.active.take()
    {
        written = run.end_job(active);
    }
    if let Err(e) = written {
        fprintf_stderr(&format!("{}: write error: {e}\n", run.opts.cmdname));
        return ExitCode::FAILURE;
    }
    if stopped {
        return ExitCode::FAILURE;
    }
    // `main` returns the larger of the graphs' statuses and `gvFreeContext`'s
    // `graphviz_errors + agerrors()`.
    let errors = i32::from(graphviz_pure_core::diag::error_count() > 0);
    let status = run.rc.max(run.graphviz_errors + errors);
    // `exit` keeps the status's low 8 bits.
    ExitCode::from(status as u8)
}
