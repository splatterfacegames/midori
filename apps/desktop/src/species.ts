/**
 * TypeScript view model for a species document.
 *
 * The TOML document (via midori-core serde) is the authority. These interfaces
 * mirror the JSON projection produced by `MidoriGenerator.toJson()` so the
 * inspector can render and edit every parameter without a parallel schema.
 */

export interface SpeciesInfoJson {
  name: string;
  scientific: string;
}

export interface TrunkParamsJson {
  height: number;
  height_variance: number;
  radius: number;
  taper: number;
  curve: number;
  curve_variance: number;
  curve_back: number;
  segments: number;
}

export interface BranchParamsJson {
  count: number;
  count_variance: number;
  length: number;
  length_variance: number;
  radius_ratio: number;
  angle: number;
  angle_variance: number;
  rotation: number;
  gravity: number;
  curve: number;
  curve_variance: number;
  segments: number;
}

export type CrownShape = 'spherical' | 'conical' | 'hemispherical' | 'flame' | 'columnar';
export type LeafDistribution = 'endpoint' | 'along_branch' | 'both';
export type LeafGeometry = 'polygon' | 'cross_billboard' | 'billboard' | 'canopy_card' | 'none';
export type LodPresetName =
  | 'ultra'
  | 'high_quality'
  | 'balanced'
  | 'mobile'
  | 'minimal'
  | 'open_world'
  | 'hero_tree'
  | 'custom';
export type PlatformTarget = 'modern_pc' | 'mobile' | 'switch' | 'quest' | 'web' | 'universal';

export interface CrownParamsJson {
  shape: CrownShape;
  offset: number;
  density: number;
  width_ratio: number;
}

export interface LeafParamsJson {
  count: number;
  min_level: number;
  size: number;
  size_variance: number;
  distribution: LeafDistribution;
  geometry: LeafGeometry;
  up_influence: number;
}

export type BarkStyle = 'furrowed' | 'plated' | 'smooth';
export type LeafShapeName =
  | 'oval'
  | 'pointed'
  | 'needle'
  | 'oak_lobed'
  | 'lobed'
  | 'maple'
  | 'serrated'
  | 'willow'
  | 'heart'
  | 'palmate'
  | 'ginkgo'
  | 'ivy'
  | 'paddle';
export type LeafCardLayout =
  | 'single'
  | 'cluster'
  | 'puff'
  | 'dense'
  | 'frond'
  | 'buxus'
  | 'scale_cypress'
  | 'floral';

export interface TextureParamsJson {
  bark_prompt: string;
  leaf_prompt: string;
  resolution: number;
  seed?: number;
  bark_style: BarkStyle;
  leaf_shape: LeafShapeName;
  leaf_card: LeafCardLayout;
  bark_color?: [number, number, number];
  leaf_color?: [number, number, number];
  bark_albedo?: string;
  bark_normal?: string;
  leaf_albedo_alpha?: string;
}

export interface LodLevelJson {
  index: number;
  name: string;
  target_triangles: number;
  max_triangles?: number;
  branch_levels: number;
  leaf_geometry: LeafGeometry;
  leaf_reduction: number;
  ring_resolution?: [number, number, number, number];
  screen_height: number;
  crown_impostor: boolean;
}

export interface LodConfigJson {
  preset: LodPresetName;
  count?: number;
  levels?: LodLevelJson[];
}

export interface PlatformConfigJson {
  target: PlatformTarget;
}

