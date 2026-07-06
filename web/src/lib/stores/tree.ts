import { writable } from 'svelte/store';

export interface BranchParams {
  count: number;
  length: number;
  angle: number;
  rotation: number;
  gravity: number;
}

export interface TreeParams {
  // Species
  name: string;
  scientificName: string;

  // Trunk
  trunk: {
    height: number;
    radius: number;
    taper: number;
    curve: number;
    segments: number;
  };

  // Branches
  branches: {
    level1: BranchParams;
    level2: BranchParams;
  };

  // Crown
  crown: {
    shape: 'spherical' | 'conical' | 'hemispherical' | 'flame' | 'columnar';
    offset: number;
  };

  // Leaves
  leaves: {
    count: number;
    size: number;
    geometry: 'polygon' | 'cross_billboard' | 'billboard' | 'none';
  };
}

export interface GenerationStats {
  stem_count: number;
  branch_count: number;
  leaf_count: number;
  bounds_min: [number, number, number];
  bounds_max: [number, number, number];
}

export interface LodOutput {
  index: number;
  name: string;
  vertex_count: number;
  triangle_count: number;
  branch_count: number;
  leaf_count: number;
  screen_height: number;
  vertices: any;
  indices: number[];
  submeshes: Array<{
    start: number;
    count: number;
    material_type: number;
  }>;
}

export interface MeshOutput {
  lods: LodOutput[];
}

export type PreviewMode = 'tree' | 'nature';

export interface NatureMeshOutput {
  name: string;
  vertex_count: number;
  triangle_count: number;
  vertices: any;
  indices: number[];
  submeshes: Array<{
    start: number;
    count: number;
    material_type: number;
  }>;
}

export interface NaturePrototypeOutput {
  name: string;
  kind: GroundcoverKind;
  lods: NatureMeshOutput[];
}

export interface NatureScatterInstance {
  position: [number, number, number];
  yaw: number;
  height: number;
  width: number;
  phase: number;
  color_variation: number;
}

export interface NatureScatterChunk {
  chunk_x: number;
  chunk_z: number;
  bounds_min: [number, number, number];
  bounds_max: [number, number, number];
  instances: NatureScatterInstance[];
}

export interface NatureScatterSet {
  layer_index: number;
  layer_name: string;
  kind: GroundcoverKind;
  chunks: NatureScatterChunk[];
}

export interface NaturePreviewOutput {
  manifest: any;
  terrain: NatureMeshOutput;
  prototypes: NaturePrototypeOutput[];
  scatter_sets: NatureScatterSet[];
  stats: {
    tile_size: number;
    terrain_vertex_count: number;
    terrain_triangle_count: number;
    prototype_count: number;
    scatter_instance_count: number;
  };
}

export type GroundcoverKind = 'grass' | 'moss' | 'flower' | 'weed' | 'litter' | 'shrub' | 'rock' | 'log';

const groundcoverKinds: GroundcoverKind[] = ['grass', 'moss', 'flower', 'weed', 'litter', 'shrub', 'rock', 'log'];

export interface NatureLayerParams {
  kind: GroundcoverKind;
  name: string;
  density: number;
  coverage: number;
  patch_scale: number;
  patch_softness: number;
  height: number;
  width: number;
  curl: number;
  relief_scale: number;
  relief_strength: number;
  color_base: string;
  color_tip: string;
}

export interface NatureParams {
  asset: {
    name: string;
    units: string;
  };
  patch: {
    size: number;
    seed: number;
    biome: string;
    tags: string[];
  };
  soil: {
    profile: string;
    mound_scale: number;
    mound_height: number;
    mound_coverage: number;
    relief_scale: number;
    relief_strength: number;
    wetness_coverage: number;
    cracks: {
      enabled: boolean;
      amount: number;
      plate_density: number;
      channel_width: number;
      warp: number;
      depth: number;
    };
  };
  groundcover: {
    layers: NatureLayerParams[];
  };
  wind: {
    strength: number;
    speed: number;
    direction_degrees: number;
    gust_scale: number;
    flutter: number;
  };
  profiles: {
    mobile: NatureProfileParams;
    console: NatureProfileParams;
  };
}

