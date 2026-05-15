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

// Bark fragment shader with procedural texture and normal perturbation
const barkFragmentShader = `
  uniform float uTime;

  varying vec2 vUv;
  varying vec3 vNormal;
  varying vec3 vWorldPosition;
  varying float vDepth;
  varying float vAO;

  // Hash and noise functions
  float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
  }

  float hash3(vec3 p) {
    return fract(sin(dot(p, vec3(127.1, 311.7, 74.7))) * 43758.5453);
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

    for (int i = 0; i < 6; i++) {
      value += amplitude * noise(p * frequency);
      amplitude *= 0.5;
      frequency *= 2.0;
    }

    return value;
  }

  // Voronoi for bark cracks
  vec2 voronoi(vec2 p) {
    vec2 n = floor(p);
    vec2 f = fract(p);

    float minDist = 1.0;
    float secondMin = 1.0;
    vec2 minPoint;

    for (int j = -1; j <= 1; j++) {
      for (int i = -1; i <= 1; i++) {
        vec2 g = vec2(float(i), float(j));
        vec2 o = vec2(hash(n + g), hash(n + g + vec2(17.0, 31.0)));
        vec2 diff = g + o - f;
        float d = dot(diff, diff);

        if (d < minDist) {
          secondMin = minDist;
          minDist = d;
          minPoint = n + g + o;
        } else if (d < secondMin) {
          secondMin = d;
        }
      }
    }

    return vec2(sqrt(minDist), sqrt(secondMin) - sqrt(minDist));
  }

  // Calculate procedural normal from height function
  vec3 calcBarkNormal(vec2 uv, float scale) {
    float eps = 0.01;
    float h0 = fbm(uv * scale);
    float hx = fbm((uv + vec2(eps, 0.0)) * scale);
    float hy = fbm((uv + vec2(0.0, eps)) * scale);

    vec3 tangent = normalize(vec3(eps, 0.0, (hx - h0) * 0.3));
    vec3 bitangent = normalize(vec3(0.0, eps, (hy - h0) * 0.3));

    return normalize(cross(bitangent, tangent));
  }

  void main() {
    // More natural bark colors
    vec3 darkBark = vec3(0.12, 0.08, 0.05);
    vec3 midBark = vec3(0.22, 0.15, 0.10);
    vec3 lightBark = vec3(0.38, 0.28, 0.20);
    vec3 mossBark = vec3(0.15, 0.18, 0.10);

    // Scale UVs for bark pattern - more stretch vertically
    vec2 barkUV = vUv * vec2(4.0, 12.0);

    // Deep vertical fissures using voronoi
    vec2 vor = voronoi(barkUV * vec2(0.8, 0.3));
    float cracks = smoothstep(0.0, 0.08, vor.y);

    // Vertical bark ridges
    float ridges = fbm(barkUV * vec2(1.5, 0.4));
    ridges = smoothstep(0.35, 0.65, ridges);

    // Fine detail texture
    float detail = fbm(barkUV * 3.0);
    float fineDetail = fbm(barkUV * 8.0);

    // Horizontal growth rings (subtle, mainly on trunk)
    float rings = sin(vUv.y * 60.0 + fbm(barkUV * 0.3) * 4.0) * 0.5 + 0.5;
    rings = pow(rings, 4.0) * (1.0 - vDepth) * 0.4;

    // Large-scale color variation
    float largeVar = fbm(vWorldPosition.xz * 0.2 + vWorldPosition.y * 0.1);

    // Moss/lichen patches (more on north-facing and lower areas)
    float mossChance = (1.0 - vNormal.y) * 0.5 + (1.0 - vUv.y) * 0.3;
    float moss = fbm(vWorldPosition.xz * 2.0) * mossChance;
    moss = smoothstep(0.4, 0.7, moss) * 0.5;

    // Build up the bark color
    vec3 barkColor = mix(darkBark, midBark, ridges);
    barkColor = mix(barkColor, darkBark * 0.6, 1.0 - cracks); // Dark in cracks
    barkColor = mix(barkColor, lightBark, rings);
    barkColor = mix(barkColor, midBark, detail * 0.3);
    barkColor *= 0.85 + largeVar * 0.3;
    barkColor *= 0.9 + fineDetail * 0.2;

    // Add moss tint
    barkColor = mix(barkColor, mossBark, moss);

    // Apply ambient occlusion (stronger in cracks)
    float ao = vAO * (0.6 + cracks * 0.4);
    barkColor *= ao;

    // Calculate perturbed normal for lighting
    vec3 bumpNormal = calcBarkNormal(barkUV, 3.0);
    vec3 perturbedNormal = normalize(vNormal + bumpNormal * 0.4);

    // Lighting
    vec3 lightDir = normalize(vec3(0.5, 1.0, 0.3));
    float NdotL = max(dot(perturbedNormal, lightDir), 0.0);
    float diffuse = NdotL * 0.65 + 0.35;

    // Deeper shadows in cracks
    diffuse *= 0.7 + cracks * 0.3;

    // Rim lighting
    vec3 viewDir = normalize(cameraPosition - vWorldPosition);
    float rim = 1.0 - max(dot(viewDir, perturbedNormal), 0.0);
    rim = pow(rim, 3.5) * 0.12;

    // Specular highlight (wet bark look)
    vec3 halfVec = normalize(lightDir + viewDir);
    float spec = pow(max(dot(perturbedNormal, halfVec), 0.0), 32.0);
    spec *= 0.08 * (1.0 - moss); // Less specular on mossy areas

    vec3 finalColor = barkColor * diffuse;
    finalColor += vec3(1.0, 0.95, 0.85) * rim;
    finalColor += vec3(1.0, 0.98, 0.95) * spec;

    gl_FragColor = vec4(finalColor, 1.0);
  }
`;

