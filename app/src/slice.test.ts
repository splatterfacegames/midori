import { describe, expect, it } from "vitest";
import { decodeResult, type TreeResultView } from "./contract";
import { readResultFixture } from "./fixture";
import { initialState, isExportable, nextToken, reduce, type SliceState, type SourceDoc } from "./slice";

const fixture = decodeResult(readResultFixture());

function source(sha256: string, name = "oak.toml"): SourceDoc {
  return { bytes: new TextEncoder().encode(name), sha256, displayName: name, origin: "preset" };
}

/** A result claiming to answer a given source digest and seed. */
function resultFor(sha256: string, seedDecimal: string): TreeResultView {
  return { ...fixture, sourceSha256: sha256, seedDecimal };
}

function opened(seedText = fixture.seedDecimal): SliceState {
  let state = reduce(initialState, { type: "openSource", source: source(fixture.sourceSha256) });
  state = reduce(state, { type: "editSeed", seedText });
  return state;
}

describe("the slice state machine", () => {
  it("starts empty and cannot generate without a source", () => {
    expect(initialState.status).toBe("empty");
    expect(reduce(initialState, { type: "generateRequested", token: 1 }).status).toBe("empty");
  });

  it("accepts a source, then a result that answers it", () => {
    let state = opened();
    const token = nextToken(state);
    state = reduce(state, { type: "generateRequested", token });
    expect(state.status).toBe("generating");
    expect(isExportable(state)).toBe(false);

    state = reduce(state, {
      type: "generateSucceeded",
      token,
      result: resultFor(fixture.sourceSha256, state.seedText),
    });
    expect(state.status).toBe("ready");
    expect(isExportable(state)).toBe(true);
    expect(state.result?.summary.speciesName).toBe("Oak");
  });
});

describe("staleness", () => {
  it("a superseded result never lands", () => {
    let state = opened("1");
    const first = nextToken(state);
    state = reduce(state, { type: "generateRequested", token: first });

    // The user edits the seed while the first generation is in flight, and
    // starts a second.
    state = reduce(state, { type: "editSeed", seedText: "2" });
    const second = nextToken(state);
    expect(second).toBeGreaterThan(first);
    state = reduce(state, { type: "generateRequested", token: second });

    // The first finishes last. It answers seed 1; the screen says seed 2.
    state = reduce(state, {
      type: "generateSucceeded",
      token: first,
      result: resultFor(fixture.sourceSha256, "1"),
    });
    expect(state.status).toBe("generating");
    expect(state.result).toBeUndefined();
    expect(isExportable(state)).toBe(false);

    // The second finishes and is accepted.
    state = reduce(state, {
      type: "generateSucceeded",
      token: second,
      result: resultFor(fixture.sourceSha256, "2"),
    });
    expect(state.status).toBe("ready");
    expect(state.result?.seedDecimal).toBe("2");
  });

  it("a superseded failure does not overwrite a newer state either", () => {
    let state = opened("1");
    const first = nextToken(state);
    state = reduce(state, { type: "generateRequested", token: first });
    state = reduce(state, { type: "editSeed", seedText: "2" });
    const second = nextToken(state);
    state = reduce(state, { type: "generateRequested", token: second });
    state = reduce(state, {
      type: "generateSucceeded",
      token: second,
      result: resultFor(fixture.sourceSha256, "2"),
    });
    expect(state.status).toBe("ready");

    state = reduce(state, { type: "generateFailed", token: first, message: "boom" });
    expect(state.status).toBe("ready");
    expect(state.error).toBeUndefined();
  });

  it("refuses a result whose identity does not match the request, even on the current token", () => {
    let state = opened("9");
    const token = nextToken(state);
    state = reduce(state, { type: "generateRequested", token });
    state = reduce(state, {
      type: "generateSucceeded",
      token,
      result: resultFor("f".repeat(64), "9"),
    });
    expect(state.status).toBe("error");
    expect(state.result).toBeUndefined();
    expect(state.error).toMatch(/different input/);
  });

  it("refuses a result for the right source but a different seed", () => {
    let state = opened("9");
    const token = nextToken(state);
    state = reduce(state, { type: "generateRequested", token });
    state = reduce(state, {
      type: "generateSucceeded",
      token,
      result: resultFor(fixture.sourceSha256, "10"),
    });
    expect(state.status).toBe("error");
    expect(isExportable(state)).toBe(false);
  });
});

describe("export readiness", () => {
  function ready(seedText = "3"): SliceState {
    let state = opened(seedText);
    const token = nextToken(state);
    state = reduce(state, { type: "generateRequested", token });
    return reduce(state, {
      type: "generateSucceeded",
      token,
      result: resultFor(fixture.sourceSha256, seedText),
    });
  }

  it("is invalidated by editing the seed", () => {
    const state = reduce(ready(), { type: "editSeed", seedText: "4" });
    expect(state.status).toBe("source");
    expect(state.result).toBeUndefined();
    expect(isExportable(state)).toBe(false);
  });

  it("is invalidated by opening a different source", () => {
    const state = reduce(ready(), {
      type: "openSource",
      source: source("a".repeat(64), "pine.toml"),
    });
    expect(state.status).toBe("source");
    expect(state.result).toBeUndefined();
    expect(isExportable(state)).toBe(false);
  });

  it("survives an edit that changes nothing", () => {
    const before = ready("3");
    const after = reduce(before, { type: "editSeed", seedText: "3" });
    expect(after).toBe(before);
    expect(isExportable(after)).toBe(true);
  });
});

describe("errors", () => {
  it("an error is retained and cannot present the previous result as the new input's export", () => {
    // Generate successfully, then edit to something the engine rejects.
    let state = opened("3");
    let token = nextToken(state);
    state = reduce(state, { type: "generateRequested", token });
    state = reduce(state, {
      type: "generateSucceeded",
      token,
      result: resultFor(fixture.sourceSha256, "3"),
    });
    expect(isExportable(state)).toBe(true);
    const goodDigest = state.result?.glb.sha256;

    state = reduce(state, { type: "editSeed", seedText: "007" });
    token = nextToken(state);
    state = reduce(state, { type: "generateRequested", token });
    state = reduce(state, {
      type: "generateFailed",
      token,
      message: "seed must be a canonical unsigned decimal u64",
    });

    expect(state.status).toBe("error");
    expect(state.error).toMatch(/canonical/);
    expect(state.result).toBeUndefined();
    expect(isExportable(state)).toBe(false);
    // The old artifact digest is nowhere in reach of the export path.
    expect(goodDigest).toBeDefined();
    expect(state.result).toBeUndefined();
  });

  it("a new source clears a previous error", () => {
    let state = opened("1");
    const token = nextToken(state);
    state = reduce(state, { type: "generateRequested", token });
    state = reduce(state, { type: "generateFailed", token, message: "bad TOML" });
    expect(state.status).toBe("error");

    state = reduce(state, { type: "openSource", source: source("b".repeat(64), "pine.toml") });
    expect(state.status).toBe("source");
    expect(state.error).toBeUndefined();
  });
});

describe("the log", () => {
  it("records what happened and stays bounded", () => {
    let state = opened();
    for (let i = 0; i < 400; i += 1) {
      state = reduce(state, { type: "note", level: "info", text: `entry ${i}` });
    }
    expect(state.log.length).toBeLessThanOrEqual(200);
    expect(state.log[state.log.length - 1]?.text).toBe("entry 399");
  });
});
