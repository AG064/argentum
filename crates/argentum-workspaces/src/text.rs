use super::*;
use argentum_domain::text_contract::{
    TextEditSelection, TextLineEndingStyle, TextNewlineMode, TextReadResult,
    MAX_TARGETED_EDIT_BYTES, MAX_TEXT_LINE_COUNT, MAX_TEXT_RESULT_BYTES, MAX_TEXT_SNAPSHOT_BYTES,
};

impl WorkspaceManager {
    // Every read, allocation and digest is bounded independently of line length and
    // of concurrent growth. Retaining one snapshot keeps returned bytes and revision
    // coherent. A revision is a precondition, never a capability or approval grant.
    fn text_snapshot(&self, path: &str, scope: SandboxScope) -> Result<String, WorkspaceError> {
        let text = match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => {
                let (absolute, file) = self.broker.open_file_for_read(relative)?;
                let metadata = file.metadata().map_err(|source| WorkspaceError::Io {
                    path: absolute.clone(),
                    source,
                })?;
                if !metadata.is_file() {
                    return Err(WorkspaceError::NotFile(absolute));
                }
                decode_snapshot(&absolute, file)?
            }
            ScopedPath::Ambient(absolute) => {
                let file = fs::File::open(&absolute).map_err(|source| WorkspaceError::Io {
                    path: absolute.clone(),
                    source,
                })?;
                let metadata = file.metadata().map_err(|source| WorkspaceError::Io {
                    path: absolute.clone(),
                    source,
                })?;
                if !metadata.is_file() {
                    return Err(WorkspaceError::NotFile(absolute));
                }
                decode_snapshot(&absolute, file)?
            }
        };
        if text.contains('\0') {
            return Err(WorkspaceError::InvalidText(
                "NUL-containing binary text is unsupported",
            ));
        }
        Ok(text)
    }

    pub fn read_text_range_with_scope(
        &self,
        path: &str,
        start_byte: u64,
        end_byte: Option<u64>,
        expected_sha256: Option<&Sha256Digest>,
        scope: SandboxScope,
    ) -> Result<TextReadResult, WorkspaceError> {
        self.read_text_range_with_scope_and_output_limit(
            path,
            start_byte,
            end_byte,
            expected_sha256,
            scope,
            MAX_TEXT_RESULT_BYTES,
        )
    }

    /// A host-selected preview budget, not a model token estimate or file limit.
    /// Requested endpoints, full-snapshot SHA and continuation remain truthful.
    pub fn read_text_range_with_scope_and_output_limit(
        &self,
        path: &str,
        start_byte: u64,
        end_byte: Option<u64>,
        expected_sha256: Option<&Sha256Digest>,
        scope: SandboxScope,
        output_limit: usize,
    ) -> Result<TextReadResult, WorkspaceError> {
        if !(2048..=MAX_TEXT_RESULT_BYTES).contains(&output_limit) {
            return Err(WorkspaceError::InvalidText(
                "unsupported text result byte limit",
            ));
        }
        if start_byte != 0 && expected_sha256.is_none() {
            return Err(WorkspaceError::InvalidText(
                "nonzero start_byte requires expected_sha256 from a prior read",
            ));
        }
        if expected_sha256.is_some_and(|digest| !valid_digest(digest)) {
            return Err(WorkspaceError::InvalidText(
                "expected_sha256 must be 64 lowercase hex digits",
            ));
        }
        let text = self.text_snapshot(path, scope)?;
        let sha256 = sha256_digest(text.as_bytes());
        if expected_sha256.is_some_and(|expected| expected != &sha256) {
            return Err(WorkspaceError::ExpectedDigestMismatch(path.into()));
        }
        let size = text.len() as u64;
        let requested_end_byte = end_byte.unwrap_or(size);
        if start_byte > requested_end_byte || requested_end_byte > size {
            return Err(WorkspaceError::InvalidText(
                "byte range must satisfy 0 <= start <= end <= file size",
            ));
        }
        let start = start_byte as usize;
        let requested_end = requested_end_byte as usize;
        if !text.is_char_boundary(start) || !text.is_char_boundary(requested_end) {
            return Err(WorkspaceError::InvalidText(
                "byte range splits a UTF-8 character",
            ));
        }
        bounded_text_read_with_output_limit(
            &text,
            sha256,
            start_byte,
            requested_end_byte,
            output_limit,
        )
    }

    pub fn read_text_lines_with_scope(
        &self,
        path: &str,
        start_line: u64,
        line_count: u16,
        expected_sha256: Option<&Sha256Digest>,
        scope: SandboxScope,
    ) -> Result<TextReadResult, WorkspaceError> {
        if start_line == 0 {
            return Err(WorkspaceError::InvalidText(
                "start_line is one-based and must be positive",
            ));
        }
        if line_count == 0 || line_count > MAX_TEXT_LINE_COUNT {
            return Err(WorkspaceError::InvalidText(
                "line_count must be between 1 and 200",
            ));
        }
        if expected_sha256.is_some_and(|digest| !valid_digest(digest)) {
            return Err(WorkspaceError::InvalidText(
                "expected_sha256 must be 64 lowercase hex digits",
            ));
        }
        let text = self.text_snapshot(path, scope)?;
        let sha256 = sha256_digest(text.as_bytes());
        if expected_sha256.is_some_and(|expected| expected != &sha256) {
            return Err(WorkspaceError::ExpectedDigestMismatch(path.into()));
        }
        let line_starts = line_start_offsets(&text);
        let start_index = usize::try_from(start_line - 1).map_err(|_| {
            WorkspaceError::InvalidText("start_line exceeds the supported file range")
        })?;
        if start_index > line_starts.len() {
            return Err(WorkspaceError::InvalidText(
                "start_line exceeds the one-based EOF position",
            ));
        }
        let requested_end_index = start_index
            .checked_add(usize::from(line_count))
            .ok_or(WorkspaceError::InvalidText(
                "line range arithmetic overflow",
            ))?
            .min(line_starts.len());
        let start = line_starts.get(start_index).copied().unwrap_or(text.len());
        let requested_end = line_starts
            .get(requested_end_index)
            .copied()
            .unwrap_or(text.len());
        let mut result = bounded_text_read(&text, sha256, start as u64, requested_end as u64)?;
        let returned_end = result.returned_end_byte as usize;
        let returned_line_count = line_starts[start_index..requested_end_index]
            .iter()
            .take_while(|offset| **offset < returned_end)
            .count()
            .try_into()
            .expect("bounded requested line count fits u16");
        result.requested_start_line = Some(start_line);
        result.requested_line_count = Some(line_count);
        result.returned_start_line = Some(start_line);
        result.returned_line_count = Some(returned_line_count);
        result.line_range_complete = Some(result.range_complete);
        if result.range_complete
            && returned_line_count > 0
            && result.content.len() <= MAX_TARGETED_EDIT_BYTES
            && matches!(
                result.line_ending_style,
                TextLineEndingStyle::Lf | TextLineEndingStyle::CrLf
            )
        {
            result.edit_selection = Some(TextEditSelection {
                expected_sha256: result.sha256.clone(),
                start_line,
                line_count: returned_line_count,
            });
        }
        Ok(result)
    }

    pub fn prepare_text_edit_with_scope(
        &self,
        path: &str,
        expected_sha256: &Sha256Digest,
        old_text: &str,
        new_text: &str,
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        self.prepare_text_edit_with_mode_and_scope(
            path,
            expected_sha256,
            old_text,
            new_text,
            TextNewlineMode::Exact,
            scope,
        )
    }

    pub fn prepare_text_edit_with_mode_and_scope(
        &self,
        path: &str,
        expected_sha256: &Sha256Digest,
        old_text: &str,
        new_text: &str,
        newline_mode: TextNewlineMode,
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        if !valid_digest(expected_sha256) {
            return Err(WorkspaceError::InvalidText(
                "edit requires a valid SHA-256 base",
            ));
        }
        if old_text.is_empty() {
            return Err(WorkspaceError::InvalidText("old_text must be nonempty"));
        }
        if old_text.len() > MAX_TARGETED_EDIT_BYTES || new_text.len() > MAX_TARGETED_EDIT_BYTES {
            return Err(WorkspaceError::InvalidText(
                "old/new text each at most 16 KiB",
            ));
        }
        if newline_mode == TextNewlineMode::Exact && old_text == new_text {
            return Err(WorkspaceError::InvalidText(
                "new_text must differ from old_text",
            ));
        }
        if old_text.contains('\0') || new_text.contains('\0') {
            return Err(WorkspaceError::InvalidText(
                "edit text must not contain NUL",
            ));
        }
        let before = self.text_snapshot(path, scope)?;
        let before_sha256 = sha256_digest(before.as_bytes());
        if &before_sha256 != expected_sha256 {
            return Err(WorkspaceError::ExpectedDigestMismatch(path.into()));
        }
        let (start, end, replacement) = match newline_mode {
            TextNewlineMode::Exact => {
                let Some(start) = before.find(old_text) else {
                    // Diagnose newline-only mismatches without normalizing or applying
                    // an edit. Exact matching and the same SHA guard still apply.
                    if before
                        .replace("\r\n", "\n")
                        .contains(&old_text.replace("\r\n", "\n"))
                    {
                        return Err(WorkspaceError::InvalidText(
                            "edit text line endings do not match",
                        ));
                    }
                    return Err(WorkspaceError::InvalidText("edit text was not found"));
                };
                ensure_unique_match(&before, old_text, start)?;
                (start, start + old_text.len(), new_text.as_bytes().to_vec())
            }
            TextNewlineMode::Preserve => resolve_preserving_edit(&before, old_text, new_text)?,
        };
        self.prepare_resolved_text_edit(
            path,
            scope,
            before,
            before_sha256,
            (start, end),
            replacement,
        )
    }

    /// Replace complete, one-based source lines. Line bodies contain no physical
    /// separators; the host supplies the admitted source's uniform separator.
    pub fn prepare_text_line_edit_with_scope(
        &self,
        path: &str,
        expected_sha256: &Sha256Digest,
        start_line: u64,
        line_count: u16,
        replacement_lines: &[String],
        scope: SandboxScope,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        if !valid_digest(expected_sha256) {
            return Err(WorkspaceError::InvalidText(
                "edit requires a valid SHA-256 base",
            ));
        }
        if start_line == 0 || line_count == 0 || line_count > MAX_TEXT_LINE_COUNT {
            return Err(WorkspaceError::InvalidText(
                "line edit requires a positive one-based start and 1 to 200 source lines",
            ));
        }
        if replacement_lines.is_empty()
            || replacement_lines.len() > usize::from(MAX_TEXT_LINE_COUNT)
        {
            return Err(WorkspaceError::InvalidText(
                "line edit requires 1 to 200 replacement line bodies; deletion is unsupported",
            ));
        }
        if replacement_lines.iter().any(|line| {
            line.bytes()
                .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
        }) {
            return Err(WorkspaceError::InvalidText(
                "replacement line bodies cannot contain physical CR, LF, or NUL",
            ));
        }
        let logical_len = replacement_lines
            .iter()
            .try_fold(replacement_lines.len() - 1, |total, line| {
                total.checked_add(line.len())
            })
            .ok_or(WorkspaceError::PatchLimitExceeded)?;
        if logical_len > MAX_TARGETED_EDIT_BYTES {
            return Err(WorkspaceError::InvalidText(
                "replacement lines exceed 16 KiB of logical UTF-8 text",
            ));
        }
        let before = self.text_snapshot(path, scope)?;
        let before_sha256 = sha256_digest(before.as_bytes());
        if &before_sha256 != expected_sha256 {
            return Err(WorkspaceError::ExpectedDigestMismatch(path.into()));
        }
        let separator: &[u8] =
            match line_ending_style(&before) {
                TextLineEndingStyle::Lf => b"\n",
                TextLineEndingStyle::CrLf => b"\r\n",
                _ => return Err(WorkspaceError::InvalidText(
                    "line edit requires uniform LF or CRLF source; exact mode remains available",
                )),
            };
        let starts = line_start_offsets(&before);
        let start_index = usize::try_from(start_line - 1).map_err(|_| {
            WorkspaceError::InvalidText("start_line exceeds the supported file range")
        })?;
        let end_index = start_index
            .checked_add(usize::from(line_count))
            .ok_or(WorkspaceError::PatchLimitExceeded)?;
        if start_index >= starts.len() || end_index > starts.len() {
            return Err(WorkspaceError::InvalidText(
                "line edit span exceeds existing source lines; insertion is unsupported",
            ));
        }
        let start = starts[start_index];
        let end = starts.get(end_index).copied().unwrap_or(before.len());
        if end - start > MAX_TARGETED_EDIT_BYTES {
            return Err(WorkspaceError::InvalidText(
                "selected source lines exceed the 16 KiB edit limit",
            ));
        }
        let terminated = end < before.len() || before.as_bytes()[start..end].ends_with(separator);
        let separator_count = replacement_lines.len() - 1 + usize::from(terminated);
        let raw_len = replacement_lines
            .iter()
            .map(String::len)
            .sum::<usize>()
            .checked_add(separator_count * separator.len())
            .ok_or(WorkspaceError::PatchLimitExceeded)?;
        if raw_len > MAX_TARGETED_EDIT_BYTES {
            return Err(WorkspaceError::InvalidText(
                "materialized replacement exceeds the 16 KiB edit limit",
            ));
        }
        let mut replacement = Vec::with_capacity(raw_len);
        for (index, line) in replacement_lines.iter().enumerate() {
            if index != 0 {
                replacement.extend_from_slice(separator);
            }
            replacement.extend_from_slice(line.as_bytes());
        }
        if terminated {
            replacement.extend_from_slice(separator);
        }
        self.prepare_resolved_text_edit(
            path,
            scope,
            before,
            before_sha256,
            (start, end),
            replacement,
        )
    }

    fn prepare_resolved_text_edit(
        &self,
        path: &str,
        scope: SandboxScope,
        before: String,
        before_sha256: Sha256Digest,
        span: (usize, usize),
        replacement: Vec<u8>,
    ) -> Result<PreparedWorkspacePatch, WorkspaceError> {
        let (start, end) = span;
        if before.as_bytes()[start..end] == replacement {
            return Err(WorkspaceError::EmptyMutation(path.into()));
        }
        let after_len = before
            .len()
            .checked_sub(end - start)
            .and_then(|size| size.checked_add(replacement.len()))
            .ok_or(WorkspaceError::PatchLimitExceeded)?;
        if after_len > MAX_TEXT_SNAPSHOT_BYTES {
            return Err(WorkspaceError::PatchLimitExceeded);
        }
        let mut after = Vec::with_capacity(after_len);
        after.extend_from_slice(&before.as_bytes()[..start]);
        after.extend_from_slice(&replacement);
        after.extend_from_slice(&before.as_bytes()[end..]);
        let relative_path = match self.resolve_path_with_scope(path, scope)? {
            ScopedPath::Workspace { relative, .. } => normalize_display_path(&relative),
            ScopedPath::Ambient(absolute) => absolute.to_string_lossy().into_owned(),
        };
        // This is only preparation. The existing runtime journals these images and
        // the existing patch executor rechecks authority, preimage and cancellation.
        Ok(PreparedWorkspacePatch {
            total_bytes: (before.len() + after.len()) as u64,
            mutations: vec![PreparedWorkspaceMutation {
                relative_path,
                before_sha256: Some(before_sha256),
                after_sha256: Some(sha256_digest(&after)),
                before: Some(before.into_bytes()),
                after: Some(after),
            }],
        })
    }
}

