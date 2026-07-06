<script lang="ts">
  import { onMount } from 'svelte';
  import Preview3D from '$lib/components/Preview3D.svelte';
  import NodeEditor from '$lib/components/NodeEditor.svelte';
  import Inspector from '$lib/components/Inspector.svelte';
  import Toolbar from '$lib/components/Toolbar.svelte';
  import { treeStore } from '$lib/stores/tree';
  import { editorStore } from '$lib/stores/editor';

  let ready = false;

  onMount(async () => {
    // Load WASM module
    await treeStore.init();
    try {
      const response = await fetch('/presets/oak.toml');
      if (response.ok) {
        await treeStore.loadSpecies(await response.text());
      }
    } catch (error) {
      console.error('Failed to load default preset:', error);
    }
    ready = true;
  });
</script>

<svelte:head>
  <title>Midori - Nature Editor</title>
</svelte:head>

<div class="app">
  <Toolbar />

  <div class="main">
    <div class="sidebar left">
      <NodeEditor />
    </div>

    <div class="preview">
      {#if ready}
        <Preview3D />
      {:else}
        <div class="loading">Loading WASM...</div>
      {/if}
    </div>

    <div class="sidebar right">
      <Inspector />
    </div>
  </div>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }

  .main {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .sidebar {
    width: 300px;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    overflow-y: auto;
  }

  .sidebar.left {
    border-right: none;
  }

  .sidebar.right {
    border-left: none;
  }

  .preview {
    flex: 1;
    position: relative;
    background: var(--bg-tertiary);
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--text-secondary);
  }
</style>
