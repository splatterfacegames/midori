# Midori Nature Unity And Unreal Validation Notes

Date: 2026-07-06

Status: automated package conformance plus Unity/Unreal importer helper scripts. Unity batch import and editorless Unity importer preflight are implemented, but the local editor is license-blocked. Unreal dry-run validation is implemented and passes, but no Unreal Editor install is available on this host. Actual Unity and Unreal project imports with screenshots/profile captures are still pending.

## Current Local Editor Probe

Latest probe command:

```powershell
pwsh -NoProfile -File scripts/validate_engine_imports.ps1 -SkipUnrealEditor
```

Current result on this host:

- Unity Hub editors are installed at `C:\Program Files\Unity\Hub\Editor\6000.3.12f1`, `6000.4.6f1`, and `6000.4.9f1`; the runner selected `6000.4.9f1`.
- Unity batch import is blocked by licensing, with `unity.status = "blocked_unity_license"` and `unity.import_exit_code = 198`.
- `UnrealEditor.exe` is not available on `PATH` or under the standard `C:\Program Files\Epic Games` / `C:\Program Files\Unreal Engine` roots, so real Unreal editor import remains `blocked_unreal_editor_not_found`.
- The refreshed evidence report remains `pending` with 1610 passed checks, 7 missing editor-only artifacts, and 0 failed checks.

## Package Under Test

Reference command:

```bash
midori nature -p presets/nature/temperate_forest_floor.toml -o target/midori_nature_validation_smoke --map-resolution 8 --preview-resolution 8 --scatter-chunk-size 8 --verbose
```

Current engine-validation package:

```bash
midori nature -p presets/nature/temperate_forest_floor.toml -o target/midori_engine_validation/forest_floor --map-resolution 8 --preview-resolution 8 --scatter-chunk-size 8 --verbose
```

Canonical local validation runner:

```powershell
pwsh -NoProfile -File scripts/validate_engine_imports.ps1
```

On a machine with Unity or Unreal installed, the runner generates validation project scaffolds under `target/midori_engine_validation/projects/` and uses them by default when explicit project paths are not supplied. You can still pass existing projects:

```powershell
pwsh -NoProfile -File scripts/validate_engine_imports.ps1 -UnityProject "D:/Projects/MidoriUnityValidation" -UnrealProject "D:/Projects/MidoriUnrealValidation/MidoriUnrealValidation.uproject"
```

The runner regenerates the package, writes the Midori conformance report, creates Unity and Unreal project scaffolds unless `-SkipProjectScaffold` is passed, runs editorless Unity preflight, runs the Unreal CPython dry-run, probes for Unity and Unreal editors, uses generated scaffolds when project paths are absent, records fake-editor screenshot artifact summaries, records real editor screenshot summaries when editor reports are produced, and writes:

- `target/midori_engine_validation/engine_validation_summary.json`
- `target/midori_engine_validation/engine_evidence_verification.json`
- `target/midori_engine_validation/forest_floor_midori_validation_report.json`
- `target/midori_engine_validation/forest_floor_unreal_dry_run_report.json`
- `target/midori_engine_validation/projects/project_scaffold_summary.json`
- `target/midori_engine_validation/forest_floor_unreal_editor_report.json` when `-UnrealProject` runs successfully
- `target/midori_engine_validation/unity_create.log` when a fallback Unity project creation is needed
- `target/midori_engine_validation/unity_import.log`
- `docs/validation/screenshots/unity_forest_floor_import.png` and `docs/validation/screenshots/unity_forest_floor_density.png` when Unity import reaches the batch importer
- `docs/validation/screenshots/unreal_forest_floor_import.png` and `docs/validation/screenshots/unreal_forest_floor_foliage_settings.png` when Unreal exposes a supported screenshot API during editor import

The runner also executes `scripts/test_project_scaffold_static.py`, which checks the generated Unity/Unreal validation scaffolds without requiring editors. It executes `scripts/test_unity_importer_static.py`, which checks the Unity importer source contract and independently validates the generated package's binary scatter buffers without requiring Unity. It executes `scripts/test_unity_importer_compile_stub.py`, which compiles the Unity importer against small Unity API stubs with `dotnet build`, then runs a stubbed C# execution harness through the importer's manifest/source/scatter validation paths, report-field helpers, native GLB fallback detail-prototype path, and detail-density layer path against the generated package. It also executes `scripts/test_unreal_importer_fake_editor.py`, which injects a small fake Unreal Python module and exercises `import_midori_nature_package`, exact import task files/destinations/extensions for manifest/maps/preview/prototype LODs, foliage type creation, screenshot capture hooks, and nonblank screenshot metrics without requiring `UnrealEditor.exe`; this writes `target/midori_engine_validation/forest_floor_unreal_fake_editor_report.json`. These checks are not substitutes for real editor imports, but they catch scaffold and importer-path regressions before licensed/installed editors are available.

Completion verifier:

