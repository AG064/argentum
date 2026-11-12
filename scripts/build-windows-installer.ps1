# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064

<#
.SYNOPSIS
Builds the per-user Windows installer (MSI) from the portable desktop zip and
the CLI zip.

.DESCRIPTION
The installer holds exactly the files of the two zips: the desktop zip goes to
%LOCALAPPDATA%\Programs\Argentum\app and the CLI zip to its cli folder. WiX 5.0.2
is installed into the work directory, so nothing is installed globally. WiX 5 is
used because later WiX versions ask the user to accept an extra EULA.

The script refuses to package llama.cpp or model files.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$DesktopZip,
    [Parameter(Mandatory)][string]$CliZip,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$Version,
    [string]$WorkDirectory = ''
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'The Windows installer must be built on Windows.' }

$wixVersion = '5.0.2'
$sourceRoot = Split-Path -Parent $PSScriptRoot
$wxs = Join-Path $sourceRoot 'platforms/windows/installer/Argentum.wxs'
$icon = Join-Path $sourceRoot 'assets/brand/argentum.ico'

# MSI versions are numeric only, so 0.1.0-rc.1 becomes 0.1.0.
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.]+)?$') { throw "Unexpected version '$Version'." }
$msiVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3])"

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $output -Force | Out-Null
if (-not $WorkDirectory) { $WorkDirectory = Join-Path ([IO.Path]::GetTempPath()) "argentum-installer-$([guid]::NewGuid().ToString('N'))" }
$work = [IO.Path]::GetFullPath($WorkDirectory)
if (Test-Path -LiteralPath $work) { throw 'Work directory already exists; choose a new one.' }
New-Item -ItemType Directory -Path $work | Out-Null

# Keep NuGet and dotnet state inside the work directory unless the caller set it.
if (-not $env:NUGET_PACKAGES) { $env:NUGET_PACKAGES = Join-Path $work 'nuget' }
if (-not $env:NUGET_HTTP_CACHE_PATH) { $env:NUGET_HTTP_CACHE_PATH = Join-Path $work 'nuget-http' }
if (-not $env:NUGET_PLUGINS_CACHE_PATH) { $env:NUGET_PLUGINS_CACHE_PATH = Join-Path $work 'nuget-plugins' }
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'
$env:DOTNET_SKIP_FIRST_TIME_EXPERIENCE = '1'

function Expand-Flat([string]$Zip, [string]$Destination) {
    $temporary = Join-Path $work ("unzip-" + [guid]::NewGuid().ToString('N'))
    Expand-Archive -LiteralPath $Zip -DestinationPath $temporary
    $roots = @(Get-ChildItem -LiteralPath $temporary -Force)
    if ($roots.Count -ne 1 -or -not $roots[0].PSIsContainer) { throw "$Zip must contain exactly one top-level folder." }
    New-Item -ItemType Directory -Path (Split-Path -Parent $Destination) -Force | Out-Null
    Move-Item -LiteralPath $roots[0].FullName -Destination $Destination
}

$payload = Join-Path $work 'payload'
Expand-Flat $DesktopZip $payload
Expand-Flat $CliZip (Join-Path $payload 'cli')
foreach ($required in @('Argentum.exe', 'cli/argentum.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $payload $required) -PathType Leaf)) { throw "Payload is missing $required." }
}
$forbidden = @(Get-ChildItem -LiteralPath $payload -Recurse -File | Where-Object {
    $_.Name -match '(?i)(llama|ggml)[^\\/]*\.(dll|exe|so)$' -or $_.Extension -match '(?i)^\.(gguf|ggml|safetensors|onnx)$'
})
if ($forbidden.Count) { throw "The installer must not contain llama.cpp or models: $($forbidden.Name -join ', ')" }

# Generate one component per directory. Per-user components need an HKCU key
# path, and each directory needs a RemoveFolder entry so uninstall leaves no
# empty folders behind.
$md5 = [Security.Cryptography.MD5]::Create()
function Get-StableGuid([string]$Text) {
    $bytes = $md5.ComputeHash([Text.Encoding]::UTF8.GetBytes("argentum-installer/$Text"))
    $bytes[6] = ($bytes[6] -band 0x0f) -bor 0x30
    $bytes[8] = ($bytes[8] -band 0x3f) -bor 0x80
    ([guid]::new([byte[]]$bytes)).ToString()
}
function Get-StableId([string]$Prefix, [string]$Text) {
    $hash = [Convert]::ToHexString($md5.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text.ToLowerInvariant())))
    "$Prefix$($hash.Substring(0, 24))"
}
function ConvertTo-XmlText([string]$Text) { [Security.SecurityElement]::Escape($Text) }

