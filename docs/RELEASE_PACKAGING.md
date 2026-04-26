# Release Packaging

AG-Claw releases publish portable binaries and a Windows MSI installer.

## Release assets

For version `0.0.1`, the release workflow produces:

- `agclaw-v0.0.1-win-x64.exe` - portable Windows CLI executable
- `agclaw-v0.0.1-win-x64.msi` - Windows installer
- `agclaw-v0.0.1-linux-x64` - portable Linux CLI executable
- `agclaw-v0.0.1-macos-x64` - portable macOS CLI executable
- `SHA256SUMS-portable.txt` and `SHA256SUMS.txt` - checksums

The MSI installs `agclaw.exe` under `Program Files\AG-Claw` and adds that folder to the system `PATH`.

## Creating a GitHub release

1. Make sure `package.json` is at the intended version.
2. Run the checks:

```bash
npm run typecheck
npm test -- --runInBand
```

3. Tag and push the release:

```bash
git tag v0.0.1
git push origin v0.0.1
```

The `Release` workflow builds the artifacts and attaches them to the GitHub release.

## Local Windows packaging

On Windows, run:

```powershell
npm run package:win
```

This builds the CLI, creates the portable `.exe`, installs the WiX CLI if needed, builds the `.msi`, and writes checksums into `artifacts/release`.

For a portable executable only:

```powershell
npm run package:win:exe
```

Requirements for MSI builds:

- Node.js 20 or newer
- .NET SDK 8 or newer
- Network access the first time `@yao-pkg/pkg` or WiX must be downloaded
