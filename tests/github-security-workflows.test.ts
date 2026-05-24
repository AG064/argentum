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
  test('CodeQL workflow uses default setup compatible configuration (no advanced queries)', () => {
    // Default setup is enabled in repository settings
    // CodeQL workflow must NOT use security-extended queries with default setup
    const codeql = workflow('codeql.yml');
    // Must not contain advanced query configuration
    expect(codeql).not.toContain('queries: security-extended');
    // Should use basic security queries only
    expect(codeql).toContain('github/codeql-action/init@v4');
    expect(codeql).toContain('github/codeql-action/analyze@v4');
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
    expect(desktop).toContain('dtolnay/rust-toolchain@3c5f7ea28cd621ae0bf5283f0e981fb97b8a7af9');
    expect(desktop).toContain('libwebkit2gtk-4.1-dev');
    expect(desktop).toContain('libayatana-appindicator3-dev');
    expect(desktop).toContain('libssl-dev');
    expect(desktop).toContain('npm run desktop:build');
    expect(desktop).toContain('actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a');
    expect(desktop).toContain('src/desktop/target/release/bundle/**/*');
    expect(desktop).toContain('src/desktop/target/*/release/bundle/**/*');
    expect(desktop).toContain('softprops/action-gh-release@403a5240f3837fa857f642062e05aad6bb3391ca');
  });

  test('OpenSSF Scorecard uploads SARIF results to GitHub code scanning', () => {
    const scorecard = workflow('scorecard.yml');

    expect(scorecard).toContain('ossf/scorecard-action@af76153369ae1eb1eaffc4118046b7fda9a8419e');
    expect(scorecard).toContain('github/codeql-action/upload-sarif@0e150e40762c1253b364a04b0fc9f2cc14effff2');
    expect(scorecard).toContain('category: scorecard');
  });
});
