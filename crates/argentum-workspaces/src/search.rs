use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use argentum_domain::TextSearchMode;
use argentum_security::CapabilityBroker;
use regex::RegexBuilder;

use super::{
    normalize_display_path, sha256_digest, truncate_utf8, wildcard_matches, WorkspaceError,
    WorkspaceFilenameMatch, WorkspaceTextContextLine, WorkspaceTextMatch,
    WorkspaceTextReadSelection, WorkspaceTextSearchOptions,
};

pub const MAX_SEARCH_ENTRIES: u64 = 16_384;
pub const MAX_SEARCH_DIRECTORIES: u64 = 4_096;
pub const MAX_SEARCH_DEPTH: usize = 64;
pub const MAX_SEARCH_FILES: u64 = 4_096;
pub const MAX_SEARCH_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_SEARCH_FILE_BYTES: u64 = 1024 * 1024;
pub const MAX_SEARCH_MATCHES: usize = 1_000;
pub const MAX_SEARCH_QUERY_BYTES: usize = 4 * 1024;
pub const MAX_SEARCH_LINE_BYTES: usize = 4 * 1024;
pub const MAX_SEARCH_CONTEXT_LINES: u8 = 8;
pub const MAX_REGEX_NESTING: u32 = 32;
pub const MAX_REGEX_COMPILED_BYTES: usize = 256 * 1024;
pub const MAX_REGEX_DFA_BYTES: usize = 256 * 1024;
pub const MAX_IGNORE_FILE_BYTES: u64 = 64 * 1024;
pub const MAX_IGNORE_TOTAL_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_IGNORE_RULES: u64 = 2_048;
pub const MAX_IGNORE_CHAIN_DEPTH: usize = 64;
const SEARCH_READ_SELECTION_RADIUS_LINES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSearchStopReason {
    Complete,
    ResultLimit,
    EntryLimit,
    DirectoryLimit,
    DepthLimit,
    FileLimit,
    ByteLimit,
    Cancelled,
    UnreadableDirectory,
    UnreadableContent,
    InvalidContent,
    OversizedContent,
    InvalidIgnoreSource,
    OversizedIgnoreSource,
}

impl WorkspaceSearchStopReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::ResultLimit => "result_limit",
            Self::EntryLimit => "entry_limit",
            Self::DirectoryLimit => "directory_limit",
            Self::DepthLimit => "depth_limit",
            Self::FileLimit => "file_limit",
            Self::ByteLimit => "byte_limit",
            Self::Cancelled => "cancelled",
            Self::UnreadableDirectory => "unreadable_directory",
            Self::UnreadableContent => "unreadable_content",
            Self::InvalidContent => "invalid_content",
            Self::OversizedContent => "oversized_content",
            Self::InvalidIgnoreSource => "invalid_ignore_source",
            Self::OversizedIgnoreSource => "oversized_ignore_source",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSearchReport<T> {
    pub matches: Vec<T>,
    pub complete: bool,
    pub stop_reason: WorkspaceSearchStopReason,
    pub entries_visited: u64,
    pub directories_opened: u64,
    pub files_considered: u64,
    pub files_read: u64,
    pub bytes_read: u64,
    pub matches_observed: u64,
    pub ignored_directories_pruned: u64,
    pub ignored_files_skipped: u64,
    pub policy_directories_pruned: u64,
    pub policy_files_skipped: u64,
    pub symlinks_skipped: u64,
    pub oversized_files_skipped: u64,
    pub binary_files_skipped: u64,
    pub invalid_files_skipped: u64,
    pub unreadable_directories: u64,
    pub unreadable_files: u64,
    pub ignore_files_loaded: u64,
    pub ignore_bytes_read: u64,
    pub ignore_rules_loaded: u64,
    pub incomplete_reasons: Vec<String>,
}

impl<T> WorkspaceSearchReport<T> {
    fn new() -> Self {
        Self {
            matches: Vec::new(),
            complete: true,
            stop_reason: WorkspaceSearchStopReason::Complete,
            entries_visited: 0,
            directories_opened: 0,
            files_considered: 0,
            files_read: 0,
            bytes_read: 0,
            matches_observed: 0,
            ignored_directories_pruned: 0,
            ignored_files_skipped: 0,
            policy_directories_pruned: 0,
            policy_files_skipped: 0,
            symlinks_skipped: 0,
            oversized_files_skipped: 0,
            binary_files_skipped: 0,
            invalid_files_skipped: 0,
            unreadable_directories: 0,
            unreadable_files: 0,
            ignore_files_loaded: 0,
            ignore_bytes_read: 0,
            ignore_rules_loaded: 0,
            incomplete_reasons: Vec::new(),
        }
    }

    fn incomplete(&mut self, reason: WorkspaceSearchStopReason) {
        self.complete = false;
        if self.stop_reason == WorkspaceSearchStopReason::Complete {
            self.stop_reason = reason;
        }
        let reason = reason.as_str().to_owned();
        if !self.incomplete_reasons.iter().any(|item| item == &reason)
            && self.incomplete_reasons.len() < 8
        {
            self.incomplete_reasons.push(reason);
        }
    }

    fn map_matches<U, F>(self, mut map: F) -> WorkspaceSearchReport<U>
    where
        F: FnMut(T) -> Option<U>,
    {
        let Self {
            matches,
            complete,
            stop_reason,
            entries_visited,
            directories_opened,
            files_considered,
            files_read,
            bytes_read,
            matches_observed,
            ignored_directories_pruned,
            ignored_files_skipped,
            policy_directories_pruned,
            policy_files_skipped,
            symlinks_skipped,
            oversized_files_skipped,
            binary_files_skipped,
            invalid_files_skipped,
            unreadable_directories,
            unreadable_files,
            ignore_files_loaded,
            ignore_bytes_read,
            ignore_rules_loaded,
            incomplete_reasons,
        } = self;
        WorkspaceSearchReport {
            matches: matches.into_iter().filter_map(&mut map).collect(),
            complete,
            stop_reason,
            entries_visited,
            directories_opened,
            files_considered,
            files_read,
            bytes_read,
            matches_observed,
            ignored_directories_pruned,
            ignored_files_skipped,
            policy_directories_pruned,
            policy_files_skipped,
            symlinks_skipped,
            oversized_files_skipped,
            binary_files_skipped,
            invalid_files_skipped,
            unreadable_directories,
            unreadable_files,
            ignore_files_loaded,
            ignore_bytes_read,
            ignore_rules_loaded,
            incomplete_reasons,
        }
    }
}

