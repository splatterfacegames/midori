# Midori Nature Goal: Mobile And Console Groundcover

Date: 2026-07-06

Goal: expand Midori from a tree generator into a deterministic nature asset generator for mobile and console games, starting with soil tiles, grass, moss, and low vegetation. Match the strongest ideas in GrassSystemThreeJS, then exceed them by producing baked, chunked, LOD-ready, engine-native assets instead of browser-runtime shader systems.

Reference repo: https://github.com/achrefelouafi/GrassSystemThreeJS

Pinned reference commit: `b236b2a38d9f35daa2ddc7b0152544b10e635d0c`

Current closure status: all editorless Midori, Unity-stub, Unreal dry-run, fake-editor, package, web, and handoff checks are implemented and passing. Phase 7 remains externally blocked on real editor evidence: a licensed Unity batch import, an installed Unreal Editor import, four returned screenshots, and completed profile notes. The latest verifier state is `pending` with 1610 passing checks, 7 missing editor-only artifacts, and 0 failed checks.

## Implementation Progress

Current implemented slice:

- `NaturePatch`, soil, cracks, groundcover, wind, and mobile/console profile schema live in [`crates/midori-core/src/nature.rs`](../crates/midori-core/src/nature.rs).
- `TerrainField` now samples deterministic height, normals, grass density, moss, wetness, and crack masks from the same Rust source of truth.
- `NatureMapSet` bakes canonical `height_u16.png`, `normal_yplus.png`, `normal_yminus.png`, `masks_rgba.png`, and `grass_density.png` buffers.
- `GroundcoverPrototype` generates low-cost grass clump, moss tuft, woodland flower, broadleaf weed, fallen litter, young shrub, faceted rock, and fallen log LOD meshes with packed UV/color metadata.
- `ScatterSet` generates deterministic chunked placement records with conservative bounds for engine importers and culling.
- `NaturePatch::write_package` writes the first package layout: `midori_nature.json`, `maps/`, `preview_tile.glb`, `prototypes/*.glb`, `instances/scatter.json`, and `instances/binary/*.bin`.
- `midori nature -p <patch.toml> -o <package_dir>` exposes that package writer through the CLI with map, preview, and scatter controls.
- `validate_nature_package` and `midori validate-nature -i <package_dir>` provide the first importer-conformance gate for manifest files, map dimensions, prototype GLB files, scatter JSON, and binary scatter records.
- `midori validate-nature --report <path>` emits the typed conformance report as JSON for engine validation runs, including decoded map dimensions, color type, channel ranges, encoded PNG checksums, map relationship checks for Unity/Unreal normal conventions and grass-density packing, engine-native `material_parameters` checks for static terrain/foliage material hookups, exportable `surface_overlays` checks for moss/wetness/crack mask channels and targets, prototype `surface_targets` for overlay routing, decoded prototype GLB mesh/primitive counts, vertex/triangle counts, normal/tangent validity, wind/color attribute presence, material-slot usage, mobile/console profile budget summaries for LOD triangles, ordered LOD switch distances, material slots, off-by-default grass/moss collision policy, predictable package memory-footprint bytes, per-tile instances, and per-chunk instances, bounds, encoded GLB checksums, per-chunk JSON/binary scatter summaries, per-chunk field ranges, file checksums, and logical record-checksum parity between debug JSON and engine binary buffers.
- `midori_nature.json` schema version 3 now includes axis conventions, terrain height range and bounds, normal conventions, material slots, engine-native `material_parameters` for static Unity/Unreal terrain and foliage materials, static `material_recipes` under `materials/*.recipe.json`, static engine import recipes under `engines/*.recipe.json`, exportable `surface_overlays` for static moss/wetness/crack masks, prototype `surface_targets` for rock/log/shrub overlay routing, prototype LOD metadata, JSON scatter metadata, binary scatter buffer metadata, wind packing, memory-footprint metadata, Unity hints, Unreal hints, and typed validation.
- Static material recipes are generated for `terrain_surface` and `groundcover_foliage`. They keep `shader_policy = "preview_only"` and `texture_pipeline = "parked"`, target Unity and Unreal engine-native material systems, list the required terrain mask texture or foliage vertex streams, and are counted in package memory footprint metadata.
- Static engine import recipes are generated for Unity mobile and Unreal console as `engines/unity_import.recipe.json` and `engines/unreal_import.recipe.json`. They keep `runtime_policy = "engine_native_static"`, list the expected native systems (`Unity TerrainData`, GPU-instanced detail mesh prefabs, `Unreal Landscape`, Static Mesh Foliage), reference all 59 package source files, bind to the static material recipes, and record profile-specific density, LOD, cull, shadow, collision, and per-chunk instance settings for editor importers to validate.
- Binary scatter buffers are written under `instances/binary/*.bin` using `midori.scatter.bin.v1`: 16-byte header plus 32-byte little-endian records.
- A JSON Schema draft for import tooling lives in [`docs/schema/midori_nature.schema.json`](schema/midori_nature.schema.json).
- Unity and Unreal importer helper scripts live in [`integrations/unity/Editor/MidoriNaturePackageImporter.cs`](../integrations/unity/Editor/MidoriNaturePackageImporter.cs) and [`integrations/unreal/midori_nature_importer.py`](../integrations/unreal/midori_nature_importer.py). Unity now has a batch import/report entry point and can create detail mesh prefabs either from Unity-imported GLB `GameObject` assets or from Midori's native GLB mesh fallback; Unreal now has a CPython `--dry-run --report` path for non-editor validation hosts. Both importers validate and report the engine-native material parameter contract, static material recipe contract, static engine import recipe contract, static surface overlay contract, and prototype surface-target routing.
- [`scripts/validate_engine_imports.ps1`](../scripts/validate_engine_imports.ps1) is the canonical local engine-validation runner: it regenerates the package, writes Midori and Unreal reports, generates Unity/Unreal validation project scaffolds by default, runs editorless Unity/Unreal preflight checks, self-tests the completed profile-notes gate, probes editor availability, uses generated scaffolds when explicit project paths are absent, records blockers, records fake-editor and real-editor screenshot artifact summaries when available, and emits `target/midori_engine_validation/engine_validation_summary.json`.
- [`scripts/export_engine_validation_handoff.ps1`](../scripts/export_engine_validation_handoff.ps1) freezes the generated validation package, Midori/Unreal/Unity-stub reports, Unity importer, Unreal importer, verifier, screenshot directory, profile-notes template, and profile-notes self-test into `target/midori_engine_validation_handoff/`, with a bundle-local `forest_floor_unity_compile_stub_report.json` reference and a `run_editor_validation.ps1` script for machines that have licensed Unity and installed Unreal editors; the handoff runner defaults to the bundled Unity/Unreal scaffold projects, the first editor run is documented with `-AllowPending` until profile notes are written, final verify-only runs preserve prior editor-run summary sections, and successful editor runs write `editor_handoff_summary.json` with report checksums plus screenshot status, bytes, checksums, and dimensions.
- [`scripts/import_engine_validation_handoff.ps1`](../scripts/import_engine_validation_handoff.ps1) preflights a returned editor handoff bundle against the local current package, then copies valid Unity/Unreal reports, screenshots, profile notes, and editor summaries into the canonical `target/midori_engine_validation/` and `docs/validation/` locations used by the strict verifier.
- [`scripts/create_engine_validation_projects.ps1`](../scripts/create_engine_validation_projects.ps1) creates portable Unity and Unreal validation project scaffolds with the Unity importer, a glTFast package dependency, an Unreal `.uproject`, Python editor scripting enabled, and README commands for source-repo or handoff-bundle editor runs.
- [`scripts/verify_engine_evidence.py`](../scripts/verify_engine_evidence.py) is the Phase 7 evidence gate. It checks the Midori package report, baked map formats/ranges/checksums, Unity/Unreal normal and density map relationships, engine-native material parameter bindings, static material recipes, static engine import recipes, prototype GLB attributes/material usage/checksums, scatter JSON/binary chunk summaries and parity, profile-specific package source fingerprints, the direct Unreal CPython dry-run report, Unreal import-task inventory/destination routing, Unity editor import report, Unreal editor import report, screenshots, profile notes, scale, density, LOD/cull metadata, material slots, wind packing, and scatter counts. It also checks the editorless Unity compile-stub import and density screenshot paths write nonblank 1024x1024 terrain and 512x512 grass/moss/crack visualizations. It rejects unresolved or under-specified profile notes and screenshot PNGs that are blank or placeholder-like. It is allowed to emit `pending` with `--allow-pending`, but should pass without that flag before Phase 7 is considered complete.
- [`scripts/test_project_scaffold_static.py`](../scripts/test_project_scaffold_static.py) checks the generated Unity/Unreal validation scaffolds without editors: Unity importer copy, `ProjectSettings`, glTFast and Unity module dependencies, Unreal `.uproject` plugins, Python editor scripting config, and validation READMEs.
- The Unity batch importer can now write the expected import and density screenshots via `-midoriImportScreenshot` and `-midoriDensityScreenshot`, then records `captured`/path/bytes/checksum/dimensions artifact summaries in the Unity report; the expected screenshot directory is documented in [`docs/validation/screenshots/README.md`](validation/screenshots/README.md).
- [`scripts/test_unity_importer_static.py`](../scripts/test_unity_importer_static.py) checks the Unity importer source contract and validates the generated reference package scatter binaries without requiring a licensed Unity editor. The Unity importer now validates scatter binary magic/version/stride/length, finite in-bounds records, yaw/phase/scale/color ranges, can generate native Unity detail mesh prefabs directly from Midori GLBs when the project GLB importer does not expose `GameObject` assets, records screenshot artifact summaries, and emits per-chunk field ranges plus file/record checksums in successful Unity reports.
- [`scripts/test_unity_importer_compile_stub.py`](../scripts/test_unity_importer_compile_stub.py) compiles the Unity importer against small Unity API stubs with `dotnet build` and, when given a generated package, executes the importer's pure C# manifest/source/scatter validation paths, Unity report-field helpers for material parameters, material recipes, engine import recipes, surface overlays, and prototype surface targets, plus the Unity terrain/detail path with a fake asset mirror to exercise native GLB fallback detail-prototype generation, detail density layers, the import screenshot camera/render path, and density screenshot channel sampling before a licensed Unity editor is available.
- [`scripts/test_profile_notes_verifier.py`](../scripts/test_profile_notes_verifier.py) self-tests the profile-notes evidence gate: missing notes remain pending, copied templates and thin notes fail, and a completed note with the required Unity/Unreal report, screenshot, instancing, cull, material, wind, and strict-verifier evidence passes.
- [`scripts/test_unreal_importer_fake_editor.py`](../scripts/test_unreal_importer_fake_editor.py) exercises the real Unreal editor import function against a fake Unreal Python module, including nonblank gradient screenshot capture with checksum/dimension/luminance metrics, exact asset import task enqueueing/destination routing for manifest, maps, preview tile, and prototype LODs, foliage type asset creation for every LOD0 prototype with console density/cull/shadow properties, scatter summary reporting, and a corrupted-yaw rejection case, so editor-path regressions are caught before a machine with `UnrealEditor.exe` is available.
- The Unreal importer can now request the expected Unreal import and foliage-settings screenshots with `--import-screenshot` and `--foliage-settings-screenshot`, recording capture/request/unavailable status plus captured-file existence, byte count, checksum, and PNG dimensions in the import report.
- The Unreal importer now validates scatter binary magic/version/stride/length, finite in-bounds records, yaw/phase/scale/color ranges, emits per-chunk field ranges plus file/record checksums in both dry-run and editor reports, reports exact Unreal import task files/destinations/extensions for the 28 importable package assets, reports static surface overlays and rock/log/shrub prototype surface targets, and reports foliage type creation status/counts plus cull distances.
- The generated flower far LOD now fits the mobile LOD2 budget, profile metadata now includes ordered LOD switch distances and off-by-default grass/moss collision policy, and package validation rejects manifests whose profile budgets are lower than decoded prototype LOD or scatter instance costs or whose LOD distances are invalid.
- [`docs/validation/midori-nature-engine-profile-notes.template.md`](validation/midori-nature-engine-profile-notes.template.md) defines the notes that must become `midori-nature-engine-profile-notes.md` after real engine profiling.
- Initial Unity/Unreal import preflight notes live in [`docs/validation/midori-nature-unity-unreal.md`](validation/midori-nature-unity-unreal.md).
- The web editor can load `Forest Floor`, generate Rust/WASM terrain/prototype/scatter preview data, render the terrain tile plus instanced grass, moss, flower, weed, litter, shrub, rock, and log prototypes, expose editable terrain/soil, groundcover, wind, and mobile/console profile controls, force preview profile density and prototype LOD, and show nature-specific package stats.
- Representative nature patch presets now include [`temperate_forest_floor.toml`](../presets/nature/temperate_forest_floor.toml), [`flowering_meadow.toml`](../presets/nature/flowering_meadow.toml), and [`arid_scrub.toml`](../presets/nature/arid_scrub.toml).

