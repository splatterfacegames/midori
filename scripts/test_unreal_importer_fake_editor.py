#!/usr/bin/env python3
"""Exercise the Unreal editor import path with a small fake Unreal module."""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import struct
import sys
import tempfile
import zlib
from pathlib import Path
from typing import Any


class FakeAssetImportTask:
    def __init__(self) -> None:
        self.properties: dict[str, Any] = {}

    def set_editor_property(self, name: str, value: Any) -> None:
        self.properties[name] = value


class FakeAssetTools:
    def __init__(self) -> None:
        self.imported_tasks: list[FakeAssetImportTask] = []
        self.created_assets: list[FakeFoliageType] = []

    def import_asset_tasks(self, tasks: list[FakeAssetImportTask]) -> None:
        self.imported_tasks.extend(tasks)

    def create_asset(
        self,
        asset_name: str,
        package_path: str,
        asset_class: type,
        factory: Any,
    ) -> Any:
        asset = FakeFoliageType(asset_name, package_path, asset_class, factory)
        self.created_assets.append(asset)
        return asset


class FakeAssetToolsHelpers:
    asset_tools = FakeAssetTools()

    @staticmethod
    def get_asset_tools() -> FakeAssetTools:
        return FakeAssetToolsHelpers.asset_tools


class FakePaths:
    project_saved_dir_value = ""

    @staticmethod
    def project_saved_dir() -> str:
        return FakePaths.project_saved_dir_value


def write_png(path: str, width: int = 256, height: int = 256) -> None:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    max_x = max(1, width - 1)
    max_y = max(1, height - 1)
    max_sum = max(1, width + height - 2)
    raw_rows = bytearray()
    for y in range(height):
        raw_rows.append(0)
        for x in range(width):
            raw_rows.extend(
                (
                    31 + (x * 91 // max_x),
                    63 + (y * 96 // max_y),
                    38 + ((x + y) * 64 // max_sum),
                    255,
                )
            )
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw_rows)))
        + chunk(b"IEND", b"")
    )
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_bytes(png)


