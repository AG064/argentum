# AG-Claw — Security Audit of Features (automated subagent)

Summary: I scanned index.ts for each feature under src/features and looked for: unsafe calls (eval/new Function, spawn/exec, subprocess), network/exfiltration (fetch/curl), use of process.env secrets, suspicious dependencies, file access, and structural issues. Findings and recommendations are below.


## air-gapped
- **Статус:** ✅ Чисто
- **AI slop:** none found
- **Безопасность:** no external network calls; feature not present (no index.ts found or minimal). Confirmed inert.
- **Рекомендации:** none

## auto-capture
- **Статус:** ✅ Чисто
- **AI slop:** none obvious in index.ts
- **Безопасность:** no eval/exec/fetch found
- **Рекомендации:** keep input sanitization for any future file parsing

## browser-automation
- **Статус:** ✅ Чисто
- **AI slop:** none obvious
- **Безопасность:** no direct dangerous use of child_process or eval in inspected index.ts
- **Рекомендации:** if browser automation adds remote download/execution, gate and validate inputs

## budget
- **Статус:** ✅ Чисто
- **AI slop:** none
- **Безопасность:** no network/external execution
- **Рекомендации:** none

## checkpoint
- **Статус:** ✅ Чисто
- **AI slop:** none
- **Безопасность:** uses disk for checkpoints; ensure directory permissions and no world-write
- **Рекомендации:** ensure checkpoint directory is owned by service user and not web-accessible

## company-templates
- **Статус:** ✅ Чисто
- **AI slop:** small helper functions only
- **Безопасность:** no secrets in code
- **Рекомендации:** none

## consolidation
- **Статус:** ✅ Чисто
- **AI slop:** none
- **Безопасность:** no suspicious calls
- **Рекомендации:** none

## container-sandbox
- **Статус:** ⚠️ Подозрительно
- **AI slop:** some utility code (buildDockerArgs) is verbose but functional
- **Безопасность:** runs arbitrary shell commands via spawn('docker', ...) and passes unescaped command to sh -c. Although workspace is mounted read-only and network disabled by default, executing untrusted commands inside docker with sh -c can still cause risk (e.g. images with setuid binaries, kernel exploits, or if networkAccess enabled). Also it spawns docker kill by container name on timeout. There is reliance on Docker being present and on spawn; user-provided 'command' is directly interpolated to sh -c (shell injection risk if command is constructed from untrusted input).
- **Рекомендации:**
  - Avoid sh -c with untrusted strings; prefer passing args directly or run specific binaries.
  - Validate/whitelist allowed commands or run inside a minimal interpreter that only allows safe ops.
  - Ensure Docker daemon access is limited (service account) and container images are from trusted registries.
  - Add stronger sandboxing (user namespaces, seccomp, drop capabilities) and resource limits.

## evening-recap
- **Статус:** ✅ Чисто
- **AI slop:** placeholder methods returning empty arrays (note: not harmful but code stubs)
- **Безопасность:** no secrets or external calls
- **Рекомендации:** implement handlers carefully to avoid leaking user data when delivering recaps

## goal-decomposition
- **Статус:** ✅ Чисто
- **AI slop:** minimal; simple in-memory structures
- **Безопасность:** uses uuid but no external calls
- **Рекомендации:** none

## goals
- **Статус:** ✅ Чисто
- **AI slop:** none
- **Безопасность:** persists to SQLite under configurable dbPath (default ./data/goals.db). Ensure DB file permissions prevent unauthorized read. SQL prepared statements used (good).
- **Рекомендации:** ensure DB path not placed in web root and is only accessible by service user

## governance
- **Статус:** ⚠️ Подозрительно
- **AI slop:** none
- **Безопасность:** stores approval tickets including action.payload (JSON) and later uses payload._rollback. Storing arbitrary payloads could include secrets. Rollback emits governance:rollback with rollbackData — be careful listeners don't execute arbitrary code from payload. DB path configurable; ensure file permissions. Approver checks rely on configured approvers list — misconfiguration can enable unauthorized approvals.
- **Рекомендации:**
  - Sanitize and limit what can be stored in action.payload; avoid storing raw credentials.
  - When executing rollback handlers, validate contents and require an approval/verification step.
  - Ensure approvers list and DB file access are protected

