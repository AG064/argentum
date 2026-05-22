import { existsSync, readFileSync } from 'fs';

const read = (path: string): string => readFileSync(path, 'utf8').replace(/\r\n/g, '\n');

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
    expect(config.version).toBe('0.0.7');
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

  test('uses the May 2026 AG logo source across bundled brand assets', () => {
    const brand = readFileSync('assets/brand/argentum.png');
    const desktop = readFileSync('src/ui/desktop/assets/argentum.png');

    expect(desktop.equals(brand)).toBe(true);

    // PNG signature plus IHDR dimensions for the provided 1254x1254 transparent source.
    expect(brand.length).toBeGreaterThan(0);
    expect(brand.subarray(0, 8).toString('hex')).toBe('89504e470d0a1a0a');
    expect(brand.readUInt32BE(16)).toBe(1254);
    expect(brand.readUInt32BE(20)).toBe(1254);
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
      "title: 'Local Server'",
      "title: 'Activity Logs'",
      "title: 'Security & Permissions'",
      "title: 'Settings'",
      "title: 'Diagnostics'",
      "title: 'Argentum System Dashboard'",
    ]) {
      expect(constants).toContain(title);
    }

    for (const actionId of [
      "id: 'gateway-start'",
      "id: 'gateway-status'",
      "id: 'gateway-stop'",
      "id: 'gateway-logs'",
      "id: 'llama-server-start'",
      "id: 'llama-server-status'",
      "id: 'llama-server-stop'",
      "id: 'llama-server-logs'",
    ]) {
      expect(constants).toContain(actionId);
    }

    expect(sections).toContain('function gatewayModule');
    expect(sections).toContain('function localServerModule');
    expect(shell).toContain('action.buttonLabel');
    expect(shell).not.toContain('Prepared</button>');
  });

  test('adds Argentum System Dashboard as a real desktop-backed tab', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const setup = read('src/ui/desktop/modules/setup.js');
    const rust = read('src/desktop/src/lib.rs');
    const main = read('src/ui/desktop/main.js');
    const dashboard = read('src/ui/dashboard/index.html');
    const packagedDashboard = read('src/ui/desktop/dashboard/index.html');
    const packagedDashboardScript = read('src/ui/desktop/dashboard/dashboard.js');

    expect(constants).toContain("id: 'pc-stats'");
    expect(constants).toContain("icon: 'cpu'");
    expect(constants).toContain("title: 'Argentum System Dashboard'");
    expect(constants).toContain("id: 'system-dashboard'");
    expect(sections).toContain('const pcStatsModule');
    expect(sections).toContain("label: 'Argentum System Dashboard'");
    expect(sections).toContain("state.selectedContextAccess.includes('system-dashboard')");
    expect(sections).toContain('src="./dashboard/index.html"');
    expect(sections).toContain('data-system-dashboard-frame="true"');
    expect(sections).not.toContain(
      'Run the installed desktop app to stream local system statistics',
    );
    expect(sections).toContain('state.desktopState?.systemStats');
    expect(sections).not.toContain('pc-stats-layout');
    expect(main).toContain('function syncSystemDashboardFrame');
    expect(main).toContain('function syncSystemDashboardPolling');
    expect(main).toContain("state.selectedContextAccess.includes('system-dashboard')");
    expect(main).toContain('refreshSystemDashboardState({ silentErrors: true })');
    expect(main).toContain('10_000');
    expect(main).toContain('function captureOnboardingStepScroll');
    expect(main).toContain('function restoreOnboardingStepScroll');
    expect(main).toContain("type: 'argentum-system-stats'");
    expect(dashboard).toContain('<title>Argentum System Dashboard</title>');
    expect(packagedDashboard).toContain('<title>Argentum System Dashboard</title>');
    expect(packagedDashboard).toContain('<script src="./dashboard.js"></script>');
    expect(packagedDashboard).toContain('monaspace-krypton-latin-400-normal.woff2');
    expect(packagedDashboard).toContain('dashboard-settings-panel');
    expect(packagedDashboard).toContain('data-dashboard-module="overview"');
    expect(packagedDashboard).toContain('data-dashboard-module="live"');
    expect(packagedDashboard).toContain('data-dashboard-module="details"');
    expect(packagedDashboard).toContain('data-module-order="overview"');
    expect(packagedDashboard).toContain('id="cpu-temp-inline"');
    expect(packagedDashboardScript).toContain('desktopBridgeShape');
    expect(packagedDashboardScript).toContain('DASHBOARD_CONFIG_KEY');
    expect(packagedDashboardScript).toContain("order: ['overview', 'live', 'details']");
    expect(packagedDashboardScript).toContain('applyDashboardConfig');
    expect(packagedDashboardScript).toContain('wireDashboardSettings');
    expect(packagedDashboardScript).toContain('cpuCoresDetail');
    expect(packagedDashboardScript).toContain('usage_pct');
    expect(packagedDashboardScript).toContain('slice(0, 32)');
    expect(packagedDashboardScript).toContain('memoryAvailableBytes');
    expect(packagedDashboardScript).toContain('memoryCachedBytes');
    expect(packagedDashboardScript).toContain('memoryUsedMb');
    expect(packagedDashboardScript).toContain('temperatureSensors');
    expect(packagedDashboardScript).toContain('GPU temperature');
    expect(packagedDashboardScript).toContain('VRAM usage');
    expect(packagedDashboardScript).toContain('if (!m.total) return;');
    expect(packagedDashboardScript).not.toContain("startsWith('/dev')");
    expect(packagedDashboardScript).toContain('window.parent === window');
    expect(setup).toContain('systemStats: previewSystemStats()');
    expect(setup).toContain('cpuCoresDetail');
    expect(rust).toContain('struct PcStatsSnapshot');
    expect(rust).toContain('struct PcProcessSnapshot');
    expect(rust).toContain('struct PcNetworkSnapshot');
    expect(rust).toContain('struct PcGpuSnapshot');
    expect(rust).toContain('cpu_cores_detail: Vec<PcCpuCoreSnapshot>');
    expect(rust).toContain('processes_count: usize');
    expect(rust).toContain('system_stats: Option<PcStatsSnapshot>');
    expect(rust).toContain('desktop_system_stats');
    expect(rust).toContain('CREATE_NO_WINDOW');
    expect(rust).toContain('PcStatsSampler');
    expect(rust).toContain('nvidia-smi');
    expect(rust).toContain('GPU Engine(*)');
    expect(rust).toContain('GPU Adapter Memory(*)');
    expect(rust).toContain('memory_cached_bytes: u64');
    expect(rust).toContain('process.cpu_usage() / cpu_count');
    expect(rust).toContain('Get-CimInstance Win32_VideoController');
    expect(rust).toContain('MSAcpi_ThermalZoneTemperature');
    expect(rust).toContain('system_profiler');
    expect(rust).toContain('/sys/class/drm');
    expect(rust).toContain('fn collect_pc_stats() -> PcStatsSnapshot');
  });

  test('rewrites onboarding with plain-language access and non-repeating steps', () => {
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const constants = read('src/ui/desktop/modules/constants.js');
    const main = read('src/ui/desktop/main.js');

    expect(constants).toContain('Welcome');
    expect(constants).toContain('Workspace');
    expect(constants).toContain('Choose provider');
    expect(constants).toContain('Configure access');
    expect(constants).toContain('Choose model');
    expect(constants).toContain('Basic behavior');
    expect(constants).toContain('Test and start');
    expect(constants).toContain(
      "export const onboardingSteps = [\n  'Welcome',\n  'Workspace',\n  'Choose provider',\n  'Configure access',\n  'Choose model',\n  'Basic behavior',\n  'Test and start',\n];",
    );
    expect(constants).not.toContain('Runtime mode');
    expect(constants).not.toContain('Capabilities');
    expect(constants).not.toContain('Channels');
    expect(constants).not.toContain('Security posture');
    expect(constants).not.toContain('Review > Test > Pass');
    expect(constants).not.toContain('Finish and launch');
    expect(onboarding).toContain('onboarding-backdrop');
    expect(onboarding).toContain('onboarding-modal');
    expect(onboarding).toContain('What Argentum is');
    expect(onboarding).toContain('Default access: all folders/files inside workspace folder');
    expect(onboarding).toContain('renderWorkspaceStep');
    expect(onboarding).toContain('renderProviderChoiceStep');
    expect(onboarding).toContain('renderProviderCredentialStep');
    expect(onboarding).toContain('renderProviderModelStep');
    expect(onboarding).toContain('renderBehaviorStep');
    expect(onboarding).toContain('renderTestAndStartStep');
    expect(onboarding).toContain('Self-repair');
    expect(onboarding).not.toContain('function renderRuntimeStep');
    expect(onboarding).not.toContain('function renderCapabilitiesStep');
    expect(onboarding).not.toContain('function renderChannelsStep');
    expect(onboarding).not.toContain('function renderSecurityStep');
    expect(onboarding).not.toContain('renderFinishStep');
    expect(onboarding).not.toContain('Default access</span><strong>Workspace</strong>');
    expect(main).toContain('goToStep(Number(stepButton.dataset.onboardingStep))');
    expect(read('src/ui/desktop/modules/onboarding-controller.js')).toContain(
      'Finish the current setup step before jumping ahead.',
    );
    expect(main).toContain('nextStep()');
    expect(main).toContain('previousStep()');
  });

  test('supports Beginner Intermediate Expert experience levels', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const state = read('src/ui/desktop/modules/state.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');

    expect(constants).toContain("id: 'beginner'");
    expect(constants).toContain("id: 'comfortable'");
    expect(constants).toContain("id: 'expert'");
    expect(constants).toContain("label: 'Intermediate'");
    expect(constants).not.toContain("label: 'Comfortable'");
    expect(state).toContain("experienceLevel: ''");
    const controller = read('src/ui/desktop/modules/onboarding-controller.js');
    expect(controller).toContain('currentStep === 1 && !state.experienceLevel.trim()');
    expect(controller).toContain('Choose Beginner, Intermediate, or Expert before continuing.');
    expect(onboarding).toContain('type="button" class="interface-card');
    expect(onboarding).toContain('type="button" class="button primary" id="next-button"');
    expect(constants).not.toContain('AI Noob');
  });

  test('routes onboarding actions through one controller with visible validation state', () => {
    const controller = read('src/ui/desktop/modules/onboarding-controller.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');

    for (const action of [
      'selectProvider',
      'selectModel',
      'nextStep',
      'previousStep',
      'validateCurrentStep',
      'completeOnboarding',
    ]) {
      expect(controller).toContain(`function ${action}`);
    }

    expect(state).toContain("onboardingError: ''");
    expect(state).toContain('onboardingValidationErrors: []');
    expect(onboarding).toContain('renderValidationError');
    expect(onboarding).toContain('role="alert"');
    expect(css).toContain('.onboarding-validation');
    expect(main).toContain('selectProvider(providerId)');
    expect(main).toContain('selectModel(target.value)');
    expect(main).toContain('advanceOnboarding');
    expect(main).toContain('completeOnboarding(result)');
    expect(main).toContain('hydrateOnboardingProgress()');
    expect(controller).toContain('ONBOARDING_PROGRESS_STORAGE_KEY');
    expect(controller).toContain('state.onboardingStep = targetStep');
    expect(controller).toContain('state.onboardingOpen = false');
    expect(controller).toContain('state.activeSection =');
  });

  test('renders onboarding in a dedicated overlay root so setup controls stay clickable', () => {
    const html = read('src/ui/desktop/index.html');
    const main = read('src/ui/desktop/main.js');

    expect(html).toContain('id="overlay-root"');
    expect(main).toContain("const overlayRoot = document.querySelector('#overlay-root');");
    expect(main).toContain('function eventTargetElement(event)');
    expect(main).toContain('event.composedPath');
    expect(main).toContain('target?.parentElement');
    expect(main).toContain('document.elementFromPoint(event.clientX, event.clientY)');
    expect(main).toContain('const element = eventTargetElement(event);');
    expect(main).toContain('function handleActivation(event)');
    expect(main).toContain('const onboardingKeyboardActivationEvents = new Set');
    expect(main).toContain("'keyup'");
    expect(main).toContain("event.key === 'Enter'");
    expect(main).toContain("event.key === ' '");
    expect(main).toContain('onboardingKeyboardActivationEvents.has(event.type)');
    expect(main).toContain('for (const activationEvent of onboardingKeyboardActivationEvents)');
    expect(main).toContain('function addActivationListeners');
    expect(main).toContain("target.addEventListener('click', handleActivation, true);");
    expect(main).toContain('addActivationListeners(document);');
    expect(main).toContain('addActivationListeners(overlayRoot);');
    expect(main).not.toContain('lastOnboardingPointerActivation');
    expect(main).not.toContain('wasHandledByPointer');
    expect(main).not.toContain('const element = target instanceof Element ? target : null;');
    expect(main).toContain('viewRoot.innerHTML = renderModule(module);');
    expect(main).toContain('overlayRoot.innerHTML =');
    expect(main).not.toContain("document.addEventListener('click', handleClick);");
    expect(main).not.toContain("viewRoot.addEventListener('click', handleClick)");
  });

  test('uses the native Tauri dialog plugin for workspace folder selection', () => {
    const cargo = read('src/desktop/Cargo.toml');
    const rust = read('src/desktop/src/lib.rs');
    const capabilities = read('src/desktop/capabilities/default.json');
    const setup = read('src/ui/desktop/modules/setup.js');
    const utils = read('src/ui/desktop/modules/utils.js');
    const packageJson = JSON.parse(read('package.json')) as {
      dependencies?: Record<string, string>;
    };

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
      'LM Studio / local',
      'Argentum llama.cpp',
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
    expect(onboarding).toContain('function renderProviderCredentialStep');
    expect(onboarding).toContain('function renderProviderModelStep');
    expect(onboarding).toContain('function renderBehaviorStep');
    expect(onboarding).toContain('function renderTestAndStartStep');
    expect(onboarding).toContain('renderAuthMethodPicker');
    expect(onboarding).not.toContain("state.providerSetupStage === 'provider'");
    expect(onboarding).not.toContain('data-provider-setup-stage="auth"');
    expect(onboarding).not.toContain('data-provider-setup-stage="model"');
    expect(onboarding).not.toContain('id="continue-provider-model"');
    expect(onboarding).not.toContain('Continue to model');
    expect(onboarding).toContain('provider-operational-details');
    expect(onboarding).toContain('provider-key-status');
    expect(onboarding).toContain('provider.requiresKey && state.providerApiKey.trim()');
    expect(constants).toContain('highlights:');
    expect(constants).toContain('dataRegion:');
    expect(constants).toContain('dataRoute:');
    expect(constants).toContain('privacyUrl:');
    expect(constants).toContain('termsUrl:');
    expect(constants).toContain('usageNotes:');
    expect(constants).toContain('accountUsageUrl:');
    expect(constants).toContain('modelSearchUrl:');
    expect(constants).toContain('API key optional for localhost');
    expect(main).toContain('function updateProviderKeyStatus');
    expect(main).toContain('advanceOnboarding');
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
    expect(onboarding).toContain('data-provider-auth-method');
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
    expect(onboarding).toContain('Optional for LM Studio/local/custom endpoints');
    expect(onboarding).toContain('id="provider-custom-model"');
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
    expect(main).toContain('renderNotificationMenu');
    expect(main).toContain('notification-menu');
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

    expect(main).toContain('completeOnboarding(result)');
    expect(read('src/ui/desktop/modules/onboarding-controller.js')).toContain(
      "state.activeSection = 'chat'",
    );
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
    expect(chat).toContain('context-usage-ring');
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
    expect(chat).toContain('composer-inline');
    expect(chat).toContain('id="attach-file"');
    expect(chat).toContain('id="voice-input"');
    expect(chat).toContain('id="thinking-level"');
    expect(main).toContain('buildLocalReply');
    expect(main).toContain('chooseChatAttachment');
    expect(main).toContain('startVoiceInput');
    expect(main).toContain('addTerminalEntry');
    expect(main).toContain('sendChatMessage');
    expect(main).toContain('state.chatStreaming = true');
    expect(main).toContain('state.chatStreaming = false');
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
    expect(css).toContain('height: calc(100vh - 138px)');
    expect(css).toContain('grid-template-rows: auto auto minmax(0, 1fr) auto');
    expect(css).toContain('overflow-y: auto');
    expect(css).toContain('.composer-inline');
    expect(css).toContain('.markdown-body');
    expect(css).toContain('.typing-indicator');
    expect(css).toContain('.context-usage-ring');
    expect(css).toContain('.model-detail-panel');
  });

  test('matches the new concept layout with conversation canvas and right inspector', () => {
    const html = read('src/ui/desktop/index.html');
    const chat = read('src/ui/desktop/modules/chat.js');
    const shell = read('src/ui/desktop/modules/shell.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).toContain('class="breadcrumb-bar"');
    expect(html).toContain('class="view-mode-switcher"');
    expect(html).toContain('id="provider-status-pill"');
    expect(html).toContain('brand-shard');
    expect(shell).toContain('renderProviderStatusPill');
    expect(chat).toContain('conversation-column');
    expect(chat).toContain('chat-canvas');
    expect(chat).toContain('workspace-empty-state');
    expect(chat).toContain('inspector-panel');
    expect(chat).toContain('prompt-suggestion-row');
    expect(chat).toContain('conversation-tabs');
    expect(css).toContain('.chat-product-shell');
    expect(css).toContain(
      'grid-template-columns: minmax(236px, 292px) minmax(0, 1fr) minmax(260px, 320px)',
    );
    expect(css).toContain('.conversation-composer');
    expect(css).toContain('.inspector-panel');
    expect(css).toContain('overflow-x: hidden');
  });

  test('uses the target active chat visual shell without replacing chat handlers', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');

    for (const className of [
      'arg-chat-shell',
      'arg-conversation-rail',
      'arg-chat-stage',
      'arg-chat-toolbar',
      'arg-message-canvas',
      'arg-composer-shell',
      'arg-chat-inspector',
      'arg-telemetry-strip',
    ]) {
      expect(chat).toContain(className);
      expect(css).toContain(`.${className}`);
    }

    for (const hook of [
      'id="send-chat"',
      'data-send-chat-button="true"',
      'id="chat-draft"',
      'data-recent-chat',
      'data-chat-filter="all"',
      'data-conversation-menu',
      'data-regenerate-message',
      'data-copy-message',
    ]) {
      expect(chat).toContain(hook);
    }

    expect(chat).toContain('renderChatTelemetry');
    expect(chat).toContain('arg-chat-line-user');
    expect(chat).toContain('arg-chat-line-assistant');
    expect(chat).toContain('arg-context-ring');
    expect(css).toContain('.view-mode-chat .arg-chat-inspector');
    expect(css).toContain('display: none');
    expect(css).toContain('.arg-chat-line-user');
    expect(css).toContain('justify-content: flex-end');
    expect(css).toContain('.arg-chat-line-assistant');
    expect(css).toContain('justify-content: flex-start');
    expect(css).toContain('conic-gradient(var(--ring-color)');
  });

  test('renders discord-style chat lines instead of message bubbles', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');
    const messageRenderer = chat.slice(
      chat.indexOf('function renderMessageComponent'),
      chat.indexOf('function renderMessageAttachments'),
    );
    const chatLineCss = css.slice(
      css.indexOf('.arg-chat-line {'),
      css.indexOf('.arg-composer-shell'),
    );

    expect(messageRenderer).toContain('arg-chat-line');
    expect(messageRenderer).toContain('arg-chat-line-user');
    expect(messageRenderer).toContain('arg-chat-line-assistant');
    expect(messageRenderer).toContain('arg-chat-content');
    expect(messageRenderer).toContain('arg-chat-meta');
    expect(messageRenderer).not.toContain('arg-msg');
    expect(chatLineCss).toContain('background: transparent;');
    expect(chatLineCss).toContain('border: 0;');
    expect(chatLineCss).toContain('box-shadow: none;');
    expect(chatLineCss).toContain('border-radius: 0;');
    expect(chatLineCss).not.toContain('clip-path: polygon');
  });

  test('keeps typing status outside chat history and uses angular seamless chat surfaces', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');

    const transcript = chat.slice(
      chat.indexOf('<div class="chat-transcript arg-message-canvas"'),
      chat.indexOf('</div>\n        ${renderNewTransmissionBar(state)}'),
    );
    const typing = chat.slice(
      chat.indexOf('function renderTypingIndicator'),
      chat.indexOf('function renderAttachmentTray'),
    );

    expect(transcript).not.toContain('${renderTypingIndicator(state)}');
    expect(transcript).not.toContain('${renderNewTransmissionBar(state)}');
    expect(chat.indexOf('${renderTypingIndicator(state)}')).toBeGreaterThan(
      chat.indexOf('</div>\n        ${renderNewTransmissionBar(state)}'),
    );
    expect(chat).toContain('data-new-transmission="true"');
    expect(typing).toContain('arg-typing-status');
    expect(typing).not.toContain('message-row');
    expect(typing).not.toContain('arg-msg');
    expect(css).toContain('.arg-typing-status');
    expect(css).toContain('.arg-chat-stage');
    expect(css).toContain('clip-path: polygon');
    expect(css).toContain('border-radius: 0');
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
    expect(rust).toContain('fn run_llama_server_action');
    expect(rust).toContain('resolve_llama_server_path');
    expect(rust).toContain('fn configured_llama_model_launch');
    expect(rust).toContain('fn resolve_llama_gguf_model_path');
    expect(rust).toContain('LLAMA_SERVER_DEFAULT_HF_REPO');
    expect(rust).toContain('"--hf-repo"');
    expect(rust).toContain('"--hf-file"');
    expect(rust).toContain('LLAMA_SERVER_BIN');
    expect(rust).toContain('llama-server-start');
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
    expect(main).toContain('llamaServer: actionId.startsWith');
    expect(main).toContain("setLlamaServerConfig('modelSource', 'file')");
    expect(packageJson).toContain('build:desktop-sidecar');
    expect(packageJson).toContain('prepare:llama-server');
    expect(packageJson).toContain('predesktop:build');
    expect(packageJson).toContain('npm run build && npm run build:desktop-sidecar');
    expect(workflow).toContain('Build desktop CLI sidecar');
  });

  test('cleans gateway terminal output and keeps gateway layout inside the viewport', () => {
    const rust = read('src/desktop/src/lib.rs');
    const cli = read('src/cli.ts');
    const cliLaunch = read('src/core/cli-launch.ts');
    const sections = read('src/ui/desktop/modules/sections.js');
    const css = read('src/ui/desktop/styles.css');

    expect(rust).toContain('fn clean_terminal_output');
    expect(rust).toContain('strip_argentum_banner');
    expect(rust).toContain('ARGENTUM_NO_BANNER');
    expect(rust).toContain('ARGENTUM_PLAIN_OUTPUT');
    expect(rust).toContain('read_preview(&gateway_log_path, 160)');
    expect(cli).toContain('function shouldPrintBanner');
    expect(cli).toContain('process.env.ARGENTUM_NO_BANNER');
    expect(cliLaunch).toContain("childEnv.ARGENTUM_NO_BANNER = '1'");
    expect(cliLaunch).toContain("childEnv.ARGENTUM_PLAIN_OUTPUT = '1'");
    expect(sections).toContain('gateway-terminal-shell');
    expect(sections).toContain('gateway-status-grid');
    expect(read('src/ui/desktop/main.js')).toContain("addTerminalEntry(command, 'Running...'");
    expect(rust).toContain('Use Gateway Status for PID, health URL, and log path');
    expect(rust).toContain('"State: {}"');
    expect(rust).toContain('format!("Log: {}", log_path.display())');
    expect(css).toContain('overflow-x: hidden');
    expect(css).toContain('.gateway-terminal-shell');
    expect(css).toContain('white-space: pre-wrap');
    expect(css).toContain('overflow-wrap: anywhere');
    expect(css).not.toContain('min-width: 960px');
  });

  test('renders a compact chat context ring near send and supports font preferences', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const state = read('src/ui/desktop/modules/state.js');
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const utils = read('src/ui/desktop/modules/utils.js');
    const css = read('src/ui/desktop/styles.css');

    expect(constants).toContain('fontOptions');
    expect(constants).toContain('JetBrains Mono');
    expect(constants).toContain('Cascadia Code');
    expect(state).toContain('uiFontFamily:');
    expect(state).toContain('codeFontFamily:');
    expect(state).toContain('hydrateUiPreferences');
    expect(state).toContain('setUiPreference');
    expect(main).toContain('applyUiPreferences');
    expect(main).toContain('hydrateUiPreferences');
    expect(chat).toContain('renderContextRing');
    expect(chat).toContain('context-usage-ring');
    expect(chat).toContain('contextPercent');
    expect(chat).toContain('composer-inline');
    expect(sections).toContain('settings-ui-font');
    expect(sections).toContain('settings-code-font');
    expect(utils).toContain('contextTokenLimit');
    expect(utils).toContain('contextUsagePercent');
    expect(css).toContain('.context-usage-ring');
    expect(css).toContain('.context-usage-ring.hot');
    expect(css).toContain('var(--font-ui)');
    expect(css).toContain('var(--font-mono)');
  });

  test('lets Settings replace provider API keys and persists runtime/font/thinking changes', () => {
    const setup = read('src/ui/desktop/modules/setup.js');
    const main = read('src/ui/desktop/main.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const state = read('src/ui/desktop/modules/state.js');

    expect(setup).toContain('persistRuntimeSettings');
    expect(sections).toContain('id="settings-provider-api-key"');
    expect(sections).toContain('Paste a new key, or leave blank to keep saved key.');
    expect(sections).toContain('id="save-settings"');
    expect(sections).toContain('settingsSections');
    expect(sections).toContain("['advanced', 'Advanced']");
    expect(main).toContain('async function saveSettingsFromInputs');
    expect(main).toContain("target.id === 'settings-provider-api-key'");
    expect(main).toContain("await persistRuntimeSettings('settings'");
    expect(main).toContain("state.providerApiKey = ''");
    expect(main).toContain('applyRuntimeMode');
    expect(main).toContain("await persistRuntimeSettings('thinking-level'");
    expect(main).toContain('persistRuntimeSettings(reason');
    expect(state).toContain('providerCatalogTab:');
  });

  test('checks MiniMax Token Plan usage and surfaces usage snapshots', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const setup = read('src/ui/desktop/modules/setup.js');
    const chat = read('src/ui/desktop/modules/chat.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const rust = read('src/desktop/src/lib.rs');

    expect(constants).toContain('MiniMax-M2.7');
    expect(constants).toContain('M2.7 requests reset on a rolling 5-hour window');
    expect(constants).toContain('providerCatalogTabs');
    expect(constants).toContain("id: 'testing'");
    expect(constants).toContain('M2.7 requests reset on a rolling 5-hour window');
    expect(setup).toContain('result.usage');
    expect(chat).toContain('formatUsageLine');
    expect(chat).toContain('usageSnapshot.summary');
    expect(sections).toContain('usage?.summary');
    expect(sections).toContain('requestResetCadence');
    expect(sections).toContain('resetCadence');
    expect(rust).toContain('MINIMAX_TOKEN_PLAN_REMAINS_URL');
    expect(rust).toContain('https://www.minimax.io/v1/token_plan/remains');
    expect(rust).toContain('async fn minimax_token_plan_usage');
    expect(rust).toContain('fn minimax_usage_snapshot');
    expect(rust).toContain('MiniMax Token Plan');
    expect(rust).toContain('M2.7 best practice');
    expect(rust).toContain('Provider usage visible to agent');
    expect(rust).toContain('reset_cadence');
    expect(rust).toContain('usage: Option<UsageLimitSnapshot>');
    expect(rust).toContain('usage = snapshot.or(usage)');
  });

  test('tracks provider usage windows and central redacted app logs', () => {
    const rust = read('src/desktop/src/lib.rs');
    const setup = read('src/ui/desktop/modules/setup.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const state = read('src/ui/desktop/modules/state.js');

    expect(rust).toContain('struct UsageQuotaWindow');
    expect(rust).toContain('modality_quotas');
    expect(rust).toContain('weekly_request_budget');
    expect(rust).toContain('five_hour_request_limit');
    expect(rust).toContain('fn append_app_log');
    expect(rust).toContain('fn app_log_path');
    expect(rust).toContain('provider.test');
    expect(rust).toContain('chat.send');
    expect(rust).toContain('gateway.action');
    expect(setup).toContain('recordUiEvent');
    expect(setup).toContain('providerUsageLine');
    expect(sections).toContain('usage-window-grid');
    expect(sections).toContain('Activity event log');
    expect(state).toContain('appLogEntries');
  });

  test('calculates chat context from the real runtime payload including system prompt', () => {
    const utils = read('src/ui/desktop/modules/utils.js');
    const chat = read('src/ui/desktop/modules/chat.js');
    const rust = read('src/desktop/src/lib.rs');

    expect(utils).toContain('buildApprovedRuntimeContextText');
    expect(utils).toContain('System prompt:');
    expect(utils).toContain('estimateRuntimeContextTokens');
    expect(chat).toContain('estimateRuntimeContextTokens(state, state.draftMessage)');
    expect(chat).toContain('state.usageSnapshot?.contextTokens');
    expect(rust).toContain('usage_from_response_body');
    expect(rust).toContain('prompt_tokens');
    expect(rust).toContain('input_tokens');
    expect(rust).toContain('Provider-reported input/context tokens');
    expect(rust).toContain('Last request context tokens');
  });

  test('keeps only the persistent rail logo and moves context out of the inspector', () => {
    const html = read('src/ui/desktop/index.html');
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).toContain('brand-wordmark');
    expect(html).toContain('brand-shard');
    expect(html).not.toContain('<div class="brand">\n          <img');
    expect(chat).not.toContain('workspace-empty-logo');
    expect(chat).not.toContain('<img class="workspace-empty-logo" src="./assets/argentum.png"');
    expect(chat).not.toContain('Context usage');
    expect(chat).not.toContain('context-gauge');
    expect(chat).toContain('Session ID');
    expect(chat).toContain('state.activeChatId');
    expect(css).toContain('.brand-shard');
    expect(css).toContain('.brand-wordmark');
    expect(css).toContain('.brand-shard');
  });

  test('sends compacted full chat history and keeps the composer on one line', () => {
    const setup = read('src/ui/desktop/modules/setup.js');
    const utils = read('src/ui/desktop/modules/utils.js');
    const chat = read('src/ui/desktop/modules/chat.js');
    const rust = read('src/desktop/src/lib.rs');
    const css = read('src/ui/desktop/styles.css');

    expect(utils).toContain('compactConversationForProvider');
    expect(utils).toContain('conversationSummary');
    expect(setup).toContain('conversationHistory');
    expect(setup).toMatch(/compactConversationForProvider\(\s*state\.chatBlocks,\s*message,?\s*\)/);
    expect(rust).toContain('conversation_history');
    expect(rust).toContain('conversation_summary');
    expect(rust).toContain('fn openai_chat_messages_from_history');
    expect(rust).toContain('Compacted earlier conversation');
    expect(chat).toContain('composer-inline');
    expect(css).toContain('.composer-inline');
    expect(css).toContain('grid-template-columns: auto auto auto minmax(180px, 1fr) auto auto');
  });

  test('keeps prompt examples above the action cards and prevents top-level chat overflow', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const css = read('src/ui/desktop/styles.css');
    const emptyState = chat.slice(
      chat.indexOf('function renderWorkspaceEmptyState'),
      chat.indexOf('function renderPromptSuggestions'),
    );
    const composer = chat.slice(
      chat.indexOf('<div class="conversation-composer composer">'),
      chat.indexOf('${renderAttachmentTray(state)}'),
    );

    expect(emptyState).toContain('${renderPromptSuggestions()}');
    expect(emptyState.indexOf('${renderPromptSuggestions()}')).toBeLessThan(
      emptyState.indexOf('<div class="workspace-action-grid">'),
    );
    expect(composer).not.toContain('${renderPromptSuggestions()}');
    expect(composer.indexOf('<div class="conversation-composer composer">')).toBeLessThan(
      composer.indexOf('<div class="composer-inline">'),
    );
    expect(main).toContain(
      'viewRoot.className = `view-root view-root-${section.id} view-mode-${state.viewMode}`',
    );
    expect(css).toContain('.view-root-chat');
    expect(css).toContain('overflow: hidden');
    expect(css).toContain('.chat-product-shell');
    expect(css).toContain('height: 100%');
    expect(css).toContain('max-width: 100%');
  });

  test('imports Telegram channel sessions into desktop chat history', () => {
    const state = read('src/ui/desktop/modules/state.js');
    const setup = read('src/ui/desktop/modules/setup.js');
    const rust = read('src/desktop/src/lib.rs');
    const index = read('src/index.ts');

    expect(state).toContain('mergeChannelChatSessions');
    expect(state).toContain("raw.channel === 'telegram'");
    expect(setup).toContain('mergeChannelChatSessions(result.channelSessions');
    expect(rust).toContain('channel_sessions');
    expect(rust).toContain('read_channel_sessions');
    expect(index).toContain('telegramSessionId');
    expect(index).toContain('appendChannelSessionMessage');
    expect(index).toContain('conversationHistoryForChannelSession');
    expect(index).toContain('this.agent.handleMessage(text, history)');
  });

  test('declutters non-chat tabs with compact product primitives', () => {
    const sections = read('src/ui/desktop/modules/sections.js');
    const css = read('src/ui/desktop/styles.css');

    expect(sections).toContain('function renderCompactPageHeader');
    expect(sections).toContain('function renderProductTabShell');
    expect(sections).toContain("renderProductTabShell(\n          'gateway'");
    expect(sections).toContain("renderProductTabShell(\n      'security'");
    expect(sections).toContain("renderProductTabShell(\n        'settings'");
    expect(sections).toContain("renderProductTabShell(\n        'diagnostics'");
    expect(sections).toContain("renderProductTabShell(\n      'logs'");
    expect(sections).toContain('compact-toolbar');
    expect(sections).toContain('compact-status-strip');
    expect(sections).toContain('compact-detail');
    expect(sections).toContain('settings-workbench');
    expect(sections).toContain('security-map-grid');
    expect(sections).toContain('diagnostics-matrix');
    expect(sections).toContain('log-console-grid');
    expect(sections).not.toContain('product-hero');
    expect(sections).not.toContain('section-command-dock');

    expect(css).toContain('.product-tab-shell');
    expect(css).toContain('.compact-page-header');
    expect(css).toContain('.compact-status-strip');
    expect(css).toContain('.compact-toolbar');
    expect(css).toContain('.compact-detail');
    expect(css).toContain('.glass-panel');
    expect(css).toContain('.settings-workbench');
    expect(css).toContain('.security-map-grid');
    expect(css).toContain('.diagnostics-matrix');
    expect(css).toContain('.log-console-grid');
    expect(css).toContain('.product-tab-shell .panel');
    expect(css).toContain('--text-base: 13px');
    expect(css).toContain('grid-template-columns: 248px minmax(0, 1fr)');
    expect(css).not.toContain('.product-hero');
    expect(css).not.toContain('.section-command-dock');
  });

  test('keeps gateway terminal chronological and scrolls terminal panels to the latest output', () => {
    const sections = read('src/ui/desktop/modules/sections.js');
    const state = read('src/ui/desktop/modules/state.js');
    const main = read('src/ui/desktop/main.js');
    const css = read('src/ui/desktop/styles.css');

    expect(sections).toContain('terminalEntriesForDisplay');
    expect(state).toContain('.reverse()');
    expect(main).toContain('function scrollTerminalPanels');
    expect(main).toContain('scrollTop = panel.scrollHeight');
    expect(css).toContain('align-content: start');
    expect(css).toContain('.terminal-body pre');
  });

  test('supports deleting chats with confirmation', () => {
    const state = read('src/ui/desktop/modules/state.js');
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const css = read('src/ui/desktop/styles.css');

    expect(state).toContain('pendingDeleteChatId');
    expect(state).toContain('requestDeleteChatSession');
    expect(state).toContain('confirmDeleteChatSession');
    expect(state).toContain('cancelDeleteChatSession');
    expect(chat).toContain('data-delete-chat');
    expect(chat).toContain('data-confirm-delete-chat');
    expect(chat).toContain('data-cancel-delete-chat');
    expect(main).toContain('requestDeleteChatSession');
    expect(main).toContain('confirmDeleteChatSession');
    expect(css).toContain('.chat-delete-confirm');
  });

  test('wires view modes, conversation filters, menus, workspace menu, and help panel', () => {
    const html = read('src/ui/desktop/index.html');
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).toContain('data-view-mode="chat"');
    expect(html).not.toContain('data-view-mode="split"');
    expect(html).not.toContain('data-view-mode="full"');
    expect(html).toContain('id="help-button"');
    expect(html).toContain('id="workspace-button"');
    expect(chat).not.toContain('data-chat-filter="recent"');
    expect(chat).toContain('data-chat-filter="pinned"');
    expect(chat).toContain('data-chat-filter="all"');
    expect(chat).toContain('filteredChatSessions');
    expect(chat).toContain('data-conversation-menu');
    expect(chat).toContain('data-pin-chat');
    expect(chat).toContain('data-clear-chat');
    expect(chat).toContain('data-rename-chat');
    expect(chat).toContain('data-toggle-chat-panel="inspector"');
    expect(chat).toContain('data-toggle-chat-panel="conversations"');
    expect(state).toContain('viewMode:');
    expect(state).toContain('chatFilter:');
    expect(state).toContain('lastMessageAt');
    expect(state).toContain('lastOpenedAt');
    expect(state).toContain('unreadCount');
    expect(state).toContain('setViewMode');
    expect(state).toContain('setChatFilter');
    expect(state).toContain('toggleChatPanel');
    expect(state).toContain('toggleChatPinned');
    expect(state).toContain('toggleWorkspaceMenu');
    expect(state).toContain('toggleHelp');
    expect(main).toContain('renderHelpPanel');
    expect(main).toContain('renderWorkspacePanel');
    expect(main).toContain('regenerateAssistantResponse');
    expect(main).toContain('chat.regenerate_state_reset');
    expect(css).toContain('.chat-product-shell.view-mode-chat');
    expect(css).toContain('.chat-product-shell.arg-chat-shell.view-mode-chat');
    expect(css).toContain('.chat-product-shell.arg-chat-shell.conversation-collapsed');
    expect(css).toContain('.chat-product-shell.arg-chat-shell.inspector-collapsed');
    expect(css).toContain('.conversation-action-menu');
    expect(css).toContain('.floating-panel');
  });

  test('keeps ChatGPT/OpenAI, MiniMax, LM Studio, and llama.cpp stable while other providers are testing', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const main = read('src/ui/desktop/main.js');

    expect(constants).toContain('providerCatalogTabs');
    expect(constants).toContain("access: 'stable'");
    expect(constants).toContain("access: 'testing'");
    expect(constants).toContain("label: 'ChatGPT / OpenAI'");
    expect(constants).toContain("id: 'minimax'");
    expect(constants).toContain("id: 'local'");
    expect(constants).toContain("label: 'LM Studio / local'");
    expect(constants).toContain("id: 'llama-cpp'");
    expect(constants).toContain("label: 'Argentum llama.cpp'");
    expect(constants).toContain("defaultBaseUrl: 'http://127.0.0.1:8080/v1'");
    expect(constants).toContain("modelSearchUrl: 'https://huggingface.co/models?search=gguf'");
    expect(constants).toContain('llamaDownloadPresets');
    expect(constants).toContain("label: 'Qwen3 0.6B'");
    expect(constants).toContain("label: 'Qwen3.5 0.8B Instruct FT'");
    expect(constants).toContain("label: 'Gemma 3 1B IT'");
    expect(constants).toContain("label: 'LFM2.5 1.2B Instruct'");
    expect(onboarding).toContain('provider-access-tabs');
    expect(onboarding).toContain('data-provider-catalog-tab');
    expect(onboarding).toContain('visibleProviders');
    expect(sections).toContain('data-provider-access');
    expect(sections).toContain('Testing access');
    expect(sections).toContain('settings-llama-model-source');
    expect(sections).toContain('settings-llama-model-preset');
    expect(sections).toContain('settings-llama-hf-repo');
    expect(sections).toContain('settings-llama-hf-file');
    expect(main).toContain(
      'state.providerCatalogTab = providerCatalogButton.dataset.providerCatalogTab',
    );
  });

  test('uses cleaner inline selection cards and keeps recent chats history-sorted', () => {
    const sections = read('src/ui/desktop/modules/sections.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');

    expect(sections).toContain('check-card-head');
    expect(onboarding).toContain('check-card-head');
    expect(css).toContain('.check-card-head');
    expect(css).toContain('grid-template-columns: auto minmax(0, 1fr)');
    expect(state).toContain('touchActiveChatSession');
    expect(state).toContain('state.chatSessions = sortChatSessions');
    expect(state).toContain(
      'if (Boolean(a.pinned) !== Boolean(b.pinned)) return a.pinned ? -1 : 1',
    );
    expect(state).toContain(
      'return (b.lastMessageAt || b.updatedAt || 0) - (a.lastMessageAt || a.updatedAt || 0)',
    );
    expect(state).not.toContain('if (a.id === state.activeChatId) return -1');
  });

  test('separates provider reasoning tags and exposes reasoning output preferences', () => {
    const state = read('src/ui/desktop/modules/state.js');
    const chat = read('src/ui/desktop/modules/chat.js');
    const sections = read('src/ui/desktop/modules/sections.js');
    const setup = read('src/ui/desktop/modules/setup.js');
    const main = read('src/ui/desktop/main.js');
    const css = read('src/ui/desktop/styles.css');
    const rust = read('src/desktop/src/lib.rs');

    expect(state).toContain("import { parseReasoningBlocks } from './reasoning-parser.js'");
    expect(existsSync('src/ui/desktop/modules/reasoning-parser.js')).toBe(true);
    expect(state).toContain('rawBody');
    expect(state).toContain('splitReasoningFromMessage');
    expect(state).toContain('redactPrivateText');
    expect(state).toContain('showThinkingInChat: false');
    expect(state).toContain('showThinkingInTelegram: false');
    expect(chat).toContain('renderReasoningPanel');
    expect(chat).toContain('reasoning-panel');
    expect(sections).toContain('settings-show-thinking-chat');
    expect(sections).toContain('settings-show-thinking-telegram');
    expect(setup).toContain('showThinkingInChat');
    expect(setup).toContain('showThinkingInTelegram');
    expect(main).toContain('settings-show-thinking-chat');
    expect(main).toContain('settings-show-thinking-telegram');
    expect(css).toContain('.reasoning-panel');
    expect(rust).toContain('show_thinking_in_chat');
    expect(rust).toContain('show_thinking_in_telegram');
    expect(rust).toContain('reasoningOutput');
    expect(rust).toContain('Privacy boundary');
  });

  test('deeply redesigns chat primitives, icons, attachments, streaming, and context controls', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const icons = read('src/ui/desktop/modules/icons.js');
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const setup = read('src/ui/desktop/modules/setup.js');
    const state = read('src/ui/desktop/modules/state.js');
    const utils = read('src/ui/desktop/modules/utils.js');
    const css = read('src/ui/desktop/styles.css');
    const rust = read('src/desktop/src/lib.rs');

    expect(constants).toContain('Monaspace Krypton');
    expect(css).toContain('@font-face');
    expect(css).toContain('Monaspace Krypton');
    expect(state).toContain('activeAssistantMessageId');
    expect(state).toContain('chatAbortRequested');

    for (const name of ['plus', 'x', 'trash', 'send', 'stop', 'copy', 'image', 'file', 'refresh']) {
      expect(icons).toMatch(new RegExp(`['"]?${name}['"]?:`));
    }

    expect(chat).toContain('function renderMessageComponent');
    expect(chat).toContain('function renderComposer');
    expect(chat).toContain('function renderAttachmentPreview');
    expect(chat).toContain('function renderMessageActions');
    expect(chat).toContain('Context ');
    expect(chat).toContain('message system');
    expect(chat).toContain('data-copy-message');
    expect(chat).toContain('data-regenerate-message');
    expect(chat).toContain('data-retry-message');
    expect(chat).not.toContain('+ New chat');
    expect(chat).not.toContain('>...</button>');

    expect(main).toContain('function streamAssistantMessage');
    expect(main).toContain('state.chatAbortRequested');
    expect(main).toContain('data-stop-generation');
    expect(main).toContain('attachmentToPayload');
    expect(main).not.toContain('Attached files:\\n');
    expect(setup).toContain('sendChatMessage(message, attachments = [])');
    expect(setup).toContain('attachments,');
    expect(utils).toContain('function inferAttachmentKind');
    expect(utils).toContain('function filePreviewUrl');

    expect(rust).toContain('struct ChatAttachmentRequest');
    expect(rust).toContain('attachments: Vec<ChatAttachmentRequest>');
    expect(rust).toContain('data:image/');
    expect(rust).toContain('validate_chat_attachments');
    expect(rust).toContain('image_url');
    expect(rust).toContain('Provider usage unavailable');
  });

  test('keeps fresh chat composer send action enabled after typing', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');

    expect(chat).toContain('data-send-chat-button');
    expect(chat).toContain('id="send-chat-status"');
    expect(main).toContain('function syncComposerSendState');
    expect(main).toContain("if (target.id === 'chat-draft') {");
    expect(main).toContain('syncComposerSendState();');
    expect(main).toContain("recordUiEvent('chat.send_attempt'");
    expect(main).toContain("recordUiEvent('chat.send_failed'");
    expect(main).toContain("recordUiEvent('chat.send_success'");
  });

  test('removes diagonal placeholder artifact and old product hero structure', () => {
    const sections = read('src/ui/desktop/modules/sections.js');
    const css = read('src/ui/desktop/styles.css');

    expect(sections).not.toContain('product-hero-signal');
    expect(css).not.toContain('.product-hero-signal');
    expect(css).not.toContain('linear-gradient(135deg, transparent 42%');
    expect(css).not.toContain('.product-hero-status');
    expect(css).toContain('.compact-page-header');
  });

  test('renders a distinct fresh chat state and calmer onboarding wizard', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const css = read('src/ui/desktop/styles.css');

    expect(chat).toContain('fresh-conversation-hero');
    expect(chat).toContain('fresh-action-grid');
    expect(chat).toContain('Fresh conversation');
    expect(chat).toContain('Start with a prompt, inspect context, or open a runtime tool.');
    expect(onboarding).toContain('onboarding-wizard');
    expect(onboarding).toContain('onboarding-step-layout');
    expect(onboarding).toContain('onboarding-step-list');
    expect(onboarding).not.toContain('setup-mini-map');
    expect(onboarding).not.toContain('copy-block compact-copy');
    expect(css).toContain('.fresh-conversation-hero');
    expect(css).toContain('.onboarding-wizard');
  });

  test('keeps onboarding stable while choices change and moves detail into hover affordances', () => {
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const state = read('src/ui/desktop/modules/state.js');
    const main = read('src/ui/desktop/main.js');
    const css = read('src/ui/desktop/styles.css');

    expect(css).toContain('height: min(720px, calc(100vh - 56px));');
    expect(css).toContain('.onboarding-step-panel');
    expect(css).toContain('overflow-y: auto;');
    expect(css).toContain('scrollbar-gutter: stable;');
    expect(onboarding).toContain('id="onboarding-provider-select"');
    expect(onboarding).toContain('data-disclosure-id="permissions"');
    expect(onboarding).toContain('<summary>Basic permissions</summary>');
    expect(onboarding).toContain('<summary>Channels</summary>');
    expect(onboarding).toContain('Advanced provider details');
    expect(onboarding).toContain('Selected channel settings');
    expect(onboarding).toContain('compact-choice');
    expect(onboarding).toContain('compact-panel');
    expect(onboarding).toContain('inline-disclosure');
    expect(onboarding).toContain('data-tooltip');
    expect(onboarding).toContain('capability-chip-row');
    expect(state).toContain('onboardingOpenDisclosures');
    expect(state).toContain('setOnboardingDisclosure');
    expect(main).toMatch(/document\.addEventListener\(\s*'toggle'/);
    expect(main).toContain("target.id === 'onboarding-provider-select'");
  });

  test('keeps chat scroll stable and exposes manual context compaction', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const utils = read('src/ui/desktop/modules/utils.js');

    expect(main).toContain('function captureChatTranscriptScroll');
    expect(main).toContain('function restoreChatTranscriptScroll');
    expect(main).toContain("state.chatScrollIntent = 'bottom'");
    expect(main).toContain('data-compact-context');
    expect(state).toContain('compactActiveChatSession');
    expect(chat).toContain('contextTokenLimit');
    expect(utils).toContain('block.rawBody || block.body');
  });

  test('adds quick top-right panels and modern notification history', () => {
    const main = read('src/ui/desktop/main.js');
    const shell = read('src/ui/desktop/modules/shell.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');

    expect(main).toContain('renderQuickSecurityPanel');
    expect(main).toContain('renderQuickSettingsPanel');
    expect(main).toContain('renderNotificationMenu');
    expect(main).toContain('toggleQuickSecurityMenu');
    expect(main).toContain('toggleQuickSettingsMenu');
    expect(state).toContain('quickSecurityMenuOpen');
    expect(state).toContain('quickSettingsMenuOpen');
    expect(shell).not.toContain('state.notificationsMenuOpen');
    expect(css).toContain('.notification-history-item.success');
    expect(css).toContain('.quick-choice.active');
  });

  test('exposes secure workspace file and localhost fetch tools to compatible providers', () => {
    const rust = read('src/desktop/src/lib.rs');
    const utils = read('src/ui/desktop/modules/utils.js');

    for (const tool of [
      'argentum_read_workspace_file',
      'argentum_write_workspace_file',
      'argentum_http_fetch',
    ]) {
      expect(rust).toContain(tool);
    }

    expect(rust).toContain('resolve_workspace_tool_path');
    expect(rust).toContain('HTTP fetch is limited to localhost or loopback URLs');
    expect(rust).toContain('config.api == "openai"');
    expect(rust).toContain('tool_arguments');
    expect(utils).toContain(
      'approved model tools may read/write files inside the selected workspace',
    );
  });

  test('streams Telegram replies into channel sessions without leaking reasoning by default', () => {
    const index = read('src/index.ts');
    const telegramChannel = read('src/channels/telegram.ts');
    const rust = read('src/desktop/src/lib.rs');
    const sections = read('src/ui/desktop/modules/sections.js');

    expect(index).toContain('formatTelegramAgentResponse');
    expect(index).toContain('splitAgentReasoning');
    expect(index).toContain('streamTelegramReply');
    expect(index).toContain('ctx.api.editMessageText');
    expect(index).toContain('sendReasoning');
    expect(index).toContain("if (!text || text.startsWith('/')) return");
    expect(index).toContain('ensureChannelSession');
    expect(index).toContain('writeTelegramDiagnostics');
    expect(index).toContain('lastResponseStatus');
    expect(index).toContain('appendChannelSessionMessage(sessionId,');
    expect(index).toContain('conversationHistoryForChannelSession(sessionId)');
    expect(index).not.toContain('await ctx.reply(response);');
    expect(index).not.toContain('🤖 Welcome to Argentum');
    expect(rust).toContain('telegram_diagnostics');
    expect(rust).toContain('telegram-status.json');
    expect(rust).toContain('telegram-status');
    expect(sections).toContain('Test Telegram status');
    expect(sections).toContain('state.desktopState?.telegramDiagnostics');

    expect(telegramChannel).toContain('streamAgentResponse');
    expect(telegramChannel).toContain('ctx.api.editMessageText');
    expect(telegramChannel).toContain('this.config.sendReasoning');
  });

  test('does not invent MiniMax usage and exposes real counters or unavailable states', () => {
    const rust = read('src/desktop/src/lib.rs');
    const sections = read('src/ui/desktop/modules/sections.js');
    const setup = read('src/ui/desktop/modules/setup.js');
    const chat = read('src/ui/desktop/modules/chat.js');

    expect(rust).not.toContain('MiniMax Token Plan usage checked.');
    expect(rust).not.toContain(
      'Optional local weekly budget overlay can be configured in Settings.',
    );
    expect(rust).toContain('Provider usage unavailable');
    expect(rust).toContain('actual_usage_summary');
    expect(sections).not.toContain('Weekly local budget');
    expect(setup).toContain('Usage unavailable from provider');
    expect(chat).toContain('Usage unavailable from provider');
  });

  test('writes Telegram runtime config and passes workspace secrets to the gateway child', () => {
    const rust = read('src/desktop/src/lib.rs');
    const cli = read('src/cli.ts');
    const cliLaunch = read('src/core/cli-launch.ts');

    expect(rust).toContain('fn split_telegram_allowlist');
    expect(rust).toContain('allowedUsers:');
    expect(rust).toContain('allowedChats:');
    expect(rust).toContain('sendReasoning:');
    expect(rust).toContain('allowAll:');
    expect(rust).not.toContain('allowlist: {telegram_allowlist}');
    expect(cli).toContain('secrets.env');
    expect(cliLaunch).toContain('extraEnv?: NodeJS.ProcessEnv');
    expect(cliLaunch).toContain('Object.assign(childEnv, extraEnv)');
    expect(read('src/channels/telegram.ts')).toContain('formatAgentResponse');
    expect(read('src/features/telegram/index.ts')).toContain('formatOutboundText');
    expect(cli).toContain('function loadWorkspaceEnv');
    expect(cli).toMatch(
      /resolveGatewayChildEnvironment\(\s*process\.env,\s*workDir,\s*loadWorkspaceEnv\(workDir\),?\s*\)/,
    );
  });

  test('gateway start refuses missing config and verifies the spawned process stays alive', () => {
    const cli = read('src/cli.ts');
    const rust = read('src/desktop/src/lib.rs');

    expect(cli).toContain('Gateway cannot start because config/default.yaml is missing');
    expect(cli).toContain('verifySpawnedGateway');
    expect(cli).toContain('process.exitCode = 1');
    expect(rust).not.toContain('or_else(|| parse_gateway_pid(&start_output))');
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
    expect(setup).toContain('showThinkingInChat');
    expect(setup).toContain('showThinkingInTelegram');
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
    expect(rust).toContain('"system-dashboard"');
    expect(rust).toContain('thinking_level: String');
    expect(rust).toContain('show_thinking_in_chat: bool');
    expect(rust).toContain('show_thinking_in_telegram: bool');
    expect(rust).toContain('security_profile: String');
    expect(rust).toContain('selected_channels: Vec<String>');
    expect(rust).toContain('fn build_system_prompt');
    expect(rust).toContain('fn build_runtime_context');
    expect(rust).toContain('CORE_CONTEXT_FILE_NAME');
    expect(rust).toContain('fn default_core_template');
    expect(rust).toContain('fn ensure_core_file');
    expect(rust).toContain('fn read_core_context');
    expect(rust).toContain('CORE update policy');
    expect(rust).toContain('fn argentum_tool_definitions');
    expect(rust).toContain('fn execute_argentum_tool');
    expect(rust).toContain('tool_calls');
    expect(rust).toContain('reasoning_effort');
    expect(rust).toContain('Provider keys are added by the desktop credential flow');
    expect(rust).toContain('fn render_config');
    expect(rust).toContain('workspaceRoot');
    expect(rust).toContain('account_usage_status');
    expect(rust).toContain('MiniMax account page browser profile');
  });

  test('documents current support matrix and stable provider state', () => {
    const packageJson = JSON.parse(read('package.json')) as { version: string };
    const readme = read('README.md');
    const release = read(`docs/releases/v${packageJson.version}.md`);

    expect(readme).toContain(`v${packageJson.version}`);
    expect(release).toContain(`v${packageJson.version}`);

    for (const document of [readme, release]) {
      expect(document).toContain('Stable Providers');
      expect(document).toContain('ChatGPT');
      expect(document).toContain('MiniMax');
      expect(document).toContain('Testing Providers');
      expect(document).toContain('Supported OS');
      expect(document).toContain('Hardware Requirements');
      expect(document).toContain('Windows 10/11 x64');
      expect(document).toContain('macOS 10.15+');
      expect(document).toContain('Ubuntu 22.04');
    }
  });

  test('documents multi-workspace/agent architecture and app-awareness boundary', () => {
    const workspaces = read('docs/WORKSPACES_AND_AGENTS.md');
    const knowledge = read('docs/ARGENTUM_APP_KNOWLEDGE.md');
    const utils = read('src/ui/desktop/modules/utils.js');
    const rust = read('src/desktop/src/lib.rs');

    expect(workspaces).toContain('Workspace');
    expect(workspaces).toContain('Session');
    expect(workspaces).toContain('Agent');
    expect(workspaces).toContain('Planned Multi-Workspace Flow');
    expect(knowledge).toContain('Current Desktop Surfaces');
    expect(knowledge).toContain('What The Assistant May Know When Approved');
    expect(knowledge).toContain('Privacy Boundary');
    expect(utils).toContain('Argentum app knowledge');
    expect(utils).toContain('Current page:');
    expect(utils).toContain('Active session:');
    expect(utils).toContain('estimateCachedTextTokens');
    expect(utils).toContain('defaultCoreContextText');
    expect(utils).toContain('Provider usage:');
    expect(rust).toContain('Available app actions in the desktop MVP');
  });
});
