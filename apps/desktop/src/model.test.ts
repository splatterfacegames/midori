// @vitest-environment node
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, beforeAll } from 'vitest';
import { MidoriModel } from './model';
import { PRESETS } from './presets';
import { NATURE_PRESETS } from './naturePresets';
import { generateNaturePreview } from './engine';
import { getParam, setParam } from './species';
import type { SpeciesJson } from './species';

const wasmPath = fileURLToPath(new URL('./wasm/midori_wasm_bg.wasm', import.meta.url));
const wasmBytes = () => readFileSync(wasmPath);

describe('MidoriModel', () => {
  let model: MidoriModel;

  beforeAll(async () => {
    model = await MidoriModel.create(wasmBytes());
  });

  it('loads the oak preset and generates LOD meshes', () => {
    const state = model.getState();
    expect(state.json?.species.name).toBe('Oak');
    expect(state.lods?.length).toBeGreaterThanOrEqual(2);
    expect(state.stats?.stem_count).toBeGreaterThan(0);
    expect(state.stats?.leaf_count).toBeGreaterThan(0);
    for (const lod of state.lods ?? []) {
      expect(lod.triangle_count).toBeGreaterThan(0);
      expect(lod.submeshes.length).toBeGreaterThan(0);
    }
  });

  it('generates deterministic material maps for the loaded species', () => {
    const maps = model.getState().maps;
    expect(maps).not.toBeNull();
    for (const map of [maps!.bark_albedo, maps!.bark_normal, maps!.leaf_card]) {
      // PNG encoding for the materials strip / file inspection.
      expect(map.png).toBeInstanceOf(Uint8Array);
      expect([...map.png!.slice(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47]);
      // Raw RGBA8 for direct viewport texture upload — the same bake.
      const rgba = map.rgba;
      expect(rgba.width).toBeGreaterThan(0);
      expect(rgba.height).toBeGreaterThan(0);
      expect(rgba.data).toBeInstanceOf(Uint8Array);
      expect(rgba.data.length).toBe(rgba.width * rgba.height * 4);
    }
    // Map resolution follows the species' [textures] params.
    const oak = model.getState().json;
    expect(oak?.textures.resolution).toBe(maps!.bark_albedo.rgba.width);
  });

  it('bakes a crown impostor on the last balanced LOD', () => {
    const lods = model.getState().lods ?? [];
    const last = lods[lods.length - 1];
    expect(last.name).toBe('Low');
    const impostor = last.submeshes.find((sub) => sub.material === 'impostor');
    expect(impostor).toBeDefined();
    expect(impostor!.index_count).toBeGreaterThan(0);
    expect(last.impostor_atlas).toBeDefined();
    // PNG encoding for the materials strip.
    expect([...last.impostor_atlas!.png!.slice(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47]);
    // Raw RGBA8 copy for direct viewport upload: 2:1 front|side atlas.
    const atlas = last.impostor_atlas!.rgba;
    expect(atlas.width).toBe(atlas.height * 2);
    expect(atlas.data.length).toBe(atlas.width * atlas.height * 4);
    expect(atlas.data.some((b, i) => i % 4 === 3 && b > 0)).toBe(true);
  });

  it('is deterministic for the same seed', () => {
    const first = model.getState().lods;
    model.setSeed(7);
    const a = model.getState().lods;
    model.setSeed(7);
    const b = model.getState().lods;
    expect(a?.length).toBe(b?.length);
    expect(a?.[0]?.triangle_count).toBe(b?.[0]?.triangle_count);
    expect(first).not.toBeNull();
  });

  it('edits a parameter and keeps TOML/JSON in sync', () => {
    model.loadPreset('pine');
    const before = model.getState().json?.trunk.height ?? 0;
    model.updateParam('trunk.height', before + 2);
    const state = model.getState();
    expect(state.json?.trunk.height).toBe(before + 2);
    expect(state.toml).toContain(`height = ${before + 2}`);
    expect(state.paramError).toBeNull();
  });

  it('round-trips species JSON through the engine', () => {
    const json = model.getState().json;
    expect(json).not.toBeNull();
    model.updateParam('species.name', 'Test Pine');
    expect(model.getState().json?.species.name).toBe('Test Pine');
    expect(model.getState().toml).toContain('name = "Test Pine"');
  });

  it('applies and rejects TOML source edits', () => {
    const state = model.getState();
    model.editSource('[species]\nname = "Broken"\n[trunk]\nheight = 5.0\nradius = 0.3\n');
    expect(model.getState().sourceDirty).toBe(true);
    expect(model.applySource()).toBe(true);
    expect(model.getState().json?.species.name).toBe('Broken');
    expect(model.getState().sourceDirty).toBe(false);

    model.editSource('this is not toml at all');
    expect(model.applySource()).toBe(false);
    expect(model.getState().sourceError).toBeTruthy();
    // Failed apply keeps the last good document live.
    expect(model.getState().json?.species.name).toBe('Broken');
    model.revertSource();
    expect(model.getState().sourceDirty).toBe(false);
  });

  it('toggles branch levels off and back on', () => {
    model.loadPreset('oak');
    model.setBranchLevel('level3', false);
    expect(model.getState().json?.branches.level3).toBeUndefined();
    model.setBranchLevel('level3', true);
    expect(model.getState().json?.branches.level3).toBeDefined();
  });

  it('exports GLB and glTF parts', () => {
    const glb = model.exportFiles(3, 'glb', 'oak');
    expect(glb).toHaveLength(1);
    expect(glb[0].name).toBe('oak.glb');
    // GLB magic
    expect([...glb[0].data.slice(0, 4)]).toEqual([0x67, 0x6c, 0x54, 0x46]);

    const parts = model.exportFiles(3, 'gltf', 'oak');
    expect(parts.map((f) => f.name).sort()).toEqual(['oak.bin', 'oak.gltf']);
    const gltf = JSON.parse(new TextDecoder().decode(parts.find((f) => f.name === 'oak.gltf')!.data));
    expect(gltf.buffers[0].uri).toBe('oak.bin');
    expect(gltf.meshes.length).toBeGreaterThan(0);
  });

  it('embeds material maps only when requested', () => {
    const textured = model.exportFiles(3, 'gltf', 'oak', true);
    const gltf = JSON.parse(
      new TextDecoder().decode(textured.find((f) => f.name === 'oak.gltf')!.data),
    );
    expect(gltf.images.length).toBeGreaterThanOrEqual(3); // bark albedo + normal, leaf card (+impostor atlas)
    expect(gltf.textures.length).toBe(gltf.images.length);
    const materials = gltf.materials.map((m: { name?: string }) => m.name);
    expect(materials).toContain('bark');
    expect(materials).toContain('leaves');
    // Textured impostor materials are named per-LOD ("impostor_Low").
    expect(materials.some((n: string) => n.startsWith('impostor'))).toBe(true);
    // Leaf + impostor primitives must not share the bark material.
    const leafPrimitive = gltf.meshes
      .flatMap((m: { primitives: unknown[] }) => m.primitives)
      .find((p: { material: number }) => gltf.materials[p.material].name === 'leaves');
    expect(leafPrimitive).toBeDefined();

    const plain = model.exportFiles(3, 'gltf', 'oak', false);
    const plainGltf = JSON.parse(
      new TextDecoder().decode(plain.find((f) => f.name === 'oak.gltf')!.data),
    );
    // The impostor atlas embeds regardless — it is the impostor's content.
    expect(plainGltf.images?.map((i: { name: string }) => i.name)).toEqual(['impostor_lod2']);
    expect(plainGltf.materials.length).toBe(3);
    const plainNames = plainGltf.materials.map((m: { name?: string }) => m.name);
    expect(plainNames.slice(0, 2)).toEqual(['bark', 'leaves']);
    expect(plainNames[2]).toMatch(/^impostor_/);
  });

  it('generates a nature preview for a bundled NaturePatch', () => {
    const preview = generateNaturePreview(NATURE_PRESETS[0].toml) as {
      stats: { prototype_count: number };
      terrain: { indices: number[] };
    };
    expect(preview.stats.prototype_count).toBeGreaterThan(0);
    expect(preview.terrain.indices.length).toBeGreaterThan(0);
  });
});

describe('species param helpers', () => {
  it('getParam resolves dotted paths', () => {
    const species = { trunk: { height: 6 }, branches: { level1: { angle: 55 } } } as unknown as SpeciesJson;
    expect(getParam(species, 'trunk.height')).toBe(6);
    expect(getParam(species, 'branches.level1.angle')).toBe(55);
    expect(getParam(species, 'branches.level9.count')).toBeUndefined();
  });

  it('setParam returns a new object without mutating the input', () => {
    const species = { trunk: { height: 6 } } as unknown as SpeciesJson;
    const next = setParam(species, 'trunk.height', 9);
    expect(species.trunk.height).toBe(6);
    expect(next.trunk.height).toBe(9);
  });
});