Verified so far:

- `cargo fmt`
- `cargo fmt --check`
- `cargo test nature --lib`
  - verifies all eight representative groundcover families produce deterministic scatter instances
- `cargo test workspace_nature_presets_export_and_validate --lib`
  - parses every workspace nature preset, exports package directories, validates manifests/maps/prototypes/scatter buffers, and verifies every declared layer produces nonzero scatter
- `cargo test -p midori-cli`
- `cargo test`
- `cargo run -p midori-cli -- nature -p presets/nature/temperate_forest_floor.toml -o target/midori_nature_validation_smoke --map-resolution 8 --preview-resolution 8 --scatter-chunk-size 8 --verbose`
  - produced schema version 3, terrain bounds for a 16 m tile, 22 prototype meshes, `midori.scatter.bin.v1`, 26 binary scatter chunks, and 32-byte instance records
- `cargo run -p midori-cli -- validate-nature -i target/midori_nature_validation_smoke --verbose`
  - validated 5 maps, 22 prototype meshes, 222 JSON scatter instances, 26 binary chunks, and 222 binary scatter instances
- `cargo run -p midori-cli -- nature -p presets/nature/temperate_forest_floor.toml -o target/midori_engine_validation/forest_floor --map-resolution 8 --preview-resolution 8 --scatter-chunk-size 8 --verbose`
- `cargo run -p midori-cli -- validate-nature -i target/midori_engine_validation/forest_floor --report target/midori_engine_validation/forest_floor_midori_validation_report.json --verbose`
  - wrote a machine-readable validation report for 5 maps, 22 prototype meshes, 222 JSON scatter instances, 26 JSON scatter chunk summaries, 26 binary chunk summaries, 26 binary chunks, 222 binary scatter instances, and zero JSON/binary scatter parity mismatches
