# Explicit disposable CI setup. Dot-sourcing is inert and opens no device.
function Invoke-CiNullScope {
    param([Parameter(Mandatory)][scriptblock]$Body, [Parameter(Mandatory)][string]$ReceiptPath)
    if (-not ('ArgentumCiNullScope' -as [type])) {
        Add-Type -Path (Join-Path $PSScriptRoot 'ci-null-scope.cs') -ErrorAction Stop
    }
    [ArgentumCiNullGuard]::RequireHostedWindows()
    $root = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\') + '\'
    $receipt = [IO.Path]::GetFullPath($ReceiptPath)
    if (-not [IO.Path]::IsPathFullyQualified($ReceiptPath) -or
        -not $receipt.StartsWith($root, [StringComparison]::OrdinalIgnoreCase) -or
        (Test-Path -LiteralPath $receipt)) { throw 'Use a new receipt under this disposable runner temp root.' }
    $mutex = [Threading.Mutex]::new($false, 'Local\ArgentumCiNullDacl')
    $held = $false
    $device = $null
    $scope = $null
    $primary = $null
    $cleanup = $null
    $result = $null
    try {
        $held = $mutex.WaitOne(0)
        if (-not $held) { throw 'Another CI NUL scope owns this device.' }
        $device = [ArgentumCiNativeNull]::new()
        $scope = [ArgentumCiNullScope]::new($device)
        $scope.Receipt() | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $receipt -Encoding utf8NoBOM
        $scope.Apply()
        $result = & $Body
    } catch { $primary = $_ }
    finally {
        if ($scope) {
            try { $scope.Restore() } catch { $cleanup = $_ }
            try { $scope.Receipt() | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $receipt -Encoding utf8NoBOM }
            catch { if (-not $cleanup) { $cleanup = $_ } }
        }
        try { if ($scope) { $scope.Dispose() } elseif ($device) { $device.Dispose() } }
        catch { if (-not $cleanup) { $cleanup = $_ } }
        if ($held) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
    if ($cleanup) { throw "CI NUL cleanup failed; not qualified. Primary failure present: $([bool]$primary). $cleanup" }
    if ($primary) { throw $primary }
    $result
}