export interface SpeciesJson {
  species: SpeciesInfoJson;
  generator?: {
    family?: string;
    notes?: string;
  };
  trunk: TrunkParamsJson;
  branches: {
    level1?: BranchParamsJson;
    level2?: BranchParamsJson;
    level3?: BranchParamsJson;
  };
  crown: CrownParamsJson;
  leaves: LeafParamsJson;
  canopy?: {
    normals?: 'geometric' | 'canopy';
    normal_blend?: number;
    up_bias?: number;
    clump_weight?: number;
    crown_fill?: number;
    shell?: number;
    card_tilt?: number;
    card_max_angle?: number;
    ao_strength?: number;
  };
  textures: TextureParamsJson;
  lod: LodConfigJson;
  platform: PlatformConfigJson;
  topiary?: {
    shape?: 'box' | 'bent_box' | 'sphere' | 'cylinder' | 'cone';
    length?: number;
    width?: number;
    height?: number;
    bevel?: number;
    bend_angle?: number;
    trim_tightness?: number;
    skirt_clearance?: number;
  };
  floral?: {
    habit?: 'mound' | 'trough' | 'cascade';
    length?: number;
    width?: number;
    height?: number;
    cascade_drop?: number;
    blossom_density?: number;
  };
  vine?: {
    width?: number;
    height?: number;
    depth?: number;
    main_vines?: number;
    hanging_runners?: number;
  };
}

export const CROWN_SHAPES: { value: CrownShape; label: string }[] = [
  { value: 'spherical', label: 'Spherical' },
  { value: 'conical', label: 'Conical' },
  { value: 'hemispherical', label: 'Hemispherical' },
  { value: 'flame', label: 'Flame' },
  { value: 'columnar', label: 'Columnar' },
];

export const LEAF_DISTRIBUTIONS: { value: LeafDistribution; label: string }[] = [
  { value: 'endpoint', label: 'Endpoint' },
  { value: 'along_branch', label: 'Along branch' },
  { value: 'both', label: 'Both' },
];

export const LEAF_GEOMETRIES: { value: LeafGeometry; label: string }[] = [
  { value: 'polygon', label: 'Polygon' },
  { value: 'cross_billboard', label: 'Cross billboard' },
  { value: 'billboard', label: 'Billboard' },
  { value: 'canopy_card', label: 'Canopy card (top-down)' },
  { value: 'none', label: 'None' },
];

export const LOD_PRESETS: { value: LodPresetName; label: string }[] = [
  { value: 'ultra', label: 'Ultra' },
  { value: 'high_quality', label: 'High quality' },
  { value: 'balanced', label: 'Balanced' },
  { value: 'open_world', label: 'Open world' },
  { value: 'hero_tree', label: 'Hero tree' },
  { value: 'mobile', label: 'Mobile' },
  { value: 'minimal', label: 'Minimal' },
  { value: 'custom', label: 'Custom' },
];

export const BARK_STYLES: { value: BarkStyle; label: string }[] = [
  { value: 'furrowed', label: 'Furrowed (oak, ash)' },
  { value: 'plated', label: 'Plated (pine, spruce)' },
  { value: 'smooth', label: 'Smooth (palm, beech)' },
];

export const LEAF_SHAPES: { value: LeafShapeName; label: string }[] = [
  { value: 'oval', label: 'Oval' },
  { value: 'pointed', label: 'Pointed' },
  { value: 'oak_lobed', label: 'Oak lobed' },
  { value: 'lobed', label: 'Lobed (legacy)' },
  { value: 'maple', label: 'Maple (Momiji)' },
  { value: 'ginkgo', label: 'Ginkgo fan' },
  { value: 'serrated', label: 'Serrated' },
  { value: 'willow', label: 'Willow lance' },
  { value: 'heart', label: 'Heart (Linden)' },
  { value: 'ivy', label: 'Ivy 5-lobe (Hedera)' },
  { value: 'paddle', label: 'Tropical paddle (Strelitzia)' },
  { value: 'palmate', label: 'Palmate / frond' },
  { value: 'needle', label: 'Needle / rosette' },
];

export const LEAF_CARD_LAYOUTS: { value: LeafCardLayout; label: string }[] = [
  { value: 'cluster', label: 'Cluster spray' },
  { value: 'single', label: 'Single / frond / paddle' },
  { value: 'puff', label: 'Canopy puff' },
  { value: 'dense', label: 'Dense spray' },
  { value: 'frond', label: 'Conifer / palm frond' },
  { value: 'buxus', label: 'Boxwood sprig (Buxus)' },
  { value: 'scale_cypress', label: 'Cypress scale fan' },
  { value: 'floral', label: 'Pansy floral bouquet (2×2)' },
];