```bash
python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation
```

On machines without the required editors, use `--allow-pending` only to produce a status report:

```bash
python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation --output target/midori_engine_validation/engine_evidence_verification.json --allow-pending
```

## Handoff Bundle

To move the exact generated package and verifier state to a machine with licensed Unity and installed Unreal editors, export a frozen handoff bundle:

```powershell
pwsh -NoProfile -File scripts/export_engine_validation_handoff.ps1
```

The bundle is written to `target/midori_engine_validation_handoff/` and contains:

- `validation_root/forest_floor/`
- the Midori validation report, Unreal dry-run report, Unity compile-stub execution report, summary, and pending evidence report
- Unity and Unreal importer helper scripts
- `scripts/verify_engine_evidence.py`
- `scripts/import_engine_validation_handoff.ps1`
- `scripts/create_engine_validation_projects.ps1`
- `scripts/test_unity_importer_compile_stub.py`
- `scripts/test_profile_notes_verifier.py`
- `projects/` with ready-to-open Unity and Unreal validation project scaffolds
- `docs/validation/screenshots/`
- `docs/validation/midori-nature-engine-profile-notes.template.md`
- `run_editor_validation.ps1`
- `handoff_manifest.json`

On the editor machine, run from inside the bundle against the bundled fixture projects:

```powershell
.\run_editor_validation.ps1 -UnityExe "C:\Program Files\Unity\Hub\Editor\<version>\Editor\Unity.exe" -UnrealEditorExe "C:\Program Files\Epic Games\UE_<version>\Engine\Binaries\Win64\UnrealEditor.exe" -AllowPending
```

Unity must be licensed. The runner uses `projects\unity\MidoriUnityValidation` by default when `-UnityProject` is omitted. The scaffold includes a glTFast Package Manager dependency by default, but Midori's Unity importer also has a native GLB fallback that can generate detail mesh prefabs directly from Midori prototype GLBs when the project importer does not expose them as `GameObject` assets. The runner uses `projects\unreal\MidoriUnrealValidation\MidoriUnrealValidation.uproject` by default when `-UnrealProject` is omitted. The Unreal scaffold enables Python editor scripting and supplies a real `.uproject`, but the editor may still need GLB import support enabled for prototype assets. To regenerate the scaffolds inside the bundle, run:

```powershell
pwsh -NoProfile -File scripts/create_engine_validation_projects.ps1 -OutputRoot "projects" -PortablePaths
```

The first editor run uses `-AllowPending` because the profile notes are normally written after the reports and screenshots exist. After editor imports and screenshot capture, inspect `validation_root/editor_handoff_summary.json`; successful editor sections record report checksums plus screenshot status, byte count, checksum, and dimensions. Then write `docs/validation/midori-nature-engine-profile-notes.md` from the template with real Unity and Unreal profiling observations. The completed notes must cite the Unity and Unreal report filenames, all four screenshot filenames, Unity Editor version, Unreal Editor version, mobile and console profile observations, Frame Debugger and RenderDoc instancing observations, material slot and wind-channel observations, Unity detail-prototype counts/fallback status, Unreal `foliage_type_count`/`foliage_type_assets`, and the 3500-7000 cm console foliage cull range. Then rerun strict verification without `-AllowPending`; the runner preserves the previous editor-run summary sections while updating verifier fields:

```powershell
.\run_editor_validation.ps1 -VerifyOnly
```

Return the completed bundle to the source repo, then preflight and ingest the real editor evidence into canonical repo paths:

```powershell
pwsh -NoProfile -File scripts/import_engine_validation_handoff.ps1 -HandoffDir "target/midori_engine_validation_handoff"
```

The ingest script verifies the returned reports/screenshots/profile notes against the local package before copying them into `target/midori_engine_validation/` and `docs/validation/`. For diagnostic checks against an incomplete bundle only, use:

```powershell
pwsh -NoProfile -File scripts/import_engine_validation_handoff.ps1 -HandoffDir "target/midori_engine_validation_handoff" -AllowPartial -AllowPending
```

Local smoke coverage for the handoff path:

```powershell
pwsh -NoProfile -File target/midori_engine_validation_handoff/run_editor_validation.ps1 -VerifyOnly -AllowPending
```

This currently reproduces the expected `pending` state with 1610 passing checks and the same seven editor-only artifacts missing. The incomplete-bundle ingest smoke also leaves canonical evidence `pending` with the same seven editor-only artifacts missing.

