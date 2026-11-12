[CmdletBinding()]
param([Parameter(Mandatory)][string]$OutputDirectory)
Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
$source=(Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
$output=[IO.Path]::GetFullPath($OutputDirectory)
if(-not [IO.Path]::IsPathFullyQualified($OutputDirectory) -or (Test-Path -LiteralPath $output) -or
   $output.Equals($source,[StringComparison]::OrdinalIgnoreCase) -or
   $output.StartsWith($source.TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)){
    throw 'Use a fresh absolute asset directory outside source. Existing assets are never overwritten.'
}
$pins=Get-Content -Raw -LiteralPath (Join-Path $source 'third-party/local-artifact-assets.json') | ConvertFrom-Json
New-Item -ItemType Directory -Path $output | Out-Null
function Get-PinnedAsset([string]$Url,[string]$Destination,[long]$Bytes,[string]$Sha256){
    if(-not $Url.StartsWith('https://',[StringComparison]::Ordinal) -or (Test-Path -LiteralPath $Destination)){throw 'Invalid create-new pinned asset destination'}
    $client=[Net.Http.HttpClient]::new()
    $client.Timeout=[TimeSpan]::FromSeconds(90)
    $deadline=[Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(90))
    $response=$null;$stream=$null;$file=$null
    try {
        $response=$client.GetAsync($Url,[Net.Http.HttpCompletionOption]::ResponseHeadersRead,$deadline.Token).GetAwaiter().GetResult()
        $response.EnsureSuccessStatusCode() | Out-Null
        if($null -ne $response.Content.Headers.ContentLength -and $response.Content.Headers.ContentLength -gt $Bytes){throw 'Asset Content-Length exceeds the pin'}
        $stream=$response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
        $file=[IO.FileStream]::new($Destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        $buffer=[byte[]]::new(65536);$total=0L
        while(($read=$stream.ReadAsync($buffer,0,$buffer.Length,$deadline.Token).GetAwaiter().GetResult()) -gt 0){
            $total+=$read
            if($total -gt $Bytes){throw 'Asset transfer exceeded its pinned byte limit'}
            $file.Write($buffer,0,$read)
        }
        $file.Flush($true)
    } finally {
        if($file){$file.Dispose()};if($stream){$stream.Dispose()};if($response){$response.Dispose()};$client.Dispose();$deadline.Dispose()
    }
    if((Get-Item -LiteralPath $Destination).Length -ne $Bytes -or
       (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Sha256){
        throw 'Pinned asset identity mismatch. Failed bytes retained; nothing installed.'
    }
}
$archive=Join-Path $output 'pdfium-win-x64.tgz'
Get-PinnedAsset $pins.pdfium.url $archive $pins.pdfium.archive_bytes $pins.pdfium.archive_sha256
$entries=@(& tar -tf $archive)
if($LASTEXITCODE -ne 0 -or $entries.Count -gt 100 -or
   @($entries | Where-Object {$_ -match '(^/|^[A-Za-z]:|\\|(^|/)\.\.(/|$))'}).Count){throw 'Unsafe fixed PDFium archive inventory'}
& tar -xf $archive -C $output
if($LASTEXITCODE -ne 0){throw 'Pinned PDFium extraction failed'}
if((Get-FileHash -LiteralPath (Join-Path $output 'bin/pdfium.dll') -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pins.pdfium.dll_sha256){throw 'PDFium DLL identity mismatch'}
$argsText=Get-Content -Raw -LiteralPath (Join-Path $output 'args.gn')
if($argsText -notmatch '(?m)^pdf_enable_v8 = false\s*$' -or $argsText -notmatch '(?m)^pdf_enable_xfa = false\s*$'){
    throw 'Pinned non-V8/non-XFA build declaration unavailable'
}
Get-PinnedAsset $pins.font.url (Join-Path $output 'NotoSans.ttf') $pins.font.bytes $pins.font.sha256
Get-PinnedAsset $pins.font.license_url (Join-Path $output 'OFL.txt') $pins.font.license_bytes $pins.font.license_sha256
Copy-Item -LiteralPath (Join-Path $source 'third-party/local-artifact-assets.json') -Destination (Join-Path $output 'asset-provenance.json')
[pscustomobject]@{directory=$output;pdfium_sha256=$pins.pdfium.dll_sha256;font_sha256=$pins.font.sha256;global_install=$false;account_access=$false}
