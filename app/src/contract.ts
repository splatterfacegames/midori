import * as flatbuffers from "flatbuffers";
import { metadata } from "./generated/metadata";
import {
  MaterialRole,
  TreeComputeRequest,
  TreeComputeResult,
} from "./generated/bindings";

/**
 * The wire contract, seen from TypeScript.
 *
 * Nothing here re-describes the schema. The tables come from the generated
 * bindings and the identity constants are read out of the generated metadata,
 * so a regenerated `.fbs` moves both sides at once and there is no hand-copied
 * digest to go stale. `contract.test.ts` proves that against a fixture the Rust
 * encoder produced.
 */

/** SHA-256 of the binary descriptor these bindings were generated from. */
export const DESCRIPTOR_SHA256 = metadata.descriptor_sha256;

/**
 * The `schema` string every request and result carries. Matches
 * `midori_contracts::SCHEMA_TAG`; both are built from the descriptor digest
 * rather than written out twice.
 */
export const SCHEMA_TAG = `Midori.Tree.V1:${DESCRIPTOR_SHA256}`;

/** FlatBuffers file identifier of the root table. */
export const FILE_IDENTIFIER = metadata.identifiers.TreeComputeResult;

/** Mirrors `midori_contracts::MAX_ENVELOPE_BYTES`. */
export const MAX_ENVELOPE_BYTES = 1 << 20;

const U64_MAX = 18446744073709551615n;

/**
 * Parse a seed exactly as the engine does.
 *
 * `compose_tree` requires the canonical decimal form of a `u64` and rejects
 * anything else, so `"007"`, `"+7"`, `" 7"` and `"1e3"` are all errors rather
 * than being quietly coerced. The value never becomes a JS `number`: 2^64-1 is
 * not representable as a double, and a seed that survives a round trip through
 * one is a different tree.
 */
export function parseCanonicalSeed(text: string): bigint {
  if (!/^(0|[1-9][0-9]*)$/.test(text)) {
    throw new Error(
      `seed must be a canonical unsigned decimal integer, got ${JSON.stringify(text)}`,
    );
  }
  const value = BigInt(text);
  if (value > U64_MAX) {
    throw new Error(`seed ${text} does not fit in a 64-bit unsigned integer`);
  }
  return value;
}

export interface TreeRequest {
  /** The exact imported UTF-8 species TOML. Never a reparsed or re-serialized form. */
  source: Uint8Array;
  /** Canonical decimal. See {@link parseCanonicalSeed}. */
  seedDecimal: string;
  /** UI provenance only; never part of document identity. */
  displayName?: string;
}

export function encodeRequest(request: TreeRequest): Uint8Array {
  const seed = parseCanonicalSeed(request.seedDecimal);
  const builder = new flatbuffers.Builder(1024 + request.source.length);
  const schema = builder.createString(SCHEMA_TAG);
  const source = TreeComputeRequest.createSourceVector(builder, request.source);
  const displayName =
    request.displayName === undefined ? 0 : builder.createString(request.displayName);

  TreeComputeRequest.startTreeComputeRequest(builder);
  TreeComputeRequest.addSchema(builder, schema);
  TreeComputeRequest.addSource(builder, source);
  TreeComputeRequest.addSeed(builder, seed);
  if (displayName !== 0) TreeComputeRequest.addDisplayName(builder, displayName);
  const root = TreeComputeRequest.endTreeComputeRequest(builder);
  builder.finish(root);
  return builder.asUint8Array();
}

export const MATERIAL_ROLE_NAMES: Readonly<Record<number, string>> = {
  [MaterialRole.Bark]: "Bark",
  [MaterialRole.Leaves]: "Leaves",
};

export interface SubmeshRangeView {
  material: MaterialRole;
  materialName: string;
  indexOffset: number;
  indexCount: number;
}

export interface LodLevelView {
  index: number;
  /** The engine's threshold name: `High`, `Medium`, `Low`. NOT the GLB node name. */
  name: string;
  screenHeight: number;
  vertexCount: number;
  indexCount: number;
  submeshes: SubmeshRangeView[];
}

export interface TreeResultView {
  schema: string;
  /** Digest of the exact source bytes the engine was handed. */
  sourceSha256: string;
  /** Canonical decimal, never a JS number. */
  seedDecimal: string;
  engine: { sourceRevision: string; buildId: string };
  summary: {
    speciesName: string;
    scientificName: string;
    stemCount: bigint;
    branchCount: bigint;
    leafCount: bigint;
    boundsMin: number[];
    boundsMax: number[];
  };
  lods: LodLevelView[];
  /** A reference to bytes held outside this envelope, not the bytes. */
  glb: { kind: string; sha256: string; byteCount: bigint };
}

export class ContractError extends Error {}

const HEX64 = /^[0-9a-f]{64}$/;

/**
 * Verify and read a result.
 *
 * **The TypeScript FlatBuffers runtime ships no verifier.** Rust gets
 * `flatbuffers::root::<T>`, which walks the buffer and proves every offset and
 * required field before a single accessor runs; there is no equivalent here, and
 * `getRootAs...` will happily read a malformed buffer. So this function does not
 * claim to detect arbitrary corruption. What it does:
 *
 *  - bounds the envelope and requires enough bytes for a root and identifier,
 *  - checks the file identifier and then the schema tag, which in practice is
 *    what catches a truncated or foreign buffer, since the tag is a 64-character
 *    string near the front and almost nothing survives its read intact,
 *  - checks the invariants the schema itself states: digests are digests, LODs
 *    are ordered by ascending index, and a submesh range addresses its own mesh.
 *
 * The real trust boundary is that these bytes come from this application's own
 * Rust backend over the Tauri IPC, where they were built by
 * `midori_contracts::encode_result`. This is not a network parser, and it must
 * not be reused as one without a verifier.
 */
