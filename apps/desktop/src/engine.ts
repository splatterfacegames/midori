/**
 * Engine boundary for the Midori workbench.
 *
 * The midori-wasm WebAssembly build is the same midori-core code that powers the
 * CLI and FFI crates, compiled for the webview. Species documents cross the
 * boundary as TOML text (authoritative) or as the serde JSON projection
 * produced by `toJson()` / consumed by `fromJson()`.
 */

import init, {
  MidoriGenerator,
  generate_nature_preview_from_toml,
  generate_stand_preview_from_toml,
} from './wasm/midori_wasm.js';

export interface VertexData {
  positions: number[];
  normals: number[];
  uvs: number[];
  uv2s: number[];
  colors: number[];
}

export type MaterialKind = 'bark' | 'upper_bark' | 'leaves' | 'organ' | 'impostor';

/** Raw RGBA8 pixels, row-major with the top row first (v = 0 at the top —
 *  the glTF/`TextureSource` UV origin). */
export interface RgbaMap {
  data: Uint8Array;
  width: number;
  height: number;
}

/** A generated image in both encodings: `png` for blob URLs and file
 *  inspection, `rgba` for direct viewport texture upload. `png` is absent
 *  only when the engine's PNG encode failed (the impostor atlas tolerates
 *  it; `generateMaps` errors instead). */
export interface GeneratedMap {
  png?: Uint8Array;
  rgba: RgbaMap;
}

export interface SubmeshRange {
  index_start: number;
  index_count: number;
  material: MaterialKind;
}

export interface LodMesh {
  name: string;
  vertices: VertexData;
  indices: number[];
  submeshes: SubmeshRange[];
  /** Baked crown-impostor atlas (front | side views side by side), present
   *  when this LOD level uses `crown_impostor`. */
  impostor_atlas?: GeneratedMap;
  vertex_count: number;
  triangle_count: number;
}

/** Species material maps, each in both encodings — the wasm `generateMaps`
 *  bakes the `TextureSet` once and returns `png` + `rgba` for all three. */
export interface MaterialMaps {
  bark_albedo: GeneratedMap;
  bark_normal: GeneratedMap;
  leaf_card: GeneratedMap;
  upper_bark_albedo?: GeneratedMap;
  upper_bark_normal?: GeneratedMap;
  organ_albedo?: GeneratedMap;
}

export interface TreeStats {
  stem_count: number;
  leaf_count: number;
  bounds_min: [number, number, number];
  bounds_max: [number, number, number];
}

export interface GltfParts {
  gltf: Uint8Array;
  bin: Uint8Array;
}

/** serde-wasm-bindgen crosses `Vec<u8>` fields as plain number arrays, not
 *  Uint8Array — normalize at the boundary so Blobs and TextureSource data
 *  get real typed arrays. */
function asBytes(data: Uint8Array | number[]): Uint8Array {
  return data instanceof Uint8Array ? data : Uint8Array.from(data);
}

/** Normalize a `GeneratedMap` that crossed the boundary as plain arrays. */
function normalizeMap(map: GeneratedMap): void {
  if (map.png) map.png = asBytes(map.png);
  map.rgba.data = asBytes(map.rgba.data);
}

let moduleReady: Promise<unknown> | null = null;

type WasmInput = Parameters<typeof init>[0];

/**
 * Initialize the WASM module once. `input` overrides where the module bytes
 * come from (tests pass a Buffer; the browser resolves `midori_wasm_bg.wasm`
 * next to the bundle via import.meta.url).
 */
export function loadEngine(input?: WasmInput): Promise<unknown> {
  moduleReady ??= Promise.resolve(init(input ? { module_or_path: input } : undefined));
  return moduleReady;
}

/**
 * A live generator bound to one species document. Recreate it whenever the
 * species changes; seeds are cheap and per-call.
 */
export class Generator {
  private constructor(private readonly inner: MidoriGenerator) {}

  static fromToml(toml: string): Generator {
    return new Generator(new MidoriGenerator(toml));
  }

  static fromTomlWithState(toml: string, state: string | null): Generator {
    return new Generator(MidoriGenerator.fromTomlWithState(toml, state ?? undefined));
  }

  static fromJson(json: unknown): Generator {
    return new Generator(MidoriGenerator.fromJson(json));
  }

  stateNames(): string[] {
    const names = this.inner.stateNames();
    return Array.isArray(names) ? (names as string[]) : [];
  }

  get name(): string {
    return this.inner.name;
  }

  /** Species as a mutable plain object (full serde projection). */
  toJson(): unknown {
    return this.inner.toJson();
  }

  /** Species serialized back to TOML. */
  toToml(): string {
    return this.inner.toToml();
  }

  /** Generate all LOD levels for `seed`. */
  generate(seed: number, opaque = false): LodMesh[] {
    const out = (
      opaque ? this.inner.generateOpaque(BigInt(seed)) : this.inner.generate(BigInt(seed))
    ) as { lods: LodMesh[] };
    for (const lod of out.lods) {
      if (lod.impostor_atlas) normalizeMap(lod.impostor_atlas);
    }
    return out.lods;
  }

  /** Stem/leaf/bounds stats without mesh output. */
  stats(seed: number): TreeStats {
    return this.inner.get_stats(BigInt(seed)) as TreeStats;
  }

  /** Worst-case stem/leaf counts — call before generating to refuse
   *  over-budget species cheaply. Throws a structured `Error` with `kind`
   *  when the estimate exceeds the engine budget. */
  estimate(): { max_stems: number; max_leaves: number } {
    return this.inner.estimateGeneration() as {
      max_stems: number;
      max_leaves: number;
    };
  }

