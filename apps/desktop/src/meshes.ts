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
  upper_bark: '#b8b0a2',
  leaves: '#3f7a2e',
  organ: '#d48b3b',
  impostor: '#5a8f46',
};

/** Alpha cutoff matching the exporter's glTF MASK materials (alphaCutoff). */
export const CUTOUT_ALPHA = 0.5;

/** Material maps the preview can bind, keyed by descriptor material. */
export interface PreviewMaps {
  bark?: TextureSource;
  barkNormal?: TextureSource;
  upper_bark?: TextureSource;
  upperBarkNormal?: TextureSource;
  leaves?: TextureSource;
  organ?: TextureSource;
  impostor?: TextureSource;
}

/** Impostor quads and flower/fruit organs ride the leaves layer toggle; upper_bark rides bark. */
function layerFor(material: MaterialKind): 'bark' | 'leaves' {
  return material === 'bark' || material === 'upper_bark' ? 'bark' : 'leaves';
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
      if (range.material === 'bark' || range.material === 'upper_bark') {
        // Bark V runs in metres along the stem — the map tiles vertically.
        descriptor.mapWrap = 'repeat';
        const nrm = range.material === 'upper_bark' ? maps?.upperBarkNormal : maps?.barkNormal;
        if (nrm) {
          descriptor.normalMap = nrm;
        }
      } else {
        // Leaves/organs/impostor are cutout (matches glTF MASK).
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
 * one instanced descriptor containing every chunked placement.
 */

import type { NaturePreview, NatureMesh, ScatterInstance } from './engine';

const KIND_COLORS: Record<string, string> = {
  grass: '#4a7d34',
  moss: '#487a2a',
  flower: '#d9d38c',
  weed: '#668f43',
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

function natureTerrainColor(manifest: unknown): string {
  if (manifest && typeof manifest === 'object') {
    const m = manifest as {
      biome?: string;
      soil_profile?: string;
      patch?: { biome?: string };
      soil?: { profile?: string };
    };
    const biome = (m.biome ?? m.patch?.biome ?? '').toLowerCase();
    const soil = (m.soil_profile ?? m.soil?.profile ?? '').toLowerCase();
    if (biome.includes('arid') || biome.includes('desert') || soil.includes('sand')) {
      return '#b59f7e';
    }
    if (biome.includes('forest')) {
      return '#475f39';
    }
    if (biome.includes('alpine') || biome.includes('meadow')) {
      return '#526d3d';
    }
  }
  return '#4f6641';
}

export function natureToDescriptors(
  preview: NaturePreview,
  revision: number,
): MeshDescriptor[] {
  const descriptors: MeshDescriptor[] = [
    natureMeshDescriptor(
      preview.terrain,
      'nature-terrain',
      revision,
      natureTerrainColor(preview.manifest),
    ),
  ];

  for (const [setIndex, set] of preview.scatter_sets.entries()) {
    const prototype = preview.prototypes[set.layer_index] ??
      preview.prototypes.find((p) => p.name === set.layer_name || p.kind === set.kind);
    const geometry = prototype?.lods[0] ?? prototype?.lods.at(-1);
    if (!geometry) continue;

    const positions = Float32Array.from(geometry.vertices.positions);
    const normals = Float32Array.from(geometry.vertices.normals);
    const indices = Uint32Array.from(geometry.indices);

    const instances = set.chunks.flatMap((chunk) => (chunk.instances as ScatterInstance[]).map((inst) => ({
      position: inst.position,
      rotation: [0, inst.yaw, 0] as const,
      scale: [inst.width, inst.height, inst.width] as const,
    })));
    const useAtlas = Boolean(prototype?.uses_card_atlas && preview.card_atlas);
    const descriptor: MeshDescriptor = {
      entityId: `nature-scatter-${setIndex}`,
      revision,
      positions,
      normals,
      indices,
      color: useAtlas
        ? '#ffffff'
        : (prototype?.color_tip ?? natureKindColor(set.kind)),
      instances,
    };
    if (useAtlas && preview.card_atlas) {
      descriptor.uvs = Float32Array.from(geometry.vertices.uvs);
      descriptor.map = preview.card_atlas.rgba;
      descriptor.mapFilter = 'linear';
      descriptor.alphaTest = CUTOUT_ALPHA;
    }
    descriptors.push(descriptor);
  }
  return descriptors;
}
