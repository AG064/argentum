# Bounded public diagnostics for credential-free, task-owned test execution.
function Export-TestDiagnostics {
    param([Parameter(Mandatory)][string]$EvidenceDirectory, [string[]]$PrivateRoots = @())
    $diagnostics = Join-Path $EvidenceDirectory 'diagnostics'
    New-Item -ItemType Directory -Path $diagnostics -Force | Out-Null
    $records = @()
    foreach ($log in @(Get-ChildItem -LiteralPath $EvidenceDirectory -Filter '*.log' -File)) {
        $tail = @(Get-Content -LiteralPath $log.FullName -Tail 160 | ForEach-Object {
            $line = [regex]::Replace($_, '\x1b\[[0-9;]*[A-Za-z]', '')
            foreach ($root in $PrivateRoots) {
                if ($root) { $line = $line.Replace($root, '<private-root>') }
            }
            $line = [regex]::Replace($line, '(?i)\b(?:sk-(?:proj-|ant-)?[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9_]{16,}|AKIA[A-Z0-9]{16})\b', '<credential-redacted>')
            $line = [regex]::Replace($line, '(?i)(Bearer\s+)[^\s"'']+', '$1<credential-redacted>')
            $line = [regex]::Replace($line, '(?i)((?:api[_-]?key|access[_-]?token|password|secret)\s*[:=]\s*)[^\s,;]+', '$1<credential-redacted>')
            if ($line.Length -gt 2048) { $line = $line.Substring(0, 2048) + ' <line-truncated>' }
            $line
        })
        $tail | Set-Content -LiteralPath (Join-Path $diagnostics $log.Name) -Encoding utf8NoBOM
        $records += [ordered]@{ name=$log.Name; raw_bytes=$log.Length; raw_sha256=(Get-FileHash -LiteralPath $log.FullName -Algorithm SHA256).Hash.ToLowerInvariant(); retained_tail_lines=$tail.Count }
    }
    $records | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $diagnostics 'log-index.json') -Encoding utf8NoBOM
    if (Test-Path -LiteralPath (Join-Path $EvidenceDirectory 'environment.json')) {
        Copy-Item -LiteralPath (Join-Path $EvidenceDirectory 'environment.json') -Destination $diagnostics
    }
    if (Test-Path -LiteralPath (Join-Path $EvidenceDirectory 'results.json')) {
        $public = @(Get-Content -LiteralPath (Join-Path $EvidenceDirectory 'results.json') -Raw | ConvertFrom-Json | ForEach-Object {
            [ordered]@{ target=$_.target; executable_sha256=$_.executable_sha256; exit_code=$_.exit_code; started=$_.started; ended=$_.ended; log=[IO.Path]::GetFileName($_.log) }
        })
        $public | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $diagnostics 'results.json') -Encoding utf8NoBOM
    }
}
