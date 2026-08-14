param(
  [string]$OutDir = "artifacts/release",
  [switch]$SkipBuild,
  [switch]$SkipMsi
)

$ErrorActionPreference = "Stop"

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [string[]]$Arguments = @()
  )

  & $FilePath @Arguments
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($Arguments -join ' ')"
  }
}

function Get-PackageVersion {
  $raw = node -p "require('./package.json').version"
  if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($raw)) {
    throw "Unable to read package.json version"
  }
  return $raw.Trim()
}

function Get-PkgFetchedBaseBinary {
  param(
    [Parameter(Mandatory = $true)][string]$CachePath
  )

  return Get-ChildItem -Path $CachePath -Recurse -File -Filter "fetched-v*-win-x64" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
}

function New-BrandedPkgBaseBinary {
  param(
    [Parameter(Mandatory = $true)][System.IO.FileInfo]$SourceBaseBinary,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [Parameter(Mandatory = $true)][string]$IconPath,
    [Parameter(Mandatory = $true)][string]$ProductVersion
  )

  Copy-Item -LiteralPath $SourceBaseBinary.FullName -Destination $OutputPath -Force

  Invoke-Checked "node" @(
    "scripts/patch-windows-exe.js",
    "--exe",
    $OutputPath,
    "--icon",
    $IconPath,
    "--version",
    $ProductVersion
  )
  return $OutputPath
}

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

$version = Get-PackageVersion
$outputDir = Join-Path $root $OutDir
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
Get-ChildItem -Path $outputDir -File -ErrorAction SilentlyContinue |
  Where-Object {
    $_.Name -like "argentum-v$version-*" -or
    $_.Name -like "argentum-cli-v$version-*" -or
    $_.Name -like "agclaw-v$version-*" -or
    $_.Name -like "argentum-test*" -or
    $_.Name -eq "SHA256SUMS.txt"
  } |
  Remove-Item -Force

$env:npm_config_cache = Join-Path $root ".npm-cache"
$env:PKG_CACHE_PATH = Join-Path $root ".pkg-cache"
$env:DOTNET_CLI_HOME = Join-Path $root ".dotnet-home"
$env:NUGET_PACKAGES = Join-Path $root ".nuget-packages"
$env:APPDATA = Join-Path $root ".appdata"
$env:LOCALAPPDATA = Join-Path $root ".localappdata"
$env:DOTNET_SKIP_FIRST_TIME_EXPERIENCE = "1"
$env:DOTNET_CLI_TELEMETRY_OPTOUT = "1"
$env:DOTNET_NOLOGO = "1"
New-Item -ItemType Directory -Force -Path $env:npm_config_cache | Out-Null
New-Item -ItemType Directory -Force -Path $env:PKG_CACHE_PATH | Out-Null
New-Item -ItemType Directory -Force -Path $env:DOTNET_CLI_HOME | Out-Null
New-Item -ItemType Directory -Force -Path $env:NUGET_PACKAGES | Out-Null
New-Item -ItemType Directory -Force -Path $env:APPDATA | Out-Null
New-Item -ItemType Directory -Force -Path $env:LOCALAPPDATA | Out-Null

if (-not $SkipBuild) {
  Invoke-Checked "npm.cmd" @("run", "build")
}

$portableExePath = Join-Path $outputDir "argentum-cli-v$version-win-x64.exe"
$installerExePath = Join-Path $outputDir "argentum-cli-v$version-win-x64-setup.exe"
$licenseRtfPath = Join-Path $root "installer/wix/license.rtf"
$iconIcoPath = Join-Path $root "installer/wix/argentum.ico"
$pkgArgs = @(
  "--yes",
  "@yao-pkg/pkg@6.6.0",
  "--config",
  "package.json",
  "dist/cli.js",
  "--targets",
  "node18-win-x64",
  "--output",
  $portableExePath,
  "--compress",
  "GZip",
  "--no-bytecode",
  "--public-packages",
  "*",
  "--public"
)

$pkgBaseBinary = Get-PkgFetchedBaseBinary -CachePath $env:PKG_CACHE_PATH
if (-not $pkgBaseBinary) {
  Invoke-Checked "npx.cmd" $pkgArgs
  $pkgBaseBinary = Get-PkgFetchedBaseBinary -CachePath $env:PKG_CACHE_PATH
  if (-not $pkgBaseBinary) {
    throw "Unable to locate pkg Windows base binary in $env:PKG_CACHE_PATH"
  }

  Remove-Item -LiteralPath $portableExePath -Force
}

$brandedBasePath = Join-Path $env:PKG_CACHE_PATH "argentum-node-v$version-win-x64.exe"
New-BrandedPkgBaseBinary -SourceBaseBinary $pkgBaseBinary -OutputPath $brandedBasePath -IconPath $iconIcoPath -ProductVersion $version | Out-Null
Write-Host "Using branded pkg base binary: $brandedBasePath"

