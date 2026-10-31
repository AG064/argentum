# Fresh credential-free portability diagnostics, separate from the full gates.
param([Parameter(Mandatory)][string]$OutputDirectory, [switch]$CiNullAccess)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'test-diagnostics.ps1')
. (Join-Path $PSScriptRoot 'process-boundary-evidence.ps1')
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$selectedExecutable=$null;$selectedHash=$null;$code=$null;$buildIdentity=@{available=$false;reason='build_identity_not_yet_observed'}
foreach ($item in @(Get-ChildItem Env: | Where-Object Name -match '(API_KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|ACCESS_KEY)')) {
    Remove-Item -LiteralPath ("Env:" + $item.Name)
}
try {
    & (Join-Path $PSScriptRoot 'device-security-diagnostics.ps1') *> (Join-Path $OutputDirectory 'device-security.log')
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
    $selectedExecutable=$exe
    $selectedHash=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $source=(Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
    $revision=(& git --no-optional-locks -C $source rev-parse HEAD).Trim()
    if($LASTEXITCODE -ne 0){throw 'Diagnostic source revision unavailable.'}
    $tree=(& git --no-optional-locks -C $source rev-parse 'HEAD^{tree}').Trim()
    if($LASTEXITCODE -ne 0){throw 'Diagnostic source tree unavailable.'}
    $dirty=@(& git --no-optional-locks -c core.fsmonitor=false -C $source status --porcelain=v1 --untracked-files=normal)
    if($LASTEXITCODE -ne 0){throw 'Diagnostic source status unavailable.'}
    $buildIdentity=@{
        available=$true
        source_revision=$revision;committed_tree=$tree;source_dirty=($dirty.Count -ne 0)
        cargo_lock_sha256=(Get-FileHash -LiteralPath (Join-Path $source 'Cargo.lock')).Hash.ToLowerInvariant()
        compile_log_sha256=(Get-FileHash -LiteralPath $compile).Hash.ToLowerInvariant()
        process_source_sha256=(Get-FileHash -LiteralPath (Join-Path $source 'crates\argentum-execution\src\process.rs')).Hash.ToLowerInvariant()
        runner_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
        retention_helper_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'process-boundary-evidence.ps1')).Hash.ToLowerInvariant()
        source_identity_boundary='Observed revision, dirty state and scoped input hashes, not a complete compiler-input manifest'
    }
    $started = [DateTime]::UtcNow
    $log = Join-Path $OutputDirectory 'process-boundary.log'
    $execute = { & $exe diagnostic --test-threads=1 --nocapture *> $log; $LASTEXITCODE }
    if ($CiNullAccess) {
        . (Join-Path $PSScriptRoot 'ci-null-scope.ps1')
        $code = Invoke-CiNullScope -Body $execute -ReceiptPath (Join-Path $OutputDirectory 'null-scope.json')
    } else { $code = & $execute }
    [ordered]@{target='process-boundary';executable_sha256=$selectedHash;exit_code=$code;started=$started.ToString('o');ended=[DateTime]::UtcNow.ToString('o');log=$log} |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8NoBOM
    if ($code -ne 0) { throw 'Process boundary diagnostics failed; preserved with the full gate results.' }
} finally {
    if($selectedExecutable -and ($null -eq $code -or $code -ne 0)){
        try {
            Save-ProcessBoundaryFailureExecutable -Executable $selectedExecutable -ExpectedSha256 $selectedHash -OutputDirectory $OutputDirectory -BuildIdentity $buildIdentity -OriginalExitCode $code|Out-Null
        }catch{Write-Warning 'Selected diagnostic executable retention failed; original outcome unchanged.' -WarningAction Continue}
    }
    Export-TestDiagnostics -EvidenceDirectory $OutputDirectory -PrivateRoots @((Get-Location).Path,$env:USERPROFILE,$env:RUNNER_TEMP)
}
