use super::common::*;
use super::package::{
    check_engine_import_recipe_report, check_material_parameter_report,
    check_material_recipe_report, check_prototype_surface_target_report,
    check_surface_overlay_report,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn resolve_reference_path(
    validation_root: &Path,
    value: Option<&Value>,
    fallback: &Path,
) -> PathBuf {
    let Some(text) = value
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    else {
        return fallback.to_path_buf();
    };
    let path = PathBuf::from(text);
    if path.is_absolute() {
        path
    } else {
        validation_root.join(path)
    }
}

fn eq_field(verifier: &mut Verifier, name: &str, value: &Value, path: &[&str], expected: Value) {
    verifier.require_equal(name, field_path(value, path), expected);
}

fn require_manifest_object(verifier: &mut Verifier, prefix: &str, manifest: &Value, name: &str) {
    let value = field(manifest, name);
    if !value.is_some_and(Value::is_object) {
        verifier.fail(
            format!("{prefix}.manifest_{name}"),
            format!("manifest is missing object {name}"),
        );
    }
}

/// Resolve a manifest scalar without ever substituting a default. A missing,
/// non-numeric or non-finite manifest field is malformed evidence, not a zero.
fn manifest_finite(manifest: &Value, path: &[&str]) -> Result<f64, String> {
    let Some(expected) = field_path(manifest, path).and_then(as_f64) else {
        return Err(format!("manifest field {path:?} is missing or not numeric"));
    };
    if !expected.is_finite() {
        return Err(format!(
            "manifest field {path:?} must be finite, got {expected}"
        ));
    }
    Ok(expected)
}

/// Resolve the manifest terrain height span. Both bounds must be present, so an
/// omitted bound can never collapse the span to a passing zero.
///
/// Resolution order is maximum then minimum, so when both bounds are malformed
/// the reported detail names `max_path`. Only one check is ever emitted for the
/// span, whichever bound is at fault.
fn manifest_span(
    manifest: &Value,
    min_path: &[&str],
    max_path: &[&str],
    floor: Option<f64>,
) -> Result<f64, String> {
    let maximum = manifest_finite(manifest, max_path)?;
    let minimum = manifest_finite(manifest, min_path)?;
    let span = maximum - minimum;
    // Both bounds are finite here, but their difference can still overflow to
    // infinity at the extremes of f64; that is malformed evidence, not a span.
    if !span.is_finite() {
        return Err(format!(
            "manifest fields {max_path:?} and {min_path:?} must span a finite range"
        ));
    }
    Ok(match floor {
        Some(floor) => span.max(floor),
        None => span,
    })
}

fn require_resolved_close(
    verifier: &mut Verifier,
    name: &str,
    actual: Option<&Value>,
    expected: &Result<f64, String>,
    tolerance: f64,
) {
    match expected {
        Ok(expected) => verifier.require_close(name, actual, *expected, tolerance),
        Err(detail) => verifier.fail(name, detail.clone()),
    }
}

fn require_resolved_vector_close(
    verifier: &mut Verifier,
    name: &str,
    actual: Option<&Value>,
    component: &str,
    expected: &Result<f64, String>,
) {
    match expected {
        Ok(expected) => require_vector_close(verifier, name, actual, component, *expected),
        Err(detail) => verifier.fail(name, detail.clone()),
    }
}

fn require_manifest_close(
    verifier: &mut Verifier,
    name: &str,
    actual: Option<&Value>,
    manifest: &Value,
    path: &[&str],
    tolerance: f64,
) {
    require_resolved_close(
        verifier,
        name,
        actual,
        &manifest_finite(manifest, path),
        tolerance,
    );
}

fn require_manifest_numeric_equal(
    verifier: &mut Verifier,
    name: &str,
    actual: Option<&Value>,
    manifest: &Value,
    path: &[&str],
) {
    let Some(expected) = field_path(manifest, path) else {
        verifier.fail(
            name,
            format!("manifest field {path:?} is missing or not numeric"),
        );
        return;
    };
    let Some(_value) = as_f64(expected).filter(|value| value.is_finite()) else {
        verifier.fail(
            name,
            format!("manifest field {path:?} must be a finite numeric scalar"),
        );
        return;
    };
    verifier.require_equal(name, actual, expected.clone());
}

fn require_nonzero_checksum(
    verifier: &mut Verifier,
    name: &str,
    value: Option<&Value>,
    failure: &str,
) {
    verifier.require(
        name,
        is_nonzero_hex_checksum(value),
        format!("{} is nonzero", py_string(value.unwrap_or(&Value::Null))),
        failure,
    );
}

fn read_text_utf8(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(text).to_string())
}

pub fn resolve_project_scaffold_root(validation_root: &Path, summary: &Value) -> Option<PathBuf> {
    let mut candidates = vec![
        validation_root.join("projects"),
        validation_root
            .parent()
            .unwrap_or(validation_root)
            .join("projects"),
    ];
    if let Some(root) = field_path(summary, &["project_scaffolds", "root"]).and_then(Value::as_str)
        && !root.is_empty()
    {
        candidates.push(PathBuf::from(root));
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.join("project_scaffold_summary.json").is_file())
}

pub fn check_project_scaffolds(verifier: &mut Verifier, summary: &Value, validation_root: &Path) {
    let Some(project_scaffolds) =
        field(summary, "project_scaffolds").filter(|value| value.is_object())
    else {
        verifier.fail(
            "summary.project_scaffolds",
            "engine summary is missing project_scaffolds",
        );
        return;
    };
    verifier.require_equal(
        "summary.project_scaffolds_status",
        field(project_scaffolds, "status"),
        json!("generated"),
    );
    verifier.require_equal(
        "summary.project_scaffolds_preflight_status",
        field(project_scaffolds, "preflight_status"),
        json!("passed"),
    );
    verifier.require(
        "summary.project_scaffolds_unity_project",
        field(project_scaffolds, "unity_project")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        py_string(field(project_scaffolds, "unity_project").unwrap_or(&Value::Null)),
        "engine summary must record a Unity scaffold project path",
    );
    verifier.require(
        "summary.project_scaffolds_unreal_project",
        field(project_scaffolds, "unreal_project")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        py_string(field(project_scaffolds, "unreal_project").unwrap_or(&Value::Null)),
        "engine summary must record an Unreal scaffold project path",
    );
    let Some(scaffold_root) = resolve_project_scaffold_root(validation_root, summary) else {
        verifier.fail(
            "summary.project_scaffolds_root",
            format!("could not locate project_scaffold_summary.json near {validation_root:?}"),
        );
        return;
    };
    verifier.pass(
        "summary.project_scaffolds_root",
        format!("{scaffold_root:?} loaded"),
    );
    let scaffold_summary_path = scaffold_root.join("project_scaffold_summary.json");
    let scaffold_summary = match load_json(&scaffold_summary_path) {
        JsonLoad::Value(value) if value.is_object() => {
            verifier.pass(
                "summary.project_scaffold_summary",
                format!("{scaffold_summary_path:?} loaded"),
            );
            value
        }
        JsonLoad::Missing => {
            verifier.fail(
                "summary.project_scaffold_summary",
                format!("{scaffold_summary_path:?} is missing or invalid"),
            );
            return;
        }
        JsonLoad::Invalid(error) => {
            verifier.fail("summary.project_scaffold_summary", error);
            return;
        }
        JsonLoad::Value(_) => {
            verifier.fail(
                "summary.project_scaffold_summary",
                format!("{scaffold_summary_path:?} is missing or invalid"),
            );
            return;
        }
    };

    let unity_project = scaffold_root.join("unity").join("MidoriUnityValidation");
    let unity_importer = unity_project
        .join("Assets")
        .join("Editor")
        .join("MidoriNaturePackageImporter.cs");
    let unity_project_settings = unity_project.join("ProjectSettings");
    let unity_manifest_path = unity_project.join("Packages").join("manifest.json");
    let unity_readme = unity_project
        .join("Assets")
        .join("Midori")
        .join("Validation")
        .join("README.md");
    let unreal_project = scaffold_root.join("unreal").join("MidoriUnrealValidation");
    let unreal_project_file = unreal_project.join("MidoriUnrealValidation.uproject");
    let unreal_config = unreal_project.join("Config").join("DefaultEngine.ini");
    let unreal_readme = unreal_project
        .join("Content")
        .join("Midori")
        .join("Validation")
        .join("README.md");

    check_artifact(
        verifier,
        "summary.project_scaffolds.unity_importer",
        &unity_importer,
    );
    verifier.require(
        "summary.project_scaffolds.unity_project_settings",
        unity_project_settings.is_dir(),
        format!("{unity_project_settings:?} exists"),
        "Unity scaffold must include a ProjectSettings directory",
    );
    check_artifact(
        verifier,
        "summary.project_scaffolds.unity_readme",
        &unity_readme,
    );
    match load_json(&unity_manifest_path) {
        JsonLoad::Value(unity_manifest) if unity_manifest.is_object() => {
            verifier.pass(
                "summary.project_scaffolds.unity_manifest",
                format!("{unity_manifest_path:?} loaded"),
            );
            if let Some(dependencies) =
                field(&unity_manifest, "dependencies").filter(|value| value.is_object())
            {
                verifier.require_equal(
                    "summary.project_scaffolds.unity_gltfast",
                    field(dependencies, "com.unity.cloud.gltfast"),
                    json!("5.2.0"),
                );
                for module_name in [
                    "com.unity.modules.imgui",
                    "com.unity.modules.jsonserialize",
                    "com.unity.modules.terrain",
                    "com.unity.modules.uielements",
                ] {
                    verifier.require_equal(
                        format!("summary.project_scaffolds.{module_name}"),
                        field(dependencies, module_name),
                        json!("1.0.0"),
                    );
                }
            } else {
                verifier.fail(
                    "summary.project_scaffolds.unity_manifest_dependencies",
                    "Unity scaffold manifest is missing dependencies",
                );
            }
        }
        JsonLoad::Missing => verifier.fail(
            "summary.project_scaffolds.unity_manifest",
            format!("{unity_manifest_path:?} is missing or invalid"),
        ),
        JsonLoad::Invalid(error) => {
            verifier.fail("summary.project_scaffolds.unity_manifest", error)
        }
        JsonLoad::Value(_) => verifier.fail(
            "summary.project_scaffolds.unity_manifest",
            format!("{unity_manifest_path:?} is missing or invalid"),
        ),
    }
    match load_json(&unreal_project_file) {
        JsonLoad::Value(unreal_project_json) if unreal_project_json.is_object() => {
            verifier.pass(
                "summary.project_scaffolds.unreal_project",
                format!("{unreal_project_file:?} loaded"),
            );
            if let Some(plugins) = value_as_array(field(&unreal_project_json, "Plugins")) {
                let by_name = object_by_name(plugins, "Name");
                for plugin_name in ["PythonScriptPlugin", "EditorScriptingUtilities"] {
                    verifier.require(
                        format!("summary.project_scaffolds.unreal_plugin.{plugin_name}"),
                        by_name
                            .get(plugin_name)
                            .is_some_and(|plugin| field(plugin, "Enabled") == Some(&json!(true))),
                        format!("{plugin_name} enabled"),
                        format!("Unreal scaffold must enable {plugin_name}"),
                    );
                }
            } else {
                verifier.fail(
                    "summary.project_scaffolds.unreal_plugins",
                    "Unreal scaffold .uproject is missing Plugins",
                );
            }
        }
        JsonLoad::Missing => verifier.fail(
            "summary.project_scaffolds.unreal_project",
            format!("{unreal_project_file:?} is missing or invalid"),
        ),
        JsonLoad::Invalid(error) => {
            verifier.fail("summary.project_scaffolds.unreal_project", error)
        }
        JsonLoad::Value(_) => verifier.fail(
            "summary.project_scaffolds.unreal_project",
            format!("{unreal_project_file:?} is missing or invalid"),
        ),
    }
    if file_nonempty(&unreal_config) {
        match read_text_utf8(&unreal_config) {
            Ok(text) => verifier.require(
                "summary.project_scaffolds.unreal_python_config",
                text.contains("PythonScriptPluginSettings") && text.contains("bDeveloperMode=True"),
                format!("{unreal_config:?} enables Python scripting"),
                "Unreal scaffold must enable Python editor scripting settings",
            ),
            Err(error) => verifier.fail(
                "summary.project_scaffolds.unreal_python_config",
                format!("{unreal_config:?} cannot be read: {error}"),
            ),
        }
    } else {
        verifier.fail(
            "summary.project_scaffolds.unreal_python_config",
            format!("{unreal_config:?} is missing"),
        );
    }
    check_artifact(
        verifier,
        "summary.project_scaffolds.unreal_readme",
        &unreal_readme,
    );
    eq_field(
        verifier,
        "summary.project_scaffold_summary.unity_gltfast_version",
        &scaffold_summary,
        &["unity", "gltfast_version"],
        json!("5.2.0"),
    );
    eq_field(
        verifier,
        "summary.project_scaffold_summary.unreal_destination_root",
        &scaffold_summary,
        &["unreal", "destination_root"],
        json!("/Game/Midori/Imported"),
    );
}

