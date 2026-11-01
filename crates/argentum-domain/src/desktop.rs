//! Data-only identities for the optional first-party desktop-control tools.
//!
//! These types carry no platform authority. A caller-supplied identity is an
//! observation that must be revalidated against the live desktop immediately
//! before any effect. The Windows implementation lives behind the optional
//! `desktop-control` feature and rejects changed, elevated, cloaked,
//! minimized, unfocused or otherwise ineligible targets.

use serde::{Deserialize, Serialize};

pub const MAX_DESKTOP_WINDOWS: u16 = 64;
pub const MAX_DESKTOP_ELEMENTS: u16 = 256;
pub const MAX_DESKTOP_DEPTH: u8 = 8;
pub const MAX_DESKTOP_TEXT_UTF16: usize = 1024;
pub const MAX_DESKTOP_KEYS: u8 = 8;
pub const MAX_DESKTOP_KEY_CHORD_CHARS: usize = 32;
pub const MAX_DESKTOP_NAME_CHARS: usize = 256;
pub const MAX_DESKTOP_CAPTURE_PIXELS: u64 = 8_000_000;
/// Bound on the encoded PNG. Base64 expansion still fits the 64 KiB
/// model-visible tool-result limit with room for metadata.
pub const MAX_DESKTOP_CAPTURE_BYTES: usize = 32 * 1024;
pub const MAX_DESKTOP_INSPECTION_BYTES: usize = 64 * 1024;

/// Screen-pixel window or element rectangle as reported to this process.
/// Negative coordinates are valid; DPI virtualization may apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRect {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

impl DesktopRect {
    pub fn pixel_count(self) -> Option<u64> {
        if self.width <= 0 || self.height <= 0 {
            return None;
        }
        u64::try_from(self.width)
            .ok()?
            .checked_mul(u64::try_from(self.height).ok()?)
    }
}

/// Fresh identity of one top-level window as observed by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopWindowIdentity {
    /// HWND rendered as lowercase hexadecimal (`0x...`) so it survives JSON.
    pub hwnd: String,
    pub process_id: u32,
    pub title: String,
    pub class_name: String,
    pub bounds: DesktopRect,
}

impl DesktopWindowIdentity {
    pub fn hwnd_value(&self) -> Option<isize> {
        let value = self.hwnd.strip_prefix("0x")?;
        isize::from_str_radix(value, 16)
            .ok()
            .filter(|value| *value != 0)
    }
}

/// Fresh identity of one UI Automation element inside an observed window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopElementIdentity {
    /// Child-index path from the window element root. Empty means the root.
    pub tree_path: Vec<u32>,
    pub automation_id: String,
    pub control_type: i32,
    pub class_name: String,
    pub name: String,
    pub bounds: DesktopRect,
}

impl DesktopElementIdentity {
    pub fn path_is_bounded(&self) -> bool {
        self.tree_path.len() <= usize::from(MAX_DESKTOP_DEPTH)
            && self
                .tree_path
                .iter()
                .all(|index| *index < u32::from(MAX_DESKTOP_ELEMENTS))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopTextMode {
    /// Write through the element's UI Automation ValuePattern. No global input.
    #[default]
    Value,
    /// Send bounded Unicode keyboard input to the already focused element.
    Keys,
}
