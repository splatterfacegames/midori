param(
    [string]$HandoffDir = "target/midori_engine_validation_handoff",
    [string]$ValidationRoot = "target/midori_engine_validation",
    [string]$PackageName = "forest_floor",
    [switch]$AllowPartial,
    [switch]$AllowPending,
    [switch]$SkipVerify
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$handoffPath = if ([System.IO.Path]::IsPathRooted($HandoffDir)) {
    [System.IO.Path]::GetFullPath($HandoffDir)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $HandoffDir))
}
$validationRootPath = if ([System.IO.Path]::IsPathRooted($ValidationRoot)) {
    [System.IO.Path]::GetFullPath($ValidationRoot)
}
else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $ValidationRoot))
}

$handoffValidationRoot = Join-Path $handoffPath "validation_root"
$handoffScreenshots = Join-Path $handoffPath "docs\validation\screenshots"
$handoffProfileNotes = Join-Path $handoffPath "docs\validation\midori-nature-engine-profile-notes.md"
$handoffUnityReport = Join-Path $handoffValidationRoot "$($PackageName)_unity_import_report.json"
$handoffUnrealReport = Join-Path $handoffValidationRoot "$($PackageName)_unreal_editor_report.json"
$handoffSummary = Join-Path $handoffValidationRoot "editor_handoff_summary.json"
$handoffVerifierReport = Join-Path $handoffValidationRoot "engine_evidence_verification_editor.json"

$canonicalUnityReport = Join-Path $validationRootPath "$($PackageName)_unity_import_report.json"
$canonicalUnrealReport = Join-Path $validationRootPath "$($PackageName)_unreal_editor_report.json"
$canonicalSummary = Join-Path $validationRootPath "editor_handoff_summary.json"
$canonicalVerifierReport = Join-Path $validationRootPath "engine_evidence_verification.json"
$ingestPreflightReport = Join-Path $validationRootPath "engine_handoff_preflight_verification.json"
$ingestSummary = Join-Path $validationRootPath "engine_handoff_ingest_summary.json"

$canonicalScreenshots = Join-Path $repoRoot "docs\validation\screenshots"
$canonicalProfileNotes = Join-Path $repoRoot "docs\validation\midori-nature-engine-profile-notes.md"
$screenshotNames = @(
    "unity_forest_floor_import.png",
    "unity_forest_floor_density.png",
    "unreal_forest_floor_import.png",
    "unreal_forest_floor_foliage_settings.png"
)

function Assert-PathExists {
    param(
        [string]$Path,
        [string]$Description
    )

    if (!(Test-Path -LiteralPath $Path)) {
        throw "$Description not found at $Path"
    }
}

function Copy-IfPresent {
    param(
        [string]$Source,
        [string]$Destination,
        [string]$Description,
        [bool]$Required
    )

    if (!(Test-Path -LiteralPath $Source)) {
        if ($Required) {
            throw "$Description not found at $Source"
        }
        return [ordered]@{
            description = $Description
            source = $Source
            destination = $Destination
            status = "missing"
        }
    }

    $destinationDirectory = Split-Path -Parent $Destination
    if ($destinationDirectory) {
        New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
    }
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
    return [ordered]@{
        description = $Description
        source = $Source
        destination = $Destination
        status = "copied"
    }
}

function Invoke-EvidenceVerifier {
    param(
        [string[]]$Arguments
    )

    & python @Arguments | Out-Host
    return $LASTEXITCODE
}

Assert-PathExists $handoffPath "Handoff directory"
Assert-PathExists $handoffValidationRoot "Handoff validation root"
Assert-PathExists (Join-Path $validationRootPath $PackageName) "Local validation package"
Assert-PathExists (Join-Path $validationRootPath "$($PackageName)_midori_validation_report.json") "Local Midori validation report"
Assert-PathExists (Join-Path $validationRootPath "$($PackageName)_unreal_dry_run_report.json") "Local Unreal dry-run report"