export interface NatureProfileParams {
  tile_size: number;
  terrain_resolution: number;
  lod0_max_triangles: number;
  lod1_max_triangles: number;
  lod2_max_triangles: number;
  lod0_max_distance: number;
  lod1_max_distance: number;
  lod2_max_distance: number;
  material_slots: number;
  max_instances_per_tile: number;
  max_instances_per_chunk: number;
  density_scale: number;
  cull_start: number;
  cull_end: number;
  shadows: boolean;
  grass_collision: boolean;
  moss_collision: boolean;
}

export interface SpeciesMetadata {
  name: string;
  scientific: string;
  latin: string;
  biome: string;
  tags: string[];
  generator_family: string;
  material_bark: string;
  material_foliage: string;
  material_notes: string;
  control_group_count: number;
}

export interface ExportStatus {
  sizeBytes: number;
  seed: number;
  lodChain: string[];
  exportedAt: string;
}

interface TreeState {
  previewMode: PreviewMode;
  species: string;
  seed: number;
  meshData: MeshOutput | null;
  natureData: NaturePreviewOutput | null;
  generationStats: GenerationStats | null;
  metadata: SpeciesMetadata | null;
  exportStatus: ExportStatus | null;
  loading: boolean;
  error: string | null;
  params: TreeParams;
  natureParams: NatureParams;
}

const defaultParams: TreeParams = {
  name: 'Oak',
  scientificName: 'Quercus robur',
  trunk: {
    height: 5,
    radius: 0.3,
    taper: 0.7,
    curve: 15,
    segments: 8
  },
  branches: {
    level1: {
      count: 5,
      length: 3,
      angle: 45,
      rotation: 137,
      gravity: 0.2
    },
    level2: {
      count: 3,
      length: 1.5,
      angle: 30,
      rotation: 90,
      gravity: 0.3
    }
  },
  crown: {
    shape: 'spherical',
    offset: 0.6
  },
  leaves: {
    count: 5000,
    size: 0.1,
    geometry: 'cross_billboard'
  }
};

const defaultNatureParams: NatureParams = {
  asset: {
    name: 'Temperate Forest Floor',
    units: 'meters'
  },
  patch: {
    size: 16,
    seed: 42,
    biome: 'forest_floor',
    tags: ['temperate', 'grass', 'moss', 'damp_soil']
  },
  soil: {
    profile: 'loam',
    mound_scale: 0.12,
    mound_height: 0.55,
    mound_coverage: 1,
    relief_scale: 0.7,
    relief_strength: 0.6,
    wetness_coverage: 0.35,
    cracks: {
      enabled: false,
      amount: 0.75,
      plate_density: 0.9,
      channel_width: 0.06,
      warp: 0.2,
      depth: 0.7
    }
  },
  groundcover: {
    layers: [
      {
        kind: 'grass',
        name: 'fine meadow grass',
        density: 0.13,
        coverage: 0.62,
        patch_scale: 0.15,
        patch_softness: 0.251,
        height: 1.5,
        width: 0.049,
        curl: 1.14,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#33421b',
        color_tip: '#9bc24a'
      },
      {
        kind: 'moss',
        name: 'low forest moss',
        density: 0.13,
        coverage: 0.55,
        patch_scale: 0.14,
        patch_softness: 0.251,
        height: 0.14,
        width: 0.049,
        curl: 0,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#33421b',
        color_tip: '#9bc24a'
      },
      {
        kind: 'flower',
        name: 'woodland violet',
        density: 0.14,
        coverage: 0.35,
        patch_scale: 0.21,
        patch_softness: 0.28,
        height: 0.28,
        width: 0.045,
        curl: 0.1,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#2e4a2a',
        color_tip: '#b8a4ff'
      },
      {
        kind: 'weed',
        name: 'broadleaf weed',
        density: 0.14,
        coverage: 0.45,
        patch_scale: 0.17,
        patch_softness: 0.24,
        height: 0.45,
        width: 0.06,
        curl: 0.45,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#2f5528',
        color_tip: '#6fa85c'
      },
      {
        kind: 'litter',
        name: 'fallen leaf litter',
        density: 0.06,
        coverage: 0.38,
        patch_scale: 0.1,
        patch_softness: 0.2,
        height: 0.18,
        width: 0.09,
        curl: 0,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#5b3f24',
        color_tip: '#9b6d35'
      },
      {
        kind: 'shrub',
        name: 'young hazel shrub',
        density: 0.055,
        coverage: 0.3,
        patch_scale: 0.09,
        patch_softness: 0.25,
        height: 0.9,
        width: 0.16,
        curl: 0.35,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#263a1f',
        color_tip: '#6f9a4b'
      },
      {
        kind: 'rock',
        name: 'mossy field stones',
        density: 0.08,
        coverage: 0.28,
        patch_scale: 0.08,
        patch_softness: 0.22,
        height: 0.22,
        width: 0.32,
        curl: 0,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#4f5148',
        color_tip: '#8a8d7d'
      },
      {
        kind: 'log',
        name: 'fallen branch log',
        density: 0.065,
        coverage: 0.24,
        patch_scale: 0.07,
        patch_softness: 0.2,
        height: 1.2,
        width: 0.12,
        curl: 0,
        relief_scale: 0.9,
        relief_strength: 0.7,
        color_base: '#4a2f1d',
        color_tip: '#8a5b34'
      }
    ]
  },
  wind: {
    strength: 0.5,
    speed: 1.8,
    direction_degrees: 20,
    gust_scale: 0.35,
    flutter: 0.6
  },
  profiles: {
    mobile: {
      tile_size: 16,
      terrain_resolution: 64,
      lod0_max_triangles: 120,
      lod1_max_triangles: 40,
      lod2_max_triangles: 8,
      lod0_max_distance: 6,
      lod1_max_distance: 18,
      lod2_max_distance: 32,
      material_slots: 1,
      max_instances_per_tile: 512,
      max_instances_per_chunk: 96,
      density_scale: 0.65,
      cull_start: 18,
      cull_end: 32,
      shadows: false,
      grass_collision: false,
      moss_collision: false
    },
    console: {
      tile_size: 32,
      terrain_resolution: 128,
      lod0_max_triangles: 300,
      lod1_max_triangles: 100,
      lod2_max_triangles: 16,
      lod0_max_distance: 12,
      lod1_max_distance: 35,
      lod2_max_distance: 70,
      material_slots: 1,
      max_instances_per_tile: 2048,
      max_instances_per_chunk: 256,
      density_scale: 1,
      cull_start: 35,
      cull_end: 70,
      shadows: false,
      grass_collision: false,
      moss_collision: false
    }
  }
};

