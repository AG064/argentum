// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! `argentum import`: bring MCP servers, skills and custom commands over from
//! Claude Code, Codex and OpenCode.
//!
//! Without a selection it lists what it found and imports nothing. In a
//! terminal it then asks which numbers to import. An import grants no
//! permission and approves no tool: MCP servers are saved as drafts that are
//! off, and skills keep no `allowed-tools`.

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use argentum_domain::imports::ImportResultView;
use argentum_import::{ImportItem, ImportScan};
use argentum_platform::AppPaths;
use argentum_store::Store;

pub struct ImportOptions {
    pub workspace: std::path::PathBuf,
    pub database: Option<std::path::PathBuf>,
    pub select: Vec<String>,
    pub all: bool,
    pub json: bool,
    pub non_interactive: bool,
}

pub fn run(options: ImportOptions) -> Result<(), Box<dyn std::error::Error>> {
    let workspace = options.workspace.canonicalize()?;
    let home = std::env::home_dir().filter(|path| path.is_absolute());
    let scan = argentum_import::scan(home.as_deref(), &workspace);
    let mut ids = resolve_selection(&scan, &options.select)?;
    if options.all {
        ids.extend(scan.importable().map(|item| item.id.clone()));
    }
    let ask = ids.is_empty()
        && !options.json
        && !options.non_interactive
        && io::stdin().is_terminal()
        && io::stdout().is_terminal();
    if !options.json {
        print_scan(&scan);
    }
    if ask && scan.importable().next().is_some() {
        print!("Numbers to import, separated by spaces (Enter for none): ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        let picked: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        ids = resolve_selection(&scan, &picked)?;
    }
    let results = if ids.is_empty() {
        Vec::new()
    } else {
        let database = options
            .database
            .clone()
            .map_or_else(|| AppPaths::discover().map(|paths| paths.database), Ok)?;
        let store = Store::open(&database)?;
        let project = store.resolve_workspace(&workspace)?.snapshot.project.id;
        argentum_import::apply(&scan, &workspace, &ids, |server| {
            store
                .save_imported_mcp_server(project, server)
                .map_err(|error| error.to_string())
        })
    };
    if options.json {
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "items": scan.items.iter().map(argentum_import::item_view).collect::<Vec<_>>(),
                "problems": scan.problems,
                "results": results,
            }))?
        );
    } else if ids.is_empty() {
        if scan.importable().next().is_some() {
            println!("Nothing imported. Pick items with: argentum import --select NUMBER_OR_ID");
        }
    } else {
        print_results(&results);
    }
    Ok(())
}

/// Accepts the numbers [`print_scan`] shows, or item ids.
fn resolve_selection(scan: &ImportScan, picked: &[String]) -> Result<Vec<String>, String> {
    let numbered: Vec<&ImportItem> = scan.items.iter().collect();
    picked
        .iter()
        .map(|value| {
            if let Ok(number) = value.parse::<usize>() {
                numbered
                    .get(number.wrapping_sub(1))
                    .map(|item| item.id.clone())
                    .ok_or_else(|| format!("there is no item {number}"))
            } else if scan.find(value).is_some() {
                Ok(value.clone())
            } else {
                Err(format!("no item has the id {value}"))
            }
        })
        .collect()
}

fn print_scan(scan: &ImportScan) {
    if scan.items.is_empty() {
        println!("Found nothing to import from Claude Code, Codex or OpenCode.");
    } else {
        println!("Found in other agents' settings:");
    }
    for (index, item) in scan.items.iter().enumerate() {
        let mark = if item.importable() { " " } else { "-" };
        println!("{mark}{:>3}. {}", index + 1, item.summary());
        println!("       from {} ({})", item.source_path, item.origin.label());
        if let Some(destination) = item.destination() {
            println!("       would write {destination}");
        }
        println!("       id {}", item.id);
        if let Some(conflict) = &item.conflict {
            println!("       cannot import: {conflict}");
        }
        if !item.dropped.is_empty() {
            println!("       left out: {}", item.dropped.join("; "));
        }
    }
    for problem in &scan.problems {
        println!("Could not read {problem}");
    }
    if scan.importable().next().is_some() {
        println!(
            "Importing grants no permission. MCP servers are saved turned off, and skills keep no allowed tools."
        );
    }
}

fn print_results(results: &[ImportResultView]) {
    for result in results {
        let state = if result.imported {
            "Imported"
        } else {
            "Skipped"
        };
        println!("{state} {}: {}", result.id, result.detail);
    }
}

/// One line for first-run setup, when there is something to import.
pub fn first_run_offer(workspace: &Path) -> Option<String> {
    let home = std::env::home_dir().filter(|path| path.is_absolute());
    let scan = argentum_import::scan(home.as_deref(), workspace);
    let count = scan.importable().count();
    (count > 0).then(|| {
        format!(
            "Found {count} item{} from Claude Code, Codex or OpenCode. Run `argentum import` to choose what to bring over.",
            if count == 1 { "" } else { "s" }
        )
    })
}
