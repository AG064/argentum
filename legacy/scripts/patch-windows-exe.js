#!/usr/bin/env node

const rcedit = require('rcedit');

function getArg(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

async function main() {
  const exePath = getArg('--exe');
  const iconPath = getArg('--icon');
  const version = getArg('--version');

  if (!exePath || !iconPath || !version) {
    throw new Error('Usage: node scripts/patch-windows-exe.js --exe <path> --icon <path> --version <x.y.z>');
  }

  await rcedit(exePath, {
    icon: iconPath,
    'file-version': version,
    'product-version': version,
    'version-string': {
      FileDescription: 'Argentum',
      ProductName: 'Argentum',
      OriginalFilename: 'argentum.exe',
      InternalName: 'argentum',
    },
  });
}

main().catch((err) => {
  console.error(err instanceof Error ? err.message : String(err));
  process.exitCode = 1;
});
