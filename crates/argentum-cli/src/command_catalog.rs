//! One source of truth for the advertised CLI surface.
//!
//! The catalog describes discovery and availability. It does not execute or
//! authorize anything and does not replace the parser's validation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    None,
    Extensions,
    DesktopControl,
    Artifacts,
}

impl Gate {
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Extensions => Some("extensions"),
            Self::DesktopControl => Some("desktop-control"),
            Self::Artifacts => Some("artifacts"),
        }
    }

    pub const fn compiled(self) -> bool {
        match self {
            Self::None => true,
            Self::Extensions => cfg!(feature = "extensions"),
            Self::DesktopControl => cfg!(all(windows, feature = "desktop-control")),
            Self::Artifacts => cfg!(feature = "artifacts"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Available,
    Reserved,
    Unavailable,
}

impl Availability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Reserved => "reserved",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CommandEntry {
    pub group: &'static str,
    pub path: &'static [&'static str],
    pub summary: &'static str,
    pub gate: Gate,
    pub mutation: bool,
    pub reserved: bool,
}

impl CommandEntry {
    pub fn availability(&self) -> Availability {
        if self.reserved {
            Availability::Reserved
        } else if self.gate.compiled() {
            Availability::Available
        } else {
            Availability::Unavailable
        }
    }

    pub fn path_string(&self) -> String {
        self.path.join(" ")
    }
}

const fn cmd(
    group: &'static str,
    path: &'static [&'static str],
    summary: &'static str,
    gate: Gate,
    mutation: bool,
) -> CommandEntry {
    CommandEntry {
        group,
        path,
        summary,
        gate,
        mutation,
        reserved: false,
    }
}

const fn reserved(path: &'static [&'static str], summary: &'static str) -> CommandEntry {
    CommandEntry {
        group: "reserved",
        path,
        summary,
        gate: Gate::None,
        mutation: false,
        reserved: true,
    }
}

