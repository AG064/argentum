#!/usr/bin/env node
const { existsSync, mkdirSync } = require('fs');
const { join } = require('path');
const { spawnSync } = require('child_process');

const root = join(__dirname, '..');
const distCli = join(root, 'dist', 'cli.js');
const binariesDir = join(root, 'src', 'desktop', 'binaries');

const targets = {
  'win32-x64': {
    pkg: 'node18-win-x64',
    triple: 'x86_64-pc-windows-msvc',
    extension: '.exe',
  },
  'linux-x64': {
    pkg: 'node18-linux-x64',
    triple: 'x86_64-unknown-linux-gnu',
    extension: '',
  },
  'darwin-x64': {
    pkg: 'node18-macos-x64',
    triple: 'x86_64-apple-darwin',
    extension: '',
  },
  'darwin-arm64': {
    pkg: 'node18-macos-arm64',
    triple: 'aarch64-apple-darwin',
    extension: '',
  },
};

function run(command, args) {
  const isWindowsCmd = process.platform === 'win32' && /\.(cmd|bat)$/i.test(command);
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: 'inherit',
    shell: isWindowsCmd,
  });

  if (result.error) {
    console.error(`[build-desktop-sidecar] Failed to run ${command}: ${result.error.message}`);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status || 1);
  }
}

if (!existsSync(distCli)) {
  run(process.platform === 'win32' ? 'npm.cmd' : 'npm', ['run', 'build']);
}

const target = targets[`${process.platform}-${process.arch}`];
if (!target) {
  throw new Error(`Unsupported desktop sidecar host: ${process.platform}-${process.arch}`);
}

mkdirSync(binariesDir, { recursive: true });

const output = join(binariesDir, `argentum-cli-${target.triple}${target.extension}`);
run(process.platform === 'win32' ? 'npx.cmd' : 'npx', [
  '--yes',
  '@yao-pkg/pkg@6.6.0',
  '--config',
  'package.json',
  'dist/cli.js',
  '--targets',
  target.pkg,
  '--output',
  output,
  '--compress',
  'GZip',
  '--no-bytecode',
  '--public-packages',
  '*',
  '--public',
]);

console.log(`Desktop sidecar written to ${output}`);
