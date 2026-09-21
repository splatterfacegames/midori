import { useMemo } from 'react';
import { Viewport3D } from '@jethac/tools-frontend-stack/viewports';
import type { Viewport3DController } from '@jethac/tools-frontend-stack/viewports';
import { natureToDescriptors } from '../meshes';
import type { WorkbenchState } from '../model';

export function NaturePanel({
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
  const preview = state.naturePreview;
  const meshes = useMemo(
    () => (preview ? natureToDescriptors(preview, state.seed) : []),
    [preview, state.seed],
  );
  const total = preview?.stats.scatter_instance_count ?? 0;
  return (
    <Viewport3D
      meshes={meshes}
      units="m"
      grid={grid}
      onReady={onReady}
      onError={onError}
    >
      <div className="midori-view-badge">
        {preview
          ? `${preview.stats.tile_size}M TILE · ${preview.stats.prototype_count} PROTOTYPES · ${total.toLocaleString()} SCATTER`
          : 'NO PATCH'}
      </div>
      {!preview && (
        <div className="midori-viewport-empty">
          <span>3D</span>
          <p>Load a nature preset to preview the patch.</p>
        </div>
      )}
    </Viewport3D>
  );
}
