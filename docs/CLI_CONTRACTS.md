# Native coding contracts

These are implementation boundaries, not provider reliability or release claims.
Use [CLI_GUIDE.md](CLI_GUIDE.md) for commands and [SECURITY.md](../SECURITY.md)
for authority. The default confirmation policy is independent of presentation.

## Observation and edit

`search_text` requires `path`, `query` and `max_results`. Literal mode is default;
explicit regex mode uses Rust regex syntax per logical line with bounded
compilation, traversal, snippets, context and serialized output. It cannot
cross line boundaries. Search returns typed read/edit selections, SHA-bound
source identity and explicit partial coverage. Ignore and disclosure policy
remain enforced.

`read_text` accepts contained `path`, optional UTF-8 byte offsets (exclusive
end), or supported line selections, and optional observed `expected_sha256`.
A nonzero continuation requires the previous SHA. Returned JSON distinguishes
requested/returned span, remaining content and full-source digest. A tail read
is not proof that the whole file was shown. Default model previews are bounded;
use focused selections rather than sending a whole repository.

`edit_text` requires contained `path`, observed `expected_sha256` and an explicit
valid form. Unique exact old/new text, supported read-line selection and unified
patch forms retain their existing validators. Empty/no-op/ambiguous or stale
requests fail. No silent coercion, fuzzy matching, inferred mode, automatic
rebasing or digest substitution is provided. Literal escapes and UTF-8/CRLF
semantics stay distinct. One file snapshot is bounded at 1 MiB. A changed
instruction scope refreshes context before any newly submitted mutation.

Prepared before/after images pass policy, approval, stale checks, journaling,
effect readback and cancellation/rollback. Model feedback includes bounded
confirmed diff and source identities. A reported change does not prove that
unrelated requirements were preserved; review its actual removals and run checks.

## Verification and completion

Fixed or exactly owner-approved project runners return source/definition-bound
receipts. Failed output uses bounded redacted diagnostics with explicit omitted
regions, prefix counts and capture availability. Async admission is not terminal
completion; drain the returned typed poll selection. Historical receipt metadata
does not reconstruct unavailable raw output or make an old revision current.

Supplemental Rust checks are model-authored executable input admitted against
actual source under a separate restricted Windows boundary. Project eligibility,
source identity, capability enablement, cancellation and resource bounds are
checked. A positive result means those checks reported pass, not universal
correctness or an independent proof of every requirement.

`goal deliver` requires current declared checks and model acknowledgement of
current feedback with no reported unfinished work. Native Complete and
verification Passed are different facts. An owner attestation, when selected,
cannot be supplied by the model or supervisor. See [NATIVE_DELIVERY.md](NATIVE_DELIVERY.md).

## History and failure

Run diagnostics are metadata-only but not anonymous. Paths, prompts, answers,
reasoning, bodies, raw errors, credentials and authority material are excluded.
Bounded retrieval declares partial coverage and unavailable fields instead of
inventing zero. Change review uses retained journal images, not Git HEAD.
`changes page` follows a scope-bound cursor and does not promise a concurrent
snapshot. Restore requires exact owner approval and current postimage matching.

FIFO follow-ups preserve separate intent and pause after failure/cancellation;
safe-point steering affects only newly assembled requests, not admitted effects.
History is observation, never authorization. Fresh session forks do not copy
credentials, grants, goals, workspace files or unknown accounting disposition.

Provider budgets, usage and account quotas are different quantities. Missing
effective budget blocks dispatch. Unknown actual usage cannot be refunded or
invented. Retry/compaction requests retain their identities and count against
the relevant allowance. Recovery does not replay effects or silently resume a
goal. No autonomous background daemon or model-owned permission expansion is
implied by a successful foreground workflow.
