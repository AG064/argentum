//! Modal hidden input, used before the chat input reader starts. No tracing or
//! terminal echo receives secret text, and every exit restores console modes.
use super::*;
use argentum_security::SecretValue;
use zeroize::{Zeroize, Zeroizing};

const LIMIT: usize = 16 * 1024;

#[cfg(windows)]
struct SecretEvents {
    input: std::fs::File,
    surrogate: Option<u16>,
    repeated: Option<(KeyEvent, u16)>,
    right_down: bool,
}

#[cfg(windows)]
impl SecretEvents {
    fn new() -> io::Result<Self> {
        Ok(Self {
            input: std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("CONIN$")?,
            surrogate: None,
            repeated: None,
            right_down: false,
        })
    }
    fn read(&mut self) -> io::Result<Event> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Console::*;
        if let Some((key, count)) = self.repeated.take() {
            if count > 1 {
                self.repeated = Some((key, count - 1));
            }
            return Ok(Event::Key(key));
        }
        loop {
            let mut record: INPUT_RECORD = unsafe { std::mem::zeroed() };
            let mut count = 0;
            if unsafe { ReadConsoleInputW(self.input.as_raw_handle(), &mut record, 1, &mut count) }
                == 0
            {
                return Err(io::Error::last_os_error());
            }
            if count != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Terminal input closed.",
                ));
            }
            if u32::from(record.EventType) == MOUSE_EVENT {
                let mouse = unsafe { record.Event.MouseEvent };
                let down = mouse.dwButtonState & RIGHTMOST_BUTTON_PRESSED != 0;
                let paste = down && !self.right_down;
                self.right_down = down;
                if paste {
                    return Ok(Event::Mouse(event::MouseEvent {
                        kind: event::MouseEventKind::Down(event::MouseButton::Right),
                        column: 0,
                        row: 0,
                        modifiers: KeyModifiers::NONE,
                    }));
                }
                continue;
            }
            if u32::from(record.EventType) != KEY_EVENT {
                continue;
            }
            let native = unsafe { record.Event.KeyEvent };
            let unit = unsafe { native.uChar.UnicodeChar };
            // Preserve Alt-numpad text, which Windows supplies on Alt release.
            if native.bKeyDown == 0 && !(native.wVirtualKeyCode == 0x12 && unit != 0) {
                continue;
            }
            let mut modifiers = KeyModifiers::NONE;
            if native.dwControlKeyState & SHIFT_PRESSED != 0 {
                modifiers |= KeyModifiers::SHIFT;
            }
            if native.dwControlKeyState & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 {
                modifiers |= KeyModifiers::CONTROL;
            }
            if native.dwControlKeyState & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0 {
                modifiers |= KeyModifiers::ALT;
            }
            // Use the original VK for shortcuts. Do not round-trip it through
            // the foreground application's possibly different keyboard layout.
            let code = match native.wVirtualKeyCode {
                vk @ (0x43 | 0x44 | 0x55 | 0x56)
                    if modifiers.contains(KeyModifiers::CONTROL)
                        && !modifiers.contains(KeyModifiers::ALT) =>
                {
                    KeyCode::Char((vk as u8 as char).to_ascii_lowercase())
                }
                0x1b => KeyCode::Esc,
                0x08 => KeyCode::Backspace,
                0x0d => KeyCode::Enter,
                0x2d => KeyCode::Insert,
                _ => {
                    if (0xd800..=0xdbff).contains(&unit) {
                        self.surrogate = Some(unit);
                        continue;
                    }
                    let ch = if (0xdc00..=0xdfff).contains(&unit) {
                        let Some(high) = self.surrogate.take() else {
                            continue;
                        };
                        let Some(Ok(ch)) = std::char::decode_utf16([high, unit]).next() else {
                            continue;
                        };
                        ch
                    } else {
                        self.surrogate = None;
                        let Some(ch) = char::from_u32(u32::from(unit)).filter(|c| *c != '\0')
                        else {
                            continue;
                        };
                        ch
                    };
                    KeyCode::Char(ch)
                }
            };
            let key = KeyEvent::new(code, modifiers);
            if matches!(code, KeyCode::Char(_))
                && !modifiers.contains(KeyModifiers::CONTROL)
                && native.wRepeatCount > 1
            {
                self.repeated =
                    Some((key, native.wRepeatCount.saturating_sub(1).min(LIMIT as u16)));
            }
            return Ok(Event::Key(key));
        }
    }
}