- `python integrations/unreal/midori_nature_importer.py target/midori_engine_validation/forest_floor --dry-run --report target/midori_engine_validation/forest_floor_unreal_dry_run_report.json`
  - validated Unreal import source files and binary scatter without Unreal Editor, reporting manifest checksum `0x0d20b1690d2c4eda`, 59 Unreal source files, source checksum XOR `0xba4964fb17d1ff26`, 26 binary chunks, 222 binary scatter instances, per-chunk scatter ranges/checksums, aggregate file/record checksum XORs, `material_parameter_set_count = 2`, static terrain parameter names `Midori_MaskTexture`, `Midori_MossMaskChannel`, `Midori_WetnessMaskChannel`, and `Midori_CrackMaskChannel`, groundcover parameter names for alpha cutoff, wind, fade, and color variation, `material_recipe_count = 2` with `materials/terrain_surface.recipe.json` and `materials/groundcover_foliage.recipe.json`, `engine_import_recipe_count = 2` with `engines/unity_import.recipe.json` and `engines/unreal_import.recipe.json`, static `surface_overlays = ["moss", "wetness", "cracks"]` on channels G/B/A, rock/log/shrub prototype surface targets, dry-run foliage status with 8 expected LOD0 foliage types, and console cull distances of 3500-7000 cm
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate_engine_imports.ps1`
  - regenerated the package and reports, recorded `project_scaffolds.status = "generated"`, `project_scaffolds.preflight_status = "passed"`, `project_scaffolds.unity_project = target/midori_engine_validation/projects/unity/MidoriUnityValidation`, `project_scaffolds.unreal_project = target/midori_engine_validation/projects/unreal/MidoriUnrealValidation/MidoriUnrealValidation.uproject`, `midori.status = "passed"`, `midori.map_summaries = 5`, `midori.map_relationships_status = "passed"`, `midori.prototype_summaries = 22`, `midori.profile_budget_summaries = 2`, `midori.profile_budget_status = "passed"`, `midori.memory_total_payload_bytes = 189889`, `midori.scatter_json_chunks = 26`, `midori.scatter_binary_summaries = 26`, `midori.scatter_parity_status = "passed"`, `profile_notes_preflight.status = "passed"`, `unity_preflight.status = "passed"`, `unity_preflight.compile_stub_status = "passed"`, `unity_preflight.compile_stub_execution_status = "passed"`, `unity_preflight.compile_stub_source_file_count = 59`, `unity_preflight.compile_stub_manifest_file_checksum = "0x0d20b1690d2c4eda"`, `unity_preflight.compile_stub_engine_import_recipe_count = 2`, `unity_preflight.compile_stub_scatter_binary_chunks = 26`, `unity_preflight.compile_stub_scatter_binary_instances = 222`, `unity_preflight.compile_stub_scatter_binary_records_validated = true`, `unreal.dry_run_status = "passed"`, `unreal.scatter_binary_records_validated = true`, `unreal.scatter_chunk_reports = 26`, `unreal.material_parameter_set_count = 2`, `unreal.material_recipe_count = 2`, `unreal.terrain_material_recipe_file = "materials/terrain_surface.recipe.json"`, `unreal.groundcover_material_recipe_file = "materials/groundcover_foliage.recipe.json"`, `unreal.engine_import_recipe_count = 2`, `unreal.unity_engine_import_recipe_file = "engines/unity_import.recipe.json"`, `unreal.unreal_engine_import_recipe_file = "engines/unreal_import.recipe.json"`, `unreal.groundcover_material_parameter_names` for alpha cutoff, wind, fade, and color variation, `unreal.terrain_material_parameter_names` for mask texture and G/B/A overlay channels, `unreal.surface_overlay_count = 3`, `unreal.surface_overlay_names = ["moss", "wetness", "cracks"]`, `unreal.rock_prototype_surface_targets = ["rock", "static_surface"]`, `unreal.log_prototype_surface_targets = ["log", "static_surface"]`, `unreal.shrub_prototype_surface_targets = ["groundcover_foliage", "shrub_base"]`, `unity.status = "blocked_unity_license"`, `unity.project_source = "generated_scaffold"`, `unity.import_exit_code = 198`, and `unreal.editor_import_status = "blocked_unreal_editor_not_found"`
- `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation --output target/midori_engine_validation/engine_evidence_verification.json --allow-pending`
  - emitted a `pending` evidence report with 1610 passing checks and 7 missing editor-only artifacts: Midori conformance, map format/range checks, map relationship checks, engine-native material parameter manifest checks for `terrain_surface` and `groundcover_foliage`, static material recipe checks for `materials/terrain_surface.recipe.json` and `materials/groundcover_foliage.recipe.json`, static engine import recipe checks for `engines/unity_import.recipe.json` and `engines/unreal_import.recipe.json`, static surface overlay manifest checks for moss/wetness/cracks on `masks_rgba.png`, prototype `surface_targets` checks for rock/log/shrub overlay routing, prototype GLB attribute/material/count checks including valid `NORMAL` and `TANGENT` frames, mobile/console profile budget checks for LOD triangles, ordered LOD switch distances, material slots, off-by-default grass/moss collision policy, package memory-footprint checks including `material_recipe_bytes = 3967` and `engine_import_recipe_bytes = 9593`, per-tile instances, and per-chunk instances, scatter JSON/binary summary and parity checks, project scaffold artifact checks, profile-specific package source identity checks, profile-notes gate self-test, editorless Unity preflight including the stubbed C# compile and validation-path execution gate with package path, checksum, terrain size, Unity height/detail resolution, native GLB detail prototype generation, nonzero detail cells, import screenshot camera/render sampling, density screenshot channel sampling, LOD count, material slot, material parameter, material recipe, engine import recipe, surface overlay, prototype surface-target, chunk-report, and scatter checksum checks, direct `forest_floor_unreal_dry_run_report.json` validation including dry-run foliage/cull status, 28 expected Unreal import tasks with map/prototype LOD/destination coverage, material parameter report fields, material recipe report fields, engine import recipe report fields, surface overlay report fields, and prototype surface-target fields, Unreal dry-run per-chunk record range/checksum checks, and `forest_floor_unreal_fake_editor_report.json` plus summary checks for fake-editor asset import, foliage creation, scatter rejection, nonblank 1024x1024 screenshot metrics, and importer-owned screenshot existence/byte/checksum/dimension summaries; real Unity editor report, Unreal editor report, screenshots, and completed profile notes are still missing
- `cargo test -p midori-core manifest_ --lib`
  - confirms the manifest records the preview-only shader policy, validates required map channels, requires static engine-native material parameters, material recipes, and engine import recipes, requires static moss/wetness/cracks `surface_overlays`, rejects incomplete material parameter sets, and rejects prototypes missing required surface targets
- `node -e "JSON.parse(require('fs').readFileSync('docs/schema/midori_nature.schema.json','utf8'))"`
  - confirms the JSON Schema remains parseable after adding required `material_parameters`, `material_recipes`, `engine_import_recipes`, and `surface_overlays`
- `python -m py_compile scripts/verify_engine_evidence.py scripts/test_profile_notes_verifier.py scripts/test_unity_importer_static.py scripts/test_unity_importer_compile_stub.py integrations/unreal/midori_nature_importer.py scripts/test_unreal_importer_fake_editor.py`
- `python scripts/test_profile_notes_verifier.py`
  - confirms missing profile notes remain pending, the copied TODO template and thin notes fail, and a completed Unity/Unreal profiling note shape passes
- `python scripts/test_unity_importer_static.py --package-dir target/midori_engine_validation/forest_floor --expect-chunks 26 --expect-instances 222`
  - confirms the Unity importer source still exposes the scatter validation/report contract, profile-specific package source fingerprint report fields, material parameter validation/report fields, material recipe validation/report fields, engine import recipe validation/report fields, prototype surface-target validation/report fields, and the generated reference package has 26 valid binary scatter chunks and 222 records
- `python scripts/test_unity_importer_compile_stub.py --source integrations/unity/Editor/MidoriNaturePackageImporter.cs --work-dir target/midori_engine_validation/unity_importer_compile_stub --package-dir target/midori_engine_validation/forest_floor --execution-report target/midori_engine_validation/forest_floor_unity_compile_stub_report.json`
  - compiles the Unity importer against local Unity API stubs with `dotnet build`, then runs a stubbed C# execution harness against the generated package; the latest report passed with manifest checksum `0x0d20b1690d2c4eda`, 59 Unity source files, source checksum XOR `0xbe19ee93741d3ab4`, terrain size `16 x 0.3901228 x 16`, Unity heightmap resolution 33, detail resolution 8, 8 LOD0 prototypes, 22 LOD files, 2 material slots, 2 material parameter sets, 2 material recipes, 2 engine import recipes, 3 surface overlays, rock/log/shrub prototype surface targets, 8 detail prototypes generated through Midori's native GLB fallback, 0 detail prototype failures, 512 nonzero detail cells, a 1024x1024 import screenshot stub with 1048576 sampled pixels and nonblank RGB ranges, a 512x512 density screenshot stub with 262144 sampled pixels and nonblank RGB channel ranges, 26 binary scatter chunks, 26 chunk reports, 222 binary scatter instances, and valid scatter records
- `python scripts/test_unreal_importer_fake_editor.py --package target/midori_engine_validation/forest_floor --report target/midori_engine_validation/forest_floor_unreal_fake_editor_report.json`
  - confirms the Unreal editor import path reports package source fingerprint fields, engine-native material parameter fields, material recipe fields, engine import recipe fields, static surface overlay fields, prototype surface-target fields, exact import task files/destinations/extensions for 28 importable package assets, scatter record validation/checksum fields, captures two fake 1024x1024 screenshots with importer-owned `exists = true`, `bytes = 152230`, `checksum = "0x572dfc9e51e071ca"`, `width = 1024`, and `height = 1024` summaries plus sidecar luminance range 91, records matching screenshot artifact fields in `engine_validation_summary.json`, enqueues every expected asset import task, creates one fake `FoliageType_InstancedStaticMesh` asset per LOD0 prototype with console density/shadow/cull settings, and rejects a corrupted out-of-range scatter yaw
- blank screenshot regression check against `scripts/verify_engine_evidence.py`
  - confirmed that a valid 256x256 flat-color PNG fails the screenshot evidence gate as placeholder-like, with luminance range 0
- `cargo test -p midori-core package_validation_rejects_wrong_map_color_type --lib`
  - confirms package validation rejects a valid PNG heightmap when it is not the required 16-bit grayscale `height_u16.png`
- `cargo test -p midori-core package_validation_rejects_mismatched_normal_convention_maps --lib`
  - confirms package validation rejects normal maps that are valid RGB8 images but are not paired Unity Y+ and Unreal Y- encodings
- `cargo test -p midori-core package_validation_rejects_mismatched_grass_density_map --lib`
  - confirms package validation rejects a grass density map that does not match the packed `masks_rgba.png` R channel
- `cargo test -p midori-core package_validation_rejects_corrupt_prototype_glb --lib`
  - confirms package validation parses prototype GLBs rather than only checking file existence
- `cargo test -p midori-core package_validation_rejects_scatter_binary_json_record_mismatch --lib`
  - confirms package validation rejects a valid-looking binary scatter buffer when its logical records no longer match the authored scatter JSON
- `cargo test -p midori-core package_validation_rejects_profile_triangle_budget_violation --lib`
  - confirms package validation rejects a manifest whose mobile profile budget is lower than decoded prototype LOD triangle costs
- `cargo test -p midori-core package_validation_rejects_profile_instance_budget_violation --lib`
  - confirms package validation rejects a manifest whose mobile profile budget is lower than decoded scatter instance counts
- `cargo test -p midori-core unordered_profile_lod_distances_fail --lib`
  - confirms profile validation rejects unordered LOD switch distances
- strict `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation`
  - confirmed the completion verifier still fails while real Unity/Unreal editor evidence is pending
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate_engine_imports.ps1 -UnrealProject <project.uproject>`
  - documented and automated as the real Unreal editor import path; this host cannot run it because no `UnrealEditor.exe` is installed
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/export_engine_validation_handoff.ps1`
  - wrote `target/midori_engine_validation_handoff/` with the frozen `forest_floor` package, Midori report, Unreal dry-run report, Unity compile-stub execution report, pending evidence report, Unity and Unreal importers, strict verifier, screenshot directory, profile-notes template and self-test, portable Unity/Unreal project scaffolds under `projects/`, README, handoff manifest, and `run_editor_validation.ps1`; the copied summary references the Unity stub report with a bundle-local relative path, the copied Unity stub report records `package_dir = "validation_root/forest_floor"`, and the handoff runner defaults to the bundled scaffold projects while recording real Unity/Unreal screenshot artifact summaries in `editor_handoff_summary.json` when editor reports are produced
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/create_engine_validation_projects.ps1 -OutputRoot target/midori_engine_validation_projects_smoke_portable -PortablePaths`
  - wrote a portable Unity scaffold with `Assets/Editor/MidoriNaturePackageImporter.cs` and `Packages/manifest.json`, plus an Unreal scaffold with `MidoriUnrealValidation.uproject`, Python editor scripting enabled, and machine-portable README commands
