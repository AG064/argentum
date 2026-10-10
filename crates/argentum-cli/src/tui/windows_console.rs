use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::HANDLE;

// Match crossterm's Windows control-key translation, which uses the foreground
// thread's keyboard layout. These two stable user32 imports avoid introducing
// another input reader or changing the user's keyboard layout.
#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> windows_sys::Win32::Foundation::HWND;
    fn GetWindowThreadProcessId(
        window: windows_sys::Win32::Foundation::HWND,
        process: *mut u32,
    ) -> u32;
}

pub(super) fn control_key_matches(character: char, letter: char) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, VkKeyScanExW};
    let Ok(character) = u16::try_from(character as u32) else {
        return false;
    };
    let scan = unsafe {
        let window = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(window, std::ptr::null_mut());
        VkKeyScanExW(character, GetKeyboardLayout(thread))
    };
    // Shift is fine; do not reinterpret characters which themselves require
    // Ctrl/Alt (AltGr) as a paste or cancellation gesture.
    scan != -1
        && (scan as u16 & 0xff) == letter.to_ascii_uppercase() as u16
        && ((scan as u16 >> 8) & 6) == 0
}

// VT/clipboard bytes are not a physical owner gesture. Only the high (down)
// bit is used, not the racy "pressed since last query" bit.
pub(super) fn access_confirmation_down() -> bool {
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x77) < 0 }
}
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetConsoleScreenBufferInfo, SetConsoleMode, CONSOLE_SCREEN_BUFFER_INFO,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
};

fn mode(handle: HANDLE) -> io::Result<u32> {
    let mut value = 0;
    // The handle is borrowed from a live File or the process standard stream.
    if unsafe { GetConsoleMode(handle, &mut value) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(value)
}

fn set_mode(handle: HANDLE, value: u32) -> io::Result<()> {
    if unsafe { SetConsoleMode(handle, value) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) struct Modes {
    input: File,
    input_mode: u32,
    output_mode: u32,
}

impl Modes {
    pub fn capture() -> io::Result<Self> {
        // This is the same console input buffer opened by crossterm 0.28.1.
        let input = OpenOptions::new().read(true).write(true).open("CONIN$")?;
        let input_mode = mode(input.as_raw_handle())?;
        let output_mode = mode(io::stdout().as_raw_handle())?;
        Ok(Self {
            input,
            input_mode,
            output_mode,
        })
    }

    pub fn enable_output(&self) -> io::Result<()> {
        let handle = io::stdout().as_raw_handle();
        let required = ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        set_mode(handle, mode(handle)? | required)?;
        if mode(handle)? & required != required {
            return Err(io::Error::other(
                "terminal output mode did not enable VT processing",
            ));
        }
        Ok(())
    }

    pub fn restore(&self) -> io::Result<()> {
        let input = set_mode(self.input.as_raw_handle(), self.input_mode);
        let output = set_mode(io::stdout().as_raw_handle(), self.output_mode);
        input.and(output)
    }
}

pub(super) fn snapshot() -> serde_json::Value {
    let input = OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
        .ok();
    let active = OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONOUT$")
        .ok();
    let output = io::stdout().as_raw_handle();
    let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
    let info_ok = unsafe { GetConsoleScreenBufferInfo(output, &mut info) } != 0;
    serde_json::json!({
        "stdin_mode": mode(io::stdin().as_raw_handle()).ok(),
        "reader_CONIN_mode": input.as_ref().and_then(|f| mode(f.as_raw_handle()).ok()),
        "stdout_mode": mode(output).ok(),
        "active_CONOUT_mode": active.as_ref().and_then(|f| mode(f.as_raw_handle()).ok()),
        "stdout_handle": format!("{output:p}"),
        "reader_api": "crossterm 0.28.1 ReadConsoleInputW(CONIN$)",
        "cursor": info_ok.then_some([info.dwCursorPosition.X, info.dwCursorPosition.Y]),
        "viewport": info_ok.then_some([info.srWindow.Left, info.srWindow.Top, info.srWindow.Right, info.srWindow.Bottom]),
        "buffer_size": info_ok.then_some([info.dwSize.X, info.dwSize.Y]),
    })
}
