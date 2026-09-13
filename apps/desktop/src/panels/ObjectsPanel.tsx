import { useEffect, useMemo } from 'react';
import { Dice5, Eye, EyeOff } from 'lucide-react';
import type { MaterialMaps } from '../engine';
import type { WorkbenchState } from '../model';

/** Blob URLs for a map set, revoked when the inputs change. */
function useMapUrls(maps: MaterialMaps | null, atlas?: Uint8Array): Record<string, string> {
  const urls = useMemo(() => {
    const out: Record<string, string> = {};
    if (maps) {
      for (const key of ['bark_albedo', 'bark_normal', 'leaf_card'] as const) {
        const png = maps[key].png;
        if (png) out[key] = URL.createObjectURL(new Blob([png as BlobPart], { type: 'image/png' }));
      }
    }
    if (atlas) {
      out.impostor = URL.createObjectURL(new Blob([atlas as BlobPart], { type: 'image/png' }));
    }
    return out;
  }, [maps, atlas]);
  useEffect(
    () => () => {
      for (const url of Object.values(urls)) URL.revokeObjectURL(url);
    },
    [urls],
  );
  return urls;
}

export function ObjectsPanel({
  state,
  onSelectLod,
  onSelectVariant,
  onNewVariant,
  onToggleLayer,
}: {
  state: WorkbenchState;
  onSelectLod: (index: number) => void;
  onSelectVariant: (seed: number) => void;
  onNewVariant: () => void;
  onToggleLayer: (layer: 'bark' | 'leaves') => void;
}) {
  const selectedLod = state.lods?.[state.selectedLod];
  const mapUrls = useMapUrls(state.maps, selectedLod?.impostor_atlas?.png);

  return (
    <div className="grove-panel-content">
      <div className="grove-section-title">
        LOD LEVELS <span>{state.lods?.length ?? 0}</span>
      </div>
      {!state.lods?.length && <p className="grove-subtle">No meshes generated.</p>}
      <div className="grove-object-list" role="radiogroup" aria-label="Preview LOD level">
        {state.lods?.map((lod, index) => (
          <button
            key={lod.name}
            type="button"
            role="radio"
            aria-checked={state.selectedLod === index}
            className="grove-object-row"
            onClick={() => onSelectLod(index)}
          >
            <span>{lod.name}</span>
            <small>
              {lod.triangle_count.toLocaleString()} tris · {lod.vertex_count.toLocaleString()} verts
              {lod.impostor_atlas ? ' · impostor' : ''}
            </small>
          </button>
        ))}
      </div>

      <div className="grove-section-title">LAYERS</div>
      <div className="grove-layer-row">
        {(['bark', 'leaves'] as const).map((layer) => (
          <button
            key={layer}
            type="button"
            aria-pressed={state.layers[layer]}
            onClick={() => onToggleLayer(layer)}
            title={`Toggle ${layer}`}
          >
            {state.layers[layer] ? <Eye size={14} /> : <EyeOff size={14} />} {layer}
          </button>
        ))}
      </div>

      <div className="grove-section-title">MATERIALS</div>
      {state.maps ? (
        <div className="grove-materials-strip">
          {(
            [
              ['bark_albedo', 'Bark'],
              ['bark_normal', 'Normal'],
              ['leaf_card', 'Leaf card'],
            ] as const
          ).map(([key, label]) => (
            <figure key={key} className="grove-material-thumb">
              <img src={mapUrls[key]} alt={label} />
              <figcaption>{label}</figcaption>
            </figure>
          ))}
          {mapUrls.impostor ? (
            <figure className="grove-material-thumb grove-material-thumb-wide">
              <img src={mapUrls.impostor} alt="Impostor atlas" />
              <figcaption>Impostor · {selectedLod?.name}</figcaption>
            </figure>
          ) : null}
        </div>
      ) : (
        <p className="grove-subtle">No maps generated yet.</p>
      )}

      <div className="grove-section-title">
        VARIANTS <span>{state.variants.length}</span>
      </div>
      <div className="grove-variant-grid" role="radiogroup" aria-label="Variant seed">
        {state.variants.map((seed) => (
          <button
            key={seed}
            type="button"
            role="radio"
            aria-checked={state.seed === seed}
            className="grove-variant"
            onClick={() => onSelectVariant(seed)}
          >
            {seed}
          </button>
        ))}
        <button type="button" className="grove-variant grove-variant-new" onClick={onNewVariant} title="Generate next seed">
          <Dice5 size={14} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
