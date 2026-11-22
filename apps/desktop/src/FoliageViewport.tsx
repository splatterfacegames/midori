import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { Viewport3DController } from '@jethac/tools-frontend-stack/viewports';
import type { LodMesh, MaterialMaps, NaturePreview, RgbaMap, ScatterInstance } from './engine';

export type LightingMood = 'golden' | 'day' | 'mist';
export type CameraAnglePreset = 'hero' | 'overview';

interface MoodConfig {
  label: string;
  sunAzimuthDeg: number;
  sunElevationDeg: number;
  sunColor: string;
  sunIntensity: number;
  hemiSky: string;
  hemiGround: string;
  hemiIntensity: number;
  fillColor: string;
  fillIntensity: number;
  zenithColor: string;
  horizonColor: string;
  groundHorizonColor: string;
  fogColor: string;
  fogDensity: number;
  sssColor: string;
  sssIntensity: number;
  exposure: number;
}

const MOODS: Record<LightingMood, MoodConfig> = {
  golden: {
    label: 'Golden',
    sunAzimuthDeg: 136,
    sunElevationDeg: 23,
    sunColor: '#ffd39a',
    sunIntensity: 3.25,
    hemiSky: '#a4cbf2',
    hemiGround: '#6b5333',
    hemiIntensity: 1.38,
    fillColor: '#a6c6e6',
    fillIntensity: 0.72,
    zenithColor: '#245294',
    horizonColor: '#e8bf8e',
    groundHorizonColor: '#69583c',
    fogColor: '#deb98e',
    fogDensity: 0.0062,
    sssColor: '#ffbc63',
    sssIntensity: 0.72,
    exposure: 1.12,
  },
  day: {
    label: 'Day',
    sunAzimuthDeg: 52,
    sunElevationDeg: 48,
    sunColor: '#fff5e2',
    sunIntensity: 3.0,
    hemiSky: '#9ad0ff',
    hemiGround: '#4d5b38',
    hemiIntensity: 1.35,
    fillColor: '#b4d6f5',
    fillIntensity: 0.55,
    zenithColor: '#3e78bf',
    horizonColor: '#cbe0f2',
    groundHorizonColor: '#3b442d',
    fogColor: '#c6dced',
    fogDensity: 0.005,
    sssColor: '#ffe89c',
    sssIntensity: 0.48,
    exposure: 1.04,
  },
  mist: {
    label: 'Mist',
    sunAzimuthDeg: 118,
    sunElevationDeg: 29,
    sunColor: '#ffe7c7',
    sunIntensity: 2.15,
    hemiSky: '#cddbdf',
    hemiGround: '#445044',
    hemiIntensity: 1.5,
    fillColor: '#c0d2d6',
    fillIntensity: 0.68,
    zenithColor: '#6d8794',
    horizonColor: '#d7dedc',
    groundHorizonColor: '#364037',
    fogColor: '#ced7d5',
    fogDensity: 0.019,
    sssColor: '#ffd99e',
    sssIntensity: 0.45,
    exposure: 1.06,
  },
};

export interface FoliageViewportProps {
  mode: 'tree' | 'nature';
  lod?: LodMesh | null;
  maps?: MaterialMaps | null;
  layers?: { bark: boolean; leaves: boolean };
  isTopdown?: boolean;
  naturePreview?: NaturePreview | null;
  grid: boolean;
  onReady: (controller: Viewport3DController) => void;
  onError: (error: Error) => void;
  children?: ReactNode;
}

function sunDirectionFromAngles(azimuthDeg: number, elevationDeg: number): THREE.Vector3 {
  const az = THREE.MathUtils.degToRad(azimuthDeg);
  const el = THREE.MathUtils.degToRad(elevationDeg);
  const cosEl = Math.cos(el);
  return new THREE.Vector3(Math.sin(az) * cosEl, Math.sin(el), Math.cos(az) * cosEl).normalize();
}

function createDataTexture(
  src: RgbaMap,
  srgb: boolean,
  wrapRepeat: boolean,
  maxAnisotropy: number,
): THREE.DataTexture {
  const copy = new Uint8Array(src.data.length);
  copy.set(src.data);
  const tex = new THREE.DataTexture(copy, src.width, src.height, THREE.RGBAFormat);
  tex.colorSpace = srgb ? THREE.SRGBColorSpace : THREE.NoColorSpace;
  tex.wrapS = wrapRepeat ? THREE.RepeatWrapping : THREE.ClampToEdgeWrapping;
  tex.wrapT = wrapRepeat ? THREE.RepeatWrapping : THREE.ClampToEdgeWrapping;
  tex.generateMipmaps = true;
  tex.minFilter = THREE.LinearMipmapLinearFilter;
  tex.magFilter = THREE.LinearFilter;
  tex.anisotropy = Math.min(16, Math.max(1, maxAnisotropy));
  tex.flipY = false;
  tex.needsUpdate = true;
  return tex;
}

function hash2d(ix: number, iy: number): number {
  let n = Math.imul(ix, 374761393) + Math.imul(iy, 668265263);
  n = Math.imul(n ^ (n >>> 13), 1274126177);
  return ((n ^ (n >>> 16)) >>> 0) / 4294967295;
}

function valueNoise2d(x: number, y: number): number {
  const ix = Math.floor(x);
  const iy = Math.floor(y);
  const fx = x - ix;
  const fy = y - iy;
  const ux = fx * fx * (3 - 2 * fx);
  const uy = fy * fy * (3 - 2 * fy);
  const a = hash2d(ix, iy);
  const b = hash2d(ix + 1, iy);
  const c = hash2d(ix, iy + 1);
  const d = hash2d(ix + 1, iy + 1);
  return a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy;
}

function fbm2d(x: number, y: number): number {
  return (
    valueNoise2d(x, y) * 0.52 +
    valueNoise2d(x * 2.07 + 13.1, y * 2.07 - 7.4) * 0.28 +
    valueNoise2d(x * 4.31 - 5.7, y * 4.31 + 19.3) * 0.14 +
    valueNoise2d(x * 9.13 + 3.2, y * 9.13 + 11.8) * 0.06
  );
}

function createPedestalTexture(maxAnisotropy: number): THREE.DataTexture {
  const res = 256;
  const data = new Uint8Array(res * res * 4);
  for (let y = 0; y < res; y++) {
    for (let x = 0; x < res; x++) {
      const u = (x + 0.5) / res - 0.5;
      const v = (y + 0.5) / res - 0.5;
      const r = Math.min(1, Math.hypot(u, v) * 2.0);
      const macro = fbm2d(u * 11.5 + 4.2, v * 11.5 - 2.8);
      const detail = fbm2d(u * 38.0 - 9.1, v * 38.0 + 6.4);
      const grain = hash2d(x * 3 + 7, y * 3 + 13) * 0.06 - 0.03;

      // Rich forest loam right around root flare transitioning into lush moss/clover turf
      const rootSoil = Math.max(0, 1 - r * 3.1) * (0.55 + 0.45 * macro);
      const mossPatch = Math.max(0, Math.min(1, (macro - 0.32) * 1.85)) * (1 - rootSoil * 0.7);
      // Match outer rim seamlessly to vista meadow (#4c5831 -> sRGB ~0.30, 0.345, 0.19)
      const rimT = Math.max(0, Math.min(1, (r - 0.55) / 0.45));
      const rimBlend = rimT * rimT * (3 - 2 * rimT);

      const loamR = 0.34 + 0.06 * detail;
      const loamG = 0.27 + 0.05 * detail;
      const loamB = 0.18 + 0.03 * detail;

      const turfR = 0.28 + 0.06 * detail;
      const turfG = 0.36 + 0.07 * detail;
      const turfB = 0.17 + 0.04 * detail;

      const mossR = 0.23 + 0.05 * detail;
      const mossG = 0.39 + 0.07 * detail;
      const mossB = 0.15 + 0.03 * detail;

      let cr = turfR * (1 - mossPatch) + mossR * mossPatch;
      let cg = turfG * (1 - mossPatch) + mossG * mossPatch;
      let cb = turfB * (1 - mossPatch) + mossB * mossPatch;

      cr = cr * (1 - rootSoil) + loamR * rootSoil;
      cg = cg * (1 - rootSoil) + loamG * rootSoil;
      cb = cb * (1 - rootSoil) + loamB * rootSoil;

      cr = (cr + grain * (1 - rimBlend)) * (1 - rimBlend) + (76 / 255) * rimBlend;
      cg = (cg + grain * (1 - rimBlend)) * (1 - rimBlend) + (88 / 255) * rimBlend;
      cb = (cb + grain * 0.6 * (1 - rimBlend)) * (1 - rimBlend) + (49 / 255) * rimBlend;

      const idx = (y * res + x) * 4;
      data[idx] = Math.round(Math.min(1, Math.max(0, cr)) * 255);
      data[idx + 1] = Math.round(Math.min(1, Math.max(0, cg)) * 255);
      data[idx + 2] = Math.round(Math.min(1, Math.max(0, cb)) * 255);
      data[idx + 3] = 255;
    }
  }
  return createDataTexture({ data, width: res, height: res }, true, false, maxAnisotropy);
}

