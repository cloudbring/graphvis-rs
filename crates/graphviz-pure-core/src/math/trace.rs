//! `math-trace` feature: appends each distinct argument tuple to
//! `$GV_MATH_TRACE/<function>.bin` (native only; used to measure libm differences).

use std::collections::HashSet;
use std::io::Write;
use std::sync::Mutex;

const NAMES: [&str; 12] = [
    "hypot", "pow", "atan2", "sin", "cos", "tan", "asin", "acos", "exp", "log", "log2", "cbrt",
];

type Seen = Vec<(HashSet<(u64, u64)>, Option<std::fs::File>)>;
static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

pub fn log(name: u8, x: f64, y: f64) {
    static DIR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(dir) = DIR.get_or_init(|| std::env::var("GV_MATH_TRACE").ok()) else {
        return;
    };
    let mut g = SEEN.lock().unwrap();
    let seen = g.get_or_insert_with(|| NAMES.iter().map(|_| Default::default()).collect());
    let (set, file) = &mut seen[name as usize];
    if set.len() < 4_000_000 && set.insert((x.to_bits(), y.to_bits())) {
        let f = file.get_or_insert_with(|| {
            let path = format!("{dir}/{}.bin", NAMES[name as usize]);
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap()
        });
        let mut rec = [0u8; 16];
        rec[..8].copy_from_slice(&x.to_le_bytes());
        rec[8..].copy_from_slice(&y.to_le_bytes());
        f.write_all(&rec).unwrap();
    }
}
