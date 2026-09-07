import { useRef } from "react";
import { Button, Input, Select } from "@jethac/tools-frontend-stack/ui";
import type { LodLevelView } from "./contract";
import type { SliceState } from "./slice";

/**
 * Panel IDs are stable identifiers persisted in saved layouts and carried across
 * window transfers. They match the shared named-slot default layout, so a saved
 * layout from the reference workbench still resolves.
 */
export const PANEL_IDS = [
  "library",
  "view-3d",
  "view-2d",
  "objects",
  "inspector",
  "bottom",
] as const;
export type PanelId = (typeof PANEL_IDS)[number];

const field: React.CSSProperties = { display: "grid", gap: 4, fontSize: 12 };
const pad: React.CSSProperties = { padding: "12px 14px", display: "grid", gap: 12, fontSize: 13 };
const mono: React.CSSProperties = {
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
  fontSize: 11,
};
const dim: React.CSSProperties = { opacity: 0.7 };

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div style={{ display: "grid", gridTemplateColumns: "132px 1fr", gap: 8, fontSize: 12 }}>
      <span style={dim}>{label}</span>
      <span style={mono}>{value}</span>
    </div>
  );
}

export interface SourcePanelProps {
  state: SliceState;
  presets: string[];
  engineAvailable: boolean;
  engineReason?: string;
  busy: boolean;
  onOpenPreset: (id: string) => void;
  onOpenFile: (file: File) => void;
  onSeedChange: (seed: string) => void;
  onGenerate: () => void;
}

/**
 * Source and seed. Nothing else is editable in this slice: the structural
 * inspector, graph edges, Show Normals and the nature Random control all have
 * source-observed defects in the legacy frontend, and shipping them here as if
 * finished would be porting known-broken behaviour.
 */
export function SourcePanel(props: SourcePanelProps) {
  const { state } = props;
  const fileInput = useRef<HTMLInputElement>(null);

  return (
    <div style={pad}>
      {!props.engineAvailable && (
        <div
          style={{
            padding: 10,
            border: "1px solid var(--tf-color-border, #666)",
            borderRadius: 6,
            fontSize: 12,
            lineHeight: 1.5,
          }}
        >
          <strong>No generator in this host.</strong>
          <div style={{ ...dim, marginTop: 4 }}>{props.engineReason}</div>
        </div>
      )}

      <label style={field}>
        <span style={dim}>Preset</span>
        <Select
          value=""
          disabled={!props.engineAvailable || props.presets.length === 0}
          onChange={(value) => {
            const id = Array.isArray(value) ? value[0] : value;
            if (id) props.onOpenPreset(id);
          }}
          options={[
            { value: "", label: "Choose a checked-in preset..." },
            ...props.presets.map((id) => ({ value: id, label: id })),
          ]}
        />
      </label>

      <div style={field}>
        <span style={dim}>Local species TOML</span>
        <input
          ref={fileInput}
          type="file"
          accept=".toml,text/plain"
          style={{ display: "none" }}
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) props.onOpenFile(file);
            event.target.value = "";
          }}
        />
        <Button variant="secondary" onClick={() => fileInput.current?.click()}>
          Open file...
        </Button>
      </div>

      {state.source && (
        <div style={{ display: "grid", gap: 4 }}>
          <Row label="opened" value={state.source.displayName} />
          <Row label="origin" value={state.source.origin} />
          <Row label="bytes" value={state.source.bytes.length.toLocaleString()} />
          <Row label="sha256" value={state.source.sha256} />
        </div>
      )}

      <label style={field}>
        <span style={dim}>Seed (exact 64-bit decimal)</span>
        <Input
          value={state.seedText}
          inputMode="numeric"
          spellCheck={false}
          onChange={(event) => props.onSeedChange(event.target.value)}
        />
      </label>

      <Button
        onClick={props.onGenerate}
        disabled={!state.source || props.busy || !props.engineAvailable}
      >
        {props.busy ? "Generating..." : "Generate"}
      </Button>

      {state.status === "error" && (
        <div style={{ ...mono, color: "var(--tf-color-danger, #d66)", whiteSpace: "pre-wrap" }}>
          {state.error}
        </div>
      )}

      {state.source && (
        <details>
          <summary style={{ ...dim, fontSize: 12, cursor: "pointer" }}>Source text</summary>
          <pre style={{ ...mono, maxHeight: 260, overflow: "auto", whiteSpace: "pre-wrap" }}>
            {new TextDecoder().decode(state.source.bytes)}
          </pre>
        </details>
      )}
    </div>
  );
}

