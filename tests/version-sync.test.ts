import { existsSync, readdirSync, readFileSync, statSync } from 'fs';
import { extname, join } from 'path';

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

  test('keeps product-owned source version literals synchronized', () => {
    const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as { version?: string };
    const expectedVersion = packageJson.version;
    const mismatches: string[] = [];

    for (const file of [...listFiles('src', new Set(['.ts', '.js'])), ...listFiles('tests', new Set(['.ts', '.js']))]) {
      const source = readFileSync(file, 'utf8');
      for (const line of source.split(/\r?\n/)) {
        if (!/\b(?:version|argentumVersion|agClawVersion|ver):|\bVERSION\s*=|\.version\b/.test(line)) {
          continue;
        }

        for (const match of line.matchAll(/(?<![\d.])v?(0\.\d+\.\d+)(?![\d.])/g)) {
          const version = match[1];
          if (version !== expectedVersion) {
            mismatches.push(`${file}: ${line.trim()}`);
          }
        }
      }
    }

    expect(mismatches).toEqual([]);
  });

  test('keeps public docs and installer version references synchronized', () => {
    const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as { version?: string };
    const expectedVersion = packageJson.version;
    const mismatches: string[] = [];
    const files = [
      ...listFiles('docs', new Set(['.md', '.html'])),
      ...listFiles('backups', new Set(['.json'])),
      '.github/ISSUE_TEMPLATE/bug_report.md',
      'install.sh',
      'README.md',
    ].filter((file) => existsSync(file));

    for (const file of files) {
      const source = readFileSync(file, 'utf8');
      for (const line of source.split(/\r?\n/)) {
        if (/Node\.js/i.test(line)) {
          continue;
        }

        for (const match of line.matchAll(/(?<![\d.])v?(0\.\d+\.\d+)(?![\d.])/g)) {
          const version = match[1];
          if (version !== expectedVersion) {
            mismatches.push(`${file}: ${line.trim()}`);
          }
        }
      }
    }

    expect(mismatches).toEqual([]);
  });
});

function listFiles(root: string, extensions: Set<string>): string[] {
  if (!existsSync(root)) return [];

  const files: string[] = [];

  for (const entry of readdirSync(root)) {
    if (entry === '.git' || entry === '.npm-cache' || entry === 'dist' || entry === 'node_modules') {
      continue;
    }

    const fullPath = join(root, entry);
    const stat = statSync(fullPath);
    if (stat.isDirectory()) {
      files.push(...listFiles(fullPath, extensions));
      continue;
    }

    if (extensions.has(extname(entry))) {
      files.push(fullPath);
    }
  }

  return files;
}
