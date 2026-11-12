# Providers

Argentum ships with presets for the providers below. Pick one in Settings > Providers,
or with `argentum provider add PRESET` in the terminal. Every preset can be changed:
the address, the model, and the context size are yours to set.

API keys are stored in the system credential store (Windows Credential Manager or
Android Keystore). Each key is only sent to the address of its own provider. In the
terminal you can also give a key through the environment variable listed for it.

DeepSeek and OpenCode Go are tested with Argentum. The others are listed under Testing
in Settings until they have been run against Argentum. Default models were checked in
November 2026; providers change their model lists often.

## Hosted services

| Provider | Preset | Default address | Default model | Key variable | Notes |
| --- | --- | --- | --- | --- | --- |
| [MiniMax](https://platform.minimax.io/docs) | `minimax` | `https://api.minimax.io/v1/` | `MiniMax-M2.7` | `MINIMAX_API_KEY` |  |
| [OpenAI API](https://platform.openai.com/docs) | `openai` | `https://api.openai.com/v1/` | `gpt-5.6` | `OPENAI_API_KEY` |  |
| [DeepSeek](https://api-docs.deepseek.com) | `deepseek` | `https://api.deepseek.com/` | `deepseek-flash` | `DEEPSEEK_API_KEY` |  |
| [ChatGPT plan](https://chatgpt.com) | `chatgpt` | `https://api.openai.com/v1/` | `gpt-6-luna` | None | Uses browser sign-in instead of an API key. |
| [Anthropic](https://docs.claude.com/en/api/messages) | `anthropic` | `https://api.anthropic.com/v1/` | `claude-sonnet-5-5` | `ANTHROPIC_API_KEY` | Uses the native Messages API. |
| [Google Gemini](https://ai.google.dev/gemini-api/docs/openai) | `google-gemini` | `https://generativelanguage.googleapis.com/v1beta/openai/` | `gemini-4-argon` | `GEMINI_API_KEY` | Uses the Gemini OpenAI-compatible endpoint. |
| [Groq](https://console.groq.com/docs) | `groq` | `https://api.groq.com/openai/v1/` | `openai/gpt-oss-120b` | `GROQ_API_KEY` |  |
| [Mistral](https://docs.mistral.ai) | `mistral` | `https://api.mistral.ai/v1/` | `devstral-latest` | `MISTRAL_API_KEY` |  |
| [xAI](https://docs.x.ai) | `xai` | `https://api.x.ai/v1/` | `grok-4.6` | `XAI_API_KEY` |  |
| [Cerebras](https://inference-docs.cerebras.ai) | `cerebras` | `https://api.cerebras.ai/v1/` | `gpt-oss-120b` | `CEREBRAS_API_KEY` |  |
| [Perplexity](https://docs.perplexity.ai) | `perplexity` | `https://api.perplexity.ai/` | `sonar-pro` | `PERPLEXITY_API_KEY` |  |
| [Cohere](https://docs.cohere.com/docs/compatibility-api) | `cohere` | `https://api.cohere.com/compatibility/v1/` | `command-a-plus-05-2026` | `COHERE_API_KEY` | Uses the Cohere compatibility API. |
| [Moonshot AI](https://platform.moonshot.ai/docs) | `moonshot` | `https://api.moonshot.ai/v1/` | `kimi-k3` | `MOONSHOT_API_KEY` |  |
| [Z.AI (GLM)](https://docs.z.ai) | `zai` | `https://api.z.ai/api/paas/v4/` | `glm-5.3` | `ZAI_API_KEY` | International endpoint; mainland accounts use open.bigmodel.cn. |
| [Qwen (DashScope)](https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope) | `qwen` | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1/` | `qwen3.8-max` | `DASHSCOPE_API_KEY` | International DashScope endpoint; mainland accounts use dashscope.aliyuncs.com. |
| [SambaNova](https://docs.sambanova.ai/) | `sambanova` | `https://api.sambanova.ai/v1/` | `gpt-oss-120b` | `SAMBANOVA_API_KEY` |  |
| [NVIDIA NIM](https://docs.api.nvidia.com/) | `nvidia` | `https://integrate.api.nvidia.com/v1/` | `deepseek-ai/deepseek-v4.1-flash` | `NVIDIA_API_KEY` |  |
| [Azure OpenAI](https://learn.microsoft.com/azure/ai-foundry/openai/api-version-lifecycle) | `azure-openai` | You give it | `gpt-5.6` | `AZURE_OPENAI_API_KEY` | Give your resource endpoint, such as https://NAME.openai.azure.com/openai/v1/. The model is your deployment name. |

## Aggregators and routers

| Provider | Preset | Default address | Default model | Key variable | Notes |
| --- | --- | --- | --- | --- | --- |
| [OpenRouter](https://openrouter.ai/docs) | `openrouter` | `https://openrouter.ai/api/v1/` | `openai/gpt-5.6` | `OPENROUTER_API_KEY` | Model IDs use the vendor/model form. |
| [Together AI](https://docs.together.ai) | `together` | `https://api.together.xyz/v1/` | `Qwen/Qwen3-Coder-Next-FP8` | `TOGETHER_API_KEY` |  |
| [Fireworks AI](https://docs.fireworks.ai) | `fireworks` | `https://api.fireworks.ai/inference/v1/` | `accounts/fireworks/models/kimi-k3` | `FIREWORKS_API_KEY` | Model IDs use the accounts/fireworks/models form. |
| [GitHub Models](https://docs.github.com/en/github-models) | `github-models` | `https://models.github.ai/inference/` | `openai/gpt-4.1` | `GITHUB_TOKEN` | Uses a GitHub token with the models:read permission. |
| [OpenCode Zen](https://opencode.ai/docs/providers/) | `opencode-zen` | `https://opencode.ai/inference/openai/v1/` | `claude-sonnet-5-5` | `OPENCODE_API_KEY` |  |
| [OpenCode Go](https://opencode.ai/docs/go/) | `opencode-go` | `https://opencode.ai/inference/go/openai/v1/` | `deepseek-v4.1-flash` | `OPENCODE_API_KEY` | Subscription with monthly limits per model. DeepSeek models need the Global region in your OpenCode privacy settings. |
| [DeepInfra](https://deepinfra.com/docs/openai_api) | `deepinfra` | `https://api.deepinfra.com/v1/openai/` | `deepseek-ai/DeepSeek-V4-Flash` | `DEEPINFRA_API_KEY` |  |
| [Novita AI](https://novita.ai/docs/guides/llm-api) | `novita` | `https://api.novita.ai/openai/v1/` | `deepseek/deepseek-v4.1-flash-1017` | `NOVITA_API_KEY` |  |
| [Hugging Face](https://huggingface.co/docs/inference-providers) | `huggingface` | `https://router.huggingface.co/v1/` | `deepseek-ai/DeepSeek-V4.1-Flash` | `HF_TOKEN` |  |
| [Chutes](https://chutes.ai/docs) | `chutes` | `https://llm.chutes.ai/v1/` | `deepseek-ai/DeepSeek-V4.1-Flash-1017` | `CHUTES_API_KEY` |  |
| [Venice](https://docs.venice.ai/) | `venice` | `https://api.venice.ai/api/v1/` | `qwen3-coder-480b-a35b-instruct-turbo` | `VENICE_API_KEY` |  |
| [Avian](https://avian.io/docs) | `avian` | `https://api.avian.io/v1/` | `deepseek/deepseek-v4.1-flash` | `AVIAN_API_KEY` |  |
| [Requesty](https://docs.requesty.ai/) | `requesty` | `https://router.requesty.ai/v1/` | `moonshot/kimi-k3.1-pro` | `REQUESTY_API_KEY` |  |
| [Vercel AI Gateway](https://vercel.com/docs/ai-gateway) | `vercel-ai-gateway` | `https://ai-gateway.vercel.sh/v1/` | `deepseek/deepseek-v4-flash` | `AI_GATEWAY_API_KEY` |  |
| [Nebius Token Factory](https://docs.tokenfactory.nebius.com/) | `nebius` | `https://api.tokenfactory.nebius.com/v1/` | `openai/gpt-oss-120b` | `NEBIUS_API_KEY` |  |
| [Hyperbolic](https://docs.hyperbolic.xyz/docs/inference-api) | `hyperbolic` | `https://api.hyperbolic.xyz/v1/` | `openai/gpt-oss-120b` | `HYPERBOLIC_API_KEY` |  |
| [SiliconFlow](https://docs.siliconflow.com/) | `siliconflow` | `https://api.siliconflow.com/v1/` | `Qwen/Qwen3-Coder-480B-A35B-Instruct` | `SILICONFLOW_API_KEY` | International endpoint; mainland accounts use api.siliconflow.cn. |
| [Featherless](https://featherless.ai/docs/getting-started) | `featherless` | `https://api.featherless.ai/v1/` | `Qwen/Qwen3-Coder-30B-A3B-Instruct` | `FEATHERLESS_API_KEY` | Model IDs use the Hugging Face owner/model form. |

## On your computer

| Provider | Preset | Default address | Default model | Key variable | Notes |
| --- | --- | --- | --- | --- | --- |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | `llama-cpp` | `local://llama-cpp/` | `Qwen3.5-0.8B-Q8_0.gguf` | None |  |
| [LM Studio](https://lmstudio.ai/docs) | `lm-studio` | `http://127.0.0.1:1234/v1/` | `local-model` | None |  |
| [Ollama](https://docs.ollama.com/api/openai-compatibility) | `ollama` | `http://127.0.0.1:11434/v1/` | `gpt-oss:20b` | None | Local server; no credential is required. |
| [vLLM](https://docs.vllm.ai/en/latest/serving/openai_compatible_server.html) | `vllm` | `http://127.0.0.1:8000/v1/` | `local-model` | None | Local server; use provider save when it does not listen on port 8000. |

## Your own server

| Provider | Preset | Default address | Default model | Key variable | Notes |
| --- | --- | --- | --- | --- | --- |
| [OpenAI compatible](https://platform.openai.com/docs/api-reference/chat) | `openai-compatible` | You give it | `model` | None | Your own server, such as llama-server. You give its address, model, and context size. |

## Not supported

These need a sign-in scheme other than an API key, so they are not available yet.

| Provider | Reason |
| --- | --- |
| Amazon Bedrock | Requires AWS SigV4 credentials; not an OpenAI-compatible endpoint. |
| Google Vertex AI | Requires Google service-account credentials; not an OpenAI-compatible endpoint. |
