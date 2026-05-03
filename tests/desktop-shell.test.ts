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

    expect(onboarding).toContain('id="provider-base-url"');
    expect(onboarding).toContain('<select id="provider-model"');
    expect(onboarding).not.toContain('<input id="provider-model"');
    expect(constants).toContain('models:');
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
    expect(onboarding).toContain('disabled');
    expect(state).toContain("providerAuthMethod: 'api-key'");
    expect(setup).toContain('providerAuthMethod');
    expect(rust).toContain('provider_auth_method: String');
    expect(rust).toContain('Browser account authorization is not supported for direct model calls');
    expect(onboarding).toContain('id="provider-api-key"');
    expect(onboarding).toContain('id="test-provider"');
    expect(setup).toContain("invokeTauri('test_provider'");
    expect(rust).toContain('struct TestProviderRequest');
    expect(rust).toContain('async fn test_provider');
    expect(rust).toContain('Provider endpoint must start with http:// or https://');
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
    expect(state).toContain('setTimeout');
    expect(state).toContain('argentum:state-change');
    expect(main).toContain('data-toggle-notification-mute');
    expect(css).toContain('.notification-layer');
    expect(css).toContain('.notification-toast');
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

  test('chat uses explicit profile fields, useful local replies, and a terminal view', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');

    expect(chat).toContain('id="profile-user-name"');
    expect(chat).toContain('id="profile-agent-name"');
    expect(chat).toContain('renderTerminalPanel');
    expect(chat).toContain('terminal-panel');
    expect(main).toContain('buildLocalReply');
    expect(main).toContain('saveProfileFromInputs');
    expect(main).toContain('addTerminalEntry');
    expect(main).toContain('sendChatMessage');
    expect(main).toContain('runChatAction');
    expect(chat).toContain('chat-action-row');
    expect(chat).toContain('Start Gateway');
    expect(chat).toContain('Check Gateway');
    expect(state).toContain('terminalEntries:');
    expect(state).toContain('agentName:');
    expect(state).toContain('userName:');
    expect(css).toContain('.profile-panel');
    expect(css).toContain('.terminal-panel');
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

  test('saves richer setup payload and secrets outside YAML', () => {
    const setup = read('src/ui/desktop/modules/setup.js');
    const rust = read('src/desktop/src/lib.rs');

    expect(setup).toContain('function buildSetupPayload()');
    expect(setup).toContain('experienceLevel');
    expect(setup).toContain('selectedChannels');
    expect(setup).toContain('providerApiKey');
    expect(rust).toContain('fn save_setup');
    expect(rust).toContain('secrets.env');
    expect(rust).toContain('Provider keys are added by the desktop credential flow');
    expect(rust).toContain('fn render_config');
    expect(rust).toContain('workspaceRoot');
  });
});
