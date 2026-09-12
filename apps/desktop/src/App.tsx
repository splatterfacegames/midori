import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import {
  WorkspaceShell,
  createBrowserPreferenceAdapter,
  type ActionPresentation,
  type DockLayout,
  type MenuDefinition,
  type PreferenceAdapter,
  type SettingsRegistration,
  type ThemeName,
} from '@jethac/tools-frontend-stack/ui';
import type { WindowHostSession } from '@jethac/tools-frontend-stack/host';
import type { Viewport3DController } from '@jethac/tools-frontend-stack/viewports';
import { GroveModel } from './model';
import type { FileHost } from './files';
import { ExportDialog, type ExportRequest } from './ExportDialog';
import { SpeciesLibrary } from './panels/SpeciesLibrary';
import { ObjectsPanel } from './panels/ObjectsPanel';
import { ParamsPanel } from './panels/ParamsPanel';
import { PreviewPanel } from './panels/PreviewPanel';
import { SourcePanel } from './panels/SourcePanel';
import { ActivityPanel } from './panels/ActivityPanel';
import './app.css';

export interface GroveAppProps {
  model: GroveModel;
  fileHost: FileHost;
  windowHost?: WindowHostSession;
  onDockLayoutChange?: (layout: DockLayout) => void;
}

const menus: MenuDefinition[] = [
  {
    id: 'file',
    label: 'File',
    items: [
      { id: 'import', actionId: 'import' },
      { id: 'export', actionId: 'export' },
    ],
  },
  {
    id: 'generate',
    label: 'Generate',
    items: [
      { id: 'variant', actionId: 'variant' },
      { id: 'regenerate', actionId: 'regenerate' },
    ],
  },
  {
    id: 'view',
    label: 'View',
    items: [
      { id: 'fit', actionId: 'fit' },
      { id: 'settings', actionId: 'workspace.settings' },
    ],
  },
];

