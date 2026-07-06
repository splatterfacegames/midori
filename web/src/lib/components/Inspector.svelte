<script lang="ts">
  import { treeStore } from '$lib/stores/tree';
  import { editorStore, type NaturePreviewProfile } from '$lib/stores/editor';
  import Section from './inspector/Section.svelte';
  import NumberInput from './inspector/NumberInput.svelte';
  import SelectInput from './inspector/SelectInput.svelte';

  // Crown shape options
  const crownShapeOptions = [
    { value: 'spherical', label: 'Spherical' },
    { value: 'conical', label: 'Conical' },
    { value: 'hemispherical', label: 'Hemispherical' },
    { value: 'flame', label: 'Flame' },
    { value: 'columnar', label: 'Columnar' }
  ];

  // Leaf geometry options
  const leafGeometryOptions = [
    { value: 'polygon', label: 'Polygon' },
    { value: 'cross_billboard', label: 'Cross Billboard' },
    { value: 'billboard', label: 'Billboard' },
    { value: 'none', label: 'None' }
  ];

  const natureProfileOptions = [
    { value: 'authoring', label: 'Authoring' },
    { value: 'mobile', label: 'Mobile' },
    { value: 'console', label: 'Console' }
  ];

  const natureLodOptions = [
    { value: '0', label: 'LOD 0' },
    { value: '1', label: 'LOD 1' },
    { value: '2', label: 'LOD 2' }
  ];

  // Local state for seed input
  let seed = 12345;
  $: seed = $treeStore.seed;

  // Species inputs
  let speciesName = '';
  let scientificName = '';
  $: speciesName = $treeStore.params.name;
  $: scientificName = $treeStore.params.scientificName;

  function updateSeed() {
    treeStore.setSeed(seed);
    if ($editorStore.autoRegenerate) {
      treeStore.regenerate();
    }
  }

  function handleRegenerate() {
    treeStore.regenerate();
  }

  function handleRandomize() {
    treeStore.randomizeSeed();
    treeStore.regenerate();
  }

  function updateSpeciesName() {
    treeStore.setSpeciesName(speciesName);
  }

  function updateScientificName() {
    treeStore.setScientificName(scientificName);
  }

  $: lods = $treeStore.meshData?.lods ?? [];
  $: selectedLodIndex = lods.length > 0 ? Math.min($editorStore.currentLod, lods.length - 1) : 0;
  $: selectedLod = lods[selectedLodIndex] ?? null;
  $: exportStatus = $treeStore.exportStatus;
  $: natureData = $treeStore.natureData;
  $: natureManifest = natureData?.manifest ?? null;
  $: natureStats = natureData?.stats ?? null;
  $: natureParams = $treeStore.natureParams;

  function formatCount(value: number | undefined): string {
    return value === undefined ? '--' : value.toLocaleString();
  }

  function formatBytes(value: number | undefined): string {
    if (value === undefined) return '--';
    if (value < 1024) return `${value} B`;
    if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
    return `${(value / (1024 * 1024)).toFixed(2)} MB`;
  }

  function setManualLod(value: string) {
    editorStore.setLodMode('manual');
    editorStore.setLod(parseInt(value, 10));
  }

  function setLodMode(value: string) {
    editorStore.setLodMode(value === 'auto' ? 'auto' : 'manual');
  }

  function setNaturePreviewProfile(value: string) {
    editorStore.setNaturePreviewProfile(value as NaturePreviewProfile);
  }

  function regenerateAfterNatureChange() {
    if ($editorStore.autoRegenerate) treeStore.regenerate();
  }

  function titleCase(value: string): string {
    return value.charAt(0).toUpperCase() + value.slice(1);
  }
</script>

