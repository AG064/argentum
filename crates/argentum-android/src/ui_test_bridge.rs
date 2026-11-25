// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! A test bridge for the emulator tests. It exists only in debug builds, never
//! in the release APK, and stays off unless the test creates
//! `files/ui-test/enabled` in the app's own private storage.
//!
//! When on, it writes what is on screen to `files/ui-test/state.json` and reads
//! one line at a time from `files/ui-test/commands`. A command calls the same
//! UI callback a button calls, so a test can enter a server address or send a
//! message without tapping at screen coordinates. Keys such as Back and Ctrl+,
//! still come from Android's real input path.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use argentum_ui::UiHandle;
use slint::{ComponentHandle, Model};

const POLL: std::time::Duration = std::time::Duration::from_millis(250);
const MAX_COMMAND_BYTES: usize = 4 * 1024;

pub(crate) struct UiTestBridge {
    _timer: slint::Timer,
}

/// Starts the bridge when the test enabled it. Returns None otherwise.
pub(crate) fn start(ui: &UiHandle, files_dir: Option<PathBuf>) -> Option<UiTestBridge> {
    let directory = files_dir?.join("ui-test");
    if !directory.join("enabled").is_file() {
        return None;
    }
    eprintln!("argentum: the debug UI test bridge is on");
    let window = ui.weak_window();
    let timer = slint::Timer::default();
    let mut last_state = String::new();
    let mut handled = 0_usize;
    timer.start(slint::TimerMode::Repeated, POLL, move || {
        let Some(window) = window.upgrade() else {
            return;
        };
        handled = run_new_commands(&window, &directory.join("commands"), handled);
        let state = state_json(&window);
        if state != last_state && write_atomically(&directory, &state).is_ok() {
            last_state = state;
        }
    });
    Some(UiTestBridge { _timer: timer })
}

fn state_json(window: &argentum_ui::MainWindow) -> String {
    let app = window.global::<argentum_ui::App>();
    let messages = app.get_conversation_messages();
    let last_reply = (0..messages.row_count())
        .rev()
        .filter_map(|index| messages.row_data(index))
        .find(|message| message.role != "user")
        .map(|message| message.text.chars().take(400).collect::<String>())
        .unwrap_or_default();
    let fields: [(&str, Json); 13] = [
        ("first_run_open", Json::Bool(app.get_first_run_open())),
        (
            "first_run_stage",
            Json::Text(app.get_first_run_stage().to_string()),
        ),
        (
            "first_run_error",
            Json::Text(app.get_first_run_error().to_string()),
        ),
        ("settings_open", Json::Bool(app.get_settings_open())),
        ("drawer_open", Json::Bool(app.get_drawer_open())),
        (
            "provider_menu_open",
            Json::Bool(app.get_provider_menu_open()),
        ),
        (
            "command_palette_open",
            Json::Bool(app.get_command_palette_open()),
        ),
        ("approval_open", Json::Bool(app.get_approval_open())),
        (
            "back_layer_open",
            Json::Bool(window.invoke_back_layer_open()),
        ),
        ("running", Json::Bool(app.get_running())),
        (
            "provider_model",
            Json::Text(app.get_provider_model().to_string()),
        ),
        ("message_count", Json::Number(messages.row_count())),
        ("last_reply_text", Json::Text(last_reply)),
    ];
    let body = fields
        .iter()
        .map(|(name, value)| format!("\"{name}\":{}", value.render()))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

enum Json {
    Bool(bool),
    Number(usize),
    Text(String),
}

impl Json {
    fn render(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => value.to_string(),
            Self::Text(text) => {
                let mut out = String::with_capacity(text.len() + 2);
                out.push('"');
                for character in text.chars() {
                    match character {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        control if control.is_control() => {
                            out.push_str(&format!("\\u{:04x}", u32::from(control)));
                        }
                        other => out.push(other),
                    }
                }
                out.push('"');
                out
            }
        }
    }
}

fn write_atomically(directory: &Path, state: &str) -> std::io::Result<()> {
    let partial = directory.join("state.json.partial");
    let mut file = fs::File::create(&partial)?;
    file.write_all(state.as_bytes())?;
    file.sync_all()?;
    fs::rename(partial, directory.join("state.json"))
}

/// Runs the commands after the first `handled` lines and returns the new count.
fn run_new_commands(window: &argentum_ui::MainWindow, path: &Path, handled: usize) -> usize {
    let Ok(text) = fs::read_to_string(path) else {
        return handled;
    };
    // Only whole lines count, so a command is never read half written.
    let complete = text.rfind('\n').map_or("", |end| &text[..end]);
    let lines = complete.lines().collect::<Vec<_>>();
    for line in lines.iter().skip(handled) {
        match run_command(window, line) {
            Ok(()) => eprintln!("argentum: UI test command done: {}", command_name(line)),
            Err(reason) => eprintln!("argentum: UI test command refused: {reason}"),
        }
    }
    lines.len().max(handled)
}

fn command_name(line: &str) -> &str {
    line.split('\t').next().unwrap_or("")
}

fn run_command(window: &argentum_ui::MainWindow, line: &str) -> Result<(), &'static str> {
    if line.len() > MAX_COMMAND_BYTES {
        return Err("command too long");
    }
    let app = window.global::<argentum_ui::App>();
    let fields = line.split('\t').collect::<Vec<_>>();
    match fields.as_slice() {
        // The Continue button on the first-run server page.
        ["first_run_server", endpoint, model, context] => {
            app.invoke_first_run_save_server(
                (*endpoint).into(),
                (*model).into(),
                (*context).into(),
            );
            Ok(())
        }
        // The Start button at the end of first run.
        ["first_run_finish"] => {
            app.invoke_first_run_finish();
            Ok(())
        }
        // The send button in the composer, with this text.
        ["submit", text] if !text.trim().is_empty() => {
            app.invoke_submit_task((*text).into());
            Ok(())
        }
        _ => Err("unknown command"),
    }
}
