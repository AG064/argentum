param(
  [string]$OutDir = "artifacts/release",
  [switch]$SkipBuild,
  [switch]$SkipMsi
)

$ErrorActionPreference = "Stop"

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(Mandatory = $true)][string[]]$Arguments
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

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

$version = Get-PackageVersion
$outputDir = Join-Path $root $OutDir
New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

if ([string]::IsNullOrWhiteSpace($env:npm_config_cache)) {
  $env:npm_config_cache = Join-Path $root ".npm-cache"
}
if ([string]::IsNullOrWhiteSpace($env:PKG_CACHE_PATH)) {
  $env:PKG_CACHE_PATH = Join-Path $root ".pkg-cache"
}
if ([string]::IsNullOrWhiteSpace($env:DOTNET_CLI_HOME)) {
  $env:DOTNET_CLI_HOME = Join-Path $root ".dotnet-home"
}
if ([string]::IsNullOrWhiteSpace($env:NUGET_PACKAGES)) {
  $env:NUGET_PACKAGES = Join-Path $root ".nuget-packages"
}
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

$exePath = Join-Path $outputDir "agclaw-v$version-win-x64.exe"
$pkgArgs = @(
  "--yes",
  "@yao-pkg/pkg@6.6.0",
  "--config",
  "package.json",
  "dist/cli.js",
  "--targets",
  "node18-win-x64",
  "--output",
  $exePath,
  "--compress",
  "GZip",
  "--no-bytecode",
  "--public-packages",
  "*",
  "--public"
)

Invoke-Checked "npx.cmd" $pkgArgs
Invoke-Checked $exePath @("--version")

if (-not $SkipMsi) {
  Invoke-Checked "dotnet" @("tool", "restore", "--configfile", (Join-Path $root "NuGet.config"))

  $msiPath = Join-Path $outputDir "agclaw-v$version-win-x64.msi"
  Invoke-Checked "dotnet" @(
    "tool",
    "run",
    "wix",
    "--",
    "build",
    "installer/wix/agclaw.wxs",
    "-arch",
    "x64",
    "-d",
    "ProductVersion=$version",
    "-d",
    "SourceExe=$exePath",
    "-out",
    $msiPath
  )
}

$hashes = Get-ChildItem -Path $outputDir -File |
  Where-Object {
    $_.Extension -in @(".exe", ".msi") -and $_.BaseName -like "agclaw-v$version-*"
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