- `powershell -NoProfile -ExecutionPolicy Bypass -File target/midori_engine_validation_handoff/run_editor_validation.ps1 -VerifyOnly -AllowPending`
  - confirmed the copied handoff verifier layout still reports `pending` with 1610 passing checks and only the real Unity report, Unreal editor report, four screenshots, and completed profile notes missing
- `powershell -NoProfile -ExecutionPolicy Bypass -File target/midori_engine_validation_handoff/run_editor_validation.ps1 -UnityExe "C:/Program Files/Unity/Hub/Editor/6000.4.9f1/Editor/Unity.exe" -UnityProject target/midori_engine_validation_handoff/projects/unity/MidoriUnityValidation -SkipUnreal -AllowPending`
  - confirmed the handoff Unity runner reaches the installed Unity editor but is blocked before importer execution by local licensing: `unity.status = "blocked_unity_license"` and `unity.import_exit_code = 198`; no Unity report or screenshots are produced on this host
- `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/import_engine_validation_handoff.ps1 -HandoffDir target/midori_engine_validation_handoff -AllowPartial -AllowPending`
  - confirmed incomplete handoff ingest is safe: it writes `engine_handoff_preflight_verification.json` and `engine_handoff_ingest_summary.json`, copies no missing completion artifacts, and leaves canonical evidence `pending` with 1610 passing checks and the same seven editor-only artifacts missing
