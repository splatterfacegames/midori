# Engine Validation Screenshots

This directory is reserved for Phase 7 Unity and Unreal evidence.

Expected generated or captured files:

- `unity_forest_floor_import.png`
- `unity_forest_floor_density.png`
- `unreal_forest_floor_import.png`
- `unreal_forest_floor_foliage_settings.png`

`scripts/verify_engine_evidence.py` treats these as evidence only when the files are valid PNG images at least 256x256 pixels and have enough visual luminance variation to reject blank or placeholder captures.