function cloneNatureParams(params: NatureParams): NatureParams {
  return {
    ...params,
    asset: { ...params.asset },
    patch: { ...params.patch, tags: [...params.patch.tags] },
    soil: { ...params.soil, cracks: { ...params.soil.cracks } },
    groundcover: {
      layers: params.groundcover.layers.map(layer => ({ ...layer }))
    },
    wind: { ...params.wind },
    profiles: {
      mobile: { ...params.profiles.mobile },
      console: { ...params.profiles.console }
    }
  };
}

function tomlString(value: string): string {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

function tomlArray(values: string[]): string {
  return `[${values.map(tomlString).join(', ')}]`;
}

function sectionBody(toml: string, section: string): string {
  const escaped = section.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = toml.match(new RegExp(`\\[${escaped}\\]([\\s\\S]*?)(?=\\n\\[|$)`));
  return match?.[1] ?? '';
}

function parseNumber(body: string, key: string, fallback: number): number {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = body.match(new RegExp(`^\\s*${escaped}\\s*=\\s*([-+0-9.eE]+)`, 'm'));
  if (!match) return fallback;
  const value = Number(match[1]);
  return Number.isFinite(value) ? value : fallback;
}

function parseBoolean(body: string, key: string, fallback: boolean): boolean {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = body.match(new RegExp(`^\\s*${escaped}\\s*=\\s*(true|false)`, 'im'));
  return match ? match[1].toLowerCase() === 'true' : fallback;
}

function parseString(body: string, key: string, fallback: string): string {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = body.match(new RegExp(`^\\s*${escaped}\\s*=\\s*"([^"]*)"`, 'm'));
  return match?.[1] ?? fallback;
}

function parseTags(body: string, fallback: string[]): string[] {
  const match = body.match(/^\s*tags\s*=\s*\[([^\]]*)\]/m);
  if (!match) return fallback;
  const tags = Array.from(match[1].matchAll(/"([^"]*)"/g)).map(item => item[1]);
  return tags.length > 0 ? tags : fallback;
}

function parseKind(value: string, fallback: GroundcoverKind): GroundcoverKind {
  return groundcoverKinds.includes(value as GroundcoverKind)
    ? (value as GroundcoverKind)
    : fallback;
}