export function decodeResult(bytes: Uint8Array): TreeResultView {
  if (bytes.length > MAX_ENVELOPE_BYTES) {
    throw new ContractError(
      `envelope is ${bytes.length} bytes, over the ${MAX_ENVELOPE_BYTES} byte limit`,
    );
  }
  if (bytes.length < 8) {
    throw new ContractError(`envelope is only ${bytes.length} bytes`);
  }
  const buffer = new flatbuffers.ByteBuffer(bytes);
  if (!TreeComputeResult.bufferHasIdentifier(buffer)) {
    throw new ContractError(`buffer is not a ${FILE_IDENTIFIER} message`);
  }
  const result = TreeComputeResult.getRootAsTreeComputeResult(buffer);

  const schema = result.schema();
  if (schema !== SCHEMA_TAG) {
    throw new ContractError(`schema mismatch: expected ${SCHEMA_TAG}, found ${schema}`);
  }

  const engine = result.engine();
  const summary = result.summary();
  const glb = result.glb();
  if (!engine || !summary || !glb) {
    throw new ContractError("result is missing a required table");
  }

  const lods: LodLevelView[] = [];
  for (let index = 0; index < result.lodsLength(); index += 1) {
    const lod = result.lods(index);
    if (!lod) throw new ContractError(`LOD ${index} is absent`);
    const submeshes: SubmeshRangeView[] = [];
    for (let slot = 0; slot < lod.submeshesLength(); slot += 1) {
      const range = lod.submeshes(slot);
      if (!range) throw new ContractError(`LOD ${index} submesh ${slot} is absent`);
      const material = range.material();
      const materialName = MATERIAL_ROLE_NAMES[material];
      if (materialName === undefined) {
        // Bark and Leaves are the only roles core has. A third one means the
        // peer is speaking a schema these bindings were not generated from.
        throw new ContractError(`unknown material role ${material}`);
      }
      submeshes.push({
        material,
        materialName,
        indexOffset: range.indexOffset(),
        indexCount: range.indexCount(),
      });
    }
    lods.push({
      index: lod.index(),
      name: lod.name() ?? "",
      screenHeight: lod.screenHeight(),
      vertexCount: lod.vertexCount(),
      indexCount: lod.indexCount(),
      submeshes,
    });
  }

  // Invariants the schema states in its own comments.
  lods.forEach((lod, position) => {
    if (lod.index !== position) {
      throw new ContractError(
        `lods must be ordered by ascending index; found ${lod.index} at position ${position}`,
      );
    }
    for (const range of lod.submeshes) {
      if (range.indexOffset + range.indexCount > lod.indexCount) {
        throw new ContractError(
          `LOD ${lod.index} has a submesh range ${range.indexOffset}+${range.indexCount} ` +
            `past its ${lod.indexCount} indices`,
        );
      }
    }
  });

  const sourceSha256 = result.sourceSha256() ?? "";
  const glbSha256 = glb.sha256() ?? "";
  for (const [label, digest] of [
    ["source_sha256", sourceSha256],
    ["glb.sha256", glbSha256],
  ] as const) {
    if (!HEX64.test(digest)) {
      throw new ContractError(`${label} is not a SHA-256 digest`);
    }
  }
  if (glb.byteCount() <= 0n) {
    throw new ContractError("the referenced artifact has no bytes");
  }

  // The bounds vectors are `required` in the schema, so a null element means the
  // buffer is not what it claims to be rather than that a bound is optional.
  const bounds = (label: string, read: (i: number) => number | null, length: number) => {
    if (length !== 3) {
      throw new ContractError(`${label} has ${length} components, expected 3`);
    }
    return Array.from({ length }, (_, i) => {
      const value = read(i);
      if (value === null || !Number.isFinite(value)) {
        throw new ContractError(`${label}[${i}] is not a finite number`);
      }
      return value;
    });
  };

  return {
    schema,
    sourceSha256,
    seedDecimal: result.seed().toString(),
    engine: {
      sourceRevision: engine.sourceRevision() ?? "",
      buildId: engine.buildId() ?? "",
    },
    summary: {
      speciesName: summary.speciesName() ?? "",
      scientificName: summary.scientificName() ?? "",
      stemCount: summary.stemCount(),
      branchCount: summary.branchCount(),
      leafCount: summary.leafCount(),
      boundsMin: bounds("bounds_min", (i) => summary.boundsMin(i), summary.boundsMinLength()),
      boundsMax: bounds("bounds_max", (i) => summary.boundsMax(i), summary.boundsMaxLength()),
    },
    lods,
    glb: {
      kind: glb.kind() ?? "",
      sha256: glbSha256,
      byteCount: glb.byteCount(),
    },
  };
}

/** SHA-256 of exactly these bytes, as lowercase hex. */
export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes as unknown as BufferSource);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}
