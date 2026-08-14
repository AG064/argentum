[CmdletBinding()]
param(
    [switch]$RequireImplementation
)

$ErrorActionPreference = "Stop"

$workspaceRoot = Split-Path -Parent $PSScriptRoot
$uiRoot = Join-Path $workspaceRoot "ui"
$docsRoot = Join-Path $workspaceRoot "docs"
$tokenFile = Join-Path $uiRoot "tokens.slint"
$fixtureDocument = Join-Path $docsRoot "MOBILE_DESIGN_DRAFT.md"
$failures = [System.Collections.Generic.List[string]]::new()
$notices = [System.Collections.Generic.List[string]]::new()

function Get-RelativePath {
    param([string]$Path)

    return [System.IO.Path]::GetRelativePath($workspaceRoot, $Path)
}

function Add-PatternFailures {
    param(
        [System.IO.FileInfo[]]$Files,
        [string]$Pattern,
        [string]$Message
    )

    foreach ($file in $Files) {
        $lineNumber = 0
        foreach ($line in Get-Content -LiteralPath $file.FullName) {
            $lineNumber += 1
            if ([regex]::IsMatch($line, $Pattern)) {
                $failures.Add("$Message at $(Get-RelativePath $file.FullName):$lineNumber")
            }
        }
    }
}

if (-not (Test-Path -LiteralPath $tokenFile -PathType Leaf)) {
    throw "Mobile design validation requires ui/tokens.slint"
}

$slintFiles = @(Get-ChildItem -LiteralPath $uiRoot -Filter "*.slint" -File -Recurse)
$mobileSlintFiles = @(
    $slintFiles | Where-Object {
        $_.BaseName -match '(?i)mobile|phone' -or
        [regex]::IsMatch(
            (Get-Content -LiteralPath $_.FullName -Raw),
            '\b(?:export\s+)?component\s+Mobile[A-Za-z0-9_]+'
        )
    }
)
$implementationPresent = $mobileSlintFiles.Count -gt 0
$mobileShellPresent = $false
foreach ($file in $mobileSlintFiles) {
    if ([regex]::IsMatch(
        (Get-Content -LiteralPath $file.FullName -Raw),
        '\b(?:export\s+)?component\s+MobileShell\b'
    )) {
        $mobileShellPresent = $true
        break
    }
}
$strictMode = $RequireImplementation.IsPresent -or $mobileShellPresent
$tokenText = Get-Content -LiteralPath $tokenFile -Raw

# Stable foundation checks apply before the dedicated mobile shell lands.
if (-not [regex]::IsMatch($tokenText, 'out\s+property\s+<length>\s+space-12\s*:\s*48px\s*;')) {
    $failures.Add("The four-pixel grid must expose space-12 as 48px in ui/tokens.slint")
}
if (-not [regex]::IsMatch($tokenText, 'out\s+property\s+<length>\s+tablet-breakpoint\s*:\s*768px\s*;')) {
    $failures.Add("The phone-to-tablet boundary must remain a named 768px token in ui/tokens.slint")
}
if (-not [regex]::IsMatch($tokenText, 'out\s+property\s+<length>\s+mobile-standard-breakpoint\s*:\s*380px\s*;')) {
    $failures.Add("The standard-phone boundary must remain a named 380px token in ui/tokens.slint")
}

# Pill and oval geometry is forbidden in every active Slint source. Compact
# rectangular controls may still use the named small-radius tokens.
$pillGeometryPattern = '(?i)(?:radius-(?:pill|oval)|border-radius\s*:\s*(?:999(?:9+)?px|50%|(?:[A-Za-z_][A-Za-z0-9_-]*\.)*(?:width|height)\s*/\s*2))'
Add-PatternFailures -Files $slintFiles -Pattern $pillGeometryPattern -Message "Pill or oval geometry is forbidden"

# Keep the mobile surface and its fixture contract free of punctuation and
# pictographs that violate the repository writing rules.
$mobileTextFiles = [System.Collections.Generic.List[System.IO.FileInfo]]::new()
foreach ($file in $mobileSlintFiles) {
    $mobileTextFiles.Add($file)
}
if (Test-Path -LiteralPath $fixtureDocument -PathType Leaf) {
    $mobileTextFiles.Add((Get-Item -LiteralPath $fixtureDocument))
}
if ($mobileTextFiles.Count -gt 0) {
    Add-PatternFailures -Files @($mobileTextFiles) -Pattern '[\u2013\u2014]' -Message "Em dash and en dash characters are forbidden"
    Add-PatternFailures -Files @($mobileTextFiles) -Pattern '[\u2600-\u27BF]|\p{Cs}' -Message "Emoji characters are forbidden"
}

if ($strictMode) {
    if (-not $implementationPresent) {
        $failures.Add("No Mobile* Slint component was found under ui")
    }

    if (-not [regex]::IsMatch($tokenText, 'out\s+property\s+<length>\s+mobile-control-height\s*:\s*48px\s*;')) {
        $failures.Add("Mobile controls require semantic token mobile-control-height: 48px")
    }
    if (-not [regex]::IsMatch($tokenText, 'out\s+property\s+<length>\s+mobile-header-height\s*:\s*[0-9]+px\s*;')) {
        $failures.Add("The mobile shell requires a named mobile-header-height token")
    }

    $mobileSource = ($mobileSlintFiles | ForEach-Object {
        Get-Content -LiteralPath $_.FullName -Raw
    }) -join "`n"
    if (-not $mobileShellPresent) {
        $failures.Add("The mobile presentation requires a MobileShell component")
    }
    if (-not [regex]::IsMatch($mobileSource, '\bAgTheme\.mobile-control-height\b')) {
        $failures.Add("Mobile interactive surfaces must consume AgTheme.mobile-control-height")
    }
}
else {
    if ($implementationPresent) {
        $notices.Add("Mobile components are present without MobileShell; foundation checks ran while integration is in progress")
    }
    else {
        $notices.Add("Dedicated Mobile* Slint components are not present; foundation checks ran only")
    }
}

# Once the dedicated contract document exists, its exact viewport fixtures are
# mandatory. Whitespace around the separator is intentionally tolerated.
if (Test-Path -LiteralPath $fixtureDocument -PathType Leaf) {
    $fixtureText = Get-Content -LiteralPath $fixtureDocument -Raw
    foreach ($fixture in @(
        @{ Label = "360x620"; Pattern = '\b360\s*x\s*620\b' },
        @{ Label = "430x800"; Pattern = '\b430\s*x\s*800\b' },
        @{ Label = "800x360"; Pattern = '\b800\s*x\s*360\b' }
    )) {
        if (-not [regex]::IsMatch($fixtureText, $fixture.Pattern, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)) {
            $failures.Add("Mobile fixture $($fixture.Label) is missing from docs/MOBILE_DESIGN_DRAFT.md")
        }
    }
}
elseif ($RequireImplementation.IsPresent) {
    $failures.Add("Strict mobile validation requires docs/MOBILE_DESIGN_DRAFT.md")
}
else {
    $notices.Add("docs/MOBILE_DESIGN_DRAFT.md is not present; viewport fixture checks are pending")
}

Write-Output "Mobile design validation mode: $(if ($strictMode) { 'integrated' } else { 'foundation' })"
Write-Output "Mobile Slint files detected: $($mobileSlintFiles.Count)"
foreach ($notice in $notices) {
    Write-Output "NOTICE: $notice"
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ }
    throw "Mobile design validation failed with $($failures.Count) issue(s)"
}

Write-Output "Argentum mobile design validation passed"
