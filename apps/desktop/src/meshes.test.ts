// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { CUTOUT_ALPHA, lodToDescriptors, MATERIAL_COLORS } from './meshes';
import type { LodMesh, RgbaMap } from './engine';

const lod: LodMesh = {
  name: 'lod0',
  vertices: {
    positions: [0, 0, 0, 1, 0, 0, 0, 1, 0],
    normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
    uvs: [],
    uv2s: [],
    colors: [],
  },
  indices: [0, 1, 2],
  submeshes: [{ index_start: 0, index_count: 3, material: 'bark' }],
  vertex_count: 3,
  triangle_count: 1,
};

/** 2x2 solid-map stub: valid TextureSource shape, opaque pixels. */
function stubMap(fill = 200): RgbaMap {
  return { data: new Uint8Array(16).fill(fill), width: 2, height: 2 };
}

describe('lodToDescriptors', () => {
  it('splits submeshes into colored descriptors sharing vertex buffers', () => {
    const multi: LodMesh = {
      ...lod,
      indices: [0, 1, 2, 2, 1, 0],
      submeshes: [
        { index_start: 0, index_count: 3, material: 'bark' },
        { index_start: 3, index_count: 3, material: 'leaves' },
      ],
    };
    const out = lodToDescriptors(multi, 1, { bark: true, leaves: true });
    expect(out).toHaveLength(2);
    expect(out[0].color).toBe(MATERIAL_COLORS.bark);
    expect(out[1].color).toBe(MATERIAL_COLORS.leaves);
    expect(out[0].positions).toBe(out[1].positions); // shared buffer
    expect([...(out[1].indices ?? [])]).toEqual([2, 1, 0]);
  });

  it('honors layer visibility and falls back when submeshes are absent', () => {
    expect(lodToDescriptors(lod, 1, { bark: false, leaves: true })).toHaveLength(0);
    const bare = { ...lod, submeshes: [] };
    const out = lodToDescriptors(bare, 1, { bark: true, leaves: true });
    expect(out).toHaveLength(1);
    expect(out[0].indices?.length).toBe(3);
  });

  it('routes impostor submeshes through the leaves layer', () => {
    const withImpostor: LodMesh = {
      ...lod,
      indices: [0, 1, 2, 2, 1, 0],
      submeshes: [
        { index_start: 0, index_count: 3, material: 'bark' },
        { index_start: 3, index_count: 3, material: 'impostor' },
      ],
    };
    const both = lodToDescriptors(withImpostor, 1, { bark: true, leaves: true });
    expect(both).toHaveLength(2);
    expect(both[1].color).toBe(MATERIAL_COLORS.impostor);
    const barkOnly = lodToDescriptors(withImpostor, 1, { bark: true, leaves: false });
    expect(barkOnly).toHaveLength(1);
    expect(barkOnly[0].color).toBe(MATERIAL_COLORS.bark);
  });

  it('binds maps + alphaTest for leaves and the baked impostor atlas', () => {
    const atlas = stubMap(160);
    const textured: LodMesh = {
      ...lod,
      vertices: {
        positions: [0, 0, 0, 1, 0, 0, 0, 1, 0],
        normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
        uvs: [0, 0, 1, 0, 0.5, 1],
        uv2s: [],
        colors: [],
      },
      indices: [0, 1, 2, 0, 2, 1],
      submeshes: [
        { index_start: 0, index_count: 3, material: 'leaves' },
        { index_start: 3, index_count: 3, material: 'impostor' },
      ],
      impostor_atlas: { rgba: atlas },
    };
    const out = lodToDescriptors(textured, 1, { bark: true, leaves: true }, { leaves: stubMap(), impostor: atlas });
    expect(out).toHaveLength(2);
    expect(out[0].map?.data).toBeInstanceOf(Uint8Array);
    expect(out[0].alphaTest).toBe(CUTOUT_ALPHA);
    expect(out[0].mapFilter).toBe('linear');
    expect(out[0].mapWrap).toBeUndefined(); // cards clamp at the card edge
    expect(out[0].color).toBe('#ffffff');
    expect(out[0].uvs).toBeInstanceOf(Float32Array);
    // The impostor descriptor binds the per-LOD baked atlas.
    expect(out[1].map).toBe(atlas);
    expect(out[1].alphaTest).toBe(CUTOUT_ALPHA);
  });

  it('binds bark with repeat wrap, original UVs, and no alphaTest', () => {
    const bark = stubMap();
    // Bark verts spanning v 0..6 (a ~6m stem at texture_v_scale 1).
    const stems: LodMesh = {
      ...lod,
      vertices: {
        positions: [0, 0, 0, 1, 0, 0, 0, 1, 0],
        normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
        uvs: [0, 0, 0.5, 3.2, 1, 6.0],
        uv2s: [],
        colors: [],
      },
    };
    const out = lodToDescriptors(stems, 1, { bark: true, leaves: true }, { bark });
    expect(out).toHaveLength(1);
    const d = out[0];
    expect(d.map).toBe(bark); // original map, no baked tiling
    expect(d.mapWrap).toBe('repeat');
    expect(d.mapFilter).toBe('linear');
    expect(d.alphaTest).toBeUndefined(); // bark is opaque
    // UVs pass through unscaled — REPEAT sampling wraps them.
    const uv = [...(d.uvs ?? [])];
    expect(uv[3]).toBeCloseTo(3.2, 6);
    expect(uv[5]).toBeCloseTo(6.0, 6);
  });

  it('binds bark identically when V stays within one tile', () => {
    const bark = stubMap();
    const stems: LodMesh = {
      ...lod,
      vertices: {
        positions: [0, 0, 0, 1, 0, 0, 0, 1, 0],
        normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
        uvs: [0, 0, 0.5, 0.5, 1, 0.9],
        uv2s: [],
        colors: [],
      },
    };
    const out = lodToDescriptors(stems, 1, { bark: true, leaves: true }, { bark });
    expect(out[0].map).toBe(bark);
    expect(out[0].mapWrap).toBe('repeat');
    expect([...(out[0].uvs ?? [])][5]).toBeCloseTo(0.9, 6);
  });
});

