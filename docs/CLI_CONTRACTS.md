# What the coding tools guarantee

This page describes what the agent's file and verification tools promise, so you know
what a successful result does and does not tell you. For commands, see the
[CLI guide](CLI_GUIDE.md). For who may approve what, see [SECURITY.md](../SECURITY.md).

## Search, read and edit

`search_text` needs a `path`, a `query` and `max_results`. It matches literal text by
default. Regular expression mode uses Rust regex syntax and matches one line at a time, so
a pattern cannot span lines. Search has limits on pattern size, files visited, and text returned. Results say when they are partial. Ignore rules and
the workspace's disclosure policy still apply.

Each search result includes a ready-made read selection and the file's SHA-256, so the
model can read and edit exactly what it found.

`read_text` takes a `path` inside the workspace. It reads a range of UTF-8 bytes (the end
is exclusive) or a range of lines. You can pass the `expected_sha256` from an earlier
read. Reading from a nonzero offset requires that SHA-256. The result states the range you
asked for, the range it returned, how much of the file remains, and the SHA-256 of the whole
file. Reading the end of a file does not mean the model saw all of it. Without a range, a
read returns a short preview, so the model should read focused ranges instead of whole
repositories.

`edit_text` needs a `path`, the `expected_sha256` of the file as the model last read it,
and one explicit edit form: exact old and new text that matches once, a line range taken
from a read, or a unified patch. An edit that is empty, changes nothing, matches more than
once, or is based on an old read is rejected. Argentum never guesses what was meant, never
fuzzy-matches, and never swaps in the file's current SHA-256 for you. Escape sequences,
UTF-8 and CRLF line endings are handled exactly as written. A file can be up to 1 MiB, and
one edit up to 16 KiB. If instructions for a folder change, Argentum reloads them before
the next edit.

Every edit goes through the policy check, your approval where the profile requires it, a
check that the file is unchanged, and a record of the before and after contents. After the
write, Argentum reads the file back. If the run is cancelled, the edit is rolled back. The
model gets a summary of the confirmed diff. A confirmed change does not prove that other
requirements still hold, so review what was removed and run the checks.

## Verification

Fixed runners, and project runners you have approved exactly, produce results tied to the
source and the runner definition they used. A failed run shows a trimmed, redacted copy of
its output and says which parts were left out. Starting a runner is not the same as it
finishing, so the model polls the job until it ends. A saved result never rebuilds output
that was not kept, and an old result does not count as current after the source changes.

`check_rust` runs Rust tests written by the model against a copy of a supported Rust
library project, in a restricted Windows process. Argentum checks the project, the source,
that you enabled the tool, cancellation and the resource limits. A pass means those tests
passed. It does not prove the project is correct.

`goal deliver` finishes only when the declared checks pass on the current source and the
model has acknowledged the latest feedback with no unfinished work. A finished run and a
passing check are different facts. If you require an attestation, only you can give it.
See [Native delivery](NATIVE_DELIVERY.md).

## History and failure

Run diagnostics hold only metadata, but the metadata can still identify a project. They
leave out paths, prompts, answers, reasoning, response bodies, raw errors, credentials and
approval data. When a field is missing or cut off, the output says so instead of showing
zero.

Change review uses the before and after copies Argentum saved, not Git. `changes page`
follows a cursor for one run, and it does not freeze a view while a run is still writing.
Restoring a change requires your exact approval and a file that still matches what the
change left.

Follow-up prompts run in the order you sent them, and each keeps its own meaning. The queue
pauses after a failure or a cancel. `/steer` text only affects model requests that have not
yet been built. It does not undo work already approved or done.

Saved history is a record, never permission. A forked session does not copy credentials,
approvals or goals.

Provider budgets, usage and account quotas are three different numbers. A model without a
budget cannot run. Usage that was never reported stays unknown. Argentum does not refund it
or make up a number. Retries and summaries count against the allowance. Recovery after a
crash does not repeat work or quietly resume a goal. Argentum has no background service, and the
model cannot give itself new permissions.
