// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Feeds arbitrary bytes to the JSONL server's request parser. Other programs
//! send these lines to `argentum serve`, so the parser must reject any input
//! without panicking.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = argentum_cli::protocol::parse_request(data);
});
