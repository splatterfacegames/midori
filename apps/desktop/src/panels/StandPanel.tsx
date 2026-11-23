import type { Viewport3DController } from '@jethac/tools-frontend-stack/viewports';
import { FoliageViewport } from '../FoliageViewport';
import type { WorkbenchState } from '../model';

export function StandPanel({
  state,
  grid,
  onSelectStandLod,
  onToggleTrack,
  onReady,
  onError,
}: {
  state: WorkbenchState;
  grid: boolean;
  onSelectStandLod: (lod: 0 | 1 | 2) => void;
  onToggleTrack: () => void;
  onReady: (controller: Viewport3DController) => void;
  onError: (error: Error) => void;
}) {
  const preview = state.standPreview;
  const activeTris =
    preview == null
      ? 0
      : state.standLod === 0
        ? preview.stats.triangles_lod0
        : state.standLod === 1
          ? preview.stats.triangles_lod1
          : preview.stats.triangles_lod2;
  const coverPct = preview ? Math.round(preview.stats.achieved_total_cover * 100) : 0;

  return (
    <FoliageViewport
      mode="stand"
      standPreview={preview}
      standLod={state.standLod}
      grid={grid}
      onReady={onReady}
      onError={onError}
    >
      <div className="midori-view-badge">
        {preview
          ? `${preview.tile_size_m}M STAND · LOD ${state.standLod} · ${activeTris.toLocaleString()} TRIS · ${preview.stats.total_stems} STEMS · ${coverPct}% COVER · OPAQUE`
          : 'NO STAND'}
      </div>
      {preview && (
        <div className="midori-state-hud" role="group" aria-label="Volumetric stand controls">
          <div className="midori-hud-group">
            {([0, 1, 2] as const).map((level) => (
              <button
                key={level}
                type="button"
                className={state.standLod === level ? 'active' : undefined}
                aria-pressed={state.standLod === level}
                onClick={() => onSelectStandLod(level)}
                title={`Stand LOD ${level}`}
              >
                LOD {level}
              </button>
            ))}
            <button
              type="button"
              className={state.standTrack ? 'active' : undefined}
              aria-pressed={state.standTrack}
              onClick={onToggleTrack}
              title="Toggle winding bush track & clearing exclusion mask"
            >
              Track edge
            </button>
          </div>
        </div>
      )}
      {!preview && (
        <div className="midori-viewport-empty">
          <span>3D</span>
          <p>Load a volumetric stand preset to preview multi-stratum bushland.</p>
        </div>
      )}
    </FoliageViewport>
  );
}
