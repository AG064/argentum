use std::path::{Component, Path};

use argentum_domain::{
    FileChangeKind, FileSnapshotRef, LineCountAvailability, Sha256Digest, UnifiedDiffSnapshot,
};
use sha2::{Digest, Sha256};
use similar::TextDiff;
use thiserror::Error;

pub const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
pub const MAX_RUN_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const SNAPSHOT_TOO_LARGE_REASON: &str = "Snapshot was too large to retain safely";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ChangeError {
    #[error("path is outside the workspace boundary")]
    InvalidPath,
    #[error("change must have a before or after image")]
    EmptyChange,
    #[error("file content changed after approval")]
    StalePrecondition,
    #[error("Binary content cannot be displayed as a unified diff")]
    BinaryDiffUnavailable,
    #[error("{SNAPSHOT_TOO_LARGE_REASON}")]
    SnapshotTooLarge,
    #[error("diff byte limit must be greater than zero")]
    InvalidDiffLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedSnapshot {
    pub reference: FileSnapshotRef,
    pub bytes: Option<Vec<u8>>,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedChange {
    pub relative_path: String,
    pub kind: FileChangeKind,
    pub before: Option<CapturedSnapshot>,
    pub after: Option<CapturedSnapshot>,
    pub additions: u32,
    pub removals: u32,
    /// Provenance of `additions`/`removals`, derived at preparation from the
    /// same admitted snapshots used for the counts.
    pub line_count_availability: LineCountAvailability,
}

pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    let digest = Sha256::digest(bytes);
    Sha256Digest(format!("{digest:x}"))
}

pub fn capture(bytes: &[u8], limit_bytes: usize) -> CapturedSnapshot {
    let retained = bytes.len() <= limit_bytes;
    CapturedSnapshot {
        reference: FileSnapshotRef {
            digest: sha256(bytes),
            size_bytes: bytes.len().try_into().unwrap_or(u64::MAX),
            retained,
        },
        bytes: retained.then(|| bytes.to_vec()),
        binary: std::str::from_utf8(bytes).is_err() || bytes.contains(&0),
    }
}

pub fn normalize_relative_path(path: &str) -> Result<String, ChangeError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ChangeError::InvalidPath);
    }
    let normalized = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        Err(ChangeError::InvalidPath)
    } else {
        Ok(normalized)
    }
}

pub fn prepare_change(
    relative_path: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
    snapshot_limit: usize,
) -> Result<PreparedChange, ChangeError> {
    let relative_path = normalize_relative_path(relative_path)?;
    let kind = match (before, after) {
        (None, Some(_)) => FileChangeKind::Added,
        (Some(_), Some(_)) => FileChangeKind::Modified,
        (Some(_), None) => FileChangeKind::Deleted,
        (None, None) => return Err(ChangeError::EmptyChange),
    };
    let before = before.map(|bytes| capture(bytes, snapshot_limit));
    let after = after.map(|bytes| capture(bytes, snapshot_limit));
    let (additions, removals, line_count_availability) =
        line_counts_with_availability(before.as_ref(), after.as_ref());
    Ok(PreparedChange {
        relative_path,
        kind,
        before,
        after,
        additions,
        removals,
        line_count_availability,
    })
}

pub fn verify_precondition(
    expected: &Option<CapturedSnapshot>,
    current: Option<&[u8]>,
) -> Result<(), ChangeError> {
    let matches = match (expected, current) {
        (None, None) => true,
        (Some(expected), Some(current)) => expected.reference.digest == sha256(current),
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(ChangeError::StalePrecondition)
    }
}

