import type { Viewport3DController } from '@jethac/tools-frontend-stack/viewports';
import { FoliageViewport } from '../FoliageViewport';
import type { WorkbenchState } from '../model';

export function PreviewPanel({
  state,
  grid,
  onReady,
  onError,
}: {
  state: WorkbenchState;
  grid: boolean;
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
      {!lod && (
        <div className="midori-viewport-empty">
          <span>3D</span>
          <p>Generated LODs appear here.</p>
        </div>
      )}
    </FoliageViewport>
  );
}
