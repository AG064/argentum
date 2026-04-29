#!/usr/bin/env node

const { existsSync, readdirSync, readFileSync, statSync, writeFileSync } = require('fs');
const { extname, join } = require('path');

const checkOnly = process.argv.includes('--check');
const packageJson = JSON.parse(readFileSync('package.json', 'utf8'));
const version = packageJson.version;

if (!/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error(`package.json version must be semver without a leading v, got ${version}`);
}

const vVersion = `v${version}`;
const changed = [];

function rewrite(file, updater) {
  if (!existsSync(file)) return;
  const original = readFileSync(file, 'utf8');
  const next = updater(original);
  if (next === original) return;
  changed.push(file);
  if (!checkOnly) {
    writeFileSync(file, next, 'utf8');
  }
}

function rewriteJsonVersion(file) {
  rewrite(file, (source) => {
    const data = JSON.parse(source);
    if (data.version === version) return source;
    data.version = version;
    return `${JSON.stringify(data, null, 2)}\n`;
  });
}

function rewriteTomlVersion(file) {
  rewrite(file, (source) =>
    source.replace(/^version = "\d+\.\d+\.\d+"/m, `version = "${version}"`),
  );
}

rewrite('package-lock.json', (source) => {
  const lock = JSON.parse(source);
  const rootVersion = lock.packages?.['']?.version;
  if (lock.version === version && rootVersion === version) {
    return source;
  }

  return source
    .replace(/("version":\s*")\d+\.\d+\.\d+(")/, `$1${version}$2`)
    .replace(
      /("packages":\s*\{\s*"":\s*\{[\s\S]*?"version":\s*")\d+\.\d+\.\d+(")/,
      `$1${version}$2`,
    );
});

rewrite('src/cli.ts', (source) =>
  source
    .replace(/const VERSION = '\d+\.\d+\.\d+';/, `const VERSION = '${version}';`)
    .replace(/version: '\d+\.\d+\.\d+'/g, `version: '${version}'`),
);

rewrite('src/core/onboarding.ts', (source) =>
  source.replace(/version: '\d+\.\d+\.\d+'/g, `version: '${version}'`),
);

rewriteJsonVersion('src/desktop/tauri.conf.json');
rewriteTomlVersion('src/desktop/Cargo.toml');
rewrite('src/ui/desktop/index.html', (source) => rewriteDocumentationVersions(source));

for (const file of [
  ...listFiles('src', new Set(['.ts', '.js'])),
  ...listFiles('tests', new Set(['.ts', '.js'])),
]) {
  rewrite(file, (source) => rewriteVersionLines(source));
}

for (const file of [
  ...listFiles('docs', new Set(['.md', '.html'])).filter(
    (file) => !file.split(/[\\/]/).includes('releases'),
  ),
  ...listFiles('backups', new Set(['.json'])),
  '.github/ISSUE_TEMPLATE/bug_report.md',
  'install.sh',
  'README.md',
]) {
  rewrite(file, (source) => rewriteDocumentationVersions(source));
}

if (changed.length > 0) {
  const message = checkOnly
    ? `Version drift detected. Run npm run version:sync. Files: ${changed.join(', ')}`
    : `Synchronized ${vVersion} in ${changed.join(', ')}`;
  console.log(message);
  if (checkOnly) process.exitCode = 1;
} else {
  console.log(`Version references are synchronized at ${vVersion}.`);
}

function listFiles(root, extensions) {
  if (!existsSync(root)) return [];

  const files = [];
  for (const entry of readdirSync(root)) {
    if (entry === 'node_modules' || entry === 'target' || entry === 'dist' || entry === 'build')
      continue;

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

function rewriteVersionLines(source) {
  const newline = source.includes('\r\n') ? '\r\n' : '\n';
  return source
    .split(/\r?\n/)
    .map((line) => {
      if (
        !/\b(?:version|argentumVersion|agClawVersion|ver):|\bVERSION\s*=|\.version\b/.test(line)
      ) {
        return line;
      }

      return line
        .replace(/(['"])v?0\.\d+\.\d+\1/g, (_match, quote) => `${quote}${version}${quote}`)
        .replace(/(?<![\d.])v0\.\d+\.\d+(?!(?:\.\d)|\d)/g, vVersion)
        .replace(/(?<![\d.])0\.\d+\.\d+(?!(?:\.\d)|\d)/g, version);
    })
    .join(newline);
}

function rewriteDocumentationVersions(source) {
  const newline = source.includes('\r\n') ? '\r\n' : '\n';
  return source
    .split(/\r?\n/)
    .map((line) => {
      if (/Node\.js/i.test(line)) {
        return line;
      }

      return line
        .replace(/(?<![\d.])v0\.\d+\.\d+(?!(?:\.\d)|\d)/g, vVersion)
        .replace(/(?<![\d.])0\.\d+\.\d+(?!(?:\.\d)|\d)/g, version);
    })
    .join(newline);
}
