import { FileUp, Leaf } from 'lucide-react';
import { NATURE_PRESETS, PRESETS } from '../presets';
import type { WorkbenchState } from '../model';

export function SpeciesLibrary({
  state,
  onLoadPreset,
  onLoadNaturePreset,
  onImport,
}: {
  state: WorkbenchState;
  onLoadPreset: (id: string) => void;
  onLoadNaturePreset: (id: string) => void;
  onImport: () => void;
}) {
  const active = state.label.startsWith('Preset · ') ? state.label.slice(9) : null;
  return (
    <div className="midori-panel-content">
      <div className="midori-eyebrow">SPECIES</div>
      <div className="midori-preset-list">
        {PRESETS.map((preset) => (
          <button
            key={preset.id}
            type="button"
            className="midori-preset"
            aria-pressed={active === preset.label}
            onClick={() => onLoadPreset(preset.id)}
          >
            <Leaf size={14} aria-hidden="true" />
            <span>{preset.label}</span>
            <small>{preset.id}.toml</small>
          </button>
        ))}
      </div>
      <div className="midori-eyebrow">NATURE PATCHES</div>
      <div className="midori-preset-list">
        {NATURE_PRESETS.map((preset) => (
          <button
            key={preset.id}
            type="button"
            className="midori-preset"
            aria-pressed={state.mode === 'nature' && active === preset.label}
            onClick={() => onLoadNaturePreset(preset.id)}
          >
            <Leaf size={14} aria-hidden="true" />
            <span>{preset.label}</span>
            <small>{preset.id}.toml</small>
          </button>
        ))}
      </div>
      <div className="midori-action-stack">
        <button type="button" onClick={onImport}>
          <FileUp size={14} aria-hidden="true" /> Import TOML…
        </button>
      </div>
      <div className="midori-section-title">DOCUMENT</div>
      <p className="midori-subtle">
        {state.label}
        {state.json?.species.scientific ? ` · ${state.json.species.scientific}` : ''}
      </p>
      <p className="midori-subtle">
        Parameters, TOML source and the generated mesh stay in sync. Seed {state.seed} produces this
        variant; every variant is reproducible from the document plus its seed.
      </p>
    </div>
  );
}
