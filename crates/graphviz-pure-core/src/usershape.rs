//! User-shape files: how C finds and opens the files named by `image`, `shapefile` and an
//! HTML `<IMG SRC>` (`safefile` in `lib/common/utils.c`, `gvusershape_file_access`,
//! `gvusershape_open` and `gvusershape_size` in `lib/gvc/gvusershape.c`, `user_init` and
//! `epsf_init` in `lib/common/psusershape.c`), with the warnings C prints when it cannot.
//!
//! The port reads no image data. It opens the file and classifies its header as `imagetype`
//! does, which is all C needs to decide whether the load failed; the size of an image that
//! loads is not read (its callers keep it at 0x0). For `shape=epsf` it reads the
//! `%%BoundingBox` comment, which gives the node its size.
//!
//! Paths follow C's Unix rules on every platform: `imagepath` and `--filepath` list
//! directories separated by `:`, joined to the file name with `/`, and a name starting with
//! `/` is absolute. The WASM build has no filesystem: every file is missing there, so it
//! reports `No such file or directory` as C does for a file that does not exist.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// `agwarningf`, except in the port's repeat sizings ([`crate::diag::quietly`]).
fn agwarningf(msg: &str) {
    if crate::diag::reporting() {
        crate::diag::agwarningf(msg);
    }
}

/// `Gvfilepath`: the directories of `dot --filepath=` (process-wide in C).
static FILEPATH: Mutex<Option<String>> = Mutex::new(None);
/// `HTTPServerEnVar`: `$SERVER_NAME`, read by `dotneato_args_initialize`.
static SERVER_NAME: Mutex<Option<String>> = Mutex::new(None);
/// `safefile`'s `onetime` flag.
static SERVER_WARNED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// `Gvimagepath` of the graph being laid out (`graph_init`).
    static IMAGEPATH: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Whether the graph being laid out declares the node attribute `shapefile`.
    static SHAPEFILE_DECLARED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The names in `UserShape` (`user_shape` in `shapes.c`), which `gvusershape_open` takes
    /// for a file it cannot classify. C never clears the list.
    static USER_SHAPES: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// Sets `Gvfilepath` (`dot --filepath=DIRS`).
pub fn set_filepath(dirs: Option<&str>) {
    *FILEPATH.lock().unwrap_or_else(|e| e.into_inner()) = dirs.map(str::to_string);
}

/// Sets `HTTPServerEnVar` (the CLI's `$SERVER_NAME`): file loading is then disabled.
pub fn set_server_name(name: Option<&str>) {
    *SERVER_NAME.lock().unwrap_or_else(|e| e.into_inner()) = name.map(str::to_string);
}