export const TOPIARY_SECTION: ParamSection = {
  id: 'topiary',
  title: 'Topiary & Hedge Hull',
  fields: [
    {
      kind: 'enum',
      key: 'shape',
      label: 'Hull shape',
      options: [
        { value: 'box', label: 'Clipped box hedge' },
        { value: 'bent_box', label: 'Curved corner hedge' },
        { value: 'sphere', label: 'Topiary globe' },
        { value: 'cylinder', label: 'Topiary drum' },
        { value: 'cone', label: 'Topiary cone' },
      ],
    },
    { kind: 'number', key: 'length', label: 'Length', min: 0.3, max: 12, step: 0.05, unit: 'm' },
    { kind: 'number', key: 'width', label: 'Width', min: 0.25, max: 6, step: 0.05, unit: 'm' },
    { kind: 'number', key: 'height', label: 'Height', min: 0.3, max: 6, step: 0.05, unit: 'm' },
    { kind: 'number', key: 'bevel', label: 'Corner bevel', min: 0, max: 0.6, step: 0.02, unit: 'm' },
    { kind: 'number', key: 'bend_angle', label: 'Arc bend', min: -170, max: 170, step: 5, unit: '°' },
    { kind: 'number', key: 'trim_tightness', label: 'Shear tightness', min: 0, max: 1, step: 0.05 },
    { kind: 'number', key: 'skirt_clearance', label: 'Base clearance', min: 0, max: 0.4, step: 0.02, unit: 'm' },
  ],
};

export const FLORAL_SECTION: ParamSection = {
  id: 'floral',
  title: 'Floral Planter & Cascade',
  fields: [
    {
      kind: 'enum',
      key: 'habit',
      label: 'Habit',
      options: [
        { value: 'mound', label: 'Cushion mound' },
        { value: 'trough', label: 'Window-box trough' },
        { value: 'cascade', label: 'Hanging cascade' },
      ],
    },
    { kind: 'number', key: 'length', label: 'Length', min: 0.25, max: 4, step: 0.05, unit: 'm' },
    { kind: 'number', key: 'width', label: 'Width', min: 0.25, max: 4, step: 0.05, unit: 'm' },
    { kind: 'number', key: 'height', label: 'Mound height', min: 0.15, max: 2, step: 0.04, unit: 'm' },
    { kind: 'number', key: 'cascade_drop', label: 'Cascade drop', min: 0, max: 4, step: 0.05, unit: 'm' },
    { kind: 'number', key: 'blossom_density', label: 'Blossom share', min: 0, max: 1, step: 0.05 },
  ],
};

export const VINE_SECTION: ParamSection = {
  id: 'vine',
  title: 'Climbing Wall Vine',
  fields: [
    { kind: 'number', key: 'width', label: 'Wall spread', min: 0.4, max: 10, step: 0.1, unit: 'm' },
    { kind: 'number', key: 'height', label: 'Climb height', min: 0.6, max: 12, step: 0.1, unit: 'm' },
    { kind: 'number', key: 'depth', label: 'Wall depth', min: 0.15, max: 2, step: 0.04, unit: 'm' },
    { kind: 'number', key: 'main_vines', label: 'Main climbers', min: 1, max: 16, step: 1, int: true },
    { kind: 'number', key: 'hanging_runners', label: 'Pendant tendrils', min: 0, max: 24, step: 1, int: true },
  ],
};

export const PLATFORM_TARGETS: { value: PlatformTarget; label: string }[] = [
  { value: 'modern_pc', label: 'Modern PC' },
  { value: 'mobile', label: 'Mobile' },
  { value: 'switch', label: 'Switch' },
  { value: 'quest', label: 'Quest' },
  { value: 'web', label: 'Web' },
  { value: 'universal', label: 'Universal' },
];

export type ParamField =
  | { kind: 'text'; key: string; label: string }
  | { kind: 'number'; key: string; label: string; min?: number; max?: number; step?: number; unit?: string; int?: boolean }
  | { kind: 'enum'; key: string; label: string; options: { value: string; label: string }[] };