pub fn load_unified_diff(
    change: &PreparedChange,
    limit_bytes: usize,
) -> Result<UnifiedDiffSnapshot, ChangeError> {
    if limit_bytes == 0 {
        return Err(ChangeError::InvalidDiffLimit);
    }
    let before = retained_text(change.before.as_ref())?;
    let after = retained_text(change.after.as_ref())?;
    let old_path = if change.kind == FileChangeKind::Added {
        "/dev/null"
    } else {
        change.relative_path.as_str()
    };
    let new_path = if change.kind == FileChangeKind::Deleted {
        "/dev/null"
    } else {
        change.relative_path.as_str()
    };
    let full = TextDiff::configure()
        .timeout(std::time::Duration::from_millis(50))
        .diff_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header(old_path, new_path)
        .to_string();
    let (text, truncated) = truncate_utf8(&full, limit_bytes);
    Ok(UnifiedDiffSnapshot {
        file_change_id: Default::default(),
        relative_path: change.relative_path.clone(),
        line_count: text.lines().count().try_into().unwrap_or(u32::MAX),
        byte_count: text.len().try_into().unwrap_or(u64::MAX),
        text,
        truncated,
        available: true,
        unavailable_reason: String::new(),
    })
}

fn retained_text(snapshot: Option<&CapturedSnapshot>) -> Result<&str, ChangeError> {
    let Some(snapshot) = snapshot else {
        return Ok("");
    };
    let bytes = snapshot
        .bytes
        .as_deref()
        .ok_or(ChangeError::SnapshotTooLarge)?;
    if snapshot.binary {
        return Err(ChangeError::BinaryDiffUnavailable);
    }
    std::str::from_utf8(bytes).map_err(|_| ChangeError::BinaryDiffUnavailable)
}

