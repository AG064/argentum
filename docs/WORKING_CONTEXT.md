# Durable working context and compaction contract (F06A/F06B)

This is the implementation contract, not a completion or release receipt.
F06B adds declared model budgets and derived summary checkpoints to F06A.
Engineering completion and remaining acceptance gates belong to the dated receipts.

CLI reliability correction: each run retains up to 33 ordered context groups
(user, exchanges and final answer) for at most 32 model requests. Schema 20
atomically replaces the former nine-group counter constraint while preserving
the original run and child records. Failed reads can retain fixed error codes
when backed by a failed execution receipt and the current read policy. The last
four closed run outcomes for the current session are appended to each request
outside the lossy summary. They contain status, a fixed failure reason and a
recorded change count, not raw exceptions, paths, credentials or authority.

## Retention policy

Capture and request selection are separate permission checks. A historical
observation never authorizes a tool, restores a grant, or establishes current
filesystem state. Original run, provider-call and runtime-call identities bind
observations to existing tool, change and verification evidence.

| Source | Permitted durable fields | Excluded fields |
| --- | --- | --- |
| User submission | Existing visible submission and identity | No additional credential capture |
| Completed assistant message | Visible text and identity | Undisclosed reasoning, old system prompts |
| `read_text` | Validated contained path, requested range and digest; complete bounded structured result including observed digest and content | Sensitive or excluded paths; malformed or oversized results |
| `edit_text` | Contained path, expected digest; structured outcome, change and revision references | Replacement text and old match text |
| Fixed verification | Runner identity, recorded status and revision/evidence references | Raw stdout, stderr, environment and process arguments |
| Process execution | Recorded outcome and existing execution references | Command body, arguments, environment, raw stdout and stderr |
| Other tools | Identity, retention disposition and known outcome only | Raw arguments and payloads |
| Automatic F05 instructions | No bodies; current instructions are reloaded for each request | Historical automatic instruction and host prompt snapshots |

Credentials, authorization headers, approval tickets, grant material, private
configuration and raw environment dumps are not permitted payload classes.
Existing containment and context exclusions apply before storage and again before
selection. Capture additionally excludes conventional private names (`.env`,
`.env.*`, SSH key names, `.ssh`, `.aws`, `.azure`, `.codex`, `.argentum`,
`.npmrc`, `.pypirc`, credential/secret filenames, and private-key/keystore
extensions). Conservative authorization/key markers omit the complete read
result, without returning a redacted string under an unchanged file digest.
These checks are not a general secret scanner. Explicit allowed instruction-file reads remain historical tool
observations, not a second instruction source. Historical paths never extend
automatic F05 scope discovery.

Repository text can contain private information. Bounded allowed file content
is still private local data and can be sent to the selected provider on a later
authorized request. There is no universal secret detector or new encryption
claim. SQLite inherits the protections of its configured directory. F06A adds
neither database encryption nor owner-only ACL enforcement. Protect the database
and its backups as conversation data. Ordinary diagnostics
must contain metadata, not retained bodies or private arguments.

The existing user/assistant UI transcript, disclosed reasoning, usage records,
and file-change recovery images retain their previous storage contracts. F06A
does not remove or sanitize that earlier data. It adds no provider credential,
header, grant or environment source. Only its allowlisted historical tool
observations enter the new projection. User and assistant prose can itself
contain private information; no universal provenance or secret-detection claim
is made.

## Storage and projection requirements

Completed protocol groups commit at an explicit SQLite boundary. Unresolved
calls and partial streaming fragments are not completed observations. Preserve
known results from failed or cancelled work without guessing missing outcomes.
The context records reference existing effect receipts; they are not a second
effect ledger and cannot make an external effect atomic with SQLite. Recording
failure after an effect must report incomplete context, preserve its existing
receipt, and never repeat the action.

Each retained result has an explicit disposition: retained, redacted,
metadata-only, omitted by policy or limit, or unavailable. Historical placeholders
identify that disposition and never invent output or success. Unsafe pairing
omits the whole group with a visible reason. Source identities, not matching
text, prevent duplicate projection. Legacy messages remain usable without
manufacturing missing tool bodies. New bodies follow session deletion; no
cross-project retrieval index or silent retention eviction is permitted.

Selection must bound SQL reads before body allocation, preserve complete groups,
and choose a deterministic recent suffix. Metadata reports omitted source ranges,
counts and reasons independently of durable retention. Current task, authoritative
guidance and fresh F05 instructions are required. Required context exceeding
existing byte/message transport bounds fails before provider dispatch rather
than truncating structured results, dropping tools or altering the transcript.