fn filepath() -> Option<String> {
    FILEPATH.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn server_name() -> Option<String> {
    SERVER_NAME
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// `graph_init`: `Gvimagepath` is the root graph's `imagepath`, or `Gvfilepath` when the
/// graph has none. `shapefile_declared`: some node declares `shapefile`.
pub fn begin_layout(imagepath: Option<&str>, shapefile_declared: bool) {
    let path = imagepath.map(str::to_string).or_else(filepath);
    IMAGEPATH.with(|p| *p.borrow_mut() = path);
    SHAPEFILE_DECLARED.with(|d| d.set(shapefile_declared));
}

/// `agget(n, "shapefile")` for a node whose own value is `value`: the declared default `""`
/// when it has none, NULL when no node declares the attribute.
pub fn agget_shapefile(value: Option<&str>) -> Option<&str> {
    value.or_else(|| SHAPEFILE_DECLARED.with(std::cell::Cell::get).then_some(""))
}

/// `user_shape` records `name` in `UserShape`.
pub fn register_user_shape(name: &str) {
    USER_SHAPES.with(|s| {
        if !s.borrow().contains(name) {
            s.borrow_mut().insert(name.to_string());
        }
    });
}

fn find_user_shape(name: &str) -> bool {
    USER_SHAPES.with(|s| s.borrow().contains(name))
}

/// `mkDirlist`: the directories of a `:`-separated list, as `tok` splits it (a leading or a
/// trailing separator gives an empty directory).
fn mk_dirlist(list: &str) -> Vec<&str> {
    let b = list.as_bytes();
    let cspn = |i: usize| b[i..].iter().take_while(|&&c| c != b':').count();
    let mut dirs = vec![&list[..cspn(0)]];
    let mut end = dirs[0].len();
    while end < b.len() {
        let start = end + b[end..].iter().take_while(|&&c| c == b':').count();
        end = start + cspn(start);
        dirs.push(&list[start..end]);
    }
    dirs
}

/// `findPath`: the first `dir/name` that can be read.
fn find_path(dirs: &[&str], name: &str) -> Option<String> {
    dirs.iter()
        .map(|d| format!("{d}/{name}"))
        .find(|f| open(f).is_ok())
}

/// `safefile`: the path C opens for `filename`, or `None` when it may not be read.
pub fn safefile(filename: &str) -> Option<String> {
    if filename.is_empty() {
        return None;
    }
    if let Some(server) = server_name() {
        if crate::diag::reporting() && !SERVER_WARNED.swap(true, Ordering::Relaxed) {
            agwarningf(&format!(
                "file loading is disabled because the environment contains SERVER_NAME=\"{server}\"\n"
            ));
        }
        return None;
    }
    if let Some(list) = filepath() {
        // only the last component of the name, looked up in `--filepath`
        let base = filename.rsplit(['/', '\\', ':']).next().unwrap_or(filename);
        return find_path(&mk_dirlist(&list), base);
    }
    let list = IMAGEPATH.with(|p| p.borrow().clone()).unwrap_or_default();
    if filename.starts_with('/') || list.is_empty() {
        Some(filename.to_string())
    } else {
        find_path(&mk_dirlist(&list), filename)
    }
}

/// `bind_shape`: a non-empty `shapefile` that `safefile` accepts makes the shape `custom`
/// (except for `epsf`).
pub fn binds_custom(shape: &str, shapefile: Option<&str>) -> bool {
    shape != "epsf" && shapefile.is_some_and(|f| safefile(f).is_some())
}

/// The bytes C passes to `fopen` for `path`.
fn os_path(path: &str) -> std::path::PathBuf {
    let bytes = crate::charset::encode(path);
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&bytes))
    }
    #[cfg(not(unix))]
    {
        std::path::PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
    }
}

/// An opened file, or a directory, which `fopen` opens on Unix and which then reads nothing.
enum Opened {
    File(File),
    Dir,
}

impl Opened {
    /// Up to `n` bytes from the start (`fread`).
    fn head(&mut self, n: usize) -> Vec<u8> {
        let mut buf = Vec::new();
        if let Opened::File(f) = self {
            let _ = f.take(n as u64).read_to_end(&mut buf);
        }
        buf
    }

    fn all(&mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        if let Opened::File(f) = self {
            let _ = f.read_to_end(&mut buf);
        }
        buf
    }
}

/// `fopen(path, "r")`.
fn open(path: &str) -> io::Result<Opened> {
    let p = os_path(path);
    if std::fs::metadata(&p).is_ok_and(|m| m.is_dir()) {
        return Ok(Opened::Dir);
    }
    File::open(p).map(Opened::File)
}

/// `strerror(errno)` for a failed `fopen`, with POSIX's texts. Without a filesystem (WASM)
/// a file is missing.
fn strerror(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::Unsupported => {
            "No such file or directory".to_string()
        }
        io::ErrorKind::PermissionDenied => "Permission denied".to_string(),
        io::ErrorKind::NotADirectory => "Not a directory".to_string(),
        io::ErrorKind::IsADirectory => "Is a directory".to_string(),
        _ => {
            let s = e.to_string();
            match s.find(" (os error ") {
                Some(i) => s[..i].to_string(),
                None => s,
            }
        }
    }
}

/// `knowntypes` in `gvusershape.c`: the magic numbers `imagetype` recognises (`HDRLEN` is 20).
const KNOWN_TYPES: [&[u8]; 10] = [
    b"\x89PNG\x0D\x0A\x1A\x0A",
    b"%!PS-Adobe-",
    b"BM",
    b"GIF8",
    b"\xFF\xD8\xFF",
    b"%PDF-",
    b"\xC5\xD0\xD3\xC6",
    b"<?xml",
    b"RIFF",
    b"\x00\x00\x01\x00",
];

