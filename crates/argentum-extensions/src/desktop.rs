//! Optional first-party Windows desktop control.
//!
//! This module is compiled only for Windows builds that explicitly enable the
//! `desktop-control` feature. It is a bounded UI Automation foundation, not a
//! general computer-use sandbox:
//!
//! * No process launch, file access, clipboard access, network access or
//!   credential read is implemented.
//! * Pointer input, focus stealing and screen-region capture are deliberately
//!   absent. Activation is semantic (UI Automation invoke/select) and key
//!   input is limited to a closed vocabulary sent to an already focused,
//!   already foreground target.
//! * Every effect revalidates the caller's window and element identity against
//!   the live desktop immediately before the primitive, rejects secure,
//!   elevated, cross-session, cloaked, minimized, unresponsive, disabled,
//!   offscreen, password and unfocused targets, and observes cancellation
//!   before each primitive.
//!
//! A successful COM call is an API return, not proof that the application
//! changed state. Callers must reobserve after every effect.

use std::{
    ffi::c_void,
    mem::size_of,
    ptr::null_mut,
    time::{SystemTime, UNIX_EPOCH},
};

use argentum_domain::desktop::{
    DesktopElementIdentity, DesktopRect, DesktopTextMode, DesktopWindowIdentity,
    MAX_DESKTOP_CAPTURE_BYTES, MAX_DESKTOP_CAPTURE_PIXELS, MAX_DESKTOP_DEPTH, MAX_DESKTOP_ELEMENTS,
    MAX_DESKTOP_INSPECTION_BYTES, MAX_DESKTOP_KEYS, MAX_DESKTOP_NAME_CHARS, MAX_DESKTOP_TEXT_UTF16,
    MAX_DESKTOP_WINDOWS,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use windows::core::{Interface, BOOL, BSTR};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, RECT, RPC_E_CHANGED_MODE, WPARAM,
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_CONTROL_FLAGS,
    DESKTOP_READOBJECTS, UOI_NAME,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, IUIAutomationTreeWalker, IUIAutomationValuePattern,
    UIA_InvokePatternId, UIA_SelectionItemPatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE,
    VK_HOME, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB,
    VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, SendMessageTimeoutW, GA_ROOT, PW_RENDERFULLCONTENT, SMTO_ABORTIFHUNG,
    SMTO_BLOCK, WM_NULL,
};

pub const CAPABILITY_ID: &str = "desktop.control";

/// Compiled only under the explicit Windows feature, so the real first-party
/// path exists whenever this module exists.
pub const fn available() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopError {
    Unavailable,
    Cancelled,
    Unsupported,
    InvalidInput(&'static str),
    SecureDesktop,
    CrossSession,
    ElevatedTarget,
    TargetUnavailable,
    TargetChanged,
    TargetNotForeground,
    TargetMinimized,
    TargetCloaked,
    TargetNotResponding,
    ElementUnavailable,
    ElementChanged,
    ElementDisabled,
    ElementOffscreen,
    ElementPassword,
    ElementReadOnly,
    ElementNotFocused,
    PatternUnsupported,
    InputRejected,
    CaptureTooLarge,
    CaptureFailed,
    ComFailure,
}

impl std::fmt::Display for DesktopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::Unavailable => "desktop control is unavailable in this build",
            Self::Cancelled => "desktop action was cancelled before its next primitive",
            Self::Unsupported => "the requested desktop action is not supported",
            Self::InvalidInput(reason) => reason,
            Self::SecureDesktop => "the input desktop is not the interactive Default desktop",
            Self::CrossSession => "the target window belongs to another Windows session",
            Self::ElevatedTarget => "the target is elevated and this process is not",
            Self::TargetUnavailable => "the target process or window cannot be inspected",
            Self::TargetChanged => "the target window identity changed",
            Self::TargetNotForeground => "the target window is not the foreground window",
            Self::TargetMinimized => "the target window is minimized",
            Self::TargetCloaked => "the target window is cloaked",
            Self::TargetNotResponding => "the target window is not responding",
            Self::ElementUnavailable => "the target element cannot be inspected",
            Self::ElementChanged => "the target element identity changed",
            Self::ElementDisabled => "the target element is disabled",
            Self::ElementOffscreen => "the target element is offscreen",
            Self::ElementPassword => "password surfaces are not addressable",
            Self::ElementReadOnly => "the target element is read-only",
            Self::ElementNotFocused => "the target element does not have keyboard focus",
            Self::PatternUnsupported => "the target element does not support a safe pattern",
            Self::InputRejected => "the operating system rejected the bounded input",
            Self::CaptureTooLarge => "the capture exceeds the bounded pixel or byte limit",
            Self::CaptureFailed => "the bounded window capture failed",
            Self::ComFailure => "the desktop automation subsystem is unavailable",
        };
        formatter.write_str(text)
    }
}

impl std::error::Error for DesktopError {}