$directories = [Collections.Generic.List[string]]::new()
$directories.Add('')
foreach ($directory in Get-ChildItem -LiteralPath $payload -Recurse -Directory | Sort-Object FullName) {
    $directories.Add([IO.Path]::GetRelativePath($payload, $directory.FullName).Replace('\', '/'))
}
function Get-DirectoryId([string]$Relative) { if ($Relative) { Get-StableId 'd' $Relative } else { 'INSTALLFOLDER' } }

$tree = [Text.StringBuilder]::new()
function Write-Directory([string]$Relative, [int]$Depth) {
    $children = @($directories | Where-Object {
        $_ -and ((Split-Path -Parent $_).Replace('\', '/') -eq $Relative)
    })
    foreach ($child in $children) {
        $name = ConvertTo-XmlText (Split-Path -Leaf $child)
        $indent = '  ' * $Depth
        [void]$tree.AppendLine("$indent<Directory Id=""$(Get-DirectoryId $child)"" Name=""$name"">")
        Write-Directory $child ($Depth + 1)
        [void]$tree.AppendLine("$indent</Directory>")
    }
}
Write-Directory '' 3

$components = [Text.StringBuilder]::new()
$fileCount = 0
foreach ($relative in $directories) {
    $id = Get-DirectoryId $relative
    $componentId = Get-StableId 'c' "component/$relative"
    $folder = if ($relative) { Join-Path $payload $relative } else { $payload }
    [void]$components.AppendLine("      <Component Id=""$componentId"" Directory=""$id"" Guid=""$(Get-StableGuid "component/$relative")"">")
    foreach ($file in Get-ChildItem -LiteralPath $folder -File | Sort-Object Name) {
        $fileRelative = if ($relative) { "$relative/$($file.Name)" } else { $file.Name }
        $source = ConvertTo-XmlText $file.FullName
        [void]$components.AppendLine("        <File Id=""$(Get-StableId 'f' $fileRelative)"" Name=""$(ConvertTo-XmlText $file.Name)"" Source=""$source"" />")
        $fileCount++
    }
    [void]$components.AppendLine("        <RemoveFolder Id=""$(Get-StableId 'r' $relative)"" On=""uninstall"" />")
    [void]$components.AppendLine("        <RegistryValue Root=""HKCU"" Key=""Software\Argentum\Installer\Components"" Name=""$componentId"" Type=""integer"" Value=""1"" KeyPath=""yes"" />")
    [void]$components.AppendLine('      </Component>')
}

$payloadWxs = Join-Path $work 'Payload.wxs'
@"
<?xml version="1.0" encoding="utf-8"?>
<!-- Generated by scripts/build-windows-installer.ps1. Do not edit. -->
<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Fragment>
    <DirectoryRef Id="INSTALLFOLDER">
$($tree.ToString().TrimEnd())
    </DirectoryRef>
  </Fragment>
  <Fragment>
    <ComponentGroup Id="Payload">
$($components.ToString().TrimEnd())
    </ComponentGroup>
  </Fragment>
</Wix>
"@ | Set-Content -LiteralPath $payloadWxs -Encoding utf8NoBOM

$tools = Join-Path $work 'tools'
& dotnet tool install wix --version $wixVersion --tool-path $tools | Out-Host
if ($LASTEXITCODE -ne 0) { throw 'Installing WiX failed.' }
$wix = Join-Path $tools 'wix.exe'
$installedVersion = (& $wix --version).Trim()
if (-not $installedVersion.StartsWith($wixVersion)) { throw "Expected WiX $wixVersion, got $installedVersion." }

$msiName = "Argentum-$Version-windows-x64.msi"
$msi = Join-Path $output $msiName
Push-Location $work
try {
    # Extensions are cached in .wix under the current folder, the work directory.
    & $wix extension add "WixToolset.Util.wixext/$wixVersion" | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'Adding the WiX Util extension failed.' }
    & $wix build -arch x64 -ext WixToolset.Util.wixext `
        -d "ProductVersion=$msiVersion" -d "DisplayVersion=$Version" -d "IconPath=$icon" `
        -o $msi $wxs $payloadWxs | Out-Host
    if ($LASTEXITCODE -ne 0) { throw 'WiX build failed.' }
} finally { Pop-Location }

Remove-Item -LiteralPath ([IO.Path]::ChangeExtension($msi, '.wixpdb')) -ErrorAction SilentlyContinue
$hash = (Get-FileHash -LiteralPath $msi -Algorithm SHA256).Hash.ToLowerInvariant()
[pscustomobject]@{ installer = $msi; sha256 = $hash; files = $fileCount; msi_version = $msiVersion; wix = $installedVersion }
