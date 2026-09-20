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

/* ── Nature patch preview ────────────────────────────────────────────────
 *
 * Terrain becomes one descriptor; each scatter_set's prototype contributes
 * up to SCATTER_SAMPLE_LIMIT per-kind descriptors (one per sampled instance,
 * placed via `transform`). Viewport3D has no instanced drawing yet — the
 * sample keeps the frame interactive at real densities.
 */

import type { NaturePreview, NatureMesh, ScatterInstance } from './engine';

/** Per-set cap on scatter instances pushed into the viewport. */
export const SCATTER_SAMPLE_LIMIT = 240;

const KIND_COLORS: Record<string, string> = {
  grass: '#4a7d34',
  forb: '#7d9a3a',
  shrub: '#3c6b33',
  rock: '#8a8578',
  log: '#6b5233',
  litter: '#7a6a42',
};

function natureMeshDescriptor(
  mesh: NatureMesh,
  entityId: string,
  revision: number,
  color: string,
): MeshDescriptor {
  return {
    entityId,
    revision,
    positions: Float32Array.from(mesh.vertices.positions),
    normals: Float32Array.from(mesh.vertices.normals),
    indices: Uint32Array.from(mesh.indices),
    color,
  };
}

function natureKindColor(kind: string): string {
  return KIND_COLORS[kind] ?? '#5f7a45';
}

export function natureToDescriptors(
  preview: NaturePreview,
  revision: number,
): MeshDescriptor[] {
  const descriptors: MeshDescriptor[] = [
    natureMeshDescriptor(preview.terrain, 'nature-terrain', revision, '#4f6641'),
  ];

  for (const [setIndex, set] of preview.scatter_sets.entries()) {
    const prototype = preview.prototypes[set.layer_index] ??
      preview.prototypes.find((p) => p.name === set.layer_name || p.kind === set.kind);
    const geometry = prototype?.lods.at(-1) ?? prototype?.lods[0];
    if (!geometry) continue;

    const positions = Float32Array.from(geometry.vertices.positions);
    const normals = Float32Array.from(geometry.vertices.normals);
    const indices = Uint32Array.from(geometry.indices);

    let placed = 0;
    outer: for (const chunk of set.chunks) {
      for (const inst of chunk.instances as ScatterInstance[]) {
        if (placed >= SCATTER_SAMPLE_LIMIT) break outer;
        descriptors.push({
          entityId: `nature-scatter-${setIndex}-${placed}`,
          revision,
          positions,
          normals,
          indices,
          color: natureKindColor(set.kind),
          transform: {
            position: inst.position,
            rotation: [0, inst.yaw, 0],
            scale: [inst.width, inst.height, inst.width],
          },
        });
        placed += 1;
      }
    }
  }
  return descriptors;
}
