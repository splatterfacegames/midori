#!/usr/bin/env python3
"""Self-test the completed profile-notes evidence gate."""

from __future__ import annotations

import tempfile
from pathlib import Path

import verify_engine_evidence as evidence


GOOD_NOTES = """# Midori Nature Engine Profile Notes

## Evidence Artifacts

- Unity report: `validation_root/forest_floor_unity_import_report.json`.
- Unreal report: `validation_root/forest_floor_unreal_editor_report.json`.
- Unity import screenshot: `docs/validation/screenshots/unity_forest_floor_import.png`.
- Unity density screenshot: `docs/validation/screenshots/unity_forest_floor_density.png`.
- Unreal import screenshot: `docs/validation/screenshots/unreal_forest_floor_import.png`.
- Unreal foliage settings screenshot: `docs/validation/screenshots/unreal_forest_floor_foliage_settings.png`.
- Package identity: Unity and Unreal both reported the same manifest checksum and source-file checksum values as the package under test.

## Unity

- Unity Editor version: Unity 6000.0 validation build, mobile profile, batch import project with the Midori importer copied into Assets/Editor.
- Project/package setup: glTFast was available and Midori native GLB fallback remained enabled for detail prototypes. The report showed current package source identity and no stale package files.
- Mobile profile: LOD distances were read from the mobile profile, grass and moss collision were disabled, and material slot usage stayed within the mobile material slot budget.
- Import report fields: `detailPrototypesCreated` was greater than zero, `detailPrototypesGeneratedFromGlb` plus asset-loaded detail prototypes equaled the created count, `detailPrototypeFailures` was zero, `detailPrototypeFallbackErrors` was zero, `scatterBinaryRecordsRead` was 222, and `scatterChunkReports` was 26.
- Scale observation: terrain size and prototype height/width matched Midori units when compared against the import screenshot and inspector values.
- Density observation: `unity_forest_floor_density.png` showed nonzero painted detail cells aligned with the grass density map and masks channel.
- Frame Debugger observation: repeated grass and ground-cover detail prototypes used instanced rendering or Unity batching paths suitable for mobile; no inspected prototype required unique per-object draws.
- Instanced rendering observation: detail prototype settings kept the density workflow engine-native, with render settings that remain practical for repeated grass instances.
- Material slot observation: imported prototype material slot counts matched the report and stayed inside the mobile budget.
- Wind channel observation: imported vertex color and secondary UV data preserved the expected Midori wind channels.

## Unreal

- Unreal Editor version: Unreal 5.4 validation build, console profile, Python editor scripting importer run against the generated validation `.uproject`.
- Project/package setup: editor scripting and import plugins were enabled, assets were written under `/Game/Midori/Imported`, and the report showed current package source identity.
- Console profile: LOD distances were read from the console profile, grass and moss collision were disabled, and material slot usage stayed within the console material slot budget.
- Import report fields: `foliage_type_count` matched the number of LOD0 prototypes, `foliage_type_assets` listed every generated foliage type, `scatterBinaryRecordsRead` was 222, and `scatterChunkReports` was 26.
- Scale observation: terrain dimensions and static-mesh prototype height/width matched Midori units in the viewport and inspector.
- Cull observation: foliage cull settings used 3500 cm start and 7000 cm end, matching the console report fields and foliage asset inspector.
- RenderDoc observation: foliage instances were grouped into instanced draw calls suitable for console rendering, with no inspected prototype using per-object draws.
- Instanced foliage observation: generated `FoliageType_InstancedStaticMesh` assets had mesh assignment, density, cull, and shadow settings matching the import report.
- Material slot observation: imported static mesh material slot counts matched the report and stayed inside the console budget.
- Wind channel observation: imported vertex color and secondary UV data preserved the expected Midori wind channels.

## Verdict

- Strict verifier command: `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation`.
- Result: strict verifier passed and wrote `target/midori_engine_validation/engine_evidence_verification.json`.
- Remaining issues: none.
"""


def run_check(path: Path) -> evidence.EvidenceVerifier:
    verifier = evidence.EvidenceVerifier()
    evidence.check_profile_notes(verifier, path)
    return verifier


def require_status(verifier: evidence.EvidenceVerifier, expected: str, label: str) -> None:
    actual = verifier.overall_status()
    if actual != expected:
        details = [f"{check.name}: {check.status}: {check.detail}" for check in verifier.checks]
        raise AssertionError(f"{label} expected {expected}, got {actual}\n" + "\n".join(details))


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    template = repo_root / "docs/validation/midori-nature-engine-profile-notes.template.md"

    with tempfile.TemporaryDirectory(prefix="midori_profile_notes_") as temp:
        temp_dir = Path(temp)
        missing_verifier = run_check(temp_dir / "absent.md")
        require_status(missing_verifier, "pending", "absent notes")

        copied_template = temp_dir / "copied-template.md"
        copied_template.write_text(template.read_text(encoding="utf-8"), encoding="utf-8")
        template_verifier = run_check(copied_template)
        require_status(template_verifier, "failed", "copied template")
        if not any(check.name == "profile.notes.unresolved_markers" for check in template_verifier.checks):
            raise AssertionError("copied template did not exercise unresolved marker detection")

        thin_notes = temp_dir / "thin.md"
        thin_notes.write_text("Unity Unreal Frame Debugger RenderDoc instanced scale density cull\n", encoding="utf-8")
        thin_verifier = run_check(thin_notes)
        require_status(thin_verifier, "failed", "thin notes")

        good_notes = temp_dir / "good.md"
        good_notes.write_text(GOOD_NOTES, encoding="utf-8")
        good_verifier = run_check(good_notes)
        require_status(good_verifier, "passed", "completed notes")

    print("profile notes verifier self-test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
