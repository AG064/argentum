import { readFileSync } from 'fs';

describe('Windows release packaging', () => {
  test('brands the optional packaged CLI executable before smoke testing', () => {
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
    expect(script).toContain('argentum-cli-v$version-win-x64.exe');
    expect(script).not.toContain('argentum-v$version-win-x64.exe');
    expect(script).not.toContain('--exe",\n    $portableExePath');
    expect(script).toContain('--icon');
    expect(script).toContain('--version');

    const helper = readFileSync('scripts/patch-windows-exe.js', 'utf8');
    expect(helper).toContain("const rcedit = require('rcedit')");
    expect(helper).toContain("'version-string'");
    expect(helper).toContain('ProductName');
    expect(helper).toContain('Argentum');
  });

  test('release workflow does not publish CLI executables as the graphical Windows installer', () => {
    const releaseWorkflow = readFileSync('.github/workflows/release.yml', 'utf8');
    const desktopWorkflow = readFileSync('.github/workflows/desktop.yml', 'utf8');

    expect(releaseWorkflow).not.toContain('Windows MSI installer');
    expect(releaseWorkflow).not.toContain('argentum-v*-win-x64*.exe');
    expect(releaseWorkflow).not.toContain('argentum-v*-win-x64.msi');
    expect(releaseWorkflow).toContain(
      'argentum-cli-v${{ steps.package.outputs.version }}-win-x64.exe',
    );
    expect(releaseWorkflow).toContain('artifacts/release/argentum-cli-v*-win-x64.exe');

    expect(desktopWorkflow).toContain('src/desktop/target/release/bundle/**/*-setup.exe');
    expect(desktopWorkflow).toContain('src/desktop/target/release/bundle/**/*.msi');
    expect(desktopWorkflow).toContain(
      '/^Argentum[_-]\\d+\\.\\d+\\.\\d+.*\\.(?:AppImage|deb|rpm|dmg|msi|exe)$/',
    );
    expect(desktopWorkflow).not.toContain('/^argentum-v\\d+\\.\\d+\\.\\d+-win-x64');
  });
});
