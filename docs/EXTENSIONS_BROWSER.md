# Extensions and local browser work

Extensions let the `argentum` CLI use packaged skills and external tool servers
(MCP) that you have reviewed. One first-party extension drives a local browser
so the model can check a web page it is building.

## Where it is available

- The `argentum-genesis-desktop-control-0.1.0-windows-x64.zip` download. That CLI
  is built with desktop control, which also turns on extensions.
- A CLI built with `cargo build -p argentum-cli --features extensions`.
- A portable package made with `scripts/package-windows-cli.ps1 -Extensions`.
  It includes the browser adapter source and lock file. It does not include
  Node, browser binaries, profiles, credentials or extension definitions.

The default CLI download does not include extensions. Starting a build with
extensions does not install packages, start a browser or server, download
anything or contact an extension endpoint. You install, configure and activate
each extension yourself.

## Packages

A package is a folder with a root `plugin.json` that follows the Agent Plugins
1.0.0 schema, a `LICENSE` file, and optionally `mcp.json`, skills and assets.

- Limits: 1,024 files, 20 MiB in total, 4 MiB per file.
- At most eight skills (`skills/<name>/SKILL.md`), with relative paths of at most
  128 bytes and bodies of at most 32 KiB.
- Symlinks, reparse points, path traversal and Windows device names are rejected.
- Provenance is what you declare at install time. Argentum does not verify an
  upstream signature.
- Host-specific hooks and apps are not supported.

Use absolute paths and the same workspace and database for every command:

```powershell
& $exe extensions inspect --source $package
& $exe extensions install --source $package --sha256 $digest --source-id $source --revision $revision --workspace $ws --database $db
& $exe extensions configure --file $definition --workspace $ws --database $db
& $exe extensions status --workspace $ws --database $db
& $exe extensions list --workspace $ws --database $db --json
& $exe extensions receipts --run-id $run --workspace $ws --database $db
& $exe extensions activate package.server --workspace $ws --database $db
& $exe extensions disable package.server --workspace $ws --database $db
& $exe extensions remove package.server --workspace $ws --database $db
```

- Installing needs the package's SHA-256 and never runs package code.
- `configure` takes a definition file. Configuring again replaces the definition
  and removes the old activation.
- `activate` shows the exact definition in your console and needs you to type
  `ACTIVATE`. Model output, package text and web pages cannot activate anything.
- `remove` stops running work and removes the registration.
- `list` and `status` accept `--json`, which prints one JSON document with
  `schema_version`, `command`, `records` and, for status, `readiness`. Other
  extension commands reject `--json`.
- `receipts` shows recorded extension operations, optionally for one run.

Skills load on demand. Skill text, tool descriptions and returned data are
untrusted, and loading a skill cannot change your permissions.

## Local programs (stdio)

A local extension runs as a program under your Windows account. You approve an
absolute executable by SHA-256 plus its exact arguments. It starts with a cleared
environment containing only allowed variables. List dependency and configuration
files with their digests in `transport.inputs`. Those files, and the package and
executable, cannot be written or deleted while the extension is connected.

Windows Job Objects limit process count, CPU and memory and end the process tree
when the work ends. This is not a filesystem or network sandbox: approving a local
program gives it your account's access.

## MCP client

Argentum uses the Rust MCP SDK `rmcp 3.9.0`. It speaks MCP 2026-07-28 and falls
back to 2025-11-25. It supports stdio and Streamable HTTP. Sampling,
elicitation, roots callbacks and automatic OAuth are not supported.

- Tool listing reads at most eight pages and 128 entries.
- A tool call is tied to the connection and to the tool's current schema, which
  is checked again before the call. Changed or unknown tools are refused.
- Messages are limited to 256 KiB, call arguments to 32 KiB and results to 64 KiB.
- Schemas that use `$ref` are not supported.
- If a result is lost after a call was sent, the effect is unknown. Argentum
  revokes access and does not retry the call.

HTTP extensions use one exact HTTPS endpoint with pinned public addresses. Plain
HTTP is allowed only for a loopback demo that you declare. Proxies and redirects
are not followed. The extension's key is stored in the operating system credential
store (Windows Credential Manager on Windows), tied to the project, namespace,
origin and account. Set it with:

```powershell
& $exe extensions key package.server --workspace $ws --database $db
```

Changing a key removes the activation. Type the secret only at the masked prompt,
never in a definition, argument, package or prompt.

## Local browser (Playwright)

The first-party adapter is `extensions/browser/adapter.mjs`. It uses Playwright
1.64.0 and its matching Chromium. You need a Node.js install. Dependencies and
the browser are downloaded only when you run `browser prepare --install`.
`browser setup` prepares the browser extension definition and starts nothing. The preview port must be 1024 to 65535 and match an `--origin`:

```powershell
& $exe extensions browser setup --source extensions/browser --node $node --origin http://127.0.0.1:4173 --preview-workspace $site --preview-port 4173 --browser-cache $cache --workspace $ws --database $db
```

### Check and prepare

```powershell
& $exe extensions browser check --workspace $ws --database $db
& $exe extensions browser check --handshake --workspace $ws --database $db
& $exe extensions browser prepare --workspace $ws --database $db
& $exe extensions browser prepare --install --confirm-network --workspace $ws --database $db
```

- `check` reports whether the definition, activation, Node, the adapter files,
  `playwright`, `playwright-core` and the Chromium cache are in place, each with
  a next step. It starts no process. It exits nonzero when a required check
  fails. `--json` prints the result as JSON.
- `check --handshake` also starts the reviewed adapter, lists its tools and stops
  it. It opens no page. It starts nothing if a dependency is missing.
- `prepare` without `--install` prints the commands it would run and starts no
  process.
- `prepare --install --confirm-network` runs `npm ci --ignore-scripts` and
  `npx --no-install playwright install chromium` in the adapter folder, using the
  pinned Node, with `PLAYWRIGHT_BROWSERS_PATH` set to your browser cache. It does
  not activate the extension. Either flag alone is rejected.
- Set `ARGENTUM_PLAYWRIGHT_BROWSERS_PATH` when the browser cache is not in the
  default place.

### Configuration

The adapter accepts `allowedOrigins`, `artifactDirectory`, an optional
`persistentProfile`, `previewWorkspace`, `previewPort` and `reviewedStylesheet`.

- Pages must be on exact `http://127.0.0.1:PORT` origins. Other websites, LAN
  addresses and DNS names are not reachable.
- The preview server is started by the adapter. It serves static HTML, CSS, JS,
  JSON, images and fonts from the approved workspace.
- Files named `token.css` or `tokens.css` are blocked by default because the
  preview treats such names as private. `reviewedStylesheet` allows one such file
  by its relative path (`path`), SHA-256 (`sha256`) and size (`bytes`, 0 to 4 MiB).
  If the file differs from those values, it is not served. Changing the entry
  needs the usual configure and activate steps.
- Each task gets its own browser, tabs and temporary profile. A persistent
  profile must live at `artifactDirectory/profile`. Existing non-Argentum state
  or a changed origin list is rejected.

Browser profiles and screenshots can contain secrets. Do not commit or share them.
Argentum does not promise to redact them.

### What the model can do

The model can start the preview, open an allowed URL, read the page structure and
form state, click, fill, check, uncheck, select, reload, press Tab or Shift+Tab,
set the viewport (320 to 1200 wide, 240 to 800 high), read console and network
metadata, take screenshots, and re-check after editing files.

- It cannot run JavaScript, press other keys, use the clipboard, download or
  upload files, open popups, use WebSockets or follow redirects.
- Subresource requests are checked as well as navigation. This is browser-level
  routing, not an operating system network sandbox.
- Element references belong to one observation. After any action or resize, the
  model must observe again.
- The page accessibility snapshot is left out when the page has password inputs,
  because it could include their values.

## Screenshots as model input

Prepare a private artifact directory first:

```powershell
& $exe extensions prepare-media package.server --workspace $ws --database $db
```

Use that directory as `artifactDirectory` in the adapter configuration and as
the definition's `image_scope`. PNG screenshots from the current run are then sent
to the model as images. Image input works only on the ChatGPT route. Other
providers reject it. Old screenshots are not sent again.

## Related

For multi-step goals see [native delivery](NATIVE_DELIVERY.md). For Windows
application control see [Windows desktop control](WINDOWS_APPLICATION_CONTROL.md).