function createSkyDome(sunDir: THREE.Vector3, mood: MoodConfig): {
  mesh: THREE.Mesh;
  uniforms: {
    uZenithColor: { value: THREE.Color };
    uHorizonColor: { value: THREE.Color };
    uGroundColor: { value: THREE.Color };
    uSunColor: { value: THREE.Color };
    uSunDir: { value: THREE.Vector3 };
  };
} {
  const uniforms = {
    uZenithColor: { value: new THREE.Color(mood.zenithColor) },
    uHorizonColor: { value: new THREE.Color(mood.horizonColor) },
    uGroundColor: { value: new THREE.Color(mood.groundHorizonColor) },
    uSunColor: { value: new THREE.Color(mood.sunColor) },
    uSunDir: { value: sunDir.clone() },
  };
  const mat = new THREE.ShaderMaterial({
    side: THREE.BackSide,
    depthWrite: false,
    fog: false,
    uniforms,
    vertexShader: /* glsl */ `
      varying vec3 vWorldDir;
      void main() {
        vWorldDir = position;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: /* glsl */ `
      uniform vec3 uZenithColor;
      uniform vec3 uHorizonColor;
      uniform vec3 uGroundColor;
      uniform vec3 uSunColor;
      uniform vec3 uSunDir;
      varying vec3 vWorldDir;
      void main() {
        vec3 d = normalize(vWorldDir);
        float h = d.y;
        vec3 sky = mix(uHorizonColor, uZenithColor, pow(clamp(h, 0.0, 1.0), 0.34));
        vec3 ground = mix(uHorizonColor, uGroundColor, pow(clamp(-h * 3.8, 0.0, 1.0), 0.78));
        vec3 col = h >= 0.0 ? sky : ground;
        float sunCos = max(dot(d, normalize(uSunDir)), 0.0);
        col += uSunColor * (0.24 * pow(sunCos, 5.0) + 0.40 * pow(sunCos, 38.0) + 1.20 * smoothstep(0.9982, 0.9997, sunCos));
        float haze = exp(-abs(h) * 11.0);
        col = mix(col, uHorizonColor * 1.02, haze * 0.32);
        gl_FragColor = vec4(col, 1.0);
        #include <tonemapping_fragment>
        #include <colorspace_fragment>
      }
    `,
  });
  const geo = new THREE.SphereGeometry(860, 32, 20);
  const mesh = new THREE.Mesh(geo, mat);
  mesh.frustumCulled = false;
  mesh.renderOrder = -100;
  return { mesh, uniforms };
}

function disposeGroup(group: THREE.Group): void {
  group.traverse((obj) => {
    if (obj instanceof THREE.Mesh || obj instanceof THREE.LineSegments) {
      obj.geometry.dispose();
      const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
      for (const m of mats) {
        if (m && typeof m === 'object') {
          const std = m as THREE.MeshStandardMaterial;
          std.map?.dispose();
          std.normalMap?.dispose();
          m.dispose();
        }
      }
    }
  });
  group.clear();
}

export function FoliageViewport({
  mode,
  lod,
  maps,
  layers = { bark: true, leaves: true },
  isTopdown = false,
  naturePreview,
  grid,
  onReady,
  onError,
  children,
}: FoliageViewportProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const [mood, setMood] = useState<LightingMood>('golden');
  const [cameraPreset, setCameraPreset] = useState<CameraAnglePreset>('hero');
  const [windOn, setWindOn] = useState(true);

  const windEnabledRef = useRef(true);
  windEnabledRef.current = windOn;

  const fitFnRef = useRef<((presetOverride?: CameraAnglePreset) => void) | null>(null);

  const sharedUniformsRef = useRef({
    uTime: { value: 0 },
    uWindStrength: { value: 1 },
    uSunDir: { value: sunDirectionFromAngles(MOODS.golden.sunAzimuthDeg, MOODS.golden.sunElevationDeg) },
    uSubsurfaceColor: { value: new THREE.Color(MOODS.golden.sssColor) },
    uSubsurfaceIntensity: { value: MOODS.golden.sssIntensity },
  });

  const sceneRefs = useRef<{
    renderer: THREE.WebGLRenderer;
    scene: THREE.Scene;
    camera: THREE.PerspectiveCamera;
    controls: OrbitControls;
    contentGroup: THREE.Group;
    gridGroup: THREE.Group;
    sunLight: THREE.DirectionalLight;
    hemiLight: THREE.HemisphereLight;
    fillLight: THREE.DirectionalLight;
    fog: THREE.FogExp2;
    skyUniforms: ReturnType<typeof createSkyDome>['uniforms'];
    sceneBounds: { center: THREE.Vector3; size: THREE.Vector3; isNature: boolean; tileSize: number; isTopdown: boolean };
  } | null>(null);

  const applyCameraPreset = useCallback((preset: CameraAnglePreset) => {
    const ctx = sceneRefs.current;
    if (!ctx) return;
    const { camera, controls, sceneBounds } = ctx;
    const { center, size, isNature, tileSize, isTopdown: topdown } = sceneBounds;

    if (isNature) {
      const half = Math.max(4, tileSize * 0.5);
      if (preset === 'hero') {
        // Immersive Ghost of Tsushima rider-height vista gazing across swaying plumes toward the horizon
        const target = new THREE.Vector3(0, center.y + 0.64, -half * 0.16);
        const camPos = new THREE.Vector3(-half * 0.42, center.y + 1.94, half * 0.68);
        camera.position.copy(camPos);
        controls.target.copy(target);
      } else {
        const dist = tileSize * 0.92;
        camera.position.set(-dist * 0.58, center.y + dist * 0.56, dist * 0.72);
        controls.target.set(0, center.y + 0.2, 0);
      }
    } else {
      const height = Math.max(0.45, size.y);
      const radius = Math.max(0.38, Math.max(size.x, size.z) * 0.5);
      const halfV = THREE.MathUtils.degToRad(camera.fov * 0.5);
      const safeAspect = Math.max(0.38, camera.aspect || 1);
      const halfH = Math.atan(Math.tan(halfV) * safeAspect);
      const fitDistV = (height * 0.56) / Math.tan(halfV);
      const fitDistH = (radius * 1.14) / Math.tan(halfH);
      const baseDist = Math.max(fitDistV, fitDistH);
      const isCompact = size.y < 2.2;

      if (topdown) {
        // Distinct isometric ARPG Hero (46 deg) vs steep tactical RTS Overview (74 deg) on sunlit 3/4 side
        const elev = THREE.MathUtils.degToRad(preset === 'overview' ? 74 : 46);
        const dist = baseDist * (preset === 'overview' ? 1.16 : 1.14);
        camera.position.set(
          center.x + dist * Math.cos(elev) * 0.60,
          center.y + dist * Math.sin(elev),
          center.z + dist * Math.cos(elev) * 0.80,
        );
        controls.target.set(center.x, center.y - height * 0.03, center.z);
      } else if (preset === 'overview') {
        const elev = THREE.MathUtils.degToRad(isCompact ? 38 : 34);
        const dist = baseDist * 1.18;
        const sideSign = isCompact ? 1 : -1;
        camera.position.set(
          center.x + sideSign * dist * Math.cos(elev) * 0.58,
          center.y + dist * Math.sin(elev),
          center.z + dist * Math.cos(elev) * 0.82,
        );
        controls.target.set(center.x, center.y - height * 0.04, center.z);
      } else if (isCompact) {
        // Sunlit 3/4 terrace / garden-eye framing for hedges, topiary, floral planters, and potted plants
        const elev = THREE.MathUtils.degToRad(21);
        const dist = baseDist * 1.14;
        camera.position.set(
          center.x + dist * Math.cos(elev) * 0.56,
          center.y + dist * Math.sin(elev),
          center.z + dist * Math.cos(elev) * 0.83,
        );
        controls.target.set(center.x, center.y - height * 0.02, center.z);
      } else {
        // Hero upward-gazing tree portrait framing root buttresses, turf carpet, and sunlit crown pads
        const elev = THREE.MathUtils.degToRad(11.5);
        const dist = baseDist * 1.08;
        camera.position.set(
          center.x - dist * Math.cos(elev) * 0.54,
          Math.max(0.55, center.y - height * 0.14 + dist * Math.sin(elev)),
          center.z + dist * Math.cos(elev) * 0.84,
        );
        controls.target.set(center.x, center.y - height * 0.02, center.z);
      }
    }
    camera.near = 0.08;
    camera.far = 1100;
    camera.updateProjectionMatrix();
    controls.update();
  }, []);

  fitFnRef.current = (presetOverride?: CameraAnglePreset) => {
    applyCameraPreset(presetOverride ?? cameraPreset);
  };

  // Mount WebGLRenderer, SkyDome, Lights, OrbitControls, and Animation Loop once
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({
        antialias: true,
        powerPreference: 'high-performance',
      });
    } catch (err) {
      onError(err instanceof Error ? err : new Error(String(err)));
      return;
    }

    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = MOODS.golden.exposure;
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    renderer.domElement.style.display = 'block';
    renderer.domElement.style.width = '100%';
    renderer.domElement.style.height = '100%';
    host.insertBefore(renderer.domElement, host.firstChild);

    const scene = new THREE.Scene();
    const initialMood = MOODS.golden;
    const fog = new THREE.FogExp2(initialMood.fogColor, initialMood.fogDensity);
    scene.fog = fog;

    const camera = new THREE.PerspectiveCamera(44, 1, 0.08, 1100);
    camera.position.set(-4, 3.2, 8);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.07;
    controls.maxPolarAngle = Math.PI * 0.495;
    controls.minDistance = 0.35;
    controls.maxDistance = 180;

    const sunDir = sunDirectionFromAngles(initialMood.sunAzimuthDeg, initialMood.sunElevationDeg);
    const sky = createSkyDome(sunDir, initialMood);
    scene.add(sky.mesh);

    const hemiLight = new THREE.HemisphereLight(
      initialMood.hemiSky,
      initialMood.hemiGround,
      initialMood.hemiIntensity,
    );
    scene.add(hemiLight);

    const sunLight = new THREE.DirectionalLight(initialMood.sunColor, initialMood.sunIntensity);
    sunLight.position.copy(sunDir).multiplyScalar(32);
    sunLight.castShadow = true;
    sunLight.shadow.mapSize.set(2048, 2048);
    sunLight.shadow.bias = -0.0005;
    sunLight.shadow.normalBias = 0.022;
    sunLight.shadow.camera.near = 1;
    sunLight.shadow.camera.far = 110;
    scene.add(sunLight);
    scene.add(sunLight.target);

    const fillLight = new THREE.DirectionalLight(initialMood.fillColor, initialMood.fillIntensity);
    fillLight.position.set(-sunDir.x * 20, 12, -sunDir.z * 20);
    scene.add(fillLight);

    const contentGroup = new THREE.Group();
    const gridGroup = new THREE.Group();
    scene.add(contentGroup);
    scene.add(gridGroup);

    const syncSize = () => {
      const rect = host.getBoundingClientRect();
      const w = Math.max(1, Math.floor(rect.width));
      const h = Math.max(1, Math.floor(rect.height));
      renderer.setSize(w, h, false);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    };
    syncSize();

    const resizeObs = new ResizeObserver(syncSize);
    resizeObs.observe(host);

    sceneRefs.current = {
      renderer,
      scene,
      camera,
      controls,
      contentGroup,
      gridGroup,
      sunLight,
      hemiLight,
      fillLight,
      fog,
      skyUniforms: sky.uniforms,
      sceneBounds: {
        center: new THREE.Vector3(0, 3, 0),
        size: new THREE.Vector3(4, 6, 4),
        isNature: mode === 'nature',
        tileSize: 24,
        isTopdown,
      },
    };

    const controller: Viewport3DController = {
      updateMeshes: () => {},
      setSelection: () => {},
      fitAll: () => fitFnRef.current?.(),
      fitSelection: () => fitFnRef.current?.(),
      setCamera: () => {},
      setGizmo: () => {},
      dispose: () => {},
    };
    onReady(controller);

    let rafId = 0;
    const startMs = performance.now();
    const tick = () => {
      rafId = requestAnimationFrame(tick);
      const elapsed = (performance.now() - startMs) * 0.001;
      sharedUniformsRef.current.uTime.value = elapsed;
      const targetWind = windEnabledRef.current ? 1 : 0;
      sharedUniformsRef.current.uWindStrength.value +=
        (targetWind - sharedUniformsRef.current.uWindStrength.value) * 0.08;
      controls.update();
      sky.mesh.position.copy(camera.position);
      renderer.render(scene, camera);
    };
    rafId = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(rafId);
      resizeObs.disconnect();
      controls.dispose();
      disposeGroup(contentGroup);
      disposeGroup(gridGroup);
      sky.mesh.geometry.dispose();
      (sky.mesh.material as THREE.Material).dispose();
      renderer.dispose();
      renderer.domElement.remove();
      sceneRefs.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Update Lighting Mood
  useEffect(() => {
    const ctx = sceneRefs.current;
    if (!ctx) return;
    const cfg = MOODS[mood];
    const sunDir = sunDirectionFromAngles(cfg.sunAzimuthDeg, cfg.sunElevationDeg);

    sharedUniformsRef.current.uSunDir.value.copy(sunDir);
    sharedUniformsRef.current.uSubsurfaceColor.value.set(cfg.sssColor);
    sharedUniformsRef.current.uSubsurfaceIntensity.value = cfg.sssIntensity;

    ctx.renderer.toneMappingExposure = cfg.exposure;
    ctx.sunLight.color.set(cfg.sunColor);
    ctx.sunLight.intensity = cfg.sunIntensity;
    const shadowDist = ctx.sceneBounds.isNature ? ctx.sceneBounds.tileSize * 1.45 : 28;
    ctx.sunLight.position.copy(ctx.sceneBounds.center).addScaledVector(sunDir, shadowDist);
    ctx.sunLight.target.position.copy(ctx.sceneBounds.center);
    ctx.sunLight.target.updateMatrixWorld();

    ctx.hemiLight.color.set(cfg.hemiSky);
    ctx.hemiLight.groundColor.set(cfg.hemiGround);
    ctx.hemiLight.intensity = cfg.hemiIntensity;

    ctx.fillLight.color.set(cfg.fillColor);
    ctx.fillLight.intensity = cfg.fillIntensity;
    ctx.fillLight.position.set(-sunDir.x * 24, 14, -sunDir.z * 24);

    ctx.fog.color.set(cfg.fogColor);
    ctx.fog.density = cfg.fogDensity;

    ctx.skyUniforms.uZenithColor.value.set(cfg.zenithColor);
    ctx.skyUniforms.uHorizonColor.value.set(cfg.horizonColor);
    ctx.skyUniforms.uGroundColor.value.set(cfg.groundHorizonColor);
    ctx.skyUniforms.uSunColor.value.set(cfg.sunColor);
    ctx.skyUniforms.uSunDir.value.copy(sunDir);
  }, [mood]);

  // Toggle optional reference scale grid
  useEffect(() => {
    const ctx = sceneRefs.current;
    if (!ctx) return;
    disposeGroup(ctx.gridGroup);
    if (!grid) return;
    const extent = ctx.sceneBounds.isNature ? Math.max(16, ctx.sceneBounds.tileSize) : 16;
    const helper = new THREE.GridHelper(extent, Math.round(extent), 0x7c8578, 0x525a4e);
    helper.position.y = 0.006;
    const mat = helper.material as THREE.LineBasicMaterial;
    mat.transparent = true;
    mat.opacity = ctx.sceneBounds.isNature ? 0.14 : 0.22;
    mat.depthWrite = false;
    ctx.gridGroup.add(helper);
  }, [grid, mode, lod, naturePreview]);

  // Build Tree or Nature Scene Content
  useEffect(() => {
    const ctx = sceneRefs.current;
    if (!ctx) return;
    disposeGroup(ctx.contentGroup);

    const maxAniso = ctx.renderer.capabilities.getMaxAnisotropy();
    const shared = sharedUniformsRef.current;

    if (mode === 'tree') {
      if (!lod || lod.vertices.positions.length < 9) return;

      const positions = Float32Array.from(lod.vertices.positions);
      const normals = Float32Array.from(lod.vertices.normals);
      const uvs = lod.vertices.uvs.length > 0 ? Float32Array.from(lod.vertices.uvs) : null;
      const rawColors = lod.vertices.colors;
      const vCount = positions.length / 3;

      const box = new THREE.Box3();
      for (let i = 0; i < positions.length; i += 3) {
        box.expandByPoint(new THREE.Vector3(positions[i], positions[i + 1], positions[i + 2]));
      }
      const center = box.getCenter(new THREE.Vector3());
      const size = box.getSize(new THREE.Vector3());
      const groundY = box.min.y < -0.08 ? box.min.y - 0.18 : 0;
      const minY = Math.min(groundY, box.min.y);
      const treeHeight = Math.max(0.5, box.max.y - minY);
      const pedRadius = Math.max(2.0, Math.max(size.x, size.z) * 0.72);

      // Compute ground-contact footprint so turf tufts never poke up inside hedges or planters
      let footMinX = Infinity;
      let footMaxX = -Infinity;
      let footMinZ = Infinity;
      let footMaxZ = -Infinity;
      for (let i = 0; i < positions.length; i += 3) {
        if (positions[i + 1] <= groundY + 0.42) {
          if (positions[i] < footMinX) footMinX = positions[i];
          if (positions[i] > footMaxX) footMaxX = positions[i];
          if (positions[i + 2] < footMinZ) footMinZ = positions[i + 2];
          if (positions[i + 2] > footMaxZ) footMaxZ = positions[i + 2];
        }
      }

      // Update shadow frustum tightly around tree + ground shadow throw
      const shadowSpan = Math.max(pedRadius * 1.45, treeHeight * 0.95);
      ctx.sunLight.shadow.camera.left = -shadowSpan;
      ctx.sunLight.shadow.camera.right = shadowSpan;
      ctx.sunLight.shadow.camera.top = shadowSpan;
      ctx.sunLight.shadow.camera.bottom = -shadowSpan;
      ctx.sunLight.shadow.camera.near = 0.5;
      ctx.sunLight.shadow.camera.far = shadowSpan * 4.5;
      ctx.sunLight.shadow.camera.updateProjectionMatrix();
      ctx.sunLight.position.copy(center).addScaledVector(shared.uSunDir.value, shadowSpan * 2.1);
      ctx.sunLight.target.position.copy(center);
      ctx.sunLight.target.updateMatrixWorld();

      // Circular polar mossy earth mound meeting root buttresses at y=0 and blending seamlessly into vista
      const discR = pedRadius * 1.18;
      const moundHeightAt = (x: number, z: number) => {
        const rf = Math.min(1, Math.hypot(x, z) / discR);
        const sm = rf * rf * (3 - 2 * rf);
        const fade = Math.max(0, 1 - Math.max(0, (rf - 0.55) / 0.45));
        const mu = (fbm2d(x * 0.65 + 3.1, z * 0.65 - 1.9) - 0.5) * 0.028 * fade * fade;
        return 0.004 * (1 - sm) - 0.068 * sm + mu;
      };
      const rings = 24;
      const sectors = 64;
      const dPos: number[] = [0, moundHeightAt(0, 0), 0];
      const dUv: number[] = [0.5, 0.5];
      const dIdx: number[] = [];
      for (let ri = 1; ri <= rings; ri++) {
        const rr = (ri / rings) * discR;
        for (let si = 0; si < sectors; si++) {
          const ang = (si / sectors) * Math.PI * 2;
          const px = Math.cos(ang) * rr;
          const pz = Math.sin(ang) * rr;
          dPos.push(px, ri === rings ? -0.068 : moundHeightAt(px, pz), pz);
          dUv.push(0.5 + px / (discR * 2), 0.5 + pz / (discR * 2));
        }
      }
      for (let si = 0; si < sectors; si++) {
        const nxt = (si + 1) % sectors;
        dIdx.push(0, 1 + nxt, 1 + si);
      }
      for (let ri = 1; ri < rings; ri++) {
        const r0 = 1 + (ri - 1) * sectors;
        const r1 = 1 + ri * sectors;
        for (let si = 0; si < sectors; si++) {
          const nxt = (si + 1) % sectors;
          dIdx.push(r0 + si, r0 + nxt, r1 + nxt, r0 + si, r1 + nxt, r1 + si);
        }
      }
      const pedGeo = new THREE.BufferGeometry();
      pedGeo.setAttribute('position', new THREE.Float32BufferAttribute(dPos, 3));
      pedGeo.setAttribute('uv', new THREE.Float32BufferAttribute(dUv, 2));
      pedGeo.setIndex(dIdx);
      pedGeo.computeVertexNormals();
      // Force outer ring normals to pure +Y so lighting matches vista ground 100% seamlessly
      const pedNrm = pedGeo.attributes.normal;
      const outerStart = 1 + (rings - 1) * sectors;
      for (let i = outerStart; i < pedNrm.count; i++) {
        pedNrm.setXYZ(i, 0, 1, 0);
      }
      const pedMat = new THREE.MeshStandardMaterial({
        map: createPedestalTexture(maxAniso),
        roughness: 0.96,
        metalness: 0.0,
      });
      const pedestal = new THREE.Mesh(pedGeo, pedMat);
      pedestal.position.set(0, groundY, 0);
      pedestal.receiveShadow = true;
      ctx.contentGroup.add(pedestal);

      // Wide sunlit landscape ground plane under horizon haze
      const vistaGeo = new THREE.CircleGeometry(700, 56);
      vistaGeo.rotateX(-Math.PI / 2);
      const vistaMat = new THREE.MeshStandardMaterial({
        color: '#4c5831',
        roughness: 0.96,
        metalness: 0.0,
      });
      const vista = new THREE.Mesh(vistaGeo, vistaMat);
      vista.position.set(0, groundY - 0.0685, 0);
      vista.receiveShadow = true;
      ctx.contentGroup.add(vista);

      // Grounding 5-blade grass tussocks & fallen species leaves around root flare (scaled for shrubs & trees)
      {
        const turfScale = Math.min(1.0, Math.max(0.45, treeHeight / 3.8));
        // 5-blade arched mini-tussock prototype (30 tris per tuft)
        const bPos: number[] = [];
        const bNrm: number[] = [];
        const bCol: number[] = [];
        const bIdx: number[] = [];
        const segs = 3;
        const bladesPerTuft = 5;
        for (let b = 0; b < bladesPerTuft; b++) {
          const bYaw = (b / bladesPerTuft) * Math.PI * 2 + 0.37 * b;
          const sinY = Math.sin(bYaw);
          const cosY = Math.cos(bYaw);
          const bScaleH = (0.78 + 0.11 * (b % 3)) * turfScale;
          const archOut = (0.14 + 0.03 * (b % 2)) * turfScale;
          const baseV = (bPos.length / 3);
          for (let s = 0; s <= segs; s++) {
            const t = s / segs;
            const hw = 0.015 * turfScale * (1 - 0.88 * t * t);
            const ly = 0.28 * bScaleH * (t - 0.18 * t * t);
            const lz = archOut * t * t + 0.018 * (1 - t);
            const lx0 = -hw * cosY + lz * sinY;
            const lz0 = hw * sinY + lz * cosY;
            const lx1 = hw * cosY + lz * sinY;
            const lz1 = -hw * sinY + lz * cosY;
            bPos.push(lx0, ly, lz0, lx1, ly, lz1);
            bNrm.push(-0.25 * cosY, 0.90, 0.25 * sinY, 0.25 * cosY, 0.90, -0.25 * sinY);
            const rCol = 0.19 + 0.34 * t;
            const gCol = 0.30 + 0.40 * t;
            const bC = 0.11 + 0.14 * t;
            bCol.push(rCol, gCol, bC, rCol, gCol, bC);
            if (s < segs) {
              const k = baseV + s * 2;
              bIdx.push(k, k + 1, k + 3, k, k + 3, k + 2);
            }
          }
        }
        const bladeGeo = new THREE.BufferGeometry();
        bladeGeo.setAttribute('position', new THREE.Float32BufferAttribute(bPos, 3));
        bladeGeo.setAttribute('normal', new THREE.Float32BufferAttribute(bNrm, 3));
        bladeGeo.setAttribute('color', new THREE.Float32BufferAttribute(bCol, 3));
        bladeGeo.setIndex(bIdx);

        const bladeMat = new THREE.MeshStandardMaterial({
          vertexColors: true,
          side: THREE.DoubleSide,
          roughness: 0.74,
          metalness: 0.0,
        });
        bladeMat.onBeforeCompile = (shader) => {
          shader.uniforms.uTime = shared.uTime;
          shader.uniforms.uWindStrength = shared.uWindStrength;
          shader.vertexShader = shader.vertexShader
            .replace(
              '#include <common>',
              `#include <common>
              uniform float uTime;
              uniform float uWindStrength;`,
            )
            .replace(
              '#include <begin_vertex>',
              `#include <begin_vertex>
              float tipT = clamp(transformed.y * 3.2, 0.0, 1.0);
              #ifdef USE_INSTANCING
              vec3 bOrig = vec3(instanceMatrix[3][0], instanceMatrix[3][1], instanceMatrix[3][2]);
              #else
              vec3 bOrig = vec3(0.0);
              #endif
              float gSway = sin(uTime * 2.1 + bOrig.x * 0.9 + bOrig.z * 0.8) * 0.065 * tipT * tipT * uWindStrength;
              transformed.x += gSway;
              transformed.z += gSway * 0.7;`,
            );
        };

        const tuftCount = 105;
        const grassInst = new THREE.InstancedMesh(bladeGeo, bladeMat, tuftCount);
        const dummyG = new THREE.Object3D();
        const innerR = Math.max(0.24, Math.min(0.72, Math.max(size.x, size.z) * 0.052));
        let placedTufts = 0;
        for (let i = 0; i < tuftCount; i++) {
          const h1 = hash2d(i * 7 + 11, 31);
          const h2 = hash2d(i * 13 + 5, 97);
          const h3 = hash2d(i * 19 + 3, 53);
          const ang = h1 * Math.PI * 2;
          const dist = innerR + Math.pow(h2, 0.72) * (pedRadius * 0.78 - innerR);
          const gx = Math.cos(ang) * dist;
          const gz = Math.sin(ang) * dist;
          if (
            gx > footMinX - 0.12 &&
            gx < footMaxX + 0.12 &&
            gz > footMinZ - 0.12 &&
            gz < footMaxZ + 0.12
          ) {
            continue;
          }
          dummyG.position.set(gx, groundY + moundHeightAt(gx, gz) - 0.012, gz);
          dummyG.rotation.set(0, h3 * Math.PI * 2, 0);
          const sH = (0.72 + 0.68 * h1) * (1 - 0.35 * (dist / pedRadius));
          dummyG.scale.set(0.88 + 0.36 * h2, Math.max(0.38, sH), 0.88 + 0.36 * h2);
          dummyG.updateMatrix();
          grassInst.setMatrixAt(placedTufts++, dummyG.matrix);
        }
        grassInst.count = placedTufts;
        grassInst.instanceMatrix.needsUpdate = true;
        grassInst.receiveShadow = true;
        ctx.contentGroup.add(grassInst);

        // Fallen leaf carpet under full-scale deciduous / broadleaf / needle crowns (skip manicured hedges & planters)
        if (maps?.leaf_card.rgba && layers.leaves && size.y >= 2.4 && groundY >= -0.02) {
          const flGeo = new THREE.PlaneGeometry(0.28, 0.28);
          flGeo.rotateX(-Math.PI / 2);
          const flMat = new THREE.MeshStandardMaterial({
            map: createDataTexture(maps.leaf_card.rgba, true, false, maxAniso),
            alphaTest: 0.36,
            side: THREE.DoubleSide,
            roughness: 0.82,
            metalness: 0.0,
          });
          const fallenCount = 48;
          const fallenInst = new THREE.InstancedMesh(flGeo, flMat, fallenCount);
          const flCol = new THREE.Color();
          for (let i = 0; i < fallenCount; i++) {
            const h1 = hash2d(i * 23 + 17, 71);
            const h2 = hash2d(i * 29 + 9, 113);
            const h3 = hash2d(i * 37 + 3, 149);
            const ang = h1 * Math.PI * 2;
            const dist = innerR * 1.15 + Math.pow(h2, 0.82) * (pedRadius * 0.74 - innerR);
            const lx = Math.cos(ang) * dist;
            const lz = Math.sin(ang) * dist;
            dummyG.position.set(lx, moundHeightAt(lx, lz) + 0.012, lz);
            dummyG.rotation.set((h1 - 0.5) * 0.16, h3 * Math.PI * 2, (h2 - 0.5) * 0.16);
            const sc = (0.75 + 0.65 * h3) * turfScale;
            dummyG.scale.set(sc, sc, sc);
            dummyG.updateMatrix();
            fallenInst.setMatrixAt(i, dummyG.matrix);
            const warm = 0.82 + 0.24 * h1;
            flCol.setRGB(warm * 1.04, warm * 0.96, warm * 0.86);
            fallenInst.setColorAt(i, flCol);
          }
          fallenInst.instanceMatrix.needsUpdate = true;
          if (fallenInst.instanceColor) fallenInst.instanceColor.needsUpdate = true;
          fallenInst.receiveShadow = true;
          ctx.contentGroup.add(fallenInst);
        }
      }

      const ranges =
        lod.submeshes.length > 0
          ? lod.submeshes
          : [{ index_start: 0, index_count: lod.indices.length, material: 'bark' as const }];

      for (const sub of ranges) {
        if (sub.index_count === 0) continue;
        const isBark = sub.material === 'bark' || sub.material === 'upper_bark';
        const isUpperBark = sub.material === 'upper_bark';
        if (isBark && !layers.bark) continue;
        if (!isBark && !layers.leaves) continue;

        const subIndices = Uint32Array.from(
          lod.indices.slice(sub.index_start, sub.index_start + sub.index_count),
        );
        const geo = new THREE.BufferGeometry();
        geo.setAttribute('position', new THREE.BufferAttribute(positions, 3));
        geo.setAttribute('normal', new THREE.BufferAttribute(normals, 3));
        if (uvs) geo.setAttribute('uv', new THREE.BufferAttribute(uvs, 2));
        geo.setIndex(new THREE.BufferAttribute(subIndices, 1));

        if (isBark) {
          const lastIdx = subIndices.length > 0 ? subIndices[subIndices.length - 1] : 0;
          const hasBarkTintAo =
            rawColors.length === vCount * 4 &&
            rawColors[lastIdx * 4 + 3] > 0.95 &&
            rawColors[lastIdx * 4 + 1] > 0.35;
          if (hasBarkTintAo) {
            const rgb = new Float32Array(vCount * 3);
            for (let v = 0; v < vCount; v++) {
              rgb[v * 3] = rawColors[v * 4];
              rgb[v * 3 + 1] = rawColors[v * 4 + 1];
              rgb[v * 3 + 2] = rawColors[v * 4 + 2];
            }
            geo.setAttribute('color', new THREE.BufferAttribute(rgb, 3));
          }

          const rawUv2 = lod.vertices.uv2s;
          const barkUv2 =
            rawUv2 && rawUv2.length === vCount * 2
              ? Float32Array.from(rawUv2)
              : new Float32Array(vCount * 2);
          geo.setAttribute('aBarkUv2', new THREE.BufferAttribute(barkUv2, 2));

          const barkAlbedoMap = isUpperBark ? maps?.upper_bark_albedo ?? maps?.bark_albedo : maps?.bark_albedo;
          const barkNormalMap = isUpperBark ? maps?.upper_bark_normal ?? maps?.bark_normal : maps?.bark_normal;
          const barkMat = new THREE.MeshStandardMaterial({
            color: barkAlbedoMap ? '#ffffff' : isUpperBark ? '#b8b0a2' : '#6c5138',
            vertexColors: hasBarkTintAo,
            roughness: isUpperBark ? 0.78 : 0.88,
            metalness: 0.0,
          });
          if (barkAlbedoMap) {
            barkMat.map = createDataTexture(barkAlbedoMap.rgba, true, true, maxAniso);
          }
          if (barkNormalMap) {
            barkMat.normalMap = createDataTexture(barkNormalMap.rgba, false, true, maxAniso);
            barkMat.normalScale = new THREE.Vector2(isUpperBark ? 1.1 : 1.55, isUpperBark ? 1.1 : 1.55);
          }
          barkMat.onBeforeCompile = (shader) => {
            shader.uniforms.uTime = shared.uTime;
            shader.uniforms.uWindStrength = shared.uWindStrength;
            shader.uniforms.uTreeHeight = { value: treeHeight };
            shader.uniforms.uTreeMinY = { value: minY };
            shader.vertexShader = shader.vertexShader
              .replace(
                '#include <common>',
                `#include <common>
                attribute vec2 aBarkUv2;
                uniform float uTime;
                uniform float uWindStrength;
                uniform float uTreeHeight;
                uniform float uTreeMinY;
                varying float vBarkRootAo;
                varying float vBarkDepth;
                varying vec3 vBarkPos;
                varying vec3 vBarkObjNormal;`,
              )
              .replace(
                '#include <begin_vertex>',
                `#include <begin_vertex>
                vBarkPos = transformed;
                vBarkObjNormal = normalize(normal);
                vBarkDepth = aBarkUv2.x;
                float hNorm = clamp((transformed.y - uTreeMinY) / max(uTreeHeight, 0.5), 0.0, 1.0);
                vBarkRootAo = mix(0.82, 1.0, smoothstep(0.0, 0.12, hNorm));
                float branchFlex = mix(0.35, 1.45, smoothstep(0.0, 0.75, aBarkUv2.x));
                float boughSway = sin(uTime * 1.35 + aBarkUv2.y * 6.283 + transformed.x * 0.25 + transformed.z * 0.22)
                  * 0.068 * pow(hNorm, 1.65) * branchFlex * uWindStrength;
                transformed.x += boughSway;
                transformed.z += boughSway * 0.6;`,
              );
            shader.fragmentShader = shader.fragmentShader
              .replace(
                '#include <common>',
                `#include <common>
                uniform float uTreeMinY;
                varying float vBarkRootAo;
                varying float vBarkDepth;
                varying vec3 vBarkPos;
                varying vec3 vBarkObjNormal;`,
              )
              .replace(
                '#include <color_fragment>',
                `#include <color_fragment>
                float barkGrain = sin(vBarkPos.y * 95.0 + sin(vBarkPos.x * 42.0 + vBarkPos.z * 42.0) * 2.2) * 0.5 + 0.5;
                float grainAmp = mix(0.09, 0.03, smoothstep(0.25, 0.70, vBarkDepth));
                diffuseColor.rgb *= (1.0 - grainAmp * 0.5 + grainAmp * barkGrain) * vBarkRootAo;
                float collarY = vBarkPos.y - uTreeMinY;
                float mossNoise = sin(vBarkPos.x * 19.0 + vBarkPos.z * 23.0) * cos(vBarkPos.y * 17.0 - vBarkPos.x * 13.0);
                float rootMoss = (1.0 - smoothstep(0.04, 0.48, collarY))
                  * smoothstep(-0.10, 0.55, vBarkObjNormal.y + mossNoise * 0.36)
                  * (1.0 - smoothstep(0.15, 0.45, vBarkDepth))
                  * 0.48;
                float boughMoss = (1.0 - smoothstep(0.22, 0.48, vBarkDepth))
                  * smoothstep(0.66, 0.94, vBarkObjNormal.y + mossNoise * 0.20)
                  * smoothstep(0.6, 2.5, collarY)
                  * 0.10;
                diffuseColor.rgb = mix(diffuseColor.rgb, vec3(0.15, 0.25, 0.09), clamp(rootMoss + boughMoss, 0.0, 0.50));`,
              )
              .replace(
                '#include <normal_fragment_maps>',
                `#ifdef USE_NORMALMAP_TANGENTSPACE
                vec3 mapN = texture2D( normalMap, vNormalMapUv ).xyz * 2.0 - 1.0;
                float hierarchyRelief = mix(1.0, 0.28, smoothstep(0.20, 0.72, vBarkDepth));
                mapN.xy *= normalScale * hierarchyRelief;
                normal = normalize( tbn * mapN );
                #else
                #include <normal_fragment_maps>
                #endif`,
              )
              .replace(
                '#include <roughnessmap_fragment>',
                `#include <roughnessmap_fragment>
                float barkLum = dot(diffuseColor.rgb, vec3(0.299, 0.587, 0.114));
                float twigSmooth = smoothstep(0.35, 0.75, vBarkDepth) * 0.12;
                roughnessFactor = clamp(mix(0.96, 0.76, smoothstep(0.06, 0.38, barkLum)) - twigSmooth, 0.62, 0.98);`,
              )
              .replace(
                '#include <opaque_fragment>',
                `outgoingLight += diffuseColor.rgb * 0.095;
                #include <opaque_fragment>`,
              );
          };
          const mesh = new THREE.Mesh(geo, barkMat);
          mesh.castShadow = true;
          mesh.receiveShadow = true;
          ctx.contentGroup.add(mesh);
        } else {
          const sampleIdx = subIndices[0] ?? 0;
          const hasTintAo =
            (sub.material === 'leaves' || sub.material === 'organ') &&
            rawColors.length === vCount * 4 &&
            rawColors[sampleIdx * 4 + 3] > 0.5;

          if (hasTintAo) {
            const rgb = new Float32Array(vCount * 3);
            for (let v = 0; v < vCount; v++) {
              rgb[v * 3] = rawColors[v * 4];
              rgb[v * 3 + 1] = rawColors[v * 4 + 1];
              rgb[v * 3 + 2] = rawColors[v * 4 + 2];
            }
            geo.setAttribute('color', new THREE.BufferAttribute(rgb, 3));
          }

          const isImpostor = sub.material === 'impostor';
          const isOrgan = sub.material === 'organ';
          const mapSource = isImpostor
            ? lod.impostor_atlas?.rgba
            : isOrgan
              ? maps?.organ_albedo?.rgba ?? maps?.leaf_card.rgba
              : maps?.leaf_card.rgba;
          const leafMat = new THREE.MeshStandardMaterial({
            color: mapSource ? '#ffffff' : isOrgan ? '#d48b3b' : '#4c8a3d',
            vertexColors: hasTintAo,
            alphaTest: mapSource ? 0.28 : 0.0,
            alphaToCoverage: Boolean(mapSource),
            side: THREE.DoubleSide,
            roughness: isOrgan ? 0.66 : 0.84,
            metalness: 0.0,
          });
          if (mapSource) {
            leafMat.map = createDataTexture(mapSource, true, false, maxAniso);
          }

          leafMat.onBeforeCompile = (shader) => {
            shader.uniforms.uTime = shared.uTime;
            shader.uniforms.uWindStrength = shared.uWindStrength;
            shader.uniforms.uTreeHeight = { value: treeHeight };
            shader.uniforms.uTreeMinY = { value: minY };
            shader.uniforms.uSunDir = shared.uSunDir;
            shader.uniforms.uSubsurfaceColor = shared.uSubsurfaceColor;
            shader.uniforms.uSubsurfaceIntensity = shared.uSubsurfaceIntensity;
            shader.uniforms.uKeepCanopyNormal = { value: isImpostor ? 0.0 : 0.82 };

            shader.vertexShader = shader.vertexShader
              .replace(
                '#include <common>',
                `#include <common>
                uniform float uTime;
                uniform float uWindStrength;
                uniform float uTreeHeight;
                uniform float uTreeMinY;`,
              )
              .replace(
                '#include <begin_vertex>',
                `#include <begin_vertex>
                float hNorm = clamp((transformed.y - uTreeMinY) / max(uTreeHeight, 0.5), 0.0, 1.0);
                float boughSway = sin(uTime * 1.35 + transformed.x * 0.25 + transformed.z * 0.22) * 0.072 * pow(hNorm, 1.75) * uWindStrength;
                #if defined( USE_MAP ) || defined( USE_ALPHAMAP )
                float tipFlex = clamp(1.0 - uv.y, 0.0, 1.0);
                #else
                float tipFlex = 0.5;
                #endif
                float flutter = sin(uTime * 3.4 + transformed.x * 2.8 + transformed.y * 2.2 + transformed.z * 2.6) * 0.025 * tipFlex * uWindStrength;
                transformed.x += boughSway + flutter;
                transformed.y += flutter * 0.36;
                transformed.z += boughSway * 0.6 - flutter * 0.72;`,
              );

            shader.fragmentShader = shader.fragmentShader
              .replace(
                '#include <common>',
                `#include <common>
                uniform vec3 uSunDir;
                uniform vec3 uSubsurfaceColor;
                uniform float uSubsurfaceIntensity;
                uniform float uKeepCanopyNormal;`,
              )
              .replace(
                '#include <normal_fragment_maps>',
                `#include <normal_fragment_maps>
                if (uKeepCanopyNormal > 0.0) {
                  vec3 canopyOutward = normalize(vNormal);
                  float leafLum = dot(diffuseColor.rgb, vec3(0.299, 0.587, 0.114));
                  vec3 vpDx = dFdx(vViewPosition);
                  vec3 vpDy = dFdy(vViewPosition);
                  vec3 veinBump = (vpDx * dFdx(leafLum) + vpDy * dFdy(leafLum)) * 1.6;
                  normal = normalize(mix(normal, canopyOutward, uKeepCanopyNormal) + veinBump);
                  vec3 viewDirClamp = normalize(vViewPosition);
                  float nvClamp = dot(normal, viewDirClamp);
                  if (nvClamp < 0.30) {
                    normal = normalize(normal + viewDirClamp * (0.30 - nvClamp));
                  }
                }`,
              )
              .replace(
                '#include <opaque_fragment>',
                `vec3 sssViewDir = normalize(vViewPosition);
                vec3 sunViewDir = normalize((viewMatrix * vec4(uSunDir, 0.0)).xyz);
                float backLit = pow(clamp(dot(-sssViewDir, sunViewDir), 0.0, 1.0), 2.2);
                float wrapDiff = clamp(dot(normal, sunViewDir) * 0.5 + 0.5, 0.0, 1.0);
                outgoingLight += diffuseColor.rgb * uSubsurfaceColor * (0.24 + 0.78 * backLit) * uSubsurfaceIntensity;
                outgoingLight += diffuseColor.rgb * (0.10 * wrapDiff);
                #include <opaque_fragment>`,
              );
          };

          const mesh = new THREE.Mesh(geo, leafMat);
          mesh.castShadow = true;
          mesh.receiveShadow = true;
          ctx.contentGroup.add(mesh);
        }
      }

      const prevWasNature = ctx.sceneBounds.isNature;
      const prevTopdown = ctx.sceneBounds.isTopdown;
      ctx.sceneBounds = {
        center,
        size,
        isNature: false,
        tileSize: 16,
        isTopdown,
      };
      if (prevWasNature || prevTopdown !== isTopdown) {
        applyCameraPreset(cameraPreset);
      }
      return;
    }

    // NATURE PATCH MODE
    if (!naturePreview) return;

    const tileSize = Math.max(8, naturePreview.stats.tile_size);
    const tVerts = naturePreview.terrain.vertices;
    const tPos = Float32Array.from(tVerts.positions);
    const tNrm = Float32Array.from(tVerts.normals);
    const tUv = Float32Array.from(tVerts.uvs);
    const tIdx = Uint32Array.from(naturePreview.terrain.indices);

    const tBox = new THREE.Box3();
    for (let i = 0; i < tPos.length; i += 3) {
      tBox.expandByPoint(new THREE.Vector3(tPos[i], tPos[i + 1], tPos[i + 2]));
    }
    const center = tBox.getCenter(new THREE.Vector3());
    const size = tBox.getSize(new THREE.Vector3());

    // Configure directional shadow frustum across the patch
    const shadowHalf = tileSize * 0.68;
    ctx.sunLight.shadow.camera.left = -shadowHalf;
    ctx.sunLight.shadow.camera.right = shadowHalf;
    ctx.sunLight.shadow.camera.top = shadowHalf;
    ctx.sunLight.shadow.camera.bottom = -shadowHalf;
    ctx.sunLight.shadow.camera.near = 1.0;
    ctx.sunLight.shadow.camera.far = tileSize * 4.2;
    ctx.sunLight.shadow.camera.updateProjectionMatrix();
    ctx.sunLight.position.copy(center).addScaledVector(shared.uSunDir.value, tileSize * 1.65);
    ctx.sunLight.target.position.copy(center);
    ctx.sunLight.target.updateMatrixWorld();

    // Distant horizon skirt matching biome palette so patch blends into vast rolling hills
    const biome = (naturePreview.biome ?? '').toLowerCase();
    const vistaColor = biome.includes('dune')
      ? '#c8be9f'
      : biome.includes('banksia')
        ? '#a7a194'
        : biome.includes('jarrah')
          ? '#6f4531'
          : biome.includes('wetland')
            ? '#38332b'
            : biome.includes('verge')
              ? '#5d6b3b'
              : biome.includes('arid')
                ? '#7c6242'
                : biome.includes('forest')
                  ? '#313820'
                  : '#48562b';
    const skirtGeo = new THREE.CircleGeometry(700, 56);
    skirtGeo.rotateX(-Math.PI / 2);
    const skirtMat = new THREE.MeshStandardMaterial({
      color: vistaColor,
      roughness: 0.98,
      metalness: 0.0,
    });
    const skirt = new THREE.Mesh(skirtGeo, skirtMat);
    skirt.position.set(0, tBox.min.y - 0.04, 0);
    skirt.receiveShadow = true;
    ctx.contentGroup.add(skirt);

    // Normal-mapped & biome-shaded terrain mesh
    const terrainGeo = new THREE.BufferGeometry();
    terrainGeo.setAttribute('position', new THREE.BufferAttribute(tPos, 3));
    terrainGeo.setAttribute('normal', new THREE.BufferAttribute(tNrm, 3));
    if (tUv.length > 0) terrainGeo.setAttribute('uv', new THREE.BufferAttribute(tUv, 2));
    terrainGeo.setIndex(new THREE.BufferAttribute(tIdx, 1));

    const terrainMat = new THREE.MeshStandardMaterial({
      color: naturePreview.terrain_albedo ? '#ffffff' : vistaColor,
      roughness: 0.94,
      metalness: 0.0,
    });
    if (naturePreview.terrain_albedo) {
      terrainMat.map = createDataTexture(naturePreview.terrain_albedo, true, false, maxAniso);
    }
    if (naturePreview.terrain_normal) {
      terrainMat.normalMap = createDataTexture(
        naturePreview.terrain_normal,
        false,
        false,
        maxAniso,
      );
      terrainMat.normalScale = new THREE.Vector2(1.3, 1.3);
    }
    const terrainMesh = new THREE.Mesh(terrainGeo, terrainMat);
    terrainMesh.receiveShadow = true;
    ctx.contentGroup.add(terrainMesh);

    // Wind parameters from NaturePatch
    const windCfg = naturePreview.wind ?? {
      direction_degrees: 38,
      strength: 0.75,
      speed: 2.1,
      gust_scale: 0.5,
      flutter: 0.75,
    };
    const windRad = THREE.MathUtils.degToRad(windCfg.direction_degrees);
    const windDirVec = new THREE.Vector2(Math.sin(windRad), Math.cos(windRad)).normalize();
    const windAmp = Math.max(0.2, windCfg.strength);
    const windSpeed = Math.max(0.6, windCfg.speed * 1.15);

    const dummy = new THREE.Object3D();
    const instCol = new THREE.Color();

    for (const set of naturePreview.scatter_sets) {
      const proto =
        naturePreview.prototypes[set.layer_index] ??
        naturePreview.prototypes.find((p) => p.name === set.layer_name || p.kind === set.kind);
      const lod0 = proto?.lods[0] ?? proto?.lods.at(-1);
      if (!lod0 || lod0.vertices.positions.length < 9) continue;

      const instances: ScatterInstance[] = [];
      for (const chunk of set.chunks) {
        for (const inst of chunk.instances) {
          instances.push(inst);
        }
      }
      if (instances.length === 0) continue;

      const pPos = Float32Array.from(lod0.vertices.positions);
      const pNrm = Float32Array.from(lod0.vertices.normals);
      const pUv = lod0.vertices.uvs.length > 0 ? Float32Array.from(lod0.vertices.uvs) : null;
      const pIdx = Uint32Array.from(lod0.indices);
      const pvCount = pPos.length / 3;

      const foliageData = new Float32Array(pvCount * 4);
      if (lod0.vertices.colors.length === pvCount * 4) {
        foliageData.set(lod0.vertices.colors);
      } else {
        for (let i = 0; i < pvCount; i++) {
          foliageData[i * 4] = 0.5;
          foliageData[i * 4 + 1] = 0.5;
          foliageData[i * 4 + 2] = Math.min(1, Math.max(0, pPos[i * 3 + 1] * 2));
          foliageData[i * 4 + 3] = 1 - foliageData[i * 4 + 2];
        }
      }

      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(pPos, 3));
      geo.setAttribute('normal', new THREE.BufferAttribute(pNrm, 3));
      if (pUv) geo.setAttribute('uv', new THREE.BufferAttribute(pUv, 2));
      geo.setAttribute('aFoliageData', new THREE.BufferAttribute(foliageData, 4));
      geo.setIndex(new THREE.BufferAttribute(pIdx, 1));

      const useAtlas = Boolean(proto?.uses_card_atlas && naturePreview.card_atlas);
      const kind = set.kind;
      const isRigid = kind === 'rock' || kind === 'log' || kind === 'litter';
      const baseHex = proto?.color_base ?? '#2f451c';
      const tipHex = proto?.color_tip ?? '#9ec052';

      const mat = new THREE.MeshStandardMaterial({
        color: '#ffffff',
        alphaTest: useAtlas ? 0.36 : 0.0,
        side: isRigid && kind !== 'litter' ? THREE.FrontSide : THREE.DoubleSide,
        roughness: kind === 'rock' ? 0.88 : kind === 'moss' ? 0.92 : 0.68,
        metalness: 0.0,
      });
      if (useAtlas && naturePreview.card_atlas) {
        mat.map = createDataTexture(naturePreview.card_atlas.rgba, true, false, maxAniso);
      }

      mat.onBeforeCompile = (shader) => {
        shader.uniforms.uTime = shared.uTime;
        shader.uniforms.uWindStrength = shared.uWindStrength;
        shader.uniforms.uWindAmp = { value: isRigid ? 0.0 : windAmp };
        shader.uniforms.uWindSpeed = { value: windSpeed };
        shader.uniforms.uWindDir = { value: windDirVec };
        shader.uniforms.uBaseColor = { value: new THREE.Color(baseHex) };
        shader.uniforms.uTipColor = { value: new THREE.Color(tipHex) };
        shader.uniforms.uStemColor = { value: new THREE.Color('#4c7a34') };
        shader.uniforms.uUseGradient = { value: useAtlas ? 0.0 : 1.0 };
        shader.uniforms.uIsFlower = { value: !useAtlas && kind === 'flower' ? 1.0 : 0.0 };
        shader.uniforms.uIsRigid = { value: isRigid ? 1.0 : 0.0 };
        shader.uniforms.uSunDir = shared.uSunDir;
        shader.uniforms.uSubsurfaceColor = shared.uSubsurfaceColor;
        shader.uniforms.uSubsurfaceIntensity = shared.uSubsurfaceIntensity;

        shader.vertexShader = shader.vertexShader
          .replace(
            '#include <common>',
            `#include <common>
            attribute vec4 aFoliageData;
            varying vec4 vFoliageData;
            varying float vGustSheen;
            uniform float uTime;
            uniform float uWindStrength;
            uniform float uWindAmp;
            uniform float uWindSpeed;
            uniform vec2 uWindDir;`,
          )
          .replace(
            '#include <begin_vertex>',
            `#include <begin_vertex>
            vFoliageData = aFoliageData;
            float flex = pow(clamp(1.0 - aFoliageData.w, 0.0, 1.0), 1.38);
            #ifdef USE_INSTANCING
            vec3 instOrigin = vec3(instanceMatrix[3][0], instanceMatrix[3][1], instanceMatrix[3][2]);
            vec2 localWind = normalize(vec2(
              dot(vec2(instanceMatrix[0][0], instanceMatrix[0][2]), uWindDir),
              dot(vec2(instanceMatrix[2][0], instanceMatrix[2][2]), uWindDir)
            ) + vec2(0.0001, 0.0));
            #else
            vec3 instOrigin = vec3(0.0);
            vec2 localWind = uWindDir;
            #endif
            float waveCoord = dot(instOrigin.xz, uWindDir) * 0.25 - uTime * uWindSpeed;
            float macroWave = sin(waveCoord) * 0.65 + sin(waveCoord * 1.95 + instOrigin.x * 0.29 - instOrigin.z * 0.31) * 0.35;
            float gust = macroWave * 0.5 + 0.5;
            float flutter = sin(uTime * 4.5 + instOrigin.x * 3.7 + instOrigin.z * 4.1 + aFoliageData.y * 6.283) * 0.12;
            float bend = (0.08 + 0.30 * gust + flutter * 0.24) * flex * uWindAmp * uWindStrength;
            transformed.x += localWind.x * bend;
            transformed.z += localWind.y * bend;
            transformed.y -= abs(bend) * 0.20 * flex;
            vGustSheen = gust * flex * uWindStrength;`,
          );

        shader.fragmentShader = shader.fragmentShader
          .replace(
            '#include <common>',
            `#include <common>
            varying vec4 vFoliageData;
            varying float vGustSheen;
            uniform vec3 uBaseColor;
            uniform vec3 uTipColor;
            uniform vec3 uStemColor;
            uniform float uUseGradient;
            uniform float uIsFlower;
            uniform float uIsRigid;
            uniform vec3 uSunDir;
            uniform vec3 uSubsurfaceColor;
            uniform float uSubsurfaceIntensity;`,
          )
          .replace(
            '#include <color_fragment>',
            `#include <color_fragment>
            float ct = clamp(vFoliageData.z, 0.0, 1.0);
            if (uUseGradient > 0.5) {
              vec3 gradCol;
              if (uIsFlower > 0.5) {
                gradCol = ct < 0.42
                  ? mix(uBaseColor, uStemColor, clamp(ct / 0.42, 0.0, 1.0))
                  : mix(uTipColor * 0.88, uTipColor * 1.08, clamp((ct - 0.42) / 0.58, 0.0, 1.0));
              } else {
                gradCol = mix(uBaseColor, uTipColor, pow(ct, 0.82));
              }
              diffuseColor.rgb *= gradCol;
            }
            float rootAo = uIsRigid > 0.5
              ? mix(0.62, 1.0, smoothstep(0.0, 0.35, ct))
              : mix(0.45, 1.0, smoothstep(0.0, 0.46, ct));
            diffuseColor.rgb *= rootAo;`,
          )
          .replace(
            '#include <opaque_fragment>',
            `if (uIsRigid < 0.5) {
              vec3 sssViewDir = normalize(vViewPosition);
              vec3 sunViewDir = normalize((viewMatrix * vec4(uSunDir, 0.0)).xyz);
              float backLit = pow(clamp(dot(-sssViewDir, sunViewDir), 0.0, 1.0), 2.3);
              float tipMask = smoothstep(0.12, 0.92, clamp(vFoliageData.z, 0.0, 1.0));
              outgoingLight += diffuseColor.rgb * uSubsurfaceColor * (0.24 + 0.76 * backLit) * tipMask * uSubsurfaceIntensity;
              outgoingLight += diffuseColor.rgb * uSubsurfaceColor * (vGustSheen * 0.22 * tipMask);
            }
            #include <opaque_fragment>`,
          );
      };

      const halfTile = Math.max(4, tileSize * 0.5);
      const heroEyeX = -halfTile * 0.42;
      const heroEyeZ = halfTile * 0.68;
      const instMesh = new THREE.InstancedMesh(geo, mat, instances.length);
      for (let i = 0; i < instances.length; i++) {
        const inst = instances[i];
        let hMul = inst.height;
        if (kind === 'weed' || kind === 'shrub') {
          const dCam = Math.hypot(inst.position[0] - heroEyeX, inst.position[2] - heroEyeZ);
          if (dCam < 1.8) {
            const tClear = Math.max(0, Math.min(1, (dCam - 0.5) / 1.3));
            hMul *= 0.28 + 0.72 * (tClear * tClear * (3 - 2 * tClear));
          }
        }
        dummy.position.set(inst.position[0], inst.position[1], inst.position[2]);
        dummy.rotation.set(0, inst.yaw, 0);
        dummy.scale.set(inst.width, hMul, inst.width);
        dummy.updateMatrix();
        instMesh.setMatrixAt(i, dummy.matrix);

        // Subtle organic per-clump brightness & warm/cool variation
        const v = inst.color_variation - 0.5;
        instCol.setRGB(
          Math.min(1.18, Math.max(0.82, 1 + v * 0.22)),
          Math.min(1.16, Math.max(0.84, 1 + v * 0.16)),
          Math.min(1.14, Math.max(0.82, 1 - v * 0.14)),
        );
        instMesh.setColorAt(i, instCol);
      }
      instMesh.instanceMatrix.needsUpdate = true;
      if (instMesh.instanceColor) instMesh.instanceColor.needsUpdate = true;
      instMesh.castShadow = kind === 'rock' || kind === 'log' || kind === 'weed' || kind === 'shrub';
      instMesh.receiveShadow = true;
      ctx.contentGroup.add(instMesh);
    }

    const prevWasNature = ctx.sceneBounds.isNature;
    ctx.sceneBounds = {
      center,
      size,
      isNature: true,
      tileSize,
      isTopdown: false,
    };
    if (!prevWasNature) {
      applyCameraPreset(cameraPreset);
    }
  }, [mode, lod, maps, layers.bark, layers.leaves, isTopdown, naturePreview, applyCameraPreset, cameraPreset]);

  return (
    <div ref={hostRef} className="midori-foliage-viewport">
      <div className="midori-viewport-hud">
        <div className="midori-hud-group" role="group" aria-label="Lighting mood">
          {(['golden', 'day', 'mist'] as const).map((m) => (
            <button
              key={m}
              type="button"
              className={mood === m ? 'active' : ''}
              onClick={() => setMood(m)}
            >
              {MOODS[m].label}
            </button>
          ))}
        </div>
        <div className="midori-hud-group" role="group" aria-label="Camera angle">
          <button
            type="button"
            className={cameraPreset === 'hero' ? 'active' : ''}
            onClick={() => {
              setCameraPreset('hero');
              applyCameraPreset('hero');
            }}
          >
            {mode === 'nature' ? 'Field' : 'Hero'}
          </button>
          <button
            type="button"
            className={cameraPreset === 'overview' ? 'active' : ''}
            onClick={() => {
              setCameraPreset('overview');
              applyCameraPreset('overview');
            }}
          >
            Overview
          </button>
        </div>
        <div className="midori-hud-group" role="group" aria-label="Wind toggle">
          <button
            type="button"
            className={windOn ? 'active' : ''}
            onClick={() => setWindOn((v) => !v)}
          >
            {windOn ? 'Wind' : 'Still'}
          </button>
        </div>
      </div>
      {children}
    </div>
  );
}
