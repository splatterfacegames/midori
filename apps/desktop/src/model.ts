/**
 * Grove workbench model.
 *
 * Owns the species document lifecycle: TOML source text is authoritative, the
 * serde JSON projection drives the parameter inspector, and every mutation is
 * validated by round-tripping through the engine (fromJson -> toToml) before
 * it is committed and the tree regenerated.
 */

import { Generator, loadEngine, type LodMesh, type MaterialMaps, type TreeStats } from './engine';
import { PRESETS } from './presets';
import { setParam, type BranchLevelId, type SpeciesJson, DEFAULT_BRANCH_PARAMS } from './species';

export interface ExportFile {
  name: string;
  data: Uint8Array;
}

export interface WorkbenchState {
  ready: boolean;
  generating: boolean;
  /** Display label for the loaded species document. */
  label: string;
  /** Committed species TOML source. */
  toml: string;
  /** Parsed species projection (null while no valid document is loaded). */
  json: SpeciesJson | null;
  seed: number;
  /** Seeds generated so far, oldest first. */
  variants: number[];
  lods: LodMesh[] | null;
  stats: TreeStats | null;
  selectedLod: number;
  /** Editable TOML shown in the source panel. */
  sourceDraft: string;
  sourceDirty: boolean;
  sourceError: string | null;
  paramError: string | null;
  layers: { bark: boolean; leaves: boolean };
  /** Generated material maps (PNG bytes) for the loaded species — bark
   *  albedo, bark normal, leaf card. Regenerated when `[textures]` params or
   *  the species name (default map seed) change. */
  maps: MaterialMaps | null;
  log: string[];
}

const MAX_LOG_LINES = 200;

export class GroveModel {
  private state: WorkbenchState;
  private listeners = new Set<() => void>();
  private generator: Generator | null = null;
  private regenTimer: ReturnType<typeof setTimeout> | null = null;
  /** Cache key for `state.maps` — texture params + the name-derived seed. */
  private mapsKey = '';

  private constructor(state: WorkbenchState) {
    this.state = state;
  }

