# Optional governed extensions and local browser work

Build `argentum-cli` with `--features extensions` to include this workflow.
The portable packager's explicit `-Extensions` switch identifies this feature
selection in provenance and includes the adapter source/lock, not Node, browser
binaries, profiles, credentials or owner definitions. Default packaging is
unchanged and excludes this feature.
The default CLI does not link the external MCP client. Starting either build
does not install packages, start a browser/server, download dependencies or
contact an extension endpoint. Activation and actual execution are separate.

## Package and owner lifecycle

The supported portable profile is Agent Plugins schema 1.0.0: root
`plugin.json`, an actual `LICENSE`, optional `mcp.json`, skills and assets.
Hidden compatibility overlays, symlinks/reparse points, path traversal and
Windows device names are rejected. Limits are 1,024 components, 20 MiB total
and 4 MiB per file. Installation requires the inspected whole-package SHA.
The supported profile has at most eight UTF-8 skills, 128-byte relative skill
paths and 32 KiB per body. Model list results expose paths and identity hashes,
not all bodies or arbitrary source-provenance text.
Provenance is an owner declaration, not an upstream signature verification.
Host-specific hooks/apps/required components are unsupported. Optional
presentation metadata is inspectable but does not render a host interface.

Use absolute owner-selected paths and the same workspace/database throughout:

```powershell
& $exe extensions inspect --source $package
& $exe extensions install --source $package --sha256 $digest --source-id $source --revision $revision --workspace $ws --database $db
& $exe extensions configure --file $definition --workspace $ws --database $db
& $exe extensions status --workspace $ws --database $db
& $exe extensions receipts --run-id $run --workspace $ws --database $db
& $exe extensions activate package.server --workspace $ws --database $db
& $exe extensions disable package.server --workspace $ws --database $db
& $exe extensions remove package.server --workspace $ws --database $db
```

Installation is immutable and never starts code. Configuration is a typed
`ExtensionDefinition`; updates use the same configure command and revoke the
old activation. Activation requires a real local console review of the exact
definition and `ACTIVATE`. It binds package, executable/dependency identities,
workspace, transport, allowed tools, destinations and resource limits. Model,
JSONL, page and manifest text cannot authorize installation or activation.
Removal drains owned work and removes registration, not retained installation
evidence. A restart does not replay calls or restore a revoked connection.

Local stdio uses an absolute SHA-bound executable, exact manifest arguments,
cleared allowlisted environment and an explicitly reviewed current-user-code
boundary. Add executable dependency/configuration files and their digests to
`transport.inputs`. Package/executable/input files are held against writes and
deletion during a connection. This is **not** a filesystem/network sandbox.
Windows Job Objects enforce owned lifetime and fixed process, CPU and memory
limits. Approving local code acknowledges its current-user OS access.

Skills are loaded progressively through `extension` list/load operations.
All descriptions, skill bodies and returned data remain untrusted. Loading a
skill cannot change policy or inject trusted host instructions. Tool schemas
are discovered only when needed; no installed catalog is added wholesale to
every request.

## MCP compatibility and remote credentials

The maintained Rust SDK is pinned to `rmcp 3.5.1`. The declared client profile
negotiates MCP 2026-07-28, with an explicit 2025-11-25 initialization fallback.
It supports stdio and Streamable HTTP, not a separate legacy SSE endpoint.
Sampling, elicitation, roots callbacks, server-requested client actions and
automatic OAuth/credential-helper execution are unavailable.

Discovery is bounded to eight server pages and 128 entries. Tool handles bind
connection, generation, name and current inline schema SHA. The catalog is
rechecked before invocation. Foreign, changed and retired handles fail closed.
Frame size is 256 KiB, arguments 32 KiB and delivered results 64 KiB. Referenced
schemas are unsupported. Server errors are failures, not successful effects.
An unavailable post-dispatch result has unknown effects: access is revoked and
the old call is not automatically retried or replayed.