export interface ParamSection {
  id: string;
  title: string;
  /** Optional branch level key; section can be enabled/disabled. */
  branchLevel?: 'level1' | 'level2' | 'level3';
  fields: ParamField[];
}

const num = (
  key: string,
  label: string,
  opts: { min?: number; max?: number; step?: number; unit?: string; int?: boolean } = {},
): ParamField => ({ kind: 'number', key, label, ...opts });

const BRANCH_FIELDS: ParamField[] = [
  num('count', 'Count', { min: 0, max: 64, step: 1, int: true }),
  num('count_variance', 'Count ±', { min: 0, max: 16, step: 1, int: true }),
  num('length', 'Length', { min: 0, step: 0.1, unit: 'm' }),
  num('length_variance', 'Length ±', { min: 0, max: 1, step: 0.05 }),
  num('radius_ratio', 'Radius ratio', { min: 0.05, max: 1, step: 0.05 }),
  num('angle', 'Angle', { min: 0, max: 180, step: 1, unit: '°' }),
  num('angle_variance', 'Angle ±', { min: 0, max: 90, step: 1, unit: '°' }),
  num('rotation', 'Rotation', { min: 0, max: 360, step: 0.5, unit: '°' }),
  num('gravity', 'Gravity', { min: -1, max: 1, step: 0.05 }),
  num('curve', 'Curve', { min: -180, max: 180, step: 1, unit: '°' }),
  num('curve_variance', 'Curve ±', { min: 0, max: 90, step: 1, unit: '°' }),
  num('segments', 'Segments', { min: 1, max: 16, step: 1, int: true }),
];

