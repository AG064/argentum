import { existsSync, readFileSync, readdirSync } from 'fs';

function workflow(name: string): string {
  return readFileSync(`.github/workflows/${name}`, 'utf8');
}

function workflows(): string[] {
  return readdirSync('.github/workflows').filter(
    (name) => name.endsWith('.yml') || name.endsWith('.yaml'),
  );
}

describe('GitHub security workflow baseline', () => {
  test('does not define an advanced CodeQL workflow when repository default setup owns CodeQL', () => {
    expect(existsSync('.github/workflows/codeql.yml')).toBe(false);

    for (const name of workflows()) {
      const contents = workflow(name);
      expect(contents).not.toContain('github/codeql-action/init');
      expect(contents).not.toContain('github/codeql-action/analyze');
    }
  });

  test('security workflows install dependencies consistently with the lockfile policy', () => {
    for (const name of ['security-scan.yml', 'weekly-security.yml', 'security-automation.yml']) {
      expect(workflow(name)).toContain('npm ci --legacy-peer-deps');
    }
  });

  test('secret scanning stays scoped to committed files instead of historical false positives', () => {
    const secretScanning = workflow('secret-scanning.yml');

    expect(secretScanning).toContain('git archive --format=tar HEAD');
    expect(secretScanning).toContain('gitleaks-scan');
    expect(secretScanning).toContain('dir /scan');
    expect(secretScanning).not.toContain('gitleaks detect');
  });

  test('release workflows use the same dependency install policy as CI', () => {
    for (const name of ['binary.yml', 'desktop.yml', 'release.yml']) {
      expect(workflow(name)).toContain('npm ci --legacy-peer-deps');
    }
  });

  test('desktop workflow builds Tauri artifacts for each supported platform', () => {
    const desktop = workflow('desktop.yml');

    expect(desktop).toContain('ubuntu-22.04');
    expect(desktop).toContain('windows-latest');
    expect(desktop).toContain('macos-latest');
    expect(desktop).toContain('macos-15-intel');
    expect(desktop).toContain('aarch64-apple-darwin');
    expect(desktop).toContain('x86_64-apple-darwin');
    expect(desktop).toContain('dtolnay/rust-toolchain@stable');
    expect(desktop).toContain('libwebkit2gtk-4.1-dev');
    expect(desktop).toContain('libayatana-appindicator3-dev');
    expect(desktop).toContain('libssl-dev');
    expect(desktop).toContain('npm run desktop:build');
    expect(desktop).toContain('actions/upload-artifact@v4');
    expect(desktop).toContain('src/desktop/target/release/bundle/**/*');
    expect(desktop).toContain('src/desktop/target/*/release/bundle/**/*');
    expect(desktop).toContain('softprops/action-gh-release@v2');
  });

  test('OpenSSF Scorecard uploads SARIF results to GitHub code scanning', () => {
    const scorecard = workflow('scorecard.yml');

    expect(scorecard).toContain('ossf/scorecard-action@v2.4.3');
    expect(scorecard).toContain('github/codeql-action/upload-sarif@v4');
    expect(scorecard).toContain('category: scorecard');
  });
});
