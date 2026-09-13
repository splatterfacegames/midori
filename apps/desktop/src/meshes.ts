/**
 * Convert engine LOD output into stack viewport descriptors.
 *
 * One MeshDescriptor per material range (bark/leaves/impostor); all
 * descriptors of a LOD share the same position/normal buffers and differ
 * only by their index slice, so no vertex data is duplicated.
 *
 * When the species' generated maps are present the descriptors carry real
 * materials: `uvs` + RGBA8 `map` (bark albedo, leaf card, baked impostor
 * atlas) with `alphaTest` cutout on leaves/impostor — matching what the
 * glTF export embeds. Without maps the viewport falls back to flat
 * per-material colors.
 *
 * Bark is the one transform: its V coordinate is metres along the stem
 * (repeats per metre via `texture_v_scale`), but stack DataTextures clamp
 * to edge, so `barkTexture` bakes K vertical copies into the map and
 * divides V by K — identical to REPEAT sampling without a wrap mode.
 * TODO(stack): drop the pre-tile when MeshDescriptor grows a wrap field.
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

/** Cap on baked bark repeats — bounds the pre-tiled map's memory. */
const MAX_BARK_TILES = 8;

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

/**
 * Bake `tiles` vertical copies of `map` and return it with UVs remapped
 * (v /= tiles) so clamped sampling reproduces REPEAT over [0, tiles].
 */
function tileBarkMap(
  map: TextureSource,
  uvs: Float32Array,
  tiles: number,
): { map: TextureSource; uvs: Float32Array } {
  if (tiles <= 1) return { map, uvs };
  const data = new Uint8Array(map.data.length * tiles);
  for (let k = 0; k < tiles; k++) data.set(map.data, k * map.data.length);
  const scaled = new Float32Array(uvs.length);
  for (let i = 0; i < uvs.length; i += 2) {
    scaled[i] = uvs[i];
    scaled[i + 1] = uvs[i + 1] / tiles;
  }
  return { map: { data, width: map.width, height: map.height * tiles }, uvs: scaled };
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

  // Bark V runs to the stem's length in metres; count repeats over the
  // bark-referenced vertices only.
  let barkMaxV = 0;
  if (maps?.bark) {
    for (const range of ranges) {
      if (range.material !== 'bark') continue;
      for (let i = range.index_start; i < range.index_start + range.index_count; i++) {
        const v = uvs[indices[i] * 2 + 1];
        if (v > barkMaxV) barkMaxV = v;
      }
    }
  }
  const barkTiles = Math.min(Math.max(1, Math.ceil(barkMaxV)), MAX_BARK_TILES);

  const descriptors: MeshDescriptor[] = [];
  for (const [i, range] of ranges.entries()) {
    if (!layers[layerFor(range.material)]) continue;
    const map = maps?.[range.material];
    const tiled =
      range.material === 'bark' && map ? tileBarkMap(map, uvs, barkTiles) : null;
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
      descriptor.uvs = tiled?.uvs ?? uvs;
      descriptor.map = tiled?.map ?? map;
      // Bark is opaque; leaves/impostor are cutout (matches glTF MASK).
      if (range.material !== 'bark') descriptor.alphaTest = CUTOUT_ALPHA;
    }
    descriptors.push(descriptor);
  }
  return descriptors;
}
