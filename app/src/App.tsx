import { useCallback, useEffect, useMemo, useReducer, useState } from "react";
import type { WindowHostSession } from "@jethac/tools-frontend-stack/host";
import {
  ThemeProvider,
  ToastProvider,
  WorkspaceShell,
  createBrowserPreferenceAdapter,
  createNamedSlotLayout,
  useToast,
  type ActionPresentation,
  type DockCommand,
  type DockCommandResult,
  type DockPanel,
  type MenuDefinition,
  type SettingsRegistration,
  type ThemeName,
} from "@jethac/tools-frontend-stack/ui";
import "@jethac/tools-frontend-stack/ui/styles.css";
import { sha256Hex } from "./contract";
import { createEngine, type TreeEngine } from "./engine";
import { APP_ID, currentLayout, isTauri } from "./host";
import {
  ExportPanel,
  LodPanel,
  OutputPanel,
  PANEL_IDS,
  PreviewPanel,
  ResultPanel,
  SourcePanel,
  type PanelId,
} from "./panels";
import { initialState, isExportable, nextToken, reduce, type SourceDoc } from "./slice";

type ThemeChoice = "system" | ThemeName;

const SETTINGS_VERSION = 1;
const preferenceAdapter = createBrowserPreferenceAdapter();

let requestSequence = 0;
function nextRequestId(prefix: string): string {
  requestSequence += 1;
  return `${prefix}-${Date.now().toString(36)}-${requestSequence.toString(36)}`;
}

function resolveTheme(choice: ThemeChoice, systemDark: boolean): ThemeName {
  return choice === "system" ? (systemDark ? "dark" : "light") : choice;
}

function useSystemDark() {
  const [dark, setDark] = useState(
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const listener = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", listener);
    return () => query.removeEventListener("change", listener);
  }, []);
  return dark;
}

export interface AppProps {
  host?: WindowHostSession;
  hostError?: string;
  /** Injectable so tests can drive the workflow without a native host. */
  engine?: TreeEngine;
}

export function App(props: AppProps) {
  return (
    <ToastProvider>
      <Midori {...props} />
    </ToastProvider>
  );
}

