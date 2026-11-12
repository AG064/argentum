[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputDirectory)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Native application adapter requires Windows.' }
$output = [IO.Path]::GetFullPath($OutputDirectory)
if (-not [IO.Path]::IsPathFullyQualified($OutputDirectory)) { throw 'Absolute disposable build directory required.' }
New-Item -ItemType Directory -Path $output -Force | Out-Null
$framework = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319'
$assemblies = @('UIAutomationClient','UIAutomationTypes','WindowsBase') | ForEach-Object {
    $assembly = @(Get-ChildItem -LiteralPath (Join-Path $env:WINDIR "Microsoft.NET/assembly/GAC_MSIL/$_") -Recurse -Filter "$_.dll")
    if ($assembly.Count -ne 1) { throw 'Exact installed managed UI Automation assembly unavailable.' }
    '/reference:' + $assembly[0].FullName
}
& (Join-Path $framework 'csc.exe') /nologo /warnaserror+ /optimize+ /platform:x64 /target:exe /reference:System.Windows.Forms.dll /reference:System.Drawing.dll /reference:System.Web.Extensions.dll @assemblies "/out:$output/argentum-desktop-adapter.exe" (Join-Path $PSScriptRoot 'Adapter.cs')
if ($LASTEXITCODE -ne 0) { throw 'Desktop adapter compile failed.' }
Get-FileHash -LiteralPath (Join-Path $output 'argentum-desktop-adapter.exe')