/** Summary of the one generation that produced the referenced GLB. */
export function ResultPanel({ state }: { state: SliceState }) {
  const result = state.result;
  if (!result) {
    return (
      <div style={pad}>
        <span style={dim}>
          {state.status === "error"
            ? "The last input did not generate. Its error is shown in Source."
            : "Open a source and generate to see a result."}
        </span>
      </div>
    );
  }
  const { summary, glb, engine } = result;
  const size = (v: number[]) => v.map((n) => n.toFixed(2)).join(", ");
  return (
    <div style={pad}>
      <Row label="species" value={summary.speciesName} />
      <Row label="scientific" value={summary.scientificName} />
      <Row label="seed" value={result.seedDecimal} />
      <Row label="source sha256" value={result.sourceSha256} />
      <Row label="stems" value={summary.stemCount.toString()} />
      <Row label="branches" value={summary.branchCount.toString()} />
      <Row label="leaves" value={summary.leafCount.toString()} />
      <Row label="bounds min" value={size(summary.boundsMin)} />
      <Row label="bounds max" value={size(summary.boundsMax)} />
      <hr style={{ border: 0, borderTop: "1px solid currentColor", opacity: 0.15 }} />
      <Row label="glb kind" value={glb.kind} />
      <Row label="glb bytes" value={glb.byteCount.toString()} />
      <Row label="glb sha256" value={glb.sha256} />
      <div style={{ ...dim, fontSize: 11, lineHeight: 1.5 }}>
        The result references the GLB by digest and byte count. The bytes are not in
        this envelope; they travel over a separate digest-checked transfer.
      </div>
      <hr style={{ border: 0, borderTop: "1px solid currentColor", opacity: 0.15 }} />
      <Row label="engine revision" value={engine.sourceRevision} />
      <Row label="engine build" value={engine.buildId} />
      <div style={{ ...dim, fontSize: 11, lineHeight: 1.5 }}>
        Engine identity is a value the host supplied. It is carried, not attested.
      </div>
    </div>
  );
}

const MATERIAL_COLOR: Record<string, string> = {
  Bark: "#8a6a44",
  Leaves: "#4f8f4a",
};

function LodBar({ lod }: { lod: LodLevelView }) {
  const total = lod.submeshes.reduce((sum, range) => sum + range.indexCount, 0) || 1;
  return (
    <div style={{ display: "flex", height: 10, borderRadius: 3, overflow: "hidden" }}>
      {lod.submeshes.map((range, slot) => (
        <div
          key={slot}
          title={`${range.materialName}: indices ${range.indexOffset}..${range.indexOffset + range.indexCount}`}
          style={{
            width: `${(range.indexCount / total) * 100}%`,
            background: MATERIAL_COLOR[range.materialName] ?? "#888",
          }}
        />
      ))}
    </div>
  );
}