Read-only inspection must use this same projection. It reports a current preview,
not an exact unrecorded prior request. It must not call providers, execute tools,
change permissions or write history. Model-budget accounting is a labelled
estimate over the same assembled representation used by admission, as below.

## Implemented schema and boundaries

SQLite schema 13 adds `context_runs` and `context_groups`. The former binds the
existing user-message ID, project, session, task and run, an expected next ordinal,
and whether a completed assistant answer was recorded. That flag is not a run
lifecycle or success claim. The latter stores version-1 groups keyed by
`(run_id, ordinal)`. User text must match the existing source. Ordinals must be
contiguous, and completed sources reject later appends. An exact duplicate source
write is accepted without another row; conflicting content is rejected.

The user group commits before provider or tool execution. Every completed
provider batch has its call IDs and protocol shape validated before its first
effect. Individual actions still undergo normal tool validation and authorization. Completed results
are then recorded together in original call order, with the provider and runtime
call IDs. The completed final assistant answer is stored separately, rather than
reusing the UI's aggregate of all round text. A failure or cancellation preserves
already completed groups. Known denial is `rejected`, an F05 refresh is `deferred`,
an unstarted tail is `not_attempted`, and genuinely unresolved work stays
`unresolved`. Missing bodies become explicit historical placeholders, not
invented tool output. Partial streaming text is not a completed context answer.

Observation receipts retain only the existing audit ID, recorded outcome and
exit code. Targeted-edit references retain existing file-change/change-set IDs,
revision and before/after digests. Verification retains allowlisted fields of
the existing evidence record, without program/arguments or raw output. Selection
checks these references against scoped authoritative records. It does not
revalidate old tests against today's filesystem or turn old digests into grants.

Legacy transcript rows are selected only for runs without a context header, with
missing tool history explicitly unavailable. The new and legacy paths never
overlap for a run. Both use one source reader and runtime projection. Ordering
uses original user-row order and group ordinal. A schema, payload, scope or
checkpoint mismatch produces a controlled error or a visible omission; deleting
all groups cannot make a retained checkpoint look like an empty complete history.
No migration fills old results by reading current files.

Debug formatting of new context sources is metadata-only. Malformed receipt
deserialization reports a fixed error instead of echoing rejected field values.

Foreign keys cascade new headers and bodies with session/project or source-user
deletion. There is no automatic pruning or eviction. This is logical SQLite
deletion, not secure erasure of database pages, WAL files or backups. Existing
retention and backup handling still applies.

## Resource bounds and inspection

| Resource | Bound |
| --- | --- |
| Visible text in one retained user/assistant record | 128 KiB UTF-8 |
| One retained tool result | Existing 64 KiB text-result bound |
| Allowlisted historical arguments per call | 16 KiB |
| Calls per group | Existing 8-call bound |
| Groups per run | 9, including user and final answer within the existing 8-round loop |
| Serialized durable group | 512 KiB |
| Source keys retrieved | 128 |
| Serialized source bodies retrieved in one projection | 512 KiB total |
| Selected historical protocol messages, including omission notice | 128 |
| Current guidance, task, exchanges and selected history | 512 KiB conservative JSON message resource bound |

Selection first sorts/limits metadata keys, then fetches individual bodies in
one read snapshot. SQL checks byte length before Rust string allocation and
honors the remaining retrieval budget. A resource window selects whole recent
groups, not prefixes of JSON or UTF-8. Serialized group pressure at capture omits
whole observations with `omitted_limit`, largest first; it does not cut their
structured output. Oversized completed assistant text gets an explicit omission
record. Existing transcript text is not rewritten.

The request byte accounting includes JSON string escaping and conservative
message-envelope allowances. Tool definitions retain their independent existing
catalog bounds and are never removed to make history fit. Four KiB is reserved
for a bounded history notice, not for model output. Required current context
overflow returns `context_capacity_exceeded` before the next provider request.
Recording failure returns `context_recording_failed`; earlier effect receipts
remain authoritative and no action is replayed to recover a missing observation.

`argentum context --json` and `/context` use the same projection as outbound
requests. The CLI supports the existing explicit isolated workspace/database
options. Inspection describes a next-task preview at the workspace root, without
a proposed prompt, and includes fresh F05 metadata or its current loading error.
It never claims to reproduce an unretained previous request. Adapters without
tool-history support omit complete exchange groups with an explicit reason.

Metadata distinguishes source/selected ranges, count and reason of omissions,
legacy/incomplete/checkpoint ranges, selected exchanges, retained result bytes,
per-disposition counts, and conservative selected message bytes. An older window
is the range before the first selected source, not deletion of those records.
Raw tool arguments and bodies are absent from this output. Unknown or invalid
model budgets appear as an accounting error, not an invented capacity.

