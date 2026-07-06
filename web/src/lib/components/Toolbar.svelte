<script lang="ts">
  import { treeStore } from '$lib/stores/tree';
  import { editorStore } from '$lib/stores/editor';

  let fileInput: HTMLInputElement;

  function isNaturePatchToml(text: string): boolean {
    return /\bkind\s*=\s*"nature_patch"/.test(text) || text.includes('[asset]');
  }

  async function loadToml(text: string) {
    if (isNaturePatchToml(text)) {
      await treeStore.loadNaturePatch(text);
    } else {
      await treeStore.loadSpecies(text);
    }
  }

  async function loadFile() {
    const file = fileInput.files?.[0];
    if (!file) return;

    const text = await file.text();
    await loadToml(text);
  }

  async function loadPreset(name: string) {
    try {
      console.log(`Loading preset: ${name}`);
      const response = await fetch(`/presets/${name}.toml`);
      if (!response.ok) {
        throw new Error(`Failed to fetch preset: ${response.status}`);
      }
      const text = await response.text();
      console.log(`Preset loaded, first 100 chars:`, text.substring(0, 100));
      await loadToml(text);
      console.log(`Species loaded successfully`);
    } catch (e) {
      console.error('Error loading preset:', e);
      alert(`Error loading preset: ${e}`);
    }
  }

  async function loadNaturePreset(name: string) {
    try {
      const response = await fetch(`/presets/nature/${name}.toml`);
      if (!response.ok) {
        throw new Error(`Failed to fetch nature preset: ${response.status}`);
      }
      await treeStore.loadNaturePatch(await response.text());
    } catch (e) {
      console.error('Error loading nature preset:', e);
      alert(`Error loading nature preset: ${e}`);
    }
  }

  async function exportGlb() {
    const blob = await treeStore.exportGlb();
    if (!blob) {
      alert('Export failed - make sure a tree is generated first');
      return;
    }

    // Create download link
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `tree_${Date.now()}.glb`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }

  $: lods = $treeStore.meshData?.lods ?? [];

  function setManualLod(value: string) {
    editorStore.setLodMode('manual');
    editorStore.setLod(parseInt(value, 10));
  }

  function setLodMode(value: string) {
    editorStore.setLodMode(value === 'auto' ? 'auto' : 'manual');
  }
</script>

<div class="toolbar">
  <div class="brand">
    <span class="icon">🌳</span>
    <span class="name">Midori</span>
  </div>

  <div class="actions">
    <div class="dropdown">
      <button>Presets</button>
      <div class="dropdown-content">
        <button on:click={() => loadPreset('oak')}>Oak</button>
        <button on:click={() => loadPreset('pine')}>Pine</button>
        <button on:click={() => loadPreset('palm')}>Palm</button>
        <button on:click={() => loadPreset('willow')}>Willow</button>
        <button on:click={() => loadPreset('joshua_prototype')}>Joshua Prototype</button>
        <button on:click={() => loadNaturePreset('temperate_forest_floor')}>Forest Floor</button>
        <button on:click={() => loadNaturePreset('flowering_meadow')}>Meadow</button>
        <button on:click={() => loadNaturePreset('arid_scrub')}>Arid Scrub</button>
      </div>
    </div>

    <input
      type="file"
      accept=".toml"
      bind:this={fileInput}
      on:change={loadFile}
      style="display: none"
    />
    <button on:click={() => fileInput.click()}>Open</button>

    <button on:click={exportGlb} disabled={$treeStore.previewMode === 'nature'}>Export GLB</button>
  </div>

  <div class="view-options">
    <button
      class:active={$editorStore.showWireframe}
      on:click={editorStore.toggleWireframe}
    >
      Wireframe
    </button>

    <select
      value={$editorStore.lodMode}
      on:change={(e) => setLodMode(e.currentTarget.value)}
      title="LOD mode"
    >
      <option value="manual">Manual LOD</option>
      <option value="auto">Auto LOD</option>
    </select>

    <select
      value={$editorStore.currentLod}
      disabled={$editorStore.lodMode === 'auto'}
      on:change={(e) => setManualLod(e.currentTarget.value)}
      title="Preview LOD"
    >
      {#each lods as lod, index}
        <option value={index}>{lod.name || `LOD ${index}`}</option>
      {:else}
        <option value={0}>LOD 0</option>
        <option value={1}>LOD 1</option>
        <option value={2}>LOD 2</option>
      {/each}
    </select>

    <div class="lod-buttons" aria-label="Preview LOD buttons">
      {#each lods as lod, index}
        <button
          type="button"
          data-testid={`lod-button-${index}`}
          class:active={$editorStore.currentLod === index && $editorStore.lodMode === 'manual'}
          disabled={$editorStore.lodMode === 'auto'}
          on:click={() => setManualLod(String(index))}
        >
          {lod.name || `LOD ${index}`}
        </button>
      {:else}
        <button type="button" disabled>LOD</button>
      {/each}
    </div>
  </div>

  <div class="generation">
    <button on:click={() => treeStore.regenerate()}>Regenerate</button>
    <button on:click={() => { treeStore.randomizeSeed(); treeStore.regenerate(); }}>
      Random
    </button>
  </div>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 1rem;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 600;
    font-size: 1.125rem;
  }

  .icon {
    font-size: 1.5rem;
  }

  .actions, .view-options, .generation {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .lod-buttons {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .lod-buttons button {
    min-width: 3.5rem;
    padding-inline: 0.5rem;
  }

  .lod-buttons button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .dropdown {
    position: relative;
  }

  .dropdown-content {
    display: none;
    position: absolute;
    top: 100%;
    left: 0;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.25rem;
    z-index: 100;
    flex-direction: column;
  }

  .dropdown:hover .dropdown-content {
    display: flex;
  }

  .dropdown-content button {
    width: 100%;
    text-align: left;
  }

  button.active {
    background: var(--accent);
    color: var(--bg-primary);
  }

  select:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
