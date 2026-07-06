<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { writable } from 'svelte/store';
  import * as THREE from 'three';
  import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
  import {
    treeStore,
    type MeshOutput,
    type NaturePreviewOutput,
    type NatureMeshOutput,
    type NatureScatterInstance
  } from '$lib/stores/tree';
  import { editorStore } from '$lib/stores/editor';
  import {
    createTreeMesh,
    createTreeGeometry,
    disposeTreeMesh,
    setWireframeMode,
    getVertexCount,
    getTriangleCount,
    updateWindTime,
    setWindParams,
    updateCameraPosition,
    type TreeMeshData,
    type TreeMeshResult
  } from '$lib/three/TreeMesh';

  function transformMeshOutput(mesh: NatureMeshOutput | any): TreeMeshData | null {
    if (!mesh || !mesh.vertices) {
      console.warn('Preview3D: Invalid mesh structure');
      return null;
    }

    const { positions, normals, uvs, uv2s, colors } = mesh.vertices;
    const indices = mesh.indices;

    if (!positions || positions.length === 0) {
      console.warn('Preview3D: Empty positions array');
      return null;
    }

    // Calculate vertex count from positions (3 floats per vertex)
    const vertexCount = Math.floor(positions.length / 3);

    // Create interleaved vertex buffer (14 floats per vertex)
    // Format: position(3) + normal(3) + uv(2) + uv2(2) + color(4)
    const FLOATS_PER_VERTEX = 14;
    const interleavedVertices = new Float32Array(vertexCount * FLOATS_PER_VERTEX);

    for (let i = 0; i < vertexCount; i++) {
      const offset = i * FLOATS_PER_VERTEX;

      // Position (3 floats)
      interleavedVertices[offset + 0] = positions[i * 3 + 0] ?? 0;
      interleavedVertices[offset + 1] = positions[i * 3 + 1] ?? 0;
      interleavedVertices[offset + 2] = positions[i * 3 + 2] ?? 0;

      // Normal (3 floats)
      interleavedVertices[offset + 3] = normals?.[i * 3 + 0] ?? 0;
      interleavedVertices[offset + 4] = normals?.[i * 3 + 1] ?? 1;
      interleavedVertices[offset + 5] = normals?.[i * 3 + 2] ?? 0;

      // UV (2 floats)
      interleavedVertices[offset + 6] = uvs?.[i * 2 + 0] ?? 0;
      interleavedVertices[offset + 7] = uvs?.[i * 2 + 1] ?? 0;

      // UV2 (2 floats)
      interleavedVertices[offset + 8] = uv2s?.[i * 2 + 0] ?? 0;
      interleavedVertices[offset + 9] = uv2s?.[i * 2 + 1] ?? 0;

      // Color (4 floats - RGBA)
      interleavedVertices[offset + 10] = colors?.[i * 4 + 0] ?? 1;
      interleavedVertices[offset + 11] = colors?.[i * 4 + 1] ?? 1;
      interleavedVertices[offset + 12] = colors?.[i * 4 + 2] ?? 1;
      interleavedVertices[offset + 13] = colors?.[i * 4 + 3] ?? 1;
    }

    // Convert indices to Uint32Array
    const indicesArray = new Uint32Array(indices ?? []);

    // Parse submesh data from WASM
    const submeshes = (mesh.submeshes ?? []).map((s: any) => ({
      start: s.start,
      count: s.count,
      material_type: s.material_type
    }));

    console.log(`Preview3D: Transformed mesh - ${vertexCount} vertices, ${Math.floor(indicesArray.length / 3)} triangles, ${submeshes.length} submeshes`);

    return {
      vertices: interleavedVertices,
      indices: indicesArray,
      submeshes
    };
  }

  /**
   * Transform WASM tree output to TreeMeshData format.
   * WASM returns separate arrays; TreeMesh expects interleaved data.
   */
  function transformWasmMeshData(wasmData: any, lodIndex: number = 0): TreeMeshData | null {
    const lods = wasmData?.lods;
    if (!lods || !Array.isArray(lods) || lods.length === 0) {
      console.warn('Preview3D: No LODs in mesh data');
      return null;
    }

    const lod = lods[Math.min(lodIndex, lods.length - 1)];
    return transformMeshOutput(lod);
  }

  // Exported stores for parent component access
  export const cameraPosition = writable<THREE.Vector3>(new THREE.Vector3(5, 5, 10));
  export const meshStats = writable<{ vertices: number; triangles: number }>({
    vertices: 0,
    triangles: 0
  });

  // Component state
  let container: HTMLDivElement;
  let renderer: THREE.WebGLRenderer | null = null;
  let scene: THREE.Scene | null = null;
  let camera: THREE.PerspectiveCamera | null = null;
  let controls: OrbitControls | null = null;
  let treeMeshResult: TreeMeshResult | null = null;
  let natureGroup: THREE.Group | null = null;
  let animationId: number = 0;
  let resizeObserver: ResizeObserver | null = null;
  let ground: THREE.Mesh | null = null;
  let gridHelper: THREE.GridHelper | null = null;
  let clock: THREE.Clock | null = null;
  let sunLight: THREE.DirectionalLight | null = null;
  let scaleReference: THREE.Group | null = null;
  let renderedMeshSource: MeshOutput | null = null;
  let renderedNatureSource: NaturePreviewOutput | null = null;
  let renderedNatureOptionsKey = '';
  let renderedNatureScatter = 0;
  let renderedNatureProfileLabel = 'Authoring';
  let renderedNatureLodLabel = 'LOD 0';
  let renderedLodIndex = -1;

  // Scene configuration
  const BACKGROUND_COLOR = 0x0f0f1a;
  const GROUND_COLOR = 0x1a1a2e;
  const GRID_COLOR_PRIMARY = 0x2a2a4a;
  const GRID_COLOR_SECONDARY = 0x1a1a2e;
  const GROUND_SIZE = 50;
  const GRID_DIVISIONS = 50;

  /**
   * Initialize the Three.js scene
   */
  function initScene(): void {
    if (!container) return;

    // Create clock for animation timing
    clock = new THREE.Clock();

    // Create scene
    scene = new THREE.Scene();
    scene.background = new THREE.Color(BACKGROUND_COLOR);

    // Create camera
    camera = new THREE.PerspectiveCamera(
      60,
      container.clientWidth / container.clientHeight,
      0.1,
      1000
    );
    camera.position.set(5, 5, 10);

    // Create renderer
    renderer = new THREE.WebGLRenderer({
      antialias: true,
      powerPreference: 'high-performance'
    });
    renderer.setSize(container.clientWidth, container.clientHeight);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.5;
    container.appendChild(renderer.domElement);

    // Create controls
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.05;
    controls.minDistance = 1;
    controls.maxDistance = 100;
    controls.maxPolarAngle = Math.PI * 0.95;
    controls.target.set(0, 2, 0);

    // Setup lighting
    setupLighting();

    // Setup ground and grid
    setupGround();
  }

  /**
   * Setup scene lighting - three-point lighting setup
   */
  function setupLighting(): void {
    if (!scene) return;

    // Strong ambient light for base illumination
    const ambientLight = new THREE.AmbientLight(0xffffff, 0.5);
    scene.add(ambientLight);

    // Key light - main directional light (sun)
    const keyLight = new THREE.DirectionalLight(0xffffff, 1.5);
    keyLight.position.set(10, 20, 10);
    keyLight.castShadow = true;
    sunLight = keyLight;

    // Shadow configuration
    keyLight.shadow.mapSize.width = 2048;
    keyLight.shadow.mapSize.height = 2048;
    keyLight.shadow.camera.near = 0.5;
    keyLight.shadow.camera.far = 50;
    keyLight.shadow.camera.left = -20;
    keyLight.shadow.camera.right = 20;
    keyLight.shadow.camera.top = 20;
    keyLight.shadow.camera.bottom = -20;
    keyLight.shadow.bias = -0.0001;

    scene.add(keyLight);
    updateSunLight();

    // Fill light - softer light from opposite side
    const fillLight = new THREE.DirectionalLight(0xffffff, 0.8);
    fillLight.position.set(-10, 15, -5);
    scene.add(fillLight);

    // Back/rim light - creates edge definition
    const backLight = new THREE.DirectionalLight(0xffffff, 0.6);
    backLight.position.set(0, 10, -15);
    scene.add(backLight);

    // Hemisphere light for natural sky/ground color variation
    const hemiLight = new THREE.HemisphereLight(0x87ceeb, 0x3d2817, 0.6);
    scene.add(hemiLight);
  }

  /**
   * Setup ground plane and grid
   */
  function setupGround(): void {
    if (!scene) return;

    // Ground plane
    const groundGeometry = new THREE.PlaneGeometry(GROUND_SIZE, GROUND_SIZE);
    const groundMaterial = new THREE.MeshStandardMaterial({
      color: GROUND_COLOR,
      roughness: 0.8,
      metalness: 0.0
    });
    ground = new THREE.Mesh(groundGeometry, groundMaterial);
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = -0.01; // Slightly below grid to prevent z-fighting
    ground.receiveShadow = true;
    scene.add(ground);

    // Grid helper
    gridHelper = new THREE.GridHelper(
      GROUND_SIZE,
      GRID_DIVISIONS,
      GRID_COLOR_PRIMARY,
      GRID_COLOR_SECONDARY
    );
    scene.add(gridHelper);
  }

  function updateSunLight(): void {
    if (!sunLight) return;

    const radius = 24;
    const azimuth = THREE.MathUtils.degToRad($editorStore.sunAzimuth);
    const elevation = THREE.MathUtils.degToRad($editorStore.sunElevation);
    sunLight.intensity = $editorStore.sunIntensity;
    sunLight.position.set(
      Math.cos(elevation) * Math.sin(azimuth) * radius,
      Math.sin(elevation) * radius,
      Math.cos(elevation) * Math.cos(azimuth) * radius
    );
  }

  function createScaleReference(): THREE.Group {
    const group = new THREE.Group();
    group.name = 'scale-reference';
    group.position.set(-1.4, 0, -1.4);

    const material = new THREE.MeshBasicMaterial({ color: 0xfacc15 });
    const baseMaterial = new THREE.MeshBasicMaterial({ color: 0x94a3b8 });

    const postGeometry = new THREE.CylinderGeometry(0.018, 0.018, 2, 8);
    const post = new THREE.Mesh(postGeometry, material);
    post.position.y = 1;
    group.add(post);

    for (const y of [1, 2]) {
      const tickGeometry = new THREE.BoxGeometry(0.45, 0.025, 0.025);
      const tick = new THREE.Mesh(tickGeometry, material);
      tick.position.set(0.225, y, 0);
      group.add(tick);
    }

    const baseGeometry = new THREE.BoxGeometry(0.55, 0.025, 0.55);
    const base = new THREE.Mesh(baseGeometry, baseMaterial);
    base.position.y = 0.0125;
    group.add(base);

    return group;
  }

  function disposeObject(object: THREE.Object3D): void {
    object.traverse(child => {
      if (child instanceof THREE.Mesh) {
        child.geometry.dispose();
        if (Array.isArray(child.material)) {
          for (const material of child.material) material.dispose();
        } else {
          child.material.dispose();
        }
      }
    });
  }

  function updateScaleReference(): void {
    if (!scene) return;

    if ($editorStore.showScaleReference && !scaleReference) {
      scaleReference = createScaleReference();
      scene.add(scaleReference);
    } else if (!$editorStore.showScaleReference && scaleReference) {
      scene.remove(scaleReference);
      disposeObject(scaleReference);
      scaleReference = null;
    }
  }

  function updateWindSettings(): void {
    const direction = THREE.MathUtils.degToRad($editorStore.windDirection);
    setWindParams(
      $editorStore.windEnabled ? $editorStore.windStrength : 0,
      $editorStore.windSpeed,
      Math.sin(direction),
      Math.cos(direction)
    );
  }

  function getSelectedLodIndex(meshData: MeshOutput, currentLod = $editorStore.currentLod): number {
    const lods = meshData.lods ?? [];
    if (lods.length === 0) return 0;
    return Math.min(currentLod, lods.length - 1);
  }

  function updateAutoLod(): void {
    if (
      $editorStore.lodMode !== 'auto' ||
      !$treeStore.meshData ||
      !camera ||
      !treeMeshResult?.geometry.boundingSphere
    ) {
      return;
    }

    const lods = $treeStore.meshData.lods ?? [];
    if (lods.length < 2) return;

    const sphere = treeMeshResult.geometry.boundingSphere;
    const distance = Math.max(camera.position.distanceTo(sphere.center), 0.001);
    const fov = THREE.MathUtils.degToRad(camera.fov);
    const screenHeight = (sphere.radius * 2) / (2 * distance * Math.tan(fov / 2));
    const nextIndex = lods.findIndex(lod => screenHeight >= (lod.screen_height ?? 0));
    const lodIndex = nextIndex >= 0 ? nextIndex : lods.length - 1;

    if (lodIndex !== $editorStore.currentLod) {
      editorStore.setLod(lodIndex);
    }
  }

  /**
   * Animation loop
   */
  function animate(): void {
    animationId = requestAnimationFrame(animate);

    if (!controls || !renderer || !scene || !camera || !clock) return;

    // Get delta time for wind animation
    const deltaTime = clock.getDelta();

    // Update wind animation
    updateWindTime(deltaTime);

    // Update camera position in shader materials
    if (treeMeshResult && treeMeshResult.materials) {
      updateCameraPosition(treeMeshResult.materials, camera);
    }

    // Update auto-rotation
    controls.autoRotate = $editorStore.autoRotate;
    controls.autoRotateSpeed = $editorStore.autoRotateSpeed;

    // Update controls
    controls.update();

    // Update camera position store
    cameraPosition.set(camera.position.clone());

    updateAutoLod();

    // Render
    renderer.render(scene, camera);
  }

  /**
   * Handle container resize
   */
  function handleResize(): void {
    if (!camera || !renderer || !container) return;

    const width = container.clientWidth;
    const height = container.clientHeight;

    camera.aspect = width / height;
    camera.updateProjectionMatrix();

    renderer.setSize(width, height);
  }

  function clearTreeMesh(): void {
    if (!treeMeshResult) return;
    if (scene) scene.remove(treeMeshResult.mesh);
    disposeTreeMesh(treeMeshResult);
    treeMeshResult = null;
    renderedMeshSource = null;
    renderedLodIndex = -1;
  }

  function clearNaturePreview(): void {
    if (!natureGroup) return;
    if (scene) scene.remove(natureGroup);
    disposeObject(natureGroup);
    natureGroup = null;
    renderedNatureSource = null;
    renderedNatureOptionsKey = '';
    renderedNatureScatter = 0;
  }

  function createNatureMaterial(kind: string, wireframe: boolean): THREE.MeshStandardMaterial {
    const color =
      kind === 'terrain'
        ? 0x4c4330
        : kind === 'moss'
          ? 0x3f6b45
          : kind === 'flower'
            ? 0xa98ce8
            : kind === 'weed'
              ? 0x5f8f44
              : kind === 'litter'
                ? 0x8b6235
                : kind === 'shrub'
                  ? 0x476b36
                  : kind === 'rock'
                    ? 0x77786c
                    : kind === 'log'
                      ? 0x6a4328
                      : 0x7aa84f;
    return new THREE.MeshStandardMaterial({
      color,
      roughness: kind === 'terrain' || kind === 'rock' ? 0.95 : 0.78,
      metalness: 0,
      side: kind === 'terrain' ? THREE.FrontSide : THREE.DoubleSide,
      wireframe
    });
  }

  function flattenScatterInstances(natureData: NaturePreviewOutput, kind: string): NatureScatterInstance[] {
    const instances: NatureScatterInstance[] = [];
    for (const set of natureData.scatter_sets ?? []) {
      if (set.kind !== kind) continue;
      for (const chunk of set.chunks ?? []) {
        instances.push(...(chunk.instances ?? []));
      }
    }
    return instances;
  }

  function currentNatureOptionsKey(): string {
    return `${$editorStore.naturePreviewProfile}:${$editorStore.naturePreviewLod}`;
  }

  function natureProfileDensityScale(natureData: NaturePreviewOutput): number {
    const manifest = natureData.manifest ?? {};
    const profile = $editorStore.naturePreviewProfile;
    if (profile === 'mobile') return manifest.mobile?.density_scale ?? 1;
    if (profile === 'console') return manifest.console?.density_scale ?? 1;
    return 1;
  }

  function natureProfileLabel(): string {
    const profile = $editorStore.naturePreviewProfile;
    return profile.charAt(0).toUpperCase() + profile.slice(1);
  }

  function keepProfileInstance(index: number, densityScale: number): boolean {
    if (densityScale >= 1) return true;
    if (densityScale <= 0) return false;
    const hash = ((index + 1) * 2654435761) >>> 0;
    return hash / 4294967295 < densityScale;
  }

  function selectProfileInstances(instances: NatureScatterInstance[], densityScale: number): NatureScatterInstance[] {
    return instances.filter((_, index) => keepProfileInstance(index, densityScale));
  }

  function focusOnNature(tileSize: number): void {
    if (!controls || !camera) return;
    const radius = Math.max(tileSize * 0.75, 4);
    controls.target.set(0, 0.3, 0);
    camera.position.set(radius * 0.8, radius * 0.55, radius * 0.9);
    controls.update();
  }

  function updateNaturePreview(natureData: NaturePreviewOutput): void {
    if (!scene) return;

    clearTreeMesh();
    clearNaturePreview();

    const group = new THREE.Group();
    group.name = 'nature-preview';

    const terrainData = transformMeshOutput(natureData.terrain);
    if (terrainData) {
      const terrainGeometry = createTreeGeometry(terrainData);
      const terrainMaterial = createNatureMaterial('terrain', $editorStore.showWireframe);
      const terrainMesh = new THREE.Mesh(terrainGeometry, terrainMaterial);
      terrainMesh.name = 'nature-terrain';
      terrainMesh.receiveShadow = true;
      group.add(terrainMesh);
    }

    let renderedVertices = natureData.stats?.terrain_vertex_count ?? 0;
    let renderedTriangles = natureData.stats?.terrain_triangle_count ?? 0;
    renderedNatureScatter = 0;
    renderedNatureProfileLabel = natureProfileLabel();
    const forcedLod = Math.max(0, Math.floor($editorStore.naturePreviewLod));
    renderedNatureLodLabel = `LOD ${forcedLod}`;
    const densityScale = Math.min(1, Math.max(0, natureProfileDensityScale(natureData)));
    const maxInstancesPerKind = 1500;
    const yAxis = new THREE.Vector3(0, 1, 0);

    for (const prototype of natureData.prototypes ?? []) {
      const lods = prototype.lods ?? [];
      const lod = lods[Math.min(forcedLod, Math.max(0, lods.length - 1))];
      if (!lod) continue;

      const meshData = transformMeshOutput(lod);
      if (!meshData) continue;

      const instances = selectProfileInstances(
        flattenScatterInstances(natureData, prototype.kind),
        densityScale
      ).slice(0, maxInstancesPerKind);
      if (instances.length === 0) continue;

      const geometry = createTreeGeometry(meshData);
      const material = createNatureMaterial(prototype.kind, $editorStore.showWireframe);
      const instanced = new THREE.InstancedMesh(geometry, material, instances.length);
      instanced.name = `nature-${prototype.kind}-${prototype.name}`;
      instanced.frustumCulled = true;
      instanced.castShadow = false;
      instanced.receiveShadow = true;

      const position = new THREE.Vector3();
      const quaternion = new THREE.Quaternion();
      const scale = new THREE.Vector3();
      const matrix = new THREE.Matrix4();

      instances.forEach((instance, index) => {
        position.set(instance.position[0], instance.position[1], instance.position[2]);
        quaternion.setFromAxisAngle(yAxis, instance.yaw);
        scale.set(instance.width, instance.height, instance.width);
        matrix.compose(position, quaternion, scale);
        instanced.setMatrixAt(index, matrix);
      });
      instanced.instanceMatrix.needsUpdate = true;
      group.add(instanced);

      renderedVertices += (lod.vertex_count ?? 0) * instances.length;
      renderedTriangles += (lod.triangle_count ?? 0) * instances.length;
      renderedNatureScatter += instances.length;
    }

    scene.add(group);
    natureGroup = group;
    renderedNatureSource = natureData;
    renderedNatureOptionsKey = currentNatureOptionsKey();
    meshStats.set({ vertices: renderedVertices, triangles: renderedTriangles });
    focusOnNature(natureData.stats?.tile_size ?? 16);
  }

  /**
   * Update tree mesh from WASM data
   */
  function updateTreeMesh(meshData: TreeMeshData, shouldFocus: boolean): void {
    if (!scene) return;

    clearNaturePreview();
    clearTreeMesh();

    // Validate mesh data
    if (!meshData || !meshData.vertices || meshData.vertices.length === 0) {
      console.warn('Preview3D: Invalid or empty mesh data');
      meshStats.set({ vertices: 0, triangles: 0 });
      return;
    }

    try {
      // Create new tree mesh
      treeMeshResult = createTreeMesh(meshData, {
        wireframe: $editorStore.showWireframe,
        castShadow: true,
        receiveShadow: true
      });

      scene.add(treeMeshResult.mesh);

      // Update stats
      meshStats.set({
        vertices: getVertexCount(meshData),
        triangles: getTriangleCount(meshData)
      });

      if (shouldFocus) {
        focusOnTree();
      }
    } catch (error) {
      console.error('Preview3D: Failed to create tree mesh:', error);
    }
  }

  /**
   * Focus camera on the tree mesh
   */
  function focusOnTree(): void {
    if (!treeMeshResult || !controls || !camera) return;

    const geometry = treeMeshResult.geometry;
    if (!geometry.boundingSphere) {
      geometry.computeBoundingSphere();
    }

    const sphere = geometry.boundingSphere;
    if (sphere) {
      const center = sphere.center;
      const radius = sphere.radius;

      // Set orbit target to center of tree
      controls.target.set(center.x, center.y, center.z);

      // Position camera to see the whole tree
      const distance = radius * 3;
      const angle = Math.PI / 4;
      camera.position.set(
        center.x + distance * Math.sin(angle),
        center.y + radius * 0.5,
        center.z + distance * Math.cos(angle)
      );
    }
  }

  /**
   * Toggle wireframe mode
   */
  function updateWireframe(wireframe: boolean): void {
    if (treeMeshResult && treeMeshResult.materials) {
      setWireframeMode(treeMeshResult.materials, wireframe);
    }
    if (natureGroup) {
      natureGroup.traverse(child => {
        if (child instanceof THREE.Mesh) {
          const materials = Array.isArray(child.material) ? child.material : [child.material];
          for (const material of materials) {
            if (material instanceof THREE.MeshStandardMaterial) {
              material.wireframe = wireframe;
            }
          }
        }
      });
    }
  }

  /**
   * Reset camera to default position
   */
  export function resetCamera(): void {
    if (!camera || !controls) return;

    camera.position.set(5, 5, 10);
    controls.target.set(0, 2, 0);
    controls.update();
  }

  /**
   * Set camera position programmatically
   */
  export function setCameraPosition(x: number, y: number, z: number): void {
    if (!camera) return;
    camera.position.set(x, y, z);
  }

  /**
   * Set camera target (look-at point)
   */
  export function setCameraTarget(x: number, y: number, z: number): void {
    if (!controls) return;
    controls.target.set(x, y, z);
  }

  /**
   * Take a screenshot of the current view
   */
  export function takeScreenshot(): string | null {
    if (!renderer) return null;
    return renderer.domElement.toDataURL('image/png');
  }

  /**
   * Get the current scene for external manipulation
   */
  export function getScene(): THREE.Scene | null {
    return scene;
  }

  /**
   * Get the current camera
   */
  export function getCamera(): THREE.PerspectiveCamera | null {
    return camera;
  }

  /**
   * Cleanup resources
   */
  function cleanup(): void {
    // Stop animation loop
    if (animationId) {
      cancelAnimationFrame(animationId);
      animationId = 0;
    }

    // Disconnect resize observer
    if (resizeObserver) {
      resizeObserver.disconnect();
      resizeObserver = null;
    }

    // Dispose tree mesh
    clearTreeMesh();
    clearNaturePreview();

    // Dispose ground
    if (ground) {
      if (scene) scene.remove(ground);
      ground.geometry.dispose();
      (ground.material as THREE.Material).dispose();
      ground = null;
    }

    // Dispose grid
    if (gridHelper && scene) {
      scene.remove(gridHelper);
      gridHelper = null;
    }

    if (scaleReference) {
      if (scene) scene.remove(scaleReference);
      disposeObject(scaleReference);
      scaleReference = null;
    }

    // Dispose controls
    if (controls) {
      controls.dispose();
      controls = null;
    }

    // Dispose renderer
    if (renderer) {
      renderer.dispose();
      if (container && renderer.domElement.parentElement === container) {
        container.removeChild(renderer.domElement);
      }
      renderer = null;
    }

    scene = null;
    camera = null;
    sunLight = null;
    renderedMeshSource = null;
    renderedNatureSource = null;
    renderedLodIndex = -1;
  }

  // Lifecycle
  onMount(() => {
    initScene();
    animate();

    // Setup resize observer
    resizeObserver = new ResizeObserver(handleResize);
    resizeObserver.observe(container);
  });

  onDestroy(() => {
    cleanup();
  });

  // Reactive statements
  $: if ($treeStore.previewMode === 'tree' && $treeStore.meshData && scene) {
    const nextLodIndex = getSelectedLodIndex($treeStore.meshData, $editorStore.currentLod);
    if ($treeStore.meshData !== renderedMeshSource || nextLodIndex !== renderedLodIndex) {
      const transformedData = transformWasmMeshData($treeStore.meshData, nextLodIndex);
      if (transformedData) {
        const shouldFocus = $treeStore.meshData !== renderedMeshSource;
        updateTreeMesh(transformedData, shouldFocus);
        renderedMeshSource = $treeStore.meshData;
        renderedLodIndex = nextLodIndex;
      }
    }
  }

  $: if ($treeStore.previewMode === 'nature' && $treeStore.natureData && scene) {
    const natureOptionsKey = `${$editorStore.naturePreviewProfile}:${$editorStore.naturePreviewLod}`;
    if ($treeStore.natureData !== renderedNatureSource || renderedNatureOptionsKey !== natureOptionsKey) {
      updateNaturePreview($treeStore.natureData);
    }
  }

  $: updateWireframe($editorStore.showWireframe);
  $: updateSunLight();
  $: updateScaleReference();
  $: updateWindSettings();

  $: currentPreviewLod = $treeStore.meshData
    ? $treeStore.meshData.lods?.[getSelectedLodIndex($treeStore.meshData, $editorStore.currentLod)] ?? null
    : null;
  $: natureStats = $treeStore.natureData?.stats ?? null;