The verifier passes only when real Unity and Unreal editor reports plus screenshots/profile notes exist and satisfy the package contract. It also checks that the Midori validation report decoded the reference package maps as `L16` height, `RGB8` Unity/Unreal normals, `RGBA8` masks, and `L8` grass density, with nonzero encoded PNG checksums and nontrivial height/density/mask/normal ranges. The Unity Y+ and Unreal Y- normal maps must be paired encodings with matching red/blue channels and inverse green channels, and `grass_density.png` must match `masks_rgba.png` R exactly. The manifest must declare engine-native static `material_parameters` for `terrain_surface` and `groundcover_foliage`: terrain needs the mask texture plus moss/wetness/crack channel bindings, and groundcover needs alpha cutoff, wind, fade, and color-variation bindings. The manifest must also declare static `material_recipes` for `materials/terrain_surface.recipe.json` and `materials/groundcover_foliage.recipe.json`, with `engine_native_static` runtime policy, `preview_only` shader policy, `parked` texture pipeline, Unity and Unreal engine-native targets, terrain mask texture requirements, and groundcover wind/color vertex stream requirements. The manifest must declare static `engine_import_recipes` for `engines/unity_import.recipe.json` and `engines/unreal_import.recipe.json`; those recipes must use `engine_native_static`, carry Unity mobile and Unreal console profiles, reference all 59 package source files, bind the material recipes, and name the native systems that should consume the package (`Unity TerrainData`, GPU-instanced detail mesh prefabs, `Unreal Landscape`, and Static Mesh Foliage). The manifest must declare static `surface_overlays` for moss, wetness, and cracks from `maps/masks_rgba.png` channels G/B/A, with moss and wetness targeting terrain, rocks, and logs, and cracks targeting terrain plus scatter exclusion. Prototype metadata must declare stable `surface_targets`, including `rock`/`static_surface` for rocks, `log`/`static_surface` for logs, and `shrub_base` for shrubs, so static overlays can be routed without name inference. Prototype GLBs must parse, match manifest vertex/triangle counts, carry `POSITION`, valid `NORMAL`, valid `TANGENT`, `TEXCOORD_0`, `TEXCOORD_1`, and `COLOR_0`, stay within profile material-slot limits for used primitive materials, stay within mobile/console LOD triangle budgets, and have nonzero encoded GLB checksums. The manifest memory footprint must match actual package files and internally consistent decoded map, material recipe, engine import recipe, scatter header, scatter record, and total payload byte math. Mobile and console profiles must also carry positive, ordered LOD switch distances that end inside the cull range and keep grass/moss collision disabled by default. Scatter debug JSON and engine binary chunks must both summarize to 26 chunks, carry nonzero file and logical record checksums, keep yaw/phase/height/width/color-variation ranges inside the authored contract, stay within mobile/console per-tile and per-chunk instance budgets, and report zero chunk-key, bounds, instance-count, and record-checksum parity mismatches. The verifier checks generated Unity/Unreal scaffold artifacts, including Unity glTFast and module dependencies, `ProjectSettings`, Unreal Python/editor-scripting plugins, and validation READMEs. It directly loads `target/midori_engine_validation/forest_floor_unreal_dry_run_report.json` and checks its dry-run flag, Unreal map/normal hints, console profile metadata, prototype/material/LOD-threshold/instance-budget/collision declarations, material parameter report fields, material recipe report fields, engine import recipe report fields, surface overlay report fields, prototype surface-target report fields, wind packing, 28 expected Unreal import tasks with destination routing for the manifest, maps, preview tile, and 22 prototype LOD files, scatter totals, per-chunk scatter ranges, aggregate checksum XORs, dry-run foliage expected-count/cull metadata, and profile-specific package source identity. The engine summary must also show the profile-notes gate self-test, editorless Unity source/scatter preflight, stubbed C# compile preflight, and stubbed C# validation-path execution preflight passed; it then loads the Unity stub report and checks the bundle-local package path, manifest/source checksum parity, terrain size, Unity height/detail resolution, native GLB detail prototype generation, nonzero detail density cells, import screenshot write/checksum/dimensions and nonblank RGB channel ranges, density screenshot write/checksum/dimensions and nonblank RGB channel ranges, LOD counts, material slot fields, material parameter report fields, material recipe report fields, engine import recipe report fields, surface overlay report fields, prototype surface-target report fields, chunk reports, validated scatter records, and aggregate scatter checksum XORs. It also loads `target/midori_engine_validation/forest_floor_unreal_fake_editor_report.json` and verifies fake-editor destination routing, manifest/source checksums, 28 import tasks, 4 map imports, 22 prototype LOD imports, 8 LOD0 imports, 8 created foliage types, 3500-7000 cm foliage cull metadata, 26 scatter chunks, 222 validated records, corrupt-scatter rejection, and two captured 1024x1024 screenshot-path artifacts with importer-owned `exists`, byte-count, checksum, and dimensions plus nonzero sidecar luminance range 91. Successful Unity and Unreal editor reports must additionally prove that each engine helper imported the current package by matching manifest/source-file checksums, reported the material parameter contract, material recipe contract, engine import recipe contract, surface overlay contract, prototype surface-target routing, Unreal import task inventory/destination routing, read and validated 26 binary chunks and 222 records, emitted aggregate checksum XORs, and wrote per-chunk scatter ranges/checksums; Unreal reports must prove that one foliage type asset was created per LOD0 prototype with the console cull range, and Unity reports must prove that every created detail prototype came from either a project-imported GLB asset or Midori's native GLB fallback with zero fallback errors. Screenshots must be valid PNG images at least 256x256 pixels and must have enough visual luminance variation to reject blank or placeholder captures. The profile notes must be substantial, include the exact report and screenshot filenames, Unity/Unreal editor versions, mobile/console profile observations, Frame Debugger and RenderDoc instancing evidence, Unity detail-prototype/fallback report fields, Unreal foliage type/cull report fields, material slot observations, wind-channel observations, and strict verifier result, with no unresolved capture markers. Use `docs/validation/midori-nature-engine-profile-notes.template.md` as the capture template, then write the completed evidence to `docs/validation/midori-nature-engine-profile-notes.md`. The current local report is `pending`, with Midori preflight/map/prototype/profile-budget/material-parameter/material-recipe/engine-import-recipe/surface-overlay/prototype-surface-target/scatter checks, project scaffold checks, profile-notes gate self-test, editorless Unity preflight, Unity stub compile plus validation-path execution including import screenshot and density screenshot sampling, direct Unreal dry-run report checks, and Unreal fake-editor report/screenshot-path checks passing and the real editor reports/screenshots/profile notes missing.

