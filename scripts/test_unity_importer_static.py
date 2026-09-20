#!/usr/bin/env python3
"""Editorless preflight for the Unity Midori nature importer.

This script does not replace a licensed Unity editor import. It catches the
things we can verify locally: the Unity importer source still contains the
binary scatter validation/report contract, and the generated reference package
contains scatter binaries that satisfy the same record-level invariants.
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SCATTER_MAGIC = b"MDSI"
SCATTER_VERSION = 1
SCATTER_STRIDE = 32
SCATTER_RECORD = struct.Struct("<ffffffff")
EPSILON = 0.001


REQUIRED_SOURCE_FRAGMENTS = {
    "binary parser": "ReadScatterChunk",
    "exact length validation": "expectedLength",
    "record range validation": "ValidateScatterInstance",
    "finite float validation": "IsFinite",
    "file checksum": "scatterBinaryFileChecksumXor",
    "record checksum": "scatterBinaryRecordChecksumXor",
    "chunk report": "scatterChunkReports",
    "preview-only shader policy": "shader_policy != \"preview_only\"",
    "parked texture pipeline": "texture_pipeline != \"parked\"",
    "Unity Y+ normal": "unity_yplus_file",
    "batch import flag": "-midoriPackage",
    "import screenshot flag": "-midoriImportScreenshot",
    "density screenshot flag": "-midoriDensityScreenshot",
    "screenshot artifact summary": "MidoriUnityScreenshotReport",
    "screenshot artifact status": "SummarizeScreenshotArtifact",
    "screenshot PNG dimension check": "TryReadPngDimensions",
    "import screenshot report": "importScreenshot",
    "density screenshot report": "densityScreenshot",
    "source file collection": "CollectUnitySourceFiles",
    "native GLB fallback": "LoadNativeGlbMesh",
    "native GLB prefab": "CreateNativeGlbPrefab",
    "native GLB generated report": "detailPrototypesGeneratedFromGlb",
    "prototype failure report": "detailPrototypeFailures",
    "prototype fallback error report": "detailPrototypeFallbackErrors",
    "manifest file checksum report": "manifestFileChecksum",
    "source file count report": "sourceFileCount",
    "source file checksum report": "sourceFileChecksumXor",
    "source file list report": "sourceFiles",
    "material parameter validation": "ValidateMaterialParameters",
    "material parameter set count report": "materialParameterSetCount",
    "material parameter semantics report": "materialParameterSemantics",
    "groundcover material parameter report": "groundcoverMaterialParameterNames",
    "material recipe validation": "ValidateMaterialRecipes",
    "material recipe count report": "materialRecipeCount",
    "material recipe engine target report": "materialRecipeEngineTargets",
    "terrain material recipe report": "terrainMaterialRecipeFile",
    "groundcover material recipe report": "groundcoverMaterialRecipeFile",
    "engine import recipe validation": "ValidateEngineImportRecipes",
    "engine import recipe count report": "engineImportRecipeCount",
    "engine import recipe systems report": "engineImportRecipeExpectedSystems",
    "Unity engine import recipe report": "unityEngineImportRecipeFile",
    "Unreal engine import recipe report": "unrealEngineImportRecipeFile",
    "surface overlay validation": "HasSurfaceOverlay",
    "surface overlay count report": "surfaceOverlayCount",
    "surface overlay name report": "surfaceOverlayNames",
    "surface overlay target report": "surfaceOverlayTargets",
    "prototype surface target validation": "ValidatePrototypeSurfaceTargets",
    "prototype surface target report": "prototypeSurfaceTargets",
    "rock prototype surface target report": "rockPrototypeSurfaceTargets",
    "log prototype surface target report": "logPrototypeSurfaceTargets",
    "mobile tile instance budget report": "mobileMaxInstancesPerTile",
    "mobile chunk instance budget report": "mobileMaxInstancesPerChunk",
    "mobile LOD0 distance report": "mobileLod0MaxDistance",
    "mobile LOD1 distance report": "mobileLod1MaxDistance",
    "mobile LOD2 distance report": "mobileLod2MaxDistance",
    "mobile grass collision report": "mobileGrassCollision",
    "mobile moss collision report": "mobileMossCollision",
}


@dataclass
class ScatterStats:
    chunks: int = 0
    instances: int = 0


def check_source_contract(source_path: Path) -> None:
    text = source_path.read_text(encoding="utf-8")
    missing = [
        label
        for label, fragment in REQUIRED_SOURCE_FRAGMENTS.items()
        if fragment not in text
    ]
    if missing:
        raise AssertionError(
            "Unity importer source is missing contract fragments: "
            + ", ".join(sorted(missing))
        )


def validate_package(package_dir: Path) -> ScatterStats:
    manifest_path = package_dir / "midori_nature.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    scatter = manifest.get("scatter") or {}
    binary_format = scatter.get("binary_format") or {}
    if (
        binary_format.get("format") != "midori.scatter.bin.v1"
        or binary_format.get("header_bytes") != 16
        or binary_format.get("record_stride_bytes") != SCATTER_STRIDE
        or binary_format.get("endian") != "little"
    ):
        raise AssertionError("Manifest scatter binary format is not midori.scatter.bin.v1")

    stats = ScatterStats()
    for item in scatter.get("binary_files", []):
        validate_scatter_file(package_dir, item)
        stats.chunks += 1
        stats.instances += int(item["instance_count"])
    return stats


def validate_scatter_file(package_dir: Path, item: dict[str, Any]) -> None:
    path = package_dir / item["file"]
    data = path.read_bytes()
    if len(data) < 16:
        raise AssertionError(f"{item['file']} is too short")
    magic = data[:4]
    version, stride, count = struct.unpack("<III", data[4:16])
    if magic != SCATTER_MAGIC:
        raise AssertionError(f"{item['file']} has invalid magic")
    if version != SCATTER_VERSION or stride != SCATTER_STRIDE:
        raise AssertionError(f"{item['file']} has invalid header")
    if count != int(item["instance_count"]):
        raise AssertionError(f"{item['file']} count does not match manifest")
    expected_length = 16 + count * SCATTER_STRIDE
    if len(data) != expected_length:
        raise AssertionError(f"{item['file']} length {len(data)} != {expected_length}")
    if count == 0:
        raise AssertionError(f"{item['file']} must not be empty")

    bounds_min = item["bounds_min"]
    bounds_max = item["bounds_max"]
    for index in range(count):
        offset = 16 + index * SCATTER_STRIDE
        record = SCATTER_RECORD.unpack_from(data, offset)
        validate_scatter_record(record, bounds_min, bounds_max, item["file"])


def validate_scatter_record(
    record: tuple[float, ...],
    bounds_min: list[float],
    bounds_max: list[float],
    label: str,
) -> None:
    x, y, z, yaw, height, width, phase, color_variation = record
    values = [x, y, z, yaw, height, width, phase, color_variation]
    if any(not math.isfinite(value) for value in values):
        raise AssertionError(f"{label} contains a non-finite record")
    if not (
        bounds_min[0] - EPSILON <= x <= bounds_max[0] + EPSILON
        and bounds_min[1] - EPSILON <= y <= bounds_max[1] + EPSILON
        and bounds_min[2] - EPSILON <= z <= bounds_max[2] + EPSILON
    ):
        raise AssertionError(f"{label} contains an out-of-bounds record")
    if not 0.0 <= yaw <= math.tau:
        raise AssertionError(f"{label} contains invalid yaw")
    if height <= 0.0 or width <= 0.0:
        raise AssertionError(f"{label} contains invalid scale")
    if not 0.0 <= phase <= math.tau:
        raise AssertionError(f"{label} contains invalid phase")
    if not 0.0 <= color_variation <= 1.0:
        raise AssertionError(f"{label} contains invalid color variation")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=Path("integrations/unity/Editor/MidoriNaturePackageImporter.cs"),
    )
    parser.add_argument(
        "--package-dir",
        type=Path,
        default=Path("target/midori_engine_validation/forest_floor"),
    )
    parser.add_argument("--expect-chunks", type=int, default=26)
    parser.add_argument("--expect-instances", type=int, default=222)
    args = parser.parse_args()

    check_source_contract(args.source)
    if args.package_dir.is_dir():
        stats = validate_package(args.package_dir)
        if stats.chunks != args.expect_chunks:
            raise AssertionError(f"expected {args.expect_chunks} chunks, got {stats.chunks}")
        if stats.instances != args.expect_instances:
            raise AssertionError(
                f"expected {args.expect_instances} instances, got {stats.instances}"
            )
        print(
            "unity importer preflight ok: "
            f"{stats.chunks} chunks, {stats.instances} scatter instances"
        )
    else:
        print("unity importer source preflight ok; package directory missing, skipped package check")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as exc:
        print(f"unity importer preflight failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
