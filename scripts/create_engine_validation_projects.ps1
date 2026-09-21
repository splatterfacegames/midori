param(
    [string]$OutputRoot = "target/midori_engine_validation_projects",
    [string]$UnityProjectName = "MidoriUnityValidation",
    [string]$UnrealProjectName = "MidoriUnrealValidation",
    [string]$UnityGltfastVersion = "5.2.0",
    [string]$UnityEditorVersion = "",
    [string]$UnrealEngineAssociation = "",
    [string]$UnrealDestinationRoot = "/Game/Midori/Imported",
    [switch]$PortablePaths
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Resolve-Path (Join-Path $scriptRoot "..")
$rootPath = $root.Path
$utf8NoBom = New-Object System.Text.UTF8Encoding $false

function Resolve-OutputPath {
    param([string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $root $Path))
}

function Write-Utf8File {
    param(
        [string]$Path,
        [string]$Text
    )

    $directory = Split-Path -Parent $Path
    if ($directory) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($Path, $Text, $script:utf8NoBom)
}

function Convert-ToDisplayPath {
    param([string]$Path)

    $fullPath = [System.IO.Path]::GetFullPath($Path)
    if (!$PortablePaths) {
        return $fullPath
    }

    if ($fullPath.StartsWith($rootPath, [System.StringComparison]::OrdinalIgnoreCase)) {
        $basePath = $rootPath
        if (!$basePath.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
            $basePath += [System.IO.Path]::DirectorySeparatorChar
        }
        $baseUri = New-Object System.Uri($basePath)
        $targetUri = New-Object System.Uri($fullPath)
        $relative = $baseUri.MakeRelativeUri($targetUri).ToString()
        return [System.Uri]::UnescapeDataString($relative).Replace(
            "/",
            [System.IO.Path]::DirectorySeparatorChar
        )
    }
    return $fullPath
}

function Copy-Importer {
    param(
        [string]$Source,
        [string]$Destination
    )

    if (!(Test-Path -LiteralPath $Source)) {
        throw "Required importer source not found: $Source"
    }
    $directory = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}

$outputPath = Resolve-OutputPath $OutputRoot
$unityProject = Join-Path $outputPath "unity/$UnityProjectName"
$unrealProject = Join-Path $outputPath "unreal/$UnrealProjectName"
$unrealProjectFile = Join-Path $unrealProject "$UnrealProjectName.uproject"
$unityProjectDisplay = Convert-ToDisplayPath $unityProject
$unrealProjectFileDisplay = Convert-ToDisplayPath $unrealProjectFile
$unityImporterDisplay = Convert-ToDisplayPath (Join-Path $unityProject "Assets/Editor/MidoriNaturePackageImporter.cs")
$unityManifestDisplay = Convert-ToDisplayPath (Join-Path $unityProject "Packages/manifest.json")

New-Item -ItemType Directory -Path $outputPath -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $unityProject "Assets/Editor") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $unityProject "Assets/Midori/Validation") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $unityProject "Packages") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $unityProject "ProjectSettings") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $unrealProject "Config") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $unrealProject "Content/Midori/Validation") -Force | Out-Null

Copy-Importer `
    (Join-Path $root "integrations/unity/Editor/MidoriNaturePackageImporter.cs") `
    (Join-Path $unityProject "Assets/Editor/MidoriNaturePackageImporter.cs")

$unityDependencies = [ordered]@{}
if ($UnityGltfastVersion) {
    $unityDependencies["com.unity.cloud.gltfast"] = $UnityGltfastVersion
}
$unityDependencies["com.unity.modules.imgui"] = "1.0.0"
$unityDependencies["com.unity.modules.jsonserialize"] = "1.0.0"
$unityDependencies["com.unity.modules.terrain"] = "1.0.0"
$unityDependencies["com.unity.modules.uielements"] = "1.0.0"

$unityManifest = [ordered]@{
    dependencies = $unityDependencies
}
Write-Utf8File `
    (Join-Path $unityProject "Packages/manifest.json") `
    (($unityManifest | ConvertTo-Json -Depth 8) + "`n")

if ($UnityEditorVersion) {
    Write-Utf8File `
        (Join-Path $unityProject "ProjectSettings/ProjectVersion.txt") `
        ("m_EditorVersion: $UnityEditorVersion`nm_EditorVersionWithRevision: $UnityEditorVersion`n")
}

