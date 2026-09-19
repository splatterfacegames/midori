/* tslint:disable */
/* eslint-disable */

/**
 * Tree generator that holds a parsed species definition.
 *
 * Create a generator from a TOML string or a JSON species object, then use it
 * to generate trees with different seeds.
 */
export class MidoriGenerator {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Export tree as separate `.gltf` JSON + `.bin` parts.
     *
     * `bin_name` is written into the glTF buffer URI. Returns an object with
     * `gltf` and `bin` Uint8Array fields. Embedded images ride inside `.bin`
     * via bufferView references.
     */
    exportGltf(seed: bigint, bin_name: string, embed_textures: boolean): any;
    /**
     * Export tree as GLB binary data.
     *
     * Returns a Uint8Array containing the complete GLB file. When
     * `embed_textures` is true the species' generated material maps (bark
     * albedo+normal, leaf card) are embedded; baked impostor atlases are
     * always embedded when a LOD uses `crown_impostor`.
     */
    export_glb(seed: bigint, embed_textures: boolean): Uint8Array;
    /**
     * Create a new generator from a species JSON object.
     *
     * Accepts the same shape produced by `toJson()`, so editors can mutate the
     * object and round-trip it through the engine for validation.
     */
    static fromJson(value: any): MidoriGenerator;
    /**
     * Generate a tree and return mesh data as a JavaScript object.
     *
     * Returns an object containing all LOD levels with their mesh data.
     * LOD generation is driven by the species' `[lod]` configuration.
     */
    generate(seed: bigint): any;
    /**
     * Generate the species' material maps.
     *
     * Returns `{ bark_albedo, bark_normal, leaf_card }`, each a
     * `GeneratedMap`: `png` carries PNG bytes for blob URLs and file
     * inspection; `rgba` carries the same image as `{ data, width, height }`
     * raw RGBA8 (top row first — the glTF/`TextureSource` V convention) for
     * hosts that upload textures directly to the GPU. One `TextureSet` bake
     * serves both encodings; `data`/`png` cross as Uint8Arrays.
     *
     * Deterministic for the species' `[textures]` parameters. The browser
     * has no filesystem, so file-slot overrides are ignored here (procedural
     * maps are used); native hosts resolve slots via `TextureSet::resolve`.
     */
    generateMaps(): any;
    /**
     * Generate only a specific LOD level.
     */
    generate_lod(seed: bigint, lod_level: number): any;
    /**
     * Get generation statistics without full mesh data.
     *
     * Useful for previewing tree complexity before generating full mesh.
     */
    get_stats(seed: bigint): any;
    /**
     * Get parsed species metadata for editor/tooling use.
     */
    metadata(): any;
    /**
     * Create a new generator from a TOML species definition string.
     */
    constructor(toml: string);
    /**
     * Return the species definition as a plain JS object.
     *
     * All defaulted fields are present in the output, so editors can render
     * every parameter without knowing the defaults.
     */
    toJson(): any;
    /**
     * Serialize the species definition back to TOML.
     */
    toToml(): string;
    /**
     * Get the species name.
     */
    readonly name: string;
}

/**
 * Nature patch generator that holds a parsed NaturePatch definition.
 */
export class MidoriNatureGenerator {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Create a new nature generator from a TOML NaturePatch definition string.
     */
    constructor(toml: string);
    /**
     * Generate terrain, prototype, scatter, and manifest data for browser preview.
     */
    preview(preview_resolution: number, scatter_chunk_size: number): any;
    /**
     * Get the patch display name.
     */
    readonly name: string;
}

/**
 * Convenience function for one-off nature preview generation.
 */
export function generate_nature_preview_from_toml(toml: string, preview_resolution: number, scatter_chunk_size: number): any;

/**
 * Quick generation without creating a generator instance.
 *
 * Convenience function for one-off tree generation.
 */
export function generate_tree_from_toml(toml: string, seed: bigint): any;

/**
 * Initialize panic hook for better error messages in WASM.
 */
export function init(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_midorigenerator_free: (a: number, b: number) => void;
    readonly __wbg_midorinaturegenerator_free: (a: number, b: number) => void;
    readonly generate_nature_preview_from_toml: (a: number, b: number, c: number, d: number) => [number, number, number];
    readonly generate_tree_from_toml: (a: number, b: number, c: bigint) => [number, number, number];
    readonly midorigenerator_exportGltf: (a: number, b: bigint, c: number, d: number, e: number) => [number, number, number];
    readonly midorigenerator_export_glb: (a: number, b: bigint, c: number) => [number, number, number];
    readonly midorigenerator_fromJson: (a: any) => [number, number, number];
    readonly midorigenerator_generate: (a: number, b: bigint) => [number, number, number];
    readonly midorigenerator_generateMaps: (a: number) => [number, number, number];
    readonly midorigenerator_generate_lod: (a: number, b: bigint, c: number) => [number, number, number];
    readonly midorigenerator_get_stats: (a: number, b: bigint) => [number, number, number];
    readonly midorigenerator_metadata: (a: number) => [number, number, number];
    readonly midorigenerator_name: (a: number) => [number, number];
    readonly midorigenerator_new: (a: number, b: number) => [number, number, number];
    readonly midorigenerator_toJson: (a: number) => [number, number, number];
    readonly midorigenerator_toToml: (a: number) => [number, number, number, number];
    readonly midorinaturegenerator_name: (a: number) => [number, number];
    readonly midorinaturegenerator_new: (a: number, b: number) => [number, number, number];
    readonly midorinaturegenerator_preview: (a: number, b: number, c: number) => [number, number, number];
    readonly init: () => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
