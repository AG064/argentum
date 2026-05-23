#!/usr/bin/env node
const {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} = require('fs');
const { execFileSync } = require('child_process');
const https = require('https');
const { basename, dirname, join, resolve } = require('path');

const root = join(__dirname, '..');
const source = process.env.LLAMA_SERVER_BIN ? resolve(process.env.LLAMA_SERVER_BIN) : '';
const cacheDir = join(root, '.cache', 'llama.cpp');

const targets = {
  'win32-x64': {
    triple: 'x86_64-pc-windows-msvc',
    extension: '.exe',
    assetPattern: /^llama-.+-bin-win-cpu-x64\.zip$/,
    serverNames: ['llama-server.exe'],
  },
  'win32-arm64': {
    triple: 'aarch64-pc-windows-msvc',
    extension: '.exe',
    assetPattern: /^llama-.+-bin-win-cpu-arm64\.zip$/,
    serverNames: ['llama-server.exe'],
  },
  'linux-x64': {
    triple: 'x86_64-unknown-linux-gnu',
    extension: '',
    assetPattern: /^llama-.+-bin-ubuntu-x64\.tar\.gz$/,
    serverNames: ['llama-server'],
  },
  'linux-arm64': {
    triple: 'aarch64-unknown-linux-gnu',
    extension: '',
    assetPattern: /^llama-.+-bin-ubuntu-arm64\.tar\.gz$/,
    serverNames: ['llama-server'],
  },
  'darwin-x64': {
    triple: 'x86_64-apple-darwin',
    extension: '',
    assetPattern: /^llama-.+-bin-macos-x64\.tar\.gz$/,
    serverNames: ['llama-server'],
  },
  'darwin-arm64': {
    triple: 'aarch64-apple-darwin',
    extension: '',
    assetPattern: /^llama-.+-bin-macos-arm64\.tar\.gz$/,
    serverNames: ['llama-server'],
  },
};

const target = targets[`${process.platform}-${process.arch}`];
if (!target) {
  console.log(
    `[prepare-llama-server] Unsupported host ${process.platform}-${process.arch}; skipping.`,
  );
  process.exit(0);
}

const brandedName = `argentum-llama-server-${target.triple}${target.extension}`;
const frontendDir = join(root, 'src', 'ui', 'desktop', 'llama.cpp', target.triple);
const binariesDir = join(root, 'src', 'desktop', 'binaries', 'llama.cpp', target.triple);
const installerHookPath = join(root, 'src', 'desktop', 'generated', 'optional-llama.nsh');

main().catch((error) => {
  console.error(`[prepare-llama-server] ${error.message}`);
  process.exit(1);
});

async function main() {
  writeOptionalInstallerHook(false);
  mkdirSync(frontendDir, { recursive: true });
  mkdirSync(binariesDir, { recursive: true });

  if (source) {
    if (!existsSync(source) || !statSync(source).isFile()) {
      throw new Error(`LLAMA_SERVER_BIN does not point to a file: ${source}`);
    }

    installFromDirectory(dirname(source), source, 'LLAMA_SERVER_BIN');
    return;
  }

  if (process.env.ARGENTUM_LLAMA_SERVER_DOWNLOAD === '0') {
    rmSync(frontendDir, { recursive: true, force: true });
    rmSync(binariesDir, { recursive: true, force: true });
    console.log(
      '[prepare-llama-server] ARGENTUM_LLAMA_SERVER_DOWNLOAD=0; llama.cpp server will not be offered by the installer.',
    );
    return;
  }

  const asset = await resolveReleaseAsset();
  const archive = join(cacheDir, asset.name);
  mkdirSync(cacheDir, { recursive: true });

  if (!existsSync(archive)) {
    console.log(`[prepare-llama-server] Downloading ${asset.name}`);
    await download(asset.browser_download_url, archive);
  } else {
    console.log(`[prepare-llama-server] Using cached ${asset.name}`);
  }

  const extractDir = join(
    cacheDir,
    asset.name.replace(/[^a-zA-Z0-9_.-]/g, '_').replace(/\.(zip|tar\.gz)$/, ''),
  );
  rmSync(extractDir, { recursive: true, force: true });
  mkdirSync(extractDir, { recursive: true });
  extractArchive(archive, extractDir);

  const server = findServerBinary(extractDir);
  if (!server) {
    throw new Error(`Downloaded ${asset.name}, but no llama-server binary was found inside it.`);
  }

  installFromDirectory(dirname(server), server, asset.name);
}

