# Midori Session Handoff

Date: 2026-07-07

This note is the cold-start handoff for picking up the Midori nature/mobile-console work in a new session. Treat the files on disk as authoritative, but this document names the current state, the important entry points, and the exact remaining blocker.

## Current Repository State

- Branch: `master`
- Working tree at handoff time: clean
- Recent semantic commits:
  - `af1cb46 docs: document Midori mission and source audits`
  - `547a6a0 chore: ignore Python cache files`
  - `c695bfa feat(web): add Midori nature previewer`
  - `c778e59 feat(engine): add Unity and Unreal nature validation handoff`
  - `62f4e53 feat(core)!: rebrand Grove crates and add nature packages`
- The old Grove crate names were replaced by Midori crate names.
- Generated outputs remain ignored under `target/`, `web/build/`, `web/node_modules/`, `web/target/`, and Python `__pycache__/`.

## Goal Status

The active product goal is documented in [`docs/midori-nature-mobile-console-goal.md`](midori-nature-mobile-console-goal.md):

- expand Midori from a tree generator into a deterministic nature asset generator
- target mobile and console game production constraints
- include soil, grass, moss, low vegetation, wind metadata, static material recipes, static engine import recipes, and Unity/Unreal import validation
- keep shader injection preview-only
- keep the texture/PBR asset pipeline parked

Current status: all editorless implementation and validation gates are done. Phase 7 is externally blocked on real editor evidence.

The latest evidence state is:

- status: `pending`
- passed checks: `1610`
- missing checks: `7`
- failed checks: `0`

The seven missing artifacts are:

- `target/midori_engine_validation/forest_floor_unity_import_report.json`
- `target/midori_engine_validation/forest_floor_unreal_editor_report.json`
- `docs/validation/screenshots/unity_forest_floor_import.png`
- `docs/validation/screenshots/unity_forest_floor_density.png`
- `docs/validation/screenshots/unreal_forest_floor_import.png`
- `docs/validation/screenshots/unreal_forest_floor_foliage_settings.png`
- `docs/validation/midori-nature-engine-profile-notes.md`

## Local Editor Blockers

The local machine has Unity Hub editors installed, but the Unity batch run is blocked by licensing:

- detected Unity versions: `6000.3.12f1`, `6000.4.6f1`, `6000.4.9f1`
- selected Unity version in the latest validation run: `6000.4.9f1`
- Unity status: `blocked_unity_license`
- Unity import exit code: `198`

Unreal Editor is not installed or discoverable locally:

- `UnrealEditor.exe` was not found on `PATH`
- no editor was found under `C:\Program Files\Epic Games`
- no editor was found under `C:\Program Files\Unreal Engine`
- Unreal status: `blocked_unreal_editor_not_found`

Do not fabricate the missing reports, screenshots, or profile notes. The strict evidence verifier is intentionally expected to fail until real editor runs provide them.

## Important Entry Points

Core implementation:

- [`crates/midori-core/src/nature.rs`](../crates/midori-core/src/nature.rs): `NaturePatch`, terrain fields, maps, prototypes, scatter, package writer, package validation, and most nature tests.
- [`crates/midori-cli/src/main.rs`](../crates/midori-cli/src/main.rs): `midori nature` and `midori validate-nature` CLI paths.
- [`crates/midori-wasm/src/lib.rs`](../crates/midori-wasm/src/lib.rs): web-facing generation bindings.

Representative presets:

- [`presets/nature/temperate_forest_floor.toml`](../presets/nature/temperate_forest_floor.toml)
- [`presets/nature/flowering_meadow.toml`](../presets/nature/flowering_meadow.toml)
- [`presets/nature/arid_scrub.toml`](../presets/nature/arid_scrub.toml)
- [`presets/species/joshua_prototype.toml`](../presets/species/joshua_prototype.toml)

Engine integration and evidence:

- [`integrations/unity/Editor/MidoriNaturePackageImporter.cs`](../integrations/unity/Editor/MidoriNaturePackageImporter.cs)
- [`integrations/unreal/midori_nature_importer.py`](../integrations/unreal/midori_nature_importer.py)
- [`scripts/validate_engine_imports.ps1`](../scripts/validate_engine_imports.ps1)
- [`scripts/verify_engine_evidence.py`](../scripts/verify_engine_evidence.py)
- [`scripts/export_engine_validation_handoff.ps1`](../scripts/export_engine_validation_handoff.ps1)
- [`scripts/import_engine_validation_handoff.ps1`](../scripts/import_engine_validation_handoff.ps1)
- [`docs/validation/midori-nature-unity-unreal.md`](validation/midori-nature-unity-unreal.md)
- [`docs/validation/midori-nature-engine-profile-notes.template.md`](validation/midori-nature-engine-profile-notes.template.md)

