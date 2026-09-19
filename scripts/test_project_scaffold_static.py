#!/usr/bin/env python3
"""Editorless preflight for generated Midori engine validation project scaffolds."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


UNITY_DEPENDENCIES = {
    "com.unity.cloud.gltfast": "5.2.0",
    "com.unity.modules.imgui": "1.0.0",
    "com.unity.modules.jsonserialize": "1.0.0",
    "com.unity.modules.terrain": "1.0.0",
    "com.unity.modules.uielements": "1.0.0",
}
UNREAL_PLUGINS = {
    "PythonScriptPlugin",
    "EditorScriptingUtilities",
}


def load_json(path: Path) -> Any:
    if not path.is_file():
        raise AssertionError(f"{path} is missing")
    return json.loads(path.read_text(encoding="utf-8-sig"))


def require_file(path: Path, label: str) -> None:
    if not path.is_file() or path.stat().st_size == 0:
        raise AssertionError(f"{label} is missing or empty: {path}")


def require_dir(path: Path, label: str) -> None:
    if not path.is_dir():
        raise AssertionError(f"{label} is missing: {path}")


def check_unity_scaffold(root: Path, summary: dict[str, Any]) -> None:
    unity = summary.get("unity")
    if not isinstance(unity, dict):
        raise AssertionError("project scaffold summary is missing unity section")

    project = root / "unity" / "MidoriUnityValidation"
    require_dir(project / "ProjectSettings", "Unity ProjectSettings directory")
    require_file(project / "Assets" / "Editor" / "MidoriNaturePackageImporter.cs", "Unity importer")
    require_file(project / "Assets" / "Midori" / "Validation" / "README.md", "Unity validation README")

    manifest_path = project / "Packages" / "manifest.json"
    manifest = load_json(manifest_path)
    dependencies = manifest.get("dependencies")
    if not isinstance(dependencies, dict):
        raise AssertionError(f"{manifest_path} is missing dependencies")
    for package, expected in UNITY_DEPENDENCIES.items():
        actual = dependencies.get(package)
        if actual != expected:
            raise AssertionError(f"Unity dependency {package} is {actual!r}, expected {expected!r}")

    if unity.get("gltfast_version") != UNITY_DEPENDENCIES["com.unity.cloud.gltfast"]:
        raise AssertionError("project scaffold summary did not record the glTFast version")


def check_unreal_scaffold(root: Path, summary: dict[str, Any]) -> None:
    unreal = summary.get("unreal")
    if not isinstance(unreal, dict):
        raise AssertionError("project scaffold summary is missing unreal section")
    if unreal.get("destination_root") != "/Game/Midori/Imported":
        raise AssertionError("project scaffold summary has the wrong Unreal destination root")

    project = root / "unreal" / "MidoriUnrealValidation"
    uproject_path = project / "MidoriUnrealValidation.uproject"
    uproject = load_json(uproject_path)
    plugins = uproject.get("Plugins")
    if not isinstance(plugins, list):
        raise AssertionError(f"{uproject_path} is missing Plugins")
    by_name = {plugin.get("Name"): plugin for plugin in plugins if isinstance(plugin, dict)}
    for plugin_name in UNREAL_PLUGINS:
        plugin = by_name.get(plugin_name)
        if not isinstance(plugin, dict) or plugin.get("Enabled") is not True:
            raise AssertionError(f"Unreal plugin {plugin_name} is not enabled")

    config_path = project / "Config" / "DefaultEngine.ini"
    require_file(config_path, "Unreal DefaultEngine.ini")
    config = config_path.read_text(encoding="utf-8-sig")
    if "PythonScriptPluginSettings" not in config or "bDeveloperMode=True" not in config:
        raise AssertionError("Unreal DefaultEngine.ini does not enable Python editor scripting")
    require_file(project / "Content" / "Midori" / "Validation" / "README.md", "Unreal validation README")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--scaffold-root",
        type=Path,
        default=Path("target/midori_engine_validation/projects"),
    )
    args = parser.parse_args()

    root = args.scaffold_root
    summary = load_json(root / "project_scaffold_summary.json")
    check_unity_scaffold(root, summary)
    check_unreal_scaffold(root, summary)
    print(f"project scaffold preflight ok: {root}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as exc:
        print(f"project scaffold preflight failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
