import { useMemo } from 'react';
import { Viewport3D } from '@jethac/tools-frontend-stack/viewports';
import type { Viewport3DController } from '@jethac/tools-frontend-stack/viewports';
import { lodToDescriptors } from '../meshes';
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
  const meshes = useMemo(
    () =>
      lod
        ? lodToDescriptors(lod, state.seed, state.layers, {
            bark: state.maps?.bark_albedo.rgba,
            leaves: state.maps?.leaf_card.rgba,
            impostor: lod.impostor_atlas?.rgba,
          })
        : [],
    [lod, state.seed, state.layers, state.maps],
  );
  return (
    <Viewport3D
      meshes={meshes}
      units="m"
      grid={grid}
      onReady={onReady}
      onError={onError}
    >
      <div className="grove-view-badge">
        {lod ? `${lod.name.toUpperCase()} · ${lod.triangle_count.toLocaleString()} TRIS` : 'NO MESH'}
      </div>
      {!lod && (
        <div className="grove-viewport-empty">
          <span>3D</span>
          <p>Generated LODs appear here.</p>
        </div>
      )}
    </Viewport3D>
  );
}
