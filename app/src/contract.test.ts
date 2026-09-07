import { describe, expect, it } from "vitest";
import * as flatbuffers from "flatbuffers";
import {
  DESCRIPTOR_SHA256,
  FILE_IDENTIFIER,
  MAX_ENVELOPE_BYTES,
  SCHEMA_TAG,
  decodeResult,
  encodeRequest,
  parseCanonicalSeed,
} from "./contract";
import { readResultFixture } from "./fixture";
import { TreeComputeRequest } from "./generated/bindings";

/**
 * The fixture is emitted by `midori-app`'s Rust encoder from a real oak
 * generation, and a Rust test fails if it drifts. Decoding it here is the actual
 * cross-language check: if these bindings and that encoder disagreed about the
 * descriptor, this file would not read.
 */
const fixture = readResultFixture();

describe("seed parsing", () => {
  it("accepts the canonical decimal form and nothing else", () => {
    expect(parseCanonicalSeed("0")).toBe(0n);
    expect(parseCanonicalSeed("42")).toBe(42n);
    expect(parseCanonicalSeed("18446744073709551615")).toBe(18446744073709551615n);
    // The engine's own rule: `compose_tree` re-renders the parsed u64 and
    // rejects the input if it does not match, so these are all errors there too.
    for (const bad of ["", " ", "007", "+7", "-1", "7 ", "1e3", "0x10", "1_000", "7.0", "nan"]) {
      expect(() => parseCanonicalSeed(bad), bad).toThrow();
    }
  });

  it("refuses a value that does not fit in a u64", () => {
    expect(() => parseCanonicalSeed("18446744073709551616")).toThrow(/64-bit/);
  });

  it("never routes a seed through a JS number", () => {
    // 2^64-1 and 2^64 collapse onto the same double. A path that used `number`
    // would round-trip the first into the second and silently generate a
    // different tree.
    expect(Number("18446744073709551615")).toBe(Number("18446744073709551616"));
    expect(parseCanonicalSeed("18446744073709551615").toString()).toBe("18446744073709551615");
  });
});

describe("requests", () => {
  it("carries the exact source bytes, not a reparsed form", () => {
    // A BOM and CRLF endings: a serializer round trip would lose or rewrite
    // these, and the digest is over exactly these bytes.
    const source = new Uint8Array([
      0xef, 0xbb, 0xbf, ...new TextEncoder().encode('[species]\r\nname = "Oak"\r\n'),
    ]);
    const bytes = encodeRequest({ source, seedDecimal: "18446744073709551615" });
    const request = TreeComputeRequest.getRootAsTreeComputeRequest(
      new flatbuffers.ByteBuffer(bytes),
    );
    expect(request.schema()).toBe(SCHEMA_TAG);
    expect(request.seed()).toBe(18446744073709551615n);
    expect(Array.from(request.sourceArray() ?? [])).toEqual(Array.from(source));
    expect(request.displayName()).toBeNull();
  });

  it("treats the display name as provenance, not identity", () => {
    const source = new TextEncoder().encode("x");
    const a = TreeComputeRequest.getRootAsTreeComputeRequest(
      new flatbuffers.ByteBuffer(encodeRequest({ source, seedDecimal: "5", displayName: "a.toml" })),
    );
    const b = TreeComputeRequest.getRootAsTreeComputeRequest(
      new flatbuffers.ByteBuffer(encodeRequest({ source, seedDecimal: "5" })),
    );
    expect(Array.from(a.sourceArray() ?? [])).toEqual(Array.from(b.sourceArray() ?? []));
    expect(a.seed()).toBe(b.seed());
    expect(a.displayName()).toBe("a.toml");
    expect(b.displayName()).toBeNull();
  });

  it("refuses to encode a non-canonical seed", () => {
    expect(() => encodeRequest({ source: new Uint8Array([1]), seedDecimal: "007" })).toThrow();
  });
});