import { natureToDescriptors } from './meshes';
import type { NatureMesh, NaturePreview, ScatterInstance } from './engine';

const natureMesh: NatureMesh = {
  name: 'proto',
  vertices: {
    positions: [0, 0, 0, 1, 0, 0, 0, 1, 0],
    normals: [0, 0, 1, 0, 0, 1, 0, 0, 1],
    uvs: [],
    uv2s: [],
    colors: [],
  },
  indices: [0, 1, 2],
  submeshes: [],
  vertex_count: 3,
  triangle_count: 1,
};

function inst(x: number): ScatterInstance {
  return {
    position: [x, 0, 0],
    yaw: 1.25,
    height: 0.4,
    width: 0.2,
    phase: 0,
    color_variation: 0.5,
  };
}

function naturePreview(instanceCount: number): NaturePreview {
  return {
    manifest: {},
    terrain: { ...natureMesh, name: 'terrain' },
    prototypes: [
      { name: 'grass_proto', kind: 'grass', lods: [natureMesh, { ...natureMesh, name: 'far' }] },
    ],
    scatter_sets: [
      {
        layer_index: 0,
        layer_name: 'grass_proto',
        kind: 'grass',
        chunks: [
          {
            chunk_x: 0,
            chunk_z: 0,
            bounds_min: [0, 0, 0],
            bounds_max: [8, 1, 8],
            instances: Array.from({ length: instanceCount }, (_, i) => inst(i)),
          },
        ],
      },
    ],
    stats: {
      tile_size: 8,
      terrain_vertex_count: 3,
      terrain_triangle_count: 1,
      prototype_count: 1,
      scatter_instance_count: instanceCount,
    },
  };
}

describe('natureToDescriptors', () => {
  it('emits terrain plus one instanced descriptor per scatter set', () => {
    const out = natureToDescriptors(naturePreview(3), 7);
    expect(out).toHaveLength(2);
    expect(out[0].entityId).toBe('nature-terrain');
    const placed = out[1];
    expect(placed.instances).toHaveLength(3);
    expect(placed.instances?.[1]).toEqual({
      position: [1, 0, 0],
      rotation: [0, 1.25, 0],
      scale: [0.2, 0.4, 0.2],
    });
    expect(placed.revision).toBe(7);
  });

  it('uses the last LOD of the resolved prototype and kind colors', () => {
    const out = natureToDescriptors(naturePreview(1), 0);
    expect(out[1].positions).toBeInstanceOf(Float32Array);
    expect(out[1].color).toBe('#4a7d34');
  });

  it('passes every scatter placement through the instance descriptor', () => {
    const out = natureToDescriptors(naturePreview(290), 0);
    expect(out).toHaveLength(2);
    expect(out[1].instances).toHaveLength(290);
  });

  it('skips scatter sets with no resolvable prototype', () => {
    const preview = naturePreview(2);
    preview.scatter_sets[0].layer_index = 99;
    preview.scatter_sets[0].layer_name = 'missing';
    preview.scatter_sets[0].kind = 'unknown_kind';
    const out = natureToDescriptors(preview, 0);
    expect(out).toHaveLength(1);
  });
});
