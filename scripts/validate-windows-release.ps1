[CmdletBinding()]
param(
    [string]$ExecutablePath = ""
)

$ErrorActionPreference = "Stop"

$workspaceRoot = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
    $ExecutablePath = Join-Path $workspaceRoot "artifacts\windows-x64\Argentum.exe"
}

$resolvedExecutable = (Resolve-Path -LiteralPath $ExecutablePath -ErrorAction Stop).Path
$resolvedWorkspace = (Resolve-Path -LiteralPath $workspaceRoot).Path
$workspacePrefix = $resolvedWorkspace.TrimEnd('\') + '\'
if (-not ($resolvedExecutable.Equals($resolvedWorkspace, [System.StringComparison]::OrdinalIgnoreCase) -or $resolvedExecutable.StartsWith($workspacePrefix, [System.StringComparison]::OrdinalIgnoreCase))) {
    throw "Validation executable must be inside the Argentum workspace"
}

$stagingDirectory = Join-Path $workspaceRoot ("artifacts\windows-validation\" + [DateTime]::UtcNow.ToString("yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Path $stagingDirectory -Force | Out-Null
$stagedExecutable = Join-Path $stagingDirectory "Argentum.exe"
Copy-Item -LiteralPath $resolvedExecutable -Destination $stagedExecutable

$process = Start-Process -FilePath $stagedExecutable -WorkingDirectory $stagingDirectory -PassThru
try {
    $deadline = (Get-Date).AddSeconds(15)
    do {
        Start-Sleep -Milliseconds 250
        $process.Refresh()
        if ($process.HasExited) {
            throw "Packaged Argentum exited during startup with code $($process.ExitCode)"
        }
    } while ($process.MainWindowHandle -eq 0 -and (Get-Date) -lt $deadline)

    if ($process.MainWindowHandle -eq 0) {
        throw "Packaged Argentum did not expose a native window within 15 seconds"
    }

    Write-Output "Native window launched from: $stagedExecutable"
    Write-Output "PID: $($process.Id)"
}
finally {
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -Force
        $process.WaitForExit(5000) | Out-Null
    }
}

Write-Output "Validation staging directory: $stagingDirectory"