fn ensure_unique_match(text: &str, needle: &str, start: usize) -> Result<(), WorkspaceError> {
    // Search one character after the first start, not after its end: overlapping
    // occurrences (for example aa in aaa) are ambiguous too.
    let next = start
        + text[start..]
            .chars()
            .next()
            .expect("nonempty match")
            .len_utf8();
    if text[next..].contains(needle) {
        return Err(WorkspaceError::InvalidText(
            "edit text has multiple matches",
        ));
    }
    Ok(())
}

fn resolve_preserving_edit(
    before: &str,
    old_text: &str,
    new_text: &str,
) -> Result<(usize, usize, Vec<u8>), WorkspaceError> {
    if contains_bare_cr(old_text) || contains_bare_cr(new_text) {
        return Err(WorkspaceError::InvalidText(
            "preserve mode arguments must not contain bare CR",
        ));
    }
    let style = line_ending_style(before);
    match style {
        TextLineEndingStyle::Mixed => {
            return Err(WorkspaceError::InvalidText(
                "preserve mode requires uniform LF or CRLF source",
            ));
        }
        TextLineEndingStyle::BareCr => {
            return Err(WorkspaceError::InvalidText(
                "preserve mode does not support source text containing bare CR",
            ));
        }
        TextLineEndingStyle::None => {
            if contains_physical_newline(old_text) || contains_physical_newline(new_text) {
                return Err(WorkspaceError::InvalidText(
                    "preserve mode cannot infer line endings for a source without newlines",
                ));
            }
            let Some(start) = before.find(old_text) else {
                return Err(WorkspaceError::InvalidText("edit text was not found"));
            };
            ensure_unique_match(before, old_text, start)?;
            return Ok((start, start + old_text.len(), new_text.as_bytes().to_vec()));
        }
        TextLineEndingStyle::Lf | TextLineEndingStyle::CrLf => {}
    }

    let (canonical_before, raw_boundaries) = canonical_newlines_with_boundaries(before)?;
    let canonical_old = canonical_newlines(old_text)?;
    let Some(logical_start) = canonical_before.find(&canonical_old) else {
        return Err(WorkspaceError::InvalidText("edit text was not found"));
    };
    ensure_unique_match(&canonical_before, &canonical_old, logical_start)?;
    let logical_end = logical_start
        .checked_add(canonical_old.len())
        .ok_or(WorkspaceError::PatchLimitExceeded)?;
    let raw_start = *raw_boundaries
        .get(logical_start)
        .ok_or(WorkspaceError::PatchLimitExceeded)?;
    let raw_end = *raw_boundaries
        .get(logical_end)
        .ok_or(WorkspaceError::PatchLimitExceeded)?;
    if !before.is_char_boundary(raw_start) || !before.is_char_boundary(raw_end) {
        return Err(WorkspaceError::InvalidText(
            "preserve mode resolved an invalid UTF-8 boundary",
        ));
    }
    let canonical_new = canonical_newlines(new_text)?;
    let replacement = materialize_newlines(&canonical_new, style)?;
    Ok((raw_start, raw_end, replacement))
}

