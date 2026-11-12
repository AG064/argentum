//! Feeds arbitrary text to the parser for the local model's context size,
//! which reads a value reported by a separate process.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = argentum_providers::parse_sidecar_context(text);
    }
});