#[cfg(not(windows))]
struct SecretEvents;
#[cfg(not(windows))]
impl SecretEvents {
    fn new() -> io::Result<Self> {
        Ok(Self)
    }
    fn read(&mut self) -> io::Result<Event> {
        event::read()
    }
}

#[derive(Default)]
pub(super) struct SecretInput {
    text: Zeroizing<String>,
    invalid: bool,
}

impl SecretInput {
    pub(super) fn mask(&self) -> String {
        if self.invalid {
            return "Invalid input; Ctrl+U clears, Esc cancels".into();
        }
        let count = self.text.chars().count();
        if count == 0 {
            "Key: (empty)".into()
        } else {
            format!("Key: {} ({count} characters)", "*".repeat(count.min(32)))
        }
    }
    fn append(&mut self, text: &str) {
        if text.chars().any(char::is_control) || self.text.len() + text.len() > LIMIT {
            self.text.zeroize();
            self.invalid = true;
        } else if !self.invalid {
            self.text.push_str(text);
        }
    }

    pub(super) fn event(&mut self, event: Event) -> Option<io::Result<SecretValue>> {
        match event {
            Event::Paste(text) => {
                let text = Zeroizing::new(text);
                self.append(text.trim_end_matches(['\r', '\n']));
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                _ if is_control_key(key, 'c') || is_control_key(key, 'd') => {
                    return Some(Err(cancelled()))
                }
                _ if is_control_key(key, 'u') => {
                    self.text.zeroize();
                    self.invalid = false;
                }
                KeyCode::Esc => return Some(Err(cancelled())),
                KeyCode::Backspace => {
                    self.text.pop();
                }
                KeyCode::Enter if key.kind == KeyEventKind::Press => {
                    return Some(if self.invalid || self.text.trim().is_empty() {
                        Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Key must be one nonempty line, at most 16 KiB. Nothing was saved.",
                        ))
                    } else {
                        Ok(SecretValue::new(self.text.trim()))
                    });
                }
                KeyCode::Char(ch)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        || key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    let mut bytes = Zeroizing::new([0u8; 4]);
                    self.append(ch.encode_utf8(&mut *bytes));
                }
                _ => {}
            },
            _ => {}
        }
        None
    }
}

fn cancelled() -> io::Error {
    io::Error::new(
        io::ErrorKind::Interrupted,
        "Key entry cancelled. Nothing was saved.",
    )
}

struct SecretGuard {
    restored: bool,
    #[cfg(windows)]
    modes: windows_console::Modes,
}

impl SecretGuard {
    fn enter() -> io::Result<Self> {
        if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
            return Err(io::Error::other(
                "Hidden key entry requires an interactive terminal.",
            ));
        }
        let guard = Self {
            restored: false,
            #[cfg(windows)]
            modes: windows_console::Modes::capture()?,
        };
        // Construct the guard first so partial setup failures restore modes too.
        terminal::enable_raw_mode()?;
        #[cfg(windows)]
        guard.modes.enable_output()?;
        execute!(
            io::stdout(),
            EnableBracketedPaste,
            event::EnableMouseCapture
        )?;
        Ok(guard)
    }

    fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let paste = execute!(
            io::stdout(),
            event::DisableMouseCapture,
            DisableBracketedPaste
        );
        let raw = terminal::disable_raw_mode();
        #[cfg(windows)]
        let modes = self.modes.restore();
        #[cfg(not(windows))]
        let modes: io::Result<()> = Ok(());
        self.restored = true;
        paste.and(raw).and(modes)
    }
}

impl Drop for SecretGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub fn prompt_provider_secret(profile_id: &str) -> io::Result<SecretValue> {
    prompt_with_clipboard(profile_id, read_clipboard)
}

