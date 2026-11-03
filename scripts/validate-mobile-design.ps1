[CmdletBinding()]
param(
    [switch]$RequireImplementation
)

$ErrorActionPreference = "Stop"

$workspaceRoot = Split-Path -Parent $PSScriptRoot
$uiRoot = Join-Path $workspaceRoot "ui"
$docsRoot = Join-Path $workspaceRoot "docs"
$tokenFile = Join-Path $uiRoot "tokens.slint"
$themeFile = Join-Path $uiRoot "workbench\theme.slint"
$shellFile = Join-Path $uiRoot "workbench\shell.slint"
$fixtureDocument = Join-Path $docsRoot "MOBILE_DESIGN_DRAFT.md"
$failures = [System.Collections.Generic.List[string]]::new()
$notices = [System.Collections.Generic.List[string]]::new()

# Only what the app compiles is held to these rules.
function Get-ActiveSlintFiles {
    param([string]$UiRoot)

    $active = @(
        (Join-Path $UiRoot "app.slint"),
        (Join-Path $UiRoot "view-models.slint"),
        (Join-Path $UiRoot "tokens.slint"),
        (Join-Path $UiRoot "components\icon.slint"),
        (Join-Path $UiRoot "components\brand.slint")
    ) | ForEach-Object { Get-Item -LiteralPath $_ }
    return @($active) + @(Get-ChildItem -LiteralPath (Join-Path $UiRoot "workbench") -Filter "*.slint" -File)
}

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

$slintFiles = @(Get-ActiveSlintFiles -UiRoot $uiRoot)
# One responsive shell serves every width, so phone layout has no separate component.
$mobileSlintFiles = @($slintFiles | Where-Object { $_.FullName -eq $shellFile })
$implementationPresent = $mobileSlintFiles.Count -gt 0
$mobileShellPresent = $implementationPresent
$strictMode = $implementationPresent -or $RequireImplementation.IsPresent

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
        $failures.Add("The Workbench shell was not found under ui/workbench")
    }
    $themeText = Get-Content -LiteralPath $themeFile -Raw
    $shellText = Get-Content -LiteralPath $shellFile -Raw
    if (-not [regex]::IsMatch($themeText, 'out\s+property\s+<length>\s+touch\s*:\s*48px\s*;')) {
        $failures.Add("Touch controls require the semantic token touch: 48px")
    }
    if (-not [regex]::IsMatch($themeText, 'out\s+property\s+<length>\s+narrow-below\s*:\s*768px\s*;')) {
        $failures.Add("The phone-to-tablet boundary must remain a named 768px token")
    }
    if (-not [regex]::IsMatch($themeText, 'control:\s*coarse\s*\?\s*touch')) {
        $failures.Add("Controls must grow to the touch target at narrow widths")
    }
    if ($shellText -notmatch 'Theme\.coarse\s*=') {
        $failures.Add("The shell must switch to touch sizing below the narrow breakpoint")
    }
    if ($shellText -notmatch 'App\.drawer_open && root\.narrow') {
        $failures.Add("The sidebar must become an overlay at narrow widths")
    }
    if ($shellText -notmatch 'root\.panel-wanted && !root\.wide-enough-for-panel') {
        $failures.Add("The side panel must become an overlay when it does not fit")
    }
    $appText = Get-Content -LiteralPath (Join-Path $uiRoot "app.slint") -Raw
    if ($appText -notmatch 'root\.height - root\.safe_area_top - root\.safe_area_bottom - root\.keyboard_inset') {
        $failures.Add("The shell must stay clear of safe areas and the on-screen keyboard")
    }
}
else {
    $notices.Add("The Workbench shell is not present; foundation checks ran only")
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
