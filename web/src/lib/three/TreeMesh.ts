/**
 * Three.js mesh builder for tree geometry
 *
 * Converts mesh data from the Midori WASM module into
 * Three.js BufferGeometry for rendering.
 *
 * Vertex data format (56 bytes per vertex):
 * - position: 3 floats (12 bytes)
 * - normal: 3 floats (12 bytes)
 * - uv: 2 floats (8 bytes)
 * - uv2: 2 floats (8 bytes)
 * - color: 4 floats (16 bytes) - RGBA
 */

import * as THREE from 'three';
import {
  createWindyBarkMaterial,
  createWindyLeafMaterial,
  updateCameraPosition
} from './WindShader';

// Re-export wind shader utilities
export { updateWindTime, setWindParams, updateCameraPosition } from './WindShader';

/**
 * Material types for tree submeshes
 */
export enum MaterialType {
  Bark = 0,
  Leaf = 1,
  Twig = 2,
  Frond = 3
}

/**
 * Submesh definition from WASM
 */
export interface Submesh {
  start: number;
  count: number;
  material_type: MaterialType;
}

/**
 * Mesh data from the Midori WASM module
 */
export interface TreeMeshData {
  vertices: Float32Array;
  indices: Uint32Array;
  submeshes: Submesh[];
}

/**
 * Constants for vertex data parsing
 */
const FLOATS_PER_VERTEX = 14; // 56 bytes / 4 bytes per float
const POSITION_OFFSET = 0;
const NORMAL_OFFSET = 3;
const UV_OFFSET = 6;
const UV2_OFFSET = 8;
const COLOR_OFFSET = 10;

/**
 * Parse interleaved vertex data into separate attribute arrays
 */
export function parseVertexData(vertices: Float32Array): {
  positions: Float32Array;
  normals: Float32Array;
  uvs: Float32Array;
  uv2s: Float32Array;
  colors: Float32Array;
} {
  const vertexCount = Math.floor(vertices.length / FLOATS_PER_VERTEX);

  const positions = new Float32Array(vertexCount * 3);
  const normals = new Float32Array(vertexCount * 3);
  const uvs = new Float32Array(vertexCount * 2);
  const uv2s = new Float32Array(vertexCount * 2);
  const colors = new Float32Array(vertexCount * 4);

  for (let i = 0; i < vertexCount; i++) {
    const baseIndex = i * FLOATS_PER_VERTEX;

    // Position (3 floats)
    positions[i * 3] = vertices[baseIndex + POSITION_OFFSET];
    positions[i * 3 + 1] = vertices[baseIndex + POSITION_OFFSET + 1];
    positions[i * 3 + 2] = vertices[baseIndex + POSITION_OFFSET + 2];

    // Normal (3 floats)
    normals[i * 3] = vertices[baseIndex + NORMAL_OFFSET];
    normals[i * 3 + 1] = vertices[baseIndex + NORMAL_OFFSET + 1];
    normals[i * 3 + 2] = vertices[baseIndex + NORMAL_OFFSET + 2];

    // UV (2 floats)
    uvs[i * 2] = vertices[baseIndex + UV_OFFSET];
    uvs[i * 2 + 1] = vertices[baseIndex + UV_OFFSET + 1];

    // UV2 (2 floats)
    uv2s[i * 2] = vertices[baseIndex + UV2_OFFSET];
    uv2s[i * 2 + 1] = vertices[baseIndex + UV2_OFFSET + 1];

    // Color (4 floats - RGBA)
    colors[i * 4] = vertices[baseIndex + COLOR_OFFSET];
    colors[i * 4 + 1] = vertices[baseIndex + COLOR_OFFSET + 1];
    colors[i * 4 + 2] = vertices[baseIndex + COLOR_OFFSET + 2];
    colors[i * 4 + 3] = vertices[baseIndex + COLOR_OFFSET + 3];
  }

  return { positions, normals, uvs, uv2s, colors };
}

/**
 * Create a Three.js BufferGeometry from parsed vertex data
 */
export function createTreeGeometry(meshData: TreeMeshData): THREE.BufferGeometry {
  const geometry = new THREE.BufferGeometry();

  // Parse interleaved vertex data
  const { positions, normals, uvs, uv2s, colors } = parseVertexData(meshData.vertices);

  // Set attributes
  geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
  geometry.setAttribute('uv', new THREE.BufferAttribute(uvs, 2));
  geometry.setAttribute('uv2', new THREE.BufferAttribute(uv2s, 2));
  geometry.setAttribute('color', new THREE.BufferAttribute(colors, 4));

  // Set indices
  geometry.setIndex(new THREE.BufferAttribute(meshData.indices, 1));

  // Add submesh groups for multi-material rendering
  if (meshData.submeshes && meshData.submeshes.length > 0) {
    for (let i = 0; i < meshData.submeshes.length; i++) {
      const submesh = meshData.submeshes[i];
      geometry.addGroup(submesh.start, submesh.count, i);
    }
  }

  // Compute bounding box and sphere for frustum culling
  geometry.computeBoundingBox();
  geometry.computeBoundingSphere();

  return geometry;
}

