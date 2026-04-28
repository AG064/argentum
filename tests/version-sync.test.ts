import { existsSync, readFileSync } from 'fs';

describe('version synchronization', () => {
  test('has a repository version sync script and CI drift check', () => {
    const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as {
      version?: string;
      scripts?: Record<string, string>;
    };
    const ciWorkflow = readFileSync('.github/workflows/ci.yml', 'utf8');

    expect(packageJson.version).toBe('0.0.3');
    expect(packageJson.scripts?.['version:sync']).toBe('node scripts/sync-version.js');
    expect(packageJson.scripts?.['version:check']).toBe('node scripts/sync-version.js --check');
    expect(existsSync('scripts/sync-version.js')).toBe(true);
    expect(ciWorkflow).toContain('npm run version:check');
  });
});
