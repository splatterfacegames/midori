#!/usr/bin/env python3
"""Verify Phase 7 Unity/Unreal evidence for a Midori nature package.

The script intentionally distinguishes preflight evidence from completion
evidence. `--allow-pending` is useful on machines without licensed/installed
editors; omit it when deciding whether Phase 7 is actually complete.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import struct
import sys
import zlib
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path("target/midori_engine_validation")
DEFAULT_SCREENSHOTS = {
    "unity_import_screenshot": Path("docs/validation/screenshots/unity_forest_floor_import.png"),
    "unity_density_screenshot": Path("docs/validation/screenshots/unity_forest_floor_density.png"),
    "unreal_import_screenshot": Path("docs/validation/screenshots/unreal_forest_floor_import.png"),
    "unreal_foliage_settings_screenshot": Path(
        "docs/validation/screenshots/unreal_forest_floor_foliage_settings.png"
    ),
    "profile_notes": Path("docs/validation/midori-nature-engine-profile-notes.md"),
}

PROFILE_NOTE_REQUIRED_TERMS = [
    "Unity",
    "Unreal",
    "Unity Editor version",
    "Unreal Editor version",
    "mobile",
    "console",
    "LOD",
    "Frame Debugger",
    "RenderDoc",
    "instanced",
    "scale",
    "density",
    "cull",
    "material slot",
    "wind",
    "strict verifier",
    "forest_floor_unity_import_report.json",
    "forest_floor_unreal_editor_report.json",
    "unity_forest_floor_import.png",
    "unity_forest_floor_density.png",
    "unreal_forest_floor_import.png",
    "unreal_forest_floor_foliage_settings.png",
    "detailPrototypesCreated",
    "detailPrototypesGeneratedFromGlb",
    "detailPrototypeFailures",
    "detailPrototypeFallbackErrors",
    "scatterBinaryRecordsRead",
    "scatterChunkReports",
    "foliage_type_count",
    "foliage_type_assets",
    "3500",
    "7000",
]
PROFILE_NOTE_REQUIRED_SECTIONS = [
    "## Evidence Artifacts",
    "## Unity",
    "## Unreal",
    "## Verdict",
]
PROFILE_NOTE_FORBIDDEN_PATTERNS = [
    ("TODO", r"\bTODO\b"),
    ("TBD", r"\bTBD\b"),
    ("placeholder", r"\bplaceholder\b"),
    ("pending", r"\bpending\b"),
    ("not run", r"\bnot\s+run\b"),
    ("not captured", r"\bnot\s+captured\b"),
    ("missing evidence", r"\bmissing\s+(?:report|screenshot|notes?|evidence|artifact)s?\b"),
    ("fill marker", r"\[(?:fill|replace|todo)[^\]\n]*\]"),
]

MIN_SCREENSHOT_SIZE = 256
MIN_SCREENSHOT_LUMINANCE_RANGE = 8
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
FNV_OFFSET_BASIS = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
U64_MASK = 0xFFFFFFFFFFFFFFFF
EXPECTED_MAP_SUMMARIES = {
    "height_u16.png": {
        "color_type": "L16",
        "channels": 1,
        "varying_channels": [0],
    },
    "normal_yplus.png": {
        "color_type": "RGB8",
        "channels": 3,
        "varying_any_channels": [0, 1, 2],
    },
    "normal_yminus.png": {
        "color_type": "RGB8",
        "channels": 3,
        "varying_any_channels": [0, 1, 2],
    },
    "masks_rgba.png": {
        "color_type": "RGBA8",
        "channels": 4,
        "varying_channels": [0, 1],
    },
    "grass_density.png": {
        "color_type": "L8",
        "channels": 1,
        "varying_channels": [0],
    },
}


@dataclass
class Check:
    name: str
    status: str
    detail: str


class EvidenceVerifier:
    def __init__(self) -> None:
        self.checks: list[Check] = []

    def pass_(self, name: str, detail: str) -> None:
        self.checks.append(Check(name, "passed", detail))

    def fail(self, name: str, detail: str) -> None:
        self.checks.append(Check(name, "failed", detail))

    def missing(self, name: str, detail: str) -> None:
        self.checks.append(Check(name, "missing", detail))

    def require(self, name: str, condition: bool, detail: str, failure: str | None = None) -> None:
        if condition:
            self.pass_(name, detail)
        else:
            self.fail(name, failure or detail)

    def require_equal(self, name: str, actual: Any, expected: Any) -> None:
        self.require(name, actual == expected, f"{actual!r} == {expected!r}", f"{actual!r} != {expected!r}")

    def require_close(self, name: str, actual: float, expected: float, tolerance: float = 0.001) -> None:
        self.require(
            name,
            math.isclose(float(actual), float(expected), abs_tol=tolerance),
            f"{actual} ~= {expected}",
            f"{actual} not within {tolerance} of {expected}",
        )

    def overall_status(self) -> str:
        statuses = {check.status for check in self.checks}
        if "failed" in statuses:
            return "failed"
        if "missing" in statuses:
            return "pending"
        return "passed"


def load_json(path: Path) -> Any | None:
    if not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8-sig"))


def file_nonempty(path: Path) -> bool:
    return path.is_file() and path.stat().st_size > 0


def png_dimensions(path: Path) -> tuple[int, int] | None:
    if not path.is_file():
        return None
    with path.open("rb") as handle:
        header = handle.read(24)
    if len(header) < 24 or header[:8] != PNG_SIGNATURE or header[12:16] != b"IHDR":
        return None
    width, height = struct.unpack(">II", header[16:24])
    return width, height


def read_png_chunks(path: Path) -> tuple[dict[str, int], bytes, list[tuple[int, int, int]] | None]:
    data = path.read_bytes()
    if len(data) < 24 or data[:8] != PNG_SIGNATURE:
        raise ValueError("missing PNG signature")

    offset = 8
    header: dict[str, int] | None = None
    idat = bytearray()
    palette: list[tuple[int, int, int]] | None = None

    while offset + 8 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        chunk_start = offset + 8
        chunk_end = chunk_start + length
        if chunk_end + 4 > len(data):
            raise ValueError("truncated PNG chunk")
        chunk = data[chunk_start:chunk_end]

        if kind == b"IHDR":
            if length != 13:
                raise ValueError("invalid IHDR length")
            width, height, bit_depth, color_type, compression, filter_method, interlace = struct.unpack(
                ">IIBBBBB", chunk
            )
            header = {
                "width": width,
                "height": height,
                "bit_depth": bit_depth,
                "color_type": color_type,
                "compression": compression,
                "filter_method": filter_method,
                "interlace": interlace,
            }
        elif kind == b"PLTE":
            if length % 3 != 0:
                raise ValueError("invalid palette length")
            palette = [
                (chunk[index], chunk[index + 1], chunk[index + 2])
                for index in range(0, length, 3)
            ]
        elif kind == b"IDAT":
            idat.extend(chunk)
        elif kind == b"IEND":
            break

        offset = chunk_end + 4

    if header is None:
        raise ValueError("missing IHDR chunk")
    if not idat:
        raise ValueError("missing IDAT data")

    return header, bytes(idat), palette


def paeth_predictor(left: int, up: int, up_left: int) -> int:
    estimate = left + up - up_left
    distance_left = abs(estimate - left)
    distance_up = abs(estimate - up)
    distance_up_left = abs(estimate - up_left)
    if distance_left <= distance_up and distance_left <= distance_up_left:
        return left
    if distance_up <= distance_up_left:
        return up
    return up_left


def unfilter_png_row(filter_type: int, row: bytearray, previous: bytes, bytes_per_pixel: int) -> bytes:
    if filter_type == 0:
        return bytes(row)
    if filter_type not in {1, 2, 3, 4}:
        raise ValueError(f"unsupported PNG filter type {filter_type}")

    for index, value in enumerate(row):
        left = row[index - bytes_per_pixel] if index >= bytes_per_pixel else 0
        up = previous[index] if previous else 0
        up_left = previous[index - bytes_per_pixel] if previous and index >= bytes_per_pixel else 0

        if filter_type == 1:
            predictor = left
        elif filter_type == 2:
            predictor = up
        elif filter_type == 3:
            predictor = (left + up) // 2
        else:
            predictor = paeth_predictor(left, up, up_left)

        row[index] = (value + predictor) & 0xFF

    return bytes(row)


def png_luminance_stats(path: Path) -> tuple[int, int, int]:
    header, idat, palette = read_png_chunks(path)
    width = header["width"]
    height = header["height"]
    bit_depth = header["bit_depth"]
    color_type = header["color_type"]

    if header["compression"] != 0 or header["filter_method"] != 0:
        raise ValueError("unsupported PNG compression or filter method")
    if header["interlace"] != 0:
        raise ValueError("interlaced PNG screenshots are not supported")
    if bit_depth != 8:
        raise ValueError("only 8-bit PNG screenshots are supported")

    channel_counts = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
    if color_type not in channel_counts:
        raise ValueError(f"unsupported PNG color type {color_type}")
    if color_type == 3 and not palette:
        raise ValueError("indexed PNG is missing a palette")

    channels = channel_counts[color_type]
    row_size = width * channels
    try:
        raw = zlib.decompress(idat)
    except zlib.error as error:
        raise ValueError(f"invalid PNG image data: {error}") from error

    expected_size = (row_size + 1) * height
    if len(raw) < expected_size:
        raise ValueError(f"truncated PNG image data: {len(raw)} bytes, expected {expected_size}")

    step_x = max(1, width // 64)
    step_y = max(1, height // 64)
    luminance_min = 255
    luminance_max = 0
    sample_count = 0
    previous = bytes(row_size)
    offset = 0

    for y in range(height):
        filter_type = raw[offset]
        row = bytearray(raw[offset + 1 : offset + 1 + row_size])
        offset += row_size + 1
        decoded = unfilter_png_row(filter_type, row, previous, channels)
        previous = decoded

        if y % step_y != 0:
            continue

        for x in range(0, width, step_x):
            index = x * channels
            if color_type == 0 or color_type == 4:
                luminance = decoded[index]
            elif color_type == 3:
                palette_index = decoded[index]
                if palette is None or palette_index >= len(palette):
                    raise ValueError("indexed PNG references a missing palette color")
                red, green, blue = palette[palette_index]
                luminance = (red + green + blue) // 3
            else:
                red, green, blue = decoded[index], decoded[index + 1], decoded[index + 2]
                luminance = (red + green + blue) // 3

            luminance_min = min(luminance_min, luminance)
            luminance_max = max(luminance_max, luminance)
            sample_count += 1

    return luminance_min, luminance_max, sample_count


def vector_component(value: Any, component: str) -> float:
    if isinstance(value, dict):
        return float(value[component])
    if isinstance(value, list):
        index = {"x": 0, "y": 1, "z": 2}[component]
        return float(value[index])
    raise TypeError(f"unsupported vector shape: {value!r}")


def int_or_default(value: Any, default: int) -> int:
    try:
        return int(value)
    except (TypeError, ValueError):
        return default


def float_or_default(value: Any, default: float) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def is_nonzero_hex_checksum(value: Any) -> bool:
    if not isinstance(value, str) or not value.startswith("0x"):
        return False
    try:
        return int(value, 16) > 0
    except ValueError:
        return False


def fnv1a(data: bytes, seed: int = FNV_OFFSET_BASIS) -> int:
    value = seed
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) & U64_MASK
    return value


def hex_u64(value: int) -> str:
    return f"0x{value & U64_MASK:016x}"


def file_checksum_hex(path: Path) -> str:
    return hex_u64(fnv1a(path.read_bytes()))


def expected_source_files(manifest: dict[str, Any], normal_key: str) -> list[str]:
    files = {
        "midori_nature.json",
        "preview_tile.glb",
        manifest["terrain"]["heightmap_file"],
        manifest["terrain"]["masks_file"],
        manifest["terrain"]["grass_density_file"],
        manifest["normal_conventions"][normal_key],
    }
    scatter = manifest.get("scatter", {})
    scatter_json = scatter.get("file")
    if scatter_json:
        files.add(scatter_json)
    for item in scatter.get("binary_files", []):
        if isinstance(item, dict):
            files.add(str(item.get("file", "")))
    for prototype in manifest.get("prototypes", []):
        if not isinstance(prototype, dict):
            continue
        for lod in prototype.get("lods", []):
            if isinstance(lod, dict):
                files.add(str(lod.get("file", "")))
    for recipe in manifest.get("material_recipes", []):
        if isinstance(recipe, dict):
            files.add(str(recipe.get("file", "")))
    for recipe in manifest.get("engine_import_recipes", []):
        if isinstance(recipe, dict):
            files.add(str(recipe.get("file", "")))
    return sorted(file for file in files if file)


def expected_unreal_import_files(manifest: dict[str, Any]) -> list[str]:
    files = {
        "midori_nature.json",
        "preview_tile.glb",
        manifest["terrain"]["heightmap_file"],
        manifest["terrain"]["masks_file"],
        manifest["terrain"]["grass_density_file"],
        manifest["normal_conventions"]["unreal_yminus_file"],
    }
    for prototype in manifest.get("prototypes", []):
        if not isinstance(prototype, dict):
            continue
        for lod in prototype.get("lods", []):
            if isinstance(lod, dict):
                files.add(str(lod.get("file", "")))
    return sorted(file for file in files if file)


def expected_unreal_import_destinations(
    destination_path: str,
    import_files: list[str],
) -> list[str]:
    destinations: list[str] = []
    for relative in import_files:
        parent = Path(relative).parent.as_posix()
        if parent == ".":
            destinations.append(destination_path)
        else:
            destinations.append(f"{destination_path}/{parent}")
    return destinations


def expected_unreal_import_map_files(manifest: dict[str, Any]) -> list[str]:
    return [
        manifest["terrain"]["heightmap_file"],
        manifest["terrain"]["masks_file"],
        manifest["terrain"]["grass_density_file"],
        manifest["normal_conventions"]["unreal_yminus_file"],
    ]


def expected_prototype_lod_files(manifest: dict[str, Any]) -> list[str]:
    files: list[str] = []
    for prototype in manifest.get("prototypes", []):
        if not isinstance(prototype, dict):
            continue
        for lod in prototype.get("lods", []):
            if isinstance(lod, dict):
                files.append(str(lod.get("file", "")))
    return sorted(file for file in files if file)


def expected_lod0_prototype_files(manifest: dict[str, Any]) -> list[str]:
    files: list[str] = []
    for prototype in manifest.get("prototypes", []):
        if not isinstance(prototype, dict):
            continue
        for lod in prototype.get("lods", []):
            if isinstance(lod, dict) and lod.get("index") == 0:
                files.append(str(lod.get("file", "")))
    return sorted(file for file in files if file)


def source_file_checksum_xor(package_dir: Path, source_files: list[str]) -> str:
    checksum = 0
    for relative in source_files:
        checksum ^= fnv1a((package_dir / relative).read_bytes())
    return hex_u64(checksum)


def check_package_identity(
    verifier: EvidenceVerifier,
    prefix: str,
    report: dict[str, Any],
    manifest: dict[str, Any],
    package_dir: Path,
    normal_key: str,
    camel_case: bool,
) -> None:
    source_files = expected_source_files(manifest, normal_key)
    try:
        expected_manifest_checksum = file_checksum_hex(package_dir / "midori_nature.json")
        expected_source_checksum = source_file_checksum_xor(package_dir, source_files)
    except OSError as error:
        verifier.fail(f"{prefix}.package_identity", f"could not read package source files: {error}")
        return

    manifest_field = "manifestFileChecksum" if camel_case else "manifest_file_checksum"
    count_field = "sourceFileCount" if camel_case else "source_file_count"
    checksum_field = "sourceFileChecksumXor" if camel_case else "source_file_checksum_xor"
    files_field = "sourceFiles" if camel_case else "source_files"

    verifier.require_equal(
        f"{prefix}.manifest_file_checksum",
        report.get(manifest_field),
        expected_manifest_checksum,
    )
    verifier.require_equal(f"{prefix}.source_file_count", report.get(count_field), len(source_files))
    verifier.require_equal(
        f"{prefix}.source_file_checksum_xor",
        report.get(checksum_field),
        expected_source_checksum,
    )
    verifier.require_equal(f"{prefix}.source_files", report.get(files_field), source_files)


def check_midori_report(verifier: EvidenceVerifier, report: dict[str, Any]) -> dict[str, Any]:
    manifest = report["manifest"]
    verifier.require_equal("midori.schema_version", manifest.get("schema_version"), 3)
    verifier.require_equal("midori.asset_name", manifest.get("asset_name"), "Temperate Forest Floor")
    verifier.require_equal("midori.map_count", len(report.get("map_files", [])), 5)
    check_midori_map_summaries(verifier, report, manifest)
    check_midori_map_relationships(verifier, report, manifest)
    verifier.require_equal("midori.prototype_file_count", len(report.get("prototype_files", [])), 22)
    check_midori_prototype_summaries(verifier, report, manifest)
    check_prototype_surface_targets(verifier, "midori", manifest)
    check_midori_profile_budgets(verifier, report, manifest)
    verifier.require_equal("midori.scatter_binary_chunks", len(report.get("scatter_binary_files", [])), 26)
    verifier.require_equal("midori.scatter_binary_instances", report.get("scatter_binary_instances"), 222)
    check_midori_scatter_summaries(verifier, report)
    verifier.require_equal("midori.shader_policy", manifest.get("shader_policy"), "preview_only")
    verifier.require_equal("midori.texture_pipeline", manifest.get("texture_pipeline"), "parked")
    check_material_parameters(verifier, "midori", manifest.get("material_parameters"))
    check_material_recipes(verifier, "midori", manifest, report)
    check_engine_import_recipes(verifier, "midori", manifest, report)
    check_surface_overlays(verifier, "midori", manifest.get("surface_overlays"))
    check_midori_memory_footprint(verifier, report, manifest)
    return manifest


def check_surface_overlays(
    verifier: EvidenceVerifier,
    prefix: str,
    overlays: Any,
) -> None:
    if not isinstance(overlays, list):
        verifier.fail(f"{prefix}.surface_overlays", "manifest is missing surface_overlays")
        return

    verifier.require_equal(f"{prefix}.surface_overlay_count", len(overlays), 3)
    by_name = {
        overlay.get("name"): overlay
        for overlay in overlays
        if isinstance(overlay, dict)
    }
    expected = {
        "moss": ("G", {"terrain_surface", "rock", "log"}),
        "wetness": ("B", {"terrain_surface", "rock", "log"}),
        "cracks": ("A", {"terrain_surface", "scatter_exclusion"}),
    }
    for name, (channel, targets) in expected.items():
        overlay = by_name.get(name)
        if not isinstance(overlay, dict):
            verifier.fail(f"{prefix}.surface_overlay.{name}", "surface overlay is missing")
            continue
        verifier.require_equal(f"{prefix}.surface_overlay.{name}.source_file", overlay.get("source_file"), "maps/masks_rgba.png")
        verifier.require_equal(f"{prefix}.surface_overlay.{name}.channel", overlay.get("channel"), channel)
        actual_targets = set(overlay.get("targets") or [])
        verifier.require(
            f"{prefix}.surface_overlay.{name}.targets",
            targets.issubset(actual_targets),
            f"{sorted(actual_targets)} includes {sorted(targets)}",
            f"{name} surface overlay must target {sorted(targets)}",
        )
        verifier.require_equal(f"{prefix}.surface_overlay.{name}.runtime_policy", overlay.get("runtime_policy"), "baked_static")


def required_material_parameter_semantics() -> dict[str, set[str]]:
    return {
        "terrain_surface": {
            "overlay_mask_texture",
            "moss_mask_channel",
            "wetness_mask_channel",
            "crack_mask_channel",
        },
        "groundcover_foliage": {
            "alpha_cutoff",
            "wind_strength",
            "wind_speed",
            "wind_direction_degrees",
            "wind_gust_scale",
            "fade_start_meters",
            "fade_end_meters",
            "color_variation_scale",
        },
    }


def expected_material_parameter_semantics(manifest: dict[str, Any]) -> list[str]:
    values: list[str] = []
    for material_set in manifest.get("material_parameters", []):
        if not isinstance(material_set, dict):
            continue
        slot = material_set.get("material_slot", "")
        for parameter in material_set.get("parameters", []):
            if isinstance(parameter, dict):
                values.append(f"{slot}:{parameter.get('semantic', '')}")
    return values


def expected_material_parameter_names_for_slot(
    manifest: dict[str, Any],
    slot: str,
) -> list[str]:
    names: set[str] = set()
    for material_set in manifest.get("material_parameters", []):
        if not isinstance(material_set, dict) or material_set.get("material_slot") != slot:
            continue
        for parameter in material_set.get("parameters", []):
            if isinstance(parameter, dict) and parameter.get("name"):
                names.add(parameter["name"])
    return sorted(names)


def check_material_parameters(
    verifier: EvidenceVerifier,
    prefix: str,
    material_parameters: Any,
) -> None:
    if not isinstance(material_parameters, list):
        verifier.fail(f"{prefix}.material_parameters", "manifest is missing material_parameters")
        return

    verifier.require_equal(f"{prefix}.material_parameter_set_count", len(material_parameters), 2)
    by_slot = {
        item.get("material_slot"): item
        for item in material_parameters
        if isinstance(item, dict)
    }
    for slot, required_semantics in required_material_parameter_semantics().items():
        material_set = by_slot.get(slot)
        if not isinstance(material_set, dict):
            verifier.fail(f"{prefix}.material_parameters.{slot}", "material parameter set is missing")
            continue
        verifier.require_equal(
            f"{prefix}.material_parameters.{slot}.runtime_policy",
            material_set.get("runtime_policy"),
            "engine_native_static",
        )
        semantics = {
            parameter.get("semantic")
            for parameter in material_set.get("parameters", [])
            if isinstance(parameter, dict)
        }
        verifier.require(
            f"{prefix}.material_parameters.{slot}.semantics",
            required_semantics.issubset(semantics),
            f"{sorted(semantics)} includes {sorted(required_semantics)}",
            f"{slot} material parameters must include required semantics",
        )
        names = {
            parameter.get("name")
            for parameter in material_set.get("parameters", [])
            if isinstance(parameter, dict)
        }
        if slot == "terrain_surface":
            required_names = {"Midori_MaskTexture", "Midori_MossMaskChannel"}
        else:
            required_names = {"Midori_WindStrength", "Midori_FadeEndMeters"}
        verifier.require(
            f"{prefix}.material_parameters.{slot}.names",
            required_names.issubset(names),
            f"{sorted(names)} includes {sorted(required_names)}",
            f"{slot} material parameters must include stable engine names",
        )


def check_material_parameter_report(
    verifier: EvidenceVerifier,
    prefix: str,
    report: dict[str, Any],
    manifest: dict[str, Any],
    camel_case: bool = False,
) -> None:
    material_parameters = manifest.get("material_parameters")
    if not isinstance(material_parameters, list):
        verifier.fail(f"{prefix}.material_parameter_report", "manifest is missing material_parameters")
        return

    count_field = "materialParameterSetCount" if camel_case else "material_parameter_set_count"
    slots_field = "materialParameterSlots" if camel_case else "material_parameter_slots"
    policies_field = (
        "materialParameterRuntimePolicies"
        if camel_case
        else "material_parameter_runtime_policies"
    )
    semantics_field = "materialParameterSemantics" if camel_case else "material_parameter_semantics"
    groundcover_names_field = (
        "groundcoverMaterialParameterNames"
        if camel_case
        else "groundcover_material_parameter_names"
    )
    terrain_names_field = (
        "terrainMaterialParameterNames" if camel_case else "terrain_material_parameter_names"
    )

    verifier.require_equal(
        f"{prefix}.material_parameter_set_count",
        report.get(count_field),
        len(material_parameters),
    )
    verifier.require_equal(
        f"{prefix}.material_parameter_slots",
        report.get(slots_field),
        [item.get("material_slot") for item in material_parameters],
    )
    verifier.require_equal(
        f"{prefix}.material_parameter_runtime_policies",
        report.get(policies_field),
        [item.get("runtime_policy") for item in material_parameters],
    )
    verifier.require_equal(
        f"{prefix}.material_parameter_semantics",
        report.get(semantics_field),
        expected_material_parameter_semantics(manifest),
    )
    verifier.require_equal(
        f"{prefix}.groundcover_material_parameter_names",
        report.get(groundcover_names_field),
        expected_material_parameter_names_for_slot(manifest, "groundcover_foliage"),
    )
    verifier.require_equal(
        f"{prefix}.terrain_material_parameter_names",
        report.get(terrain_names_field),
        expected_material_parameter_names_for_slot(manifest, "terrain_surface"),
    )


def expected_material_recipe_files(manifest: dict[str, Any]) -> list[str]:
    return [
        item.get("file", "")
        for item in manifest.get("material_recipes", [])
        if isinstance(item, dict)
    ]


def expected_material_recipe_engine_targets(manifest: dict[str, Any]) -> list[str]:
    values: list[str] = []
    for recipe in manifest.get("material_recipes", []):
        if not isinstance(recipe, dict):
            continue
        slot = recipe.get("material_slot", "")
        for target in recipe.get("engine_targets", []):
            values.append(f"{slot}:{target}")
    return values


def material_recipe_file_for_slot(manifest: dict[str, Any], slot: str) -> str:
    for recipe in manifest.get("material_recipes", []):
        if isinstance(recipe, dict) and recipe.get("material_slot") == slot:
            return recipe.get("file", "")
    return ""


def check_material_recipes(
    verifier: EvidenceVerifier,
    prefix: str,
    manifest: dict[str, Any],
    report: dict[str, Any],
) -> None:
    recipes = manifest.get("material_recipes")
    if not isinstance(recipes, list):
        verifier.fail(f"{prefix}.material_recipes", "manifest is missing material_recipes")
        return

    verifier.require_equal(f"{prefix}.material_recipe_count", len(recipes), 2)
    by_slot = {
        item.get("material_slot"): item
        for item in recipes
        if isinstance(item, dict)
    }
    required_targets = {
        "terrain_surface": {"unity_terrain_material", "unreal_landscape_material"},
        "groundcover_foliage": {
            "unity_detail_mesh_material",
            "unreal_static_mesh_foliage_material",
        },
    }
    for slot, targets in required_targets.items():
        recipe = by_slot.get(slot)
        if not isinstance(recipe, dict):
            verifier.fail(f"{prefix}.material_recipe.{slot}", "material recipe is missing")
            continue
        verifier.require_equal(
            f"{prefix}.material_recipe.{slot}.runtime_policy",
            recipe.get("runtime_policy"),
            "engine_native_static",
        )
        file_name = str(recipe.get("file", ""))
        verifier.require(
            f"{prefix}.material_recipe.{slot}.file",
            file_name.startswith("materials/") and file_name.endswith(".recipe.json"),
            file_name,
            f"{slot} material recipe must live under materials/*.recipe.json",
        )
        actual_targets = set(recipe.get("engine_targets") or [])
        verifier.require(
            f"{prefix}.material_recipe.{slot}.engine_targets",
            targets.issubset(actual_targets),
            f"{sorted(actual_targets)} includes {sorted(targets)}",
            f"{slot} material recipe must target Unity and Unreal systems",
        )

    summaries = report.get("material_recipe_summaries")
    if isinstance(summaries, list):
        verifier.require_equal(
            f"{prefix}.material_recipe_summary_count",
            len(summaries),
            len(recipes),
        )
        summary_by_slot = {
            item.get("material_slot"): item
            for item in summaries
            if isinstance(item, dict)
        }
        for slot in required_targets:
            summary = summary_by_slot.get(slot)
            if not isinstance(summary, dict):
                verifier.fail(f"{prefix}.material_recipe_summary.{slot}", "summary is missing")
                continue
            verifier.require_equal(
                f"{prefix}.material_recipe_summary.{slot}.runtime_policy",
                summary.get("runtime_policy"),
                "engine_native_static",
            )
            verifier.require_equal(
                f"{prefix}.material_recipe_summary.{slot}.shader_policy",
                summary.get("shader_policy"),
                "preview_only",
            )
            verifier.require_equal(
                f"{prefix}.material_recipe_summary.{slot}.texture_pipeline",
                summary.get("texture_pipeline"),
                "parked",
            )
            verifier.require(
                f"{prefix}.material_recipe_summary.{slot}.checksum",
                int_or_default(summary.get("file_checksum"), 0) != 0,
                f"{summary.get('file_checksum')} is nonzero",
                f"{slot} material recipe checksum must be nonzero",
            )
            if slot == "terrain_surface":
                verifier.require(
                    f"{prefix}.material_recipe_summary.{slot}.required_textures",
                    "maps/masks_rgba.png" in set(summary.get("required_textures") or []),
                    str(summary.get("required_textures")),
                    "terrain material recipe must require masks_rgba.png",
                )
                verifier.require(
                    f"{prefix}.material_recipe_summary.{slot}.surface_overlays",
                    {"moss", "wetness", "cracks"}.issubset(
                        set(summary.get("surface_overlays") or [])
                    ),
                    str(summary.get("surface_overlays")),
                    "terrain material recipe must list static overlays",
                )
            else:
                streams = set(summary.get("required_vertex_streams") or [])
                verifier.require(
                    f"{prefix}.material_recipe_summary.{slot}.vertex_streams",
                    any(stream.startswith("TEXCOORD_1.x") for stream in streams)
                    and any(stream.startswith("COLOR_0.y") for stream in streams),
                    str(sorted(streams)),
                    "groundcover material recipe must require wind/color streams",
                )


def check_material_recipe_report(
    verifier: EvidenceVerifier,
    prefix: str,
    report: dict[str, Any],
    manifest: dict[str, Any],
    camel_case: bool = False,
) -> None:
    recipes = manifest.get("material_recipes")
    if not isinstance(recipes, list):
        verifier.fail(f"{prefix}.material_recipe_report", "manifest is missing material_recipes")
        return

    count_field = "materialRecipeCount" if camel_case else "material_recipe_count"
    files_field = "materialRecipeFiles" if camel_case else "material_recipe_files"
    policies_field = (
        "materialRecipeRuntimePolicies"
        if camel_case
        else "material_recipe_runtime_policies"
    )
    targets_field = (
        "materialRecipeEngineTargets" if camel_case else "material_recipe_engine_targets"
    )
    terrain_file_field = (
        "terrainMaterialRecipeFile" if camel_case else "terrain_material_recipe_file"
    )
    groundcover_file_field = (
        "groundcoverMaterialRecipeFile" if camel_case else "groundcover_material_recipe_file"
    )

    verifier.require_equal(
        f"{prefix}.material_recipe_count",
        report.get(count_field),
        len(recipes),
    )
    verifier.require_equal(
        f"{prefix}.material_recipe_files",
        report.get(files_field),
        expected_material_recipe_files(manifest),
    )
    verifier.require_equal(
        f"{prefix}.material_recipe_runtime_policies",
        report.get(policies_field),
        [item.get("runtime_policy") for item in recipes],
    )
    verifier.require_equal(
        f"{prefix}.material_recipe_engine_targets",
        report.get(targets_field),
        expected_material_recipe_engine_targets(manifest),
    )
    verifier.require_equal(
        f"{prefix}.terrain_material_recipe_file",
        report.get(terrain_file_field),
        material_recipe_file_for_slot(manifest, "terrain_surface"),
    )
    verifier.require_equal(
        f"{prefix}.groundcover_material_recipe_file",
        report.get(groundcover_file_field),
        material_recipe_file_for_slot(manifest, "groundcover_foliage"),
    )


def expected_engine_import_recipe_files(manifest: dict[str, Any]) -> list[str]:
    return [
        item.get("file", "")
        for item in manifest.get("engine_import_recipes", [])
        if isinstance(item, dict)
    ]


def expected_engine_import_recipe_profiles(manifest: dict[str, Any]) -> list[str]:
    return [
        f"{item.get('engine', '')}:{item.get('profile', '')}"
        for item in manifest.get("engine_import_recipes", [])
        if isinstance(item, dict)
    ]


def expected_engine_import_recipe_systems(manifest: dict[str, Any]) -> list[str]:
    values: list[str] = []
    for recipe in manifest.get("engine_import_recipes", []):
        if not isinstance(recipe, dict):
            continue
        engine = recipe.get("engine", "")
        for system in recipe.get("expected_systems", []):
            values.append(f"{engine}:{system}")
    return values


def engine_import_recipe_file_for_engine(manifest: dict[str, Any], engine: str) -> str:
    for recipe in manifest.get("engine_import_recipes", []):
        if isinstance(recipe, dict) and recipe.get("engine") == engine:
            return recipe.get("file", "")
    return ""


def check_engine_import_recipes(
    verifier: EvidenceVerifier,
    prefix: str,
    manifest: dict[str, Any],
    report: dict[str, Any],
) -> None:
    recipes = manifest.get("engine_import_recipes")
    if not isinstance(recipes, list):
        verifier.fail(f"{prefix}.engine_import_recipes", "manifest is missing engine_import_recipes")
        return

    verifier.require_equal(f"{prefix}.engine_import_recipe_count", len(recipes), 2)
    by_engine = {
        item.get("engine"): item
        for item in recipes
        if isinstance(item, dict)
    }
    required = {
        "unity": ("mobile", {"Unity TerrainData", "GPU-instanced terrain detail mesh prefabs"}),
        "unreal": ("console", {"Unreal Landscape", "Static Mesh Foliage"}),
    }
    for engine, (profile, systems) in required.items():
        recipe = by_engine.get(engine)
        if not isinstance(recipe, dict):
            verifier.fail(f"{prefix}.engine_import_recipe.{engine}", "engine import recipe is missing")
            continue
        verifier.require_equal(
            f"{prefix}.engine_import_recipe.{engine}.profile",
            recipe.get("profile"),
            profile,
        )
        verifier.require_equal(
            f"{prefix}.engine_import_recipe.{engine}.runtime_policy",
            recipe.get("runtime_policy"),
            "engine_native_static",
        )
        file_name = str(recipe.get("file", ""))
        verifier.require(
            f"{prefix}.engine_import_recipe.{engine}.file",
            file_name.startswith("engines/") and file_name.endswith(".recipe.json"),
            file_name,
            f"{engine} engine import recipe must live under engines/*.recipe.json",
        )
        actual_systems = set(recipe.get("expected_systems") or [])
        verifier.require(
            f"{prefix}.engine_import_recipe.{engine}.expected_systems",
            systems.issubset(actual_systems),
            f"{sorted(actual_systems)} includes {sorted(systems)}",
            f"{engine} engine import recipe must list required engine-native systems",
        )

    summaries = report.get("engine_import_recipe_summaries")
    if isinstance(summaries, list):
        verifier.require_equal(
            f"{prefix}.engine_import_recipe_summary_count",
            len(summaries),
            len(recipes),
        )
        summary_by_engine = {
            item.get("engine"): item
            for item in summaries
            if isinstance(item, dict)
        }
        for engine in required:
            summary = summary_by_engine.get(engine)
            if not isinstance(summary, dict):
                verifier.fail(f"{prefix}.engine_import_recipe_summary.{engine}", "summary is missing")
                continue
            normal_key = "unreal_yminus_file" if engine == "unreal" else "unity_yplus_file"
            verifier.require_equal(
                f"{prefix}.engine_import_recipe_summary.{engine}.source_file_count",
                summary.get("source_file_count"),
                len(expected_source_files(manifest, normal_key)),
            )
            verifier.require_equal(
                f"{prefix}.engine_import_recipe_summary.{engine}.scatter_instances",
                summary.get("scatter_instances"),
                222,
            )
            verifier.require_equal(
                f"{prefix}.engine_import_recipe_summary.{engine}.scatter_binary_chunks",
                summary.get("scatter_binary_chunks"),
                26,
            )
            verifier.require(
                f"{prefix}.engine_import_recipe_summary.{engine}.checksum",
                int_or_default(summary.get("file_checksum"), 0) != 0,
                f"{summary.get('file_checksum')} is nonzero",
                f"{engine} engine import recipe checksum must be nonzero",
            )


def check_engine_import_recipe_report(
    verifier: EvidenceVerifier,
    prefix: str,
    report: dict[str, Any],
    manifest: dict[str, Any],
    camel_case: bool = False,
) -> None:
    recipes = manifest.get("engine_import_recipes")
    if not isinstance(recipes, list):
        verifier.fail(f"{prefix}.engine_import_recipe_report", "manifest is missing engine_import_recipes")
        return

    count_field = "engineImportRecipeCount" if camel_case else "engine_import_recipe_count"
    files_field = "engineImportRecipeFiles" if camel_case else "engine_import_recipe_files"
    profiles_field = "engineImportRecipeProfiles" if camel_case else "engine_import_recipe_profiles"
    policies_field = (
        "engineImportRecipeRuntimePolicies"
        if camel_case
        else "engine_import_recipe_runtime_policies"
    )
    systems_field = (
        "engineImportRecipeExpectedSystems"
        if camel_case
        else "engine_import_recipe_expected_systems"
    )
    unity_file_field = "unityEngineImportRecipeFile" if camel_case else "unity_engine_import_recipe_file"
    unreal_file_field = "unrealEngineImportRecipeFile" if camel_case else "unreal_engine_import_recipe_file"

    verifier.require_equal(
        f"{prefix}.engine_import_recipe_count",
        report.get(count_field),
        len(recipes),
    )
    verifier.require_equal(
        f"{prefix}.engine_import_recipe_files",
        report.get(files_field),
        expected_engine_import_recipe_files(manifest),
    )
    verifier.require_equal(
        f"{prefix}.engine_import_recipe_profiles",
        report.get(profiles_field),
        expected_engine_import_recipe_profiles(manifest),
    )
    verifier.require_equal(
        f"{prefix}.engine_import_recipe_runtime_policies",
        report.get(policies_field),
        [item.get("runtime_policy") for item in recipes],
    )
    verifier.require_equal(
        f"{prefix}.engine_import_recipe_expected_systems",
        report.get(systems_field),
        expected_engine_import_recipe_systems(manifest),
    )
    verifier.require_equal(
        f"{prefix}.unity_engine_import_recipe_file",
        report.get(unity_file_field),
        engine_import_recipe_file_for_engine(manifest, "unity"),
    )
    verifier.require_equal(
        f"{prefix}.unreal_engine_import_recipe_file",
        report.get(unreal_file_field),
        engine_import_recipe_file_for_engine(manifest, "unreal"),
    )


def check_surface_overlay_report(
    verifier: EvidenceVerifier,
    prefix: str,
    report: dict[str, Any],
    manifest: dict[str, Any],
    camel_case: bool = False,
) -> None:
    overlays = manifest.get("surface_overlays")
    if not isinstance(overlays, list):
        verifier.fail(f"{prefix}.surface_overlay_report", "manifest is missing surface_overlays")
        return

    names = [overlay.get("name") for overlay in overlays if isinstance(overlay, dict)]
    sources = [overlay.get("source_file") for overlay in overlays if isinstance(overlay, dict)]
    channels = [overlay.get("channel") for overlay in overlays if isinstance(overlay, dict)]
    runtime_policies = [
        overlay.get("runtime_policy")
        for overlay in overlays
        if isinstance(overlay, dict)
    ]
    count_field = "surfaceOverlayCount" if camel_case else "surface_overlay_count"
    names_field = "surfaceOverlayNames" if camel_case else "surface_overlay_names"
    sources_field = "surfaceOverlaySourceFiles" if camel_case else "surface_overlay_source_files"
    channels_field = "surfaceOverlayChannels" if camel_case else "surface_overlay_channels"
    policies_field = (
        "surfaceOverlayRuntimePolicies" if camel_case else "surface_overlay_runtime_policies"
    )
    targets_field = "surfaceOverlayTargets" if camel_case else "surface_overlay_targets"

    verifier.require_equal(f"{prefix}.surface_overlay_count", report.get(count_field), len(overlays))
    verifier.require_equal(f"{prefix}.surface_overlay_names", report.get(names_field), names)
    verifier.require_equal(f"{prefix}.surface_overlay_source_files", report.get(sources_field), sources)
    verifier.require_equal(f"{prefix}.surface_overlay_channels", report.get(channels_field), channels)
    verifier.require_equal(
        f"{prefix}.surface_overlay_runtime_policies",
        report.get(policies_field),
        runtime_policies,
    )
    reported_targets = report.get(targets_field)
    if camel_case and isinstance(reported_targets, list):
        reported_targets = [
            str(targets).split(",") if isinstance(targets, str) else targets
            for targets in reported_targets
        ]
    moss_targets = next(
        (
            overlay.get("targets", [])
            for overlay in overlays
            if isinstance(overlay, dict) and overlay.get("name") == "moss"
        ),
        [],
    )
    verifier.require(
        f"{prefix}.surface_overlay_moss_targets",
        isinstance(reported_targets, list) and moss_targets in reported_targets,
        f"{moss_targets} reported",
        "import report must include moss surface overlay targets",
    )


def required_surface_targets_for_kind(kind: str) -> set[str]:
    if kind in {"grass", "flower", "weed", "litter"}:
        return {"groundcover_foliage"}
    if kind == "moss":
        return {"groundcover_foliage", "moss_tuft"}
    if kind == "shrub":
        return {"groundcover_foliage", "shrub_base"}
    if kind == "rock":
        return {"static_surface", "rock"}
    if kind == "log":
        return {"static_surface", "log"}
    return set()


def expected_prototype_surface_target_entries(manifest: dict[str, Any]) -> list[str]:
    return [
        f"{prototype.get('name', '')}:{','.join(prototype.get('surface_targets') or [])}"
        for prototype in manifest.get("prototypes", [])
        if isinstance(prototype, dict)
    ]


def check_prototype_surface_targets(
    verifier: EvidenceVerifier,
    prefix: str,
    manifest: dict[str, Any],
) -> None:
    prototypes = manifest.get("prototypes")
    if not isinstance(prototypes, list):
        verifier.fail(f"{prefix}.prototype_surface_targets", "manifest is missing prototypes")
        return

    by_kind: dict[str, set[str]] = {}
    for prototype in prototypes:
        if not isinstance(prototype, dict):
            continue
        name = prototype.get("name", "")
        kind = prototype.get("kind", "")
        targets = set(prototype.get("surface_targets") or [])
        verifier.require(
            f"{prefix}.prototype_surface_targets.{name}",
            bool(targets),
            f"{sorted(targets)}",
            "prototype must declare at least one surface target",
        )
        required = required_surface_targets_for_kind(kind)
        verifier.require(
            f"{prefix}.prototype_surface_targets.{name}.required",
            required.issubset(targets),
            f"{sorted(targets)} includes {sorted(required)}",
            f"{kind} prototype must include required surface targets",
        )
        by_kind.setdefault(kind, set()).update(targets)

    expected_kind_targets = {
        "rock": {"static_surface", "rock"},
        "log": {"static_surface", "log"},
        "shrub": {"groundcover_foliage", "shrub_base"},
    }
    for kind, required in expected_kind_targets.items():
        actual = by_kind.get(kind, set())
        verifier.require(
            f"{prefix}.prototype_surface_targets.{kind}",
            required.issubset(actual),
            f"{sorted(actual)} includes {sorted(required)}",
            f"{kind} prototypes must expose overlay-compatible surface targets",
        )


def check_prototype_surface_target_report(
    verifier: EvidenceVerifier,
    prefix: str,
    report: dict[str, Any],
    manifest: dict[str, Any],
    camel_case: bool = False,
) -> None:
    if camel_case:
        all_field = "prototypeSurfaceTargets"
        rock_field = "rockPrototypeSurfaceTargets"
        log_field = "logPrototypeSurfaceTargets"
        shrub_field = "shrubPrototypeSurfaceTargets"
        expected_all: Any = expected_prototype_surface_target_entries(manifest)
    else:
        all_field = "prototype_surface_targets"
        rock_field = "rock_prototype_surface_targets"
        log_field = "log_prototype_surface_targets"
        shrub_field = "shrub_prototype_surface_targets"
        expected_all = [
            {
                "name": prototype.get("name", ""),
                "kind": prototype.get("kind", ""),
                "targets": prototype.get("surface_targets") or [],
            }
            for prototype in manifest.get("prototypes", [])
            if isinstance(prototype, dict)
        ]

    verifier.require_equal(
        f"{prefix}.prototype_surface_targets",
        report.get(all_field),
        expected_all,
    )
    kind_targets = [
        ("rock", rock_field, {"static_surface", "rock"}),
        ("log", log_field, {"static_surface", "log"}),
        ("shrub", shrub_field, {"groundcover_foliage", "shrub_base"}),
    ]
    for kind, field, required in kind_targets:
        actual = set(report.get(field) or [])
        verifier.require(
            f"{prefix}.{kind}_prototype_surface_targets",
            required.issubset(actual),
            f"{sorted(actual)} includes {sorted(required)}",
            f"{prefix} report must include {kind} prototype surface targets",
        )


def check_midori_memory_footprint(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
) -> None:
    footprint = manifest.get("memory_footprint")
    summary = report.get("memory_footprint")
    if not isinstance(footprint, dict):
        verifier.fail("midori.memory_footprint", "manifest is missing memory_footprint")
        return
    if not isinstance(summary, dict):
        verifier.fail("midori.memory_footprint_summary", "report is missing memory_footprint")
        return

    for field in [
        "map_pixel_count",
        "decoded_map_bytes",
        "encoded_map_bytes",
        "material_recipe_bytes",
        "engine_import_recipe_bytes",
        "preview_mesh_bytes",
        "prototype_mesh_bytes",
        "scatter_json_bytes",
        "scatter_binary_bytes",
        "scatter_binary_header_bytes",
        "scatter_binary_record_bytes",
        "total_payload_bytes",
    ]:
        verifier.require_equal(
            f"midori.memory_footprint.{field}",
            summary.get(field),
            footprint.get(field),
        )

    map_resolution = int_or_default(manifest.get("map_resolution"), 0)
    map_pixel_count = int_or_default(footprint.get("map_pixel_count"), 0)
    decoded_map_bytes = int_or_default(footprint.get("decoded_map_bytes"), 0)
    scatter_binary_files = manifest.get("scatter", {}).get("binary_files", [])
    scatter_chunk_count = len(scatter_binary_files) if isinstance(scatter_binary_files, list) else 0
    scatter_instance_count = sum(
        int_or_default(item.get("instance_count"), 0)
        for item in scatter_binary_files
        if isinstance(item, dict)
    )
    expected_total = (
        int_or_default(footprint.get("encoded_map_bytes"), 0)
        + int_or_default(footprint.get("material_recipe_bytes"), 0)
        + int_or_default(footprint.get("engine_import_recipe_bytes"), 0)
        + int_or_default(footprint.get("preview_mesh_bytes"), 0)
        + int_or_default(footprint.get("prototype_mesh_bytes"), 0)
        + int_or_default(footprint.get("scatter_json_bytes"), 0)
        + int_or_default(footprint.get("scatter_binary_bytes"), 0)
    )
    verifier.require_equal("midori.memory_footprint.map_pixels", map_pixel_count, map_resolution * map_resolution)
    verifier.require_equal("midori.memory_footprint.decoded_maps", decoded_map_bytes, map_pixel_count * 13)
    verifier.require_equal(
        "midori.memory_footprint.scatter_binary_header_bytes",
        footprint.get("scatter_binary_header_bytes"),
        scatter_chunk_count * 16,
    )
    verifier.require_equal(
        "midori.memory_footprint.scatter_binary_record_bytes",
        footprint.get("scatter_binary_record_bytes"),
        scatter_instance_count * 32,
    )
    verifier.require_equal(
        "midori.memory_footprint.scatter_binary_total",
        int_or_default(footprint.get("scatter_binary_bytes"), 0),
        int_or_default(footprint.get("scatter_binary_header_bytes"), 0)
        + int_or_default(footprint.get("scatter_binary_record_bytes"), 0),
    )
    verifier.require_equal(
        "midori.memory_footprint.total_payload",
        int_or_default(footprint.get("total_payload_bytes"), 0),
        expected_total,
    )
    for field in [
        "encoded_map_bytes",
        "material_recipe_bytes",
        "engine_import_recipe_bytes",
        "preview_mesh_bytes",
        "prototype_mesh_bytes",
        "scatter_json_bytes",
        "scatter_binary_bytes",
        "total_payload_bytes",
    ]:
        verifier.require(
            f"midori.memory_footprint.{field}_nonzero",
            int_or_default(footprint.get(field), 0) > 0,
            f"{footprint.get(field)} bytes",
            "default validation package should record nonzero payload bytes",
        )


def check_midori_map_summaries(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
) -> None:
    summaries = report.get("map_summaries")
    if not isinstance(summaries, list):
        verifier.fail("midori.map_summaries", "validation report is missing map_summaries")
        return

    verifier.require_equal("midori.map_summary_count", len(summaries), len(EXPECTED_MAP_SUMMARIES))
    by_name = {
        Path(str(summary.get("file", ""))).name: summary
        for summary in summaries
        if isinstance(summary, dict)
    }
    map_resolution = manifest.get("map_resolution")

    for file_name, expected in EXPECTED_MAP_SUMMARIES.items():
        summary = by_name.get(file_name)
        if summary is None:
            verifier.fail(f"midori.map.{file_name}", "map summary is missing")
            continue

        verifier.require_equal(
            f"midori.map.{file_name}.width",
            summary.get("width"),
            map_resolution,
        )
        verifier.require_equal(
            f"midori.map.{file_name}.height",
            summary.get("height"),
            map_resolution,
        )
        verifier.require_equal(
            f"midori.map.{file_name}.color_type",
            summary.get("color_type"),
            expected["color_type"],
        )
        verifier.require_equal(
            f"midori.map.{file_name}.channels",
            summary.get("channels"),
            expected["channels"],
        )
        try:
            checksum = int(summary.get("file_checksum", 0))
        except (TypeError, ValueError):
            checksum = 0
        verifier.require(
            f"midori.map.{file_name}.checksum",
            checksum > 0,
            f"{checksum} is a nonzero encoded PNG checksum",
            "encoded PNG checksum must be present and nonzero",
        )

        channel_min = summary.get("channel_min", [])
        channel_max = summary.get("channel_max", [])
        if not isinstance(channel_min, list) or not isinstance(channel_max, list):
            verifier.fail(f"midori.map.{file_name}.range", "channel ranges are missing")
            continue

        for channel in expected.get("varying_channels", []):
            if channel >= len(channel_min) or channel >= len(channel_max):
                verifier.fail(f"midori.map.{file_name}.channel_{channel}", "channel range is missing")
                continue
            verifier.require(
                f"midori.map.{file_name}.channel_{channel}_varies",
                int(channel_max[channel]) > int(channel_min[channel]),
                f"{channel_min[channel]}..{channel_max[channel]}",
                f"channel {channel} must have nontrivial range",
            )

        any_channels = expected.get("varying_any_channels", [])
        if any_channels:
            varies = False
            ranges: list[str] = []
            for channel in any_channels:
                if channel < len(channel_min) and channel < len(channel_max):
                    ranges.append(f"{channel}:{channel_min[channel]}..{channel_max[channel]}")
                    varies = varies or int(channel_max[channel]) > int(channel_min[channel])
            verifier.require(
                f"midori.map.{file_name}.normal_variation",
                varies,
                ", ".join(ranges),
                "normal map must have nontrivial channel variation",
            )


def check_midori_map_relationships(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
) -> None:
    relationships = report.get("map_relationships")
    if not isinstance(relationships, dict):
        verifier.fail("midori.map_relationships", "validation report is missing map_relationships")
        return

    expected_pixels = int_or_default(manifest.get("map_resolution"), 0) ** 2
    verifier.require_equal(
        "midori.map_relationships.normal_pair_pixels",
        relationships.get("normal_pair_pixels"),
        expected_pixels,
    )
    verifier.require_equal(
        "midori.map_relationships.normal_red_blue_mismatches",
        relationships.get("normal_red_blue_mismatches"),
        0,
    )
    verifier.require_equal(
        "midori.map_relationships.normal_green_flip_mismatches",
        relationships.get("normal_green_flip_mismatches"),
        0,
    )
    verifier.require(
        "midori.map_relationships.normal_green_flip_max_error",
        int_or_default(relationships.get("normal_green_flip_max_error"), 999) <= 1,
        f"{relationships.get('normal_green_flip_max_error')} <= 1",
        "normal green-channel inversion must be exact within byte rounding tolerance",
    )
    verifier.require_equal(
        "midori.map_relationships.grass_density_pixels",
        relationships.get("grass_density_pixels"),
        expected_pixels,
    )
    verifier.require_equal(
        "midori.map_relationships.grass_density_mask_r_mismatches",
        relationships.get("grass_density_mask_r_mismatches"),
        0,
    )


def check_midori_prototype_summaries(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
) -> None:
    summaries = report.get("prototype_summaries")
    if not isinstance(summaries, list):
        verifier.fail("midori.prototype_summaries", "validation report is missing prototype_summaries")
        return

    expected_lods: list[tuple[str, dict[str, Any]]] = []
    for prototype in manifest.get("prototypes", []):
        for lod in prototype.get("lods", []):
            if isinstance(lod, dict):
                expected_lods.append((Path(str(lod.get("file", ""))).name, lod))

    verifier.require_equal("midori.prototype_summary_count", len(summaries), len(expected_lods))
    by_name = {
        Path(str(summary.get("file", ""))).name: summary
        for summary in summaries
        if isinstance(summary, dict)
    }
    material_slot_limit = min(
        int(manifest.get("mobile", {}).get("material_slots", 0)),
        int(manifest.get("console", {}).get("material_slots", 0)),
    )

    required_attributes = [
        "has_positions",
        "has_normals",
        "has_tangents",
        "normals_are_valid",
        "tangents_are_valid",
        "has_texcoord0",
        "has_texcoord1",
        "has_color0",
    ]
    for file_name, lod in expected_lods:
        summary = by_name.get(file_name)
        if summary is None:
            verifier.fail(f"midori.prototype.{file_name}", "prototype summary is missing")
            continue

        verifier.require_equal(
            f"midori.prototype.{file_name}.lod_index",
            summary.get("lod_index"),
            lod.get("index"),
        )
        verifier.require_equal(
            f"midori.prototype.{file_name}.vertex_count",
            summary.get("vertex_count"),
            lod.get("vertex_count"),
        )
        verifier.require_equal(
            f"midori.prototype.{file_name}.triangle_count",
            summary.get("triangle_count"),
            lod.get("triangle_count"),
        )
        verifier.require(
            f"midori.prototype.{file_name}.mesh_count",
            int(summary.get("mesh_count", 0)) >= 1,
            f"{summary.get('mesh_count')} meshes",
            "prototype GLB must contain at least one mesh",
        )
        verifier.require(
            f"midori.prototype.{file_name}.primitive_count",
            int(summary.get("primitive_count", 0)) >= 1,
            f"{summary.get('primitive_count')} primitives",
            "prototype GLB must contain at least one primitive",
        )
        for attribute in required_attributes:
            verifier.require_equal(
                f"midori.prototype.{file_name}.{attribute}",
                summary.get(attribute),
                True,
            )
        verifier.require(
            f"midori.prototype.{file_name}.used_materials",
            0 < int(summary.get("used_material_count", 0)) <= material_slot_limit,
            f"{summary.get('used_material_count')} used materials within limit {material_slot_limit}",
            "prototype GLB must use at least one material and stay within the profile material-slot limit",
        )
        try:
            checksum = int(summary.get("file_checksum", 0))
        except (TypeError, ValueError):
            checksum = 0
        verifier.require(
            f"midori.prototype.{file_name}.checksum",
            checksum > 0,
            f"{checksum} is a nonzero encoded GLB checksum",
            "encoded GLB checksum must be present and nonzero",
        )


def check_midori_profile_budgets(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
) -> None:
    summaries = report.get("profile_budget_summaries")
    prototype_summaries = report.get("prototype_summaries")
    if not isinstance(summaries, list):
        verifier.fail("midori.profile_budget_summaries", "validation report is missing profile_budget_summaries")
        return
    if not isinstance(prototype_summaries, list):
        verifier.fail("midori.profile_budget_inputs", "validation report is missing prototype_summaries")
        return

    expected_lod_count = sum(
        1
        for prototype in manifest.get("prototypes", [])
        if isinstance(prototype, dict)
        for lod in prototype.get("lods", [])
        if isinstance(lod, dict)
    )
    max_lod0 = 0
    max_lod1 = 0
    max_lod2 = 0
    max_used_materials = 0
    scatter_instance_count = int_or_default(report.get("scatter_binary_instances"), 0)
    scatter_chunks = report.get("scatter_binary_summaries")
    if not isinstance(scatter_chunks, list) or not scatter_chunks:
        scatter_chunks = report.get("scatter_json_chunks")
    max_chunk_instances = 0
    if isinstance(scatter_chunks, list):
        for chunk in scatter_chunks:
            if isinstance(chunk, dict):
                max_chunk_instances = max(
                    max_chunk_instances,
                    int_or_default(chunk.get("instance_count"), 0),
                )
    for summary in prototype_summaries:
        if not isinstance(summary, dict):
            continue
        triangle_count = int_or_default(summary.get("triangle_count"), 0)
        lod_index = int_or_default(summary.get("lod_index"), 2)
        if lod_index == 0:
            max_lod0 = max(max_lod0, triangle_count)
        elif lod_index == 1:
            max_lod1 = max(max_lod1, triangle_count)
        else:
            max_lod2 = max(max_lod2, triangle_count)
        max_used_materials = max(max_used_materials, int_or_default(summary.get("used_material_count"), 0))

    verifier.require_equal("midori.profile_budget_summary_count", len(summaries), 2)
    by_profile = {
        str(summary.get("profile")): summary
        for summary in summaries
        if isinstance(summary, dict)
    }
    for profile_name in ["mobile", "console"]:
        summary = by_profile.get(profile_name)
        if summary is None:
            verifier.fail(f"midori.profile_budget.{profile_name}", "profile budget summary is missing")
            continue

        profile = manifest.get(profile_name, {})
        verifier.require_equal(f"midori.profile_budget.{profile_name}.passed", summary.get("passed"), True)
        lod0_distance = float_or_default(profile.get("lod0_max_distance"), -1.0)
        lod1_distance = float_or_default(profile.get("lod1_max_distance"), -1.0)
        lod2_distance = float_or_default(profile.get("lod2_max_distance"), -1.0)
        cull_end = float_or_default(profile.get("cull_end"), -1.0)
        verifier.require(
            f"midori.profile_budget.{profile_name}.lod_distances_ordered",
            0.0 < lod0_distance <= lod1_distance <= lod2_distance <= cull_end,
            f"{lod0_distance} <= {lod1_distance} <= {lod2_distance} <= {cull_end}",
            "profile LOD distances must be positive, ordered, and inside the cull range",
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.grass_collision_off",
            profile.get("grass_collision"),
            False,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.moss_collision_off",
            profile.get("moss_collision"),
            False,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.prototype_lod_count",
            summary.get("prototype_lod_count"),
            expected_lod_count,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.max_lod0_triangles",
            summary.get("max_lod0_triangles"),
            max_lod0,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.max_lod1_triangles",
            summary.get("max_lod1_triangles"),
            max_lod1,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.max_lod2_triangles",
            summary.get("max_lod2_triangles"),
            max_lod2,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.lod0_budget",
            summary.get("lod0_triangle_budget"),
            profile.get("lod0_max_triangles"),
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.lod1_budget",
            summary.get("lod1_triangle_budget"),
            profile.get("lod1_max_triangles"),
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.lod2_budget",
            summary.get("lod2_triangle_budget"),
            profile.get("lod2_max_triangles"),
        )
        verifier.require(
            f"midori.profile_budget.{profile_name}.lod0_within_budget",
            max_lod0 <= int_or_default(profile.get("lod0_max_triangles"), 0),
            f"{max_lod0} <= {profile.get('lod0_max_triangles')}",
            "LOD0 prototypes must fit the profile triangle budget",
        )
        verifier.require(
            f"midori.profile_budget.{profile_name}.lod1_within_budget",
            max_lod1 <= int_or_default(profile.get("lod1_max_triangles"), 0),
            f"{max_lod1} <= {profile.get('lod1_max_triangles')}",
            "LOD1 prototypes must fit the profile triangle budget",
        )
        verifier.require(
            f"midori.profile_budget.{profile_name}.lod2_within_budget",
            max_lod2 <= int_or_default(profile.get("lod2_max_triangles"), 0),
            f"{max_lod2} <= {profile.get('lod2_max_triangles')}",
            "LOD2/far prototypes must fit the profile triangle budget",
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.max_used_material_count",
            summary.get("max_used_material_count"),
            max_used_materials,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.material_slot_budget",
            summary.get("material_slot_budget"),
            profile.get("material_slots"),
        )
        verifier.require(
            f"midori.profile_budget.{profile_name}.materials_within_budget",
            max_used_materials <= int_or_default(profile.get("material_slots"), 0),
            f"{max_used_materials} <= {profile.get('material_slots')}",
            "prototype material usage must fit the profile material-slot budget",
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.scatter_instance_count",
            summary.get("scatter_instance_count"),
            scatter_instance_count,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.max_chunk_instance_count",
            summary.get("max_chunk_instance_count"),
            max_chunk_instances,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.max_instances_per_tile_budget",
            summary.get("max_instances_per_tile_budget"),
            profile.get("max_instances_per_tile"),
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.max_instances_per_chunk_budget",
            summary.get("max_instances_per_chunk_budget"),
            profile.get("max_instances_per_chunk"),
        )
        verifier.require(
            f"midori.profile_budget.{profile_name}.tile_instances_within_budget",
            scatter_instance_count <= int_or_default(profile.get("max_instances_per_tile"), 0),
            f"{scatter_instance_count} <= {profile.get('max_instances_per_tile')}",
            "authored scatter instances must fit the profile per-tile budget",
        )
        verifier.require(
            f"midori.profile_budget.{profile_name}.chunk_instances_within_budget",
            max_chunk_instances <= int_or_default(profile.get("max_instances_per_chunk"), 0),
            f"{max_chunk_instances} <= {profile.get('max_instances_per_chunk')}",
            "authored scatter instances must fit the profile per-chunk budget",
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.triangle_violations",
            summary.get("triangle_budget_violation_count"),
            0,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.material_violations",
            summary.get("material_slot_violation_count"),
            0,
        )
        verifier.require_equal(
            f"midori.profile_budget.{profile_name}.instance_violations",
            summary.get("instance_budget_violation_count"),
            0,
        )


def check_midori_scatter_summaries(verifier: EvidenceVerifier, report: dict[str, Any]) -> None:
    json_summaries = report.get("scatter_json_chunks")
    binary_summaries = report.get("scatter_binary_summaries")
    parity = report.get("scatter_parity")

    if not isinstance(json_summaries, list):
        verifier.fail("midori.scatter_json_chunks", "validation report is missing scatter_json_chunks")
        json_summaries = []
    if not isinstance(binary_summaries, list):
        verifier.fail(
            "midori.scatter_binary_summaries",
            "validation report is missing scatter_binary_summaries",
        )
        binary_summaries = []
    if not isinstance(parity, dict):
        verifier.fail("midori.scatter_parity", "validation report is missing scatter_parity")
        parity = {}

    verifier.require_equal("midori.scatter_json_summary_count", len(json_summaries), 26)
    verifier.require_equal("midori.scatter_binary_summary_count", len(binary_summaries), 26)
    verifier.require_equal("midori.scatter_parity.json_chunk_count", parity.get("json_chunk_count"), 26)
    verifier.require_equal("midori.scatter_parity.binary_chunk_count", parity.get("binary_chunk_count"), 26)
    verifier.require_equal("midori.scatter_parity.matching_chunk_count", parity.get("matching_chunk_count"), 26)
    for field in [
        "missing_binary_chunk_count",
        "extra_binary_chunk_count",
        "instance_count_mismatch_count",
        "bounds_mismatch_count",
        "record_checksum_mismatch_count",
    ]:
        verifier.require_equal(f"midori.scatter_parity.{field}", parity.get(field), 0)

    for source, summaries in [("json", json_summaries), ("binary", binary_summaries)]:
        for index, summary in enumerate(summaries):
            if not isinstance(summary, dict):
                verifier.fail(f"midori.scatter_{source}_summary.{index}", "summary must be an object")
                continue

            label = f"midori.scatter_{source}_summary.{index}"
            verifier.require_equal(f"{label}.source", summary.get("source"), source)
            verifier.require(
                f"{label}.instance_count",
                int_or_default(summary.get("instance_count"), 0) > 0,
                f"{summary.get('instance_count')} instances",
                f"{source} scatter chunks must contain at least one instance",
            )
            for checksum_field in ["file_checksum", "record_checksum"]:
                verifier.require(
                    f"{label}.{checksum_field}",
                    int_or_default(summary.get(checksum_field), 0) > 0,
                    f"{summary.get(checksum_field)} is nonzero",
                    f"{checksum_field} must be present and nonzero",
                )
            verifier.require(
                f"{label}.yaw_range",
                0.0 <= float_or_default(summary.get("yaw_min"), -1.0)
                <= float_or_default(summary.get("yaw_max"), -1.0)
                <= math.tau,
                f"{summary.get('yaw_min')}..{summary.get('yaw_max')}",
                "yaw range must be ordered and inside 0..tau",
            )
            verifier.require(
                f"{label}.phase_range",
                0.0 <= float_or_default(summary.get("phase_min"), -1.0)
                <= float_or_default(summary.get("phase_max"), -1.0)
                <= math.tau,
                f"{summary.get('phase_min')}..{summary.get('phase_max')}",
                "phase range must be ordered and inside 0..tau",
            )
            verifier.require(
                f"{label}.height_range",
                0.0 < float_or_default(summary.get("height_min"), 0.0)
                <= float_or_default(summary.get("height_max"), 0.0),
                f"{summary.get('height_min')}..{summary.get('height_max')}",
                "height multiplier range must be positive and ordered",
            )
            verifier.require(
                f"{label}.width_range",
                0.0 < float_or_default(summary.get("width_min"), 0.0)
                <= float_or_default(summary.get("width_max"), 0.0),
                f"{summary.get('width_min')}..{summary.get('width_max')}",
                "width multiplier range must be positive and ordered",
            )
            verifier.require(
                f"{label}.color_variation_range",
                0.0 <= float_or_default(summary.get("color_variation_min"), -1.0)
                <= float_or_default(summary.get("color_variation_max"), -1.0)
                <= 1.0,
                f"{summary.get('color_variation_min')}..{summary.get('color_variation_max')}",
                "color variation range must be ordered and inside 0..1",
            )


def resolve_project_scaffold_root(validation_root: Path, summary: dict[str, Any]) -> Path | None:
    candidates = [
        validation_root / "projects",
        validation_root.parent / "projects",
    ]
    project_scaffolds = summary.get("project_scaffolds")
    if isinstance(project_scaffolds, dict):
        root = project_scaffolds.get("root")
        if isinstance(root, str) and root:
            candidates.append(Path(root))

    for candidate in candidates:
        if (candidate / "project_scaffold_summary.json").is_file():
            return candidate
    return None


def check_project_scaffolds(
    verifier: EvidenceVerifier,
    summary: dict[str, Any],
    validation_root: Path,
) -> None:
    project_scaffolds = summary.get("project_scaffolds")
    if not isinstance(project_scaffolds, dict):
        verifier.fail("summary.project_scaffolds", "engine summary is missing project_scaffolds")
        return

    verifier.require_equal("summary.project_scaffolds_status", project_scaffolds.get("status"), "generated")
    verifier.require_equal(
        "summary.project_scaffolds_preflight_status",
        project_scaffolds.get("preflight_status"),
        "passed",
    )
    verifier.require(
        "summary.project_scaffolds_unity_project",
        bool(project_scaffolds.get("unity_project")),
        f"{project_scaffolds.get('unity_project')}",
        "engine summary must record a Unity scaffold project path",
    )
    verifier.require(
        "summary.project_scaffolds_unreal_project",
        bool(project_scaffolds.get("unreal_project")),
        f"{project_scaffolds.get('unreal_project')}",
        "engine summary must record an Unreal scaffold project path",
    )

    scaffold_root = resolve_project_scaffold_root(validation_root, summary)
    if scaffold_root is None:
        verifier.fail(
            "summary.project_scaffolds_root",
            f"could not locate project_scaffold_summary.json near {validation_root}",
        )
        return

    verifier.pass_("summary.project_scaffolds_root", f"{scaffold_root} loaded")
    scaffold_summary_path = scaffold_root / "project_scaffold_summary.json"
    scaffold_summary = load_json(scaffold_summary_path)
    if not isinstance(scaffold_summary, dict):
        verifier.fail("summary.project_scaffold_summary", f"{scaffold_summary_path} is missing or invalid")
        return
    verifier.pass_("summary.project_scaffold_summary", f"{scaffold_summary_path} loaded")

    unity_project = scaffold_root / "unity" / "MidoriUnityValidation"
    unity_importer = unity_project / "Assets" / "Editor" / "MidoriNaturePackageImporter.cs"
    unity_project_settings = unity_project / "ProjectSettings"
    unity_manifest_path = unity_project / "Packages" / "manifest.json"
    unity_readme = unity_project / "Assets" / "Midori" / "Validation" / "README.md"
    unreal_project = scaffold_root / "unreal" / "MidoriUnrealValidation"
    unreal_project_file = unreal_project / "MidoriUnrealValidation.uproject"
    unreal_config = unreal_project / "Config" / "DefaultEngine.ini"
    unreal_readme = unreal_project / "Content" / "Midori" / "Validation" / "README.md"

    check_artifact(verifier, "summary.project_scaffolds.unity_importer", unity_importer)
    verifier.require(
        "summary.project_scaffolds.unity_project_settings",
        unity_project_settings.is_dir(),
        f"{unity_project_settings} exists",
        "Unity scaffold must include a ProjectSettings directory",
    )
    check_artifact(verifier, "summary.project_scaffolds.unity_readme", unity_readme)

    unity_manifest = load_json(unity_manifest_path)
    if not isinstance(unity_manifest, dict):
        verifier.fail(
            "summary.project_scaffolds.unity_manifest",
            f"{unity_manifest_path} is missing or invalid",
        )
    else:
        verifier.pass_("summary.project_scaffolds.unity_manifest", f"{unity_manifest_path} loaded")
        dependencies = unity_manifest.get("dependencies")
        if not isinstance(dependencies, dict):
            verifier.fail(
                "summary.project_scaffolds.unity_manifest_dependencies",
                "Unity scaffold manifest is missing dependencies",
            )
        else:
            verifier.require_equal(
                "summary.project_scaffolds.unity_gltfast",
                dependencies.get("com.unity.cloud.gltfast"),
                "5.2.0",
            )
            for module_name in [
                "com.unity.modules.imgui",
                "com.unity.modules.jsonserialize",
                "com.unity.modules.terrain",
                "com.unity.modules.uielements",
            ]:
                verifier.require_equal(
                    f"summary.project_scaffolds.{module_name}",
                    dependencies.get(module_name),
                    "1.0.0",
                )

    unreal_project_json = load_json(unreal_project_file)
    if not isinstance(unreal_project_json, dict):
        verifier.fail(
            "summary.project_scaffolds.unreal_project",
            f"{unreal_project_file} is missing or invalid",
        )
    else:
        verifier.pass_("summary.project_scaffolds.unreal_project", f"{unreal_project_file} loaded")
        plugins = unreal_project_json.get("Plugins")
        if not isinstance(plugins, list):
            verifier.fail(
                "summary.project_scaffolds.unreal_plugins",
                "Unreal scaffold .uproject is missing Plugins",
            )
        else:
            by_name = {
                plugin.get("Name"): plugin
                for plugin in plugins
                if isinstance(plugin, dict)
            }
            for plugin_name in ["PythonScriptPlugin", "EditorScriptingUtilities"]:
                plugin = by_name.get(plugin_name)
                verifier.require(
                    f"summary.project_scaffolds.unreal_plugin.{plugin_name}",
                    isinstance(plugin, dict) and plugin.get("Enabled") is True,
                    f"{plugin_name} enabled",
                    f"Unreal scaffold must enable {plugin_name}",
                )

    if file_nonempty(unreal_config):
        text = unreal_config.read_text(encoding="utf-8-sig")
        verifier.require(
            "summary.project_scaffolds.unreal_python_config",
            "PythonScriptPluginSettings" in text and "bDeveloperMode=True" in text,
            f"{unreal_config} enables Python scripting",
            "Unreal scaffold must enable Python editor scripting settings",
        )
    else:
        verifier.fail("summary.project_scaffolds.unreal_python_config", f"{unreal_config} is missing")
    check_artifact(verifier, "summary.project_scaffolds.unreal_readme", unreal_readme)

    verifier.require_equal(
        "summary.project_scaffold_summary.unity_gltfast_version",
        scaffold_summary.get("unity", {}).get("gltfast_version"),
        "5.2.0",
    )
    verifier.require_equal(
        "summary.project_scaffold_summary.unreal_destination_root",
        scaffold_summary.get("unreal", {}).get("destination_root"),
        "/Game/Midori/Imported",
    )


def check_summary(
    verifier: EvidenceVerifier,
    summary: dict[str, Any],
    validation_root: Path,
    manifest: dict[str, Any] | None = None,
) -> None:
    check_project_scaffolds(verifier, summary, validation_root)
    verifier.require_equal("summary.midori_status", summary.get("midori", {}).get("status"), "passed")
    verifier.require_equal("summary.midori_map_summaries", summary.get("midori", {}).get("map_summaries"), 5)
    verifier.require_equal(
        "summary.midori_map_relationships_status",
        summary.get("midori", {}).get("map_relationships_status"),
        "passed",
    )
    verifier.require_equal(
        "summary.midori_prototype_summaries",
        summary.get("midori", {}).get("prototype_summaries"),
        22,
    )
    verifier.require_equal(
        "summary.midori_profile_budget_summaries",
        summary.get("midori", {}).get("profile_budget_summaries"),
        2,
    )
    verifier.require_equal(
        "summary.midori_profile_budget_status",
        summary.get("midori", {}).get("profile_budget_status"),
        "passed",
    )
    verifier.require_equal(
        "summary.midori_profile_budget_failure_count",
        summary.get("midori", {}).get("profile_budget_failure_count"),
        0,
    )
    verifier.require_equal("summary.midori_scatter_json_chunks", summary.get("midori", {}).get("scatter_json_chunks"), 26)
    verifier.require_equal(
        "summary.midori_scatter_binary_summaries",
        summary.get("midori", {}).get("scatter_binary_summaries"),
        26,
    )
    verifier.require_equal(
        "summary.midori_scatter_parity_status",
        summary.get("midori", {}).get("scatter_parity_status"),
        "passed",
    )
    verifier.require_equal(
        "summary.midori_scatter_record_checksum_mismatches",
        summary.get("midori", {}).get("scatter_record_checksum_mismatches"),
        0,
    )
    verifier.require_equal(
        "summary.unity_preflight_status",
        summary.get("unity_preflight", {}).get("status"),
        "passed",
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_status",
        summary.get("unity_preflight", {}).get("compile_stub_status"),
        "passed",
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_execution_status",
        summary.get("unity_preflight", {}).get("compile_stub_execution_status"),
        "passed",
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_source_file_count",
        summary.get("unity_preflight", {}).get("compile_stub_source_file_count"),
        59,
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_material_recipe_count",
        summary.get("unity_preflight", {}).get("compile_stub_material_recipe_count"),
        2,
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_engine_import_recipe_count",
        summary.get("unity_preflight", {}).get("compile_stub_engine_import_recipe_count"),
        2,
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_scatter_binary_chunks",
        summary.get("unity_preflight", {}).get("compile_stub_scatter_binary_chunks"),
        26,
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_scatter_binary_instances",
        summary.get("unity_preflight", {}).get("compile_stub_scatter_binary_instances"),
        222,
    )
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_records_validated",
        summary.get("unity_preflight", {}).get("compile_stub_scatter_binary_records_validated"),
        True,
    )
    verifier.require(
        "summary.unity_preflight_compile_stub_manifest_checksum",
        is_nonzero_hex_checksum(
            summary.get("unity_preflight", {}).get("compile_stub_manifest_file_checksum")
        ),
        f"{summary.get('unity_preflight', {}).get('compile_stub_manifest_file_checksum')} is nonzero",
        "Unity compile-stub manifest checksum must be present and nonzero",
    )
    verifier.require(
        "summary.unity_preflight_compile_stub_source_checksum",
        is_nonzero_hex_checksum(
            summary.get("unity_preflight", {}).get("compile_stub_source_file_checksum_xor")
        ),
        f"{summary.get('unity_preflight', {}).get('compile_stub_source_file_checksum_xor')} is nonzero",
        "Unity compile-stub source checksum must be present and nonzero",
    )
    compile_stub_report_value = summary.get("unity_preflight", {}).get(
        "compile_stub_execution_report"
    )
    if compile_stub_report_value:
        compile_stub_report_path = Path(compile_stub_report_value)
        if not compile_stub_report_path.is_absolute():
            compile_stub_report_path = validation_root / compile_stub_report_path
    else:
        compile_stub_report_path = validation_root / "forest_floor_unity_compile_stub_report.json"
    compile_stub_report = load_json(compile_stub_report_path)
    if not isinstance(compile_stub_report, dict):
        verifier.fail(
            "summary.unity_preflight_compile_stub_report",
            f"{compile_stub_report_path} is missing or invalid",
        )
    else:
        verifier.pass_(
            "summary.unity_preflight_compile_stub_report",
            f"{compile_stub_report_path} loaded",
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_status",
            compile_stub_report.get("status"),
            "passed",
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_schema_version",
            compile_stub_report.get("schema_version"),
            3,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_asset_name",
            compile_stub_report.get("asset_name"),
            "Temperate Forest Floor",
        )
        report_package_dir = compile_stub_report.get("package_dir")
        if isinstance(report_package_dir, str):
            report_package_path = Path(report_package_dir)
            if report_package_path.is_absolute():
                resolved_report_package = report_package_path.resolve()
            else:
                package_candidates = [
                    validation_root / report_package_path,
                    validation_root.parent / report_package_path,
                ]
                resolved_report_package = next(
                    (candidate for candidate in package_candidates if candidate.exists()),
                    package_candidates[-1],
                ).resolve()
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_package_dir",
                str(resolved_report_package),
                str((validation_root / "forest_floor").resolve()),
            )
        else:
            verifier.fail(
                "summary.unity_preflight_compile_stub_report_package_dir",
                f"{report_package_dir!r} is not a package path string",
            )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_manifest_checksum",
            compile_stub_report.get("manifest_file_checksum"),
            summary.get("unity_preflight", {}).get("compile_stub_manifest_file_checksum"),
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_source_file_count",
            compile_stub_report.get("source_file_count"),
            59,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_source_checksum",
            compile_stub_report.get("source_file_checksum_xor"),
            summary.get("unity_preflight", {}).get("compile_stub_source_file_checksum_xor"),
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_material_recipe_count",
            compile_stub_report.get("material_recipe_count"),
            2,
        )
        if manifest is not None:
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_prototypes_declared",
                compile_stub_report.get("prototypesDeclared"),
                len(manifest["prototypes"]),
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_lod0_declared",
                compile_stub_report.get("lod0PrototypesDeclared"),
                len(manifest["prototypes"]),
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_lod_files_declared",
                compile_stub_report.get("lodFilesDeclared"),
                22,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_material_slots_declared",
                compile_stub_report.get("materialSlotsDeclared"),
                len(manifest["material_slots"]),
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_has_terrain_surface_slot",
                compile_stub_report.get("hasTerrainSurfaceMaterialSlot"),
                True,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_has_groundcover_slot",
                compile_stub_report.get("hasGroundcoverFoliageMaterialSlot"),
                True,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_groundcover_alpha",
                compile_stub_report.get("groundcoverMaterialAlphaMode"),
                "masked",
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_groundcover_double_sided",
                compile_stub_report.get("groundcoverMaterialDoubleSided"),
                True,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_groundcover_shadows",
                compile_stub_report.get("groundcoverMaterialShadows"),
                False,
            )
            verifier.require_close(
                "summary.unity_preflight_compile_stub_report_terrain_size_x",
                compile_stub_report.get("terrain_size_x"),
                manifest["tile_size"],
            )
            verifier.require_close(
                "summary.unity_preflight_compile_stub_report_terrain_size_y",
                compile_stub_report.get("terrain_size_y"),
                manifest["terrain"]["height_max"] - manifest["terrain"]["height_min"],
            )
            verifier.require_close(
                "summary.unity_preflight_compile_stub_report_terrain_size_z",
                compile_stub_report.get("terrain_size_z"),
                manifest["tile_size"],
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_heightmap_resolution",
                compile_stub_report.get("heightmapResolution"),
                33,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_resolution",
                compile_stub_report.get("detailResolution"),
                manifest["map_resolution"],
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_resolution_per_patch",
                compile_stub_report.get("detailResolutionPerPatch"),
                8,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_prototypes_created",
                compile_stub_report.get("detailPrototypesCreated"),
                len(manifest["prototypes"]),
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_prototypes_loaded_from_assets",
                compile_stub_report.get("detailPrototypesLoadedFromAssets"),
                0,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_prototypes_generated_from_glb",
                compile_stub_report.get("detailPrototypesGeneratedFromGlb"),
                len(manifest["prototypes"]),
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_prototype_failures",
                compile_stub_report.get("detailPrototypeFailures"),
                0,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_generated_file_count",
                compile_stub_report.get("detailPrototypeGeneratedFileCount"),
                len(manifest["prototypes"]),
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_detail_fallback_error_count",
                compile_stub_report.get("detailPrototypeFallbackErrorCount"),
                0,
            )
            verifier.require(
                "summary.unity_preflight_compile_stub_report_nonzero_detail_cells",
                int_or_default(compile_stub_report.get("nonZeroDetailCells"), 0) > 0,
                f"{compile_stub_report.get('nonZeroDetailCells')} nonzero cells",
                "Unity compile-stub terrain detail path must produce nonzero detail cells",
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_import_screenshot_exists",
                compile_stub_report.get("importScreenshotExists"),
                True,
            )
            verifier.require(
                "summary.unity_preflight_compile_stub_report_import_screenshot_bytes",
                int_or_default(compile_stub_report.get("importScreenshotBytes"), 0) > 0,
                f"{compile_stub_report.get('importScreenshotBytes')} bytes",
                "Unity compile-stub import screenshot path must write bytes",
            )
            verifier.require(
                "summary.unity_preflight_compile_stub_report_import_screenshot_checksum",
                is_nonzero_hex_checksum(compile_stub_report.get("importScreenshotChecksum")),
                f"{compile_stub_report.get('importScreenshotChecksum')} is nonzero",
                "Unity compile-stub import screenshot checksum must be nonzero",
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_import_screenshot_width",
                compile_stub_report.get("importScreenshotWidth"),
                1024,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_import_screenshot_height",
                compile_stub_report.get("importScreenshotHeight"),
                1024,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_import_screenshot_pixels",
                compile_stub_report.get("importScreenshotPixelCount"),
                1024 * 1024,
            )
            for channel in ["R", "G", "B"]:
                minimum = float_or_default(
                    compile_stub_report.get(f"importScreenshotMin{channel}"),
                    -1.0,
                )
                maximum = float_or_default(
                    compile_stub_report.get(f"importScreenshotMax{channel}"),
                    -1.0,
                )
                verifier.require(
                    f"summary.unity_preflight_compile_stub_report_import_screenshot_{channel.lower()}_range",
                    0.0 <= minimum < maximum <= 1.0,
                    f"{minimum}..{maximum}",
                    "Unity compile-stub import screenshot channels must be nonblank and inside 0..1",
                )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_density_screenshot_exists",
                compile_stub_report.get("densityScreenshotExists"),
                True,
            )
            verifier.require(
                "summary.unity_preflight_compile_stub_report_density_screenshot_bytes",
                int_or_default(compile_stub_report.get("densityScreenshotBytes"), 0) > 0,
                f"{compile_stub_report.get('densityScreenshotBytes')} bytes",
                "Unity compile-stub density screenshot path must write bytes",
            )
            verifier.require(
                "summary.unity_preflight_compile_stub_report_density_screenshot_checksum",
                is_nonzero_hex_checksum(compile_stub_report.get("densityScreenshotChecksum")),
                f"{compile_stub_report.get('densityScreenshotChecksum')} is nonzero",
                "Unity compile-stub density screenshot checksum must be nonzero",
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_density_screenshot_width",
                compile_stub_report.get("densityScreenshotWidth"),
                512,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_density_screenshot_height",
                compile_stub_report.get("densityScreenshotHeight"),
                512,
            )
            verifier.require_equal(
                "summary.unity_preflight_compile_stub_report_density_screenshot_pixels",
                compile_stub_report.get("densityScreenshotPixelCount"),
                512 * 512,
            )
            for channel in ["R", "G", "B"]:
                minimum = float_or_default(
                    compile_stub_report.get(f"densityScreenshotMin{channel}"),
                    -1.0,
                )
                maximum = float_or_default(
                    compile_stub_report.get(f"densityScreenshotMax{channel}"),
                    -1.0,
                )
                verifier.require(
                    f"summary.unity_preflight_compile_stub_report_density_screenshot_{channel.lower()}_range",
                    0.0 <= minimum < maximum <= 1.0,
                    f"{minimum}..{maximum}",
                    "Unity compile-stub density screenshot channels must be nonblank and inside 0..1",
                )
            check_material_parameter_report(
                verifier,
                "summary.unity_preflight_compile_stub_report",
                compile_stub_report,
                manifest,
                camel_case=True,
            )
            check_material_recipe_report(
                verifier,
                "summary.unity_preflight_compile_stub_report",
                compile_stub_report,
                manifest,
                camel_case=True,
            )
            check_engine_import_recipe_report(
                verifier,
                "summary.unity_preflight_compile_stub_report",
                compile_stub_report,
                manifest,
                camel_case=True,
            )
            check_surface_overlay_report(
                verifier,
                "summary.unity_preflight_compile_stub_report",
                compile_stub_report,
                manifest,
                camel_case=True,
            )
            check_prototype_surface_target_report(
                verifier,
                "summary.unity_preflight_compile_stub_report",
                compile_stub_report,
                manifest,
                camel_case=True,
            )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_scatter_instances",
            compile_stub_report.get("scatter_binary_instances"),
            222,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_scatter_chunks",
            compile_stub_report.get("scatter_binary_chunks"),
            26,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_engine_recipe_count",
            compile_stub_report.get("engine_import_recipe_count"),
            2,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_engine_recipe_files",
            compile_stub_report.get("engine_import_recipe_files"),
            ["engines/unity_import.recipe.json", "engines/unreal_import.recipe.json"],
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_surface_overlay_count",
            compile_stub_report.get("surface_overlay_count"),
            3,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_prototype_count",
            compile_stub_report.get("prototype_count"),
            8,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_scatter_chunk_reports",
            compile_stub_report.get("scatter_chunk_reports"),
            26,
        )
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_records_validated",
            compile_stub_report.get("scatter_binary_records_validated"),
            True,
        )
        verifier.require(
            "summary.unity_preflight_compile_stub_report_file_checksum",
            is_nonzero_hex_checksum(compile_stub_report.get("scatter_binary_file_checksum_xor")),
            f"{compile_stub_report.get('scatter_binary_file_checksum_xor')} is nonzero",
            "Unity compile-stub scatter file checksum XOR must be present and nonzero",
        )
        verifier.require(
            "summary.unity_preflight_compile_stub_report_record_checksum",
            is_nonzero_hex_checksum(compile_stub_report.get("scatter_binary_record_checksum_xor")),
            f"{compile_stub_report.get('scatter_binary_record_checksum_xor')} is nonzero",
            "Unity compile-stub scatter record checksum XOR must be present and nonzero",
        )
    verifier.require_equal(
        "summary.unity_preflight_instances",
        summary.get("unity_preflight", {}).get("scatter_binary_instances"),
        222,
    )
    verifier.require_equal(
        "summary.profile_notes_preflight_status",
        summary.get("profile_notes_preflight", {}).get("status"),
        "passed",
    )
    verifier.require_equal(
        "summary.unreal_dry_run_status",
        summary.get("unreal", {}).get("dry_run_status"),
        "passed",
    )
    verifier.require_equal(
        "summary.unreal_dry_run_instances",
        summary.get("unreal", {}).get("scatter_binary_instances"),
        222,
    )
    verifier.require_equal(
        "summary.unreal_dry_run_records_validated",
        summary.get("unreal", {}).get("scatter_binary_records_validated"),
        True,
    )
    verifier.require_equal(
        "summary.unreal_dry_run_records_read",
        summary.get("unreal", {}).get("scatter_binary_records_read"),
        222,
    )
    verifier.require(
        "summary.unreal_dry_run_file_checksum_xor",
        is_nonzero_hex_checksum(summary.get("unreal", {}).get("scatter_binary_file_checksum_xor")),
        f"{summary.get('unreal', {}).get('scatter_binary_file_checksum_xor')} is nonzero",
        "Unreal dry-run file checksum XOR must be present and nonzero",
    )
    verifier.require(
        "summary.unreal_dry_run_record_checksum_xor",
        is_nonzero_hex_checksum(summary.get("unreal", {}).get("scatter_binary_record_checksum_xor")),
        f"{summary.get('unreal', {}).get('scatter_binary_record_checksum_xor')} is nonzero",
        "Unreal dry-run record checksum XOR must be present and nonzero",
    )
    verifier.require_equal(
        "summary.unreal_dry_run_scatter_chunk_reports",
        summary.get("unreal", {}).get("scatter_chunk_reports"),
        26,
    )
    verifier.require_equal(
        "summary.unreal_fake_editor_status",
        summary.get("unreal_fake_editor", {}).get("status"),
        "passed",
    )
    fake_editor_report_value = summary.get("unreal_fake_editor", {}).get("report")
    if fake_editor_report_value:
        fake_editor_report_path = Path(fake_editor_report_value)
        if not fake_editor_report_path.is_absolute():
            fake_editor_report_path = validation_root / fake_editor_report_path
    else:
        fake_editor_report_path = validation_root / "forest_floor_unreal_fake_editor_report.json"
    fake_editor_report = load_json(fake_editor_report_path)
    if not isinstance(fake_editor_report, dict):
        verifier.fail(
            "summary.unreal_fake_editor_report",
            f"{fake_editor_report_path} is missing or invalid",
        )
    else:
        verifier.pass_(
            "summary.unreal_fake_editor_report",
            f"{fake_editor_report_path} loaded",
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_status",
            fake_editor_report.get("status"),
            "passed",
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_dry_run",
            fake_editor_report.get("dry_run"),
            False,
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_destination",
            fake_editor_report.get("destination_path"),
            "/Game/Midori/FakeEditor/Temperate_Forest_Floor",
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_source_file_count",
            fake_editor_report.get("source_file_count"),
            59,
        )
        verifier.require(
            "summary.unreal_fake_editor_report_manifest_checksum",
            is_nonzero_hex_checksum(fake_editor_report.get("manifest_file_checksum")),
            f"{fake_editor_report.get('manifest_file_checksum')} is nonzero",
            "Fake Unreal editor report manifest checksum must be present and nonzero",
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_manifest_checksum_matches_dry_run",
            fake_editor_report.get("manifest_file_checksum"),
            summary.get("unreal", {}).get("manifest_file_checksum"),
        )
        verifier.require(
            "summary.unreal_fake_editor_report_source_checksum",
            is_nonzero_hex_checksum(fake_editor_report.get("source_file_checksum_xor")),
            f"{fake_editor_report.get('source_file_checksum_xor')} is nonzero",
            "Fake Unreal editor report source checksum must be present and nonzero",
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_source_checksum_matches_dry_run",
            fake_editor_report.get("source_file_checksum_xor"),
            summary.get("unreal", {}).get("source_file_checksum_xor"),
        )
        if manifest is not None:
            verifier.require_equal(
                "summary.unreal_fake_editor_report_import_task_count",
                fake_editor_report.get("import_task_count"),
                len(expected_unreal_import_files(manifest)),
            )
            verifier.require_equal(
                "summary.unreal_fake_editor_report_map_import_count",
                fake_editor_report.get("imported_map_file_count"),
                len(expected_unreal_import_map_files(manifest)),
            )
            verifier.require_equal(
                "summary.unreal_fake_editor_report_prototype_lod_import_count",
                fake_editor_report.get("imported_prototype_lod_file_count"),
                len(expected_prototype_lod_files(manifest)),
            )
            verifier.require_equal(
                "summary.unreal_fake_editor_report_lod0_import_count",
                fake_editor_report.get("imported_lod0_prototype_file_count"),
                len(expected_lod0_prototype_files(manifest)),
            )
            verifier.require_equal(
                "summary.unreal_fake_editor_report_foliage_expected_count",
                fake_editor_report.get("foliage_type_expected_count"),
                len(manifest["prototypes"]),
            )
            verifier.require_equal(
                "summary.unreal_fake_editor_report_foliage_count",
                fake_editor_report.get("foliage_type_count"),
                len(manifest["prototypes"]),
            )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_foliage_status",
            fake_editor_report.get("foliage_type_status"),
            "created",
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_foliage_cull_start_cm",
            fake_editor_report.get("foliage_cull_start_cm"),
            3500,
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_foliage_cull_end_cm",
            fake_editor_report.get("foliage_cull_end_cm"),
            7000,
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_scatter_chunks",
            fake_editor_report.get("scatter_binary_chunks"),
            26,
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_scatter_instances",
            fake_editor_report.get("scatter_binary_instances"),
            222,
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_scatter_records_validated",
            fake_editor_report.get("scatter_binary_records_validated"),
            True,
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_scatter_records_read",
            fake_editor_report.get("scatter_binary_records_read"),
            222,
        )
        verifier.require(
            "summary.unreal_fake_editor_report_scatter_file_checksum",
            is_nonzero_hex_checksum(fake_editor_report.get("scatter_binary_file_checksum_xor")),
            f"{fake_editor_report.get('scatter_binary_file_checksum_xor')} is nonzero",
            "Fake Unreal editor scatter file checksum XOR must be present and nonzero",
        )
        verifier.require(
            "summary.unreal_fake_editor_report_scatter_record_checksum",
            is_nonzero_hex_checksum(fake_editor_report.get("scatter_binary_record_checksum_xor")),
            f"{fake_editor_report.get('scatter_binary_record_checksum_xor')} is nonzero",
            "Fake Unreal editor scatter record checksum XOR must be present and nonzero",
        )
        verifier.require_equal(
            "summary.unreal_fake_editor_report_corrupt_scatter_rejection",
            fake_editor_report.get("corrupt_scatter_rejection"),
            "passed",
        )
        screenshots = fake_editor_report.get("screenshots", {})
        metrics = fake_editor_report.get("screenshot_metrics", {})
        fake_editor_summary = summary.get("unreal_fake_editor", {})
        for key, label in [
            ("import", "import"),
            ("foliage_settings", "foliage_settings"),
        ]:
            screenshot = screenshots.get(key, {}) if isinstance(screenshots, dict) else {}
            metric = metrics.get(key, {}) if isinstance(metrics, dict) else {}
            prefix = f"summary.unreal_fake_editor_report_{label}_screenshot"
            summary_field_prefix = f"{label}_screenshot"
            verifier.require_equal(f"{prefix}_status", screenshot.get("status"), "captured")
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_status",
                fake_editor_summary.get(f"{summary_field_prefix}_status"),
                screenshot.get("status"),
            )
            verifier.require_equal(
                f"{prefix}_method",
                screenshot.get("method"),
                "AutomationLibrary.take_high_res_screenshot",
            )
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_method",
                fake_editor_summary.get(f"{summary_field_prefix}_method"),
                screenshot.get("method"),
            )
            verifier.require_equal(f"{prefix}_reported_exists", screenshot.get("exists"), True)
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_report_exists",
                fake_editor_summary.get(f"{summary_field_prefix}_report_exists"),
                screenshot.get("exists"),
            )
            verifier.require(
                f"{prefix}_reported_bytes",
                int_or_default(screenshot.get("bytes"), 0) > 0,
                f"{screenshot.get('bytes')} bytes",
                "Fake Unreal importer screenshot report must write bytes",
            )
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_report_bytes",
                fake_editor_summary.get(f"{summary_field_prefix}_report_bytes"),
                screenshot.get("bytes"),
            )
            verifier.require(
                f"{prefix}_reported_checksum",
                is_nonzero_hex_checksum(screenshot.get("checksum")),
                f"{screenshot.get('checksum')} is nonzero",
                "Fake Unreal importer screenshot report must include a nonzero checksum",
            )
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_report_checksum",
                fake_editor_summary.get(f"{summary_field_prefix}_report_checksum"),
                screenshot.get("checksum"),
            )
            verifier.require_equal(f"{prefix}_reported_width", screenshot.get("width"), 1024)
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_report_width",
                fake_editor_summary.get(f"{summary_field_prefix}_report_width"),
                screenshot.get("width"),
            )
            verifier.require_equal(f"{prefix}_reported_height", screenshot.get("height"), 1024)
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_report_height",
                fake_editor_summary.get(f"{summary_field_prefix}_report_height"),
                screenshot.get("height"),
            )
            verifier.require_equal(f"{prefix}_exists", metric.get("exists"), True)
            verifier.require(
                f"{prefix}_bytes",
                int_or_default(metric.get("bytes"), 0) > 0,
                f"{metric.get('bytes')} bytes",
                "Fake Unreal screenshot path must write bytes",
            )
            verifier.require(
                f"{prefix}_checksum",
                is_nonzero_hex_checksum(metric.get("checksum")),
                f"{metric.get('checksum')} is nonzero",
                "Fake Unreal screenshot checksum must be present and nonzero",
            )
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_metric_checksum",
                fake_editor_summary.get(f"{summary_field_prefix}_checksum"),
                metric.get("checksum"),
            )
            verifier.require_equal(f"{prefix}_width", metric.get("width"), 1024)
            verifier.require_equal(f"{prefix}_height", metric.get("height"), 1024)
            verifier.require_equal(f"{prefix}_sample_count", metric.get("sample_count"), 1024 * 1024)
            verifier.require(
                f"{prefix}_luminance_range",
                int_or_default(metric.get("luminance_range"), 0) >= MIN_SCREENSHOT_LUMINANCE_RANGE,
                f"{metric.get('luminance_range')} luminance range",
                "Fake Unreal screenshot must be nonblank enough to model the real screenshot gate",
            )
            verifier.require_equal(
                f"summary.unreal_fake_editor_{label}_screenshot_luminance_range",
                fake_editor_summary.get(f"{summary_field_prefix}_luminance_range"),
                metric.get("luminance_range"),
            )


def check_unity_report(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
    package_dir: Path,
) -> None:
    terrain = manifest["terrain"]
    mobile = manifest["mobile"]
    unity = manifest["unity"]
    wind = manifest["wind_packing"]

    verifier.require_equal("unity.schema_version", report.get("schemaVersion"), manifest["schema_version"])
    verifier.require_equal("unity.profile", report.get("profile"), "mobile")
    check_package_identity(
        verifier,
        "unity",
        report,
        manifest,
        package_dir,
        "unity_yplus_file",
        camel_case=True,
    )
    verifier.require_close("unity.tile_size", report.get("tileSizeMeters"), manifest["tile_size"])
    verifier.require_close(
        "unity.terrain_size_x",
        vector_component(report.get("terrainSize"), "x"),
        manifest["tile_size"],
    )
    verifier.require_close(
        "unity.terrain_size_z",
        vector_component(report.get("terrainSize"), "z"),
        manifest["tile_size"],
    )
    verifier.require_close(
        "unity.terrain_height",
        vector_component(report.get("terrainSize"), "y"),
        max(0.01, terrain["height_max"] - terrain["height_min"]),
    )
    verifier.require_equal("unity.heightmap_file", report.get("heightmapFile"), terrain["heightmap_file"])
    verifier.require_equal("unity.density_map", report.get("densityMapFile"), terrain["grass_density_file"])
    verifier.require_equal("unity.normal_map", report.get("normalMapFile"), manifest["normal_conventions"]["unity_yplus_file"])
    verifier.require_equal("unity.hint_heightmap", report.get("unityHintHeightmap"), unity["terrain_heightmap"])
    verifier.require_equal("unity.hint_density", report.get("unityHintDensityMap"), unity["detail_density_map"])
    verifier.require_equal("unity.hint_normal", report.get("unityHintNormalMap"), unity["normal_map"])
    check_unity_screenshot_report(
        verifier,
        "unity.import_screenshot_report",
        report.get("importScreenshot"),
        1024,
        1024,
    )
    check_unity_screenshot_report(
        verifier,
        "unity.density_screenshot_report",
        report.get("densityScreenshot"),
        512,
        512,
    )
    verifier.require_close("unity.mobile_density", report.get("mobileDensityScale"), mobile["density_scale"])
    verifier.require_close("unity.mobile_lod0_distance", report.get("mobileLod0MaxDistance"), mobile["lod0_max_distance"])
    verifier.require_close("unity.mobile_lod1_distance", report.get("mobileLod1MaxDistance"), mobile["lod1_max_distance"])
    verifier.require_close("unity.mobile_lod2_distance", report.get("mobileLod2MaxDistance"), mobile["lod2_max_distance"])
    verifier.require_close("unity.mobile_cull_start", report.get("mobileCullStartMeters"), mobile["cull_start"])
    verifier.require_close("unity.mobile_cull_end", report.get("mobileCullEndMeters"), mobile["cull_end"])
    verifier.require_equal("unity.mobile_shadows", report.get("mobileShadows"), mobile["shadows"])
    verifier.require_equal("unity.mobile_material_slots", report.get("mobileMaterialSlots"), mobile["material_slots"])
    verifier.require_equal(
        "unity.mobile_max_instances_per_tile",
        report.get("mobileMaxInstancesPerTile"),
        mobile["max_instances_per_tile"],
    )
    verifier.require_equal(
        "unity.mobile_max_instances_per_chunk",
        report.get("mobileMaxInstancesPerChunk"),
        mobile["max_instances_per_chunk"],
    )
    verifier.require_equal("unity.mobile_grass_collision", report.get("mobileGrassCollision"), mobile["grass_collision"])
    verifier.require_equal("unity.mobile_moss_collision", report.get("mobileMossCollision"), mobile["moss_collision"])
    verifier.require_equal("unity.material_slots_declared", report.get("materialSlotsDeclared"), len(manifest["material_slots"]))
    verifier.require_equal("unity.has_terrain_surface_slot", report.get("hasTerrainSurfaceMaterialSlot"), True)
    verifier.require_equal("unity.has_groundcover_slot", report.get("hasGroundcoverFoliageMaterialSlot"), True)
    verifier.require_equal("unity.groundcover_alpha", report.get("groundcoverMaterialAlphaMode"), "masked")
    verifier.require_equal("unity.groundcover_double_sided", report.get("groundcoverMaterialDoubleSided"), True)
    verifier.require_equal("unity.groundcover_shadows", report.get("groundcoverMaterialShadows"), False)
    check_material_parameter_report(verifier, "unity", report, manifest, camel_case=True)
    check_material_recipe_report(verifier, "unity", report, manifest, camel_case=True)
    check_engine_import_recipe_report(verifier, "unity", report, manifest, camel_case=True)
    check_surface_overlay_report(verifier, "unity", report, manifest, camel_case=True)
    check_prototype_surface_target_report(verifier, "unity", report, manifest, camel_case=True)
    verifier.require_equal("unity.wind_phase", report.get("windPhase"), wind["phase"])
    verifier.require_equal("unity.wind_stiffness", report.get("windStiffness"), wind["stiffness"])
    verifier.require_equal("unity.wind_height", report.get("windHeight"), wind["height"])
    verifier.require_equal("unity.wind_color_variation", report.get("windColorVariation"), wind["color_variation"])
    verifier.require_equal(
        "unity.wind_normalized_progress",
        report.get("windNormalizedProgress"),
        wind["normalized_progress"],
    )
    verifier.require_equal("unity.prototypes_declared", report.get("prototypesDeclared"), len(manifest["prototypes"]))
    verifier.require_equal("unity.lod0_declared", report.get("lod0PrototypesDeclared"), len(manifest["prototypes"]))
    verifier.require_equal("unity.lod_files_declared", report.get("lodFilesDeclared"), 22)
    verifier.require(
        "unity.detail_prototypes_created",
        int(report.get("detailPrototypesCreated", 0)) > 0,
        f"{report.get('detailPrototypesCreated')} detail prototypes created",
        "Unity must create at least one detail prototype",
    )
    verifier.require_equal(
        "unity.detail_prototype_failures",
        report.get("detailPrototypeFailures"),
        0,
    )
    verifier.require_equal(
        "unity.detail_prototype_fallback_errors",
        len(report.get("detailPrototypeFallbackErrors") or []),
        0,
    )
    verifier.require_equal(
        "unity.detail_prototype_source_count",
        int_or_default(report.get("detailPrototypesLoadedFromAssets"), 0)
        + int_or_default(report.get("detailPrototypesGeneratedFromGlb"), 0),
        report.get("detailPrototypesCreated"),
    )
    verifier.require(
        "unity.detail_prototype_glb_sources",
        int_or_default(report.get("detailPrototypesLoadedFromAssets"), 0) > 0
        or int_or_default(report.get("detailPrototypesGeneratedFromGlb"), 0) > 0,
        (
            f"{report.get('detailPrototypesLoadedFromAssets')} AssetDatabase prototypes, "
            f"{report.get('detailPrototypesGeneratedFromGlb')} native GLB prototypes"
        ),
        "Unity detail prototypes must come from imported GLB assets or Midori's native GLB fallback",
    )
    verifier.require(
        "unity.detail_density_nonzero",
        int(report.get("nonZeroDetailCells", 0)) > 0,
        f"{report.get('nonZeroDetailCells')} nonzero detail cells",
        "Unity detail density must produce nonzero detail cells",
    )
    verifier.require_equal("unity.scatter_instances", report.get("scatterBinaryInstances"), 222)
    check_unity_scatter_report(verifier, report)


def check_unity_screenshot_report(
    verifier: EvidenceVerifier,
    prefix: str,
    report: Any,
    expected_width: int,
    expected_height: int,
) -> None:
    if not isinstance(report, dict):
        verifier.fail(prefix, "Unity report is missing screenshot artifact summary")
        return
    verifier.require(
        f"{prefix}.path",
        isinstance(report.get("path"), str) and bool(report.get("path")),
        str(report.get("path")),
        "Unity screenshot report must include a nonempty path",
    )
    verifier.require_equal(f"{prefix}.status", report.get("status"), "captured")
    verifier.require_equal(f"{prefix}.exists", report.get("exists"), True)
    verifier.require(
        f"{prefix}.bytes",
        int_or_default(report.get("bytes"), 0) > 0,
        f"{report.get('bytes')} bytes",
        "Unity screenshot report must record nonzero bytes",
    )
    verifier.require(
        f"{prefix}.checksum",
        is_nonzero_hex_checksum(report.get("checksum")),
        f"{report.get('checksum')} is nonzero",
        "Unity screenshot report must record a nonzero checksum",
    )
    verifier.require_equal(f"{prefix}.width", report.get("width"), expected_width)
    verifier.require_equal(f"{prefix}.height", report.get("height"), expected_height)


def check_unity_scatter_report(verifier: EvidenceVerifier, report: dict[str, Any]) -> None:
    verifier.require_equal("unity.scatter_chunks", report.get("scatterBinaryChunks"), 26)
    verifier.require_equal("unity.scatter_records_validated", report.get("scatterBinaryRecordsValidated"), True)
    verifier.require_equal("unity.scatter_records_read", report.get("scatterBinaryRecordsRead"), 222)
    verifier.require(
        "unity.scatter_file_checksum_xor",
        is_nonzero_hex_checksum(report.get("scatterBinaryFileChecksumXor")),
        f"{report.get('scatterBinaryFileChecksumXor')} is nonzero",
        "Unity scatter file checksum XOR must be present and nonzero",
    )
    verifier.require(
        "unity.scatter_record_checksum_xor",
        is_nonzero_hex_checksum(report.get("scatterBinaryRecordChecksumXor")),
        f"{report.get('scatterBinaryRecordChecksumXor')} is nonzero",
        "Unity scatter record checksum XOR must be present and nonzero",
    )

    summaries = report.get("scatterChunkReports")
    if not isinstance(summaries, list):
        verifier.fail("unity.scatter_chunk_reports", "Unity report is missing scatterChunkReports")
        return

    verifier.require_equal("unity.scatter_chunk_report_count", len(summaries), 26)
    total_instances = 0
    for index, summary in enumerate(summaries):
        if not isinstance(summary, dict):
            verifier.fail(f"unity.scatter_chunk_report.{index}", "summary must be an object")
            continue

        label = f"unity.scatter_chunk_report.{index}"
        instance_count = int_or_default(summary.get("instanceCount"), 0)
        total_instances += instance_count
        verifier.require(
            f"{label}.instance_count",
            instance_count > 0,
            f"{instance_count} instances",
            "Unity scatter chunk reports must contain at least one instance",
        )
        verifier.require(
            f"{label}.file_checksum",
            is_nonzero_hex_checksum(summary.get("fileChecksum")),
            f"{summary.get('fileChecksum')} is nonzero",
            "Unity scatter chunk file checksum must be present and nonzero",
        )
        verifier.require(
            f"{label}.record_checksum",
            is_nonzero_hex_checksum(summary.get("recordChecksum")),
            f"{summary.get('recordChecksum')} is nonzero",
            "Unity scatter chunk record checksum must be present and nonzero",
        )
        verifier.require(
            f"{label}.yaw_range",
            0.0 <= float_or_default(summary.get("yawMin"), -1.0)
            <= float_or_default(summary.get("yawMax"), -1.0)
            <= math.tau,
            f"{summary.get('yawMin')}..{summary.get('yawMax')}",
            "Unity scatter yaw range must be ordered and inside 0..tau",
        )
        verifier.require(
            f"{label}.phase_range",
            0.0 <= float_or_default(summary.get("phaseMin"), -1.0)
            <= float_or_default(summary.get("phaseMax"), -1.0)
            <= math.tau,
            f"{summary.get('phaseMin')}..{summary.get('phaseMax')}",
            "Unity scatter phase range must be ordered and inside 0..tau",
        )
        verifier.require(
            f"{label}.height_range",
            0.0 < float_or_default(summary.get("heightMin"), 0.0)
            <= float_or_default(summary.get("heightMax"), 0.0),
            f"{summary.get('heightMin')}..{summary.get('heightMax')}",
            "Unity scatter height multiplier range must be positive and ordered",
        )
        verifier.require(
            f"{label}.width_range",
            0.0 < float_or_default(summary.get("widthMin"), 0.0)
            <= float_or_default(summary.get("widthMax"), 0.0),
            f"{summary.get('widthMin')}..{summary.get('widthMax')}",
            "Unity scatter width multiplier range must be positive and ordered",
        )
        verifier.require(
            f"{label}.color_variation_range",
            0.0 <= float_or_default(summary.get("colorVariationMin"), -1.0)
            <= float_or_default(summary.get("colorVariationMax"), -1.0)
            <= 1.0,
            f"{summary.get('colorVariationMin')}..{summary.get('colorVariationMax')}",
            "Unity scatter color variation range must be ordered and inside 0..1",
        )

    verifier.require_equal("unity.scatter_chunk_report_instances", total_instances, 222)


def check_unreal_report(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
    package_dir: Path,
) -> None:
    console = manifest["console"]
    unreal = manifest["unreal"]
    groundcover = report.get("groundcover_material", {})

    verifier.require_equal("unreal.schema_version", report.get("schema_version"), manifest["schema_version"])
    verifier.require_equal("unreal.dry_run_false", report.get("dry_run"), False)
    verifier.require_equal("unreal.profile", report.get("profile"), "console")
    check_package_identity(
        verifier,
        "unreal",
        report,
        manifest,
        package_dir,
        "unreal_yminus_file",
        camel_case=False,
    )
    verifier.require_close("unreal.tile_size", report.get("tile_size_meters"), manifest["tile_size"])
    verifier.require_equal("unreal.landscape_heightmap", report.get("landscape_heightmap"), unreal["landscape_heightmap"])
    verifier.require_equal("unreal.landscape_weightmap", report.get("landscape_weightmap"), unreal["landscape_weightmap"])
    verifier.require_equal("unreal.normal_map", report.get("normal_map"), unreal["normal_map"])
    screenshots = report.get("screenshots", {})
    check_unreal_screenshot_report(
        verifier,
        "unreal.import_screenshot_report",
        screenshots.get("import") if isinstance(screenshots, dict) else None,
        1024,
        1024,
    )
    check_unreal_screenshot_report(
        verifier,
        "unreal.foliage_settings_screenshot_report",
        screenshots.get("foliage_settings") if isinstance(screenshots, dict) else None,
        1024,
        1024,
    )
    verifier.require_equal("unreal.console_density", report.get("console_density_scale"), console["density_scale"])
    verifier.require_close(
        "unreal.console_lod0_distance_m",
        report.get("console_lod0_max_distance_meters"),
        console["lod0_max_distance"],
    )
    verifier.require_close(
        "unreal.console_lod1_distance_m",
        report.get("console_lod1_max_distance_meters"),
        console["lod1_max_distance"],
    )
    verifier.require_close(
        "unreal.console_lod2_distance_m",
        report.get("console_lod2_max_distance_meters"),
        console["lod2_max_distance"],
    )
    verifier.require_equal("unreal.console_material_slots", report.get("console_material_slots"), console["material_slots"])
    verifier.require_equal(
        "unreal.console_max_instances_per_tile",
        report.get("console_max_instances_per_tile"),
        console["max_instances_per_tile"],
    )
    verifier.require_equal(
        "unreal.console_max_instances_per_chunk",
        report.get("console_max_instances_per_chunk"),
        console["max_instances_per_chunk"],
    )
    verifier.require_equal("unreal.console_shadows", report.get("console_shadows"), console["shadows"])
    verifier.require_equal("unreal.console_grass_collision", report.get("console_grass_collision"), console["grass_collision"])
    verifier.require_equal("unreal.console_moss_collision", report.get("console_moss_collision"), console["moss_collision"])
    verifier.require_close("unreal.console_cull_start_m", report.get("console_cull_start_meters"), console["cull_start"])
    verifier.require_close("unreal.console_cull_end_m", report.get("console_cull_end_meters"), console["cull_end"])
    verifier.require_equal("unreal.console_cull_start_cm", report.get("console_cull_start_cm"), 3500)
    verifier.require_equal("unreal.console_cull_end_cm", report.get("console_cull_end_cm"), 7000)
    verifier.require_equal("unreal.prototypes_declared", report.get("prototypes_declared"), len(manifest["prototypes"]))
    verifier.require_equal("unreal.lod0_declared", report.get("lod0_prototypes_declared"), len(manifest["prototypes"]))
    verifier.require_equal("unreal.lod_files_declared", report.get("lod_files_declared"), 22)
    verifier.require_equal("unreal.foliage_type_status", report.get("foliage_type_status"), "created")
    verifier.require_equal(
        "unreal.foliage_type_expected_count",
        report.get("foliage_type_expected_count"),
        len(manifest["prototypes"]),
    )
    verifier.require_equal("unreal.foliage_type_count", report.get("foliage_type_count"), len(manifest["prototypes"]))
    foliage_assets = report.get("foliage_type_assets")
    verifier.require(
        "unreal.foliage_type_assets",
        isinstance(foliage_assets, list) and len(foliage_assets) == len(manifest["prototypes"]),
        f"{len(foliage_assets) if isinstance(foliage_assets, list) else 'missing'} foliage assets",
        "Unreal editor import must create one foliage type asset per LOD0 prototype",
    )
    verifier.require_equal("unreal.foliage_cull_start_cm", report.get("foliage_cull_start_cm"), 3500)
    verifier.require_equal("unreal.foliage_cull_end_cm", report.get("foliage_cull_end_cm"), 7000)
    verifier.require_equal("unreal.material_slots_declared", report.get("material_slots_declared"), len(manifest["material_slots"]))
    verifier.require_equal("unreal.has_terrain_surface_slot", report.get("has_terrain_surface_material_slot"), True)
    verifier.require_equal("unreal.has_groundcover_slot", report.get("has_groundcover_foliage_material_slot"), True)
    verifier.require_equal("unreal.groundcover_alpha", groundcover.get("alpha_mode"), "masked")
    verifier.require_equal("unreal.groundcover_double_sided", groundcover.get("double_sided"), True)
    verifier.require_equal("unreal.groundcover_shadows", groundcover.get("shadows"), False)
    check_material_parameter_report(verifier, "unreal", report, manifest)
    check_material_recipe_report(verifier, "unreal", report, manifest)
    check_engine_import_recipe_report(verifier, "unreal", report, manifest)
    check_surface_overlay_report(verifier, "unreal", report, manifest)
    check_prototype_surface_target_report(verifier, "unreal", report, manifest)
    check_unreal_import_tasks(verifier, "unreal", report, manifest)
    verifier.require_equal("unreal.wind_packing", report.get("wind_packing"), manifest["wind_packing"])
    verifier.require_equal("unreal.scatter_instances", report.get("scatter_binary_instances"), 222)
    check_unreal_scatter_report(verifier, report, "unreal")


def check_unreal_screenshot_report(
    verifier: EvidenceVerifier,
    prefix: str,
    report: Any,
    expected_width: int,
    expected_height: int,
) -> None:
    if not isinstance(report, dict):
        verifier.fail(prefix, "Unreal report is missing screenshot artifact summary")
        return
    verifier.require(
        f"{prefix}.path",
        isinstance(report.get("path"), str) and bool(report.get("path")),
        str(report.get("path")),
        "Unreal screenshot report must include a nonempty path",
    )
    status = report.get("status")
    verifier.require(
        f"{prefix}.status",
        status in {"captured", "requested"},
        str(status),
        "Unreal screenshot report must be captured or requested by a supported API",
    )
    verifier.require(
        f"{prefix}.method",
        isinstance(report.get("method"), str) and bool(report.get("method")),
        str(report.get("method")),
        "Unreal screenshot report must include the capture/request method",
    )
    if status == "captured":
        verifier.require_equal(f"{prefix}.exists", report.get("exists"), True)
        verifier.require(
            f"{prefix}.bytes",
            int_or_default(report.get("bytes"), 0) > 0,
            f"{report.get('bytes')} bytes",
            "Captured Unreal screenshot report must record nonzero bytes",
        )
        verifier.require(
            f"{prefix}.checksum",
            is_nonzero_hex_checksum(report.get("checksum")),
            f"{report.get('checksum')} is nonzero",
            "Captured Unreal screenshot report must record a nonzero checksum",
        )
        verifier.require_equal(f"{prefix}.width", report.get("width"), expected_width)
        verifier.require_equal(f"{prefix}.height", report.get("height"), expected_height)


def check_unreal_dry_run_report(
    verifier: EvidenceVerifier,
    report: dict[str, Any],
    manifest: dict[str, Any],
    package_dir: Path,
) -> None:
    console = manifest["console"]
    unreal = manifest["unreal"]
    groundcover = report.get("groundcover_material", {})

    verifier.require_equal("unreal_dry_run.schema_version", report.get("schema_version"), manifest["schema_version"])
    verifier.require_equal("unreal_dry_run.dry_run_true", report.get("dry_run"), True)
    verifier.require_equal("unreal_dry_run.profile", report.get("profile"), "console")
    check_package_identity(
        verifier,
        "unreal_dry_run",
        report,
        manifest,
        package_dir,
        "unreal_yminus_file",
        camel_case=False,
    )
    verifier.require_close("unreal_dry_run.tile_size", report.get("tile_size_meters"), manifest["tile_size"])
    verifier.require_equal(
        "unreal_dry_run.landscape_heightmap",
        report.get("landscape_heightmap"),
        unreal["landscape_heightmap"],
    )
    verifier.require_equal(
        "unreal_dry_run.landscape_weightmap",
        report.get("landscape_weightmap"),
        unreal["landscape_weightmap"],
    )
    verifier.require_equal("unreal_dry_run.normal_map", report.get("normal_map"), unreal["normal_map"])
    verifier.require_equal("unreal_dry_run.console_density", report.get("console_density_scale"), console["density_scale"])
    verifier.require_close(
        "unreal_dry_run.console_lod0_distance_m",
        report.get("console_lod0_max_distance_meters"),
        console["lod0_max_distance"],
    )
    verifier.require_close(
        "unreal_dry_run.console_lod1_distance_m",
        report.get("console_lod1_max_distance_meters"),
        console["lod1_max_distance"],
    )
    verifier.require_close(
        "unreal_dry_run.console_lod2_distance_m",
        report.get("console_lod2_max_distance_meters"),
        console["lod2_max_distance"],
    )
    verifier.require_equal("unreal_dry_run.console_material_slots", report.get("console_material_slots"), console["material_slots"])
    verifier.require_equal(
        "unreal_dry_run.console_max_instances_per_tile",
        report.get("console_max_instances_per_tile"),
        console["max_instances_per_tile"],
    )
    verifier.require_equal(
        "unreal_dry_run.console_max_instances_per_chunk",
        report.get("console_max_instances_per_chunk"),
        console["max_instances_per_chunk"],
    )
    verifier.require_equal("unreal_dry_run.console_shadows", report.get("console_shadows"), console["shadows"])
    verifier.require_equal(
        "unreal_dry_run.console_grass_collision",
        report.get("console_grass_collision"),
        console["grass_collision"],
    )
    verifier.require_equal(
        "unreal_dry_run.console_moss_collision",
        report.get("console_moss_collision"),
        console["moss_collision"],
    )
    verifier.require_close("unreal_dry_run.console_cull_start_m", report.get("console_cull_start_meters"), console["cull_start"])
    verifier.require_close("unreal_dry_run.console_cull_end_m", report.get("console_cull_end_meters"), console["cull_end"])
    verifier.require_equal("unreal_dry_run.console_cull_start_cm", report.get("console_cull_start_cm"), 3500)
    verifier.require_equal("unreal_dry_run.console_cull_end_cm", report.get("console_cull_end_cm"), 7000)
    verifier.require_equal("unreal_dry_run.prototypes_declared", report.get("prototypes_declared"), len(manifest["prototypes"]))
    verifier.require_equal("unreal_dry_run.lod0_declared", report.get("lod0_prototypes_declared"), len(manifest["prototypes"]))
    verifier.require_equal("unreal_dry_run.lod_files_declared", report.get("lod_files_declared"), 22)
    verifier.require_equal("unreal_dry_run.foliage_type_status", report.get("foliage_type_status"), "dry_run")
    verifier.require_equal(
        "unreal_dry_run.foliage_type_expected_count",
        report.get("foliage_type_expected_count"),
        len(manifest["prototypes"]),
    )
    verifier.require_equal("unreal_dry_run.foliage_type_count", report.get("foliage_type_count"), 0)
    verifier.require_equal("unreal_dry_run.foliage_type_assets", report.get("foliage_type_assets"), [])
    verifier.require_equal("unreal_dry_run.foliage_cull_start_cm", report.get("foliage_cull_start_cm"), 3500)
    verifier.require_equal("unreal_dry_run.foliage_cull_end_cm", report.get("foliage_cull_end_cm"), 7000)
    verifier.require_equal(
        "unreal_dry_run.material_slots_declared",
        report.get("material_slots_declared"),
        len(manifest["material_slots"]),
    )
    verifier.require_equal(
        "unreal_dry_run.has_terrain_surface_slot",
        report.get("has_terrain_surface_material_slot"),
        True,
    )
    verifier.require_equal(
        "unreal_dry_run.has_groundcover_slot",
        report.get("has_groundcover_foliage_material_slot"),
        True,
    )
    verifier.require_equal("unreal_dry_run.groundcover_alpha", groundcover.get("alpha_mode"), "masked")
    verifier.require_equal("unreal_dry_run.groundcover_double_sided", groundcover.get("double_sided"), True)
    verifier.require_equal("unreal_dry_run.groundcover_shadows", groundcover.get("shadows"), False)
    check_material_parameter_report(verifier, "unreal_dry_run", report, manifest)
    check_material_recipe_report(verifier, "unreal_dry_run", report, manifest)
    check_engine_import_recipe_report(verifier, "unreal_dry_run", report, manifest)
    check_surface_overlay_report(verifier, "unreal_dry_run", report, manifest)
    check_prototype_surface_target_report(verifier, "unreal_dry_run", report, manifest)
    check_unreal_import_tasks(verifier, "unreal_dry_run", report, manifest)
    verifier.require_equal("unreal_dry_run.wind_packing", report.get("wind_packing"), manifest["wind_packing"])
    verifier.require_equal("unreal_dry_run.scatter_instances", report.get("scatter_binary_instances"), 222)
    check_unreal_scatter_report(verifier, report, "unreal_dry_run")


def check_unreal_import_tasks(
    verifier: EvidenceVerifier,
    prefix: str,
    report: dict[str, Any],
    manifest: dict[str, Any],
) -> None:
    expected_files = expected_unreal_import_files(manifest)
    expected_destinations = expected_unreal_import_destinations(
        str(report.get("destination_path", "")),
        expected_files,
    )
    expected_extensions = sorted({Path(relative).suffix.lower() for relative in expected_files})
    expected_maps = expected_unreal_import_map_files(manifest)
    expected_lod_files = expected_prototype_lod_files(manifest)
    expected_lod0_files = expected_lod0_prototype_files(manifest)

    verifier.require_equal(f"{prefix}.imported_files", report.get("imported_files"), expected_files)
    verifier.require_equal(f"{prefix}.import_task_count", report.get("import_task_count"), len(expected_files))
    verifier.require_equal(f"{prefix}.import_task_files", report.get("import_task_files"), expected_files)
    verifier.require_equal(
        f"{prefix}.import_task_destinations",
        report.get("import_task_destination_paths"),
        expected_destinations,
    )
    verifier.require_equal(
        f"{prefix}.import_task_extensions",
        report.get("import_task_extensions"),
        expected_extensions,
    )
    verifier.require_equal(f"{prefix}.imported_manifest", report.get("imported_manifest"), True)
    verifier.require_equal(f"{prefix}.imported_preview_tile", report.get("imported_preview_tile"), True)
    verifier.require_equal(f"{prefix}.imported_map_files", report.get("imported_map_files"), expected_maps)
    verifier.require_equal(f"{prefix}.imported_map_file_count", report.get("imported_map_file_count"), len(expected_maps))
    verifier.require_equal(
        f"{prefix}.imported_prototype_lod_files",
        report.get("imported_prototype_lod_files"),
        expected_lod_files,
    )
    verifier.require_equal(
        f"{prefix}.imported_prototype_lod_file_count",
        report.get("imported_prototype_lod_file_count"),
        len(expected_lod_files),
    )
    verifier.require_equal(
        f"{prefix}.imported_lod0_prototype_files",
        report.get("imported_lod0_prototype_files"),
        expected_lod0_files,
    )
    verifier.require_equal(
        f"{prefix}.imported_lod0_prototype_file_count",
        report.get("imported_lod0_prototype_file_count"),
        len(expected_lod0_files),
    )

    tasks = report.get("import_tasks")
    if not isinstance(tasks, list):
        verifier.fail(f"{prefix}.import_tasks", "Unreal report is missing import task details")
        return
    verifier.require_equal(f"{prefix}.import_task_report_count", len(tasks), len(expected_files))
    for index, (task, relative, destination) in enumerate(zip(tasks, expected_files, expected_destinations)):
        label = f"{prefix}.import_task.{index}"
        if not isinstance(task, dict):
            verifier.fail(label, "Unreal import task report must be an object")
            continue
        verifier.require_equal(f"{label}.file", task.get("file"), relative)
        verifier.require_equal(f"{label}.destination_path", task.get("destination_path"), destination)
        verifier.require_equal(f"{label}.extension", task.get("extension"), Path(relative).suffix.lower())


def check_unreal_scatter_report(verifier: EvidenceVerifier, report: dict[str, Any], prefix: str) -> None:
    verifier.require_equal(f"{prefix}.scatter_chunks", report.get("scatter_binary_chunks"), 26)
    verifier.require_equal(f"{prefix}.scatter_records_validated", report.get("scatter_binary_records_validated"), True)
    verifier.require_equal(f"{prefix}.scatter_records_read", report.get("scatter_binary_records_read"), 222)
    verifier.require(
        f"{prefix}.scatter_file_checksum_xor",
        is_nonzero_hex_checksum(report.get("scatter_binary_file_checksum_xor")),
        f"{report.get('scatter_binary_file_checksum_xor')} is nonzero",
        f"{prefix} scatter file checksum XOR must be present and nonzero",
    )
    verifier.require(
        f"{prefix}.scatter_record_checksum_xor",
        is_nonzero_hex_checksum(report.get("scatter_binary_record_checksum_xor")),
        f"{report.get('scatter_binary_record_checksum_xor')} is nonzero",
        f"{prefix} scatter record checksum XOR must be present and nonzero",
    )

    summaries = report.get("scatter_chunks")
    if not isinstance(summaries, list):
        verifier.fail(f"{prefix}.scatter_chunks_report", "Unreal report is missing scatter_chunks")
        return

    verifier.require_equal(f"{prefix}.scatter_chunk_report_count", len(summaries), 26)
    total_instances = 0
    for index, summary in enumerate(summaries):
        if not isinstance(summary, dict):
            verifier.fail(f"{prefix}.scatter_chunk_report.{index}", "summary must be an object")
            continue

        label = f"{prefix}.scatter_chunk_report.{index}"
        instance_count = int_or_default(summary.get("instance_count"), 0)
        total_instances += instance_count
        verifier.require(
            f"{label}.instance_count",
            instance_count > 0,
            f"{instance_count} instances",
            f"{prefix} scatter chunk reports must contain at least one instance",
        )
        verifier.require(
            f"{label}.file_checksum",
            is_nonzero_hex_checksum(summary.get("file_checksum")),
            f"{summary.get('file_checksum')} is nonzero",
            f"{prefix} scatter chunk file checksum must be present and nonzero",
        )
        verifier.require(
            f"{label}.record_checksum",
            is_nonzero_hex_checksum(summary.get("record_checksum")),
            f"{summary.get('record_checksum')} is nonzero",
            f"{prefix} scatter chunk record checksum must be present and nonzero",
        )
        verifier.require(
            f"{label}.yaw_range",
            0.0 <= float_or_default(summary.get("yaw_min"), -1.0)
            <= float_or_default(summary.get("yaw_max"), -1.0)
            <= math.tau,
            f"{summary.get('yaw_min')}..{summary.get('yaw_max')}",
            f"{prefix} scatter yaw range must be ordered and inside 0..tau",
        )
        verifier.require(
            f"{label}.phase_range",
            0.0 <= float_or_default(summary.get("phase_min"), -1.0)
            <= float_or_default(summary.get("phase_max"), -1.0)
            <= math.tau,
            f"{summary.get('phase_min')}..{summary.get('phase_max')}",
            f"{prefix} scatter phase range must be ordered and inside 0..tau",
        )
        verifier.require(
            f"{label}.height_range",
            0.0 < float_or_default(summary.get("height_min"), 0.0)
            <= float_or_default(summary.get("height_max"), 0.0),
            f"{summary.get('height_min')}..{summary.get('height_max')}",
            f"{prefix} scatter height multiplier range must be positive and ordered",
        )
        verifier.require(
            f"{label}.width_range",
            0.0 < float_or_default(summary.get("width_min"), 0.0)
            <= float_or_default(summary.get("width_max"), 0.0),
            f"{summary.get('width_min')}..{summary.get('width_max')}",
            f"{prefix} scatter width multiplier range must be positive and ordered",
        )
        verifier.require(
            f"{label}.color_variation_range",
            0.0 <= float_or_default(summary.get("color_variation_min"), -1.0)
            <= float_or_default(summary.get("color_variation_max"), -1.0)
            <= 1.0,
            f"{summary.get('color_variation_min')}..{summary.get('color_variation_max')}",
            f"{prefix} scatter color variation range must be ordered and inside 0..1",
        )

    verifier.require_equal(f"{prefix}.scatter_chunk_report_instances", total_instances, 222)


def check_artifact(verifier: EvidenceVerifier, name: str, path: Path) -> None:
    if file_nonempty(path):
        verifier.pass_(name, f"{path} exists")
    else:
        verifier.missing(name, f"{path} is missing or empty")


def check_png_artifact(verifier: EvidenceVerifier, name: str, path: Path) -> None:
    if not file_nonempty(path):
        verifier.missing(name, f"{path} is missing or empty")
        return

    dimensions = png_dimensions(path)
    if dimensions is None:
        verifier.fail(name, f"{path} is not a valid PNG")
        return

    width, height = dimensions
    if width < MIN_SCREENSHOT_SIZE or height < MIN_SCREENSHOT_SIZE:
        verifier.fail(
            name,
            f"{path} is {width}x{height}, below {MIN_SCREENSHOT_SIZE}x{MIN_SCREENSHOT_SIZE}",
        )
        return

    try:
        luminance_min, luminance_max, sample_count = png_luminance_stats(path)
    except ValueError as error:
        verifier.fail(name, f"{path} cannot be inspected for visual content: {error}")
        return

    luminance_range = luminance_max - luminance_min
    if sample_count < 2 or luminance_range < MIN_SCREENSHOT_LUMINANCE_RANGE:
        verifier.fail(
            name,
            (
                f"{path} appears blank or placeholder-like: luminance range "
                f"{luminance_range}, required at least {MIN_SCREENSHOT_LUMINANCE_RANGE}"
            ),
        )
        return

    verifier.pass_(
        name,
        f"{path} is a valid {width}x{height} PNG with luminance range {luminance_range}",
    )


def check_profile_notes(verifier: EvidenceVerifier, path: Path) -> None:
    if not file_nonempty(path):
        verifier.missing("profile.notes", f"{path} is missing or empty")
        return

    text = path.read_text(encoding="utf-8-sig")
    stripped = text.strip()
    lower_text = stripped.lower()
    failed = False

    verifier.pass_("profile.notes.file", f"{path} loaded")

    if len(stripped) < 1000:
        verifier.fail("profile.notes.length", f"{path} is too short to be credible profile evidence")
        failed = True
    else:
        verifier.pass_("profile.notes.length", f"{path} is substantial enough for profile evidence")

    forbidden = [
        label
        for label, pattern in PROFILE_NOTE_FORBIDDEN_PATTERNS
        if re.search(pattern, text, flags=re.IGNORECASE)
    ]
    if forbidden:
        verifier.fail("profile.notes.unresolved_markers", f"{path} contains unresolved markers: {', '.join(forbidden)}")
        failed = True
    else:
        verifier.pass_("profile.notes.unresolved_markers", f"{path} contains no unresolved capture markers")

    missing_sections = [section for section in PROFILE_NOTE_REQUIRED_SECTIONS if section.lower() not in lower_text]
    if missing_sections:
        verifier.fail("profile.notes.sections", f"{path} is missing sections: {', '.join(missing_sections)}")
        failed = True
    else:
        verifier.pass_("profile.notes.sections", f"{path} contains required evidence sections")

    missing = [term for term in PROFILE_NOTE_REQUIRED_TERMS if term.lower() not in lower_text]
    if missing:
        verifier.fail("profile.notes.required_terms", f"{path} is missing required terms: {', '.join(missing)}")
        failed = True
    else:
        verifier.pass_("profile.notes.required_terms", f"{path} contains required engine evidence terms")

    if failed:
        return

    verifier.pass_("profile.notes", f"{path} contains Unity/Unreal profiling evidence")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--validation-root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--midori-report", type=Path)
    parser.add_argument("--summary", type=Path)
    parser.add_argument("--unity-report", type=Path)
    parser.add_argument("--unreal-dry-run-report", type=Path)
    parser.add_argument("--unreal-report", type=Path)
    parser.add_argument("--unity-import-screenshot", type=Path, default=DEFAULT_SCREENSHOTS["unity_import_screenshot"])
    parser.add_argument("--unity-density-screenshot", type=Path, default=DEFAULT_SCREENSHOTS["unity_density_screenshot"])
    parser.add_argument("--unreal-import-screenshot", type=Path, default=DEFAULT_SCREENSHOTS["unreal_import_screenshot"])
    parser.add_argument(
        "--unreal-foliage-settings-screenshot",
        type=Path,
        default=DEFAULT_SCREENSHOTS["unreal_foliage_settings_screenshot"],
    )
    parser.add_argument("--profile-notes", type=Path, default=DEFAULT_SCREENSHOTS["profile_notes"])
    parser.add_argument("--output", type=Path)
    parser.add_argument("--allow-pending", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = args.validation_root
    midori_report_path = args.midori_report or root / "forest_floor_midori_validation_report.json"
    summary_path = args.summary or root / "engine_validation_summary.json"
    unity_report_path = args.unity_report or root / "forest_floor_unity_import_report.json"
    unreal_dry_run_report_path = args.unreal_dry_run_report or root / "forest_floor_unreal_dry_run_report.json"
    unreal_report_path = args.unreal_report or root / "forest_floor_unreal_editor_report.json"
    package_dir = root / "forest_floor"

    verifier = EvidenceVerifier()

    midori_report = load_json(midori_report_path)
    if midori_report is None:
        verifier.missing("midori.report", f"{midori_report_path} is missing")
        manifest: dict[str, Any] | None = None
    else:
        verifier.pass_("midori.report", f"{midori_report_path} loaded")
        manifest = check_midori_report(verifier, midori_report)

    summary = load_json(summary_path)
    if summary is None:
        verifier.missing("summary.report", f"{summary_path} is missing")
    else:
        verifier.pass_("summary.report", f"{summary_path} loaded")
        check_summary(verifier, summary, root, manifest)

    if manifest is not None:
        unreal_dry_run_report = load_json(unreal_dry_run_report_path)
        if unreal_dry_run_report is None:
            verifier.missing("unreal_dry_run.report", f"{unreal_dry_run_report_path} is missing")
        else:
            verifier.pass_("unreal_dry_run.report", f"{unreal_dry_run_report_path} loaded")
            check_unreal_dry_run_report(verifier, unreal_dry_run_report, manifest, package_dir)

        unity_report = load_json(unity_report_path)
        if unity_report is None:
            verifier.missing("unity.report", f"{unity_report_path} is missing")
        else:
            verifier.pass_("unity.report", f"{unity_report_path} loaded")
            check_unity_report(verifier, unity_report, manifest, package_dir)

        unreal_report = load_json(unreal_report_path)
        if unreal_report is None:
            verifier.missing("unreal.editor_report", f"{unreal_report_path} is missing")
        else:
            verifier.pass_("unreal.editor_report", f"{unreal_report_path} loaded")
            check_unreal_report(verifier, unreal_report, manifest, package_dir)

    check_png_artifact(verifier, "unity.import_screenshot", args.unity_import_screenshot)
    check_png_artifact(verifier, "unity.density_screenshot", args.unity_density_screenshot)
    check_png_artifact(verifier, "unreal.import_screenshot", args.unreal_import_screenshot)
    check_png_artifact(
        verifier,
        "unreal.foliage_settings_screenshot",
        args.unreal_foliage_settings_screenshot,
    )
    check_profile_notes(verifier, args.profile_notes)

    result = {
        "status": verifier.overall_status(),
        "checks": [check.__dict__ for check in verifier.checks],
    }
    text = json.dumps(result, indent=2)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n", encoding="utf-8")
    print(text)

    if result["status"] == "passed":
        return 0
    return 0 if args.allow_pending else 1


if __name__ == "__main__":
    sys.exit(main())
