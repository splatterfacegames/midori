import type { WorkbenchState } from '../model';
import { formatBytes } from '../model';

function formatBounds(state: WorkbenchState): string | null {
  const stats = state.stats;
  if (!stats) return null;
  const [x0, y0, z0] = stats.bounds_min;
  const [x1, y1, z1] = stats.bounds_max;
  const size = [x1 - x0, y1 - y0, z1 - z0].map((v) => v.toFixed(2));
  return `${size[0]} × ${size[1]} × ${size[2]} m`;
}

export function ActivityPanel({ state }: { state: WorkbenchState }) {
  const lods = state.lods ?? [];
  const totalTris = lods.reduce((sum, lod) => sum + lod.triangle_count, 0);
  const bounds = formatBounds(state);
  return (
    <div className="midori-activity">
      <div className="midori-activity-summary">
        <strong>{state.generating ? 'Generating…' : state.lods ? 'Generated' : 'Idle'}</strong>
        <p>
          {state.stats
            ? `${state.stats.stem_count} stems · ${state.stats.leaf_count} leaves · ${lods.length} LODs · ${totalTris.toLocaleString()} total tris`
            : 'No tree generated yet.'}
          {bounds ? ` · bounds ${bounds}` : ''}
          {state.lods ? ` · seed ${state.seed}` : ''}
        </p>
      </div>
      <div className="midori-log" role="log" aria-label="Activity log">
        {state.log.map((line, index) => (
          <p key={index}>{line}</p>
        ))}
      </div>
      <ExportSizeNote lods={lods} />
    </div>
  );
}

function ExportSizeNote({ lods }: { lods: WorkbenchState['lods'] }) {
  if (!lods?.length) return null;
  const lod0 = lods[0];
  const approx =
    lod0.vertex_count * (12 + 12 + 8 + 8 + 16) + lod0.indices.length * 4;
  return <p className="midori-subtle">LOD0 payload ≈ {formatBytes(approx)} before glTF packaging.</p>;
}