  /** Single-file binary glTF for `seed`. `embedTextures` embeds the species'
   *  generated material maps (bark albedo+normal, leaf card). Impostor
   *  atlases are embedded whenever a LOD bakes one. */
  exportGlb(seed: number, embedTextures: boolean, opaque = false): Uint8Array {
    return opaque
      ? this.inner.exportGlbOpaque(BigInt(seed), embedTextures)
      : this.inner.export_glb(BigInt(seed), embedTextures);
  }

  /** Separate .gltf JSON + .bin parts. `binName` becomes the buffer URI;
   *  embedded images ride inside the .bin via bufferView references. */
  exportGltf(seed: number, binName: string, embedTextures: boolean, opaque = false): GltfParts {
    return (
      opaque
        ? this.inner.exportGltfOpaque(BigInt(seed), binName, embedTextures)
        : this.inner.exportGltf(BigInt(seed), binName, embedTextures)
    ) as GltfParts;
  }

  /** The species' procedural material maps — `png` + `rgba` encodings from
   *  a single bake (deterministic). */
  generateMaps(): MaterialMaps {
    const maps = this.inner.generateMaps() as MaterialMaps;
    normalizeMap(maps.bark_albedo);
    normalizeMap(maps.bark_normal);
    normalizeMap(maps.leaf_card);
    if (maps.upper_bark_albedo) normalizeMap(maps.upper_bark_albedo);
    if (maps.upper_bark_normal) normalizeMap(maps.upper_bark_normal);
    if (maps.organ_albedo) normalizeMap(maps.organ_albedo);
    return maps;
  }

  free(): void {
    this.inner.free();
  }
}

/* ── Nature patch preview ─────────────────────────────────────────────── */

export interface NatureMesh {
  name: string;
  vertices: VertexData;
  indices: number[];
  submeshes: { index_start: number; index_count: number; material: MaterialKind }[];
  vertex_count: number;
  triangle_count: number;
}

export interface NaturePrototype {
  name: string;
  kind: string;
  uses_card_atlas?: boolean;
  color_base?: string;
  color_tip?: string;
  lods: NatureMesh[];
}

export interface NatureWind {
  direction_degrees: number;
  strength: number;
  speed: number;
  gust_scale: number;
  flutter: number;
}

export interface ScatterInstance {
  position: [number, number, number];
  yaw: number;
  height: number;
  width: number;
  phase: number;
  color_variation: number;
}

export interface ScatterChunk {
  chunk_x: number;
  chunk_z: number;
  bounds_min: [number, number, number];
  bounds_max: [number, number, number];
  instances: ScatterInstance[];
}

export interface ScatterSet {
  layer_index: number;
  layer_name: string;
  kind: string;
  chunks: ScatterChunk[];
}

export interface NaturePreview {
  manifest: unknown;
  biome?: string;
  wind?: NatureWind;
  terrain: NatureMesh;
  terrain_albedo?: RgbaMap;
  terrain_normal?: RgbaMap;
  prototypes: NaturePrototype[];
  scatter_sets: ScatterSet[];
  card_atlas?: GeneratedMap;
  stats: {
    tile_size: number;
    terrain_vertex_count: number;
    terrain_triangle_count: number;
    prototype_count: number;
    scatter_instance_count: number;
  };
}

/** Preview a NaturePatch document: terrain mesh, groundcover prototype LODs,
 *  and chunked scatter placements. `previewResolution` is the terrain grid
 *  resolution; `scatterChunkSize` the world-space chunk edge in metres. */
export function generateNaturePreview(
  toml: string,
  previewResolution = 64,
  scatterChunkSize = 16,
): NaturePreview {
  const out = generate_nature_preview_from_toml(
    toml,
    previewResolution,
    scatterChunkSize,
  ) as NaturePreview;
  if (out.card_atlas) normalizeMap(out.card_atlas);
  if (out.terrain_albedo) out.terrain_albedo.data = asBytes(out.terrain_albedo.data);
  if (out.terrain_normal) out.terrain_normal.data = asBytes(out.terrain_normal.data);
  return out;
}

/* ── Volumetric nature stand preview ──────────────────────────────────── */

export interface StandStratumStats {
  name: string;
  stems: number;
  instanced: number;
  merged: number;
  requested_cover: number;
  achieved_cover: number;
  lod0_triangles: number;
}

export interface StandTileStats {
  tile: [number, number];
  tile_size_m: number;
  total_stems: number;
  instanced_count: number;
  merged_count: number;
  requested_total_cover: number;
  achieved_total_cover: number;
  triangles_lod0: number;
  triangles_lod1: number;
  triangles_lod2: number;
  material_count: number;
  foliage_mode: 'cutout' | 'opaque';
  generation_ms: number;
  strata: StandStratumStats[];
}

export interface StandPreview {
  name: string;
  biome: string;
  tile_size_m: number;
  ground: NatureMesh;
  lod0: NatureMesh;
  lod1: NatureMesh;
  lod2: NatureMesh;
  stats: StandTileStats;
}

/** Preview a volumetric NatureStand TOML in-memory (ground + Stand LOD 0/1/2 + stats). */
export function generateStandPreview(
  standToml: string,
  speciesMapJson: string,
  tileSizeM = 64,
  includeTrack = true,
  groundRgb: [number, number, number] = [0.68, 0.64, 0.56],
): StandPreview {
  return generate_stand_preview_from_toml(
    standToml,
    speciesMapJson,
    tileSizeM,
    includeTrack,
    groundRgb[0],
    groundRgb[1],
    groundRgb[2],
  ) as StandPreview;
}
