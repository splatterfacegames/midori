/**
 * WASM loader for Midori tree generation module
 *
 * This module handles loading and initializing the midori-wasm
 * WebAssembly module that provides tree generation capabilities.
 */

export interface MeshData {
  vertices: Float32Array;
  normals: Float32Array;
  uvs: Float32Array;
  indices: Uint32Array;
  vertexCount: number;
  triangleCount: number;
}

export interface MidoriGeneratorInterface {
  name: string;
  generate(seed: bigint): MeshData;
  setParameter(name: string, value: number): void;
  getParameter(name: string): number;
}

let wasmModule: any = null;
let initialized = false;

/**
 * Initialize the WASM module
 * @returns Promise that resolves when WASM is ready
 */
export async function initWasm(): Promise<boolean> {
  if (initialized) return true;

  try {
    // Dynamic import of the WASM package
    wasmModule = await import('midori-wasm');

    // Initialize the WASM module (calls the init function)
    await wasmModule.default();

    initialized = true;
    console.log('Midori WASM module initialized');
    return true;
  } catch (error) {
    console.error('Failed to initialize Midori WASM module:', error);
    return false;
  }
}

/**
 * Check if WASM module is initialized
 */
export function isInitialized(): boolean {
  return initialized;
}

/**
 * Create a new tree generator from TOML configuration
 * @param toml - TOML configuration string for the tree species
 * @returns MidoriGenerator instance
 */
export function createGenerator(toml: string): MidoriGeneratorInterface | null {
  if (!initialized || !wasmModule) {
    console.error('WASM module not initialized');
    return null;
  }

  try {
    return new wasmModule.MidoriGenerator(toml);
  } catch (error) {
    console.error('Failed to create generator:', error);
    return null;
  }
}

/**
 * Get the WASM module version
 */
export function getVersion(): string {
  if (!initialized || !wasmModule) {
    return 'unknown';
  }
  return wasmModule.version?.() ?? 'unknown';
}
