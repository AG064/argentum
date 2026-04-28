#!/usr/bin/env node

const { existsSync, readFileSync, writeFileSync } = require('fs');

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

rewrite('package-lock.json', (source) => {
  const lock = JSON.parse(source);
  const rootVersion = lock.packages?.['']?.version;
  if (lock.version === version && rootVersion === version) {
    return source;
  }

  return source
    .replace(/("version":\s*")\d+\.\d+\.\d+(")/, `$1${version}$2`)
    .replace(/("packages":\s*\{\s*"":\s*\{[\s\S]*?"version":\s*")\d+\.\d+\.\d+(")/, `$1${version}$2`);
});

rewrite('src/cli.ts', (source) =>
  source
    .replace(/const VERSION = '\d+\.\d+\.\d+';/, `const VERSION = '${version}';`)
    .replace(/version: '\d+\.\d+\.\d+'/g, `version: '${version}'`),
);

rewrite('src/core/onboarding.ts', (source) =>
  source.replace(/version: '\d+\.\d+\.\d+'/g, `version: '${version}'`),
);

rewrite('README.md', (source) => source.replace(/v\d+\.\d+\.\d+/g, vVersion));
rewrite('docs/RELEASE_PACKAGING.md', (source) => source.replace(/v\d+\.\d+\.\d+/g, vVersion));
rewrite(`docs/releases/${vVersion}.md`, (source) => source.replace(/v\d+\.\d+\.\d+/g, vVersion));

if (changed.length > 0) {
  const message = checkOnly
    ? `Version drift detected. Run npm run version:sync. Files: ${changed.join(', ')}`
    : `Synchronized ${vVersion} in ${changed.join(', ')}`;
  console.log(message);
  if (checkOnly) process.exitCode = 1;
} else {
  console.log(`Version references are synchronized at ${vVersion}.`);
}
