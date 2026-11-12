param([Parameter(Mandatory)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
if (-not [IO.Path]::IsPathFullyQualified($OutputDirectory) -or (Test-Path -LiteralPath $OutputDirectory)) { throw 'Use new absolute disposable evidence.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
Add-Type -Path @((Join-Path $PSScriptRoot 'ci-null-scope.cs'), (Join-Path $PSScriptRoot 'tests/ci-null-scope.tests.cs')) -ErrorAction Stop
$result = [ArgentumCiNullTests]::Run()
foreach ($file in @('ci-null-scope.ps1','test-process-boundary.ps1','test-offline.ps1','test-diagnostics.ps1')) {
    $errors = $null; $tokens = $null
    [void][Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $file), [ref]$tokens, [ref]$errors)
    if ($errors.Count) { throw 'Modified driver syntax failed.' }
}
$result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'results.json') -Encoding utf8NoBOM
$result | ConvertTo-Json
