/**
 * Engine boundary for the Midori workbench.
 *
 * The midori-wasm WebAssembly build is the same midori-core code that powers the
 * CLI and FFI crates, compiled for the webview. Species documents cross the
 * boundary as TOML text (authoritative) or as the serde JSON projection
 * produced by `toJson()` / consumed by `fromJson()`.
 */

import init, { MidoriGenerator } from './wasm/midori_wasm.js';

export interface VertexData {
  positions: number[];
  normals: number[];
  uvs: number[];
  uv2s: number[];
  colors: number[];
}

export type MaterialKind = 'bark' | 'leaves' | 'impostor';

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

  static fromJson(json: unknown): Generator {
    return new Generator(MidoriGenerator.fromJson(json));
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
  generate(seed: number): LodMesh[] {
    const out = this.inner.generate(BigInt(seed)) as { lods: LodMesh[] };
    for (const lod of out.lods) {
      if (lod.impostor_atlas) normalizeMap(lod.impostor_atlas);
    }
    return out.lods;
  }

  /** Stem/leaf/bounds stats without mesh output. */
  stats(seed: number): TreeStats {
    return this.inner.get_stats(BigInt(seed)) as TreeStats;
  }

  /** Single-file binary glTF for `seed`. `embedTextures` embeds the species'
   *  generated material maps (bark albedo+normal, leaf card). Impostor
   *  atlases are embedded whenever a LOD bakes one. */
  exportGlb(seed: number, embedTextures: boolean): Uint8Array {
    return this.inner.export_glb(BigInt(seed), embedTextures);
  }

  /** Separate .gltf JSON + .bin parts. `binName` becomes the buffer URI;
   *  embedded images ride inside the .bin via bufferView references. */
  exportGltf(seed: number, binName: string, embedTextures: boolean): GltfParts {
    return this.inner.exportGltf(BigInt(seed), binName, embedTextures) as GltfParts;
  }

  /** The species' procedural material maps — `png` + `rgba` encodings from
   *  a single bake (deterministic). */
  generateMaps(): MaterialMaps {
    const maps = this.inner.generateMaps() as MaterialMaps;
    normalizeMap(maps.bark_albedo);
    normalizeMap(maps.bark_normal);
    normalizeMap(maps.leaf_card);
    return maps;
  }

  free(): void {
    this.inner.free();
  }
}