## group-management
- **Статус:** ✅ Чисто
- **AI slop:** none
- **Безопасность:** in-memory data structures only; no persistent storage. Access control relies on role checks — ensure callers enforce these checks (feature exposes methods but enforcement must be at API layer).
- **Рекомендации:** ensure exported API validates caller identity before invoking role-changing methods

## knowledge-graph
- **Статус:** ⚠️ Подозрительно
- **AI slop:** Markdown parser contains regexes and ad-hoc parsing (works but fragile)
- **Безопасность:** reads files from import paths (readFileSync) and JSON.parse of imported JSON. Importing untrusted Markdown/JSON could create malicious entries or extremely large payloads. The SQLite backend takes file paths from config; ensure path validation to avoid reading arbitrary system files. No network calls. DB file permissions should be tight.
- **Рекомендации:**
  - Validate and sandbox import files (size limits, path sanitation).
  - When importing JSON, avoid trusting 'name' fields mapping to IDs without checks.
  - Rate-limit and size-check imports.

## life-domains
- **Статус:** ✅ Чисто
- **AI slop:** lots of keyword lists (expected)
- **Безопасность:** uses SQLite; ensure DB permissions. Auto-classification is local keyword matching (no external LLM calls)
- **Рекомендации:** none

## live-canvas
- **Статус:** ✅ Чисто
- **AI slop:** trivial in-memory code; elements stored in Map
- **Безопасность:** collaborators set is in-memory; for multi-user deployments ensure access control
- **Рекомендации:** none

## mesh-workflows
- **Статус:** 🔴 Проблема
- **AI slop:** contains code that dynamically evaluates conditions using new Function ("new Function('vars', `with(vars) { return ${condition}; }`)(vars)"). This is effectively eval and a remote code execution vector if conditions are user-supplied.
- **Безопасность:** dynamic function execution allows arbitrary JS execution with 'vars' scope — high risk if conditions or step configs come from untrusted sources. Also step handlers may be registered and can run arbitrary code. The feature executes handlers for step types 'agent' etc., which may trigger other risky code. Persistence of checkpoint data may include stepResults. Overall, this module can execute arbitrary code if workflow definitions are attacker-controlled.
- **Рекомендации:**
  - Remove new Function usage; replace with a safe, sandboxed expression evaluator (e.g., jsep + limited interpreter, or an expression language like jsonata) or strict allowlist of operations.
  - Validate workflow definitions before registering (signature, owner, allowlist of step types).
  - Ensure only trusted users can register workflows or steps that execute code.
  - Consider running step handlers inside the container-sandbox or other confined runtime.

## morning-briefing
- **Статус:** ✅ Чисто
- **AI slop:** placeholder sections (no external calls yet)
- **Безопасность:** will integrate with external data sources later; ensure credentials are stored securely
- **Рекомендации:** when integrating calendar/weather/news, use least-privilege credentials and avoid logging full responses

## multimodal-memory
- **Статус:** ✅ Чисто
- **AI slop:** basic in-memory store
- **Безопасность:** stores content (potentially sensitive) in memory; ensure access control on API surface. computeTextSimilarity is naive but safe.
- **Рекомендации:** add limits to content size and establish retention policies

## skills-library
- **Статус:** ⚠️ Подозрительно
- **AI slop:** stores arbitrary code strings in SkillRecord.code and writes them to disk. It also exposes recordUsage and update/remove which rewrite files.
- **Безопасность:** saving arbitrary code snippets to disk is not itself dangerous, but if those snippets are executed elsewhere (reloaded and executed), it can be code injection. Files are stored under configurable storageDir; ensure directory ownership and no web exposure. removeSkill uses require('fs').unlinkSync — fine.
- **Рекомендации:**
  - Treat stored code as data only; never evaluate or require() it. If executing saved skills is a feature, implement strict sandboxing and permissions.
  - Protect storageDir from web access and set proper filesystem permissions.

## smart-recommendations
- **Статус:** ✅ Чисто
- **AI slop:** in-memory heuristics; OK
- **Безопасность:** records behavior events which may contain sensitive strings; ensure these are not leaked and persisted safely if stored
- **Рекомендации:** redact or limit sensitive fields in behavior events

