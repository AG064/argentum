[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$workspaceRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $workspaceRoot "ui\assets\icons\catalog.json"
$galleryPath = Join-Path $workspaceRoot "ui\gallery\icon-gallery.slint"
$runnerPath = Join-Path $workspaceRoot "crates\argentum-icon-gallery\src\main.rs"
$appPath = Join-Path $workspaceRoot "ui\app.slint"
$iconComponentPath = Join-Path $workspaceRoot "ui\components\icon.slint"
$mobileComponentPath = Join-Path $workspaceRoot "ui\components\mobile.slint"
$conversationSurfacePath = Join-Path $workspaceRoot "ui\surfaces\conversation.slint"
$failures = [System.Collections.Generic.List[string]]::new()
$requiredRoles = @(
    "new-task",
    "search",
    "sessions",
    "activity",
    "plan",
    "trajectory",
    "changes",
    "inspector",
    "provider",
    "model",
    "workspace",
    "approval",
    "run",
    "stop",
    "settings",
    "more"
)
$productionIconMap = [ordered]@{
    "new-task" = "square-pen.svg"
    "search" = "search.svg"
    "sessions" = "message-square.svg"
    "activity" = "activity.svg"
    "plan" = "list-checks.svg"
    "trajectory" = "git-branch.svg"
    "changes" = "file-diff.svg"
    "inspector" = "panel-right.svg"
    "provider" = "server.svg"
    "model" = "cpu.svg"
    "workspace" = "folder.svg"
    "approval" = "shield-check.svg"
    "run" = "play.svg"
    "stop" = "square.svg"
    "settings" = "sliders-horizontal.svg"
    "more" = "ellipsis.svg"
}

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Icon catalog is missing at ui/assets/icons/catalog.json"
}

$catalog = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if ($catalog.schema_version -ne 1) {
    $failures.Add("Icon catalog schema_version must be 1")
}
if ($catalog.source.name -ne "Lucide") {
    $failures.Add("Icon catalog source must be Lucide")
}
if ($catalog.source.commit -notmatch '^[0-9a-f]{40}$') {
    $failures.Add("Icon catalog must pin a full upstream commit")
}
if ($catalog.source.license -ne "ISC") {
    $failures.Add("Icon catalog must record the Lucide ISC license")
}

$entries = @($catalog.icons)
$roleNames = @($entries | ForEach-Object { $_.role })
foreach ($role in $requiredRoles) {
    if ($roleNames -notcontains $role) {
        $failures.Add("Icon catalog is missing required role '$role'")
    }
}
if (($roleNames | Sort-Object -Unique).Count -ne $roleNames.Count) {
    $failures.Add("Icon catalog roles must be unique")
}

foreach ($entry in $entries) {
    if ($entry.file -notmatch '^lucide/[a-z0-9-]+\.svg$') {
        $failures.Add("Icon '$($entry.role)' has an invalid local file path")
        continue
    }

    $assetPath = Join-Path (Split-Path -Parent $manifestPath) $entry.file
    if (-not (Test-Path -LiteralPath $assetPath -PathType Leaf)) {
        $failures.Add("Icon asset is missing for role '$($entry.role)': $($entry.file)")
        continue
    }

    $svg = Get-Content -LiteralPath $assetPath -Raw
    if ($svg -notmatch 'viewBox="0 0 24 24"') {
        $failures.Add("Icon '$($entry.role)' must use the canonical 24 px Lucide viewBox")
    }
    if ($svg -match '<(?:script|image|text)\b|(?:href|xlink:href)\s*=') {
        $failures.Add("Icon '$($entry.role)' contains forbidden active, raster, text, or linked content")
    }
    $actualHash = (Get-FileHash -LiteralPath $assetPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $entry.sha256) {
        $failures.Add("Icon '$($entry.role)' does not match its recorded SHA-256")
    }
}

