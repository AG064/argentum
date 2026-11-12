# Local model add-on

Status: included in 0.1.0 for Windows desktop.

The Windows downloads do not include llama.cpp or any model. Local inference is an
optional add-on that you install from Settings > Providers, in the "Local model
add-on" section at the bottom of the page. You can remove it again. Argentum works
fully without it, and hosted providers and custom OpenAI-compatible servers do not
depend on it.

## What you see

1. Settings lists three models with their size, license and source.
2. "Review" shows exactly what would be downloaded: each file, its size, its host,
   how it is checked and the folder it goes to. Files already in place are left out.
3. Nothing is downloaded until you press "Download and install". That press
   approves only the files shown. "Cancel download" stops the download and rolls
   it back.
4. Once installed, "Remove add-on" stops the local model, then deletes the runtime
   and the models from the list below. Other model files you added stay.

Installing does not start the model. The runtime starts only when you choose the
local model and approve it.

If llama.cpp is selected and the add-on is not installed, Argentum says "the local
model add-on is not installed" and points you to Settings.

## What is downloaded

| Item | Source | Checks |
| --- | --- | --- |
| llama.cpp b10621, Windows x64, Vulkan | github.com/ggml-org/llama.cpp release | Pinned size and SHA-256, the publisher's signed attestation, then the runtime's own archive checks |
| Qwen3.5 0.8B Q8_0 | Hugging Face, ggml-org/Qwen3.5-0.8B-GGUF at a pinned commit | Pinned size and SHA-256 |
| Qwen3.5 2B Q4_K_M | Hugging Face, unsloth/Qwen3.5-2B-GGUF at a pinned commit | Pinned size and SHA-256 |
| Qwen3.5 4B Q4_K_M | Hugging Face, unsloth/Qwen3.5-4B-GGUF at a pinned commit | Pinned size and SHA-256 |

The pins are in `crates/argentum-update/src/addon.rs`. The runtime pin must match
the local runtime's own pin in `crates/argentum-providers/src/runtime_bundles.rs`.

## Publisher signature

The llama.cpp release workflow publishes a GitHub artifact attestation for each
build: a Sigstore bundle with a statement listing every file and its SHA-256.
Argentum checks it offline, before any download
(`crates/argentum-update/src/sigstore.rs`):

1. The signing certificate chains to the pinned Sigstore Fulcio root, for code
   signing, at the time the Rekor log recorded.
2. The certificate names the workflow
   `https://github.com/ggml-org/llama.cpp/.github/workflows/release.yml@refs/heads/master`
   and the issuer `https://token.actions.githubusercontent.com`.
3. The statement's signature verifies with the certificate's key.
4. Rekor's signed entry timestamp verifies with the pinned Rekor key, and the log
   entry is for this statement, signature and certificate.
5. The statement lists the runtime archive's file name with the pinned SHA-256.

The bundle and trust material are in `crates/argentum-update/pins/`.

Limits:

- The Merkle inclusion proof is not checked. The signed entry timestamp is Rekor's
  promise of inclusion, which is what the time check needs.
- Hugging Face publishes no signature for model files, so models rest on their
  pinned SHA-256 alone. The Review text says so.

## Where files go

Under the Argentum data folder (`<data>`):

- Runtime: `<data>/runtimes/llama-cpp/v0.3.0/vulkan-x64/bundle.zip`
- Models: `<data>/models/`
- Temporary files: `<data>/addons/`

Installing goes in this order:

1. Leftovers from an earlier attempt are rolled back.
2. The attestation is checked.
3. Each missing file downloads into `addons/staging/` as a `.partial` file and is
   kept only if its size and SHA-256 match. The runtime archive must also pass
   the check the runtime repeats before every launch.
4. A journal lists the paths about to be placed. The files are moved into place.
5. Deleting the journal completes the install.

A failure or cancellation before step 5 removes what the attempt added. If the
process is killed, the next start rolls back using the journal.
