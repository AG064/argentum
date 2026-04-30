import { existsSync, readFileSync } from 'fs';

describe('Argentum desktop shell scaffold', () => {
  test('defines a Tauri desktop application branded as Argentum', () => {
    expect(existsSync('src/desktop/tauri.conf.json')).toBe(true);
    expect(existsSync('src/desktop/src/lib.rs')).toBe(true);

    const config = JSON.parse(readFileSync('src/desktop/tauri.conf.json', 'utf8')) as {
      productName?: string;
      version?: string;
      identifier?: string;
      app?: {
        windows?: Array<{ title?: string; width?: number; height?: number }>;
        security?: { csp?: string };
      };
      bundle?: { active?: boolean; icon?: string[] };
    };

    expect(config.productName).toBe('Argentum');
    expect(config.version).toBe('0.0.4');
    expect(config.identifier).toBe('com.argentum.app');
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
    expect(existsSync('installer/macos/argentum.icns')).toBe(true);
    expect(config.app?.security?.csp).toContain("default-src 'self'");

    const cargo = readFileSync('src/desktop/Cargo.toml', 'utf8');
    const main = readFileSync('src/desktop/src/main.rs', 'utf8');
    const lib = readFileSync('src/desktop/src/lib.rs', 'utf8');

    expect(cargo).toContain('[lib]');
    expect(main).toContain('argentum_desktop_lib::run()');
    expect(lib).toContain('pub fn run()');
  });

  test('ships an onboarding-first desktop UI entry point', () => {
    for (const file of [
      'src/ui/desktop/index.html',
      'src/ui/desktop/main.js',
      'src/ui/desktop/styles.css',
    ]) {
      expect(existsSync(file)).toBe(true);
    }

    const html = readFileSync('src/ui/desktop/index.html', 'utf8');
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');

    expect(html).toContain('<title>Argentum</title>');
    expect(html).toContain('../../assets/brand/argentum.png');
    expect(js).toContain('Workspace and data location');
    expect(js).toContain('Security & Permissions');
    expect(js).toContain('Agent Runner');
    expect(js).toContain('Knowledge Graph');
    expect(js).toContain('Skills Library');
  });

  test('lets users revise setup and launch into the desktop interface', () => {
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');

    expect(js).toContain('setupComplete: false');
    expect(js).toContain('Launch Argentum');
    expect(js).toContain("state.activeSection = 'chat'");
    expect(js).toContain('state.setupComplete = true');
    expect(js).toContain("querySelector('#workspace-input')");
    expect(js).toContain("addEventListener('input'");
    expect(js).toContain('disabled: state.onboardingStep === 1');
  });

  test('captures first-run setup choices for runtime, provider, channels, and security', () => {
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');

    expect(js).toContain("runtimeMode: 'desktop'");
    expect(js).toContain("llmProvider: 'openai'");
    expect(js).toContain("channelMode: 'local-only'");
    expect(js).toContain("securityProfile: 'restricted'");
    expect(js).toContain('id="runtime-mode"');
    expect(js).toContain('id="llm-provider"');
    expect(js).toContain('id="channel-mode"');
    expect(js).toContain('id="security-profile"');
    expect(js).toContain("bindSelect('#runtime-mode'");
    expect(js).toContain("bindSelect('#llm-provider'");
    expect(js).toContain("bindSelect('#channel-mode'");
    expect(js).toContain("bindSelect('#security-profile'");
    expect(js).toContain('renderReviewRows');
  });

  test('bridges desktop onboarding to a Tauri setup save command', () => {
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');
    const rust = readFileSync('src/desktop/src/lib.rs', 'utf8');

    expect(js).toContain('function buildSetupPayload()');
    expect(js).toContain('async function saveSetup()');
    expect(js).toContain("invoke('save_setup'");
    expect(js).toContain('await saveSetup()');
    expect(js).toContain('setup_saved');
    expect(js).toContain('setup-error');
    expect(rust).toContain('struct SaveSetupRequest');
    expect(rust).toContain('#[tauri::command]');
    expect(rust).toContain('fn save_setup');
    expect(rust).toContain('std::fs::create_dir_all');
    expect(rust).toContain('fn ensure_allowed');
    expect(rust).toContain('["desktop", "cli", "service"]');
    expect(rust).toContain('["restricted", "ask", "session", "trusted"]');
    expect(rust).toContain('config/default.yaml');
    expect(rust).toContain('secrets.env');
    expect(rust).toContain('save_setup,');
    expect(rust).toContain('run_desktop_action,');
    expect(rust).toContain('desktop_defaults');
    expect(rust).toContain('desktop_state');
  });

  test('hydrates onboarding with a resolved desktop default workspace path', () => {
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');
    const rust = readFileSync('src/desktop/src/lib.rs', 'utf8');

    expect(js).toContain('async function hydrateDesktopDefaults()');
    expect(js).toContain("invoke('desktop_defaults'");
    expect(js).toContain('defaultWorkspacePath');
    expect(js).toContain('hydrateDesktopDefaults().then');
    expect(rust).toContain('struct DesktopDefaultsResponse');
    expect(rust).toContain('fn default_workspace_path');
    expect(rust).toContain('fn desktop_defaults');
    expect(rust).toContain('default_workspace_path');
  });

  test('maps primary CLI workflows into GUI actions', () => {
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');

    expect(js).toContain('const commandCatalog');
    expect(js).toContain('argentum gateway start');
    expect(js).toContain('argentum gateway status');
    expect(js).toContain('argentum doctor');
    expect(js).toContain('argentum agents list');
    expect(js).toContain('argentum memory search');
    expect(js).toContain('argentum skill list');
    expect(js).toContain('argentum security status');
    expect(js).toContain('argentum image "prompt"');
    expect(js).toContain('data-copy-command');
    expect(js).toContain('data-run-action');
    expect(js).toContain('renderActionCards');
    expect(js).toContain("invoke('run_desktop_action'");
    expect(js).toContain('request: { actionId');
  });

  test('renders specialized desktop surfaces instead of placeholders', () => {
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');
    const css = readFileSync('src/ui/desktop/styles.css', 'utf8');

    for (const renderer of [
      'renderChatSection',
      'renderAgentsSection',
      'renderRunnerSection',
      'renderSkillsSection',
      'renderWebchatSection',
      'renderGraphSection',
      'renderMemorySection',
      'renderLogsSection',
      'renderSecuritySection',
      'renderSettingsSection',
      'renderDiagnosticsSection',
    ]) {
      expect(js).toContain(`function ${renderer}`);
    }

    expect(js).not.toContain('Desktop shell route is ready for runtime wiring.');
    expect(css).toContain('.hero-strip');
    expect(css).toContain('.command-card');
    expect(css).toContain('.chat-shell');
    expect(css).toContain('.graph-canvas');
    expect(css).toContain('.approval-row');
  });

  test('exposes a safe allowlisted desktop action bridge', () => {
    const rust = readFileSync('src/desktop/src/lib.rs', 'utf8');

    expect(rust).toContain('struct RunDesktopActionRequest');
    expect(rust).toContain('struct RunDesktopActionResponse');
    expect(rust).toContain('fn run_desktop_action');
    expect(rust).toContain('"doctor"');
    expect(rust).toContain('"gateway-status"');
    expect(rust).toContain('"security-status"');
    expect(rust).toContain('Unknown desktop action');
    expect(rust).toContain('run_desktop_action,');
  });

  test('loads read-only workspace state into desktop diagnostics', () => {
    const js = readFileSync('src/ui/desktop/main.js', 'utf8');
    const rust = readFileSync('src/desktop/src/lib.rs', 'utf8');

    expect(rust).toContain('struct DesktopStateRequest');
    expect(rust).toContain('struct DesktopStateResponse');
    expect(rust).toContain('fn desktop_state');
    expect(rust).toContain('gateway_log_preview');
    expect(rust).toContain('audit_log_preview');
    expect(rust).toContain('redact_sensitive_line');
    expect(rust).toContain('desktop_state,');
    expect(js).toContain('desktopState:');
    expect(js).toContain('async function refreshDesktopState');
    expect(js).toContain("invoke('desktop_state'");
    expect(js).toContain('data-refresh-state');
    expect(js).toContain('formatWorkspaceHealth');
    expect(js).toContain('gatewayLogPreview');
    expect(js).toContain('auditLogPreview');
  });

  test('exposes npm scripts for desktop development and packaging', () => {
    const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as {
      scripts?: Record<string, string>;
      devDependencies?: Record<string, string>;
      pkg?: { assets?: string[] };
    };

    expect(packageJson.scripts?.['desktop:dev']).toBe('cd src/desktop && tauri dev');
    expect(packageJson.scripts?.['desktop:build']).toBe('cd src/desktop && tauri build');
    expect(packageJson.scripts?.['package:win']).toBe('npm run desktop:build');
    expect(packageJson.scripts?.['package:win:gui']).toBe('npm run desktop:build');
    expect(packageJson.scripts?.['package:win:cli']).toContain(
      'build-windows-release.ps1 -SkipMsi',
    );
    expect(packageJson.devDependencies).toHaveProperty('@tauri-apps/cli');
    expect(packageJson.pkg?.assets).toEqual(expect.arrayContaining(['src/ui/desktop/**/*']));
  });
});