def file_checksum_hex(path: Path) -> str:
    checksum = 0xCBF29CE484222325
    for byte in path.read_bytes():
        checksum ^= byte
        checksum = (checksum * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"0x{checksum:016x}"


def png_artifact_metrics(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    if len(data) < 33 or data[:8] != b"\x89PNG\r\n\x1a\n":
        raise AssertionError(f"{path} is not a PNG")
    if data[12:16] != b"IHDR":
        raise AssertionError(f"{path} is missing IHDR")
    width, height, bit_depth, color_type, compression, filter_method, interlace = struct.unpack(
        ">IIBBBBB",
        data[16:29],
    )
    if bit_depth != 8 or color_type != 6 or compression != 0 or filter_method != 0 or interlace != 0:
        raise AssertionError(f"{path} is not an 8-bit non-interlaced RGBA PNG")

    offset = 8
    idat = bytearray()
    while offset + 8 <= len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        chunk_start = offset + 8
        chunk_end = chunk_start + length
        if chunk_end + 4 > len(data):
            raise AssertionError(f"{path} has a truncated PNG chunk")
        if kind == b"IDAT":
            idat.extend(data[chunk_start:chunk_end])
        elif kind == b"IEND":
            break
        offset = chunk_end + 4
    if not idat:
        raise AssertionError(f"{path} is missing IDAT data")

    raw = zlib.decompress(bytes(idat))
    stride = width * 4
    expected = (stride + 1) * height
    if len(raw) != expected:
        raise AssertionError(f"{path} has unexpected decompressed size {len(raw)} != {expected}")

    luminance_min = 255
    luminance_max = 0
    sample_count = 0
    for y in range(height):
        row_start = y * (stride + 1)
        if raw[row_start] != 0:
            raise AssertionError(f"{path} uses unsupported PNG filter {raw[row_start]}")
        row = raw[row_start + 1 : row_start + 1 + stride]
        for x in range(0, len(row), 4):
            r, g, b = row[x], row[x + 1], row[x + 2]
            luminance = (299 * r + 587 * g + 114 * b) // 1000
            luminance_min = min(luminance_min, luminance)
            luminance_max = max(luminance_max, luminance)
            sample_count += 1

    return {
        "exists": path.is_file(),
        "bytes": len(data),
        "checksum": file_checksum_hex(path),
        "width": width,
        "height": height,
        "sample_count": sample_count,
        "luminance_min": luminance_min,
        "luminance_max": luminance_max,
        "luminance_range": luminance_max - luminance_min,
    }


class FakeAutomationLibrary:
    @staticmethod
    def take_high_res_screenshot(width: int, height: int, filename: str) -> bool:
        write_png(filename, width, height)
        return True


class FakeStaticMesh:
    def __init__(self, object_path: str) -> None:
        self.object_path = object_path


class FakeFoliageType:
    def __init__(self, name: str, package_path: str, asset_class: type, factory: Any) -> None:
        self.name = name
        self.package_path = package_path
        self.asset_class = asset_class
        self.factory = factory
        self.properties: dict[str, Any] = {}
        self.saved = False

    def set_editor_property(self, name: str, value: Any) -> None:
        self.properties[name] = value

    def get_name(self) -> str:
        return self.name


class FakeFoliageTypeInstancedStaticMesh:
    pass


class FakeFoliageTypeInstancedStaticMeshFactory:
    pass


class FakeInt32Interval:
    def __init__(self, minimum: int, maximum: int) -> None:
        self.minimum = minimum
        self.maximum = maximum


class FakeEditorAssetLibrary:
    saved_assets: list[FakeFoliageType] = []

    @staticmethod
    def load_asset(object_path: str) -> FakeStaticMesh:
        return FakeStaticMesh(object_path)

    @staticmethod
    def save_loaded_asset(asset: FakeFoliageType) -> bool:
        asset.saved = True
        FakeEditorAssetLibrary.saved_assets.append(asset)
        return True


class FakeUnrealModule:
    AssetImportTask = FakeAssetImportTask
    AssetToolsHelpers = FakeAssetToolsHelpers
    AutomationLibrary = FakeAutomationLibrary
    EditorAssetLibrary = FakeEditorAssetLibrary
    FoliageType_InstancedStaticMesh = FakeFoliageTypeInstancedStaticMesh
    FoliageType_InstancedStaticMeshFactory = FakeFoliageTypeInstancedStaticMeshFactory
    Int32Interval = FakeInt32Interval
    Paths = FakePaths
    warnings: list[str] = []
    logs: list[str] = []

    @staticmethod
    def log(message: str) -> None:
        FakeUnrealModule.logs.append(message)

    @staticmethod
    def log_warning(message: str) -> None:
        FakeUnrealModule.warnings.append(message)


def load_importer(repo_root: Path) -> Any:
    path = repo_root / "integrations" / "unreal" / "midori_nature_importer.py"
    spec = importlib.util.spec_from_file_location("midori_nature_importer_under_test", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Could not load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, default=Path("target/midori_engine_validation/forest_floor"))
    parser.add_argument("--report", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[1]
    package_dir = (repo_root / args.package).resolve()
    if not package_dir.is_dir():
        raise SystemExit(f"Package directory does not exist: {package_dir}")

    importer = load_importer(repo_root)
    previous_unreal = sys.modules.get("unreal")
    FakeAssetToolsHelpers.asset_tools = FakeAssetTools()
    FakeEditorAssetLibrary.saved_assets = []
    FakeUnrealModule.warnings = []
    FakeUnrealModule.logs = []
    sys.modules["unreal"] = FakeUnrealModule
    try:
        with tempfile.TemporaryDirectory(prefix="midori_fake_unreal_") as saved_dir:
            FakePaths.project_saved_dir_value = saved_dir
            import_screenshot = str(Path(saved_dir) / "fake_import.png")
            foliage_screenshot = str(Path(saved_dir) / "fake_foliage.png")
            report = importer.import_midori_nature_package(
                str(package_dir),
                "/Game/Midori/FakeEditor",
                import_screenshot=import_screenshot,
                foliage_settings_screenshot=foliage_screenshot,
            )
            if not Path(import_screenshot).is_file():
                raise AssertionError("fake editor import did not write import screenshot")
            if not Path(foliage_screenshot).is_file():
                raise AssertionError("fake editor import did not write foliage screenshot")
            screenshot_metrics = {
                "import": png_artifact_metrics(Path(import_screenshot)),
                "foliage_settings": png_artifact_metrics(Path(foliage_screenshot)),
            }
    finally:
        if previous_unreal is None:
            sys.modules.pop("unreal", None)
        else:
            sys.modules["unreal"] = previous_unreal

    if report["dry_run"]:
        raise AssertionError("fake editor import unexpectedly produced a dry-run report")
    if report["scatter_binary_chunks"] != 26:
        raise AssertionError(f"unexpected chunk count: {report['scatter_binary_chunks']}")
    if report["scatter_binary_instances"] != 222:
        raise AssertionError(f"unexpected instance count: {report['scatter_binary_instances']}")
    manifest = json.loads((package_dir / "midori_nature.json").read_text(encoding="utf-8"))
    expected_source_files = importer.collect_source_files(manifest, "unreal_yminus_file")
    if report.get("source_files") != expected_source_files:
        raise AssertionError("fake editor import did not report the expected source file list")
    if report.get("source_file_count") != len(expected_source_files):
        raise AssertionError("fake editor import did not report the expected source file count")
    expected_manifest_checksum = importer.file_checksum_hex(str(package_dir / "midori_nature.json"))
    expected_source_checksum = importer.source_file_checksum_xor(
        str(package_dir),
        expected_source_files,
    )
    if report.get("manifest_file_checksum") != expected_manifest_checksum:
        raise AssertionError("fake editor import did not report manifest file checksum")
    if report.get("source_file_checksum_xor") != expected_source_checksum:
        raise AssertionError("fake editor import did not report source file checksum XOR")
    expected_import_files = importer.collect_import_files(manifest)
    def expected_destination(relative: str) -> str:
        parent = Path(relative).parent.as_posix()
        return report["destination_path"] if parent == "." else f"{report['destination_path']}/{parent}"

    expected_import_destinations = [
        expected_destination(relative)
        for relative in expected_import_files
    ]
    if report.get("imported_files") != expected_import_files:
        raise AssertionError("fake editor import did not report expected imported files")
    if report.get("import_task_count") != len(expected_import_files):
        raise AssertionError("fake editor import did not report expected import task count")
    if report.get("import_task_files") != expected_import_files:
        raise AssertionError("fake editor import did not report expected import task files")
    if report.get("import_task_destination_paths") != expected_import_destinations:
        raise AssertionError("fake editor import did not report expected import destinations")
    if report.get("import_task_extensions") != sorted(
        {Path(relative).suffix.lower() for relative in expected_import_files}
    ):
        raise AssertionError("fake editor import did not report expected import extensions")
    if report.get("imported_manifest") is not True:
        raise AssertionError("fake editor import did not report manifest import coverage")
    if report.get("imported_preview_tile") is not True:
        raise AssertionError("fake editor import did not report preview import coverage")
    expected_map_files = importer.import_map_files(manifest)
    if report.get("imported_map_files") != expected_map_files:
        raise AssertionError("fake editor import did not report expected map import files")
    if report.get("imported_map_file_count") != len(expected_map_files):
        raise AssertionError("fake editor import did not report expected map import count")
    expected_lod_files = importer.prototype_lod_files(manifest)
    if report.get("imported_prototype_lod_files") != expected_lod_files:
        raise AssertionError("fake editor import did not report expected prototype LOD files")
    if report.get("imported_prototype_lod_file_count") != len(expected_lod_files):
        raise AssertionError("fake editor import did not report expected prototype LOD count")
    expected_lod0_files = importer.prototype_lod0_files(manifest)
    if report.get("imported_lod0_prototype_files") != expected_lod0_files:
        raise AssertionError("fake editor import did not report expected LOD0 prototype files")
    if report.get("imported_lod0_prototype_file_count") != len(expected_lod0_files):
        raise AssertionError("fake editor import did not report expected LOD0 prototype count")
    import_tasks = report.get("import_tasks")
    if not isinstance(import_tasks, list) or len(import_tasks) != len(expected_import_files):
        raise AssertionError("fake editor import did not report every import task")
    for task_report, relative, destination in zip(
        import_tasks,
        expected_import_files,
        expected_import_destinations,
    ):
        if task_report.get("file") != relative:
            raise AssertionError(f"fake editor import task file mismatch for {relative}")
        if task_report.get("destination_path") != destination:
            raise AssertionError(f"fake editor import task destination mismatch for {relative}")
        if task_report.get("extension") != Path(relative).suffix.lower():
            raise AssertionError(f"fake editor import task extension mismatch for {relative}")
    actual_import_tasks = FakeAssetToolsHelpers.asset_tools.imported_tasks
    if len(actual_import_tasks) != len(expected_import_files):
        raise AssertionError("fake editor did not enqueue every expected import task")
    for task, relative, destination in zip(
        actual_import_tasks,
        expected_import_files,
        expected_import_destinations,
    ):
        properties = task.properties
        if Path(properties.get("filename", "")).resolve() != package_dir / relative:
            raise AssertionError(f"fake editor import task filename mismatch for {relative}")
        if properties.get("destination_path") != destination:
            raise AssertionError(f"fake editor import task destination mismatch for {relative}")
        if properties.get("automated") is not True:
            raise AssertionError(f"fake editor import task is not automated for {relative}")
        if properties.get("replace_existing") is not False:
            raise AssertionError(f"fake editor import task replace policy mismatch for {relative}")
        if properties.get("save") is not True:
            raise AssertionError(f"fake editor import task save flag mismatch for {relative}")
    for field in [
        "lod0_max_distance",
        "lod1_max_distance",
        "lod2_max_distance",
    ]:
        report_field = f"console_{field}_meters"
        if report.get(report_field) != manifest["console"][field]:
            raise AssertionError(f"fake editor import did not report {report_field}")
    if report.get("console_max_instances_per_tile") != manifest["console"]["max_instances_per_tile"]:
        raise AssertionError("fake editor import did not report console tile instance budget")
    if report.get("console_max_instances_per_chunk") != manifest["console"]["max_instances_per_chunk"]:
        raise AssertionError("fake editor import did not report console chunk instance budget")
    if report.get("console_grass_collision") != manifest["console"]["grass_collision"]:
        raise AssertionError("fake editor import did not report console grass collision policy")
    if report.get("console_moss_collision") != manifest["console"]["moss_collision"]:
        raise AssertionError("fake editor import did not report console moss collision policy")
    expected_material_sets = manifest.get("material_parameters", [])
    if report.get("material_parameter_set_count") != len(expected_material_sets):
        raise AssertionError("fake editor import did not report material parameter set count")
    if report.get("material_parameter_slots") != [
        item["material_slot"] for item in expected_material_sets
    ]:
        raise AssertionError("fake editor import did not report material parameter slots")
    if report.get("material_parameter_runtime_policies") != [
        item["runtime_policy"] for item in expected_material_sets
    ]:
        raise AssertionError("fake editor import did not report material parameter runtime policies")
    reported_semantics = set(report.get("material_parameter_semantics", []))
    for required_semantic in [
        "terrain_surface:overlay_mask_texture",
        "terrain_surface:moss_mask_channel",
        "groundcover_foliage:wind_strength",
        "groundcover_foliage:fade_end_meters",
        "groundcover_foliage:color_variation_scale",
    ]:
        if required_semantic not in reported_semantics:
            raise AssertionError(
                f"fake editor import did not report material semantic {required_semantic}"
            )
    if not {"Midori_WindStrength", "Midori_FadeEndMeters"}.issubset(
        set(report.get("groundcover_material_parameter_names", []))
    ):
        raise AssertionError("fake editor import did not report groundcover material parameters")
    if "Midori_MaskTexture" not in set(report.get("terrain_material_parameter_names", [])):
        raise AssertionError("fake editor import did not report terrain material parameters")
    expected_recipes = manifest.get("material_recipes", [])
    if report.get("material_recipe_count") != len(expected_recipes):
        raise AssertionError("fake editor import did not report material recipe count")
    if report.get("material_recipe_files") != [item["file"] for item in expected_recipes]:
        raise AssertionError("fake editor import did not report material recipe files")
    if report.get("material_recipe_runtime_policies") != [
        item["runtime_policy"] for item in expected_recipes
    ]:
        raise AssertionError("fake editor import did not report material recipe runtime policies")
    reported_recipe_targets = set(report.get("material_recipe_engine_targets", []))
    for required_target in [
        "terrain_surface:unity_terrain_material",
        "terrain_surface:unreal_landscape_material",
        "groundcover_foliage:unity_detail_mesh_material",
        "groundcover_foliage:unreal_static_mesh_foliage_material",
    ]:
        if required_target not in reported_recipe_targets:
            raise AssertionError(
                f"fake editor import did not report material recipe target {required_target}"
            )
    if report.get("terrain_material_recipe_file") != "materials/terrain_surface.recipe.json":
        raise AssertionError("fake editor import did not report terrain material recipe file")
    if report.get("groundcover_material_recipe_file") != "materials/groundcover_foliage.recipe.json":
        raise AssertionError("fake editor import did not report groundcover material recipe file")
    expected_engine_recipes = manifest.get("engine_import_recipes", [])
    if report.get("engine_import_recipe_count") != len(expected_engine_recipes):
        raise AssertionError("fake editor import did not report engine import recipe count")
    if report.get("engine_import_recipe_files") != [
        item["file"] for item in expected_engine_recipes
    ]:
        raise AssertionError("fake editor import did not report engine import recipe files")
    if report.get("engine_import_recipe_profiles") != [
        f"{item['engine']}:{item['profile']}" for item in expected_engine_recipes
    ]:
        raise AssertionError("fake editor import did not report engine import recipe profiles")
    reported_engine_systems = set(report.get("engine_import_recipe_expected_systems", []))
    for required_system in [
        "unity:Unity TerrainData",
        "unity:GPU-instanced terrain detail mesh prefabs",
        "unreal:Unreal Landscape",
        "unreal:Static Mesh Foliage",
    ]:
        if required_system not in reported_engine_systems:
            raise AssertionError(
                f"fake editor import did not report engine import system {required_system}"
            )
    if report.get("unity_engine_import_recipe_file") != "engines/unity_import.recipe.json":
        raise AssertionError("fake editor import did not report Unity engine import recipe file")
    if report.get("unreal_engine_import_recipe_file") != "engines/unreal_import.recipe.json":
        raise AssertionError("fake editor import did not report Unreal engine import recipe file")
    expected_overlays = manifest.get("surface_overlays", [])
    expected_overlay_names = [overlay["name"] for overlay in expected_overlays]
    if report.get("surface_overlay_count") != len(expected_overlays):
        raise AssertionError("fake editor import did not report surface overlay count")
    if report.get("surface_overlay_names") != expected_overlay_names:
        raise AssertionError("fake editor import did not report surface overlay names")
    if report.get("surface_overlay_source_files") != [
        overlay["source_file"] for overlay in expected_overlays
    ]:
        raise AssertionError("fake editor import did not report surface overlay source files")
    if report.get("surface_overlay_channels") != [
        overlay["channel"] for overlay in expected_overlays
    ]:
        raise AssertionError("fake editor import did not report surface overlay channels")
    if report.get("surface_overlay_runtime_policies") != [
        overlay["runtime_policy"] for overlay in expected_overlays
    ]:
        raise AssertionError("fake editor import did not report surface overlay runtime policies")
    moss_overlay = next(overlay for overlay in expected_overlays if overlay["name"] == "moss")
    if not {"terrain_surface", "rock", "log"}.issubset(set(moss_overlay["targets"])):
        raise AssertionError("manifest moss overlay does not target terrain, rock, and log")
    reported_targets = report.get("surface_overlay_targets", [])
    if moss_overlay["targets"] not in reported_targets:
        raise AssertionError("fake editor import did not report moss overlay targets")
    expected_surface_targets = [
        {
            "name": prototype["name"],
            "kind": prototype["kind"],
            "targets": prototype.get("surface_targets", []),
        }
        for prototype in manifest["prototypes"]
    ]
    if report.get("prototype_surface_targets") != expected_surface_targets:
        raise AssertionError("fake editor import did not report prototype surface targets")
    if not {"static_surface", "rock"}.issubset(
        set(report.get("rock_prototype_surface_targets", []))
    ):
        raise AssertionError("fake editor import did not report rock prototype surface targets")
    if not {"static_surface", "log"}.issubset(
        set(report.get("log_prototype_surface_targets", []))
    ):
        raise AssertionError("fake editor import did not report log prototype surface targets")
    if not {"groundcover_foliage", "shrub_base"}.issubset(
        set(report.get("shrub_prototype_surface_targets", []))
    ):
        raise AssertionError("fake editor import did not report shrub prototype surface targets")
    expected_lod0 = sum(
        1
        for prototype in manifest["prototypes"]
        if any(lod.get("index") == 0 for lod in prototype.get("lods", []))
    )
    if report.get("foliage_type_status") != "created":
        raise AssertionError(f"fake editor foliage status was {report.get('foliage_type_status')!r}")
    if report.get("foliage_type_expected_count") != expected_lod0:
        raise AssertionError("fake editor import did not report expected foliage type count")
    if report.get("foliage_type_count") != expected_lod0:
        raise AssertionError("fake editor import did not create a foliage type for every LOD0 prototype")
    if len(report.get("foliage_type_assets", [])) != expected_lod0:
        raise AssertionError("fake editor import did not report every created foliage type asset")
    if report.get("foliage_cull_start_cm") != round(manifest["console"]["cull_start"] * 100):
        raise AssertionError("fake editor import did not report foliage cull start")
    if report.get("foliage_cull_end_cm") != round(manifest["console"]["cull_end"] * 100):
        raise AssertionError("fake editor import did not report foliage cull end")
    created_foliage = FakeAssetToolsHelpers.asset_tools.created_assets
    if len(created_foliage) != expected_lod0:
        raise AssertionError("fake editor did not create the expected foliage assets")
    if len(FakeEditorAssetLibrary.saved_assets) != expected_lod0:
        raise AssertionError("fake editor did not save every foliage asset")
    for asset in created_foliage:
        if not isinstance(asset.properties.get("mesh"), FakeStaticMesh):
            raise AssertionError("fake foliage asset missing mesh property")
        if asset.properties.get("cast_shadow") != manifest["console"]["shadows"]:
            raise AssertionError("fake foliage asset has wrong shadow policy")
        if asset.properties.get("density") != manifest["console"]["density_scale"]:
            raise AssertionError("fake foliage asset has wrong density")
        if asset.properties.get("start_cull_distance") != round(manifest["console"]["cull_start"] * 100):
            raise AssertionError("fake foliage asset has wrong start cull distance")
        if asset.properties.get("end_cull_distance") != round(manifest["console"]["cull_end"] * 100):
            raise AssertionError("fake foliage asset has wrong end cull distance")
        cull_interval = asset.properties.get("cull_distance")
        if (
            not isinstance(cull_interval, FakeInt32Interval)
            or cull_interval.minimum != round(manifest["console"]["cull_start"] * 100)
            or cull_interval.maximum != round(manifest["console"]["cull_end"] * 100)
        ):
            raise AssertionError("fake foliage asset has wrong cull interval")
    if report.get("scatter_binary_records_validated") is not True:
        raise AssertionError("fake editor import did not validate scatter binary records")
    if report.get("scatter_binary_records_read") != 222:
        raise AssertionError(f"unexpected record count: {report.get('scatter_binary_records_read')}")
    if len(report.get("scatter_chunks", [])) != 26:
        raise AssertionError("fake editor import did not report every scatter chunk")
    first_chunk = report["scatter_chunks"][0]
    for field in [
        "file_checksum",
        "record_checksum",
        "yaw_min",
        "yaw_max",
        "phase_min",
        "phase_max",
        "height_min",
        "height_max",
        "width_min",
        "width_max",
        "color_variation_min",
        "color_variation_max",
    ]:
        if not hasattr(first_chunk, field):
            raise AssertionError(f"fake editor scatter chunk missing {field}")
    if not str(report.get("scatter_binary_file_checksum_xor", "")).startswith("0x"):
        raise AssertionError("fake editor import did not report file checksum XOR")
    if not str(report.get("scatter_binary_record_checksum_xor", "")).startswith("0x"):
        raise AssertionError("fake editor import did not report record checksum XOR")
    if not FakeUnrealModule.logs:
        raise AssertionError("fake editor import did not log completion")
    if report["screenshots"]["import"]["status"] != "captured":
        raise AssertionError("fake import screenshot was not marked captured")
    if report["screenshots"]["foliage_settings"]["status"] != "captured":
        raise AssertionError("fake foliage screenshot was not marked captured")
    for key in ["import", "foliage_settings"]:
        screenshot = report["screenshots"][key]
        metrics = screenshot_metrics[key]
        if screenshot.get("exists") is not True:
            raise AssertionError(f"fake {key} screenshot did not report existence")
        if screenshot.get("bytes") != metrics["bytes"]:
            raise AssertionError(f"fake {key} screenshot did not report byte count")
        if screenshot.get("checksum") != metrics["checksum"]:
            raise AssertionError(f"fake {key} screenshot did not report checksum")
        if screenshot.get("width") != 1024 or screenshot.get("height") != 1024:
            raise AssertionError(f"fake {key} screenshot did not report 1024x1024 dimensions")
    assert_rejects_corrupt_scatter(importer, package_dir)

    if args.report:
        test_report = {
            "status": "passed",
            "package_dir": str(package_dir),
            "dry_run": report.get("dry_run"),
            "destination_path": report.get("destination_path"),
            "manifest_file_checksum": report.get("manifest_file_checksum"),
            "source_file_count": report.get("source_file_count"),
            "source_file_checksum_xor": report.get("source_file_checksum_xor"),
            "import_task_count": report.get("import_task_count"),
            "imported_map_file_count": report.get("imported_map_file_count"),
            "imported_prototype_lod_file_count": report.get("imported_prototype_lod_file_count"),
            "imported_lod0_prototype_file_count": report.get("imported_lod0_prototype_file_count"),
            "foliage_type_status": report.get("foliage_type_status"),
            "foliage_type_expected_count": report.get("foliage_type_expected_count"),
            "foliage_type_count": report.get("foliage_type_count"),
            "foliage_cull_start_cm": report.get("foliage_cull_start_cm"),
            "foliage_cull_end_cm": report.get("foliage_cull_end_cm"),
            "scatter_binary_chunks": report.get("scatter_binary_chunks"),
            "scatter_binary_instances": report.get("scatter_binary_instances"),
            "scatter_binary_records_validated": report.get("scatter_binary_records_validated"),
            "scatter_binary_records_read": report.get("scatter_binary_records_read"),
            "scatter_binary_file_checksum_xor": report.get("scatter_binary_file_checksum_xor"),
            "scatter_binary_record_checksum_xor": report.get("scatter_binary_record_checksum_xor"),
            "screenshots": report.get("screenshots"),
            "screenshot_metrics": screenshot_metrics,
            "corrupt_scatter_rejection": "passed",
        }
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(test_report, indent=2) + "\n", encoding="utf-8")

    print("fake unreal import ok")
    return 0


def assert_rejects_corrupt_scatter(importer: Any, package_dir: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="midori_corrupt_unreal_") as temp_dir:
        corrupt_dir = Path(temp_dir) / "forest_floor"
        shutil.copytree(package_dir, corrupt_dir)
        manifest = json.loads((corrupt_dir / "midori_nature.json").read_text(encoding="utf-8"))
        first_binary = manifest["scatter"]["binary_files"][0]["file"]
        first_binary_path = corrupt_dir / first_binary
        with first_binary_path.open("r+b") as handle:
            handle.seek(16 + 12)
            handle.write(struct.pack("<f", 7.0))

        try:
            importer.dry_run_midori_nature_package(str(corrupt_dir))
        except ValueError as exc:
            if "invalid yaw" not in str(exc):
                raise AssertionError(f"unexpected corrupt scatter error: {exc}") from exc
        else:
            raise AssertionError("corrupt Unreal scatter yaw was not rejected")


if __name__ == "__main__":
    raise SystemExit(main())