#[derive(Debug, Clone, Serialize)]
pub struct WindowSummary {
    #[serde(flatten)]
    pub identity: DesktopWindowIdentity,
    pub visible: bool,
    pub responding: bool,
    pub minimized: bool,
    pub cloaked: bool,
    pub elevated: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ElementInfo {
    #[serde(flatten)]
    pub identity: DesktopElementIdentity,
    pub control_type_name: &'static str,
    pub depth: u32,
    pub enabled: bool,
    pub keyboard_focusable: bool,
    pub focused: bool,
    pub offscreen: bool,
    pub password: bool,
    pub patterns: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Inspection {
    pub window: DesktopWindowIdentity,
    pub captured_at_unix_seconds: u64,
    pub generation: String,
    pub elements: Vec<ElementInfo>,
    pub truncated: bool,
    pub coordinate_space: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionReceipt {
    pub action: &'static str,
    pub pattern: &'static str,
    pub window: DesktopWindowIdentity,
    pub element: DesktopElementIdentity,
    pub at_unix_seconds: u64,
    pub api_return_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextReceipt {
    pub action: &'static str,
    pub mode: &'static str,
    pub characters: usize,
    pub primitives: u32,
    pub window: DesktopWindowIdentity,
    pub element: DesktopElementIdentity,
    pub at_unix_seconds: u64,
    pub api_return_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyReceipt {
    pub action: &'static str,
    pub chords: usize,
    pub primitives: u32,
    pub window: DesktopWindowIdentity,
    pub element: DesktopElementIdentity,
    pub at_unix_seconds: u64,
    pub api_return_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Capture {
    pub window: DesktopWindowIdentity,
    pub width: u32,
    pub height: u32,
    pub png_bytes: usize,
    pub png_sha256: String,
    pub png_base64: String,
    pub at_unix_seconds: u64,
    pub print_window_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowList {
    pub windows: Vec<WindowSummary>,
    pub truncated: bool,
    pub window_limit: u16,
}

pub fn list_windows(
    max_windows: u16,
    cancel: &CancellationToken,
) -> Result<WindowList, DesktopError> {
    with_com(|| {
        if max_windows == 0 || max_windows > MAX_DESKTOP_WINDOWS {
            return Err(DesktopError::InvalidInput(
                "window limit must be between 1 and 64",
            ));
        }
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        if !secure_desktop_ok() {
            return Err(DesktopError::SecureDesktop);
        }
        let mut enumeration = Enumeration {
            windows: Vec::new(),
            limit: usize::from(max_windows),
            truncated: false,
            cancel,
        };
        let pointer = &mut enumeration as *mut Enumeration;
        let _ = unsafe { EnumWindows(Some(enumerate_window), LPARAM(pointer as isize)) };
        if enumeration.cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let mut list = WindowList {
            windows: enumeration.windows,
            truncated: enumeration.truncated,
            window_limit: max_windows,
        };
        bound_window_list(&mut list);
        Ok(list)
    })
}

fn bound_window_list(list: &mut WindowList) {
    while list.windows.len() > 1
        && serde_json::to_vec(list)
            .map(|bytes| bytes.len() > MAX_DESKTOP_INSPECTION_BYTES)
            .unwrap_or(true)
    {
        list.windows.pop();
        list.truncated = true;
    }
}

pub fn inspect(
    window: &DesktopWindowIdentity,
    max_depth: u8,
    max_elements: u16,
    cancel: &CancellationToken,
) -> Result<Inspection, DesktopError> {
    with_com(|| {
        if max_depth == 0 || max_depth > MAX_DESKTOP_DEPTH {
            return Err(DesktopError::InvalidInput("depth must be between 1 and 8"));
        }
        if max_elements == 0 || max_elements > MAX_DESKTOP_ELEMENTS {
            return Err(DesktopError::InvalidInput(
                "element limit must be between 1 and 256",
            ));
        }
        let hwnd = validate_window(window, false)?;
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let uia = automation()?;
        let root =
            unsafe { uia.ElementFromHandle(hwnd) }.map_err(|_| DesktopError::ElementUnavailable)?;
        let walker =
            unsafe { uia.ControlViewWalker() }.map_err(|_| DesktopError::ElementUnavailable)?;
        let mut walk = Walk {
            walker,
            cancel,
            max_depth: u32::from(max_depth),
            max_elements: usize::from(max_elements),
            process_id: window.process_id,
            elements: Vec::new(),
            truncated: false,
        };
        let root_snapshot = element_snapshot(&root)?;
        if u32::try_from(root_snapshot.process_id).ok() != Some(window.process_id) {
            return Err(DesktopError::ElementUnavailable);
        }
        walk.push(root_snapshot, 0, Vec::new());
        walk.children(&root, 1, Vec::new())?;
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let mut inspection = Inspection {
            window: window.clone(),
            captured_at_unix_seconds: unix_seconds(),
            generation: String::new(),
            elements: walk.elements,
            truncated: walk.truncated,
            coordinate_space:
                "screen pixels reported to this process; DPI virtualization may apply",
        };
        bound_inspection(&mut inspection);
        let payload = serde_json::to_vec(&(&inspection.window, &inspection.elements))
            .map_err(|_| DesktopError::ElementUnavailable)?;
        inspection.generation = format!("{:x}", Sha256::digest(payload));
        Ok(inspection)
    })
}

pub fn invoke(
    window: &DesktopWindowIdentity,
    element: &DesktopElementIdentity,
    cancel: &CancellationToken,
) -> Result<ActionReceipt, DesktopError> {
    with_com(|| {
        let hwnd = validate_window(window, false)?;
        let uia = automation()?;
        let target = resolve_element(&uia, hwnd, window, element, cancel)?;
        let snapshot = element_snapshot(&target)?;
        require_actionable(&snapshot)?;
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let pattern = if let Ok(pattern) =
            unsafe { target.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
        {
            if cancel.is_cancelled() {
                return Err(DesktopError::Cancelled);
            }
            revalidate_window(window)?;
            revalidate_element(&uia, hwnd, window, element, cancel)?;
            unsafe { pattern.Invoke() }.map_err(|_| DesktopError::InputRejected)?;
            "InvokePattern"
        } else if let Ok(pattern) = unsafe {
            target.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                UIA_SelectionItemPatternId,
            )
        } {
            if cancel.is_cancelled() {
                return Err(DesktopError::Cancelled);
            }
            revalidate_window(window)?;
            revalidate_element(&uia, hwnd, window, element, cancel)?;
            unsafe { pattern.Select() }.map_err(|_| DesktopError::InputRejected)?;
            "SelectionItemPattern.Select"
        } else {
            return Err(DesktopError::PatternUnsupported);
        };
        Ok(ActionReceipt {
            action: "invoke",
            pattern,
            window: window.clone(),
            element: element.clone(),
            at_unix_seconds: unix_seconds(),
            api_return_only: true,
        })
    })
}

pub fn type_text(
    window: &DesktopWindowIdentity,
    element: &DesktopElementIdentity,
    text: &str,
    mode: DesktopTextMode,
    cancel: &CancellationToken,
) -> Result<TextReceipt, DesktopError> {
    if text.len() > MAX_DESKTOP_TEXT_UTF16 * 4 {
        return Err(DesktopError::InvalidInput(
            "text exceeds 1024 UTF-16 code units",
        ));
    }
    if text.encode_utf16().count() > MAX_DESKTOP_TEXT_UTF16 {
        return Err(DesktopError::InvalidInput(
            "text exceeds 1024 UTF-16 code units",
        ));
    }
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
    {
        return Err(DesktopError::InvalidInput(
            "text contains unsupported control characters",
        ));
    }
    with_com(|| {
        let hwnd = validate_window(window, mode == DesktopTextMode::Keys)?;
        let uia = automation()?;
        let target = resolve_element(&uia, hwnd, window, element, cancel)?;
        let snapshot = element_snapshot(&target)?;
        require_actionable(&snapshot)?;
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let (mode_name, primitives) = match mode {
            DesktopTextMode::Value => {
                let pattern = unsafe {
                    target.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                }
                .map_err(|_| DesktopError::PatternUnsupported)?;
                if unsafe { pattern.CurrentIsReadOnly() }
                    .map(|value| value.as_bool())
                    .unwrap_or(true)
                {
                    return Err(DesktopError::ElementReadOnly);
                }
                if cancel.is_cancelled() {
                    return Err(DesktopError::Cancelled);
                }
                revalidate_window(window)?;
                revalidate_element(&uia, hwnd, window, element, cancel)?;
                unsafe { pattern.SetValue(&BSTR::from(text)) }
                    .map_err(|_| DesktopError::InputRejected)?;
                ("value", 1)
            }
            DesktopTextMode::Keys => {
                if !snapshot.focused {
                    return Err(DesktopError::ElementNotFocused);
                }
                if !is_foreground(hwnd) {
                    return Err(DesktopError::TargetNotForeground);
                }
                let mut primitives = 0u32;
                for (index, unit) in text.encode_utf16().enumerate() {
                    if cancel.is_cancelled() {
                        return Err(DesktopError::Cancelled);
                    }
                    if index % 16 == 0 && !secure_desktop_ok() {
                        return Err(DesktopError::SecureDesktop);
                    }
                    if !is_foreground(hwnd) || window_process(hwnd) != window.process_id {
                        return Err(DesktopError::TargetChanged);
                    }
                    let events = if unit == u16::from(b'\n') {
                        press_key_events(VK_RETURN)
                    } else if unit == u16::from(b'\t') {
                        press_key_events(VK_TAB)
                    } else {
                        unicode_events(unit)
                    };
                    primitives = primitives.saturating_add(send_events(&events)?);
                }
                ("keys", primitives)
            }
        };
        Ok(TextReceipt {
            action: "type_text",
            mode: mode_name,
            characters: text.chars().count(),
            primitives,
            window: window.clone(),
            element: element.clone(),
            at_unix_seconds: unix_seconds(),
            api_return_only: true,
        })
    })
}

pub fn press_keys(
    window: &DesktopWindowIdentity,
    element: &DesktopElementIdentity,
    chords: &[String],
    cancel: &CancellationToken,
) -> Result<KeyReceipt, DesktopError> {
    if chords.is_empty() || chords.len() > usize::from(MAX_DESKTOP_KEYS) {
        return Err(DesktopError::InvalidInput(
            "key count must be between 1 and 8",
        ));
    }
    let parsed = chords
        .iter()
        .map(|chord| parse_chord(chord))
        .collect::<Result<Vec<_>, _>>()?;
    with_com(|| {
        let hwnd = validate_window(window, true)?;
        let uia = automation()?;
        let target = resolve_element(&uia, hwnd, window, element, cancel)?;
        let snapshot = element_snapshot(&target)?;
        require_actionable(&snapshot)?;
        if !snapshot.focused {
            return Err(DesktopError::ElementNotFocused);
        }
        if !is_foreground(hwnd) {
            return Err(DesktopError::TargetNotForeground);
        }
        let mut primitives = 0u32;
        for chord in &parsed {
            if cancel.is_cancelled() {
                return Err(DesktopError::Cancelled);
            }
            if !secure_desktop_ok() {
                return Err(DesktopError::SecureDesktop);
            }
            if !is_foreground(hwnd) || window_process(hwnd) != window.process_id {
                return Err(DesktopError::TargetChanged);
            }
            primitives = primitives.saturating_add(send_events(&chord_events(chord))?);
        }
        Ok(KeyReceipt {
            action: "press_keys",
            chords: parsed.len(),
            primitives,
            window: window.clone(),
            element: element.clone(),
            at_unix_seconds: unix_seconds(),
            api_return_only: true,
        })
    })
}

pub fn capture(
    window: &DesktopWindowIdentity,
    cancel: &CancellationToken,
) -> Result<Capture, DesktopError> {
    with_com(|| {
        let hwnd = validate_window(window, false)?;
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        if window_contains_password(hwnd, cancel)? {
            return Err(DesktopError::ElementPassword);
        }
        revalidate_window(window)?;
        let rect = window_rect(hwnd)?;
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let width = u32::try_from(rect.width).map_err(|_| DesktopError::InvalidInput("width"))?;
        let height =
            u32::try_from(rect.height).map_err(|_| DesktopError::InvalidInput("height"))?;
        if width == 0 || height == 0 {
            return Err(DesktopError::CaptureFailed);
        }
        if u64::from(width).saturating_mul(u64::from(height)) > MAX_DESKTOP_CAPTURE_PIXELS {
            return Err(DesktopError::CaptureTooLarge);
        }
        let (rgba, captured_width, captured_height) = print_window(hwnd, width, height)?;
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let mut rgba = rgba;
        let mut captured_width = captured_width;
        let mut captured_height = captured_height;
        let mut png = Vec::new();
        for _ in 0..4 {
            png = encode_png(captured_width, captured_height, &rgba)?;
            if png.len() <= MAX_DESKTOP_CAPTURE_BYTES {
                break;
            }
            if captured_width <= 16 || captured_height <= 16 {
                break;
            }
            (rgba, captured_width, captured_height) =
                downscale(captured_width, captured_height, &rgba);
        }
        if png.len() > MAX_DESKTOP_CAPTURE_BYTES {
            return Err(DesktopError::CaptureTooLarge);
        }
        Ok(Capture {
            window: window.clone(),
            width: captured_width,
            height: captured_height,
            png_bytes: png.len(),
            png_sha256: format!("{:x}", Sha256::digest(&png)),
            png_base64: base64(&png),
            at_unix_seconds: unix_seconds(),
            print_window_only: true,
        })
    })
}

/// Parse a bounded chord such as `Ctrl+Shift+S`. The vocabulary is closed:
/// no Windows key, no global shell shortcuts, no bare modifier chord.
pub fn parse_chord(chord: &str) -> Result<KeyChord, DesktopError> {
    if chord.is_empty() || chord.len() > argentum_domain::desktop::MAX_DESKTOP_KEY_CHORD_CHARS {
        return Err(DesktopError::InvalidInput(
            "key chord must be between 1 and 32 bytes",
        ));
    }
    if !chord.is_ascii() {
        return Err(DesktopError::InvalidInput("key chord must be ASCII"));
    }
    let mut control = false;
    let mut shift = false;
    let mut alt = false;
    let mut key = None;
    let parts = chord.split('+').collect::<Vec<_>>();
    if parts.len() > 4 {
        return Err(DesktopError::InvalidInput(
            "key chord accepts at most three modifiers and one key",
        ));
    }
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" if key.is_none() && !control => control = true,
            "shift" if key.is_none() && !shift => shift = true,
            "alt" if key.is_none() && !alt => alt = true,
            "win" | "lwin" | "rwin" | "meta" | "super" => {
                return Err(DesktopError::InvalidInput(
                    "the Windows key is not available",
                ))
            }
            _ if key.is_none() => key = Some(parse_key(part)?),
            _ => {
                return Err(DesktopError::InvalidInput(
                    "key chord must contain modifiers followed by one key",
                ))
            }
        }
    }
    let key = key.ok_or(DesktopError::InvalidInput(
        "key chord must contain exactly one non-modifier key",
    ))?;
    if control && alt && matches!(key, VirtualKey::Delete) {
        return Err(DesktopError::InvalidInput(
            "the secure attention sequence is not available",
        ));
    }
    if alt && matches!(key, VirtualKey::Tab | VirtualKey::Escape) {
        return Err(DesktopError::InvalidInput(
            "global window switching is not available",
        ));
    }
    if control && matches!(key, VirtualKey::Escape) {
        return Err(DesktopError::InvalidInput(
            "global shell shortcuts are not available",
        ));
    }
    if alt && matches!(key, VirtualKey::F(4)) {
        return Err(DesktopError::InvalidInput(
            "window close shortcuts are not available",
        ));
    }
    let clipboard_key = matches!(key, VirtualKey::Character(0x43 | 0x58 | 0x56));
    if (control && clipboard_key) || (shift && matches!(key, VirtualKey::Delete)) {
        return Err(DesktopError::InvalidInput(
            "clipboard shortcuts are not available",
        ));
    }
    Ok(KeyChord {
        control,
        shift,
        alt,
        key,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChord {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: VirtualKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualKey {
    Character(u16),
    Digit(u16),
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

impl VirtualKey {
    fn code(self) -> VIRTUAL_KEY {
        match self {
            Self::Character(value) | Self::Digit(value) => VIRTUAL_KEY(value),
            Self::Enter => VK_RETURN,
            Self::Tab => VK_TAB,
            Self::Escape => VK_ESCAPE,
            Self::Backspace => VK_BACK,
            Self::Delete => VK_DELETE,
            Self::Space => VK_SPACE,
            Self::Left => VK_LEFT,
            Self::Right => VK_RIGHT,
            Self::Up => VK_UP,
            Self::Down => VK_DOWN,
            Self::Home => VK_HOME,
            Self::End => VK_END,
            Self::PageUp => VK_PRIOR,
            Self::PageDown => VK_NEXT,
            Self::F(number) => VIRTUAL_KEY(111 + u16::from(number)),
        }
    }
}

fn parse_key(key: &str) -> Result<VirtualKey, DesktopError> {
    if key.len() == 1 {
        let character = key.as_bytes()[0];
        if character.is_ascii_alphabetic() {
            return Ok(VirtualKey::Character(u16::from(
                character.to_ascii_uppercase(),
            )));
        }
        if character.is_ascii_digit() {
            return Ok(VirtualKey::Digit(u16::from(character)));
        }
        return Err(DesktopError::InvalidInput(
            "only letters, digits and named navigation keys are available",
        ));
    }
    match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => Ok(VirtualKey::Enter),
        "tab" => Ok(VirtualKey::Tab),
        "escape" | "esc" => Ok(VirtualKey::Escape),
        "backspace" => Ok(VirtualKey::Backspace),
        "delete" | "del" => Ok(VirtualKey::Delete),
        "space" => Ok(VirtualKey::Space),
        "left" => Ok(VirtualKey::Left),
        "right" => Ok(VirtualKey::Right),
        "up" => Ok(VirtualKey::Up),
        "down" => Ok(VirtualKey::Down),
        "home" => Ok(VirtualKey::Home),
        "end" => Ok(VirtualKey::End),
        "pageup" | "pgup" => Ok(VirtualKey::PageUp),
        "pagedown" | "pgdn" => Ok(VirtualKey::PageDown),
        value if value.starts_with('f') && value.len() <= 3 => value[1..]
            .parse::<u8>()
            .ok()
            .filter(|number| (1..=12).contains(number))
            .map(VirtualKey::F)
            .ok_or(DesktopError::InvalidInput(
                "function keys are F1 through F12",
            )),
        _ => Err(DesktopError::InvalidInput("unsupported key name in chord")),
    }
}

struct Enumeration<'a> {
    windows: Vec<WindowSummary>,
    limit: usize,
    truncated: bool,
    cancel: &'a CancellationToken,
}

unsafe extern "system" fn enumerate_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = unsafe { &mut *(lparam.0 as *mut Enumeration<'_>) };
    if state.cancel.is_cancelled() {
        return BOOL(0);
    }
    if state.windows.len() >= state.limit {
        state.truncated = true;
        return BOOL(0);
    }
    if let Ok(Some(summary)) = summarize_window(hwnd) {
        state.windows.push(summary);
    }
    BOOL(1)
}

fn summarize_window(hwnd: HWND) -> Result<Option<WindowSummary>, DesktopError> {
    let identity = window_identity(hwnd)?;
    if identity.title.is_empty() && !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return Ok(None);
    }
    let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
    let minimized = unsafe { IsIconic(hwnd) }.as_bool();
    let cloaked = window_is_cloaked(hwnd);
    let responding = window_is_responding(hwnd);
    let elevated = process_elevation(identity.process_id).ok();
    Ok(Some(WindowSummary {
        identity,
        visible,
        responding,
        minimized,
        cloaked,
        elevated,
    }))
}

fn validate_window(
    identity: &DesktopWindowIdentity,
    require_foreground: bool,
) -> Result<HWND, DesktopError> {
    let hwnd = window_handle(identity)?;
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err(DesktopError::TargetChanged);
    }
    if window_process(hwnd) != identity.process_id {
        return Err(DesktopError::TargetChanged);
    }
    let current = window_identity(hwnd)?;
    if current.title != identity.title
        || current.class_name != identity.class_name
        || current.bounds != identity.bounds
    {
        return Err(DesktopError::TargetChanged);
    }
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return Err(DesktopError::TargetUnavailable);
    }
    if unsafe { IsIconic(hwnd) }.as_bool() {
        return Err(DesktopError::TargetMinimized);
    }
    if window_is_cloaked(hwnd) {
        return Err(DesktopError::TargetCloaked);
    }
    if !window_is_responding(hwnd) {
        return Err(DesktopError::TargetNotResponding);
    }
    if !secure_desktop_ok() {
        return Err(DesktopError::SecureDesktop);
    }
    if !same_session(identity.process_id) {
        return Err(DesktopError::CrossSession);
    }
    let elevated = process_elevation(identity.process_id)?;
    if elevated && !current_process_elevated()? {
        return Err(DesktopError::ElevatedTarget);
    }
    if require_foreground && !is_foreground(hwnd) {
        return Err(DesktopError::TargetNotForeground);
    }
    Ok(hwnd)
}

fn revalidate_window(identity: &DesktopWindowIdentity) -> Result<(), DesktopError> {
    let hwnd = window_handle(identity)?;
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool()
        || window_process(hwnd) != identity.process_id
        || window_identity(hwnd)? != *identity
    {
        return Err(DesktopError::TargetChanged);
    }
    if !window_is_responding(hwnd) {
        return Err(DesktopError::TargetNotResponding);
    }
    if !secure_desktop_ok() {
        return Err(DesktopError::SecureDesktop);
    }
    Ok(())
}

fn revalidate_element(
    uia: &IUIAutomation,
    hwnd: HWND,
    window: &DesktopWindowIdentity,
    element: &DesktopElementIdentity,
    cancel: &CancellationToken,
) -> Result<(), DesktopError> {
    let target = resolve_element(uia, hwnd, window, element, cancel)?;
    let snapshot = element_snapshot(&target)?;
    require_actionable(&snapshot)
}

fn window_handle(identity: &DesktopWindowIdentity) -> Result<HWND, DesktopError> {
    let value = identity
        .hwnd_value()
        .ok_or(DesktopError::InvalidInput("window hwnd is invalid"))?;
    Ok(HWND(value as *mut c_void))
}

fn window_identity(hwnd: HWND) -> Result<DesktopWindowIdentity, DesktopError> {
    let title = window_text(hwnd)?;
    let class_name = window_class(hwnd)?;
    let rect = window_rect(hwnd)?;
    let process_id = window_process(hwnd);
    if process_id == 0 {
        return Err(DesktopError::TargetUnavailable);
    }
    Ok(DesktopWindowIdentity {
        hwnd: format!("0x{:x}", hwnd.0 as isize),
        process_id,
        title,
        class_name,
        bounds: rect,
    })
}

fn window_text(hwnd: HWND) -> Result<String, DesktopError> {
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length < 0 {
        return Err(DesktopError::TargetUnavailable);
    }
    let capacity = usize::try_from(length)
        .unwrap_or(0)
        .saturating_add(1)
        .min(MAX_DESKTOP_NAME_CHARS.saturating_add(1));
    let mut buffer = vec![0u16; capacity.max(1)];
    let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    let written = usize::try_from(written.max(0))
        .unwrap_or(0)
        .min(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..written]))
}

fn window_class(hwnd: HWND) -> Result<String, DesktopError> {
    let mut buffer = vec![0u16; MAX_DESKTOP_NAME_CHARS.saturating_add(1)];
    let written = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let written = usize::try_from(written.max(0))
        .unwrap_or(0)
        .min(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..written]))
}

fn window_rect(hwnd: HWND) -> Result<DesktopRect, DesktopError> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(|_| DesktopError::TargetUnavailable)?;
    Ok(DesktopRect {
        left: rect.left,
        top: rect.top,
        width: rect.right.saturating_sub(rect.left),
        height: rect.bottom.saturating_sub(rect.top),
    })
}

fn window_process(hwnd: HWND) -> u32 {
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    process_id
}

fn window_is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            size_of::<u32>() as u32,
        )
    };
    result.is_err() || cloaked != 0
}

