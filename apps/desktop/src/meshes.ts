/**
 * Convert engine LOD output into stack viewport descriptors.
 *
 * One MeshDescriptor per material range (bark/leaves/impostor); all
 * descriptors of a LOD share the same position/normal/uv buffers and differ
 * only by their index slice, so no vertex data is duplicated.
 *
 * When the species' generated maps are present the descriptors carry real
 * materials: `uvs` + RGBA8 `map` (bark albedo, leaf card, baked impostor
 * atlas) with `alphaTest` cutout on leaves/impostor — matching what the
 * glTF export embeds. Without maps the viewport falls back to flat
 * per-material colors.
 *
 * Bark V coordinates are metres along the stem (repeating once per metre
 * via `texture_v_scale`), so bark binds `mapWrap: 'repeat'`. Bound maps get
 * `mapFilter: 'linear'` — trilinear filtering keeps minified bark and the
 * far-LOD impostor atlas from shimmering.
 */

import type { MeshDescriptor, TextureSource } from '@jethac/tools-frontend-stack/viewports';
import type { LodMesh, MaterialKind } from './engine';

export const MATERIAL_COLORS: Record<MaterialKind, string> = {
  bark: '#6d4c2f',
  leaves: '#3f7a2e',
  impostor: '#5a8f46',
};

/** Alpha cutoff matching the exporter's glTF MASK materials (alphaCutoff). */
export const CUTOUT_ALPHA = 0.5;

/** Material maps the preview can bind, keyed by descriptor material. */
export interface PreviewMaps {
  bark?: TextureSource;
  leaves?: TextureSource;
  impostor?: TextureSource;
}

/** Impostor quads are baked foliage — they ride the leaves layer toggle. */
function layerFor(material: MaterialKind): 'bark' | 'leaves' {
  return material === 'bark' ? 'bark' : 'leaves';
}

export function lodToDescriptors(
  lod: LodMesh,
  revision: number,
  layers: { bark: boolean; leaves: boolean },
  maps?: PreviewMaps,
): MeshDescriptor[] {
  const positions = Float32Array.from(lod.vertices.positions);
  const normals = Float32Array.from(lod.vertices.normals);
  const indices = Uint32Array.from(lod.indices);
  const uvs = Float32Array.from(lod.vertices.uvs);

  const ranges =
    lod.submeshes.length > 0
      ? lod.submeshes
      : [{ index_start: 0, index_count: indices.length, material: 'bark' as const }];

  const descriptors: MeshDescriptor[] = [];
  for (const [i, range] of ranges.entries()) {
    if (!layers[layerFor(range.material)]) continue;
    const map = maps?.[range.material];
    const descriptor: MeshDescriptor = {
      entityId: `${lod.name}-${range.material}-${i}`,
      revision,
      positions,
      normals,
      indices: indices.slice(range.index_start, range.index_start + range.index_count),
      // A bound map multiplies with `color`; white keeps the texture honest,
      // the flat palette stays for the untextured fallback.
      color: map ? '#ffffff' : MATERIAL_COLORS[range.material],
    };
    if (map) {
      descriptor.uvs = uvs;
      descriptor.map = map;
      descriptor.mapFilter = 'linear';
      if (range.material === 'bark') {
        // Bark V runs in metres along the stem — the map tiles vertically.
        descriptor.mapWrap = 'repeat';
      } else {
        // Leaves/impostor are cutout (matches glTF MASK).
        descriptor.alphaTest = CUTOUT_ALPHA;
      }
    }
    descriptors.push(descriptor);
  }
  return descriptors;
}
