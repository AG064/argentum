import { existsSync, readFileSync } from 'fs';

function workflow(name: string): string {
  return readFileSync(`.github/workflows/${name}`, 'utf8');
}

describe('GitHub security workflow baseline', () => {
  test('runs CodeQL with extended JavaScript and TypeScript security queries', () => {
    expect(existsSync('.github/workflows/codeql.yml')).toBe(true);

    const codeql = workflow('codeql.yml');
    expect(codeql).toContain('github/codeql-action/init@v4');
    expect(codeql).toContain('github/codeql-action/analyze@v4');
    expect(codeql).toContain('javascript-typescript');
    expect(codeql).toContain('queries: security-and-quality');
    expect(codeql).toContain('security-and-quality');
    expect(codeql).not.toContain('queries: +');
    expect(codeql).toContain('security-events: write');
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
    for (const name of ['binary.yml', 'release.yml']) {
      expect(workflow(name)).toContain('npm ci --legacy-peer-deps');
    }
  });

  test('OpenSSF Scorecard uploads SARIF results to GitHub code scanning', () => {
    const scorecard = workflow('scorecard.yml');

    expect(scorecard).toContain('ossf/scorecard-action@v2.4.3');
    expect(scorecard).toContain('github/codeql-action/upload-sarif@v4');
    expect(scorecard).toContain('category: scorecard');
  });
});
