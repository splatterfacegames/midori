import { readFileSync } from "node:fs";
import { resolve } from "node:path";

/**
 * The cross-language fixture, loaded for tests only.
 *
 * `app/fixtures/tree-compute-result.bin` is written by `midori-app`'s Rust
 * encoder from a real oak generation at seed 1000003, and the Rust test
 * `the_typescript_fixture_is_this_encoder_s_own_output` fails if it drifts.
 * Reading it here is what makes the TypeScript assertions evidence about the
 * engine rather than about this file's own writer.
 *
 * Vitest runs with the project root as the working directory. `import.meta.url`
 * is not usable for this: Vite rewrites it to an http URL under the dev server.
 */
export function readResultFixture(): Uint8Array {
  return new Uint8Array(readFileSync(resolve(process.cwd(), "fixtures/tree-compute-result.bin")));
}