fn object_by_name<'a>(items: &'a [Value], field_name: &str) -> BTreeMap<String, &'a Value> {
    items
        .iter()
        .filter_map(|item| {
            item.as_object().and_then(|_| {
                string_field(field(item, field_name)).map(|name| (name.to_string(), item))
            })
        })
        .collect()
}

pub fn check_summary(
    verifier: &mut Verifier,
    summary: &Value,
    validation_root: &Path,
    manifest: Option<&Value>,
) {
    check_project_scaffolds(verifier, summary, validation_root);
    eq_field(
        verifier,
        "summary.midori_status",
        summary,
        &["midori", "status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.midori_map_summaries",
        summary,
        &["midori", "map_summaries"],
        json!(5),
    );
    eq_field(
        verifier,
        "summary.midori_map_relationships_status",
        summary,
        &["midori", "map_relationships_status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.midori_prototype_summaries",
        summary,
        &["midori", "prototype_summaries"],
        json!(22),
    );
    eq_field(
        verifier,
        "summary.midori_profile_budget_summaries",
        summary,
        &["midori", "profile_budget_summaries"],
        json!(2),
    );
    eq_field(
        verifier,
        "summary.midori_profile_budget_status",
        summary,
        &["midori", "profile_budget_status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.midori_profile_budget_failure_count",
        summary,
        &["midori", "profile_budget_failure_count"],
        json!(0),
    );
    eq_field(
        verifier,
        "summary.midori_scatter_json_chunks",
        summary,
        &["midori", "scatter_json_chunks"],
        json!(26),
    );
    eq_field(
        verifier,
        "summary.midori_scatter_binary_summaries",
        summary,
        &["midori", "scatter_binary_summaries"],
        json!(26),
    );
    eq_field(
        verifier,
        "summary.midori_scatter_parity_status",
        summary,
        &["midori", "scatter_parity_status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.midori_scatter_record_checksum_mismatches",
        summary,
        &["midori", "scatter_record_checksum_mismatches"],
        json!(0),
    );
    eq_field(
        verifier,
        "summary.unity_preflight_status",
        summary,
        &["unity_preflight", "status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.unity_preflight_compile_stub_status",
        summary,
        &["unity_preflight", "compile_stub_status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.unity_preflight_compile_stub_execution_status",
        summary,
        &["unity_preflight", "compile_stub_execution_status"],
        json!("passed"),
    );
    for (name, path, expected) in [
        (
            "summary.unity_preflight_compile_stub_source_file_count",
            &["unity_preflight", "compile_stub_source_file_count"][..],
            json!(59),
        ),
        (
            "summary.unity_preflight_compile_stub_material_recipe_count",
            &["unity_preflight", "compile_stub_material_recipe_count"][..],
            json!(2),
        ),
        (
            "summary.unity_preflight_compile_stub_engine_import_recipe_count",
            &["unity_preflight", "compile_stub_engine_import_recipe_count"][..],
            json!(2),
        ),
        (
            "summary.unity_preflight_compile_stub_scatter_binary_chunks",
            &["unity_preflight", "compile_stub_scatter_binary_chunks"][..],
            json!(26),
        ),
        (
            "summary.unity_preflight_compile_stub_scatter_binary_instances",
            &["unity_preflight", "compile_stub_scatter_binary_instances"][..],
            json!(222),
        ),
    ] {
        eq_field(verifier, name, summary, path, expected);
    }
    eq_field(
        verifier,
        "summary.unity_preflight_compile_stub_records_validated",
        summary,
        &[
            "unity_preflight",
            "compile_stub_scatter_binary_records_validated",
        ],
        json!(true),
    );
    require_nonzero_checksum(
        verifier,
        "summary.unity_preflight_compile_stub_manifest_checksum",
        field_path(
            summary,
            &["unity_preflight", "compile_stub_manifest_file_checksum"],
        ),
        "Unity compile-stub manifest checksum must be present and nonzero",
    );
    require_nonzero_checksum(
        verifier,
        "summary.unity_preflight_compile_stub_source_checksum",
        field_path(
            summary,
            &["unity_preflight", "compile_stub_source_file_checksum_xor"],
        ),
        "Unity compile-stub source checksum must be present and nonzero",
    );
    let compile_stub_report_path = resolve_reference_path(
        validation_root,
        field_path(
            summary,
            &["unity_preflight", "compile_stub_execution_report"],
        ),
        &validation_root.join("forest_floor_unity_compile_stub_report.json"),
    );
    let compile_stub_report = match load_json(&compile_stub_report_path) {
        JsonLoad::Value(value) if value.is_object() => {
            verifier.pass(
                "summary.unity_preflight_compile_stub_report",
                format!("{compile_stub_report_path:?} loaded"),
            );
            Some(value)
        }
        JsonLoad::Missing => {
            verifier.fail(
                "summary.unity_preflight_compile_stub_report",
                format!("{compile_stub_report_path:?} is missing or invalid"),
            );
            None
        }
        JsonLoad::Invalid(error) => {
            verifier.fail("summary.unity_preflight_compile_stub_report", error);
            None
        }
        JsonLoad::Value(_) => {
            verifier.fail(
                "summary.unity_preflight_compile_stub_report",
                format!("{compile_stub_report_path:?} is missing or invalid"),
            );
            None
        }
    };
    if let Some(compile_stub_report) = compile_stub_report.as_ref() {
        check_compile_stub_report(
            verifier,
            compile_stub_report,
            summary,
            validation_root,
            manifest,
        );
    }
    eq_field(
        verifier,
        "summary.unity_preflight_instances",
        summary,
        &["unity_preflight", "scatter_binary_instances"],
        json!(222),
    );
    eq_field(
        verifier,
        "summary.profile_notes_preflight_status",
        summary,
        &["profile_notes_preflight", "status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.unreal_dry_run_status",
        summary,
        &["unreal", "dry_run_status"],
        json!("passed"),
    );
    eq_field(
        verifier,
        "summary.unreal_dry_run_instances",
        summary,
        &["unreal", "scatter_binary_instances"],
        json!(222),
    );
    eq_field(
        verifier,
        "summary.unreal_dry_run_records_validated",
        summary,
        &["unreal", "scatter_binary_records_validated"],
        json!(true),
    );
    eq_field(
        verifier,
        "summary.unreal_dry_run_records_read",
        summary,
        &["unreal", "scatter_binary_records_read"],
        json!(222),
    );
    require_nonzero_checksum(
        verifier,
        "summary.unreal_dry_run_file_checksum_xor",
        field_path(summary, &["unreal", "scatter_binary_file_checksum_xor"]),
        "Unreal dry-run file checksum XOR must be present and nonzero",
    );
    require_nonzero_checksum(
        verifier,
        "summary.unreal_dry_run_record_checksum_xor",
        field_path(summary, &["unreal", "scatter_binary_record_checksum_xor"]),
        "Unreal dry-run record checksum XOR must be present and nonzero",
    );
    eq_field(
        verifier,
        "summary.unreal_dry_run_scatter_chunk_reports",
        summary,
        &["unreal", "scatter_chunk_reports"],
        json!(26),
    );
    eq_field(
        verifier,
        "summary.unreal_fake_editor_status",
        summary,
        &["unreal_fake_editor", "status"],
        json!("passed"),
    );
    let fake_editor_report_path = resolve_reference_path(
        validation_root,
        field_path(summary, &["unreal_fake_editor", "report"]),
        &validation_root.join("forest_floor_unreal_fake_editor_report.json"),
    );
    let fake_editor_report = match load_json(&fake_editor_report_path) {
        JsonLoad::Value(value) if value.is_object() => {
            verifier.pass(
                "summary.unreal_fake_editor_report",
                format!("{fake_editor_report_path:?} loaded"),
            );
            Some(value)
        }
        JsonLoad::Missing => {
            verifier.fail(
                "summary.unreal_fake_editor_report",
                format!("{fake_editor_report_path:?} is missing or invalid"),
            );
            None
        }
        JsonLoad::Invalid(error) => {
            verifier.fail("summary.unreal_fake_editor_report", error);
            None
        }
        JsonLoad::Value(_) => {
            verifier.fail(
                "summary.unreal_fake_editor_report",
                format!("{fake_editor_report_path:?} is missing or invalid"),
            );
            None
        }
    };
    if let Some(fake_editor_report) = fake_editor_report.as_ref() {
        check_fake_editor_report(verifier, fake_editor_report, summary, manifest);
    }
}

fn canonicalish(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn check_report_screenshot_channel_range(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    channel: &str,
) {
    let minimum = float_or_default(
        field(report, &format!("importScreenshotMin{channel}")),
        -1.0,
    );
    let maximum = float_or_default(
        field(report, &format!("importScreenshotMax{channel}")),
        -1.0,
    );
    verifier.require(
        format!("{prefix}_{}_range", channel.to_ascii_lowercase()),
        minimum >= 0.0 && minimum < maximum && maximum <= 1.0,
        format!("{minimum}..{maximum}"),
        "Unity compile-stub import screenshot channels must be nonblank and inside 0..1",
    );
}

fn check_report_density_channel_range(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    channel: &str,
) {
    let minimum = float_or_default(
        field(report, &format!("densityScreenshotMin{channel}")),
        -1.0,
    );
    let maximum = float_or_default(
        field(report, &format!("densityScreenshotMax{channel}")),
        -1.0,
    );
    verifier.require(
        format!("{prefix}_{}_range", channel.to_ascii_lowercase()),
        minimum >= 0.0 && minimum < maximum && maximum <= 1.0,
        format!("{minimum}..{maximum}"),
        "Unity compile-stub density screenshot channels must be nonblank and inside 0..1",
    );
}

fn check_compile_stub_report(
    verifier: &mut Verifier,
    report: &Value,
    summary: &Value,
    validation_root: &Path,
    manifest: Option<&Value>,
) {
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_status",
        field(report, "status"),
        json!("passed"),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_schema_version",
        field(report, "schema_version"),
        json!(3),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_asset_name",
        field(report, "asset_name"),
        json!("Temperate Forest Floor"),
    );
    let package_dir_value = field(report, "package_dir");
    if let Some(package_dir_string) = package_dir_value.and_then(Value::as_str) {
        let report_package_path = PathBuf::from(package_dir_string);
        let resolved_report_package = if report_package_path.is_absolute() {
            canonicalish(&report_package_path)
        } else {
            let first = validation_root.join(&report_package_path);
            let second = validation_root
                .parent()
                .unwrap_or(validation_root)
                .join(&report_package_path);
            canonicalish(if first.exists() { &first } else { &second })
        };
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_package_dir",
            Some(&json!(
                resolved_report_package.to_string_lossy().to_string()
            )),
            json!(
                canonicalish(&validation_root.join("forest_floor"))
                    .to_string_lossy()
                    .to_string()
            ),
        );
    } else {
        verifier.fail(
            "summary.unity_preflight_compile_stub_report_package_dir",
            format!(
                "{} is not a package path string",
                py_repr(package_dir_value.unwrap_or(&Value::Null))
            ),
        );
    }
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_manifest_checksum",
        field(report, "manifest_file_checksum"),
        field_path(
            summary,
            &["unity_preflight", "compile_stub_manifest_file_checksum"],
        )
        .cloned()
        .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_source_file_count",
        field(report, "source_file_count"),
        json!(59),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_source_checksum",
        field(report, "source_file_checksum_xor"),
        field_path(
            summary,
            &["unity_preflight", "compile_stub_source_file_checksum_xor"],
        )
        .cloned()
        .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_material_recipe_count",
        field(report, "material_recipe_count"),
        json!(2),
    );
    if let Some(manifest) = manifest {
        let prototype_count = value_len(field(manifest, "prototypes"));
        let material_slot_count = value_len(field(manifest, "material_slots"));
        for (name, field_name, expected) in [
            (
                "summary.unity_preflight_compile_stub_report_prototypes_declared",
                "prototypesDeclared",
                json!(prototype_count),
            ),
            (
                "summary.unity_preflight_compile_stub_report_lod0_declared",
                "lod0PrototypesDeclared",
                json!(prototype_count),
            ),
            (
                "summary.unity_preflight_compile_stub_report_lod_files_declared",
                "lodFilesDeclared",
                json!(22),
            ),
            (
                "summary.unity_preflight_compile_stub_report_material_slots_declared",
                "materialSlotsDeclared",
                json!(material_slot_count),
            ),
        ] {
            verifier.require_equal(name, field(report, field_name), expected);
        }
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_has_terrain_surface_slot",
            field(report, "hasTerrainSurfaceMaterialSlot"),
            json!(true),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_has_groundcover_slot",
            field(report, "hasGroundcoverFoliageMaterialSlot"),
            json!(true),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_groundcover_alpha",
            field(report, "groundcoverMaterialAlphaMode"),
            json!("masked"),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_groundcover_double_sided",
            field(report, "groundcoverMaterialDoubleSided"),
            json!(true),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_groundcover_shadows",
            field(report, "groundcoverMaterialShadows"),
            json!(false),
        );
        let tile_size = manifest_finite(manifest, &["tile_size"]);
        let terrain_height = manifest_span(
            manifest,
            &["terrain", "height_min"],
            &["terrain", "height_max"],
            None,
        );
        require_resolved_close(
            verifier,
            "summary.unity_preflight_compile_stub_report_terrain_size_x",
            field(report, "terrain_size_x"),
            &tile_size,
            0.001,
        );
        require_resolved_close(
            verifier,
            "summary.unity_preflight_compile_stub_report_terrain_size_y",
            field(report, "terrain_size_y"),
            &terrain_height,
            0.001,
        );
        require_resolved_close(
            verifier,
            "summary.unity_preflight_compile_stub_report_terrain_size_z",
            field(report, "terrain_size_z"),
            &tile_size,
            0.001,
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_heightmap_resolution",
            field(report, "heightmapResolution"),
            json!(33),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_resolution",
            field(report, "detailResolution"),
            field(manifest, "map_resolution")
                .cloned()
                .unwrap_or(Value::Null),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_resolution_per_patch",
            field(report, "detailResolutionPerPatch"),
            json!(8),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_prototypes_created",
            field(report, "detailPrototypesCreated"),
            json!(prototype_count),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_prototypes_loaded_from_assets",
            field(report, "detailPrototypesLoadedFromAssets"),
            json!(0),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_prototypes_generated_from_glb",
            field(report, "detailPrototypesGeneratedFromGlb"),
            json!(prototype_count),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_prototype_failures",
            field(report, "detailPrototypeFailures"),
            json!(0),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_generated_file_count",
            field(report, "detailPrototypeGeneratedFileCount"),
            json!(prototype_count),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_detail_fallback_error_count",
            field(report, "detailPrototypeFallbackErrorCount"),
            json!(0),
        );
        verifier.require(
            "summary.unity_preflight_compile_stub_report_nonzero_detail_cells",
            int_or_default(field(report, "nonZeroDetailCells"), 0) > 0,
            format!(
                "{} nonzero cells",
                py_string(field(report, "nonZeroDetailCells").unwrap_or(&Value::Null))
            ),
            "Unity compile-stub terrain detail path must produce nonzero detail cells",
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_import_screenshot_exists",
            field(report, "importScreenshotExists"),
            json!(true),
        );
        verifier.require(
            "summary.unity_preflight_compile_stub_report_import_screenshot_bytes",
            int_or_default(field(report, "importScreenshotBytes"), 0) > 0,
            format!(
                "{} bytes",
                py_string(field(report, "importScreenshotBytes").unwrap_or(&Value::Null))
            ),
            "Unity compile-stub import screenshot path must write bytes",
        );
        require_nonzero_checksum(
            verifier,
            "summary.unity_preflight_compile_stub_report_import_screenshot_checksum",
            field(report, "importScreenshotChecksum"),
            "Unity compile-stub import screenshot checksum must be nonzero",
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_import_screenshot_width",
            field(report, "importScreenshotWidth"),
            json!(1024),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_import_screenshot_height",
            field(report, "importScreenshotHeight"),
            json!(1024),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_import_screenshot_pixels",
            field(report, "importScreenshotPixelCount"),
            json!(1024 * 1024),
        );
        for channel in ["R", "G", "B"] {
            check_report_screenshot_channel_range(
                verifier,
                "summary.unity_preflight_compile_stub_report_import_screenshot",
                report,
                channel,
            );
        }
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_density_screenshot_exists",
            field(report, "densityScreenshotExists"),
            json!(true),
        );
        verifier.require(
            "summary.unity_preflight_compile_stub_report_density_screenshot_bytes",
            int_or_default(field(report, "densityScreenshotBytes"), 0) > 0,
            format!(
                "{} bytes",
                py_string(field(report, "densityScreenshotBytes").unwrap_or(&Value::Null))
            ),
            "Unity compile-stub density screenshot path must write bytes",
        );
        require_nonzero_checksum(
            verifier,
            "summary.unity_preflight_compile_stub_report_density_screenshot_checksum",
            field(report, "densityScreenshotChecksum"),
            "Unity compile-stub density screenshot checksum must be nonzero",
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_density_screenshot_width",
            field(report, "densityScreenshotWidth"),
            json!(512),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_density_screenshot_height",
            field(report, "densityScreenshotHeight"),
            json!(512),
        );
        verifier.require_equal(
            "summary.unity_preflight_compile_stub_report_density_screenshot_pixels",
            field(report, "densityScreenshotPixelCount"),
            json!(512 * 512),
        );
        for channel in ["R", "G", "B"] {
            check_report_density_channel_range(
                verifier,
                "summary.unity_preflight_compile_stub_report_density_screenshot",
                report,
                channel,
            );
        }
        check_material_parameter_report(
            verifier,
            "summary.unity_preflight_compile_stub_report",
            report,
            manifest,
            true,
        );
        check_material_recipe_report(
            verifier,
            "summary.unity_preflight_compile_stub_report",
            report,
            manifest,
            true,
        );
        check_engine_import_recipe_report(
            verifier,
            "summary.unity_preflight_compile_stub_report",
            report,
            manifest,
            true,
        );
        check_surface_overlay_report(
            verifier,
            "summary.unity_preflight_compile_stub_report",
            report,
            manifest,
            true,
        );
        check_prototype_surface_target_report(
            verifier,
            "summary.unity_preflight_compile_stub_report",
            report,
            manifest,
            true,
        );
    }
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_scatter_instances",
        field(report, "scatter_binary_instances"),
        json!(222),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_scatter_chunks",
        field(report, "scatter_binary_chunks"),
        json!(26),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_engine_recipe_count",
        field(report, "engine_import_recipe_count"),
        json!(2),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_engine_recipe_files",
        field(report, "engine_import_recipe_files"),
        json!([
            "engines/unity_import.recipe.json",
            "engines/unreal_import.recipe.json"
        ]),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_surface_overlay_count",
        field(report, "surface_overlay_count"),
        json!(3),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_prototype_count",
        field(report, "prototype_count"),
        json!(8),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_scatter_chunk_reports",
        field(report, "scatter_chunk_reports"),
        json!(26),
    );
    verifier.require_equal(
        "summary.unity_preflight_compile_stub_report_records_validated",
        field(report, "scatter_binary_records_validated"),
        json!(true),
    );
    require_nonzero_checksum(
        verifier,
        "summary.unity_preflight_compile_stub_report_file_checksum",
        field(report, "scatter_binary_file_checksum_xor"),
        "Unity compile-stub scatter file checksum XOR must be present and nonzero",
    );
    require_nonzero_checksum(
        verifier,
        "summary.unity_preflight_compile_stub_report_record_checksum",
        field(report, "scatter_binary_record_checksum_xor"),
        "Unity compile-stub scatter record checksum XOR must be present and nonzero",
    );
}

fn check_fake_screenshot(
    verifier: &mut Verifier,
    summary: &Value,
    label: &str,
    screenshot: Option<&Value>,
    metric: Option<&Value>,
) {
    let prefix = format!("summary.unreal_fake_editor_report_{label}_screenshot");
    let summary_prefix = format!("{label}_screenshot");
    let screenshot = screenshot
        .filter(|value| value.is_object())
        .unwrap_or(&Value::Null);
    let metric = metric
        .filter(|value| value.is_object())
        .unwrap_or(&Value::Null);
    verifier.require_equal(
        format!("{prefix}_status"),
        field(screenshot, "status"),
        json!("captured"),
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_status"),
        field_path(
            summary,
            &["unreal_fake_editor", &format!("{summary_prefix}_status")],
        ),
        field(screenshot, "status").cloned().unwrap_or(Value::Null),
    );
    verifier.require_equal(
        format!("{prefix}_method"),
        field(screenshot, "method"),
        json!("AutomationLibrary.take_high_res_screenshot"),
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_method"),
        field_path(
            summary,
            &["unreal_fake_editor", &format!("{summary_prefix}_method")],
        ),
        field(screenshot, "method").cloned().unwrap_or(Value::Null),
    );
    verifier.require_equal(
        format!("{prefix}_reported_exists"),
        field(screenshot, "exists"),
        json!(true),
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_report_exists"),
        field_path(
            summary,
            &[
                "unreal_fake_editor",
                &format!("{summary_prefix}_report_exists"),
            ],
        ),
        field(screenshot, "exists").cloned().unwrap_or(Value::Null),
    );
    verifier.require(
        format!("{prefix}_reported_bytes"),
        int_or_default(field(screenshot, "bytes"), 0) > 0,
        format!(
            "{} bytes",
            py_string(field(screenshot, "bytes").unwrap_or(&Value::Null))
        ),
        "Fake Unreal importer screenshot report must write bytes",
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_report_bytes"),
        field_path(
            summary,
            &[
                "unreal_fake_editor",
                &format!("{summary_prefix}_report_bytes"),
            ],
        ),
        field(screenshot, "bytes").cloned().unwrap_or(Value::Null),
    );
    require_nonzero_checksum(
        verifier,
        &format!("{prefix}_reported_checksum"),
        field(screenshot, "checksum"),
        "Fake Unreal importer screenshot report must include a nonzero checksum",
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_report_checksum"),
        field_path(
            summary,
            &[
                "unreal_fake_editor",
                &format!("{summary_prefix}_report_checksum"),
            ],
        ),
        field(screenshot, "checksum")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        format!("{prefix}_reported_width"),
        field(screenshot, "width"),
        json!(1024),
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_report_width"),
        field_path(
            summary,
            &[
                "unreal_fake_editor",
                &format!("{summary_prefix}_report_width"),
            ],
        ),
        field(screenshot, "width").cloned().unwrap_or(Value::Null),
    );
    verifier.require_equal(
        format!("{prefix}_reported_height"),
        field(screenshot, "height"),
        json!(1024),
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_report_height"),
        field_path(
            summary,
            &[
                "unreal_fake_editor",
                &format!("{summary_prefix}_report_height"),
            ],
        ),
        field(screenshot, "height").cloned().unwrap_or(Value::Null),
    );
    verifier.require_equal(
        format!("{prefix}_exists"),
        field(metric, "exists"),
        json!(true),
    );
    verifier.require(
        format!("{prefix}_bytes"),
        int_or_default(field(metric, "bytes"), 0) > 0,
        format!(
            "{} bytes",
            py_string(field(metric, "bytes").unwrap_or(&Value::Null))
        ),
        "Fake Unreal screenshot path must write bytes",
    );
    require_nonzero_checksum(
        verifier,
        &format!("{prefix}_checksum"),
        field(metric, "checksum"),
        "Fake Unreal screenshot checksum must be present and nonzero",
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_metric_checksum"),
        field_path(
            summary,
            &["unreal_fake_editor", &format!("{summary_prefix}_checksum")],
        ),
        field(metric, "checksum").cloned().unwrap_or(Value::Null),
    );
    verifier.require_equal(
        format!("{prefix}_width"),
        field(metric, "width"),
        json!(1024),
    );
    verifier.require_equal(
        format!("{prefix}_height"),
        field(metric, "height"),
        json!(1024),
    );
    verifier.require_equal(
        format!("{prefix}_sample_count"),
        field(metric, "sample_count"),
        json!(1024 * 1024),
    );
    verifier.require(
        format!("{prefix}_luminance_range"),
        int_or_default(field(metric, "luminance_range"), 0)
            >= MIN_SCREENSHOT_LUMINANCE_RANGE as i64,
        format!(
            "{} luminance range",
            py_string(field(metric, "luminance_range").unwrap_or(&Value::Null))
        ),
        "Fake Unreal screenshot must be nonblank enough to model the real screenshot gate",
    );
    verifier.require_equal(
        format!("summary.unreal_fake_editor_{summary_prefix}_luminance_range"),
        field_path(
            summary,
            &[
                "unreal_fake_editor",
                &format!("{summary_prefix}_luminance_range"),
            ],
        ),
        field(metric, "luminance_range")
            .cloned()
            .unwrap_or(Value::Null),
    );
}

fn check_fake_editor_report(
    verifier: &mut Verifier,
    report: &Value,
    summary: &Value,
    manifest: Option<&Value>,
) {
    verifier.require_equal(
        "summary.unreal_fake_editor_report_status",
        field(report, "status"),
        json!("passed"),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_dry_run",
        field(report, "dry_run"),
        json!(false),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_destination",
        field(report, "destination_path"),
        json!("/Game/Midori/FakeEditor/Temperate_Forest_Floor"),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_source_file_count",
        field(report, "source_file_count"),
        json!(59),
    );
    require_nonzero_checksum(
        verifier,
        "summary.unreal_fake_editor_report_manifest_checksum",
        field(report, "manifest_file_checksum"),
        "Fake Unreal editor report manifest checksum must be present and nonzero",
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_manifest_checksum_matches_dry_run",
        field(report, "manifest_file_checksum"),
        field_path(summary, &["unreal", "manifest_file_checksum"])
            .cloned()
            .unwrap_or(Value::Null),
    );
    require_nonzero_checksum(
        verifier,
        "summary.unreal_fake_editor_report_source_checksum",
        field(report, "source_file_checksum_xor"),
        "Fake Unreal editor report source checksum must be present and nonzero",
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_source_checksum_matches_dry_run",
        field(report, "source_file_checksum_xor"),
        field_path(summary, &["unreal", "source_file_checksum_xor"])
            .cloned()
            .unwrap_or(Value::Null),
    );
    if let Some(manifest) = manifest {
        let prototypes = value_len(field(manifest, "prototypes"));
        let expected_import_files = expected_unreal_import_files(manifest);
        let map_files = expected_unreal_import_map_files(manifest);
        let lod_files = expected_prototype_lod_files(manifest);
        let lod0_files = expected_lod0_prototype_files(manifest);
        for (name, field_name, expected) in [
            (
                "summary.unreal_fake_editor_report_import_task_count",
                "import_task_count",
                json!(expected_import_files.len()),
            ),
            (
                "summary.unreal_fake_editor_report_map_import_count",
                "imported_map_file_count",
                json!(map_files.len()),
            ),
            (
                "summary.unreal_fake_editor_report_prototype_lod_import_count",
                "imported_prototype_lod_file_count",
                json!(lod_files.len()),
            ),
            (
                "summary.unreal_fake_editor_report_lod0_import_count",
                "imported_lod0_prototype_file_count",
                json!(lod0_files.len()),
            ),
            (
                "summary.unreal_fake_editor_report_foliage_expected_count",
                "foliage_type_expected_count",
                json!(prototypes),
            ),
            (
                "summary.unreal_fake_editor_report_foliage_count",
                "foliage_type_count",
                json!(prototypes),
            ),
        ] {
            verifier.require_equal(name, field(report, field_name), expected);
        }
    }
    verifier.require_equal(
        "summary.unreal_fake_editor_report_foliage_status",
        field(report, "foliage_type_status"),
        json!("created"),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_foliage_cull_start_cm",
        field(report, "foliage_cull_start_cm"),
        json!(3500),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_foliage_cull_end_cm",
        field(report, "foliage_cull_end_cm"),
        json!(7000),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_scatter_chunks",
        field(report, "scatter_binary_chunks"),
        json!(26),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_scatter_instances",
        field(report, "scatter_binary_instances"),
        json!(222),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_scatter_records_validated",
        field(report, "scatter_binary_records_validated"),
        json!(true),
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_scatter_records_read",
        field(report, "scatter_binary_records_read"),
        json!(222),
    );
    require_nonzero_checksum(
        verifier,
        "summary.unreal_fake_editor_report_scatter_file_checksum",
        field(report, "scatter_binary_file_checksum_xor"),
        "Fake Unreal editor scatter file checksum XOR must be present and nonzero",
    );
    require_nonzero_checksum(
        verifier,
        "summary.unreal_fake_editor_report_scatter_record_checksum",
        field(report, "scatter_binary_record_checksum_xor"),
        "Fake Unreal editor scatter record checksum XOR must be present and nonzero",
    );
    verifier.require_equal(
        "summary.unreal_fake_editor_report_corrupt_scatter_rejection",
        field(report, "corrupt_scatter_rejection"),
        json!("passed"),
    );
    let screenshots = field(report, "screenshots");
    let metrics = field(report, "screenshot_metrics");
    for (key, label) in [
        ("import", "import"),
        ("foliage_settings", "foliage_settings"),
    ] {
        check_fake_screenshot(
            verifier,
            summary,
            label,
            field(screenshots.unwrap_or(&Value::Null), key),
            field(metrics.unwrap_or(&Value::Null), key),
        );
    }
}

pub fn check_unity_report(
    verifier: &mut Verifier,
    report: &Value,
    manifest: &Value,
    package_dir: &Path,
) {
    let terrain = field(manifest, "terrain").unwrap_or(&Value::Null);
    let mobile = field(manifest, "mobile").unwrap_or(&Value::Null);
    let unity = field(manifest, "unity");
    let wind = field(manifest, "wind_packing").unwrap_or(&Value::Null);
    for name in [
        "terrain",
        "mobile",
        "unity",
        "normal_conventions",
        "wind_packing",
    ] {
        require_manifest_object(verifier, "unity", manifest, name);
    }
    verifier.require_equal(
        "unity.schema_version",
        field(report, "schemaVersion"),
        field(manifest, "schema_version")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal("unity.profile", field(report, "profile"), json!("mobile"));
    check_package_identity(
        verifier,
        "unity",
        report,
        manifest,
        package_dir,
        "unity_yplus_file",
        true,
    );
    let tile_size = manifest_finite(manifest, &["tile_size"]);
    let terrain_height = manifest_span(
        manifest,
        &["terrain", "height_min"],
        &["terrain", "height_max"],
        Some(0.01),
    );
    require_resolved_close(
        verifier,
        "unity.tile_size",
        field(report, "tileSizeMeters"),
        &tile_size,
        0.001,
    );
    for (name, component, expected) in [
        ("unity.terrain_size_x", "x", &tile_size),
        ("unity.terrain_size_z", "z", &tile_size),
        ("unity.terrain_height", "y", &terrain_height),
    ] {
        require_resolved_vector_close(
            verifier,
            name,
            field(report, "terrainSize"),
            component,
            expected,
        );
    }
    verifier.require_equal(
        "unity.heightmap_file",
        field(report, "heightmapFile"),
        field(terrain, "heightmap_file")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unity.density_map",
        field(report, "densityMapFile"),
        field(terrain, "grass_density_file")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unity.normal_map",
        field(report, "normalMapFile"),
        field_path(manifest, &["normal_conventions", "unity_yplus_file"])
            .cloned()
            .unwrap_or(Value::Null),
    );
    for (name, report_field, manifest_path) in [
        (
            "unity.hint_heightmap",
            "unityHintHeightmap",
            &["terrain_heightmap"][..],
        ),
        (
            "unity.hint_density",
            "unityHintDensityMap",
            &["detail_density_map"][..],
        ),
        (
            "unity.hint_normal",
            "unityHintNormalMap",
            &["normal_map"][..],
        ),
    ] {
        match unity.and_then(|value| field_path(value, manifest_path)) {
            Some(expected) => {
                verifier.require_equal(name, field(report, report_field), expected.clone())
            }
            None => verifier.fail(
                name,
                format!("manifest unity hint field {manifest_path:?} is missing"),
            ),
        }
    }
    check_unity_screenshot_report(
        verifier,
        "unity.import_screenshot_report",
        field(report, "importScreenshot"),
        1024,
        1024,
    );
    check_unity_screenshot_report(
        verifier,
        "unity.density_screenshot_report",
        field(report, "densityScreenshot"),
        512,
        512,
    );
    for (name, field_name, manifest_path) in [
        (
            "unity.mobile_density",
            "mobileDensityScale",
            &["mobile", "density_scale"][..],
        ),
        (
            "unity.mobile_lod0_distance",
            "mobileLod0MaxDistance",
            &["mobile", "lod0_max_distance"][..],
        ),
        (
            "unity.mobile_lod1_distance",
            "mobileLod1MaxDistance",
            &["mobile", "lod1_max_distance"][..],
        ),
        (
            "unity.mobile_lod2_distance",
            "mobileLod2MaxDistance",
            &["mobile", "lod2_max_distance"][..],
        ),
        (
            "unity.mobile_cull_start",
            "mobileCullStartMeters",
            &["mobile", "cull_start"][..],
        ),
        (
            "unity.mobile_cull_end",
            "mobileCullEndMeters",
            &["mobile", "cull_end"][..],
        ),
    ] {
        require_manifest_close(
            verifier,
            name,
            field(report, field_name),
            manifest,
            manifest_path,
            0.001,
        );
    }
    for (name, field_name, expected) in [
        (
            "unity.mobile_shadows",
            "mobileShadows",
            field(mobile, "shadows").cloned().unwrap_or(Value::Null),
        ),
        (
            "unity.mobile_material_slots",
            "mobileMaterialSlots",
            field(mobile, "material_slots")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unity.mobile_max_instances_per_tile",
            "mobileMaxInstancesPerTile",
            field(mobile, "max_instances_per_tile")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unity.mobile_max_instances_per_chunk",
            "mobileMaxInstancesPerChunk",
            field(mobile, "max_instances_per_chunk")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unity.mobile_grass_collision",
            "mobileGrassCollision",
            field(mobile, "grass_collision")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unity.mobile_moss_collision",
            "mobileMossCollision",
            field(mobile, "moss_collision")
                .cloned()
                .unwrap_or(Value::Null),
        ),
    ] {
        verifier.require_equal(name, field(report, field_name), expected);
    }
    verifier.require_equal(
        "unity.material_slots_declared",
        field(report, "materialSlotsDeclared"),
        json!(value_len(field(manifest, "material_slots"))),
    );
    verifier.require_equal(
        "unity.has_terrain_surface_slot",
        field(report, "hasTerrainSurfaceMaterialSlot"),
        json!(true),
    );
    verifier.require_equal(
        "unity.has_groundcover_slot",
        field(report, "hasGroundcoverFoliageMaterialSlot"),
        json!(true),
    );
    verifier.require_equal(
        "unity.groundcover_alpha",
        field(report, "groundcoverMaterialAlphaMode"),
        json!("masked"),
    );
    verifier.require_equal(
        "unity.groundcover_double_sided",
        field(report, "groundcoverMaterialDoubleSided"),
        json!(true),
    );
    verifier.require_equal(
        "unity.groundcover_shadows",
        field(report, "groundcoverMaterialShadows"),
        json!(false),
    );
    check_material_parameter_report(verifier, "unity", report, manifest, true);
    check_material_recipe_report(verifier, "unity", report, manifest, true);
    check_engine_import_recipe_report(verifier, "unity", report, manifest, true);
    check_surface_overlay_report(verifier, "unity", report, manifest, true);
    check_prototype_surface_target_report(verifier, "unity", report, manifest, true);
    for (name, field_name, expected) in [
        (
            "unity.wind_phase",
            "windPhase",
            field(wind, "phase").cloned().unwrap_or(Value::Null),
        ),
        (
            "unity.wind_stiffness",
            "windStiffness",
            field(wind, "stiffness").cloned().unwrap_or(Value::Null),
        ),
        (
            "unity.wind_height",
            "windHeight",
            field(wind, "height").cloned().unwrap_or(Value::Null),
        ),
        (
            "unity.wind_color_variation",
            "windColorVariation",
            field(wind, "color_variation")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unity.wind_normalized_progress",
            "windNormalizedProgress",
            field(wind, "normalized_progress")
                .cloned()
                .unwrap_or(Value::Null),
        ),
    ] {
        verifier.require_equal(name, field(report, field_name), expected);
    }
    let prototype_count = value_len(field(manifest, "prototypes"));
    verifier.require_equal(
        "unity.prototypes_declared",
        field(report, "prototypesDeclared"),
        json!(prototype_count),
    );
    verifier.require_equal(
        "unity.lod0_declared",
        field(report, "lod0PrototypesDeclared"),
        json!(prototype_count),
    );
    verifier.require_equal(
        "unity.lod_files_declared",
        field(report, "lodFilesDeclared"),
        json!(22),
    );
    verifier.require(
        "unity.detail_prototypes_created",
        int_or_default(field(report, "detailPrototypesCreated"), 0) > 0,
        format!(
            "{} detail prototypes created",
            py_string(field(report, "detailPrototypesCreated").unwrap_or(&Value::Null))
        ),
        "Unity must create at least one detail prototype",
    );
    verifier.require_equal(
        "unity.detail_prototype_failures",
        field(report, "detailPrototypeFailures"),
        json!(0),
    );
    let fallback_errors = field(report, "detailPrototypeFallbackErrors");
    match fallback_errors {
        Some(value) if value.is_array() => verifier.require_equal(
            "unity.detail_prototype_fallback_errors",
            Some(&json!(value_len(Some(value)))),
            json!(0),
        ),
        Some(value) => verifier.fail(
            "unity.detail_prototype_fallback_errors",
            format!(
                "detailPrototypeFallbackErrors must be an array, got {}",
                py_repr(value)
            ),
        ),
        None => verifier.fail(
            "unity.detail_prototype_fallback_errors",
            "Unity report is missing detailPrototypeFallbackErrors",
        ),
    }
    let loaded = int_or_default(field(report, "detailPrototypesLoadedFromAssets"), 0);
    let generated = int_or_default(field(report, "detailPrototypesGeneratedFromGlb"), 0);
    let created = int_or_default(field(report, "detailPrototypesCreated"), 0);
    match loaded.checked_add(generated) {
        Some(total) => verifier.require_equal(
            "unity.detail_prototype_source_count",
            Some(&json!(total)),
            json!(created),
        ),
        None => verifier.fail(
            "unity.detail_prototype_source_count",
            "Unity detail prototype source count overflows a signed 64-bit integer",
        ),
    }
    verifier.require("unity.detail_prototype_glb_sources", loaded > 0 || generated > 0, format!("{loaded} AssetDatabase prototypes, {generated} native GLB prototypes"), "Unity detail prototypes must come from imported GLB assets or Midori's native GLB fallback");
    verifier.require(
        "unity.detail_density_nonzero",
        int_or_default(field(report, "nonZeroDetailCells"), 0) > 0,
        format!(
            "{} nonzero detail cells",
            py_string(field(report, "nonZeroDetailCells").unwrap_or(&Value::Null))
        ),
        "Unity detail density must produce nonzero detail cells",
    );
    verifier.require_equal(
        "unity.scatter_instances",
        field(report, "scatterBinaryInstances"),
        json!(222),
    );
    check_unity_scatter_report(verifier, report);
}

fn check_unity_screenshot_report(
    verifier: &mut Verifier,
    prefix: &str,
    report: Option<&Value>,
    expected_width: u32,
    expected_height: u32,
) {
    let Some(report) = report.filter(|value| value.is_object()) else {
        verifier.fail(
            prefix,
            "Unity report is missing screenshot artifact summary",
        );
        return;
    };
    verifier.require(
        format!("{prefix}.path"),
        field(report, "path")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        py_string(field(report, "path").unwrap_or(&Value::Null)),
        "Unity screenshot report must include a nonempty path",
    );
    verifier.require_equal(
        format!("{prefix}.status"),
        field(report, "status"),
        json!("captured"),
    );
    verifier.require_equal(
        format!("{prefix}.exists"),
        field(report, "exists"),
        json!(true),
    );
    verifier.require(
        format!("{prefix}.bytes"),
        int_or_default(field(report, "bytes"), 0) > 0,
        format!(
            "{} bytes",
            py_string(field(report, "bytes").unwrap_or(&Value::Null))
        ),
        "Unity screenshot report must record nonzero bytes",
    );
    require_nonzero_checksum(
        verifier,
        &format!("{prefix}.checksum"),
        field(report, "checksum"),
        "Unity screenshot report must record a nonzero checksum",
    );
    verifier.require_equal(
        format!("{prefix}.width"),
        field(report, "width"),
        json!(expected_width),
    );
    verifier.require_equal(
        format!("{prefix}.height"),
        field(report, "height"),
        json!(expected_height),
    );
}

fn check_unity_scatter_report(verifier: &mut Verifier, report: &Value) {
    verifier.require_equal(
        "unity.scatter_chunks",
        field(report, "scatterBinaryChunks"),
        json!(26),
    );
    verifier.require_equal(
        "unity.scatter_records_validated",
        field(report, "scatterBinaryRecordsValidated"),
        json!(true),
    );
    verifier.require_equal(
        "unity.scatter_records_read",
        field(report, "scatterBinaryRecordsRead"),
        json!(222),
    );
    require_nonzero_checksum(
        verifier,
        "unity.scatter_file_checksum_xor",
        field(report, "scatterBinaryFileChecksumXor"),
        "Unity scatter file checksum XOR must be present and nonzero",
    );
    require_nonzero_checksum(
        verifier,
        "unity.scatter_record_checksum_xor",
        field(report, "scatterBinaryRecordChecksumXor"),
        "Unity scatter record checksum XOR must be present and nonzero",
    );
    let Some(summaries) = value_as_array(field(report, "scatterChunkReports")) else {
        verifier.fail(
            "unity.scatter_chunk_reports",
            "Unity report is missing scatterChunkReports",
        );
        return;
    };
    verifier.require_equal(
        "unity.scatter_chunk_report_count",
        Some(&json!(summaries.len())),
        json!(26),
    );
    let mut total = Some(0i64);
    for (index, summary) in summaries.iter().enumerate() {
        let Some(summary) = summary.as_object().map(|_| summary) else {
            verifier.fail(
                format!("unity.scatter_chunk_report.{index}"),
                "summary must be an object",
            );
            continue;
        };
        let label = format!("unity.scatter_chunk_report.{index}");
        let instances = int_or_default(field(summary, "instanceCount"), 0);
        if let Some(running) = total {
            total = running.checked_add(instances);
            if total.is_none() {
                verifier.fail(
                    "unity.scatter_chunk_report_instances",
                    "Unity scatter chunk instance count overflows a signed 64-bit integer",
                );
            }
        }
        verifier.require(
            format!("{label}.instance_count"),
            instances > 0,
            format!("{instances} instances"),
            "Unity scatter chunk reports must contain at least one instance",
        );
        require_nonzero_checksum(
            verifier,
            &format!("{label}.file_checksum"),
            field(summary, "fileChecksum"),
            "Unity scatter chunk file checksum must be present and nonzero",
        );
        require_nonzero_checksum(
            verifier,
            &format!("{label}.record_checksum"),
            field(summary, "recordChecksum"),
            "Unity scatter chunk record checksum must be present and nonzero",
        );
        check_range(
            verifier,
            &format!("{label}.yaw_range"),
            float_or_default(field(summary, "yawMin"), -1.0),
            float_or_default(field(summary, "yawMax"), -1.0),
            0.0,
            std::f64::consts::TAU,
            "Unity scatter yaw range must be ordered and inside 0..tau",
        );
        check_range(
            verifier,
            &format!("{label}.phase_range"),
            float_or_default(field(summary, "phaseMin"), -1.0),
            float_or_default(field(summary, "phaseMax"), -1.0),
            0.0,
            std::f64::consts::TAU,
            "Unity scatter phase range must be ordered and inside 0..tau",
        );
        check_positive_range(
            verifier,
            &format!("{label}.height_range"),
            float_or_default(field(summary, "heightMin"), 0.0),
            float_or_default(field(summary, "heightMax"), 0.0),
            f64::INFINITY,
            "Unity scatter height multiplier range must be positive and ordered",
        );
        check_positive_range(
            verifier,
            &format!("{label}.width_range"),
            float_or_default(field(summary, "widthMin"), 0.0),
            float_or_default(field(summary, "widthMax"), 0.0),
            f64::INFINITY,
            "Unity scatter width multiplier range must be positive and ordered",
        );
        check_range(
            verifier,
            &format!("{label}.color_variation_range"),
            float_or_default(field(summary, "colorVariationMin"), -1.0),
            float_or_default(field(summary, "colorVariationMax"), -1.0),
            0.0,
            1.0,
            "Unity scatter color variation range must be ordered and inside 0..1",
        );
    }
    if let Some(total) = total {
        verifier.require_equal(
            "unity.scatter_chunk_report_instances",
            Some(&json!(total)),
            json!(222),
        );
    }
}

pub fn check_unreal_report(
    verifier: &mut Verifier,
    report: &Value,
    manifest: &Value,
    package_dir: &Path,
) {
    let console = field(manifest, "console").unwrap_or(&Value::Null);
    let unreal = field(manifest, "unreal").unwrap_or(&Value::Null);
    let groundcover = field(report, "groundcover_material").unwrap_or(&Value::Null);
    for name in ["console", "unreal", "normal_conventions", "wind_packing"] {
        require_manifest_object(verifier, "unreal", manifest, name);
    }
    verifier.require_equal(
        "unreal.schema_version",
        field(report, "schema_version"),
        field(manifest, "schema_version")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unreal.dry_run_false",
        field(report, "dry_run"),
        json!(false),
    );
    verifier.require_equal("unreal.profile", field(report, "profile"), json!("console"));
    check_package_identity(
        verifier,
        "unreal",
        report,
        manifest,
        package_dir,
        "unreal_yminus_file",
        false,
    );
    require_manifest_close(
        verifier,
        "unreal.tile_size",
        field(report, "tile_size_meters"),
        manifest,
        &["tile_size"],
        0.001,
    );
    verifier.require_equal(
        "unreal.landscape_heightmap",
        field(report, "landscape_heightmap"),
        field(unreal, "landscape_heightmap")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unreal.landscape_weightmap",
        field(report, "landscape_weightmap"),
        field(unreal, "landscape_weightmap")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unreal.normal_map",
        field(report, "normal_map"),
        field(unreal, "normal_map").cloned().unwrap_or(Value::Null),
    );
    let screenshots = field(report, "screenshots");
    check_unreal_screenshot_report(
        verifier,
        "unreal.import_screenshot_report",
        field(screenshots.unwrap_or(&Value::Null), "import"),
        1024,
        1024,
    );
    check_unreal_screenshot_report(
        verifier,
        "unreal.foliage_settings_screenshot_report",
        field(screenshots.unwrap_or(&Value::Null), "foliage_settings"),
        1024,
        1024,
    );
    require_manifest_numeric_equal(
        verifier,
        "unreal.console_density",
        field(report, "console_density_scale"),
        manifest,
        &["console", "density_scale"],
    );
    for (name, field_name, manifest_field) in [
        (
            "unreal.console_lod0_distance_m",
            "console_lod0_max_distance_meters",
            "lod0_max_distance",
        ),
        (
            "unreal.console_lod1_distance_m",
            "console_lod1_max_distance_meters",
            "lod1_max_distance",
        ),
        (
            "unreal.console_lod2_distance_m",
            "console_lod2_max_distance_meters",
            "lod2_max_distance",
        ),
    ] {
        require_manifest_close(
            verifier,
            name,
            field(report, field_name),
            manifest,
            &["console", manifest_field],
            0.001,
        );
    }
    for (name, field_name, manifest_field) in [
        (
            "unreal.console_material_slots",
            "console_material_slots",
            "material_slots",
        ),
        (
            "unreal.console_max_instances_per_tile",
            "console_max_instances_per_tile",
            "max_instances_per_tile",
        ),
        (
            "unreal.console_max_instances_per_chunk",
            "console_max_instances_per_chunk",
            "max_instances_per_chunk",
        ),
    ] {
        require_manifest_numeric_equal(
            verifier,
            name,
            field(report, field_name),
            manifest,
            &["console", manifest_field],
        );
    }
    for (name, field_name, expected) in [
        (
            "unreal.console_shadows",
            "console_shadows",
            field(console, "shadows").cloned().unwrap_or(Value::Null),
        ),
        (
            "unreal.console_grass_collision",
            "console_grass_collision",
            field(console, "grass_collision")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unreal.console_moss_collision",
            "console_moss_collision",
            field(console, "moss_collision")
                .cloned()
                .unwrap_or(Value::Null),
        ),
    ] {
        verifier.require_equal(name, field(report, field_name), expected);
    }
    for (name, field_name, manifest_field) in [
        (
            "unreal.console_cull_start_m",
            "console_cull_start_meters",
            "cull_start",
        ),
        (
            "unreal.console_cull_end_m",
            "console_cull_end_meters",
            "cull_end",
        ),
    ] {
        require_manifest_close(
            verifier,
            name,
            field(report, field_name),
            manifest,
            &["console", manifest_field],
            0.001,
        );
    }
    verifier.require_equal(
        "unreal.console_cull_start_cm",
        field(report, "console_cull_start_cm"),
        json!(3500),
    );
    verifier.require_equal(
        "unreal.console_cull_end_cm",
        field(report, "console_cull_end_cm"),
        json!(7000),
    );
    let prototype_count = value_len(field(manifest, "prototypes"));
    for (name, field_name, expected) in [
        (
            "unreal.prototypes_declared",
            "prototypes_declared",
            json!(prototype_count),
        ),
        (
            "unreal.lod0_declared",
            "lod0_prototypes_declared",
            json!(prototype_count),
        ),
        ("unreal.lod_files_declared", "lod_files_declared", json!(22)),
        (
            "unreal.foliage_type_status",
            "foliage_type_status",
            json!("created"),
        ),
        (
            "unreal.foliage_type_expected_count",
            "foliage_type_expected_count",
            json!(prototype_count),
        ),
        (
            "unreal.foliage_type_count",
            "foliage_type_count",
            json!(prototype_count),
        ),
    ] {
        verifier.require_equal(name, field(report, field_name), expected);
    }
    let foliage_assets = field(report, "foliage_type_assets");
    verifier.require(
        "unreal.foliage_type_assets",
        value_as_array(foliage_assets).is_some_and(|items| items.len() == prototype_count),
        format!(
            "{} foliage assets",
            value_as_array(foliage_assets)
                .map_or("missing".to_string(), |items| items.len().to_string())
        ),
        "Unreal editor import must create one foliage type asset per LOD0 prototype",
    );
    verifier.require_equal(
        "unreal.foliage_cull_start_cm",
        field(report, "foliage_cull_start_cm"),
        json!(3500),
    );
    verifier.require_equal(
        "unreal.foliage_cull_end_cm",
        field(report, "foliage_cull_end_cm"),
        json!(7000),
    );
    verifier.require_equal(
        "unreal.material_slots_declared",
        field(report, "material_slots_declared"),
        json!(value_len(field(manifest, "material_slots"))),
    );
    verifier.require_equal(
        "unreal.has_terrain_surface_slot",
        field(report, "has_terrain_surface_material_slot"),
        json!(true),
    );
    verifier.require_equal(
        "unreal.has_groundcover_slot",
        field(report, "has_groundcover_foliage_material_slot"),
        json!(true),
    );
    verifier.require_equal(
        "unreal.groundcover_alpha",
        field(groundcover, "alpha_mode"),
        json!("masked"),
    );
    verifier.require_equal(
        "unreal.groundcover_double_sided",
        field(groundcover, "double_sided"),
        json!(true),
    );
    verifier.require_equal(
        "unreal.groundcover_shadows",
        field(groundcover, "shadows"),
        json!(false),
    );
    check_material_parameter_report(verifier, "unreal", report, manifest, false);
    check_material_recipe_report(verifier, "unreal", report, manifest, false);
    check_engine_import_recipe_report(verifier, "unreal", report, manifest, false);
    check_surface_overlay_report(verifier, "unreal", report, manifest, false);
    check_prototype_surface_target_report(verifier, "unreal", report, manifest, false);
    check_unreal_import_tasks(verifier, "unreal", report, manifest);
    verifier.require_equal(
        "unreal.wind_packing",
        field(report, "wind_packing"),
        field(manifest, "wind_packing")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unreal.scatter_instances",
        field(report, "scatter_binary_instances"),
        json!(222),
    );
    check_unreal_scatter_report(verifier, report, "unreal");
}

fn check_unreal_screenshot_report(
    verifier: &mut Verifier,
    prefix: &str,
    report: Option<&Value>,
    expected_width: u32,
    expected_height: u32,
) {
    let Some(report) = report.filter(|value| value.is_object()) else {
        verifier.fail(
            prefix,
            "Unreal report is missing screenshot artifact summary",
        );
        return;
    };
    verifier.require(
        format!("{prefix}.path"),
        field(report, "path")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        py_string(field(report, "path").unwrap_or(&Value::Null)),
        "Unreal screenshot report must include a nonempty path",
    );
    let status = string_field(field(report, "status")).unwrap_or("");
    verifier.require(
        format!("{prefix}.status"),
        status == "captured" || status == "requested",
        status,
        "Unreal screenshot report must be captured or requested by a supported API",
    );
    verifier.require(
        format!("{prefix}.method"),
        field(report, "method")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        py_string(field(report, "method").unwrap_or(&Value::Null)),
        "Unreal screenshot report must include the capture/request method",
    );
    if status == "captured" {
        verifier.require_equal(
            format!("{prefix}.exists"),
            field(report, "exists"),
            json!(true),
        );
        verifier.require(
            format!("{prefix}.bytes"),
            int_or_default(field(report, "bytes"), 0) > 0,
            format!(
                "{} bytes",
                py_string(field(report, "bytes").unwrap_or(&Value::Null))
            ),
            "Captured Unreal screenshot report must record nonzero bytes",
        );
        require_nonzero_checksum(
            verifier,
            &format!("{prefix}.checksum"),
            field(report, "checksum"),
            "Captured Unreal screenshot report must record a nonzero checksum",
        );
        verifier.require_equal(
            format!("{prefix}.width"),
            field(report, "width"),
            json!(expected_width),
        );
        verifier.require_equal(
            format!("{prefix}.height"),
            field(report, "height"),
            json!(expected_height),
        );
    }
}

pub fn check_unreal_dry_run_report(
    verifier: &mut Verifier,
    report: &Value,
    manifest: &Value,
    package_dir: &Path,
) {
    let console = field(manifest, "console").unwrap_or(&Value::Null);
    let unreal = field(manifest, "unreal").unwrap_or(&Value::Null);
    let groundcover = field(report, "groundcover_material").unwrap_or(&Value::Null);
    for name in ["console", "unreal", "normal_conventions", "wind_packing"] {
        require_manifest_object(verifier, "unreal_dry_run", manifest, name);
    }
    verifier.require_equal(
        "unreal_dry_run.schema_version",
        field(report, "schema_version"),
        field(manifest, "schema_version")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unreal_dry_run.dry_run_true",
        field(report, "dry_run"),
        json!(true),
    );
    verifier.require_equal(
        "unreal_dry_run.profile",
        field(report, "profile"),
        json!("console"),
    );
    check_package_identity(
        verifier,
        "unreal_dry_run",
        report,
        manifest,
        package_dir,
        "unreal_yminus_file",
        false,
    );
    require_manifest_close(
        verifier,
        "unreal_dry_run.tile_size",
        field(report, "tile_size_meters"),
        manifest,
        &["tile_size"],
        0.001,
    );
    for (name, field_name, expected) in [
        (
            "unreal_dry_run.landscape_heightmap",
            "landscape_heightmap",
            field(unreal, "landscape_heightmap")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unreal_dry_run.landscape_weightmap",
            "landscape_weightmap",
            field(unreal, "landscape_weightmap")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unreal_dry_run.normal_map",
            "normal_map",
            field(unreal, "normal_map").cloned().unwrap_or(Value::Null),
        ),
    ] {
        verifier.require_equal(name, field(report, field_name), expected);
    }
    require_manifest_numeric_equal(
        verifier,
        "unreal_dry_run.console_density",
        field(report, "console_density_scale"),
        manifest,
        &["console", "density_scale"],
    );
    for (name, field_name, manifest_field) in [
        (
            "unreal_dry_run.console_lod0_distance_m",
            "console_lod0_max_distance_meters",
            "lod0_max_distance",
        ),
        (
            "unreal_dry_run.console_lod1_distance_m",
            "console_lod1_max_distance_meters",
            "lod1_max_distance",
        ),
        (
            "unreal_dry_run.console_lod2_distance_m",
            "console_lod2_max_distance_meters",
            "lod2_max_distance",
        ),
    ] {
        require_manifest_close(
            verifier,
            name,
            field(report, field_name),
            manifest,
            &["console", manifest_field],
            0.001,
        );
    }
    for (name, field_name, manifest_field) in [
        (
            "unreal_dry_run.console_material_slots",
            "console_material_slots",
            "material_slots",
        ),
        (
            "unreal_dry_run.console_max_instances_per_tile",
            "console_max_instances_per_tile",
            "max_instances_per_tile",
        ),
        (
            "unreal_dry_run.console_max_instances_per_chunk",
            "console_max_instances_per_chunk",
            "max_instances_per_chunk",
        ),
    ] {
        require_manifest_numeric_equal(
            verifier,
            name,
            field(report, field_name),
            manifest,
            &["console", manifest_field],
        );
    }
    for (name, field_name, expected) in [
        (
            "unreal_dry_run.console_shadows",
            "console_shadows",
            field(console, "shadows").cloned().unwrap_or(Value::Null),
        ),
        (
            "unreal_dry_run.console_grass_collision",
            "console_grass_collision",
            field(console, "grass_collision")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "unreal_dry_run.console_moss_collision",
            "console_moss_collision",
            field(console, "moss_collision")
                .cloned()
                .unwrap_or(Value::Null),
        ),
    ] {
        verifier.require_equal(name, field(report, field_name), expected);
    }
    require_manifest_close(
        verifier,
        "unreal_dry_run.console_cull_start_m",
        field(report, "console_cull_start_meters"),
        manifest,
        &["console", "cull_start"],
        0.001,
    );
    require_manifest_close(
        verifier,
        "unreal_dry_run.console_cull_end_m",
        field(report, "console_cull_end_meters"),
        manifest,
        &["console", "cull_end"],
        0.001,
    );
    verifier.require_equal(
        "unreal_dry_run.console_cull_start_cm",
        field(report, "console_cull_start_cm"),
        json!(3500),
    );
    verifier.require_equal(
        "unreal_dry_run.console_cull_end_cm",
        field(report, "console_cull_end_cm"),
        json!(7000),
    );
    let prototype_count = value_len(field(manifest, "prototypes"));
    for (name, field_name, expected) in [
        (
            "unreal_dry_run.prototypes_declared",
            "prototypes_declared",
            json!(prototype_count),
        ),
        (
            "unreal_dry_run.lod0_declared",
            "lod0_prototypes_declared",
            json!(prototype_count),
        ),
        (
            "unreal_dry_run.lod_files_declared",
            "lod_files_declared",
            json!(22),
        ),
        (
            "unreal_dry_run.foliage_type_status",
            "foliage_type_status",
            json!("dry_run"),
        ),
        (
            "unreal_dry_run.foliage_type_expected_count",
            "foliage_type_expected_count",
            json!(prototype_count),
        ),
        (
            "unreal_dry_run.foliage_type_count",
            "foliage_type_count",
            json!(0),
        ),
        (
            "unreal_dry_run.foliage_type_assets",
            "foliage_type_assets",
            json!([]),
        ),
        (
            "unreal_dry_run.foliage_cull_start_cm",
            "foliage_cull_start_cm",
            json!(3500),
        ),
        (
            "unreal_dry_run.foliage_cull_end_cm",
            "foliage_cull_end_cm",
            json!(7000),
        ),
        (
            "unreal_dry_run.material_slots_declared",
            "material_slots_declared",
            json!(value_len(field(manifest, "material_slots"))),
        ),
        (
            "unreal_dry_run.has_terrain_surface_slot",
            "has_terrain_surface_material_slot",
            json!(true),
        ),
        (
            "unreal_dry_run.has_groundcover_slot",
            "has_groundcover_foliage_material_slot",
            json!(true),
        ),
    ] {
        verifier.require_equal(name, field(report, field_name), expected);
    }
    verifier.require_equal(
        "unreal_dry_run.groundcover_alpha",
        field(groundcover, "alpha_mode"),
        json!("masked"),
    );
    verifier.require_equal(
        "unreal_dry_run.groundcover_double_sided",
        field(groundcover, "double_sided"),
        json!(true),
    );
    verifier.require_equal(
        "unreal_dry_run.groundcover_shadows",
        field(groundcover, "shadows"),
        json!(false),
    );
    check_material_parameter_report(verifier, "unreal_dry_run", report, manifest, false);
    check_material_recipe_report(verifier, "unreal_dry_run", report, manifest, false);
    check_engine_import_recipe_report(verifier, "unreal_dry_run", report, manifest, false);
    check_surface_overlay_report(verifier, "unreal_dry_run", report, manifest, false);
    check_prototype_surface_target_report(verifier, "unreal_dry_run", report, manifest, false);
    check_unreal_import_tasks(verifier, "unreal_dry_run", report, manifest);
    verifier.require_equal(
        "unreal_dry_run.wind_packing",
        field(report, "wind_packing"),
        field(manifest, "wind_packing")
            .cloned()
            .unwrap_or(Value::Null),
    );
    verifier.require_equal(
        "unreal_dry_run.scatter_instances",
        field(report, "scatter_binary_instances"),
        json!(222),
    );
    check_unreal_scatter_report(verifier, report, "unreal_dry_run");
}

pub fn check_unreal_import_tasks(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    manifest: &Value,
) {
    let expected_files = expected_unreal_import_files(manifest);
    let destination = string_field(field(report, "destination_path")).unwrap_or("");
    let expected_destinations = expected_unreal_import_destinations(destination, &expected_files);
    let expected_extensions = expected_files
        .iter()
        .filter_map(|relative| {
            Path::new(relative)
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| format!(".{}", extension.to_lowercase()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let expected_maps = expected_unreal_import_map_files(manifest);
    let expected_lod_files = expected_prototype_lod_files(manifest);
    let expected_lod0_files = expected_lod0_prototype_files(manifest);
    verifier.require_equal(
        format!("{prefix}.imported_files"),
        field(report, "imported_files"),
        json!(expected_files),
    );
    verifier.require_equal(
        format!("{prefix}.import_task_count"),
        field(report, "import_task_count"),
        json!(expected_files.len()),
    );
    verifier.require_equal(
        format!("{prefix}.import_task_files"),
        field(report, "import_task_files"),
        json!(expected_files),
    );
    verifier.require_equal(
        format!("{prefix}.import_task_destinations"),
        field(report, "import_task_destination_paths"),
        json!(expected_destinations),
    );
    verifier.require_equal(
        format!("{prefix}.import_task_extensions"),
        field(report, "import_task_extensions"),
        json!(expected_extensions),
    );
    verifier.require_equal(
        format!("{prefix}.imported_manifest"),
        field(report, "imported_manifest"),
        json!(true),
    );
    verifier.require_equal(
        format!("{prefix}.imported_preview_tile"),
        field(report, "imported_preview_tile"),
        json!(true),
    );
    verifier.require_equal(
        format!("{prefix}.imported_map_files"),
        field(report, "imported_map_files"),
        json!(expected_maps.clone()),
    );
    verifier.require_equal(
        format!("{prefix}.imported_map_file_count"),
        field(report, "imported_map_file_count"),
        json!(expected_maps.len()),
    );
    verifier.require_equal(
        format!("{prefix}.imported_prototype_lod_files"),
        field(report, "imported_prototype_lod_files"),
        json!(expected_lod_files.clone()),
    );
    verifier.require_equal(
        format!("{prefix}.imported_prototype_lod_file_count"),
        field(report, "imported_prototype_lod_file_count"),
        json!(expected_lod_files.len()),
    );
    verifier.require_equal(
        format!("{prefix}.imported_lod0_prototype_files"),
        field(report, "imported_lod0_prototype_files"),
        json!(expected_lod0_files.clone()),
    );
    verifier.require_equal(
        format!("{prefix}.imported_lod0_prototype_file_count"),
        field(report, "imported_lod0_prototype_file_count"),
        json!(expected_lod0_files.len()),
    );
    let Some(tasks) = value_as_array(field(report, "import_tasks")) else {
        verifier.fail(
            format!("{prefix}.import_tasks"),
            "Unreal report is missing import task details",
        );
        return;
    };
    verifier.require_equal(
        format!("{prefix}.import_task_report_count"),
        Some(&json!(tasks.len())),
        json!(expected_files.len()),
    );
    for (index, (task, (relative, destination))) in tasks
        .iter()
        .zip(expected_files.iter().zip(expected_destinations.iter()))
        .enumerate()
    {
        let Some(task) = task.as_object().map(|_| task) else {
            verifier.fail(
                format!("{prefix}.import_task.{index}"),
                "Unreal import task report must be an object",
            );
            continue;
        };
        let label = format!("{prefix}.import_task.{index}");
        verifier.require_equal(
            format!("{label}.file"),
            field(task, "file"),
            json!(relative),
        );
        verifier.require_equal(
            format!("{label}.destination_path"),
            field(task, "destination_path"),
            json!(destination),
        );
        let extension = Path::new(relative)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{}", value.to_lowercase()))
            .unwrap_or_default();
        verifier.require_equal(
            format!("{label}.extension"),
            field(task, "extension"),
            json!(extension),
        );
    }
}

pub(crate) fn check_unreal_scatter_report(verifier: &mut Verifier, report: &Value, prefix: &str) {
    verifier.require_equal(
        format!("{prefix}.scatter_chunks"),
        field(report, "scatter_binary_chunks"),
        json!(26),
    );
    verifier.require_equal(
        format!("{prefix}.scatter_records_validated"),
        field(report, "scatter_binary_records_validated"),
        json!(true),
    );
    verifier.require_equal(
        format!("{prefix}.scatter_records_read"),
        field(report, "scatter_binary_records_read"),
        json!(222),
    );
    require_nonzero_checksum(
        verifier,
        &format!("{prefix}.scatter_file_checksum_xor"),
        field(report, "scatter_binary_file_checksum_xor"),
        &format!("{prefix} scatter file checksum XOR must be present and nonzero"),
    );
    require_nonzero_checksum(
        verifier,
        &format!("{prefix}.scatter_record_checksum_xor"),
        field(report, "scatter_binary_record_checksum_xor"),
        &format!("{prefix} scatter record checksum XOR must be present and nonzero"),
    );
    let Some(summaries) = value_as_array(field(report, "scatter_chunks")) else {
        verifier.fail(
            format!("{prefix}.scatter_chunks_report"),
            "Unreal report is missing scatter_chunks",
        );
        return;
    };
    verifier.require_equal(
        format!("{prefix}.scatter_chunk_report_count"),
        Some(&json!(summaries.len())),
        json!(26),
    );
    let mut total = Some(0i64);
    for (index, summary) in summaries.iter().enumerate() {
        let Some(summary) = summary.as_object().map(|_| summary) else {
            verifier.fail(
                format!("{prefix}.scatter_chunk_report.{index}"),
                "summary must be an object",
            );
            continue;
        };
        let label = format!("{prefix}.scatter_chunk_report.{index}");
        let instances = int_or_default(field(summary, "instance_count"), 0);
        if let Some(running) = total {
            total = running.checked_add(instances);
            if total.is_none() {
                verifier.fail(
                    format!("{prefix}.scatter_chunk_report_instances"),
                    "Unreal scatter chunk instance count overflows a signed 64-bit integer",
                );
            }
        }
        verifier.require(
            format!("{label}.instance_count"),
            instances > 0,
            format!("{instances} instances"),
            format!("{prefix} scatter chunk reports must contain at least one instance"),
        );
        require_nonzero_checksum(
            verifier,
            &format!("{label}.file_checksum"),
            field(summary, "file_checksum"),
            &format!("{prefix} scatter chunk file checksum must be present and nonzero"),
        );
        require_nonzero_checksum(
            verifier,
            &format!("{label}.record_checksum"),
            field(summary, "record_checksum"),
            &format!("{prefix} scatter chunk record checksum must be present and nonzero"),
        );
        check_range(
            verifier,
            &format!("{label}.yaw_range"),
            float_or_default(field(summary, "yaw_min"), -1.0),
            float_or_default(field(summary, "yaw_max"), -1.0),
            0.0,
            std::f64::consts::TAU,
            &format!("{prefix} scatter yaw range must be ordered and inside 0..tau"),
        );
        check_range(
            verifier,
            &format!("{label}.phase_range"),
            float_or_default(field(summary, "phase_min"), -1.0),
            float_or_default(field(summary, "phase_max"), -1.0),
            0.0,
            std::f64::consts::TAU,
            &format!("{prefix} scatter phase range must be ordered and inside 0..tau"),
        );
        check_positive_range(
            verifier,
            &format!("{label}.height_range"),
            float_or_default(field(summary, "height_min"), 0.0),
            float_or_default(field(summary, "height_max"), 0.0),
            f64::INFINITY,
            &format!("{prefix} scatter height multiplier range must be positive and ordered"),
        );
        check_positive_range(
            verifier,
            &format!("{label}.width_range"),
            float_or_default(field(summary, "width_min"), 0.0),
            float_or_default(field(summary, "width_max"), 0.0),
            f64::INFINITY,
            &format!("{prefix} scatter width multiplier range must be positive and ordered"),
        );
        check_range(
            verifier,
            &format!("{label}.color_variation_range"),
            float_or_default(field(summary, "color_variation_min"), -1.0),
            float_or_default(field(summary, "color_variation_max"), -1.0),
            0.0,
            1.0,
            &format!("{prefix} scatter color variation range must be ordered and inside 0..1"),
        );
    }
    if let Some(total) = total {
        verifier.require_equal(
            format!("{prefix}.scatter_chunk_report_instances"),
            Some(&json!(total)),
            json!(222),
        );
    }
}

pub(crate) fn check_range(
    verifier: &mut Verifier,
    name: &str,
    minimum: f64,
    maximum: f64,
    lower: f64,
    upper: f64,
    failure: &str,
) {
    verifier.require(
        name,
        minimum >= lower && minimum <= maximum && maximum <= upper,
        format!("{minimum}..{maximum}"),
        failure,
    );
}

pub(crate) fn check_positive_range(
    verifier: &mut Verifier,
    name: &str,
    minimum: f64,
    maximum: f64,
    upper: f64,
    failure: &str,
) {
    verifier.require(
        name,
        minimum > 0.0 && minimum <= maximum && maximum <= upper,
        format!("{minimum}..{maximum}"),
        failure,
    );
}
