//! User-shape files: how C finds and opens the files named by `image`, `shapefile` and an
//! HTML `<IMG SRC>` (`safefile` in `lib/common/utils.c`, `gvusershape_file_access`,
//! `gvusershape_open` and `gvusershape_size` in `lib/gvc/gvusershape.c`, `user_init` and
//! `epsf_init` in `lib/common/psusershape.c`), with the warnings C prints when it cannot.
//!
//! As C, the port reads no image data: it classifies the file's header as `imagetype`
//! does and reads the size from it (the readers in [`size`]), and keeps what it read by
//! name, as `ImageDict` does, for the renderers (`gvrender_usershape`). For `shape=epsf`
//! it reads the `%%BoundingBox` comment, which gives the node its size.
//!
//! Paths follow C's Unix rules on every platform: `imagepath` and `--filepath` list
//! directories separated by `:`, joined to the file name with `/`, and a name starting with
//! `/` is absolute. The WASM build has no filesystem: every file is missing there, so it
//! reports `No such file or directory` as C does for a file that does not exist.

mod size;

pub use size::ImageType;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};

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
/// graph has none. `shapefile_declared`: some node declares `shapefile`. `dpi` is
/// `GD_drawing(g)->dpi` (`dpi`, else `resolution`, else 0).
pub fn begin_layout(imagepath: Option<&str>, shapefile_declared: bool, dpi: f64) {
    // a graph's own `imagepath` is a new string, which `gvusershape_size` sees as a new
    // path (and empties `ImageDict` for); `Gvfilepath` stays the same. (C compares the
    // pointers: an allocator that gave the next graph's string the same address would keep
    // the files, which the reference build was not seen to do.)
    let id = if imagepath.is_some() {
        NEXT_IMAGEPATH_ID.fetch_add(1, Ordering::Relaxed)
    } else {
        0
    };
    IMAGEPATH_ID.with(|p| p.set(id));
    LAYOUT_ID.with(|l| l.set(NEXT_IMAGEPATH_ID.fetch_add(1, Ordering::Relaxed)));
    let path = imagepath.map(str::to_string).or_else(filepath);
    IMAGEPATH.with(|p| *p.borrow_mut() = path);
    SHAPEFILE_DECLARED.with(|d| d.set(shapefile_declared));
    GRAPH_DPI.with(|d| d.set(dpi));
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

/// `find_user_shape`: `name` is in `UserShape`.
pub fn find_user_shape(name: &str) -> bool {
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

/// `findPath`: the first `dir/name` that can be read. C builds each name in one static
/// buffer (`safefilename`), which keeps the last one tried.
fn find_path(dirs: &[&str], name: &str) -> Option<String> {
    for d in dirs {
        let f = format!("{d}/{name}");
        FIND_PATH_BUF.with(|b| b.borrow_mut().clone_from(&f));
        if open(&f).is_ok() {
            return Some(f);
        }
    }
    None
}

/// `safefile`: the path C opens for `filename`, or `None` when it may not be read.
pub fn safefile(filename: &str) -> Option<String> {
    safefile_from(filename).map(|(path, _)| path)
}

/// [`safefile`], and whether C returns `findPath`'s static buffer (`true`) or the name it
/// was given, a string of the graph.
fn safefile_from(filename: &str) -> Option<(String, bool)> {
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
        return find_path(&mk_dirlist(&list), base).map(|p| (p, true));
    }
    let list = IMAGEPATH.with(|p| p.borrow().clone()).unwrap_or_default();
    if filename.starts_with('/') || list.is_empty() {
        Some((filename.to_string(), false))
    } else {
        find_path(&mk_dirlist(&list), filename).map(|p| (p, true))
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
    /// The stream the size readers read.
    fn into_cfile(self) -> size::CFile<BufReader<File>> {
        match self {
            Opened::File(f) => size::CFile::new(BufReader::new(f)),
            Opened::Dir => size::CFile::unreadable(),
        }
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

/// `usershape_t` as `gvusershape_open` leaves it: a file it loaded, with the type and
/// the size its header gives (`dpi` 0: the size is in pixels at the graph's resolution).
/// A name C finds as a shape library member (`find_user_shape`) has type `(lib)` and
/// size 0x0.
#[derive(Debug, Clone, PartialEq)]
pub struct UserShape {
    pub name: String,
    pub ty: ImageType,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub dpi: i32,
}

impl UserShape {
    /// `us->stringtype`.
    pub fn stringtype(&self) -> &'static str {
        self.ty.stringtype()
    }
}

thread_local! {
    /// `ImageDict`: the files `gvusershape_open` loaded, by name, with the warnings their
    /// reading gave in one of the port's quiet sizings, which the first sizing that reports
    /// prints (C reads each file once, in the sizing that reports).
    static IMAGE_DICT: RefCell<HashMap<String, (UserShape, Vec<String>)>> =
        RefCell::new(HashMap::new());
    /// `Gvimagepath` as `gvusershape_size` compares it: a new value for each graph with an
    /// `imagepath`, 0 for `Gvfilepath`.
    static IMAGEPATH_ID: Cell<u64> = const { Cell::new(0) };
    /// `gvusershape_size`'s `static char *oldpath`.
    static OLDPATH_ID: Cell<u64> = const { Cell::new(0) };
    /// The graph being laid out, for the keys of [`EPSF_CONTENTS`].
    static LAYOUT_ID: Cell<u64> = const { Cell::new(0) };
    /// `findPath`'s static buffer: the last path it tried.
    static FIND_PATH_BUF: RefCell<String> = const { RefCell::new(String::new()) };
    /// `GD_drawing(g)->dpi` of the graph being laid out.
    static GRAPH_DPI: Cell<f64> = const { Cell::new(0.0) };
}

/// The ids [`IMAGEPATH_ID`] takes.
static NEXT_IMAGEPATH_ID: AtomicU64 = AtomicU64::new(1);

/// `gvusershape_open`: the file `name` loaded, from `ImageDict` or opened and read, or
/// `None` with C's warning when it cannot be opened or is neither an image nor a shape
/// library member.
fn gvusershape_open(name: &str) -> Option<UserShape> {
    let reporting = crate::diag::reporting();
    let found = IMAGE_DICT.with(|d| {
        let mut d = d.borrow_mut();
        let (us, pending) = d.get_mut(name)?;
        let pending = if reporting {
            std::mem::take(pending)
        } else {
            Vec::new()
        };
        Some((us.clone(), pending))
    });
    if let Some((us, pending)) = found {
        for m in pending {
            agwarningf(&m);
        }
        return Some(us);
    }
    // gvusershape_file_access
    let Some(path) = safefile(name) else {
        agwarningf(&format!("Filename \"{name}\" is unsafe\n"));
        return None;
    };
    let mut f = match open(&path) {
        Ok(f) => f.into_cfile(),
        Err(e) => {
            agwarningf(&format!("{} while opening {path}\n", strerror(&e)));
            return None;
        }
    };
    let ty = size::imagetype(&mut f);
    if ty == ImageType::Null && !find_user_shape(name) {
        agwarningf(&format!(
            "\"{name}\" was not found as a file or as a shape library member\n"
        ));
        return None;
    }
    let mut warnings = Vec::new();
    let d = size::read_size(&mut f, ty, name, &mut |m| warnings.push(m.to_string()));
    if reporting {
        for m in warnings.drain(..) {
            agwarningf(&m);
        }
    }
    let us = UserShape {
        name: name.to_string(),
        ty,
        x: d.x,
        y: d.y,
        w: d.w,
        h: d.h,
        dpi: d.dpi,
    };
    IMAGE_DICT.with(|dict| {
        dict.borrow_mut()
            .insert(name.to_string(), (us.clone(), warnings))
    });
    Some(us)
}

/// `gvusershape_find`: the file `name` if `gvusershape_open` loaded it.
pub fn find(name: &str) -> Option<UserShape> {
    IMAGE_DICT.with(|d| d.borrow().get(name).map(|e| e.0.clone()))
}

/// `gvusershape_size_dpi`: the size of `us` in points, at its own resolution or at `dpi`.
pub fn size_dpi(us: &UserShape, dpi: (f64, f64)) -> (i32, i32) {
    let dpi = if us.dpi != 0 {
        (f64::from(us.dpi), f64::from(us.dpi))
    } else {
        dpi
    };
    // the conversion to int saturates on the reference platform
    ((us.w * 72.0 / dpi.0) as i32, (us.h * 72.0 / dpi.1) as i32)
}

/// `gvusershape_size`: the size in points of the file `name` at the graph's `dpi` (96
/// below 1), loading it if needed; `(-1, -1)` without a name or when it does not load.
pub fn gvusershape_size(name: Option<&str>) -> (i32, i32) {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return (-1, -1);
    };
    let path_id = IMAGEPATH_ID.with(Cell::get);
    if server_name().is_none() && OLDPATH_ID.with(Cell::get) != path_id {
        OLDPATH_ID.with(|o| o.set(path_id));
        IMAGE_DICT.with(|d| d.borrow_mut().clear());
    }
    let dpi = GRAPH_DPI.with(Cell::get);
    let dpi = if dpi >= 1.0 { dpi } else { 96.0 };
    gvusershape_open(name).map_or((-1, -1), |us| size_dpi(&us, (dpi, dpi)))
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

/// An epsf file `user_init` read: its `%%BoundingBox` and its `macro_id`, the number of
/// the PostScript procedure `user_shape_N` that `epsf_gencode` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpsfShape {
    pub bb: [i32; 4],
    pub macro_id: i32,
}

/// The key `user_init` stores a file under: the pointer `safefile` returned.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EpsfKey {
    /// `findPath`'s static buffer: its text is that of the last path it built.
    Static,
    /// A string of the graph (laid out as [`LAYOUT_ID`]), freed with it.
    Graph(u64, String),
}

/// What `epsf_init` gave a node, with the warnings not yet printed.
type EpsfInit = (Option<EpsfShape>, Vec<String>);

thread_local! {
    /// `EPSF_contents`, the most recently inserted or matched entry last (the root of C's
    /// splay tree). C never clears it, but its keys are pointers: a key that is a graph's
    /// string matches no lookup once that graph is closed, and a key that is `findPath`'s
    /// buffer has the text of the last path built there, so a lookup through the buffer
    /// matches it whatever file it names. Not modelled, as C reads freed memory there: a
    /// buffer that moved when it grew (past 24 bytes, then on reallocation), and the way
    /// freed keys steer the tree search.
    static EPSF_CONTENTS: RefCell<Vec<(EpsfKey, EpsfShape)>> = const { RefCell::new(Vec::new()) };
    /// `epsf_init` of each node of the layout: C runs it once per node; the port's repeat
    /// sizings (quiet passes, the renderers' model) get the same answer, and the first
    /// sizing that reports prints its warnings.
    static EPSF_NODES: RefCell<HashMap<(u64, String), EpsfInit>> = RefCell::new(HashMap::new());
}

/// `dtmatch(EPSF_contents, str)`: keys compare by text (`strcmp`).
fn epsf_lookup(path: &str) -> Option<EpsfShape> {
    let layout = LAYOUT_ID.with(Cell::get);
    let buf = FIND_PATH_BUF.with(|b| b.borrow().clone());
    EPSF_CONTENTS.with(|c| {
        let mut c = c.borrow_mut();
        let i = c.iter().rposition(|(k, _)| match k {
            EpsfKey::Static => buf == path,
            EpsfKey::Graph(g, p) => *g == layout && p == path,
        })?;
        let e = c.remove(i);
        let us = e.1;
        c.push(e);
        Some(us)
    })
}

/// `N_EPSF_files`, process-wide in C.
static N_EPSF_FILES: AtomicI32 = AtomicI32::new(0);

/// `epsf_init`: the `%%BoundingBox` (`lx ly ux uy`) and macro number of a `shape=epsf`
/// node's `shapefile`, or `None`, with C's warning, when there is none; the node then keeps
/// no shape information and is not drawn.
pub fn epsf_init(shapefile: Option<&str>, node: &str) -> Option<EpsfShape> {
    let key = (LAYOUT_ID.with(Cell::get), node.to_string());
    let reporting = crate::diag::reporting();
    let (us, warnings) = EPSF_NODES.with(|n| {
        let mut n = n.borrow_mut();
        let entry = n.entry(key).or_insert_with(|| user_init(shapefile, node));
        let warnings = if reporting {
            std::mem::take(&mut entry.1)
        } else {
            Vec::new()
        };
        (entry.0, warnings)
    });
    if reporting && server_name().is_some() {
        // safefile's one warning, if a quiet sizing made the first call
        let _ = shapefile.map(safefile);
    }
    for m in warnings {
        agwarningf(&m);
    }
    us
}

/// `epsf_init`'s `safefile` and `user_init`, with the warnings they give.
fn user_init(shapefile: Option<&str>, node: &str) -> EpsfInit {
    let Some((path, via_buffer)) = shapefile.and_then(safefile_from) else {
        let m = format!("shapefile not set or not found for epsf node {node}\n");
        return (None, vec![m]);
    };
    if let Some(us) = epsf_lookup(&path) {
        return (Some(us), Vec::new());
    }
    let Ok(mut f) = open(&path) else {
        return (None, vec![format!("couldn't open epsf file {path}\n")]);
    };
    let Some(bb) = bounding_box(&f.all()) else {
        let m = format!("BoundingBox not found in epsf file {path}\n");
        return (None, vec![m]);
    };
    let us = EpsfShape {
        bb,
        macro_id: N_EPSF_FILES.fetch_add(1, Ordering::Relaxed),
    };
    let key = if via_buffer {
        EpsfKey::Static
    } else {
        EpsfKey::Graph(LAYOUT_ID.with(Cell::get), path)
    };
    EPSF_CONTENTS.with(|c| c.borrow_mut().push((key, us)));
    (Some(us), Vec::new())
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
        begin_layout(None, false, 0.0);
        assert_eq!(safefile(""), None);
        assert_eq!(safefile("x.png").as_deref(), Some("x.png"));
        // imagepath: relative names must be found in one of its directories
        begin_layout(Some("/nonexistent-dir-c18"), false, 0.0);
        assert_eq!(safefile("x.png"), None);
        assert_eq!(safefile("/abs/x.png").as_deref(), Some("/abs/x.png"));
        begin_layout(Some(""), false, 0.0);
        assert_eq!(safefile("x.png").as_deref(), Some("x.png"));
        begin_layout(None, false, 0.0);
    }

    fn scratch_file(name: &str, data: &[u8]) -> String {
        let dir = std::env::temp_dir().join(format!("gvp-us347-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, data).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn sizes_are_in_points_at_the_graph_dpi() {
        let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
        png.extend(40u32.to_be_bytes());
        png.extend(20u32.to_be_bytes());
        png.extend([0; 9]);
        let png = scratch_file("a.png", &png);
        let ps = scratch_file("a.ps", b"%!PS-Adobe-3.0\n%%BoundingBox: 10 20 110 70\n");
        begin_layout(None, false, 0.0);
        // 96 dpi by default; PostScript is at 72 dpi whatever the graph's
        assert_eq!(gvusershape_size(Some(&png)), (30, 15));
        assert_eq!(gvusershape_size(Some(&ps)), (100, 50));
        assert_eq!(gvusershape_size(None), (-1, -1));
        begin_layout(None, false, 150.0);
        assert_eq!(gvusershape_size(Some(&png)), (19, 9));
        let us = find(&png).unwrap();
        assert_eq!(
            (us.stringtype(), us.w, us.h, us.dpi),
            ("png", 40.0, 20.0, 0)
        );
        assert_eq!(size_dpi(&us, (72.0, 72.0)), (40, 20));
        assert_eq!(size_dpi(&find(&ps).unwrap(), (300.0, 300.0)), (100, 50));
        // the files stay loaded from graph to graph, until a graph has its own imagepath
        begin_layout(None, false, 0.0);
        assert!(find(&png).is_some());
        begin_layout(Some(""), false, 0.0);
        assert_eq!(gvusershape_size(Some(&ps)), (100, 50));
        assert!(find(&png).is_none());
        begin_layout(None, false, 0.0);
    }

    #[test]
    fn epsf_files_are_numbered_once() {
        let ps = scratch_file("e.ps", b"%!PS-Adobe-3.0\n%%BoundingBox: 10 20 110 70\n");
        let other = scratch_file("f.ps", b"%%BoundingBox: 0 0 1 2\n");
        begin_layout(None, false, 0.0);
        let a = epsf_init(Some(&ps), "a").unwrap();
        assert_eq!(a.bb, [10, 20, 110, 70]);
        assert_eq!(epsf_init(Some(&ps), "b"), Some(a));
        let c = epsf_init(Some(&other), "c").unwrap();
        assert_ne!(c.macro_id, a.macro_id);
        // a repeat sizing of a node gets the first answer, without reading anything
        std::fs::write(&other, b"no box any more\n").unwrap();
        assert_eq!(epsf_init(Some(&other), "c"), Some(c));
        // a later graph's lookup by its own string does not find the earlier graph's
        begin_layout(None, false, 0.0);
        let a2 = epsf_init(Some(&ps), "a").unwrap();
        assert_ne!(a2.macro_id, a.macro_id);
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