export const PARAM_SECTIONS: ParamSection[] = [
  {
    id: 'trunk',
    title: 'Trunk',
    fields: [
      num('height', 'Height', { min: 0.1, step: 0.1, unit: 'm' }),
      num('height_variance', 'Height ±', { min: 0, max: 1, step: 0.05 }),
      num('radius', 'Radius', { min: 0.01, step: 0.01, unit: 'm' }),
      num('taper', 'Taper', { min: 0, max: 1, step: 0.05 }),
      num('curve', 'Curve', { min: -180, max: 180, step: 1, unit: '°' }),
      num('curve_variance', 'Curve ±', { min: 0, max: 90, step: 1, unit: '°' }),
      num('curve_back', 'Curve back', { min: -180, max: 180, step: 1, unit: '°' }),
      num('segments', 'Segments', { min: 1, max: 32, step: 1, int: true }),
    ],
  },
  { id: 'branches.level1', title: 'Branches · Level 1', branchLevel: 'level1', fields: BRANCH_FIELDS },
  { id: 'branches.level2', title: 'Branches · Level 2', branchLevel: 'level2', fields: BRANCH_FIELDS },
  { id: 'branches.level3', title: 'Branches · Level 3', branchLevel: 'level3', fields: BRANCH_FIELDS },
  {
    id: 'crown',
    title: 'Crown',
    fields: [
      { kind: 'enum', key: 'shape', label: 'Shape', options: CROWN_SHAPES },
      num('offset', 'Offset', { min: 0, max: 1, step: 0.05 }),
      num('density', 'Density', { min: 0, max: 4, step: 0.1 }),
      num('width_ratio', 'Width ratio', { min: 0.1, max: 4, step: 0.05 }),
    ],
  },
  {
    id: 'leaves',
    title: 'Leaves',
    fields: [
      num('count', 'Count', { min: 0, step: 100, int: true }),
      num('min_level', 'Min level', { min: 0, max: 3, step: 1, int: true }),
      num('size', 'Size', { min: 0.01, step: 0.01, unit: 'm' }),
      num('size_variance', 'Size ±', { min: 0, max: 1, step: 0.05 }),
      { kind: 'enum', key: 'distribution', label: 'Distribution', options: LEAF_DISTRIBUTIONS },
      { kind: 'enum', key: 'geometry', label: 'Geometry', options: LEAF_GEOMETRIES },
      num('up_influence', 'Up influence', { min: 0, max: 1, step: 0.05 }),
    ],
  },
  {
    id: 'canopy',
    title: 'Canopy & Top-down',
    fields: [
      {
        kind: 'enum',
        key: 'normals',
        label: 'Normals',
        options: [
          { value: 'geometric', label: 'Geometric' },
          { value: 'canopy', label: 'Canopy dome' },
        ],
      },
      num('crown_fill', 'Crown fill', { min: 0, step: 10, int: true }),
      num('shell', 'Outer shell', { min: 0, max: 1, step: 0.05 }),
      num('card_tilt', 'Card tilt', { min: 0, max: 90, step: 1, unit: '°' }),
      num('card_max_angle', 'Max card angle', { min: 5, max: 90, step: 1, unit: '°' }),
      num('ao_strength', 'AO strength', { min: 0, max: 1, step: 0.05 }),
    ],
  },
  {
    id: 'textures',
    title: 'Textures',
    fields: [
      num('resolution', 'Map size', { min: 16, max: 4096, step: 64, unit: 'px', int: true }),
      num('seed', 'Map seed (0 = from species name)', { min: 0, step: 1, int: true }),
      { kind: 'enum', key: 'bark_style', label: 'Bark style', options: BARK_STYLES },
      { kind: 'enum', key: 'leaf_shape', label: 'Leaf shape', options: LEAF_SHAPES },
      { kind: 'enum', key: 'leaf_card', label: 'Leaf card', options: LEAF_CARD_LAYOUTS },
      { kind: 'text', key: 'bark_prompt', label: 'Bark prompt' },
      { kind: 'text', key: 'leaf_prompt', label: 'Leaf prompt' },
      { kind: 'text', key: 'bark_albedo', label: 'Bark albedo file' },
      { kind: 'text', key: 'bark_normal', label: 'Bark normal file' },
      { kind: 'text', key: 'leaf_albedo_alpha', label: 'Leaf card file' },
    ],
  },
  {
    id: 'lod',
    title: 'LOD',
    fields: [{ kind: 'enum', key: 'preset', label: 'Preset', options: LOD_PRESETS }],
  },
  {
    id: 'platform',
    title: 'Platform',
    fields: [{ kind: 'enum', key: 'target', label: 'Target', options: PLATFORM_TARGETS }],
  },
];

export const BRANCH_LEVEL_IDS = ['level1', 'level2', 'level3'] as const;
export type BranchLevelId = (typeof BRANCH_LEVEL_IDS)[number];

/** Defaults for enabling a previously-disabled branch level. */
export const DEFAULT_BRANCH_PARAMS: BranchParamsJson = {
  count: 4,
  count_variance: 1,
  length: 2.0,
  length_variance: 0.3,
  radius_ratio: 0.5,
  angle: 50,
  angle_variance: 15,
  rotation: 137.5,
  gravity: -0.2,
  curve: 30,
  curve_variance: 10,
  segments: 4,
};

/** Resolve a dotted path like "branches.level2.angle" against the species JSON. */
export function getParam(species: SpeciesJson, path: string): unknown {
  let cursor: unknown = species;
  for (const part of path.split('.')) {
    if (cursor == null || typeof cursor !== 'object') return undefined;
    cursor = (cursor as Record<string, unknown>)[part];
  }
  return cursor;
}

/** Return a new species object with `path` set to `value`. */
export function setParam(species: SpeciesJson, path: string, value: unknown): SpeciesJson {
  const parts = path.split('.');
  const clone = structuredClone(species) as unknown as Record<string, unknown>;
  let cursor: Record<string, unknown> = clone;
  for (let i = 0; i < parts.length - 1; i++) {
    const next = cursor[parts[i]];
    if (next == null || typeof next !== 'object') {
      cursor[parts[i]] = {};
    }
    cursor = cursor[parts[i]] as Record<string, unknown>;
  }
  cursor[parts[parts.length - 1]] = value;
  return clone as unknown as SpeciesJson;
}
