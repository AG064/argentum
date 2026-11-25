// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

pub mod insets;
pub mod jni_bridge;

#[cfg(target_os = "android")]
mod host;

#[cfg(target_os = "android")]
mod shared_storage;

#[cfg(all(target_os = "android", feature = "android-inference-test-hook"))]
mod inference_test_hook;

// Debug builds only: the release APK has no test bridge.
#[cfg(all(target_os = "android", debug_assertions))]
mod ui_test_bridge;

#[cfg(target_os = "android")]
pub use host::android_main;