fn window_is_responding(hwnd: HWND) -> bool {
    let result = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            100,
            None,
        )
    };
    result.0 != 0
}

fn is_foreground(hwnd: HWND) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground == hwnd {
        return true;
    }
    !foreground.is_invalid() && unsafe { GetAncestor(foreground, GA_ROOT) } == hwnd
}

fn secure_desktop_ok() -> bool {
    let Ok(desktop) =
        (unsafe { OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS) })
    else {
        return false;
    };
    let mut buffer = [0u16; 128];
    let mut needed = 0u32;
    let result = unsafe {
        GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(buffer.as_mut_ptr() as *mut c_void),
            (buffer.len() * size_of::<u16>()) as u32,
            Some(&mut needed),
        )
    };
    let _ = unsafe { CloseDesktop(desktop) };
    if result.is_err() {
        return false;
    }
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length]).eq_ignore_ascii_case("Default")
}

fn same_session(process_id: u32) -> bool {
    let mut target = 0u32;
    let mut current = 0u32;
    let target_ok = unsafe { ProcessIdToSessionId(process_id, &mut target) }.is_ok();
    let current_ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut current) }.is_ok();
    target_ok && current_ok && target == current
}

fn process_elevation(process_id: u32) -> Result<bool, DesktopError> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|_| DesktopError::TargetUnavailable)?;
    let result = token_elevation(process);
    let _ = unsafe { CloseHandle(process) };
    result
}

