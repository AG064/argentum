# Assurance case

This document argues why Argentum meets the security requirements in
[SECURITY.md](../SECURITY.md). It covers the threat model, the trust boundaries,
how the design follows secure design principles, and how common implementation
weaknesses are countered. [architecture.md](architecture.md) describes the
components it refers to.

## Security requirements

Argentum is a single-user agent harness. Its requirements are the ones listed under
"What Argentum protects against" in SECURITY.md, in short:

1. The project's saved policy alone decides what the agent may do.
2. File tools stay inside the chosen project folder.
3. Writes, commands, network access and other programs need approval for the exact
   action, unless the owner's profile allows them.
4. Provider keys stay in the operating system's credential store and go only to
   their own provider.
5. Release files can be checked for integrity and origin.

## Assumptions

- The operating system, the owner's account and the device are not compromised.
- The owner reads approval prompts before approving them.
- Model providers, models, web content, tool output and files in the project are
  untrusted.

## Trust boundaries and threats

| Boundary | What crosses it | Main threats | How they are countered |
| --- | --- | --- | --- |
| Owner and app | Prompts, approvals, settings | Approval of something other than what was shown | An approval is bound to the exact action, command, arguments and path it was shown for. A changed action needs a new approval. |
| App and model provider | Prompts and file content out; replies and tool calls in | Prompt injection, tool calls that try to gain authority, malformed replies | Model output is data. Tool calls are checked against the saved policy by the capability broker; nothing in a reply can change the policy. Replies are parsed with bounded sizes. |
| App and project folder | File reads and writes | Path traversal, symbolic link escape, edits based on stale content | Paths are resolved and must stay inside the canonical project folder; paths through symbolic links are refused. An edit must match the content the agent read. Every change is recorded and can be restored. |
| App and child processes | Commands the agent runs | Command injection, runaway processes, secret leakage through the environment | Commands run from an argument list after approval, with a cleared environment and output limits. On Windows a job object caps CPU and memory and stops the process tree when Argentum exits. |
| App and paired devices | Tasks and session data over the local network | Eavesdropping, impersonation, a paired device gaining authority | Devices pair with Ed25519 identities. Each connection uses ephemeral X25519 key agreement, HKDF-SHA256 and ChaCha20-Poly1305, so traffic has forward secrecy. A paired device cannot choose Full Access, and the working device keeps its own approvals. |
| App and GitHub (updates) | Release metadata and files | A tampered or wrong update | Updates run only on request, are checked against SHA256SUMS.txt from the same release, and roll back if the install fails. Release files also carry GitHub build attestations. |
| App and the local model add-on | Downloads of llama.cpp and a model | Tampered downloads | Pinned addresses and pinned SHA-256 hashes. The server listens on 127.0.0.1 only. |

## Secure design principles

- **Economy of mechanism:** one capability broker decides every tool action. There is
  no second path around it.
- **Fail-safe defaults:** actions are refused unless the saved policy allows them.
  Optional features such as device pairing, All files access and the local model
  add-on are off until the owner turns them on.
- **Complete mediation:** every tool call is checked when it runs, not only when a
  run starts. A changed action needs a new decision.
- **Open design:** the source is public under the AGPL-3.0. Security does not depend
  on hiding how it works.
- **Separation of privilege:** the model proposes and the owner approves. Model
  output, tool arguments and provider metadata cannot grant authority.
- **Least privilege:** commands get a cleared environment and resource caps. The Git
  tools have no push, merge, rebase, reset, tag or remote commands. Each provider key
  goes only to its own provider.
- **Least common mechanism:** each project has its own policy, approvals and
  sessions. Approvals do not carry over to another project.
- **Psychological acceptability:** approval prompts show the full command, its
  folder and its time limit, and each approval says how long it lasts.

## Common weaknesses and how they are countered

| Weakness | Countermeasure |
| --- | --- |
| Memory safety (CWE-119 and related) | The code is Rust. `unsafe` is limited to operating-system interfaces: Windows APIs, Android JNI and process control. |
| Path traversal (CWE-22) and link following (CWE-59) | Canonical project root, refusal of `..` and symbolic links, tests for each case. |
| OS command injection (CWE-78) | Commands run from an argument list, and approvals bind the exact command and arguments. |
| SQL injection (CWE-89) | SQLite through rusqlite with bound parameters. Only fixed internal table and column names are placed in query text. |
| Unsafe parsing of untrusted data (CWE-20, CWE-502) | Typed deserialization with size limits. The request and context parsers have fuzz targets that run in CI. |
| Exposure of secrets (CWE-312, CWE-532) | Keys live in Windows Credential Manager or the Android Keystore. Session exports and logs redact known secret values. |
| Race between check and use (CWE-367) | Edits are refused when the file changed after it was read. |
| Resource exhaustion (CWE-400) | Output limits, request limits per run, and job object caps for child processes. |
| Weak cryptography (CWE-327) | TLS through rustls with certificate verification. ChaCha20-Poly1305, X25519, Ed25519, Argon2 and SHA-256 from maintained libraries. No home-made primitives. |
| Supply chain | Locked dependencies, `cargo deny` for advisories, licenses and sources, Dependabot, pinned GitHub Actions, build attestations and an SBOM for every release. |

## Evidence

- About 1,900 automated tests, including tests for the boundaries above, run in CI
  on every push and pull request.
- Clippy with warnings as errors, CodeQL, OpenSSF Scorecard and `cargo deny` run in
  CI.
- Fuzz targets for untrusted input run in CI.
- Release files are built in GitHub Actions with build provenance attestations.

## Residual risks

These are accepted and documented in SECURITY.md under "What Argentum does not
protect against": Argentum is not an operating-system sandbox, data sent to a
provider follows that provider's terms, conversation history is not encrypted by
Argentum, and malware on the same account can read Argentum's data.
