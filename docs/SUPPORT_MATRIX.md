# Support and evidence matrix

Status is scoped to native `0.1.0-rc.1`, not the historical TypeScript release.
Source presence, compilation, synthetic contracts, live provider evidence and
release qualification are separate facts.

Observed CLI imports include `VCRUNTIME140.dll` and Universal CRT API sets.
An installed x64 Microsoft Visual C++ runtime is a prerequisite; it is not
bundled. Local runtime evidence is Windows 10.0.26100, not a pristine VM or a
general Windows minimum-version qualification.

The qualification runner is `windows-2025`. Retained Server 2022
and newer-runner failures establish that their LPAC cannot reopen the NUL
device: the same child works with inherited stdin, but Cargo's output capture
failed before execution. The runner change alone did not establish qualification.
Those historical runs made no device-permission exception. The owner subsequently
approved a temporary NUL-only DACL change on the disposable hosted job, documented
in [CI host preparation](CI_NULL_DEVICE.md). Its actual apply/restoration and
restricted-child capture controls passed in run `38153983871`; separate
PowerShell startup failures mean that revision is still not qualified. No token
capability, file, registry, network or integrity-label permission was expanded.
Full process, privacy and workspace test contracts remain unchanged.
Server 2022 and other Windows environments are not qualified by a successful
newer-runner result. Local build 26100 and remote CI evidence remain separate.

Final dated qualification, 2026-10-12: the RELEASE01 Genesis core CLI at
`9eb7e4497060bc7408d3e5702d62298a7a58a777` passed both
[PR 38171425611](https://github.com/AG064/argentum/actions/runs/38171425611) and
[push 38171419947](https://github.com/AG064/argentum/actions/runs/38171419947)
under the approved disposable NUL-only condition. Earlier failures above remain
historical failures. See [the exact RC binding](releases/0.1.0-rc.1.md).
CLI qualification is independent of desktop/browser acceptance. NEXT01's
optional feature source and failed frozen live completion do not change this
core RC scope or establish full computer use/Codex replacement.

| Route | Native implementation | Evidence and qualification |
| --- | --- | --- |
| Windows x64 MSVC CLI | Setup, chat, headless, JSONL, tools, review, verification, recovery | Primary RC target; local clean-source and package qualification is recorded in the RC document |
| Windows native Slint UI | Shared command host and existing UI modules | Source retained; not packaged or visually release-qualified by RELEASE01 |
| Android | Host/Keystore/activity and optional in-process inference | Source retained with platform Gradle inputs; no APK/device/release-signing gate claimed here |
| Linux/macOS CLI | Shared Rust modules and platform-specific branches | Not release-qualified by this Windows RC; Windows authority/job/supplemental behavior must not be inferred |
| ChatGPT plan | Native dynamic registration, browser consent, OS secure vault, Responses transport | Official optional preview path for eligible open-source/local apps; prior assisted Sol workflow evidence, no new login/inference here |
| OpenAI-compatible HTTP | Explicit configured endpoint/model, chat-completions tools and budget | Credential-free loopback contracts; MiniMax/DeepSeek historical live results remain qualified separately |
| MiniMax / DeepSeek | Provider-bound continuation, usage and error handling | Prior evidence is not a universal model-quality or provider-entitlement guarantee |
| llama.cpp Windows sidecar | Pinned optional runtime, separate install/launch consent, template and exact token checks | Runtime/model not bundled; prior local workflow evidence, no new download or inference here |
| Other saved provider kinds | Inspectable adapters and capability projections | Availability does not establish a supported coding route; use effective status and adapter-specific evidence |
| Supplemental Rust checks | Owner-enabled constrained library checks against admitted source | Windows-specific restricted execution; unsupported project shapes fail closed |
| Portable skills/MCP | Optional `extensions` build, reviewed package/stdio/Streamable HTTP lifecycle | Bounded native model/tool-loop controls; declared subset and trust boundaries in [extension guide](EXTENSIONS_BROWSER.md) |
| Local browser development | Optional pinned Playwright adapter, owned local preview/context/tabs and current PNG input on ChatGPT route | Offline controls and live application acceptance are separate; not unrestricted web access or native desktop control |
| Windows desktop/computer use | Not implemented by this milestone | Remains an explicit subsequent roadmap gate |

No release package contains credentials, user stores, model weights, private
benchmark evidence or another application's sessions. A successful catalogue
request does not prove inference entitlement. Provider/account limits can change;
declared budgets are not newly verified capacity. No claimed general autonomous
reliability follows from loopback controls or an assisted contribution.