  /** Load the WASM module and the default preset. */
  static async create(wasmInput?: Parameters<typeof loadEngine>[0]): Promise<GroveModel> {
    await loadEngine(wasmInput);
    const model = new GroveModel({
      ready: true,
      generating: false,
      label: 'Untitled',
      toml: '',
      json: null,
      seed: 1,
      variants: [1],
      lods: null,
      stats: null,
      selectedLod: 0,
      sourceDraft: '',
      sourceDirty: false,
      sourceError: null,
      paramError: null,
      layers: { bark: true, leaves: true },
      maps: null,
      log: [],
    });
    model.loadPreset(PRESETS[0].id);
    return model;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getState = (): WorkbenchState => this.state;

  private emit(patch: Partial<WorkbenchState>): void {
    this.state = { ...this.state, ...patch };
    for (const listener of this.listeners) listener();
  }

  private pushLog(line: string): void {
    const log = [...this.state.log, line];
    this.emit({ log: log.slice(-MAX_LOG_LINES) });
  }

  // ---- document lifecycle -------------------------------------------------

  loadPreset(id: string): void {
    const preset = PRESETS.find((entry) => entry.id === id);
    if (!preset) return;
    this.commitToml(preset.toml, `Preset · ${preset.label}`);
  }

  importToml(text: string, label: string): boolean {
    return this.commitToml(text, `Custom · ${label}`);
  }

  /** Parse + commit TOML; returns false and records the error on failure. */
  private commitToml(toml: string, label: string): boolean {
    try {
      const generator = Generator.fromToml(toml);
      this.generator?.free();
      this.generator = generator;
      const json = generator.toJson() as SpeciesJson;
      this.emit({
        label,
        toml,
        json,
        sourceDraft: toml,
        sourceDirty: false,
        sourceError: null,
        paramError: null,
      });
      this.pushLog(`Loaded ${generator.name}`);
      this.refreshMaps();
      this.regenerate();
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.emit({ sourceError: message });
      this.pushLog(`Load failed: ${message}`);
      return false;
    }
  }

  /** Regenerate material maps when the texture-affecting params changed.
   *  Maps are deterministic in the species doc, so re-rendering an unchanged
   *  section is skipped. */
  private refreshMaps(): void {
    if (!this.generator || !this.state.json) return;
    const key = JSON.stringify([this.state.json.species.name, this.state.json.textures]);
    if (key === this.mapsKey && this.state.maps) return;
    this.mapsKey = key;
    try {
      const maps = this.generator.generateMaps();
      this.emit({ maps });
    } catch (error) {
      this.emit({ maps: null });
      this.pushLog(`Map generation failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  // ---- generation ----------------------------------------------------------

  regenerate(): void {
    if (!this.generator) return;
    this.emit({ generating: true });
    try {
      const lods = this.generator.generate(this.state.seed);
      const stats = this.generator.stats(this.state.seed);
      const selectedLod = Math.min(this.state.selectedLod, Math.max(0, lods.length - 1));
      this.emit({ lods, stats, selectedLod, generating: false });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.emit({ generating: false });
      this.pushLog(`Generation failed: ${message}`);
    }
  }

  private scheduleRegenerate(): void {
    if (this.regenTimer) clearTimeout(this.regenTimer);
    this.regenTimer = setTimeout(() => {
      this.regenTimer = null;
      this.regenerate();
    }, 120);
  }

  setSeed(seed: number): void {
    if (!Number.isFinite(seed)) return;
    const next = Math.max(0, Math.floor(seed));
    const variants = this.state.variants.includes(next)
      ? this.state.variants
      : [...this.state.variants, next];
    this.emit({ seed: next, variants });
    this.regenerate();
  }

  /** Append a fresh variant (current seed + 1) and switch to it. */
  nextVariant(): void {
    this.setSeed(this.state.seed + 1);
  }

  selectLod(index: number): void {
    this.emit({ selectedLod: index });
  }

  toggleLayer(layer: 'bark' | 'leaves'): void {
    this.emit({ layers: { ...this.state.layers, [layer]: !this.state.layers[layer] } });
  }

  // ---- parameter editing ---------------------------------------------------

  /** Edit one parameter by dotted path, validate through the engine, commit. */
  updateParam(path: string, value: unknown): void {
    if (!this.state.json) return;
    const json = setParam(this.state.json, path, value);
    this.commitJson(json);
  }

  setBranchLevel(level: BranchLevelId, enabled: boolean): void {
    if (!this.state.json) return;
    const json = setParam(this.state.json, `branches.${level}`, enabled ? DEFAULT_BRANCH_PARAMS : undefined);
    // setParam writes undefined values which serde treats as missing.
    this.commitJson(json);
  }

  private commitJson(json: SpeciesJson): void {
    try {
      const generator = Generator.fromJson(json);
      this.generator?.free();
      this.generator = generator;
      const toml = generator.toToml();
      this.emit({
        toml,
        json,
        sourceDraft: this.state.sourceDirty ? this.state.sourceDraft : toml,
        paramError: null,
      });
      this.refreshMaps();
      this.scheduleRegenerate();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.emit({ paramError: message });
    }
  }

  // ---- source panel ---------------------------------------------------------

  editSource(text: string): void {
    this.emit({ sourceDraft: text, sourceDirty: text !== this.state.toml });
  }

  applySource(): boolean {
    const draft = this.state.sourceDraft;
    try {
      const generator = Generator.fromToml(draft);
      this.generator?.free();
      this.generator = generator;
      const json = generator.toJson() as SpeciesJson;
      const toml = generator.toToml();
      this.emit({
        toml,
        json,
        sourceDraft: toml,
        sourceDirty: false,
        sourceError: null,
        paramError: null,
      });
      this.pushLog(`Source applied · ${generator.name}`);
      this.refreshMaps();
      this.regenerate();
      return true;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.emit({ sourceError: message });
      return false;
    }
  }

  revertSource(): void {
    this.emit({ sourceDraft: this.state.toml, sourceDirty: false, sourceError: null });
  }

  // ---- export ----------------------------------------------------------------

  /** Build the export file set for a seed (all LODs, current species). */
  exportFiles(seed: number, format: 'glb' | 'gltf', baseName: string, embedTextures = true): ExportFile[] {
    if (!this.generator) throw new Error('No species loaded');
    const stem = baseName.replace(/\.(glb|gltf)$/i, '') || 'tree';
    if (format === 'glb') {
      return [{ name: `${stem}.glb`, data: this.generator.exportGlb(seed, embedTextures) }];
    }
    const parts = this.generator.exportGltf(seed, `${stem}.bin`, embedTextures);
    return [
      { name: `${stem}.gltf`, data: parts.gltf },
      { name: `${stem}.bin`, data: parts.bin },
    ];
  }

  recordExport(files: ExportFile[], destination: string): void {
    const summary = files.map((file) => `${file.name} (${formatBytes(file.data.length)})`).join(', ');
    this.pushLog(`Exported ${summary} → ${destination}`);
  }

  recordExportError(message: string): void {
    this.pushLog(`Export failed: ${message}`);
  }
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;
}
