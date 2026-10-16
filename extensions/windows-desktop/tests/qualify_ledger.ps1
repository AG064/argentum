[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputDirectory)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or -not [IO.Path]::IsPathFullyQualified($OutputDirectory)) {
    throw 'Windows and an absolute disposable output directory are required.'
}
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Fresh disposable output required.' }
New-Item -ItemType Directory -Path $OutputDirectory | Out-Null
$assemblyReferences = @('UIAutomationClient','UIAutomationTypes','WindowsBase') | ForEach-Object {
    $matches = @(Get-ChildItem -LiteralPath (Join-Path $env:WINDIR "Microsoft.NET/assembly/GAC_MSIL/$_") -Recurse -Filter "$_.dll")
    if ($matches.Count -ne 1) { throw 'Exact installed UI Automation assembly unavailable.' }
    '/reference:' + $matches[0].FullName
}
$testExecutable = Join-Path $OutputDirectory 'ledger-controls.exe'
& (Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe') /nologo /warnaserror+ /optimize+ /platform:x64 /target:exe /main:TestLedger /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.Web.Extensions.dll @assemblyReferences "/out:$testExecutable" (Join-Path $PSScriptRoot '../Adapter.cs') (Join-Path $PSScriptRoot 'TestLedger.cs')
if ($LASTEXITCODE -ne 0) { throw 'Counter control compile failed.' }
& $testExecutable (Join-Path $OutputDirectory 'state')
if ($LASTEXITCODE -ne 0) { throw 'Counter controls failed.' }
