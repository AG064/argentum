# AG-Claw - Post-formatting error check

Date: 2026-03-18

Summary
-------
Ran TypeScript check and inspected key files after code formatter.

Findings
--------
1) TypeScript compile errors (npx tsc --noEmit --skipLibCheck) — these are mostly due to missing runtime/type deps and lib configuration:
   - src/features/browser-automation/index.ts
     - Cannot find module 'playwright' or 'puppeteer' (TS2307)
     - Several DOM-related names missing (Element, document, window, HTMLAnchorElement, etc.) — suggests tsconfig lib doesn't include 'dom' or these files are intended to run inside page.evaluate (browser context)
     - Property 'launch' does not exist on type 'Function' (engine typing is unknown)
     - Property 'press' does not exist on type 'Function' (uses page.keyboard.press)
   - src/features/goal-decomposition/index.ts and src/features/skills-library/index.ts
     - Missing declaration file for module 'uuid' (TS7016)

   These errors caused tsc exit code 2.

2) package.json — looks valid JSON and scripts/deps look consistent. No syntax errors. Note: playwright/puppeteer and @types/uuid are not declared in dependencies/devDependencies, which explains the tsc complaints.

3) config/default.yaml — valid YAML (no syntax errors found). Values look sane.

4) Mesh Workflows
   - The conditional evaluation previously used new Function(...). It has been commented out and replaced with a safe fallback (result = false). The commented line is inside a try/catch and does not break runtime. Behavior: condition handling currently disabled (returns false). This is intentional and documented with TODO.

5) Container Sandbox
   - A clear WARNING comment about shell command injection is present in buildDockerArgs. This is correct and intentional.

6) Webhooks
   - The import/usage appears correct. Signature verification strips 'sha256=' prefix; verifySignature assumes signature present. fetch is used (Node 18+ supports global fetch). There's an explicit WARNING about SSRF risk in deliverWithRetry — this is correct and intentional.

Recommendations
---------------
- To fix tsc errors without changing feature code:
  1. Install missing types/deps if you intend to compile with these features enabled:
     - npm i -D @types/uuid
     - npm i playwright puppeteer (or at least add as optional deps) and/or their types if needed
  2. If browser automation files run both in Node and evaluate browser JS, add DOM lib for TypeScript when compiling node-targeted code that includes DOM types used inside evaluate callbacks. Options:
     - In tsconfig.json, add "lib": ["ES2020", "DOM"] (if acceptable)
     - Or narrow types in browser-automation to avoid referencing DOM types at top-level (use any or string casts inside page.evaluate)
  3. For uuid, either install @types/uuid or import uuid functions by path (uuid v8+ has ESM types) and adjust imports.

- Mesh workflows: If you need condition evaluation, replace new Function with a safe expression evaluator (e.g., jsep + interpreter, or a sandboxed wasm) and document security implications.

- Container sandbox: Ensure commands passed to runInSandbox are sanitized; consider passing args as array or using a safer exec mechanism inside container.

- Webhooks: Validate subscriber URLs against allowlist to mitigate SSRF; ensure signature header presence before calling .replace.

Files inspected
---------------
- src/features/browser-automation/index.ts
- src/features/mesh-workflows/index.ts
- src/features/container-sandbox/index.ts
- src/features/webhooks/index.ts
- package.json
- config/default.yaml

Actions performed
-----------------
- Ran: npx tsc --noEmit --skipLibCheck (failed with TypeScript errors listed above)
- Validated package.json (no syntax errors)
- Validated config/default.yaml parsing (no errors)
- Inspected the specific files mentioned for logical/formatting issues

Conclusion
----------
No formatting-induced logical breakages found in the inspected files. The TypeScript errors are real but stem from missing type declarations and lib configuration rather than syntax errors introduced by the formatter. Mesh-workflows, container-sandbox, and webhooks contain intentional comments/warnings and appear consistent.

Suggested next steps
--------------------
- If CI requires clean tsc, install missing types and/or adjust tsconfig as suggested.
- If you want me to produce a small PR that adds devDependencies (@types/uuid) and optional dependencies or adjusts tsconfig to include DOM, I can prepare the changes (note: task forbids changing code without approval).

