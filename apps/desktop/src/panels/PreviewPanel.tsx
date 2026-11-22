import type { Viewport3DController } from '@jethac/tools-frontend-stack/viewports';
import { FoliageViewport } from '../FoliageViewport';
import type { WorkbenchState } from '../model';

export function PreviewPanel({
  state,
  grid,
  onSelectState,
  onReady,
  onError,
}: {
  state: WorkbenchState;
  grid: boolean;
  onSelectState: (stateName: string | null) => void;
  onReady: (controller: Viewport3DController) => void;
  onError: (error: Error) => void;
}) {
  const lod = state.lods?.[state.selectedLod] ?? null;
  const speciesName = state.json?.species.name.toLowerCase() ?? '';
  const isTopdown =
    speciesName.includes('topdown') ||
    speciesName.includes('top-down') ||
    state.json?.leaves.geometry === 'canopy_card';

  return (
    <FoliageViewport
      mode="tree"
      lod={lod}
      maps={state.maps}
      layers={state.layers}
      isTopdown={isTopdown}
      grid={grid}
      onReady={onReady}
      onError={onError}
    >
      <div className="midori-view-badge">
        {lod ? `${lod.name.toUpperCase()} · ${lod.triangle_count.toLocaleString()} TRIS` : 'NO MESH'}
      </div>
      {state.availableStates.length > 0 && (
        <div className="midori-state-hud" role="group" aria-label="Species state">
          <div className="midori-hud-group">
            <button
              type="button"
              className={state.selectedState === null ? 'active' : undefined}
              aria-pressed={state.selectedState === null}
              onClick={() => onSelectState(null)}
              title="Default species state"
            >
              default
            </button>
            {state.availableStates.map((st) => (
              <button
                key={st}
                type="button"
                className={state.selectedState === st ? 'active' : undefined}
                aria-pressed={state.selectedState === st}
                onClick={() => onSelectState(st)}
                title={`[states.${st}]`}
              >
                {st.replace(/_/g, ' ')}
              </button>
            ))}
          </div>
        </div>
      )}
      {!lod && (
        <div className="midori-viewport-empty">
          <span>3D</span>
          <p>Generated LODs appear here.</p>
        </div>
      )}
    </FoliageViewport>
  );
}
