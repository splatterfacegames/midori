/**
 * Engine boundary for the Grove workbench.
 *
 * The grove-wasm WebAssembly build is the same grove-core code that powers the
 * CLI and FFI crates, compiled for the webview. Species documents cross the
 * boundary as TOML text (authoritative) or as the serde JSON projection
 * produced by `toJson()` / consumed by `fromJson()`.
 */

import init, { GroveGenerator } from './wasm/grove_wasm.js';

export interface VertexData {
  positions: number[];
  normals: number[];
  uvs: number[];
  uv2s: number[];
  colors: number[];
}

export type MaterialKind = 'bark' | 'leaves' | 'impostor';

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
  /** Baked crown-impostor atlas as PNG bytes (front | side views side by
   *  side), present when this LOD level uses `crown_impostor`. */
  impostor_atlas?: Uint8Array;
  vertex_count: number;
  triangle_count: number;
}

/** Species material maps as PNG-encoded bytes. */
export interface MaterialMaps {
  bark_albedo: Uint8Array;
  bark_normal: Uint8Array;
  leaf_card: Uint8Array;
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

let moduleReady: Promise<unknown> | null = null;

type WasmInput = Parameters<typeof init>[0];

/**
 * Initialize the WASM module once. `input` overrides where the module bytes
 * come from (tests pass a Buffer; the browser resolves `grove_wasm_bg.wasm`
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
  private constructor(private readonly inner: GroveGenerator) {}

  static fromToml(toml: string): Generator {
    return new Generator(new GroveGenerator(toml));
  }

  static fromJson(json: unknown): Generator {
    return new Generator(GroveGenerator.fromJson(json));
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

  /** The species' procedural material maps as PNG bytes (deterministic). */
  generateMaps(): MaterialMaps {
    return this.inner.generateMaps() as MaterialMaps;
  }

  free(): void {
    this.inner.free();
  }
}
