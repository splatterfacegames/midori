import type { TreeResultView } from "./contract";

/**
 * The tree slice's state machine.
 *
 * Deliberately pure and free of React so the ordering rules can be tested
 * directly rather than inferred from rendered output. Three rules the design
 * makes binding, all enforced here:
 *
 *  - A completed older computation must not replace a newer input. Every
 *    generation carries a token, and a result whose token is not the current one
 *    is dropped. On top of that a result must match the source digest and seed
 *    it claims to answer, which also catches a result for entirely different
 *    content.
 *  - Editing the source or the seed invalidates export readiness. A `ready`
 *    state cannot survive an edit, so the export button can never be pointing at
 *    an artifact for input the user has since changed.
 *  - Invalid input retains its error. An `error` state carries no result, so
 *    there is nothing for the UI to mistake for the new input's export.
 *
 * This is pure in-process computation: no durable project, no committed
 * revision, no job queue, and no cancellation. Dropping a superseded result is
 * not cancellation - the work still ran - and nothing here claims otherwise.
 */

export interface SourceDoc {
  /** The exact imported bytes. */
  bytes: Uint8Array;
  /** Digest of exactly those bytes. */
  sha256: string;
  /** Provenance for display. Not part of document identity. */
  displayName: string;
  /** Where it came from, for the UI to label honestly. */
  origin: "preset" | "file";
}

export interface SliceState {
  status: "empty" | "source" | "generating" | "ready" | "error";
  source?: SourceDoc;
  /** Raw text as typed. Validated only when a generation is requested. */
  seedText: string;
  /** Token of the generation currently awaited, if any. */
  pending?: number;
  /** Present only in `ready`. */
  result?: TreeResultView;
  /** Present only in `error`. */
  error?: string;
  /** Monotonic; the next generation takes `issued + 1`. */
  issued: number;
  /** Append-only, bounded log of what happened. */
  log: LogEntry[];
}

export interface LogEntry {
  at: number;
  level: "info" | "error";
  text: string;
}

const MAX_LOG = 200;

export type SliceAction =
  | { type: "openSource"; source: SourceDoc }
  | { type: "editSeed"; seedText: string }
  | { type: "generateRequested"; token: number }
  | { type: "generateSucceeded"; token: number; result: TreeResultView }
  | { type: "generateFailed"; token: number; message: string }
  | { type: "note"; level: "info" | "error"; text: string };

export const initialState: SliceState = {
  status: "empty",
  seedText: "1",
  issued: 0,
  log: [],
};

function note(state: SliceState, level: "info" | "error", text: string): LogEntry[] {
  const log = [...state.log, { at: Date.now(), level, text }];
  return log.length > MAX_LOG ? log.slice(log.length - MAX_LOG) : log;
}

/** The token the next generation should carry. */
export function nextToken(state: SliceState): number {
  return state.issued + 1;
}

/** Whether the state holds a result that belongs to the input on screen. */
export function isExportable(state: SliceState): state is SliceState & { result: TreeResultView } {
  return state.status === "ready" && state.result !== undefined;
}

export function reduce(state: SliceState, action: SliceAction): SliceState {
  switch (action.type) {
    case "openSource":
      // A new source invalidates any previous result and any error against the
      // previous input. Nothing carries over but the seed the user typed.
      return {
        ...state,
        status: "source",
        source: action.source,
        result: undefined,
        error: undefined,
        pending: undefined,
        log: note(
          state,
          "info",
          `opened ${action.source.displayName} (${action.source.origin}, ` +
            `${action.source.bytes.length} bytes, sha256 ${action.source.sha256.slice(0, 12)}...)`,
        ),
      };

    case "editSeed": {
      if (action.seedText === state.seedText) return state;
      // Editing the seed invalidates export readiness even mid-flight: the
      // result now in the air answers a question the user has moved on from.
      return {
        ...state,
        seedText: action.seedText,
        status: state.source ? "source" : "empty",
        result: undefined,
        error: undefined,
        pending: undefined,
        log: state.log,
      };
    }

    case "generateRequested":
      if (!state.source) return state;
      return {
        ...state,
        status: "generating",
        pending: action.token,
        issued: Math.max(state.issued, action.token),
        result: undefined,
        error: undefined,
        log: note(state, "info", `generating ${state.source.displayName} at seed ${state.seedText}`),
      };

    case "generateSucceeded": {
      if (state.pending !== action.token) {
        // Superseded. The work completed, but a newer input is on screen and
        // this result is not an answer to it.
        return { ...state, log: note(state, "info", `dropped superseded result #${action.token}`) };
      }
      if (
        !state.source ||
        action.result.sourceSha256 !== state.source.sha256 ||
        action.result.seedDecimal !== state.seedText
      ) {
        // The token matched but the identity did not. Something is wrong at the
        // boundary; refusing it is the only safe answer.
        return {
          ...state,
          status: "error",
          pending: undefined,
          result: undefined,
          error:
            "the engine returned a result for different input " +
            `(source ${action.result.sourceSha256.slice(0, 12)}..., seed ${action.result.seedDecimal})`,
          log: note(state, "error", "rejected a result that did not match the request"),
        };
      }
      return {
        ...state,
        status: "ready",
        pending: undefined,
        result: action.result,
        error: undefined,
        log: note(
          state,
          "info",
          `ready: ${action.result.lods.length} LODs, GLB ${action.result.glb.byteCount} bytes ` +
            `sha256 ${action.result.glb.sha256.slice(0, 12)}...`,
        ),
      };
    }

    case "generateFailed": {
      if (state.pending !== action.token) {
        return { ...state, log: note(state, "info", `dropped superseded failure #${action.token}`) };
      }
      // The error is the state. No previous result is restored, so nothing can
      // present the last good export as this input's.
      return {
        ...state,
        status: "error",
        pending: undefined,
        result: undefined,
        error: action.message,
        log: note(state, "error", action.message),
      };
    }

    case "note":
      return { ...state, log: note(state, action.level, action.text) };

    default:
      return state;
  }
}