#[derive(Debug, Clone)]
struct IgnoreRule {
    base: PathBuf,
    pattern: String,
    negated: bool,
    directory_only: bool,
    /// A leading or middle separator binds the pattern to the ignore base.
    anchored_to_base: bool,
}

impl IgnoreRule {
    fn matches(&self, candidate: &Path, is_directory: bool) -> bool {
        if self.directory_only && !is_directory {
            return false;
        }
        // The ignore file's own directory is the origin for every pattern, so a
        // candidate outside that subtree can never match this rule.
        let Ok(relative) = candidate.strip_prefix(&self.base) else {
            return false;
        };
        let relative = normalize_display_path(relative);
        if relative.is_empty() {
            return false;
        }
        if !self.pattern.contains('/') {
            if self.anchored_to_base && relative.contains('/') {
                return false;
            }
            return relative
                .rsplit('/')
                .next()
                .is_some_and(|name| wildcard_matches(&self.pattern, name));
        }
        if wildcard_matches(&self.pattern, &relative) {
            return true;
        }
        if self.anchored_to_base {
            return false;
        }
        let mut components = relative.split('/').collect::<Vec<_>>();
        while components.len() > 1 {
            components.remove(0);
            if wildcard_matches(&self.pattern, &components.join("/")) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug)]
enum SearchKind {
    Text(TextSearch),
    Filename(String),
}

#[derive(Debug)]
struct TextSearch {
    matcher: TextMatcher,
    before_context: u8,
    after_context: u8,
}

#[derive(Debug)]
enum TextMatcher {
    Literal(String),
    Regex(regex::Regex),
}

impl TextMatcher {
    fn is_match(&self, line: &str) -> bool {
        match self {
            Self::Literal(query) => line.contains(query),
            Self::Regex(regex) => regex.is_match(line),
        }
    }
}

#[derive(Debug)]
struct SearchWalker<'a, C> {
    broker: Option<&'a CapabilityBroker>,
    workspace_root: Option<&'a Path>,
    base: PathBuf,
    workspace_base: Option<PathBuf>,
    kind: SearchKind,
    max_results: usize,
    cancelled: C,
    report: WorkspaceSearchReport<SearchMatch>,
    rules: Vec<IgnoreRule>,
    stop: bool,
}

#[derive(Debug)]
enum SearchMatch {
    Text(WorkspaceTextMatch),
    Filename(WorkspaceFilenameMatch),
}

enum SearchFile {
    Workspace(cap_std::fs::File),
    Ambient(File),
}

fn cancelled_report<T>() -> WorkspaceSearchReport<T> {
    let mut report = WorkspaceSearchReport::new();
    report.incomplete(WorkspaceSearchStopReason::Cancelled);
    report
}

fn text_search_kind(
    query: &str,
    options: WorkspaceTextSearchOptions,
) -> Result<SearchKind, WorkspaceError> {
    if query.is_empty() || query.len() > MAX_SEARCH_QUERY_BYTES {
        return Err(WorkspaceError::LimitExceeded("search query"));
    }
    if options.before_context > MAX_SEARCH_CONTEXT_LINES
        || options.after_context > MAX_SEARCH_CONTEXT_LINES
    {
        return Err(WorkspaceError::LimitExceeded("search context lines"));
    }
    let matcher = match options.mode {
        TextSearchMode::Literal => TextMatcher::Literal(query.to_owned()),
        TextSearchMode::Regex => TextMatcher::Regex(
            RegexBuilder::new(query)
                .case_insensitive(false)
                .multi_line(false)
                .dot_matches_new_line(false)
                .unicode(true)
                .size_limit(MAX_REGEX_COMPILED_BYTES)
                .dfa_size_limit(MAX_REGEX_DFA_BYTES)
                .nest_limit(MAX_REGEX_NESTING)
                .build()
                .map_err(|_| {
                    WorkspaceError::InvalidText(
                        "regular expression is invalid or exceeds bounded search limits",
                    )
                })?,
        ),
    };
    Ok(SearchKind::Text(TextSearch {
        matcher,
        before_context: options.before_context,
        after_context: options.after_context,
    }))
}

fn bounded_context_line(index: usize, line: &str) -> WorkspaceTextContextLine {
    WorkspaceTextContextLine {
        line_number: index.saturating_add(1).try_into().unwrap_or(u64::MAX),
        line: truncate_utf8(line, MAX_SEARCH_LINE_BYTES),
        truncated: line.len() > MAX_SEARCH_LINE_BYTES,
    }
}

pub(crate) fn workspace_text<C: FnMut() -> bool>(
    broker: &CapabilityBroker,
    relative: &Path,
    query: &str,
    max_results: usize,
    options: WorkspaceTextSearchOptions,
    mut cancelled: C,
) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
    if cancelled() {
        return Ok(cancelled_report());
    }
    let kind = text_search_kind(query, options)?;
    let root = broker.root().to_path_buf();
    let base = root.join(relative);
    let mut walker = SearchWalker::new(
        Some(broker),
        Some(&root),
        base,
        Some(relative.to_path_buf()),
        kind,
        max_results,
        cancelled,
    )?;
    walker.load_workspace_ignore_chain(relative)?;
    walker.walk_scope()?;
    Ok(walker.into_text_report())
}

pub(crate) fn workspace_filenames<C: FnMut() -> bool>(
    broker: &CapabilityBroker,
    relative: &Path,
    pattern: &str,
    max_results: usize,
    cancelled: C,
) -> Result<WorkspaceSearchReport<WorkspaceFilenameMatch>, WorkspaceError> {
    let root = broker.root().to_path_buf();
    let base = root.join(relative);
    let mut walker = SearchWalker::new(
        Some(broker),
        Some(&root),
        base,
        Some(relative.to_path_buf()),
        SearchKind::Filename(pattern.to_owned()),
        max_results,
        cancelled,
    )?;
    walker.load_workspace_ignore_chain(relative)?;
    walker.walk_scope()?;
    Ok(walker.into_filename_report())
}

