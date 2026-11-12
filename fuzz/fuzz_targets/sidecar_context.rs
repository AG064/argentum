// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Feeds arbitrary text to the parser for the local model's context size,
//! which reads a value reported by a separate process.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = argentum_providers::parse_sidecar_context(text);
    }
});