Web/editor:

- [`web/src/lib/stores/tree.ts`](../web/src/lib/stores/tree.ts)
- [`web/src/lib/components/Preview3D.svelte`](../web/src/lib/components/Preview3D.svelte)
- [`web/src/lib/components/Inspector.svelte`](../web/src/lib/components/Inspector.svelte)
- [`web/src/lib/midori/wasm.ts`](../web/src/lib/midori/wasm.ts)
- [`web/scripts/browser-smoke.mjs`](../web/scripts/browser-smoke.mjs)

Planning and audits:

- [`docs/seedthree-audit.md`](seedthree-audit.md)
- [`docs/grasssystemthreejs-audit.md`](grasssystemthreejs-audit.md)
- [`docs/midori-goal-plan.md`](midori-goal-plan.md)
- [`docs/architecture.md`](architecture.md)

## Commands To Re-establish State

Run these from the repo root unless noted.

Core checks:

```bash
cargo fmt --check
cargo test
```

Web build:

```bash
cd web
npm run build
```

Regenerate local engine validation outputs and current summary:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/validate_engine_imports.ps1 -SkipUnrealEditor
```

Use `-SkipUnrealEditor` on this host because no Unreal Editor is installed. Without that flag, the script still records the Unreal editor blocker; the dry-run path remains covered.

Refresh the allow-pending evidence report:

```bash
python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation --allow-pending --output target/midori_engine_validation/engine_evidence_verification.json
```

Strict verifier, expected to fail until real editor evidence exists:

```bash
python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation
```

The expected strict result right now is exit code `1`, with `1610` passing checks, `7` missing checks, and `0` failed checks.

## How To Finish Phase 7

Export a frozen bundle from this repo:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/export_engine_validation_handoff.ps1
```

Move `target/midori_engine_validation_handoff/` to a machine that has:

- licensed Unity Editor
- installed Unreal Editor
- permission to run batch/editor Python scripts

From inside the handoff bundle on that editor machine, run:

```powershell
.\run_editor_validation.ps1 -UnityExe "C:\Program Files\Unity\Hub\Editor\<version>\Editor\Unity.exe" -UnrealEditorExe "C:\Program Files\Epic Games\UE_<version>\Engine\Binaries\Win64\UnrealEditor.exe" -AllowPending
```

The first editor run uses `-AllowPending` because profile notes are normally written after screenshots and reports exist.

After the editor run:

1. Inspect `validation_root/editor_handoff_summary.json`.
2. Confirm Unity and Unreal sections are real editor results, not `verify_only`.
3. Confirm the two editor reports exist in `validation_root/`.
4. Confirm all four screenshots exist under `docs/validation/screenshots/`.
5. Create `docs/validation/midori-nature-engine-profile-notes.md` from the template.
6. Include exact report filenames, screenshot filenames, Unity and Unreal editor versions, mobile and console observations, Frame Debugger and RenderDoc instancing evidence, Unity detail-prototype/fallback fields, Unreal foliage type/cull fields, material slot observations, wind-channel observations, and strict-verifier result.
7. Rerun:

```powershell
.\run_editor_validation.ps1 -VerifyOnly
```

The handoff runner preserves previous real editor summary sections during final verify-only runs, so report/screenshot metadata should survive after profile notes are written.

Return the completed bundle to this repo and ingest it:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/import_engine_validation_handoff.ps1 -HandoffDir "target/midori_engine_validation_handoff"
```

Then run:

```bash
python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation
```

Phase 7 is complete only when that strict verifier exits `0`.

## Known Warnings

`npm run build` currently passes but reports existing warnings:

- `OutputNode.svelte` has an unused exported `id` property.
- SvelteKit dependency export warnings appear from the installed Svelte/SvelteKit package combination.
- One client chunk is larger than 500 kB after minification.

These warnings were present when the current commits were made and do not block the current handoff.

## Commit/Working Tree Guidance

If continuing from this handoff:

- Check `git status --short` first.
- Do not commit generated `target/`, `web/build/`, `web/node_modules/`, `web/target/`, or `__pycache__/` files.
- Commit returned real editor evidence only after `scripts/import_engine_validation_handoff.ps1` accepts it.
- Use semantic commits; the recent stack uses `feat(core)!`, `feat(engine)`, `feat(web)`, `chore`, and `docs`.

## Completion Criteria

Do not mark the goal complete unless all of these are true:

- `cargo fmt --check` passes.
- `cargo test` passes.
- `npm run build` passes.
- `python scripts/verify_engine_evidence.py --validation-root target/midori_engine_validation` exits `0`.
- The seven editor-only artifacts named above exist and are verified.
- `docs/validation/midori-nature-engine-profile-notes.md` contains real profile observations, not template placeholders.
