# Support and evidence matrix

Status is scoped to native `0.1.0-rc.1`, not the historical TypeScript release.
Source presence, compilation, synthetic contracts, live provider evidence and
release qualification are separate facts.

Observed CLI imports include `VCRUNTIME140.dll` and Universal CRT API sets.
An installed x64 Microsoft Visual C++ runtime is a prerequisite; it is not
bundled. Local runtime evidence is Windows 10.0.26100, not a pristine VM or a
general Windows minimum-version qualification.

The qualification runner is `windows-2025`. Retained Server 2022 (build 20348)
failures establish that its LPAC cannot reopen the NUL device: the same child
works with inherited stdin, but Cargo's output capture fails before execution.
No additional capability or global device permission is granted to obtain a
pass. Full process, privacy and workspace test contracts remain unchanged.
Server 2022 and other Windows environments are not qualified by a successful
newer-runner result. Local build 26100 and remote CI evidence remain separate.

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
| External plugins/MCP/browser/computer use | Not implemented by the native CLI | Roadmap only; historical Node modules are not native capabilities |

No release package contains credentials, user stores, model weights, private
benchmark evidence or another application's sessions. A successful catalogue
request does not prove inference entitlement. Provider/account limits can change;
declared budgets are not newly verified capacity. No claimed general autonomous
reliability follows from loopback controls or an assisted contribution.