<div class="inspector">
  {#if $treeStore.previewMode === 'nature'}
    <Section title="Nature Patch">
      <div class="stats">
        <div class="stat">
          <span class="stat-label">Name</span>
          <span class="stat-value">{natureManifest?.asset_name ?? $treeStore.species}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Tile</span>
          <span class="stat-value">{natureStats?.tile_size ?? natureManifest?.tile_size ?? '--'}m</span>
        </div>
        <div class="stat">
          <span class="stat-label">Seed</span>
          <span class="stat-value">{natureManifest?.seed ?? '--'}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Layers</span>
          <span class="stat-value">{formatCount(natureManifest?.groundcover_layers?.length)}</span>
        </div>
      </div>
      <div class="regenerate-row">
        <button class="btn-primary" on:click={handleRegenerate}>
          Regenerate Preview
        </button>
      </div>
      <SelectInput
        label="Preview Profile"
        value={$editorStore.naturePreviewProfile}
        options={natureProfileOptions}
        on:change={(e) => setNaturePreviewProfile(e.detail)}
      />
      <SelectInput
        label="Prototype LOD"
        value={String($editorStore.naturePreviewLod)}
        options={natureLodOptions}
        on:change={(e) => editorStore.setNaturePreviewLod(parseInt(e.detail, 10))}
      />
    </Section>

    <Section title="Terrain And Soil">
      <NumberInput
        label="Patch Seed"
        value={natureParams.patch.seed}
        min={0}
        max={999999}
        step={1}
        showSlider={false}
        on:change={(e) => {
          treeStore.updateNaturePatch('seed', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Tile Size"
        value={natureParams.patch.size}
        min={8}
        max={64}
        step={1}
        on:change={(e) => {
          treeStore.updateNaturePatch('size', e.detail);
          treeStore.updateNatureProfile('mobile', 'tile_size', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Mound Height"
        value={natureParams.soil.mound_height}
        min={0}
        max={1.5}
        step={0.01}
        on:change={(e) => {
          treeStore.updateNatureSoil('mound_height', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Relief Strength"
        value={natureParams.soil.relief_strength}
        min={0}
        max={1.5}
        step={0.01}
        on:change={(e) => {
          treeStore.updateNatureSoil('relief_strength', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Wetness"
        value={natureParams.soil.wetness_coverage}
        min={0}
        max={1}
        step={0.01}
        on:change={(e) => {
          treeStore.updateNatureSoil('wetness_coverage', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <div class="checkbox-row">
        <label>
          <input
            type="checkbox"
            checked={natureParams.soil.cracks.enabled}
            on:change={(e) => {
              treeStore.updateNatureCracks('enabled', e.currentTarget.checked);
              regenerateAfterNatureChange();
            }}
          />
          Cracks
        </label>
      </div>
      <NumberInput
        label="Crack Amount"
        value={natureParams.soil.cracks.amount}
        min={0}
        max={1}
        step={0.01}
        on:change={(e) => {
          treeStore.updateNatureCracks('amount', e.detail);
          regenerateAfterNatureChange();
        }}
      />
    </Section>

    <Section title="Groundcover">
      {#each natureParams.groundcover.layers as layer, index}
        <div class="layer-header">
          <span>{titleCase(layer.kind)}</span>
          <span>{layer.name}</span>
        </div>
        <NumberInput
          label={`${titleCase(layer.kind)} Density`}
          value={layer.density}
          min={0}
          max={0.5}
          step={0.005}
          on:change={(e) => {
            treeStore.updateNatureLayer(index, 'density', e.detail);
            regenerateAfterNatureChange();
          }}
        />
        <NumberInput
          label={`${titleCase(layer.kind)} Coverage`}
          value={layer.coverage}
          min={0}
          max={1}
          step={0.01}
          on:change={(e) => {
            treeStore.updateNatureLayer(index, 'coverage', e.detail);
            regenerateAfterNatureChange();
          }}
        />
        <NumberInput
          label={`${titleCase(layer.kind)} Height`}
          value={layer.height}
          min={0.03}
          max={2}
          step={0.01}
          on:change={(e) => {
            treeStore.updateNatureLayer(index, 'height', e.detail);
            regenerateAfterNatureChange();
          }}
        />
      {/each}
    </Section>

    <Section title="Patch Wind" expanded={false}>
      <NumberInput
        label="Patch Wind Strength"
        value={natureParams.wind.strength}
        min={0}
        max={2}
        step={0.05}
        on:change={(e) => {
          treeStore.updateNatureWind('strength', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Patch Wind Speed"
        value={natureParams.wind.speed}
        min={0}
        max={5}
        step={0.05}
        on:change={(e) => {
          treeStore.updateNatureWind('speed', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Patch Wind Direction"
        value={natureParams.wind.direction_degrees}
        min={0}
        max={360}
        step={1}
        on:change={(e) => {
          treeStore.updateNatureWind('direction_degrees', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Gust Scale"
        value={natureParams.wind.gust_scale}
        min={0}
        max={2}
        step={0.05}
        on:change={(e) => {
          treeStore.updateNatureWind('gust_scale', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Flutter"
        value={natureParams.wind.flutter}
        min={0}
        max={2}
        step={0.05}
        on:change={(e) => {
          treeStore.updateNatureWind('flutter', e.detail);
          regenerateAfterNatureChange();
        }}
      />
    </Section>

    <Section title="Profiles" expanded={false}>
      <NumberInput
        label="Mobile Density"
        value={natureParams.profiles.mobile.density_scale}
        min={0}
        max={2}
        step={0.05}
        on:change={(e) => {
          treeStore.updateNatureProfile('mobile', 'density_scale', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Mobile Cull End"
        value={natureParams.profiles.mobile.cull_end}
        min={4}
        max={120}
        step={1}
        on:change={(e) => {
          treeStore.updateNatureProfile('mobile', 'cull_end', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Console Density"
        value={natureParams.profiles.console.density_scale}
        min={0}
        max={2}
        step={0.05}
        on:change={(e) => {
          treeStore.updateNatureProfile('console', 'density_scale', e.detail);
          regenerateAfterNatureChange();
        }}
      />
      <NumberInput
        label="Console Cull End"
        value={natureParams.profiles.console.cull_end}
        min={8}
        max={200}
        step={1}
        on:change={(e) => {
          treeStore.updateNatureProfile('console', 'cull_end', e.detail);
          regenerateAfterNatureChange();
        }}
      />
    </Section>

    <Section title="Package Contract">
      <div class="stats">
        <div class="stat">
          <span class="stat-label">Schema</span>
          <span class="stat-value">v{natureManifest?.schema_version ?? '--'}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Unity Normal</span>
          <span class="stat-value">{natureManifest?.unity?.normal_map ?? '--'}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Unreal Normal</span>
          <span class="stat-value">{natureManifest?.unreal?.normal_map ?? '--'}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Shader</span>
          <span class="stat-value">{natureManifest?.shader_policy ?? '--'}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Textures</span>
          <span class="stat-value">{natureManifest?.texture_pipeline ?? '--'}</span>
        </div>
      </div>
    </Section>

    <Section title="Nature Statistics">
      <div class="stats">
        <div class="stat">
          <span class="stat-label">Terrain Verts</span>
          <span class="stat-value">{formatCount(natureStats?.terrain_vertex_count)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Terrain Tris</span>
          <span class="stat-value">{formatCount(natureStats?.terrain_triangle_count)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Prototypes</span>
          <span class="stat-value">{formatCount(natureStats?.prototype_count)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Scatter</span>
          <span class="stat-value">{formatCount(natureStats?.scatter_instance_count)}</span>
        </div>
      </div>
    </Section>
  {:else}
  <!-- Species Section -->
  <Section title="Species">
    <div class="text-input">
      <label for="species-name">Name</label>
      <input
        id="species-name"
        type="text"
        bind:value={speciesName}
        on:change={updateSpeciesName}
      />
    </div>
    <div class="text-input">
      <label for="scientific-name">Scientific Name</label>
      <input
        id="scientific-name"
        type="text"
        bind:value={scientificName}
        on:change={updateScientificName}
        class="italic"
      />
    </div>
  </Section>

  <!-- Trunk Section -->
  <Section title="Trunk">
    <NumberInput
      label="Height"
      value={$treeStore.params.trunk.height}
      min={0.5}
      max={20}
      step={0.1}
      on:change={(e) => {
        treeStore.updateTrunk('height', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Radius"
      value={$treeStore.params.trunk.radius}
      min={0.05}
      max={2}
      step={0.01}
      on:change={(e) => {
        treeStore.updateTrunk('radius', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Taper"
      value={$treeStore.params.trunk.taper}
      min={0}
      max={1}
      step={0.01}
      on:change={(e) => {
        treeStore.updateTrunk('taper', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Curve"
      value={$treeStore.params.trunk.curve}
      min={0}
      max={90}
      step={1}
      on:change={(e) => {
        treeStore.updateTrunk('curve', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Segments"
      value={$treeStore.params.trunk.segments}
      min={3}
      max={16}
      step={1}
      showSlider={false}
      on:change={(e) => {
        treeStore.updateTrunk('segments', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
  </Section>

  <!-- Branches Level 1 Section -->
  <Section title="Branches Level 1">
    <NumberInput
      label="Count"
      value={$treeStore.params.branches.level1.count}
      min={0}
      max={20}
      step={1}
      on:change={(e) => {
        treeStore.updateBranchLevel1('count', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Length"
      value={$treeStore.params.branches.level1.length}
      min={0.1}
      max={10}
      step={0.1}
      on:change={(e) => {
        treeStore.updateBranchLevel1('length', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Angle"
      value={$treeStore.params.branches.level1.angle}
      min={0}
      max={90}
      step={1}
      on:change={(e) => {
        treeStore.updateBranchLevel1('angle', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Rotation"
      value={$treeStore.params.branches.level1.rotation}
      min={0}
      max={360}
      step={1}
      on:change={(e) => {
        treeStore.updateBranchLevel1('rotation', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Gravity"
      value={$treeStore.params.branches.level1.gravity}
      min={-1}
      max={1}
      step={0.01}
      on:change={(e) => {
        treeStore.updateBranchLevel1('gravity', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
  </Section>

  <!-- Branches Level 2 Section -->
  <Section title="Branches Level 2" expanded={false}>
    <NumberInput
      label="Count"
      value={$treeStore.params.branches.level2.count}
      min={0}
      max={20}
      step={1}
      on:change={(e) => {
        treeStore.updateBranchLevel2('count', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Length"
      value={$treeStore.params.branches.level2.length}
      min={0.1}
      max={10}
      step={0.1}
      on:change={(e) => {
        treeStore.updateBranchLevel2('length', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Angle"
      value={$treeStore.params.branches.level2.angle}
      min={0}
      max={90}
      step={1}
      on:change={(e) => {
        treeStore.updateBranchLevel2('angle', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Rotation"
      value={$treeStore.params.branches.level2.rotation}
      min={0}
      max={360}
      step={1}
      on:change={(e) => {
        treeStore.updateBranchLevel2('rotation', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Gravity"
      value={$treeStore.params.branches.level2.gravity}
      min={-1}
      max={1}
      step={0.01}
      on:change={(e) => {
        treeStore.updateBranchLevel2('gravity', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
  </Section>

  <!-- Crown Section -->
  <Section title="Crown">
    <SelectInput
      label="Shape"
      value={$treeStore.params.crown.shape}
      options={crownShapeOptions}
      on:change={(e) => {
        treeStore.updateCrown('shape', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Offset"
      value={$treeStore.params.crown.offset}
      min={0}
      max={1}
      step={0.01}
      on:change={(e) => {
        treeStore.updateCrown('offset', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
  </Section>

  <!-- Leaves Section -->
  <Section title="Leaves">
    <NumberInput
      label="Count"
      value={$treeStore.params.leaves.count}
      min={0}
      max={10000}
      step={100}
      on:change={(e) => {
        treeStore.updateLeaves('count', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <NumberInput
      label="Size"
      value={$treeStore.params.leaves.size}
      min={0.01}
      max={0.5}
      step={0.01}
      on:change={(e) => {
        treeStore.updateLeaves('size', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
    <SelectInput
      label="Geometry"
      value={$treeStore.params.leaves.geometry}
      options={leafGeometryOptions}
      on:change={(e) => {
        treeStore.updateLeaves('geometry', e.detail);
        if ($editorStore.autoRegenerate) treeStore.regenerate();
      }}
    />
  </Section>

  <!-- Generation Section -->
  <Section title="Generation">
    <div class="seed-row">
      <div class="seed-input">
        <label for="seed">Seed</label>
        <input
          id="seed"
          type="number"
          bind:value={seed}
          on:change={updateSeed}
        />
      </div>
      <button class="btn-small" on:click={handleRandomize} title="Randomize Seed">
        Rand
      </button>
    </div>
    <div class="regenerate-row">
      <button class="btn-primary" on:click={handleRegenerate}>
        Regenerate Tree
      </button>
    </div>
    <div class="auto-regenerate">
      <label>
        <input
          type="checkbox"
          checked={$editorStore.autoRegenerate}
          on:change={editorStore.toggleAutoRegenerate}
        />
        Auto-regenerate on change
      </label>
    </div>
  </Section>

  <!-- Validation Section -->
  <Section title="Validation">
    <div class="select-row">
      <label for="lod-mode">LOD Mode</label>
      <select
        id="lod-mode"
        value={$editorStore.lodMode}
        on:change={(e) => setLodMode(e.currentTarget.value)}
      >
        <option value="manual">Manual</option>
        <option value="auto">Auto</option>
      </select>
    </div>

    <div class="select-row">
      <label for="lod-level">Preview LOD</label>
      <select
        id="lod-level"
        value={$editorStore.currentLod}
        disabled={$editorStore.lodMode === 'auto'}
        on:change={(e) => setManualLod(e.currentTarget.value)}
      >
        {#each lods as lod, index}
          <option value={index}>{lod.name || `LOD ${index}`}</option>
        {:else}
          <option value={0}>LOD 0</option>
        {/each}
      </select>
    </div>

    <div class="checkbox-row">
      <label>
        <input
          type="checkbox"
          checked={$editorStore.showScaleReference}
          on:change={editorStore.toggleScaleReference}
        />
        Scale Reference
      </label>
    </div>
  </Section>

  {/if}

  <!-- Environment Section -->
  <Section title="Environment" expanded={false}>
    <div class="checkbox-row">
      <label>
        <input
          type="checkbox"
          checked={$editorStore.windEnabled}
          on:change={editorStore.toggleWindEnabled}
        />
        Wind
      </label>
    </div>
    <NumberInput
      label="Wind Strength"
      value={$editorStore.windStrength}
      min={0}
      max={2}
      step={0.05}
      on:change={(e) => editorStore.setWindStrength(e.detail)}
    />
    <NumberInput
      label="Wind Speed"
      value={$editorStore.windSpeed}
      min={0}
      max={5}
      step={0.05}
      on:change={(e) => editorStore.setWindSpeed(e.detail)}
    />
    <NumberInput
      label="Wind Direction"
      value={$editorStore.windDirection}
      min={0}
      max={360}
      step={1}
      on:change={(e) => editorStore.setWindDirection(e.detail)}
    />
    <NumberInput
      label="Sun Azimuth"
      value={$editorStore.sunAzimuth}
      min={0}
      max={360}
      step={1}
      on:change={(e) => editorStore.setSunAzimuth(e.detail)}
    />
    <NumberInput
      label="Sun Elevation"
      value={$editorStore.sunElevation}
      min={5}
      max={85}
      step={1}
      on:change={(e) => editorStore.setSunElevation(e.detail)}
    />
    <NumberInput
      label="Sun Intensity"
      value={$editorStore.sunIntensity}
      min={0}
      max={4}
      step={0.05}
      on:change={(e) => editorStore.setSunIntensity(e.detail)}
    />
  </Section>

  <!-- View Options Section -->
  <Section title="View Options" expanded={false}>
    <div class="checkbox-row">
      <label>
        <input
          type="checkbox"
          checked={$editorStore.showWireframe}
          on:change={editorStore.toggleWireframe}
        />
        Show Wireframe
      </label>
    </div>
    <div class="checkbox-row">
      <label>
        <input
          type="checkbox"
          checked={$editorStore.showNormals}
          on:change={editorStore.toggleNormals}
        />
        Show Normals
      </label>
    </div>
  </Section>

  {#if $treeStore.previewMode === 'tree'}
    <!-- Statistics Section -->
    <Section title="Statistics" expanded={false}>
      <div class="stats">
        <div class="stat">
          <span class="stat-label">LOD</span>
          <span class="stat-value">{selectedLod?.name ?? '--'}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Vertices</span>
          <span class="stat-value">{formatCount(selectedLod?.vertex_count)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Triangles</span>
          <span class="stat-value">{formatCount(selectedLod?.triangle_count)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Branches</span>
          <span class="stat-value">{formatCount(selectedLod?.branch_count)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Leaves</span>
          <span class="stat-value">{formatCount(selectedLod?.leaf_count)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Generated</span>
          <span class="stat-value">{formatCount($treeStore.generationStats?.branch_count)}</span>
        </div>
      </div>
    </Section>

    <!-- Export Section -->
    <Section title="Export" expanded={false}>
      <div class="stats">
        <div class="stat">
          <span class="stat-label">Size</span>
          <span class="stat-value">{formatBytes(exportStatus?.sizeBytes)}</span>
        </div>
        <div class="stat">
          <span class="stat-label">Seed</span>
          <span class="stat-value">{exportStatus?.seed ?? '--'}</span>
        </div>
      </div>
      <div class="status-list">
        <span class="stat-label">LOD Chain</span>
        <span class="stat-value">{exportStatus?.lodChain?.join(' / ') || '--'}</span>
      </div>
    </Section>
  {/if}
</div>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow-y: auto;
  }

  .text-input {
    padding: 0.375rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .text-input label {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .text-input input {
    width: 100%;
    padding: 0.375rem 0.5rem;
    font-size: 0.8125rem;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-primary);
  }

  .text-input input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .text-input input.italic {
    font-style: italic;
  }

  .seed-row {
    padding: 0.375rem 1rem;
    display: flex;
    gap: 0.5rem;
    align-items: flex-end;
  }

  .seed-input {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .seed-input label {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .seed-input input {
    width: 100%;
    padding: 0.375rem 0.5rem;
    font-size: 0.8125rem;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-primary);
  }

  .seed-input input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .btn-small {
    padding: 0.375rem 0.75rem;
    font-size: 0.75rem;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-primary);
    cursor: pointer;
  }

  .btn-small:hover {
    background: var(--bg-secondary);
    border-color: var(--text-secondary);
  }

  .regenerate-row {
    padding: 0.5rem 1rem;
  }

  .btn-primary {
    width: 100%;
    padding: 0.5rem 1rem;
    font-size: 0.8125rem;
    font-weight: 500;
    background: var(--accent);
    border: none;
    border-radius: 4px;
    color: var(--bg-primary);
    cursor: pointer;
  }

  .btn-primary:hover {
    filter: brightness(1.1);
  }

  .auto-regenerate {
    padding: 0.375rem 1rem;
  }

  .auto-regenerate label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .checkbox-row {
    padding: 0.375rem 1rem;
  }

  .checkbox-row label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
    color: var(--text-primary);
    cursor: pointer;
  }

  .select-row {
    padding: 0.375rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .select-row label {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .select-row select {
    width: 100%;
    padding: 0.375rem 0.5rem;
    font-size: 0.8125rem;
    background: var(--bg-tertiary);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text-primary);
  }

  .select-row select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .stats {
    padding: 0.5rem 1rem;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }

  .stat {
    display: flex;
    justify-content: space-between;
    padding: 0.375rem 0.5rem;
    background: var(--bg-tertiary);
    border-radius: 4px;
  }

  .stat-label {
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .stat-value {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--accent);
    text-align: right;
  }

  .status-list {
    margin: 0 1rem 0.75rem;
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    background: var(--bg-tertiary);
    border-radius: 4px;
    overflow-wrap: anywhere;
  }

  .layer-header {
    margin: 0.5rem 1rem 0.25rem;
    padding: 0.375rem 0.5rem;
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    background: var(--bg-tertiary);
    border-radius: 4px;
    color: var(--text-primary);
    font-size: 0.75rem;
    font-weight: 600;
  }

  .layer-header span:last-child {
    color: var(--text-secondary);
    font-weight: 500;
    text-align: right;
  }
</style>
