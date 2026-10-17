# Optional local document artifacts

This Windows-only development profile connects existing native read/guarded
authoring, optional MCP, image input, fixed verification and `goal deliver`.
It does not require Google, an OAuth client or another connected account.
The existing approved hosted model may receive permitted text and images;
local document processing is not a claim of fully offline inference.

The editable deliverable is UTF-8 Markdown. The bounded PDF profile supports
headings 1 through 3, prose, one-level lists, rectangular tables up to four
columns, rules and literal code. It rejects raw HTML, links/images, inline
emphasis, nested structures, executable templates and unsupported syntax.
Current font coverage is horizontal precomposed Latin, Greek and Cyrillic.
Missing glyphs, shaping outside that profile, overlong code lines, oversized
rows, documents or images fail explicitly. There is no universal office-format,
complex-script, OCR, WYSIWYG or remote-resource-fetching claim.

## Build and commands

Core defaults remain unchanged. Select the optional feature explicitly:

```powershell
cargo build -p argentum-cli --features artifacts --bin argentum --release --locked
cargo build -p argentum-artifacts --bin argentum-pdf-worker --release --locked
& $exe artifacts help
& $exe artifacts tools
& $exe artifacts inspect --config $configuration
```

`artifacts serve --config ABSOLUTE_FILE` is the reviewed local MCP producer.
It is not a model-selected process or an authority grant. Use normal extension
configuration and exact owner activation before a native model can invoke it.
The three scoped tools are `artifact_open`, `artifact_render` and `artifact_page`.
Markdown authoring/corrections use ordinary read and SHA-bound file tools.
No document-result authoring shortcut exists in the producer.

`artifacts verify --config ABSOLUTE_FILE` is the fixed verifier entry point.
Register it through the existing exact project-runner approval path and select
that runner as a native goal criterion. A manual invocation or its JSON result
is not itself a native runtime receipt. Project executable resolution uses the
trusted PATH; portable owner launchers may set a process-local package PATH,
without changing system or user configuration.

Configuration is a private, exact leased file outside the model workspace.
It pins two distinct input paths/hashes, one editable Markdown path, a generated
output directory, separate private worker/media roots, and exact worker/font/
PDFium identities. It contains no credentials or account access. Source text,
Markdown and generated records are data, not permission or execution authority.
Changing the definition or its pinned inputs removes the old activation.

## Actual artifact and image chain

Every generation preserves input identities, actual Markdown bytes, worker/
font/parser identities, actual PDF bytes/MIME/digest, render manifest and page
tile digests. Generated UUID directories are create-new; earlier generations
remain retained. Selection uses the explicitly observed previous artifact ID.
Stale Markdown or selection requires a fresh observation and a new proposal.
An exclusive empty selection-lock handle serializes producer processes across
generation and commit; termination releases the handle without replaying work.
Four retained generations are a finite storage guard, not automatic cleanup.

The worker runs only through the existing Windows LPAC byte-copy and queried
Job Object path: one process, 512 MiB, 30 seconds and bounded diagnostics/storage.
It has no network capability or owner credentials. The pinned PDFium build has
V8 and XFA disabled. Interactive forms, JavaScript, attachments, encryption and
malformed PDFs are rejected. This is the existing declared Windows boundary,
not a new sandbox against a compromised operating system or administrator.

Pages are rendered at a fixed width into overlapping static PNG tiles, each
within the existing 128 KiB, 1200 by 800 and decoded-size limits. No image limit
is increased. The reserved logical producer origin
`https://local-artifacts.argentum.invalid` identifies local artifact images;
it is not a fetched URL or a claim of web-origin content.

The normal supported ChatGPT image route sends actual PNG bytes. Other routes
remain explicitly unqualified. Images bind current run, exact producer
activation and selected-artifact context. When a newly selected artifact
supersedes an earlier one, later requests omit those earlier transient pixels
and label their retained tool text historical. This changes no prior charge,
receipt, event or source content. Revocation, foreign runs or changed producer
bindings still prevent dispatch. Historical images are not restored on restart.

Producer page preparation, actual serialized image input, transport acceptance,
terminal usage and model visual observations remain separate evidence. Preparing
a PNG or accepting an image header does not prove the model understood it.

## Verification and completion

The native verifier checks pinned source preservation, current Markdown/PDF/
render identity, a fresh isolated raw-PDF parse/render, matching pixel hashes,
text-token inventory and preparation coverage for every current page tile.
It does not prove natural-language correctness, table fact associations,
absence of clipping/overlap or model comprehension. Independent content and
visual review remain necessary for acceptance.

Changes to sources, editable Markdown, PDF, worker, style/font or rendered bytes
invalidate their downstream checks. Interrupted/failed work is not labelled
verified or silently replayed. Actual native completion additionally requires
current approved-runner receipts, normal feedback, model acknowledgement,
settled accounting and a terminal goal with no remaining work.

The declared profile completed one frozen local workflow on 2026-10-17 after
offline qualification and exact owner approval. The model read both sources,
authored the brief, received actual page pixels, obtained current fixed-verifier
results and acknowledged them before normal completed-goal delivery. Independent
content, rendering and preservation review passed. See [the scoped acceptance
record](NEXT03_LOCAL_ACCEPTANCE.md) for provenance and separate evidence dimensions.
This does not establish Google/connected-app acceptance, general document
compatibility, release readiness or a reliability rate.
