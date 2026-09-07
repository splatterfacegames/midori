import { invoke } from "@tauri-apps/api/core";
import {
  createBrowserWindowHost,
  createTauriWindowHost,
  type WindowHostOptions,
  type WindowHostSession,
} from "@jethac/tools-frontend-stack/host";
import type { DockLayout } from "@jethac/tools-frontend-stack/ui";

export const APP_ID = "midori-app";

/**
 * Stable project scope. This slice is pure in-process computation with no
 * durable project, so there is exactly one scope and it is honestly named.
 */
export const DEFAULT_PROJECT_ID = "default";

/** Latest dock layout, captured by the host when a panel moves to another window. */
export const currentLayout: { current: DockLayout | undefined } = { current: undefined };

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/**
 * Native windows go through the shared Tauri plugin; plain browser development
 * uses the same-origin popup broker. Both speak the same generated FlatBuffers
 * host codec, so the rest of the application never branches on which host it is
 * running under.
 */
export function createWindowHost(options: WindowHostOptions): Promise<WindowHostSession> {
  return isTauri()
    ? createTauriWindowHost({ invoke, ...options })
    : createBrowserWindowHost(options);
}

export interface HostBootstrap {
  host?: WindowHostSession;
  error?: string;
}

/**
 * A failed host is not fatal. The shell still runs with docking confined to a
 * single window, which is what a browser tab opened without an opener gets.
 */
export async function bootstrapHost(panelIds: readonly string[]): Promise<HostBootstrap> {
  try {
    const host = await createWindowHost({
      appId: APP_ID,
      projectId: DEFAULT_PROJECT_ID,
      panelIds,
      captureLayout: () => JSON.stringify(currentLayout.current ?? null),
      onError: (error) => console.error("[midori] window host error", error),
    });
    return { host };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}
