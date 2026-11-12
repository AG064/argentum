# Systems, requirements and support

What Argentum 0.1.0 runs on, what it needs, and what has been tested.

"Tested" means the release build was installed and used on that system. "Not
tested" means it may work, but nobody has checked it. Reports are welcome at
contact@ag064.eu or as GitHub issues.

## Downloads

| File | For |
| --- | --- |
| `Argentum-0.1.0-windows-x64.msi` | Windows installer for the desktop app and the CLI |
| `Argentum-0.1.0-windows-x64.zip` | The desktop app without installing |
| `Argentum-0.1.0-android-arm64.apk` | The Android app |
| `argentum-genesis-0.1.0-windows-x64.zip` | Argentum Genesis, the command-line agent |
| `argentum-genesis-desktop-control-0.1.0-windows-x64.zip` | Argentum Genesis with Windows desktop control, extensions, and MCP |

## Windows

| | |
| --- | --- |
| Supported | Windows 11, 64-bit (x64) |
| Tested on | Windows 11 Pro 24H2 (build 26100) on a desktop PC, and a clean Windows Sandbox on the same build with no extra runtime installed and no network |
| Not tested | Windows 10, Windows Server, Windows on ARM (x64 emulation) |
| Processor | Any 64-bit x86 processor (Intel or AMD) |
| Memory | 4 GB of RAM or more is suggested |
| Graphics | Nothing special. The app draws with the CPU by default |
| Other software | None. The Visual C++ runtime is not needed. Git is only needed for the Git diff module |
| Network | Needed for hosted model providers and for update checks you start yourself |
| Install | The installer needs no administrator rights and installs for the current user. The portable zip runs from any folder |

The Windows files are not code signed yet, so SmartScreen may warn the first
time you run them. Choose "More info", then "Run anyway".

### Local model add-on (Windows)

Settings > Providers > Local model add-on downloads llama.cpp and one model.
Nothing is downloaded until you choose a model and press "Download and install".
Every file is pinned by address, size, and SHA-256.

| Download | Size | Suggested free memory |
| --- | --- | --- |
| llama.cpp runtime b10621, Vulkan build | 34 MB | |
| Qwen3.5 0.8B, Q8_0 | 0.83 GB | 2 GB |
| Qwen3.5 2B, Q4_K_M | 1.28 GB | 3 GB |
| Qwen3.5 4B, Q4_K_M | 2.74 GB | 5 GB |

The memory figures are estimates. A graphics card with a current Vulkan driver
makes local models faster. Small local models are fine for questions and small
edits, and much weaker than hosted models for larger coding tasks.

## Android

| | |
| --- | --- |
| Supported | Android 9 (API 28) or later on a 64-bit ARM phone or tablet (arm64-v8a) |
| Tested on | Xiaomi 13T Pro with Android 16, and the Android 16 emulator |
| Not tested | Android 9 to 15 on real devices, tablets, foldables, ChromeOS, phones with less than 8 GB of RAM |
| Not supported | 32-bit ARM and x86 phones |
| Network | Needed for hosted model providers |
| Install | Download the APK and allow your browser or file manager to install apps when Android asks. The APK is signed with the project's key |

On Android, the agent works in the app's own folder. You can grant "All files
access" in Android settings to let it reach your shared storage. Shell commands
run with the phone's built-in shell, with the same approvals as on Windows.

The local model add-on is not in the Android app. Use a hosted provider, or a
model server on your computer over your local network.

## Linux and macOS

There are no release files for Linux or macOS. The CLI builds from source on
both, and is not tested for release there. The desktop app has not been tested
on them.

## Model providers

| Provider | Status |
| --- | --- |
| DeepSeek | Tested with Argentum on Windows and Android |
| OpenCode Go | Tested with Argentum on Windows |
| ChatGPT plan | Supported, with browser sign-in from the CLI |
| Anthropic | Supported through its native Messages API |
| OpenAI, Azure OpenAI, Google Gemini, MiniMax, OpenRouter, Groq, Mistral, xAI, and the other hosted entries | Supported. Listed on the Testing tab in Settings until tested with Argentum |
| llama.cpp, LM Studio, Ollama, vLLM | Supported on your own computer |
| Any OpenAI-compatible server | Supported with an address and model you enter |
| Amazon Bedrock, Google Vertex AI | Not supported, because they need a cloud sign-in instead of an API key |

A model needs tool calling to work as a coding agent. Every preset, with its
address, default model, and key variable, is in [PROVIDERS.md](PROVIDERS.md).

## Optional features

| Feature | Where |
| --- | --- |
| Windows desktop control | In the `argentum-genesis-desktop-control` download. See [WINDOWS_APPLICATION_CONTROL.md](WINDOWS_APPLICATION_CONTROL.md) |
| Extensions, MCP servers, and the Playwright browser adapter | In the `argentum-genesis-desktop-control` download. Node.js and the browser are installed separately. See [EXTENSIONS_BROWSER.md](EXTENSIONS_BROWSER.md) |
| Local PDF artifacts | Build from source. See [LOCAL_ARTIFACTS.md](LOCAL_ARTIFACTS.md) |
| On-device models on Android | Build from source. See [ANDROID_HOST.md](ANDROID_HOST.md) |

No release file contains credentials, user data, or model weights.