$required = -not $AllowPartial
$artifactSources = @(
    @{ Path = $handoffUnityReport; Description = "Unity editor import report" },
    @{ Path = $handoffUnrealReport; Description = "Unreal editor import report" },
    @{ Path = Join-Path $handoffScreenshots "unity_forest_floor_import.png"; Description = "Unity import screenshot" },
    @{ Path = Join-Path $handoffScreenshots "unity_forest_floor_density.png"; Description = "Unity density screenshot" },
    @{ Path = Join-Path $handoffScreenshots "unreal_forest_floor_import.png"; Description = "Unreal import screenshot" },
    @{ Path = Join-Path $handoffScreenshots "unreal_forest_floor_foliage_settings.png"; Description = "Unreal foliage settings screenshot" },
    @{ Path = $handoffProfileNotes; Description = "engine profile notes" }
)
if ($required) {
    foreach ($artifact in $artifactSources) {
        Assert-PathExists $artifact.Path $artifact.Description
    }
}

New-Item -ItemType Directory -Path $validationRootPath -Force | Out-Null

$preflightExitCode = $null
if (!$SkipVerify) {
    $preflightArgs = @(
        (Join-Path $repoRoot "scripts\verify_engine_evidence.py"),
        "--validation-root", $validationRootPath,
        "--unity-report", $handoffUnityReport,
        "--unreal-report", $handoffUnrealReport,
        "--unity-import-screenshot", (Join-Path $handoffScreenshots "unity_forest_floor_import.png"),
        "--unity-density-screenshot", (Join-Path $handoffScreenshots "unity_forest_floor_density.png"),
        "--unreal-import-screenshot", (Join-Path $handoffScreenshots "unreal_forest_floor_import.png"),
        "--unreal-foliage-settings-screenshot", (Join-Path $handoffScreenshots "unreal_forest_floor_foliage_settings.png"),
        "--profile-notes", $handoffProfileNotes,
        "--output", $ingestPreflightReport
    )
    if ($AllowPending -or $AllowPartial) {
        $preflightArgs += "--allow-pending"
    }

    $preflightExitCode = Invoke-EvidenceVerifier -Arguments $preflightArgs
    if ($preflightExitCode -ne 0) {
        throw "Handoff evidence preflight failed with exit code $preflightExitCode. No artifacts were copied."
    }
}

$copied = @()
$copied += Copy-IfPresent $handoffUnityReport $canonicalUnityReport "Unity editor import report" $required
$copied += Copy-IfPresent $handoffUnrealReport $canonicalUnrealReport "Unreal editor import report" $required
foreach ($name in $screenshotNames) {
    $copied += Copy-IfPresent `
        (Join-Path $handoffScreenshots $name) `
        (Join-Path $canonicalScreenshots $name) `
        "screenshot $name" `
        $required
}
$copied += Copy-IfPresent $handoffProfileNotes $canonicalProfileNotes "engine profile notes" $required
$copied += Copy-IfPresent $handoffSummary $canonicalSummary "editor handoff summary" $false
$copied += Copy-IfPresent $handoffVerifierReport (Join-Path $validationRootPath "engine_evidence_verification_editor.json") "editor handoff verifier report" $false

$canonicalExitCode = $null
if (!$SkipVerify) {
    $canonicalArgs = @(
        (Join-Path $repoRoot "scripts\verify_engine_evidence.py"),
        "--validation-root", $validationRootPath,
        "--output", $canonicalVerifierReport
    )
    if ($AllowPending -or $AllowPartial) {
        $canonicalArgs += "--allow-pending"
    }
    $canonicalExitCode = Invoke-EvidenceVerifier -Arguments $canonicalArgs
    if ($canonicalExitCode -ne 0) {
        throw "Canonical evidence verification failed after ingest with exit code $canonicalExitCode."
    }
}

$summary = [ordered]@{
    generated_at = (Get-Date).ToString("o")
    handoff_dir = $handoffPath
    validation_root = $validationRootPath
    package_name = $PackageName
    allow_partial = [bool]$AllowPartial
    allow_pending = [bool]$AllowPending
    skip_verify = [bool]$SkipVerify
    preflight_exit_code = $preflightExitCode
    canonical_exit_code = $canonicalExitCode
    preflight_report = $ingestPreflightReport
    canonical_report = $canonicalVerifierReport
    copied_artifacts = $copied
}
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText(
    $ingestSummary,
    (($summary | ConvertTo-Json -Depth 8) + "`n"),
    $utf8NoBom
)

Write-Output "Engine validation handoff ingest summary: $ingestSummary"
Write-Output "Canonical evidence verification: $canonicalVerifierReport"