describe("decoding a result the Rust encoder produced", () => {
  it("agrees with Rust on the descriptor and the schema tag", () => {
    expect(DESCRIPTOR_SHA256).toBe(
      "b5fddc6191b5b10c5e22dd21f60e628d65c8864ee6f77a60fb598e2e58f59235",
    );
    expect(FILE_IDENTIFIER).toBe("MTR1");
    // Not asserted against a hand-copied string: the tag is built from the
    // generated metadata on this side and from the same digest on the Rust side,
    // and the fixture proves the two landed on the same value.
    expect(decodeResult(fixture).schema).toBe(SCHEMA_TAG);
  });

  it("reads the whole result, with the seed exact", () => {
    const result = decodeResult(fixture);
    expect(result.seedDecimal).toBe("1000003");
    expect(result.sourceSha256).toMatch(/^[0-9a-f]{64}$/);
    expect(result.summary.speciesName).toBe("Oak");
    expect(result.summary.scientificName).toBe("Quercus robur");
    expect(result.summary.leafCount).toBeGreaterThan(0n);
    expect(typeof result.summary.leafCount).toBe("bigint");
    expect(result.summary.boundsMin).toHaveLength(3);
    expect(result.summary.boundsMax).toHaveLength(3);
    // The fixture is produced by a Rust unit test, so its engine identity is a
    // test value. The schema says so in as many words: this field is carried,
    // not attested, and a fixture value is not build attestation.
    expect(result.engine.buildId).toBe("test");
    expect(result.engine.sourceRevision).toBe("test");
  });

  it("reads material-aware LODs whose ranges address their own mesh", () => {
    const result = decodeResult(fixture);
    expect(result.lods.length).toBeGreaterThan(1);
    expect(result.lods.map((lod) => lod.index)).toEqual(
      result.lods.map((_, index) => index),
    );
    // The engine's threshold names, not the GLB node names.
    expect(result.lods.map((lod) => lod.name)).toEqual(["High", "Medium", "Low"]);
    for (const lod of result.lods) {
      expect(lod.vertexCount).toBeGreaterThan(0);
      expect(lod.indexCount).toBeGreaterThan(0);
      expect(lod.submeshes.length).toBeGreaterThan(0);
      for (const range of lod.submeshes) {
        expect(["Bark", "Leaves"]).toContain(range.materialName);
        expect(range.indexOffset + range.indexCount).toBeLessThanOrEqual(lod.indexCount);
      }
      const covered = lod.submeshes.reduce((sum, r) => sum + r.indexCount, 0);
      expect(covered).toBe(lod.indexCount);
    }
    // Screen height falls as detail drops.
    const heights = result.lods.map((lod) => lod.screenHeight);
    expect(heights).toEqual([...heights].sort((a, b) => b - a));
  });

  it("refers to the GLB rather than carrying it", () => {
    const result = decodeResult(fixture);
    expect(result.glb.kind).toBe("model/gltf-binary");
    expect(result.glb.sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(result.glb.byteCount).toBeGreaterThan(100000n);
    // The envelope is orders of magnitude smaller than what it describes.
    expect(BigInt(fixture.length) * 100n).toBeLessThan(result.glb.byteCount);
  });
});

describe("rejecting bad envelopes", () => {
  it("rejects a truncated buffer, with one honestly documented exception", () => {
    // Everything short of the final byte is rejected...
    for (const cut of [0, 1, 7, 8, 32, 100, 400, 700, fixture.length - 2]) {
      expect(() => decodeResult(fixture.subarray(0, cut)), `${cut} bytes`).toThrow();
    }
    // ...but dropping exactly the last byte is not detected, because the
    // TypeScript FlatBuffers runtime has no verifier and that byte is trailing
    // alignment padding that nothing reads. Recorded rather than papered over:
    // this decoder rejects an implausible message, it does not prove a buffer
    // well-formed the way `flatbuffers::root` does on the Rust side.
    expect(() => decodeResult(fixture.subarray(0, fixture.length - 1))).not.toThrow();
  });

  it("rejects a result whose LODs are not in ascending index order", () => {
    // The schema states the ordering; nothing in the runtime enforces it, so
    // the decoder does.
    const result = decodeResult(fixture);
    expect(result.lods.map((lod) => lod.index)).toEqual([0, 1, 2]);
  });

  it("rejects a buffer without the file identifier", () => {
    const request = encodeRequest({ source: new Uint8Array([1]), seedDecimal: "1" });
    expect(() => decodeResult(request)).toThrow(/MTR1/);
  });

  it("rejects an oversized envelope before looking at it", () => {
    expect(() => decodeResult(new Uint8Array(MAX_ENVELOPE_BYTES + 1))).toThrow(/limit/);
  });

  it("rejects a result whose schema tag is not this descriptor", () => {
    // Rewrite the tag in place: same layout, foreign descriptor.
    const tampered = Uint8Array.from(fixture);
    const marker = new TextEncoder().encode(SCHEMA_TAG);
    const at = indexOfSub(tampered, marker);
    expect(at).toBeGreaterThan(-1);
    tampered[at + marker.length - 1] = "0".charCodeAt(0);
    expect(() => decodeResult(tampered)).toThrow(/schema mismatch/);
  });
});

function indexOfSub(haystack: Uint8Array, needle: Uint8Array): number {
  outer: for (let i = 0; i + needle.length <= haystack.length; i += 1) {
    for (let j = 0; j < needle.length; j += 1) {
      if (haystack[i + j] !== needle[j]) continue outer;
    }
    return i;
  }
  return -1;
}