pub(crate) fn ambient_text<C: FnMut() -> bool>(
    base: &Path,
    query: &str,
    max_results: usize,
    options: WorkspaceTextSearchOptions,
    mut cancelled: C,
) -> Result<WorkspaceSearchReport<WorkspaceTextMatch>, WorkspaceError> {
    if cancelled() {
        return Ok(cancelled_report());
    }
    let kind = text_search_kind(query, options)?;
    let mut walker = SearchWalker::new(
        None,
        None,
        base.to_path_buf(),
        None,
        kind,
        max_results,
        cancelled,
    )?;
    let base = walker.base.clone();
    walker.load_ignore_file(&base);
    walker.walk_scope()?;
    Ok(walker.into_text_report())
}

pub(crate) fn ambient_filenames<C: FnMut() -> bool>(
    base: &Path,
    pattern: &str,
    max_results: usize,
    cancelled: C,
) -> Result<WorkspaceSearchReport<WorkspaceFilenameMatch>, WorkspaceError> {
    let mut walker = SearchWalker::new(
        None,
        None,
        base.to_path_buf(),
        None,
        SearchKind::Filename(pattern.to_owned()),
        max_results,
        cancelled,
    )?;
    let base = walker.base.clone();
    walker.load_ignore_file(&base);
    walker.walk_scope()?;
    Ok(walker.into_filename_report())
}

impl<'a, C: FnMut() -> bool> SearchWalker<'a, C> {
    fn new(
        broker: Option<&'a CapabilityBroker>,
        workspace_root: Option<&'a Path>,
        base: PathBuf,
        workspace_base: Option<PathBuf>,
        kind: SearchKind,
        max_results: usize,
        cancelled: C,
    ) -> Result<Self, WorkspaceError> {
        if max_results == 0 {
            return Err(WorkspaceError::LimitExceeded("search results"));
        }
        if matches!(&kind, SearchKind::Filename(pattern) if pattern.len() > MAX_SEARCH_QUERY_BYTES)
        {
            return Err(WorkspaceError::LimitExceeded("search query"));
        }
        let metadata = fs::symlink_metadata(&base).map_err(|source| WorkspaceError::Io {
            path: base.clone(),
            source,
        })?;
        if unsupported_link(&metadata) {
            return Err(WorkspaceError::SymlinkDenied(base));
        }
        if !metadata.is_dir() && !metadata.is_file() {
            return Err(WorkspaceError::NotFile(base));
        }
        Ok(Self {
            broker,
            workspace_root,
            base,
            workspace_base,
            kind,
            max_results: max_results.min(MAX_SEARCH_MATCHES),
            cancelled,
            report: WorkspaceSearchReport::new(),
            rules: Vec::new(),
            stop: false,
        })
    }

    fn load_workspace_ignore_chain(&mut self, relative: &Path) -> Result<(), WorkspaceError> {
        let Some(root) = self.workspace_root else {
            return Ok(());
        };
        let base_metadata = fs::symlink_metadata(&self.base).ok();
        let relative = if base_metadata.as_ref().is_some_and(fs::Metadata::is_file) {
            relative.parent().unwrap_or_else(|| Path::new(""))
        } else {
            relative
        };
        let depth = relative
            .components()
            .filter(|component| matches!(component, std::path::Component::Normal(_)))
            .count();
        if depth > MAX_IGNORE_CHAIN_DEPTH {
            self.report
                .incomplete(WorkspaceSearchStopReason::DepthLimit);
            return Ok(());
        }
        let mut current = root.to_path_buf();
        self.load_ignore_file(&current);
        for component in relative.components() {
            if let std::path::Component::Normal(component) = component {
                current.push(component);
                self.load_ignore_file(&current);
            }
        }
        Ok(())
    }

