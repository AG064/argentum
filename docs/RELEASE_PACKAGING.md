# Release Packaging

Argentum releases publish GUI desktop installers from the Desktop Builds workflow and optional portable CLI binaries from the Release workflow.

## Release assets

For version `0.0.4`, the desktop workflow produces the user-facing app installers:

- `Argentum_0.0.4_x64-setup.exe` - graphical Windows setup executable
- `Argentum_0.0.4_x64_en-US.msi` - Windows Installer package
- `Argentum_0.0.4_amd64.AppImage` - Linux desktop AppImage
- `Argentum_0.0.4_amd64.deb` - Linux Debian package
- `Argentum-0.0.4-1.x86_64.rpm` - Linux RPM package
- `Argentum_0.0.4_x64.dmg` - macOS Intel desktop package
- `Argentum_0.0.4_aarch64.dmg` - macOS Apple Silicon desktop package

The release workflow also produces optional terminal-first CLI binaries:

- `argentum-cli-v0.0.4-win-x64.exe` - portable Windows CLI executable
- `argentum-v0.0.4-linux-x64` - portable Linux CLI executable
- `argentum-v0.0.4-macos-x64` - portable macOS CLI executable
- `SHA256SUMS-portable.txt` - checksums for portable CLI binaries

The Windows setup executable and MSI are GUI desktop installers. They create normal Windows app entries and should launch the Argentum interface, not the terminal CLI. CLI assets are deliberately named with `argentum-cli-`.

## Creating a GitHub release

1. Make sure `package.json` is at the intended version.
2. Run the checks:

```bash
npm run typecheck
npm test -- --runInBand
```

3. Tag and push the release:

```bash
git tag v0.0.4
git push origin v0.0.4
```

The `Release` workflow builds the artifacts and attaches them to the GitHub release.

## Local Windows packaging

On Windows, run:

```powershell
npm run package:win
```

This builds the GUI desktop app installers through Tauri.

Product icon sources live at `assets/brand/argentum.png` and `installer/wix/argentum.ico`.

For an optional portable CLI executable only:

```powershell
npm run package:win:cli
```

Requirements:

- Node.js 18 or newer
- Rust stable and the Tauri build prerequisites for desktop installers
- Network access the first time Tauri or `@yao-pkg/pkg` dependencies must be downloaded
