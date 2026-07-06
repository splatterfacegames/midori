# SeedThree Audit Notes

Source inspected: <https://github.com/SkyeShark/SeedThree> at commit `6c124c8a01551b93aa3e6c6b01dfa2ede706f019`.

SeedThree's code is MIT licensed. Its README separately warns that generated textures/audio come from third-party models and xeno-canto recordings, so Midori should borrow architecture and workflows before copying assets. If code is copied directly, retain the MIT notice.

## What SeedThree Does Well

- Treats the app as an end-to-end generator, not just a mesh function: species presets, texture derivation scripts, renderer, LOD preview, export, and documentation all reinforce one workflow.
- Keeps morphology explicit. `docs/morphology.md` records species-specific signatures like bark, branch posture, crown shape, and leaf geometry, then maps those traits to generator parameters.
- Uses multiple generator families. Weber-Penn handles broadleaf and conifer trees, while a dichotomous/L-system path handles Joshua tree, yucca, and cactus-like plants that do not fit a normal trunk/branch/leaf model.
- Bakes believable far LODs. The chain is full mesh, reduced mesh, branch cards, then a crossed-card billboard impostor with albedo, normal, roughness, and translucency channels.
- Designs export as a first-class path. It bakes live instancing into plain geometry, names LOD nodes predictably, and adds glTF extensions for LOD and foliage transmission.
- Uses the viewer as QA. The live scene includes lighting, wind, terrain, forest instances, LOD switches, stats, and export from the same assets the user edits.
- Documents the asset pipeline. Texture scripts derive alpha, normal, roughness, and translucency maps from generated albedo/card images with strict naming conventions.

## What Midori Already Has

- Rust core with deterministic seeded generation, TOML species presets, LOD mesh generation, Pivot Painter-style vertex data, and glTF export.
- Svelte/Three editor with WASM generation, LOD selection, wireframe preview, mesh stats, wind shader support, and GLB export.
- LOD configuration and adaptive branch ring resolution are already in the core, so we can evolve rather than replace them.

## Best Ideas To Crib

1. Make species presets richer.
   Add optional metadata for biome, latin name, bark/leaf material names, foliage mode, and generator family. Keep TOML as the authoring format, but stop making every species squeeze through the same branch vocabulary.

2. Split generator families by morphology.
   Keep the current Weber-Penn-style path for ordinary trees. Add a second generator trait for rosettes, succulents, and cacti: ribbed swept tubes, stochastic fork grammar, terminal rosettes, areole/spine placement, and pad/segment chains.

3. Finish impostor LODs.
   Midori has `crown_impostor` flags but no real billboard bake. SeedThree's biggest practical win is a real far-LOD bake. The Rust core can export metadata, while the web editor can bake texture impostors with Three before export.

4. Upgrade glTF material and LOD export.
   Midori currently exports one primitive even when submeshes exist. Fix material splitting first, then add `_LOD0` naming plus a machine-readable LOD extension. Leaf transmission should use standard glTF material extensions where practical.

5. Add an asset-generation contract.
   Use SeedThree's naming discipline: `<asset>_albedo.png`, `<asset>_normal.png`, `<asset>_roughness.png`, and leaf/card `_translucency.png`. Add scripts later for alpha dilation and derived PBR maps.

6. Improve branch junctions and normals.
   Midori already has collar swelling. Next steps are rotation-minimizing frames, smoother normal averaging across coincident seam/junction vertices, pipe-model child radius, base flare, lobes/ribs, and explicit vertical tropism.

7. Make the editor a validation harness.
   Add scale reference, sun controls, wind controls, LOD-distance preview, export size/stats, and a small instanced forest ring. These are not decorative; they reveal whether LODs, wind, and export behave under real viewing conditions.

8. Keep deterministic generation sacred.
   Thread one seeded RNG through generation in a stable traversal order. Avoid frontend `Math.random()` for geometry decisions; use it only for UI conveniences like random seed selection.

## Suggested Priority

- P0: Keep the Midori rename consistent across crates, CLI, WASM, UI, docs, lockfiles, and generated wasm artifacts.
- P1: Fix glTF submesh material splitting and add better LOD node naming.
- P2: Implement the missing `crown_impostor` path as a real billboard/card LOD.
- P3: Expand species schema with generator family and material metadata.
- P4: Add morphology notes and texture-generation scripts before adding more species.
- P5: Prototype a second generator family for Joshua tree or saguaro, because that forces the architecture to handle non-tree plants honestly.
