#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| graphviz_pure_fuzz::parse(data));