</script>

<div class="preview-container" bind:this={container}>
  {#if $treeStore.loading}
    <div class="overlay">
      <div class="spinner"></div>
      <span>Generating...</span>
    </div>
  {/if}

  {#if $treeStore.error}
    <div class="error">
      <span>Error: {$treeStore.error}</span>
    </div>
  {/if}

  {#if $treeStore.previewMode === 'nature'}
    <div class="stats" data-testid="preview-stats">
      <span>Nature Patch</span>
      <span>Profile: {renderedNatureProfileLabel}</span>
      <span>LOD: {renderedNatureLodLabel}</span>
      <span>Tile: {natureStats?.tile_size?.toLocaleString() ?? '--'}m</span>
      <span>Vertices: {$meshStats.vertices.toLocaleString()}</span>
      <span>Triangles: {$meshStats.triangles.toLocaleString()}</span>
      <span>Scatter: {renderedNatureScatter.toLocaleString()}</span>
      <span>Prototypes: {natureStats?.prototype_count?.toLocaleString() ?? '--'}</span>
    </div>
  {:else}
    <div class="stats" data-testid="preview-stats">
      <span>{currentPreviewLod?.name ?? 'LOD --'}</span>
      <span>Vertices: {$meshStats.vertices.toLocaleString()}</span>
      <span>Triangles: {$meshStats.triangles.toLocaleString()}</span>
      <span>Branches: {currentPreviewLod?.branch_count?.toLocaleString() ?? '--'}</span>
      <span>Leaves: {currentPreviewLod?.leaf_count?.toLocaleString() ?? '--'}</span>
    </div>
  {/if}
</div>

<style>
  .preview-container {
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
  }

  .overlay {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    color: var(--text-secondary, #9ca3af);
    z-index: 10;
    pointer-events: none;
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 3px solid var(--border, #374151);
    border-top-color: var(--accent, #4ade80);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error {
    position: absolute;
    bottom: 1rem;
    left: 50%;
    transform: translateX(-50%);
    background: rgba(220, 38, 38, 0.9);
    padding: 0.5rem 1rem;
    border-radius: 4px;
    color: white;
    z-index: 10;
    max-width: 90%;
    text-align: center;
  }

  .stats {
    position: absolute;
    bottom: 0.5rem;
    left: 0.5rem;
    display: flex;
    flex-wrap: wrap;
    gap: 1rem;
    font-size: 0.75rem;
    color: var(--text-secondary, #9ca3af);
    background: rgba(15, 15, 26, 0.8);
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    z-index: 5;
  }

  .stats span {
    font-family: monospace;
  }
</style>
