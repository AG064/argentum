// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! A plain description of what a shell command can reach on this platform.
//!
//! The same words go to the agent, in its system prompt, and to the owner, in
//! Settings, so both read one account of the environment. The description is
//! chosen when Argentum is built for a platform. It describes containment the
//! execution layer applies; it grants nothing and changes no policy.

/// What the shell is and what a command can reach, for this build's platform.
pub const fn execution_environment() -> &'static str {
    if cfg!(target_os = "android") {
        ANDROID
    } else if cfg!(windows) {
        WINDOWS
    } else {
        UNIX
    }
}

const WINDOWS: &str = "Shell commands run in Windows PowerShell (powershell.exe, without a profile and without a console window), starting in the workspace folder. Each command and every process it starts runs in a Windows job object with CPU and memory limits, and all of them stop when the command times out or Argentum closes. The environment is cleared except for system folders, PATH, TEMP, the user profile folders and the Rust toolchain variables, so tokens and keys set in your own environment are not visible. A command cannot read from a keyboard, so anything that waits for input fails. A command runs for at most five minutes, and output past 256 KiB is cut off. Commands run with your Windows account's access: they can reach files outside the workspace and the network, which is why each one asks first unless your permissions allow it. Installed programs such as git, cargo, node or python work when they are on PATH.";

const ANDROID: &str = "Shell commands run in Android's own shell (/system/bin/sh with toybox commands such as ls, cat, grep and sed), starting in the workspace folder, with the system PATH and a cleared environment. There is no Git, Rust toolchain, package manager or root access, and programs cannot be installed. A command reaches the app's private storage and, when All files access is on, shared storage such as Documents and Download. Commands run inside the Argentum app's Android sandbox, so they cannot see other apps' data. A command cannot read from a keyboard, so anything that waits for input fails. A command runs for at most five minutes and stops when Argentum closes, and output past 256 KiB is cut off.";

const UNIX: &str = "Shell commands run in /bin/sh, starting in the workspace folder, with a cleared environment that keeps PATH and the Rust toolchain variables. A command cannot read from a keyboard, so anything that waits for input fails. A command runs for at most five minutes and stops when Argentum closes, and output past 256 KiB is cut off. Commands run with your account's access: they can reach files outside the workspace and the network, which is why each one asks first unless your permissions allow it.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_description_is_plain_and_bounded() {
        for text in [WINDOWS, ANDROID, UNIX] {
            assert!(text.len() < 1_200, "{}", text.len());
            assert!(!text.contains('\u{2014}'), "no em dashes");
            assert!(text.contains("workspace"));
            assert!(text.contains("256 KiB") && text.contains("five minutes"));
        }
        assert_eq!(
            execution_environment(),
            if cfg!(windows) { WINDOWS } else { UNIX }
        );
    }
}
