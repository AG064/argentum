// SPDX-License-Identifier: MIT
'use strict';

const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const packageVersion = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version;

function directoryBytes(target, ignoredNames = new Set()) {
  if (!fs.existsSync(target)) return 0;
  let total = 0;
  const pending = [target];
  while (pending.length) {
    const current = pending.pop();
    let entries;
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (entry.isSymbolicLink() || ignoredNames.has(entry.name)) continue;
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(fullPath);
      } else if (entry.isFile()) {
        try {
          total += fs.statSync(fullPath).size;
        } catch {
          // A concurrently removed build file is ignored.
        }
      }
    }
  }
  return total;
}

function fileBytes(target) {
  try {
    return fs.statSync(target).size;
  } catch {
    return 0;
  }
}

function mib(bytes) {
  return Number((bytes / 1024 / 1024).toFixed(2));
}

const generatedNames = new Set([
  '.git',
  'node_modules',
  'target',
  'dist',
  'build',
  '.gradle',
  '.cache',
  '.pkg-cache',
  '.npm-cache',
  '.localappdata',
  'artifacts',
  'data',
  'backups',
]);
const sourceOnly = directoryBytes(root, generatedNames);
const nodeModules = directoryBytes(path.join(root, 'node_modules'));
const rustTarget = directoryBytes(path.join(root, 'src', 'desktop', 'target'));
const git = directoryBytes(path.join(root, '.git'));
const dist = directoryBytes(path.join(root, 'dist'));
const androidBuild = directoryBytes(path.join(root, 'android', 'app', 'build'));
const androidGradle = directoryBytes(path.join(root, 'android', '.gradle'));
const llamaPrepared = directoryBytes(
  path.join(root, 'src', 'ui', 'desktop', 'llama.cpp', 'x86_64-pc-windows-msvc'),
);
const localCaches =
  directoryBytes(path.join(root, '.cache')) +
  directoryBytes(path.join(root, '.pkg-cache')) +
  directoryBytes(path.join(root, '.npm-cache')) +
  directoryBytes(path.join(root, '.localappdata'));
const artifacts = directoryBytes(path.join(root, 'artifacts'));
const runtimeData =
  directoryBytes(path.join(root, 'data')) + directoryBytes(path.join(root, 'backups'));
const totalWorkspace = directoryBytes(root);

const desktopExe = fileBytes(
  path.join(root, 'src', 'desktop', 'target', 'release', 'argentum-desktop.exe'),
);
const sidecarExe = Math.max(
  fileBytes(path.join(root, 'src', 'desktop', 'target', 'release', 'argentum-cli.exe')),
  fileBytes(
    path.join(root, 'src', 'desktop', 'binaries', 'argentum-cli-x86_64-pc-windows-msvc.exe'),
  ),
);
const windowsMsi = fileBytes(
  path.join(
    root,
    'src',
    'desktop',
    'target',
    'release',
    'bundle',
    'msi',
    `Argentum_${packageVersion}_x64_en-US.msi`,
  ),
);
const windowsNsis = fileBytes(
  path.join(
    root,
    'src',
    'desktop',
    'target',
    'release',
    'bundle',
    'nsis',
    `Argentum_${packageVersion}_x64-setup.exe`,
  ),
);

const report = {
  measuredAt: new Date().toISOString(),
  root,
  categoriesMiB: {
    sourceCheckoutExcludingGenerated: mib(sourceOnly),
    nodeModules: mib(nodeModules),
    rustTarget: mib(rustTarget),
    gitMetadata: mib(git),
    dist: mib(dist),
    androidBuild: mib(androidBuild),
    androidGradle: mib(androidGradle),
    preparedOptionalLlamaCppWindows: mib(llamaPrepared),
    localBuildAndPackageCaches: mib(localCaches),
    retainedArtifacts: mib(artifacts),
    runtimeDataAndBackups: mib(runtimeData),
    totalCurrentWorkspace: mib(totalWorkspace),
  },
  releaseArtifactsMiB: {
    windowsMsi: mib(windowsMsi),
    windowsNsis: mib(windowsNsis),
    desktopExecutable: mib(desktopExe),
    cliSidecar: mib(sidecarExe),
    estimatedUnpackedCorePayload: mib(desktopExe + sidecarExe),
    estimatedUnpackedWithOptionalLlamaCpp: mib(desktopExe + sidecarExe + llamaPrepared),
  },
  notes: [
    'Source size excludes .git, node_modules, Rust target, dist, Android build, and Gradle caches.',
    'The installed payload values are component sums; use an administrative MSI extraction for the clean installed measurement.',
    'The optional llama.cpp value is the Windows x64 payload copied by the current NSIS hook, not duplicate or other-platform preparation directories.',
    'Optional model weights and user workspaces are excluded because their size is user-selected.',
  ],
};

console.log(JSON.stringify(report, null, 2));
