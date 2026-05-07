import { existsSync, readFileSync } from 'fs';

const read = (path: string): string => readFileSync(path, 'utf8');

describe('Argentum desktop shell', () => {
  test('defines a native desktop application branded as Argentum', () => {
    expect(existsSync('src/desktop/tauri.conf.json')).toBe(true);
    expect(existsSync('src/desktop/src/lib.rs')).toBe(true);
    expect(existsSync('src/ui/desktop/assets/argentum.png')).toBe(true);

    const config = JSON.parse(read('src/desktop/tauri.conf.json')) as {
      productName?: string;
      version?: string;
      identifier?: string;
      app?: { windows?: Array<{ title?: string; width?: number; height?: number }> };
      bundle?: { active?: boolean; icon?: string[]; externalBin?: string[] };
    };

    expect(config.productName).toBe('Argentum');
    expect(config.version).toBe('0.0.4');
    expect(config.identifier).toBe('com.argentum.desktop');
    expect(config.app?.windows?.[0]).toEqual(
      expect.objectContaining({ title: 'Argentum', width: 1280, height: 800 }),
    );
    expect(config.bundle?.active).toBe(true);
    expect(config.bundle?.icon).toEqual(
      expect.arrayContaining([
        '../../assets/brand/argentum.png',
        '../../installer/macos/argentum.icns',
        '../../installer/wix/argentum.ico',
      ]),
    );
    expect(config.bundle?.externalBin).toEqual(['binaries/argentum-cli']);

    const html = read('src/ui/desktop/index.html');
    expect(html).toContain('<title>Argentum</title>');
    expect(html).toContain('./assets/argentum.png');
    expect(html).toContain('type="module"');
    expect(html).toContain('data-icon="shield"');
    expect(html).toContain('data-icon="settings"');
    expect(html).not.toContain('../../assets/brand/argentum.png');
  });

  test('splits desktop UI into focused modules with small contracts', () => {
    for (const file of [
      'src/ui/desktop/modules/constants.js',
      'src/ui/desktop/modules/state.js',
      'src/ui/desktop/modules/setup.js',
      'src/ui/desktop/modules/shell.js',
      'src/ui/desktop/modules/onboarding.js',
      'src/ui/desktop/modules/chat.js',
      'src/ui/desktop/modules/sections.js',
      'src/ui/desktop/modules/icons.js',
    ]) {
      expect(existsSync(file)).toBe(true);
    }

    const sections = read('src/ui/desktop/modules/sections.js');
    const constants = read('src/ui/desktop/modules/constants.js');
    expect(sections).toContain('healthCheck');
    expect(sections).toContain('gatewayModule');
    expect(sections).not.toContain('honestModule');
    expect(sections).not.toContain('Not fully wired yet');
    for (const hiddenTitle of [
      "title: 'Agents'",
      "title: 'Agent Runner'",
      "title: 'Skills Library'",
      "title: 'Channels'",
      "title: 'Knowledge Graph'",
      "title: 'Memory'",
    ]) {
      expect(constants).not.toContain(hiddenTitle);
    }

    const shell = read('src/ui/desktop/modules/shell.js');
    expect(shell).toContain('renderModule(module)');
    expect(shell).toContain('Module contained');
    expect(shell).toContain('Other Argentum modules remain available');
  });

  test('desktop MVP exposes only working product surfaces and gateway actions', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const shell = read('src/ui/desktop/modules/shell.js');

    for (const title of [
      "title: 'Chat'",
      "title: 'Gateway'",
      "title: 'Activity Logs'",
      "title: 'Security & Permissions'",
      "title: 'Settings'",
      "title: 'Diagnostics'",
    ]) {
      expect(constants).toContain(title);
    }

    for (const actionId of [
      "id: 'gateway-start'",
      "id: 'gateway-status'",
      "id: 'gateway-stop'",
      "id: 'gateway-logs'",
    ]) {
      expect(constants).toContain(actionId);
    }

    expect(sections).toContain('function gatewayModule');
    expect(shell).toContain('action.buttonLabel');
    expect(shell).not.toContain('Prepared</button>');
  });

  test('rewrites onboarding with plain-language access and non-repeating steps', () => {
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const constants = read('src/ui/desktop/modules/constants.js');

    expect(constants).toContain('Workspace location');
    expect(constants).toContain('Capabilities');
    expect(constants).toContain('Review > Test > Pass');
    expect(constants).not.toContain('Finish and launch');
    expect(onboarding).toContain('onboarding-backdrop');
    expect(onboarding).toContain('onboarding-modal');
    expect(onboarding).toContain('What Argentum is');
    expect(onboarding).toContain('Default access: all folders/files inside workspace folder');
    expect(onboarding).toContain('renderCapabilitiesStep');
    expect(onboarding).toContain('Self-repair');
    expect(onboarding).not.toContain('renderFinishStep');
    expect(onboarding).not.toContain('Default access</span><strong>Workspace</strong>');
  });

  test('supports Beginner Comfortable Expert experience levels', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const state = read('src/ui/desktop/modules/state.js');

    expect(constants).toContain("id: 'beginner'");
    expect(constants).toContain("id: 'comfortable'");
    expect(constants).toContain("id: 'expert'");
    expect(state).toContain("experienceLevel: 'beginner'");
    expect(constants).not.toContain('AI Noob');
  });

  test('uses the native Tauri dialog plugin for workspace folder selection', () => {
    const cargo = read('src/desktop/Cargo.toml');
    const rust = read('src/desktop/src/lib.rs');
    const capabilities = read('src/desktop/capabilities/default.json');
    const setup = read('src/ui/desktop/modules/setup.js');
    const utils = read('src/ui/desktop/modules/utils.js');
    const packageJson = JSON.parse(read('package.json')) as { dependencies?: Record<string, string> };

    expect(packageJson.dependencies).toHaveProperty('@tauri-apps/plugin-dialog');
    expect(cargo).toContain('tauri-plugin-dialog');
    expect(rust).toContain('tauri_plugin_dialog::init()');
    expect(capabilities).toContain('dialog:allow-open');
    expect(setup).toContain('chooseWorkspaceFolder');
    expect(utils).toContain('window.__TAURI__?.dialog?.open');
    expect(utils).toContain('directory: true');
  });

  test('expands provider setup and custom endpoint testing', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const setup = read('src/ui/desktop/modules/setup.js');
    const state = read('src/ui/desktop/modules/state.js');
    const rust = read('src/desktop/src/lib.rs');
    const main = read('src/ui/desktop/main.js');
    const icons = read('src/ui/desktop/modules/icons.js');

    for (const provider of [
      'OpenAI',
      'Anthropic Claude',
      'Google Gemini',
      'OpenRouter',
      'NVIDIA',
      'Groq',
      'MiniMax',
      'Ollama / local',
      'Custom endpoint',
    ]) {
      expect(constants).toContain(provider);
    }

    expect(constants).toContain('websiteUrl:');
    expect(constants).toContain("authMethods: ['api-key', 'browser-account']");
    expect(onboarding).toContain('provider-website-link');
    expect(onboarding).toContain('data-open-external');
    expect(icons).toContain('externalLink');
    expect(state).toContain("providerSetupStage: 'provider'");
    expect(state).toContain('providerSelectionConfirmed: false');
    expect(onboarding).toContain('function renderProviderChoiceStep');
    expect(onboarding).toContain('function renderProviderAuthStep');
    expect(onboarding).toContain('function renderProviderCredentialStep');
    expect(onboarding).toContain('function renderProviderModelStep');
    expect(onboarding).toContain("state.providerSetupStage === 'provider'");
    expect(onboarding).toContain('data-provider-setup-stage="auth"');
    expect(onboarding).toContain('data-provider-setup-stage="model"');
    expect(onboarding).toContain('id="continue-provider-model"');
    expect(onboarding).toContain('id="provider-base-url"');
    expect(onboarding).toContain('<select id="provider-model"');
    expect(onboarding).not.toContain('<input id="provider-model"');
    expect(onboarding).toContain('renderProgressiveProviderFrame');
    expect(onboarding).toContain('provider-focus-panel');
    expect(onboarding).toContain('data-model-auth-methods');
    expect(constants).toContain('models:');
    expect(constants).toContain('codexModels:');
    for (const model of [
      'gpt-5.5',
      'gpt-5.5-pro',
      'gpt-5.4',
      'gpt-5.4-pro',
      'gpt-5.4-mini',
      'gpt-5.4-nano',
      'gpt-5-mini',
      'gpt-5-nano',
      'gpt5',
      'gpt-4.1',
    ]) {
      expect(constants).toContain(`id: '${model}'`);
    }
    expect(constants).toContain('providerAuthMethods');
    expect(constants).toContain('API key / Platform API');
    expect(constants).toContain('Browser account authorization');
    expect(onboarding).toContain('id="provider-auth-method"');
    expect(onboarding).toContain('id="start-codex-oauth"');
    expect(onboarding).toContain('id="complete-codex-oauth"');
    expect(state).toContain("providerAuthMethod: 'api-key'");
    expect(state).toContain('codexOAuth');
    expect(setup).toContain('providerAuthMethod');
    expect(setup).toContain("invokeTauri('start_codex_oauth'");
    expect(setup).toContain("invokeTauri('complete_codex_oauth'");
    expect(setup).toContain('openExternalUrl');
    expect(setup).toContain("state.apiTest = {\n        status: 'idle'");
    expect(setup).not.toContain('Test Provider can now use the workspace credential');
    expect(main).toContain('data-open-external');
    expect(rust).toContain('provider_auth_method: String');
    expect(rust).toContain('struct CodexOAuthStartRequest');
    expect(rust).toContain('async fn start_codex_oauth');
    expect(rust).toContain('async fn complete_codex_oauth');
    expect(rust).toContain('struct OpenExternalUrlRequest');
    expect(rust).toContain('fn open_external_url');
    expect(rust).toContain('allowed_external_url');
    expect(rust).toContain('codex_oauth_tokens_saved');
    expect(rust).toContain('https://auth.openai.com/api/accounts/deviceauth/usercode');
    expect(rust).not.toContain('urn:ietf:params:oauth:grant-type:token-exchange');
    expect(rust).not.toContain('OpenAI/Codex API key exchange');
    expect(rust).toContain('"OPENAI_API_KEY": serde_json::Value::Null');
    expect(onboarding).toContain('id="provider-api-key"');
    expect(onboarding).toContain('id="test-provider"');
    expect(setup).toContain("invokeTauri('test_provider'");
    expect(setup).toContain('persistRuntimeSettings');
    expect(setup).toContain("await persistRuntimeSettings('chat')");
    expect(setup).toContain("await persistRuntimeSettings('provider-test')");
    expect(rust).toContain('struct TestProviderRequest');
    expect(rust).toContain('async fn test_provider');
    expect(rust).toContain('Provider endpoint must start with http:// or https://');
    expect(rust).toContain('const CODEX_COMPAT_CLIENT_VERSION');
    expect(rust).toContain('fn codex_models_url');
    expect(rust).toContain('async fn test_codex_browser_provider');
    expect(rust).toContain('originator');
    expect(rust).toContain('USER_AGENT');
    expect(rust).not.toContain('.header("version", env!("CARGO_PKG_VERSION"))');
  });

  test('supports multiple selected channels including WhatsApp as advanced pending', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const state = read('src/ui/desktop/modules/state.js');
    const rust = read('src/desktop/src/lib.rs');

    expect(constants).toContain("id: 'local'");
    expect(constants).toContain("id: 'webchat'");
    expect(constants).toContain("id: 'telegram'");
    expect(constants).toContain("id: 'whatsapp'");
    expect(constants).toContain('Advanced');
    expect(onboarding).toContain('type="checkbox"');
    expect(onboarding).toContain('data-channel-id');
    expect(state).toContain("selectedChannels: ['local']");
    expect(rust).toContain('selected_channels: Vec<String>');
    expect(rust).toContain('whatsapp-bridge');
  });

  test('renders layered notifications with history, mute, and auto-dismiss', () => {
    const shell = read('src/ui/desktop/modules/shell.js');
    const state = read('src/ui/desktop/modules/state.js');
    const main = read('src/ui/desktop/main.js');
    const css = read('src/ui/desktop/styles.css');

    expect(shell).toContain('renderNotifications');
    expect(shell).toContain('notification-layer');
    expect(shell).toContain('notification-menu');
    expect(state).toContain('notificationHistory:');
    expect(state).toContain('notificationsMuted:');
    expect(state).toContain('notificationsMenuOpen:');
    expect(state).toContain('state.notificationHistory = [];');
    expect(state).toContain('setTimeout');
    expect(state).toContain('argentum:state-change');
    expect(main).toContain('data-toggle-notification-mute');
    expect(css).toContain('.notification-layer');
    expect(css).toContain('.notification-toast');
    expect(css).toContain('height: 100vh');
    expect(css).toContain('.topbar');
    expect(css).toContain('position: sticky');
    expect(css).toContain('--z-notification: 220');
    expect(css).toContain('--z-onboarding: 900');
    expect(css).toContain('z-index: var(--z-onboarding)');
  });

  test('shows onboarding as a blocking overlay and can restart after setup', () => {
    const main = read('src/ui/desktop/main.js');
    const shell = read('src/ui/desktop/modules/shell.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const sections = read('src/ui/desktop/modules/sections.js');

    expect(main).toContain('state.onboardingOpen ? renderModule(modules.onboarding) :');
    expect(main).toContain('restartOnboarding');
    expect(main).toContain('cancelOnboarding');
    expect(onboarding).toContain('data-cancel-onboarding');
    expect(onboarding).toContain('state.setupComplete');
    expect(shell).toContain("section.id !== 'onboarding'");
    expect(sections).toContain('data-restart-onboarding');
  });

  test('hides onboarding after completion and launches Chat without local setup loop', () => {
    const main = read('src/ui/desktop/main.js');

    expect(main).toContain("state.setupComplete = true");
    expect(main).toContain("state.onboardingOpen = false");
    expect(main).toContain("state.activeSection = 'chat'");
    expect(main).toContain('resetIntroChat');
    expect(main).not.toContain(
      'Got it. I am keeping this local for now. Once provider testing passes, this same chat surface can switch to live model execution.',
    );
  });

  test('runtime previews use real examples with animated abstract flow', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const css = read('src/ui/desktop/styles.css');

    expect(constants).toContain('examples:');
    expect(constants).toContain('demoSteps:');
    expect(onboarding).toContain('renderRuntimeDemo');
    expect(onboarding).toContain('runtime-demo');
    expect(onboarding).toContain('data-demo-step');
    expect(css).toContain('@keyframes demo-flow');
  });

  test('chat stays focused on conversation and composer controls', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');
    const utils = read('src/ui/desktop/modules/utils.js');
    const constants = read('src/ui/desktop/modules/constants.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');

    expect(chat).not.toContain('chat-action-row');
    expect(chat).not.toContain('Start Gateway');
    expect(chat).not.toContain('Check Gateway');
    expect(chat).toContain('renderMarkdown');
    expect(chat).toContain('markdown-body');
    expect(chat).toContain('renderTypingIndicator');
    expect(chat).toContain('context-meter');
    expect(chat).toContain('model-context-summary');
    expect(utils).toContain('function renderMarkdown');
    expect(utils).toContain('estimateContextTokens');
    expect(utils).toContain('modelMetadataFor');
    expect(constants).toContain('thinkingLevels');
    expect(constants).toContain('modelMetadata');
    expect(onboarding).toContain('model-detail-panel');
    expect(onboarding).toContain('modelMetadataFor');
    expect(chat).toContain('recent-chat-list');
    expect(chat).toContain('new-chat');
    expect(chat).toContain('activeChatId');
    expect(chat).toContain('composer-tools');
    expect(chat).toContain('id="attach-file"');
    expect(chat).toContain('id="voice-input"');
    expect(chat).toContain('id="thinking-level"');
    expect(main).toContain('buildLocalReply');
    expect(main).toContain('chooseChatAttachment');
    expect(main).toContain('startVoiceInput');
    expect(main).toContain('addTerminalEntry');
    expect(main).toContain('sendChatMessage');
    expect(main).toContain("state.chatStreaming = true");
    expect(main).toContain("state.chatStreaming = false");
    expect(main).toContain('setActiveChatSession');
    expect(main).toContain('createChatSession');
    expect(main).toContain('hydrateChatHistory');
    expect(state).toContain('chatSessions:');
    expect(state).toContain('activeChatId:');
    expect(state).toContain('setActiveChatSession');
    expect(state).toContain('createChatSession');
    expect(state).toContain('syncActiveChatSession');
    expect(state).toContain('hydrateChatHistory');
    expect(state).toContain('argentum.chatHistory.v1');
    expect(state).toContain('storage.setItem');
    expect(state).toContain("thinkingLevel: 'balanced'");
    expect(state).toContain('chatStreaming:');
    expect(state).toContain('systemPrompt:');
    expect(state).toContain('selectedContextAccess:');
    expect(state).toContain('chatAttachments:');
    expect(state).toContain('terminalEntries:');
    expect(state).toContain('agentName:');
    expect(state).toContain('userName:');
    expect(css).toContain('.recent-chat-list');
    expect(css).toContain('.composer-tools');
    expect(css).toContain('.markdown-body');
    expect(css).toContain('.typing-indicator');
    expect(css).toContain('.context-meter');
    expect(css).toContain('.model-detail-panel');
  });

  test('desktop actions execute through whitelisted Tauri commands with structured output', () => {
    const rust = read('src/desktop/src/lib.rs');
    const setup = read('src/ui/desktop/modules/setup.js');
    const main = read('src/ui/desktop/main.js');
    const packageJson = read('package.json');
    const workflow = read('.github/workflows/desktop.yml');

    expect(rust).toContain('struct RunDesktopActionResponse');
    expect(rust).toContain('pid: Option<String>');
    expect(rust).toContain('health_url: Option<String>');
    expect(rust).toContain('log_path: Option<String>');
    expect(rust).toContain('fn run_gateway_action');
    expect(rust).toContain('fn resolve_sidecar_path');
    expect(rust).toContain('fn sidecar_file_names');
    expect(rust).toContain('argentum-cli.exe');
    expect(rust).toContain('"Gateway failed to start because port');
    expect(rust).not.toContain('Gateway start is prepared');
    expect(rust).toContain('send_chat_message');
    expect(rust).toContain('fn provider_http_error');
    expect(rust).toContain('rate or quota limit');
    expect(setup).toContain("invokeTauri('send_chat_message'");
    expect(main).toContain("actionId: 'gateway-start'");
    expect(packageJson).toContain('build:desktop-sidecar');
    expect(packageJson).toContain('predesktop:build');
    expect(workflow).toContain('Build desktop CLI sidecar');
  });

  test('routes OpenAI browser-account OAuth through live Codex chat runtime', () => {
    const rust = read('src/desktop/src/lib.rs');
    const cargo = read('src/desktop/Cargo.toml');

    expect(cargo).toContain('base64');
    expect(rust).toContain('struct CodexBrowserAuth');
    expect(rust).toContain('fn codex_oauth_auth');
    expect(rust).toContain('fn codex_responses_url');
    expect(rust).toContain('https://chatgpt.com/backend-api/codex');
    expect(rust).toContain('ChatGPT-Account-ID');
    expect(rust).toContain('X-OpenAI-Fedramp');
    expect(rust).toContain('async fn send_codex_chat_message');
    expect(rust).toContain('fn parse_codex_sse_response');
    expect(rust).toContain('"stream": true');
    expect(rust).toContain('codex_browser_headers');
    expect(rust).toContain('Codex model catalog');
    expect(rust).toContain('requires a newer Codex client');
    expect(rust).toContain('OpenAI/Codex browser account auth is ready for live Codex chat.');
    expect(rust).not.toContain('live Codex runtime routing is not wired');
    expect(rust).not.toContain('Use API key auth for live Platform API chat until');
  });

  test('saves richer setup payload and secrets outside YAML', () => {
    const setup = read('src/ui/desktop/modules/setup.js');
    const rust = read('src/desktop/src/lib.rs');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const constants = read('src/ui/desktop/modules/constants.js');

    expect(setup).toContain('function buildSetupPayload()');
    expect(setup).toContain('experienceLevel');
    expect(setup).toContain('selectedChannels');
    expect(setup).toContain('providerApiKey');
    expect(setup).toContain('agentName');
    expect(setup).toContain('userName');
    expect(setup).toContain('systemPrompt');
    expect(setup).toContain('selectedContextAccess');
    expect(setup).toContain('thinkingLevel');
    expect(setup).toContain('securityProfile');
    expect(setup).toContain('selectedChannels');
    expect(onboarding).toContain('id="onboarding-user-name"');
    expect(onboarding).toContain('id="onboarding-agent-name"');
    expect(onboarding).toContain('id="onboarding-system-prompt"');
    expect(sections).toContain('security-settings-grid');
    expect(sections).toContain('data-context-access');
    expect(constants).toContain('contextAccessOptions');
    expect(rust).toContain('fn save_setup');
    expect(rust).toContain('secrets.env');
    expect(rust).toContain('fn merge_existing_secrets');
    expect(rust).toContain('agent_name: String');
    expect(rust).toContain('system_prompt: String');
    expect(rust).toContain('selected_context_access: Vec<String>');
    expect(rust).toContain('thinking_level: String');
    expect(rust).toContain('security_profile: String');
    expect(rust).toContain('selected_channels: Vec<String>');
    expect(rust).toContain('fn build_system_prompt');
    expect(rust).toContain('fn build_runtime_context');
    expect(rust).toContain('fn argentum_tool_definitions');
    expect(rust).toContain('fn execute_argentum_tool');
    expect(rust).toContain('tool_calls');
    expect(rust).toContain('reasoning_effort');
    expect(rust).toContain('Provider keys are added by the desktop credential flow');
    expect(rust).toContain('fn render_config');
    expect(rust).toContain('workspaceRoot');
  });
});
