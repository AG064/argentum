# Fresh credential-free portability diagnostics, separate from the full gates.
param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'test-diagnostics.ps1')
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
foreach ($item in @(Get-ChildItem Env: | Where-Object Name -match '(API_KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|ACCESS_KEY)')) {
    Remove-Item -LiteralPath ("Env:" + $item.Name)
}
try {
    $compile = Join-Path $OutputDirectory 'compile.log'
    & cargo test -p argentum-execution --lib --no-run --locked --offline -j 2 --message-format=json *> $compile
    if ($LASTEXITCODE -ne 0) { throw 'Fresh diagnostic compilation failed.' }
    $executables = @(Get-Content -LiteralPath $compile | ForEach-Object {
        try { $event = $_ | ConvertFrom-Json -ErrorAction Stop } catch { return }
        if ($event.reason -eq 'compiler-artifact' -and $event.profile.test -and $event.executable) { $event.executable }
    })
    if ($executables.Count -ne 1) { throw 'Expected one current execution test artifact.' }
    $exe = $executables[0]
    if (-not [IO.Path]::IsPathFullyQualified($exe) -or -not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Current diagnostic executable is unavailable.' }
    $started = [DateTime]::UtcNow
    $log = Join-Path $OutputDirectory 'process-boundary.log'
    & $exe diagnostic --test-threads=1 --nocapture *> $log
    $code = $LASTEXITCODE
    [ordered]@{target='process-boundary';executable_sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant();exit_code=$code;started=$started.ToString('o');ended=[DateTime]::UtcNow.ToString('o');log=$log} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8NoBOM
    if ($code -ne 0) { throw 'Process boundary diagnostics failed; preserved with the full gate results.' }
} finally {
    Export-TestDiagnostics -EvidenceDirectory $OutputDirectory -PrivateRoots @((Get-Location).Path,$env:USERPROFILE,$env:RUNNER_TEMP)
}
