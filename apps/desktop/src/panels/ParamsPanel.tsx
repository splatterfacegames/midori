import { useEffect, useState } from 'react';
import { CircleMinus, CirclePlus } from 'lucide-react';
import {
  PARAM_SECTIONS,
  getParam,
  type BranchLevelId,
  type ParamField,
  type ParamSection,
  type SpeciesJson,
} from '../species';
import type { WorkbenchState } from '../model';

/** Format f32-sourced values without round-trip noise (0.15000000596 → 0.15). */
export function formatParamValue(value: number): string {
  if (Number.isInteger(value)) return String(value);
  return String(Number(value.toPrecision(6)));
}

function NumberField({
  field,
  path,
  value,
  onCommit,
}: {
  field: Extract<ParamField, { kind: 'number' }>;
  path: string;
  value: number;
  onCommit: (path: string, value: number) => void;
}) {
  // Local draft so typing "4." doesn't snap back before commit.
  const [draft, setDraft] = useState(() => formatParamValue(value));
  useEffect(() => setDraft(formatParamValue(value)), [value]);
  const commit = () => {
    const parsed = field.int ? parseInt(draft, 10) : parseFloat(draft);
    if (Number.isFinite(parsed)) {
      let next = parsed;
      if (field.min !== undefined) next = Math.max(field.min, next);
      if (field.max !== undefined) next = Math.min(field.max, next);
      onCommit(path, next);
    } else {
      setDraft(formatParamValue(value));
    }
  };
  return (
    <label className="midori-field">
      <span className="midori-field-label">
        {field.label}
        {field.unit ? <em>{field.unit}</em> : null}
      </span>
      <input
        type="number"
        value={draft}
        min={field.min}
        max={field.max}
        step={field.step ?? 'any'}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === 'Enter') (event.target as HTMLInputElement).blur();
        }}
      />
    </label>
  );
}

function Section({
  section,
  json,
  onCommit,
  onToggleBranch,
}: {
  section: ParamSection;
  json: SpeciesJson;
  onCommit: (path: string, value: unknown) => void;
  onToggleBranch: (level: BranchLevelId, enabled: boolean) => void;
}) {
  const enabled = !section.branchLevel || json.branches[section.branchLevel] != null;
  return (
    <details className="midori-section" open>
      <summary>
        {section.title}
        {section.branchLevel ? (
          <button
            type="button"
            className="midori-section-toggle"
            title={enabled ? `Remove ${section.branchLevel}` : `Add ${section.branchLevel}`}
            onClick={(event) => {
              event.preventDefault();
              onToggleBranch(section.branchLevel as BranchLevelId, !enabled);
            }}
          >
            {enabled ? <CircleMinus size={14} /> : <CirclePlus size={14} />}
          </button>
        ) : null}
      </summary>
      {enabled ? (
        <div className="midori-fields">
          {section.fields.map((field) => {
            const path = `${section.id}.${field.key}`;
            const value = getParam(json, path);
            if (field.kind === 'enum') {
              return (
                <label key={field.key} className="midori-field">
                  <span className="midori-field-label">{field.label}</span>
                  <select
                    value={String(value ?? '')}
                    onChange={(event) => onCommit(path, event.target.value)}
                  >
                    {field.options.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
              );
            }
            if (field.kind === 'text') {
              return (
                <label key={field.key} className="midori-field">
                  <span className="midori-field-label">{field.label}</span>
                  <input
                    type="text"
                    value={String(value ?? '')}
                    onChange={(event) => onCommit(path, event.target.value)}
                  />
                </label>
              );
            }
            return (
              <NumberField
                key={field.key}
                field={field}
                path={path}
                value={typeof value === 'number' ? value : 0}
                onCommit={onCommit}
              />
            );
          })}
        </div>
      ) : (
        <p className="midori-subtle">Disabled — no {section.branchLevel} branches generated.</p>
      )}
    </details>
  );
}

export function ParamsPanel({
  state,
  onCommit,
  onToggleBranch,
}: {
  state: WorkbenchState;
  onCommit: (path: string, value: unknown) => void;
  onToggleBranch: (level: BranchLevelId, enabled: boolean) => void;
}) {
  const json = state.json;
  if (!json) {
    return (
      <div className="midori-panel-content">
        <p className="midori-subtle">No species loaded.</p>
      </div>
    );
  }
  return (
    <div className="midori-panel-content" aria-label="Species parameters">
      <div className="midori-eyebrow">WEBER–PENN PARAMETERS</div>
      <div className="midori-fields">
        <label className="midori-field">
          <span className="midori-field-label">Name</span>
          <input
            type="text"
            value={json.species.name}
            onChange={(event) => onCommit('species.name', event.target.value)}
          />
        </label>
        <label className="midori-field">
          <span className="midori-field-label">Scientific</span>
          <input
            type="text"
            value={json.species.scientific}
            onChange={(event) => onCommit('species.scientific', event.target.value)}
          />
        </label>
      </div>
      {PARAM_SECTIONS.map((section) => (
        <Section
          key={section.id}
          section={section}
          json={json}
          onCommit={onCommit}
          onToggleBranch={onToggleBranch}
        />
      ))}
      {state.paramError ? (
        <p className="midori-error" role="alert">
          {state.paramError}
        </p>
      ) : null}
    </div>
  );
}
