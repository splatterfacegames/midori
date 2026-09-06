use super::common::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn object_by_string_field<'a>(
    values: Option<&'a Vec<Value>>,
    field_name: &str,
) -> BTreeMap<String, &'a Value> {
    values
        .into_iter()
        .flatten()
        .filter_map(|value| {
            value.as_object().and_then(|_| {
                string_field(field(value, field_name)).map(|key| (key.to_string(), value))
            })
        })
        .collect()
}

fn string_set(value: Option<&Value>) -> BTreeSet<String> {
    value_as_array(value)
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

pub fn check_midori_report(verifier: &mut Verifier, report: &Value) -> Option<Value> {
    let Some(manifest) = field(report, "manifest").filter(|value| value.is_object()) else {
        verifier.fail("midori.manifest", "validation report is missing manifest");
        return None;
    };
    verifier.require_equal(
        "midori.schema_version",
        field(manifest, "schema_version"),
        json!(3),
    );
    verifier.require_equal(
        "midori.asset_name",
        field(manifest, "asset_name"),
        json!("Temperate Forest Floor"),
    );
    require_array_len(verifier, "midori.map_count", field(report, "map_files"), 5);
    check_midori_map_summaries(verifier, report, manifest);
    check_midori_map_relationships(verifier, report, manifest);
    require_array_len(
        verifier,
        "midori.prototype_file_count",
        field(report, "prototype_files"),
        22,
    );
    check_midori_prototype_summaries(verifier, report, manifest);
    check_prototype_surface_targets(verifier, "midori", manifest);
    check_midori_profile_budgets(verifier, report, manifest);
    require_array_len(
        verifier,
        "midori.scatter_binary_chunks",
        field(report, "scatter_binary_files"),
        26,
    );
    verifier.require_equal(
        "midori.scatter_binary_instances",
        field(report, "scatter_binary_instances"),
        json!(222),
    );
    check_midori_scatter_summaries(verifier, report);
    verifier.require_equal(
        "midori.shader_policy",
        field(manifest, "shader_policy"),
        json!("preview_only"),
    );
    verifier.require_equal(
        "midori.texture_pipeline",
        field(manifest, "texture_pipeline"),
        json!("parked"),
    );
    check_material_parameters(verifier, "midori", field(manifest, "material_parameters"));
    check_material_recipes(verifier, "midori", manifest, report);
    check_engine_import_recipes(verifier, "midori", manifest, report);
    check_surface_overlays(verifier, "midori", field(manifest, "surface_overlays"));
    check_midori_memory_footprint(verifier, report, manifest);
    Some(manifest.clone())
}

fn require_array_len(verifier: &mut Verifier, name: &str, value: Option<&Value>, expected: usize) {
    verifier.require_equal(name, Some(&json!(value_len(value))), json!(expected));
}

pub fn check_surface_overlays(verifier: &mut Verifier, prefix: &str, overlays: Option<&Value>) {
    let Some(overlays) = value_as_array(overlays) else {
        verifier.fail(
            format!("{prefix}.surface_overlays"),
            "manifest is missing surface_overlays",
        );
        return;
    };
    verifier.require_equal(
        format!("{prefix}.surface_overlay_count"),
        Some(&json!(overlays.len())),
        json!(3),
    );
    let by_name = object_by_string_field(Some(overlays), "name");
    let expected: [(&str, &str, &[&str]); 3] = [
        ("moss", "G", &["terrain_surface", "rock", "log"]),
        ("wetness", "B", &["terrain_surface", "rock", "log"]),
        ("cracks", "A", &["terrain_surface", "scatter_exclusion"]),
    ];
    for (name, channel, targets) in expected {
        let Some(overlay) = by_name.get(name) else {
            verifier.fail(
                format!("{prefix}.surface_overlay.{name}"),
                "surface overlay is missing",
            );
            continue;
        };
        verifier.require_equal(
            format!("{prefix}.surface_overlay.{name}.source_file"),
            field(overlay, "source_file"),
            json!("maps/masks_rgba.png"),
        );
        verifier.require_equal(
            format!("{prefix}.surface_overlay.{name}.channel"),
            field(overlay, "channel"),
            json!(channel),
        );
        let actual_targets = string_set(field(overlay, "targets"));
        let required = targets.iter().copied().collect::<BTreeSet<_>>();
        verifier.require(
            format!("{prefix}.surface_overlay.{name}.targets"),
            required
                .iter()
                .all(|target| actual_targets.contains(*target)),
            format!("{actual_targets:?} includes {required:?}"),
            format!("{name} surface overlay must target {required:?}"),
        );
        verifier.require_equal(
            format!("{prefix}.surface_overlay.{name}.runtime_policy"),
            field(overlay, "runtime_policy"),
            json!("baked_static"),
        );
    }
}

fn required_material_parameter_semantics() -> [(&'static str, &'static [&'static str]); 2] {
    [
        (
            "terrain_surface",
            &[
                "overlay_mask_texture",
                "moss_mask_channel",
                "wetness_mask_channel",
                "crack_mask_channel",
            ],
        ),
        (
            "groundcover_foliage",
            &[
                "alpha_cutoff",
                "wind_strength",
                "wind_speed",
                "wind_direction_degrees",
                "wind_gust_scale",
                "fade_start_meters",
                "fade_end_meters",
                "color_variation_scale",
            ],
        ),
    ]
}

pub fn check_material_parameters(
    verifier: &mut Verifier,
    prefix: &str,
    material_parameters: Option<&Value>,
) {
    let Some(material_parameters) = value_as_array(material_parameters) else {
        verifier.fail(
            format!("{prefix}.material_parameters"),
            "manifest is missing material_parameters",
        );
        return;
    };
    verifier.require_equal(
        format!("{prefix}.material_parameter_set_count"),
        Some(&json!(material_parameters.len())),
        json!(2),
    );
    let by_slot = object_by_string_field(Some(material_parameters), "material_slot");
    for (slot, required_semantics) in required_material_parameter_semantics() {
        let Some(material_set) = by_slot.get(slot) else {
            verifier.fail(
                format!("{prefix}.material_parameters.{slot}"),
                "material parameter set is missing",
            );
            continue;
        };
        verifier.require_equal(
            format!("{prefix}.material_parameters.{slot}.runtime_policy"),
            field(material_set, "runtime_policy"),
            json!("engine_native_static"),
        );
        let semantics = value_as_array(field(material_set, "parameters"))
            .into_iter()
            .flatten()
            .filter_map(|parameter| string_field(field(parameter, "semantic")).map(str::to_string))
            .collect::<BTreeSet<_>>();
        let required = required_semantics.iter().copied().collect::<BTreeSet<_>>();
        verifier.require(
            format!("{prefix}.material_parameters.{slot}.semantics"),
            required
                .iter()
                .all(|semantic| semantics.contains(*semantic)),
            format!("{semantics:?} includes {required:?}"),
            format!("{slot} material parameters must include required semantics"),
        );
        let names = value_as_array(field(material_set, "parameters"))
            .into_iter()
            .flatten()
            .filter_map(|parameter| string_field(field(parameter, "name")).map(str::to_string))
            .collect::<BTreeSet<_>>();
        let required_names: BTreeSet<&str> = if slot == "terrain_surface" {
            ["Midori_MaskTexture", "Midori_MossMaskChannel"]
                .into_iter()
                .collect()
        } else {
            ["Midori_WindStrength", "Midori_FadeEndMeters"]
                .into_iter()
                .collect()
        };
        verifier.require(
            format!("{prefix}.material_parameters.{slot}.names"),
            required_names.iter().all(|name| names.contains(*name)),
            format!("{names:?} includes {required_names:?}"),
            format!("{slot} material parameters must include stable engine names"),
        );
    }
}

pub fn check_material_parameter_report(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    manifest: &Value,
    camel_case: bool,
) {
    let Some(material_parameters) = value_as_array(field(manifest, "material_parameters")) else {
        verifier.fail(
            format!("{prefix}.material_parameter_report"),
            "manifest is missing material_parameters",
        );
        return;
    };
    let (count, slots, policies, semantics, groundcover_names, terrain_names) = if camel_case {
        (
            "materialParameterSetCount",
            "materialParameterSlots",
            "materialParameterRuntimePolicies",
            "materialParameterSemantics",
            "groundcoverMaterialParameterNames",
            "terrainMaterialParameterNames",
        )
    } else {
        (
            "material_parameter_set_count",
            "material_parameter_slots",
            "material_parameter_runtime_policies",
            "material_parameter_semantics",
            "groundcover_material_parameter_names",
            "terrain_material_parameter_names",
        )
    };
    verifier.require_equal(
        format!("{prefix}.material_parameter_set_count"),
        field(report, count),
        json!(material_parameters.len()),
    );
    verifier.require_equal(
        format!("{prefix}.material_parameter_slots"),
        field(report, slots),
        json!(
            material_parameters
                .iter()
                .map(|item| string_field(field(item, "material_slot")).unwrap_or(""))
                .collect::<Vec<_>>()
        ),
    );
    verifier.require_equal(
        format!("{prefix}.material_parameter_runtime_policies"),
        field(report, policies),
        json!(
            material_parameters
                .iter()
                .map(|item| string_field(field(item, "runtime_policy")).unwrap_or(""))
                .collect::<Vec<_>>()
        ),
    );
    verifier.require_equal(
        format!("{prefix}.material_parameter_semantics"),
        field(report, semantics),
        json!(expected_material_parameter_semantics(manifest)),
    );
    verifier.require_equal(
        format!("{prefix}.groundcover_material_parameter_names"),
        field(report, groundcover_names),
        json!(expected_material_parameter_names_for_slot(
            manifest,
            "groundcover_foliage"
        )),
    );
    verifier.require_equal(
        format!("{prefix}.terrain_material_parameter_names"),
        field(report, terrain_names),
        json!(expected_material_parameter_names_for_slot(
            manifest,
            "terrain_surface"
        )),
    );
}

pub fn check_material_recipes(
    verifier: &mut Verifier,
    prefix: &str,
    manifest: &Value,
    report: &Value,
) {
    let Some(recipes) = value_as_array(field(manifest, "material_recipes")) else {
        verifier.fail(
            format!("{prefix}.material_recipes"),
            "manifest is missing material_recipes",
        );
        return;
    };
    verifier.require_equal(
        format!("{prefix}.material_recipe_count"),
        Some(&json!(recipes.len())),
        json!(2),
    );
    let by_slot = object_by_string_field(Some(recipes), "material_slot");
    let required_targets: [(&str, &[&str]); 2] = [
        (
            "terrain_surface",
            &["unity_terrain_material", "unreal_landscape_material"],
        ),
        (
            "groundcover_foliage",
            &[
                "unity_detail_mesh_material",
                "unreal_static_mesh_foliage_material",
            ],
        ),
    ];
    for (slot, targets) in required_targets {
        let Some(recipe) = by_slot.get(slot) else {
            verifier.fail(
                format!("{prefix}.material_recipe.{slot}"),
                "material recipe is missing",
            );
            continue;
        };
        verifier.require_equal(
            format!("{prefix}.material_recipe.{slot}.runtime_policy"),
            field(recipe, "runtime_policy"),
            json!("engine_native_static"),
        );
        let file_name = string_field(field(recipe, "file")).unwrap_or("");
        verifier.require(
            format!("{prefix}.material_recipe.{slot}.file"),
            file_name.starts_with("materials/") && file_name.ends_with(".recipe.json"),
            file_name,
            format!("{slot} material recipe must live under materials/*.recipe.json"),
        );
        let actual_targets = string_set(field(recipe, "engine_targets"));
        let required = targets.iter().copied().collect::<BTreeSet<_>>();
        verifier.require(
            format!("{prefix}.material_recipe.{slot}.engine_targets"),
            required
                .iter()
                .all(|target| actual_targets.contains(*target)),
            format!("{actual_targets:?} includes {required:?}"),
            format!("{slot} material recipe must target Unity and Unreal systems"),
        );
    }

    let Some(summaries) = value_as_array(field(report, "material_recipe_summaries")) else {
        return;
    };
    verifier.require_equal(
        format!("{prefix}.material_recipe_summary_count"),
        Some(&json!(summaries.len())),
        json!(recipes.len()),
    );
    let summary_by_slot = object_by_string_field(Some(summaries), "material_slot");
    for slot in ["terrain_surface", "groundcover_foliage"] {
        let Some(summary) = summary_by_slot.get(slot) else {
            verifier.fail(
                format!("{prefix}.material_recipe_summary.{slot}"),
                "summary is missing",
            );
            continue;
        };
        verifier.require_equal(
            format!("{prefix}.material_recipe_summary.{slot}.runtime_policy"),
            field(summary, "runtime_policy"),
            json!("engine_native_static"),
        );
        verifier.require_equal(
            format!("{prefix}.material_recipe_summary.{slot}.shader_policy"),
            field(summary, "shader_policy"),
            json!("preview_only"),
        );
        verifier.require_equal(
            format!("{prefix}.material_recipe_summary.{slot}.texture_pipeline"),
            field(summary, "texture_pipeline"),
            json!("parked"),
        );
        verifier.require(
            format!("{prefix}.material_recipe_summary.{slot}.checksum"),
            int_or_default(field(summary, "file_checksum"), 0) != 0,
            format!(
                "{} is nonzero",
                py_string(field(summary, "file_checksum").unwrap_or(&Value::Null))
            ),
            format!("{slot} material recipe checksum must be nonzero"),
        );
        if slot == "terrain_surface" {
            verifier.require(
                format!("{prefix}.material_recipe_summary.{slot}.required_textures"),
                string_set(field(summary, "required_textures")).contains("maps/masks_rgba.png"),
                py_repr(field(summary, "required_textures").unwrap_or(&Value::Null)),
                "terrain material recipe must require masks_rgba.png",
            );
            let overlays = string_set(field(summary, "surface_overlays"));
            let required = ["moss", "wetness", "cracks"];
            verifier.require(
                format!("{prefix}.material_recipe_summary.{slot}.surface_overlays"),
                required.iter().all(|item| overlays.contains(*item)),
                py_repr(field(summary, "surface_overlays").unwrap_or(&Value::Null)),
                "terrain material recipe must list static overlays",
            );
        } else {
            let streams = string_set(field(summary, "required_vertex_streams"));
            verifier.require(
                format!("{prefix}.material_recipe_summary.{slot}.vertex_streams"),
                streams
                    .iter()
                    .any(|stream| stream.starts_with("TEXCOORD_1.x"))
                    && streams.iter().any(|stream| stream.starts_with("COLOR_0.y")),
                format!("{streams:?}"),
                "groundcover material recipe must require wind/color streams",
            );
        }
    }
}

pub fn check_material_recipe_report(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    manifest: &Value,
    camel_case: bool,
) {
    let Some(recipes) = value_as_array(field(manifest, "material_recipes")) else {
        verifier.fail(
            format!("{prefix}.material_recipe_report"),
            "manifest is missing material_recipes",
        );
        return;
    };
    let (count, files, policies, targets, terrain_file, groundcover_file) = if camel_case {
        (
            "materialRecipeCount",
            "materialRecipeFiles",
            "materialRecipeRuntimePolicies",
            "materialRecipeEngineTargets",
            "terrainMaterialRecipeFile",
            "groundcoverMaterialRecipeFile",
        )
    } else {
        (
            "material_recipe_count",
            "material_recipe_files",
            "material_recipe_runtime_policies",
            "material_recipe_engine_targets",
            "terrain_material_recipe_file",
            "groundcover_material_recipe_file",
        )
    };
    verifier.require_equal(
        format!("{prefix}.material_recipe_count"),
        field(report, count),
        json!(recipes.len()),
    );
    verifier.require_equal(
        format!("{prefix}.material_recipe_files"),
        field(report, files),
        json!(expected_material_recipe_files(manifest)),
    );
    verifier.require_equal(
        format!("{prefix}.material_recipe_runtime_policies"),
        field(report, policies),
        json!(
            recipes
                .iter()
                .map(|item| string_field(field(item, "runtime_policy")).unwrap_or(""))
                .collect::<Vec<_>>()
        ),
    );
    verifier.require_equal(
        format!("{prefix}.material_recipe_engine_targets"),
        field(report, targets),
        json!(expected_material_recipe_engine_targets(manifest)),
    );
    verifier.require_equal(
        format!("{prefix}.terrain_material_recipe_file"),
        field(report, terrain_file),
        json!(material_recipe_file_for_slot(manifest, "terrain_surface")),
    );
    verifier.require_equal(
        format!("{prefix}.groundcover_material_recipe_file"),
        field(report, groundcover_file),
        json!(material_recipe_file_for_slot(
            manifest,
            "groundcover_foliage"
        )),
    );
}

pub fn check_engine_import_recipes(
    verifier: &mut Verifier,
    prefix: &str,
    manifest: &Value,
    report: &Value,
) {
    let Some(recipes) = value_as_array(field(manifest, "engine_import_recipes")) else {
        verifier.fail(
            format!("{prefix}.engine_import_recipes"),
            "manifest is missing engine_import_recipes",
        );
        return;
    };
    verifier.require_equal(
        format!("{prefix}.engine_import_recipe_count"),
        Some(&json!(recipes.len())),
        json!(2),
    );
    let by_engine = object_by_string_field(Some(recipes), "engine");
    let required: [(&str, &str, &[&str]); 2] = [
        (
            "unity",
            "mobile",
            &[
                "Unity TerrainData",
                "GPU-instanced terrain detail mesh prefabs",
            ],
        ),
        (
            "unreal",
            "console",
            &["Unreal Landscape", "Static Mesh Foliage"],
        ),
    ];
    for (engine, profile, systems) in required {
        let Some(recipe) = by_engine.get(engine) else {
            verifier.fail(
                format!("{prefix}.engine_import_recipe.{engine}"),
                "engine import recipe is missing",
            );
            continue;
        };
        verifier.require_equal(
            format!("{prefix}.engine_import_recipe.{engine}.profile"),
            field(recipe, "profile"),
            json!(profile),
        );
        verifier.require_equal(
            format!("{prefix}.engine_import_recipe.{engine}.runtime_policy"),
            field(recipe, "runtime_policy"),
            json!("engine_native_static"),
        );
        let file_name = string_field(field(recipe, "file")).unwrap_or("");
        verifier.require(
            format!("{prefix}.engine_import_recipe.{engine}.file"),
            file_name.starts_with("engines/") && file_name.ends_with(".recipe.json"),
            file_name,
            format!("{engine} engine import recipe must live under engines/*.recipe.json"),
        );
        let actual_systems = string_set(field(recipe, "expected_systems"));
        let required_systems = systems.iter().copied().collect::<BTreeSet<_>>();
        verifier.require(
            format!("{prefix}.engine_import_recipe.{engine}.expected_systems"),
            required_systems
                .iter()
                .all(|system| actual_systems.contains(*system)),
            format!("{actual_systems:?} includes {required_systems:?}"),
            format!("{engine} engine import recipe must list required engine-native systems"),
        );
    }

    let Some(summaries) = value_as_array(field(report, "engine_import_recipe_summaries")) else {
        return;
    };
    verifier.require_equal(
        format!("{prefix}.engine_import_recipe_summary_count"),
        Some(&json!(summaries.len())),
        json!(recipes.len()),
    );
    let summary_by_engine = object_by_string_field(Some(summaries), "engine");
    for engine in ["unity", "unreal"] {
        let Some(summary) = summary_by_engine.get(engine) else {
            verifier.fail(
                format!("{prefix}.engine_import_recipe_summary.{engine}"),
                "summary is missing",
            );
            continue;
        };
        let normal_key = if engine == "unreal" {
            "unreal_yminus_file"
        } else {
            "unity_yplus_file"
        };
        verifier.require_equal(
            format!("{prefix}.engine_import_recipe_summary.{engine}.source_file_count"),
            field(summary, "source_file_count"),
            json!(expected_source_files(manifest, normal_key).len()),
        );
        verifier.require_equal(
            format!("{prefix}.engine_import_recipe_summary.{engine}.scatter_instances"),
            field(summary, "scatter_instances"),
            json!(222),
        );
        verifier.require_equal(
            format!("{prefix}.engine_import_recipe_summary.{engine}.scatter_binary_chunks"),
            field(summary, "scatter_binary_chunks"),
            json!(26),
        );
        verifier.require(
            format!("{prefix}.engine_import_recipe_summary.{engine}.checksum"),
            int_or_default(field(summary, "file_checksum"), 0) != 0,
            format!(
                "{} is nonzero",
                py_string(field(summary, "file_checksum").unwrap_or(&Value::Null))
            ),
            format!("{engine} engine import recipe checksum must be nonzero"),
        );
    }
}

pub fn check_engine_import_recipe_report(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    manifest: &Value,
    camel_case: bool,
) {
    let Some(recipes) = value_as_array(field(manifest, "engine_import_recipes")) else {
        verifier.fail(
            format!("{prefix}.engine_import_recipe_report"),
            "manifest is missing engine_import_recipes",
        );
        return;
    };
    let (count, files, profiles, policies, systems, unity_file, unreal_file) = if camel_case {
        (
            "engineImportRecipeCount",
            "engineImportRecipeFiles",
            "engineImportRecipeProfiles",
            "engineImportRecipeRuntimePolicies",
            "engineImportRecipeExpectedSystems",
            "unityEngineImportRecipeFile",
            "unrealEngineImportRecipeFile",
        )
    } else {
        (
            "engine_import_recipe_count",
            "engine_import_recipe_files",
            "engine_import_recipe_profiles",
            "engine_import_recipe_runtime_policies",
            "engine_import_recipe_expected_systems",
            "unity_engine_import_recipe_file",
            "unreal_engine_import_recipe_file",
        )
    };
    verifier.require_equal(
        format!("{prefix}.engine_import_recipe_count"),
        field(report, count),
        json!(recipes.len()),
    );
    verifier.require_equal(
        format!("{prefix}.engine_import_recipe_files"),
        field(report, files),
        json!(expected_engine_import_recipe_files(manifest)),
    );
    verifier.require_equal(
        format!("{prefix}.engine_import_recipe_profiles"),
        field(report, profiles),
        json!(expected_engine_import_recipe_profiles(manifest)),
    );
    verifier.require_equal(
        format!("{prefix}.engine_import_recipe_runtime_policies"),
        field(report, policies),
        json!(
            recipes
                .iter()
                .map(|item| string_field(field(item, "runtime_policy")).unwrap_or(""))
                .collect::<Vec<_>>()
        ),
    );
    verifier.require_equal(
        format!("{prefix}.engine_import_recipe_expected_systems"),
        field(report, systems),
        json!(expected_engine_import_recipe_systems(manifest)),
    );
    verifier.require_equal(
        format!("{prefix}.unity_engine_import_recipe_file"),
        field(report, unity_file),
        json!(engine_import_recipe_file_for_engine(manifest, "unity")),
    );
    verifier.require_equal(
        format!("{prefix}.unreal_engine_import_recipe_file"),
        field(report, unreal_file),
        json!(engine_import_recipe_file_for_engine(manifest, "unreal")),
    );
}

pub fn check_surface_overlay_report(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    manifest: &Value,
    camel_case: bool,
) {
    let Some(overlays) = value_as_array(field(manifest, "surface_overlays")) else {
        verifier.fail(
            format!("{prefix}.surface_overlay_report"),
            "manifest is missing surface_overlays",
        );
        return;
    };
    let names: Vec<String> = overlays
        .iter()
        .filter_map(|item| string_field(field(item, "name")).map(str::to_string))
        .collect();
    let sources: Vec<String> = overlays
        .iter()
        .filter_map(|item| string_field(field(item, "source_file")).map(str::to_string))
        .collect();
    let channels: Vec<String> = overlays
        .iter()
        .filter_map(|item| string_field(field(item, "channel")).map(str::to_string))
        .collect();
    let policies: Vec<String> = overlays
        .iter()
        .filter_map(|item| string_field(field(item, "runtime_policy")).map(str::to_string))
        .collect();
    let (count, names_field, sources_field, channels_field, policies_field, targets_field) =
        if camel_case {
            (
                "surfaceOverlayCount",
                "surfaceOverlayNames",
                "surfaceOverlaySourceFiles",
                "surfaceOverlayChannels",
                "surfaceOverlayRuntimePolicies",
                "surfaceOverlayTargets",
            )
        } else {
            (
                "surface_overlay_count",
                "surface_overlay_names",
                "surface_overlay_source_files",
                "surface_overlay_channels",
                "surface_overlay_runtime_policies",
                "surface_overlay_targets",
            )
        };
    verifier.require_equal(
        format!("{prefix}.surface_overlay_count"),
        field(report, count),
        json!(overlays.len()),
    );
    verifier.require_equal(
        format!("{prefix}.surface_overlay_names"),
        field(report, names_field),
        json!(names),
    );
    verifier.require_equal(
        format!("{prefix}.surface_overlay_source_files"),
        field(report, sources_field),
        json!(sources),
    );
    verifier.require_equal(
        format!("{prefix}.surface_overlay_channels"),
        field(report, channels_field),
        json!(channels),
    );
    verifier.require_equal(
        format!("{prefix}.surface_overlay_runtime_policies"),
        field(report, policies_field),
        json!(policies),
    );
    let mut reported_targets = field(report, targets_field).cloned();
    if camel_case {
        if let Some(items) = reported_targets.as_mut().and_then(Value::as_array_mut) {
            for item in items {
                if let Some(text) = item.as_str() {
                    *item = json!(text.split(',').map(str::to_string).collect::<Vec<_>>());
                }
            }
        }
    }
    let moss_targets = overlays
        .iter()
        .find_map(|overlay| {
            (string_field(field(overlay, "name")) == Some("moss")).then(|| {
                field(overlay, "targets")
                    .cloned()
                    .unwrap_or(Value::Array(Vec::new()))
            })
        })
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let matches = reported_targets
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(|item| values_equal(item, &moss_targets)));
    verifier.require(
        format!("{prefix}.surface_overlay_moss_targets"),
        matches,
        format!("{} reported", py_repr(&moss_targets)),
        "import report must include moss surface overlay targets",
    );
}

pub fn check_prototype_surface_targets(verifier: &mut Verifier, prefix: &str, manifest: &Value) {
    let Some(prototypes) = value_as_array(field(manifest, "prototypes")) else {
        verifier.fail(
            format!("{prefix}.prototype_surface_targets"),
            "manifest is missing prototypes",
        );
        return;
    };
    let mut by_kind: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for prototype in prototypes {
        let name = string_field(field(prototype, "name")).unwrap_or("");
        let kind = string_field(field(prototype, "kind")).unwrap_or("");
        let targets = string_set(field(prototype, "surface_targets"));
        verifier.require(
            format!("{prefix}.prototype_surface_targets.{name}"),
            !targets.is_empty(),
            format!("{targets:?}"),
            "prototype must declare at least one surface target",
        );
        let required = required_surface_targets_for_kind(kind);
        let required_set = required.iter().copied().collect::<BTreeSet<_>>();
        verifier.require(
            format!("{prefix}.prototype_surface_targets.{name}.required"),
            required_set.iter().all(|target| targets.contains(*target)),
            format!("{targets:?} includes {required_set:?}"),
            format!("{kind} prototype must include required surface targets"),
        );
        by_kind.entry(kind.to_string()).or_default().extend(targets);
    }
    for (kind, required) in [
        ("rock", &["static_surface", "rock"][..]),
        ("log", &["static_surface", "log"][..]),
        ("shrub", &["groundcover_foliage", "shrub_base"][..]),
    ] {
        let actual = by_kind.get(kind).cloned().unwrap_or_default();
        verifier.require(
            format!("{prefix}.prototype_surface_targets.{kind}"),
            required.iter().all(|target| actual.contains(*target)),
            format!("{actual:?} includes {required:?}"),
            format!("{kind} prototypes must expose overlay-compatible surface targets"),
        );
    }
}

pub fn check_prototype_surface_target_report(
    verifier: &mut Verifier,
    prefix: &str,
    report: &Value,
    manifest: &Value,
    camel_case: bool,
) {
    let (all_field, rock_field, log_field, shrub_field, expected_all) = if camel_case {
        (
            "prototypeSurfaceTargets",
            "rockPrototypeSurfaceTargets",
            "logPrototypeSurfaceTargets",
            "shrubPrototypeSurfaceTargets",
            json!(expected_prototype_surface_target_entries(manifest)),
        )
    } else {
        let expected = value_as_array(field(manifest, "prototypes"))
            .map(|items| items.iter().map(|prototype| json!({
                "name": string_field(field(prototype, "name")).unwrap_or(""),
                "kind": string_field(field(prototype, "kind")).unwrap_or(""),
                "targets": field(prototype, "surface_targets").cloned().unwrap_or_else(|| json!([])),
            })).collect::<Vec<_>>())
            .unwrap_or_default();
        (
            "prototype_surface_targets",
            "rock_prototype_surface_targets",
            "log_prototype_surface_targets",
            "shrub_prototype_surface_targets",
            json!(expected),
        )
    };
    verifier.require_equal(
        format!("{prefix}.prototype_surface_targets"),
        field(report, all_field),
        expected_all,
    );
    for (kind, field_name, required) in [
        ("rock", rock_field, &["static_surface", "rock"][..]),
        ("log", log_field, &["static_surface", "log"][..]),
        (
            "shrub",
            shrub_field,
            &["groundcover_foliage", "shrub_base"][..],
        ),
    ] {
        let actual = string_set(field(report, field_name));
        let required_set = required.iter().copied().collect::<BTreeSet<_>>();
        verifier.require(
            format!("{prefix}.{kind}_prototype_surface_targets"),
            required_set.iter().all(|target| actual.contains(*target)),
            format!("{actual:?} includes {required_set:?}"),
            format!("{prefix} report must include {kind} prototype surface targets"),
        );
    }
}

pub fn check_midori_memory_footprint(verifier: &mut Verifier, report: &Value, manifest: &Value) {
    let Some(footprint) = field(manifest, "memory_footprint").filter(|value| value.is_object())
    else {
        verifier.fail(
            "midori.memory_footprint",
            "manifest is missing memory_footprint",
        );
        return;
    };
    let Some(summary) = field(report, "memory_footprint").filter(|value| value.is_object()) else {
        verifier.fail(
            "midori.memory_footprint_summary",
            "report is missing memory_footprint",
        );
        return;
    };
    for field_name in [
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
    ] {
        verifier.require_equal(
            format!("midori.memory_footprint.{field_name}"),
            field(summary, field_name),
            field(footprint, field_name).cloned().unwrap_or(Value::Null),
        );
    }
    let map_resolution = int_or_default(field(manifest, "map_resolution"), 0);
    let map_pixel_count = int_or_default(field(footprint, "map_pixel_count"), 0);
    let decoded_map_bytes = int_or_default(field(footprint, "decoded_map_bytes"), 0);
    let scatter_chunk_count = value_as_array(field_path(manifest, &["scatter", "binary_files"]))
        .map_or(0, Vec::len) as i64;
    let scatter_instance_count = value_as_array(field_path(manifest, &["scatter", "binary_files"]))
        .into_iter()
        .flatten()
        .map(|item| int_or_default(field(item, "instance_count"), 0))
        .sum::<i64>();
    let expected_total = [
        "encoded_map_bytes",
        "material_recipe_bytes",
        "engine_import_recipe_bytes",
        "preview_mesh_bytes",
        "prototype_mesh_bytes",
        "scatter_json_bytes",
        "scatter_binary_bytes",
    ]
    .into_iter()
    .map(|field_name| int_or_default(field(footprint, field_name), 0))
    .sum::<i64>();
    verifier.require_equal(
        "midori.memory_footprint.map_pixels",
        Some(&json!(map_pixel_count)),
        json!(map_resolution.saturating_mul(map_resolution)),
    );
    verifier.require_equal(
        "midori.memory_footprint.decoded_maps",
        Some(&json!(decoded_map_bytes)),
        json!(map_pixel_count.saturating_mul(13)),
    );
    verifier.require_equal(
        "midori.memory_footprint.scatter_binary_header_bytes",
        field(footprint, "scatter_binary_header_bytes"),
        json!(scatter_chunk_count.saturating_mul(16)),
    );
    verifier.require_equal(
        "midori.memory_footprint.scatter_binary_record_bytes",
        field(footprint, "scatter_binary_record_bytes"),
        json!(scatter_instance_count.saturating_mul(32)),
    );
    let scatter_total =
        int_or_default(field(footprint, "scatter_binary_header_bytes"), 0).saturating_add(
            int_or_default(field(footprint, "scatter_binary_record_bytes"), 0),
        );
    verifier.require_equal(
        "midori.memory_footprint.scatter_binary_total",
        Some(&json!(int_or_default(
            field(footprint, "scatter_binary_bytes"),
            0
        ))),
        json!(scatter_total),
    );
    verifier.require_equal(
        "midori.memory_footprint.total_payload",
        Some(&json!(int_or_default(
            field(footprint, "total_payload_bytes"),
            0
        ))),
        json!(expected_total),
    );
    for field_name in [
        "encoded_map_bytes",
        "material_recipe_bytes",
        "engine_import_recipe_bytes",
        "preview_mesh_bytes",
        "prototype_mesh_bytes",
        "scatter_json_bytes",
        "scatter_binary_bytes",
        "total_payload_bytes",
    ] {
        verifier.require(
            format!("midori.memory_footprint.{field_name}_nonzero"),
            int_or_default(field(footprint, field_name), 0) > 0,
            format!(
                "{} bytes",
                py_string(field(footprint, field_name).unwrap_or(&Value::Null))
            ),
            "default validation package should record nonzero payload bytes",
        );
    }
}

pub fn check_midori_map_summaries(verifier: &mut Verifier, report: &Value, manifest: &Value) {
    let Some(summaries) = value_as_array(field(report, "map_summaries")) else {
        verifier.fail(
            "midori.map_summaries",
            "validation report is missing map_summaries",
        );
        return;
    };
    verifier.require_equal(
        "midori.map_summary_count",
        Some(&json!(summaries.len())),
        json!(EXPECTED_MAP_SUMMARIES.len()),
    );
    let by_name = summaries
        .iter()
        .filter_map(|summary| {
            let file = string_field(field(summary, "file"))?;
            let normalized = file.replace('\\', "/");
            let name = normalized.rsplit('/').next().unwrap_or(file);
            Some((name.to_string(), summary))
        })
        .collect::<BTreeMap<_, _>>();
    let map_resolution = field(manifest, "map_resolution")
        .cloned()
        .unwrap_or(Value::Null);
    for (file_name, color_type, channels, varying_channels, varying_any_channels) in
        EXPECTED_MAP_SUMMARIES
    {
        let Some(summary) = by_name.get(*file_name) else {
            verifier.fail(format!("midori.map.{file_name}"), "map summary is missing");
            continue;
        };
        verifier.require_equal(
            format!("midori.map.{file_name}.width"),
            field(summary, "width"),
            map_resolution.clone(),
        );
        verifier.require_equal(
            format!("midori.map.{file_name}.height"),
            field(summary, "height"),
            map_resolution.clone(),
        );
        verifier.require_equal(
            format!("midori.map.{file_name}.color_type"),
            field(summary, "color_type"),
            json!(color_type),
        );
        verifier.require_equal(
            format!("midori.map.{file_name}.channels"),
            field(summary, "channels"),
            json!(*channels),
        );
        verifier.require(
            format!("midori.map.{file_name}.checksum"),
            int_or_default(field(summary, "file_checksum"), 0) > 0,
            format!(
                "{} is a nonzero encoded PNG checksum",
                py_string(field(summary, "file_checksum").unwrap_or(&Value::Null))
            ),
            "encoded PNG checksum must be present and nonzero",
        );
        let Some(channel_min) = value_as_array(field(summary, "channel_min")) else {
            verifier.fail(
                format!("midori.map.{file_name}.range"),
                "channel ranges are missing",
            );
            continue;
        };
        let Some(channel_max) = value_as_array(field(summary, "channel_max")) else {
            verifier.fail(
                format!("midori.map.{file_name}.range"),
                "channel ranges are missing",
            );
            continue;
        };
        for channel in *varying_channels {
            let Some(minimum) = channel_min.get(*channel) else {
                verifier.fail(
                    format!("midori.map.{file_name}.channel_{channel}"),
                    "channel range is missing",
                );
                continue;
            };
            let Some(maximum) = channel_max.get(*channel) else {
                verifier.fail(
                    format!("midori.map.{file_name}.channel_{channel}"),
                    "channel range is missing",
                );
                continue;
            };
            verifier.require(
                format!("midori.map.{file_name}.channel_{channel}_varies"),
                int_or_default(Some(maximum), 0) > int_or_default(Some(minimum), 0),
                format!("{}..{}", py_string(minimum), py_string(maximum)),
                format!("channel {channel} must have nontrivial range"),
            );
        }
        if !varying_any_channels.is_empty() {
            let mut varies = false;
            let mut ranges = Vec::new();
            for channel in *varying_any_channels {
                if let (Some(minimum), Some(maximum)) =
                    (channel_min.get(*channel), channel_max.get(*channel))
                {
                    ranges.push(format!(
                        "{channel}:{}..{}",
                        py_string(minimum),
                        py_string(maximum)
                    ));
                    varies |= int_or_default(Some(maximum), 0) > int_or_default(Some(minimum), 0);
                }
            }
            verifier.require(
                format!("midori.map.{file_name}.normal_variation"),
                varies,
                ranges.join(", "),
                "normal map must have nontrivial channel variation",
            );
        }
    }
}

pub fn check_midori_map_relationships(verifier: &mut Verifier, report: &Value, manifest: &Value) {
    let Some(relationships) = field(report, "map_relationships").filter(|value| value.is_object())
    else {
        verifier.fail(
            "midori.map_relationships",
            "validation report is missing map_relationships",
        );
        return;
    };
    let resolution = int_or_default(field(manifest, "map_resolution"), 0);
    let expected_pixels = resolution.saturating_mul(resolution);
    verifier.require_equal(
        "midori.map_relationships.normal_pair_pixels",
        field(relationships, "normal_pair_pixels"),
        json!(expected_pixels),
    );
    verifier.require_equal(
        "midori.map_relationships.normal_red_blue_mismatches",
        field(relationships, "normal_red_blue_mismatches"),
        json!(0),
    );
    verifier.require_equal(
        "midori.map_relationships.normal_green_flip_mismatches",
        field(relationships, "normal_green_flip_mismatches"),
        json!(0),
    );
    verifier.require(
        "midori.map_relationships.normal_green_flip_max_error",
        int_or_default(field(relationships, "normal_green_flip_max_error"), 999) <= 1,
        format!(
            "{} <= 1",
            py_string(field(relationships, "normal_green_flip_max_error").unwrap_or(&Value::Null))
        ),
        "normal green-channel inversion must be exact within byte rounding tolerance",
    );
    verifier.require_equal(
        "midori.map_relationships.grass_density_pixels",
        field(relationships, "grass_density_pixels"),
        json!(expected_pixels),
    );
    verifier.require_equal(
        "midori.map_relationships.grass_density_mask_r_mismatches",
        field(relationships, "grass_density_mask_r_mismatches"),
        json!(0),
    );
}

pub fn check_midori_prototype_summaries(verifier: &mut Verifier, report: &Value, manifest: &Value) {
    let Some(summaries) = value_as_array(field(report, "prototype_summaries")) else {
        verifier.fail(
            "midori.prototype_summaries",
            "validation report is missing prototype_summaries",
        );
        return;
    };
    let expected_lods = value_as_array(field(manifest, "prototypes"))
        .into_iter()
        .flatten()
        .flat_map(|prototype| {
            value_as_array(field(prototype, "lods"))
                .into_iter()
                .flatten()
        })
        .filter_map(|lod| {
            let file = string_field(field(lod, "file"))?;
            Some((
                file.replace('\\', "/")
                    .rsplit('/')
                    .next()
                    .unwrap_or(file)
                    .to_string(),
                lod,
            ))
        })
        .collect::<Vec<_>>();
    verifier.require_equal(
        "midori.prototype_summary_count",
        Some(&json!(summaries.len())),
        json!(expected_lods.len()),
    );
    let by_name = summaries
        .iter()
        .filter_map(|summary| {
            let file = string_field(field(summary, "file"))?;
            let normalized = file.replace('\\', "/");
            let name = normalized.rsplit('/').next().unwrap_or(file);
            Some((name.to_string(), summary))
        })
        .collect::<BTreeMap<_, _>>();
    let mobile_slots = int_or_default(field_path(manifest, &["mobile", "material_slots"]), 0);
    let console_slots = int_or_default(field_path(manifest, &["console", "material_slots"]), 0);
    let material_slot_limit = mobile_slots.min(console_slots);
    let required_attributes = [
        "has_positions",
        "has_normals",
        "has_tangents",
        "normals_are_valid",
        "tangents_are_valid",
        "has_texcoord0",
        "has_texcoord1",
        "has_color0",
    ];
    for (file_name, lod) in expected_lods {
        let Some(summary) = by_name.get(&file_name) else {
            verifier.fail(
                format!("midori.prototype.{file_name}"),
                "prototype summary is missing",
            );
            continue;
        };
        verifier.require_equal(
            format!("midori.prototype.{file_name}.lod_index"),
            field(summary, "lod_index"),
            field(lod, "index").cloned().unwrap_or(Value::Null),
        );
        verifier.require_equal(
            format!("midori.prototype.{file_name}.vertex_count"),
            field(summary, "vertex_count"),
            field(lod, "vertex_count").cloned().unwrap_or(Value::Null),
        );
        verifier.require_equal(
            format!("midori.prototype.{file_name}.triangle_count"),
            field(summary, "triangle_count"),
            field(lod, "triangle_count").cloned().unwrap_or(Value::Null),
        );
        verifier.require(
            format!("midori.prototype.{file_name}.mesh_count"),
            int_or_default(field(summary, "mesh_count"), 0) >= 1,
            format!(
                "{} meshes",
                py_string(field(summary, "mesh_count").unwrap_or(&Value::Null))
            ),
            "prototype GLB must contain at least one mesh",
        );
        verifier.require(
            format!("midori.prototype.{file_name}.primitive_count"),
            int_or_default(field(summary, "primitive_count"), 0) >= 1,
            format!(
                "{} primitives",
                py_string(field(summary, "primitive_count").unwrap_or(&Value::Null))
            ),
            "prototype GLB must contain at least one primitive",
        );
        for attribute in required_attributes {
            verifier.require_equal(
                format!("midori.prototype.{file_name}.{attribute}"),
                field(summary, attribute),
                json!(true),
            );
        }
        let used_materials = int_or_default(field(summary, "used_material_count"), 0);
        verifier.require(
            format!("midori.prototype.{file_name}.used_materials"),
            used_materials > 0 && used_materials <= material_slot_limit,
            format!("{used_materials} used materials within limit {material_slot_limit}"),
            "prototype GLB must use at least one material and stay within the profile material-slot limit",
        );
        verifier.require(
            format!("midori.prototype.{file_name}.checksum"),
            int_or_default(field(summary, "file_checksum"), 0) > 0,
            format!(
                "{} is a nonzero encoded GLB checksum",
                py_string(field(summary, "file_checksum").unwrap_or(&Value::Null))
            ),
            "encoded GLB checksum must be present and nonzero",
        );
    }
}

pub fn check_midori_profile_budgets(verifier: &mut Verifier, report: &Value, manifest: &Value) {
    let Some(summaries) = value_as_array(field(report, "profile_budget_summaries")) else {
        verifier.fail(
            "midori.profile_budget_summaries",
            "validation report is missing profile_budget_summaries",
        );
        return;
    };
    let Some(prototype_summaries) = value_as_array(field(report, "prototype_summaries")) else {
        verifier.fail(
            "midori.profile_budget_inputs",
            "validation report is missing prototype_summaries",
        );
        return;
    };
    let expected_lod_count = value_as_array(field(manifest, "prototypes"))
        .into_iter()
        .flatten()
        .flat_map(|prototype| {
            value_as_array(field(prototype, "lods"))
                .into_iter()
                .flatten()
        })
        .count() as i64;
    let mut max_lod0 = 0i64;
    let mut max_lod1 = 0i64;
    let mut max_lod2 = 0i64;
    let mut max_used_materials = 0i64;
    let scatter_instance_count = int_or_default(field(report, "scatter_binary_instances"), 0);
    let scatter_chunks = value_as_array(field(report, "scatter_binary_summaries"))
        .filter(|chunks| !chunks.is_empty())
        .or_else(|| value_as_array(field(report, "scatter_json_chunks")));
    let max_chunk_instances = scatter_chunks
        .into_iter()
        .flatten()
        .map(|chunk| int_or_default(field(chunk, "instance_count"), 0))
        .max()
        .unwrap_or(0);
    for summary in prototype_summaries {
        let triangle_count = int_or_default(field(summary, "triangle_count"), 0);
        match int_or_default(field(summary, "lod_index"), 2) {
            0 => max_lod0 = max_lod0.max(triangle_count),
            1 => max_lod1 = max_lod1.max(triangle_count),
            _ => max_lod2 = max_lod2.max(triangle_count),
        }
        max_used_materials =
            max_used_materials.max(int_or_default(field(summary, "used_material_count"), 0));
    }
    verifier.require_equal(
        "midori.profile_budget_summary_count",
        Some(&json!(summaries.len())),
        json!(2),
    );
    let by_profile = object_by_string_field(Some(summaries), "profile");
    for profile_name in ["mobile", "console"] {
        let Some(summary) = by_profile.get(profile_name) else {
            verifier.fail(
                format!("midori.profile_budget.{profile_name}"),
                "profile budget summary is missing",
            );
            continue;
        };
        let profile = field(manifest, profile_name).filter(|value| value.is_object());
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.passed"),
            field(summary, "passed"),
            json!(true),
        );
        let lod0_distance = float_or_default(
            profile.and_then(|value| field(value, "lod0_max_distance")),
            -1.0,
        );
        let lod1_distance = float_or_default(
            profile.and_then(|value| field(value, "lod1_max_distance")),
            -1.0,
        );
        let lod2_distance = float_or_default(
            profile.and_then(|value| field(value, "lod2_max_distance")),
            -1.0,
        );
        let cull_end = float_or_default(profile.and_then(|value| field(value, "cull_end")), -1.0);
        verifier.require(
            format!("midori.profile_budget.{profile_name}.lod_distances_ordered"),
            lod0_distance > 0.0
                && lod0_distance <= lod1_distance
                && lod1_distance <= lod2_distance
                && lod2_distance <= cull_end,
            format!("{lod0_distance} <= {lod1_distance} <= {lod2_distance} <= {cull_end}"),
            "profile LOD distances must be positive, ordered, and inside the cull range",
        );
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.grass_collision_off"),
            profile.and_then(|value| field(value, "grass_collision")),
            json!(false),
        );
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.moss_collision_off"),
            profile.and_then(|value| field(value, "moss_collision")),
            json!(false),
        );
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.prototype_lod_count"),
            field(summary, "prototype_lod_count"),
            json!(expected_lod_count),
        );
        for (suffix, actual, expected) in [
            ("max_lod0_triangles", "max_lod0_triangles", max_lod0),
            ("max_lod1_triangles", "max_lod1_triangles", max_lod1),
            ("max_lod2_triangles", "max_lod2_triangles", max_lod2),
        ] {
            verifier.require_equal(
                format!("midori.profile_budget.{profile_name}.{suffix}"),
                field(summary, actual),
                json!(expected),
            );
        }
        for (suffix, field_name) in [
            ("lod0_budget", "lod0_max_triangles"),
            ("lod1_budget", "lod1_max_triangles"),
            ("lod2_budget", "lod2_max_triangles"),
        ] {
            verifier.require_equal(
                format!("midori.profile_budget.{profile_name}.{suffix}"),
                field(
                    summary,
                    suffix.replace("_budget", "_triangle_budget").as_str(),
                ),
                profile
                    .and_then(|value| field(value, field_name))
                    .cloned()
                    .unwrap_or(Value::Null),
            );
        }
        for (suffix, actual, budget_field) in [
            ("lod0_within_budget", max_lod0, "lod0_max_triangles"),
            ("lod1_within_budget", max_lod1, "lod1_max_triangles"),
            ("lod2_within_budget", max_lod2, "lod2_max_triangles"),
        ] {
            let budget = int_or_default(profile.and_then(|value| field(value, budget_field)), 0);
            verifier.require(
                format!("midori.profile_budget.{profile_name}.{suffix}"),
                actual <= budget,
                format!("{actual} <= {budget}"),
                "LOD prototypes must fit the profile triangle budget",
            );
        }
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.max_used_material_count"),
            field(summary, "max_used_material_count"),
            json!(max_used_materials),
        );
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.material_slot_budget"),
            field(summary, "material_slot_budget"),
            profile
                .and_then(|value| field(value, "material_slots"))
                .cloned()
                .unwrap_or(Value::Null),
        );
        let material_budget =
            int_or_default(profile.and_then(|value| field(value, "material_slots")), 0);
        verifier.require(
            format!("midori.profile_budget.{profile_name}.materials_within_budget"),
            max_used_materials <= material_budget,
            format!("{max_used_materials} <= {material_budget}"),
            "prototype material usage must fit the profile material-slot budget",
        );
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.scatter_instance_count"),
            field(summary, "scatter_instance_count"),
            json!(scatter_instance_count),
        );
        verifier.require_equal(
            format!("midori.profile_budget.{profile_name}.max_chunk_instance_count"),
            field(summary, "max_chunk_instance_count"),
            json!(max_chunk_instances),
        );
        for (suffix, field_name) in [
            ("max_instances_per_tile_budget", "max_instances_per_tile"),
            ("max_instances_per_chunk_budget", "max_instances_per_chunk"),
        ] {
            verifier.require_equal(
                format!("midori.profile_budget.{profile_name}.{suffix}"),
                field(summary, suffix.replace("_budget", "_budget").as_str()),
                profile
                    .and_then(|value| field(value, field_name))
                    .cloned()
                    .unwrap_or(Value::Null),
            );
        }
        let tile_budget = int_or_default(
            profile.and_then(|value| field(value, "max_instances_per_tile")),
            0,
        );
        let chunk_budget = int_or_default(
            profile.and_then(|value| field(value, "max_instances_per_chunk")),
            0,
        );
        verifier.require(
            format!("midori.profile_budget.{profile_name}.tile_instances_within_budget"),
            scatter_instance_count <= tile_budget,
            format!("{scatter_instance_count} <= {tile_budget}"),
            "authored scatter instances must fit the profile per-tile budget",
        );
        verifier.require(
            format!("midori.profile_budget.{profile_name}.chunk_instances_within_budget"),
            max_chunk_instances <= chunk_budget,
            format!("{max_chunk_instances} <= {chunk_budget}"),
            "authored scatter instances must fit the profile per-chunk budget",
        );
        for (suffix, field_name) in [
            ("triangle_violations", "triangle_budget_violation_count"),
            ("material_violations", "material_slot_violation_count"),
            ("instance_violations", "instance_budget_violation_count"),
        ] {
            verifier.require_equal(
                format!("midori.profile_budget.{profile_name}.{suffix}"),
                field(summary, field_name),
                json!(0),
            );
        }
    }
}

