[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$TargetDirectory = '',
    [ValidateRange(1, 8)][int]$Jobs = 1
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
if (-not [IO.Path]::IsPathFullyQualified($OutputDirectory)) { throw 'Use an absolute new evidence directory.' }
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) { throw 'Preserve existing results; choose a new output directory.' }
if ($output.Equals($source, [StringComparison]::OrdinalIgnoreCase) -or
    $output.StartsWith($source.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Keep test evidence outside the source checkout.'
}
New-Item -ItemType Directory -Path $output | Out-Null
Push-Location $source
$saved = @{}
try {
    $args = @('test','--workspace','--all-targets','--no-run','--locked','--offline','--message-format=json','-j',[string]$Jobs)
    if ($TargetDirectory) {
        if (-not [IO.Path]::IsPathFullyQualified($TargetDirectory)) { throw 'Target directory must be absolute.' }
        $args += @('--target-dir', $TargetDirectory)
    }
    & cargo @args 1> (Join-Path $output 'compiler-artifacts.jsonl') 2> (Join-Path $output 'compile.log')
    if ($LASTEXITCODE -ne 0) { throw 'Offline test compilation failed; preserve compile.log.' }
    $artifacts = @(Get-Content -LiteralPath (Join-Path $output 'compiler-artifacts.jsonl') | ForEach-Object {
        $event = $_ | ConvertFrom-Json -Depth 100
        if ($event.reason -eq 'compiler-artifact' -and $event.profile.test -and $event.executable) { $event }
    } | Sort-Object executable -Unique)
    if (-not $artifacts.Count) { throw 'No Cargo-authored test executables were returned.' }
    # Build-only settings must not leak into a runtime guard's effective inputs.
    # Do not weaken production rejection of ambient compiler/profile overrides.
    $clear = @(Get-ChildItem Env: | Where-Object {
        $_.Name -match '^(CARGO_PROFILE_|CARGO_BUILD_|CARGO_TARGET_|RUSTFLAGS$|RUSTDOCFLAGS$|CARGO_ENCODED_RUST|RUSTC$|RUSTDOC$|RUSTC_WRAPPER$|RUSTC_WORKSPACE_WRAPPER$|RUSTC_BOOTSTRAP$|CARGO_INCREMENTAL$|ARGENTUM_.*CLI_EXE$)' -or
        $_.Name -match '(API_KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|ACCESS_KEY)'
    })
    foreach ($entry in $clear) {
        $saved[$entry.Name] = $entry.Value
        [Environment]::SetEnvironmentVariable($entry.Name, $null, 'Process')
    }
    $results = @()
    $number = 0
    foreach ($artifact in $artifacts) {
        $number++
        $exe = $artifact.executable
        if (-not [IO.Path]::IsPathFullyQualified($exe) -or -not (Test-Path -LiteralPath $exe -PathType Leaf)) {
            throw 'Cargo returned an unavailable test executable.'
        }
        $started = [DateTime]::UtcNow
        $log = Join-Path $output ("{0:d3}-{1}.log" -f $number, $artifact.target.name)
        & $exe --test-threads=1 *> $log
        $code = $LASTEXITCODE
        $results += [ordered]@{
            package_id=$artifact.package_id; target=$artifact.target.name
            executable=$exe; executable_sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
            exit_code=$code; started=$started.ToString('o'); ended=[DateTime]::UtcNow.ToString('o'); log=$log
        }
        Write-Output ("{0}/{1} {2}: exit {3}" -f $number,$artifacts.Count,$artifact.target.name,$code)
    }
    $results | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding utf8NoBOM
    if (@($results | Where-Object exit_code -ne 0).Count) { throw 'Offline test failures retained in results.json.' }
} finally {
    foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    Pop-Location
}
