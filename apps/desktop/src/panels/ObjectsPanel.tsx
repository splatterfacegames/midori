import { useEffect, useState } from 'react';
import { Dice5, Eye, EyeOff } from 'lucide-react';
import type { MaterialMaps } from '../engine';
import type { WorkbenchState } from '../model';

/** Blob URLs for a map set, revoked when the inputs change. Creation lives
 *  in the effect — under StrictMode's remount a useMemo-made URL set would
 *  be revoked by the first cleanup while the DOM still references it. */
function useMapUrls(maps: MaterialMaps | null, atlas?: Uint8Array): Record<string, string> {
  const [urls, setUrls] = useState<Record<string, string>>({});
  useEffect(() => {
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
    setUrls(out);
    return () => {
      for (const url of Object.values(out)) URL.revokeObjectURL(url);
    };
  }, [maps, atlas]);
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
    <div className="midori-panel-content">
      <div className="midori-section-title">
        LOD LEVELS <span>{state.lods?.length ?? 0}</span>
      </div>
      {!state.lods?.length && <p className="midori-subtle">No meshes generated.</p>}
      <div className="midori-object-list" role="radiogroup" aria-label="Preview LOD level">
        {state.lods?.map((lod, index) => (
          <button
            key={lod.name}
            type="button"
            role="radio"
            aria-checked={state.selectedLod === index}
            className="midori-object-row"
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

      <div className="midori-section-title">LAYERS</div>
      <div className="midori-layer-row">
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

      <div className="midori-section-title">MATERIALS</div>
      {state.maps ? (
        <div className="midori-materials-strip">
          {(
            [
              ['bark_albedo', 'Bark'],
              ['bark_normal', 'Normal'],
              ['leaf_card', 'Leaf card'],
            ] as const
          ).map(([key, label]) => (
            <figure key={key} className="midori-material-thumb">
              <img src={mapUrls[key]} alt={label} />
              <figcaption>{label}</figcaption>
            </figure>
          ))}
          {mapUrls.impostor ? (
            <figure className="midori-material-thumb midori-material-thumb-wide">
              <img src={mapUrls.impostor} alt="Impostor atlas" />
              <figcaption>Impostor · {selectedLod?.name}</figcaption>
            </figure>
          ) : null}
        </div>
      ) : (
        <p className="midori-subtle">No maps generated yet.</p>
      )}

      <div className="midori-section-title">
        VARIANTS <span>{state.variants.length}</span>
      </div>
      <div className="midori-variant-grid" role="radiogroup" aria-label="Variant seed">
        {state.variants.map((seed) => (
          <button
            key={seed}
            type="button"
            role="radio"
            aria-checked={state.seed === seed}
            className="midori-variant"
            onClick={() => onSelectVariant(seed)}
          >
            {seed}
          </button>
        ))}
        <button type="button" className="midori-variant midori-variant-new" onClick={onNewVariant} title="Generate next seed">
          <Dice5 size={14} aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}
