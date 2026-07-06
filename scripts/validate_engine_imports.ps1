param(
    [string]$Patch = "presets/nature/temperate_forest_floor.toml",
    [string]$OutputDir = "target/midori_engine_validation",
    [string]$PackageName = "forest_floor",
    [string]$UnityExe = "",
    [string]$UnityProject = "",
    [string]$UnrealEditorExe = "",
    [string]$UnrealProject = "",
    [string]$UnrealDestinationRoot = "/Game/Midori/Imported",
    [string]$ProjectScaffoldRoot = "",
    [switch]$SkipUnity,
    [switch]$SkipUnrealDryRun,
    [switch]$SkipUnrealEditor,
    [switch]$SkipProjectScaffold,
    [switch]$KeepUnityProject
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$outputRoot = Join-Path $repoRoot $OutputDir
$packageDir = Join-Path $outputRoot $PackageName
$midoriReport = Join-Path $outputRoot "$($PackageName)_midori_validation_report.json"
$unrealDryRunReport = Join-Path $outputRoot "$($PackageName)_unreal_dry_run_report.json"
$unrealFakeEditorReport = Join-Path $outputRoot "$($PackageName)_unreal_fake_editor_report.json"
$unrealEditorReport = Join-Path $outputRoot "$($PackageName)_unreal_editor_report.json"
$unityReport = Join-Path $outputRoot "$($PackageName)_unity_import_report.json"
$summaryPath = Join-Path $outputRoot "engine_validation_summary.json"
$unityCreateLog = Join-Path $outputRoot "unity_create.log"
$unityImportLog = Join-Path $outputRoot "unity_import.log"
$unrealImportLog = Join-Path $outputRoot "unreal_import.log"
$unrealPythonScript = Join-Path $outputRoot "run_midori_unreal_import.py"
$evidenceReport = Join-Path $outputRoot "engine_evidence_verification.json"
$validationScreenshotRoot = Join-Path $repoRoot "docs\validation\screenshots"
$unityImportScreenshot = Join-Path $validationScreenshotRoot "unity_forest_floor_import.png"
$unityDensityScreenshot = Join-Path $validationScreenshotRoot "unity_forest_floor_density.png"
$unrealImportScreenshot = Join-Path $validationScreenshotRoot "unreal_forest_floor_import.png"
$unrealFoliageScreenshot = Join-Path $validationScreenshotRoot "unreal_forest_floor_foliage_settings.png"
$unityCompileStubDir = Join-Path $outputRoot "unity_importer_compile_stub"
$unityCompileStubReport = Join-Path $outputRoot "$($PackageName)_unity_compile_stub_report.json"

if (!$ProjectScaffoldRoot) {
    $ProjectScaffoldRoot = Join-Path $outputRoot "projects"
}
elseif (![System.IO.Path]::IsPathRooted($ProjectScaffoldRoot)) {
    $ProjectScaffoldRoot = Join-Path $repoRoot $ProjectScaffoldRoot
}
$projectScaffoldSummary = Join-Path $ProjectScaffoldRoot "project_scaffold_summary.json"
$scaffoldUnityProject = Join-Path $ProjectScaffoldRoot "unity\MidoriUnityValidation"
$scaffoldUnrealProject = Join-Path $ProjectScaffoldRoot "unreal\MidoriUnrealValidation\MidoriUnrealValidation.uproject"

function Invoke-Checked {
    param(
        [string]$FilePath,
        [string[]]$Arguments,
        [string]$WorkingDirectory = $repoRoot
    )

    Push-Location $WorkingDirectory
    try {
        & $FilePath @Arguments
        $exitCode = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }

    if ($null -ne $exitCode -and $exitCode -ne 0) {
        throw "$FilePath exited with code $exitCode"
    }
}

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

function Convert-ToPythonString {
    param([string]$Value)

    return ConvertTo-Json $Value -Compress
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

function Remove-TempUnityProject {
    param([string]$Path)

    if (!$Path -or !(Test-Path -LiteralPath $Path)) {
        return
    }

    $resolvedOutput = (Resolve-Path -LiteralPath $outputRoot).Path
    $resolvedProject = (Resolve-Path -LiteralPath $Path).Path
    if (!$resolvedProject.StartsWith($resolvedOutput, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove Unity project outside output root: $resolvedProject"
    }

    Remove-Item -LiteralPath $resolvedProject -Recurse -Force
}

function Invoke-ProjectScaffold {
    $scriptPath = Join-Path $repoRoot "scripts\create_engine_validation_projects.ps1"
    & powershell `
        -NoProfile `
        -ExecutionPolicy Bypass `
        -File $scriptPath `
        -OutputRoot $ProjectScaffoldRoot `
        -UnrealDestinationRoot $UnrealDestinationRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Project scaffold generation failed with exit code $LASTEXITCODE"
    }
}

function Copy-UnityImporterIntoProject {
    param([string]$ProjectPath)

    $editorDir = Join-Path $ProjectPath "Assets\Editor"
    New-Item -ItemType Directory -Path $editorDir -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $repoRoot "integrations\unity\Editor\MidoriNaturePackageImporter.cs") `
        -Destination (Join-Path $editorDir "MidoriNaturePackageImporter.cs") `
        -Force
}

New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null

$projectScaffoldStatus = if ($SkipProjectScaffold) { "skipped" } else { "generated" }
$projectScaffoldPreflightStatus = if ($SkipProjectScaffold) { "skipped" } else { "not_run" }
if (!$SkipProjectScaffold) {
    Invoke-ProjectScaffold
    Invoke-Checked "python" @(
        (Join-Path $repoRoot "scripts\test_project_scaffold_static.py"),
        "--scaffold-root", $ProjectScaffoldRoot
    )
    $projectScaffoldPreflightStatus = "passed"
}

$summary = [ordered]@{
    generated_at = (Get-Date).ToString("o")
    repo_root = $repoRoot.Path
    package_dir = $packageDir
    project_scaffolds = [ordered]@{
        status = $projectScaffoldStatus
        preflight_status = $projectScaffoldPreflightStatus
        preflight_test = Join-Path $repoRoot "scripts\test_project_scaffold_static.py"
        root = $ProjectScaffoldRoot
        summary = $projectScaffoldSummary
        unity_project = $scaffoldUnityProject
        unreal_project = $scaffoldUnrealProject
    }
    midori = [ordered]@{}
    unity = [ordered]@{}
    unreal = [ordered]@{}
    unreal_fake_editor = [ordered]@{}
    profile_notes_preflight = [ordered]@{}
    completion = [ordered]@{
        phase_7_complete = $false
        reason = "Real Unity and Unreal editor imports still require successful editor runs with screenshots/profile notes."
    }
}

Invoke-Checked "python" @(
    (Join-Path $repoRoot "scripts\test_profile_notes_verifier.py")
)
$summary.profile_notes_preflight = [ordered]@{
    status = "passed"
    test = Join-Path $repoRoot "scripts\test_profile_notes_verifier.py"
    template = Join-Path $repoRoot "docs\validation\midori-nature-engine-profile-notes.template.md"
    note = "Editorless self-test for the completed Unity/Unreal profiling notes gate."
}

Invoke-Checked "cargo" @(
    "run", "-p", "midori-cli", "--",
    "nature",
    "-p", (Join-Path $repoRoot $Patch),
    "-o", $packageDir,
    "--map-resolution", "8",
    "--preview-resolution", "8",
    "--scatter-chunk-size", "8",
    "--verbose"
)

Invoke-Checked "cargo" @(
    "run", "-p", "midori-cli", "--",
    "validate-nature",
    "-i", $packageDir,
    "--report", $midoriReport,
    "--verbose"
)

$midoriReportJson = Get-Content -LiteralPath $midoriReport -Raw | ConvertFrom-Json
$profileBudgetFailures = @($midoriReportJson.profile_budget_summaries | Where-Object { -not $_.passed }).Count
$summary.midori = [ordered]@{
    status = "passed"
    report = $midoriReport
    asset_name = $midoriReportJson.manifest.asset_name
    schema_version = $midoriReportJson.manifest.schema_version
    map_files = $midoriReportJson.map_files.Count
    map_summaries = $midoriReportJson.map_summaries.Count
    map_relationships_status = "passed"
    normal_green_flip_mismatches = $midoriReportJson.map_relationships.normal_green_flip_mismatches
    grass_density_mask_r_mismatches = $midoriReportJson.map_relationships.grass_density_mask_r_mismatches
    prototype_files = $midoriReportJson.prototype_files.Count
    prototype_summaries = $midoriReportJson.prototype_summaries.Count
    profile_budget_summaries = $midoriReportJson.profile_budget_summaries.Count
    profile_budget_status = $(if ($profileBudgetFailures -eq 0) { "passed" } else { "failed" })
    profile_budget_failure_count = $profileBudgetFailures
    memory_total_payload_bytes = $midoriReportJson.memory_footprint.total_payload_bytes
    memory_decoded_map_bytes = $midoriReportJson.memory_footprint.decoded_map_bytes
    memory_scatter_binary_bytes = $midoriReportJson.memory_footprint.scatter_binary_bytes
    memory_prototype_mesh_bytes = $midoriReportJson.memory_footprint.prototype_mesh_bytes
    scatter_binary_chunks = $midoriReportJson.scatter_binary_files.Count
    scatter_json_chunks = $midoriReportJson.scatter_json_chunks.Count
    scatter_binary_summaries = $midoriReportJson.scatter_binary_summaries.Count
    scatter_binary_instances = $midoriReportJson.scatter_binary_instances
    scatter_parity_status = "passed"
    scatter_parity_matching_chunks = $midoriReportJson.scatter_parity.matching_chunk_count
    scatter_parity_missing_binary_chunks = $midoriReportJson.scatter_parity.missing_binary_chunk_count
    scatter_parity_extra_binary_chunks = $midoriReportJson.scatter_parity.extra_binary_chunk_count
    scatter_record_checksum_mismatches = $midoriReportJson.scatter_parity.record_checksum_mismatch_count
}

Invoke-Checked "python" @(
    (Join-Path $repoRoot "scripts\test_unity_importer_static.py"),
    "--package-dir", $packageDir,
    "--expect-chunks", "26",
    "--expect-instances", "222"
)

Invoke-Checked "python" @(
    (Join-Path $repoRoot "scripts\test_unity_importer_compile_stub.py"),
    "--source", (Join-Path $repoRoot "integrations\unity\Editor\MidoriNaturePackageImporter.cs"),
    "--work-dir", $unityCompileStubDir,
    "--package-dir", $packageDir,
    "--execution-report", $unityCompileStubReport
)

$unityCompileStubJson = Get-Content -LiteralPath $unityCompileStubReport -Raw | ConvertFrom-Json

$summary.unity_preflight = [ordered]@{
    status = "passed"
    test = Join-Path $repoRoot "scripts\test_unity_importer_static.py"
    compile_stub_status = "passed"
    compile_stub_test = Join-Path $repoRoot "scripts\test_unity_importer_compile_stub.py"
    compile_stub_dir = $unityCompileStubDir
    compile_stub_execution_status = $unityCompileStubJson.status
    compile_stub_execution_report = $unityCompileStubReport
    compile_stub_source_file_count = $unityCompileStubJson.source_file_count
    compile_stub_manifest_file_checksum = $unityCompileStubJson.manifest_file_checksum
    compile_stub_source_file_checksum_xor = $unityCompileStubJson.source_file_checksum_xor
    compile_stub_material_recipe_count = $unityCompileStubJson.material_recipe_count
    compile_stub_engine_import_recipe_count = $unityCompileStubJson.engine_import_recipe_count
    compile_stub_scatter_binary_chunks = $unityCompileStubJson.scatter_binary_chunks
    compile_stub_scatter_binary_instances = $unityCompileStubJson.scatter_binary_instances
    compile_stub_scatter_binary_records_validated = $unityCompileStubJson.scatter_binary_records_validated
    package = $packageDir
    scatter_binary_chunks = $midoriReportJson.scatter_binary_files.Count
    scatter_binary_instances = $midoriReportJson.scatter_binary_instances
    note = "Editorless source-contract, scatter-binary, stubbed C# compile, and stubbed C# validation-path execution preflight; not a substitute for a licensed Unity import."
}

if (!$SkipUnrealDryRun) {
    Invoke-Checked "python" @(
        (Join-Path $repoRoot "integrations\unreal\midori_nature_importer.py"),
        $packageDir,
        "--dry-run",
        "--report", $unrealDryRunReport
    )

    $unrealDryRunJson = Get-Content -LiteralPath $unrealDryRunReport -Raw | ConvertFrom-Json
    $summary.unreal = [ordered]@{
        dry_run_status = "passed"
        dry_run_report = $unrealDryRunReport
        editor_exe = Find-UnrealEditor
        editor_import_status = "not_run"
        asset_name = $unrealDryRunJson.asset_name
        manifest_file_checksum = $unrealDryRunJson.manifest_file_checksum
        source_file_count = $unrealDryRunJson.source_file_count
        source_file_checksum_xor = $unrealDryRunJson.source_file_checksum_xor
        scatter_binary_chunks = $unrealDryRunJson.scatter_binary_chunks
        scatter_binary_instances = $unrealDryRunJson.scatter_binary_instances
        scatter_binary_records_validated = $unrealDryRunJson.scatter_binary_records_validated
        scatter_binary_records_read = $unrealDryRunJson.scatter_binary_records_read
        scatter_binary_file_checksum_xor = $unrealDryRunJson.scatter_binary_file_checksum_xor
        scatter_binary_record_checksum_xor = $unrealDryRunJson.scatter_binary_record_checksum_xor
        scatter_chunk_reports = $unrealDryRunJson.scatter_chunks.Count
        foliage_type_status = $unrealDryRunJson.foliage_type_status
        foliage_type_expected_count = $unrealDryRunJson.foliage_type_expected_count
        foliage_type_count = $unrealDryRunJson.foliage_type_count
        foliage_cull_start_cm = $unrealDryRunJson.foliage_cull_start_cm
        foliage_cull_end_cm = $unrealDryRunJson.foliage_cull_end_cm
        console_cull_start_cm = $unrealDryRunJson.console_cull_start_cm
        console_cull_end_cm = $unrealDryRunJson.console_cull_end_cm
        material_parameter_set_count = $unrealDryRunJson.material_parameter_set_count
        groundcover_material_parameter_names = $unrealDryRunJson.groundcover_material_parameter_names
        terrain_material_parameter_names = $unrealDryRunJson.terrain_material_parameter_names
        material_recipe_count = $unrealDryRunJson.material_recipe_count
        terrain_material_recipe_file = $unrealDryRunJson.terrain_material_recipe_file
        groundcover_material_recipe_file = $unrealDryRunJson.groundcover_material_recipe_file
        engine_import_recipe_count = $unrealDryRunJson.engine_import_recipe_count
        unity_engine_import_recipe_file = $unrealDryRunJson.unity_engine_import_recipe_file
        unreal_engine_import_recipe_file = $unrealDryRunJson.unreal_engine_import_recipe_file
        surface_overlay_count = $unrealDryRunJson.surface_overlay_count
        surface_overlay_names = $unrealDryRunJson.surface_overlay_names
        rock_prototype_surface_targets = $unrealDryRunJson.rock_prototype_surface_targets
        log_prototype_surface_targets = $unrealDryRunJson.log_prototype_surface_targets
        shrub_prototype_surface_targets = $unrealDryRunJson.shrub_prototype_surface_targets
    }

    if (!$summary.unreal.editor_exe) {
        $summary.unreal.editor_import_status = "blocked_unreal_editor_not_found"
    }
}
else {
    $summary.unreal = [ordered]@{
        dry_run_status = "skipped"
        editor_exe = Find-UnrealEditor
        editor_import_status = "not_run"
    }
}

if (!$SkipUnrealEditor) {
    $unrealEditor = Find-UnrealEditor
    $summary.unreal.editor_exe = $unrealEditor
    $unrealProjectPath = $UnrealProject
    $unrealProjectSource = "supplied"
    if (!$unrealProjectPath -and !$SkipProjectScaffold -and (Test-Path -LiteralPath $scaffoldUnrealProject)) {
        $unrealProjectPath = $scaffoldUnrealProject
        $unrealProjectSource = "generated_scaffold"
    }

    if (!$unrealEditor) {
        $summary.unreal.editor_import_status = "blocked_unreal_editor_not_found"
    }
    elseif (!$unrealProjectPath) {
        $summary.unreal.editor_import_status = "blocked_unreal_project_not_supplied"
        $summary.unreal.editor_import_hint = "Pass -UnrealProject <path-to-uproject> or allow project scaffold generation to run the real Unreal editor import."
    }
    elseif (!(Test-Path -LiteralPath $unrealProjectPath)) {
        $summary.unreal.editor_import_status = "blocked_unreal_project_not_found"
        $summary.unreal.editor_project = $unrealProjectPath
    }
    else {
        $integrationDir = Join-Path $repoRoot "integrations\unreal"
        $wrapper = @(
            "import sys",
            "sys.path.insert(0, $(Convert-ToPythonString $integrationDir))",
            "import midori_nature_importer",
            "report = midori_nature_importer.import_midori_nature_package($(Convert-ToPythonString $packageDir), $(Convert-ToPythonString $UnrealDestinationRoot), import_screenshot=$(Convert-ToPythonString $unrealImportScreenshot), foliage_settings_screenshot=$(Convert-ToPythonString $unrealFoliageScreenshot))",
            "midori_nature_importer.write_report_file($(Convert-ToPythonString $unrealEditorReport), report)"
        ) -join "`n"
        $utf8NoBom = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllText($unrealPythonScript, $wrapper + "`n", $utf8NoBom)

        $summary.unreal.editor_import_status = "attempted"
        $summary.unreal.editor_project = (Resolve-Path -LiteralPath $unrealProjectPath).Path
        $summary.unreal.editor_project_source = $unrealProjectSource
        $summary.unreal.editor_report = $unrealEditorReport
        $summary.unreal.editor_log = $unrealImportLog
        $summary.unreal.editor_python_script = $unrealPythonScript
        $summary.unreal.import_screenshot = $unrealImportScreenshot
        $summary.unreal.foliage_settings_screenshot = $unrealFoliageScreenshot

        $unrealExitCode = Invoke-ProcessWait $unrealEditor @(
            (Resolve-Path -LiteralPath $unrealProjectPath).Path,
            "-Unattended",
            "-NoSplash",
            "-NullRHI",
            "-ExecutePythonScript=$unrealPythonScript",
            "-log=$unrealImportLog"
        )
        $summary.unreal.editor_exit_code = $unrealExitCode

        if ($unrealExitCode -eq 0 -and (Test-Path -LiteralPath $unrealEditorReport)) {
            $unrealEditorJson = Get-Content -LiteralPath $unrealEditorReport -Raw | ConvertFrom-Json
            $summary.unreal.editor_import_status = "passed"
            $summary.unreal.editor_manifest_file_checksum = $unrealEditorJson.manifest_file_checksum
            $summary.unreal.editor_source_file_count = $unrealEditorJson.source_file_count
            $summary.unreal.editor_source_file_checksum_xor = $unrealEditorJson.source_file_checksum_xor
            $summary.unreal.editor_scatter_binary_instances = $unrealEditorJson.scatter_binary_instances
            $summary.unreal.editor_scatter_binary_records_validated = $unrealEditorJson.scatter_binary_records_validated
            $summary.unreal.editor_scatter_binary_records_read = $unrealEditorJson.scatter_binary_records_read
            $summary.unreal.editor_scatter_binary_file_checksum_xor = $unrealEditorJson.scatter_binary_file_checksum_xor
            $summary.unreal.editor_scatter_binary_record_checksum_xor = $unrealEditorJson.scatter_binary_record_checksum_xor
            $summary.unreal.editor_scatter_chunk_reports = $unrealEditorJson.scatter_chunks.Count
            $summary.unreal.editor_foliage_type_status = $unrealEditorJson.foliage_type_status
            $summary.unreal.editor_foliage_type_expected_count = $unrealEditorJson.foliage_type_expected_count
            $summary.unreal.editor_foliage_type_count = $unrealEditorJson.foliage_type_count
            $summary.unreal.editor_foliage_cull_start_cm = $unrealEditorJson.foliage_cull_start_cm
            $summary.unreal.editor_foliage_cull_end_cm = $unrealEditorJson.foliage_cull_end_cm
            $summary.unreal.editor_console_cull_start_cm = $unrealEditorJson.console_cull_start_cm
            $summary.unreal.editor_console_cull_end_cm = $unrealEditorJson.console_cull_end_cm
            $summary.unreal.editor_material_parameter_set_count = $unrealEditorJson.material_parameter_set_count
            $summary.unreal.editor_groundcover_material_parameter_names = $unrealEditorJson.groundcover_material_parameter_names
            $summary.unreal.editor_terrain_material_parameter_names = $unrealEditorJson.terrain_material_parameter_names
            $summary.unreal.editor_material_recipe_count = $unrealEditorJson.material_recipe_count
            $summary.unreal.editor_terrain_material_recipe_file = $unrealEditorJson.terrain_material_recipe_file
            $summary.unreal.editor_groundcover_material_recipe_file = $unrealEditorJson.groundcover_material_recipe_file
            $summary.unreal.editor_engine_import_recipe_count = $unrealEditorJson.engine_import_recipe_count
            $summary.unreal.editor_unity_engine_import_recipe_file = $unrealEditorJson.unity_engine_import_recipe_file
            $summary.unreal.editor_unreal_engine_import_recipe_file = $unrealEditorJson.unreal_engine_import_recipe_file
            $summary.unreal.editor_rock_prototype_surface_targets = $unrealEditorJson.rock_prototype_surface_targets
            $summary.unreal.editor_log_prototype_surface_targets = $unrealEditorJson.log_prototype_surface_targets
            $summary.unreal.editor_shrub_prototype_surface_targets = $unrealEditorJson.shrub_prototype_surface_targets
            $summary.unreal.editor_import_screenshot_status = $unrealEditorJson.screenshots.import.status
            $summary.unreal.editor_import_screenshot_method = $unrealEditorJson.screenshots.import.method
            $summary.unreal.editor_import_screenshot_exists = $unrealEditorJson.screenshots.import.exists
            $summary.unreal.editor_import_screenshot_bytes = $unrealEditorJson.screenshots.import.bytes
            $summary.unreal.editor_import_screenshot_checksum = $unrealEditorJson.screenshots.import.checksum
            $summary.unreal.editor_import_screenshot_width = $unrealEditorJson.screenshots.import.width
            $summary.unreal.editor_import_screenshot_height = $unrealEditorJson.screenshots.import.height
            $summary.unreal.editor_foliage_settings_screenshot_status = $unrealEditorJson.screenshots.foliage_settings.status
            $summary.unreal.editor_foliage_settings_screenshot_method = $unrealEditorJson.screenshots.foliage_settings.method
            $summary.unreal.editor_foliage_settings_screenshot_exists = $unrealEditorJson.screenshots.foliage_settings.exists
            $summary.unreal.editor_foliage_settings_screenshot_bytes = $unrealEditorJson.screenshots.foliage_settings.bytes
            $summary.unreal.editor_foliage_settings_screenshot_checksum = $unrealEditorJson.screenshots.foliage_settings.checksum
            $summary.unreal.editor_foliage_settings_screenshot_width = $unrealEditorJson.screenshots.foliage_settings.width
            $summary.unreal.editor_foliage_settings_screenshot_height = $unrealEditorJson.screenshots.foliage_settings.height
        }
        else {
            $summary.unreal.editor_import_status = "failed_import"
        }
    }
}

Invoke-Checked "python" @(
    (Join-Path $repoRoot "scripts\test_unreal_importer_fake_editor.py"),
    "--package", $packageDir,
    "--report", $unrealFakeEditorReport
)
$unrealFakeEditorJson = Get-Content -LiteralPath $unrealFakeEditorReport -Raw | ConvertFrom-Json
$summary.unreal_fake_editor = [ordered]@{
    status = "passed"
    report = $unrealFakeEditorReport
    import_task_count = $unrealFakeEditorJson.import_task_count
    foliage_type_count = $unrealFakeEditorJson.foliage_type_count
    scatter_binary_chunks = $unrealFakeEditorJson.scatter_binary_chunks
    scatter_binary_instances = $unrealFakeEditorJson.scatter_binary_instances
    scatter_binary_records_validated = $unrealFakeEditorJson.scatter_binary_records_validated
    import_screenshot_status = $unrealFakeEditorJson.screenshots.import.status
    import_screenshot_method = $unrealFakeEditorJson.screenshots.import.method
    import_screenshot_report_exists = $unrealFakeEditorJson.screenshots.import.exists
    import_screenshot_report_bytes = $unrealFakeEditorJson.screenshots.import.bytes
    import_screenshot_report_checksum = $unrealFakeEditorJson.screenshots.import.checksum
    import_screenshot_report_width = $unrealFakeEditorJson.screenshots.import.width
    import_screenshot_report_height = $unrealFakeEditorJson.screenshots.import.height
    import_screenshot_checksum = $unrealFakeEditorJson.screenshot_metrics.import.checksum
    import_screenshot_luminance_range = $unrealFakeEditorJson.screenshot_metrics.import.luminance_range
    foliage_settings_screenshot_status = $unrealFakeEditorJson.screenshots.foliage_settings.status
    foliage_settings_screenshot_method = $unrealFakeEditorJson.screenshots.foliage_settings.method
    foliage_settings_screenshot_report_exists = $unrealFakeEditorJson.screenshots.foliage_settings.exists
    foliage_settings_screenshot_report_bytes = $unrealFakeEditorJson.screenshots.foliage_settings.bytes
    foliage_settings_screenshot_report_checksum = $unrealFakeEditorJson.screenshots.foliage_settings.checksum
    foliage_settings_screenshot_report_width = $unrealFakeEditorJson.screenshots.foliage_settings.width
    foliage_settings_screenshot_report_height = $unrealFakeEditorJson.screenshots.foliage_settings.height
    foliage_settings_screenshot_checksum = $unrealFakeEditorJson.screenshot_metrics.foliage_settings.checksum
    foliage_settings_screenshot_luminance_range = $unrealFakeEditorJson.screenshot_metrics.foliage_settings.luminance_range
    note = "Fake Unreal module exercised import_midori_nature_package, screenshot capture, foliage asset creation, and scatter rejection without requiring UnrealEditor.exe."
}

if ($SkipUnity) {
    $summary.unity = [ordered]@{
        status = "skipped"
        editor_exe = Find-UnityEditor
    }
}
else {
    $unityEditor = Find-UnityEditor
    if (!$unityEditor) {
        $summary.unity = [ordered]@{
            status = "blocked_unity_editor_not_found"
            editor_exe = $null
        }
    }
    else {
        $createdTempProject = $false
        $unityProjectPath = $UnityProject
        $unityProjectSource = "supplied"
        $needsProjectCreate = $false
        if (!$unityProjectPath) {
            if (!$SkipProjectScaffold -and (Test-Path -LiteralPath $scaffoldUnityProject)) {
                $unityProjectPath = $scaffoldUnityProject
                $unityProjectSource = "generated_scaffold"
            }
            else {
                $unityProjectPath = Join-Path $outputRoot ("unity_project_" + [Guid]::NewGuid().ToString("N"))
                $unityProjectSource = "temporary_create"
                $createdTempProject = $true
                $needsProjectCreate = $true
            }
        }
        elseif (!(Test-Path -LiteralPath $unityProjectPath)) {
            $unityProjectSource = "supplied_create"
            $needsProjectCreate = $true
        }

        $summary.unity = [ordered]@{
            status = "attempted"
            editor_exe = $unityEditor
            project_path = $unityProjectPath
            project_source = $unityProjectSource
            create_log = $unityCreateLog
            import_log = $unityImportLog
            report = $unityReport
            import_screenshot = $unityImportScreenshot
            density_screenshot = $unityDensityScreenshot
        }

        if ($needsProjectCreate) {
            $createExitCode = Invoke-ProcessWait $unityEditor @(
                "-batchmode",
                "-quit",
                "-createProject", $unityProjectPath,
                "-logFile", $unityCreateLog
            )
            $licenseBlocked = Test-LogContains $unityCreateLog "No valid Unity Editor license"
            if ($createExitCode -ne 0 -or $licenseBlocked -or !(Test-Path -LiteralPath $unityProjectPath)) {
                $summary.unity.status = if ($licenseBlocked) {
                    "blocked_unity_license"
                }
                else {
                    "failed_project_create"
                }
                $summary.unity.create_exit_code = $createExitCode
                if ($createdTempProject) {
                    Remove-TempUnityProject $unityProjectPath
                    $summary.unity.temp_project_removed = !(Test-Path -LiteralPath $unityProjectPath)
                }
            }
        }

        if ($summary.unity.status -eq "attempted") {
            Copy-UnityImporterIntoProject $unityProjectPath

            $importExitCode = Invoke-ProcessWait $unityEditor @(
                "-batchmode",
                "-quit",
                "-projectPath", $unityProjectPath,
                "-executeMethod", "Midori.Unity.MidoriNaturePackageImporter.BatchImportNaturePackage",
                "-midoriPackage", $packageDir,
                "-midoriImportRoot", "Assets/Midori/Imported",
                "-midoriReport", $unityReport,
                "-midoriImportScreenshot", $unityImportScreenshot,
                "-midoriDensityScreenshot", $unityDensityScreenshot,
                "-logFile", $unityImportLog
            )
            $licenseBlocked = Test-LogContains $unityImportLog "No valid Unity Editor license"
            $summary.unity.import_exit_code = $importExitCode

            if ($importExitCode -eq 0 -and (Test-Path -LiteralPath $unityReport)) {
                $unityReportJson = Get-Content -LiteralPath $unityReport -Raw | ConvertFrom-Json
                $summary.unity.status = "passed"
                $summary.unity.asset_name = $unityReportJson.assetName
                $summary.unity.manifest_file_checksum = $unityReportJson.manifestFileChecksum
                $summary.unity.source_file_count = $unityReportJson.sourceFileCount
                $summary.unity.source_file_checksum_xor = $unityReportJson.sourceFileChecksumXor
                $summary.unity.detail_prototypes_created = $unityReportJson.detailPrototypesCreated
                $summary.unity.scatter_binary_instances = $unityReportJson.scatterBinaryInstances
                $summary.unity.scatter_binary_records_validated = $unityReportJson.scatterBinaryRecordsValidated
                $summary.unity.scatter_binary_records_read = $unityReportJson.scatterBinaryRecordsRead
                $summary.unity.scatter_binary_file_checksum_xor = $unityReportJson.scatterBinaryFileChecksumXor
                $summary.unity.scatter_binary_record_checksum_xor = $unityReportJson.scatterBinaryRecordChecksumXor
                $summary.unity.scatter_chunk_reports = $unityReportJson.scatterChunkReports.Count
                $summary.unity.material_parameter_set_count = $unityReportJson.materialParameterSetCount
                $summary.unity.groundcover_material_parameter_names = $unityReportJson.groundcoverMaterialParameterNames
                $summary.unity.terrain_material_parameter_names = $unityReportJson.terrainMaterialParameterNames
                $summary.unity.material_recipe_count = $unityReportJson.materialRecipeCount
                $summary.unity.terrain_material_recipe_file = $unityReportJson.terrainMaterialRecipeFile
                $summary.unity.groundcover_material_recipe_file = $unityReportJson.groundcoverMaterialRecipeFile
                $summary.unity.engine_import_recipe_count = $unityReportJson.engineImportRecipeCount
                $summary.unity.unity_engine_import_recipe_file = $unityReportJson.unityEngineImportRecipeFile
                $summary.unity.unreal_engine_import_recipe_file = $unityReportJson.unrealEngineImportRecipeFile
                $summary.unity.rock_prototype_surface_targets = $unityReportJson.rockPrototypeSurfaceTargets
                $summary.unity.log_prototype_surface_targets = $unityReportJson.logPrototypeSurfaceTargets
                $summary.unity.shrub_prototype_surface_targets = $unityReportJson.shrubPrototypeSurfaceTargets
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
            elseif ($licenseBlocked) {
                $summary.unity.status = "blocked_unity_license"
            }
            else {
                $summary.unity.status = "failed_import"
            }

            if ($createdTempProject -and !$KeepUnityProject) {
                Remove-TempUnityProject $unityProjectPath
                $summary.unity.temp_project_removed = !(Test-Path -LiteralPath $unityProjectPath)
            }
        }
    }
}

$summaryJson = $summary | ConvertTo-Json -Depth 8
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText($summaryPath, $summaryJson, $utf8NoBom)
Invoke-Checked "python" @(
    (Join-Path $repoRoot "scripts\verify_engine_evidence.py"),
    "--validation-root", $outputRoot,
    "--output", $evidenceReport,
    "--allow-pending"
)
Write-Output "Engine validation summary: $summaryPath"
Write-Output "Engine evidence verification: $evidenceReport"