## Model budget and counting contract (F06B)

The trusted local command is:

```text
argentum provider budget PROFILE_ID --limits TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY --json
```

Supply actual owner-reviewed integer limits, not the synthetic fixture values.
All five values are required by this command. They are stored separately from
credentials and bound to the exact profile ID, provider kind, normalized endpoint,
model and a new declaration UUID. Routing/model changes invalidate the declaration.
Labels and provider selection alone do not change those numerical limits.
JSONL, model tools, repository instructions and provider messages cannot declare
budgets. The command refuses active project work.

Every declaration is labelled `trusted_owner_supplied_not_provider_verified`.
Existing bundled hosted-model metadata and an available exact-origin/model
catalog provide additional ceilings, not usable input/output declarations.
Catalog cache keys include profile ID, endpoint/kind binding and model ID; usage
responses never populate that cache. Missing metadata still requires an owner
declaration. A conflicting available ceiling fails closed. A local model's
advertised window is never substituted for its configured runtime capacity.

Zero/invalid limits, overflow, a separate input limit above total capacity,
requested output above the declared maximum, or output plus uncertainty consuming
the entire window are rejected. Total capacity is bounded at 100 million units;
the uncertainty allowance must be at least 128. No requested output reserve is
silently reduced. The domain representation can express no separate input limit;
the CLI instead requires an explicit input ceiling.

No matching tokenizer is available in the supported request path. Counting is
`estimated_utf8_wire_bytes_v1_not_tokenizer_upper_bound`: one estimated input
unit per serialized UTF-8 request-body byte, including JSON escaping, complete
message envelopes, the entire tool catalog and schemas, model and generation
controls. An explicit owner uncertainty allowance covers unmeasured tokenizer,
template and provider-side transformations. This is deliberately conservative
for common text, not exact tokenization or a universal upper bound. HTTP
credentials are not model input and are not included in accounting or logs.
No tokenizer or model download is required.

F19A child requests use the same selected provider profile and declared model
budget but have a separate `delegated_readonly_generation` purpose. Each physical
request is admitted and recorded through the existing accounting and recovery
path. The child additionally has a 32,000-unit cumulative client-accounted
allowance across at most four physical requests. Missing or conflicting reported
usage is unresolved and stops the parent before another inference request. Child
compaction is disabled, and no child transcript becomes ordinary durable history.
Only the bounded parent-facing result envelope can enter later working context;
its brief is represented by byte count and SHA-256, and the result is omitted if
the capability, route, path authority, or typed source references are no longer
eligible.

Admission requires both:

```text
estimated input + requested output + uncertainty <= declared usable total
estimated input + uncertainty <= declared separate input limit, when present
```

The existing OpenAI-compatible/LM Studio chat representation and compatibility
Anthropic text representation share their actual wire builders with accounting.
They transmit the reserved value as `max_tokens`. Anthropic still does not admit
tool exchanges. Other local backend representations currently return the typed
`context_accounting_unsupported` failure; a declaration cannot enable an
unimplemented accounting/output-control route. This is an explicit limitation,
not local-provider acceptance or a fallback to theoretical GGUF capacity.
The accepted request types are text and supported complete tool groups, not
images or opaque multimodal blocks. Unknown payload representations are not
counted as zero. Server acceptance, tokenizer behavior and billing enforcement
remain provider acceptance work.

All main rounds and summarizer requests are checked. F01 catalog bounds and
F06A byte/message/loading bounds remain independent. Required current guidance,
fresh instructions, prompt, catalog and complete current exchanges are never
trimmed. If those alone cannot fit, no summarizer or next main request is sent.
Previously completed effects remain recorded and are not replayed.

## Small compaction path

`/compact` and `argentum compact --json` perform one explicit selected-model
operation. `/compact deactivate` and `argentum compact --deactivate --json`
clear only the derived active projection, reconstructing retained originals on
the next inspection/request. Deactivation does not call a provider or promise
that the larger uncompressed history fits.

Manual work acquires existing session admission and rejects a busy session.
Provider/tool activity and pending approvals prevent overlapping compaction.
Automatic compaction gets one attempt before an otherwise over-budget main
request, inside that task's admission, between tasks or settled tool rounds.
Compaction events are distinct from task terminal events and cannot advance the
FIFO. A failed or cancelled continuation retains existing paused-queue behavior.