For real Unity reports, `scripts/verify_engine_evidence.py` also requires the nested `importScreenshot` and `densityScreenshot` summaries to report `captured`, nonempty paths, existing files, nonzero byte counts and checksums, and exact 1024x1024 and 512x512 PNG dimensions. The standalone screenshot PNG checks still inspect the returned files for valid nonblank visual content.

For real Unreal reports, `scripts/verify_engine_evidence.py` also requires the nested `screenshots.import` and `screenshots.foliage_settings` summaries to include nonempty paths, supported capture/request methods, and `captured` or `requested` status. Captured entries must also carry `exists = true`, nonzero byte counts and checksums, and exact 1024x1024 PNG dimensions. The standalone returned screenshot PNGs still prove visual content.

Expected package layout:

- `midori_nature.json`
- `preview_tile.glb`
- `maps/height_u16.png`
- `maps/normal_yplus.png`
- `maps/normal_yminus.png`
- `maps/masks_rgba.png`
- `maps/grass_density.png`
- `materials/terrain_surface.recipe.json`
- `materials/groundcover_foliage.recipe.json`
- `engines/unity_import.recipe.json`
- `engines/unreal_import.recipe.json`
- `prototypes/fine_meadow_grass_lod0.glb`
- `prototypes/fine_meadow_grass_lod1.glb`
- `prototypes/fine_meadow_grass_lod2.glb`
- `prototypes/low_forest_moss_lod0.glb`
- `prototypes/low_forest_moss_lod1.glb`
- `prototypes/woodland_violet_lod0.glb`
- `prototypes/woodland_violet_lod1.glb`
- `prototypes/woodland_violet_lod2.glb`
- `prototypes/broadleaf_weed_lod0.glb`
- `prototypes/broadleaf_weed_lod1.glb`
- `prototypes/broadleaf_weed_lod2.glb`
- `prototypes/fallen_leaf_litter_lod0.glb`
- `prototypes/fallen_leaf_litter_lod1.glb`
- `prototypes/young_hazel_shrub_lod0.glb`
- `prototypes/young_hazel_shrub_lod1.glb`
- `prototypes/young_hazel_shrub_lod2.glb`
- `prototypes/mossy_field_stones_lod0.glb`
- `prototypes/mossy_field_stones_lod1.glb`
- `prototypes/mossy_field_stones_lod2.glb`
- `prototypes/fallen_branch_log_lod0.glb`
- `prototypes/fallen_branch_log_lod1.glb`
- `prototypes/fallen_branch_log_lod2.glb`
- `instances/scatter.json`
- `instances/binary/*.bin`

## Automated Conformance Gate

Run this before engine importer work:

```bash
midori validate-nature -i target/midori_engine_validation/forest_floor --report target/midori_engine_validation/forest_floor_midori_validation_report.json --verbose
```

The validator checks:

