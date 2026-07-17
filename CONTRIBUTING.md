# Contributing to Argentum

Argentum is maintained by a small team. Keep changes scoped, test behavior rather
than screenshots, and describe unfinished work as unfinished.

## Developer Certificate of Origin

Every commit must be signed off under the [DCO](DCO.md):

```text
Signed-off-by: Name <email@example.com>
```

Use `git commit -s` to add the line. This is a DCO sign-off, not a GPG signature.

## Setup

Requirements:

- Node.js 18 or newer and npm 9 or newer
- Rust stable plus Tauri v2 prerequisites for desktop work
- JDK 17 and Android SDK 34 for Android work

```bash
npm install
npm run build
```

Do not commit `.env`, `secrets.env`, Android keystores, Tauri signing keys,
downloaded models, `node_modules`, Rust targets, or build output.

## Commit flow

Argentum enforces [Conventional Commits](https://www.conventionalcommits.org/).
Examples:

```text
feat(models): add bounded Hugging Face search
fix(security): prevent requests from widening context access
docs: clarify Android signing requirements
```

The local Husky hooks are installed by `npm install`/`npm run prepare`:

| Hook         | Enforced behavior                                                                                   |
| ------------ | --------------------------------------------------------------------------------------------------- |
| `pre-commit` | Runs lint-staged: Prettier and ESLint fixes for staged source, and Prettier for staged docs/config. |
| `commit-msg` | Runs commitlint against the commit message.                                                         |
| `pre-push`   | Runs typecheck, lint, desktop asset parity, version parity, and the Jest suite.                     |

Hooks do not replace CI. Do not bypass them to make a failing change appear
ready. If a hook changes staged files, review the diff and stage the intended
result before committing again.

Useful validation commands:

```bash
npm run validate:quick
npm run validate:push
npm run build
cd src/desktop && cargo test --lib
```

For a release candidate also run the platform build you changed. Android release
builds require the persistent signing secrets documented in
[docs/ANDROID_BUILD.md](docs/ANDROID_BUILD.md).

## Branches and pull requests

Create a focused branch such as `feat/model-search`, `fix/context-policy`, or
`docs/release-flow`. Open feature/design issues before a large implementation.
Pull requests should explain:

- the behavior and reason for the change;
- security, data, migration, and compatibility impact;
- tests run and any tests not run;
- screenshots only when UI changed;
- follow-up work that remains intentionally out of scope.

New behavior needs targeted tests when practical. A bug fix should include a
regression test. Do not add fake API results, placeholder success states, dead
controls, or production dependencies without a license/security review.

## Modular feature requirements

Non-core capabilities must be independently configurable and disabled by
default. A feature module must:

- declare configuration and lifecycle cleanup;
- fail closed when permissions, credentials, or dependencies are missing;
- avoid opening a listener, timer, browser, or OS capability while disabled;
- expose honest health/unavailable states;
- log meaningful state transitions without secrets;
- document its provider/data boundary and license.

AI tools are default-deny. The persisted workspace policy is authoritative; UI
request data must not widen it. Side-effecting skills/plugins require explicit
activation and a capability check at execution time.

## Licensing

Contributions are licensed under MIT. Third-party code, models, skills, plugins,
and assets need source, revision, license, and notice information. MIT,
Apache-2.0, BSD, ISC, 0BSD, and similarly permissive licenses are generally
compatible, but every imported component must be checked. Unknown, custom,
source-available, noncommercial, SSPL, BSL, and AGPL material requires explicit
maintainer/legal review before inclusion.

## Security reports

Do not open a public issue for a vulnerability. Follow [SECURITY.md](SECURITY.md).