Selection uses only the bounded, currently permitted F06A projection. It chooses
a contiguous eligible prefix and retains at least three recent groups verbatim.
Unresolved groups and source gaps cannot be silently covered. Earlier ranges
omitted by the F06A window are reported to the summarizer as omitted, not claimed
as summarized. There is one provider call, a 10-second summarizer deadline,
a 128 KiB prompt bound, an 8 KiB narrative bound and no recursive retries.
Failure to find a fitting source prefix is explicit. Nothing eligible is a
`no_op`, not successful compaction.

The request has no tools and uses the same authorized profile/model and output
reserve. It is visible model work that may consume usage. A returned tool call,
incomplete stream, invalid/overlong JSON or invalid source reference is rejected.
The local schema asks for objective, decisions/constraints, observed work,
unresolved work and the exact ordered source references. It does not depend on
a provider structured-output feature. Parsed F06A tool JSON stays structured in
the source envelope, avoiding another layer of escaped protocol JSON without
changing retained fields or content.

The host reconstructs separate evidence anchors from the validated source
projection. Original digests, read spans, change references and recorded outcomes
are not supplied by the model. In particular, an actual failed formatter remains
failed even when the regression preserving that failure passes. Narrative is
explicitly lossy and cannot authorize actions, establish current file state or
override those anchors. Schema/reference checks are not semantic-fidelity proof.

A candidate must preserve the exact full source coverage and raw suffix and
reduce the complete rebuilt request by at least 20 percent and 1,024 estimated
input units, including wrappers, anchors and complete catalog. Its target must
fit. No extra history is silently dropped to pass this test. Main requests are
reassembled with fresh instructions and readmitted after automatic compaction.

## Retention and checkpoint integrity

Schema 14 non-destructively adds `model_budgets` and `context_checkpoints`.
The existing transcript, F06A groups, receipts, verification evidence and change
journals are not replaced or rewritten. Candidates exist only in memory until
validation completes. A single SQLite transaction activates the whole checkpoint
body and coverage; there is no cursor pointing at an unfinished candidate.

The optimistic commit token is deliberately database-wide: SQLite connection
change count plus external data version. Any intervening database mutation can
reject a candidate, including unrelated session activity. Fresh source, profile,
budget and F05 checks are also required. Cancellation is rechecked inside the
activation transaction, including after lock acquisition. A failure before
commit leaves the previous complete checkpoint in place. A crash around commit
can leave the command outcome unknown, but reopening sees the old or new complete
projection, not a partial pointer. No inactive candidate cleanup or restart-time
model work is necessary because partial candidates were never persisted.

Checkpoints are bounded to 96 KiB and 96 original source identities. Replacement
flattens original coverage rather than keeping a growing summary chain. Reuse
reconstructs and hashes every contributing currently permitted source and checks
the host anchors again. Missing, revoked, policy-filtered or conflicting sources
invalidate the entire affected summary for that projection, including transitive
coverage. Raw permitted sources are then used with explicit omissions; summary
prose is not partially redacted. A source leaving the bounded F06A retrieval
window also makes the summary unavailable. This bound is intentional and is not
unbounded session memory.

Process payloads remain metadata-only; no other store is queried to restore
stdout, arguments or environment. Automatic instruction/host bodies are not
sources. Direct copies of current instruction lines and host markers in a
candidate are rejected conservatively; this is not a general paraphrase or
secret detector. Current F05 guidance is always reconstructed independently.
Historical source text and summaries remain private, untrusted data. Existing
deletion, filesystem containment, database and backup protections still apply.

Malformed or cross-scope stored checkpoints fail explicitly. The trusted local
deactivation path also works for malformed checkpoints without deleting evidence.
This is summary-checkpoint recovery, not F08 abandoned-run settlement or F09
exactly-once commands.

## Inspection and outcomes

`/context` and `context --json` remain provider-free metadata previews. They show
model/profile/endpoint, budget revision and source, method, uncertainty, input
components, output reserve, limits, estimated fit and summary/source coverage.
The preview has no future prompt and cannot promise that a future task fits.
Main-request accounting events have a separate internal request ID and describe
the actual assembled request, not vendor acceptance. They persist bounded
accounting metadata, never request bodies or summaries. Compaction progress and
completion are separate events; they do not settle a task.

Compaction JSON distinguishes success, no-op, failure and deactivation. It includes
the source references, checkpoint identity, before/after target accounting,
summarizer accounting when available, and reported usage or `null` for unknown.
Failure after reported usage preserves that observation. A failed candidate's
`after` accounting, when available, describes that rejected candidate, not an
active checkpoint. Progress goes to events/stderr, leaving CLI JSON parseable.
F07 budgets, F08 recovery, F09 replay, packaged TUI interaction, formal security
review and live-provider/summary-quality acceptance remain separate gates.