pub fn check_midori_scatter_summaries(verifier: &mut Verifier, report: &Value) {
    let json_summaries = value_as_array(field(report, "scatter_json_chunks"));
    let binary_summaries = value_as_array(field(report, "scatter_binary_summaries"));
    let parity = field(report, "scatter_parity").filter(|value| value.is_object());
    if json_summaries.is_none() {
        verifier.fail(
            "midori.scatter_json_chunks",
            "validation report is missing scatter_json_chunks",
        );
    }
    if binary_summaries.is_none() {
        verifier.fail(
            "midori.scatter_binary_summaries",
            "validation report is missing scatter_binary_summaries",
        );
    }
    if parity.is_none() {
        verifier.fail(
            "midori.scatter_parity",
            "validation report is missing scatter_parity",
        );
    }
    let empty = Vec::new();
    let json_summaries = json_summaries.unwrap_or(&empty);
    let binary_summaries = binary_summaries.unwrap_or(&empty);
    verifier.require_equal(
        "midori.scatter_json_summary_count",
        Some(&json!(json_summaries.len())),
        json!(26),
    );
    verifier.require_equal(
        "midori.scatter_binary_summary_count",
        Some(&json!(binary_summaries.len())),
        json!(26),
    );
    let parity = parity.unwrap_or(&Value::Null);
    for (field_name, expected) in [
        ("json_chunk_count", 26),
        ("binary_chunk_count", 26),
        ("matching_chunk_count", 26),
    ] {
        verifier.require_equal(
            format!("midori.scatter_parity.{field_name}"),
            field(parity, field_name),
            json!(expected),
        );
    }
    for field_name in [
        "missing_binary_chunk_count",
        "extra_binary_chunk_count",
        "instance_count_mismatch_count",
        "bounds_mismatch_count",
        "record_checksum_mismatch_count",
    ] {
        verifier.require_equal(
            format!("midori.scatter_parity.{field_name}"),
            field(parity, field_name),
            json!(0),
        );
    }
    for (source, summaries) in [("json", json_summaries), ("binary", binary_summaries)] {
        for (index, summary) in summaries.iter().enumerate() {
            let Some(summary) = summary.as_object().map(|_| summary) else {
                verifier.fail(
                    format!("midori.scatter_{source}_summary.{index}"),
                    "summary must be an object",
                );
                continue;
            };
            let label = format!("midori.scatter_{source}_summary.{index}");
            verifier.require_equal(
                format!("{label}.source"),
                field(summary, "source"),
                json!(source),
            );
            verifier.require(
                format!("{label}.instance_count"),
                int_or_default(field(summary, "instance_count"), 0) > 0,
                format!(
                    "{} instances",
                    py_string(field(summary, "instance_count").unwrap_or(&Value::Null))
                ),
                format!("{source} scatter chunks must contain at least one instance"),
            );
            for checksum_field in ["file_checksum", "record_checksum"] {
                verifier.require(
                    format!("{label}.{checksum_field}"),
                    int_or_default(field(summary, checksum_field), 0) > 0,
                    format!(
                        "{} is nonzero",
                        py_string(field(summary, checksum_field).unwrap_or(&Value::Null))
                    ),
                    format!("{checksum_field} must be present and nonzero"),
                );
            }
            check_range(
                verifier,
                &format!("{label}.yaw_range"),
                float_or_default(field(summary, "yaw_min"), -1.0),
                float_or_default(field(summary, "yaw_max"), -1.0),
                0.0,
                std::f64::consts::TAU,
                "yaw range must be ordered and inside 0..tau",
            );
            check_range(
                verifier,
                &format!("{label}.phase_range"),
                float_or_default(field(summary, "phase_min"), -1.0),
                float_or_default(field(summary, "phase_max"), -1.0),
                0.0,
                std::f64::consts::TAU,
                "phase range must be ordered and inside 0..tau",
            );
            check_positive_range(
                verifier,
                &format!("{label}.height_range"),
                float_or_default(field(summary, "height_min"), 0.0),
                float_or_default(field(summary, "height_max"), 0.0),
                f64::INFINITY,
                "height multiplier range must be positive and ordered",
            );
            check_positive_range(
                verifier,
                &format!("{label}.width_range"),
                float_or_default(field(summary, "width_min"), 0.0),
                float_or_default(field(summary, "width_max"), 0.0),
                f64::INFINITY,
                "width multiplier range must be positive and ordered",
            );
            check_range(
                verifier,
                &format!("{label}.color_variation_range"),
                float_or_default(field(summary, "color_variation_min"), -1.0),
                float_or_default(field(summary, "color_variation_max"), -1.0),
                0.0,
                1.0,
                "color variation range must be ordered and inside 0..1",
            );
        }
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