## task-checkout
- **Статус:** ✅ Чисто
- **AI slop:** none
- **Безопасность:** uses SQLite with prepared statements; ensure DB file permissions
- **Рекомендации:** none

## voice
- **Статус:** ⚠️ Подозрительно
- **AI slop:** helper functions calling external APIs
- **Безопасность:** uses process.env.ELEVENLABS_API_KEY and OPENAI_API_KEY, and fetch to external services. This is expected, but environment keys must be kept secret. generateSpeech constructs fetch POST with text payload — ensure text is not logged. speechToText builds FormData with audio buffer; uses OPENAI_API_KEY from env. Missing rate-limiting/error handling could leak error contents.
- **Рекомендации:**
  - Do not log full TTS/STT payloads.
  - Validate environment is secure. Consider using vault/secret manager.

## webchat
- **Статус:** ⚠️ Подозрительно
- **AI slop:** large embedded HTML string and client JS (expected), some deprecated usages (readAsBinaryString) in client
- **Безопасность:** accepts file uploads via WebSocket, stores uploaded files in memory map and serves via /files/:id. Max file size enforced (default 10MB) and allowedFileTypes restricts types but enforcement in ws 'file' path uses mimeType from client (can be spoofed). Files are kept in memory (could exhaust memory). Message history is stored in memory and served to clients. No authentication shown — endpoints are open on configured port. WebSocket connections use room/user params from URL without auth; this could allow impersonation. Local UI served without CSRF/auth checks. XSS risk: message contents are inserted into HTML via innerHTML in md() output; while md() escapes &<>, it later replaces content with HTML including links — still risky if untrusted HTML passes through. Also client uses btoa(reader.result) and reads binary as string — may corrupt binary.
- **Рекомендации:**
  - Require authentication for webchat endpoints or restrict by network.
  - Validate and sanitize attachments server-side; enforce allowedFileTypes based on content sniffing not client-provided mimeType.
  - Limit memory usage for uploads and persist to disk with quotas instead of keeping buffers indefinitely.
  - Add CSRF/authorization to message posting and ws connection (tokens).
  - Sanitize message HTML server-side and avoid inserting raw HTML into innerHTML; render Markdown safely.

## webhooks
- **Статус:** ⚠️ Подозрительно
- **AI slop:** none
- **Безопасность:** dispatch() posts event payloads to subscriber URLs and signs them with subscriber secret. This can be used to exfiltrate data to subscriber endpoints if subscribers are malicious. Subscriptions are created with a secret; they are stored in-memory (not persisted) which limits long-term exposure but also means restart loses subscriptions. verifySignature compares provided signature but caller must supply secret for verification. The deliverWithRetry uses fetch to sub.url — no validation of URL (could be internal IPs). Also payload passed to createHmac uses sub.secret — good for integrity. There is no outgoing proxying restrictions (SSR check), so server could be used as a requester to internal resources if attacker can create subscription with internal URL.
- **Рекомендации:**
  - Validate subscription URLs (deny internal IP ranges, localhost) or require admin approval for external webhooks.
  - Limit what fields are included in event payloads sent to subscribers, avoid sending secrets or PII.
  - Persist subscriptions securely if persistence is desired and protect subscription store.


---

Notes about structure/paths and general issues observed:
- Most features use local SQLite files under ./data/*.db — ensure these directories are not world-readable and are owned by the service user.
- I did not find use of eval() except via new Function in mesh-workflows.
- spawn/child_process is used in container-sandbox (docker) — high-impact area, review carefully.
- Many features expect process.env API keys (voice), these must be provisioned securely (env or secrets manager).
- Several modules read/write files (skills-library, knowledge-graph imports, webchat uploads) — enforce size limits, content validation, and directory permissions.

If you want, I can:
- produce a focused remediation patch list for the 4 risky features (container-sandbox, mesh-workflows, webchat, webhooks, voice, skills-library, governance, knowledge-graph) with suggested code changes, libraries to use (sandboxed expression evaluators), and configuration hardening steps.


Generated by subagent security-audit-features