function Midori({ host, hostError, engine: injected }: AppProps) {
  const { addToast } = useToast();
  const systemDark = useSystemDark();
  const [themeChoice, setThemeChoice] = useState<ThemeChoice>("system");
  const [dockCommand, setDockCommand] = useState<DockCommand>();
  const theme = resolveTheme(themeChoice, systemDark);
  const secondary = host?.identity.secondary ?? false;

  const engine = useMemo(() => injected ?? createEngine(), [injected]);
  const [state, dispatch] = useReducer(reduce, initialState);
  const [presets, setPresets] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [exportPath, setExportPath] = useState<string>();
  const [report, setReport] = useState<string>();

  useEffect(() => {
    let cancelled = false;
    engine
      .presetIds()
      .then((ids) => {
        if (!cancelled) setPresets(ids);
      })
      .catch((error) => {
        if (!cancelled) dispatch({ type: "note", level: "error", text: String(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [engine]);

  useEffect(() => {
    if (!engine.available && engine.unavailableReason) {
      dispatch({ type: "note", level: "error", text: engine.unavailableReason });
    }
  }, [engine]);

  // Restore the saved appearance choice. The settings screen keeps it current.
  useEffect(() => {
    let cancelled = false;
    preferenceAdapter
      .read(APP_ID, SETTINGS_VERSION)
      .then(({ values, error }) => {
        if (cancelled) return;
        if (error) {
          addToast(error.message, { type: "warning", title: "Could not load appearance settings" });
        }
        const saved = values.theme;
        if (saved === "system" || saved === "light" || saved === "dark") setThemeChoice(saved);
      })
      .catch((error) => {
        if (!cancelled) addToast(String(error), { type: "warning" });
      });
    return () => {
      cancelled = true;
    };
  }, [addToast]);

  // A secondary window takes ownership only once its shell is mounted and ready.
  useEffect(() => {
    if (!host?.identity.secondary) return;
    host
      .acceptPanels()
      .catch((error) => addToast(String(error), { type: "error", title: "Could not accept panels" }));
  }, [host, addToast]);

  useEffect(() => {
    if (hostError) {
      addToast(hostError, { type: "warning", title: "Window host unavailable", duration: 0 });
    }
  }, [hostError, addToast]);

  // Any change of input invalidates a written export as surely as it invalidates
  // the result, so the export path and its report go with it.
  const invalidateExport = useCallback(() => {
    setExportPath(undefined);
    setReport(undefined);
  }, []);

  const openBytes = useCallback(
    async (bytes: Uint8Array, displayName: string, origin: SourceDoc["origin"]) => {
      // Digest exactly the imported bytes. No parse, no normalization, no
      // round trip through a serializer.
      const sha256 = await sha256Hex(bytes);
      invalidateExport();
      dispatch({ type: "openSource", source: { bytes, sha256, displayName, origin } });
    },
    [invalidateExport],
  );

  const onOpenPreset = useCallback(
    (id: string) => {
      engine
        .presetSource(id)
        .then((bytes) => openBytes(bytes, `${id}.toml`, "preset"))
        .catch((error) => dispatch({ type: "note", level: "error", text: String(error) }));
    },
    [engine, openBytes],
  );

  const onOpenFile = useCallback(
    (file: File) => {
      file
        .arrayBuffer()
        .then((buffer) => openBytes(new Uint8Array(buffer), file.name, "file"))
        .catch((error) => dispatch({ type: "note", level: "error", text: String(error) }));
    },
    [openBytes],
  );

  const onSeedChange = useCallback(
    (seedText: string) => {
      invalidateExport();
      dispatch({ type: "editSeed", seedText });
    },
    [invalidateExport],
  );

  const onGenerate = useCallback(() => {
    if (!state.source) return;
    const token = nextToken(state);
    const source = state.source;
    const seedDecimal = state.seedText;
    invalidateExport();
    dispatch({ type: "generateRequested", token });
    setBusy(true);
    engine
      .compute({ source: source.bytes, seedDecimal, displayName: source.displayName })
      .then((result) => dispatch({ type: "generateSucceeded", token, result }))
      .catch((error) =>
        dispatch({
          type: "generateFailed",
          token,
          message: error instanceof Error ? error.message : String(error),
        }),
      )
      .finally(() => setBusy(false));
  }, [engine, state, invalidateExport]);

  const onExport = useCallback(() => {
    if (!isExportable(state)) return;
    const digest = state.result.glb.sha256;
    setBusy(true);
    engine
      .suggestedName(digest)
      .then((name) => engine.export(digest, name))
      .then((path) => {
        setExportPath(path);
        setReport(undefined);
        dispatch({ type: "note", level: "info", text: `exported to ${path}` });
        addToast(path, { type: "success", title: "Exported GLB" });
      })
      .catch((error) => {
        dispatch({ type: "note", level: "error", text: String(error) });
        addToast(String(error), { type: "error", title: "Export failed" });
      })
      .finally(() => setBusy(false));
  }, [engine, state, addToast]);

  const onVerify = useCallback(() => {
    if (!isExportable(state) || !exportPath) return;
    const digest = state.result.glb.sha256;
    setBusy(true);
    engine
      .inspect(digest, exportPath)
      .then((text) => {
        setReport(text);
        dispatch({
          type: "note",
          level: text.includes("DOES NOT MATCH") ? "error" : "info",
          text: `reopened ${exportPath}`,
        });
      })
      .catch((error) => {
        dispatch({ type: "note", level: "error", text: String(error) });
        addToast(String(error), { type: "error", title: "Verification failed" });
      })
      .finally(() => setBusy(false));
  }, [engine, state, exportPath, addToast]);

  // Pulls the GLB over the separate digest-checked path and reports what
  // arrived. It does not write a file; that is what Export does.
  const onDownload = useCallback(() => {
    if (!isExportable(state)) return;
    const { sha256, byteCount } = state.result.glb;
    setBusy(true);
    engine
      .artifact(sha256)
      .then((bytes) =>
        dispatch({
          type: "note",
          level: bytes.length === Number(byteCount) ? "info" : "error",
          text:
            `fetched ${bytes.length} bytes for ${sha256.slice(0, 12)}... ` +
            `(result said ${byteCount}); digest verified on arrival`,
        }),
      )
      .catch((error) => dispatch({ type: "note", level: "error", text: String(error) }))
      .finally(() => setBusy(false));
  }, [engine, state]);

  const reveal = useCallback((panelId: PanelId) => {
    setDockCommand({ requestId: nextRequestId(`reveal-${panelId}`), kind: "activate", panelId });
  }, []);

  const onDockCommandResult = useCallback(
    (result: DockCommandResult) => {
      if (result.status === "rejected") {
        addToast(result.error ?? "The workspace rejected that request.", {
          type: "error",
          title: "Could not reveal panel",
        });
      }
    },
    [addToast],
  );

  const panels: DockPanel[] = useMemo(
    () => [
      {
        id: "library",
        title: "Source",
        content: (
          <SourcePanel
            state={state}
            presets={presets}
            engineAvailable={engine.available}
            engineReason={engine.unavailableReason}
            busy={busy}
            onOpenPreset={onOpenPreset}
            onOpenFile={onOpenFile}
            onSeedChange={onSeedChange}
            onGenerate={onGenerate}
          />
        ),
      },
      { id: "view-3d", title: "Preview", viewport: true, content: <PreviewPanel /> },
      {
        id: "view-2d",
        title: "Export & verify",
        content: (
          <ExportPanel
            state={state}
            busy={busy}
            exportPath={exportPath}
            report={report}
            onExport={onExport}
            onVerify={onVerify}
            onDownload={onDownload}
            canDownload={engine.available}
          />
        ),
      },
      { id: "objects", title: "LODs", content: <LodPanel state={state} /> },
      { id: "inspector", title: "Result", content: <ResultPanel state={state} /> },
      { id: "bottom", title: "Output", content: <OutputPanel state={state} /> },
    ],
    [
      state,
      presets,
      engine,
      busy,
      exportPath,
      report,
      onOpenPreset,
      onOpenFile,
      onSeedChange,
      onGenerate,
      onExport,
      onVerify,
      onDownload,
    ],
  );

  const actions: ActionPresentation[] = useMemo(
    () => [
      {
        id: "tree.generate",
        label: "Generate Tree",
        group: "Tree",
        disabled: !state.source || busy || !engine.available,
        disabledReason: engine.available ? "Open a source first." : engine.unavailableReason,
        onInvoke: onGenerate,
      },
      {
        id: "tree.export",
        label: "Export GLB",
        group: "Tree",
        disabled: !isExportable(state) || busy,
        disabledReason: "A result must match the source and seed on screen.",
        onInvoke: onExport,
      },
      {
        id: "tree.verify",
        label: "Reopen and Verify Export",
        group: "Tree",
        disabled: !exportPath || busy,
        disabledReason: "Export a GLB first.",
        onInvoke: onVerify,
      },
      { id: "view.reveal.source", label: "Reveal Source", group: "View", onInvoke: () => reveal("library") },
      { id: "view.reveal.lods", label: "Reveal LODs", group: "View", onInvoke: () => reveal("objects") },
      { id: "view.reveal.result", label: "Reveal Result", group: "View", onInvoke: () => reveal("inspector") },
      { id: "view.reveal.export", label: "Reveal Export", group: "View", onInvoke: () => reveal("view-2d") },
      { id: "view.reveal.output", label: "Reveal Output", group: "View", onInvoke: () => reveal("bottom") },
      {
        id: "view.layout.reset",
        label: "Reset Layout",
        group: "View",
        onInvoke: () => setDockCommand({ requestId: nextRequestId("reset"), kind: "reset" }),
      },
      {
        id: "view.theme.toggle",
        label: "Toggle Dark Mode",
        aliases: ["theme", "appearance"],
        group: "View",
        onInvoke: () => setThemeChoice(theme === "dark" ? "light" : "dark"),
      },
      {
        id: "window.return",
        label: "Return Panels to Main Window",
        group: "Window",
        disabled: !secondary,
        disabledReason: secondary ? undefined : "Only available in a secondary window.",
        onInvoke: async () => {
          try {
            await host?.returnToMain();
          } catch (error) {
            addToast(String(error), { type: "error", title: "Return failed" });
          }
        },
      },
      {
        id: "help.about",
        label: "About This Slice",
        group: "Help",
        onInvoke: () =>
          addToast(
            "Open a source, edit the seed, generate in Rust, inspect LODs and " +
              "materials, export a GLB and reopen it. Running under the " +
              (isTauri() ? "native" : "browser") +
              " host.",
            { type: "info", title: "Midori tree slice" },
          ),
      },
    ],
    [state, busy, engine, exportPath, secondary, theme, host, addToast, reveal, onGenerate, onExport, onVerify],
  );

  const menus: MenuDefinition[] = useMemo(
    () => [
      {
        id: "tree",
        label: "Tree",
        items: [
          { id: "tree.generate", actionId: "tree.generate" },
          { id: "tree.sep", kind: "separator" },
          { id: "tree.export", actionId: "tree.export" },
          { id: "tree.verify", actionId: "tree.verify" },
        ],
      },
      {
        id: "view",
        label: "View",
        items: [
          { id: "view.source", actionId: "view.reveal.source" },
          { id: "view.lods", actionId: "view.reveal.lods" },
          { id: "view.result", actionId: "view.reveal.result" },
          { id: "view.export", actionId: "view.reveal.export" },
          { id: "view.output", actionId: "view.reveal.output" },
          { id: "view.sep", kind: "separator" },
          { id: "view.reset", actionId: "view.layout.reset" },
          { id: "view.theme", actionId: "view.theme.toggle" },
        ],
      },
      { id: "window", label: "Window", items: [{ id: "window.return", actionId: "window.return" }] },
      { id: "help", label: "Help", items: [{ id: "help.about", actionId: "help.about" }] },
    ],
    [],
  );

  const settings: SettingsRegistration = useMemo(
    () => ({
      appId: APP_ID,
      version: SETTINGS_VERSION,
      preferenceAdapter,
      categories: [
        {
          id: "appearance",
          label: "Appearance",
          fields: [
            {
              id: "theme",
              type: "enum",
              label: "Theme",
              description: "Follow the operating system or pick a fixed theme.",
              scope: "device",
              defaultValue: "system",
              choices: [
                { value: "system", label: "System" },
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
              ],
            },
          ],
        },
      ],
      appearance: {
        fieldId: "theme",
        onValueChange: (value) => {
          if (value === "system" || value === "light" || value === "dark") setThemeChoice(value);
        },
      },
    }),
    [],
  );

  const status = [
    state.source ? `${state.source.displayName} @ ${state.seedText}` : "no source",
    state.status,
    host ? `${host.identity.windowId} - project ${host.identity.projectId}` : "no window host",
  ].join("  |  ");

  return (
    <ThemeProvider theme={theme}>
      <WorkspaceShell
        title={secondary ? "Midori (secondary window)" : "Midori"}
        theme={theme}
        panels={panels}
        actions={actions}
        menus={menus}
        settings={settings}
        docking={{
          appId: APP_ID,
          preferenceAdapter,
          // The shared named-slot layout, unmodified, and it places every panel
          // correctly. Measured in the running application: `#root` is exactly
          // `window.innerWidth` CSS pixels, and the six panel tabs sit at
          // 0.0005, 0.1695, 0.4693, 0.7691, 0.7691 and 0.0005 of that width -
          // which is what `createNamedSlotLayout`'s ratios specify.
          defaultLayout: createNamedSlotLayout(),
          windowHost: host,
        }}
        dockCommand={dockCommand}
        onDockCommandResult={onDockCommandResult}
        onDockLayoutChange={(layout) => {
          currentLayout.current = layout;
        }}
        status={status}
      />
    </ThemeProvider>
  );
}

export { PANEL_IDS };
