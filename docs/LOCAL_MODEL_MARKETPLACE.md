<!-- SPDX-License-Identifier: AGPL-3.0-only -->
<!-- Copyright (C) 2026 AG064 -->
# Model marketplace

Status: on the development branch for Windows desktop, not released. The
interface is a minimal first version. The interface revamp is planned for 0.2.0.

Settings > Providers has a "Model marketplace (Hugging Face)" section under the
local model add-on. It finds GGUF models on Hugging Face and downloads one into
the same model folder the add-on uses, `<data>/models/`. Any GGUF there can be
chosen as the llama.cpp model, and starting it asks for approval as before.

## What you see

1. Type a search and press Search. Argentum asks the public Hugging Face API for
   GGUF repositories matching it, most downloaded first, and shows downloads,
   likes, license and whether the model is gated.
2. Open a repository to see its GGUF files: each quantization with its size and
   file names, split models as one entry with their parts, and vision
   projectors (`mmproj`) separately. The repository line shows license, context
   length, parameter count, architecture and the commit the files come from.
3. Review a file to see exactly what would be downloaded: each file's path and
   size, the SHA-256 it is checked against, the license, the disk space it
   needs against the free space on the drive, and the folder it goes to. If the
   drive does not have the space plus 1 GiB, Download stays disabled.
4. Download fetches the files. Cancel stops it and keeps what was downloaded,
   so downloading again resumes. "Discard partial download" deletes it.

## Checks

- Every file is pinned to the commit that was listed, so a repository change
  during a download cannot swap the file.
- Each file is checked against the SHA-256 that Hugging Face publishes for it,
  its Git LFS object id, and its exact size. A file that does not match is
  deleted. Files with no published SHA-256 are not offered.
- A partial download is kept as `<file>.download` with a small record of the
  repository, commit, path, size and SHA-256. It is resumed with an HTTP range
  request only if that record matches. If the server ignores the range, the
  file starts over.
- Repository names and file names are checked strictly. A path that could
  leave the model folder is not offered.
- Model files have no publisher signature. The checksum proves the file is the
  one Hugging Face lists, not who made it. Read the model card and license.

## Gated models and the token

Public models need no account. A gated model needs a Hugging Face token and
its terms accepted on huggingface.co. "Set Hugging Face token" opens the
Windows credential prompt, and the token is kept in the Windows Credential
Manager under `marketplace/huggingface-token`. It is never shown, logged or
placed in the interface's text. It is sent only to huggingface.co. When a
download redirects to Hugging Face's storage host, the token is not sent
there. "Remove Hugging Face token" deletes it.

## Network use

Requests go to huggingface.co (and the storage host a download redirects to)
only when you press Search, Open or Download. Nothing about your computer is
sent. See the no telemetry rule in `SECURITY.md`.

## Limits

- A split model's later parts also appear as files in the llama.cpp model
  list. Pick the first part.
- A downloaded `mmproj` file is not added to the model's `.adapters` list
  automatically. Add an `mmproj <file>` line as described in
  `docs/LOCAL_MODEL_ADDON.md`.
- The code is in `crates/argentum-update/src/hub.rs` (client and download) and
  `crates/argentum-app/src/market.rs` (Settings).
