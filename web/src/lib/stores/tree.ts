import { writable, derived } from 'svelte/store';

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

interface TreeState {
  species: string;
  seed: number;
  meshData: any | null;
  loading: boolean;
  error: string | null;
  params: TreeParams;
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

function createTreeStore() {
  const { subscribe, set, update } = writable<TreeState>({
    species: '',
    seed: 12345,
    meshData: null,
    loading: false,
    error: null,
    params: { ...defaultParams }
  });

  let generator: any = null;
  let wasmModule: any = null;

  return {
    subscribe,

    async init() {
      try {
        // Dynamic import of WASM from local build
        const wasm = await import('$lib/wasm/grove_wasm.js');
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
        console.log('Creating GroveGenerator...');
        generator = new wasmModule.GroveGenerator(toml);
        console.log('GroveGenerator created, name:', generator.name);
        update(s => ({ ...s, species: generator.name, loading: false }));
        await this.regenerate();
      } catch (e: any) {
        console.error('loadSpecies error:', e);
        update(s => ({ ...s, loading: false, error: e.toString() }));
      }
    },

    async regenerate() {
      if (!generator) return;

      update(s => ({ ...s, loading: true }));

      try {
        const state = await new Promise<TreeState>(resolve => {
          subscribe(s => resolve(s))();
        });

        const meshData = generator.generate(BigInt(state.seed));
        update(s => ({ ...s, meshData, loading: false }));
      } catch (e: any) {
        update(s => ({ ...s, loading: false, error: e.toString() }));
      }
    },

    setSeed(seed: number) {
      update(s => ({ ...s, seed }));
    },

    randomizeSeed() {
      update(s => ({ ...s, seed: Math.floor(Math.random() * 2147483647) }));
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
        return new Blob([glbData], { type: 'model/gltf-binary' });
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
