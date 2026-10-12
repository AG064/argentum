[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$TargetDirectory = '',
    [ValidateRange(1, 8)][int]$Jobs = 1,
    [switch]$Extensions
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Windows MSVC packaging requires PowerShell 7 on Windows.' }
$sourceRoot = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (-not [IO.Path]::IsPathFullyQualified($OutputDirectory) -or
    $output.Equals($sourceRoot, [StringComparison]::OrdinalIgnoreCase) -or
    $output.StartsWith($sourceRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Choose a new absolute output directory outside the source checkout.'
}
if (Test-Path -LiteralPath $output) { throw 'Output already exists; preserve it and choose a new directory.' }
Push-Location $sourceRoot
try {
    $dirty = @(& git status --porcelain=v1 --untracked-files=all)
    if ($LASTEXITCODE -ne 0 -or $dirty.Count -ne 0) { throw 'Package only a clean committed checkout.' }
    $commit = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Could not resolve source commit.' }
    $tree = (& git rev-parse 'HEAD^{tree}').Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Could not resolve source tree.' }
    $rustc = (& rustc --version).Trim()
    $cargo = (& cargo --version).Trim()
    New-Item -ItemType Directory -Path $output | Out-Null
    $targetArgs = @()
    $featureArgs = if ($Extensions) { @('--features','argentum-cli/extensions') } else { @() }
    if ($TargetDirectory) {
        if (-not [IO.Path]::IsPathFullyQualified($TargetDirectory)) { throw 'Target directory must be absolute.' }
        $targetArgs = @('--target-dir', [IO.Path]::GetFullPath($TargetDirectory))
    }
    $previousDebug = [Environment]::GetEnvironmentVariable('CARGO_PROFILE_RELEASE_DEBUG', 'Process')
    $previousIncremental = [Environment]::GetEnvironmentVariable('CARGO_INCREMENTAL', 'Process')
    try {
        $env:CARGO_PROFILE_RELEASE_DEBUG = '0'
        $env:CARGO_INCREMENTAL = '0'
        & cargo build -p argentum-cli --bin argentum --release --locked -j $Jobs @featureArgs @targetArgs *> (Join-Path $output 'build.log')
        if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed. Retain build.log.' }
    } finally {
        [Environment]::SetEnvironmentVariable('CARGO_PROFILE_RELEASE_DEBUG', $previousDebug, 'Process')
        [Environment]::SetEnvironmentVariable('CARGO_INCREMENTAL', $previousIncremental, 'Process')
    }
    $metadataText = & cargo metadata --locked --format-version 1 --filter-platform x86_64-pc-windows-msvc @featureArgs
    if ($LASTEXITCODE -ne 0) { throw 'Locked dependency metadata failed.' }
    $metadata = $metadataText | ConvertFrom-Json -Depth 100
    $cli = $metadata.packages | Where-Object name -eq 'argentum-cli'
    $version = $cli.version
    $suffix = if ($Extensions) { '-extensions' } else { '' }
    $package = Join-Path $output "argentum-$version-windows-x64$suffix"
    New-Item -ItemType Directory -Path $package | Out-Null
    $targetRoot = if ($TargetDirectory) { [IO.Path]::GetFullPath($TargetDirectory) } else { $metadata.target_directory }
    Copy-Item -LiteralPath (Join-Path $targetRoot 'release/argentum.exe') -Destination (Join-Path $package 'argentum.exe')
    foreach ($relative in @('LICENSE', 'THIRD_PARTY_NOTICES.md', 'README.md')) {
        Copy-Item -LiteralPath (Join-Path $sourceRoot $relative) -Destination (Join-Path $package $relative)
    }
    New-Item -ItemType Directory -Path (Join-Path $package 'docs') | Out-Null
    foreach ($relative in @('CLI_GUIDE.md', 'CHATGPT_SIGN_IN.md', 'SUPPORT_MATRIX.md', 'MIGRATION_NATIVE.md', 'NATIVE_DELIVERY.md')) {
        Copy-Item -LiteralPath (Join-Path $sourceRoot "docs/$relative") -Destination (Join-Path $package "docs/$relative")
    }
    if ($Extensions) {
        foreach ($relative in @(& git ls-files -- docs)) {
            $destination = Join-Path $package $relative
            New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $sourceRoot $relative) -Destination $destination
        }
        foreach ($relative in @('ROADMAP.md','SECURITY.md','CONTRIBUTING.md')) {
            Copy-Item -LiteralPath (Join-Path $sourceRoot $relative) -Destination (Join-Path $package $relative)
        }
        $browser = Join-Path $package 'extensions/browser'
        New-Item -ItemType Directory -Path $browser -Force | Out-Null
        foreach ($relative in @('adapter.mjs','package.json','package-lock.json')) {
            Copy-Item -LiteralPath (Join-Path $sourceRoot "extensions/browser/$relative") -Destination (Join-Path $browser $relative)
        }
        # Node, browser binaries, profiles, definitions and state are separate
        # reviewed owner preparation, never silently bundled or started.
    }
    # Retain actual license/notice bytes for the selected CLI dependency closure.
    # Build dependencies are included conservatively; dev-only and unrelated UI
    # workspace packages are not described as linked CLI runtime components.
    $pending = [Collections.Generic.Queue[string]]::new()
    $selected = [Collections.Generic.HashSet[string]]::new()
    $pending.Enqueue($cli.id)
    while ($pending.Count) {
        $id = $pending.Dequeue()
        if (-not $selected.Add($id)) { continue }
        $node = $metadata.resolve.nodes | Where-Object id -eq $id
        foreach ($dep in $node.deps) {
            if (@($dep.dep_kinds | Where-Object kind -ne 'dev').Count) { $pending.Enqueue($dep.pkg) }
        }
    }
    $notices = @()
    foreach ($dependency in $metadata.packages | Sort-Object name, version) {
        if (-not $selected.Contains($dependency.id) -or -not $dependency.source) { continue }
        $directory = Split-Path -Parent $dependency.manifest_path
        $destination = Join-Path $package "licenses/$($dependency.name)-$($dependency.version)"
        $files = @(Get-ChildItem -LiteralPath $directory -File | Where-Object Name -Match '^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)([._-]|$)')
        if ($dependency.license_file) {
            $declared = [IO.Path]::GetFullPath((Join-Path $directory $dependency.license_file))
            if (-not $declared.StartsWith($directory.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Dependency license path escapes its source package.'
            }
            if (Test-Path -LiteralPath $declared -PathType Leaf) { $files += Get-Item -LiteralPath $declared }
        }
        $files = @($files | Sort-Object FullName -Unique)
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        if (-not $files.Count) {
            # Exact crate archives can omit license text. Retain the pinned
            # upstream terms when present; use the already declared Apache
            # selection only for the two existing audited dual-license entries.
            $ledger = Get-Content -Raw -LiteralPath (Join-Path $sourceRoot 'third-party/missing-license-texts.json') | ConvertFrom-Json
            $entry = @($ledger.packages | Where-Object { $_.name -eq $dependency.name -and $_.version -eq $dependency.version })
            if ($entry.Count -ne 1 -or $entry[0].declared -ne $dependency.license) {
                throw "No retained license text for $($dependency.name) $($dependency.version)."
            }
            $vcs = Get-Content -Raw -LiteralPath (Join-Path $directory '.cargo_vcs_info.json') | ConvertFrom-Json
            if ($vcs.git.sha1 -ne $entry[0].vcs) { throw 'License declaration source revision mismatch.' }
            $specificText = $entry[0].PSObject.Properties['text']
            $text = if ($specificText) { $specificText.Value } else { $ledger.text }
            $textHash = if ($specificText) { $entry[0].text_sha256 } else { $ledger.text_sha256 }
            $termsRoot = [IO.Path]::GetFullPath((Join-Path $sourceRoot 'third-party'))
            $terms = [IO.Path]::GetFullPath((Join-Path $termsRoot $text))
            if (-not $terms.StartsWith($termsRoot.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) -or
                -not (Test-Path -LiteralPath $terms -PathType Leaf) -or
                ((Get-Item -LiteralPath $terms).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                throw 'Retained dependency terms must be a regular contained first-party ledger file.'
            }
            if ((Get-FileHash -LiteralPath $terms -Algorithm SHA256).Hash.ToLowerInvariant() -ne $textHash) {
                throw 'Retained standard license text identity mismatch.'
            }
            $licenseName = if ($specificText) { 'LICENSE-UPSTREAM' } else { 'LICENSE-APACHE' }
            Copy-Item -LiteralPath $terms -Destination (Join-Path $destination $licenseName)
            Copy-Item -LiteralPath $dependency.manifest_path -Destination (Join-Path $destination 'Cargo.toml.upstream')
            Copy-Item -LiteralPath (Join-Path $directory 'README.md') -Destination (Join-Path $destination 'README.upstream.md')
            $entry[0] | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $destination 'license-selection.json') -Encoding utf8NoBOM
        }
        foreach ($file in $files) { Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $destination $file.Name) }
        $notices += [ordered]@{ name=$dependency.name; version=$dependency.version; license=$dependency.license; source=$dependency.source }
    }
    $notices | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $package 'dependency-notices.json') -Encoding utf8NoBOM
    $sourceInputs = @(& git ls-files | Sort-Object | ForEach-Object {
        [ordered]@{ path=$_; sha256=(Get-FileHash -LiteralPath (Join-Path $sourceRoot $_) -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    $sourceInputs | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $package 'source-inputs.json') -Encoding utf8NoBOM
    $binaryHash = (Get-FileHash -LiteralPath (Join-Path $package 'argentum.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    $provenance = [ordered]@{
        schema_version=1; product='Argentum native CLI'; version=$version
        repository='https://github.com/AG064/argentum'; commit=$commit; tree=$tree
        target='x86_64-pc-windows-msvc'; rustc=$rustc; cargo=$cargo
        build_arguments=@('build','-p','argentum-cli','--bin','argentum','--release','--locked','-j',[string]$Jobs) + $featureArgs
        features=if ($Extensions) { @('extensions') } else { @() }
        optional_browser_adapter_included=[bool]$Extensions
        profile_environment=@{CARGO_PROFILE_RELEASE_DEBUG='0';CARGO_INCREMENTAL='0'}
        executable_sha256=$binaryHash; signed=$false; published=$false
        credentials_included=$false; stores_included=$false; models_included=$false
    }
    $provenance | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $package 'provenance.json') -Encoding utf8NoBOM
    if ((& git rev-parse HEAD).Trim() -ne $commit -or @(& git status --porcelain=v1 --untracked-files=all).Count) {
        throw 'Source changed during packaging.'
    }
    $checksums = Get-ChildItem -LiteralPath $package -File -Recurse | Sort-Object FullName | ForEach-Object {
        $relative = [IO.Path]::GetRelativePath($package, $_.FullName).Replace('\', '/')
        "$( (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() )  $relative"
    }
    $checksums | Set-Content -LiteralPath (Join-Path $package 'SHA256SUMS') -Encoding ascii
    $archive = "$package.zip"
    Compress-Archive -LiteralPath $package -DestinationPath $archive
    "$((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($archive))" |
        Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS') -Encoding ascii
    [pscustomobject]@{ package=$package; archive=$archive; executable_sha256=$binaryHash; source_commit=$commit }
} finally { Pop-Location }