- `python scripts/test_project_scaffold_static.py --scaffold-root target/midori_engine_validation/projects`
- `python scripts/test_project_scaffold_static.py --scaffold-root target/midori_engine_validation_handoff/projects`
  - confirmed canonical and handoff Unity/Unreal project scaffolds carry the expected editor importer, Unity package dependencies, Unreal plugins, Python settings, and validation READMEs
- PowerShell parser checks for `scripts/validate_engine_imports.ps1`, `scripts/export_engine_validation_handoff.ps1`, and `target/midori_engine_validation_handoff/run_editor_validation.ps1`
  - confirmed the validation and handoff scripts parse after the surface-overlay report fields were added
- stale evidence-count search over docs, scripts, and the handoff README
  - confirmed no stale pre-recipe evidence counts remain in docs or validation scripts
- `git diff --check`
  - reported only existing LF-to-CRLF normalization warnings in tracked files; no whitespace errors
- `cargo run -p midori-cli -- nature -p presets/nature/flowering_meadow.toml -o target/midori_flowering_meadow_smoke --map-resolution 8 --preview-resolution 8 --scatter-chunk-size 8 --verbose`
  - produced 14 prototype meshes, 328 JSON scatter instances, and 26 binary chunks for a 24 m meadow tile
- `cargo run -p midori-cli -- validate-nature -i target/midori_flowering_meadow_smoke --verbose`
- `cargo run -p midori-cli -- nature -p presets/nature/arid_scrub.toml -o target/midori_arid_scrub_smoke --map-resolution 8 --preview-resolution 8 --scatter-chunk-size 8 --verbose`
  - produced 20 prototype meshes, 283 JSON scatter instances, and 47 binary chunks for a 24 m arid scrub tile
- `cargo run -p midori-cli -- validate-nature -i target/midori_arid_scrub_smoke --verbose`
- `python -m py_compile integrations/unreal/midori_nature_importer.py`
- `node -e "JSON.parse(require('fs').readFileSync('docs/schema/midori_nature.schema.json','utf8'))"`
- `wasm-pack build crates/midori-wasm --target web --out-dir ..\..\web\src\lib\wasm --out-name midori_wasm`
- `cd web && npm run build`
- `cd web && npm run test:browser`, producing screenshots for forest floor, flowering meadow, and arid scrub nature previews; forest floor reports 222 authoring scatter instances and 8 prototype families, arid scrub reports 283 authoring scatter instances and 7 prototype families after preset switching, and the smoke still verifies `Grass Density`, `Preview Profile`, and `Prototype LOD` controls
- `MIDORI_VISUAL_URL=http://127.0.0.1:5174 npm run test:browser` from `web/` on 2026-07-05
  - reran the browser smoke against a fresh Vite server after the static engine import recipe work, exited 0, and rewrote `web/target/browser-smoke/oak-high.png`, `joshua-high.png`, `forest-floor-nature.png`, `flowering-meadow-nature.png`, and `arid-scrub-nature.png`
  - final smoke summary was mode `nature`, canvas `809x852`, authoring LOD0 arid scrub with 10,984 vertices, 13,626 triangles, 283 scatter instances, and 7 prototype families
- `cd web && npm run build` on 2026-07-05
  - reran the production Svelte build after the browser smoke, exited 0, and reported only the existing unused `OutputNode.id`, SvelteKit dependency export, and large chunk warnings

Still open:

- actual Unity and Unreal project import validation with screenshots/profile notes
- `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation` must pass without `--allow-pending`
- Unity import validation is currently blocked on this host by Unity Editor licensing; `6000.4.9f1` exits with `No valid Unity Editor license found` before importer execution
- Unreal import validation is currently blocked on this host because `UnrealEditor.exe` is not installed
- hardening Unity and Unreal importers inside real projects after editor API testing

