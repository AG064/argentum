[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$TargetDirectory = '',
    [string]$TargetName = '',
    [ValidateRange(1, 8)][int]$Jobs = 1,
    [switch]$CiNullAccess
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'test-diagnostics.ps1')
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
    [ordered]@{
        source_revision=(& git rev-parse HEAD)
        operating_system=[Runtime.InteropServices.RuntimeInformation]::OSDescription
        architecture=[Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        powershell=$PSVersionTable.PSVersion.ToString()
        cargo=(& cargo --version)
        rustc=(& rustc --version)
        jobs=$Jobs
        test_threads=1
        token_owner_is_user=[Security.Principal.WindowsIdentity]::GetCurrent().Owner.Equals([Security.Principal.WindowsIdentity]::GetCurrent().User)
        build_profile_names=@(Get-ChildItem Env: | Where-Object Name -match '^CARGO_PROFILE_' | Select-Object -ExpandProperty Name)
    } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'environment.json') -Encoding utf8NoBOM
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
    if ($TargetName) {
        $artifacts = @($artifacts | Where-Object { $_.target.name -eq $TargetName })
        if ($artifacts.Count -ne 1) { throw 'Selected target must identify exactly one Cargo-authored test executable.' }
    }
    # Build-only settings must not leak into a runtime guard's effective inputs.
    # Do not weaken production rejection of ambient compiler/profile overrides.
    $clear = @(Get-ChildItem Env: | Where-Object {
        $_.Name -match '^(CARGO_PROFILE_|CARGO_BUILD_|CARGO_TARGET_|RUSTFLAGS$|RUSTDOCFLAGS$|CARGO_ENCODED_RUST|RUSTC$|RUSTDOC$|RUSTC_WRAPPER$|RUSTC_WORKSPACE_WRAPPER$|RUSTC_BOOTSTRAP$|CARGO_INCREMENTAL$|ARGENTUM_.*CLI_EXE$)' -or
        $_.Name -match '(API_KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|ACCESS_KEY)'
    })
    foreach ($entry in $clear) {
        $saved[$entry.Name] = $entry.Value
        Remove-Item -LiteralPath ("Env:" + $entry.Name) -ErrorAction Stop
    }
    # Cargo ordinarily supplies this path when it launches its test harnesses.
    $saved['CARGO'] = [Environment]::GetEnvironmentVariable('CARGO', 'Process')
    $env:CARGO = (Get-Command cargo -CommandType Application -ErrorAction Stop).Source
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
        $execute = { & $exe --test-threads=1 *> $log; $LASTEXITCODE }
        if ($CiNullAccess -and $artifact.target.name -in @('argentum_execution', 'tool_catalog_http_cli')) {
            . (Join-Path $PSScriptRoot 'ci-null-scope.ps1')
            $code = Invoke-CiNullScope -Body $execute -ReceiptPath (Join-Path $output ("{0:d3}-null-scope.json" -f $number))
        } else { $code = & $execute }
        $results += [ordered]@{
            package_id=$artifact.package_id; target=$artifact.target.name
            executable=$exe; executable_sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
            exit_code=$code; started=$started.ToString('o'); ended=[DateTime]::UtcNow.ToString('o'); log=$log
        }
        $results | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding utf8NoBOM
        Write-Output ("{0}/{1} {2}: exit {3}" -f $number,$artifacts.Count,$artifact.target.name,$code)
    }
    Export-TestDiagnostics -EvidenceDirectory $output -PrivateRoots @($source, $env:USERPROFILE, $env:RUNNER_TEMP)
    foreach ($failed in @($results | Where-Object exit_code -ne 0)) {
        Write-Output ("Failed target: {0}; current executable SHA-256: {1}" -f $failed.target,$failed.executable_sha256)
        Get-Content -LiteralPath (Join-Path (Join-Path $output 'diagnostics') ([IO.Path]::GetFileName($failed.log))) -Tail 80 | Write-Output
    }
    if (@($results | Where-Object exit_code -ne 0).Count) { throw 'Offline test failures retained in results.json.' }
} finally {
    Export-TestDiagnostics -EvidenceDirectory $output -PrivateRoots @($source, $env:USERPROFILE, $env:RUNNER_TEMP)
    foreach ($name in $saved.Keys) {
        if ($null -eq $saved[$name]) { Remove-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    }
    Pop-Location
}