- `midori_nature.json` parses and passes typed schema-version validation
- every manifest-referenced map exists under `maps/` and matches `map_resolution`
- canonical map PNGs decode to the expected import formats: `height_u16.png` as 16-bit grayscale, `normal_yplus.png` and `normal_yminus.png` as RGB8, `masks_rgba.png` as RGBA8, and `grass_density.png` as 8-bit grayscale
- the JSON report records map width, height, color type, channel count, per-channel min/max values, and encoded PNG checksum for engine evidence checks
- map relationship checks prove `normal_yminus.png` is the green-channel-flipped partner of `normal_yplus.png`, and `grass_density.png` is identical to the packed grass channel in `masks_rgba.png`
- material recipe checks prove `materials/terrain_surface.recipe.json` and `materials/groundcover_foliage.recipe.json` exist, use the engine-native static runtime policy, keep shader injection preview-only, explicitly park the texture/PBR pipeline, match the manifest material parameters, and declare Unity/Unreal engine-native targets
- engine import recipe checks prove `engines/unity_import.recipe.json` and `engines/unreal_import.recipe.json` exist, use the engine-native static runtime policy, cover Unity mobile and Unreal console profiles, reference all package source files, bind the static material recipes, and declare the native engine systems expected to consume the package
- every manifest-referenced prototype GLB parses, matches declared vertex/triangle counts and bounds, carries valid `NORMAL` and `TANGENT` attributes plus wind/variation attributes in `TEXCOORD_1` and `COLOR_0`, uses indexed triangle primitives, and stays within mobile/console used-material limits
- profile budget summaries prove both `mobile` and `console` profiles pass with maximum prototype LOD triangle counts of 48 / 24 / 8, ordered LOD switch distances of 6 / 18 / 32 m for mobile and 12 / 35 / 70 m for console, grass/moss collision disabled by default, 222 authored scatter instances per tile, max 29 instances per culling chunk, and no triangle, material-slot, or instance-budget violations
- memory footprint metadata proves the reference package payload is 189889 bytes excluding the manifest, with 832 decoded map bytes, 3967 material recipe bytes, 9593 engine import recipe bytes, 83084 prototype GLB bytes, 7520 binary scatter bytes, and internally consistent scatter header/record byte counts
- profile-specific package identity proves the Unreal dry-run covered manifest checksum `0x0d20b1690d2c4eda`, 59 Unreal source files, and source checksum XOR `0xba4964fb17d1ff26`; real Unity and Unreal editor reports must match their profile-specific source-file checksums before they count as completion evidence
- Unity and Unreal hint paths resolve to package files
- every manifest-referenced prototype GLB exists and is non-empty
- `instances/scatter.json` parses into chunked scatter sets with finite in-bounds records
- every `instances/binary/*.bin` file has `MDSI` magic, version 1, 32-byte stride, matching instance count, exact byte length, finite records, and positions inside the manifest bounds
- JSON and binary scatter chunks have matching chunk keys, bounds, instance counts, and logical record checksums when both outputs are present
- the JSON report records per-chunk scatter source, layer, kind, bounds, field ranges, file checksum, and record checksum for both debug JSON and binary engine buffers

Reference smoke output currently validates 5 maps, 5 map summaries, passing normal/density relationship checks, 2 static material recipe files, 2 static engine import recipe files, 22 prototype meshes, 22 prototype GLB summaries, 2 passing profile budget summaries, 222 JSON scatter instances, 26 JSON scatter chunk summaries, 26 binary chunk summaries, 26 binary chunks, 222 binary scatter instances, and zero scatter parity mismatches. The smoke manifest reports terrain bounds for a 16 m tile.

Generated report:

- `target/midori_engine_validation/forest_floor_midori_validation_report.json`
- `target/midori_engine_validation/engine_validation_summary.json`
- `target/midori_engine_validation/engine_evidence_verification.json`

Additional preset smoke packages:

- `Flowering Meadow`: 24 m tile, 5 groundcover layers, 14 prototype meshes, 328 JSON scatter instances, 26 binary chunks, and 328 binary scatter instances.
- `Arid Scrub`: 24 m tile, 7 groundcover layers, 20 prototype meshes, 283 JSON scatter instances, 47 binary chunks, and 283 binary scatter instances.

## Manifest Checks

`midori_nature.json` schema version 3 must include:

- Y-up, Z-forward, right-handed axis metadata
- terrain height range and bounds for reconstructing Unity Terrain or Unreal Landscape scale
- Unity Y+ normal map path: `maps/normal_yplus.png`
- Unreal Y- normal map path: `maps/normal_yminus.png`
- `terrain_surface` and `groundcover_foliage` material slots
- `material_parameters` with `engine_native_static` terrain mask/channel bindings and groundcover alpha, wind, fade, and color-variation bindings
- `material_recipes` and `engine_import_recipes` with `engine_native_static` runtime policy
- prototype LOD files, vertex counts, triangle counts, and bounds for grass, moss, flower, weed, litter, shrub, rock, and log families
- chunked scatter metadata, JSON debug output, binary instance buffer list, and field order
- binary scatter format `midori.scatter.bin.v1` with 16-byte header and 32-byte little-endian records
- wind packing for `TEXCOORD_1` and `COLOR_0`
- mobile and console budget profiles
- memory-footprint byte metadata for maps, preview mesh, prototype meshes, scatter JSON, and scatter binary buffers
- Unity and Unreal import hints
- `shader_policy = "preview_only"`
- `texture_pipeline = "parked"`

## Unity Preflight

Target path: Unity Terrain plus GPU-instanced detail mesh prefabs.

Helper script:

- `integrations/unity/Editor/MidoriNaturePackageImporter.cs`

