[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$workspaceRoot = Split-Path -Parent $PSScriptRoot
$targetRoot = [System.IO.Path]::GetFullPath((Join-Path $workspaceRoot "target"))
$releaseTargetRoot = [System.IO.Path]::GetFullPath((Join-Path $targetRoot "release-packaging"))
$targetBoundary = $targetRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (-not $releaseTargetRoot.StartsWith($targetBoundary, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Release target directory escaped the workspace target directory"
}
$binaryPath = Join-Path $releaseTargetRoot "release\argentum.exe"
$artifactDirectory = Join-Path $workspaceRoot "artifacts\windows-x64"
$artifactPath = Join-Path $artifactDirectory "Argentum.exe"
$hashPath = Join-Path $artifactDirectory "Argentum.exe.sha256"
$brandIconPath = Join-Path $workspaceRoot "assets\brand\argentum.ico"
$brandManifestPath = Join-Path $workspaceRoot "assets\brand\manifest.json"
$artifactIconPath = Join-Path $artifactDirectory "Argentum.ico"
$artifactBrandManifestPath = Join-Path $artifactDirectory "Argentum.brand.json"
$artifactHashesPath = Join-Path $artifactDirectory "SHA256SUMS.txt"
$designValidationScript = Join-Path $PSScriptRoot "validate-design-system.ps1"

& $designValidationScript

if (Test-Path -LiteralPath $releaseTargetRoot) {
    Remove-Item -LiteralPath $releaseTargetRoot -Recurse -Force
}

Push-Location $workspaceRoot
try {
    cargo build --locked --release -p argentum-app --target-dir $releaseTargetRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo release build failed with exit code $LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
    throw "Release executable was not found at $binaryPath"
}

New-Item -ItemType Directory -Path $artifactDirectory -Force | Out-Null
Copy-Item -LiteralPath $binaryPath -Destination $artifactPath -Force
Copy-Item -LiteralPath $brandIconPath -Destination $artifactIconPath -Force
Copy-Item -LiteralPath $brandManifestPath -Destination $artifactBrandManifestPath -Force

$versionInfo = (Get-Item -LiteralPath $artifactPath).VersionInfo
if ($versionInfo.ProductName -ne "Argentum") {
    throw "Release executable is missing the Argentum product identity resource"
}
if ($versionInfo.FileDescription -ne "Argentum native task workbench") {
    throw "Release executable has an unexpected file description"
}

Add-Type -AssemblyName System.Drawing
$embeddedIcon = [System.Drawing.Icon]::ExtractAssociatedIcon($artifactPath)
if ($null -eq $embeddedIcon) {
    throw "Release executable does not expose an embedded Windows icon"
}
try {
    if ($embeddedIcon.Width -le 0 -or $embeddedIcon.Height -le 0) {
        throw "Embedded Windows icon has invalid geometry"
    }
}
finally {
    $embeddedIcon.Dispose()
}

$hash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath $hashPath -Value "$hash  Argentum.exe" -Encoding ascii
$artifactHashes = @($artifactPath, $artifactIconPath, $artifactBrandManifestPath) |
    ForEach-Object {
        $itemHash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
        "$itemHash  $([System.IO.Path]::GetFileName($_))"
    }
Set-Content -LiteralPath $artifactHashesPath -Value $artifactHashes -Encoding ascii

Write-Output "Artifact: $artifactPath"
Write-Output "SHA256:   $hash"
Write-Output "Identity: embedded icon, $artifactIconPath"
Write-Output "Manifest: $artifactBrandManifestPath"
