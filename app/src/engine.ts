import { invoke } from "@tauri-apps/api/core";
import { decodeResult, encodeRequest, type TreeRequest, type TreeResultView } from "./contract";
import { isTauri } from "./host";

/**
 * The engine, as the UI sees it.
 *
 * Midori's generator is native Rust. There is no browser implementation of this
 * interface and inventing one - a fixture, a stub, a JS reimplementation - would
 * be fabricating a capability, so the browser build gets {@link unavailableEngine}
 * and says so plainly instead. Window hosting is a separate concern and does
 * work in both hosts; see `host.ts`.
 */
export interface TreeEngine {
  readonly available: boolean;
  /** Why not, when `available` is false. */
  readonly unavailableReason?: string;
  presetIds(): Promise<string[]>;
  /** The exact embedded bytes of a preset. */
  presetSource(id: string): Promise<Uint8Array>;
  /** `TreeComputeRequest` in, `TreeComputeResult` out. */
  compute(request: TreeRequest): Promise<TreeResultView>;
  /** GLB bytes for a digest named in a result's `ArtifactRef`. */
  artifact(sha256: string): Promise<Uint8Array>;
  suggestedName(sha256: string): Promise<string>;
  /** Write the artifact to disk; resolves to the absolute path written. */
  export(sha256: string, fileName: string): Promise<string>;
  /** Reopen a file from disk and report what is actually in it. */
  inspect(sha256: string, path: string): Promise<string>;
}

/** Tauri hands a raw command response back as an ArrayBuffer. */
function toBytes(value: unknown): Uint8Array {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (value instanceof Uint8Array) return value;
  if (Array.isArray(value)) return Uint8Array.from(value as number[]);
  throw new Error(`expected bytes from the engine, got ${typeof value}`);
}

export function createTauriEngine(): TreeEngine {
  return {
    available: true,

    async presetIds() {
      return invoke<string[]>("tree_preset_ids");
    },

    async presetSource(id) {
      return toBytes(await invoke("tree_preset_source", { id }));
    },

    async compute(request) {
      // The envelope crosses as bytes in both directions. The seed is validated
      // here, encoded as a ulong, and never becomes a JS number.
      const bytes = encodeRequest(request);
      const response = await invoke("tree_compute", { request: Array.from(bytes) });
      const result = decodeResult(toBytes(response));
      // Belt and braces at the boundary: a result that does not answer the
      // request we sent is not this request's result, whatever it says.
      if (result.seedDecimal !== request.seedDecimal) {
        throw new Error(
          `engine answered seed ${result.seedDecimal} for a request with seed ${request.seedDecimal}`,
        );
      }
      return result;
    },

    async artifact(sha256) {
      const bytes = toBytes(await invoke("tree_artifact", { sha256 }));
      // The digest is the whole point of the separate transfer path; check it
      // on arrival rather than trusting the channel.
      const actual = await sha256Of(bytes);
      if (actual !== sha256) {
        throw new Error(`artifact bytes hash to ${actual}, not the requested ${sha256}`);
      }
      return bytes;
    },

    async suggestedName(sha256) {
      return invoke<string>("tree_suggested_name", { sha256 });
    },

    async export(sha256, fileName) {
      return invoke<string>("tree_export", { sha256, fileName });
    },

    async inspect(sha256, path) {
      return invoke<string>("tree_inspect", { sha256, path });
    },
  };
}

async function sha256Of(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes as unknown as BufferSource);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

const NO_ENGINE =
  "The Midori generator is native Rust and is reached through the Tauri command " +
  "boundary. This page is running in a plain browser, so no tree can be generated " +
  "here. Run `npm run desktop` for the application.";

export function unavailableEngine(reason = NO_ENGINE): TreeEngine {
  const fail = async (): Promise<never> => {
    throw new Error(reason);
  };
  return {
    available: false,
    unavailableReason: reason,
    presetIds: async () => [],
    presetSource: fail,
    compute: fail,
    artifact: fail,
    suggestedName: async () => "tree",
    export: fail,
    inspect: fail,
  };
}

/** The engine for the host this window is actually running under. */
export function createEngine(): TreeEngine {
  return isTauri() ? createTauriEngine() : unavailableEngine();
}