fn contains_physical_newline(text: &str) -> bool {
    text.as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'\r' | b'\n'))
}

fn contains_bare_cr(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
}

fn line_ending_style(text: &str) -> TextLineEndingStyle {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut saw_lf = false;
    let mut saw_crlf = false;
    let mut saw_bare_cr = false;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                saw_crlf = true;
                index += 2;
            }
            b'\r' => {
                saw_bare_cr = true;
                index += 1;
            }
            b'\n' => {
                saw_lf = true;
                index += 1;
            }
            _ => index += 1,
        }
    }
    if saw_bare_cr {
        TextLineEndingStyle::BareCr
    } else if saw_lf && saw_crlf {
        TextLineEndingStyle::Mixed
    } else if saw_crlf {
        TextLineEndingStyle::CrLf
    } else if saw_lf {
        TextLineEndingStyle::Lf
    } else {
        TextLineEndingStyle::None
    }
}

fn canonical_newlines(text: &str) -> Result<String, WorkspaceError> {
    canonical_newlines_with_boundaries(text).map(|(canonical, _)| canonical)
}

fn canonical_newlines_with_boundaries(text: &str) -> Result<(String, Vec<usize>), WorkspaceError> {
    let bytes = text.as_bytes();
    let mut canonical = Vec::with_capacity(bytes.len());
    let mut raw_boundaries = Vec::with_capacity(bytes.len() + 1);
    raw_boundaries.push(0);
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if bytes.get(index + 1) != Some(&b'\n') {
                return Err(WorkspaceError::InvalidText(
                    "preserve mode text must not contain bare CR",
                ));
            }
            canonical.push(b'\n');
            index += 2;
            raw_boundaries.push(index);
        } else {
            canonical.push(bytes[index]);
            index += 1;
            raw_boundaries.push(index);
        }
    }
    let canonical =
        String::from_utf8(canonical).expect("newline canonicalization preserves valid UTF-8 bytes");
    Ok((canonical, raw_boundaries))
}

fn materialize_newlines(
    canonical: &str,
    style: TextLineEndingStyle,
) -> Result<Vec<u8>, WorkspaceError> {
    let newline_count = canonical.bytes().filter(|byte| *byte == b'\n').count();
    let expanded_len = match style {
        TextLineEndingStyle::Lf => canonical.len(),
        TextLineEndingStyle::CrLf => canonical
            .len()
            .checked_add(newline_count)
            .ok_or(WorkspaceError::PatchLimitExceeded)?,
        _ => {
            return Err(WorkspaceError::InvalidText(
                "preserve mode requires uniform LF or CRLF source",
            ));
        }
    };
    if expanded_len > MAX_TARGETED_EDIT_BYTES {
        return Err(WorkspaceError::InvalidText(
            "materialized new_text exceeds 16 KiB",
        ));
    }
    let mut output = Vec::with_capacity(expanded_len);
    for byte in canonical.bytes() {
        if byte == b'\n' && style == TextLineEndingStyle::CrLf {
            output.push(b'\r');
        }
        output.push(byte);
    }
    Ok(output)
}

fn line_start_offsets(text: &str) -> Vec<usize> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut starts = vec![0];
    for (index, byte) in text.bytes().enumerate() {
        if byte == b'\n' && index + 1 < text.len() {
            starts.push(index + 1);
        }
    }
    starts
}

fn bounded_text_read(
    text: &str,
    sha256: Sha256Digest,
    requested_start_byte: u64,
    requested_end_byte: u64,
) -> Result<TextReadResult, WorkspaceError> {
    bounded_text_read_with_output_limit(
        text,
        sha256,
        requested_start_byte,
        requested_end_byte,
        MAX_TEXT_RESULT_BYTES,
    )
}

