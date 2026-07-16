import { existsSync, readdirSync, readFileSync, statSync } from 'fs';
import { extname, join } from 'path';

describe('version synchronization', () => {
  test('has a repository version sync script and CI drift check', () => {
    const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as {
      version?: string;
      scripts?: Record<string, string>;
    };
    const ciWorkflow = readFileSync('.github/workflows/ci.yml', 'utf8');

    expect(typeof packageJson.version).toBe('string');
    expect(packageJson.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(packageJson.scripts?.['version:sync']).toBe('node scripts/sync-version.js');
    expect(packageJson.scripts?.['version:check']).toBe('node scripts/sync-version.js --check');
    expect(existsSync('scripts/sync-version.js')).toBe(true);
    expect(readFileSync('scripts/sync-version.js', 'utf8')).toContain(
      "rewriteJsonVersion('src/desktop/tauri.conf.json')",
    );
    expect(readFileSync('scripts/sync-version.js', 'utf8')).toContain(
      "rewriteTomlVersion('src/desktop/Cargo.toml')",
    );
    expect(readFileSync('scripts/sync-version.js', 'utf8')).toContain(
      "rewriteCargoLockVersion('src/desktop/Cargo.lock')",
    );
    expect(readFileSync('scripts/sync-version.js', 'utf8')).toContain(
      "rewrite('src/ui/desktop/index.html'",
    );
    expect(ciWorkflow).toContain('npm run version:check');

    const cargoLock = readFileSync('src/desktop/Cargo.lock', 'utf8');
    const escapedVersion = packageJson.version?.replace(/\./g, '\\.') ?? '';
    expect(cargoLock).toMatch(
      new RegExp(
        `\\[\\[package\\]\\]\\r?\\nname = "argentum-desktop"\\r?\\nversion = "${escapedVersion}"`,
      ),
    );
  });

  test('keeps product-owned source version literals synchronized', () => {
    const packageJson = JSON.parse(readFileSync('package.json', 'utf8')) as { version?: string };
    const expectedVersion = packageJson.version;
    const mismatches: string[] = [];

    // Match a version literal: 0.0.8 with optional dotted pre-release.
    // We require the version to be followed by either end-of-line, a space,
    // or a non-alphanumeric character (so we don't over-match).
    const versionRe =
      /(?<![\d.])v?(0\.\d+\.\d+(?:-[a-zA-Z0-9]+(?:\.[a-zA-Z0-9]+)*)?)(?![-a-zA-Z0-9.]|\d)/g;

    for (const file of [
      ...listFiles('src', new Set(['.ts', '.js'])),
      ...listFiles('tests', new Set(['.ts', '.js'])),
    ]) {
      const source = readFileSync(file, 'utf8');
      for (const line of source.split(/\r?\n/)) {
        if (
          !/\b(?:version|argentumVersion|agClawVersion|ver):|\bVERSION\s*=|\.version\b/.test(line)
        ) {
          continue;
        }

        for (const match of line.matchAll(versionRe)) {
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
      ...listFiles('docs', new Set(['.md', '.html'])).filter(
        (file) => !file.split(/[\\/]/).includes('releases'),
      ),
      'src/ui/desktop/index.html',
      '.github/ISSUE_TEMPLATE/bug_report.md',
      'install.sh',
      'README.md',
    ].filter((file) => existsSync(file));

    // Match a version literal: 0.0.8 with optional dotted pre-release.
    // We require the version to be followed by either end-of-line, a space,
    // or a non-alphanumeric character (so we don't over-match in things like
    // `argentum-v0.0.8-linux-x64` where the suffix isn't a pre-release).
    const versionRe =
      /(?<![\d.])v?(0\.\d+\.\d+(?:-[a-zA-Z0-9]+(?:\.[a-zA-Z0-9]+)*)?)(?![-a-zA-Z0-9.]|\d)/g;

    for (const file of files) {
      const source = readFileSync(file, 'utf8');
      for (const line of source.split(/\r?\n/)) {
        if (/Node\.js/i.test(line)) {
          continue;
        }

        // Strip backtick-quoted code spans — these are illustrative
        // filenames / commands, not version references.
        const stripped = line.replace(/`[^`]*`/g, '');
        // Strip markdown links — the URL inside [text](url) is not a
        // version reference even if it contains one.
        const stripped2 = stripped.replace(/\[[^\]]*\]\(([^)]+)\)/g, '');
        // Strip inline code fences and HTML tags.
        const cleaned = stripped2.replace(/<[^>]+>/g, '');

        for (const match of cleaned.matchAll(versionRe)) {
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
    if (
      entry === '.git' ||
      entry === '.npm-cache' ||
      entry === 'dist' ||
      entry === 'node_modules' ||
      entry === 'target' ||
      entry === 'build'
    ) {
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
