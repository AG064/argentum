pub mod insets;
pub mod jni_bridge;

#[cfg(target_os = "android")]
mod host;

#[cfg(all(target_os = "android", feature = "android-inference-test-hook"))]
mod inference_test_hook;

#[cfg(target_os = "android")]
pub use host::android_main;