fn bounded_text_read_with_output_limit(
    text: &str,
    sha256: Sha256Digest,
    requested_start_byte: u64,
    requested_end_byte: u64,
    output_limit: usize,
) -> Result<TextReadResult, WorkspaceError> {
    let start = usize::try_from(requested_start_byte)
        .map_err(|_| WorkspaceError::InvalidText("byte range exceeds the supported file size"))?;
    let requested_end = usize::try_from(requested_end_byte)
        .map_err(|_| WorkspaceError::InvalidText("byte range exceeds the supported file size"))?;
    if start > requested_end || requested_end > text.len() {
        return Err(WorkspaceError::InvalidText(
            "byte range must satisfy 0 <= start <= end <= file size",
        ));
    }
    if !text.is_char_boundary(start) || !text.is_char_boundary(requested_end) {
        return Err(WorkspaceError::InvalidText(
            "byte range splits a UTF-8 character",
        ));
    }
    // Reserve space for all fixed metadata, including the digest and numeric
    // endpoints. Count JSON escaping, not merely raw content bytes.
    let mut remaining = output_limit - 1024;
    let mut end = start;
    for character in text[start..requested_end].chars() {
        let cost = match character {
            '"' | '\\' => 2,
            '\u{0008}' | '\u{0009}' | '\u{000a}' | '\u{000c}' | '\u{000d}' => 2,
            '\u{0000}'..='\u{001f}' => 6,
            _ => character.len_utf8(),
        };
        if cost > remaining {
            break;
        }
        remaining -= cost;
        end += character.len_utf8();
    }
    let file_has_more = end < text.len();
    Ok(TextReadResult {
        schema_version: 1,
        position_unit: "utf8_bytes_end_exclusive".into(),
        line_ending_style: line_ending_style(text),
        requested_start_byte,
        requested_end_byte,
        returned_start_byte: requested_start_byte,
        returned_end_byte: end as u64,
        range_complete: end == requested_end,
        file_has_more,
        is_complete_file: start == 0 && end == text.len(),
        next_start_byte: file_has_more.then_some(end as u64),
        requested_start_line: None,
        requested_line_count: None,
        returned_start_line: None,
        returned_line_count: None,
        line_range_complete: None,
        edit_selection: None,
        file_size_bytes: text.len() as u64,
        sha256,
        content: text[start..end].into(),
    })
}