export function GroveApp({ model, fileHost, windowHost, onDockLayoutChange }: GroveAppProps) {
  const state = useSyncExternalStore(model.subscribe, model.getState);
  const [theme, setTheme] = useState<ThemeName>('dark');
  const [grid, setGrid] = useState(true);
  const [exportOpen, setExportOpen] = useState(false);
  const [viewportError, setViewportError] = useState('');
  const viewport = useRef<Viewport3DController | null>(null);
  const accepted = useRef(false);

  const preferences = useMemo<PreferenceAdapter>(() => {
    let base: PreferenceAdapter;
    try {
      base = createBrowserPreferenceAdapter();
    } catch (cause) {
      const error = cause instanceof Error ? cause : new Error(String(cause));
      base = { read: async () => ({ values: {}, error }), write: async () => ({ ok: false, error }) };
    }
    return {
      read: base.read,
      write: async (appId, version, values) => {
        const result = await base.write(appId, version, values);
        if (result.ok && appId === 'grove') {
          if (values.theme === 'dark' || values.theme === 'light') setTheme(values.theme);
          if (typeof values.grid === 'boolean') setGrid(values.grid);
        }
        return result;
      },
    };
  }, []);

  useEffect(() => {
    void preferences.read('grove', 1).then((result) => {
      if (result.values.theme === 'dark' || result.values.theme === 'light') setTheme(result.values.theme);
      if (typeof result.values.grid === 'boolean') setGrid(result.values.grid);
    });
  }, [preferences]);

  // Secondary host windows accept transferred panels once the workspace is ready.
  useEffect(() => {
    if (windowHost && !accepted.current && state.ready && !state.generating) {
      accepted.current = true;
      void windowHost.acceptPanels().catch(() => {
        accepted.current = false;
      });
    }
  }, [windowHost, state.ready, state.generating]);

  const settings = useMemo<SettingsRegistration>(
    () => ({
      appId: 'grove',
      version: 1,
      preferenceAdapter: preferences,
      appearance: { fieldId: 'theme', onValueChange: (value) => setTheme(value === 'light' ? 'light' : 'dark') },
      categories: [
        {
          id: 'workspace',
          label: 'Workspace',
          description: 'Display preferences for this device.',
          fields: [
            {
              id: 'theme',
              label: 'Theme',
              type: 'enum',
              defaultValue: 'dark',
              scope: 'device',
              choices: [
                { value: 'dark', label: 'Dark' },
                { value: 'light', label: 'Light' },
              ],
            },
            { id: 'grid', label: 'Show viewport grid', type: 'boolean', defaultValue: true, scope: 'device' },
          ],
        },
      ],
    }),
    [preferences],
  );

  const runExport = async (request: ExportRequest) => {
    setExportOpen(false);
    try {
      const files = request.seeds.flatMap((seed, index) => {
        const base = request.seeds.length > 1 ? `${request.baseName}_${index}` : request.baseName;
        return model.exportFiles(seed, request.format, base, request.embedTextures);
      });
      const destination = await fileHost.saveFiles(`${request.baseName}.${request.format}`, files);
      if (destination) model.recordExport(files, destination);
    } catch (error) {
      model.recordExportError(error instanceof Error ? error.message : String(error));
    }
  };

  const runImport = async () => {
    const picked = await fileHost.openTextFile('Import species TOML', ['toml']);
    if (picked) model.importToml(picked.text, picked.name);
  };

  const actions: ActionPresentation[] = [
    { id: 'import', label: 'Import TOML…', aliases: ['open'], onInvoke: () => void runImport() },
    {
      id: 'export',
      label: 'Export glTF…',
      aliases: ['save'],
      disabled: !state.lods?.length,
      disabledReason: 'Generate a tree first.',
      onInvoke: () => setExportOpen(true),
    },
    {
      id: 'variant',
      label: 'New variant',
      aliases: ['next seed'],
      disabled: state.generating,
      onInvoke: () => model.nextVariant(),
    },
    {
      id: 'regenerate',
      label: 'Regenerate',
      aliases: ['rebuild'],
      disabled: state.generating,
      onInvoke: () => model.regenerate(),
    },
    { id: 'fit', label: 'Fit view', onInvoke: () => viewport.current?.fitAll() },
  ];

  const docking = useMemo(
    () => ({ appId: 'grove', preferenceAdapter: preferences, windowHost }),
    [preferences, windowHost],
  );

  const viewToolbar = (
    <div className="grove-view-tools">
      {state.lods?.map((lod, index) => (
        <button
          key={lod.name}
          type="button"
          aria-pressed={state.selectedLod === index}
          onClick={() => model.selectLod(index)}
          title={`${lod.triangle_count.toLocaleString()} triangles`}
        >
          {lod.name}
        </button>
      ))}
      <button type="button" onClick={() => viewport.current?.fitAll()}>
        Fit
      </button>
    </div>
  );

  return (
    <div className="grove-app" data-theme={theme}>
      <WorkspaceShell
        title="Grove"
        theme={theme}
        actions={actions}
        menus={menus}
        settings={settings}
        docking={docking}
        onDockLayoutChange={onDockLayoutChange}
        initialLayout={{ libraryWidth: 220, rightDockWidth: 300, bottomHeight: 150 }}
        library={<SpeciesLibrary state={state} onLoadPreset={(id) => model.loadPreset(id)} onImport={() => void runImport()} />}
        objects={
          <ObjectsPanel
            state={state}
            onSelectLod={(index) => model.selectLod(index)}
            onSelectVariant={(seed) => model.setSeed(seed)}
            onNewVariant={() => model.nextVariant()}
            onToggleLayer={(layer) => model.toggleLayer(layer)}
          />
        }
        inspector={
          <ParamsPanel
            state={state}
            onCommit={(path, value) => model.updateParam(path, value)}
            onToggleBranch={(level, enabled) => model.setBranchLevel(level, enabled)}
          />
        }
        bottom={<ActivityPanel state={state} />}
        view3d={
          <PreviewPanel
            state={state}
            grid={grid}
            onReady={(controller) => {
              viewport.current = controller;
            }}
            onError={(error) => setViewportError(error.message)}
          />
        }
        view2d={
          <SourcePanel
            state={state}
            onEdit={(text) => model.editSource(text)}
            onApply={() => model.applySource()}
            onRevert={() => model.revertSource()}
          />
        }
        panelLabels={{
          library: 'Species',
          objects: 'Tree',
          inspector: 'Parameters',
          bottom: 'Activity',
        }}
        viewLabels={{ primary: 'LOD preview', secondary: 'Species source' }}
        viewToolbars={{ primary: viewToolbar }}
        headerTools={
          <span className="grove-header-species" title={state.label}>
            {state.json?.species.name ?? 'Grove'}
            {viewportError ? <em className="grove-error-inline"> · viewport: {viewportError}</em> : null}
          </span>
        }
        status={
          <>
            <span className={`grove-status-dot${state.generating ? ' grove-status-busy' : ''}`} />
            {state.generating ? 'Generating…' : `${state.label} · seed ${state.seed}`}
            <span className="grove-status-right">
              {state.lods ? `${state.lods.length} LODs · ` : ''}Weber–Penn engine · WASM
            </span>
          </>
        }
      />
      {exportOpen ? (
        <ExportDialog state={state} onCancel={() => setExportOpen(false)} onExport={(request) => void runExport(request)} />
      ) : null}
    </div>
  );
}
