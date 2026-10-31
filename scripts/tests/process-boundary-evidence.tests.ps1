param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$SelectedExecutable,
    [Parameter(Mandatory)][string]$ProjectionLog
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
if(Test-Path -LiteralPath $OutputDirectory){throw 'Preserve previous control output'}
New-Item -ItemType Directory -Path $OutputDirectory|Out-Null
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'process-boundary-evidence.ps1')
. (Join-Path (Split-Path -Parent $PSScriptRoot) 'test-diagnostics.ps1')
$hash=(Get-FileHash -LiteralPath $SelectedExecutable).Hash.ToLowerInvariant()
$build=@{available=$true;qualification='synthetic retention controls, not a failed startup replay';selected_executable_sha256=$hash}
$controls=[Collections.Generic.List[string]]::new()
function Require([bool]$Condition,[string]$Message){if(-not $Condition){throw $Message}}
function Record-Control([string]$Name){$controls.Add($Name);Write-Output ($Name+': passed')}
function Save-Control([string]$Name,[string]$File,[string]$Digest){
    Save-ProcessBoundaryFailureExecutable -Executable $File -ExpectedSha256 $Digest -OutputDirectory (Join-Path $OutputDirectory $Name) -BuildIdentity $build -OriginalExitCode 101
}

$saved=Save-Control 'retained' $SelectedExecutable $hash
Require ($saved.status -eq 'retained') 'Selected executable retention failed'
$copy=Join-Path $OutputDirectory 'retained\diagnostics\failed-executable\argentum-execution-tests.exe'
Require ((Get-FileHash -LiteralPath $copy).Hash.ToLowerInvariant() -ceq $hash) 'Retained raw bytes differ'
Require (@(Get-ChildItem -LiteralPath (Split-Path -Parent $copy) -File).Count -eq 1) 'Copied unselected neighbouring files'
Require ($saved.original_exit_code -eq 101 -and $saved.build.available) 'Original result/build identity lost'
Require ($saved.raw_sha256 -ceq $hash -and $saved.expected_sha256 -ceq $hash) 'Actual/expected binary identities lost'
Record-Control 'actual-selected-byte-identity'

$receipt=Join-Path $OutputDirectory 'retained\diagnostics\failed-executable.json'
$receiptHash=(Get-FileHash -LiteralPath $receipt).Hash
$again=Save-Control 'retained' $SelectedExecutable $hash
Require ($again.reason -eq 'prior_receipt_preserved') 'Existing receipt not preserved'
Require ((Get-FileHash -LiteralPath $receipt).Hash -ceq $receiptHash) 'Existing receipt overwritten'
Record-Control 'no-overwrite'

$passedDirectory=Join-Path $OutputDirectory 'passed'
$passed=Save-ProcessBoundaryFailureExecutable -Executable (Join-Path $OutputDirectory 'missing.exe') -ExpectedSha256 $hash -OutputDirectory $passedDirectory -BuildIdentity $build -OriginalExitCode 0
Require ($passed.status -eq 'not_needed' -and -not (Test-Path -LiteralPath $passedDirectory)) 'Passing result caused binary retention'
Record-Control 'passing-no-retention'

$wrong=Save-Control 'wrong-hash' $SelectedExecutable ('0'*64)
Require ($wrong.status -eq 'not_retained' -and $wrong.reason -eq 'selected_identity_changed' -and $wrong.original_exit_code -eq 101) 'Wrong-hash control changed original failure'
Require ($wrong.raw_sha256 -ceq $hash -and $wrong.expected_sha256 -ceq ('0'*64)) 'Expected digest was mislabelled as observed bytes'
Record-Control 'wrong-hash-failure-preserved'

$savedWarnings=$WarningPreference
try{
    $WarningPreference='Stop'
    $missing=Save-Control 'missing' (Join-Path $OutputDirectory 'missing.exe') $hash
}finally{$WarningPreference=$savedWarnings}
Require ($missing.status -eq 'not_retained' -and $missing.reason -eq 'selected_file_unavailable' -and $missing.original_exit_code -eq 101) 'Unavailable-source control failed'
Record-Control 'missing-source-failure-preserved-with-warning-stop'

