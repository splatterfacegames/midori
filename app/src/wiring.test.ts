import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Cross-checks the Tauri command boundary by reading both sides.
 *
 * The Rust commands and the `invoke(...)` calls in `engine.ts` are two
 * hand-written lists that have to agree on every name. A typo in either compiles
 * cleanly on both sides and only fails at runtime, in a window, on the one path
 * the user takes.
 *
 * The stronger check would be to drive the real invoke handler with
 * `tauri::test::mock_builder`. That was tried and abandoned: the resulting test
 * binary fails to load on this machine with `STATUS_ENTRYPOINT_NOT_FOUND`
 * (0xc0000139) before any test runs, both as a unit test and as an integration
 * test, and with or without the `staticlib`/`cdylib` crate types. So this is a
 * static cross-check of names, not proof that a command executes. It catches the
 * typo class and nothing more, and it should be replaced by a real invoke test
 * if that harness is ever made to load here.
 */

const root = process.cwd();
const commandsRs = readFileSync(resolve(root, "src-tauri/src/commands.rs"), "utf8");
const libRs = readFileSync(resolve(root, "src-tauri/src/lib.rs"), "utf8");
const engineTs = readFileSync(resolve(root, "src/engine.ts"), "utf8");

/**
 * Split a Rust parameter list on top-level commas only. `State<'_,
 * tree::TreeState>` contains a comma inside angle brackets, and splitting
 * naively turns one parameter into two nonsense ones.
 */
function splitParams(list: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of list) {
    if (ch === "<" || ch === "(" || ch === "[") depth += 1;
    else if (ch === ">" || ch === ")" || ch === "]") depth -= 1;
    if (ch === "," && depth === 0) {
      parts.push(current);
      current = "";
    } else {
      current += ch;
    }
  }
  parts.push(current);
  return parts.map((p) => p.trim()).filter(Boolean);
}

/** `pub fn tree_x(a: A, b_c: B, ...)` -> name and parameter names. */
function rustCommands(): Map<string, string[]> {
  const commands = new Map<string, string[]>();
  const re = /#\[tauri::command\]\s*(?:\/\/\/[^\n]*\n\s*)*pub fn (\w+)\s*(?:<[^>]*>)?\s*\(([^)]*)\)/g;
  for (const match of commandsRs.matchAll(re)) {
    const params = splitParams(match[2]!)
      .map((p) => p.split(":")[0]!.trim())
      // Injected by Tauri, never sent by the caller.
      .filter((name) => name !== "app" && name !== "state" && name !== "window" && name !== "webview");
    commands.set(match[1]!, params);
  }
  return commands;
}

/** `invoke("name", { a, bC: x })` -> command and argument keys. */
function invocations(): Map<string, string[]> {
  const calls = new Map<string, string[]>();
  const re = /invoke(?:<[^>]*>)?\(\s*"(\w+)"(?:\s*,\s*\{([^}]*)\})?/g;
  for (const match of engineTs.matchAll(re)) {
    const args = (match[2] ?? "")
      .split(",")
      .map((a) => a.trim())
      .filter(Boolean)
      .map((a) => (a.includes(":") ? a.split(":")[0]!.trim() : a));
    calls.set(match[1]!, args);
  }
  return calls;
}

const snakeToCamel = (name: string) =>
  name.replace(/_([a-z])/g, (_, ch: string) => ch.toUpperCase());

describe("the Tauri command boundary", () => {
  const commands = rustCommands();
  const calls = invocations();

  it("found the commands and the calls at all", () => {
    // Guards the parsing itself: a regex that silently matches nothing would
    // make every assertion below vacuously true.
    expect(commands.size).toBe(7);
    expect(calls.size).toBe(7);
  });

  it("registers every command it defines", () => {
    for (const name of commands.keys()) {
      expect(libRs, `${name} is not in generate_handler!`).toContain(`commands::${name},`);
    }
    // And registers nothing it does not define.
    const registered = [...libRs.matchAll(/commands::(\w+),/g)].map((m) => m[1]!);
    expect(new Set(registered)).toEqual(new Set(commands.keys()));
  });

  it("invokes only commands that exist", () => {
    for (const name of calls.keys()) {
      expect([...commands.keys()], `engine.ts invokes ${name}`).toContain(name);
    }
  });

  it("sends every argument each command expects, under the camelCase key Tauri uses", () => {
    for (const [name, params] of commands) {
      const args = calls.get(name);
      expect(args, `engine.ts never invokes ${name}`).toBeDefined();
      const expected = params.map(snakeToCamel).sort();
      expect(args!.slice().sort(), `arguments for ${name}`).toEqual(expected);
    }
  });

  it("has the same argument names on both sides for the file-name case specifically", () => {
    // `file_name` is the only parameter whose Rust and JS spellings differ, so
    // it is the one most likely to be got wrong.
    expect(commands.get("tree_export")).toEqual(["sha256", "file_name"]);
    expect(calls.get("tree_export")!.sort()).toEqual(["fileName", "sha256"]);
  });
});