/// `gvusershape_size` failing: C returns `-1,-1` when there is no name, the file cannot be
/// opened (after `gvusershape_file_access`'s warning) or it is neither an image nor a user
/// shape. `true` when C loads it.
pub fn usershape_loads(name: Option<&str>) -> bool {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return false;
    };
    // gvusershape_file_access
    let Some(path) = safefile(name) else {
        agwarningf(&format!("Filename \"{name}\" is unsafe\n"));
        return false;
    };
    let mut f = match open(&path) {
        Ok(f) => f,
        Err(e) => {
            agwarningf(&format!("{} while opening {path}\n", strerror(&e)));
            return false;
        }
    };
    // imagetype: FT_NULL unless the first HDRLEN bytes start with a known magic number
    let header = f.head(20);
    let known = header.len() == 20 && KNOWN_TYPES.iter().any(|m| header.starts_with(m));
    if !known && !find_user_shape(name) {
        agwarningf(&format!(
            "\"{name}\" was not found as a file or as a shape library member\n"
        ));
        return false;
    }
    true
}

/// `user_init`'s scan of the file, line by line as `fgets` reads them (`BUFSIZ` is 1024 on
/// the reference platform): every `sscanf` stores the numbers it matches, so the last
/// `%%BoundingBox` line wins, and the scan stops early only once it has also seen a line
/// that is not a comment and contains `read` (`must_inline`).
fn bounding_box(data: &[u8]) -> Option<[i32; 4]> {
    const BUFSIZ: usize = 1024;
    let mut bb = [0i32; 4];
    let (mut saw_bb, mut must_inline) = (false, false);
    let mut rest = data;
    while !rest.is_empty() {
        let len = rest
            .iter()
            .take(BUFSIZ - 1)
            .position(|&c| c == b'\n')
            .map_or(rest.len().min(BUFSIZ - 1), |i| i + 1);
        let (line, tail) = rest.split_at(len);
        rest = tail;
        // sscanf and strstr read the line as a C string
        let line = &line[..line.iter().position(|&c| c == 0).unwrap_or(line.len())];
        if scan_bounding_box(line, &mut bb) == 4 {
            saw_bb = true;
        }
        if line.first() != Some(&b'%') && line.windows(4).any(|w| w == b"read") {
            must_inline = true;
        }
        if saw_bb && must_inline {
            break;
        }
    }
    saw_bb.then_some(bb)
}

/// `isspace` in the C locale.
fn c_isspace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

/// `sscanf(line, "%%%%BoundingBox: %d %d %d %d", ...)`: stores each number it matches in
/// `v` and returns how many. `%%` skips white space before its `%`, as conversions do.
fn scan_bounding_box(line: &[u8], v: &mut [i32; 4]) -> usize {
    let skip = |s: &[u8]| s.iter().take_while(|&&c| c_isspace(c)).count();
    let mut s = line;
    for _ in 0..2 {
        s = &s[skip(s)..];
        match s.strip_prefix(b"%") {
            Some(t) => s = t,
            None => return 0,
        }
    }
    let Some(t) = s.strip_prefix(b"BoundingBox:") else {
        return 0;
    };
    s = t;
    for (k, x) in v.iter_mut().enumerate() {
        s = &s[skip(s)..];
        let sign = usize::from(matches!(s.first(), Some(b'+' | b'-')));
        let digits = s[sign..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return k;
        }
        // strtol: clamped to the long range, then stored into an int
        let neg = s.first() == Some(&b'-');
        let mut n: i64 = 0;
        for &d in &s[sign..sign + digits] {
            let d = i64::from(d - b'0');
            n = n.saturating_mul(10);
            n = if neg {
                n.saturating_sub(d)
            } else {
                n.saturating_add(d)
            };
        }
        *x = n as i32;
        s = &s[sign + digits..];
    }
    4
}