fn paste_requested(event: &Event) -> bool {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            is_control_key(*key, 'v')
                || (key.code == KeyCode::Insert && key.modifiers.contains(KeyModifiers::SHIFT))
        }
        Event::Mouse(mouse) => mouse.kind == event::MouseEventKind::Down(event::MouseButton::Right),
        _ => false,
    }
}

#[cfg(not(windows))]
pub(super) fn read_clipboard() -> io::Result<Zeroizing<String>> {
    Err(io::Error::other("Use your terminal's paste action."))
}

#[cfg(windows)]
pub(super) fn read_clipboard() -> io::Result<Zeroizing<String>> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    // CF_UNICODETEXT from WinUser.h. Read only on an explicit paste gesture.
    const UNICODE_TEXT: u32 = 13;
    if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
        return Err(io::Error::other("Clipboard is busy. Try paste again."));
    }
    struct Clipboard;
    impl Drop for Clipboard {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    let _clipboard = Clipboard;
    let handle = unsafe { GetClipboardData(UNICODE_TEXT) };
    if handle.is_null() {
        return Err(io::Error::other("Clipboard has no text."));
    }
    let size = unsafe { GlobalSize(handle) };
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        return Err(io::Error::other("Clipboard text is unavailable."));
    }
    struct Locked(windows_sys::Win32::Foundation::HGLOBAL);
    impl Drop for Locked {
        fn drop(&mut self) {
            unsafe {
                GlobalUnlock(self.0);
            }
        }
    }
    let _locked = Locked(handle);
    // Never scan beyond the allocation or the bounded input size. The OS owns
    // this allocation: do not free, overwrite or clear the user's clipboard.
    let units =
        unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), (size / 2).min(LIMIT + 1)) };
    let length = units
        .iter()
        .position(|c| *c == 0)
        .ok_or_else(|| io::Error::other("Clipboard text exceeds the key limit."))?;
    String::from_utf16(&units[..length])
        .map(Zeroizing::new)
        .map_err(|_| io::Error::other("Clipboard text is invalid."))
}