foreach ($requiredFile in @($galleryPath, $runnerPath)) {
    if (-not (Test-Path -LiteralPath $requiredFile -PathType Leaf)) {
        $relative = [System.IO.Path]::GetRelativePath($workspaceRoot, $requiredFile)
        $failures.Add("Required icon gallery file is missing: $relative")
    }
}

if (Test-Path -LiteralPath $galleryPath -PathType Leaf) {
    $gallerySource = Get-Content -LiteralPath $galleryPath -Raw
    if ($gallerySource -notmatch '(?s)width:\s*AgTheme\.accent-bar-width;\s*height:\s*parent\.height;\s*x:\s*0px;') {
        $failures.Add("Gallery state accent bars must be anchored to the left edge")
    }
}

if (Test-Path -LiteralPath $appPath -PathType Leaf) {
    $appSource = Get-Content -LiteralPath $appPath -Raw
    if ($appSource -match 'icon-gallery|IconGallery') {
        $failures.Add("Production app.slint must not import the development icon gallery")
    }
    if ($appSource -match 'BrandLockup\s*\{') {
        $failures.Add("Persistent navigation must use a text product label, not the Argentum mark")
    }
}

if (Test-Path -LiteralPath $mobileComponentPath -PathType Leaf) {
    $mobileComponentSource = Get-Content -LiteralPath $mobileComponentPath -Raw
    if ($mobileComponentSource -match 'BrandMark\s*\{') {
        $failures.Add("Mobile navigation must not reuse the Argentum mark as a control glyph")
    }
}

if (Test-Path -LiteralPath $conversationSurfacePath -PathType Leaf) {
    $conversationSurfaceSource = Get-Content -LiteralPath $conversationSurfacePath -Raw
    $conversationBrandMarkCount = ([regex]::Matches($conversationSurfaceSource, 'BrandMark\s*\{')).Count
    if ($conversationBrandMarkCount -ne 1) {
        $failures.Add("Conversation UI must reserve exactly one Argentum mark for the new-task empty state")
    }
}

if (-not (Test-Path -LiteralPath $iconComponentPath -PathType Leaf)) {
    $failures.Add("Production AgIcon component is missing")
} else {
    $iconComponentSource = Get-Content -LiteralPath $iconComponentPath -Raw
    foreach ($role in $productionIconMap.Keys) {
        $escapedRole = [regex]::Escape($role)
        $escapedFile = [regex]::Escape($productionIconMap[$role])
        if ($iconComponentSource -notmatch "root\.name\s*==\s*`"$escapedRole`"") {
            $failures.Add("Production AgIcon is missing semantic role '$role'")
        }
        if ($iconComponentSource -notmatch "assets/icons/lucide/$escapedFile") {
            $failures.Add("Production AgIcon role '$role' is not backed by lucide/$($productionIconMap[$role])")
        }
    }
}

$legacyProductionRoles = "plus|session|timeline|diff|review|panel|lock|play|sliders|folder"
$productionSlintFiles = Get-ChildItem -LiteralPath (Join-Path $workspaceRoot "ui") -Recurse -Filter "*.slint" |
    Where-Object {
        $_.FullName -ne $iconComponentPath -and
        $_.FullName -notlike "*\ui\gallery\*"
    }
foreach ($productionSlintFile in $productionSlintFiles) {
    $source = Get-Content -LiteralPath $productionSlintFile.FullName -Raw
    $legacyMatches = [regex]::Matches($source, "(?:name|icon|glyph)\s*:\s*`"($legacyProductionRoles)`"")
    foreach ($legacyMatch in $legacyMatches) {
        $relative = [System.IO.Path]::GetRelativePath($workspaceRoot, $productionSlintFile.FullName)
        $failures.Add("Production UI uses legacy icon role '$($legacyMatch.Groups[1].Value)' in $relative")
    }
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ }
    throw "Icon-system validation failed with $($failures.Count) issue(s)"
}

Write-Output "Argentum icon-system validation passed for $($entries.Count) curated SVG roles"
