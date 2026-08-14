param(
    [string]$OutDir = "artifacts/desktop",
    [switch]$SkipBuild,
    [switch]$SkipTauriBuild,
    [switch]$WithMsi
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

function Run-Checked {
    param([string]$File, [string[]]$CmdArgs)
    & $File @CmdArgs
    if ($LASTEXITCODE -ne 0) { throw "Failed: $File $($CmdArgs -join ' ')" }
}

function Get-PackageVersion {
    $v = node -p "require('./package.json').version"
    if ($LASTEXITCODE -ne 0) { throw "Cannot read package.json version" }
    return $v.Trim()
}

$version = Get-PackageVersion
$outputDir = Join-Path $root $OutDir
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

if (-not $SkipBuild) {
    Run-Checked "npm.cmd" @("run", "package:win")
} elseif (-not $SkipTauriBuild) {
    # Temporarily set targets to nsis only so tauri build skips the slow MSI step.
    # Tauri 2.x rejects "all" as a string value in the schema, so we read it as-is
    # and only modify if it's safe to do so.
    $confFile = Join-Path $root "src\desktop\tauri.conf.json"
    $confOrig = Get-Content $confFile -Raw
    $conf = $confOrig | ConvertFrom-Json
    $targets = $conf.bundle.targets
    $needRestore = $false

    if ($targets -is [string]) {
        # "all" is a plain string — leave it; tauri will use its default behaviour.
        Write-Host "Bundle targets is '$targets', leaving as-is."
    } elseif ($targets -is [System.Array] -and ($targets | Where-Object { $_ -ne "nsis" -and $_ -ne "msi" })) {
        # Contains something other than nsis/msi — restrict to nsis only.
        $conf.bundle.targets = @("nsis")
        $conf | ConvertTo-Json -Depth 10 | Set-Content $confFile -Encoding utf8
        $needRestore = $true
        Write-Host "Restricted bundle targets to [nsis]."
    }

    Push-Location "src\desktop"
    try {
        Run-Checked "npx.cmd" @("tauri", "build")
    } finally {
        Pop-Location
        if ($needRestore) {
            Set-Content -Path $confFile -Value $confOrig -Encoding utf8
        }
    }

    if ($WithMsi) {
        $conf2 = Get-Content $confFile -Raw | ConvertFrom-Json
        $conf2Orig = Get-Content $confFile -Raw
        $conf2.bundle.targets = @("msi")
        $conf2 | ConvertTo-Json -Depth 10 | Set-Content $confFile -Encoding utf8
        Push-Location "src\desktop"
        try {
            Run-Checked "npx.cmd" @("tauri", "build")
        } catch {
            Write-Host "[WARN] MSI build failed - NSIS installer is still available."
        } finally {
            Pop-Location
            Set-Content -Path $confFile -Value $confOrig -Encoding utf8
        }
    }
}

$bundleDir = Join-Path $root "src\desktop\target\release\bundle"
$nsisDir = Join-Path $bundleDir "nsis"
$wixDir = Join-Path $bundleDir "wix"

$artifacts = @()

if (Test-Path $nsisDir) {
    Get-ChildItem -Path $nsisDir -File -Filter "*.exe" | ForEach-Object {
        $dest = Join-Path $outputDir $_.Name
        Copy-Item -LiteralPath $_.FullName -Destination $dest -Force
        $artifacts += $dest
        Write-Host "Copied NSIS: $dest"
    }
}

if (Test-Path $wixDir) {
    Get-ChildItem -Path $wixDir -File -Filter "*.msi" | ForEach-Object {
        $dest = Join-Path $outputDir $_.Name
        Copy-Item -LiteralPath $_.FullName -Destination $dest -Force
        $artifacts += $dest
        Write-Host "Copied MSI: $dest"
    }
}

$rawExe = Join-Path $root "src\desktop\target\release\argentum-desktop.exe"
if (Test-Path $rawExe) {
    $portableName = "Argentum_${version}_x64_portable.exe"
    $dest = Join-Path $outputDir $portableName
    Copy-Item -LiteralPath $rawExe -Destination $dest -Force
    $artifacts += $dest
    Write-Host "Copied portable: $dest"
}

$sidecarExe = Join-Path $root "src\desktop\binaries\argentum-cli-x86_64-pc-windows-msvc.exe"
if (Test-Path $sidecarExe) {
    $dest = Join-Path $outputDir "argentum-cli-x86_64-pc-windows-msvc.exe"
    Copy-Item -LiteralPath $sidecarExe -Destination $dest -Force
    $artifacts += $dest
    Write-Host "Copied sidecar: $dest"
}

$checksums = $artifacts | ForEach-Object {
    $h = Get-FileHash -Algorithm SHA256 -LiteralPath $_
    $h.Hash.ToLowerInvariant() + "  " + (Split-Path $_ -Leaf)
}
$checksums | Set-Content -Path (Join-Path $outputDir "SHA256SUMS.txt") -Encoding ascii

Write-Host ""
Write-Host "Done. Artifacts in: $outputDir"
Get-ChildItem $outputDir -File | Format-Table Name, @{L="SizeMB";E={[math]::Round($_.Length/1MB,2)}}
