[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$workspaceRoot = Split-Path -Parent $PSScriptRoot
$uiRoot = Join-Path $workspaceRoot "ui"
$appSlint = Join-Path $uiRoot "app.slint"
$tokenSlint = Join-Path $uiRoot "tokens.slint"
$brandScript = Join-Path $PSScriptRoot "brand_assets.py"
$failures = [System.Collections.Generic.List[string]]::new()
$rawSpacingDebtLimit = 0
$rawBreakpointDebtLimit = 0

function Add-MatchFailures {
    param(
        [System.IO.FileInfo[]]$Files,
        [string]$Pattern,
        [string]$Message,
        [string[]]$AllowedValues = @()
    )

    foreach ($file in $Files) {
        $lineNumber = 0
        foreach ($line in Get-Content -LiteralPath $file.FullName) {
            $lineNumber += 1
            foreach ($match in [regex]::Matches($line, $Pattern)) {
                if ($AllowedValues.Count -eq 0 -or $AllowedValues -notcontains $match.Groups[1].Value) {
                    $relative = [System.IO.Path]::GetRelativePath($workspaceRoot, $file.FullName)
                    $failures.Add("$Message at ${relative}:$lineNumber")
                }
            }
        }
    }
}

$slintFiles = @(Get-ChildItem -LiteralPath $uiRoot -Filter "*.slint" -File -Recurse)
$nonTokenSlintFiles = @($slintFiles | Where-Object { $_.FullName -ne $tokenSlint })
$appFile = @(Get-Item -LiteralPath $appSlint)

$activeTextFiles = @(
    Get-ChildItem -LiteralPath $workspaceRoot -File |
        Where-Object { $_.Extension -in @(".md", ".toml", ".yml", ".yaml") }
    foreach ($relativeRoot in @(".github", "crates", "docs", "scripts", "ui")) {
        $root = Join-Path $workspaceRoot $relativeRoot
        if (Test-Path -LiteralPath $root) {
            Get-ChildItem -LiteralPath $root -File -Recurse |
                Where-Object {
                    $_.Extension -in @(".json", ".md", ".ps1", ".py", ".rs", ".slint", ".toml", ".yml", ".yaml") -and
                    $_.FullName -notmatch '[\\/]legacy[\\/]'
                }
        }
    }
)

Add-MatchFailures -Files $slintFiles -Pattern "\bradius-pill\b" -Message "Pill radius token is forbidden"
Add-MatchFailures -Files $slintFiles -Pattern 'text\s*:\s*"Ag"' -Message "Synthesized Ag brand mark is forbidden"
Add-MatchFailures -Files $slintFiles -Pattern '(?:name|icon)\s*:\s*"mark"' -Message "Generic mark icon substitute is forbidden"
Add-MatchFailures -Files $slintFiles -Pattern 'argentum-app\.svg' -Message "Retired synthesized app icon is forbidden"
Add-MatchFailures -Files $nonTokenSlintFiles -Pattern '#[0-9A-Fa-f]{6,8}' -Message "Raw colors belong in ui/tokens.slint"
Add-MatchFailures -Files $activeTextFiles -Pattern '[\u2013\u2014]' -Message "Em dash and en dash characters are forbidden"
Add-MatchFailures -Files $activeTextFiles -Pattern '[\u2600-\u27BF]|\p{Cs}' -Message "Emoji characters are forbidden"

$checkIcon = Join-Path $uiRoot "assets\icons\check.svg"
$checkIconText = Get-Content -LiteralPath $checkIcon -Raw
if (-not [regex]::IsMatch($checkIconText, 'stroke-width="1\.7"')) {
    $failures.Add("Check icon must use the standard 1.7 px stroke at ui/assets/icons/check.svg")
}

$rawSpacingDebt = 0
$rawBreakpointDebt = 0
$rawSpacingStatementPattern = '(?m)^\s*(?:spacing|padding(?:-left|-right|-top|-bottom)?)\s*:\s*([^;]+);'
$rawLengthPattern = '\b[0-9]+(?:\.[0-9]+)?px\b'
$dimensionPattern = '\b(?:[A-Za-z_][A-Za-z0-9_-]*\.)*(?:width|height)\b'
$rawBreakpointPattern = "(?:$dimensionPattern\s*(?:<=|>=|<|>)\s*$rawLengthPattern|$rawLengthPattern\s*(?:<=|>=|<|>)\s*$dimensionPattern)"

foreach ($file in $nonTokenSlintFiles) {
    $source = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($statement in [regex]::Matches($source, $rawSpacingStatementPattern)) {
        if ([regex]::IsMatch($statement.Groups[1].Value, $rawLengthPattern)) {
            $rawSpacingDebt += 1
        }
    }
    $rawBreakpointDebt += [regex]::Matches($source, $rawBreakpointPattern).Count
}

if ($rawSpacingDebt -gt $rawSpacingDebtLimit) {
    $failures.Add(
        "Raw spacing debt increased to $rawSpacingDebt declarations; limit is $rawSpacingDebtLimit and release target is 0"
    )
}
if ($rawBreakpointDebt -gt $rawBreakpointDebtLimit) {
    $failures.Add(
        "Raw local breakpoint debt increased to $rawBreakpointDebt comparisons; limit is $rawBreakpointDebtLimit and release target is 0"
    )
}

Write-Output "Design migration debt: raw spacing declarations $rawSpacingDebt/$rawSpacingDebtLimit, release target 0"
Write-Output "Design migration debt: raw local breakpoint comparisons $rawBreakpointDebt/$rawBreakpointDebtLimit, release target 0"
foreach ($file in $appFile) {
    $appText = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($statement in [regex]::Matches($appText, 'font-weight\s*:\s*([^;]+);')) {
        foreach ($weight in [regex]::Matches($statement.Groups[1].Value, '\b([0-9]{3})\b')) {
            if (@("400", "500", "600", "700") -notcontains $weight.Groups[1].Value) {
                $absoluteOffset = $statement.Groups[1].Index + $weight.Index
                $lineNumber = [regex]::Matches(
                    $appText.Substring(0, $absoluteOffset),
                    "\r?\n"
                ).Count + 1
                $relative = [System.IO.Path]::GetRelativePath($workspaceRoot, $file.FullName)
                $failures.Add("Unsupported numeric font weight at ${relative}:$lineNumber")
            }
        }
    }
}

$activeSourceFiles = @(
    Get-ChildItem -LiteralPath $uiRoot -File -Recurse |
        Where-Object { $_.Extension -in @(".slint", ".rs", ".toml") }
    Get-ChildItem -LiteralPath (Join-Path $workspaceRoot "crates\argentum-app") -File -Recurse |
        Where-Object { $_.Extension -in @(".rs", ".toml") }
    Get-ChildItem -LiteralPath (Join-Path $workspaceRoot "crates\argentum-ui") -File -Recurse |
        Where-Object { $_.Extension -in @(".rs", ".toml") }
    Get-ChildItem -LiteralPath (Join-Path $workspaceRoot "crates\argentum-runtime") -File -Recurse |
        Where-Object { $_.Extension -in @(".rs", ".toml") }
)
Add-MatchFailures -Files $activeSourceFiles -Pattern '(?i)(?:^|["''])(?:\.\.[\\/])*legacy[\\/]' -Message "Active source references the legacy tree"

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ }
    throw "Design-system validation failed with $($failures.Count) issue(s)"
}

$python = Get-Command python -ErrorAction SilentlyContinue
if ($null -ne $python) {
    & $python.Source $brandScript
}
else {
    $py = Get-Command py -ErrorAction SilentlyContinue
    if ($null -eq $py) {
        throw "Python with Pillow $([string]::Join('.', @(12, 2, 0))) is required to verify brand assets"
    }
    & $py.Source -3 $brandScript
}
if ($LASTEXITCODE -ne 0) {
    throw "Brand asset validation failed with exit code $LASTEXITCODE"
}

Write-Output "Argentum design-system validation passed"
