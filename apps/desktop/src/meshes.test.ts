// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { lodToDescriptors, MATERIAL_COLORS } from './meshes';
import type { LodMesh } from './engine';

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
});
