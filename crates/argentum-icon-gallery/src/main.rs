// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

use slint::ComponentHandle;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    IconGallery::new()?.run()
}