// Leaf fragment shader with realistic texture, veins, and subsurface scattering
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

  // Leaf shape SDF for alpha masking
  float leafSDF(vec2 uv) {
    // Center UV and scale
    vec2 p = (uv - 0.5) * 2.0;

    // Base oval shape, pointy at tip
    float tipFactor = 1.0 - p.y * 0.4; // Narrower toward tip (positive Y)
    float baseFactor = 1.0 + p.y * 0.2; // Wider at base (negative Y)
    float width = 0.7 * tipFactor * baseFactor;

    // Egg-like shape
    float d = length(vec2(p.x / width, p.y * 0.8)) - 0.5;

    // Add slight lobes/serration at edges
    float angle = atan(p.x, p.y);
    float serration = sin(angle * 12.0) * 0.02 * (1.0 - abs(p.y));
    d -= serration;

    return d;
  }

  // Procedural vein pattern
  float veins(vec2 uv) {
    vec2 p = uv - 0.5;

    // Main midrib (center vein)
    float midrib = 1.0 - smoothstep(0.0, 0.025, abs(p.x));
    midrib *= smoothstep(-0.5, 0.4, p.y); // Fade at base

    // Lateral veins branching from midrib
    float lateralVeins = 0.0;
    for (float i = 0.0; i < 6.0; i++) {
      float yPos = -0.3 + i * 0.12;
      float veinY = p.y - yPos;
      // Veins angle outward and upward
      float veinAngle = 0.5 + i * 0.05;
      float veinX = abs(p.x) - abs(veinY) * veinAngle;

      // Only draw vein in the right region
      if (veinY > -0.05 && veinY < 0.15) {
        float d = abs(veinX) + abs(veinY) * 0.3;
        float vein = 1.0 - smoothstep(0.0, 0.02, d);
        vein *= smoothstep(-0.02, 0.02, veinY);
        vein *= smoothstep(0.15, 0.05, veinY);
        vein *= step(0.02, abs(p.x)); // Don't overlap midrib
        lateralVeins = max(lateralVeins, vein * 0.6);
      }
    }

    // Fine tertiary veins (noise-based)
    float fineVeins = noise(uv * 40.0) * 0.15;
    fineVeins *= smoothstep(0.4, 0.2, abs(p.x)); // Stronger near center

    return midrib * 0.8 + lateralVeins + fineVeins;
  }

  void main() {
    // Leaf shape alpha
    float sdf = leafSDF(vUv);
    float alpha = 1.0 - smoothstep(-0.02, 0.02, sdf);

    // Discard pixels outside leaf shape
    if (alpha < 0.1) discard;

    // Edge darkening
    float edgeDist = -sdf;
    float edgeDarkening = smoothstep(0.0, 0.15, edgeDist);

    // Per-leaf color variation
    float leafId = hash(floor(vWorldPosition.xz * 5.0));

    // Base colors - more natural greens
    vec3 leafGreen = vec3(0.15, 0.38, 0.08);
    vec3 leafLight = vec3(0.25, 0.48, 0.12);
    vec3 leafDark = vec3(0.06, 0.20, 0.04);
    vec3 leafYellow = vec3(0.35, 0.42, 0.08);

    // Mix base color based on leaf variation
    vec3 baseColor = mix(leafGreen, leafLight, leafId * 0.4);
    baseColor = mix(baseColor, leafYellow, pow(leafId, 3.0) * 0.3);

    // Vein pattern
    float veinPattern = veins(vUv);
    vec3 veinColor = leafDark * 0.7;
    baseColor = mix(baseColor, veinColor, veinPattern * 0.5);

    // Edge darkening and natural variation
    baseColor *= 0.7 + edgeDarkening * 0.3;

    // Add subtle noise variation across leaf surface
    float surfaceNoise = noise(vUv * 20.0);
    baseColor *= 0.9 + surfaceNoise * 0.2;

    // Lighting
    vec3 lightDir = normalize(vec3(0.5, 1.0, 0.3));
    float NdotL = dot(vNormal, lightDir);

    // Front lighting with softer falloff
    float frontLight = max(NdotL, 0.0) * 0.6 + 0.4;

    // Subsurface scattering (backlit leaves glow golden-green)
    float subsurface = max(-NdotL, 0.0);
    vec3 sssColor = vec3(0.6, 0.75, 0.15);
    float sssStrength = subsurface * 0.7 * (1.0 - veinPattern * 0.5);

    // View-dependent effects
    vec3 viewDir = normalize(cameraPosition - vWorldPosition);
    float rim = 1.0 - abs(dot(viewDir, vNormal));
    rim = pow(rim, 2.5) * 0.25;

    // Combine lighting
    vec3 finalColor = baseColor * frontLight;
    finalColor += baseColor * sssColor * sssStrength;
    finalColor += vec3(0.7, 0.85, 0.5) * rim;
    finalColor *= vAO;

    // Soften alpha at edges for anti-aliasing
    alpha = smoothstep(0.0, 0.15, edgeDist);

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
