[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$Node,
    [Parameter(Mandatory)][string]$BrowserCache,
    [string]$TargetDirectory = '',
    [string]$CliExecutable = '',
    [string]$AdapterPath = '',
    [ValidateRange(1, 8)][int]$Jobs = 1
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$source=(Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$output=[IO.Path]::GetFullPath($OutputDirectory)
if(-not [IO.Path]::IsPathFullyQualified($OutputDirectory) -or
   $output.Equals($source,[StringComparison]::OrdinalIgnoreCase) -or
   $output.StartsWith($source.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase) -or
   (Test-Path -LiteralPath $output)){throw 'Use a fresh absolute evidence directory outside source.'}
if(-not [IO.Path]::IsPathFullyQualified($Node) -or -not (Test-Path -LiteralPath $Node -PathType Leaf) -or
   -not [IO.Path]::IsPathFullyQualified($BrowserCache) -or -not (Test-Path -LiteralPath $BrowserCache -PathType Container)){throw 'Existing reviewed Node and browser cache required; no installation or provider request.'}
foreach($selected in @($CliExecutable,$AdapterPath)){
    if($selected -and (-not [IO.Path]::IsPathFullyQualified($selected) -or -not (Test-Path -LiteralPath $selected -PathType Leaf))){throw 'Selected packaged components must be existing absolute files.'}
}
New-Item -ItemType Directory -Path $output | Out-Null
$targetArgs=@()
if($TargetDirectory){
    if(-not [IO.Path]::IsPathFullyQualified($TargetDirectory)){throw 'Absolute target required.'}
    $targetArgs=@('--target-dir',$TargetDirectory)
}
Push-Location $source
$saved=@{}
try {
    & cargo test -p argentum-cli --features extensions --tests --no-run --locked --offline -j $Jobs @targetArgs --message-format=json 1> (Join-Path $output 'compiler-artifacts.jsonl') 2> (Join-Path $output 'compile.log')
    if($LASTEXITCODE -ne 0){throw 'Extension test compilation failed; keep compile.log.'}
    & cargo build -p argentum-extensions --bin mcp_fixture --locked --offline -j $Jobs @targetArgs *> (Join-Path $output 'fixture-build.log')
    if($LASTEXITCODE -ne 0){throw 'Owned fixture build failed.'}
    $metadata=& cargo metadata --no-deps --format-version 1 --locked --offline | ConvertFrom-Json -Depth 100
    if($LASTEXITCODE -ne 0){throw 'Could not resolve compiler outputs.'}
    $target=if($TargetDirectory){$TargetDirectory}else{$metadata.target_directory}
    $artifacts=@(Get-Content -LiteralPath (Join-Path $output 'compiler-artifacts.jsonl') | ForEach-Object {$_ | ConvertFrom-Json -Depth 100} | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable })
    $names=@('extensions_cli','extensions_browser_cli','extensions_lifecycle_cli')
    $selected=@($artifacts | Where-Object {$names -contains $_.target.name} | Sort-Object executable -Unique)
    if($selected.Count -ne $names.Count){throw 'Each required extension target must have a compiler-authored executable.'}
    foreach($entry in @(Get-ChildItem Env: | Where-Object { $_.Name -match '^(CARGO_PROFILE_|CARGO_BUILD_|CARGO_TARGET_|RUSTFLAGS$|RUSTDOCFLAGS$|CARGO_ENCODED_RUST|RUSTC$|RUSTDOC$|CARGO_INCREMENTAL$|ARGENTUM_NEXT01_)' -or $_.Name -match '(API_KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|ACCESS_KEY)' })){
        $saved[$entry.Name]=$entry.Value; Remove-Item -LiteralPath ('Env:'+$entry.Name)
    }
    foreach($name in @('ARGENTUM_NEXT01_NODE','ARGENTUM_PLAYWRIGHT_BROWSERS_PATH','ARGENTUM_NEXT01_CLI_EXE','ARGENTUM_NEXT01_ADAPTER','ARGENTUM_NEXT01_MCP_FIXTURE_EXE')){
        if(-not $saved.ContainsKey($name)){$saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
    }
    $env:ARGENTUM_NEXT01_NODE=$Node
    $env:ARGENTUM_PLAYWRIGHT_BROWSERS_PATH=$BrowserCache
    $env:ARGENTUM_NEXT01_CLI_EXE=if($CliExecutable){$CliExecutable}else{Join-Path $target 'debug/argentum.exe'}
    $env:ARGENTUM_NEXT01_ADAPTER=if($AdapterPath){$AdapterPath}else{Join-Path $source 'extensions/browser/adapter.mjs'}
    $env:ARGENTUM_NEXT01_MCP_FIXTURE_EXE=Join-Path $target 'debug/mcp_fixture.exe'
    $selectedCliSha=(Get-FileHash -LiteralPath $env:ARGENTUM_NEXT01_CLI_EXE).Hash.ToLowerInvariant()
    $selectedAdapterSha=(Get-FileHash -LiteralPath $env:ARGENTUM_NEXT01_ADAPTER).Hash.ToLowerInvariant()
    $results=@()
    foreach($artifact in $selected){
        $log=Join-Path $output ($artifact.target.name+'.log')
        $arguments=if($artifact.target.name -eq 'extensions_cli'){@('--test-threads=1')}else{@('--ignored','--test-threads=1')}
        & $artifact.executable @arguments *> $log
        $results += [ordered]@{target=$artifact.target.name;executable_sha256=(Get-FileHash -LiteralPath $artifact.executable).Hash.ToLowerInvariant();cli_path=$env:ARGENTUM_NEXT01_CLI_EXE;cli_sha256=$selectedCliSha;adapter_path=$env:ARGENTUM_NEXT01_ADAPTER;adapter_sha256=$selectedAdapterSha;exit_code=$LASTEXITCODE;log=$log}
        if((Get-FileHash -LiteralPath $env:ARGENTUM_NEXT01_CLI_EXE).Hash.ToLowerInvariant() -ne $selectedCliSha -or
           (Get-FileHash -LiteralPath $env:ARGENTUM_NEXT01_ADAPTER).Hash.ToLowerInvariant() -ne $selectedAdapterSha){throw 'Selected packaged component changed during its controls.'}
        $results | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding utf8NoBOM
        Get-Content -LiteralPath $log -Tail 20
    }
    if(@($results | Where-Object exit_code -ne 0).Count){throw 'Extension CLI failures retained.'}
} finally {
    foreach($name in $saved.Keys){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
    Pop-Location
}
