// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Reads pictures the owner chose in the composer's picker. Only the size and
//! count are bounded here; the runtime checks the format, the Images module
//! and the model before anything is sent.

use std::path::PathBuf;

use argentum_domain::ImageAttachment;
use argentum_providers::{MAX_ATTACHED_IMAGES, MAX_ATTACHED_IMAGE_BYTES};

/// The first four readable files of at most 5 MiB, base64 encoded. Others are
/// left out.
pub fn read_image_attachments(paths: &[PathBuf]) -> Vec<ImageAttachment> {
    use base64::Engine;
    paths
        .iter()
        .filter_map(|path| {
            let size = std::fs::metadata(path).ok()?.len();
            if size > MAX_ATTACHED_IMAGE_BYTES as u64 {
                return None;
            }
            let bytes = std::fs::read(path).ok()?;
            Some(ImageAttachment {
                name: path.file_name()?.to_string_lossy().into_owned(),
                data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            })
        })
        .take(MAX_ATTACHED_IMAGES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_at_most_four_bounded_files() {
        let directory = tempfile::tempdir().expect("dir");
        let mut paths = Vec::new();
        for index in 0..6 {
            let path = directory.path().join(format!("{index}.png"));
            std::fs::write(&path, [index as u8; 8]).expect("write");
            paths.push(path);
        }
        let large = directory.path().join("large.png");
        std::fs::write(&large, vec![0_u8; MAX_ATTACHED_IMAGE_BYTES + 1]).expect("write");
        paths.insert(0, large);
        paths.insert(1, directory.path().join("missing.png"));
        let read = read_image_attachments(&paths);
        assert_eq!(
            read.iter()
                .map(|image| image.name.as_str())
                .collect::<Vec<_>>(),
            ["0.png", "1.png", "2.png", "3.png"]
        );
        assert_eq!(read[0].data_base64, "AAAAAAAAAAA=");
    }
}
