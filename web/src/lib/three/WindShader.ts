/**
 * Wind animation and procedural texture shaders for tree rendering
 *
 * Uses Pivot Painter data encoded in vertex attributes:
 * - uv2.x: hierarchy depth (0=trunk, 1=leaves)
 * - uv2.y: animation phase offset
 * - color.xyz: normalized pivot position
 * - color.w: stiffness (0=flexible, 1=rigid)
 */

import * as THREE from 'three';

// Shared wind uniforms - updated each frame
export const windUniforms = {
  uTime: { value: 0 },
  uWindStrength: { value: 1.0 },
  uWindFrequency: { value: 1.5 },
  uWindDirection: { value: new THREE.Vector2(1, 0.3) }
};

/**
 * Update wind time - call this in animation loop
 */
export function updateWindTime(deltaTime: number): void {
  windUniforms.uTime.value += deltaTime;
}

/**
 * Set wind parameters
 */
export function setWindParams(strength: number, frequency: number, dirX: number, dirZ: number): void {
  windUniforms.uWindStrength.value = strength;
  windUniforms.uWindFrequency.value = frequency;
  windUniforms.uWindDirection.value.set(dirX, dirZ);
}

// Vertex shader with wind animation
// Note: uv2 is automatically provided by Three.js when geometry has uv2 attribute
// but color must be declared explicitly for ShaderMaterial
const windVertexShader = `
  attribute vec4 color;

  uniform float uTime;
  uniform float uWindStrength;
  uniform float uWindFrequency;
  uniform vec2 uWindDirection;

  varying vec2 vUv;
  varying vec3 vNormal;
  varying vec3 vWorldPosition;
  varying float vDepth;
  varying float vAO;

  // Simple noise function
  float noise(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
  }

  // Smooth noise
  float smoothNoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);

    float a = noise(i);
    float b = noise(i + vec2(1.0, 0.0));
    float c = noise(i + vec2(0.0, 1.0));
    float d = noise(i + vec2(1.0, 1.0));

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
  }

  void main() {
    vUv = uv;
    vNormal = normalize(normalMatrix * normal);

    // Pivot Painter data
    float depth = uv2.x;           // 0 = trunk, 1 = leaves
    float phase = uv2.y;           // Animation phase offset
    vec3 pivotPos = color.xyz;     // Normalized pivot position
    float stiffness = color.w;     // 0 = flexible, 1 = rigid

    vDepth = depth;
    vAO = 1.0 - depth * 0.3; // Simple AO based on depth

    // Wind calculation
    vec3 worldPos = (modelMatrix * vec4(position, 1.0)).xyz;
    vWorldPosition = worldPos;

    // Base wind movement
    float windTime = uTime * uWindFrequency;
    float windPhase = phase * 6.28318 + windTime;

    // Multi-frequency wind
    float wind1 = sin(windPhase + worldPos.x * 0.5) * 0.5 + 0.5;
    float wind2 = sin(windPhase * 1.7 + worldPos.z * 0.3) * 0.5 + 0.5;
    float wind3 = smoothNoise(vec2(windTime * 0.3, worldPos.x * 0.1)) * 0.5;

    float windFactor = (wind1 + wind2 * 0.5 + wind3 * 0.3) / 1.8;

    // Flexibility based on depth and stiffness
    float flexibility = depth * (1.0 - stiffness);

    // Trunk sway (affects everything above ground)
    float trunkSway = sin(windTime * 0.5) * 0.02 * uWindStrength;

    // Branch movement (increases with depth)
    float branchMovement = windFactor * flexibility * uWindStrength * 0.15;

    // Leaf flutter (high frequency for leaves only)
    float leafFlutter = 0.0;
    if (depth > 0.7) {
      leafFlutter = sin(windPhase * 3.0 + phase * 20.0) * (depth - 0.7) * 0.1 * uWindStrength;
    }

    // Apply displacement
    vec3 displacement = vec3(0.0);
    displacement.x = (trunkSway + branchMovement) * uWindDirection.x;
    displacement.z = (trunkSway + branchMovement) * uWindDirection.y;
    displacement.y = -abs(branchMovement) * 0.3; // Slight droop
    displacement += normal * leafFlutter;

    // Height-based amplification
    float heightFactor = smoothstep(0.0, 2.0, position.y);
    displacement *= heightFactor;

    vec3 newPosition = position + displacement;

    gl_Position = projectionMatrix * modelViewMatrix * vec4(newPosition, 1.0);
  }
`;

