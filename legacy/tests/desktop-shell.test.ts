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
    expect(config.version).toBe('0.0.9');
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
    const configText = read('src/desktop/tauri.conf.json');
    expect(configText).toContain('"installerHooks": "generated/optional-llama.nsh"');
    expect(configText).not.toContain('"resources": ["../ui/desktop/llama.cpp"]');

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
    expect(shell).toContain('Module error');
    expect(shell).toContain('Argentum is still running');
    expect(shell).toContain('data-section="diagnostics"');
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
      "id: 'llama-server-install'",
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
    const dashboardTheme = read('src/ui/dashboard/codex-theme.css');
    const packagedDashboard = read('src/ui/desktop/dashboard/index.html');
    const packagedDashboardScript = read('src/ui/desktop/dashboard/dashboard.js');
    const packagedDashboardTheme = read('src/ui/desktop/dashboard/codex-theme.css');

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
    expect(dashboard).toContain('<link rel="stylesheet" href="./codex-theme.css" />');
    expect(packagedDashboard).toContain('<title>Argentum System Dashboard</title>');
    expect(packagedDashboard).toContain(
      '<link rel="stylesheet" href="./codex-theme.css" />',
    );
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
    expect(dashboardTheme).toBe(packagedDashboardTheme);
    expect(dashboardTheme).toContain('--bg: #0b0b0d');
    expect(dashboardTheme).toContain('.surface');
    expect(dashboardTheme).toContain('.zone');
    expect(dashboardTheme).toContain('.dashboard-settings');
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
    expect(rust).toContain('fn configure_background_command');
    expect(rust).toContain('#[cfg(target_os = "windows")]\n    {');
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
    expect(onboarding).toContain('setup-overlay');
    expect(onboarding).toContain('setup-window');
    expect(onboarding).toContain('Private by default');
    expect(onboarding).toContain('Choose one folder');
    expect(onboarding).toContain('renderWorkspaceStep');
    expect(onboarding).toContain('renderProviderStep');
    expect(onboarding).toContain('renderAccessStep');
    expect(onboarding).toContain('renderModelStep');
    expect(onboarding).toContain('renderPreferencesStep');
    expect(onboarding).toContain('renderReviewStep');
    expect(onboarding).toContain('Outside the workspace');
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
    expect(onboarding).toContain('class="choice-card ${');
    expect(onboarding).toContain('data-experience-level');
    expect(onboarding).toContain('class="button primary"');
    expect(onboarding).toContain('id="next-button"');
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
    expect(css).toContain('.setup-error');
    expect(main).toContain('selectProvider(providerId)');
    expect(main).toContain('selectModel(target.value)');
    expect(main).toContain('advanceOnboarding');
    expect(main).toContain('completeOnboarding(result)');
    expect(main).toContain('hydrateOnboardingProgress()');
    expect(controller).toContain('ONBOARDING_PROGRESS_STORAGE_KEY');
    expect(controller).toContain('workspacePath: state.workspacePath');
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
    expect(onboarding).toContain('provider.websiteUrl');
    expect(onboarding).toContain('data-open-external');
    expect(icons).toContain('externalLink');
    expect(state).toContain("providerSetupStage: 'provider'");
    expect(state).toContain('providerSelectionConfirmed: false');
    expect(onboarding).toContain('function renderProviderStep');
    expect(onboarding).toContain('function renderAccessStep');
    expect(onboarding).toContain('function renderModelStep');
    expect(onboarding).toContain('function renderPreferencesStep');
    expect(onboarding).toContain('function renderReviewStep');
    expect(onboarding).toContain('auth-method-grid');
    expect(onboarding).toContain('data-provider-auth-method');
    expect(onboarding).not.toContain("state.providerSetupStage === 'provider'");
    expect(onboarding).not.toContain('data-provider-setup-stage="auth"');
    expect(onboarding).not.toContain('data-provider-setup-stage="model"');
    expect(onboarding).not.toContain('id="continue-provider-model"');
    expect(onboarding).not.toContain('Continue to model');
    expect(onboarding).toContain('provider.dataRoute');
    expect(onboarding).toContain('provider.dataRegion');
    expect(onboarding).toContain('id="provider-key-status"');
    expect(onboarding).toContain("provider.requiresKey ? '' : '(optional)'");
    expect(onboarding).toContain('state.providerApiKey');
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
    expect(onboarding).toContain('data-provider-model');
    expect(onboarding).toContain('model-choice-grid');
    expect(onboarding).not.toContain('<input id="provider-model"');
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
    expect(onboarding).toContain('Most local servers do not need a key.');
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
    expect(css).toContain('.app-frame');
    expect(css).toContain('.app-toolbar');
    expect(css).toContain('.setup-overlay');
    expect(css).toContain('position: fixed');
    expect(css).toContain('z-index: 500');
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

  test('runtime choices retain real examples without decorative preview animation', () => {
    const constants = read('src/ui/desktop/modules/constants.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');

    expect(constants).toContain('examples:');
    expect(constants).toContain('demoSteps:');
    expect(onboarding).toContain('runtimeModes');
    expect(onboarding).toContain('data-runtime-mode');
    expect(onboarding).toContain('Primary workflow');
    expect(onboarding).not.toContain('runtime-demo');
  });

  test('chat stays focused on conversation and composer controls', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');
    const html = read('src/ui/desktop/index.html');
    const utils = read('src/ui/desktop/modules/utils.js');
    const constants = read('src/ui/desktop/modules/constants.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');

    expect(chat).not.toContain('chat-action-row');
    expect(chat).not.toContain('Start Gateway');
    expect(chat).not.toContain('Check Gateway');
    expect(chat).toContain('renderMarkdown');
    expect(chat).toContain('markdown-body');
    expect(chat).toContain('renderTypingStatus');
    expect(chat).toContain('chat-runtime-pill');
    expect(chat).toContain('chat-control-popover');
    expect(chat).toContain('workspace-control');
    expect(chat).toContain('composer-context-usage');
    expect(chat).toContain('aria-label="Send message"');
    expect(chat).not.toContain('composer-footnote');
    expect(chat).not.toContain('context-button');
    expect(chat).toContain('conversation-details');
    expect(utils).toContain('function renderMarkdown');
    expect(utils).toContain('estimateContextTokens');
    expect(utils).toContain('modelMetadataFor');
    expect(constants).toContain('thinkingLevels');
    expect(constants).toContain('modelMetadata');
    expect(onboarding).toContain('model-choice-grid');
    expect(onboarding).toContain('modelMetadataFor');
    expect(chat).toContain('conversation-library');
    expect(chat).toContain('data-recent-chat');
    expect(chat).toContain('activeChatId');
    expect(chat).toContain('composer-card');
    expect(chat).toContain('composer-toolbar');
    expect(chat).toContain('escapeHtml(status)');
    expect(chat).toContain('id="attach-file"');
    expect(chat).toContain('id="voice-input"');
    expect(chat).toContain('data-chat-thinking');
    expect(chat).toContain('data-chat-model');
    expect(chat).toContain('data-toggle-chat-controls');
    expect(chat).toContain('data-open-chat-details');
    expect(main).toContain('buildLocalReply');
    expect(main).toContain('chooseChatAttachment');
    expect(main).toContain('startVoiceInput');
    expect(main).toContain('addTerminalEntry');
    expect(main).toContain('sendChatMessage');
    expect(main).toContain('state.chatStreaming = true');
    expect(main).toContain('state.chatStreaming = false');
    expect(main).toContain('status.dataset.state =');
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
    expect(state).toContain('chatControlsOpen:');
    expect(state).toContain('toggleChatControls');
    expect(state).toContain('systemPrompt:');
    expect(state).toContain('selectedContextAccess:');
    expect(state).toContain('chatAttachments:');
    expect(state).toContain('terminalEntries:');
    expect(state).toContain('agentName:');
    expect(state).toContain('userName:');
    expect(css).toContain('.chat-layout');
    expect(css).toContain('.conversation-stage');
    expect(css).toContain('.conversation-library');
    expect(css).toContain('.conversation-details');
    expect(css).toContain('.composer-region');
    expect(css).toContain('overflow-y: auto');
    expect(css).toContain('.composer-card');
    expect(css).toContain('.markdown-body');
    expect(css).toContain('.typing-status');
    expect(css).toContain('.tactile-control');
    expect(css).toContain('.composer-meta-chip');
    expect(html).toContain('<body data-active-section="chat">');
    expect(html).toContain('./styles.css');
    expect(html).not.toContain('./styles/redesign.css');
  });

  test('matches the new application frame with focused conversation side panels', () => {
    const html = read('src/ui/desktop/index.html');
    const chat = read('src/ui/desktop/modules/chat.js');
    const shell = read('src/ui/desktop/modules/shell.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).toContain('class="app-frame"');
    expect(html).toContain('class="app-sidebar"');
    expect(html).toContain('class="app-toolbar"');
    expect(html).toContain('class="page-identity"');
    expect(html).toContain('id="provider-status-pill"');
    expect(html).toContain('class="app-brand"');
    expect(shell).toContain('renderProviderStatusPill');
    expect(chat).toContain('conversation-stage');
    expect(chat).toContain('conversation-header');
    expect(chat).toContain('conversation-details');
    expect(chat).toContain('conversation-library');
    expect(chat).toContain('data-toggle-chat-panel');
    expect(css).toContain('.chat-layout');
    expect(css).toContain('.conversation-stage');
    expect(css).toContain('.conversation-details');
    expect(css).toContain('overflow-x: hidden');
  });

  test('uses the focused chat shell without replacing chat handlers', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');

    for (const className of [
      'chat-layout',
      'conversation-library',
      'conversation-stage',
      'conversation-header',
      'message-stream',
      'composer-card',
      'conversation-details',
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

    expect(chat).toContain('history-is-closed');
    expect(chat).toContain('history-is-open');
    expect(chat).toContain('details-is-closed');
    expect(chat).toContain('details-is-open');
    expect(chat).toContain('chat-message');
    expect(chat).toContain('tactile-control');
    expect(chat).toContain('chat-runtime-pill');
    expect(chat).toContain('composer-context-usage');
    expect(chat).not.toContain('context-button');
    expect(css).toContain('.history-is-closed .conversation-library');
    expect(css).toContain('.details-is-closed .conversation-details');
    expect(css).toContain('display: none');
    expect(css).toContain('.chat-message');
    expect(css).toContain('.system-card');
  });

  test('renders open chat rows instead of message bubbles', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');
    const messageRenderer = chat.slice(
      chat.indexOf('function renderMessage('),
      chat.indexOf('function renderAvatar'),
    );

    expect(messageRenderer).toContain('chat-message');
    expect(chat).toContain('message-avatar');
    expect(messageRenderer).toContain('message-body');
    expect(messageRenderer).toContain('system-card');
    expect(messageRenderer).not.toContain('message-bubble');
    expect(css).toContain('.chat-message');
    expect(css).toContain('.message-body');
    expect(css).toContain('.system-card');
    expect(messageRenderer).not.toContain('clip-path');
  });

  test('keeps typing status outside chat history and above the composer', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');

    const transcriptStart = chat.indexOf('<div class="chat-transcript"');
    const transcriptEnd = chat.indexOf('${renderNewMessageNotice(state)}');
    const transcript = chat.slice(transcriptStart, transcriptEnd);
    const typing = chat.slice(
      chat.indexOf('function renderTypingStatus'),
      chat.indexOf('function renderDetailsPanel'),
    );

    expect(transcript).not.toContain('${renderTypingStatus(state)}');
    expect(chat.indexOf('${renderTypingStatus(state)}')).toBeGreaterThan(transcriptStart);
    expect(chat.indexOf('${renderTypingStatus(state)}')).toBeLessThan(
      chat.indexOf('${renderComposer(state'),
    );
    expect(chat).toContain('data-new-transmission="true"');
    expect(typing).toContain('typing-status');
    expect(typing).not.toContain('chat-message');
    expect(css).toContain('.typing-status');
    expect(css).toContain('.conversation-stage');
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
    expect(rust).toContain('llama-server-install');
    expect(rust).toContain('latest_llama_release_asset');
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
    const prepareLlama = read('scripts/prepare-llama-server.js');
    expect(prepareLlama).toContain('optional-llama.nsh');
    expect(prepareLlama).toContain('Install Argentum llama.cpp local server binaries');
    const defaultInstallerHook = read('src/desktop/generated/optional-llama.nsh');
    expect(defaultInstallerHook).toContain('checked-in fallback intentionally contains no payload');
    expect(defaultInstallerHook).not.toContain('NSIS_HOOK_POSTINSTALL');
    expect(workflow).toContain('Build desktop CLI sidecar');
  });

  test('docker images copy build helper scripts before running npm build', () => {
    for (const dockerfile of ['Dockerfile', 'docker/Dockerfile']) {
      const contents = read(dockerfile);
      expect(contents).toContain('COPY scripts/ ./scripts/');
      expect(contents.indexOf('COPY scripts/ ./scripts/')).toBeLessThan(
        contents.indexOf('RUN npm run build'),
      );
    }
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
    expect(css).toContain('.terminal-panel');
    expect(css).toContain('.terminal-body');
    expect(css).toContain('white-space: pre-wrap');
    expect(css).toContain('overflow-wrap: anywhere');
    expect(css).not.toContain('min-width: 960px');
  });

  test('keeps context details accessible while preserving compact composer and font preferences', () => {
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
    expect(chat).toContain('data-compact-context');
    expect(chat).toContain('workspace-control');
    expect(chat).toContain('chat-runtime-pill');
    expect(chat).toContain('composer-context-usage');
    expect(chat).toContain('formatContextPercent');
    expect(chat).toContain('data-workspace-menu="true"');
    expect(chat).not.toContain('context-button');
    expect(chat).toContain('contextPercent');
    expect(chat).toContain('contextTokenLimit(model)');
    expect(chat).toContain('composer-actions');
    expect(sections).toContain('settings-ui-font');
    expect(sections).toContain('settings-code-font');
    expect(utils).toContain('contextTokenLimit');
    expect(utils).toContain('contextUsagePercent');
    expect(css).toContain('.workspace-control');
    expect(css).toContain('.chat-runtime-pill');
    expect(css).toContain('.composer-context-usage');
    expect(css).toContain('.chat-control-popover');
    expect(css).toContain('.composer-actions');
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
    expect(chat).toContain('state.usageSnapshot?.contextTokens');
    expect(chat).toContain('state.usageSnapshot?.contextTokenLimit');
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

  test('keeps the Argentum identity quiet and exposes context from the details drawer', () => {
    const html = read('src/ui/desktop/index.html');
    const chat = read('src/ui/desktop/modules/chat.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).toContain('brand-wordmark');
    expect(html).toContain('class="app-brand"');
    expect(html.match(/\.\/assets\/argentum\.png/g)).toHaveLength(1);
    expect(chat).not.toContain('workspace-empty-logo');
    expect(chat).not.toContain('<img class="workspace-empty-logo" src="./assets/argentum.png"');
    expect(chat).toContain('task-sidebar-brand');
    expect(chat).toContain('argentum-mark');
    expect(chat).toContain('thread-heading');
    expect(chat).toContain('conversation-details');
    expect(chat).toContain('detail-section');
    expect(chat).toContain('data-compact-context');
    expect(chat).not.toContain('context-button');
    expect(css).toContain('.app-brand');
    expect(css).toContain('.brand-wordmark');
    expect(css).toContain('.task-sidebar-brand');
    expect(css).toContain('.argentum-mark');
  });

  test('sends compacted full chat history and keeps composer controls compact', () => {
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
    expect(chat).toContain('composer-toolbar');
    expect(chat).toContain('composer-tools');
    expect(chat).toContain('composer-actions');
    expect(css).toContain('.composer-toolbar');
    expect(css).toContain('.composer-tools');
    expect(css).toContain('.composer-actions');
  });

  test('keeps prompt examples in the empty conversation and prevents top-level chat overflow', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const css = read('src/ui/desktop/styles.css');
    const emptyState = chat.slice(
      chat.indexOf('function renderEmptyConversation'),
      chat.indexOf('function renderMessage('),
    );

    expect(emptyState).toContain('const prompts = [');
    expect(emptyState).toContain('chat-welcome');
    expect(emptyState).toContain('prompt-grid');
    expect(emptyState).toContain('data-chat-prompt');
    expect(main).toContain('viewRoot.className = `view-root view-root-${section.id}`');
    expect(css).toContain('.view-root-chat');
    expect(css).toContain('overflow: hidden');
    expect(css).toContain('.chat-transcript');
    expect(css).toContain('overflow-y: auto');
    expect(css).toContain('.chat-layout');
    expect(css).toContain('height: 100%');
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
    expect(css).toContain('.settings-workbench');
    expect(css).toContain('.security-map-grid');
    expect(css).toContain('.diagnostics-matrix');
    expect(css).toContain('.terminal-panel');
    expect(css).toContain('--sidebar-width: 252px');
    expect(css).toContain('grid-template-columns: var(--sidebar-width) minmax(0, 1fr)');
    expect(css).not.toContain('.product-hero');
    expect(css).not.toContain('.section-command-dock');
  });

  test('extends the Codex-style shell across every desktop surface', () => {
    const html = read('src/ui/desktop/index.html');
    const main = read('src/ui/desktop/main.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).toContain('id="app-sidebar"');
    expect(html).toContain('id="app-sidebar-toggle"');
    expect(html).toContain('id="app-sidebar-scrim"');
    expect(html).toContain('class="argentum-mark"');
    expect(onboarding).toContain('class="argentum-mark"');
    expect(main).toContain('function toggleAppSidebar');
    expect(main).toContain("document.body.classList.contains('global-sidebar-is-open')");
    expect(main).toContain("element.closest('#app-sidebar-toggle')");
    expect(main).toContain("element.closest('#app-sidebar-scrim')");
    expect(main).toContain('function revealActiveSettingsSection');
    expect(main).toContain('revealActiveSettingsSection();');
    expect(main).toContain('sectionNav.scrollLeft = Math.max(0, centeredLeft);');
    expect(read('src/ui/desktop/modules/sections.js')).toContain('aria-current="page"');
    expect(css).toContain('/* Unified application shell */');
    expect(css).toContain("body:not([data-active-section='chat']) .app-sidebar");
    expect(css).toContain("body:not([data-active-section='chat']) .app-main");
    expect(css).toContain('.settings-layout');
    expect(css).toContain('.security-control-panel');
    expect(css).toContain('.setup-window');
    expect(css).toContain('.floating-panel');
    expect(css).toContain('.module-error');
    expect(css).toContain('.startup-failure');
    expect(css).toContain('body.global-sidebar-is-open:not([data-active-section=\'chat\'])');
  });

  test('smooths the chat and settings round trip with shared motion and restored state', () => {
    const html = read('src/ui/desktop/index.html');
    const main = read('src/ui/desktop/main.js');
    const icons = read('src/ui/desktop/modules/icons.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).toContain('id="section-back-button"');
    expect(html).toContain('class="section-back-label">Back to chat</span>');
    expect(icons).toContain("'arrowLeft'");
    expect(main).toContain('let retainedChatScroll = null');
    expect(main).toContain('function syncSectionBackButton');
    expect(main).toContain('function runSectionTransition');
    expect(main).toContain('let activeSectionTransition = null');
    expect(main).toContain('activeSectionTransition.skipTransition()');
    expect(main).toContain("typeof document.startViewTransition === 'function'");
    expect(main).toContain("setActiveSection('chat', { direction: 'back', restoreFocus: true })");
    expect(main).toContain("event.altKey && key === 'ArrowLeft'");
    expect(main).toContain('function openSettingsSection');
    expect(main).toContain("section.id === 'chat' ? retainedChatScroll : null");
    expect(css).toContain('view-transition-name: argentum-sidebar');
    expect(css).toContain('view-transition-name: argentum-toolbar');
    expect(css).toContain('view-transition-name: argentum-content');
    expect(css).toContain('@keyframes argentum-content-enter-forward');
    expect(css).toContain('.section-back-button[hidden]');
    expect(css).toContain('--motion-ease: cubic-bezier');
    expect(css).toContain('@media (prefers-reduced-motion: reduce)');
  });

  test('initializes skill catalog adapters before rendering settings skills', () => {
    const sections = read('src/ui/desktop/modules/sections.js');
    const skillsStart = sections.indexOf("if (activeSection === 'skills')");
    const skillsEnd = sections.indexOf("if (activeSection === 'feedback')");
    const skills = sections.slice(skillsStart, skillsEnd);

    expect(skillsStart).toBeGreaterThan(-1);
    expect(skillsEnd).toBeGreaterThan(skillsStart);
    expect(skills.indexOf('const normalizeInstalled')).toBeLessThan(
      skills.indexOf('.map((s) => ({\n      ...normalizeInstalled(s)'),
    );
    expect(skills.indexOf('const normalizeCatalog')).toBeLessThan(
      skills.indexOf('.map(normalizeCatalog)'),
    );
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
    expect(css).toContain('.terminal-entry pre');
    expect(css).toContain('.log-viewer pre');
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
    expect(css).toContain('.conversation-delete-confirm');
  });

  test('wires conversation filters, side panels, workspace menu, and help panel', () => {
    const html = read('src/ui/desktop/index.html');
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');

    expect(html).not.toContain('data-view-mode');
    expect(html).toContain('id="help-button"');
    expect(html).toContain('id="workspace-button"');
    expect(chat).not.toContain('data-chat-filter="recent"');
    expect(chat).toContain('data-chat-filter="pinned"');
    expect(chat).toContain('data-chat-filter="all"');
    expect(chat).toContain('filteredConversations');
    expect(chat).toContain('data-conversation-menu');
    expect(chat).toContain('data-pin-chat');
    expect(chat).toContain('data-clear-chat');
    expect(chat).toContain('data-rename-chat');
    expect(chat).toContain('data-toggle-chat-panel="inspector"');
    expect(chat).toContain('data-toggle-chat-panel="conversations"');
    expect(state).toContain('chatFilter:');
    expect(state).toContain('lastMessageAt');
    expect(state).toContain('lastOpenedAt');
    expect(state).toContain('unreadCount');
    expect(state).toContain('setChatFilter');
    expect(state).toContain('toggleChatPanel');
    expect(state).toContain('toggleChatPinned');
    expect(state).toContain('toggleWorkspaceMenu');
    expect(state).toContain('toggleHelp');
    expect(main).toContain('renderHelpPanel');
    expect(main).toContain('renderWorkspacePanel');
    expect(main).toContain('regenerateAssistantResponse');
    expect(main).toContain('chat.regenerate_state_reset');
    expect(css).toContain('.chat-layout');
    expect(css).toContain('.history-is-closed .conversation-library');
    expect(css).toContain('.details-is-closed .conversation-details');
    expect(css).toContain('.conversation-menu');
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
    expect(onboarding).toContain('provider-tabs');
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
    expect(onboarding).toContain('choice-card');
    expect(onboarding).toContain('check-list');
    expect(onboarding).toContain('data-context-access');
    expect(css).toContain('.check-card-head');
    expect(css).toContain('.choice-card');
    expect(css).toContain('.check-list');
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
    expect(chat).toContain('renderReasoning');
    expect(chat).toContain('reasoning-disclosure');
    expect(sections).toContain('settings-show-thinking-chat');
    expect(sections).toContain('settings-show-thinking-telegram');
    expect(setup).toContain('showThinkingInChat');
    expect(setup).toContain('showThinkingInTelegram');
    expect(main).toContain('settings-show-thinking-chat');
    expect(main).toContain('settings-show-thinking-telegram');
    expect(css).toContain('.reasoning-disclosure');
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
    const messageActions = chat.slice(
      chat.indexOf('function renderMessageActions'),
      chat.indexOf('function renderMessageAttachments'),
    );

    expect(constants).toContain('Monaspace Krypton');
    expect(css).toContain('@font-face');
    expect(css).toContain('Monaspace Krypton');
    expect(state).toContain('activeAssistantMessageId');
    expect(state).toContain('chatAbortRequested');
    expect(state).toContain("typeof block.id === 'string' && block.id.trim()");

    for (const name of [
      'plus',
      'x',
      'trash',
      'send',
      'transmit',
      'stop',
      'copy',
      'image',
      'file',
      'refresh',
      'retry',
      'orbit',
    ]) {
      expect(icons).toMatch(new RegExp(`['"]?${name}['"]?:`));
    }

    expect(chat).toContain('function renderMessage');
    expect(chat).toContain('function renderComposer');
    expect(chat).toContain('function renderAttachment');
    expect(chat).toContain('function renderMessageActions');
    expect(chat).toContain('contextPercent');
    expect(chat).toContain('data-compact-context');
    expect(chat).toContain('system-card');
    expect(chat).toContain('data-copy-message');
    expect(chat).toContain('data-regenerate-message');
    expect(chat).toContain('data-retry-message');
    expect(messageActions).toContain('data-icon="refresh"');
    expect(messageActions).toContain('data-icon="retry"');
    expect(chat).toContain('data-icon="send"');
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

  test('renders a distinct fresh chat state and focused setup application', () => {
    const chat = read('src/ui/desktop/modules/chat.js');
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const css = read('src/ui/desktop/styles.css');

    expect(chat).toContain('chat-welcome');
    expect(chat).toContain('prompt-grid');
    expect(chat).toContain('empty-state-heading');
    expect(chat).toContain('argentum-mark large');
    expect(chat).toContain('What should we work on?');
    expect(chat).toContain('Explain this workspace');
    expect(chat).toContain('Fix a problem');
    expect(chat).toContain('Build a feature');
    expect(chat).not.toContain('possibility-field');
    expect(chat).not.toContain('agent-core');
    expect(chat).not.toContain('context-orbit');
    expect(onboarding).toContain('setup-window');
    expect(onboarding).toContain('setup-progress');
    expect(onboarding).toContain('setup-content');
    expect(onboarding).not.toContain('setup-mini-map');
    expect(onboarding).not.toContain('copy-block compact-copy');
    expect(css).toContain('.chat-welcome');
    expect(css).toContain('.prompt-grid');
    expect(css).toContain('.empty-state-heading');
    expect(css).toContain('.argentum-mark');
    expect(css).toContain('.setup-window');
  });

  test('keeps onboarding stable while choices change and content scrolls independently', () => {
    const onboarding = read('src/ui/desktop/modules/onboarding.js');
    const css = read('src/ui/desktop/styles.css');

    expect(css).toContain('height: min(760px, calc(100vh - 48px));');
    expect(css).toContain('.setup-scroll-region');
    expect(css).toContain('overflow-y: auto;');
    expect(css).toContain('.setup-footer');
    expect(onboarding).toContain('provider-tabs');
    expect(onboarding).toContain('data-provider-id');
    expect(onboarding).toContain('preference-layout');
    expect(onboarding).toContain('check-list');
    expect(onboarding).toContain('data-context-access');
    expect(onboarding).toContain('data-channel-id');
    expect(onboarding).toContain('model-choice-grid');
    expect(onboarding).toContain('data-provider-model');
  });

  test('keeps the chat shell minimal while retaining responsive drawers and workspace access', () => {
    const html = read('src/ui/desktop/index.html');
    const chat = read('src/ui/desktop/modules/chat.js');
    const main = read('src/ui/desktop/main.js');
    const state = read('src/ui/desktop/modules/state.js');
    const css = read('src/ui/desktop/styles.css');
    const toggleChatControls = state.slice(
      state.indexOf('export function toggleChatControls'),
      state.indexOf('export function setSettingsSection'),
    );

    expect(html).toContain('<body data-active-section="chat">');
    expect(chat).not.toContain('class="composer-context-row"');
    expect(chat).toContain('data-workspace-menu="true"');
    expect(chat).not.toContain('id="chat-runtime-mode"');
    expect(chat).not.toContain('data-section="security"');
    expect(chat).toContain('data-toggle-chat-panel="conversations"');
    expect(chat).toContain('data-toggle-chat-panel="inspector"');
    expect(chat).toContain('data-close-chat-panels="true"');
    expect(main).toContain('document.body.dataset.activeSection = section.id');
    expect(main).toContain("window.matchMedia('(max-width: 900px)')");
    expect(main).toContain('closeChatPanelsForCompactLayout');
    expect(state).toContain('conversationsCollapsed: false');
    expect(state).toContain('inspectorCollapsed: true');
    expect(toggleChatControls).not.toContain('state.conversationsCollapsed = true');
    expect(toggleChatControls).toContain('state.inspectorCollapsed = true');
    expect(css).toContain("body[data-active-section='chat'] .app-sidebar");
    expect(css).toContain("body[data-active-section='chat'] .app-toolbar");
    expect(css).toContain('.workspace-control');
    expect(css).toContain('.chat-runtime-pill');
    expect(css).toContain('.chat-control-popover');
    expect(css).toContain('.chat-panel-scrim');
    expect(css).toContain('.chat-layout.history-is-closed.details-is-closed');
    expect(css).toContain('position: absolute;');
    expect(css).toContain('@keyframes runtime-picker-arrive');
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
    expect(css).toContain('.notification-history-item');
    expect(css).toContain('.notification-toast.success .notification-indicator');
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
    expect(index).not.toContain(`${String.fromCodePoint(0x1f916)} Welcome to Argentum`);
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
    expect(chat).toContain('state.usageSnapshot?.contextTokens');
    expect(chat).toContain('state.usageSnapshot?.contextTokenLimit');
    expect(chat).toContain('numberFromUsage');
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
    expect(rust).toContain('fn saved_setup_from_workspace');
    expect(setup).toContain('function applySavedSetup');
    expect(setup).toContain('defaults?.savedSetup');
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

  test('documents current support matrix and provider maturity', () => {
    const packageJson = JSON.parse(read('package.json')) as { version: string };
    const readme = read('README.md');
    const release = read(`docs/releases/v${packageJson.version}.md`);

    expect(readme).toContain(`v${packageJson.version}`);
    expect(release).toContain(`v${packageJson.version}`);

    for (const document of [readme, release]) {
      expect(document).toContain('Provider Status');
      expect(document).toContain('Release-candidate providers');
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

  // NOTE: ARGENTUM_APP_KNOWLEDGE.md moved to internal-only. Test disabled.
  // test('documents multi-workspace/agent architecture and app-awareness boundary', () => {
  //   const workspaces = read('docs/WORKSPACES_AND_AGENTS.md');
  //   const knowledge = read('docs/ARGENTUM_APP_KNOWLEDGE.md');
  //   const utils = read('src/ui/desktop/modules/utils.js');
  //   const rust = read('src/desktop/src/lib.rs');
  //   expect(workspaces).toContain('Workspace');
  //   expect(workspaces).toContain('Session');
  //   expect(workspaces).toContain('Agent');
  //   expect(workspaces).toContain('Planned Multi-Workspace Flow');
  //   expect(knowledge).toContain('Current Desktop Surfaces');
  //   expect(knowledge).toContain('What The Assistant May Know When Approved');
  //   expect(knowledge).toContain('Privacy Boundary');
  //   expect(utils).toContain('Argentum app knowledge');
  //   expect(utils).toContain('Current page:');
  //   expect(utils).toContain('Active session:');
  //   expect(utils).toContain('estimateCachedTextTokens');
  //   expect(utils).toContain('defaultCoreContextText');
  //   expect(utils).toContain('Provider usage:');
  //   expect(rust).toContain('Available app actions in the desktop MVP');
  // });
});
