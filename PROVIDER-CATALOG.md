# Provider catalog sources

The CLI provider catalog is data-driven from `PROVIDER_CATALOG` in
`crates/argentum-providers/src/lib.rs`. This file records where the endpoints,
default models, and credential variables came from, and which routes are
deliberately unsupported.

Defaults were checked on 2026-11-01. Model identifiers change often; the default
is a starting point and can be overridden with `provider model` or
`provider save`.

## Primary sources

- OpenCode provider directory: https://opencode.ai/docs/providers/
- models.dev provider and model API: https://models.dev/api.json
- Pi coding agent provider configuration: https://pi.dev/docs/latest/providers
- OpenClaw model providers: https://docs.openclaw.ai/concepts/model-providers
- OpenClaw custom providers: https://docs.openclaw.ai/concepts/model-providers/custom-providers

Pi and OpenClaw are agent frameworks, not inference services. Their provider
documentation was used to cross-check base URLs and authentication modes. Neither
publishes its own inference endpoint, so the catalog does not invent one.

## Catalog rules

- Every hosted entry uses the provider's documented OpenAI-compatible base URL
  where one exists. Anthropic keeps its native kind because it is not
  OpenAI-compatible.
- Local entries use loopback endpoints and require no credential.
- Entries that need a non-HTTP auth scheme or a resource-specific header are
  marked unsupported instead of being presented as working.
- API keys are read from the named environment variable or the OS secret store.
  No secret is stored in source.

## Entries

| Id | Label | Kind | Default endpoint | Credential | Status |
| --- | --- | --- | --- | --- | --- |
| `openai-compatible` | OpenAI compatible | openai-compatible | explicit endpoint | none | supported |
| `minimax` | MiniMax | openai-compatible | `https://api.minimax.io/v1/` | `MINIMAX_API_KEY` | supported |
| `openai` | OpenAI API | openai-compatible | `https://api.openai.com/v1/` | `OPENAI_API_KEY` | supported |
| `deepseek` | DeepSeek | openai-compatible | `https://api.deepseek.com/` | `DEEPSEEK_API_KEY` | supported |
| `llama-cpp` | llama.cpp | local llama.cpp | `local://llama-cpp/` | none | supported |
| `lm-studio` | LM Studio | local LM Studio | `http://127.0.0.1:1234/v1/` | none | supported |
| `chatgpt` | ChatGPT plan | openai-compatible | `https://api.openai.com/v1/` | browser sign-in | supported |
| `anthropic` | Anthropic | anthropic | `https://api.anthropic.com/v1/` | `ANTHROPIC_API_KEY` | unsupported |
| `google-gemini` | Google Gemini | openai-compatible | `https://generativelanguage.googleapis.com/v1beta/openai/` | `GEMINI_API_KEY` | supported |
| `openrouter` | OpenRouter | openai-compatible | `https://openrouter.ai/api/v1/` | `OPENROUTER_API_KEY` | supported |
| `groq` | Groq | openai-compatible | `https://api.groq.com/openai/v1/` | `GROQ_API_KEY` | supported |
| `mistral` | Mistral | openai-compatible | `https://api.mistral.ai/v1/` | `MISTRAL_API_KEY` | supported |
| `xai` | xAI | openai-compatible | `https://api.x.ai/v1/` | `XAI_API_KEY` | supported |
| `together` | Together AI | openai-compatible | `https://api.together.xyz/v1/` | `TOGETHER_API_KEY` | supported |
| `fireworks` | Fireworks AI | openai-compatible | `https://api.fireworks.ai/inference/v1/` | `FIREWORKS_API_KEY` | supported |
| `cerebras` | Cerebras | openai-compatible | `https://api.cerebras.ai/v1/` | `CEREBRAS_API_KEY` | supported |
| `perplexity` | Perplexity | openai-compatible | `https://api.perplexity.ai/` | `PERPLEXITY_API_KEY` | supported |
| `cohere` | Cohere | openai-compatible | `https://api.cohere.com/compatibility/v1/` | `COHERE_API_KEY` | supported |
| `moonshot` | Moonshot AI | openai-compatible | `https://api.moonshot.ai/v1/` | `MOONSHOT_API_KEY` | supported |
| `zai` | Z.AI (GLM) | openai-compatible | `https://api.z.ai/api/paas/v4/` | `ZAI_API_KEY` | supported |
| `qwen` | Qwen (DashScope) | openai-compatible | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1/` | `DASHSCOPE_API_KEY` | supported |
| `github-models` | GitHub Models | openai-compatible | `https://models.github.ai/inference/` | `GITHUB_TOKEN` | supported |
| `opencode-zen` | OpenCode Zen | openai-compatible | `https://opencode.ai/inference/openai/v1/` | `OPENCODE_API_KEY` | supported |
| `ollama` | Ollama | openai-compatible | `http://127.0.0.1:11434/v1/` | none | supported |
| `vllm` | vLLM | openai-compatible | `http://127.0.0.1:8000/v1/` | none | supported |
| `azure-openai` | Azure OpenAI | openai-compatible | resource-specific | `AZURE_OPENAI_API_KEY` | unsupported |
| `amazon-bedrock` | Amazon Bedrock | unknown | none | AWS SigV4 | unsupported |
| `google-vertex` | Google Vertex AI | unknown | none | service account | unsupported |

## Unsupported entries

- Anthropic has a native text adapter, but it is not registered for coding
  dispatch and has no structured-tool route, so the catalog does not offer it as
  a working preset.
- Azure OpenAI uses an `api-key` header and a resource-specific endpoint. The
  shared OpenAI-compatible route sends a Bearer token, so the catalog refuses to
  pretend the default route works.
- Amazon Bedrock needs AWS SigV4 credentials.
- Google Vertex AI needs Google service-account credentials.

## Endpoint notes

- Anthropic uses its native messages API. The text adapter is not registered for
  coding dispatch and does not implement the shared structured-tool contract, so
  the preset is listed as unsupported.
- Z.AI and Qwen list international endpoints. Mainland accounts use
  `open.bigmodel.cn` and `dashscope.aliyuncs.com` respectively.
- Cohere uses its compatibility API.
- GitHub Models uses a GitHub token with the `models:read` permission.
- Ollama and vLLM are local servers. Use `provider save` when vLLM listens on a
  different port.

## Per-provider documentation

- Anthropic: https://docs.anthropic.com
- Google Gemini OpenAI compatibility: https://ai.google.dev/gemini-api/docs/openai
- OpenRouter: https://openrouter.ai/docs
- Groq: https://console.groq.com/docs
- Mistral: https://docs.mistral.ai
- xAI: https://docs.x.ai
- Together: https://docs.together.ai
- Fireworks: https://docs.fireworks.ai
- Cerebras: https://inference-docs.cerebras.ai
- Perplexity: https://docs.perplexity.ai
- Cohere compatibility API: https://docs.cohere.com/docs/compatibility-api
- Moonshot: https://platform.moonshot.ai/docs
- Z.AI: https://docs.z.ai
- Qwen/DashScope compatibility: https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope
- GitHub Models: https://docs.github.com/en/github-models
- OpenCode Zen: https://opencode.ai/docs/providers/
- Ollama OpenAI compatibility: https://docs.ollama.com/api/openai-compatibility
- vLLM OpenAI-compatible server: https://docs.vllm.ai/en/latest/serving/openai_compatible_server.html
- Azure OpenAI: https://learn.microsoft.com/azure/ai-foundry/openai/api-version-lifecycle
- Amazon Bedrock: https://docs.aws.amazon.com/bedrock/
- Google Vertex AI: https://cloud.google.com/vertex-ai/generative-ai/docs