    fn load_ignore_file(&mut self, directory: &Path) {
        if self.report.ignore_rules_loaded >= MAX_IGNORE_RULES {
            self.report
                .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
            return;
        }
        let path = directory.join(".gitignore");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => return,
            Err(_) => {
                self.report
                    .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
                return;
            }
        };
        if unsupported_link(&metadata) || !metadata.is_file() {
            self.report
                .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
            return;
        }
        if metadata.len() > MAX_IGNORE_FILE_BYTES {
            self.report
                .incomplete(WorkspaceSearchStopReason::OversizedIgnoreSource);
            return;
        }
        if self.report.ignore_bytes_read.saturating_add(metadata.len()) > MAX_IGNORE_TOTAL_BYTES {
            self.report
                .incomplete(WorkspaceSearchStopReason::OversizedIgnoreSource);
            return;
        }
        let mut file = match (self.broker, self.workspace_root) {
            (Some(broker), Some(root)) => {
                let Ok(relative_directory) = directory.strip_prefix(root) else {
                    self.report
                        .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
                    return;
                };
                match broker.open_file_for_read(relative_directory.join(".gitignore")) {
                    Ok((_, file)) => SearchFile::Workspace(file),
                    Err(_) => {
                        self.report
                            .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
                        return;
                    }
                }
            }
            _ => match File::open(&path) {
                Ok(file) => SearchFile::Ambient(file),
                Err(_) => {
                    self.report
                        .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
                    return;
                }
            },
        };
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let mut buffer = [0_u8; 8 * 1024];
        while bytes.len() < MAX_IGNORE_FILE_BYTES as usize {
            let read_limit = buffer
                .len()
                .min((MAX_IGNORE_FILE_BYTES as usize).saturating_sub(bytes.len()));
            let read = match &mut file {
                SearchFile::Workspace(file) => file.read(&mut buffer[..read_limit]),
                SearchFile::Ambient(file) => file.read(&mut buffer[..read_limit]),
            };
            let read = match read {
                Ok(read) => read,
                Err(_) => {
                    self.report
                        .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
                    return;
                }
            };
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
        }
        if bytes.len() == MAX_IGNORE_FILE_BYTES as usize && metadata.len() < MAX_IGNORE_FILE_BYTES {
            self.report
                .incomplete(WorkspaceSearchStopReason::OversizedIgnoreSource);
            return;
        }
        self.report.ignore_files_loaded = self.report.ignore_files_loaded.saturating_add(1);
        self.report.ignore_bytes_read = self
            .report
            .ignore_bytes_read
            .saturating_add(bytes.len() as u64);
        let Ok(text) = String::from_utf8(bytes) else {
            self.report
                .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
            return;
        };
        for line in text.lines() {
            if line.len() > MAX_SEARCH_LINE_BYTES || line.contains('\0') {
                self.report
                    .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
                continue;
            }
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                continue;
            }
            let mut pattern = line;
            if let Some(rest) = pattern.strip_prefix("\\#") {
                pattern = rest;
            } else if pattern.starts_with('#') {
                continue;
            }
            let negated = if let Some(rest) = pattern.strip_prefix('!') {
                pattern = rest;
                true
            } else if let Some(rest) = pattern.strip_prefix("\\!") {
                pattern = rest;
                false
            } else {
                false
            };
            let directory_only = pattern.ends_with('/');
            pattern = pattern.trim_end_matches('/');
            // A trailing directory-only separator has already been removed.
            let anchored_to_base = pattern.contains('/');
            pattern = pattern.trim_start_matches('/');
            if pattern.is_empty()
                || pattern.contains('[')
                || pattern.contains(']')
                || pattern.contains('\\')
                || self.report.ignore_rules_loaded >= MAX_IGNORE_RULES
            {
                self.report
                    .incomplete(WorkspaceSearchStopReason::InvalidIgnoreSource);
                continue;
            }
            self.rules.push(IgnoreRule {
                base: directory.to_path_buf(),
                pattern: pattern.to_owned(),
                negated,
                directory_only,
                anchored_to_base,
            });
            self.report.ignore_rules_loaded = self.report.ignore_rules_loaded.saturating_add(1);
        }
    }

    fn walk_scope(&mut self) -> Result<(), WorkspaceError> {
        if self.workspace_base.as_deref().is_some_and(|path| {
            path.components().any(|component| {
                matches!(component, std::path::Component::Normal(value) if value == ".git" || value == "target")
            })
        }) {
            self.report.policy_directories_pruned = 1;
            return Ok(());
        }
        if self.cancelled() {
            return Ok(());
        }
        let metadata = fs::symlink_metadata(&self.base).map_err(|source| WorkspaceError::Io {
            path: self.base.clone(),
            source,
        })?;
        if metadata.is_file() {
            let base = self.base.clone();
            let relative = self.workspace_relative(&self.base)?;
            if self.ignored(&base, false) {
                self.report.ignored_files_skipped = 1;
                return Ok(());
            }
            self.process_file(&base, &relative, metadata.len());
            return Ok(());
        }
        let base = self.base.clone();
        self.walk_directory(&base, self.workspace_base.clone(), 0, true);
        Ok(())
    }

    fn walk_directory(
        &mut self,
        directory: &Path,
        workspace_relative: Option<PathBuf>,
        depth: usize,
        ignore_loaded: bool,
    ) {
        if self.stop || self.cancelled() {
            return;
        }
        if depth > MAX_SEARCH_DEPTH {
            self.report
                .incomplete(WorkspaceSearchStopReason::DepthLimit);
            self.stop = true;
            return;
        }
        if self.report.directories_opened >= MAX_SEARCH_DIRECTORIES {
            self.report
                .incomplete(WorkspaceSearchStopReason::DirectoryLimit);
            self.stop = true;
            return;
        }
        self.report.directories_opened = self.report.directories_opened.saturating_add(1);
        if !ignore_loaded {
            self.load_ignore_file(directory);
        }
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => {
                self.report.unreadable_directories =
                    self.report.unreadable_directories.saturating_add(1);
                self.report
                    .incomplete(WorkspaceSearchStopReason::UnreadableDirectory);
                return;
            }
        };
        for entry in entries {
            if self.stop || self.cancelled() {
                return;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    self.report.unreadable_directories =
                        self.report.unreadable_directories.saturating_add(1);
                    self.report
                        .incomplete(WorkspaceSearchStopReason::UnreadableDirectory);
                    continue;
                }
            };
            self.report.entries_visited = self.report.entries_visited.saturating_add(1);
            if self.report.entries_visited > MAX_SEARCH_ENTRIES {
                self.report
                    .incomplete(WorkspaceSearchStopReason::EntryLimit);
                self.stop = true;
                return;
            }
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    self.report.unreadable_files = self.report.unreadable_files.saturating_add(1);
                    self.report
                        .incomplete(WorkspaceSearchStopReason::UnreadableContent);
                    continue;
                }
            };
            if unsupported_link(&metadata) {
                self.report.symlinks_skipped = self.report.symlinks_skipped.saturating_add(1);
                continue;
            }
            let is_directory = metadata.is_dir();
            let child_relative = workspace_relative
                .as_ref()
                .map(|relative| relative.join(entry.file_name()));
            if self.policy_ignored(child_relative.as_deref()) {
                if is_directory {
                    self.report.policy_directories_pruned =
                        self.report.policy_directories_pruned.saturating_add(1);
                } else {
                    self.report.policy_files_skipped =
                        self.report.policy_files_skipped.saturating_add(1);
                }
                continue;
            }
            if self.ignored(&path, is_directory) {
                if is_directory {
                    self.report.ignored_directories_pruned =
                        self.report.ignored_directories_pruned.saturating_add(1);
                } else {
                    self.report.ignored_files_skipped =
                        self.report.ignored_files_skipped.saturating_add(1);
                }
                continue;
            }
            if is_directory {
                if depth.saturating_add(1) > MAX_SEARCH_DEPTH {
                    self.report
                        .incomplete(WorkspaceSearchStopReason::DepthLimit);
                    self.stop = true;
                    return;
                }
                self.walk_directory(&path, child_relative, depth.saturating_add(1), false);
            } else if metadata.is_file() {
                let relative = match self.workspace_relative(&path) {
                    Ok(relative) => relative,
                    Err(_) => {
                        self.report.unreadable_files =
                            self.report.unreadable_files.saturating_add(1);
                        self.report
                            .incomplete(WorkspaceSearchStopReason::UnreadableContent);
                        continue;
                    }
                };
                self.process_file(&path, &relative, metadata.len());
            }
        }
    }

    fn process_file(&mut self, path: &Path, relative: &Path, size: u64) {
        if self.stop || self.cancelled() {
            return;
        }
        self.report.files_considered = self.report.files_considered.saturating_add(1);
        if self.report.files_considered > MAX_SEARCH_FILES {
            self.report.incomplete(WorkspaceSearchStopReason::FileLimit);
            self.stop = true;
            return;
        }
        match &self.kind {
            SearchKind::Filename(pattern) => {
                let name = relative
                    .file_name()
                    .map(|value| value.to_string_lossy())
                    .unwrap_or_default();
                if wildcard_matches(pattern, &name) {
                    self.report.matches_observed = self.report.matches_observed.saturating_add(1);
                    self.add_match(SearchMatch::Filename(WorkspaceFilenameMatch {
                        path: self.display_path(relative, path),
                    }));
                    if self.report.matches.len() >= self.max_results {
                        self.report
                            .incomplete(WorkspaceSearchStopReason::ResultLimit);
                        self.stop = true;
                    }
                }
            }
            SearchKind::Text(_) => {
                if size > MAX_SEARCH_FILE_BYTES {
                    self.report.oversized_files_skipped =
                        self.report.oversized_files_skipped.saturating_add(1);
                    self.report
                        .incomplete(WorkspaceSearchStopReason::OversizedContent);
                    return;
                }
                if self.report.bytes_read.saturating_add(size) > MAX_SEARCH_TOTAL_BYTES {
                    self.report.incomplete(WorkspaceSearchStopReason::ByteLimit);
                    self.stop = true;
                    return;
                }
                self.report.files_read = self.report.files_read.saturating_add(1);
                let mut file = match self.open_file(path, relative) {
                    Ok(file) => file,
                    Err(_) => {
                        self.report.unreadable_files =
                            self.report.unreadable_files.saturating_add(1);
                        self.report
                            .incomplete(WorkspaceSearchStopReason::UnreadableContent);
                        return;
                    }
                };
                let mut bytes = Vec::with_capacity(size as usize);
                let mut file_bytes_read = 0_u64;
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    if self.cancelled() {
                        return;
                    }
                    let remaining_file_bytes =
                        MAX_SEARCH_FILE_BYTES.saturating_sub(file_bytes_read);
                    let remaining_total_bytes =
                        MAX_SEARCH_TOTAL_BYTES.saturating_sub(self.report.bytes_read);
                    if remaining_file_bytes == 0 {
                        if size > file_bytes_read {
                            self.report.oversized_files_skipped =
                                self.report.oversized_files_skipped.saturating_add(1);
                            self.report
                                .incomplete(WorkspaceSearchStopReason::OversizedContent);
                        }
                        return;
                    }
                    if remaining_total_bytes == 0 {
                        self.report.incomplete(WorkspaceSearchStopReason::ByteLimit);
                        self.stop = true;
                        return;
                    }
                    let read_limit = buffer
                        .len()
                        .min(remaining_file_bytes as usize)
                        .min(remaining_total_bytes as usize);
                    let read = match &mut file {
                        SearchFile::Workspace(file) => file.read(&mut buffer[..read_limit]),
                        SearchFile::Ambient(file) => file.read(&mut buffer[..read_limit]),
                    };
                    let read = match read {
                        Ok(read) => read,
                        Err(_) => {
                            self.report.unreadable_files =
                                self.report.unreadable_files.saturating_add(1);
                            self.report
                                .incomplete(WorkspaceSearchStopReason::UnreadableContent);
                            return;
                        }
                    };
                    if read == 0 {
                        break;
                    }
                    file_bytes_read = file_bytes_read.saturating_add(read as u64);
                    self.report.bytes_read = self.report.bytes_read.saturating_add(read as u64);
                    bytes.extend_from_slice(&buffer[..read]);
                    if file_bytes_read > MAX_SEARCH_FILE_BYTES {
                        self.report.oversized_files_skipped =
                            self.report.oversized_files_skipped.saturating_add(1);
                        self.report
                            .incomplete(WorkspaceSearchStopReason::OversizedContent);
                        return;
                    }
                }
                if bytes.contains(&0) {
                    self.report.binary_files_skipped =
                        self.report.binary_files_skipped.saturating_add(1);
                    return;
                }
                let expected_sha256 = sha256_digest(&bytes);
                let Ok(text) = String::from_utf8(bytes) else {
                    self.report.invalid_files_skipped =
                        self.report.invalid_files_skipped.saturating_add(1);
                    self.report
                        .incomplete(WorkspaceSearchStopReason::InvalidContent);
                    return;
                };
                let lines = text.lines().collect::<Vec<_>>();
                let (before_context_requested, after_context_requested) = match &self.kind {
                    SearchKind::Text(search) => (search.before_context, search.after_context),
                    SearchKind::Filename(_) => unreachable!("text branch"),
                };
                for (index, line) in lines.iter().copied().enumerate() {
                    if self.cancelled() {
                        return;
                    }
                    let matched = match &self.kind {
                        SearchKind::Text(search) => search.matcher.is_match(line),
                        SearchKind::Filename(_) => false,
                    };
                    if matched {
                        let before_start = index.saturating_sub(before_context_requested.into());
                        let before_context = lines[before_start..index]
                            .iter()
                            .enumerate()
                            .map(|(offset, line)| bounded_context_line(before_start + offset, line))
                            .collect::<Vec<_>>();
                        let after_end = lines
                            .len()
                            .min(index.saturating_add(1 + usize::from(after_context_requested)));
                        let after_context = lines[index.saturating_add(1)..after_end]
                            .iter()
                            .enumerate()
                            .map(|(offset, line)| {
                                bounded_context_line(index.saturating_add(1 + offset), line)
                            })
                            .collect::<Vec<_>>();
                        let context_complete = before_context
                            .iter()
                            .chain(&after_context)
                            .all(|line| !line.truncated);
                        let read_start_index =
                            index.saturating_sub(SEARCH_READ_SELECTION_RADIUS_LINES);
                        let read_end_index = lines
                            .len()
                            .min(index.saturating_add(SEARCH_READ_SELECTION_RADIUS_LINES + 1));
                        let display_path = self.display_path(relative, path);
                        self.report.matches_observed =
                            self.report.matches_observed.saturating_add(1);
                        self.add_match(SearchMatch::Text(WorkspaceTextMatch {
                            path: display_path.clone(),
                            line_number: index.saturating_add(1).try_into().unwrap_or(u64::MAX),
                            line: truncate_utf8(line, MAX_SEARCH_LINE_BYTES),
                            line_truncated: line.len() > MAX_SEARCH_LINE_BYTES,
                            read_selection: WorkspaceTextReadSelection {
                                path: display_path,
                                start_line: read_start_index
                                    .saturating_add(1)
                                    .try_into()
                                    .unwrap_or(u64::MAX),
                                line_count: (read_end_index - read_start_index)
                                    .try_into()
                                    .expect("fixed search read window fits u16"),
                                expected_sha256: expected_sha256.clone(),
                            },
                            before_context_requested,
                            after_context_requested,
                            before_context,
                            after_context,
                            context_complete,
                        }));
                        if self.report.matches.len() >= self.max_results {
                            self.report
                                .incomplete(WorkspaceSearchStopReason::ResultLimit);
                            self.stop = true;
                            return;
                        }
                    }
                }
            }
        }
    }

    fn add_match(&mut self, item: SearchMatch) {
        if self.report.matches.len() < self.max_results {
            self.report.matches.push(item);
        }
    }

    fn open_file(&self, path: &Path, relative: &Path) -> Result<SearchFile, WorkspaceError> {
        if let Some(broker) = self.broker {
            let (_, file) = broker
                .open_file_for_read(relative)
                .map_err(WorkspaceError::Security)?;
            Ok(SearchFile::Workspace(file))
        } else {
            File::open(path)
                .map(SearchFile::Ambient)
                .map_err(|source| WorkspaceError::Io {
                    path: path.to_path_buf(),
                    source,
                })
        }
    }

    fn workspace_relative(&self, path: &Path) -> Result<PathBuf, WorkspaceError> {
        if let Some(root) = self.workspace_root {
            return path.strip_prefix(root).map(Path::to_path_buf).map_err(|_| {
                WorkspaceError::Security(argentum_security::SecurityError::OutsideWorkspace(
                    path.to_path_buf(),
                ))
            });
        }
        Ok(path.to_path_buf())
    }

    fn display_path(&self, relative: &Path, absolute: &Path) -> String {
        if self.workspace_root.is_some() {
            normalize_display_path(relative)
        } else {
            absolute.to_string_lossy().into_owned()
        }
    }

    fn policy_ignored(&self, relative: Option<&Path>) -> bool {
        relative.is_some_and(|path| {
            path.components().any(|component| {
                matches!(component, std::path::Component::Normal(value) if value == ".git" || value == "target")
            })
        })
    }

    fn ignored(&self, path: &Path, is_directory: bool) -> bool {
        let mut ignored = false;
        for rule in &self.rules {
            if rule.matches(path, is_directory) {
                ignored = !rule.negated;
            }
        }
        ignored
    }

    fn cancelled(&mut self) -> bool {
        if (self.cancelled)() {
            self.report.incomplete(WorkspaceSearchStopReason::Cancelled);
            self.stop = true;
            true
        } else {
            false
        }
    }

    fn into_text_report(self) -> WorkspaceSearchReport<WorkspaceTextMatch> {
        self.report.map_matches(|item| match item {
            SearchMatch::Text(item) => Some(item),
            SearchMatch::Filename(_) => None,
        })
    }

    fn into_filename_report(self) -> WorkspaceSearchReport<WorkspaceFilenameMatch> {
        self.report.map_matches(|item| match item {
            SearchMatch::Filename(item) => Some(item),
            SearchMatch::Text(_) => None,
        })
    }
}