async function resolveReleaseAsset() {
  const tag = process.env.LLAMA_CPP_RELEASE_TAG;
  const releaseUrl = tag
    ? `https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/${encodeURIComponent(tag)}`
    : 'https://api.github.com/repos/ggml-org/llama.cpp/releases/latest';
  const release = await getJson(releaseUrl);
  const assets = Array.isArray(release.assets) ? release.assets : [];
  const asset = assets.find((item) => target.assetPattern.test(item.name));

  if (!asset) {
    throw new Error(
      `No llama.cpp release asset matched ${target.assetPattern} for ${process.platform}-${process.arch}. Set LLAMA_SERVER_BIN to a vetted llama-server binary.`,
    );
  }

  return asset;
}

function getJson(url) {
  return new Promise((resolveJson, reject) => {
    const request = https.get(
      url,
      {
        headers: {
          'User-Agent': 'ArgentumBuild',
          'Accept': 'application/vnd.github+json',
        },
      },
      (response) => {
        if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
          response.resume();
          getJson(response.headers.location).then(resolveJson, reject);
          return;
        }

        if (response.statusCode !== 200) {
          response.resume();
          reject(new Error(`GitHub release lookup failed with HTTP ${response.statusCode}.`));
          return;
        }

        let body = '';
        response.setEncoding('utf8');
        response.on('data', (chunk) => {
          body += chunk;
        });
        response.on('end', () => {
          try {
            resolveJson(JSON.parse(body));
          } catch (error) {
            reject(error);
          }
        });
      },
    );
    request.on('error', reject);
  });
}

function download(url, destination) {
  return new Promise((resolveDownload, reject) => {
    const file = require('fs').createWriteStream(destination);
    const request = https.get(url, { headers: { 'User-Agent': 'ArgentumBuild' } }, (response) => {
      if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
        file.close();
        rmSync(destination, { force: true });
        download(response.headers.location, destination).then(resolveDownload, reject);
        return;
      }

      if (response.statusCode !== 200) {
        file.close();
        rmSync(destination, { force: true });
        response.resume();
        reject(new Error(`Download failed with HTTP ${response.statusCode}: ${url}`));
        return;
      }

      response.pipe(file);
      file.on('finish', () => {
        file.close(resolveDownload);
      });
    });
    request.on('error', (error) => {
      file.close();
      rmSync(destination, { force: true });
      reject(error);
    });
  });
}

function extractArchive(archive, destination) {
  if (archive.endsWith('.zip')) {
    if (process.platform === 'win32') {
      execFileSync(
        'powershell',
        [
          '-NoProfile',
          '-ExecutionPolicy',
          'Bypass',
          '-Command',
          `Expand-Archive -LiteralPath '${archive.replaceAll("'", "''")}' -DestinationPath '${destination.replaceAll("'", "''")}' -Force`,
        ],
        { stdio: 'inherit' },
      );
      return;
    }

    execFileSync('unzip', ['-q', archive, '-d', destination], { stdio: 'inherit' });
    return;
  }

  if (archive.endsWith('.tar.gz')) {
    execFileSync('tar', ['-xzf', archive, '-C', destination], { stdio: 'inherit' });
    return;
  }

  throw new Error(`Unsupported llama.cpp archive format: ${archive}`);
}

function findServerBinary(directory) {
  const entries = readdirSync(directory, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = join(directory, entry.name);
    if (entry.isDirectory()) {
      const found = findServerBinary(fullPath);
      if (found) return found;
      continue;
    }

    if (entry.isFile() && target.serverNames.includes(entry.name)) {
      return fullPath;
    }
  }

  return '';
}

