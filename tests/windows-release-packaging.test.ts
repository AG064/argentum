import { readFileSync } from 'fs';

describe('Windows release packaging', () => {
  test('brands the packaged executable before smoke testing and MSI packaging', () => {
    const script = readFileSync('scripts/build-windows-release.ps1', 'utf8');

    const packageIndex = script.indexOf('Invoke-Checked "npx.cmd" $pkgArgs');
    const iconPatchIndex = script.indexOf('New-BrandedPkgBaseBinary');
    const smokeIndex = script.indexOf('Invoke-Checked $portableExePath @("--version")');

    expect(packageIndex).toBeGreaterThanOrEqual(0);
    expect(iconPatchIndex).toBeGreaterThanOrEqual(0);
    expect(iconPatchIndex).toBeLessThan(smokeIndex);
    expect(script).toContain('-Filter "fetched-v*-win-x64"');
    expect(script).toContain('PKG_NODE_PATH');
    expect(script).toContain('Using branded pkg base binary');
    expect(script).not.toContain('--exe",\n    $portableExePath');
    expect(script).toContain('--icon');
    expect(script).toContain('--version');

    const helper = readFileSync('scripts/patch-windows-exe.js', 'utf8');
    expect(helper).toContain("const rcedit = require('rcedit')");
    expect(helper).toContain("'version-string'");
    expect(helper).toContain('ProductName');
    expect(helper).toContain('Argentum');
  });
});
