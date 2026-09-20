"""Unreal Editor helper for importing a validated Midori nature package.

Usage from the Unreal Python console:

    py "B:/workshop/trees/midori/integrations/unreal/midori_nature_importer.py" \
        "B:/workshop/trees/midori/target/midori_nature_validation_smoke" \
        "/Game/Midori/Imported"

Dry-run from CPython when Unreal Editor is not installed on the validation host:

    python integrations/unreal/midori_nature_importer.py \
        target/midori_nature_validation_smoke \
        --dry-run --report target/midori_unreal_dry_run_report.json

The script imports package files, reads Midori scatter binaries, writes an
import report under Saved/MidoriImportReports, and attempts to create foliage
type assets for LOD0 prototype meshes when the Unreal foliage Python classes
are available in the installed engine version.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import struct
import sys
from dataclasses import dataclass
from typing import Any


SCATTER_MAGIC = b"MDSI"
SCATTER_VERSION = 1
SCATTER_STRIDE = 32
SCATTER_RECORD = struct.Struct("<ffffffff")
SCATTER_HEADER_BYTES = 16
SCATTER_EPSILON = 0.001
FNV_OFFSET_BASIS = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
U64_MASK = 0xFFFFFFFFFFFFFFFF


@dataclass
class ScatterChunkReport:
    file: str
    layer_name: str
    kind: str
    chunk_x: int
    chunk_z: int
    instance_count: int
    bounds_min: list[float]
    bounds_max: list[float]
    position_min: list[float]
    position_max: list[float]
    yaw_min: float
    yaw_max: float
    height_min: float
    height_max: float
    width_min: float
    width_max: float
    phase_min: float
    phase_max: float
    color_variation_min: float
    color_variation_max: float
    file_checksum: str
    record_checksum: str


def import_midori_nature_package(
    package_dir: str,
    destination_root: str = "/Game/Midori/Imported",
    import_screenshot: str | None = None,
    foliage_settings_screenshot: str | None = None,
) -> dict[str, Any]:
    import unreal

    package_dir = os.path.abspath(package_dir)
    manifest = read_manifest(package_dir)
    validate_manifest(manifest)

    asset_name = sanitize_asset_name(manifest["asset_name"])
    destination_root = destination_root.rstrip("/")
    destination_path = f"{destination_root}/{asset_name}"

    files_to_import = collect_import_files(manifest)
    source_files = collect_source_files(manifest, "unreal_yminus_file")
    validate_import_files(package_dir, source_files)
    import_tasks = import_assets(unreal, package_dir, destination_path, files_to_import)

    foliage_summary = create_foliage_types(unreal, manifest, destination_path)
    screenshot_report = capture_unreal_screenshots(
        unreal,
        import_screenshot,
        foliage_settings_screenshot,
    )
    report = build_import_report(
        package_dir,
        manifest,
        destination_path,
        files_to_import,
        source_files,
        foliage_summary,
        dry_run=False,
        screenshots=screenshot_report,
        import_tasks=import_tasks,
    )
    report_path = write_report(unreal, asset_name, report)
    unreal.log(
        "Midori imported {asset} to {dest}: {chunks} binary chunks, "
        "{instances} instances, report {report}".format(
            asset=manifest["asset_name"],
            dest=destination_path,
            chunks=report["scatter_binary_chunks"],
            instances=report["scatter_binary_instances"],
            report=report_path,
        )
    )
    return report


def dry_run_midori_nature_package(
    package_dir: str,
    destination_root: str = "/Game/Midori/Imported",
) -> dict[str, Any]:
    package_dir = os.path.abspath(package_dir)
    manifest = read_manifest(package_dir)
    validate_manifest(manifest)

    asset_name = sanitize_asset_name(manifest["asset_name"])
    destination_path = f"{destination_root.rstrip('/')}/{asset_name}"
    files_to_import = collect_import_files(manifest)
    source_files = collect_source_files(manifest, "unreal_yminus_file")
    validate_import_files(package_dir, source_files)
    return build_import_report(
        package_dir,
        manifest,
        destination_path,
        files_to_import,
        source_files,
        foliage_summary=build_foliage_summary(manifest, "dry_run"),
        dry_run=True,
    )


def read_manifest(package_dir: str) -> dict[str, Any]:
    manifest_path = os.path.join(package_dir, "midori_nature.json")
    with open(manifest_path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def validate_manifest(manifest: dict[str, Any]) -> None:
    if manifest.get("schema_version") != 3:
        raise ValueError("Midori schema version 3 is required.")
    if manifest.get("shader_policy") != "preview_only":
        raise ValueError("Midori shader_policy must be preview_only.")
    if manifest.get("texture_pipeline") != "parked":
        raise ValueError("Midori texture_pipeline must be parked.")
    validate_material_parameters(manifest)
    validate_material_recipes(manifest)
    validate_engine_import_recipes(manifest)
    overlay_names = {
        overlay.get("name")
        for overlay in manifest.get("surface_overlays", [])
        if isinstance(overlay, dict)
    }
    for required_overlay in ("moss", "wetness", "cracks"):
        if required_overlay not in overlay_names:
            raise ValueError(
                "Midori surface_overlays must include moss, wetness, and cracks."
            )
    validate_prototype_surface_targets(manifest)

    terrain = manifest.get("terrain") or {}
    if terrain.get("height_max", 0.0) < terrain.get("height_min", 0.0):
        raise ValueError("Midori terrain height range is invalid.")

    scatter = manifest.get("scatter") or {}
    binary_format = scatter.get("binary_format") or {}
    if (
        binary_format.get("format") != "midori.scatter.bin.v1"
        or binary_format.get("header_bytes") != 16
        or binary_format.get("record_stride_bytes") != 32
        or binary_format.get("endian") != "little"
    ):
        raise ValueError("Midori binary scatter format is not v1.")


def validate_material_parameters(manifest: dict[str, Any]) -> None:
    required_by_slot = {
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
    sets = manifest.get("material_parameters") or []
    for slot, required_semantics in required_by_slot.items():
        material_set = next(
            (
                item
                for item in sets
                if isinstance(item, dict) and item.get("material_slot") == slot
            ),
            None,
        )
        if not material_set:
            raise ValueError(f"Midori material_parameters must include {slot}.")
        if material_set.get("runtime_policy") != "engine_native_static":
            raise ValueError(
                f"Midori material parameter set {material_set.get('parameter_set', '')} "
                "must use engine_native_static."
            )
        semantics = {
            parameter.get("semantic", "")
            for parameter in material_set.get("parameters", [])
            if isinstance(parameter, dict)
        }
        missing = sorted(required_semantics.difference(semantics))
        if missing:
            raise ValueError(
                f"Midori material parameter set {slot} is missing semantics: {missing}."
            )
        for parameter in material_set.get("parameters", []):
            if not isinstance(parameter, dict):
                continue
            for field in ("name", "semantic", "value_type", "source", "default_value"):
                if not parameter.get(field):
                    raise ValueError(
                        f"Midori material parameter set {slot} has an incomplete parameter."
                    )


def validate_material_recipes(manifest: dict[str, Any]) -> None:
    required_targets = {
        "terrain_surface": {"unity_terrain_material", "unreal_landscape_material"},
        "groundcover_foliage": {
            "unity_detail_mesh_material",
            "unreal_static_mesh_foliage_material",
        },
    }
    recipes = manifest.get("material_recipes") or []
    by_slot = {
        recipe.get("material_slot"): recipe
        for recipe in recipes
        if isinstance(recipe, dict)
    }
    for slot, targets in required_targets.items():
        recipe = by_slot.get(slot)
        if not isinstance(recipe, dict):
            raise ValueError(f"Midori material_recipes must include {slot}.")
        if recipe.get("runtime_policy") != "engine_native_static":
            raise ValueError(
                f"Midori material recipe {recipe.get('file', '')} must use engine_native_static."
            )
        file_name = recipe.get("file", "")
        if not file_name.startswith("materials/") or not file_name.endswith(".recipe.json"):
            raise ValueError(
                f"Midori material recipe for {slot} must live under materials/*.recipe.json."
            )
        actual_targets = set(recipe.get("engine_targets") or [])
        missing = sorted(targets.difference(actual_targets))
        if missing:
            raise ValueError(
                f"Midori material recipe {slot} is missing engine targets: {missing}."
            )


def validate_engine_import_recipes(manifest: dict[str, Any]) -> None:
    required = {
        "unity": ("mobile", {"Unity TerrainData", "GPU-instanced terrain detail mesh prefabs"}),
        "unreal": ("console", {"Unreal Landscape", "Static Mesh Foliage"}),
    }
    recipes = manifest.get("engine_import_recipes") or []
    by_engine = {
        recipe.get("engine"): recipe
        for recipe in recipes
        if isinstance(recipe, dict)
    }
    for engine, (profile, systems) in required.items():
        recipe = by_engine.get(engine)
        if not isinstance(recipe, dict):
            raise ValueError(f"Midori engine_import_recipes must include {engine}.")
        if recipe.get("profile") != profile:
            raise ValueError(
                f"Midori engine import recipe {engine} must target {profile}."
            )
        if recipe.get("runtime_policy") != "engine_native_static":
            raise ValueError(
                f"Midori engine import recipe {recipe.get('file', '')} must use engine_native_static."
            )
        file_name = recipe.get("file", "")
        if not file_name.startswith("engines/") or not file_name.endswith(".recipe.json"):
            raise ValueError(
                f"Midori engine import recipe for {engine} must live under engines/*.recipe.json."
            )
        actual_systems = set(recipe.get("expected_systems") or [])
        missing = sorted(systems.difference(actual_systems))
        if missing:
            raise ValueError(
                f"Midori engine import recipe {engine} is missing systems: {missing}."
            )


def validate_prototype_surface_targets(manifest: dict[str, Any]) -> None:
    for prototype in manifest.get("prototypes", []):
        if not isinstance(prototype, dict):
            continue
        targets = prototype.get("surface_targets") or []
        if not targets:
            raise ValueError(
                f"Midori prototype {prototype.get('name', '<unnamed>')} is missing surface target metadata."
            )
        for required in required_surface_targets_for_kind(prototype.get("kind", "")):
            if required not in targets:
                raise ValueError(
                    f"Midori prototype {prototype.get('name', '<unnamed>')} kind {prototype.get('kind', '')} "
                    f"must include surface target {required}."
                )


def required_surface_targets_for_kind(kind: str) -> tuple[str, ...]:
    if kind in ("grass", "flower", "weed", "litter"):
        return ("groundcover_foliage",)
    if kind == "moss":
        return ("groundcover_foliage", "moss_tuft")
    if kind == "shrub":
        return ("groundcover_foliage", "shrub_base")
    if kind == "rock":
        return ("static_surface", "rock")
    if kind == "log":
        return ("static_surface", "log")
    return ()


def collect_import_files(manifest: dict[str, Any]) -> list[str]:
    files: list[str] = [
        "midori_nature.json",
        "preview_tile.glb",
        manifest["terrain"]["heightmap_file"],
        manifest["terrain"]["masks_file"],
        manifest["terrain"]["grass_density_file"],
        manifest["normal_conventions"]["unreal_yminus_file"],
    ]
    for prototype in manifest.get("prototypes", []):
        for lod in prototype.get("lods", []):
            files.append(lod["file"])
    return sorted(set(files))


def import_map_files(manifest: dict[str, Any]) -> list[str]:
    return [
        manifest["terrain"]["heightmap_file"],
        manifest["terrain"]["masks_file"],
        manifest["terrain"]["grass_density_file"],
        manifest["normal_conventions"]["unreal_yminus_file"],
    ]


def prototype_lod_files(manifest: dict[str, Any]) -> list[str]:
    files: list[str] = []
    for prototype in manifest.get("prototypes", []):
        for lod in prototype.get("lods", []):
            files.append(lod["file"])
    return sorted(files)


def prototype_lod0_files(manifest: dict[str, Any]) -> list[str]:
    files: list[str] = []
    for prototype in manifest.get("prototypes", []):
        for lod in prototype.get("lods", []):
            if lod.get("index") == 0:
                files.append(lod["file"])
    return sorted(files)


def collect_source_files(manifest: dict[str, Any], normal_key: str) -> list[str]:
    files: list[str] = [
        "midori_nature.json",
        "preview_tile.glb",
        manifest["terrain"]["heightmap_file"],
        manifest["terrain"]["masks_file"],
        manifest["terrain"]["grass_density_file"],
        manifest["normal_conventions"][normal_key],
    ]
    scatter = manifest.get("scatter", {})
    scatter_json = scatter.get("file")
    if scatter_json:
        files.append(scatter_json)
    for item in scatter.get("binary_files", []):
        files.append(item["file"])
    for prototype in manifest.get("prototypes", []):
        for lod in prototype.get("lods", []):
            files.append(lod["file"])
    for recipe in manifest.get("material_recipes", []):
        files.append(recipe["file"])
    for recipe in manifest.get("engine_import_recipes", []):
        files.append(recipe["file"])
    return sorted(set(files))


def validate_import_files(package_dir: str, relative_files: list[str]) -> None:
    missing = [
        relative
        for relative in relative_files
        if not os.path.isfile(os.path.join(package_dir, relative))
    ]
    if missing:
        raise ValueError(f"Midori import source files missing: {', '.join(missing)}")


def build_import_report(
    package_dir: str,
    manifest: dict[str, Any],
    destination_path: str,
    files_to_import: list[str],
    source_files: list[str],
    foliage_summary: dict[str, Any],
    dry_run: bool,
    screenshots: dict[str, Any] | None = None,
    import_tasks: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    scatter_chunks = [
        read_scatter_chunk(package_dir, item)
        for item in manifest["scatter"].get("binary_files", [])
    ]
    scatter_instance_count = sum(chunk.instance_count for chunk in scatter_chunks)
    scatter_file_checksum_xor = 0
    scatter_record_checksum_xor = 0
    for chunk in scatter_chunks:
        scatter_file_checksum_xor ^= int(chunk.file_checksum, 16)
        scatter_record_checksum_xor ^= int(chunk.record_checksum, 16)
    import_tasks = import_tasks or build_import_task_reports(destination_path, files_to_import)

    return {
        "asset_name": manifest["asset_name"],
        "schema_version": manifest["schema_version"],
        "destination_path": destination_path,
        "dry_run": dry_run,
        "manifest_file_checksum": file_checksum_hex(os.path.join(package_dir, "midori_nature.json")),
        "source_file_count": len(source_files),
        "source_file_checksum_xor": source_file_checksum_xor(package_dir, source_files),
        "source_files": source_files,
        "tile_size_meters": manifest["tile_size"],
        "terrain": manifest["terrain"],
        "profile": "console",
        "unreal_hints": manifest.get("unreal", {}),
        "landscape_heightmap": (manifest.get("unreal") or {}).get("landscape_heightmap", ""),
        "landscape_weightmap": (manifest.get("unreal") or {}).get("landscape_weightmap", ""),
        "normal_map": (manifest.get("unreal") or {}).get("normal_map", ""),
        "foliage_mode": (manifest.get("unreal") or {}).get("foliage_mode", ""),
        "static_mesh_pipeline": (manifest.get("unreal") or {}).get("static_mesh_pipeline", ""),
        "console_density_scale": manifest["console"]["density_scale"],
        "console_lod0_max_distance_meters": manifest["console"].get("lod0_max_distance"),
        "console_lod1_max_distance_meters": manifest["console"].get("lod1_max_distance"),
        "console_lod2_max_distance_meters": manifest["console"].get("lod2_max_distance"),
        "console_material_slots": manifest["console"].get("material_slots"),
        "console_max_instances_per_tile": manifest["console"].get("max_instances_per_tile"),
        "console_max_instances_per_chunk": manifest["console"].get("max_instances_per_chunk"),
        "console_shadows": manifest["console"].get("shadows"),
        "console_grass_collision": manifest["console"].get("grass_collision"),
        "console_moss_collision": manifest["console"].get("moss_collision"),
        "console_cull_start_meters": manifest["console"]["cull_start"],
        "console_cull_end_meters": manifest["console"]["cull_end"],
        "console_cull_start_cm": meters_to_unreal_cm(manifest["console"]["cull_start"]),
        "console_cull_end_cm": meters_to_unreal_cm(manifest["console"]["cull_end"]),
        "prototypes_declared": len(manifest.get("prototypes", [])),
        "lod0_prototypes_declared": count_lod0_prototypes(manifest),
        "lod_files_declared": count_lod_files(manifest),
        "groundcover_kinds": sorted(
            {prototype.get("kind", "") for prototype in manifest.get("prototypes", [])}
        ),
        "prototype_surface_targets": prototype_surface_targets(manifest),
        "rock_prototype_surface_targets": prototype_surface_targets_for_kind(
            manifest,
            "rock",
        ),
        "log_prototype_surface_targets": prototype_surface_targets_for_kind(
            manifest,
            "log",
        ),
        "shrub_prototype_surface_targets": prototype_surface_targets_for_kind(
            manifest,
            "shrub",
        ),
        "material_slots_declared": len(manifest.get("material_slots", [])),
        "has_terrain_surface_material_slot": has_material_slot(manifest, "terrain_surface"),
        "has_groundcover_foliage_material_slot": has_material_slot(
            manifest,
            "groundcover_foliage",
        ),
        "groundcover_material": material_slot_report(manifest, "groundcover_foliage"),
        "material_parameters": manifest.get("material_parameters", []),
        "material_parameter_set_count": len(manifest.get("material_parameters", [])),
        "material_parameter_slots": [
            item.get("material_slot", "")
            for item in manifest.get("material_parameters", [])
        ],
        "material_parameter_runtime_policies": [
            item.get("runtime_policy", "")
            for item in manifest.get("material_parameters", [])
        ],
        "material_parameter_semantics": material_parameter_semantics(manifest),
        "groundcover_material_parameter_names": material_parameter_names_for_slot(
            manifest,
            "groundcover_foliage",
        ),
        "terrain_material_parameter_names": material_parameter_names_for_slot(
            manifest,
            "terrain_surface",
        ),
        "material_recipes": manifest.get("material_recipes", []),
        "material_recipe_count": len(manifest.get("material_recipes", [])),
        "material_recipe_files": [
            item.get("file", "") for item in manifest.get("material_recipes", [])
        ],
        "material_recipe_runtime_policies": [
            item.get("runtime_policy", "")
            for item in manifest.get("material_recipes", [])
        ],
        "material_recipe_engine_targets": material_recipe_engine_targets(manifest),
        "terrain_material_recipe_file": material_recipe_file_for_slot(
            manifest,
            "terrain_surface",
        ),
        "groundcover_material_recipe_file": material_recipe_file_for_slot(
            manifest,
            "groundcover_foliage",
        ),
        "engine_import_recipes": manifest.get("engine_import_recipes", []),
        "engine_import_recipe_count": len(manifest.get("engine_import_recipes", [])),
        "engine_import_recipe_files": [
            item.get("file", "") for item in manifest.get("engine_import_recipes", [])
        ],
        "engine_import_recipe_profiles": engine_import_recipe_profiles(manifest),
        "engine_import_recipe_runtime_policies": [
            item.get("runtime_policy", "")
            for item in manifest.get("engine_import_recipes", [])
        ],
        "engine_import_recipe_expected_systems": engine_import_recipe_expected_systems(
            manifest
        ),
        "unity_engine_import_recipe_file": engine_import_recipe_file_for_engine(
            manifest,
            "unity",
        ),
        "unreal_engine_import_recipe_file": engine_import_recipe_file_for_engine(
            manifest,
            "unreal",
        ),
        "surface_overlays": manifest.get("surface_overlays", []),
        "surface_overlay_count": len(manifest.get("surface_overlays", [])),
        "surface_overlay_names": [
            overlay.get("name", "")
            for overlay in manifest.get("surface_overlays", [])
        ],
        "surface_overlay_source_files": [
            overlay.get("source_file", "")
            for overlay in manifest.get("surface_overlays", [])
        ],
        "surface_overlay_channels": [
            overlay.get("channel", "")
            for overlay in manifest.get("surface_overlays", [])
        ],
        "surface_overlay_targets": [
            overlay.get("targets", [])
            for overlay in manifest.get("surface_overlays", [])
        ],
        "surface_overlay_runtime_policies": [
            overlay.get("runtime_policy", "")
            for overlay in manifest.get("surface_overlays", [])
        ],
        "wind_packing": manifest.get("wind_packing", {}),
        "screenshots": screenshots or {
            "import": {"path": "", "status": "not_requested"},
            "foliage_settings": {"path": "", "status": "not_requested"},
        },
        "imported_files": files_to_import,
        "import_task_count": len(import_tasks),
        "import_tasks": import_tasks,
        "import_task_files": [task["file"] for task in import_tasks],
        "import_task_destination_paths": [
            task["destination_path"] for task in import_tasks
        ],
        "import_task_extensions": sorted(
            {task["extension"] for task in import_tasks}
        ),
        "imported_manifest": "midori_nature.json" in files_to_import,
        "imported_preview_tile": "preview_tile.glb" in files_to_import,
        "imported_map_files": import_map_files(manifest),
        "imported_map_file_count": len(import_map_files(manifest)),
        "imported_prototype_lod_files": prototype_lod_files(manifest),
        "imported_prototype_lod_file_count": len(prototype_lod_files(manifest)),
        "imported_lod0_prototype_files": prototype_lod0_files(manifest),
        "imported_lod0_prototype_file_count": len(prototype_lod0_files(manifest)),
        "scatter_binary_chunks": len(scatter_chunks),
        "scatter_binary_instances": scatter_instance_count,
        "scatter_binary_records_validated": True,
        "scatter_binary_records_read": scatter_instance_count,
        "scatter_binary_file_checksum_xor": hex_u64(scatter_file_checksum_xor),
        "scatter_binary_record_checksum_xor": hex_u64(scatter_record_checksum_xor),
        "scatter_chunks": scatter_chunks,
        "foliage_type_status": foliage_summary["status"],
        "foliage_type_expected_count": foliage_summary["expected_count"],
        "foliage_type_count": foliage_summary["created_count"],
        "foliage_type_assets": foliage_summary["assets"],
        "foliage_cull_start_cm": foliage_summary["cull_start_cm"],
        "foliage_cull_end_cm": foliage_summary["cull_end_cm"],
        "foliage_warnings": foliage_summary["warnings"],
        "notes": [
            "Landscape creation is left to the project import pipeline.",
            "Use terrain.heightmap_file and terrain.height_min/height_max for landscape Z scale.",
            "Use terrain.masks_file as landscape weight/mask source.",
            "Use surface_overlays for static moss, wetness, and crack masks on terrain, rocks, and logs.",
            "No Midori runtime shader injection is required.",
        ],
    }


def capture_unreal_screenshots(
    unreal: Any,
    import_screenshot: str | None,
    foliage_settings_screenshot: str | None,
) -> dict[str, Any]:
    return {
        "import": capture_unreal_screenshot(unreal, import_screenshot, "import"),
        "foliage_settings": capture_unreal_screenshot(
            unreal,
            foliage_settings_screenshot,
            "foliage_settings",
        ),
    }


def capture_unreal_screenshot(
    unreal: Any,
    screenshot_path: str | None,
    label: str,
) -> dict[str, Any]:
    if not screenshot_path:
        report = {"path": "", "status": "not_requested", "label": label}
        report.update(summarize_screenshot_artifact(""))
        return report

    screenshot_path = os.path.abspath(screenshot_path)
    os.makedirs(os.path.dirname(screenshot_path), exist_ok=True)

    automation = getattr(unreal, "AutomationLibrary", None)
    take_high_res = getattr(automation, "take_high_res_screenshot", None) if automation else None
    if callable(take_high_res):
        try:
            result = take_high_res(1024, 1024, screenshot_path)
            report = {
                "path": screenshot_path,
                "status": "captured" if os.path.isfile(screenshot_path) else "requested",
                "label": label,
                "method": "AutomationLibrary.take_high_res_screenshot",
                "result": str(result),
            }
            report.update(summarize_screenshot_artifact(screenshot_path))
            return report
        except Exception as exc:
            unreal.log_warning(f"Midori could not capture {label} screenshot via AutomationLibrary: {exc}")

    system_library = getattr(unreal, "SystemLibrary", None)
    execute_console_command = (
        getattr(system_library, "execute_console_command", None)
        if system_library
        else None
    )
    if callable(execute_console_command):
        try:
            execute_console_command(None, f'HighResShot 1024x1024 filename="{screenshot_path}"')
            report = {
                "path": screenshot_path,
                "status": "captured" if os.path.isfile(screenshot_path) else "requested",
                "label": label,
                "method": "SystemLibrary.execute_console_command HighResShot",
            }
            report.update(summarize_screenshot_artifact(screenshot_path))
            return report
        except Exception as exc:
            unreal.log_warning(f"Midori could not request {label} screenshot via HighResShot: {exc}")

    unreal.log_warning(
        f"Midori could not capture {label} screenshot; no supported Unreal screenshot API was available."
    )
    report = {
        "path": screenshot_path,
        "status": "unavailable",
        "label": label,
        "method": "",
    }
    report.update(summarize_screenshot_artifact(screenshot_path))
    return report


def summarize_screenshot_artifact(path: str) -> dict[str, Any]:
    summary: dict[str, Any] = {
        "exists": False,
        "bytes": 0,
        "checksum": "",
        "width": 0,
        "height": 0,
    }
    if not path or not os.path.isfile(path):
        return summary

    summary["exists"] = True
    summary["bytes"] = os.path.getsize(path)
    summary["checksum"] = file_checksum_hex(path)
    dimensions = read_png_dimensions(path)
    if dimensions is not None:
        summary["width"], summary["height"] = dimensions
    return summary


def read_png_dimensions(path: str) -> tuple[int, int] | None:
    with open(path, "rb") as handle:
        header = handle.read(24)
    if (
        len(header) < 24
        or header[:8] != b"\x89PNG\r\n\x1a\n"
        or header[12:16] != b"IHDR"
    ):
        return None
    return struct.unpack(">II", header[16:24])


def count_lod0_prototypes(manifest: dict[str, Any]) -> int:
    return sum(
        1
        for prototype in manifest.get("prototypes", [])
        if any(lod.get("index") == 0 for lod in prototype.get("lods", []))
    )


def count_lod_files(manifest: dict[str, Any]) -> int:
    return sum(len(prototype.get("lods", [])) for prototype in manifest.get("prototypes", []))


def has_material_slot(manifest: dict[str, Any], name: str) -> bool:
    return any(slot.get("name") == name for slot in manifest.get("material_slots", []))


def prototype_surface_targets(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    return [
        {
            "name": prototype.get("name", ""),
            "kind": prototype.get("kind", ""),
            "targets": list(prototype.get("surface_targets") or []),
        }
        for prototype in manifest.get("prototypes", [])
    ]


def prototype_surface_targets_for_kind(manifest: dict[str, Any], kind: str) -> list[str]:
    targets: set[str] = set()
    for prototype in manifest.get("prototypes", []):
        if prototype.get("kind") != kind:
            continue
        targets.update(prototype.get("surface_targets") or [])
    return sorted(targets)


def material_slot_report(manifest: dict[str, Any], name: str) -> dict[str, Any]:
    for slot in manifest.get("material_slots", []):
        if slot.get("name") == name:
            return {
                "name": slot.get("name", ""),
                "alpha_mode": slot.get("alpha_mode", ""),
                "double_sided": bool(slot.get("double_sided")),
                "shadows": bool(slot.get("shadows")),
            }
    return {}


def material_parameter_semantics(manifest: dict[str, Any]) -> list[str]:
    values: list[str] = []
    for material_set in manifest.get("material_parameters", []):
        slot = material_set.get("material_slot", "")
        for parameter in material_set.get("parameters", []):
            values.append(f"{slot}:{parameter.get('semantic', '')}")
    return values


def material_parameter_names_for_slot(manifest: dict[str, Any], slot: str) -> list[str]:
    names: set[str] = set()
    for material_set in manifest.get("material_parameters", []):
        if material_set.get("material_slot") != slot:
            continue
        for parameter in material_set.get("parameters", []):
            names.add(parameter.get("name", ""))
    return sorted(name for name in names if name)


def material_recipe_engine_targets(manifest: dict[str, Any]) -> list[str]:
    values: list[str] = []
    for recipe in manifest.get("material_recipes", []):
        slot = recipe.get("material_slot", "")
        for target in recipe.get("engine_targets", []):
            values.append(f"{slot}:{target}")
    return values


def material_recipe_file_for_slot(manifest: dict[str, Any], slot: str) -> str:
    for recipe in manifest.get("material_recipes", []):
        if recipe.get("material_slot") == slot:
            return recipe.get("file", "")
    return ""


def engine_import_recipe_profiles(manifest: dict[str, Any]) -> list[str]:
    return [
        f"{recipe.get('engine', '')}:{recipe.get('profile', '')}"
        for recipe in manifest.get("engine_import_recipes", [])
    ]


def engine_import_recipe_expected_systems(manifest: dict[str, Any]) -> list[str]:
    values: list[str] = []
    for recipe in manifest.get("engine_import_recipes", []):
        engine = recipe.get("engine", "")
        for system in recipe.get("expected_systems", []):
            values.append(f"{engine}:{system}")
    return values


def engine_import_recipe_file_for_engine(manifest: dict[str, Any], engine: str) -> str:
    for recipe in manifest.get("engine_import_recipes", []):
        if recipe.get("engine") == engine:
            return recipe.get("file", "")
    return ""


def build_foliage_summary(
    manifest: dict[str, Any],
    status: str,
    assets: list[str] | None = None,
    warnings: list[str] | None = None,
) -> dict[str, Any]:
    assets = assets or []
    return {
        "status": status,
        "expected_count": count_lod0_prototypes(manifest),
        "created_count": len(assets),
        "assets": assets,
        "cull_start_cm": meters_to_unreal_cm(manifest["console"]["cull_start"]),
        "cull_end_cm": meters_to_unreal_cm(manifest["console"]["cull_end"]),
        "warnings": warnings or [],
    }


def build_import_task_reports(
    destination_path: str,
    relative_files: list[str],
) -> list[dict[str, Any]]:
    reports: list[dict[str, Any]] = []
    for relative in relative_files:
        destination_subdir = os.path.dirname(relative).replace("\\", "/")
        reports.append(
            {
                "file": relative,
                "destination_path": f"{destination_path}/{destination_subdir}".rstrip("/"),
                "extension": os.path.splitext(relative)[1].lower(),
            }
        )
    return reports


def import_assets(
    unreal: Any,
    package_dir: str,
    destination_path: str,
    relative_files: list[str],
) -> list[dict[str, Any]]:
    task_reports = build_import_task_reports(destination_path, relative_files)
    tasks = []
    for task_report in task_reports:
        relative = task_report["file"]
        source = os.path.join(package_dir, relative)
        if not os.path.isfile(source):
            unreal.log_warning(f"Midori import source file missing: {source}")
            continue

        task = unreal.AssetImportTask()
        task.set_editor_property("filename", source)
        task.set_editor_property("destination_path", task_report["destination_path"])
        task.set_editor_property("automated", True)
        task.set_editor_property("replace_existing", False)
        task.set_editor_property("save", True)
        tasks.append(task)

    if tasks:
        unreal.AssetToolsHelpers.get_asset_tools().import_asset_tasks(tasks)
    return task_reports


def read_scatter_chunk(
    package_dir: str,
    manifest_file: dict[str, Any],
) -> ScatterChunkReport:
    path = os.path.join(package_dir, manifest_file["file"])
    with open(path, "rb") as handle:
        data = handle.read()

    if len(data) < SCATTER_HEADER_BYTES:
        raise ValueError(f"{manifest_file['file']} is too short.")
    magic = data[:4]
    if magic != SCATTER_MAGIC:
        raise ValueError(f"{manifest_file['file']} has invalid Midori scatter magic.")
    version, stride, count = struct.unpack("<III", data[4:SCATTER_HEADER_BYTES])
    if version != SCATTER_VERSION or stride != SCATTER_STRIDE:
        raise ValueError(f"{manifest_file['file']} has an unsupported scatter header.")
    if count != manifest_file["instance_count"]:
        raise ValueError(f"{manifest_file['file']} count does not match the manifest.")
    if count == 0:
        raise ValueError(f"{manifest_file['file']} must not be empty.")

    expected_length = SCATTER_HEADER_BYTES + count * SCATTER_STRIDE
    if len(data) != expected_length:
        raise ValueError(
            f"{manifest_file['file']} length {len(data)} != {expected_length}."
        )

    position_min = [math.inf, math.inf, math.inf]
    position_max = [-math.inf, -math.inf, -math.inf]
    yaw_min = math.inf
    yaw_max = -math.inf
    height_min = math.inf
    height_max = -math.inf
    width_min = math.inf
    width_max = -math.inf
    phase_min = math.inf
    phase_max = -math.inf
    color_variation_min = math.inf
    color_variation_max = -math.inf
    record_checksum = FNV_OFFSET_BASIS

    for index in range(count):
        offset = SCATTER_HEADER_BYTES + index * SCATTER_STRIDE
        record_bytes = data[offset : offset + SCATTER_STRIDE]
        record = SCATTER_RECORD.unpack(record_bytes)
        validate_scatter_record(record, manifest_file)

        x, y, z, yaw, height, width, phase, color_variation = record
        position_min[0] = min(position_min[0], x)
        position_min[1] = min(position_min[1], y)
        position_min[2] = min(position_min[2], z)
        position_max[0] = max(position_max[0], x)
        position_max[1] = max(position_max[1], y)
        position_max[2] = max(position_max[2], z)
        yaw_min = min(yaw_min, yaw)
        yaw_max = max(yaw_max, yaw)
        height_min = min(height_min, height)
        height_max = max(height_max, height)
        width_min = min(width_min, width)
        width_max = max(width_max, width)
        phase_min = min(phase_min, phase)
        phase_max = max(phase_max, phase)
        color_variation_min = min(color_variation_min, color_variation)
        color_variation_max = max(color_variation_max, color_variation)
        record_checksum = fnv1a(record_bytes, record_checksum)

    return ScatterChunkReport(
        file=manifest_file["file"],
        layer_name=manifest_file["layer_name"],
        kind=manifest_file["kind"],
        chunk_x=manifest_file["chunk_x"],
        chunk_z=manifest_file["chunk_z"],
        instance_count=count,
        bounds_min=manifest_file["bounds_min"],
        bounds_max=manifest_file["bounds_max"],
        position_min=position_min,
        position_max=position_max,
        yaw_min=yaw_min,
        yaw_max=yaw_max,
        height_min=height_min,
        height_max=height_max,
        width_min=width_min,
        width_max=width_max,
        phase_min=phase_min,
        phase_max=phase_max,
        color_variation_min=color_variation_min,
        color_variation_max=color_variation_max,
        file_checksum=hex_u64(fnv1a(data)),
        record_checksum=hex_u64(record_checksum),
    )


def validate_scatter_record(record: tuple[float, ...], manifest_file: dict[str, Any]) -> None:
    x, y, z, yaw, height, width, phase, color_variation = record
    if any(not math.isfinite(value) for value in record):
        raise ValueError(f"{manifest_file['file']} contains a non-finite instance.")
    bounds_min = manifest_file["bounds_min"]
    bounds_max = manifest_file["bounds_max"]
    if not (
        bounds_min[0] - SCATTER_EPSILON <= x <= bounds_max[0] + SCATTER_EPSILON
        and bounds_min[1] - SCATTER_EPSILON <= y <= bounds_max[1] + SCATTER_EPSILON
        and bounds_min[2] - SCATTER_EPSILON <= z <= bounds_max[2] + SCATTER_EPSILON
    ):
        raise ValueError(f"{manifest_file['file']} contains an out-of-bounds instance.")
    if not 0.0 <= yaw <= math.tau:
        raise ValueError(f"{manifest_file['file']} contains invalid yaw.")
    if height <= 0.0 or width <= 0.0:
        raise ValueError(f"{manifest_file['file']} contains a non-positive scale.")
    if not 0.0 <= phase <= math.tau:
        raise ValueError(f"{manifest_file['file']} contains invalid phase.")
    if not 0.0 <= color_variation <= 1.0:
        raise ValueError(f"{manifest_file['file']} contains invalid color variation.")


def fnv1a(data: bytes, seed: int = FNV_OFFSET_BASIS) -> int:
    value = seed
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) & U64_MASK
    return value


def hex_u64(value: int) -> str:
    return f"0x{value & U64_MASK:016x}"


def file_checksum_hex(path: str) -> str:
    with open(path, "rb") as handle:
        return hex_u64(fnv1a(handle.read()))


def source_file_checksum_xor(package_dir: str, source_files: list[str]) -> str:
    checksum = 0
    for relative in source_files:
        with open(os.path.join(package_dir, relative), "rb") as handle:
            checksum ^= fnv1a(handle.read())
    return hex_u64(checksum)


def create_foliage_types(
    unreal: Any,
    manifest: dict[str, Any],
    destination_path: str,
) -> dict[str, Any]:
    warnings: list[str] = []
    foliage_class = getattr(unreal, "FoliageType_InstancedStaticMesh", None)
    foliage_factory_class = getattr(unreal, "FoliageType_InstancedStaticMeshFactory", None)
    if foliage_class is None or foliage_factory_class is None:
        warning = (
            "FoliageType_InstancedStaticMesh Python factory is unavailable in this engine; "
            "skipping foliage type asset creation."
        )
        warnings.append(warning)
        unreal.log_warning(warning)
        return build_foliage_summary(manifest, "unavailable", warnings=warnings)

    editor_asset_library = getattr(unreal, "EditorAssetLibrary", None)
    load_asset = getattr(editor_asset_library, "load_asset", None) if editor_asset_library else None
    save_loaded_asset = (
        getattr(editor_asset_library, "save_loaded_asset", None)
        if editor_asset_library
        else None
    )
    if not callable(load_asset) or not callable(save_loaded_asset):
        warning = "EditorAssetLibrary load/save helpers are unavailable; skipping foliage type asset creation."
        warnings.append(warning)
        unreal.log_warning(warning)
        return build_foliage_summary(manifest, "unavailable", warnings=warnings)

    foliage_dir = f"{destination_path}/FoliageTypes"
    asset_tools = unreal.AssetToolsHelpers.get_asset_tools()
    created_assets: list[str] = []
    cull_start = meters_to_unreal_cm(manifest["console"]["cull_start"])
    cull_end = meters_to_unreal_cm(manifest["console"]["cull_end"])

    for prototype in manifest.get("prototypes", []):
        lod0 = next((lod for lod in prototype.get("lods", []) if lod.get("index") == 0), None)
        if lod0 is None:
            continue

        mesh_path = asset_object_path(destination_path, lod0["file"])
        mesh = load_asset(mesh_path)
        if mesh is None:
            warning = f"Could not load Midori LOD0 mesh for foliage type: {mesh_path}"
            warnings.append(warning)
            unreal.log_warning(warning)
            continue

        asset_name = sanitize_asset_name(f"{prototype['name']}_{prototype['kind']}_FoliageType")
        foliage_type = asset_tools.create_asset(
            asset_name,
            foliage_dir,
            foliage_class,
            foliage_factory_class(),
        )
        if foliage_type is None:
            warning = f"Could not create foliage type asset: {asset_name}"
            warnings.append(warning)
            unreal.log_warning(warning)
            continue

        try_set_editor_property(unreal, foliage_type, "mesh", mesh)
        try_set_editor_property(unreal, foliage_type, "cast_shadow", bool(manifest["console"]["shadows"]))
        try_set_editor_property(unreal, foliage_type, "density", float(manifest["console"]["density_scale"]))
        try:
            try_set_editor_property(
                unreal,
                foliage_type,
                "cull_distance",
                unreal.Int32Interval(cull_start, cull_end),
            )
        except Exception as exc:
            warning = f"Could not create Unreal cull distance interval: {exc}"
            warnings.append(warning)
            unreal.log_warning(warning)
        try_set_editor_property(unreal, foliage_type, "start_cull_distance", cull_start)
        try_set_editor_property(unreal, foliage_type, "end_cull_distance", cull_end)
        save_loaded_asset(foliage_type)
        created_assets.append(f"{foliage_dir}/{asset_name}")

    expected_count = count_lod0_prototypes(manifest)
    status = "created" if len(created_assets) == expected_count else "partial"
    if len(created_assets) == 0:
        status = "not_created"
    return build_foliage_summary(manifest, status, created_assets, warnings)


def try_set_editor_property(unreal: Any, asset: Any, name: str, value: Any) -> None:
    try:
        asset.set_editor_property(name, value)
    except Exception as exc:  # Unreal properties vary across engine versions.
        unreal.log_warning(f"Could not set {name} on {asset.get_name()}: {exc}")


def asset_object_path(destination_path: str, relative_file: str) -> str:
    directory = os.path.dirname(relative_file).replace("\\", "/")
    name = os.path.splitext(os.path.basename(relative_file))[0]
    return f"{destination_path}/{directory}/{name}".rstrip("/")


def write_report(unreal: Any, asset_name: str, report: dict[str, Any]) -> str:
    report_dir = os.path.join(unreal.Paths.project_saved_dir(), "MidoriImportReports")
    os.makedirs(report_dir, exist_ok=True)
    report_path = os.path.join(report_dir, f"{sanitize_asset_name(asset_name)}.json")
    write_report_file(report_path, report)
    return report_path


def write_report_file(report_path: str, report: dict[str, Any]) -> None:
    report_dir = os.path.dirname(os.path.abspath(report_path))
    if report_dir:
        os.makedirs(report_dir, exist_ok=True)
    with open(report_path, "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2, default=dataclass_to_dict)


def dataclass_to_dict(value: Any) -> Any:
    if hasattr(value, "__dataclass_fields__"):
        return value.__dict__
    raise TypeError(f"{type(value).__name__} is not JSON serializable")


def sanitize_asset_name(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9_]+", "_", value).strip("_") or "MidoriNature"


def meters_to_unreal_cm(value: float) -> int:
    return int(round(value * 100.0))


def main(argv: list[str]) -> None:
    parser = argparse.ArgumentParser(
        description="Import or dry-run-validate a Midori nature package for Unreal."
    )
    parser.add_argument("package_dir")
    parser.add_argument("destination_root", nargs="?", default="/Game/Midori/Imported")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--report")
    parser.add_argument("--import-screenshot")
    parser.add_argument("--foliage-settings-screenshot")
    args = parser.parse_args(argv)

    if args.dry_run:
        report = dry_run_midori_nature_package(args.package_dir, args.destination_root)
        if args.report:
            write_report_file(args.report, report)
        else:
            print(json.dumps(report, indent=2, default=dataclass_to_dict))
        return

    report = import_midori_nature_package(
        args.package_dir,
        args.destination_root,
        import_screenshot=args.import_screenshot,
        foliage_settings_screenshot=args.foliage_settings_screenshot,
    )
    if args.report:
        write_report_file(args.report, report)


if __name__ == "__main__":
    main(sys.argv[1:])