fn current_process_elevated() -> Result<bool, DesktopError> {
    token_elevation(unsafe { GetCurrentProcess() }).map_err(|_| DesktopError::Unavailable)
}

fn token_elevation(process: HANDLE) -> Result<bool, DesktopError> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }
        .map_err(|_| DesktopError::TargetUnavailable)?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut length = 0u32;
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(token) };
    result.map_err(|_| DesktopError::TargetUnavailable)?;
    Ok(elevation.TokenIsElevated != 0)
}

fn with_com<T>(operation: impl FnOnce() -> Result<T, DesktopError>) -> Result<T, DesktopError> {
    let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let initialized = result.is_ok();
    if result.is_err() && result != RPC_E_CHANGED_MODE {
        return Err(DesktopError::ComFailure);
    }
    let output = operation();
    if initialized {
        unsafe { CoUninitialize() };
    }
    output
}

fn automation() -> Result<IUIAutomation, DesktopError> {
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|_| DesktopError::ComFailure)?;
    if let Ok(automation2) = automation.cast::<IUIAutomation2>() {
        let _ = unsafe { automation2.SetConnectionTimeout(2_000) };
        let _ = unsafe { automation2.SetTransactionTimeout(5_000) };
    }
    Ok(automation)
}

struct Walk<'a> {
    walker: IUIAutomationTreeWalker,
    cancel: &'a CancellationToken,
    max_depth: u32,
    max_elements: usize,
    process_id: u32,
    elements: Vec<ElementInfo>,
    truncated: bool,
}