$previousPkgNodePath = $env:PKG_NODE_PATH
try {
  $env:PKG_NODE_PATH = $brandedBasePath
  Invoke-Checked "npx.cmd" $pkgArgs
} finally {
  $env:PKG_NODE_PATH = $previousPkgNodePath
}
Invoke-Checked $portableExePath @("--version")

$smokeWorkDir = Join-Path $outputDir "smoke-workdir"
if (Test-Path $smokeWorkDir) {
  Remove-Item -LiteralPath $smokeWorkDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $smokeWorkDir | Out-Null
$previousArgentumWorkDir = $env:ARGENTUM_WORKDIR
$previousLegacyWorkDir = $env:AGCLAW_WORKDIR
$previousSkipExitPause = $env:ARGENTUM_SKIP_EXIT_PAUSE
$previousLegacySkipExitPause = $env:AGCLAW_SKIP_EXIT_PAUSE
try {
  $env:ARGENTUM_WORKDIR = $smokeWorkDir
  $env:AGCLAW_WORKDIR = $null
  $env:ARGENTUM_SKIP_EXIT_PAUSE = "1"
  $env:AGCLAW_SKIP_EXIT_PAUSE = "1"
  Invoke-Checked $portableExePath @(
    "onboard",
    "--yes",
    "--provider",
    "nvidia",
    "--model",
    "smoke-model",
    "--port",
    "3133",
    "--with-webchat",
    "--force"
  )
  Invoke-Checked $portableExePath @()
} finally {
  $env:ARGENTUM_WORKDIR = $previousArgentumWorkDir
  $env:AGCLAW_WORKDIR = $previousLegacyWorkDir
  $env:ARGENTUM_SKIP_EXIT_PAUSE = $previousSkipExitPause
  $env:AGCLAW_SKIP_EXIT_PAUSE = $previousLegacySkipExitPause
}

if (-not $SkipMsi) {
  Invoke-Checked "dotnet" @("tool", "restore", "--configfile", (Join-Path $root "NuGet.config"))
  Invoke-Checked "dotnet" @("tool", "run", "wix", "--", "extension", "add", "WixToolset.UI.wixext/6.0.2")
  Invoke-Checked "dotnet" @("tool", "run", "wix", "--", "extension", "add", "WixToolset.Util.wixext/6.0.2")
  Invoke-Checked "dotnet" @("tool", "run", "wix", "--", "extension", "add", "WixToolset.Bal.wixext/6.0.2")

  $msiPath = Join-Path $outputDir "argentum-cli-v$version-win-x64.msi"
  Invoke-Checked "dotnet" @(
    "tool",
    "run",
    "wix",
    "--",
    "build",
    "installer/wix/argentum.wxs",
    "-arch",
    "x64",
    "-d",
    "ProductVersion=$version",
    "-d",
    "SourceExe=$portableExePath",
    "-d",
    "LicenseRtf=$licenseRtfPath",
    "-d",
    "IconIco=$iconIcoPath",
    "-ext",
    "WixToolset.UI.wixext",
    "-ext",
    "WixToolset.Util.wixext",
    "-out",
    $msiPath
  )

  $balExtension = Get-ChildItem -Path (Join-Path $root ".wix/extensions/WixToolset.Bal.wixext/6.0.2/wixext6") -Filter "*.dll" |
    Select-Object -First 1
  if (-not $balExtension) {
    throw "Unable to locate WixToolset.Bal.wixext after extension restore"
  }

  Invoke-Checked "dotnet" @(
    "tool",
    "run",
    "wix",
    "--",
    "build",
    "installer/wix/argentum-bundle.wxs",
    "-d",
    "ProductVersion=$version",
    "-d",
    "SourceMsi=$msiPath",
    "-d",
    "LicenseRtf=$licenseRtfPath",
    "-d",
    "IconIco=$iconIcoPath",
    "-ext",
    $balExtension.FullName,
    "-out",
    $installerExePath
  )
}

$hashes = Get-ChildItem -Path $outputDir -File |
  Where-Object {
    $_.Extension -in @(".exe", ".msi") -and $_.BaseName -like "argentum-cli-v$version-*"
  } |
  Sort-Object Name |
  ForEach-Object {
    $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName
    "$($hash.Hash.ToLowerInvariant())  $($_.Name)"
  }

$checksumsPath = Join-Path $outputDir "SHA256SUMS.txt"
$hashes | Set-Content -Path $checksumsPath -Encoding ascii

Write-Host "Release artifacts written to $outputDir"
Get-ChildItem -Path $outputDir -File | Select-Object Name, Length
