# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064

<#
.SYNOPSIS
Installs the Argentum MSI for the current user, checks what it created, then
uninstalls it and checks that it removed its own files and kept user data.

.DESCRIPTION
Run it on a PC or runner where this MSI is not already installed. It writes one
marker file into the Argentum user data folder before uninstalling, to prove
that uninstall keeps user data, and deletes the marker afterwards.

-TestDataRemoval also checks the opt-in REMOVEUSERDATA=1 uninstall. That
deletes the whole Argentum user data folder, so it refuses to run unless the
folder did not exist before the test. Use it only on a disposable machine.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$LogDirectory,
    [switch]$TestDataRemoval
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$msi = (Resolve-Path -LiteralPath $Installer).Path
New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
$logs = (Resolve-Path -LiteralPath $LogDirectory).Path

$installFolder = Join-Path $env:LOCALAPPDATA 'Programs\Argentum\app'
$menuFolder = Join-Path ([Environment]::GetFolderPath('Programs')) 'Argentum'
$cliFolder = Join-Path $installFolder 'cli'
$roamingData = Join-Path $env:APPDATA 'Argentum\Argentum'
$localData = Join-Path $env:LOCALAPPDATA 'Argentum\Argentum'
$installerKey = 'HKCU:\Software\Argentum\Installer'

function Invoke-Msiexec([string[]]$Arguments, [string]$Log) {
    $all = $Arguments + @('/qn', '/norestart', '/l*v', (Join-Path $logs $Log))
    $process = Start-Process -FilePath msiexec.exe -ArgumentList $all -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "msiexec $($Arguments -join ' ') failed with exit code $($process.ExitCode); see $Log." }
}
function Get-UserPath { @(([Environment]::GetEnvironmentVariable('Path', 'User') ?? '') -split ';' | Where-Object { $_ }) }
function Get-ArgentumMsiEntry {
    @(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall' -ErrorAction SilentlyContinue |
        Get-ItemProperty | Where-Object {
            $_.PSObject.Properties['DisplayName'] -and $_.DisplayName -eq 'Argentum' -and
            $_.PSObject.Properties['WindowsInstaller'] -and $_.WindowsInstaller -eq 1
        })
}
function Assert([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "Check failed: $Message" }
    Write-Output "ok  $Message"
}

if (Test-Path -LiteralPath $installFolder) { throw "$installFolder already exists; uninstall Argentum first." }
if (@(Get-ArgentumMsiEntry).Count) { throw 'An Argentum MSI is already installed; uninstall it first.' }
$dataExisted = (Test-Path -LiteralPath $roamingData) -or (Test-Path -LiteralPath $localData)
if ($TestDataRemoval -and $dataExisted) { throw 'Refusing to test data removal: Argentum user data already exists here.' }
$pathBefore = Get-UserPath

Invoke-Msiexec @('/i', $msi) 'install.log'
Assert (Test-Path -LiteralPath (Join-Path $installFolder 'Argentum.exe') -PathType Leaf) 'desktop app installed'
Assert (Test-Path -LiteralPath (Join-Path $cliFolder 'argentum.exe') -PathType Leaf) 'CLI installed'
Assert (Test-Path -LiteralPath (Join-Path $installFolder 'LICENSE') -PathType Leaf) 'license installed'
Assert (Test-Path -LiteralPath (Join-Path $menuFolder 'Argentum.lnk') -PathType Leaf) 'Start menu entry for the app'
Assert (Test-Path -LiteralPath (Join-Path $menuFolder 'Argentum Genesis CLI.lnk') -PathType Leaf) 'Start menu entry for the CLI'
Assert ((Get-UserPath) -contains $cliFolder) 'CLI folder on the user PATH'
Assert (@(Get-ArgentumMsiEntry).Count -eq 1) 'listed in installed apps'
$shell = New-Object -ComObject WScript.Shell
Assert ($shell.CreateShortcut((Join-Path $menuFolder 'Argentum.lnk')).TargetPath -eq (Join-Path $installFolder 'Argentum.exe')) 'app shortcut target'
$llama = @(Get-ChildItem -LiteralPath $installFolder -Recurse -File | Where-Object { $_.Name -match '(?i)llama|ggml|\.gguf$' })
Assert ($llama.Count -eq 0) 'no llama.cpp runtime or models installed'
$versionText = (& (Join-Path $cliFolder 'argentum.exe') --version | Out-String).Trim()
Assert ($LASTEXITCODE -eq 0 -and $versionText) "installed CLI runs: $versionText"

$marker = Join-Path $roamingData "installer-test-$([guid]::NewGuid().ToString('N')).txt"
New-Item -ItemType Directory -Path $roamingData -Force | Out-Null
Set-Content -LiteralPath $marker -Value 'Written by scripts/test-windows-installer.ps1' -Encoding ascii
try {
    Invoke-Msiexec @('/x', $msi) 'uninstall.log'
    Assert (-not (Test-Path -LiteralPath $installFolder)) 'install folder removed'
    Assert (-not (Test-Path -LiteralPath $menuFolder)) 'Start menu entries removed'
    Assert (-not ((Get-UserPath) -contains $cliFolder)) 'CLI folder removed from the user PATH'
    Assert ((@(Get-UserPath) -join ';') -eq (@($pathBefore) -join ';')) 'user PATH back to what it was'
    Assert (@(Get-ArgentumMsiEntry).Count -eq 0) 'removed from installed apps'
    Assert (-not (Test-Path -LiteralPath $installerKey)) 'installer registry values removed'
    Assert (Test-Path -LiteralPath $marker -PathType Leaf) 'user data kept after uninstall'
}
finally {
    Remove-Item -LiteralPath $marker -ErrorAction SilentlyContinue
    if (-not $dataExisted) {
        Remove-Item -LiteralPath $roamingData -Recurse -ErrorAction SilentlyContinue
        $parent = Split-Path -Parent $roamingData
        if (-not @(Get-ChildItem -LiteralPath $parent -Force -ErrorAction SilentlyContinue).Count) { Remove-Item -LiteralPath $parent -ErrorAction SilentlyContinue }
    }
}

if ($TestDataRemoval) {
    Invoke-Msiexec @('/i', $msi) 'install-again.log'
    foreach ($folder in @($roamingData, $localData)) {
        New-Item -ItemType Directory -Path (Join-Path $folder 'data') -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $folder 'data\marker.txt') -Value 'test' -Encoding ascii
    }
    Invoke-Msiexec @('/x', $msi, 'REMOVEUSERDATA=1') 'uninstall-remove-data.log'
    Assert (-not (Test-Path -LiteralPath $roamingData)) 'REMOVEUSERDATA=1 removed the roaming data folder'
    Assert (-not (Test-Path -LiteralPath $localData)) 'REMOVEUSERDATA=1 removed the local data folder'
    Assert (-not (Test-Path -LiteralPath $installFolder)) 'install folder removed again'
}
Write-Output 'Installer install and uninstall checks passed.'