impl Walk<'_> {
    fn push(&mut self, snapshot: ElementSnapshot, depth: u32, path: Vec<u32>) {
        if self.elements.len() >= self.max_elements {
            self.truncated = true;
            return;
        }
        let control_type_name = control_type_name(snapshot.control_type);
        let patterns = snapshot.patterns.clone();
        self.elements.push(ElementInfo {
            identity: DesktopElementIdentity {
                tree_path: path,
                automation_id: snapshot.automation_id,
                control_type: snapshot.control_type,
                class_name: snapshot.class_name,
                name: snapshot.name,
                bounds: snapshot.bounds,
            },
            control_type_name,
            enabled: snapshot.enabled,
            keyboard_focusable: snapshot.keyboard_focusable,
            focused: snapshot.focused,
            offscreen: snapshot.offscreen,
            password: snapshot.password,
            patterns,
            depth,
        });
    }

    fn children(
        &mut self,
        element: &IUIAutomationElement,
        depth: u32,
        path: Vec<u32>,
    ) -> Result<(), DesktopError> {
        if depth > self.max_depth {
            self.truncated = true;
            return Ok(());
        }
        let mut child = match unsafe { self.walker.GetFirstChildElement(element) } {
            Ok(child) => child,
            Err(_) => return Ok(()),
        };
        let mut index = 0u32;
        loop {
            if self.cancel.is_cancelled() {
                return Err(DesktopError::Cancelled);
            }
            if self.elements.len() >= self.max_elements {
                self.truncated = true;
                return Ok(());
            }
            if let Ok(snapshot) = element_snapshot(&child) {
                if u32::try_from(snapshot.process_id).ok() == Some(self.process_id) {
                    let mut child_path = path.clone();
                    child_path.push(index);
                    let descend = !snapshot.password;
                    self.push(snapshot, depth, child_path.clone());
                    if descend {
                        self.children(&child, depth.saturating_add(1), child_path)?;
                    }
                }
            }
            child = match unsafe { self.walker.GetNextSiblingElement(&child) } {
                Ok(sibling) => sibling,
                Err(_) => return Ok(()),
            };
            index = index.saturating_add(1);
            if index >= u32::from(MAX_DESKTOP_ELEMENTS) {
                self.truncated = true;
                return Ok(());
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ElementSnapshot {
    process_id: i32,
    automation_id: String,
    control_type: i32,
    class_name: String,
    name: String,
    bounds: DesktopRect,
    enabled: bool,
    keyboard_focusable: bool,
    focused: bool,
    offscreen: bool,
    password: bool,
    patterns: Vec<&'static str>,
}

fn element_snapshot(element: &IUIAutomationElement) -> Result<ElementSnapshot, DesktopError> {
    let name = unsafe { element.CurrentName() }
        .map_err(|_| DesktopError::ElementUnavailable)?
        .to_string();
    let automation_id = unsafe { element.CurrentAutomationId() }
        .map_err(|_| DesktopError::ElementUnavailable)?
        .to_string();
    let class_name = unsafe { element.CurrentClassName() }
        .map_err(|_| DesktopError::ElementUnavailable)?
        .to_string();
    let control_type = unsafe { element.CurrentControlType() }
        .map_err(|_| DesktopError::ElementUnavailable)?
        .0;
    let rect = unsafe { element.CurrentBoundingRectangle() }
        .map_err(|_| DesktopError::ElementUnavailable)?;
    let bounds = DesktopRect {
        left: rect.left,
        top: rect.top,
        width: rect.right.saturating_sub(rect.left),
        height: rect.bottom.saturating_sub(rect.top),
    };
    let password = unsafe { element.CurrentIsPassword() }
        .map(|value| value.as_bool())
        .unwrap_or(true);
    let process_id = unsafe { element.CurrentProcessId() }.unwrap_or(-1);
    let enabled = unsafe { element.CurrentIsEnabled() }
        .map(|value| value.as_bool())
        .unwrap_or(false);
    let keyboard_focusable = unsafe { element.CurrentIsKeyboardFocusable() }
        .map(|value| value.as_bool())
        .unwrap_or(false);
    let focused = unsafe { element.CurrentHasKeyboardFocus() }
        .map(|value| value.as_bool())
        .unwrap_or(false);
    let offscreen = unsafe { element.CurrentIsOffscreen() }
        .map(|value| value.as_bool())
        .unwrap_or(true);
    let mut patterns = Vec::new();
    if unsafe { element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) }
        .is_ok()
    {
        patterns.push("InvokePattern");
    }
    if unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .is_ok()
    {
        patterns.push("ValuePattern");
    }
    if unsafe {
        element.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
    }
    .is_ok()
    {
        patterns.push("SelectionItemPattern");
    }
    Ok(ElementSnapshot {
        process_id,
        automation_id,
        control_type,
        class_name,
        name: if password {
            String::new()
        } else {
            truncate(&name, MAX_DESKTOP_NAME_CHARS)
        },
        bounds,
        enabled,
        keyboard_focusable,
        focused,
        offscreen,
        password,
        patterns,
    })
}

fn window_contains_password(hwnd: HWND, cancel: &CancellationToken) -> Result<bool, DesktopError> {
    let uia = automation()?;
    let root =
        unsafe { uia.ElementFromHandle(hwnd) }.map_err(|_| DesktopError::ElementUnavailable)?;
    let walker =
        unsafe { uia.ControlViewWalker() }.map_err(|_| DesktopError::ElementUnavailable)?;
    let mut remaining = usize::from(MAX_DESKTOP_ELEMENTS);
    contains_password_element(&walker, &root, 0, &mut remaining, cancel)
}

fn contains_password_element(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    depth: u32,
    remaining: &mut usize,
    cancel: &CancellationToken,
) -> Result<bool, DesktopError> {
    if *remaining == 0 || depth > u32::from(MAX_DESKTOP_DEPTH) {
        return Ok(false);
    }
    if cancel.is_cancelled() {
        return Err(DesktopError::Cancelled);
    }
    *remaining = (*remaining).saturating_sub(1);
    if unsafe { element.CurrentIsPassword() }
        .map(|value| value.as_bool())
        .unwrap_or(true)
    {
        return Ok(true);
    }
    let mut child = match unsafe { walker.GetFirstChildElement(element) } {
        Ok(child) => child,
        Err(_) => return Ok(false),
    };
    loop {
        if *remaining == 0 {
            return Ok(false);
        }
        if contains_password_element(walker, &child, depth.saturating_add(1), remaining, cancel)? {
            return Ok(true);
        }
        child = match unsafe { walker.GetNextSiblingElement(&child) } {
            Ok(sibling) => sibling,
            Err(_) => return Ok(false),
        };
    }
}

fn resolve_element(
    uia: &IUIAutomation,
    hwnd: HWND,
    window: &DesktopWindowIdentity,
    identity: &DesktopElementIdentity,
    cancel: &CancellationToken,
) -> Result<IUIAutomationElement, DesktopError> {
    if !identity.path_is_bounded() {
        return Err(DesktopError::InvalidInput("element path is not bounded"));
    }
    let root =
        unsafe { uia.ElementFromHandle(hwnd) }.map_err(|_| DesktopError::ElementUnavailable)?;
    let walker =
        unsafe { uia.ControlViewWalker() }.map_err(|_| DesktopError::ElementUnavailable)?;
    let mut current = root;
    for index in &identity.tree_path {
        if cancel.is_cancelled() {
            return Err(DesktopError::Cancelled);
        }
        let mut child = unsafe { walker.GetFirstChildElement(&current) }
            .map_err(|_| DesktopError::ElementChanged)?;
        for _ in 0..*index {
            child = unsafe { walker.GetNextSiblingElement(&child) }
                .map_err(|_| DesktopError::ElementChanged)?;
        }
        current = child;
    }
    let snapshot = element_snapshot(&current)?;
    if u32::try_from(snapshot.process_id).ok() != Some(window.process_id)
        || snapshot.automation_id != identity.automation_id
        || snapshot.control_type != identity.control_type
        || snapshot.class_name != identity.class_name
        || snapshot.name != identity.name
        || snapshot.bounds != identity.bounds
    {
        return Err(DesktopError::ElementChanged);
    }
    let native = unsafe { current.CurrentNativeWindowHandle() }.unwrap_or_default();
    if !native.is_invalid() {
        let root_of_native = unsafe { GetAncestor(native, GA_ROOT) };
        if root_of_native != hwnd {
            return Err(DesktopError::ElementChanged);
        }
    }
    Ok(current)
}

fn require_actionable(snapshot: &ElementSnapshot) -> Result<(), DesktopError> {
    if snapshot.password {
        return Err(DesktopError::ElementPassword);
    }
    if !snapshot.enabled {
        return Err(DesktopError::ElementDisabled);
    }
    if snapshot.offscreen {
        return Err(DesktopError::ElementOffscreen);
    }
    Ok(())
}

fn bound_inspection(inspection: &mut Inspection) {
    let limit = MAX_DESKTOP_INSPECTION_BYTES.saturating_sub(4096);
    while inspection.elements.len() > 1
        && serde_json::to_vec(&(&inspection.window, &inspection.elements))
            .map(|bytes| bytes.len() > limit)
            .unwrap_or(true)
    {
        inspection.elements.pop();
        inspection.truncated = true;
    }
}

fn truncate(value: &str, maximum: usize) -> String {
    value.chars().take(maximum).collect()
}

fn control_type_name(control_type: i32) -> &'static str {
    match control_type {
        50000 => "ControlType.Button",
        50001 => "ControlType.Calendar",
        50002 => "ControlType.CheckBox",
        50003 => "ControlType.ComboBox",
        50004 => "ControlType.Edit",
        50005 => "ControlType.Hyperlink",
        50006 => "ControlType.Image",
        50007 => "ControlType.ListItem",
        50008 => "ControlType.List",
        50009 => "ControlType.Menu",
        50010 => "ControlType.MenuBar",
        50011 => "ControlType.MenuItem",
        50012 => "ControlType.ProgressBar",
        50013 => "ControlType.RadioButton",
        50014 => "ControlType.ScrollBar",
        50015 => "ControlType.Slider",
        50016 => "ControlType.Spinner",
        50017 => "ControlType.StatusBar",
        50018 => "ControlType.Tab",
        50019 => "ControlType.TabItem",
        50020 => "ControlType.Text",
        50021 => "ControlType.ToolBar",
        50022 => "ControlType.ToolTip",
        50023 => "ControlType.Tree",
        50024 => "ControlType.TreeItem",
        50025 => "ControlType.Custom",
        50026 => "ControlType.Group",
        50027 => "ControlType.Thumb",
        50028 => "ControlType.DataGrid",
        50029 => "ControlType.DataItem",
        50030 => "ControlType.Document",
        50031 => "ControlType.SplitButton",
        50032 => "ControlType.Window",
        50033 => "ControlType.Pane",
        50034 => "ControlType.Header",
        50035 => "ControlType.HeaderItem",
        50036 => "ControlType.Table",
        50037 => "ControlType.TitleBar",
        50038 => "ControlType.Separator",
        _ => "ControlType.Unknown",
    }
}

fn press_key_events(key: VIRTUAL_KEY) -> Vec<INPUT> {
    vec![
        keyboard_event(key, KEYBD_EVENT_FLAGS(0)),
        keyboard_event(key, KEYEVENTF_KEYUP),
    ]
}

fn unicode_events(unit: u16) -> Vec<INPUT> {
    vec![
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ]
}

fn chord_events(chord: &KeyChord) -> Vec<INPUT> {
    let mut events = Vec::with_capacity(8);
    if chord.control {
        events.push(keyboard_event(VK_CONTROL, KEYBD_EVENT_FLAGS(0)));
    }
    if chord.shift {
        events.push(keyboard_event(VK_SHIFT, KEYBD_EVENT_FLAGS(0)));
    }
    if chord.alt {
        events.push(keyboard_event(VK_MENU, KEYBD_EVENT_FLAGS(0)));
    }
    events.push(keyboard_event(chord.key.code(), KEYBD_EVENT_FLAGS(0)));
    events.push(keyboard_event(chord.key.code(), KEYEVENTF_KEYUP));
    if chord.alt {
        events.push(keyboard_event(VK_MENU, KEYEVENTF_KEYUP));
    }
    if chord.shift {
        events.push(keyboard_event(VK_SHIFT, KEYEVENTF_KEYUP));
    }
    if chord.control {
        events.push(keyboard_event(VK_CONTROL, KEYEVENTF_KEYUP));
    }
    events
}

fn keyboard_event(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_events(events: &[INPUT]) -> Result<u32, DesktopError> {
    let inserted = unsafe { SendInput(events, size_of::<INPUT>() as i32) };
    if usize::try_from(inserted).unwrap_or(0) != events.len() {
        return Err(DesktopError::InputRejected);
    }
    Ok(inserted)
}

fn print_window(hwnd: HWND, width: u32, height: u32) -> Result<(Vec<u8>, u32, u32), DesktopError> {
    let screen: HDC = unsafe { GetDC(None) };
    if screen.is_invalid() {
        return Err(DesktopError::CaptureFailed);
    }
    let memory: HDC = unsafe { CreateCompatibleDC(Some(screen)) };
    if memory.is_invalid() {
        let _ = unsafe { ReleaseDC(None, screen) };
        return Err(DesktopError::CaptureFailed);
    }
    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = i32::try_from(width).map_err(|_| DesktopError::CaptureTooLarge)?;
    info.bmiHeader.biHeight = -i32::try_from(height).map_err(|_| DesktopError::CaptureTooLarge)?;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB.0;
    let mut bits: *mut c_void = null_mut();
    let bitmap: HBITMAP = match unsafe {
        CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
    } {
        Ok(bitmap) if !bitmap.is_invalid() && !bits.is_null() => bitmap,
        _ => {
            let _ = unsafe { DeleteDC(memory) };
            let _ = unsafe { ReleaseDC(None, screen) };
            return Err(DesktopError::CaptureFailed);
        }
    };
    let previous = unsafe { SelectObject(memory, HGDIOBJ(bitmap.0)) };
    if previous.is_invalid() {
        let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
        let _ = unsafe { DeleteDC(memory) };
        let _ = unsafe { ReleaseDC(None, screen) };
        return Err(DesktopError::CaptureFailed);
    }
    let printed = unsafe { PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)) };
    let result = if printed.as_bool() {
        let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| DesktopError::CaptureTooLarge)?;
        let source = unsafe { std::slice::from_raw_parts(bits as *const u8, pixel_count * 4) };
        let mut rgba = Vec::with_capacity(pixel_count * 4);
        let (pixels, _) = source.as_chunks::<4>();
        for pixel in pixels {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
        Ok((rgba, width, height))
    } else {
        Err(DesktopError::CaptureFailed)
    };
    let _ = unsafe { SelectObject(memory, previous) };
    let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
    let _ = unsafe { DeleteDC(memory) };
    let _ = unsafe { ReleaseDC(None, screen) };
    result
}

