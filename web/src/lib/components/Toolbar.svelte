<script lang="ts">
  import { treeStore } from '$lib/stores/tree';
  import { editorStore } from '$lib/stores/editor';

  let fileInput: HTMLInputElement;

  async function loadFile() {
    const file = fileInput.files?.[0];
    if (!file) return;

    const text = await file.text();
    await treeStore.loadSpecies(text);
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
      await treeStore.loadSpecies(text);
      console.log(`Species loaded successfully`);
    } catch (e) {
      console.error('Error loading preset:', e);
      alert(`Error loading preset: ${e}`);
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
</script>

<div class="toolbar">
  <div class="brand">
    <span class="icon">🌳</span>
    <span class="name">Grove</span>
  </div>

  <div class="actions">
    <div class="dropdown">
      <button>Presets</button>
      <div class="dropdown-content">
        <button on:click={() => loadPreset('oak')}>Oak</button>
        <button on:click={() => loadPreset('pine')}>Pine</button>
        <button on:click={() => loadPreset('palm')}>Palm</button>
        <button on:click={() => loadPreset('willow')}>Willow</button>
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

    <button on:click={exportGlb}>Export glTF</button>
  </div>

  <div class="view-options">
    <button
      class:active={$editorStore.showWireframe}
      on:click={editorStore.toggleWireframe}
    >
      Wireframe
    </button>

    <select
      value={$editorStore.currentLod}
      on:change={(e) => editorStore.setLod(parseInt(e.currentTarget.value))}
    >
      <option value={0}>LOD 0 (High)</option>
      <option value={1}>LOD 1 (Medium)</option>
      <option value={2}>LOD 2 (Low)</option>
    </select>
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
    gap: 0.5rem;
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
</style>