Copy the script into a Unity project's `Assets/Editor/` folder, then use `Tools > Midori > Import Nature Package...`. glTFast or another project GLB importer is useful and included in the generated scaffold, but the Midori importer can fall back to generating Unity `Mesh` assets and prefabs directly from Midori GLBs.

For batch validation:

```bash
Unity.exe -batchmode -quit -projectPath <unity_project> -executeMethod Midori.Unity.MidoriNaturePackageImporter.BatchImportNaturePackage -midoriPackage <package_dir> -midoriImportRoot Assets/Midori/Imported -midoriReport <report_path> -midoriImportScreenshot <import_png> -midoriDensityScreenshot <density_png>
```

The script currently:

- copies a validated package under `Assets/Midori/Imported`
- supports a batch import entry point with `-midoriPackage`, `-midoriImportRoot`, and `-midoriReport`
- configures height, mask, density, and Unity Y+ normal textures for import
- creates a `TerrainData` asset using `terrain.height_min`, `terrain.height_max`, `terrain.bounds_min`, and `tile_size`, resampling the Midori heightmap to a Unity-safe Terrain heightmap resolution
- creates GPU-instanced mesh detail prototypes from LOD0 prototype GLBs when Unity can load them as `GameObject` assets, otherwise uses Midori's native GLB parser to generate Unity `Mesh` assets and prefabs
- applies grass and moss detail layers from `grass_density.png` and `masks_rgba.png`
- reads and validates `instances/binary/*.bin` into a `MidoriNatureScatterAsset` ScriptableObject for deterministic placement tooling
- optionally writes an offscreen top-down terrain import screenshot with `-midoriImportScreenshot`
- optionally writes a density/moss/crack heatmap screenshot with `-midoriDensityScreenshot`
- records Unity screenshot artifact summaries with path, `captured` status, existence, byte count, checksum, and PNG dimensions
- writes a JSON report with imported asset path, terrain asset path, Unity map convention paths, terrain size and position, height/detail resolution, declared LOD counts, created detail prototype count, nonzero detail density cells, screenshot artifact summaries, material slot contract, material recipe files/targets, engine import recipe profiles/systems, wind packing fields, binary chunk count, scatter instance count, binary validation status, aggregate checksum XORs, per-chunk scatter ranges/checksums, tile size, mobile density/cull/shadow/material-slot profile values, and notes

Editorless Unity preflight:

```bash
python scripts/test_unity_importer_static.py --package-dir target/midori_engine_validation/forest_floor --expect-chunks 26 --expect-instances 222
python scripts/test_unity_importer_compile_stub.py --source integrations/unity/Editor/MidoriNaturePackageImporter.cs --work-dir target/midori_engine_validation/unity_importer_compile_stub --package-dir target/midori_engine_validation/forest_floor --execution-report target/midori_engine_validation/forest_floor_unity_compile_stub_report.json
```

These checks prove that `MidoriNaturePackageImporter.cs` still contains the expected binary scatter validation/report surface plus material recipe and engine import recipe report fields, that the generated reference package's `midori.scatter.bin.v1` files satisfy the same header, length, bounds, yaw, phase, scale, and color-variation invariants, that the importer compiles against the local Unity API stub surface, and that the importer's C# manifest/source/scatter validation paths accept the current generated package. They write `target/midori_engine_validation/forest_floor_unity_compile_stub_report.json`, currently passed with manifest checksum `0x0d20b1690d2c4eda`, 59 Unity source files, source checksum XOR `0xbe19ee93741d3ab4`, terrain size `16 x 0.3901228 x 16`, Unity heightmap resolution 33, detail resolution 8, 8 LOD0 prototypes, 22 LOD files, 2 material slots, 2 material parameter sets, 2 material recipes, 2 engine import recipes, 3 surface overlays, rock/log/shrub surface targets, 8 detail prototypes generated through Midori's native GLB fallback, 0 detail prototype failures, 512 nonzero detail cells, an import screenshot stub with checksum `0x69fa18ab71ae0339`, 1024x1024 dimensions, 1048576 sampled pixels, nonblank R/G/B ranges `0.08..0.72`, `0.1..0.82`, and `0.08..0.35`, a density screenshot stub with checksum `0x1d7d192510e1fc5e`, 512x512 dimensions, 262144 sampled pixels, nonblank R/G/B ranges `0.25..0.75`, `0.25..0.75`, and `0.3625..0.7875`, 26 binary scatter chunks, 26 chunk reports, 222 binary scatter instances, valid scatter records, and nonzero aggregate scatter checksum XORs. They are recorded by the validation runner as `unity_preflight.status = "passed"`, `unity_preflight.compile_stub_status = "passed"`, and `unity_preflight.compile_stub_execution_status = "passed"`.

Local attempt on 2026-07-05:

- Detected Unity editors under `C:\Program Files\Unity\Hub\Editor`: `6000.3.12f1`, `6000.4.6f1`, and `6000.4.9f1`.
- Generated the default validation scaffold at `target/midori_engine_validation/projects/unity/MidoriUnityValidation`, including `Assets/Editor/MidoriNaturePackageImporter.cs` and a glTFast Package Manager dependency.
- Attempted `6000.4.9f1` batch import against the generated scaffold.
- Unity exited before script execution with `No valid Unity Editor license found. Please activate your license.` and `return code 198`; see `target/midori_engine_validation/unity_import.log`.
- `scripts/validate_engine_imports.ps1` records this as `unity.status = "blocked_unity_license"`, `unity.project_source = "generated_scaffold"`, and `unity.import_exit_code = 198`.
- No Unity import report was produced because the editor never reached script compilation or `BatchImportNaturePackage`.

Import checks:

- Import `maps/height_u16.png` as the terrain height source.
- Import `maps/normal_yplus.png` for Unity/OpenGL normal convention.
- Import `maps/grass_density.png` as the primary terrain detail density map.
- Import prototype GLBs as detail mesh prefabs, either through the project's GLB importer or through Midori's native Unity fallback.
- Use `instances/binary/*.bin` for deterministic imported detail positions when the project does not regenerate scatter from seed and masks.
- Keep grass and moss collision disabled by default.
- Keep `groundcover_foliage` masked and two-sided.
- Respect the manifest mobile profile first: material slots = 1, grass shadows off, density scale = 0.65, cull start/end = 18 m / 32 m.
- Verify scale: the terrain tile is 16 m wide when `tile_size = 16.0`.
- Verify no custom runtime renderer or shader injection is required.

Evidence still needed:

- Unity import screenshot.
- Detail mesh density screenshot.
- Frame/debugger note proving instanced detail rendering path.
- Successful `forest_floor_unity_import_report.json` from a licensed Unity editor, with all created detail prototypes loaded from project-imported GLB assets or generated by Midori's native GLB fallback and zero fallback failures.
- `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation` must pass Unity checks for scale, mobile density, nonzero detail cells, material slots, wind packing, scatter instance count, and Unity scatter binary validation summaries.

## Unreal Preflight

Target path: Landscape height/weight maps plus Static Mesh Foliage or Landscape Grass Type.

Helper script:

- `integrations/unreal/midori_nature_importer.py`

Run from the Unreal Python console:

```python
py "B:/workshop/trees/midori/integrations/unreal/midori_nature_importer.py" "B:/workshop/trees/midori/target/midori_nature_validation_smoke" "/Game/Midori/Imported" --import-screenshot "B:/workshop/trees/midori/docs/validation/screenshots/unreal_forest_floor_import.png" --foliage-settings-screenshot "B:/workshop/trees/midori/docs/validation/screenshots/unreal_forest_floor_foliage_settings.png"
```

Run from the validation runner when Unreal Editor and a project are available:

```powershell
pwsh -NoProfile -File scripts/validate_engine_imports.ps1 -UnrealEditorExe "C:/Program Files/Epic Games/UE_5.4/Engine/Binaries/Win64/UnrealEditor.exe" -UnrealProject "D:/Projects/MidoriValidation/MidoriValidation.uproject"
```

Run a strict CPython dry-run without Unreal Editor:

```bash
python integrations/unreal/midori_nature_importer.py target/midori_engine_validation/forest_floor --dry-run --report target/midori_engine_validation/forest_floor_unreal_dry_run_report.json
```

The script currently:

- imports the manifest, preview tile, maps, normal map, and prototype GLBs
- validates every `midori.scatter.bin.v1` chunk while reading it
- writes a JSON import report under `Saved/MidoriImportReports`
- supports `--dry-run --report <path>` from CPython, validating all import sources and scatter binaries without importing Unreal's Python module
- reports Unreal landscape/normal hints, prototype LOD counts, material slot contract, wind packing fields, console density/cull/shadow/material-slot profile values, scatter chunk/instance totals, binary validation status, aggregate checksum XORs, and per-chunk scatter ranges/checksums
- reports static material recipe files, runtime policies, and Unity/Unreal engine-native material targets for terrain and groundcover
- reports static engine import recipe files, Unity mobile and Unreal console profiles, runtime policies, and expected native engine systems
- reports exact Unreal import task files, destination content paths, and file extensions for the 28 importable package assets: manifest, preview tile, four map/normal PNGs, and 22 prototype LOD GLBs
- optionally requests screenshots with `--import-screenshot` and `--foliage-settings-screenshot`, recording whether Unreal captured, requested, or could not support each screenshot plus captured-file existence, byte count, checksum, and PNG dimensions
- converts console cull start/end distances from meters to Unreal centimeters
- attempts to create `FoliageType_InstancedStaticMesh` assets for LOD0 prototypes when the installed Unreal Python API exposes the foliage factory
- leaves Landscape creation to the project pipeline, using `terrain.heightmap_file`, `terrain.height_min`, `terrain.height_max`, and `terrain.masks_file`

