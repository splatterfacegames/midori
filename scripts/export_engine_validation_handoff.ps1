param(
    [string]$ValidationRoot = "target/midori_engine_validation",
    [string]$PackageName = "forest_floor",
    [string]$HandoffDir = "target/midori_engine_validation_handoff",
    [switch]$Regenerate
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$validationRootPath = Join-Path $repoRoot $ValidationRoot
$packageDir = Join-Path $validationRootPath $PackageName
$midoriReport = Join-Path $validationRootPath "$($PackageName)_midori_validation_report.json"
$unrealDryRunReport = Join-Path $validationRootPath "$($PackageName)_unreal_dry_run_report.json"
$unrealFakeEditorReport = Join-Path $validationRootPath "$($PackageName)_unreal_fake_editor_report.json"
$summaryPath = Join-Path $validationRootPath "engine_validation_summary.json"
$evidenceReport = Join-Path $validationRootPath "engine_evidence_verification.json"
$unityCompileStubReport = Join-Path $validationRootPath "$($PackageName)_unity_compile_stub_report.json"
$utf8NoBom = New-Object System.Text.UTF8Encoding $false

function Resolve-OutputPath {
    param([string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Assert-ExistingPath {
    param(
        [string]$Path,
        [string]$Description
    )

    if (!(Test-Path -LiteralPath $Path)) {
        throw "$Description not found at $Path. Run scripts\validate_engine_imports.ps1 first, or pass -Regenerate."
    }
}

function Copy-RequiredFile {
    param(
        [string]$Source,
        [string]$Destination
    )

    $destinationDirectory = Split-Path -Parent $Destination
    if ($destinationDirectory) {
        New-Item -ItemType Directory -Path $destinationDirectory -Force | Out-Null
    }
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

if ($Regenerate) {
    & (Join-Path $repoRoot "scripts\validate_engine_imports.ps1") `
        -OutputDir $ValidationRoot `
        -PackageName $PackageName
    if ($LASTEXITCODE -ne 0) {
        throw "scripts\validate_engine_imports.ps1 failed with exit code $LASTEXITCODE"
    }
}

Assert-ExistingPath $packageDir "Midori package"
Assert-ExistingPath $midoriReport "Midori validation report"
Assert-ExistingPath $unrealDryRunReport "Unreal dry-run report"
Assert-ExistingPath $unrealFakeEditorReport "Unreal fake-editor report"
Assert-ExistingPath $summaryPath "engine validation summary"
Assert-ExistingPath $evidenceReport "pending evidence report"
Assert-ExistingPath $unityCompileStubReport "Unity compile-stub execution report"

$handoffPath = Resolve-OutputPath $HandoffDir
$targetRoot = Resolve-OutputPath "target"
if (Test-Path -LiteralPath $handoffPath) {
    $resolvedHandoff = (Resolve-Path -LiteralPath $handoffPath).Path
    $resolvedTarget = (Resolve-Path -LiteralPath $targetRoot).Path
    if (!$resolvedHandoff.StartsWith($resolvedTarget, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to clean handoff directory outside repo target/: $resolvedHandoff"
    }
    Remove-Item -LiteralPath $resolvedHandoff -Recurse -Force
}

New-Item -ItemType Directory -Path $handoffPath -Force | Out-Null
$bundleValidationRoot = Join-Path $handoffPath "validation_root"
$bundleScripts = Join-Path $handoffPath "scripts"
$bundleUnityIntegration = Join-Path $handoffPath "integrations\unity\Editor"
$bundleUnrealIntegration = Join-Path $handoffPath "integrations\unreal"
$bundleScreenshots = Join-Path $handoffPath "docs\validation\screenshots"
$bundleDocsValidation = Join-Path $handoffPath "docs\validation"
$bundleProjects = Join-Path $handoffPath "projects"

New-Item -ItemType Directory -Path $bundleValidationRoot -Force | Out-Null
Copy-Item -LiteralPath $packageDir -Destination $bundleValidationRoot -Recurse -Force
Copy-RequiredFile $midoriReport (Join-Path $bundleValidationRoot "$($PackageName)_midori_validation_report.json")
Copy-RequiredFile $unrealDryRunReport (Join-Path $bundleValidationRoot "$($PackageName)_unreal_dry_run_report.json")
Copy-RequiredFile $unrealFakeEditorReport (Join-Path $bundleValidationRoot "$($PackageName)_unreal_fake_editor_report.json")
$bundleUnityCompileStubReport = Join-Path $bundleValidationRoot "$($PackageName)_unity_compile_stub_report.json"
Copy-RequiredFile $unityCompileStubReport $bundleUnityCompileStubReport
$bundleSummaryPath = Join-Path $bundleValidationRoot "engine_validation_summary.json"
Copy-RequiredFile $summaryPath $bundleSummaryPath
Copy-RequiredFile $evidenceReport (Join-Path $bundleValidationRoot "engine_evidence_verification.json")

$bundleSummaryJson = Get-Content -LiteralPath $bundleSummaryPath -Raw | ConvertFrom-Json
if ($bundleSummaryJson.unity_preflight) {
    $bundleSummaryJson.unity_preflight.compile_stub_execution_report = "$($PackageName)_unity_compile_stub_report.json"
}
if ($bundleSummaryJson.unreal_fake_editor) {
    $bundleSummaryJson.unreal_fake_editor.report = "$($PackageName)_unreal_fake_editor_report.json"
}
[System.IO.File]::WriteAllText(
    $bundleSummaryPath,
    (($bundleSummaryJson | ConvertTo-Json -Depth 64) + "`n"),
    $utf8NoBom
)

$bundleUnityCompileStubJson = Get-Content -LiteralPath $bundleUnityCompileStubReport -Raw | ConvertFrom-Json
$bundleUnityCompileStubJson.package_dir = "validation_root/$PackageName"
[System.IO.File]::WriteAllText(
    $bundleUnityCompileStubReport,
    (($bundleUnityCompileStubJson | ConvertTo-Json -Depth 64) + "`n"),
    $utf8NoBom
)

Copy-RequiredFile (Join-Path $repoRoot "scripts\verify_engine_evidence.py") `
    (Join-Path $bundleScripts "verify_engine_evidence.py")
Copy-RequiredFile (Join-Path $repoRoot "scripts\import_engine_validation_handoff.ps1") `
    (Join-Path $bundleScripts "import_engine_validation_handoff.ps1")
Copy-RequiredFile (Join-Path $repoRoot "scripts\create_engine_validation_projects.ps1") `
    (Join-Path $bundleScripts "create_engine_validation_projects.ps1")
Copy-RequiredFile (Join-Path $repoRoot "scripts\test_unity_importer_compile_stub.py") `
    (Join-Path $bundleScripts "test_unity_importer_compile_stub.py")
Copy-RequiredFile (Join-Path $repoRoot "scripts\test_profile_notes_verifier.py") `
    (Join-Path $bundleScripts "test_profile_notes_verifier.py")
Copy-RequiredFile (Join-Path $repoRoot "integrations\unity\Editor\MidoriNaturePackageImporter.cs") `
    (Join-Path $bundleUnityIntegration "MidoriNaturePackageImporter.cs")
Copy-RequiredFile (Join-Path $repoRoot "integrations\unreal\midori_nature_importer.py") `
    (Join-Path $bundleUnrealIntegration "midori_nature_importer.py")
Copy-RequiredFile (Join-Path $repoRoot "docs\validation\midori-nature-engine-profile-notes.template.md") `
    (Join-Path $bundleDocsValidation "midori-nature-engine-profile-notes.template.md")
Copy-RequiredFile (Join-Path $repoRoot "docs\validation\screenshots\README.md") `
    (Join-Path $bundleScreenshots "README.md")

$existingProfileNotes = Join-Path $repoRoot "docs\validation\midori-nature-engine-profile-notes.md"
if (Test-Path -LiteralPath $existingProfileNotes) {
    Copy-RequiredFile $existingProfileNotes `
        (Join-Path $bundleDocsValidation "midori-nature-engine-profile-notes.md")
}

$runScript = @'
param(
    [string]$UnityExe = "",
    [string]$UnityProject = "",
    [string]$UnrealEditorExe = "",
    [string]$UnrealProject = "",
    [string]$UnrealDestinationRoot = "/Game/Midori/Imported",
    [switch]$SkipUnity,
    [switch]$SkipUnreal,
    [switch]$VerifyOnly,
    [switch]$AllowPending
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$bundleRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$validationRoot = Join-Path $bundleRoot "validation_root"
$packageDir = Join-Path $validationRoot "__PACKAGE_NAME__"
$unityReport = Join-Path $validationRoot "__PACKAGE_NAME___unity_import_report.json"
$unrealReport = Join-Path $validationRoot "__PACKAGE_NAME___unreal_editor_report.json"
$unityLog = Join-Path $validationRoot "unity_import.log"
$unityCreateLog = Join-Path $validationRoot "unity_create.log"
$unrealLog = Join-Path $validationRoot "unreal_import.log"
$unrealWrapper = Join-Path $validationRoot "run_midori_unreal_import.py"
$summaryPath = Join-Path $validationRoot "editor_handoff_summary.json"
$screenshotsRoot = Join-Path $bundleRoot "docs\validation\screenshots"
$unityImportScreenshot = Join-Path $screenshotsRoot "unity_forest_floor_import.png"
$unityDensityScreenshot = Join-Path $screenshotsRoot "unity_forest_floor_density.png"
$unrealImportScreenshot = Join-Path $screenshotsRoot "unreal_forest_floor_import.png"
$unrealFoliageScreenshot = Join-Path $screenshotsRoot "unreal_forest_floor_foliage_settings.png"
$profileNotes = Join-Path $bundleRoot "docs\validation\midori-nature-engine-profile-notes.md"
$verifyReport = Join-Path $validationRoot "engine_evidence_verification_editor.json"
$defaultUnityProject = Join-Path $bundleRoot "projects\unity\MidoriUnityValidation"
$defaultUnrealProject = Join-Path $bundleRoot "projects\unreal\MidoriUnrealValidation\MidoriUnrealValidation.uproject"

function Convert-ToProcessArgument {
    param([string]$Value)

    return '"' + $Value.Replace('"', '\"') + '"'
}

function Invoke-ProcessWait {
    param(
        [string]$FilePath,
        [string[]]$Arguments
    )

    $argumentLine = ($Arguments | ForEach-Object { Convert-ToProcessArgument $_ }) -join " "
    $process = Start-Process `
        -FilePath $FilePath `
        -ArgumentList $argumentLine `
        -Wait `
        -PassThru `
        -WindowStyle Hidden
    return $process.ExitCode
}

function Find-UnityEditor {
    if ($UnityExe -and (Test-Path -LiteralPath $UnityExe)) {
        return (Resolve-Path -LiteralPath $UnityExe).Path
    }

    $pathCommand = Get-Command Unity.exe -ErrorAction SilentlyContinue
    if ($pathCommand) {
        return $pathCommand.Source
    }

    $hubRoot = "C:\Program Files\Unity\Hub\Editor"
    if (Test-Path -LiteralPath $hubRoot) {
        $editors = Get-ChildItem -LiteralPath $hubRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending
        foreach ($editor in $editors) {
            $candidate = Join-Path $editor.FullName "Editor\Unity.exe"
            if (Test-Path -LiteralPath $candidate) {
                return $candidate
            }
        }
    }

    return $null
}

function Find-UnrealEditor {
    if ($UnrealEditorExe -and (Test-Path -LiteralPath $UnrealEditorExe)) {
        return (Resolve-Path -LiteralPath $UnrealEditorExe).Path
    }

    $pathCommand = Get-Command UnrealEditor.exe -ErrorAction SilentlyContinue
    if ($pathCommand) {
        return $pathCommand.Source
    }

    $epicRoot = "C:\Program Files\Epic Games"
    if (Test-Path -LiteralPath $epicRoot) {
        $engines = Get-ChildItem -LiteralPath $epicRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending
        foreach ($engine in $engines) {
            $candidate = Join-Path $engine.FullName "Engine\Binaries\Win64\UnrealEditor.exe"
            if (Test-Path -LiteralPath $candidate) {
                return $candidate
            }
        }
    }

    return $null
}

function Test-LogContains {
    param(
        [string]$Path,
        [string]$Pattern
    )

    if (!(Test-Path -LiteralPath $Path)) {
        return $false
    }

    return [bool](Select-String -LiteralPath $Path -Pattern $Pattern -Quiet)
}

function Convert-ToPythonString {
    param([string]$Value)

    return ConvertTo-Json $Value -Compress
}

function Invoke-EvidenceVerifier {
    $arguments = @(
        (Join-Path $bundleRoot "scripts\verify_engine_evidence.py"),
        "--validation-root", $validationRoot,
        "--unity-import-screenshot", $unityImportScreenshot,
        "--unity-density-screenshot", $unityDensityScreenshot,
        "--unreal-import-screenshot", $unrealImportScreenshot,
        "--unreal-foliage-settings-screenshot", $unrealFoliageScreenshot,
        "--profile-notes", $profileNotes,
        "--output", $verifyReport
    )
    if ($AllowPending) {
        $arguments += "--allow-pending"
    }

    & python @arguments
    $script:EvidenceVerifierExitCode = $LASTEXITCODE
}

function Get-SectionStatus {
    param([object]$Section)

    if ($null -eq $Section) {
        return ""
    }
    if ($Section -is [System.Collections.IDictionary] -and $Section.Contains("status")) {
        return [string]$Section["status"]
    }
    $property = $Section.PSObject.Properties["status"]
    if ($null -eq $property) {
        return ""
    }
    return [string]$property.Value
}

function Set-SectionStatus {
    param(
        [object]$Section,
        [string]$Status
    )

    if ($Section -is [System.Collections.IDictionary]) {
        $Section["status"] = $Status
        return
    }
    $Section.status = $Status
}

function Get-PreviousEditorSummarySection {
    param(
        [object]$PreviousSummary,
        [string]$Name
    )

    if ($null -eq $PreviousSummary) {
        return [ordered]@{}
    }
    $property = $PreviousSummary.PSObject.Properties[$Name]
    if ($null -eq $property -or $null -eq $property.Value) {
        return [ordered]@{}
    }
    $section = $property.Value
    if ((Get-SectionStatus $section) -and (Get-SectionStatus $section) -ne "verify_only") {
        return $section
    }
    return [ordered]@{}
}

New-Item -ItemType Directory -Path $screenshotsRoot -Force | Out-Null
$previousSummary = $null
if ($VerifyOnly -and (Test-Path -LiteralPath $summaryPath)) {
    try {
        $previousSummary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
    }
    catch {
        $previousSummary = $null
    }
}
$summary = [ordered]@{
    generated_at = (Get-Date).ToString("o")
    bundle_root = $bundleRoot
    package_dir = $packageDir
    unity = $(if ($VerifyOnly) { Get-PreviousEditorSummarySection $previousSummary "unity" } else { [ordered]@{} })
    unreal = $(if ($VerifyOnly) { Get-PreviousEditorSummarySection $previousSummary "unreal" } else { [ordered]@{} })
    verifier = [ordered]@{}
}

$profileNotesSelfTest = Join-Path $bundleRoot "scripts\test_profile_notes_verifier.py"
& python $profileNotesSelfTest
$summary.verifier.profile_notes_self_test = $profileNotesSelfTest
$summary.verifier.profile_notes_self_test_exit_code = $LASTEXITCODE
if ($LASTEXITCODE -ne 0) {
    throw "Profile notes verifier self-test failed with exit code $LASTEXITCODE"
}

if (!$VerifyOnly -and !$SkipUnity) {
    $unityEditor = Find-UnityEditor
    if (!$unityEditor) {
        $summary.unity.status = "blocked_unity_editor_not_found"
    }
    else {
        $projectPath = $UnityProject
        $projectSource = "supplied"
        $createdProject = $false
        if (!$projectPath) {
            if (Test-Path -LiteralPath $defaultUnityProject) {
                $projectPath = $defaultUnityProject
                $projectSource = "bundled_scaffold"
            }
            else {
                $projectPath = Join-Path $bundleRoot "unity_validation_project"
                $projectSource = "temporary_create"
                $createdProject = !(Test-Path -LiteralPath $projectPath)
            }
        }

        $summary.unity.status = "attempted"
        $summary.unity.editor_exe = $unityEditor
        $summary.unity.project_path = $projectPath
        $summary.unity.project_source = $projectSource
        $summary.unity.report = $unityReport
        $summary.unity.import_screenshot = $unityImportScreenshot
        $summary.unity.density_screenshot = $unityDensityScreenshot

        if ($createdProject) {
            $createExitCode = Invoke-ProcessWait $unityEditor @(
                "-batchmode",
                "-quit",
                "-createProject", $projectPath,
                "-logFile", $unityCreateLog
            )
            $summary.unity.create_exit_code = $createExitCode
            if ($createExitCode -ne 0 -or (Test-LogContains $unityCreateLog "No valid Unity Editor license")) {
                $summary.unity.status = "blocked_unity_license_or_project_create_failed"
            }
        }

        if ($summary.unity.status -eq "attempted") {
            $editorDir = Join-Path $projectPath "Assets\Editor"
            New-Item -ItemType Directory -Path $editorDir -Force | Out-Null
            Copy-Item -LiteralPath (Join-Path $bundleRoot "integrations\unity\Editor\MidoriNaturePackageImporter.cs") `
                -Destination (Join-Path $editorDir "MidoriNaturePackageImporter.cs") `
                -Force

            $importExitCode = Invoke-ProcessWait $unityEditor @(
                "-batchmode",
                "-quit",
                "-projectPath", $projectPath,
                "-executeMethod", "Midori.Unity.MidoriNaturePackageImporter.BatchImportNaturePackage",
                "-midoriPackage", $packageDir,
                "-midoriImportRoot", "Assets/Midori/Imported",
                "-midoriReport", $unityReport,
                "-midoriImportScreenshot", $unityImportScreenshot,
                "-midoriDensityScreenshot", $unityDensityScreenshot,
                "-logFile", $unityLog
            )
            $summary.unity.import_exit_code = $importExitCode
        if ($importExitCode -eq 0 -and (Test-Path -LiteralPath $unityReport)) {
            $summary.unity.status = "passed"
            $unityReportJson = Get-Content -LiteralPath $unityReport -Raw | ConvertFrom-Json
            $summary.unity.manifest_file_checksum = $unityReportJson.manifestFileChecksum
            $summary.unity.source_file_count = $unityReportJson.sourceFileCount
            $summary.unity.source_file_checksum_xor = $unityReportJson.sourceFileChecksumXor
            $summary.unity.import_screenshot_status = $unityReportJson.importScreenshot.status
            $summary.unity.import_screenshot_exists = $unityReportJson.importScreenshot.exists
            $summary.unity.import_screenshot_bytes = $unityReportJson.importScreenshot.bytes
            $summary.unity.import_screenshot_checksum = $unityReportJson.importScreenshot.checksum
            $summary.unity.import_screenshot_width = $unityReportJson.importScreenshot.width
            $summary.unity.import_screenshot_height = $unityReportJson.importScreenshot.height
            $summary.unity.density_screenshot_status = $unityReportJson.densityScreenshot.status
            $summary.unity.density_screenshot_exists = $unityReportJson.densityScreenshot.exists
            $summary.unity.density_screenshot_bytes = $unityReportJson.densityScreenshot.bytes
            $summary.unity.density_screenshot_checksum = $unityReportJson.densityScreenshot.checksum
            $summary.unity.density_screenshot_width = $unityReportJson.densityScreenshot.width
            $summary.unity.density_screenshot_height = $unityReportJson.densityScreenshot.height
        }
            elseif (Test-LogContains $unityLog "No valid Unity Editor license") {
                $summary.unity.status = "blocked_unity_license"
            }
            else {
                $summary.unity.status = "failed_import"
            }
        }
    }
}
elseif ($SkipUnity) {
    Set-SectionStatus $summary.unity "skipped"
}
else {
    if (!(Get-SectionStatus $summary.unity)) {
        Set-SectionStatus $summary.unity "verify_only"
    }
}

if (!$VerifyOnly -and !$SkipUnreal) {
    $unrealEditor = Find-UnrealEditor
    $projectPath = $UnrealProject
    $projectSource = "supplied"
    if (!$projectPath -and (Test-Path -LiteralPath $defaultUnrealProject)) {
        $projectPath = $defaultUnrealProject
        $projectSource = "bundled_scaffold"
    }
    if (!$unrealEditor) {
        $summary.unreal.status = "blocked_unreal_editor_not_found"
    }
    elseif (!$projectPath) {
        $summary.unreal.status = "blocked_unreal_project_not_supplied"
    }
    elseif (!(Test-Path -LiteralPath $projectPath)) {
        $summary.unreal.status = "blocked_unreal_project_not_found"
        $summary.unreal.project = $projectPath
        $summary.unreal.project_source = $projectSource
    }
    else {
        $integrationDir = Join-Path $bundleRoot "integrations\unreal"
        $wrapper = @(
            "import sys",
            "sys.path.insert(0, $(Convert-ToPythonString $integrationDir))",
            "import midori_nature_importer",
            "report = midori_nature_importer.import_midori_nature_package($(Convert-ToPythonString $packageDir), $(Convert-ToPythonString $UnrealDestinationRoot), import_screenshot=$(Convert-ToPythonString $unrealImportScreenshot), foliage_settings_screenshot=$(Convert-ToPythonString $unrealFoliageScreenshot))",
            "midori_nature_importer.write_report_file($(Convert-ToPythonString $unrealReport), report)"
        ) -join "`n"
        $utf8NoBom = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllText($unrealWrapper, $wrapper + "`n", $utf8NoBom)

        $summary.unreal.status = "attempted"
        $summary.unreal.editor_exe = $unrealEditor
        $summary.unreal.project = (Resolve-Path -LiteralPath $projectPath).Path
        $summary.unreal.project_source = $projectSource
        $summary.unreal.report = $unrealReport
        $summary.unreal.import_screenshot = $unrealImportScreenshot
        $summary.unreal.foliage_settings_screenshot = $unrealFoliageScreenshot

        $unrealExitCode = Invoke-ProcessWait $unrealEditor @(
            (Resolve-Path -LiteralPath $projectPath).Path,
            "-Unattended",
            "-NoSplash",
            "-NullRHI",
            "-ExecutePythonScript=$unrealWrapper",
            "-log=$unrealLog"
        )
        $summary.unreal.exit_code = $unrealExitCode
        if ($unrealExitCode -eq 0 -and (Test-Path -LiteralPath $unrealReport)) {
            $summary.unreal.status = "passed"
            $unrealReportJson = Get-Content -LiteralPath $unrealReport -Raw | ConvertFrom-Json
            $summary.unreal.manifest_file_checksum = $unrealReportJson.manifest_file_checksum
            $summary.unreal.source_file_count = $unrealReportJson.source_file_count
            $summary.unreal.source_file_checksum_xor = $unrealReportJson.source_file_checksum_xor
            $summary.unreal.import_screenshot_status = $unrealReportJson.screenshots.import.status
            $summary.unreal.import_screenshot_method = $unrealReportJson.screenshots.import.method
            $summary.unreal.import_screenshot_exists = $unrealReportJson.screenshots.import.exists
            $summary.unreal.import_screenshot_bytes = $unrealReportJson.screenshots.import.bytes
            $summary.unreal.import_screenshot_checksum = $unrealReportJson.screenshots.import.checksum
            $summary.unreal.import_screenshot_width = $unrealReportJson.screenshots.import.width
            $summary.unreal.import_screenshot_height = $unrealReportJson.screenshots.import.height
            $summary.unreal.foliage_settings_screenshot_status = $unrealReportJson.screenshots.foliage_settings.status
            $summary.unreal.foliage_settings_screenshot_method = $unrealReportJson.screenshots.foliage_settings.method
            $summary.unreal.foliage_settings_screenshot_exists = $unrealReportJson.screenshots.foliage_settings.exists
            $summary.unreal.foliage_settings_screenshot_bytes = $unrealReportJson.screenshots.foliage_settings.bytes
            $summary.unreal.foliage_settings_screenshot_checksum = $unrealReportJson.screenshots.foliage_settings.checksum
            $summary.unreal.foliage_settings_screenshot_width = $unrealReportJson.screenshots.foliage_settings.width
            $summary.unreal.foliage_settings_screenshot_height = $unrealReportJson.screenshots.foliage_settings.height
        }
        else {
            $summary.unreal.status = "failed_import"
        }
    }
}
elseif ($SkipUnreal) {
    Set-SectionStatus $summary.unreal "skipped"
}
else {
    if (!(Get-SectionStatus $summary.unreal)) {
        Set-SectionStatus $summary.unreal "verify_only"
    }
}

$script:EvidenceVerifierExitCode = 1
Invoke-EvidenceVerifier
$verifyExitCode = $script:EvidenceVerifierExitCode
$summary.verifier.exit_code = $verifyExitCode
$summary.verifier.report = $verifyReport
$summary.verifier.profile_notes = $profileNotes
$summary.verifier.allow_pending = [bool]$AllowPending

$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText(
    $summaryPath,
    (($summary | ConvertTo-Json -Depth 8) + "`n"),
    $utf8NoBom
)

Write-Output "Editor handoff summary: $summaryPath"
Write-Output "Evidence verifier report: $verifyReport"
exit $verifyExitCode
'@

$runScript = $runScript.Replace("__PACKAGE_NAME__", $PackageName)
[System.IO.File]::WriteAllText((Join-Path $handoffPath "run_editor_validation.ps1"), $runScript + "`n", $utf8NoBom)

& (Join-Path $bundleScripts "create_engine_validation_projects.ps1") `
    -OutputRoot $bundleProjects `
    -PortablePaths

$readme = @'
# Midori Engine Validation Handoff

This bundle freezes the generated `__PACKAGE_NAME__` Midori nature package and the current preflight reports so a machine with real Unity and Unreal editors can produce the remaining Phase 7 evidence.

## Contents

- `validation_root/__PACKAGE_NAME__/` - generated Midori package under test
- `validation_root/__PACKAGE_NAME___midori_validation_report.json` - package conformance report
- `validation_root/__PACKAGE_NAME___unreal_dry_run_report.json` - editorless Unreal dry-run report
- `validation_root/__PACKAGE_NAME___unreal_fake_editor_report.json` - editorless fake Unreal editor import and screenshot-path report
- `validation_root/__PACKAGE_NAME___unity_compile_stub_report.json` - editorless Unity C# validation-path execution report
- `integrations/unity/Editor/MidoriNaturePackageImporter.cs` - Unity batch importer
- `integrations/unreal/midori_nature_importer.py` - Unreal Python importer
- `scripts/verify_engine_evidence.py` - strict evidence verifier
- `scripts/import_engine_validation_handoff.ps1` - repo-side ingest helper copied for reference
- `scripts/create_engine_validation_projects.ps1` - regenerates the editor fixture projects if needed
- `scripts/test_unity_importer_compile_stub.py` - optional editorless C# compile check for the Unity importer when .NET SDK is available
- `scripts/test_profile_notes_verifier.py` - self-test for the completed profiling notes gate
- `projects/` - ready-to-open Unity and Unreal validation project scaffolds
- `docs/validation/screenshots/` - expected screenshot output directory
- `docs/validation/midori-nature-engine-profile-notes.template.md` - notes template

## Editor Run

From this bundle directory, run the editor imports against the bundled fixture projects:

```powershell
.\run_editor_validation.ps1 -UnityExe "C:\Program Files\Unity\Hub\Editor\<version>\Editor\Unity.exe" -UnrealEditorExe "C:\Program Files\Epic Games\UE_<version>\Engine\Binaries\Win64\UnrealEditor.exe" -AllowPending
```

Unity must be licensed. The runner uses `projects\unity\MidoriUnityValidation` by default when `-UnityProject` is omitted. The scaffold includes a glTFast Package Manager dependency by default, and the Unity importer also has a native Midori GLB fallback so basic detail-prototype evidence does not depend on an external GLB importer. Unreal uses `projects\unreal\MidoriUnrealValidation\MidoriUnrealValidation.uproject` by default when `-UnrealProject` is omitted; the scaffold enables Python editor scripting, but the editor may still need GLB import support enabled for prototype assets.

To use custom existing projects instead, pass their project paths to `-UnityProject` and `-UnrealProject`. To regenerate the bundled scaffolds, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/create_engine_validation_projects.ps1 -OutputRoot "projects" -PortablePaths
```

The first editor run should use `-AllowPending` because the profile notes are normally written after the reports and screenshots exist. After the editor run, inspect `validation_root/editor_handoff_summary.json`; successful editor sections record the report checksums plus screenshot status, byte count, checksum, and dimensions. Then create `docs/validation/midori-nature-engine-profile-notes.md` from the template with real Unity and Unreal profiling observations. The completed notes must cite the exact report and screenshot filenames, Unity and Unreal editor versions, mobile and console profile observations, Frame Debugger and RenderDoc instancing evidence, Unity detail-prototype fallback fields, Unreal foliage type/cull fields, material slot observations, wind-channel observations, and the strict verifier result. Then rerun verification only without `-AllowPending`; the runner preserves the previous editor-run summary sections while updating verifier fields:

```powershell
.\run_editor_validation.ps1 -VerifyOnly
```

For diagnostic runs on machines still missing editor artifacts, use:

```powershell
.\run_editor_validation.ps1 -VerifyOnly -AllowPending
```

Return the completed bundle to the source repo, then run this from the repo root to copy verified artifacts into canonical validation locations:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/import_engine_validation_handoff.ps1 -HandoffDir "target/midori_engine_validation_handoff"
```

The strict verifier passes only when `validation_root/__PACKAGE_NAME___unity_import_report.json`, `validation_root/__PACKAGE_NAME___unreal_editor_report.json`, the four screenshots, and completed profile notes all exist and satisfy `scripts/verify_engine_evidence.py`.
'@
$readme = $readme.Replace("__PACKAGE_NAME__", $PackageName)
[System.IO.File]::WriteAllText((Join-Path $handoffPath "README.md"), $readme + "`n", $utf8NoBom)

$manifest = [ordered]@{
    generated_at = (Get-Date).ToString("o")
    source_repo = $repoRoot.Path
    source_validation_root = (Resolve-Path -LiteralPath $validationRootPath).Path
    handoff_dir = $handoffPath
    package_name = $PackageName
    package_dir = "validation_root/$PackageName"
    midori_report = "validation_root/$($PackageName)_midori_validation_report.json"
    unreal_dry_run_report = "validation_root/$($PackageName)_unreal_dry_run_report.json"
    unreal_fake_editor_report = "validation_root/$($PackageName)_unreal_fake_editor_report.json"
    unity_compile_stub_report = "validation_root/$($PackageName)_unity_compile_stub_report.json"
    evidence_report = "validation_root/engine_evidence_verification.json"
    run_script = "run_editor_validation.ps1"
    ingest_script = "scripts/import_engine_validation_handoff.ps1"
    project_scaffold_script = "scripts/create_engine_validation_projects.ps1"
    project_scaffold_summary = "projects/project_scaffold_summary.json"
    unity_project = "projects/unity/MidoriUnityValidation"
    unreal_project = "projects/unreal/MidoriUnrealValidation/MidoriUnrealValidation.uproject"
    strict_completion_requires = @(
        "validation_root/$($PackageName)_unity_import_report.json",
        "validation_root/$($PackageName)_unreal_editor_report.json",
        "docs/validation/screenshots/unity_forest_floor_import.png",
        "docs/validation/screenshots/unity_forest_floor_density.png",
        "docs/validation/screenshots/unreal_forest_floor_import.png",
        "docs/validation/screenshots/unreal_forest_floor_foliage_settings.png",
        "docs/validation/midori-nature-engine-profile-notes.md"
    )
}
[System.IO.File]::WriteAllText(
    (Join-Path $handoffPath "handoff_manifest.json"),
    (($manifest | ConvertTo-Json -Depth 8) + "`n"),
    $utf8NoBom
)

Write-Output "Midori engine validation handoff written to $handoffPath"
Write-Output "Run $handoffPath\run_editor_validation.ps1 on a machine with licensed Unity and installed Unreal."