fn downscale(width: u32, height: u32, rgba: &[u8]) -> (Vec<u8>, u32, u32) {
    let target_width = (width / 2).max(1);
    let target_height = (height / 2).max(1);
    let mut output = Vec::with_capacity((target_width * target_height * 4) as usize);
    for y in 0..target_height {
        for x in 0..target_width {
            let mut channels = [0u32; 4];
            let mut samples = 0u32;
            for dy in 0..2 {
                for dx in 0..2 {
                    let source_x = x * 2 + dx;
                    let source_y = y * 2 + dy;
                    if source_x >= width || source_y >= height {
                        continue;
                    }
                    let index = ((source_y * width + source_x) * 4) as usize;
                    for channel in 0..4 {
                        channels[channel] += u32::from(rgba[index + channel]);
                    }
                    samples += 1;
                }
            }
            for channel in channels {
                output.push((channel / samples.max(1)) as u8);
            }
        }
    }
    (output, target_width, target_height)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, DesktopError> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| DesktopError::CaptureFailed)?;
        writer
            .write_image_data(rgba)
            .map_err(|_| DesktopError::CaptureFailed)?;
        writer.finish().map_err(|_| DesktopError::CaptureFailed)?;
    }
    Ok(output)
}

fn base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(third & 0b0011_1111) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_chords_are_closed_and_bounded() {
        assert!(parse_chord("Ctrl+S").is_ok());
        assert!(parse_chord("ctrl+shift+z").is_ok());
        assert!(parse_chord("Enter").is_ok());
        assert!(parse_chord("F12").is_ok());
        assert!(parse_chord("Win+R").is_err());
        assert!(parse_chord("Alt+Tab").is_err());
        assert!(parse_chord("Alt+F4").is_err());
        assert!(parse_chord("Ctrl+Alt+Delete").is_err());
        assert!(parse_chord("Ctrl+Shift+Escape").is_err());
        assert!(parse_chord("Shift").is_err());
        assert!(parse_chord("Ctrl+S+Enter").is_err());
        assert!(parse_chord("Ctrl+\u{1F600}").is_err());
    }

    #[test]
    fn base64_is_bounded_and_padded() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }
}