/// `epsf_init`: the `%%BoundingBox` (`lx ly ux uy`) of a `shape=epsf` node's `shapefile`,
/// or `None`, with C's warning, when there is none; the node then keeps no shape
/// information and is not drawn.
pub fn epsf_bounding_box(shapefile: Option<&str>, node: &str) -> Option<[i32; 4]> {
    let Some(path) = shapefile.and_then(safefile) else {
        agwarningf(&format!(
            "shapefile not set or not found for epsf node {node}\n"
        ));
        return None;
    };
    // user_init
    let Ok(mut f) = open(&path) else {
        agwarningf(&format!("couldn't open epsf file {path}\n"));
        return None;
    };
    let bb = bounding_box(&f.all());
    if bb.is_none() {
        agwarningf(&format!("BoundingBox not found in epsf file {path}\n"));
    }
    bb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_box_follows_user_init() {
        assert_eq!(
            bounding_box(b"%!PS\n%%BoundingBox: 10 20 110 70\n"),
            Some([10, 20, 110, 70])
        );
        assert_eq!(
            bounding_box(b"%%BoundingBox:-1 +2\t3   4 junk"),
            Some([-1, 2, 3, 4])
        );
        // sscanf skips white space (\v too) before `%%` and before each number
        assert_eq!(
            bounding_box(b" %%BoundingBox:\x0b1 2 30 40\n"),
            Some([1, 2, 30, 40])
        );
        assert_eq!(
            bounding_box(b"% %BoundingBox: 1 2 30 40\n"),
            Some([1, 2, 30, 40])
        );
        assert_eq!(bounding_box(b"%%BoundingBox: (atend)\n"), None);
        assert_eq!(bounding_box(b"%%BoundingBox: 1 2 3\n"), None);
        // the last line wins, and a partial match overwrites the numbers it matched
        assert_eq!(
            bounding_box(b"%%BoundingBox: 0 0 100 50\n%%BoundingBox: 10 10 30 40\n"),
            Some([10, 10, 30, 40])
        );
        assert_eq!(
            bounding_box(b"%%BoundingBox: 0 0 100 50\n%%BoundingBox: 7 8 (atend)\n"),
            Some([7, 8, 100, 50])
        );
        // strtol clamps, and the int keeps the low bits
        assert_eq!(
            bounding_box(b"%%BoundingBox: -99999999999999999999 0 100 50\n"),
            Some([0, 0, 100, 50])
        );
        // a NUL ends the line for sscanf and strstr
        assert_eq!(
            bounding_box(b"%%BoundingBox: 0 0 100 50\n\0read\n%%BoundingBox: 0 0 10 10\n"),
            Some([0, 0, 10, 10])
        );
        // ... unless a non-comment line with `read` came before
        assert_eq!(
            bounding_box(b"%%BoundingBox: 0 0 100 50\nreadimage\n%%BoundingBox: 1 1 2 2\n"),
            Some([0, 0, 100, 50])
        );
    }

    #[test]
    fn safefile_resolves_like_c() {
        begin_layout(None, false);
        assert_eq!(safefile(""), None);
        assert_eq!(safefile("x.png").as_deref(), Some("x.png"));
        // imagepath: relative names must be found in one of its directories
        begin_layout(Some("/nonexistent-dir-c18"), false);
        assert_eq!(safefile("x.png"), None);
        assert_eq!(safefile("/abs/x.png").as_deref(), Some("/abs/x.png"));
        begin_layout(Some(""), false);
        assert_eq!(safefile("x.png").as_deref(), Some("x.png"));
        begin_layout(None, false);
    }

    #[test]
    fn dirlist_splits_like_tok() {
        assert_eq!(mk_dirlist("a:b"), ["a", "b"]);
        assert_eq!(mk_dirlist("a::b:"), ["a", "b", ""]);
        assert_eq!(mk_dirlist(":a"), ["", "a"]);
        assert_eq!(mk_dirlist(""), [""]);
    }

    #[test]
    fn strerror_uses_posix_texts() {
        let e = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(strerror(&e), "No such file or directory");
        let e = io::Error::from(io::ErrorKind::Unsupported);
        assert_eq!(strerror(&e), "No such file or directory");
        let e = io::Error::from(io::ErrorKind::PermissionDenied);
        assert_eq!(strerror(&e), "Permission denied");
    }
}