/**
 * Create bark material with wind animation and procedural texture
 */
export function createBarkMaterial(wireframe: boolean = false): THREE.Material {
  return createWindyBarkMaterial(wireframe);
}

/**
 * Create leaf material with wind animation and subsurface scattering
 */
export function createLeafMaterial(wireframe: boolean = false): THREE.Material {
  return createWindyLeafMaterial(wireframe);
}

/**
 * Create twig material
 */
export function createTwigMaterial(wireframe: boolean = false): THREE.MeshStandardMaterial {
  return new THREE.MeshStandardMaterial({
    color: 0x4a3728,
    roughness: 0.85,
    metalness: 0.0,
    side: THREE.DoubleSide,
    vertexColors: true,
    wireframe
  });
}

/**
 * Create frond material (for palm-like trees)
 */
export function createFrondMaterial(wireframe: boolean = false): THREE.MeshStandardMaterial {
  return new THREE.MeshStandardMaterial({
    color: 0x2d5a27,
    roughness: 0.7,
    metalness: 0.0,
    side: THREE.DoubleSide,
    vertexColors: true,
    transparent: true,
    alphaTest: 0.3,
    wireframe
  });
}

/**
 * Create all tree materials based on submesh types
 */
export function createTreeMaterials(
  submeshes: Submesh[],
  wireframe: boolean = false
): THREE.Material[] {
  return submeshes.map(submesh => {
    switch (submesh.material_type) {
      case MaterialType.Bark:
        return createBarkMaterial(wireframe);
      case MaterialType.Leaf:
        return createLeafMaterial(wireframe);
      case MaterialType.Twig:
        return createTwigMaterial(wireframe);
      case MaterialType.Frond:
        return createFrondMaterial(wireframe);
      default:
        return createBarkMaterial(wireframe);
    }
  });
}

/**
 * Create wireframe material for debug view
 */
export function createWireframeMaterial(): THREE.MeshBasicMaterial {
  return new THREE.MeshBasicMaterial({
    color: 0x4ade80,
    wireframe: true
  });
}

/**
 * Result of building a tree mesh
 */
export interface TreeMeshResult {
  mesh: THREE.Mesh;
  geometry: THREE.BufferGeometry;
  materials: THREE.Material[];
}

/**
 * Create a tree mesh with proper materials for each submesh
 */
export function createTreeMesh(
  meshData: TreeMeshData,
  options: {
    wireframe?: boolean;
    castShadow?: boolean;
    receiveShadow?: boolean;
  } = {}
): TreeMeshResult {
  const { wireframe = false, castShadow = true, receiveShadow = true } = options;

  const geometry = createTreeGeometry(meshData);

  let materials: THREE.Material[];

  if (meshData.submeshes && meshData.submeshes.length > 0) {
    materials = createTreeMaterials(meshData.submeshes, wireframe);
  } else {
    // Fallback to single bark material if no submeshes defined
    materials = [createBarkMaterial(wireframe)];
  }

  const mesh = new THREE.Mesh(
    geometry,
    materials.length === 1 ? materials[0] : materials
  );

  mesh.castShadow = castShadow;
  mesh.receiveShadow = receiveShadow;
  mesh.name = 'tree';

  return { mesh, geometry, materials };
}

/**
 * Update wireframe mode on all materials
 */
export function setWireframeMode(materials: THREE.Material[], wireframe: boolean): void {
  for (const material of materials) {
    if (material instanceof THREE.MeshStandardMaterial) {
      material.wireframe = wireframe;
    } else if (material instanceof THREE.ShaderMaterial) {
      material.wireframe = wireframe;
    }
  }
}

/**
 * Dispose of tree mesh resources
 */
export function disposeTreeMesh(result: TreeMeshResult | null): void {
  if (!result) return;

  // Dispose geometry
  if (result.geometry) {
    result.geometry.dispose();
  }

  // Dispose materials
  for (const material of result.materials) {
    material.dispose();
  }
}

/**
 * Get vertex count from mesh data
 */
export function getVertexCount(meshData: TreeMeshData): number {
  return Math.floor(meshData.vertices.length / FLOATS_PER_VERTEX);
}

/**
 * Get triangle count from mesh data
 */
export function getTriangleCount(meshData: TreeMeshData): number {
  return Math.floor(meshData.indices.length / 3);
}