fn prompt_with_clipboard(
    profile_id: &str,
    mut clipboard: impl FnMut() -> io::Result<Zeroizing<String>>,
) -> io::Result<SecretValue> {
    if !argentum_domain::is_supported_hosted_credential_provider_id(profile_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported credential provider.",
        ));
    }
    let mut guard = SecretGuard::enter()?;
    let result = (|| {
        let mut out = io::stdout();
        write!(
            out,
            "API key for {profile_id} (masked). Enter saves; Esc cancels.\r\nCtrl+V, Shift+Insert or right-click pastes; Ctrl+U clears.\r\n"
        )?;
        out.flush()?;
        let mut input = SecretInput::default();
        let mut events = SecretEvents::new()?;
        let mut previous = String::new();
        loop {
            let mask = input.mask();
            if mask != previous {
                execute!(
                    out,
                    crossterm::cursor::MoveToColumn(0),
                    terminal::Clear(terminal::ClearType::CurrentLine),
                    crossterm::style::Print(&mask)
                )?;
                out.flush()?;
                previous = mask;
            }
            let event = events.read()?;
            if paste_requested(&event) {
                match clipboard() {
                    Ok(text) => input.append(text.trim_end_matches(['\r', '\n'])),
                    Err(_) => {
                        execute!(
                            out,
                            crossterm::cursor::MoveToColumn(0),
                            terminal::Clear(terminal::ClearType::CurrentLine),
                            crossterm::style::Print(
                                "Paste unavailable; type the key or try paste again."
                            )
                        )?;
                        out.flush()?;
                    }
                }
                continue;
            }
            if let Some(result) = input.event(event) {
                return result;
            }
        }
    })();
    let restored = guard.restore();
    writeln!(io::stdout())?;
    restored?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn control_case_raw_codes_and_altgr_do_not_break_secret_input() {
        for (ch, modifiers) in [
            ('u', KeyModifiers::CONTROL),
            ('U', KeyModifiers::CONTROL),
            ('\u{15}', KeyModifiers::NONE),
        ] {
            let mut input = SecretInput::default();
            input.append("synthetic");
            input.event(Event::Key(KeyEvent::new(KeyCode::Char(ch), modifiers)));
            assert!(input.text.is_empty());
        }
        for ch in ['c', 'C', 'd', 'D'] {
            assert!(SecretInput::default()
                .event(Event::Key(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::CONTROL
                )))
                .unwrap()
                .is_err());
        }
        assert!(paste_requested(&Event::Key(KeyEvent::new(
            KeyCode::Char('\u{16}'),
            KeyModifiers::NONE
        ))));
        assert!(!paste_requested(&Event::Key(KeyEvent::new(
            KeyCode::Char('v'),
            KeyModifiers::CONTROL | KeyModifiers::ALT
        ))));
        let mut input = SecretInput::default();
        input.event(Event::Key(KeyEvent::new(
            KeyCode::Char('@'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        )));
        assert!(input.text.as_str() == "@");
    }

    #[test]
    #[ignore = "explicit terminal masked input and synthetic clipboard routing"]
    fn native_masked_clipboard_routes() {
        let mut reads = 0;
        let secret = prompt_with_clipboard("minimax", || {
            reads += 1;
            Ok(Zeroizing::new("SYNTHETIC-CLIPBOARD-NOT-REAL".into()))
        })
        .unwrap();
        assert!(
            secret.expose() == "SYNTHETIC-CLIPBOARD-NOT-REAL",
            "synthetic input mismatch; value withheld"
        );
        assert!(
            reads >= 2,
            "exercise Ctrl+V and Shift+Insert, clearing between pastes"
        );
        println!("Masked clipboard dispatch and cancellation-safe terminal restoration passed; clipboard data was synthetic.");
    }

    #[test]
    fn hidden_key_accepts_single_paste_edits_and_ignores_releases() {
        let mut input = SecretInput::default();
        assert!(input
            .event(Event::Paste("synthetic-not-a-real-key-X".into()))
            .is_none());
        input.event(key(KeyCode::Backspace));
        input.event(key(KeyCode::Char('Y')));
        assert!(input.mask().contains("***"));
        assert!(!input.mask().contains("synthetic"));
        input.event(Event::Key(KeyEvent::new_with_kind(
            KeyCode::Enter,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        )));
        let key = input.event(key(KeyCode::Enter)).unwrap().unwrap();
        assert_eq!(key.expose(), "synthetic-not-a-real-key-Y");
        assert!(!format!("{key:?}").contains("synthetic"));
    }

    #[test]
    fn explicit_paste_gestures_do_not_include_release_or_pasted_text() {
        assert!(paste_requested(&Event::Key(KeyEvent::new(
            KeyCode::Char('v'),
            KeyModifiers::CONTROL
        ))));
        assert!(paste_requested(&Event::Key(KeyEvent::new(
            KeyCode::Insert,
            KeyModifiers::SHIFT
        ))));
        assert!(paste_requested(&Event::Mouse(event::MouseEvent {
            kind: event::MouseEventKind::Down(event::MouseButton::Right),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE
        })));
        assert!(!paste_requested(&Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('v'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release
        ))));
        assert!(!paste_requested(&Event::Paste("\u{16}".into())));
    }

    #[test]
    fn hidden_key_rejects_controls_overflow_and_cancel_without_a_secret() {
        for text in [
            String::new(),
            "first\nsecond".into(),
            "secret\x1b[2J".into(),
            "x".repeat(LIMIT + 1),
        ] {
            let mut input = SecretInput::default();
            input.event(Event::Paste(text));
            assert!(input.event(key(KeyCode::Enter)).unwrap().is_err());
        }
        for event in [
            key(KeyCode::Esc),
            Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        ] {
            let mut input = SecretInput::default();
            input.append("synthetic-not-saved");
            assert_eq!(
                input.event(event).unwrap().unwrap_err().kind(),
                io::ErrorKind::Interrupted
            );
        }
    }
}
