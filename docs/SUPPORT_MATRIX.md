# Systems, requirements and support

What Argentum 0.1.0 runs on, what it needs, and what has been tested.

"Tested" means the release build was installed and used on that system. "Not
tested" means it may work, but nobody has checked it. Reports are welcome at
contact@ag064.eu or as GitHub issues.

## What is supported

Official support covers the release files listed below: the Argentum app, its
built-in modules and themes, and Argentum Genesis. Bugs in them are fixed in
later releases.

Extensions, MCP servers, widgets, modules, themes and other additions made by
other people, and builds changed from source, are not officially supported.
Questions about them are still welcome, and help is given when time allows.

## Downloads

| File | For |
| --- | --- |
| `Argentum-0.1.0-windows-x64.msi` | Windows installer for the desktop app and the CLI |
| `Argentum-0.1.0-windows-x64.zip` | The desktop app without installing |
| `Argentum-0.1.0-android-arm64.apk` | The Android app |
| `argentum-genesis-0.1.0-windows-x64.zip` | Argentum Genesis, the command-line agent |
| `argentum-genesis-desktop-control-0.1.0-windows-x64.zip` | Argentum Genesis with Windows desktop control, extensions, and MCP |

Linux and macOS files come with the next release. See [Linux and macOS](#linux-and-macos).

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
| llama.cpp runtime b12650 prerelease, Vulkan build | 38.9 MB | |
| Qwen3.5 0.8B, Q8_0 | 0.83 GB | 2 GB |
| Qwen3.5 2B, Q4_K_M | 1.28 GB | 3 GB |
| Qwen3.5 4B, Q4_K_M | 2.74 GB | 5 GB |
| Inkling-2-Mini AD-Q4_K with LoRA and mmproj | 17.2 GB | Depends on GPU memory and context size |

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

Linux and macOS builds are new in 0.1.1 and are not released yet. They are built
and tested in CI on every pull request, and the `Release` workflow builds their
files. Nobody has used them on a real desktop yet, so treat them as previews.

| | Linux | macOS |
| --- | --- | --- |
| Supported | x64 with glibc 2.35 or later, such as Ubuntu 22.04, Debian 12 and Fedora 36 or later | macOS 11 or later, on Apple silicon (arm64) and Intel (x64) |
| Files | `Argentum-<version>-linux-x64.tar.gz` (desktop app), `argentum-genesis-<version>-linux-x64.tar.gz` (CLI), `argentum_<version>_amd64.deb` (both) | `Argentum-<version>-macos-arm64.tar.gz` and `-macos-x64.tar.gz` (`Argentum.app`), `argentum-genesis-<version>-macos-arm64.tar.gz` and `-macos-x64.tar.gz` (CLI) |
| Tested | CI only: workspace tests on Ubuntu 24.04, release build on Ubuntu 22.04, `.deb` install and removal, desktop app start under a virtual display | CI only: workspace tests on macOS 15 Apple silicon; build, strict lint and desktop app start on Apple silicon and Intel |
| Desktop needs | X11 or Wayland, fontconfig and libxkbcommon | Nothing extra |
| Signing | Not signed | Not signed or notarized. macOS blocks the app the first time: Control-click it, choose Open, or run `xattr -d com.apple.quarantine Argentum.app` |

On Linux and macOS the agent works the same way as on Windows: the same project
policy, approvals, guarded edits, restore, goals and providers. These parts
differ:

| Feature | On Linux and macOS |
| --- | --- |
| Shell commands and fixed verification runners | Run with `/bin/sh` or the runner's own program, as your own user, in their own process group, after the same approvals. Stopping a command stops its whole group. There is no memory, CPU or process-count limit like the Windows job object, and a program that deliberately leaves its group (for example with `setsid`) can outlive the command |
| API keys | macOS keeps them in the Keychain. Linux keeps them in the kernel key store, which holds them in memory only: a restart clears them, and they can expire a few days after you log out. Set the provider's key variable if you want a key kept. The Windows credential prompt has no equivalent, so enter keys in the terminal or Settings |
| ChatGPT plan sign-in | Opens your browser with `xdg-open` (Linux) or `open` (macOS) |
| Updates | The app tells you when a new version is out. Installing it from the app is Windows-only, so download the new files yourself |
| Supplemental Rust checks (`check_rust`) | Not available. They need a Windows AppContainer |
| Local model add-on | Not available. Run llama.cpp, Ollama, LM Studio or vLLM yourself and add it as a provider |
| Windows desktop control, extensions and MCP builds, local PDF artifacts | Windows-only builds |
| Owner-only file permission checks | Windows-only. They are used only by the optional extension and artifact features above |
| Desktop UI tests | Two window tests are skipped on macOS, because Slint must open windows on the main thread there |
| Known open issue | One terminal test that picks a provider, model and context in `argentum chat` fails on the macOS CI runners: the model list request is closed before the reply arrives. It is skipped on macOS until it is diagnosed, so treat model selection in `argentum chat` on macOS as untested |

The Windows-only parts are tracked in the [roadmap](../ROADMAP.md).

## Model providers

| Provider | Status |
| --- | --- |
| DeepSeek | Tested with Argentum on Windows and Android |
| OpenCode Go | Tested with Argentum on Windows |
| ChatGPT plan | Supported, with browser sign-in from the CLI |
| Anthropic | Supported through its native Messages API |
| OpenAI, Azure OpenAI, Google Gemini, MiniMax, OpenRouter, Groq, Mistral, xAI, and the other hosted entries | Supported. Listed on the Testing tab in Settings until tested with Argentum |
| llama.cpp, LM Studio, Ollama, vLLM | Supported on your own computer |
| Any OpenAI-compatible server | Supported with an address and model you enter, when it needs no API key. Keys are stored only for the presets |
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