pub const COMMANDS: &[CommandEntry] = &[
    cmd(
        "core",
        &["commands"],
        "List the CLI surface, availability and feature gates",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["chat"],
        "Open the interactive terminal shell",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["setup"],
        "Run deterministic first-run onboarding",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["doctor"],
        "Read-only workspace, database, provider, harness and recovery diagnostics",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["serve"],
        "Keep one runtime alive and exchange protocol v1 JSONL on stdin/stdout",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["run"],
        "Submit one task and stream its output",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["result"],
        "Read one settled run result without launching work",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["diagnostics"],
        "Read bounded metadata for one run",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["accounting", "inspect"],
        "Inspect accounting reservations read-only",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["accounting", "resolve"],
        "Resolve an accounting reservation with owner confirmation",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["skills", "list"],
        "List project-local skills",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["skills", "inspect"],
        "Inspect one project-local skill",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["status"],
        "Show workspace and provider readiness",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["terminal-diagnostic"],
        "Write one synthetic terminal diagnostic",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["local-chat"],
        "Open llama.cpp coding chat with policy-controlled tools",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["local-text"],
        "Open explicit text-only local chat",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["context"],
        "Preview selected working-context metadata",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["instructions"],
        "Preview selected project instruction metadata",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["compact"],
        "Compact or deactivate the active session context",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["workspace", "status"],
        "Show the persisted workspace selection",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["workspace", "set"],
        "Set the persisted workspace selection",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["sessions"],
        "List durable sessions for the current workspace",
        Gate::None,
        false,
    ),
    cmd(
        "core",
        &["tool", "list"],
        "List registered developer tools and availability",
        Gate::None,
        false,
    ),
    cmd(
        "provider",
        &["provider", "probe"],
        "Probe one configured provider",
        Gate::None,
        false,
    ),
    cmd(
        "provider",
        &["provider", "list"],
        "List provider profiles",
        Gate::None,
        false,
    ),
    cmd(
        "provider",
        &["provider", "save"],
        "Save a provider profile",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "presets"],
        "List provider catalog presets",
        Gate::None,
        false,
    ),
    cmd(
        "provider",
        &["provider", "add"],
        "Add a profile from a provider preset",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "login"],
        "Start ChatGPT browser sign-in",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "login-new"],
        "Start a new ChatGPT browser sign-in",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "accounts"],
        "List ChatGPT accounts",
        Gate::None,
        false,
    ),
    cmd(
        "provider",
        &["provider", "account"],
        "Select a ChatGPT account",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "logout"],
        "Sign out of a ChatGPT account",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "select"],
        "Select a provider profile",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "models"],
        "List models for a provider profile",
        Gate::None,
        false,
    ),
    cmd(
        "provider",
        &["provider", "model"],
        "Select a model for a provider profile",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "budget"],
        "Declare model budget limits",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "context"],
        "Configure local llama.cpp context",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "credential", "set"],
        "Save a secure provider credential",
        Gate::None,
        true,
    ),
    cmd(
        "provider",
        &["provider", "credential", "clear"],
        "Clear a secure provider credential",
        Gate::None,
        true,
    ),
    cmd(
        "session",
        &["session", "select"],
        "Select the active session",
        Gate::None,
        true,
    ),
    cmd(
        "session",
        &["session", "inspect"],
        "Inspect one session",
        Gate::None,
        false,
    ),
    cmd(
        "session",
        &["session", "pin"],
        "Pin a session",
        Gate::None,
        true,
    ),
    cmd(
        "session",
        &["session", "unpin"],
        "Unpin a session",
        Gate::None,
        true,
    ),
    cmd(
        "session",
        &["session", "archive"],
        "Archive a session",
        Gate::None,
        true,
    ),
    cmd(
        "session",
        &["session", "unarchive"],
        "Unarchive a session",
        Gate::None,
        true,
    ),
    cmd(
        "session",
        &["session", "title"],
        "Set a session title",
        Gate::None,
        true,
    ),
    cmd(
        "session",
        &["session", "fork"],
        "Fork a session",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "status"],
        "Show the active goal contract",
        Gate::None,
        false,
    ),
    cmd(
        "goal",
        &["goal", "limits"],
        "Amend aggregate goal limits",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "criteria"],
        "Set goal verification runners",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "attest"],
        "Attest goal completion",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "supervise"],
        "Supervise a finite goal",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "deliver"],
        "Deliver a finite multi-stage goal",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "set"],
        "Set a goal objective and budgets",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "pause"],
        "Pause the active goal",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "resume"],
        "Resume a paused goal",
        Gate::None,
        true,
    ),
    cmd(
        "goal",
        &["goal", "clear"],
        "Clear the active goal",
        Gate::None,
        true,
    ),
    cmd(
        "harness",
        &["harness", "status"],
        "Show harness composition and readiness",
        Gate::None,
        false,
    ),
    cmd(
        "harness",
        &["harness", "profile"],
        "Select a harness profile",
        Gate::None,
        true,
    ),
    cmd(
        "harness",
        &["harness", "surface"],
        "Show or hide a harness surface",
        Gate::None,
        true,
    ),
    cmd(
        "harness",
        &["harness", "execution"],
        "Select an execution policy profile",
        Gate::None,
        true,
    ),
    cmd(
        "harness",
        &["harness", "capability"],
        "Enable or disable a capability",
        Gate::None,
        true,
    ),
    cmd(
        "harness",
        &["harness", "backend"],
        "Enable or disable a local backend",
        Gate::None,
        true,
    ),
    cmd(
        "harness",
        &["harness", "module"],
        "Install, enable, disable or remove a module",
        Gate::None,
        true,
    ),
    cmd(
        "harness",
        &["harness", "trajectory"],
        "Inspect a session trajectory",
        Gate::None,
        false,
    ),
    cmd(
        "desktop",
        &["desktop", "status"],
        "Show desktop-control build and capability status",
        Gate::DesktopControl,
        false,
    ),
    cmd(
        "desktop",
        &["desktop", "list"],
        "List bounded top-level windows",
        Gate::DesktopControl,
        false,
    ),
    cmd(
        "desktop",
        &["desktop", "inspect"],
        "Inspect a bounded UIA element tree",
        Gate::DesktopControl,
        false,
    ),
    cmd(
        "desktop",
        &["desktop", "screenshot"],
        "Capture one bounded window PNG",
        Gate::DesktopControl,
        true,
    ),
    cmd(
        "desktop",
        &["desktop", "invoke"],
        "Invoke one observed element",
        Gate::DesktopControl,
        true,
    ),
    cmd(
        "desktop",
        &["desktop", "type"],
        "Type bounded text into one element",
        Gate::DesktopControl,
        true,
    ),
    cmd(
        "desktop",
        &["desktop", "keys"],
        "Send bounded key chords",
        Gate::DesktopControl,
        true,
    ),
    cmd(
        "permissions",
        &["permissions", "list"],
        "Inspect permission profiles",
        Gate::None,
        false,
    ),
    cmd(
        "permissions",
        &["permissions", "select"],
        "Select a permission profile",
        Gate::None,
        true,
    ),
    cmd(
        "permissions",
        &["permissions", "grants"],
        "List approval grants",
        Gate::None,
        false,
    ),
    cmd(
        "permissions",
        &["permissions", "revoke"],
        "Revoke an approval grant",
        Gate::None,
        true,
    ),
    cmd(
        "verification",
        &["verification", "list"],
        "List fixed verification runners",
        Gate::None,
        false,
    ),
    cmd(
        "verification",
        &["verification", "inspect"],
        "Inspect verification runner state",
        Gate::None,
        false,
    ),
    cmd(
        "verification",
        &["verification", "approve"],
        "Approve one verification runner",
        Gate::None,
        true,
    ),
    cmd(
        "verification",
        &["verification", "status"],
        "Show verification status",
        Gate::None,
        false,
    ),
    cmd(
        "changes",
        &["changes", "page"],
        "Page durable change history",
        Gate::None,
        false,
    ),
    cmd(
        "changes",
        &["changes", "list"],
        "List changes for one run",
        Gate::None,
        false,
    ),
    cmd(
        "changes",
        &["changes", "diff"],
        "Show one change diff",
        Gate::None,
        false,
    ),
    cmd(
        "changes",
        &["changes", "restore"],
        "Request an approved restore",
        Gate::None,
        true,
    ),
    cmd(
        "changes",
        &["changes", "recovery"],
        "Retry or restore a change operation",
        Gate::None,
        true,
    ),
    cmd(
        "core",
        &["device", "show"],
        "Show this device's pairing fingerprint, creating its key on first use",
        Gate::None,
        true,
    ),
    cmd(
        "recovery",
        &["recovery", "inspect"],
        "Inspect abandoned local owner state",
        Gate::None,
        false,
    ),
    cmd(
        "recovery",
        &["recovery", "reconcile"],
        "Reconcile abandoned local owner state",
        Gate::None,
        true,
    ),
    cmd(
        "recovery",
        &["recovery", "review"],
        "Review one recovery revision",
        Gate::None,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "inspect"],
        "Inspect a package",
        Gate::Extensions,
        false,
    ),
    cmd(
        "extensions",
        &["extensions", "install"],
        "Install a reviewed package",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "configure"],
        "Configure an extension definition",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "browser", "setup"],
        "Configure the first-party browser adapter",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "browser", "status"],
        "Show browser adapter definition status",
        Gate::Extensions,
        false,
    ),
    cmd(
        "extensions",
        &["extensions", "browser", "check"],
        "Check browser runtime readiness",
        Gate::Extensions,
        false,
    ),
    cmd(
        "extensions",
        &["extensions", "browser", "prepare"],
        "Prepare browser dependencies with explicit network consent",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "list"],
        "List installed extension records",
        Gate::Extensions,
        false,
    ),
    cmd(
        "extensions",
        &["extensions", "status"],
        "Show extension records and readiness",
        Gate::Extensions,
        false,
    ),
    cmd(
        "extensions",
        &["extensions", "receipts"],
        "Show extension operation receipts",
        Gate::Extensions,
        false,
    ),
    cmd(
        "extensions",
        &["extensions", "activate"],
        "Activate an extension in the local owner console",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "disable"],
        "Disable an extension",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "remove"],
        "Remove an extension registration",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "key"],
        "Save a secure extension service key",
        Gate::Extensions,
        true,
    ),
    cmd(
        "extensions",
        &["extensions", "prepare-media"],
        "Prepare a private extension image directory",
        Gate::Extensions,
        true,
    ),
    cmd(
        "artifacts",
        &["artifacts", "tools"],
        "List local artifact tools",
        Gate::Artifacts,
        false,
    ),
    cmd(
        "artifacts",
        &["artifacts", "inspect"],
        "Inspect a local artifact configuration",
        Gate::Artifacts,
        false,
    ),
    cmd(
        "artifacts",
        &["artifacts", "serve"],
        "Serve a local artifact configuration",
        Gate::Artifacts,
        false,
    ),
    cmd(
        "artifacts",
        &["artifacts", "verify"],
        "Verify a local artifact configuration",
        Gate::Artifacts,
        false,
    ),
    reserved(
        &["model"],
        "Reserved model-management alias; use provider models or provider model",
    ),
    reserved(
        &["profile"],
        "Reserved profile-management alias; use provider save and provider select",
    ),
    reserved(
        &["module"],
        "Reserved module-management alias; use harness module",
    ),
    reserved(
        &["plugin"],
        "Reserved plugin-management alias; use extensions install and configure",
    ),
    reserved(
        &["mcp"],
        "Reserved MCP server-management alias; use the extensions family",
    ),
    reserved(
        &["migrate"],
        "Reserved explicit store-migration command; schema migration runs on open",
    ),
    reserved(&["prompt"], "Reserved prompt-template management"),
    reserved(&["self"], "Reserved self-management command"),
    reserved(&["update"], "Reserved self-update command"),
];