$packageHint = "target/midori_engine_validation/forest_floor"
$unityReportHint = "target/midori_engine_validation/forest_floor_unity_import_report.json"
$unrealReportHint = "target/midori_engine_validation/forest_floor_unreal_editor_report.json"
$screenshotsHint = "docs/validation/screenshots"
if (Test-Path -LiteralPath (Join-Path $root "validation_root")) {
    $packageHint = "validation_root/forest_floor"
    $unityReportHint = "validation_root/forest_floor_unity_import_report.json"
    $unrealReportHint = "validation_root/forest_floor_unreal_editor_report.json"
}

$unityReadme = @'
# Midori Unity Validation Fixture

This folder is a minimal Unity project scaffold for the Midori nature Phase 7 validation run.

It includes:

- `Assets/Editor/MidoriNaturePackageImporter.cs`
- `Packages/manifest.json`
- a glTFast package dependency unless `-UnityGltfastVersion ""` was passed to the scaffold script

Open this project once in a licensed Unity editor so Package Manager can restore dependencies, then run the batch importer from the repo or handoff bundle.

```powershell
Unity.exe -batchmode -quit -projectPath "__UNITY_PROJECT__" -executeMethod Midori.Unity.MidoriNaturePackageImporter.BatchImportNaturePackage -midoriPackage "__PACKAGE_DIR__" -midoriImportRoot Assets/Midori/Imported -midoriReport "__UNITY_REPORT__" -midoriImportScreenshot "__UNITY_IMPORT_SCREENSHOT__" -midoriDensityScreenshot "__UNITY_DENSITY_SCREENSHOT__"
```

The strict evidence gate still requires the generated report plus real screenshots and profile notes.
'@
$unityReadmeText = $unityReadme.Replace("__UNITY_PROJECT__", $unityProjectDisplay)
$unityReadmeText = $unityReadmeText.Replace("__PACKAGE_DIR__", $packageHint)
$unityReadmeText = $unityReadmeText.Replace("__UNITY_REPORT__", $unityReportHint)
$unityReadmeText = $unityReadmeText.Replace(
    "__UNITY_IMPORT_SCREENSHOT__",
    (Join-Path $screenshotsHint "unity_forest_floor_import.png")
)
$unityReadmeText = $unityReadmeText.Replace(
    "__UNITY_DENSITY_SCREENSHOT__",
    (Join-Path $screenshotsHint "unity_forest_floor_density.png")
)
Write-Utf8File `
    (Join-Path $unityProject "Assets/Midori/Validation/README.md") `
    ($unityReadmeText + "`n")

$uproject = [ordered]@{
    FileVersion = 3
    Category = "Validation"
    Description = "Midori nature mobile/console validation fixture."
    Plugins = @(
        [ordered]@{
            Name = "PythonScriptPlugin"
            Enabled = $true
        },
        [ordered]@{
            Name = "EditorScriptingUtilities"
            Enabled = $true
        }
    )
}
if ($UnrealEngineAssociation) {
    $uproject["EngineAssociation"] = $UnrealEngineAssociation
}
Write-Utf8File $unrealProjectFile (($uproject | ConvertTo-Json -Depth 8) + "`n")

$unrealConfig = @'
[/Script/PythonScriptPlugin.PythonScriptPluginSettings]
bDeveloperMode=True
bRemoteExecution=True
'@
Write-Utf8File `
    (Join-Path $unrealProject "Config/DefaultEngine.ini") `
    ($unrealConfig + "`n")

$unrealReadme = @'
# Midori Unreal Validation Fixture

This folder is a minimal Unreal project scaffold for the Midori nature Phase 7 validation run.

It includes:

- `__UNREAL_PROJECT_NAME__.uproject`
- Python editor scripting enabled in `Config/DefaultEngine.ini`
- `Content/Midori/Validation/README.md`

Open this project once in Unreal Editor, enable the engine's glTF/Interchange import support if your installation does not already import GLB files, then run the validation importer from the repo or handoff bundle.

```powershell
UnrealEditor.exe "__UNREAL_PROJECT__" -Unattended -NoSplash -NullRHI -ExecutePythonScript="validation_root/run_midori_unreal_import.py"
```