// Bark fragment shader with procedural texture
const barkFragmentShader = `
  uniform float uTime;

  varying vec2 vUv;
  varying vec3 vNormal;
  varying vec3 vWorldPosition;
  varying float vDepth;
  varying float vAO;

  // FBM noise for bark texture
  float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
  }

  float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);

    float a = hash(i);
    float b = hash(i + vec2(1.0, 0.0));
    float c = hash(i + vec2(0.0, 1.0));
    float d = hash(i + vec2(1.0, 1.0));

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
  }

  float fbm(vec2 p) {
    float value = 0.0;
    float amplitude = 0.5;
    float frequency = 1.0;

    for (int i = 0; i < 5; i++) {
      value += amplitude * noise(p * frequency);
      amplitude *= 0.5;
      frequency *= 2.0;
    }

    return value;
  }

  void main() {
    // Base bark colors
    vec3 darkBark = vec3(0.18, 0.12, 0.08);
    vec3 lightBark = vec3(0.35, 0.25, 0.18);
    vec3 midBark = vec3(0.28, 0.20, 0.14);

    // UV-based bark pattern
    vec2 barkUV = vUv * vec2(3.0, 8.0);

    // Vertical fissures
    float fissure = fbm(barkUV * vec2(2.0, 0.5));
    fissure = smoothstep(0.3, 0.7, fissure);

    // Horizontal rings (more visible on trunk)
    float rings = sin(vUv.y * 40.0 + fbm(barkUV * 0.5) * 3.0) * 0.5 + 0.5;
    rings = smoothstep(0.4, 0.6, rings) * (1.0 - vDepth);

    // Large scale variation
    float largeNoise = fbm(vWorldPosition.xz * 0.3);

    // Small scale detail
    float detail = fbm(barkUV * 4.0);

    // Combine for final color
    vec3 barkColor = mix(darkBark, midBark, fissure);
    barkColor = mix(barkColor, lightBark, rings * 0.3);
    barkColor = mix(barkColor, darkBark, detail * 0.2);
    barkColor *= 0.8 + largeNoise * 0.4;

    // Apply ambient occlusion
    barkColor *= vAO;

    // Simple lighting
    vec3 lightDir = normalize(vec3(0.5, 1.0, 0.3));
    float NdotL = max(dot(vNormal, lightDir), 0.0);
    float diffuse = NdotL * 0.6 + 0.4;

    // Rim lighting
    vec3 viewDir = normalize(cameraPosition - vWorldPosition);
    float rim = 1.0 - max(dot(viewDir, vNormal), 0.0);
    rim = pow(rim, 3.0) * 0.15;

    vec3 finalColor = barkColor * diffuse + vec3(1.0, 0.95, 0.9) * rim;

    gl_FragColor = vec4(finalColor, 1.0);
  }
`;

// Leaf fragment shader with subsurface scattering
const leafFragmentShader = `
  uniform float uTime;

  varying vec2 vUv;
  varying vec3 vNormal;
  varying vec3 vWorldPosition;
  varying float vDepth;
  varying float vAO;

  float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
  }

  void main() {
    // Leaf color with variation
    vec3 leafGreen = vec3(0.18, 0.42, 0.12);
    vec3 leafYellow = vec3(0.45, 0.50, 0.15);
    vec3 leafDark = vec3(0.08, 0.22, 0.05);

    // Per-leaf color variation based on world position
    float variation = hash(floor(vWorldPosition.xz * 10.0));
    vec3 baseColor = mix(leafGreen, leafYellow, variation * 0.3);
    baseColor = mix(baseColor, leafDark, (1.0 - variation) * 0.2);

    // Vein pattern (simplified)
    float vein = abs(vUv.x - 0.5) * 2.0;
    vein = smoothstep(0.0, 0.1, vein);
    baseColor = mix(baseColor * 0.8, baseColor, vein);

    // Lighting
    vec3 lightDir = normalize(vec3(0.5, 1.0, 0.3));
    float NdotL = dot(vNormal, lightDir);

    // Front lighting
    float frontLight = max(NdotL, 0.0) * 0.5 + 0.5;

    // Subsurface scattering (backlit leaves glow)
    float subsurface = max(-NdotL, 0.0);
    vec3 sssColor = vec3(0.5, 0.7, 0.2); // Warm green glow
    float sssStrength = subsurface * 0.6;

    // View-dependent effects
    vec3 viewDir = normalize(cameraPosition - vWorldPosition);

    // Rim/fresnel
    float rim = 1.0 - abs(dot(viewDir, vNormal));
    rim = pow(rim, 2.0) * 0.2;

    // Combine lighting
    vec3 finalColor = baseColor * frontLight;
    finalColor += sssColor * sssStrength; // Add SSS
    finalColor += vec3(0.8, 0.9, 0.7) * rim; // Add rim
    finalColor *= vAO;

    // Alpha based on UV (for leaf shape)
    float alpha = 1.0;

    gl_FragColor = vec4(finalColor, alpha);
  }
`;

/**
 * Create bark material with wind animation and procedural texture
 */
export function createWindyBarkMaterial(wireframe: boolean = false): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: {
      ...windUniforms,
      cameraPosition: { value: new THREE.Vector3() }
    },
    vertexShader: windVertexShader,
    fragmentShader: barkFragmentShader,
    side: THREE.DoubleSide,
    wireframe
  });
}

/**
 * Create leaf material with wind animation and subsurface scattering
 */
export function createWindyLeafMaterial(wireframe: boolean = false): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: {
      ...windUniforms,
      cameraPosition: { value: new THREE.Vector3() }
    },
    vertexShader: windVertexShader,
    fragmentShader: leafFragmentShader,
    side: THREE.DoubleSide,
    transparent: true,
    wireframe
  });
}

/**
 * Update camera position uniform for all wind materials
 */
export function updateCameraPosition(materials: THREE.Material[], camera: THREE.Camera): void {
  for (const material of materials) {
    if (material instanceof THREE.ShaderMaterial && material.uniforms.cameraPosition) {
      material.uniforms.cameraPosition.value.copy(camera.position);
    }
  }
}
