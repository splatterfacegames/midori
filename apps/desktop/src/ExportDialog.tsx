import { useState } from 'react';
import type { WorkbenchState } from './model';

export interface ExportRequest {
  format: 'glb' | 'gltf';
  baseName: string;
  seeds: number[];
  /** Embed the species' material maps (bark albedo+normal, leaf card). */
  embedTextures: boolean;
}

export function ExportDialog({
  state,
  onCancel,
  onExport,
}: {
  state: WorkbenchState;
  onCancel: () => void;
  onExport: (request: ExportRequest) => void;
}) {
  const [format, setFormat] = useState<'glb' | 'gltf'>('glb');
  const [baseName, setBaseName] = useState(
    (state.json?.species.name ?? 'tree').toLowerCase().replace(/[^a-z0-9]+/g, '_') || 'tree',
  );
  const [scope, setScope] = useState<'current' | 'all'>('current');
  const [embedTextures, setEmbedTextures] = useState(true);

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    const seeds = scope === 'all' ? state.variants : [state.seed];
    onExport({ format, baseName, seeds, embedTextures });
  };

  return (
    <div className="grove-modal-backdrop" role="presentation" onClick={onCancel}>
      <form
        className="grove-modal"
        role="dialog"
        aria-label="Export glTF"
        onClick={(event) => event.stopPropagation()}
        onSubmit={submit}
      >
        <h2>Export glTF</h2>
        <label className="grove-field">
          <span className="grove-field-label">File name</span>
          <input
            type="text"
            value={baseName}
            onChange={(event) => setBaseName(event.target.value)}
            pattern="[A-Za-z0-9_\-]+"
            required
          />
        </label>
        <label className="grove-field">
          <span className="grove-field-label">Format</span>
          <select value={format} onChange={(event) => setFormat(event.target.value as 'glb' | 'gltf')}>
            <option value="glb">.glb — single binary</option>
            <option value="gltf">.gltf + .bin — separate files</option>
          </select>
        </label>
        <label className="grove-field">
          <span className="grove-field-label">Variants</span>
          <select value={scope} onChange={(event) => setScope(event.target.value as 'current' | 'all')}>
            <option value="current">Current (seed {state.seed})</option>
            <option value="all">All {state.variants.length} listed variants</option>
          </select>
        </label>
        <label className="grove-field grove-field-inline">
          <input
            type="checkbox"
            checked={embedTextures}
            onChange={(event) => setEmbedTextures(event.target.checked)}
          />
          <span className="grove-field-label">Embed material maps</span>
        </label>
        <p className="grove-subtle">
          Every LOD level is included, with Pivot Painter data in TEXCOORD_1 and COLOR_0.
          Material maps embed the species' generated bark albedo+normal and leaf card PNGs;
          baked impostor atlases are always embedded when a LOD uses crown impostors.
        </p>
        <div className="grove-modal-actions">
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
          <button type="submit">Export</button>
        </div>
      </form>
    </div>
  );
}
