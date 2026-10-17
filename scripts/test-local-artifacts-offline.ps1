[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$AssetsDirectory,
    [string]$TargetDirectory='',
    [string]$CliExecutable='',
    [string]$WorkerExecutable='',
    [switch]$PackagedAssets,
    [switch]$CiNullAccess
)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$source=(Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$output=[IO.Path]::GetFullPath($OutputDirectory)
if(-not [IO.Path]::IsPathFullyQualified($OutputDirectory) -or (Test-Path -LiteralPath $output) -or
   $output.Equals($source,[StringComparison]::OrdinalIgnoreCase) -or $output.StartsWith($source.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)){
    throw 'Use a fresh absolute evidence directory outside source.'
}
if(-not [IO.Path]::IsPathFullyQualified($AssetsDirectory)){throw 'Explicit prepared artifact assets required'}
New-Item -ItemType Directory -Path $output | Out-Null
$targetArgs=@()
if($TargetDirectory){if(-not [IO.Path]::IsPathFullyQualified($TargetDirectory)){throw 'Absolute compiler target required'};$targetArgs=@('--target-dir',$TargetDirectory)}
$saved=@{}
Push-Location $source
try {
    foreach($selection in @(@{name='artifact';args=@('-p','argentum-artifacts','--all-targets')},@{name='cli';args=@('-p','argentum-cli','--features','artifacts','--test','artifacts_cli')},@{name='provider';args=@('-p','argentum-providers','--lib')})){
        $log=Join-Path $output ($selection.name+'-compiler.jsonl')
        & cargo test @($selection.args) --no-run --message-format=json --locked --offline -j 1 @targetArgs 1> $log 2> (Join-Path $output ($selection.name+'-compile.log'))
        if($LASTEXITCODE -ne 0){throw 'Artifact control compilation failed; retain its log'}
    }
    $metadata=& cargo metadata --no-deps --format-version 1 --locked --offline | ConvertFrom-Json -Depth 50
    if($LASTEXITCODE -ne 0){throw 'Compiler output discovery failed'}
    $target=if($TargetDirectory){$TargetDirectory}else{$metadata.target_directory}
    if(-not $CliExecutable){$CliExecutable=Join-Path $target 'debug/argentum.exe'}
    if(-not $WorkerExecutable){$WorkerExecutable=Join-Path $target 'debug/argentum-pdf-worker.exe'}
    $saved['PATH']=$env:PATH
    $env:PATH=(Split-Path -Parent $CliExecutable)+[IO.Path]::PathSeparator+$env:PATH
    $dllRelative=if($PackagedAssets){'pdfium.dll'}else{'bin/pdfium.dll'}
    $pdfium=Join-Path $AssetsDirectory $dllRelative
    foreach($file in @($CliExecutable,$WorkerExecutable,$pdfium,(Join-Path $AssetsDirectory 'NotoSans.ttf'))){
        if(-not [IO.Path]::IsPathFullyQualified($file) -or -not (Test-Path -LiteralPath $file -PathType Leaf)){throw 'An explicit qualification component is unavailable'}
    }
    foreach($item in @(Get-ChildItem Env: | Where-Object Name -Match '(API_KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|ACCESS_KEY)')){
        $saved[$item.Name]=$item.Value;Remove-Item -LiteralPath ('Env:'+$item.Name)
    }
    $values=@{ARGENTUM_TEST_ARTIFACT_CLI_EXE=$CliExecutable;ARGENTUM_TEST_ARTIFACT_WORKER_EXE=$WorkerExecutable;
        ARGENTUM_TEST_PDFIUM_DLL=$pdfium;ARGENTUM_TEST_ARTIFACT_FONT=(Join-Path $AssetsDirectory 'NotoSans.ttf');
        ARGENTUM_TEST_ARTIFACT_RETAIN=(Join-Path $output 'roundtrip')}
    foreach($name in $values.Keys){if(-not $saved.ContainsKey($name)){$saved[$name]=[Environment]::GetEnvironmentVariable($name,'Process')};[Environment]::SetEnvironmentVariable($name,$values[$name],'Process')}
    $saved['ARGENTUM_TEST_ARTIFACT_PNG']=[Environment]::GetEnvironmentVariable('ARGENTUM_TEST_ARTIFACT_PNG','Process')
    $artifacts=@(Get-Content -LiteralPath (Join-Path $output 'artifact-compiler.jsonl'),(Join-Path $output 'cli-compiler.jsonl') | ForEach-Object {
        try{$_ | ConvertFrom-Json -Depth 50}catch{return}
    } | Where-Object {$_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable})
    $names=@('argentum_artifacts','artifact_contract','pdf_isolation','artifacts_cli')
    $selected=@($artifacts | Where-Object {$names -contains $_.target.name} | Sort-Object executable -Unique)
    if($selected.Count -ne $names.Count){throw 'Each current control executable must have a compiler-authored identity'}
    $body={
        # The fixed verifier resolver intentionally rejects executables under
        # CWD. CI builds under source/target; run controls in their disposable
        # evidence directory, not the source checkout. Keep the resolver intact.
        Push-Location $output
        try {
        $results=[Collections.Generic.List[object]]::new()
        foreach($artifact in $selected){
            $log=Join-Path $output ($artifact.target.name+'.log')
            & $artifact.executable --include-ignored --test-threads=1 *> $log
            $results.Add([ordered]@{target=$artifact.target.name;exit_code=$LASTEXITCODE;log=$log;
                executable_sha256=(Get-FileHash -LiteralPath $artifact.executable).Hash.ToLowerInvariant();
                cli_sha256=(Get-FileHash -LiteralPath $CliExecutable).Hash.ToLowerInvariant();worker_sha256=(Get-FileHash -LiteralPath $WorkerExecutable).Hash.ToLowerInvariant()})
            $results | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding utf8NoBOM
            Get-Content -LiteralPath $log -Tail 12 | Write-Host
        }
        $images=@(Get-ChildItem -LiteralPath (Join-Path $output 'roundtrip') -File -Filter '*.png' | Sort-Object Name)
        if(-not $images.Count){throw 'The real PDF roundtrip did not retain an actual page PNG'}
        $env:ARGENTUM_TEST_ARTIFACT_PNG=$images[0].FullName
        $provider=@(Get-Content -LiteralPath (Join-Path $output 'provider-compiler.jsonl') | ForEach-Object {
            try{$_ | ConvertFrom-Json -Depth 50}catch{return}
        } | Where-Object {$_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable -and $_.target.name -eq 'argentum_providers'})
        if($provider.Count -ne 1){throw 'The canonical provider control executable is unavailable'}
        $log=Join-Path $output 'actual-png-provider-wire.log'
        & $provider[0].executable 'png_reaches_actual_responses_input_and_wire_accounting' --include-ignored --test-threads=1 *> $log
        $results.Add([ordered]@{target='canonical-provider-actual-png';exit_code=$LASTEXITCODE;log=$log;
            executable_sha256=(Get-FileHash -LiteralPath $provider[0].executable).Hash.ToLowerInvariant();
            png_sha256=(Get-FileHash -LiteralPath $images[0].FullName).Hash.ToLowerInvariant();
            boundary='canonical Responses serialization and accounting, not a live provider or one synthetic image-capable CLI transport'})
        $results | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'results.json') -Encoding utf8NoBOM
        Get-Content -LiteralPath $log -Tail 12 | Write-Host
        if(@($results | Where-Object exit_code -ne 0).Count){return 1};return 0
        } finally { Pop-Location }
    }
    if($CiNullAccess){. (Join-Path $PSScriptRoot 'ci-null-scope.ps1');$code=Invoke-CiNullScope -Body $body -ReceiptPath (Join-Path $output 'null-scope.json')}
    else{$code=& $body}
    if($code -ne 0){throw 'Offline artifact failures retained'}
} finally {
    foreach($name in $saved.Keys){[Environment]::SetEnvironmentVariable($name,$saved[$name],'Process')}
    Pop-Location
}