function installFromDirectory(sourceDir, serverPath, label) {
  for (const outputDir of [frontendDir, binariesDir]) {
    rmSync(outputDir, { recursive: true, force: true });
    mkdirSync(outputDir, { recursive: true });
    copyDirectory(sourceDir, outputDir);
    const brandedPath = join(outputDir, brandedName);
    copyFileSync(serverPath, brandedPath);
    if (process.platform !== 'win32') chmodSync(brandedPath, 0o755);
    console.log(`[prepare-llama-server] Installed ${label} to ${brandedPath}`);
  }

  writeOptionalInstallerHook(true);
}

function copyDirectory(sourceDir, targetDir) {
  mkdirSync(targetDir, { recursive: true });
  for (const entry of readdirSync(sourceDir, { withFileTypes: true })) {
    const sourcePath = join(sourceDir, entry.name);
    const targetPath = join(targetDir, entry.name);
    if (entry.isDirectory()) {
      copyDirectory(sourcePath, targetPath);
      continue;
    }

    if (entry.isFile()) {
      copyFileSync(sourcePath, targetPath);
      if (process.platform !== 'win32' && target.serverNames.includes(entry.name)) {
        chmodSync(targetPath, 0o755);
      }
    }
  }
}

function nsisPath(value) {
  return value.replaceAll('\\', '\\\\').replaceAll('$', '$$');
}

function writeOptionalInstallerHook(enabled) {
  mkdirSync(dirname(installerHookPath), { recursive: true });

  if (!enabled || !existsSync(frontendDir)) {
    writeFileSync(
      installerHookPath,
      [
        '; Generated by scripts/prepare-llama-server.js.',
        '; llama.cpp binaries are not available for this build, so the setup.exe does not show the optional local-server page.',
        '',
      ].join('\r\n'),
    );
    return;
  }

  const sourceGlob = `${nsisPath(frontendDir)}\\*.*`;
  const installDir = `_up_\\ui\\desktop\\llama.cpp\\${target.triple}`;
  writeFileSync(
    installerHookPath,
    [
      '; Generated by scripts/prepare-llama-server.js.',
      '!include LogicLib.nsh',
      '!include nsDialogs.nsh',
      '',
      'Var ArgentumLlamaCheckbox',
      'Var ArgentumInstallLlamaCpp',
      '',
      'Page custom ArgentumLlamaOptionsPage ArgentumLlamaOptionsLeave',
      '',
      'Function ArgentumLlamaOptionsPage',
      '  ${If} ${Silent}',
      '    Abort',
      '  ${EndIf}',
      '  nsDialogs::Create 1018',
      '  Pop $0',
      '  ${If} $0 == error',
      '    Abort',
      '  ${EndIf}',
      '  ${NSD_CreateLabel} 0 0 100% 18u "Optional local model server"',
      '  Pop $1',
      '  ${NSD_CreateLabel} 0 22u 100% 36u "Install Argentum llama.cpp binaries now. If you skip this, Argentum can install them later when you enable Local Server inside the app."',
      '  Pop $2',
      '  ${NSD_CreateCheckbox} 0 66u 100% 14u "Install Argentum llama.cpp local server binaries"',
      '  Pop $ArgentumLlamaCheckbox',
      '  ${NSD_SetState} $ArgentumLlamaCheckbox ${BST_UNCHECKED}',
      '  nsDialogs::Show',
      'FunctionEnd',
      '',
      'Function ArgentumLlamaOptionsLeave',
      '  ${NSD_GetState} $ArgentumLlamaCheckbox $ArgentumInstallLlamaCpp',
      'FunctionEnd',
      '',
      '!macro NSIS_HOOK_POSTINSTALL',
      '  ${If} $ArgentumInstallLlamaCpp == ${BST_CHECKED}',
      `    SetOutPath "$INSTDIR\\${installDir}"`,
      `    File /r "${sourceGlob}"`,
      '    DetailPrint "Installed Argentum llama.cpp local server binaries."',
      '  ${Else}',
      '    DetailPrint "Skipped Argentum llama.cpp local server binaries. Enable Local Server inside Argentum to install later."',
      '  ${EndIf}',
      '!macroend',
      '',
      '!macro NSIS_HOOK_PREUNINSTALL',
      '  RMDir /r "$INSTDIR\\_up_\\ui\\desktop\\llama.cpp"',
      '!macroend',
      '',
    ].join('\r\n'),
  );
}