HTTP uses one exact HTTPS endpoint and explicit pinned public addresses.
Literal loopback HTTP is available only for an owner-declared local demo.
No proxy, redirect following, unsolicited event GET or session reinitialization
is enabled. Keys are stored in the OS vault under project, namespace, origin
and account bindings, not copied from provider credentials. Use:

```powershell
& $exe extensions key package.server --workspace $ws --database $db
```

Key changes revoke activation. Enter a secret only in the masked local prompt,
never a definition, shell argument, package, prompt or report.

## First-party Playwright local-development profile

The optional adapter is `extensions/browser/adapter.mjs`, using Playwright
1.64.0 and its matching official browser installation. Dependency installation
and browser downloads are explicit owner/engineer preparation, never CLI
startup behavior. Review source/license/lock and pin the actual Node program,
adapter, configuration, dependency inputs and browser files in the definition.

Adapter configuration accepts only `allowedOrigins`, `artifactDirectory`,
optional `persistentProfile`, `previewWorkspace`, `previewPort` and
`reviewedStylesheet`. The current
profile is local HTTP at exact `127.0.0.1:PORT` origins, not arbitrary web, LAN,
DNS aliases or private service access. Its fixed preview serves the approved
static workspace's bounded regular HTML/CSS/JS/JSON/images/fonts. It is an owned
Node process, not a model-selected command, shell or general development server.

The optional `reviewedStylesheet` declaration permits one explicitly reviewed
CSS file whose leaf name is `token.css` or `tokens.css`. These names otherwise
collide with the private-resource guard and remain denied by default. Declare
its literal normalized preview-root-relative path using forward slashes, the
SHA-256 of its exact bytes as 64 lowercase hexadecimal digits, and its expected
byte length (`bytes`, an integer from zero through 4 MiB). For example, the
declaration's fields are `path: "styles/tokens.css"`, `sha256: <reviewed digest>`
and `bytes: <reviewed byte count>`; the placeholders are not valid configuration.

This is permission to disclose particular reviewed bytes, not proof that CSS
contains no secrets. It cannot authorize credential/secret names, protected
parent directories, other leaf names, dot/traversal paths, Windows device or
alternate-stream syntax, outside-root files, unsupported types or oversized
files. The preview checks the digest and length of the existing bounded,
admitted buffer and serves that same buffer. Missing permission, a different
literal path, changed bytes or changed length releases no stylesheet body,
including for HEAD requests. Other static files keep their existing behavior.

The declaration is owner configuration, never a browser tool argument or page
instruction. Add or change it only through the normal extension configuration
and exact activation lifecycle, including the configuration file's updated
`transport.inputs` identity. Configuration updates revoke old activation, and
configuration/dependency inputs remain held during a connection. Changed
stylesheet content is blocked until the owner reviews its new digest/length
and reconfigures and activates the exact updated definition. A build, verifier
or preparation hook must not rewrite the approval to keep a changed file working.
No declaration is enabled automatically, and no new approval console is added.

Browser processes/contexts/tabs are task-owned. Profiles are isolated by
default. Persistence requires the explicit `artifactDirectory/profile` path.
Existing non-Argentum state and a changed origin set are rejected before launch;
use a fresh dedicated artifact/profile scope instead. Do not copy another
application's profile, cookie or session state. Persistent browser state and
screenshots may contain secrets: do not commit or export them. There is no
universal screenshot or diagnostic redaction guarantee.

The normal model tool loop discovers scoped browser operations, starts the
preview, opens an admitted URL, observes DOM/accessibility and current target
references, interacts, reads console/network metadata and revisits after a
guarded source edit. It cannot evaluate model-supplied JavaScript. References
bind observed revision and actual element identity; replaced elements and stale
tabs require a fresh observation. Screenshot capture also checks freshness.
Playwright may change a file input's empty style attribute on first capture;
that stale capture is rejected, not silently normalized or rebased.