Local dry-run on 2026-07-05:

- `python -m py_compile integrations/unreal/midori_nature_importer.py` passed.
- CPython dry-run report passed for `target/midori_engine_validation/forest_floor`.
- Report path: `target/midori_engine_validation/forest_floor_unreal_dry_run_report.json`.
- Reported 26 binary chunks, 222 binary scatter instances, `scatter_binary_records_validated = true`, 26 per-chunk scatter summaries, nonzero file/record checksum XORs, `foliage_type_status = "dry_run"`, `foliage_type_expected_count = 8`, `foliage_cull_start_cm = 3500`, `foliage_cull_end_cm = 7000`, `console_cull_start_cm = 3500`, and `console_cull_end_cm = 7000`.
- Reported `import_task_count = 28`, import task extensions `.glb`, `.json`, and `.png`, 4 imported map files, 22 imported prototype LOD files, and 8 imported LOD0 prototype files with Unreal content destination paths.
- Reported `manifest_file_checksum = "0x0d20b1690d2c4eda"`, `source_file_count = 59`, `source_file_checksum_xor = "0xba4964fb17d1ff26"`, `material_recipe_count = 2`, `terrain_material_recipe_file = "materials/terrain_surface.recipe.json"`, `groundcover_material_recipe_file = "materials/groundcover_foliage.recipe.json"`, `engine_import_recipe_count = 2`, `unity_engine_import_recipe_file = "engines/unity_import.recipe.json"`, and `unreal_engine_import_recipe_file = "engines/unreal_import.recipe.json"`.
- `scripts/verify_engine_evidence.py --allow-pending` directly loads this dry-run report and verifies the Unreal profile/map/material/material-recipe/wind declarations, 28 import task reports, 26 chunk reports, 222 records, dry-run foliage/cull fields, per-chunk field ranges, and aggregate checksum XORs before marking the remaining editor-only evidence as pending.
- `scripts/test_unreal_importer_fake_editor.py --package target/midori_engine_validation/forest_floor --report target/midori_engine_validation/forest_floor_unreal_fake_editor_report.json` passed, including material recipe and engine import recipe report checks, fake 1024x1024 screenshot capture with importer-owned `exists = true`, `bytes = 152230`, `checksum = "0x572dfc9e51e071ca"`, `width = 1024`, and `height = 1024` summaries plus sidecar luminance range 91, matching `engine_validation_summary.json` screenshot fields, exact asset import task enqueueing/destination routing, one fake `FoliageType_InstancedStaticMesh` asset per LOD0 prototype with console density/shadow/cull settings, scatter report field checks, and corrupted out-of-range yaw rejection.
- `scripts/validate_engine_imports.ps1` records this as `unreal.dry_run_status = "passed"`, `unreal.scatter_binary_records_validated = true`, `unreal.scatter_chunk_reports = 26`, `unreal.material_recipe_count = 2`, `unreal.foliage_type_status = "dry_run"`, `unreal.foliage_type_expected_count = 8`, and `unreal.editor_import_status = "blocked_unreal_editor_not_found"`.
- `UnrealEditor.exe` was not found on `PATH`; `C:\Program Files\Epic Games` contains only `DirectXRedist` and `Launcher`, so real Unreal import was not run on this host.
- If Unreal Editor is found and `-UnrealProject` is omitted, the runner now uses the generated scaffold at `target/midori_engine_validation/projects/unreal/MidoriUnrealValidation/MidoriUnrealValidation.uproject`. It records `blocked_unreal_project_not_supplied` only when project scaffolds are skipped or unavailable and no explicit `.uproject` is passed.

Import checks:

- Import `maps/height_u16.png` as the landscape height source.
- Import `maps/masks_rgba.png` as landscape weight/mask data.
- Import `maps/normal_yminus.png` for Unreal/DirectX normal convention.
- Import prototype GLBs as Static Mesh assets, or route through FBX only if the project pipeline requires it.
- Use `instances/binary/*.bin` for deterministic foliage placement import when the project does not regenerate scatter from seed and masks.
- Configure foliage cull start/end from the console profile: 35 m / 70 m.
- Keep `groundcover_foliage` masked and two-sided.
- Keep grass shadows off unless a hero-patch profile enables them.
- Verify scale: the landscape tile is 16 m wide for the reference patch.
- Verify no custom runtime renderer or shader injection is required.

Evidence still needed:

- Unreal import screenshot.
- Foliage Type or Landscape Grass Type settings screenshot.
- RenderDoc/profile note proving instanced foliage path.
- Successful editor import report under `Saved/MidoriImportReports` from a machine with Unreal Editor installed.
- `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation --unreal-report <Saved/MidoriImportReports/report.json>` must pass Unreal checks for `dry_run = false`, normal/map hints, console cull metadata, LOD declarations, material slots, wind packing, scatter instance count, and Unreal scatter binary validation summaries.