/// Text view of an image used for line counting.
///
/// A genuinely absent image counts as the empty string, so creation, deletion
/// and empty-image changes keep their exact text counts. An image that exists
/// but cannot be read as text (bytes not retained, invalid UTF-8 or
/// NUL-containing binary) yields `None`, which the caller reports as the
/// conservative `(0, 0)` convention instead of a false whole-file change.
fn counted_text(snapshot: Option<&CapturedSnapshot>) -> Option<&str> {
    let Some(snapshot) = snapshot else {
        return Some("");
    };
    let bytes = snapshot.bytes.as_deref()?;
    if snapshot.binary {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

#[cfg(test)]
fn line_counts(before: Option<&CapturedSnapshot>, after: Option<&CapturedSnapshot>) -> (u32, u32) {
    let (additions, removals, _) = line_counts_with_availability(before, after);
    (additions, removals)
}

/// Count lines and report the provenance of those counts from the same
/// admitted snapshots.
///
/// The numeric results keep the REAL02 convention exactly: a genuine absence
/// is the empty image and an unreadable existing image falls back to
/// `(0, 0)`. The availability state distinguishes the two cases that share
/// `(0, 0)`:
///
/// * Both sides readable (including a real absence and a known empty image)
///   is `Computed`, and a genuine zero diff stays `Computed`.
/// * An existing image that cannot be read as text (bytes not retained,
///   NUL-containing binary or invalid UTF-8) is `Unavailable`.
///
/// `Unknown` is never produced here. It only describes records whose
/// provenance was not recorded at all.
pub fn line_counts_with_availability(
    before: Option<&CapturedSnapshot>,
    after: Option<&CapturedSnapshot>,
) -> (u32, u32, LineCountAvailability) {
    let (Some(before), Some(after)) = (counted_text(before), counted_text(after)) else {
        return (0, 0, LineCountAvailability::Unavailable);
    };
    let (additions, removals) = count_lines(before, after);
    (additions, removals, LineCountAvailability::Computed)
}

fn count_lines(before: &str, after: &str) -> (u32, u32) {
    let diff = TextDiff::from_lines(before, after);
    let additions = diff
        .iter_all_changes()
        .filter(|change| change.tag() == similar::ChangeTag::Insert)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let removals = diff
        .iter_all_changes()
        .filter(|change| change.tag() == similar::ChangeTag::Delete)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    (additions, removals)
}

fn truncate_utf8(value: &str, limit: usize) -> (String, bool) {
    if value.len() <= limit {
        return (value.to_owned(), false);
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: usize = 32;

    #[test]
    fn classifies_added_modified_and_deleted_content() {
        let added = prepare_change("new.txt", None, Some(b"one\ntwo\n"), LIMIT).unwrap();
        let modified =
            prepare_change("edit.txt", Some(b"one\n"), Some(b"one\ntwo\n"), LIMIT).unwrap();
        let deleted = prepare_change("old.txt", Some(b"one\n"), None, LIMIT).unwrap();

        assert_eq!(added.kind, argentum_domain::FileChangeKind::Added);
        assert_eq!((added.additions, added.removals), (2, 0));
        assert_eq!(modified.kind, argentum_domain::FileChangeKind::Modified);
        assert_eq!((modified.additions, modified.removals), (1, 0));
        assert_eq!(deleted.kind, argentum_domain::FileChangeKind::Deleted);
        assert_eq!((deleted.additions, deleted.removals), (0, 1));
    }

    #[test]
    fn binary_and_oversized_snapshots_report_truthful_diff_unavailability() {
        let binary = prepare_change("image.bin", None, Some(&[0, 159, 146, 150]), LIMIT).unwrap();
        let oversized = prepare_change("large.txt", None, Some(&[b'x'; LIMIT + 1]), LIMIT).unwrap();

        assert_eq!(
            load_unified_diff(&binary, 4096).unwrap_err().to_string(),
            "Binary content cannot be displayed as a unified diff"
        );
        assert_eq!(
            load_unified_diff(&oversized, 4096).unwrap_err().to_string(),
            "Snapshot was too large to retain safely"
        );
    }

    #[test]
    fn unified_diff_preserves_missing_newline() {
        let change = prepare_change("note.txt", Some(b"old"), Some(b"new"), LIMIT).unwrap();
        let diff = load_unified_diff(&change, 4096).unwrap();

        assert!(diff.text.contains("\\ No newline at end of file"));
        assert!(!diff.truncated);
    }

    #[test]
    fn unified_diff_marks_byte_bounded_truncation() {
        let change = prepare_change(
            "note.txt",
            Some(b"old line one\nold line two\n"),
            Some(b"new line one\nnew line two\n"),
            LIMIT,
        )
        .unwrap();
        let diff = load_unified_diff(&change, 24).unwrap();

        assert!(diff.truncated);
        assert!(diff.byte_count <= 24);
    }

    #[test]
    fn write_and_restore_preconditions_reject_stale_content() {
        let prepared = prepare_change("note.txt", Some(b"before"), Some(b"after"), LIMIT).unwrap();

        assert!(verify_precondition(&prepared.before, Some(b"changed")).is_err());
        assert!(verify_precondition(&prepared.after, Some(b"changed")).is_err());
        assert!(verify_precondition(&prepared.after, Some(b"after")).is_ok());
    }

    fn multibyte_change(path: &str, before: &str, after: &str) -> PreparedChange {
        prepare_change(path, Some(before.as_bytes()), Some(after.as_bytes()), LIMIT).unwrap()
    }

    // Independently derive the longest prefix of `source` that is valid UTF-8
    // and fits within `cap` bytes. This uses character iterators rather than
    // reproducing the production truncation helper's backwards scan.
    fn greatest_fitting_prefix(source: &str, cap: usize) -> &str {
        let mut end = 0;
        for (offset, character) in source.char_indices() {
            let next = offset + character.len_utf8();
            if next > cap {
                break;
            }
            end = next;
        }
        &source[..end]
    }

    #[test]
    fn truncation_preserves_valid_utf8_across_two_three_and_four_byte_chars() {
        // Path and diff body each mix 2-byte (é, U+00E9), 3-byte (ソ, U+30BD)
        // and 4-byte (𝄞, U+1D11E) sequences, so a byte cap can land mid-character.
        let path = "éソ𝄞-note.txt";
        let before = "café ナ ソ line\n";
        let after = "café ナ ソ 𝄞 line\n";
        let change = multibyte_change(path, before, after);
        let full = load_unified_diff(&change, 4096).unwrap();

        assert!(full.text.is_char_boundary(full.text.len()));
        assert!(std::str::from_utf8(full.text.as_bytes()).is_ok());
        assert_eq!(full.byte_count, full.text.len() as u64);
        assert_eq!(full.line_count as usize, full.text.lines().count());
        assert!(!full.truncated);

        // Exercise every positive cap across the entire rendered diff, including
        // the body, wherever the multibyte characters fall.
        for cap in 1..=full.text.len() {
            let limited = load_unified_diff(&change, cap).unwrap();
            let text = limited.text.as_str();
            let expected = greatest_fitting_prefix(&full.text, cap);

            assert!(
                std::str::from_utf8(text.as_bytes()).is_ok(),
                "cap {cap} broke UTF-8"
            );
            assert_eq!(
                text, expected,
                "cap {cap} was not the greatest fitting prefix"
            );
            assert!(
                limited.byte_count <= cap as u64,
                "cap {cap} exceeded the byte cap"
            );
            assert_eq!(limited.byte_count, text.len() as u64);
            assert_eq!(limited.line_count as usize, text.lines().count());
            assert!(
                full.text.starts_with(text),
                "cap {cap} was not a prefix of the full diff"
            );
            assert_eq!(limited.truncated, text.len() < full.text.len());
        }
    }

    #[test]
    fn caps_at_and_above_full_length_are_not_truncated() {
        let change = multibyte_change("ソース.txt", "café\n", "café 𝄞\n");
        let full = load_unified_diff(&change, 4096).unwrap();
        let full_len = full.byte_count;
        let cap = usize::try_from(full_len).unwrap();

        let exact = load_unified_diff(&change, cap).unwrap();
        assert!(!exact.truncated);
        assert_eq!(exact.text, full.text);
        assert_eq!(exact.byte_count, full_len);
        assert_eq!(exact.line_count, full.line_count);

        let larger = load_unified_diff(&change, cap + 512).unwrap();
        assert!(!larger.truncated);
        assert_eq!(larger.text, full.text);
        assert_eq!(larger.byte_count, full_len);
        assert_eq!(larger.line_count, full.line_count);
    }

    #[test]
    fn zero_cap_is_rejected_as_invalid_diff_limit() {
        let change = multibyte_change("note.txt", "a\n", "b\n");

        let error = load_unified_diff(&change, 0).unwrap_err();
        assert!(matches!(error, ChangeError::InvalidDiffLimit));
    }

    // Recompute line counts from the current snapshots on a prepared change.
    // The stored fields are computed once at prepare time, so these tests must
    // re-run production counting against snapshots whose bytes were dropped.
    fn count_now(change: &PreparedChange) -> (u32, u32) {
        line_counts(change.before.as_ref(), change.after.as_ref())
    }

    fn unretain(snapshot: &mut Option<CapturedSnapshot>) {
        let snapshot = snapshot.as_mut().expect("snapshot must exist");
        snapshot.bytes = None;
        snapshot.reference.retained = false;
    }

    // An unretained image exists but its bytes are gone. Line counts must fall
    // back to the conservative (0, 0) convention instead of pretending the file
    // was created or deleted in full.
    #[test]
    fn unretained_modification_reports_conservative_counts() {
        let mut change = prepare_change(
            "note.txt",
            Some(b"one\ntwo\nthree\n"),
            Some(b"one\ntwo\nthree\nfour\n"),
            LIMIT,
        )
        .unwrap();
        assert_eq!(change.kind, argentum_domain::FileChangeKind::Modified);
        assert_eq!(
            count_now(&change),
            (1, 0),
            "readable sides must keep exact counts"
        );

        unretain(&mut change.before);
        assert_eq!(
            count_now(&change),
            (0, 0),
            "unretained before must not be counted as a whole-file addition"
        );

        let mut removed_after = change.clone();
        removed_after.before = prepare_change(
            "note.txt",
            Some(b"one\ntwo\nthree\n"),
            Some(b"one\ntwo\nthree\nfour\n"),
            LIMIT,
        )
        .unwrap()
        .before;
        unretain(&mut removed_after.after);
        assert_eq!(
            count_now(&removed_after),
            (0, 0),
            "unretained after must not be counted as a whole-file removal"
        );
    }

    // Both sides can be unretained at once, for example when a large binary was
    // replaced by another large binary.
    #[test]
    fn unretained_change_with_both_sides_gone_reports_conservative_counts() {
        let oversized = [b'x'; LIMIT + 1];
        let change = prepare_change("big.txt", Some(&oversized), Some(&oversized), LIMIT).unwrap();
        assert_eq!(change.kind, argentum_domain::FileChangeKind::Modified);
        assert!(change.before.as_ref().unwrap().bytes.is_none());
        assert!(change.after.as_ref().unwrap().bytes.is_none());
        assert_eq!(count_now(&change), (0, 0));
    }

    // NUL-containing content is binary even when it is valid UTF-8, and invalid
    // UTF-8 is binary on either side.
    #[test]
    fn nul_and_invalid_utf8_images_report_conservative_counts_on_either_side() {
        let nul_after = prepare_change(
            "blob.txt",
            Some(b"one\ntwo\n"),
            Some(b"one\ntwo\0three\n"),
            LIMIT,
        )
        .unwrap();
        assert!(nul_after.after.as_ref().unwrap().binary);
        assert_eq!((nul_after.additions, nul_after.removals), (0, 0));

        let nul_before = prepare_change(
            "blob.txt",
            Some(b"one\ntwo\0three\n"),
            Some(b"one\ntwo\n"),
            LIMIT,
        )
        .unwrap();
        assert_eq!((nul_before.additions, nul_before.removals), (0, 0));

        let invalid_after = prepare_change(
            "image.bin",
            Some(b"one\n"),
            Some(&[0x6f, 0x6e, 0x65, 0x9f]),
            LIMIT,
        )
        .unwrap();
        assert_eq!((invalid_after.additions, invalid_after.removals), (0, 0));

        let invalid_before = prepare_change(
            "image.bin",
            Some(&[0x6f, 0x6e, 0x65, 0x9f]),
            Some(b"one\n"),
            LIMIT,
        )
        .unwrap();
        assert_eq!((invalid_before.additions, invalid_before.removals), (0, 0));
    }

    // Real absence is not the same as an unreadable image: the missing side is
    // the empty image and the existing side keeps its exact text counts.
    #[test]
    fn actual_absence_keeps_exact_text_counts() {
        let added = prepare_change("new.txt", None, Some(b"one\ntwo\n"), LIMIT).unwrap();
        assert_eq!(added.kind, argentum_domain::FileChangeKind::Added);
        assert_eq!((added.additions, added.removals), (2, 0));
        assert_eq!(count_now(&added), (2, 0));

        let deleted = prepare_change("old.txt", Some(b"one\ntwo\n"), None, LIMIT).unwrap();
        assert_eq!(deleted.kind, argentum_domain::FileChangeKind::Deleted);
        assert_eq!((deleted.additions, deleted.removals), (0, 2));
        assert_eq!(count_now(&deleted), (0, 2));

        // The same deletion with the remaining image unretained is unknown, not a
        // whole-file removal.
        let mut unknown = prepare_change("old.txt", Some(b"one\ntwo\n"), None, LIMIT).unwrap();
        unretain(&mut unknown.before);
        assert_eq!(count_now(&unknown), (0, 0));
    }

    // Known empty images stay readable and keep their exact counts.
    #[test]
    fn known_empty_images_keep_exact_text_counts() {
        let empty_to_empty = prepare_change("empty.txt", Some(b""), Some(b""), LIMIT).unwrap();
        assert_eq!(
            empty_to_empty.kind,
            argentum_domain::FileChangeKind::Modified
        );
        assert_eq!((empty_to_empty.additions, empty_to_empty.removals), (0, 0));
        assert_eq!(count_now(&empty_to_empty), (0, 0));

        let empty_created = prepare_change("empty.txt", None, Some(b""), LIMIT).unwrap();
        assert_eq!((empty_created.additions, empty_created.removals), (0, 0));

        let from_empty = prepare_change("empty.txt", Some(b""), Some(b"one\n"), LIMIT).unwrap();
        assert_eq!((from_empty.additions, from_empty.removals), (1, 0));
        assert_eq!(count_now(&from_empty), (1, 0));

        let to_empty = prepare_change("empty.txt", Some(b"one\n"), Some(b""), LIMIT).unwrap();
        assert_eq!((to_empty.additions, to_empty.removals), (0, 1));
        assert_eq!(count_now(&to_empty), (0, 1));
    }

    // Ordinary readable text keeps the existing exact counts.
    #[test]
    fn readable_text_keeps_exact_line_counts() {
        let modified = prepare_change(
            "note.txt",
            Some(b"keep\nold\nkeep\n"),
            Some(b"keep\nnew\nkeep\n"),
            LIMIT,
        )
        .unwrap();
        assert_eq!((modified.additions, modified.removals), (1, 1));
        assert_eq!(count_now(&modified), (1, 1));
    }

    // Natural one-sided unretention through the public API, with no manual
    // mutation: a snapshot limit smaller than one image keeps that image
    // unretained while the other stays retained. The returned counts must be
    // the conservative (0, 0) convention, not a whole-file change.
    #[test]
    fn public_api_unretained_before_keeps_conservative_counts() {
        let before = b"one\ntwo\nthree\n";
        let after = b"one\ntwo\n";
        let change = prepare_change("note.txt", Some(before), Some(after), after.len()).unwrap();

        assert_eq!(change.kind, argentum_domain::FileChangeKind::Modified);
        let before_snapshot = change.before.as_ref().unwrap();
        assert!(!before_snapshot.reference.retained);
        assert!(before_snapshot.bytes.is_none());
        assert_eq!(before_snapshot.reference.size_bytes, before.len() as u64);
        let after_snapshot = change.after.as_ref().unwrap();
        assert!(after_snapshot.reference.retained);
        assert!(after_snapshot.bytes.is_some());

        assert_eq!((change.additions, change.removals), (0, 0));
        assert_eq!(count_now(&change), (0, 0));
    }

    // The inverse: only the after image is naturally unretained.
    #[test]
    fn public_api_unretained_after_keeps_conservative_counts() {
        let before = b"one\ntwo\n";
        let after = b"one\ntwo\nthree\n";
        let change = prepare_change("note.txt", Some(before), Some(after), before.len()).unwrap();

        assert_eq!(change.kind, argentum_domain::FileChangeKind::Modified);
        let before_snapshot = change.before.as_ref().unwrap();
        assert!(before_snapshot.reference.retained);
        assert!(before_snapshot.bytes.is_some());
        let after_snapshot = change.after.as_ref().unwrap();
        assert!(!after_snapshot.reference.retained);
        assert!(after_snapshot.bytes.is_none());
        assert_eq!(after_snapshot.reference.size_bytes, after.len() as u64);

        assert_eq!((change.additions, change.removals), (0, 0));
        assert_eq!(count_now(&change), (0, 0));
    }

    // Natural unretained creation and deletion, observed through the returned
    // counts and retention flags of the public constructor.
    #[test]
    fn public_api_unretained_creation_and_deletion_keep_conservative_counts() {
        let oversized = [b'x'; LIMIT + 1];

        let created = prepare_change("big.txt", None, Some(&oversized), LIMIT).unwrap();
        assert_eq!(created.kind, argentum_domain::FileChangeKind::Added);
        assert!(created.before.is_none());
        let created_after = created.after.as_ref().unwrap();
        assert!(!created_after.reference.retained);
        assert!(created_after.bytes.is_none());
        assert_eq!((created.additions, created.removals), (0, 0));
        assert_eq!(count_now(&created), (0, 0));

        let deleted = prepare_change("big.txt", Some(&oversized), None, LIMIT).unwrap();
        assert_eq!(deleted.kind, argentum_domain::FileChangeKind::Deleted);
        assert!(deleted.after.is_none());
        let deleted_before = deleted.before.as_ref().unwrap();
        assert!(!deleted_before.reference.retained);
        assert!(deleted_before.bytes.is_none());
        assert_eq!((deleted.additions, deleted.removals), (0, 0));
        assert_eq!(count_now(&deleted), (0, 0));
    }

    use argentum_domain::LineCountAvailability;

    fn availability_now(change: &PreparedChange) -> LineCountAvailability {
        line_counts_with_availability(change.before.as_ref(), change.after.as_ref()).2
    }

    // Readable changes record their provenance as computed, including the
    // genuine zero results that the numeric convention cannot distinguish from
    // an unreadable image.
    #[test]
    fn readable_changes_are_computed_including_genuine_zero() {
        let added = prepare_change("new.txt", None, Some(b"one\ntwo\n"), LIMIT).unwrap();
        assert_eq!(
            added.line_count_availability,
            LineCountAvailability::Computed
        );

        let deleted = prepare_change("old.txt", Some(b"one\ntwo\n"), None, LIMIT).unwrap();
        assert_eq!(
            deleted.line_count_availability,
            LineCountAvailability::Computed
        );

        let modified = prepare_change("edit.txt", Some(b"one\n"), Some(b"two\n"), LIMIT).unwrap();
        assert_eq!(
            modified.line_count_availability,
            LineCountAvailability::Computed
        );

        let empty_to_empty = prepare_change("empty.txt", Some(b""), Some(b""), LIMIT).unwrap();
        assert_eq!((empty_to_empty.additions, empty_to_empty.removals), (0, 0));
        assert_eq!(
            empty_to_empty.line_count_availability,
            LineCountAvailability::Computed,
            "a genuine zero result is computed, not unknown"
        );

        let unicode_crlf = prepare_change(
            "caf\u{e9}.txt",
            Some("a\r\n\u{30bd}\r\n".as_bytes()),
            Some("b\r\n\u{30bd}\r\n".as_bytes()),
            LIMIT,
        )
        .unwrap();
        assert_eq!((unicode_crlf.additions, unicode_crlf.removals), (1, 1));
        assert_eq!(
            unicode_crlf.line_count_availability,
            LineCountAvailability::Computed
        );
    }

    // An existing image whose bytes are unreadable is unavailable, for either
    // side and for one or both sides at once.
    #[test]
    fn unreadable_images_are_unavailable_including_both_sides() {
        let oversized = [b'x'; LIMIT + 1];

        let before_gone =
            prepare_change("note.txt", Some(&oversized), Some(b"one\ntwo\n"), LIMIT).unwrap();
        assert_eq!((before_gone.additions, before_gone.removals), (0, 0));
        assert_eq!(
            before_gone.line_count_availability,
            LineCountAvailability::Unavailable
        );

        let after_gone =
            prepare_change("note.txt", Some(b"one\ntwo\n"), Some(&oversized), LIMIT).unwrap();
        assert_eq!(
            after_gone.line_count_availability,
            LineCountAvailability::Unavailable
        );

        let both_gone =
            prepare_change("big.txt", Some(&oversized), Some(&oversized), LIMIT).unwrap();
        assert_eq!(
            both_gone.line_count_availability,
            LineCountAvailability::Unavailable
        );

        let nul_after = prepare_change("blob.txt", Some(b"a\n"), Some(b"a\0b\n"), LIMIT).unwrap();
        assert_eq!(
            nul_after.line_count_availability,
            LineCountAvailability::Unavailable
        );

        let nul_before = prepare_change("blob.txt", Some(b"a\0b\n"), Some(b"a\n"), LIMIT).unwrap();
        assert_eq!(
            nul_before.line_count_availability,
            LineCountAvailability::Unavailable
        );

        let invalid_after = prepare_change(
            "image.bin",
            Some(b"one\n"),
            Some(&[0x6f, 0x6e, 0x65, 0x9f]),
            LIMIT,
        )
        .unwrap();
        assert_eq!(
            invalid_after.line_count_availability,
            LineCountAvailability::Unavailable
        );

        let invalid_before = prepare_change(
            "image.bin",
            Some(&[0x6f, 0x6e, 0x65, 0x9f]),
            Some(b"one\n"),
            LIMIT,
        )
        .unwrap();
        assert_eq!(
            invalid_before.line_count_availability,
            LineCountAvailability::Unavailable
        );
    }

    // Real absence in creation and deletion is not missing evidence, so it
    // stays computed even when the numeric counts are not zero.
    #[test]
    fn real_absence_is_computed_not_unavailable() {
        let added = prepare_change("new.txt", None, Some(b"one\ntwo\n"), LIMIT).unwrap();
        assert_eq!((added.additions, added.removals), (2, 0));
        assert_eq!(
            added.line_count_availability,
            LineCountAvailability::Computed
        );
        assert_eq!(availability_now(&added), LineCountAvailability::Computed);

        let deleted = prepare_change("old.txt", Some(b"one\ntwo\n"), None, LIMIT).unwrap();
        assert_eq!((deleted.additions, deleted.removals), (0, 2));
        assert_eq!(
            deleted.line_count_availability,
            LineCountAvailability::Computed
        );
    }

    // Preparation derives the state from the same snapshots used for the
    // counts, so re-deriving from the returned snapshots agrees.
    #[test]
    fn availability_agrees_with_returned_snapshots() {
        let change = prepare_change("note.txt", Some(b"one\n"), Some(b"two\n"), LIMIT).unwrap();
        assert_eq!(
            change.line_count_availability,
            availability_now(&change),
            "the stored state must come from the admitted snapshots"
        );

        let mut unretained = prepare_change(
            "note.txt",
            Some(b"one\ntwo\n"),
            Some(b"one\ntwo\nthree\n"),
            LIMIT,
        )
        .unwrap();
        unretain(&mut unretained.after);
        assert_eq!(
            availability_now(&unretained),
            LineCountAvailability::Unavailable,
            "an admitted snapshot whose bytes are gone is unavailable"
        );
    }

    // Missing legacy metadata decodes as unknown regardless of the numeric
    // values, today's files or retention flags.
    #[test]
    fn legacy_missing_metadata_decodes_as_unknown() {
        let legacy_zero = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000000",
            "change_set_id": "00000000-0000-0000-0000-000000000001",
            "project_id": "00000000-0000-0000-0000-000000000002",
            "session_id": "00000000-0000-0000-0000-000000000003",
            "run_id": "00000000-0000-0000-0000-000000000004",
            "revision": 1, "relative_path": "note.txt", "kind": "modified",
            "origin": "tool_call",
            "before": {"digest": "00".repeat(32), "size_bytes": 2, "retained": false},
            "after": null, "additions": 0, "removals": 0,
            "created_at": argentum_domain::now()
        });
        let record: argentum_domain::FileChangeRecord =
            serde_json::from_value(legacy_zero.clone()).expect("legacy record must stay decodable");
        assert_eq!(
            record.line_count_availability,
            LineCountAvailability::Unknown,
            "legacy zero counts must not be guessed"
        );

        let mut legacy_nonzero = legacy_zero;
        legacy_nonzero["additions"] = serde_json::json!(7);
        legacy_nonzero["removals"] = serde_json::json!(9);
        let record: argentum_domain::FileChangeRecord =
            serde_json::from_value(legacy_nonzero).expect("legacy record must stay decodable");
        assert_eq!(
            record.line_count_availability,
            LineCountAvailability::Unknown,
            "legacy numeric values must not imply availability"
        );

        // Round-tripping a record that carries a real state still works.
        let mut current = record.clone();
        current.line_count_availability = LineCountAvailability::Computed;
        let encoded = serde_json::to_string(&current).unwrap();
        let decoded: argentum_domain::FileChangeRecord = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            decoded.line_count_availability,
            LineCountAvailability::Computed
        );
        assert_eq!((decoded.additions, decoded.removals), (7, 9));
    }
}
