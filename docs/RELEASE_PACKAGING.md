# Release Packaging

Argentum releases publish portable binaries, a Windows MSI installer, and a graphical Windows setup executable.

## Release assets

For version `0.0.3`, the release workflow produces:

- `argentum-v0.0.3-win-x64.exe` - graphical Windows setup executable
- `argentum-v0.0.3-win-x64.msi` - Windows installer
- `argentum-v0.0.3-win-x64-portable.exe` - portable Windows CLI executable
- `argentum-v0.0.3-linux-x64` - portable Linux CLI executable
- `argentum-v0.0.3-macos-x64` - portable macOS CLI executable
- `SHA256SUMS-portable.txt` and `SHA256SUMS.txt` - checksums

The MSI installs `argentum.exe` under `Program Files\Argentum`, adds that folder to the system `PATH`, creates a Start Menu shortcut, creates a desktop shortcut, and uses the Argentum product icon for Add/Remove Programs and shortcuts.

The setup `.exe` is a WiX bundle that delegates the visible wizard to the MSI UI. Users see the normal installer flow: welcome, license agreement, install folder, ready to install, progress, and finish. The finish page offers to launch Argentum.

## Creating a GitHub release

1. Make sure `package.json` is at the intended version.
2. Run the checks:

```bash
npm run typecheck
npm test -- --runInBand
```

3. Tag and push the release:

```bash
git tag v0.0.3
git push origin v0.0.3
```

The `Release` workflow builds the artifacts and attaches them to the GitHub release.

## Local Windows packaging

On Windows, run:

```powershell
npm run package:win
```

This builds the CLI, creates the portable `.exe`, installs the WiX CLI/extensions if needed, builds the `.msi`, wraps it in a graphical setup `.exe`, and writes checksums into `artifacts/release`.

Product icon sources live at `assets/brand/argentum.png` and `installer/wix/argentum.ico`.

For a portable executable only:

```powershell
npm run package:win:exe
```

Requirements for MSI builds:

- Node.js 18 or newer
- .NET SDK 8 or newer
- Network access the first time `@yao-pkg/pkg` or WiX must be downloaded
