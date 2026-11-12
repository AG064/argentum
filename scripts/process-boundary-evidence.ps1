# Test-runner evidence only. This does not authorize or execute a binary.
function Save-ProcessBoundaryFailureExecutable {
    param(
        [Parameter(Mandatory)][string]$Executable,
        [Parameter(Mandatory)][string]$ExpectedSha256,
        [Parameter(Mandatory)][string]$OutputDirectory,
        [Parameter(Mandatory)][hashtable]$BuildIdentity,
        [AllowNull()][Nullable[int]]$OriginalExitCode
    )
    $record=[ordered]@{
        schema_version=1;kind='failed_process_boundary_executable'
        status='not_retained';reason='not_attempted';maximum_bytes=64MB
        original_exit_code=$OriginalExitCode;expected_sha256=$ExpectedSha256;raw_sha256=$null
        bytes=$null;relative_path=$null;build=$BuildIdentity
    }
    if($null -ne $OriginalExitCode -and $OriginalExitCode -eq 0){
        $record.status='not_needed';$record.reason='diagnostic_passed'
        return [pscustomobject]$record
    }
    $inputStream=$null;$outputStream=$null;$hasher=$null
    $receipt=Join-Path $OutputDirectory 'diagnostics\failed-executable.json'
    if(Test-Path -LiteralPath $receipt){
        $record.reason='prior_receipt_preserved'
        Write-Warning 'Diagnostic executable retention not attempted: prior receipt preserved.' -WarningAction Continue
        return [pscustomobject]$record
    }
    try {
        if(-not [IO.Path]::IsPathFullyQualified($Executable) -or $ExpectedSha256 -cnotmatch '^[0-9a-f]{64}$'){
            $record.reason='invalid_selected_identity';throw 'Invalid selected identity'
        }
        $file=Get-Item -LiteralPath $Executable -ErrorAction Stop
        if($file.PSIsContainer -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)){
            $record.reason='selected_file_not_regular';throw 'Selected file is not regular'
        }
        # Hold the selected bytes without write/delete sharing through retention.
        $inputStream=[IO.File]::Open($file.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
        $record.bytes=$inputStream.Length
        if($inputStream.Length -gt $record.maximum_bytes){
            $record.reason='selected_file_exceeds_bound';throw 'Selected file exceeds bound'
        }
        $hasher=[Security.Cryptography.SHA256]::Create()
        $observed=[Convert]::ToHexString($hasher.ComputeHash($inputStream)).ToLowerInvariant()
        $record.raw_sha256=$observed
        if($observed -cne $ExpectedSha256){
            $record.reason='selected_identity_changed';throw 'Selected identity changed'
        }
        $inputStream.Position=0
        $record.reason='retention_io_failed'
        $directory=Join-Path $OutputDirectory 'diagnostics\failed-executable'
        New-Item -ItemType Directory -Path $directory -Force -ErrorAction Stop|Out-Null
        $destination=Join-Path $directory 'argentum-execution-tests.exe'
        $outputStream=[IO.File]::Open($destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        $inputStream.CopyTo($outputStream)
        $outputStream.Flush($true)
        $outputStream.Dispose();$outputStream=$null
        if((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant() -cne $ExpectedSha256){
            $record.reason='retained_identity_mismatch';throw 'Retained identity mismatch'
        }
        $record.relative_path='failed-executable/argentum-execution-tests.exe'
        $record.status='retained';$record.reason='selected_failed_diagnostic'
    }catch{
        if($record.reason -eq 'not_attempted'){$record.reason='selected_file_unavailable'}
        # Preserve the original test/launcher outcome; this is additional evidence.
        Write-Warning ('Diagnostic executable retention unavailable: '+$record.reason) -WarningAction Continue
    }finally{
        if($outputStream){$outputStream.Dispose()}
        if($inputStream){$inputStream.Dispose()}
        if($hasher){$hasher.Dispose()}
        try {
            New-Item -ItemType Directory -Path (Split-Path -Parent $receipt) -Force -ErrorAction Stop|Out-Null
            $json=$record|ConvertTo-Json -Depth 8
            $receiptStream=[IO.File]::Open($receipt,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
            try {
                $bytes=[Text.UTF8Encoding]::new($false).GetBytes($json)
                $receiptStream.Write($bytes,0,$bytes.Length)
            }finally{$receiptStream.Dispose()}
        }catch{Write-Warning 'Diagnostic executable retention receipt unavailable; original outcome unchanged.' -WarningAction Continue}
    }
    return [pscustomobject]$record
}
