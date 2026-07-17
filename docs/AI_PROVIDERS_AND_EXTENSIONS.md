# AI Providers and Extension Compatibility

Last reviewed: 2026-07-17. Provider APIs and extension formats change quickly;
revalidate primary documentation before each release.

## Provider matrix

| Provider          | Desktop configuration            | Current route                                                                | Release status |
| ----------------- | -------------------------------- | ---------------------------------------------------------------------------- | -------------- |
| OpenAI / Codex    | API key or Codex browser account | OpenAI-compatible API; dedicated Codex account flow                          | Candidate      |
| MiniMax           | API key                          | OpenAI-compatible endpoint; API style can be changed to Anthropic-compatible | Candidate      |
| Anthropic         | API key                          | Messages API with supported thinking payload                                 | Testing        |
| Google Gemini     | API key                          | Google OpenAI-compatible endpoint                                            | Testing        |
| OpenRouter        | API key                          | OpenAI-compatible                                                            | Testing        |
| NVIDIA NIM        | API key                          | OpenAI-compatible                                                            | Testing        |
| Groq              | API key                          | OpenAI-compatible                                                            | Testing        |
| llama.cpp         | Local, key optional              | Managed localhost OpenAI-compatible server                                   | Candidate      |
| LM Studio / local | Local, key optional              | User-configured OpenAI-compatible endpoint                                   | Candidate      |
| Ollama            | Local, key optional              | OpenAI-compatible endpoint                                                   | Testing        |
| Custom            | Optional key                     | User-selected OpenAI- or Anthropic-compatible endpoint                       | Testing        |

Every provider is selectable for testing. “Testing” means the adapter and test
path exist; it does not mean the maintainers possess credentials for or have
verified every account, region, quota, model, attachment, streaming, and tool
combination. Test Provider must surface authentication, permission, rate-limit,
unavailable-service, and invalid-model failures rather than silently falling
back to success.

MiniMax documents an Anthropic-compatible M2.7 endpoint at
[`https://api.minimax.io/anthropic`](https://platform.minimax.io/docs/token-plan/quickstart).
Its official MCP implementations cover multimodal tools, while Token Plan MCP
adds web search and image understanding. These must be separate opt-in adapters,
not implicit permissions ([general MCP guide](https://platform.minimax.io/docs/guides/mcp-guide),
[Token Plan MCP](https://platform.minimax.io/docs/token-plan/mcp-guide)).

## Context and quota behavior

- The UI estimates tokens and can compact older history.
- The Rust bridge caps message/history/summary size and rejects an estimated
  input above the selected model's conservative budget with an output reserve.
- Unknown/local models use a conservative context default until an endpoint
  reports an authoritative limit.
- Quota/usage is displayed only from provider headers, response bodies, or an
  official quota endpoint. Missing data is “unknown,” never zero or unlimited.
- Remote-plan limits cannot be enforced locally when the provider does not expose
  them. Local budget modules are an additional user policy, not provider truth.

## Extension compatibility plan

Argentum should use one internal extension manifest with optional adapters for:

- Agent Skills (`SKILL.md`, progressive/on-demand context);
- Claude Code plugins: skills, agents, hooks, MCP, LSP, monitors, settings;
- Codex plugins: skills plus app/tool integrations and templates;
- MCP servers and deferred tool discovery;
- hooks/events with pre-action deny/approval and post-action audit;
- subagents/agent teams with isolated context and explicit delegation;
- browser and computer-use capability brokers.

Current Claude Code plugins can package skills, agents, hooks, MCP/LSP servers,
background monitors, executables, and settings. Marketplace plugins are copied
to a versioned cache, but Anthropic explicitly warns that included software is
not automatically trustworthy ([plugin guide](https://code.claude.com/docs/en/plugins),
[marketplace guide](https://code.claude.com/docs/en/discover-plugins)). Claude
hooks include deterministic pre/post tool gates and async forms
([hooks guide](https://code.claude.com/docs/en/hooks-guide)). Claude computer use
is a disabled-by-default MCP server with per-session app approval; Argentum
should preserve or strengthen that model ([computer use](https://code.claude.com/docs/en/computer-use)).

OpenAI Codex plugins package reusable skills and app integrations. Support must
import declared permissions and keep initial versions read-only where possible
([OpenAI plugin overview](https://help.openai.com/en/articles/20001256-plugins-in-codex)).

## Required security manifest

An extension is not activatable until Argentum records:

- canonical source and immutable revision/digest;
- license identifier, license file, required notices, and model/data terms;
- publisher identity when available and verification status;
- requested filesystem roots, commands, environment names, network hosts,
  browser/apps, MCP transports, hooks, and background execution;
- context contribution and estimated always-loaded token cost;
- install/update/rollback state and audit history.

Default policy:

- installation does not enable execution;
- missing/unknown capabilities are denied;
- side-effecting skills are user-invoked only;
- command hooks cannot run until individually approved or policy-allowlisted;
- remote MCP uses HTTPS and explicit hosts; stdio commands use fixed executables
  and arguments, never an unreviewed shell string;
- browser/computer use is per session and per target, with sensitive app warnings;
- untrusted retrieved text cannot grant permissions or alter policy.

## Commercial licensing gate

Do not import a repository based on its top-level reputation. Record license per
component and revision. MIT, Apache-2.0, BSD, ISC, 0BSD, CC0, and Unlicense are
generally commercial-compatible when their notice/attribution terms are met.
Unknown/custom, noncommercial, source-available, SSPL, BSL, and strong-copyleft
material require explicit legal/maintainer review.

Important examples:

- llama.cpp is MIT-licensed, which permits commercial use subject to the license
  notice ([license](https://github.com/ggml-org/llama.cpp/blob/master/LICENSE)).
- OpenAI's public skills repository uses per-skill licenses; inspect each skill's
  `LICENSE.txt` rather than assuming a repository-wide grant
  ([repository](https://github.com/openai/skills)).
- Anthropic's skills repository states that some document skills are
  source-available rather than open source. Do not redistribute or commercially
  incorporate those without a separate compatible grant
  ([repository](https://github.com/anthropics/skills)).
- Hugging Face model licenses and gating vary per repository. Search results are
  discovery, not a commercial-use approval. The picker must show the reported
  license/gated state and link to the model card
  ([Hub search](https://huggingface.co/docs/huggingface_hub/en/guides/search),
  [GGUF](https://huggingface.co/docs/hub/gguf)).
- The v0.0.9 quick-download presets are limited to currently available GGUF
  repositories reporting Apache-2.0 (Qwen2.5 0.5B, Qwen3 0.6B/1.7B,
  TinyLlama 1.1B, and SmolLM2 360M). This is a point-in-time catalog check, not
  a substitute for rechecking the model card before download or redistribution.

## Implementation order

1. Manifest parser, license/integrity record, capability declaration, disabled
   install state.
2. Read-only skills and deferred context/tool discovery.
3. MCP transports with host/process allowlists and secret references.
4. Deterministic hooks with pre-action deny and complete audit.
5. Isolated agents and explicit context handoff.
6. Browser use, then computer use, after session approvals and prompt-injection
   defenses have integration tests.