function parseLayerBlocks(toml: string, fallbackLayers: NatureLayerParams[]): NatureLayerParams[] {
  const blocks = Array.from(
    toml.matchAll(/\[\[groundcover\.layers\]\]([\s\S]*?)(?=\n\[\[groundcover\.layers\]\]|\n\[[^\[]|$)/g)
  ).map(match => match[1]);
  if (blocks.length === 0) return fallbackLayers.map(layer => ({ ...layer }));

  return blocks.map((body, index) => {
    const fallback = fallbackLayers[index] ?? fallbackLayers[0];
    const kind = parseKind(parseString(body, 'kind', fallback.kind), fallback.kind);
    return {
      kind,
      name: parseString(body, 'name', fallback.name || kind),
      density: parseNumber(body, 'density', fallback.density),
      coverage: parseNumber(body, 'coverage', fallback.coverage),
      patch_scale: parseNumber(body, 'patch_scale', fallback.patch_scale),
      patch_softness: parseNumber(body, 'patch_softness', fallback.patch_softness),
      height: parseNumber(body, 'height', fallback.height),
      width: parseNumber(body, 'width', fallback.width),
      curl: parseNumber(body, 'curl', fallback.curl),
      relief_scale: parseNumber(body, 'relief_scale', fallback.relief_scale),
      relief_strength: parseNumber(body, 'relief_strength', fallback.relief_strength),
      color_base: parseString(body, 'color_base', fallback.color_base),
      color_tip: parseString(body, 'color_tip', fallback.color_tip)
    };
  });
}

function parseProfile(toml: string, section: string, fallback: NatureProfileParams): NatureProfileParams {
  const body = sectionBody(toml, section);
  return {
    tile_size: parseNumber(body, 'tile_size', fallback.tile_size),
    terrain_resolution: parseNumber(body, 'terrain_resolution', fallback.terrain_resolution),
    lod0_max_triangles: parseNumber(body, 'lod0_max_triangles', fallback.lod0_max_triangles),
    lod1_max_triangles: parseNumber(body, 'lod1_max_triangles', fallback.lod1_max_triangles),
    lod2_max_triangles: parseNumber(body, 'lod2_max_triangles', fallback.lod2_max_triangles),
    lod0_max_distance: parseNumber(body, 'lod0_max_distance', fallback.lod0_max_distance),
    lod1_max_distance: parseNumber(body, 'lod1_max_distance', fallback.lod1_max_distance),
    lod2_max_distance: parseNumber(body, 'lod2_max_distance', fallback.lod2_max_distance),
    material_slots: parseNumber(body, 'material_slots', fallback.material_slots),
    max_instances_per_tile: parseNumber(
      body,
      'max_instances_per_tile',
      fallback.max_instances_per_tile
    ),
    max_instances_per_chunk: parseNumber(
      body,
      'max_instances_per_chunk',
      fallback.max_instances_per_chunk
    ),
    density_scale: parseNumber(body, 'density_scale', fallback.density_scale),
    cull_start: parseNumber(body, 'cull_start', fallback.cull_start),
    cull_end: parseNumber(body, 'cull_end', fallback.cull_end),
    shadows: parseBoolean(body, 'shadows', fallback.shadows),
    grass_collision: parseBoolean(body, 'grass_collision', fallback.grass_collision),
    moss_collision: parseBoolean(body, 'moss_collision', fallback.moss_collision)
  };
}

function parseNatureToml(toml: string): NatureParams {
  const fallback = cloneNatureParams(defaultNatureParams);
  const asset = sectionBody(toml, 'asset');
  const patch = sectionBody(toml, 'patch');
  const soil = sectionBody(toml, 'soil');
  const cracks = sectionBody(toml, 'soil.cracks');
  const wind = sectionBody(toml, 'wind');

  return {
    asset: {
      name: parseString(asset, 'name', fallback.asset.name),
      units: parseString(asset, 'units', fallback.asset.units)
    },
    patch: {
      size: parseNumber(patch, 'size', fallback.patch.size),
      seed: parseNumber(patch, 'seed', fallback.patch.seed),
      biome: parseString(patch, 'biome', fallback.patch.biome),
      tags: parseTags(patch, fallback.patch.tags)
    },
    soil: {
      profile: parseString(soil, 'profile', fallback.soil.profile),
      mound_scale: parseNumber(soil, 'mound_scale', fallback.soil.mound_scale),
      mound_height: parseNumber(soil, 'mound_height', fallback.soil.mound_height),
      mound_coverage: parseNumber(soil, 'mound_coverage', fallback.soil.mound_coverage),
      relief_scale: parseNumber(soil, 'relief_scale', fallback.soil.relief_scale),
      relief_strength: parseNumber(soil, 'relief_strength', fallback.soil.relief_strength),
      wetness_coverage: parseNumber(soil, 'wetness_coverage', fallback.soil.wetness_coverage),
      cracks: {
        enabled: parseBoolean(cracks, 'enabled', fallback.soil.cracks.enabled),
        amount: parseNumber(cracks, 'amount', fallback.soil.cracks.amount),
        plate_density: parseNumber(cracks, 'plate_density', fallback.soil.cracks.plate_density),
        channel_width: parseNumber(cracks, 'channel_width', fallback.soil.cracks.channel_width),
        warp: parseNumber(cracks, 'warp', fallback.soil.cracks.warp),
        depth: parseNumber(cracks, 'depth', fallback.soil.cracks.depth)
      }
    },
    groundcover: {
      layers: parseLayerBlocks(toml, fallback.groundcover.layers)
    },
    wind: {
      strength: parseNumber(wind, 'strength', fallback.wind.strength),
      speed: parseNumber(wind, 'speed', fallback.wind.speed),
      direction_degrees: parseNumber(wind, 'direction_degrees', fallback.wind.direction_degrees),
      gust_scale: parseNumber(wind, 'gust_scale', fallback.wind.gust_scale),
      flutter: parseNumber(wind, 'flutter', fallback.wind.flutter)
    },
    profiles: {
      mobile: parseProfile(toml, 'profiles.mobile', fallback.profiles.mobile),
      console: parseProfile(toml, 'profiles.console', fallback.profiles.console)
    }
  };
}

function natureParamsToToml(params: NatureParams): string {
  const p = params;
  const layerToml = p.groundcover.layers.map(layer => `[[groundcover.layers]]
kind = ${tomlString(layer.kind)}
name = ${tomlString(layer.name)}
density = ${layer.density}
coverage = ${layer.coverage}
patch_scale = ${layer.patch_scale}
patch_softness = ${layer.patch_softness}
height = ${layer.height}
width = ${layer.width}
curl = ${layer.curl}
relief_scale = ${layer.relief_scale}
relief_strength = ${layer.relief_strength}
color_base = ${tomlString(layer.color_base)}
color_tip = ${tomlString(layer.color_tip)}
`).join('\n');

  return `[asset]
kind = "nature_patch"
name = ${tomlString(p.asset.name)}
units = ${tomlString(p.asset.units)}

[patch]
size = ${p.patch.size}
seed = ${Math.round(p.patch.seed)}
biome = ${tomlString(p.patch.biome)}
tags = ${tomlArray(p.patch.tags)}

[soil]
profile = ${tomlString(p.soil.profile)}
mound_scale = ${p.soil.mound_scale}
mound_height = ${p.soil.mound_height}
mound_coverage = ${p.soil.mound_coverage}
relief_scale = ${p.soil.relief_scale}
relief_strength = ${p.soil.relief_strength}
wetness_coverage = ${p.soil.wetness_coverage}

[soil.cracks]
enabled = ${p.soil.cracks.enabled}
amount = ${p.soil.cracks.amount}
plate_density = ${p.soil.cracks.plate_density}
channel_width = ${p.soil.cracks.channel_width}
warp = ${p.soil.cracks.warp}
depth = ${p.soil.cracks.depth}

${layerToml}
[wind]
strength = ${p.wind.strength}
speed = ${p.wind.speed}
direction_degrees = ${p.wind.direction_degrees}
gust_scale = ${p.wind.gust_scale}
flutter = ${p.wind.flutter}

[profiles.mobile]
tile_size = ${p.profiles.mobile.tile_size}
terrain_resolution = ${Math.round(p.profiles.mobile.terrain_resolution)}
lod0_max_triangles = ${Math.round(p.profiles.mobile.lod0_max_triangles)}
lod1_max_triangles = ${Math.round(p.profiles.mobile.lod1_max_triangles)}
lod2_max_triangles = ${Math.round(p.profiles.mobile.lod2_max_triangles)}
lod0_max_distance = ${p.profiles.mobile.lod0_max_distance}
lod1_max_distance = ${p.profiles.mobile.lod1_max_distance}
lod2_max_distance = ${p.profiles.mobile.lod2_max_distance}
material_slots = ${Math.round(p.profiles.mobile.material_slots)}
max_instances_per_tile = ${Math.round(p.profiles.mobile.max_instances_per_tile)}
max_instances_per_chunk = ${Math.round(p.profiles.mobile.max_instances_per_chunk)}
density_scale = ${p.profiles.mobile.density_scale}
cull_start = ${p.profiles.mobile.cull_start}
cull_end = ${p.profiles.mobile.cull_end}
shadows = ${p.profiles.mobile.shadows}
grass_collision = ${p.profiles.mobile.grass_collision}
moss_collision = ${p.profiles.mobile.moss_collision}

[profiles.console]
tile_size = ${p.profiles.console.tile_size}
terrain_resolution = ${Math.round(p.profiles.console.terrain_resolution)}
lod0_max_triangles = ${Math.round(p.profiles.console.lod0_max_triangles)}
lod1_max_triangles = ${Math.round(p.profiles.console.lod1_max_triangles)}
lod2_max_triangles = ${Math.round(p.profiles.console.lod2_max_triangles)}
lod0_max_distance = ${p.profiles.console.lod0_max_distance}
lod1_max_distance = ${p.profiles.console.lod1_max_distance}
lod2_max_distance = ${p.profiles.console.lod2_max_distance}
material_slots = ${Math.round(p.profiles.console.material_slots)}
max_instances_per_tile = ${Math.round(p.profiles.console.max_instances_per_tile)}
max_instances_per_chunk = ${Math.round(p.profiles.console.max_instances_per_chunk)}
density_scale = ${p.profiles.console.density_scale}
cull_start = ${p.profiles.console.cull_start}
cull_end = ${p.profiles.console.cull_end}
shadows = ${p.profiles.console.shadows}
grass_collision = ${p.profiles.console.grass_collision}
moss_collision = ${p.profiles.console.moss_collision}
`;
}

function createTreeStore() {
  const { subscribe, set, update } = writable<TreeState>({
    previewMode: 'tree',
    species: '',
    seed: 12345,
    meshData: null,
    natureData: null,
    generationStats: null,
    metadata: null,
    exportStatus: null,
    loading: false,
    error: null,
    params: { ...defaultParams },
    natureParams: cloneNatureParams(defaultNatureParams)
  });

  let generator: any = null;
  let natureGenerator: any = null;
  let wasmModule: any = null;
  const NATURE_PREVIEW_RESOLUTION = 48;
  const NATURE_SCATTER_CHUNK_SIZE = 8.0;

  return {
    subscribe,

    async init() {
      try {
        // Dynamic import of WASM from local build
        const wasm = await import('$lib/wasm/midori_wasm.js');
        await wasm.default();
        wasmModule = wasm;
        console.log('WASM loaded successfully');
      } catch (e) {
        console.error('Failed to load WASM:', e);
        update(s => ({ ...s, error: 'Failed to load WASM module' }));
      }
    },

    async loadSpecies(toml: string) {
      if (!wasmModule) {
        console.error('WASM module not loaded');
        return;
      }

      console.log('loadSpecies called with TOML length:', toml.length);
      console.log('TOML content:\n', toml);
      update(s => ({ ...s, loading: true, error: null }));

      try {
        console.log('Creating MidoriGenerator...');
        generator = new wasmModule.MidoriGenerator(toml);
        natureGenerator = null;
        console.log('MidoriGenerator created, name:', generator.name);
        const metadata = generator.metadata() as SpeciesMetadata;
        update(s => ({
          ...s,
          previewMode: 'tree',
          species: generator.name,
          metadata,
          natureData: null,
          exportStatus: null,
          loading: false,
          params: {
            ...s.params,
            name: metadata?.name || generator.name,
            scientificName: metadata?.latin || metadata?.scientific || s.params.scientificName
          }
        }));
        await this.regenerate();
      } catch (e: any) {
        console.error('loadSpecies error:', e);
        update(s => ({ ...s, loading: false, error: e.toString() }));
      }
    },

    async loadNaturePatch(toml: string) {
      if (!wasmModule) {
        console.error('WASM module not loaded');
        return;
      }

      update(s => ({ ...s, loading: true, error: null }));

      try {
        const natureParams = parseNatureToml(toml);
        natureGenerator = new wasmModule.MidoriNatureGenerator(natureParamsToToml(natureParams));
        generator = null;
        const natureData = natureGenerator.preview(NATURE_PREVIEW_RESOLUTION, NATURE_SCATTER_CHUNK_SIZE) as NaturePreviewOutput;
        const name = natureGenerator.name;
        update(s => ({
          ...s,
          previewMode: 'nature',
          species: name,
          metadata: null,
          meshData: null,
          natureData,
          generationStats: null,
          exportStatus: null,
          loading: false,
          natureParams,
          params: {
            ...s.params,
            name,
            scientificName: natureData?.manifest?.asset_name ?? name
          }
        }));
      } catch (e: any) {
        console.error('loadNaturePatch error:', e);
        update(s => ({ ...s, loading: false, error: e.toString() }));
      }
    },

    async regenerate() {
      const state = await new Promise<TreeState>(resolve => {
        subscribe(s => resolve(s))();
      });

      if (state.previewMode === 'nature') {
        if (!wasmModule) return;
        update(s => ({ ...s, loading: true }));
        try {
          natureGenerator = new wasmModule.MidoriNatureGenerator(natureParamsToToml(state.natureParams));
          const natureData = natureGenerator.preview(NATURE_PREVIEW_RESOLUTION, NATURE_SCATTER_CHUNK_SIZE) as NaturePreviewOutput;
          update(s => ({ ...s, natureData, exportStatus: null, loading: false }));
        } catch (e: any) {
          update(s => ({ ...s, loading: false, error: e.toString() }));
        }
        return;
      }

      if (!generator) return;

      update(s => ({ ...s, loading: true }));

      try {
        const meshData = generator.generate(BigInt(state.seed)) as MeshOutput;
        const generationStats = generator.get_stats(BigInt(state.seed)) as GenerationStats;
        update(s => ({ ...s, meshData, generationStats, exportStatus: null, loading: false }));
      } catch (e: any) {
        update(s => ({ ...s, loading: false, error: e.toString() }));
      }
    },

    setSeed(seed: number) {
      update(s => ({ ...s, seed, exportStatus: null }));
    },

    randomizeSeed() {
      update(s => ({ ...s, seed: Math.floor(Math.random() * 2147483647), exportStatus: null }));
    },

    // Parameter update methods
    updateParam<K extends keyof TreeParams>(key: K, value: TreeParams[K]) {
      update(s => ({
        ...s,
        params: { ...s.params, [key]: value }
      }));
    },

    updateTrunk<K extends keyof TreeParams['trunk']>(key: K, value: TreeParams['trunk'][K]) {
      update(s => ({
        ...s,
        params: {
          ...s.params,
          trunk: { ...s.params.trunk, [key]: value }
        }
      }));
    },

    updateBranchLevel1<K extends keyof BranchParams>(key: K, value: BranchParams[K]) {
      update(s => ({
        ...s,
        params: {
          ...s.params,
          branches: {
            ...s.params.branches,
            level1: { ...s.params.branches.level1, [key]: value }
          }
        }
      }));
    },

    updateBranchLevel2<K extends keyof BranchParams>(key: K, value: BranchParams[K]) {
      update(s => ({
        ...s,
        params: {
          ...s.params,
          branches: {
            ...s.params.branches,
            level2: { ...s.params.branches.level2, [key]: value }
          }
        }
      }));
    },

    updateCrown<K extends keyof TreeParams['crown']>(key: K, value: TreeParams['crown'][K]) {
      update(s => ({
        ...s,
        params: {
          ...s.params,
          crown: { ...s.params.crown, [key]: value }
        }
      }));
    },

    updateLeaves<K extends keyof TreeParams['leaves']>(key: K, value: TreeParams['leaves'][K]) {
      update(s => ({
        ...s,
        params: {
          ...s.params,
          leaves: { ...s.params.leaves, [key]: value }
        }
      }));
    },

    updateNaturePatch<K extends keyof NatureParams['patch']>(key: K, value: NatureParams['patch'][K]) {
      update(s => ({
        ...s,
        exportStatus: null,
        natureParams: {
          ...s.natureParams,
          patch: { ...s.natureParams.patch, [key]: value }
        }
      }));
    },

    updateNatureSoil<K extends keyof Omit<NatureParams['soil'], 'cracks'>>(key: K, value: NatureParams['soil'][K]) {
      update(s => ({
        ...s,
        exportStatus: null,
        natureParams: {
          ...s.natureParams,
          soil: { ...s.natureParams.soil, [key]: value }
        }
      }));
    },

    updateNatureCracks<K extends keyof NatureParams['soil']['cracks']>(
      key: K,
      value: NatureParams['soil']['cracks'][K]
    ) {
      update(s => ({
        ...s,
        exportStatus: null,
        natureParams: {
          ...s.natureParams,
          soil: {
            ...s.natureParams.soil,
            cracks: { ...s.natureParams.soil.cracks, [key]: value }
          }
        }
      }));
    },

    updateNatureWind<K extends keyof NatureParams['wind']>(key: K, value: NatureParams['wind'][K]) {
      update(s => ({
        ...s,
        exportStatus: null,
        natureParams: {
          ...s.natureParams,
          wind: { ...s.natureParams.wind, [key]: value }
        }
      }));
    },

    updateNatureLayer<K extends keyof NatureLayerParams>(
      index: number,
      key: K,
      value: NatureLayerParams[K]
    ) {
      update(s => ({
        ...s,
        exportStatus: null,
        natureParams: {
          ...s.natureParams,
          groundcover: {
            layers: s.natureParams.groundcover.layers.map((layer, layerIndex) =>
              layerIndex === index ? { ...layer, [key]: value } : layer
            )
          }
        }
      }));
    },

    updateNatureProfile<K extends keyof NatureProfileParams>(
      profile: 'mobile' | 'console',
      key: K,
      value: NatureProfileParams[K]
    ) {
      update(s => ({
        ...s,
        exportStatus: null,
        natureParams: {
          ...s.natureParams,
          profiles: {
            ...s.natureParams.profiles,
            [profile]: { ...s.natureParams.profiles[profile], [key]: value }
          }
        }
      }));
    },

    // Set species name and scientific name
    setSpeciesName(name: string) {
      update(s => ({
        ...s,
        species: name,
        params: { ...s.params, name }
      }));
    },

    setScientificName(scientificName: string) {
      update(s => ({
        ...s,
        params: { ...s.params, scientificName }
      }));
    },

    // Generate TOML from current params
    paramsToToml(): string {
      let state: TreeState | null = null;
      subscribe(s => state = s)();
      if (!state) return '';

      const p = state.params;
      return `[species]
name = "${p.name}"
scientific = "${p.scientificName}"

[trunk]
height = ${p.trunk.height}
radius = ${p.trunk.radius}
taper = ${p.trunk.taper}
curve = ${p.trunk.curve}
segments = ${p.trunk.segments}

[branches.level1]
count = ${p.branches.level1.count}
length = ${p.branches.level1.length}
angle = ${p.branches.level1.angle}
rotation = ${p.branches.level1.rotation}
gravity = ${p.branches.level1.gravity}
segments = 6

[branches.level2]
count = ${p.branches.level2.count}
length = ${p.branches.level2.length}
angle = ${p.branches.level2.angle}
rotation = ${p.branches.level2.rotation}
gravity = ${p.branches.level2.gravity}
segments = 4

[branches.level3]
count = 2
length = 0.5
angle = 45
rotation = 137.5
gravity = 0.3
segments = 2

[crown]
shape = "${p.crown.shape}"
offset = ${p.crown.offset}

[leaves]
count = ${p.leaves.count}
size = ${p.leaves.size}
geometry = "${p.leaves.geometry}"
`;
    },

    natureParamsToToml(): string {
      let state: TreeState | null = null;
      subscribe(s => state = s)();
      return state ? natureParamsToToml(state.natureParams) : '';
    },

    // Generate from current params
    async generateFromParams() {
      const toml = this.paramsToToml();
      await this.loadSpecies(toml);
    },

    // Export GLB file
    async exportGlb(): Promise<Blob | null> {
      if (!generator || !wasmModule) return null;

      try {
        const state = await new Promise<TreeState>(resolve => {
          subscribe(s => resolve(s))();
        });

        // Get GLB data from WASM
        const glbData = generator.export_glb(BigInt(state.seed));
        const blob = new Blob([glbData], { type: 'model/gltf-binary' });
        const lodChain = state.meshData?.lods?.map(lod => lod.name) ?? [];
        update(s => ({
          ...s,
          exportStatus: {
            sizeBytes: blob.size,
            seed: state.seed,
            lodChain,
            exportedAt: new Date().toISOString()
          }
        }));
        return blob;
      } catch (e: any) {
        console.error('Export failed:', e);
        update(s => ({ ...s, error: `Export failed: ${e.toString()}` }));
        return null;
      }
    },

    // Check if generator is ready
    isReady(): boolean {
      return generator !== null && wasmModule !== null;
    }
  };
}

export const treeStore = createTreeStore();
