/* tslint:disable */
/* eslint-disable */

/**
 * Tree generator that holds a parsed species definition.
 *
 * Create a generator from a TOML string, then use it to generate
 * trees with different seeds.
 */
export class MidoriGenerator {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Export tree as GLB binary data.
     *
     * Returns a Uint8Array containing the complete GLB file.
     */
    export_glb(seed: bigint): Uint8Array;
    /**
     * Generate a tree and return mesh data as a JavaScript object.
     *
     * Returns an object containing all LOD levels with their mesh data.
     */
    generate(seed: bigint): any;
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
    readonly midorigenerator_export_glb: (a: number, b: bigint) => [number, number, number];
    readonly midorigenerator_generate: (a: number, b: bigint) => [number, number, number];
    readonly midorigenerator_generate_lod: (a: number, b: bigint, c: number) => [number, number, number];
    readonly midorigenerator_get_stats: (a: number, b: bigint) => [number, number, number];
    readonly midorigenerator_metadata: (a: number) => [number, number, number];
    readonly midorigenerator_name: (a: number) => [number, number];
    readonly midorigenerator_new: (a: number, b: number) => [number, number, number];
    readonly midorinaturegenerator_name: (a: number) => [number, number];
    readonly midorinaturegenerator_new: (a: number, b: number) => [number, number, number];
    readonly midorinaturegenerator_preview: (a: number, b: number, c: number) => [number, number, number];
    readonly init: () => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_externrefs: WebAssembly.Table;
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