fn valid_digest(digest: &Sha256Digest) -> bool {
    digest.0.len() == 64
        && digest
            .0
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn decode_snapshot(path: &Path, file: impl Read) -> Result<String, WorkspaceError> {
    let mut bytes = Vec::with_capacity(MAX_TEXT_SNAPSHOT_BYTES + 1);
    file.take((MAX_TEXT_SNAPSHOT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|source| WorkspaceError::Io {
            path: path.into(),
            source,
        })?;
    if bytes.len() > MAX_TEXT_SNAPSHOT_BYTES {
        return Err(WorkspaceError::FileTooLarge {
            path: path.into(),
            limit_bytes: MAX_TEXT_SNAPSHOT_BYTES,
        });
    }
    String::from_utf8(bytes).map_err(|_| WorkspaceError::InvalidText("expected valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_security::ApprovalPolicy;

    const SCOPE: SandboxScope = SandboxScope::WorkspaceWrite;

    #[test]
    fn host_preview_budget_preserves_requested_eof_sha_and_truthful_continuation() {
        let source = "UTF-8 æ and quoted \\\"data\\\"\r\n".repeat(5000);
        let (directory, workspace) = fixture(source.as_bytes());
        let preview = workspace
            .read_text_range_with_scope_and_output_limit("source.txt", 0, None, None, SCOPE, 8192)
            .unwrap();
        assert!(preview.content.len() < 8192);
        assert_eq!(preview.requested_end_byte, source.len() as u64);
        assert!(!preview.range_complete && preview.file_has_more && !preview.is_complete_file);
        assert!(preview.returned_end_byte > 0);
        assert_eq!(
            preview.content,
            source[..preview.returned_end_byte as usize]
        );
        assert_eq!(preview.sha256, sha256_digest(source.as_bytes()));
        let explicit = workspace
            .read_text_range_with_scope("source.txt", 0, Some(source.len() as u64), None, SCOPE)
            .unwrap();
        assert!(explicit.returned_end_byte > preview.returned_end_byte);
        assert_eq!(explicit.sha256, preview.sha256);
        let tail = workspace
            .read_text_range_with_scope_and_output_limit(
                "source.txt",
                preview.next_start_byte.unwrap(),
                None,
                Some(&preview.sha256),
                SCOPE,
                8192,
            )
            .unwrap();
        assert_eq!(
            tail.content,
            source[tail.returned_start_byte as usize..tail.returned_end_byte as usize]
        );
        assert!(tail.returned_end_byte > preview.returned_end_byte);
        fs::write(directory.path().join("source.txt"), "changed").unwrap();
        assert!(matches!(
            workspace.read_text_range_with_scope_and_output_limit(
                "source.txt",
                0,
                None,
                Some(&preview.sha256),
                SCOPE,
                8192
            ),
            Err(WorkspaceError::ExpectedDigestMismatch(_))
        ));
    }

    fn fixture(bytes: &[u8]) -> (tempfile::TempDir, WorkspaceManager) {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("source.txt"), bytes).unwrap();
        let broker = CapabilityBroker::new(directory.path(), ApprovalPolicy::default()).unwrap();
        (directory, WorkspaceManager::new(broker))
    }

    #[test]
    fn text_sizes_and_random_access_are_truthful_and_bounded() {
        for size in [
            0,
            1,
            65535,
            65536,
            65537,
            131071,
            131072,
            131073,
            350_000,
            MAX_TEXT_SNAPSHOT_BYTES,
        ] {
            let original = "a".repeat(size);
            let (_dir, manager) = fixture(original.as_bytes());
            let first = manager
                .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
                .unwrap();
            assert_eq!(first.file_size_bytes, size as u64);
            assert_eq!(first.requested_end_byte, size as u64);
            assert_eq!(first.returned_end_byte, first.content.len() as u64);
            assert_eq!(first.content, original[..first.content.len()]);
            assert_eq!(first.range_complete, first.content.len() == size);
            assert_eq!(first.is_complete_file, first.range_complete);
            assert_eq!(first.file_has_more, !first.range_complete);
            if first.file_has_more {
                assert_eq!(first.next_start_byte, Some(first.returned_end_byte));
                let last = manager
                    .read_text_range_with_scope(
                        "source.txt",
                        (size - 3) as u64,
                        None,
                        Some(&first.sha256),
                        SCOPE,
                    )
                    .unwrap();
                assert_eq!(last.content, "aaa");
                assert!(last.range_complete);
                assert!(!last.is_complete_file || size == 3);
                assert!(!last.file_has_more);
                assert_eq!(last.next_start_byte, None);
            }
        }
        let (_dir, manager) = fixture(&vec![b'a'; MAX_TEXT_SNAPSHOT_BYTES + 1]);
        assert!(matches!(
            manager.read_text_range_with_scope("source.txt", 0, None, None, SCOPE),
            Err(WorkspaceError::FileTooLarge { .. })
        ));
    }

    #[test]
    fn line_addressing_preserves_source_bytes_and_defines_eof() {
        let original = "α\r\nbeta\r\n中\r\nlast";
        let (_directory, manager) = fixture(original.as_bytes());
        let read = manager
            .read_text_lines_with_scope("source.txt", 2, 2, None, SCOPE)
            .expect("fresh line read");
        assert_eq!(read.content, "beta\r\n中\r\n");
        assert_eq!(read.requested_start_line, Some(2));
        assert_eq!(read.requested_line_count, Some(2));
        assert_eq!(read.returned_start_line, Some(2));
        assert_eq!(read.returned_line_count, Some(2));
        assert_eq!(read.line_range_complete, Some(true));
        assert_eq!(
            read.edit_selection
                .as_ref()
                .map(|selection| (selection.start_line, selection.line_count)),
            Some((2, 2))
        );
        assert_eq!(
            read.edit_selection
                .as_ref()
                .map(|selection| &selection.expected_sha256),
            Some(&read.sha256)
        );
        assert_eq!(read.returned_start_byte, "α\r\n".len() as u64);
        assert_eq!(read.returned_end_byte, "α\r\nbeta\r\n中\r\n".len() as u64);
        assert!(read.range_complete && read.file_has_more && !read.is_complete_file);

        let eof = manager
            .read_text_lines_with_scope("source.txt", 5, 1, Some(&read.sha256), SCOPE)
            .expect("one-based EOF position");
        assert_eq!(eof.content, "");
        assert_eq!(eof.returned_line_count, Some(0));
        assert!(eof.edit_selection.is_none());
        assert!(eof.range_complete && !eof.file_has_more);
        assert!(manager
            .read_text_lines_with_scope("source.txt", 6, 1, Some(&read.sha256), SCOPE)
            .is_err());

        let (_directory, empty) = fixture(b"");
        let empty_read = empty
            .read_text_lines_with_scope("source.txt", 1, 1, None, SCOPE)
            .expect("empty-file EOF read");
        assert_eq!(empty_read.content, "");
        assert_eq!(empty_read.returned_line_count, Some(0));
        assert!(empty_read.edit_selection.is_none());
        assert!(empty_read.is_complete_file && empty_read.range_complete);
    }

    #[test]
    fn line_addressing_bounds_count_staleness_and_overlong_continuation() {
        let long_line = "x".repeat(MAX_TEXT_RESULT_BYTES * 2);
        let original = format!("{long_line}\r\nnext\r\n");
        let (directory, manager) = fixture(original.as_bytes());
        let first = manager
            .read_text_lines_with_scope("source.txt", 1, 1, None, SCOPE)
            .expect("bounded overlong line read");
        assert!(!first.range_complete);
        assert_eq!(first.line_range_complete, Some(false));
        assert!(first.edit_selection.is_none());
        assert_eq!(first.returned_line_count, Some(1));
        let next = first
            .next_start_byte
            .expect("progressing byte continuation");
        assert!(next > first.returned_start_byte);
        let continued = manager
            .read_text_range_with_scope(
                "source.txt",
                next,
                Some(first.requested_end_byte),
                Some(&first.sha256),
                SCOPE,
            )
            .expect("revision-bound continuation");
        assert!(continued.returned_end_byte > next);

        assert!(manager
            .read_text_lines_with_scope(
                "source.txt",
                1,
                MAX_TEXT_LINE_COUNT,
                Some(&first.sha256),
                SCOPE,
            )
            .is_ok());
        assert!(manager
            .read_text_lines_with_scope(
                "source.txt",
                1,
                MAX_TEXT_LINE_COUNT + 1,
                Some(&first.sha256),
                SCOPE,
            )
            .is_err());
        fs::write(directory.path().join("source.txt"), "changed\r\n").unwrap();
        assert!(matches!(
            manager.read_text_lines_with_scope("source.txt", 1, 1, Some(&first.sha256), SCOPE,),
            Err(WorkspaceError::ExpectedDigestMismatch(_))
        ));
    }

    #[test]
    fn text_utf8_crlf_escape_budget_and_invalid_inputs() {
        let original = format!(
            "{}é中\u{10400}\r\nend\r\n",
            "a".repeat(MAX_TEXT_RESULT_BYTES - 1025)
        );
        let (_dir, manager) = fixture(original.as_bytes());
        let first = manager
            .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
            .unwrap();
        assert!(original.is_char_boundary(first.returned_end_byte as usize));
        let last = manager
            .read_text_range_with_scope(
                "source.txt",
                first.next_start_byte.unwrap(),
                None,
                Some(&first.sha256),
                SCOPE,
            )
            .unwrap();
        assert_eq!(format!("{}{}", first.content, last.content), original);
        for (start, end, digest) in [
            (2, Some(1), Some(&first.sha256)),
            (0, Some(u64::MAX), None),
            (
                (MAX_TEXT_RESULT_BYTES - 1024) as u64,
                None,
                Some(&first.sha256),
            ),
            (1, None, None),
        ] {
            assert!(manager
                .read_text_range_with_scope("source.txt", start, end, digest, SCOPE)
                .is_err());
        }
        for bytes in [&[0xff][..], b"binary\0data"] {
            let (_dir, manager) = fixture(bytes);
            assert!(manager
                .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
                .is_err());
        }
        let (_dir, manager) = fixture(b"small\r\n");
        let result = manager
            .read_text_range_with_scope("source.txt", 0, Some(2), None, SCOPE)
            .unwrap();
        assert!(result.range_complete && result.file_has_more && !result.is_complete_file);
        assert_eq!(result.content, "sm");
    }

    #[test]
    fn short_escape_controls_use_their_true_json_cost() {
        // JSON short escapes for backspace, tab, LF, form feed and CR are two
        // bytes; other C0 controls stay at six. The prefix is the greatest one
        // whose escaped cost fits the same allowance.
        let allowance = 8192 - 1024;
        for (short, short_cost) in [
            ('\u{0008}', 2),
            ('\u{0009}', 2),
            ('\u{000a}', 2),
            ('\u{000c}', 2),
            ('\u{000d}', 2),
        ] {
            let source = short.to_string().repeat(allowance);
            let (_dir, manager) = fixture(source.as_bytes());
            let read = manager
                .read_text_range_with_scope_and_output_limit(
                    "source.txt",
                    0,
                    None,
                    None,
                    SCOPE,
                    8192,
                )
                .unwrap();
            let expected = allowance / short_cost;
            assert_eq!(read.returned_end_byte as usize, expected * short.len_utf8());
            assert!(!read.range_complete && read.file_has_more);
            assert_eq!(read.next_start_byte, Some(read.returned_end_byte));
            assert_eq!(read.content.chars().count(), expected);
        }
    }

    #[test]
    fn exotic_c0_controls_keep_their_six_byte_json_cost() {
        let allowance = 8192 - 1024;
        // 0x0b (vertical tab) has no JSON short escape and costs six bytes.
        let source = '\u{000b}'.to_string().repeat(allowance);
        let (_dir, manager) = fixture(source.as_bytes());
        let read = manager
            .read_text_range_with_scope_and_output_limit("source.txt", 0, None, None, SCOPE, 8192)
            .unwrap();
        assert_eq!(read.returned_end_byte as usize, allowance / 6);
        assert!(!read.range_complete && read.file_has_more);
        // Mixed input: a six-byte control must not be admitted as a two-byte one.
        let mixed = "\u{0001}\t";
        let (_dir, manager) = fixture(mixed.as_bytes());
        let read = manager
            .read_text_range_with_scope_and_output_limit("source.txt", 0, None, None, SCOPE, 2048)
            .unwrap();
        assert!(read.range_complete);
        assert_eq!(read.content, mixed);
    }

    #[test]
    fn escape_budget_admits_exact_fit_and_stops_at_just_over() {
        let allowance = 8192 - 1024;
        // Exact fit: two-byte tab escapes plus one four-byte character. Their
        // escaped cost equals the allowance exactly, so the whole source fits.
        let exact = format!("{}\u{10400}", "\t".repeat(allowance / 2 - 2));
        let exact_escaped = (allowance / 2 - 2) * 2 + 4;
        assert_eq!(exact_escaped, allowance);
        let (_dir, manager) = fixture(exact.as_bytes());
        let read = manager
            .read_text_range_with_scope_and_output_limit("source.txt", 0, None, None, SCOPE, 8192)
            .unwrap();
        assert!(read.range_complete && !read.file_has_more && read.is_complete_file);
        assert_eq!(read.content, exact);
        assert_eq!(read.returned_end_byte as usize, exact.len());

        // Just over: one extra tab escape would exceed the allowance by two, so
        // the greatest fitting prefix is every tab but that last one. Admitted
        // tabs consume two encoded bytes each but one source byte.
        let just_over_tabs = allowance / 2 + 1;
        let just_over = "\t".repeat(just_over_tabs);
        let (_dir, manager) = fixture(just_over.as_bytes());
        let read = manager
            .read_text_range_with_scope_and_output_limit("source.txt", 0, None, None, SCOPE, 8192)
            .unwrap();
        assert!(!read.range_complete && read.file_has_more);
        assert!(just_over.is_char_boundary(read.returned_end_byte as usize));
        assert_eq!(read.returned_end_byte as usize, just_over_tabs - 1);
        assert_eq!(read.content, "\t".repeat(just_over_tabs - 1));
    }
    #[test]
    fn text_continuation_rejects_changed_base_and_path_escape() {
        let (directory, manager) = fixture(b"before\r\n");
        let base = manager
            .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
            .unwrap();
        fs::write(directory.path().join("source.txt"), "after!\r\n").unwrap();
        assert!(matches!(
            manager.read_text_range_with_scope("source.txt", 1, None, Some(&base.sha256), SCOPE),
            Err(WorkspaceError::ExpectedDigestMismatch(_))
        ));
        assert!(manager
            .read_text_range_with_scope("../outside", 0, None, None, SCOPE)
            .is_err());
        assert!(manager
            .prepare_text_edit_with_scope("../outside", &base.sha256, "a", "b", SCOPE)
            .is_err());
    }

    #[test]
    fn edit_rejection_distinguishes_valid_digest_from_invalid_text_fields() {
        let (directory, manager) = fixture(b"before\r\n");
        let base = manager
            .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
            .unwrap();
        for (old, new, expected) in [
            ("", "after", "old_text must be nonempty"),
            ("before", "before", "new_text must differ from old_text"),
            ("before", "after\0", "edit text must not contain NUL"),
            ("before\n", "after\n", "edit text line endings do not match"),
            ("absent\r\n", "after\r\n", "edit text was not found"),
        ] {
            let error = manager
                .prepare_text_edit_with_scope("source.txt", &base.sha256, old, new, SCOPE)
                .unwrap_err();
            assert!(matches!(error, WorkspaceError::InvalidText(reason) if reason == expected));
            assert_eq!(
                std::fs::read(directory.path().join("source.txt")).unwrap(),
                b"before\r\n"
            );
        }
    }

    #[test]
    fn read_reports_line_endings_from_the_same_raw_snapshot() {
        for (bytes, expected) in [
            (&b"single line"[..], TextLineEndingStyle::None),
            (&b"one\ntwo\n"[..], TextLineEndingStyle::Lf),
            (&b"one\r\ntwo\r\n"[..], TextLineEndingStyle::CrLf),
            (&b"one\r\ntwo\n"[..], TextLineEndingStyle::Mixed),
            (&b"one\rtwo\n"[..], TextLineEndingStyle::BareCr),
        ] {
            let (_directory, manager) = fixture(bytes);
            let read = manager
                .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
                .unwrap();
            assert_eq!(read.line_ending_style, expected);
            assert_eq!(read.sha256, sha256_digest(bytes));
            assert_eq!(read.content.as_bytes(), bytes);
        }
    }

    #[test]
    fn preserve_mode_maps_one_logical_match_to_raw_crlf_and_lf_spans() {
        let crlf = "prefix\r\nfn sample() {\r\n    let quoted = \\\"\\\\n\\\";\r\n    println!(\\\"näide 中\\\");\r\n}\r\nsuffix\r\n";
        let (crlf_directory, crlf_manager) = fixture(crlf.as_bytes());
        let crlf_digest = sha256_digest(crlf.as_bytes());
        assert!(matches!(
            crlf_manager.prepare_text_edit_with_scope(
                "source.txt",
                &crlf_digest,
                "fn sample() {\n    let quoted = \\\"\\\\n\\\";\n    println!(\\\"näide 中\\\");\n}",
                "fn sample() {\n    let quoted = \\\"\\\\r\\\\n\\\";\n    println!(\\\"changed 中\\\");\n}",
                SCOPE,
            ),
            Err(WorkspaceError::InvalidText("edit text line endings do not match"))
        ));
        let patch = crlf_manager
            .prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &crlf_digest,
                "fn sample() {\n    let quoted = \\\"\\\\n\\\";\n    println!(\\\"näide 中\\\");\n}",
                "fn sample() {\n    let quoted = \\\"\\\\r\\\\n\\\";\n    println!(\\\"changed 中\\\");\n}",
                TextNewlineMode::Preserve,
                SCOPE,
            )
            .unwrap();
        let expected_crlf = "prefix\r\nfn sample() {\r\n    let quoted = \\\"\\\\r\\\\n\\\";\r\n    println!(\\\"changed 中\\\");\r\n}\r\nsuffix\r\n";
        assert_eq!(
            patch.mutations[0].after.as_deref(),
            Some(expected_crlf.as_bytes())
        );
        crlf_manager
            .apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &ApprovalGrant::for_capabilities([Capability::WriteFiles]),
                SCOPE,
                || false,
            )
            .unwrap();
        assert_eq!(
            fs::read(crlf_directory.path().join("source.txt")).unwrap(),
            expected_crlf.as_bytes()
        );

        let lf = "prefix\nold α\nold β\nsuffix";
        let (_lf_directory, lf_manager) = fixture(lf.as_bytes());
        let patch = lf_manager
            .prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &sha256_digest(lf.as_bytes()),
                "old α\r\nold β",
                "new α\r\nnew β",
                TextNewlineMode::Preserve,
                SCOPE,
            )
            .unwrap();
        assert_eq!(
            patch.mutations[0].after.as_deref(),
            Some("prefix\nnew α\nnew β\nsuffix".as_bytes())
        );
    }

    #[test]
    fn preserve_mode_handles_eof_deletion_single_line_and_truthful_noop() {
        for original in ["head\r\ntail", "head\r\ntail\r\n"] {
            let (_directory, manager) = fixture(original.as_bytes());
            let patch = manager
                .prepare_text_edit_with_mode_and_scope(
                    "source.txt",
                    &sha256_digest(original.as_bytes()),
                    "head\ntail",
                    "head\nchanged",
                    TextNewlineMode::Preserve,
                    SCOPE,
                )
                .unwrap();
            let suffix = if original.ends_with("\r\n") {
                "\r\n"
            } else {
                ""
            };
            assert_eq!(
                patch.mutations[0].after.as_deref(),
                Some(format!("head\r\nchanged{suffix}").as_bytes())
            );
        }

        let original = "one\r\nremove\r\nthree\r\n";
        let (_directory, manager) = fixture(original.as_bytes());
        let deletion = manager
            .prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &sha256_digest(original.as_bytes()),
                "remove\n",
                "",
                TextNewlineMode::Preserve,
                SCOPE,
            )
            .unwrap();
        assert_eq!(
            deletion.mutations[0].after.as_deref(),
            Some(&b"one\r\nthree\r\n"[..])
        );

        let no_newline = "single value";
        let (_directory, manager) = fixture(no_newline.as_bytes());
        assert!(manager
            .prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &sha256_digest(no_newline.as_bytes()),
                "value",
                "result",
                TextNewlineMode::Preserve,
                SCOPE,
            )
            .is_ok());
        assert!(matches!(
            manager.prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &sha256_digest(no_newline.as_bytes()),
                "value",
                "value\nnext",
                TextNewlineMode::Preserve,
                SCOPE,
            ),
            Err(WorkspaceError::InvalidText(
                "preserve mode cannot infer line endings for a source without newlines"
            ))
        ));

        let unchanged = "a\r\n";
        let (_directory, manager) = fixture(unchanged.as_bytes());
        assert!(matches!(
            manager.prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &sha256_digest(unchanged.as_bytes()),
                "a\n",
                "a\r\n",
                TextNewlineMode::Preserve,
                SCOPE,
            ),
            Err(WorkspaceError::EmptyMutation(_))
        ));
        assert!(matches!(
            manager.prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &sha256_digest(unchanged.as_bytes()),
                "a\n",
                "a\n",
                TextNewlineMode::Preserve,
                SCOPE,
            ),
            Err(WorkspaceError::EmptyMutation(_))
        ));
    }

    #[test]
    fn preserve_mode_rejects_ambiguous_or_unsupported_equivalence_without_mutation() {
        for (original, old, new, expected) in [
            (
                "x\r\ny\r\nx\r\ny\r\n",
                "x\ny",
                "z",
                "edit text has multiple matches",
            ),
            (
                "a\r\na\r\na\r\n",
                "a\na",
                "z",
                "edit text has multiple matches",
            ),
            (
                "one\r\ntwo\n",
                "one\ntwo",
                "changed",
                "preserve mode requires uniform LF or CRLF source",
            ),
            (
                "one\rtwo\n",
                "one\rtwo",
                "changed",
                "preserve mode arguments must not contain bare CR",
            ),
            (
                "one\r\ntwo\r\n",
                "one \ntwo",
                "changed",
                "edit text was not found",
            ),
        ] {
            let (directory, manager) = fixture(original.as_bytes());
            let error = manager
                .prepare_text_edit_with_mode_and_scope(
                    "source.txt",
                    &sha256_digest(original.as_bytes()),
                    old,
                    new,
                    TextNewlineMode::Preserve,
                    SCOPE,
                )
                .unwrap_err();
            assert!(matches!(error, WorkspaceError::InvalidText(reason) if reason == expected));
            assert_eq!(
                fs::read(directory.path().join("source.txt")).unwrap(),
                original.as_bytes()
            );
        }

        let mixed = "one\r\ntwo\n";
        let (_directory, manager) = fixture(mixed.as_bytes());
        assert!(matches!(
            manager.prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &sha256_digest(b"different"),
                "one\rtwo",
                "changed",
                TextNewlineMode::Preserve,
                SCOPE,
            ),
            Err(WorkspaceError::ExpectedDigestMismatch(_))
        ));
        assert!(manager
            .prepare_text_edit_with_scope(
                "source.txt",
                &sha256_digest(mixed.as_bytes()),
                "two\n",
                "changed\n",
                SCOPE,
            )
            .is_ok());
    }

    #[test]
    fn preserve_mode_checks_materialized_crlf_size_and_stale_raw_preimage() {
        let original = "old\r\n";
        let (directory, manager) = fixture(original.as_bytes());
        let digest = sha256_digest(original.as_bytes());
        let oversized_after_expansion = "\n".repeat(MAX_TARGETED_EDIT_BYTES / 2 + 1);
        assert!(matches!(
            manager.prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &digest,
                "old\n",
                &oversized_after_expansion,
                TextNewlineMode::Preserve,
                SCOPE,
            ),
            Err(WorkspaceError::InvalidText(
                "materialized new_text exceeds 16 KiB"
            ))
        ));

        let patch = manager
            .prepare_text_edit_with_mode_and_scope(
                "source.txt",
                &digest,
                "old\n",
                "new\n",
                TextNewlineMode::Preserve,
                SCOPE,
            )
            .unwrap();
        fs::write(directory.path().join("source.txt"), b"user\r\n").unwrap();
        assert!(matches!(
            manager.apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &ApprovalGrant::for_capabilities([Capability::WriteFiles]),
                SCOPE,
                || false,
            ),
            Err(WorkspaceError::StaleMutation(_))
        ));
        assert_eq!(
            fs::read(directory.path().join("source.txt")).unwrap(),
            b"user\r\n"
        );
    }

    #[test]
    fn text_multibyte_characters_straddle_64k_and_128k_boundaries() {
        for boundary in [64 * 1024, 128 * 1024] {
            let original = format!("{}\u{10400}\r\nend", "a".repeat(boundary - 1));
            let (_dir, manager) = fixture(original.as_bytes());
            let first = manager
                .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
                .unwrap();
            assert!(manager
                .read_text_range_with_scope("source.txt", 0, Some(boundary as u64), None, SCOPE)
                .is_err());
            let last = manager
                .read_text_range_with_scope(
                    "source.txt",
                    (boundary - 1) as u64,
                    None,
                    Some(&first.sha256),
                    SCOPE,
                )
                .unwrap();
            assert_eq!(last.content, "\u{10400}\r\nend");
        }
    }

    #[test]
    fn snapshot_io_is_bounded_even_when_source_keeps_growing() {
        struct GrowingReader(usize);
        impl Read for GrowingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                assert!(self.0 + buffer.len() <= MAX_TEXT_SNAPSHOT_BYTES + 1);
                buffer.fill(b'x');
                self.0 += buffer.len();
                Ok(buffer.len())
            }
        }
        assert!(matches!(
            decode_snapshot(Path::new("fixture"), GrowingReader(0)),
            Err(WorkspaceError::FileTooLarge { .. })
        ));
    }

    #[test]
    fn targeted_deletion_preserves_utf8_and_crlf_and_bounds_after_image() {
        let (dir, manager) = fixture("é\r\nremove\r\n中\r\n".as_bytes());
        let digest = sha256_digest("é\r\nremove\r\n中\r\n".as_bytes());
        let patch = manager
            .prepare_text_edit_with_scope("source.txt", &digest, "remove\r\n", "", SCOPE)
            .unwrap();
        manager
            .apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &ApprovalGrant::for_capabilities([Capability::WriteFiles]),
                SCOPE,
                || false,
            )
            .unwrap();
        assert_eq!(
            fs::read(dir.path().join("source.txt")).unwrap(),
            "é\r\n中\r\n".as_bytes()
        );
        let original = format!("unique{}", "a".repeat(MAX_TEXT_SNAPSHOT_BYTES - 6));
        let (_dir, manager) = fixture(original.as_bytes());
        assert!(matches!(
            manager.prepare_text_edit_with_scope(
                "source.txt",
                &sha256_digest(original.as_bytes()),
                "unique",
                "unique!",
                SCOPE
            ),
            Err(WorkspaceError::PatchLimitExceeded)
        ));
    }

    #[test]
    fn targeted_edit_preserves_unseen_bytes_and_rechecks_stale_base() {
        let original = format!("{}\r\nunique_target = 1;\r\n尾\r\n", "z".repeat(350_000));
        let (directory, manager) = fixture(original.as_bytes());
        let base = manager
            .read_text_range_with_scope("source.txt", 0, None, None, SCOPE)
            .unwrap();
        let patch = manager
            .prepare_text_edit_with_scope(
                "source.txt",
                &base.sha256,
                "unique_target = 1",
                "unique_target = 2",
                SCOPE,
            )
            .unwrap();
        let expected = original.replacen("unique_target = 1", "unique_target = 2", 1);
        assert_eq!(
            patch.mutations[0].before.as_deref(),
            Some(original.as_bytes())
        );
        assert_eq!(
            patch.mutations[0].after.as_deref(),
            Some(expected.as_bytes())
        );
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);
        manager
            .apply_prepared_patch_with_scope_and_grant_cancellable(&patch, &grant, SCOPE, || false)
            .unwrap();
        assert_eq!(
            fs::read(directory.path().join("source.txt")).unwrap(),
            expected.as_bytes()
        );
        assert!(matches!(
            manager.prepare_text_edit_with_scope(
                "source.txt",
                &base.sha256,
                "unique_target = 2",
                "unique_target = 3",
                SCOPE
            ),
            Err(WorkspaceError::ExpectedDigestMismatch(_))
        ));
        fs::write(directory.path().join("source.txt"), "user change").unwrap();
        assert!(matches!(
            manager.apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &grant,
                SCOPE,
                || false
            ),
            Err(WorkspaceError::StaleMutation(_))
        ));
        assert_eq!(
            fs::read(directory.path().join("source.txt")).unwrap(),
            b"user change"
        );
    }

    #[test]
    fn targeted_edit_rejects_missing_duplicate_overlapping_and_invalid_matches() {
        for (original, old, new) in [
            ("text", "missing", "new"),
            ("x x", "x", "y"),
            ("aaa", "aa", "b"),
            ("text", "", "x"),
            ("text", "text", "text"),
            ("text", "text", "\0"),
        ] {
            let (directory, manager) = fixture(original.as_bytes());
            let digest = sha256_digest(original.as_bytes());
            assert!(manager
                .prepare_text_edit_with_scope("source.txt", &digest, old, new, SCOPE)
                .is_err());
            assert_eq!(
                fs::read(directory.path().join("source.txt")).unwrap(),
                original.as_bytes()
            );
        }
        let (_directory, manager) = fixture(b"a");
        let digest = sha256_digest(b"a");
        assert!(manager
            .prepare_text_edit_with_scope(
                "source.txt",
                &digest,
                "a",
                &"b".repeat(MAX_TARGETED_EDIT_BYTES + 1),
                SCOPE
            )
            .is_err());
        assert!(manager
            .prepare_text_edit_with_scope(
                "source.txt",
                &Sha256Digest("bad".into()),
                "a",
                "b",
                SCOPE
            )
            .is_err());
        let (_dir, manager) = fixture(&vec![b'a'; MAX_TEXT_SNAPSHOT_BYTES]);
        let digest = sha256_digest(&vec![b'a'; MAX_TEXT_SNAPSHOT_BYTES]);
        assert!(manager
            .prepare_text_edit_with_scope("source.txt", &digest, "a", "aa", SCOPE)
            .is_err());
    }

    #[test]
    fn targeted_edit_denied_cancelled_and_rollback_use_existing_transaction() {
        let (directory, manager) = fixture(b"original\r\n");
        let digest = sha256_digest(b"original\r\n");
        let patch = manager
            .prepare_text_edit_with_scope("source.txt", &digest, "original", "changed", SCOPE)
            .unwrap();
        assert!(manager
            .apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &ApprovalGrant::default(),
                SCOPE,
                || false
            )
            .is_err());
        let grant = ApprovalGrant::for_capabilities([Capability::WriteFiles]);
        // Sandbox policy is enforced by the runtime authorization evaluator. This
        // lower-level executor receives a broker policy and an already-issued grant.
        let denied_manager = WorkspaceManager::new(
            CapabilityBroker::new(
                directory.path(),
                ApprovalPolicy::default().deny(Capability::WriteFiles),
            )
            .unwrap(),
        );
        assert!(denied_manager
            .apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &grant,
                SandboxScope::ReadOnly,
                || false
            )
            .is_err());
        assert!(matches!(
            manager.apply_prepared_patch_with_scope_and_grant_cancellable(
                &patch,
                &grant,
                SCOPE,
                || true
            ),
            Err(WorkspaceError::Cancelled)
        ));
        // The third cancellation check runs after the single image was written.
        let mut checks = 0;
        let result = manager.apply_prepared_patch_with_scope_and_grant_cancellable(
            &patch,
            &grant,
            SCOPE,
            || {
                checks += 1;
                checks >= 3
            },
        );
        assert!(matches!(result, Err(WorkspaceError::Cancelled)));
        assert_eq!(
            fs::read(directory.path().join("source.txt")).unwrap(),
            b"original\r\n"
        );
    }

    #[test]
    fn line_edit_preserves_exact_outside_bytes_and_literal_escape_data() {
        for separator in ["\n", "\r\n"] {
            let source =
                format!("prefix{separator}old one{separator}old two{separator}suffix{separator}");
            let (directory, manager) = fixture(source.as_bytes());
            let read = manager
                .read_text_lines_with_scope("source.txt", 2, 2, None, SCOPE)
                .unwrap();
            let selection = read.edit_selection.expect("complete eligible lines");
            let lines = vec![
                r#"  let text = "\r\n";"#.to_owned(),
                "\tprintln!(\"näide 中\");".to_owned(),
            ];
            let patch = manager
                .prepare_text_line_edit_with_scope(
                    "source.txt",
                    &selection.expected_sha256,
                    selection.start_line,
                    selection.line_count,
                    &lines,
                    SCOPE,
                )
                .unwrap();
            let expected = format!(
                "prefix{separator}{}{separator}{}{separator}suffix{separator}",
                lines[0], lines[1]
            );
            assert_eq!(
                patch.mutations[0].after.as_deref(),
                Some(expected.as_bytes())
            );
            assert_eq!(
                patch.mutations[0].before.as_deref(),
                Some(source.as_bytes())
            );
            assert_eq!(
                fs::read(directory.path().join("source.txt")).unwrap(),
                source.as_bytes()
            );
            assert!(expected.contains(r#""\r\n""#));
        }
    }

    #[test]
    fn line_edit_preserves_eof_termination_and_rejects_unsupported_cases() {
        for (source, expected) in [
            ("prefix\nold", "prefix\nnew"),
            ("prefix\nold\n", "prefix\nnew\n"),
            ("prefix\r\nold", "prefix\r\nnew"),
            ("prefix\r\nold\r\n", "prefix\r\nnew\r\n"),
        ] {
            let (_directory, manager) = fixture(source.as_bytes());
            let digest = sha256_digest(source.as_bytes());
            let patch = manager
                .prepare_text_line_edit_with_scope(
                    "source.txt",
                    &digest,
                    2,
                    1,
                    &["new".into()],
                    SCOPE,
                )
                .unwrap();
            assert_eq!(
                patch.mutations[0].after.as_deref(),
                Some(expected.as_bytes())
            );
        }
        for source in ["one\ntwo\r\n", "one\rtwo\n", "single", ""] {
            let (directory, manager) = fixture(source.as_bytes());
            let digest = sha256_digest(source.as_bytes());
            assert!(manager
                .prepare_text_line_edit_with_scope(
                    "source.txt",
                    &digest,
                    1,
                    1,
                    &["changed".into()],
                    SCOPE,
                )
                .is_err());
            assert_eq!(
                fs::read(directory.path().join("source.txt")).unwrap(),
                source.as_bytes()
            );
        }
        let (directory, manager) = fixture(b"prefix\r\nold\r\n");
        let digest = sha256_digest(b"prefix\r\nold\r\n");
        for (start, count, lines) in [
            (0, 1, vec!["new".into()]),
            (2, 0, vec!["new".into()]),
            (2, 201, vec!["new".into()]),
            (3, 1, vec!["new".into()]),
            (2, 1, vec![]),
            (2, 1, vec!["old".into()]),
            (2, 1, vec!["new\nline".into()]),
            (2, 1, vec!["new\rline".into()]),
            (2, 1, vec!["x".repeat(MAX_TARGETED_EDIT_BYTES)]),
        ] {
            assert!(manager
                .prepare_text_line_edit_with_scope(
                    "source.txt",
                    &digest,
                    start,
                    count,
                    &lines,
                    SCOPE,
                )
                .is_err());
        }
        assert!(matches!(
            manager.prepare_text_line_edit_with_scope(
                "source.txt",
                &sha256_digest(b"stale"),
                2,
                1,
                &["new".into()],
                SCOPE,
            ),
            Err(WorkspaceError::ExpectedDigestMismatch(_))
        ));
        assert_eq!(
            fs::read(directory.path().join("source.txt")).unwrap(),
            b"prefix\r\nold\r\n"
        );
        let large = format!("{}\r\nnext\r\n", "x".repeat(MAX_TARGETED_EDIT_BYTES));
        let (_directory, manager) = fixture(large.as_bytes());
        assert!(manager
            .prepare_text_line_edit_with_scope(
                "source.txt",
                &sha256_digest(large.as_bytes()),
                1,
                1,
                &["short".into()],
                SCOPE,
            )
            .is_err());
        let maximum = "a\n".repeat(usize::from(MAX_TEXT_LINE_COUNT));
        let (_directory, manager) = fixture(maximum.as_bytes());
        assert!(manager
            .prepare_text_line_edit_with_scope(
                "source.txt",
                &sha256_digest(maximum.as_bytes()),
                1,
                MAX_TEXT_LINE_COUNT,
                &vec!["b".into(); usize::from(MAX_TEXT_LINE_COUNT)],
                SCOPE,
            )
            .is_ok());
    }
}