$large=Join-Path $OutputDirectory 'synthetic-oversize.bin'
$stream=[IO.File]::Open($large,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
try{$stream.SetLength(64MB+1)}finally{$stream.Dispose()}
$oversize=Save-Control 'oversize' $large $hash
Require ($oversize.status -eq 'not_retained' -and $oversize.reason -eq 'selected_file_exceeds_bound' -and $oversize.bytes -eq 64MB+1 -and $oversize.original_exit_code -eq 101) 'Bound control lost failure identity'
Record-Control 'oversize-failure-preserved'

$blocked=Join-Path $OutputDirectory 'blocked\diagnostics'
New-Item -ItemType Directory -Path $blocked -Force|Out-Null
[IO.File]::WriteAllText((Join-Path $blocked 'failed-executable'),'SYNTHETIC BLOCKER')
$copyFailure=Save-Control 'blocked' $SelectedExecutable $hash
Require ($copyFailure.status -eq 'not_retained' -and $copyFailure.reason -eq 'retention_io_failed' -and $copyFailure.original_exit_code -eq 101) 'Copy failure masked original failure'
Record-Control 'copy-failure-preserved'

# Consume the actual Rust formatter's explicitly labelled synthetic unit record.
$projection=[regex]::Match((Get-Content -LiteralPath $ProjectionLog -Raw),'SYNTHETIC_STARTUP_DIAGNOSTIC (\{[^\r\n]+\})')
Require $projection.Success 'Synthetic Rust formatter record missing'
$summary=$projection.Groups[1].Value
Require ($summary.Length -lt 2048) 'Formatter record exceeds exporter line bound'
$exportRoot=Join-Path $OutputDirectory 'synthetic-export'
New-Item -ItemType Directory -Path $exportRoot|Out-Null
$private=Join-Path $OutputDirectory 'SYNTHETIC-PRIVATE-ROOT'
$lines=@('---- synthetic_startup_failure stdout ----','SYNTHETIC CONTROL: no failed child process was executed',$summary,($private+' api_key=synthetic-sensitive-value'))
$lines+=@(1..250|ForEach-Object {'filler '+$_})
$lines|Set-Content -LiteralPath (Join-Path $exportRoot 'synthetic.log') -Encoding utf8NoBOM
Export-TestDiagnostics -EvidenceDirectory $exportRoot -PrivateRoots @($private)
$exported=Get-Content -LiteralPath (Join-Path $exportRoot 'diagnostics\synthetic.log')
$essential=@($exported|Where-Object {$_ -match '"kind":"process_startup_diagnostic"'})
Require ($essential.Count -eq 1) 'Clipped export did not retain exactly one essential record'
$value=$essential[0]|ConvertFrom-Json -Depth 10
Require ($value.state -eq 'TimedOut' -and $null -eq $value.exit_code -and $value.duration_ms -eq 20001 -and -not $value.marker_exists) 'Process/marker fields lost during export'
Require ($value.stdout_bytes -eq 8192 -and $value.stderr_bytes -eq 4096 -and $value.stdout_truncated -and $value.stderr_truncated) 'Capture fields lost during export'
Require ($value.stdout_sha256 -match '^[0-9a-f]{64}$' -and $value.stderr_sha256 -match '^[0-9a-f]{64}$' -and $value.stages.Count -eq 6) 'Hashes/stages lost during export'
$text=$exported -join "`n"
Require (-not $text.Contains($private) -and -not $text.Contains('synthetic-sensitive-value')) 'Exporter leaked synthetic private markers'
$index=Get-Content -LiteralPath (Join-Path $exportRoot 'diagnostics\log-index.json') -Raw|ConvertFrom-Json
Require ($index.retained_failure_sections -eq 1) 'Control did not exercise failure-section retention'
Record-Control 'actual-formatter-through-clipped-sanitized-export'

[ordered]@{complete=$true;synthetic_controls=$controls.ToArray();startup_processes_executed=0;selected_executable_sha256=$hash;binary_retention_bound=64MB}|ConvertTo-Json -Depth 6|Set-Content -LiteralPath (Join-Path $OutputDirectory 'control-results.json') -Encoding utf8NoBOM
