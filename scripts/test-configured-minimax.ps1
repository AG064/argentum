param(
    [string]$Workspace = 'A:\Argentum',
    [string]$SecretsPath = '',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not ('Argentum.Smoke.NativeMethods' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Argentum.Smoke
{
    [StructLayout(LayoutKind.Sequential)]
    public struct ByHandleFileInformation
    {
        public uint FileAttributes;
        public System.Runtime.InteropServices.ComTypes.FILETIME CreationTime;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastAccessTime;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWriteTime;
        public uint VolumeSerialNumber;
        public uint FileSizeHigh;
        public uint FileSizeLow;
        public uint NumberOfLinks;
        public uint FileIndexHigh;
        public uint FileIndexLow;
    }

    public static class NativeMethods
    {
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool GetFileInformationByHandle(
            SafeFileHandle file,
            out ByHandleFileInformation information);
    }
}
'@
}

function Test-ReparsePoint {
    param([System.IO.FileSystemInfo]$Item)

    return ($Item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0
}

function Assert-SafeSecretsPath {
    param([string]$WorkspacePath, [string]$Path)

    $workspaceItem = Get-Item -LiteralPath $WorkspacePath -Force -ErrorAction Stop
    if (-not $workspaceItem.PSIsContainer -or (Test-ReparsePoint -Item $workspaceItem)) {
        throw 'The configured workspace must be a regular directory, not a linked directory.'
    }

    $secretItem = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
    if ($secretItem.PSIsContainer -or (Test-ReparsePoint -Item $secretItem)) {
        throw 'Linked secrets files are not accepted by this smoke test.'
    }

    $secretFullPath = [System.IO.Path]::GetFullPath($secretItem.FullName)
    $workspacePrefix = $WorkspacePath.TrimEnd('\') + '\'
    if (-not $secretFullPath.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'The configured secrets file must be inside the selected workspace.'
    }

    $pathWithoutDrive = $secretFullPath.Substring([System.IO.Path]::GetPathRoot($secretFullPath).Length)
    if ($pathWithoutDrive.Contains(':')) {
        throw 'Alternate data streams are not accepted as a secrets source.'
    }

    $relativeParent = [System.IO.Path]::GetDirectoryName($secretFullPath.Substring($workspacePrefix.Length))
    $currentDirectory = $WorkspacePath
    if (-not [string]::IsNullOrWhiteSpace($relativeParent)) {
        foreach ($part in $relativeParent.Split([char[]]'\/', [System.StringSplitOptions]::RemoveEmptyEntries)) {
            $currentDirectory = Join-Path $currentDirectory $part
            $directoryItem = Get-Item -LiteralPath $currentDirectory -Force -ErrorAction Stop
            if (-not $directoryItem.PSIsContainer -or (Test-ReparsePoint -Item $directoryItem)) {
                throw 'Linked directories are not accepted in the secrets path.'
            }
        }
    }

    return $secretFullPath
}

function Read-EnvSecret {
    param([string]$Path, [string]$Name)

    $stream = $null
    $reader = $null
    try {
        $stream = New-Object System.IO.FileStream(
            $Path,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::Read,
            [System.IO.FileShare]::Read
        )
        if ($stream.Length -gt 262144) {
            throw 'The configured secrets source is not a bounded file.'
        }

        $information = New-Object Argentum.Smoke.ByHandleFileInformation
        if (-not [Argentum.Smoke.NativeMethods]::GetFileInformationByHandle(
                $stream.SafeFileHandle,
                [ref]$information
            )) {
            throw 'The configured secrets source could not be inspected safely.'
        }
        if (($information.FileAttributes -band [uint32][System.IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'Linked secrets files are not accepted by this smoke test.'
        }
        if ($information.NumberOfLinks -ne 1) {
            throw 'Hard-linked secrets files are not accepted by this smoke test.'
        }

        $reader = New-Object System.IO.StreamReader(
            $stream,
            [System.Text.Encoding]::UTF8,
            $true,
            4096,
            $true
        )
        while (($line = $reader.ReadLine()) -ne $null) {
            if ($line -notmatch '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$') {
                continue
            }
            if ($Matches[1] -ne $Name) {
                continue
            }
            $value = $Matches[2].Trim().Trim('"').Trim("'")
            if ($value.Length -lt 16 -or $value.IndexOfAny([char[]]"`r`n") -ge 0) {
                throw 'The configured MiniMax credential is not usable.'
            }
            return $value
        }
        throw 'MINIMAX_API_KEY is not present in the configured secrets file.'
    }
    finally {
        if ($null -ne $reader) {
            $reader.Dispose()
        }
        if ($null -ne $stream) {
            $stream.Dispose()
        }
    }
}

function Invoke-ArgentumCli {
    param([string[]]$Arguments)

    $previousPreference = $ErrorActionPreference
    $output = @()
    $exitCode = $null
    try {
        $ErrorActionPreference = 'Continue'
        $output = @(& $script:cliPath @Arguments 2>&1 | ForEach-Object { "$_" })
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($null -eq $exitCode -or $exitCode -ne 0) {
        $operation = if ($Arguments.Count -gt 1 -and $Arguments[0] -eq 'provider') {
            "provider $($Arguments[1])"
        }
        else {
            $Arguments[0]
        }
        $terminalMessage = $output | ForEach-Object {
            $trimmed = $_.Trim()
            if (-not $trimmed.StartsWith('{')) {
                return
            }
            try {
                $record = $trimmed | ConvertFrom-Json
                if ($record.type -eq 'command_failed') {
                    $record.message
                }
            }
            catch {
                return
            }
        } | Select-Object -Last 1
        if ([string]::IsNullOrWhiteSpace($terminalMessage)) {
            $terminalMessage = 'No safe terminal detail was returned.'
        }
        throw "The Argentum CLI smoke command failed during $operation. $terminalMessage"
    }
    return $output
}

function Read-JsonLines {
    param([string[]]$Lines)

    foreach ($line in $Lines) {
        $trimmed = $line.Trim()
        if (-not $trimmed.StartsWith('{')) {
            continue
        }
        $trimmed | ConvertFrom-Json
    }
}

function Remove-SmokeDatabase {
    param([string]$Path)

    $candidatePaths = @(
        $Path,
        "$Path-wal",
        "$Path-shm",
        "$Path-journal"
    )
    foreach ($candidate in $candidatePaths) {
        if (-not (Test-Path -LiteralPath $candidate)) {
            continue
        }
        $item = Get-Item -LiteralPath $candidate -Force -ErrorAction Stop
        if ($item.PSIsContainer -or (Test-ReparsePoint -Item $item)) {
            throw 'The isolated smoke database was replaced by an unsafe filesystem object.'
        }
        Remove-Item -LiteralPath $item.FullName -Force -ErrorAction Stop
        if (Test-Path -LiteralPath $item.FullName) {
            throw 'The isolated smoke database could not be removed.'
        }
    }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$workspacePath = [System.IO.Path]::GetFullPath($Workspace)
if (-not (Test-Path -LiteralPath $workspacePath -PathType Container)) {
    throw 'The configured Argentum workspace does not exist.'
}

if ([string]::IsNullOrWhiteSpace($SecretsPath)) {
    $SecretsPath = Join-Path $workspacePath 'secrets.env'
}
$secretFullPath = Assert-SafeSecretsPath -WorkspacePath $workspacePath -Path $SecretsPath

$previousKey = [Environment]::GetEnvironmentVariable('MINIMAX_API_KEY', 'Process')
$originalProviderId = $null
$providerMutationStarted = $false
$script:cliPath = $null
$smokeTempRoot = $null
$databasePath = $null
$databaseOwnedBySmoke = $false
$locationPushed = $false
$primaryFailure = $null
$smokeSucceeded = $false
$selectionRestored = $false
$cleanupFailures = New-Object System.Collections.Generic.List[string]

try {
    Push-Location $repoRoot
    $locationPushed = $true

    if (-not $SkipBuild) {
        cargo build --locked -p argentum-cli
        if ($LASTEXITCODE -ne 0) {
            throw 'The Argentum CLI build failed.'
        }
    }

    $script:cliPath = Join-Path $repoRoot 'target\debug\argentum-cli.exe'
    if (-not (Test-Path -LiteralPath $script:cliPath -PathType Leaf)) {
        throw 'The Argentum CLI executable is missing.'
    }

    $smokeTempRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
        'argentum-minimax-smoke-' + [Guid]::NewGuid().ToString('N')
    )
    [System.IO.Directory]::CreateDirectory($smokeTempRoot) | Out-Null
    $databasePath = Join-Path $smokeTempRoot 'smoke.db'
    $databaseStream = [System.IO.File]::Open(
        $databasePath,
        [System.IO.FileMode]::CreateNew,
        [System.IO.FileAccess]::ReadWrite,
        [System.IO.FileShare]::None
    )
    $databaseStream.Dispose()
    $databaseOwnedBySmoke = $true

    $profiles = Invoke-ArgentumCli @(
        'provider', 'list', '--workspace', $workspacePath,
        '--database', $databasePath, '--json'
    )
    $profileRows = ($profiles -join "`n") | ConvertFrom-Json
    $originalProviderId = ($profileRows | Where-Object { $_.selected } | Select-Object -First 1).id
    if ([string]::IsNullOrWhiteSpace($originalProviderId)) {
        throw 'The isolated smoke database did not initialize a selected provider.'
    }

    $env:MINIMAX_API_KEY = Read-EnvSecret -Path $secretFullPath -Name 'MINIMAX_API_KEY'
    $providerMutationStarted = $true
    Invoke-ArgentumCli @(
        'provider', 'save', 'minimax',
        '--label', 'MiniMax',
        '--kind', 'openai-compatible',
        '--endpoint', 'https://api.minimax.io/v1/',
        '--model', 'MiniMax-M2.7',
        '--select',
        '--workspace', $workspacePath,
        '--database', $databasePath,
        '--json'
    ) | Out-Null

    $catalogOutput = Invoke-ArgentumCli @(
        'provider', 'models', 'minimax', '--workspace', $workspacePath,
        '--database', $databasePath, '--json'
    )
    $catalog = ($catalogOutput -join "`n") | ConvertFrom-Json
    $catalogModel = @($catalog.models) | Where-Object { $_.id -eq 'MiniMax-M2.7' } | Select-Object -First 1
    if ($catalog.provider_id -ne 'minimax' -or
        $catalog.selected_model -ne 'MiniMax-M2.7' -or
        $null -eq $catalogModel -or
        $catalogModel.context_window_tokens -ne 204800) {
        throw 'MiniMax did not return the expected scoped model catalog and context limit.'
    }

    Invoke-ArgentumCli @(
        'provider', 'model', $originalProviderId,
        '--model', 'argentum-smoke-local-model',
        '--workspace', $workspacePath,
        '--database', $databasePath,
        '--json'
    ) | Out-Null
    $profileRows = ((Invoke-ArgentumCli @(
        'provider', 'list', '--workspace', $workspacePath,
        '--database', $databasePath, '--json'
    )) -join "`n") | ConvertFrom-Json
    $selectedAfterOtherModelChange = ($profileRows | Where-Object { $_.selected } | Select-Object -First 1).id
    $changedOtherProfile = $profileRows | Where-Object {
        $_.id -eq $originalProviderId -and $_.model -eq 'argentum-smoke-local-model'
    } | Select-Object -First 1
    if ($selectedAfterOtherModelChange -ne 'minimax' -or $null -eq $changedOtherProfile) {
        throw 'Selecting a model on another profile changed the selected provider or wrong profile.'
    }

    $probeEvents = Read-JsonLines (Invoke-ArgentumCli @(
        'provider', 'probe', 'minimax', '--workspace', $workspacePath,
        '--database', $databasePath, '--json'
    ))
    $connected = $probeEvents | Where-Object {
        if ($_.type -ne 'event') {
            return $false
        }
        $statusProperty = $_.event.PSObject.Properties['ProviderStatus']
        $null -ne $statusProperty -and $statusProperty.Value.connected -eq $true
    }
    if (-not $connected) {
        throw 'MiniMax did not report a reachable models endpoint.'
    }

    $sentinel = 'ARGENTUM_MINIMAX_SMOKE_OK'
    $runEvents = Read-JsonLines (Invoke-ArgentumCli @(
        'run', '--workspace', $workspacePath,
        '--database', $databasePath,
        '--prompt', "Reply with exactly $sentinel and no other text.",
        '--json'
    ))
    $text = ($runEvents | Where-Object { $_.type -eq 'event' } | ForEach-Object {
        $deltaProperty = $_.event.PSObject.Properties['AssistantDelta']
        if ($null -ne $deltaProperty) {
            $deltaProperty.Value.text
        }
    }) -join ''
    $reasoning = ($runEvents | Where-Object { $_.type -eq 'event' } | ForEach-Object {
        $reasoningProperty = $_.event.PSObject.Properties['AssistantReasoningDelta']
        if ($null -ne $reasoningProperty) {
            $reasoningProperty.Value.text
        }
    }) -join ''
    $usageEvents = @($runEvents | Where-Object { $_.type -eq 'event' } | ForEach-Object {
        $usageProperty = $_.event.PSObject.Properties['ModelUsageUpdated']
        if ($null -ne $usageProperty) {
            $usageProperty.Value
        }
    })
    $usageEvent = $usageEvents | Select-Object -Last 1
    $completed = $runEvents | Where-Object { $_.type -eq 'command_completed' }
    if (-not $completed -or $text.Trim() -ne $sentinel) {
        throw 'MiniMax streaming completed without the expected smoke marker.'
    }
    if ([string]::IsNullOrWhiteSpace($reasoning) -or
        $text.Contains('<think>') -or
        $text.Contains('</think>') -or
        $reasoning.Contains('<think>') -or
        $reasoning.Contains('</think>')) {
        throw 'MiniMax reasoning was missing or raw reasoning markers reached a typed stream.'
    }
    if ($usageEvents.Count -ne 1 -or
        $null -eq $usageEvent -or
        $usageEvent.profile_id -ne 'minimax' -or
        $usageEvent.model -ne 'MiniMax-M2.7' -or
        $usageEvent.usage.total_tokens -ne (
            $usageEvent.usage.input_tokens + $usageEvent.usage.output_tokens
        ) -or
        $usageEvent.usage.context_window_tokens -ne 204800) {
        throw 'MiniMax did not report one consistent usage record with the expected context limit.'
    }

    $smokeSucceeded = $true
}
catch {
    $primaryFailure = $_
}
finally {
    try {
        [Environment]::SetEnvironmentVariable('MINIMAX_API_KEY', $previousKey, 'Process')
    }
    catch {
        $cleanupFailures.Add('The prior process credential could not be restored.')
    }

    if ($providerMutationStarted -and
        -not [string]::IsNullOrWhiteSpace($originalProviderId) -and
        $null -ne $script:cliPath -and
        $databaseOwnedBySmoke) {
        try {
            Invoke-ArgentumCli @(
                'provider', 'select', $originalProviderId,
                '--workspace', $workspacePath,
                '--database', $databasePath,
                '--json'
            ) | Out-Null
            $restoredProfiles = Invoke-ArgentumCli @(
                'provider', 'list', '--workspace', $workspacePath,
                '--database', $databasePath, '--json'
            )
            $restoredRows = ($restoredProfiles -join "`n") | ConvertFrom-Json
            $restoredProviderId = ($restoredRows | Where-Object { $_.selected } | Select-Object -First 1).id
            if ($restoredProviderId -ne $originalProviderId) {
                throw 'Provider selection verification failed.'
            }
            $selectionRestored = $true
        }
        catch {
            $cleanupFailures.Add('The isolated provider selection could not be restored and verified.')
        }
    }

    if ($databaseOwnedBySmoke -and $null -ne $databasePath) {
        try {
            Remove-SmokeDatabase -Path $databasePath
        }
        catch {
            $cleanupFailures.Add('The isolated smoke database could not be cleaned up safely.')
        }
    }

    if ($null -ne $smokeTempRoot) {
        try {
            $resolvedTempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
            $resolvedSmokeRoot = [System.IO.Path]::GetFullPath($smokeTempRoot)
            $smokeRootItem = Get-Item -LiteralPath $resolvedSmokeRoot -Force -ErrorAction Stop
            if (-not $resolvedSmokeRoot.StartsWith($resolvedTempRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
                -not [System.IO.Path]::GetFileName($resolvedSmokeRoot).StartsWith(
                    'argentum-minimax-smoke-',
                    [System.StringComparison]::Ordinal
                ) -or
                -not $smokeRootItem.PSIsContainer -or
                (Test-ReparsePoint -Item $smokeRootItem)) {
                throw 'The temporary smoke directory failed its cleanup boundary check.'
            }
            if (@(Get-ChildItem -LiteralPath $resolvedSmokeRoot -Force -ErrorAction Stop).Count -ne 0) {
                throw 'The temporary smoke directory is not empty after database cleanup.'
            }
            Remove-Item -LiteralPath $resolvedSmokeRoot -Force -ErrorAction Stop
            if (Test-Path -LiteralPath $resolvedSmokeRoot) {
                throw 'The temporary smoke directory could not be removed.'
            }
        }
        catch {
            $cleanupFailures.Add('The temporary smoke directory could not be cleaned up safely.')
        }
    }

    if ($locationPushed) {
        try {
            Pop-Location
        }
        catch {
            $cleanupFailures.Add('The prior shell location could not be restored.')
        }
    }
}

if ($cleanupFailures.Count -ne 0) {
    throw ($cleanupFailures -join ' ')
}
if ($null -ne $primaryFailure) {
    throw $primaryFailure
}
if ($smokeSucceeded) {
    Write-Output 'MiniMax models endpoint reached.'
    Write-Output 'MiniMax model catalog and profile-scoped selection verified.'
    Write-Output 'MiniMax reasoning and exact context usage verified without disclosure.'
    Write-Output 'MiniMax streamed response verified through CommandHost.'
}
if ($selectionRestored) {
    Write-Output "Restored provider selection to $originalProviderId."
}