fn unsupported_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use argentum_security::{ApprovalPolicy, CapabilityBroker};

    use super::*;
    use crate::WorkspaceManager;

    fn manager(root: &Path) -> WorkspaceManager {
        let broker = CapabilityBroker::new(root, ApprovalPolicy::default()).expect("search broker");
        WorkspaceManager::new(broker)
    }

    #[test]
    fn ignore_rules_prune_directories_and_preserve_policy_exclusions() {
        let fixture = tempfile::tempdir().expect("search fixture");
        let root = fixture.path();
        fs::create_dir_all(root.join("ignored/deep")).expect("ignored tree");
        fs::create_dir_all(root.join("nested/local")).expect("nested ignore tree");
        fs::create_dir_all(root.join(".git")).expect("git exclusion");
        fs::create_dir_all(root.join("target")).expect("target exclusion");
        fs::write(
            root.join(".gitignore"),
            "ignored/\n*.secret\n!keep.secret\n",
        )
        .expect("root ignore");
        fs::write(root.join("nested/.gitignore"), "local/\n").expect("nested ignore");
        fs::write(root.join("visible.txt"), "needle\r\nquoted \"value\"\r\n")
            .expect("visible text");
        fs::write(root.join("keep.secret"), "needle\n").expect("negated file");
        fs::write(root.join("drop.secret"), "needle\n").expect("ignored file");
        fs::write(root.join("ignored/deep/hidden.txt"), "needle\n").expect("hidden text");
        fs::write(root.join("nested/allowed.txt"), "nested needle\n").expect("nested text");
        fs::write(root.join("nested/local/hidden.txt"), "needle\n").expect("local hidden text");
        fs::write(root.join(".git/index"), "needle\n").expect("git canary");
        fs::write(root.join("target/output.txt"), "needle\n").expect("target canary");

        let workspace = manager(root);
        let text = workspace
            .search_text_bounded_report(".", "needle", 16)
            .expect("bounded ignored text search");
        let paths = text
            .matches
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>();
        assert!(text.complete);
        assert_eq!(text.stop_reason, WorkspaceSearchStopReason::Complete);
        assert!(paths.contains(&"visible.txt"));
        assert!(paths.contains(&"keep.secret"));
        assert!(paths.contains(&"nested/allowed.txt"));
        assert!(!paths
            .iter()
            .any(|path| path.contains("ignored") || path.contains("hidden")));
        assert!(text.ignored_directories_pruned >= 2);
        assert!(text.policy_directories_pruned >= 2);
        assert!(text.ignore_files_loaded >= 2);
        assert_eq!(text.matches_observed, 3);

        let names = workspace
            .search_filenames_bounded_report(".", "*.txt", 16)
            .expect("bounded ignored filename search");
        assert!(names.complete);
        assert_eq!(names.matches_observed, 2);
        assert_eq!(
            names
                .matches
                .iter()
                .map(|item| item.path.as_str())
                .collect::<Vec<_>>(),
            ["nested/allowed.txt", "visible.txt"]
        );
    }

    #[test]
    fn result_and_content_bounds_report_truthful_incompleteness() {
        let fixture = tempfile::tempdir().expect("search bounds fixture");
        let root = fixture.path();
        fs::write(root.join("one.txt"), "needle\n").expect("first result");
        fs::write(root.join("two.txt"), "needle\n").expect("second result");
        fs::write(
            root.join("long-line.txt"),
            format!("{}needle\n", "x".repeat(8 * 1024)),
        )
        .expect("long line");
        fs::write(root.join("invalid.txt"), [0xff, 0xfe, 0xfd]).expect("invalid text");
        fs::write(
            root.join("binary.bin"),
            [b'n', b'e', 0, b'e', b'd', b'l', b'e'],
        )
        .expect("binary text");
        fs::write(
            root.join("oversized.txt"),
            vec![b'x'; MAX_SEARCH_FILE_BYTES as usize + 1],
        )
        .expect("oversized text");

        let limited_fixture = tempfile::tempdir().expect("result limit fixture");
        fs::write(limited_fixture.path().join("one.txt"), "needle\n").expect("limited first");
        fs::write(limited_fixture.path().join("two.txt"), "needle\n").expect("limited second");
        let limited = manager(limited_fixture.path())
            .search_text_bounded_report(".", "needle", 1)
            .expect("result-limited search");
        assert_eq!(limited.matches.len(), 1);
        assert!(!limited.complete);
        assert_eq!(limited.stop_reason, WorkspaceSearchStopReason::ResultLimit);

        let workspace = manager(root);
        let full = workspace
            .search_text_bounded_report(".", "needle", 16)
            .expect("content-bound search");
        assert!(!full.complete);
        assert!(full.invalid_files_skipped >= 1);
        assert!(full.binary_files_skipped >= 1);
        assert!(full.oversized_files_skipped >= 1);
        let long_line = full
            .matches
            .iter()
            .find(|item| item.path == "long-line.txt")
            .expect("long line match");
        assert!(long_line.line.len() <= MAX_SEARCH_LINE_BYTES);
    }

    #[test]
    fn malformed_ignore_source_and_cancellation_are_explicit() {
        let fixture = tempfile::tempdir().expect("search cancellation fixture");
        let root = fixture.path();
        fs::write(root.join(".gitignore"), [0xff, 0xfe]).expect("malformed ignore");
        for index in 0..16 {
            fs::write(root.join(format!("file-{index}.txt")), "needle\n").expect("search file");
        }
        let workspace = manager(root);
        let invalid_ignore = workspace
            .search_text_bounded_report(".", "needle", 16)
            .expect("malformed ignore search");
        assert!(!invalid_ignore.complete);
        assert_eq!(
            invalid_ignore.stop_reason,
            WorkspaceSearchStopReason::InvalidIgnoreSource
        );

        let oversized_fixture = tempfile::tempdir().expect("oversized ignore fixture");
        fs::write(
            oversized_fixture.path().join(".gitignore"),
            vec![b'#'; MAX_IGNORE_FILE_BYTES as usize + 1],
        )
        .expect("oversized ignore source");
        fs::write(oversized_fixture.path().join("visible.txt"), "needle\n")
            .expect("oversized ignore visible file");
        let oversized = manager(oversized_fixture.path())
            .search_text_bounded_report(".", "needle", 16)
            .expect("oversized ignore search");
        assert!(!oversized.complete);
        assert_eq!(
            oversized.stop_reason,
            WorkspaceSearchStopReason::OversizedIgnoreSource
        );
        assert_eq!(oversized.ignore_files_loaded, 0);
        assert!(oversized
            .incomplete_reasons
            .iter()
            .any(|reason| reason == "oversized_ignore_source"));

        let cancellation_fixture = tempfile::tempdir().expect("cancellation-only fixture");
        for index in 0..16 {
            fs::write(
                cancellation_fixture
                    .path()
                    .join(format!("file-{index}.txt")),
                "needle\n",
            )
            .expect("cancellation file");
        }
        let checks = Cell::new(0_u8);
        let cancelled = manager(cancellation_fixture.path())
            .search_text_bounded_report_with_cancellation(".", "needle", 16, || {
                let next = checks.get().saturating_add(1);
                checks.set(next);
                next > 4
            })
            .expect("cancelled search");
        assert!(!cancelled.complete);
        assert_eq!(cancelled.stop_reason, WorkspaceSearchStopReason::Cancelled);
        assert!(cancelled
            .incomplete_reasons
            .iter()
            .any(|reason| reason == "cancelled"));
    }

    fn regex_options(before_context: u8, after_context: u8) -> WorkspaceTextSearchOptions {
        WorkspaceTextSearchOptions {
            mode: TextSearchMode::Regex,
            before_context,
            after_context,
        }
    }

    #[test]
    fn regex_is_explicit_bounded_and_never_falls_back_to_literal() {
        let fixture = tempfile::tempdir().expect("regex mode fixture");
        fs::write(fixture.path().join("values.txt"), "value a.b\nvalue axb\n")
            .expect("regex mode source");
        let workspace = manager(fixture.path());

        let literal = workspace
            .search_text_bounded_report(".", "a.b", 8)
            .expect("default literal punctuation search");
        assert_eq!(literal.matches_observed, 1);
        assert_eq!(literal.matches[0].line_number, 1);
        assert!(literal.matches[0].before_context.is_empty());
        assert!(literal.matches[0].after_context.is_empty());

        let regex = workspace
            .search_text_bounded_report_with_options(".", "a.b", 8, regex_options(0, 0))
            .expect("explicit regex punctuation search");
        assert_eq!(regex.matches_observed, 2);
        assert_eq!(
            regex
                .matches
                .iter()
                .map(|item| item.line_number)
                .collect::<Vec<_>>(),
            [1, 2]
        );

        for pattern in ["(", "(?=value)", "(?:(?:a){1000}){1000}"] {
            let checks = Cell::new(0_u8);
            let error = workspace
                .search_text_bounded_report_with_options_and_cancellation(
                    ".",
                    pattern,
                    8,
                    regex_options(0, 0),
                    || {
                        checks.set(checks.get().saturating_add(1));
                        false
                    },
                )
                .expect_err("invalid or oversized regex");
            assert!(matches!(error, WorkspaceError::InvalidText(_)));
            assert_eq!(checks.get(), 1, "invalid regex traversed workspace");
        }

        let cancelled = workspace
            .search_text_bounded_report_with_options_and_cancellation(
                ".",
                "(",
                8,
                regex_options(0, 0),
                || true,
            )
            .expect("cancellation wins before regex compilation");
        assert!(!cancelled.complete);
        assert_eq!(cancelled.stop_reason, WorkspaceSearchStopReason::Cancelled);
        assert_eq!(cancelled.entries_visited, 0);
        assert_eq!(cancelled.files_read, 0);
    }

    #[test]
    fn regex_context_preserves_line_roles_boundaries_crlf_and_unicode() {
        let fixture = tempfile::tempdir().expect("regex context fixture");
        let root = fixture.path();
        fs::write(
            root.join("context.txt"),
            "начало\r\ne\u{301} and wide 界\r\ntarget_one\r\ntarget_two\r\nконец",
        )
        .expect("context source");
        fs::write(root.join("empty.txt"), "").expect("empty source");
        fs::write(root.join("blank.txt"), "\n").expect("blank source");
        let workspace = manager(root);

        let adjacent = workspace
            .search_text_bounded_report_with_options(
                "context.txt",
                r"^target_(?:one|two)$",
                8,
                regex_options(2, 2),
            )
            .expect("adjacent regex context");
        assert!(adjacent.complete);
        assert_eq!(adjacent.matches_observed, 2);
        assert_eq!(adjacent.matches.len(), 2);
        assert_eq!(adjacent.matches[0].line_number, 3);
        assert_eq!(adjacent.matches[1].line_number, 4);
        assert_eq!(
            adjacent.matches[0]
                .before_context
                .iter()
                .map(|line| line.line_number)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            adjacent.matches[0]
                .after_context
                .iter()
                .map(|line| line.line_number)
                .collect::<Vec<_>>(),
            [4, 5]
        );
        assert_eq!(adjacent.matches[0].after_context[1].line, "конец");
        assert!(adjacent.matches.iter().all(|item| item.context_complete));
        assert!(adjacent
            .matches
            .iter()
            .flat_map(|item| item.before_context.iter().chain(&item.after_context))
            .all(|line| !line.line.contains('\r')));

        let boundaries = workspace
            .search_text_bounded_report_with_options(
                "context.txt",
                r"^(?:начало|конец)$",
                8,
                regex_options(2, 2),
            )
            .expect("file boundary context");
        assert_eq!(boundaries.matches.len(), 2);
        assert!(boundaries.matches[0].before_context.is_empty());
        assert_eq!(boundaries.matches[0].after_context.len(), 2);
        assert_eq!(boundaries.matches[1].before_context.len(), 2);
        assert!(boundaries.matches[1].after_context.is_empty());
        assert!(boundaries.matches.iter().all(|item| item.context_complete));

        let zero_width = workspace
            .search_text_bounded_report_with_options("context.txt", r"^", 8, regex_options(0, 0))
            .expect("zero-width line search");
        assert_eq!(zero_width.matches_observed, 5);
        let empty = workspace
            .search_text_bounded_report_with_options("empty.txt", r"^", 8, regex_options(0, 0))
            .expect("empty file search");
        assert!(empty.complete);
        assert!(empty.matches.is_empty());
        let blank = workspace
            .search_text_bounded_report_with_options("blank.txt", r"^$", 8, regex_options(0, 0))
            .expect("blank logical line search");
        assert_eq!(blank.matches_observed, 1);
    }

    #[test]
    fn context_snippet_truncation_and_regex_cancellation_are_truthful() {
        let fixture = tempfile::tempdir().expect("regex truncation fixture");
        let root = fixture.path();
        fs::write(
            root.join("long.txt"),
            format!("{}\nMATCH\n{}", "я".repeat(3_000), "z".repeat(5_000)),
        )
        .expect("long context source");
        for index in 0..16 {
            fs::write(
                root.join(format!("scan-{index}.txt")),
                "first\nMATCH\nlast\n",
            )
            .expect("regex cancellation source");
        }
        let workspace = manager(root);
        let report = workspace
            .search_text_bounded_report_with_options("long.txt", r"^MATCH$", 8, regex_options(1, 1))
            .expect("truncated context search");
        assert_eq!(report.matches.len(), 1);
        assert!(!report.matches[0].context_complete);
        assert!(report.matches[0].before_context[0].truncated);
        assert!(report.matches[0].after_context[0].truncated);
        assert!(report.matches[0].before_context[0].line.len() <= MAX_SEARCH_LINE_BYTES);
        assert!(report.matches[0].after_context[0].line.len() <= MAX_SEARCH_LINE_BYTES);

        let checks = Cell::new(0_u8);
        let cancelled = workspace
            .search_text_bounded_report_with_options_and_cancellation(
                ".",
                r"^MATCH$",
                100,
                regex_options(1, 1),
                || {
                    let next = checks.get().saturating_add(1);
                    checks.set(next);
                    next > 8
                },
            )
            .expect("cancelled regex scan");
        assert!(!cancelled.complete);
        assert_eq!(cancelled.stop_reason, WorkspaceSearchStopReason::Cancelled);
        assert!(cancelled.entries_visited > 0 || cancelled.files_read > 0);
    }
}