Subresources are checked as well as navigation. Redirects, unadmitted popups,
WebSockets, worker traffic, downloads and upload targets are unavailable.
Browser routing/CSP is not an OS network sandbox or a claim that Chromium makes
no background network requests. Teardown closes owned tabs, browser and preview;
cancellation also drains the containing process tree.

The existing browser operations include bounded `browser_viewport` selection:
320 through 1200 pixels wide and 240 through 800 high, retaining the admitted
PNG boundary. Resizing invalidates old observations and references. Observe
again before an effect. `browser_observe` returns viewport/layout and current
form state; `text_limit` and `include_accessibility` can narrow its projection
without disabling source/target freshness. Whole-page ARIA output is omitted
when password inputs are present because that API can disclose their values;
this is an explicit omission, not a universal redaction guarantee.

`browser_interact` also supports literal `reload`, `tab` and `shift_tab`
actions. Each requires an existing current tab, exact observed revision and
live observed control reference. Reload navigates only that owned page, under
the same destination rules and operation deadline. The keyboard actions send
only fixed Tab or Shift+Tab from the referenced control; arbitrary keys,
clipboard input and JavaScript are not available. These actions accept no
`value`. Every action invalidates references and requires a fresh observation.
Current observed targets include a `focused` boolean, not keyboard history or
proof of pixel comprehension. Changing the pinned adapter revokes the old
activation and requires exact owner review before using this contract.

## Native delivery and private-safe evidence

Use the existing `goal deliver` workflow for a finite multi-stage objective,
not an ordinary one-shot `run` expecting an automatic supervisor. See
[native delivery](NATIVE_DELIVERY.md). Each admitted model run drains its own
extension connection, browser and preview at settlement or handoff. A next run
uses the still-current exact owner activation but must rediscover and reopen
with new connection/page/observation handles. Old images are not replayed.
Feedback-stage work also receives scheduled request/time handoffs; aggregate
limits and current verification remain authoritative.

Typed operation receipts are retained in the existing event store and exposed
by `extensions receipts`, optionally for one project-scoped run. Inspection
starts no extension/provider request. The bounded event window reports
truncation explicitly; missing historical detail is never reconstructed.

Requested/admitted/completed stages are distinct. Client send counters separate
MCP request attempts, notifications, responses, completed messages and observed
same-RPC-ID repeats. HTTP attempts, including teardown, are separate from MCP
messages. These count the inspected client transport layer, not packets,
provider generations or server effects. Incomplete framing/write/identity
coverage is explicit. Cumulative counters are scoped to connection identity:
do not add the same snapshot once per tool call.

Browser/context/page/generation, selected Playwright-action counts and closed
failure reasons are marked server-reported. They are not independently observed
application correctness or complete browser activity. A successful tool return
does not satisfy fixed source verification. Closed failure feedback asks for
fresh observation/scope before a newly submitted action; it promises no rollback.

Image preparation and the actual serialized PNG input accepted by the Responses
HTTP/SSE transport are separate typed receipts, joined by request identity and
image SHA. Header acceptance is not terminal provider usage or proof that the
model understood the pixels. Current binding and actual terminal usage must
still be inspected. The receipts contain no raw arguments/results, selectors,
typed text, page bodies, cookies, credentials, private URLs or arbitrary errors.

For real visual model input, first prepare a private artifact directory:

```powershell
& $exe extensions prepare-media package.server --workspace $ws --database $db
```

Set the same directory in adapter configuration and the definition's
`image_scope`, with the exact allowed origins. Actual bounded SHA-bound PNG
bytes from the current run/context are sent as Responses image input on the
configured ChatGPT route. Historical images are not replayed; filenames alone
are not vision evidence. Other routes currently reject image input explicitly.
Account/model eligibility remains separate from wire-format support.

Source tests and rendered/visual acceptance are separate. Extension results
are not fixed verification receipts. Generic model shell/argv remains disabled
in the verification-derived coding policy. This browser profile is not native
Windows desktop/computer-use support; that remains the next milestone.
