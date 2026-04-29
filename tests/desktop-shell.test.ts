import { existsSync, readFileSync } from 'fs';

describe('Argentum desktop shell scaffold', () => {
  test('defines a Tauri desktop application branded as Argentum', () => {
    expect(existsSync('src/desktop/tauri.conf.json')).toBe(true);

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
    expect(config.version).toBe('0.0.3');
    expect(config.identifier).toBe('com.argentum.app');
    expect(config.app?.windows?.[0]).toEqual(
      expect.objectContaining({ title: 'Argentum', width: 1280, height: 800 }),
    );
    expect(config.bundle?.active).toBe(true);
    expect(config.bundle?.icon).toEqual(
      expect.arrayContaining([
        '../../assets/brand/argentum.png',
        '../../installer/wix/argentum.ico',
      ]),
    );
    expect(config.app?.security?.csp).toContain("default-src 'self'");
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

  test('exposes npm scripts for desktop development and packaging', () => {
    const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as {
      scripts?: Record<string, string>;
      devDependencies?: Record<string, string>;
      pkg?: { assets?: string[] };
    };

    expect(packageJson.scripts?.['desktop:dev']).toBe(
      'tauri dev --config src/desktop/tauri.conf.json',
    );
    expect(packageJson.scripts?.['desktop:build']).toBe(
      'tauri build --config src/desktop/tauri.conf.json',
    );
    expect(packageJson.devDependencies).toHaveProperty('@tauri-apps/cli');
    expect(packageJson.pkg?.assets).toEqual(expect.arrayContaining(['src/ui/desktop/**/*']));
  });
});