The validation runner and handoff script generate the exact Python wrapper and report path automatically. The strict evidence gate still requires the generated report plus real screenshots and profile notes.
'@
$unrealReadmeText = $unrealReadme.Replace("__UNREAL_PROJECT_NAME__", $UnrealProjectName)
$unrealReadmeText = $unrealReadmeText.Replace("__UNREAL_PROJECT__", $unrealProjectFileDisplay)
Write-Utf8File `
    (Join-Path $unrealProject "Content/Midori/Validation/README.md") `
    ($unrealReadmeText + "`n")

$topReadme = @'
# Midori Engine Validation Project Scaffolds

This directory contains ready-to-open Unity and Unreal fixture projects for the Midori nature mobile/console validation pass.

## Unity

- Project: `__UNITY_PROJECT__`
- Importer: `__UNITY_IMPORTER__`
- Package manifest: `__UNITY_MANIFEST__`

The Unity project includes the Midori editor importer and a glTFast dependency by default. Open it once in a licensed Unity editor so Package Manager can restore packages before running the validation importer.

## Unreal

- Project file: `__UNREAL_PROJECT__`
- Destination root: `__UNREAL_DESTINATION_ROOT__`

The Unreal project enables Python editor scripting. Enable GLB import support in the editor if your installed engine does not import Midori prototype GLBs out of the box.

## Commands

'@
$sourceRunner = Join-Path $root "scripts/validate_engine_imports.ps1"
if (Test-Path -LiteralPath $sourceRunner) {
    $topReadme += @'

From the source repo root:

```powershell
pwsh -NoProfile -File scripts/validate_engine_imports.ps1 -UnityProject "__UNITY_PROJECT__" -UnrealProject "__UNREAL_PROJECT__"
```
'@
}
$handoffRunner = Join-Path $root "run_editor_validation.ps1"
if (Test-Path -LiteralPath $handoffRunner) {
    $topReadme += @'

From the handoff bundle root:

```powershell
./run_editor_validation.ps1 -UnityProject "__UNITY_PROJECT__" -UnrealProject "__UNREAL_PROJECT__"
```

When the run succeeds, complete `docs/validation/midori-nature-engine-profile-notes.md`, rerun verification, and return the bundle for ingest.

'@
}
$topReadme += @'

If the fixture projects were copied somewhere else, pass the copied project paths to the same runner commands.
'@
$topReadmeText = $topReadme.Replace("__UNITY_PROJECT__", $unityProjectDisplay)
$topReadmeText = $topReadmeText.Replace(
    "__UNITY_IMPORTER__",
    $unityImporterDisplay
)
$topReadmeText = $topReadmeText.Replace(
    "__UNITY_MANIFEST__",
    $unityManifestDisplay
)
$topReadmeText = $topReadmeText.Replace("__UNREAL_PROJECT__", $unrealProjectFileDisplay)
$topReadmeText = $topReadmeText.Replace("__UNREAL_DESTINATION_ROOT__", $UnrealDestinationRoot)
Write-Utf8File (Join-Path $outputPath "README.md") ($topReadmeText + "`n")

$summary = [ordered]@{
    generated_at = (Get-Date).ToString("o")
    root = $root.Path
    output_root = $outputPath
    portable_paths = [bool]$PortablePaths
    package_hint = $packageHint
    unity = [ordered]@{
        project_path = $unityProject
        project_path_display = $unityProjectDisplay
        importer = Join-Path $unityProject "Assets/Editor/MidoriNaturePackageImporter.cs"
        importer_display = $unityImporterDisplay
        manifest = Join-Path $unityProject "Packages/manifest.json"
        manifest_display = $unityManifestDisplay
        gltfast_version = $UnityGltfastVersion
        editor_version = $UnityEditorVersion
    }
    unreal = [ordered]@{
        project_path = $unrealProject
        uproject = $unrealProjectFile
        uproject_display = $unrealProjectFileDisplay
        engine_association = $UnrealEngineAssociation
        destination_root = $UnrealDestinationRoot
    }
    reports = [ordered]@{
        unity = $unityReportHint
        unreal = $unrealReportHint
        screenshots = $screenshotsHint
    }
}
Write-Utf8File `
    (Join-Path $outputPath "project_scaffold_summary.json") `
    (($summary | ConvertTo-Json -Depth 8) + "`n")

Write-Output "Midori validation project scaffolds written to $outputPath"
Write-Output "Unity project: $unityProject"
Write-Output "Unreal project: $unrealProjectFile"
