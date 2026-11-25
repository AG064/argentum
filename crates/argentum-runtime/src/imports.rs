// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Importing settings from Claude Code, Codex and OpenCode.
//!
//! Every import starts from a fresh scan of the files on disk, so an id from
//! an earlier scan only selects; it never carries content. MCP servers become
//! drafts that are off, skills and commands become project skills. Nothing is
//! granted, approved or turned on.

use std::path::PathBuf;

use argentum_domain::imports::ImportResultView;
use argentum_domain::AppEvent;
use argentum_import::ImportScan;

use super::{RuntimeError, RuntimeService};

/// The owner's home folder, where other agents keep account settings.
pub fn import_home() -> Option<PathBuf> {
    std::env::home_dir().filter(|path| path.is_absolute())
}

impl RuntimeService {
    fn scan_imports(&self) -> ImportScan {
        argentum_import::scan(import_home().as_deref(), self.workspace.root())
    }

    pub(super) fn publish_import_scan(
        &self,
        results: Vec<ImportResultView>,
    ) -> Result<(), RuntimeError> {
        let scan = self.scan_imports();
        self.publish_transient(AppEvent::ImportScanSnapshot {
            items: scan.items.iter().map(argentum_import::item_view).collect(),
            problems: scan.problems,
            results,
        });
        Ok(())
    }

    pub(super) fn import_items(&self, ids: Vec<String>) -> Result<(), RuntimeError> {
        let scan = self.scan_imports();
        let results = argentum_import::apply(&scan, self.workspace.root(), &ids, |server| {
            self.store
                .save_imported_mcp_server(self.project_id, server)
                .map_err(|error| error.to_string())
        });
        self.publish_import_scan(results)
    }
}
