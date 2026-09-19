# Midori Nature Engine Profile Notes Template

Use this template to create `docs/validation/midori-nature-engine-profile-notes.md` after real Unity and Unreal editor imports run.

Before saving the completed evidence file, replace every TODO line with measured observations and remove this instruction block. The strict verifier rejects unresolved TODO/TBD/placeholder/pending/not-run language.

## Evidence Artifacts

- Unity report: `validation_root/forest_floor_unity_import_report.json`
- Unreal report: `validation_root/forest_floor_unreal_editor_report.json`
- Unity import screenshot: `docs/validation/screenshots/unity_forest_floor_import.png`
- Unity density screenshot: `docs/validation/screenshots/unity_forest_floor_density.png`
- Unreal import screenshot: `docs/validation/screenshots/unreal_forest_floor_import.png`
- Unreal foliage settings screenshot: `docs/validation/screenshots/unreal_forest_floor_foliage_settings.png`
- Package identity: TODO record the matching manifest checksum and source-file checksum evidence from both engine reports.

## Unity

- Unity Editor version: TODO record the exact editor version and render pipeline used for the mobile validation project.
- Project/package setup: TODO record the project path, importer path, glTFast/native GLB fallback status, and any packages enabled for the run.
- Mobile profile: TODO record the mobile profile used, including LOD distance interpretation, disabled grass/moss collision policy, and material slot budget.
- Import report fields: TODO record `detailPrototypesCreated`, `detailPrototypesGeneratedFromGlb`, `detailPrototypeFailures`, `detailPrototypeFallbackErrors`, `scatterBinaryRecordsRead`, and `scatterChunkReports`.
- Scale observation: TODO confirm terrain dimensions, prototype height/width scale, and that imported detail prototypes visually match Midori units.
- Density observation: TODO compare `unity_forest_floor_density.png` against the grass density map and report nonzero painted detail cells.
- Frame Debugger observation: TODO capture whether the detail prototypes render through instanced or batched draw paths and note any non-instanced fallback.
- Instanced rendering observation: TODO record the inspected renderer/detail-prototype settings proving the mobile path is suitable for repeated grass and ground-cover instances.
- Material slot observation: TODO record observed material slot counts on imported prototypes and confirm they stay inside the mobile material slot budget.
- Wind channel observation: TODO confirm imported vertex data keeps the expected Midori wind channels, including color and secondary UV usage.

## Unreal

- Unreal Editor version: TODO record the exact editor version and target RHI used for the console validation project.
- Project/package setup: TODO record the `.uproject`, enabled editor scripting/import plugins, destination root, and import command used.
- Console profile: TODO record the console profile used, including LOD distance interpretation, disabled grass/moss collision policy, and material slot budget.
- Import report fields: TODO record `foliage_type_count`, `foliage_type_assets`, `scatterBinaryRecordsRead`, and `scatterChunkReports`.
- Scale observation: TODO confirm terrain dimensions, prototype height/width scale, and that imported static meshes visually match Midori units.
- Cull observation: TODO confirm foliage cull settings use 3500 cm start and 7000 cm end for the console path.
- RenderDoc observation: TODO capture whether foliage instances are grouped into instanced draw calls and whether any prototype falls back to per-object draws.
- Instanced foliage observation: TODO inspect the generated `FoliageType_InstancedStaticMesh` assets and record density, shadow, cull, and mesh assignment evidence.
- Material slot observation: TODO record observed material slot counts on imported static meshes and confirm they stay inside the console material slot budget.
- Wind channel observation: TODO confirm imported vertex data keeps the expected Midori wind channels, including color and secondary UV usage.

## Verdict

- Strict verifier command: `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation`
- Result: TODO record the strict verifier result and output path.
- Remaining issues: TODO write `none` only after the strict verifier passes without `--allow-pending`.
