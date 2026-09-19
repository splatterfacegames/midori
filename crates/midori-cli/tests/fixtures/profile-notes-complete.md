# Synthetic profile text for automated tests

## Evidence Artifacts

The synthetic report names are forest_floor_unity_import_report.json and
forest_floor_unreal_editor_report.json. The synthetic image names are
unity_forest_floor_import.png, unity_forest_floor_density.png,
unreal_forest_floor_import.png and unreal_forest_floor_foliage_settings.png.
This file tests string validation only and makes no claim that an editor was
executed or an image was captured. It must stay in the Rust test fixture folder.

## Unity

Unity Editor version: synthetic test value. The mobile profile mentions LOD,
Frame Debugger, instanced rendering, scale, density, material slot and wind.
Fields exercised by the text policy are detailPrototypesCreated,
detailPrototypesGeneratedFromGlb, detailPrototypeFailures,
detailPrototypeFallbackErrors, scatterBinaryRecordsRead and scatterChunkReports.
These names are deliberate input vocabulary for validating text requirements;
they are not measured values and cannot replace real import report checks.

## Unreal

Unreal Editor version: synthetic test value. The console profile mentions
RenderDoc, instanced rendering, scale, density, cull, material slot and wind.
Fields exercised by the text policy are foliage_type_count and
foliage_type_assets. The cull vocabulary contains 3500 and 7000. The production
gate must still independently check actual assets, checksums, images, recipes,
scatter buffers and engine reports. String completeness is only one component.

## Verdict

The strict verifier remains a separate full-engine acceptance gate. This
synthetic fixture passes only the profile text completeness policy and cannot
prove the existence, freshness or quality of any real profiling observation.