## Mission

Midori should generate nature assets that a game team can import, profile, and ship:

- trees, shrubs, cacti, yucca, and other plant meshes
- grass clumps, moss tufts, flowers, weeds, dead litter, and small groundcover
- soil and terrain tiles with height, normal, density, and material masks
- biome patches that combine plants, groundcover, soil, wind, LOD policy, and export metadata

The default export target is not a dynamic ecosystem simulator. The default target is reliable static or mostly-static nature content for mobile and console games.

Runtime-change systems are optional profiles for projects that actually need growth, seasons, trampling, farming, dynamic wetness, snow, or moss accumulation. The core Midori pipeline should not require those systems.

## Parked Work

The texture/PBR asset generation pipeline remains parked.

This goal may define material slots, expected map names, mask ranges, normal-map conventions, import metadata, and placeholder map outputs. It should not build image-generation prompts, PBR derivation, texture upscaling, texture curation, or generated albedo/normal/roughness asset pipelines.

## What To Match From GrassSystemThreeJS

### Shared Terrain Source Of Truth

GrassSystemThreeJS uses one shared height field across soil and grass:

- soil shaping uniforms: [`src/main.js#L153-L183`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L153-L183)
- moss height/texture uniforms: [`src/main.js#L199-L218`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L199-L218)
- `HEIGHT_FUNCTIONS`, including `mossMaskAt`, `mossHeightAt`, and `groundHeightAt`: [`src/main.js#L312-L367`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L312-L367)
- grass receives the same `soilUniforms`, `mossUniforms`, `noiseGLSL`, and `heightGLSL`: [`src/main.js#L569-L575`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L569-L575)
- each blade snaps to `groundHeightAt(iPos)`: [`src/grass.js#L213`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L213)

Midori must match the concept, but not the implementation. The source of truth should be a Rust-side `TerrainField` that can be baked into maps, sampled by tests, previewed in the editor, and exported through manifests.

### Procedural Soil And Overlay Masks

GrassSystemThreeJS exposes mounds, tone variation, moisture, cracks, moss, and grass coverage as separate controls:

- crack uniforms: [`src/main.js#L175-L181`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L175-L181)
- crack field function: [`src/main.js#L399-L409`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L399-L409)
- analytic surface normal from the height field: [`src/main.js#L424-L430`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L424-L430)
- GUI controls for mound, tone, wetness, cracks, and moss: [`src/main.js#L747-L888`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L747-L888)

Midori must expose equivalent authoring controls, then bake the results into deterministic map channels and metadata:

- height
- normal
- grass density
- moss mask
- wetness mask
- crack mask
- material blend masks
- optional scatter exclusion masks

### Compact Grass Instance Attributes

GrassSystemThreeJS uses a single instanced prototype and compact per-blade attributes:

- `InstancedBufferGeometry`: [`src/grass.js#L57`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L57)
- per-instance attributes for position, yaw, height, width, phase, curl, and color variation: [`src/grass.js#L65-L91`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L65-L91)
- density changes by editing `geometry.instanceCount`: [`src/grass.js#L92`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L92), [`src/grass.js#L276-L277`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L276-L277)
- coverage mask: [`src/grass.js#L169-L174`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L169-L174)
- blade curl and shape: [`src/grass.js#L176-L201`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L176-L201)
- wind offset: [`src/grass.js#L204-L214`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L204-L214)
- color gradient and translucency: [`src/grass.js#L235-L260`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L235-L260)

Midori should match the data model, but export engine-friendly assets:

- grass clump meshes, not one exported mesh per blade
- vertex colors or packed UV channels for phase, stiffness, height, color variation, and wind response
- deterministic scatter maps or instance buffers
- chunked tile instance data
- LOD0 clumps, LOD1 simplified clumps, LOD2 cards, LOD3 density fade or terrain-only

### Coherent Moss And Surface Overlay Idea

GrassSystemThreeJS applies moss consistently to the ground and imported models:

- shared moss enable and texture uniforms: [`src/model.js#L19-L22`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/model.js#L19-L22), [`src/model.js#L41-L56`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/model.js#L41-L56)
- model-space coverage mask and upward-facing accumulation: [`src/model.js#L117-L129`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/model.js#L117-L129)
- model-locked moss coordinates via inverse matrix: [`src/model.js#L164-L168`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/model.js#L164-L168), [`src/model.js#L253-L255`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/model.js#L253-L255)

Midori should exceed this by making overlays exportable:

- moss, wetness, dust, snow, and leaf-litter masks should be data channels
- overlays should work on terrain tiles, rocks, logs, roots, and trunk bases
- runtime shader support should be optional and engine-profile specific

### Performance Controls

GrassSystemThreeJS has several useful performance ideas:

- grass shadows disabled: [`src/grass.js#L267-L268`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L267-L268)
- direct density control in GUI: [`src/main.js#L960-L969`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L960-L969)
- expensive cloud raymarch has reduced-resolution controls: [`src/clouds.js#L234-L247`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/clouds.js#L234-L247), [`src/main.js#L1075-L1079`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L1075-L1079)

Midori should exceed this by making performance part of the exported contract:

- mobile and console export profiles
- per-profile density multipliers
- per-profile LOD thresholds
- per-profile shadow policy
- chunked culling bounds
- material-slot limits
- triangle and instance budgets

## What Not To Match

Do not copy these choices into Midori core:

- shader injection through Three.js `onBeforeCompile`: [`src/main.js#L436-L449`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/main.js#L436-L449), [`src/grass.js#L124-L226`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L124-L226), [`src/model.js#L147-L200`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/model.js#L147-L200)
- global `mesh.frustumCulled = false`: [`src/grass.js#L267`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L267)
- `Math.random()` instance generation: [`src/grass.js#L74-L82`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/grass.js#L74-L82)
- post-processing and volumetric cloud stack as part of asset output: [`src/postfx.js#L91-L138`](https://github.com/achrefelouafi/GrassSystemThreeJS/blob/b236b2a38d9f35daa2ddc7b0152544b10e635d0c/src/postfx.js#L91-L138)

These are fine for Midori's previewer. They should not be required in exported Unity or Unreal assets.

## Mobile And Console Product Requirements

### Runtime Assumptions

Default Midori nature assets should assume:

- content is static after import
- wind is lightweight and material-driven
- grass is placed by engine-native instancing or deterministic imported instance data
- terrain height and masks are baked
- density can be scaled by platform
- artists can override engine materials without losing asset structure

Optional dynamic modes may exist, but they must be explicit:

- dynamic growth
- crop/farming simulation
- trampling
- procedural erosion
- seasonal color/mask swaps
- runtime wetness/snow/moss accumulation

### Mobile Profile

Initial target: modern iOS/Android and handheld console style constraints.

Budgets per visible terrain tile:

- tile size: 8 m, 16 m, or 32 m presets
- terrain preview mesh: 32x32 to 128x128 vertices depending on profile
- grass draw path: engine instanced detail mesh or packed chunk mesh
- LOD0 grass clump: target 24 to 120 triangles
- LOD1 grass clump: target 8 to 40 triangles
- LOD2 grass card: target 2 to 8 triangles
- material slots per grass asset: 1
- alpha mode: prefer masked/alpha clip over translucent
- shadows: off by default for grass; optional near-only profile
- collision: off by default for grass and moss
- density maps: 8-bit unless a platform profile asks for more
- masks per tile: pack into RGBA where possible

Quality requirements:

- no single giant unculled field
- no required per-frame terrain height recomputation
- no required compute shaders
- no required tessellation
- no required PBR texture generation
- predictable memory footprint in the manifest

### Console Profile

Initial target: PlayStation/Xbox/Switch-like production constraints, with scalable quality.

Budgets per visible terrain tile:

- tile size: 16 m, 32 m, or 64 m presets
- terrain preview mesh: 64x64 to 256x256 vertices depending on profile
- LOD0 grass clump: target 80 to 300 triangles
- LOD1 grass clump: target 24 to 100 triangles
- LOD2 grass card: target 2 to 16 triangles
- material slots per grass asset: 1 by default, 2 maximum for high profile
- shadows: near-only or hero-patch only by default
- wind: vertex-stage, packed attributes, no per-instance CPU updates
- density scaling: mandatory metadata
- cull/fade distances: mandatory metadata
- LOD thresholds: mandatory metadata

Quality requirements:

- chunked culling and conservative bounds
- engine-native foliage support first
- stable LOD naming and aligned pivots
- generated normals and tangents must be valid
- compatible normal-map convention metadata for Unity and Unreal

## Export Contract

Midori nature exports should be directory packages, not just one GLB.

Example package:

```text
temperate_forest_floor_midori/
  midori_nature.json
  preview_tile.glb
  prototypes/
    grass_fine_LOD0.glb
    grass_fine_LOD1.glb
    grass_fine_LOD2.glb
    moss_tuft_LOD0.glb
    moss_tuft_LOD1.glb
    woodland_violet_LOD0.glb
    broadleaf_weed_LOD0.glb
    fallen_leaf_litter_LOD0.glb
  maps/
    height_u16.png
    normal_yplus.png
    normal_yminus.png
    masks_rgba.png
    grass_density.png
  materials/
    terrain_surface.recipe.json
    groundcover_foliage.recipe.json
  engines/
    unity_import.recipe.json
    unreal_import.recipe.json
  instances/
    scatter.json
    binary/
      fine_meadow_grass_grass_0_0.bin
      moss_moss_0_0.bin
```

`midori_nature.json` should include:

- generator version
- source seed
- units and axis conventions
- tile dimensions
- height range and terrain bounds
- bounds per tile and per prototype
- map dimensions and channel meanings
- material slot names
- surface overlay names, mask channels, targets, and static runtime policy
- static material recipe files and engine import recipe files
- normal convention
- LOD chain and thresholds
- mobile and console profile budgets
- density scale defaults
- wind attribute packing
- Unity import hints
- Unreal import hints

## Engine Integration Targets

### Unity

Use Unity's terrain and detail systems rather than requiring a custom runtime renderer.

Output should support:

- Terrain heightmap import
- Terrain Layer map/mask import
- Detail Mesh prefabs for grass, moss, flowers, weeds, and debris
- GPU-instanced detail mesh mode
- URP/HDRP Shader Graph templates as optional helpers
- Y+ normal maps by default
- density maps and scatter seeds for importer scripts

Unity documentation to respect:

- Terrain grass/details support textured quads and full meshes, with instanced mesh recommended for most arbitrary mesh details: https://docs.unity3d.com/Manual/terrain-Grass.html
- GPU-instanced details use the prefab material/shader and render in batches of 1,023 or fewer instances: https://docs.unity3d.com/Manual/terrain-Grass.html
- Unity normal maps are Y+, also known as OpenGL format: https://docs.unity3d.com/Manual/StandardShaderMaterialParameterNormalMap.html
- Unity glTFast is available for efficient glTF import/export workflows: https://docs.unity3d.com/Packages/com.unity.cloud.gltfast@5.2/manual/index.html

### Unreal

Use Unreal's foliage and landscape systems rather than requiring a custom runtime renderer.

Output should support:

- Landscape height and weight maps
- Static Mesh grass/moss/groundcover prototypes
- Foliage Type metadata
- Landscape Grass Type metadata
- material function parameter names for wind/fade/color variation
- DirectX/Y- normal maps or explicit Y+ import guidance
- FBX adapter path for Static Mesh assets where Unreal pipeline support is stronger than glTF

Unreal documentation to respect:

- Landscape Grass Type only works with Landscape Terrain Actor: https://dev.epicgames.com/documentation/en-us/unreal-engine/grass-quick-start-in-unreal-engine
- Static Mesh Foliage uses mesh instancing and can render many instances with a single draw call, while Actor Foliage has normal Actor cost: https://dev.epicgames.com/documentation/en-us/unreal-engine/foliage-mode-in-unreal-engine
- foliage cluster culling, start/end fade, scalability, and LOD caveats should shape exported metadata: https://dev.epicgames.com/documentation/en-us/unreal-engine/foliage-mode-in-unreal-engine
- Unreal glTF support is useful but extension support is not universal: https://dev.epicgames.com/documentation/unreal-engine/gltf-file-format-support-in-unreal-engine
- Unreal's FBX static mesh pipeline supports materials, multiple UV sets, smoothing groups, vertex colors, LODs, and custom collision: https://dev.epicgames.com/documentation/en-us/unreal-engine/fbx-static-mesh-pipeline-in-unreal-engine

## Data Model

Add a `NaturePatch` schema alongside `Species`.

```toml
[asset]
kind = "nature_patch"
name = "Temperate Forest Floor"
units = "meters"

[patch]
size = 16.0
seed = 42
biome = "forest_floor"
tags = ["temperate", "grass", "moss", "damp_soil"]

[soil]
profile = "loam"
mound_scale = 0.12
mound_height = 0.55
mound_coverage = 1.0
relief_scale = 0.7
relief_strength = 0.6

[soil.cracks]
enabled = false
amount = 0.75
plate_density = 0.9
channel_width = 0.06
warp = 0.0
depth = 0.7

[[groundcover.layers]]
kind = "grass"
name = "fine meadow grass"
density = 0.13
coverage = 0.62
patch_scale = 0.15
patch_softness = 0.251
height = 1.5
width = 0.049
curl = 1.14
color_base = "#33421b"
color_tip = "#9bc24a"

[[groundcover.layers]]
kind = "moss"
coverage = 0.55
patch_scale = 0.14
height = 0.14
relief_scale = 0.9
relief_strength = 0.7

[[groundcover.layers]]
kind = "flower"
name = "woodland violet"
density = 0.025
coverage = 0.18
patch_scale = 0.21
height = 0.28
width = 0.045

[[groundcover.layers]]
kind = "weed"
name = "broadleaf weed"
density = 0.04
coverage = 0.3
patch_scale = 0.17
height = 0.45
width = 0.06

[[groundcover.layers]]
kind = "litter"
name = "fallen leaf litter"
density = 0.06
coverage = 0.38
patch_scale = 0.1
height = 0.18
width = 0.09

[wind]
strength = 0.5
speed = 1.8
direction_degrees = 20
gust_scale = 0.35
flutter = 0.6
```

Core Rust outputs:

- `NaturePatch`
- `TerrainField`
- `GroundcoverLayer`
- `GroundcoverPrototype`
- `ScatterSet`
- `NatureTileMesh`
- `NatureExportManifest`

## Implementation Plan

### Phase 0: Mission And Guardrails

- Update project docs to state that Midori generates nature assets, not only trees.
- Keep the existing tree/plant pipeline stable.
- Keep shader injection explicitly preview-only; default engine packages should use baked maps, GLB/static meshes, scatter buffers, and engine-native material systems unless a dynamic simulation target explicitly asks for runtime nature mutation.
- Keep texture/PBR generation parked.
- Add this goal doc to the main planning index if one exists later.

Acceptance:

- docs describe plants, soil, groundcover, and biome patches
- no current tree preset behavior changes
- `cargo test` stays green

### Phase 1: NaturePatch Schema

- Add TOML parsing for `asset.kind = "nature_patch"`.
- Add soil, cracks, groundcover layers, and wind sections.
- Add validation for mobile/console budget fields.
- Add fixture tests for minimal and representative patches.

Acceptance:

- valid patch TOML parses into structured Rust types
- invalid layer kinds and negative sizes fail with clear errors
- fixed seed produces stable mask checksums and scatter counts

### Phase 2: TerrainField And Masks

- Port the useful shared-field concept from GrassSystemThreeJS into Rust:
  - mounds
  - edge taper
  - fine relief
  - moss mask
  - moss height contribution
  - crack mask
  - wetness mask
- Add CPU sampling for height and normals.
- Add image export for height, normals, and packed masks.

Acceptance:

- terrain height and normal are deterministic
- maps have expected dimensions, channel ranges, and stable checksums
- preview mesh can be generated from the same field

### Phase 3: Grass And Groundcover Prototypes

- Generate clump meshes from blade strips.
- Pack wind and variation data into vertex color and UV channels.
- Create LOD0, LOD1, and LOD2 prototype meshes.
- Add moss tuft and low vegetation prototype support.
- Keep material slots to one by default.

Acceptance:

- prototypes export as valid GLB
- all attributes are finite
- LOD pivots and bounds are aligned
- mobile and console budgets are checked in tests

### Phase 4: Deterministic Scatter

- Generate scatter from seed plus density/mask fields.
- Chunk instances by tile.
- Support density scale profiles.
- Export either instance buffers or seed/mask metadata for engine importers.

Acceptance:

- fixed seed produces stable instance count and positions
- scatter respects masks and exclusion zones
- chunk bounds are conservative
- debug JSON and binary engine buffers have matching chunk keys, bounds, instance counts, and logical record checksums
- no global no-culling assumption is needed

### Phase 5: Engine Package Export

- Export package directory with manifest, maps, prototype meshes, and optional instance buffers.
- Add Unity profile fields.
- Add Unreal profile fields.
- Add normal map convention output for both Y+ and Y- where possible.
- Keep GLB for prototype meshes and preview tile; add optional FBX adapter only if needed for Unreal workflows.

Acceptance:

- package validates against manifest schema
- Unity and Unreal metadata is present
- map channel meanings are documented in the manifest
- no engine-specific runtime shader is required for the default package

### Phase 6: Web Previewer

- Add nature patch preview mode to the editor.
- Use shader tricks as preview acceleration only.
- Let users scrub soil, moss, grass, wind, LOD, density, and profile settings.
- Add browser smoke for a soil+grass patch.

Acceptance:

- preview uses the same Rust-generated maps or equivalent deterministic field
- forced LOD/profile preview works
- browser smoke verifies nonblank terrain and groundcover
- browser smoke verifies a nature control edit regenerates the preview

### Phase 7: Engine Validation

- Create a small Unity import fixture project or documented import script.
- Create a small Unreal import fixture project or documented import script.
- Validate scale, normals, masks, density, LODs, culling, material slots, wind channel interpretation, and scatter JSON/binary parity.

Acceptance:

- at least one mobile profile imports into Unity with expected scale and density
- at least one console profile imports into Unreal with expected LOD/cull metadata
- manual screenshots and notes are added to `docs/validation/`

## Definition Of Done

- Midori has a documented nature mission covering soil and groundcover.
- `NaturePatch` schema exists and has fixtures.
- Rust can generate deterministic soil height/mask data.
- Rust can generate grass or moss prototype meshes with LODs.
- Export packages include maps, prototypes, manifest, and engine profile metadata.
- Web preview can inspect a representative nature patch.
- Mobile and console budgets are tested.
- Unity and Unreal import paths are documented or minimally automated.
- Shader injection remains preview-only; shipped mobile/console assets do not require runtime shader patching.
- Texture/PBR generation remains parked.

## Open Questions

- Should Midori's first engine helper target be Unity or Unreal?
- Should instance buffers be exported by default, or should importers regenerate scatter from seed and maps?
- Should terrain tiles target square patches only at first?
- Should we support hero grass clumps separately from mass groundcover?
- Answered for the first slice: moss is both a `surface_overlays` mask/material contract and a generated tuft mesh system.
- What is the first representative biome: temperate forest floor, meadow, arid scrub, or roadside grass?

## Recommended First Slice

Build a temperate forest floor patch:

- 16 m tile
- loam soil height field
- grass density mask
- moss mask
- wetness mask
- one fine grass clump with three LODs
- one moss tuft with two LODs
- one woodland flower with three LODs
- one broadleaf weed with three LODs
- one fallen litter patch with two LODs
- mobile and console export profiles
- web preview
- manifest validation

This slice is small enough to validate the architecture, but broad enough to prove Midori is no longer tree-only.