/** Per-LOD geometry, broken down by the material each index range draws with. */
export function LodPanel({ state }: { state: SliceState }) {
  const result = state.result;
  if (!result) {
    return (
      <div style={pad}>
        <span style={dim}>No LODs yet.</span>
      </div>
    );
  }
  return (
    <div style={pad}>
      <div style={{ ...dim, fontSize: 11, lineHeight: 1.5 }}>
        Balanced LOD set. <code>name</code> is the engine&rsquo;s threshold name; the
        exported GLB names its nodes <code>&lt;Species&gt;_LOD&lt;index&gt;</code>
        instead, with the thresholds in root extras rather than an MSFT_lod hierarchy.
      </div>
      {result.lods.map((lod) => (
        <div key={lod.index} style={{ display: "grid", gap: 6 }}>
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12 }}>
            <strong>
              LOD {lod.index} &middot; {lod.name}
            </strong>
            <span style={dim}>screen height {lod.screenHeight}</span>
          </div>
          <LodBar lod={lod} />
          <div style={{ ...mono, ...dim }}>
            {lod.vertexCount.toLocaleString()} vertices, {lod.indexCount.toLocaleString()} indices
          </div>
          <table style={{ ...mono, borderCollapse: "collapse", width: "100%" }}>
            <thead>
              <tr style={dim}>
                <th style={{ textAlign: "left" }}>material</th>
                <th style={{ textAlign: "right" }}>offset</th>
                <th style={{ textAlign: "right" }}>count</th>
              </tr>
            </thead>
            <tbody>
              {lod.submeshes.map((range, slot) => (
                <tr key={slot}>
                  <td>
                    <span
                      style={{
                        display: "inline-block",
                        width: 8,
                        height: 8,
                        marginRight: 6,
                        background: MATERIAL_COLOR[range.materialName] ?? "#888",
                      }}
                    />
                    {range.materialName}
                  </td>
                  <td style={{ textAlign: "right" }}>{range.indexOffset.toLocaleString()}</td>
                  <td style={{ textAlign: "right" }}>{range.indexCount.toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
      <div style={{ ...dim, fontSize: 11, lineHeight: 1.5 }}>
        Bark and Leaves are the only roles public <code>midori_core::MaterialType</code>
        has. The legacy frontend also showed Twig and Frond labels; those are not
        engine variants and are not invented here.
      </div>
    </div>
  );
}

/** An honest account of what this slice does not draw. */
export function PreviewPanel() {
  return (
    <div style={{ ...pad, alignContent: "start", lineHeight: 1.6 }}>
      <strong>No rendered preview in this slice.</strong>
      <div style={dim}>
        The shared viewport baseline exposes a <code>MeshDescriptor</code> of positions,
        indices, normals and colour. It does not yet expose GLB loading, UV or
        submesh materials, wind hooks or instancing, so a viewport mounted here
        could only show a flattened, solid-coloured approximation. The design this
        slice implements says explicitly that such a preview does not pass, so
        rather than ship one, this panel says what is missing.
      </div>
      <div style={dim}>
        Material-aware geometry is inspected instead: the LODs panel shows every
        index range and the material it draws with, and Export &amp; verify reopens
        the written file and reports its actual nodes, primitives, materials and
        attribute semantics.
      </div>
      <div style={dim}>
        Wind is Midori-owned and has no hook proof yet; it is not advertised as
        working.
      </div>
    </div>
  );
}

export interface ExportPanelProps {
  state: SliceState;
  busy: boolean;
  exportPath?: string;
  report?: string;
  onExport: () => void;
  onVerify: () => void;
  onDownload: () => void;
  canDownload: boolean;
}

export function ExportPanel(props: ExportPanelProps) {
  const ready = props.state.status === "ready" && props.state.result !== undefined;
  return (
    <div style={pad}>
      {!ready && (
        <span style={dim}>
          Export becomes available when a result matches the source and seed on screen.
          Editing either invalidates it.
        </span>
      )}
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Button onClick={props.onExport} disabled={!ready || props.busy}>
          Export GLB
        </Button>
        <Button
          variant="secondary"
          onClick={props.onVerify}
          disabled={!ready || !props.exportPath || props.busy}
        >
          Reopen and verify
        </Button>
        <Button
          variant="secondary"
          onClick={props.onDownload}
          disabled={!ready || !props.canDownload || props.busy}
        >
          Fetch bytes
        </Button>
      </div>
      {props.exportPath && <Row label="written to" value={props.exportPath} />}
      {props.report && (
        <pre
          style={{
            ...mono,
            whiteSpace: "pre-wrap",
            maxHeight: 420,
            overflow: "auto",
            margin: 0,
          }}
        >
          {props.report}
        </pre>
      )}
    </div>
  );
}

export function OutputPanel({ state }: { state: SliceState }) {
  return (
    <div style={{ padding: "8px 12px", ...mono, display: "grid", gap: 2 }}>
      {state.log.length === 0 && <span style={dim}>Nothing yet.</span>}
      {state.log
        .slice()
        .reverse()
        .map((entry, index) => (
          <div
            key={`${entry.at}-${index}`}
            style={{ color: entry.level === "error" ? "var(--tf-color-danger, #d66)" : undefined }}
          >
            <span style={dim}>{new Date(entry.at).toLocaleTimeString()} </span>
            {entry.text}
          </div>
        ))}
    </div>
  );
}