pub fn command_count() -> usize {
    COMMANDS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_paths_are_unique_and_well_formed() {
        let mut seen = std::collections::BTreeSet::new();
        for entry in COMMANDS {
            assert!(!entry.group.is_empty(), "empty group");
            assert!(
                !entry.summary.is_empty(),
                "empty summary for {:?}",
                entry.path
            );
            assert!(!entry.path.is_empty(), "empty path");
            let path = entry.path_string();
            assert!(seen.insert(path.clone()), "duplicate catalog path: {path}");
        }
        assert!(command_count() >= 100, "catalog unexpectedly small");
    }

    #[test]
    fn reserved_entries_match_the_parser_stub_set() {
        let expected = [
            "mcp", "migrate", "model", "module", "plugin", "profile", "prompt", "self", "update",
        ];
        let mut reserved = COMMANDS
            .iter()
            .filter(|entry| entry.reserved)
            .map(|entry| entry.path_string())
            .collect::<Vec<_>>();
        reserved.sort();
        assert_eq!(reserved, expected);
    }

    #[test]
    fn availability_respects_feature_gates_and_reservations() {
        for entry in COMMANDS {
            match entry.availability() {
                Availability::Reserved => assert!(entry.reserved),
                Availability::Available => {
                    assert!(!entry.reserved);
                    assert!(entry.gate.compiled());
                }
                Availability::Unavailable => {
                    assert!(!entry.reserved);
                    assert!(!entry.gate.compiled());
                }
            }
        }
    }

    #[test]
    fn documented_index_matches_the_catalog() {
        let guide = include_str!("../../../docs/CLI_GUIDE.md");
        for entry in COMMANDS {
            let path = entry.path_string();
            let line = format!("| `{path}` |");
            assert!(
                guide.contains(&line),
                "CLI_GUIDE.md is missing catalog path: {path}"
            );
        }
    }
}
