use midori_cli::evidence::FullEvidenceOptions;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{TempDir, tempdir};

/// Synthetic evidence mutations used by the native self-tests. This fixture is
/// intentionally not real-engine acceptance evidence.
#[derive(Clone, Copy)]
pub enum Mutation {
    MidoriIdentity,
    ManifestChecksum,
    PngDimensions,
    PngContent,
    PngFilter,
    PngDecompression,
    PngIncompleteZlib,
    PrototypeAttribute,
    MapRelationship,
    ScatterParity,
    ScatterRange,
    MaterialRecipe,
    MaterialParameter,
    EngineImportRecipe,
    SurfaceOverlay,
    PrototypeTargets,
    ProfileBudget,
    MemoryFootprint,
    Scaffold,
    CompileStub,
    FakeEditor,
    UnityField,
    UnrealImportTasks,
    UnrealFoliage,
    UnrealCull,
    Notes,
}

pub struct SyntheticFullEvidence {
    _root: TempDir,
    validation: PathBuf,
}

impl SyntheticFullEvidence {
    pub fn create() -> Self {
        let root = tempdir().expect("synthetic fixture tempdir");
        let validation = root.path().join("validation");
        let package = validation.join("forest_floor");
        for directory in ["maps", "scatter", "prototypes", "materials", "engines"] {
            fs::create_dir_all(package.join(directory)).unwrap();
        }
        let mut manifest = manifest();
        write_glb(&package.join("preview_tile.glb"));
        write_maps(&package);
        let records = scatter_records();
        write_scatter_json(&package.join("scatter/scatter.json"), &records);
        for prototype in manifest["prototypes"].as_array().unwrap() {
            for lod in prototype["lods"].as_array().unwrap() {
                let file = lod["file"].as_str().unwrap();
                write_glb(&package.join(file));
            }
        }
        for file in [
            "materials/terrain_surface.recipe.json",
            "materials/groundcover_foliage.recipe.json",
            "engines/unity_import.recipe.json",
            "engines/unreal_import.recipe.json",
        ] {
            write_recipe(&package.join(file), file, &manifest);
        }
        let mut binary_files = Vec::new();
        for index in 0..26 {
            let file = format!("scatter/chunk_{index:02}.bin");
            write_scatter_binary(&package.join(&file), scatter_count(index) as u32, index);
            binary_files.push(file);
        }
        manifest["memory_footprint"] = memory_footprint(&package, &manifest);
        write_json(&package.join("midori_nature.json"), &manifest);
        write_scaffolds(&validation);
        let screenshots = validation.join("screenshots");
        write_png(&screenshots.join("unity_import.png"), 256, 256);
        write_png(&screenshots.join("unity_density.png"), 256, 256);
        write_png(&screenshots.join("unreal_import.png"), 256, 256);
        write_png(&screenshots.join("unreal_foliage.png"), 256, 256);
        let unity_source_files = source_files(&manifest, "unity_yplus_file");
        let unreal_source_files = source_files(&manifest, "unreal_yminus_file");
        let manifest_checksum = fnv_hex(&fs::read(package.join("midori_nature.json")).unwrap());
        let unity_source_checksum = source_xor(&package, &unity_source_files);
        let unreal_source_checksum = source_xor(&package, &unreal_source_files);
        let midori = midori_report(&manifest, &binary_files, &package);
        let summary = summary(
            &manifest,
            &validation,
            &manifest_checksum,
            &unity_source_checksum,
            &unreal_source_checksum,
            &package,
        );
        let compile_stub = compile_stub_report(
            &manifest,
            &validation,
            &manifest_checksum,
            &unity_source_checksum,
            &package,
        );
        let fake_editor = fake_editor_report(
            &manifest_checksum,
            &unreal_source_checksum,
            &package,
            &validation,
        );
        let unity = unity_report(
            &manifest,
            &manifest_checksum,
            &unity_source_checksum,
            &package,
            &validation,
        );
        let dry_run = unreal_report(
            &manifest,
            &manifest_checksum,
            &unreal_source_checksum,
            true,
            &package,
            &validation,
        );
        let unreal = unreal_report(
            &manifest,
            &manifest_checksum,
            &unreal_source_checksum,
            false,
            &package,
            &validation,
        );
        write_json(
            &validation.join("forest_floor_midori_validation_report.json"),
            &midori,
        );
        write_json(&validation.join("engine_validation_summary.json"), &summary);
        write_json(
            &validation.join("forest_floor_unity_compile_stub_report.json"),
            &compile_stub,
        );
        write_json(
            &validation.join("forest_floor_unreal_fake_editor_report.json"),
            &fake_editor,
        );
        write_json(
            &validation.join("forest_floor_unity_import_report.json"),
            &unity,
        );
        write_json(
            &validation.join("forest_floor_unreal_dry_run_report.json"),
            &dry_run,
        );
        write_json(
            &validation.join("forest_floor_unreal_editor_report.json"),
            &unreal,
        );
        fs::write(validation.join("profile-notes.md"), profile_notes()).unwrap();
        Self {
            _root: root,
            validation,
        }
    }

    pub fn validation_root(&self) -> PathBuf {
        self.validation.clone()
    }

    pub fn options(&self) -> FullEvidenceOptions {
        let mut options = FullEvidenceOptions::from_validation_root(self.validation.clone());
        options.unity_import_screenshot = self.validation.join("screenshots/unity_import.png");
        options.unity_density_screenshot = self.validation.join("screenshots/unity_density.png");
        options.unreal_import_screenshot = self.validation.join("screenshots/unreal_import.png");
        options.unreal_foliage_settings_screenshot =
            self.validation.join("screenshots/unreal_foliage.png");
        options.profile_notes = self.validation.join("profile-notes.md");
        options
    }

    pub fn remove_editor_only_evidence(&self) {
        for path in [
            "forest_floor_unreal_dry_run_report.json",
            "forest_floor_unity_import_report.json",
            "forest_floor_unreal_editor_report.json",
            "screenshots/unity_import.png",
            "screenshots/unity_density.png",
            "screenshots/unreal_import.png",
            "screenshots/unreal_foliage.png",
            "profile-notes.md",
        ] {
            let _ = fs::remove_file(self.validation.join(path));
        }
    }

    /// Copy this synthetic tree onto the CLI's cwd-relative default locations
    /// beneath `cwd`. The copied tree is still synthetic self-test data and
    /// certifies nothing about a Unity or Unreal editor ever having been run.
    pub fn install_default_layout(&self, cwd: &Path) {
        copy_tree(
            &self.validation,
            &cwd.join("target/midori_engine_validation"),
        );
        let screenshots = cwd.join("docs/validation/screenshots");
        fs::create_dir_all(&screenshots).unwrap();
        for (source, destination) in [
            (
                "screenshots/unity_import.png",
                "unity_forest_floor_import.png",
            ),
            (
                "screenshots/unity_density.png",
                "unity_forest_floor_density.png",
            ),
            (
                "screenshots/unreal_import.png",
                "unreal_forest_floor_import.png",
            ),
            (
                "screenshots/unreal_foliage.png",
                "unreal_forest_floor_foliage_settings.png",
            ),
        ] {
            fs::copy(self.validation.join(source), screenshots.join(destination)).unwrap();
        }
        fs::copy(
            self.validation.join("profile-notes.md"),
            cwd.join("docs/validation/midori-nature-engine-profile-notes.md"),
        )
        .unwrap();
    }

    /// Move the five top-level evidence reports to alternate names so a test can
    /// only reach them through explicit CLI path overrides.
    pub fn divert_reports(&self) -> Vec<(&'static str, PathBuf)> {
        let alternates = self.validation.join("alternate");
        fs::create_dir_all(&alternates).unwrap();
        let mut moved = Vec::new();
        for (flag, name) in [
            (
                "--midori-report",
                "forest_floor_midori_validation_report.json",
            ),
            ("--summary", "engine_validation_summary.json"),
            ("--unity-report", "forest_floor_unity_import_report.json"),
            (
                "--unreal-dry-run-report",
                "forest_floor_unreal_dry_run_report.json",
            ),
            ("--unreal-report", "forest_floor_unreal_editor_report.json"),
        ] {
            let destination = alternates.join(name);
            fs::rename(self.validation.join(name), &destination).unwrap();
            moved.push((flag, destination));
        }
        moved
    }

    pub fn mutate_destination_dot(&self) {
        let dot_heightmap = "./maps/height_u16.png";
        edit_json(
            self.validation
                .join("forest_floor_midori_validation_report.json"),
            |report| {
                report["manifest"]["terrain"]["heightmap_file"] = json!(dot_heightmap);
            },
        );
        edit_json(
            self.validation
                .join("forest_floor_unity_import_report.json"),
            |report| {
                report["heightmapFile"] = json!(dot_heightmap);
                replace_in_value(
                    &mut report["sourceFiles"],
                    "maps/height_u16.png",
                    dot_heightmap,
                );
                sort_value_strings(&mut report["sourceFiles"]);
            },
        );
        for report_file in [
            "forest_floor_unreal_dry_run_report.json",
            "forest_floor_unreal_editor_report.json",
        ] {
            edit_json(self.validation.join(report_file), |report| {
                for field_name in ["imported_files", "import_task_files", "source_files"] {
                    replace_in_value(
                        &mut report[field_name],
                        "maps/height_u16.png",
                        dot_heightmap,
                    );
                    sort_value_strings(&mut report[field_name]);
                }
                replace_in_value(
                    &mut report["imported_map_files"],
                    "maps/height_u16.png",
                    dot_heightmap,
                );
                replace_in_value(
                    &mut report["import_tasks"],
                    "maps/height_u16.png",
                    dot_heightmap,
                );
                sort_value_objects_by_string(&mut report["import_tasks"], "file");
            });
        }
    }

    pub fn mutate(&self, mutation: Mutation) {
        match mutation {
            Mutation::MidoriIdentity => edit_json(
                self.validation
                    .join("forest_floor_unity_import_report.json"),
                |report| report["sourceFileCount"] = json!(1),
            ),
            Mutation::ManifestChecksum => edit_json(
                self.validation
                    .join("forest_floor_unity_import_report.json"),
                |report| report["manifestFileChecksum"] = json!("0x0000000000000000"),
            ),
            Mutation::PngDimensions => write_png(
                &self.validation.join("screenshots/unity_import.png"),
                32,
                32,
            ),
            Mutation::PngContent => write_png_constant(
                &self.validation.join("screenshots/unity_import.png"),
                256,
                256,
                100,
            ),
            Mutation::PngFilter => fs::write(
                self.validation.join("screenshots/unity_import.png"),
                png_with_bad_filter(),
            )
            .unwrap(),
            Mutation::PngDecompression => fs::write(
                self.validation.join("screenshots/unity_import.png"),
                png_with_bad_decompression(),
            )
            .unwrap(),
            Mutation::PngIncompleteZlib => fs::write(
                self.validation.join("screenshots/unity_import.png"),
                png_with_incomplete_zlib(),
            )
            .unwrap(),
            Mutation::PrototypeAttribute => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["prototype_summaries"][0]["has_normals"] = json!(false),
            ),
            Mutation::MapRelationship => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["map_relationships"]["normal_green_flip_mismatches"] = json!(1),
            ),
            Mutation::ScatterParity => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["scatter_parity"]["record_checksum_mismatch_count"] = json!(1),
            ),
            Mutation::ScatterRange => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["scatter_json_chunks"][0]["yaw_max"] = json!(99.0),
            ),
            Mutation::MaterialRecipe => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["manifest"]["material_recipes"][0]["engine_targets"] = json!([]),
            ),
            Mutation::MaterialParameter => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| {
                    report["manifest"]["material_parameters"][0]["parameters"][0]["semantic"] =
                        json!("wrong_semantic");
                },
            ),
            Mutation::EngineImportRecipe => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| {
                    report["manifest"]["engine_import_recipes"][0]["profile"] =
                        json!("wrong_profile");
                },
            ),
            Mutation::SurfaceOverlay => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["manifest"]["surface_overlays"][0]["channel"] = json!("R"),
            ),
            Mutation::PrototypeTargets => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["manifest"]["prototypes"][0]["surface_targets"] = json!([]),
            ),
            Mutation::ProfileBudget => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["profile_budget_summaries"][0]["lod0_triangle_budget"] = json!(1),
            ),
            Mutation::MemoryFootprint => edit_json(
                self.validation
                    .join("forest_floor_midori_validation_report.json"),
                |report| report["memory_footprint"]["total_payload_bytes"] = json!(1),
            ),
            Mutation::Scaffold => edit_json(
                self.validation
                    .join("projects/unreal/MidoriUnrealValidation/MidoriUnrealValidation.uproject"),
                |project| project["Plugins"][0]["Enabled"] = json!(false),
            ),
            Mutation::CompileStub => edit_json(
                self.validation
                    .join("forest_floor_unity_compile_stub_report.json"),
                |report| report["detailPrototypeFailures"] = json!(1),
            ),
            Mutation::FakeEditor => edit_json(
                self.validation
                    .join("forest_floor_unreal_fake_editor_report.json"),
                |report| report["foliage_type_count"] = json!(1),
            ),
            Mutation::UnityField => edit_json(
                self.validation
                    .join("forest_floor_unity_import_report.json"),
                |report| report["mobileDensityScale"] = json!(9.0),
            ),
            Mutation::UnrealImportTasks => edit_json(
                self.validation
                    .join("forest_floor_unreal_editor_report.json"),
                |report| report["import_task_files"][0] = json!("wrong.glb"),
            ),
            Mutation::UnrealFoliage => edit_json(
                self.validation
                    .join("forest_floor_unreal_editor_report.json"),
                |report| report["foliage_type_count"] = json!(1),
            ),
            Mutation::UnrealCull => edit_json(
                self.validation
                    .join("forest_floor_unreal_editor_report.json"),
                |report| report["foliage_cull_start_cm"] = json!(1),
            ),
            Mutation::Notes => {
                fs::write(self.validation.join("profile-notes.md"), "Unity Unreal").unwrap()
            }
        }
    }
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn write_maps(package: &Path) {
    let mut height = Vec::with_capacity(8 * 8 * 2);
    let mut density = Vec::with_capacity(8 * 8);
    let mut normal_yplus = Vec::with_capacity(8 * 8 * 3);
    let mut normal_yminus = Vec::with_capacity(8 * 8 * 3);
    let mut masks = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8u16 {
        for x in 0..8u16 {
            let value = x * 2048 + y * 1024;
            height.extend(value.to_be_bytes());
            // Keep the density map's authored red channel identical to the
            // mask's R channel so the report's independently derived
            // relationship summary is true for the bytes on disk.
            density.push((x * 31) as u8);
            normal_yplus.extend_from_slice(&[
                (x * 16 + 16) as u8,
                (y * 16 + 16) as u8,
                ((x + y) * 16 + 16) as u8,
            ]);
            normal_yminus.extend_from_slice(&[
                (x * 16 + 16) as u8,
                255u16.saturating_sub(y * 16 + 16) as u8,
                ((x + y) * 16 + 16) as u8,
            ]);
            masks.extend_from_slice(&[(x * 31) as u8, (y * 31) as u8, ((x + y) * 15) as u8, 255]);
        }
    }
    write_png_encoded(
        &package.join("maps/height_u16.png"),
        8,
        8,
        png::ColorType::Grayscale,
        png::BitDepth::Sixteen,
        &height,
    );
    write_png_encoded(
        &package.join("maps/normal_yplus.png"),
        8,
        8,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &normal_yplus,
    );
    write_png_encoded(
        &package.join("maps/normal_yminus.png"),
        8,
        8,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &normal_yminus,
    );
    write_png_encoded(
        &package.join("maps/masks_rgba.png"),
        8,
        8,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &masks,
    );
    write_png_encoded(
        &package.join("maps/grass_density.png"),
        8,
        8,
        png::ColorType::Grayscale,
        png::BitDepth::Eight,
        &density,
    );
}

fn write_png_encoded(
    path: &Path,
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(color);
    encoder.set_depth(depth);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
}

fn write_glb(path: &Path) {
    let mut binary = Vec::new();
    let mut push_f32 = |value: f32| binary.extend(value.to_le_bytes());
    for values in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in values {
            push_f32(value);
        }
    }
    for _ in 0..3 {
        for value in [0.0, 1.0, 0.0] {
            push_f32(value);
        }
    }
    for _ in 0..3 {
        for value in [1.0, 0.0, 0.0, 1.0] {
            push_f32(value);
        }
    }
    for _ in 0..3 {
        for value in [0.0, 0.0] {
            push_f32(value);
        }
    }
    for _ in 0..3 {
        for value in [0.0, 1.0] {
            push_f32(value);
        }
    }
    for _ in 0..3 {
        for value in [1.0, 1.0, 1.0, 1.0] {
            push_f32(value);
        }
    }
    for value in [0u16, 1, 2] {
        binary.extend(value.to_le_bytes());
    }
    while binary.len() % 4 != 0 {
        binary.push(0);
    }
    let json_chunk = json!({
        "asset": {"version": "2.0"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{
            "attributes": {
                "POSITION": 0, "NORMAL": 1, "TANGENT": 2,
                "TEXCOORD_0": 3, "TEXCOORD_1": 4, "COLOR_0": 5
            },
            "indices": 6, "material": 0
        }]}],
        "materials": [{"pbrMetallicRoughness": {}}],
        "buffers": [{"byteLength": binary.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962},
            {"buffer": 0, "byteOffset": 36, "byteLength": 36, "target": 34962},
            {"buffer": 0, "byteOffset": 72, "byteLength": 48, "target": 34962},
            {"buffer": 0, "byteOffset": 120, "byteLength": 24, "target": 34962},
            {"buffer": 0, "byteOffset": 144, "byteLength": 24, "target": 34962},
            {"buffer": 0, "byteOffset": 168, "byteLength": 48, "target": 34962},
            {"buffer": 0, "byteOffset": 216, "byteLength": 6, "target": 34963}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [1.0, 1.0, 0.0]},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4"},
            {"bufferView": 3, "componentType": 5126, "count": 3, "type": "VEC2"},
            {"bufferView": 4, "componentType": 5126, "count": 3, "type": "VEC2"},
            {"bufferView": 5, "componentType": 5126, "count": 3, "type": "VEC4"},
            {"bufferView": 6, "componentType": 5123, "count": 3, "type": "SCALAR"}
        ]
    });
    let mut json_bytes = serde_json::to_vec(&json_chunk).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let total_length = 12 + 8 + json_bytes.len() + 8 + binary.len();
    let mut output = Vec::with_capacity(total_length);
    output.extend_from_slice(b"glTF");
    output.extend(2u32.to_le_bytes());
    output.extend((total_length as u32).to_le_bytes());
    output.extend((json_bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(b"JSON");
    output.extend(json_bytes);
    output.extend((binary.len() as u32).to_le_bytes());
    output.extend_from_slice(b"BIN\0");
    output.extend(binary);
    write_bytes(path, &output);
}

fn write_recipe(path: &Path, file: &str, manifest: &Value) {
    let value = if file.contains("terrain_surface") || file.contains("groundcover_foliage") {
        let slot = if file.contains("terrain_surface") {
            "terrain_surface"
        } else {
            "groundcover_foliage"
        };
        let recipe = manifest["material_recipes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|recipe| recipe["material_slot"] == slot)
            .unwrap();
        let parameters = manifest["material_parameters"]
            .as_array()
            .unwrap()
            .iter()
            .find(|set| set["material_slot"] == slot)
            .unwrap();
        json!({
            "schema": "midori.material_recipe.v1",
            "material_slot": slot,
            "parameter_set": parameters["parameter_set"],
            "runtime_policy": recipe["runtime_policy"],
            "shader_policy": manifest["shader_policy"],
            "texture_pipeline": manifest["texture_pipeline"],
            "engine_targets": recipe["engine_targets"],
            "parameters": parameters["parameters"],
            "required_textures": if slot == "terrain_surface" { json!(["maps/masks_rgba.png"]) } else { json!([]) },
            "required_vertex_streams": if slot == "groundcover_foliage" { json!([
                "TEXCOORD_1.x phase_radians", "TEXCOORD_1.y bend_stiffness",
                "COLOR_0.x normalized_height", "COLOR_0.y color_variation",
                "COLOR_0.z normalized_progress", "COLOR_0.w bend_stiffness"
            ]) } else { json!([]) },
            "surface_overlays": if slot == "terrain_surface" { json!(["moss", "wetness", "cracks"]) } else { json!([]) },
            "notes": ["Synthetic native fixture recipe with static engine import metadata."]
        })
    } else {
        let engine = if file.contains("unity_import") {
            "unity"
        } else {
            "unreal"
        };
        let profile = if engine == "unity" {
            "mobile"
        } else {
            "console"
        };
        let normal_key = if engine == "unity" {
            "unity_yplus_file"
        } else {
            "unreal_yminus_file"
        };
        let profile_value = &manifest[profile];
        let expected_systems = manifest["engine_import_recipes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|recipe| recipe["engine"] == engine)
            .unwrap()["expected_systems"]
            .clone();
        let source_files = source_files(manifest, normal_key);
        let material_recipe_files = manifest["material_recipes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|recipe| recipe["file"].clone())
            .collect::<Vec<_>>();
        let instance_count: usize = manifest["scatter"]["binary_files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["instance_count"].as_u64().unwrap() as usize)
            .sum();
        let groundcover_target = if engine == "unity" {
            "Unity Terrain detail mesh prefabs"
        } else {
            "Unreal Static Mesh Foliage"
        };
        let prototype_source = if engine == "unity" {
            "LOD0 GLB detail mesh prefabs with native GLB fallback"
        } else {
            "LOD0 GLB Static Mesh assets"
        };
        let placement_source = if engine == "unity" {
            "grass_density/masks plus midori.scatter.bin.v1 ScriptableObject"
        } else {
            "midori.scatter.bin.v1 foliage placement buffers"
        };
        let terrain_normal = manifest["normal_conventions"][normal_key].clone();
        let terrain_target = if engine == "unity" {
            "Unity TerrainData"
        } else {
            "Unreal Landscape"
        };
        json!({
            "schema": "midori.engine_import_recipe.v1",
            "engine": engine, "profile": profile,
            "runtime_policy": "engine_native_static",
            "expected_systems": expected_systems,
            "source_files": source_files,
            "material_recipe_files": material_recipe_files,
            "terrain": {
                "target_system": terrain_target,
                "heightmap_file": manifest["terrain"]["heightmap_file"],
                "mask_file": manifest["terrain"]["masks_file"],
                "density_map_file": manifest["terrain"]["grass_density_file"],
                "normal_map_file": terrain_normal,
                "tile_size_meters": manifest["tile_size"],
                "height_min": manifest["terrain"]["height_min"],
                "height_max": manifest["terrain"]["height_max"]
            },
            "groundcover": {
                "target_system": groundcover_target,
                "prototype_source": prototype_source,
                "material_slot": "groundcover_foliage",
                "material_recipe_file": "materials/groundcover_foliage.recipe.json",
                "prototype_family_count": manifest["prototypes"].as_array().unwrap().len(),
                "lod0_prototype_count": manifest["prototypes"].as_array().unwrap().len(),
                "double_sided": true, "alpha_mode": "masked"
            },
            "scatter": {
                "placement_source": placement_source,
                "scatter_json_file": manifest["scatter"]["file"],
                "binary_chunk_count": manifest["scatter"]["binary_files"].as_array().unwrap().len(),
                "instance_count": instance_count,
                "chunk_size_meters": manifest["scatter"]["chunk_size"]
            },
            "profile_settings": {
                "name": profile,
                "density_scale": profile_value["density_scale"],
                "lod0_max_distance_meters": profile_value["lod0_max_distance"],
                "lod1_max_distance_meters": profile_value["lod1_max_distance"],
                "lod2_max_distance_meters": profile_value["lod2_max_distance"],
                "cull_start_meters": profile_value["cull_start"],
                "cull_end_meters": profile_value["cull_end"],
                "shadows": profile_value["shadows"],
                "material_slots": profile_value["material_slots"],
                "max_instances_per_tile": profile_value["max_instances_per_tile"],
                "max_instances_per_chunk": profile_value["max_instances_per_chunk"],
                "grass_collision": profile_value["grass_collision"],
                "moss_collision": profile_value["moss_collision"]
            },
            "notes": ["Synthetic native fixture engine import recipe."]
        })
    };
    write_json(path, &value);
}

fn scatter_records() -> Vec<Vec<[f32; 8]>> {
    (0..26)
        .map(|chunk| {
            (0..scatter_count(chunk) as usize)
                .map(|index| {
                    [
                        chunk as f32 + 0.25 + index as f32 * 0.01,
                        0.5 + index as f32 * 0.01,
                        0.25 + index as f32 * 0.01,
                        (index as f32) * 0.1,
                        0.8 + index as f32 * 0.01,
                        0.9 + index as f32 * 0.01,
                        (index as f32) * 0.05,
                        0.2 + index as f32 * 0.01,
                    ]
                })
                .collect()
        })
        .collect()
}

fn write_scatter_json(path: &Path, records: &[Vec<[f32; 8]>]) {
    let chunks = records
        .iter()
        .enumerate()
        .map(|(index, records)| {
            json!({
                "chunk_x": index as i32, "chunk_z": 0,
                "bounds_min": [index as f32, 0.0, 0.0],
                "bounds_max": [index as f32 + 1.0, 2.0, 1.0],
                "instances": records.iter().map(|record| json!({
                    "position": [record[0], record[1], record[2]],
                    "yaw": record[3], "height": record[4], "width": record[5],
                    "phase": record[6], "color_variation": record[7]
                })).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    write_json(
        path,
        &json!([{
            "layer_index": 0, "layer_name": "grass", "kind": "grass", "chunks": chunks
        }]),
    );
}

fn write_scatter_binary(path: &Path, count: u32, chunk: usize) {
    let records = scatter_records();
    let records = &records[chunk];
    assert_eq!(records.len(), count as usize);
    let mut bytes = Vec::with_capacity(16 + records.len() * 32);
    bytes.extend_from_slice(b"MDSI");
    bytes.extend(1u32.to_le_bytes());
    bytes.extend(32u32.to_le_bytes());
    bytes.extend(count.to_le_bytes());
    for record in records {
        for value in record {
            bytes.extend(value.to_le_bytes());
        }
    }
    write_bytes(path, &bytes);
}

fn memory_footprint(package: &Path, manifest: &Value) -> Value {
    let map_files = [
        "maps/height_u16.png",
        "maps/normal_yplus.png",
        "maps/normal_yminus.png",
        "maps/masks_rgba.png",
        "maps/grass_density.png",
    ];
    let encoded_map_bytes = map_files
        .iter()
        .map(|file| fs::metadata(package.join(file)).unwrap().len())
        .sum::<u64>();
    let material_recipe_bytes = [
        "materials/terrain_surface.recipe.json",
        "materials/groundcover_foliage.recipe.json",
    ]
    .iter()
    .map(|file| fs::metadata(package.join(file)).unwrap().len())
    .sum::<u64>();
    let engine_import_recipe_bytes = [
        "engines/unity_import.recipe.json",
        "engines/unreal_import.recipe.json",
    ]
    .iter()
    .map(|file| fs::metadata(package.join(file)).unwrap().len())
    .sum::<u64>();
    let prototype_mesh_bytes = manifest["prototypes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|prototype| prototype["lods"].as_array().unwrap())
        .map(|lod| {
            fs::metadata(package.join(lod["file"].as_str().unwrap()))
                .unwrap()
                .len()
        })
        .sum::<u64>();
    let scatter_binary_bytes = manifest["scatter"]["binary_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            fs::metadata(package.join(item["file"].as_str().unwrap()))
                .unwrap()
                .len()
        })
        .sum::<u64>();
    let values = [
        encoded_map_bytes,
        material_recipe_bytes,
        engine_import_recipe_bytes,
        fs::metadata(package.join("preview_tile.glb"))
            .unwrap()
            .len(),
        prototype_mesh_bytes,
        fs::metadata(package.join("scatter/scatter.json"))
            .unwrap()
            .len(),
        scatter_binary_bytes,
    ];
    let total_payload_bytes = values.iter().sum::<u64>();
    json!({
        "map_pixel_count": 64,
        "decoded_map_bytes": 832,
        "encoded_map_bytes": encoded_map_bytes,
        "material_recipe_bytes": material_recipe_bytes,
        "engine_import_recipe_bytes": engine_import_recipe_bytes,
        "preview_mesh_bytes": fs::metadata(package.join("preview_tile.glb")).unwrap().len(),
        "prototype_mesh_bytes": prototype_mesh_bytes,
        "scatter_json_bytes": fs::metadata(package.join("scatter/scatter.json")).unwrap().len(),
        "scatter_binary_header_bytes": 26 * 16,
        "scatter_binary_record_bytes": 222 * 32,
        "scatter_binary_bytes": scatter_binary_bytes,
        "total_payload_bytes": total_payload_bytes,
    })
}
fn write_json(path: &Path, value: &Value) {
    write_bytes(path, &serde_json::to_vec_pretty(value).unwrap());
}
fn edit_json(path: PathBuf, edit: impl FnOnce(&mut Value)) {
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    write_json(&path, &value);
}

fn replace_in_value(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::String(text) if text == from => *text = to.to_string(),
        Value::Array(values) => {
            for value in values {
                replace_in_value(value, from, to);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_in_value(value, from, to);
            }
        }
        _ => {}
    }
}

fn sort_value_strings(value: &mut Value) {
    if let Value::Array(values) = value {
        values.sort_by_key(|value| value.as_str().unwrap_or("").to_string());
    }
}

fn sort_value_objects_by_string(value: &mut Value, key: &str) {
    if let Value::Array(values) = value {
        values.sort_by_key(|value| value[key].as_str().unwrap_or("").to_string());
    }
}
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut value = 0xcbf29ce484222325u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    value
}
fn fnv_hex(bytes: &[u8]) -> String {
    format!("0x{:016x}", fnv1a(bytes))
}
fn fnv_hex_u64(value: u64) -> String {
    format!("0x{value:016x}")
}
fn scatter_checksums(package: &Path) -> (u64, u64) {
    let mut file_checksum = 0u64;
    let mut record_checksum = 0u64;
    for index in 0..26 {
        let bytes = fs::read(package.join(format!("scatter/chunk_{index:02}.bin"))).unwrap();
        file_checksum ^= fnv1a(&bytes);
        record_checksum ^= fnv1a(&bytes[16..]);
    }
    (file_checksum, record_checksum)
}
fn source_xor(package: &Path, files: &[String]) -> String {
    let mut value = 0u64;
    for file in files {
        value ^= fnv1a(&fs::read(package.join(file)).unwrap());
    }
    format!("0x{value:016x}")
}
fn source_files(manifest: &Value, normal_key: &str) -> Vec<String> {
    let mut files = BTreeSet::new();
    files.insert("midori_nature.json".to_string());
    files.insert("preview_tile.glb".to_string());
    for key in ["heightmap_file", "masks_file", "grass_density_file"] {
        files.insert(manifest["terrain"][key].as_str().unwrap().to_string());
    }
    if let Some(file) = manifest["scatter"]["file"].as_str() {
        files.insert(file.to_string());
    }
    files.insert(
        manifest["normal_conventions"][normal_key]
            .as_str()
            .unwrap()
            .to_string(),
    );
    for item in manifest["scatter"]["binary_files"].as_array().unwrap() {
        files.insert(item["file"].as_str().unwrap().to_string());
    }
    for prototype in manifest["prototypes"].as_array().unwrap() {
        for lod in prototype["lods"].as_array().unwrap() {
            files.insert(lod["file"].as_str().unwrap().to_string());
        }
    }
    for item in manifest["material_recipes"].as_array().unwrap() {
        files.insert(item["file"].as_str().unwrap().to_string());
    }
    for item in manifest["engine_import_recipes"].as_array().unwrap() {
        files.insert(item["file"].as_str().unwrap().to_string());
    }
    files.into_iter().collect()
}

fn manifest() -> Value {
    let kinds = [
        ("rock", vec!["static_surface", "rock"], 3usize),
        ("log", vec!["static_surface", "log"], 3),
        ("shrub", vec!["groundcover_foliage", "shrub_base"], 3),
        ("grass", vec!["groundcover_foliage"], 3),
        ("flower", vec!["groundcover_foliage"], 3),
        ("weed", vec!["groundcover_foliage"], 3),
        ("moss", vec!["groundcover_foliage", "moss_tuft"], 2),
        ("litter", vec!["groundcover_foliage"], 2),
    ];
    let prototypes: Vec<Value> = kinds.iter().enumerate().map(|(index, (kind, targets, lod_count))| {
        let name = format!("{kind}_prototype");
        let lods: Vec<Value> = (0..*lod_count).map(|lod| json!({
            "index": lod, "file": format!("prototypes/{name}_lod{lod}.glb"),
            "vertex_count": 3, "triangle_count": 1,
            "bounds_min": [0.0, 0.0, 0.0], "bounds_max": [1.0, 1.0, 0.0],
        })).collect();
        let _ = index;
        json!({"name": name, "kind": kind, "material_slot": "groundcover_foliage", "surface_targets": targets, "lods": lods})
    }).collect();
    let binary_files: Vec<Value> = (0..26)
        .map(|index| {
            json!({
                "layer_index": 0, "layer_name": "grass", "kind": "grass",
                "chunk_x": index, "chunk_z": 0,
                "file": format!("scatter/chunk_{index:02}.bin"),
                "instance_count": if index < 14 { 9 } else { 8 },
                "bounds_min": [index as f64, 0.0, 0.0],
                "bounds_max": [index as f64 + 1.0, 2.0, 1.0],
            })
        })
        .collect();
    let material_parameter =
        |name: &str, semantic: &str, value_type: &str, source: &str, default_value: &str| {
            json!({
                "name": name, "semantic": semantic, "value_type": value_type,
                "source": source, "default_value": default_value
            })
        };
    let terrain_parameters = vec![
        material_parameter(
            "Midori_MaskTexture",
            "overlay_mask_texture",
            "texture2d",
            "maps/masks_rgba.png",
            "maps/masks_rgba.png",
        ),
        material_parameter(
            "Midori_MossMaskChannel",
            "moss_mask_channel",
            "channel",
            "surface_overlays.moss.channel",
            "G",
        ),
        material_parameter(
            "Midori_WetnessMaskChannel",
            "wetness_mask_channel",
            "channel",
            "surface_overlays.wetness.channel",
            "B",
        ),
        material_parameter(
            "Midori_CrackMaskChannel",
            "crack_mask_channel",
            "channel",
            "surface_overlays.cracks.channel",
            "A",
        ),
    ];
    let groundcover_parameters = vec![
        material_parameter(
            "Midori_AlphaCutoff",
            "alpha_cutoff",
            "float",
            "material_slots.groundcover_foliage.alpha_mode",
            "0.5",
        ),
        material_parameter(
            "Midori_WindStrength",
            "wind_strength",
            "float",
            "wind.strength",
            "0.8",
        ),
        material_parameter(
            "Midori_WindSpeed",
            "wind_speed",
            "float",
            "wind.speed",
            "1.2",
        ),
        material_parameter(
            "Midori_WindDirectionDegrees",
            "wind_direction_degrees",
            "float",
            "wind.direction_degrees",
            "35.0",
        ),
        material_parameter(
            "Midori_WindGustScale",
            "wind_gust_scale",
            "float",
            "wind.gust_scale",
            "0.35",
        ),
        material_parameter(
            "Midori_FadeStartMeters",
            "fade_start_meters",
            "float",
            "profiles.mobile.cull_start",
            "35.0",
        ),
        material_parameter(
            "Midori_FadeEndMeters",
            "fade_end_meters",
            "float",
            "profiles.mobile.cull_end",
            "70.0",
        ),
        material_parameter(
            "Midori_ColorVariationScale",
            "color_variation_scale",
            "float",
            "scatter.color_variation",
            "1.0",
        ),
    ];
    let groundcover_layers: Vec<Value> = [
        ("grass", "grass", 0.13, 0.62),
        ("moss", "moss", 0.10, 0.55),
        ("flower", "flower", 0.14, 0.35),
        ("weed", "weed", 0.14, 0.45),
        ("litter", "litter", 0.06, 0.38),
        ("shrub", "shrub", 0.055, 0.30),
        ("rock", "rock", 0.08, 0.28),
        ("log", "log", 0.065, 0.24),
    ]
    .into_iter()
    .map(|(kind, name, density, coverage)| {
        json!({
            "kind": kind, "name": name, "density": density, "coverage": coverage,
            "patch_scale": 0.18, "patch_softness": 0.12, "seed_offset": 0.0,
            "height": 0.25, "width": 0.12, "curl": 0.2,
            "relief_scale": 0.4, "relief_strength": 0.2,
            "color_base": "#527a3c", "color_tip": "#b8d98a"
        })
    })
    .collect();
    let memory = json!({
        "map_pixel_count": 64, "decoded_map_bytes": 832, "encoded_map_bytes": 1,
        "material_recipe_bytes": 1, "engine_import_recipe_bytes": 1,
        "preview_mesh_bytes": 1, "prototype_mesh_bytes": 1, "scatter_json_bytes": 1,
        "scatter_binary_header_bytes": 416, "scatter_binary_record_bytes": 7104,
        "scatter_binary_bytes": 7520, "total_payload_bytes": 7,
    });
    json!({
        "schema_version": 3, "generator": "midori", "generator_version": "0.1.0",
        "asset_name": "Temperate Forest Floor", "seed": 1337, "units": "meters",
        "map_resolution": 8, "tile_size": 16.0,
        "axis": {"up_axis": "Y", "forward_axis": "Z", "handedness": "right", "unit_scale": 1.0},
        "terrain": {"heightmap_file": "maps/height_u16.png", "masks_file": "maps/masks_rgba.png", "grass_density_file": "maps/grass_density.png", "height_min": 0.0, "height_max": 0.3901228, "bounds_min": [-8.0, 0.0, -8.0], "bounds_max": [8.0, 0.3901228, 8.0]},
        "map_channels": [
            {"file": "height_u16.png", "channels": "16-bit normalized terrain height"},
            {"file": "normal_yplus.png", "channels": "RGB terrain normal, green channel Y+"},
            {"file": "normal_yminus.png", "channels": "RGB terrain normal, green channel Y-"},
            {"file": "masks_rgba.png", "channels": "R grass density, G moss, B wetness, A cracks"},
            {"file": "grass_density.png", "channels": "single-channel grass density"}
        ],
        "normal_conventions": {"unity_yplus_file": "maps/normal_yplus.png", "unreal_yminus_file": "maps/normal_yminus.png"},
        "material_slots": [
            {"name": "terrain_surface", "purpose": "preview terrain and baked soil surface", "alpha_mode": "opaque", "double_sided": false, "shadows": true},
            {"name": "groundcover_foliage", "purpose": "grass, moss, and low groundcover prototype meshes", "alpha_mode": "masked", "double_sided": true, "shadows": false}
        ],
        "mobile": {"tile_size": 16.0, "terrain_resolution": 8, "density_scale": 0.75, "lod0_max_distance": 10.0, "lod1_max_distance": 20.0, "lod2_max_distance": 30.0, "cull_start": 35.0, "cull_end": 70.0, "shadows": false, "material_slots": 2, "max_instances_per_tile": 300, "max_instances_per_chunk": 20, "lod0_max_triangles": 120, "lod1_max_triangles": 80, "lod2_max_triangles": 40, "grass_collision": false, "moss_collision": false},
        "console": {"tile_size": 16.0, "terrain_resolution": 8, "density_scale": 1.0, "lod0_max_distance": 15.0, "lod1_max_distance": 30.0, "lod2_max_distance": 50.0, "cull_start": 35.0, "cull_end": 70.0, "shadows": true, "material_slots": 2, "max_instances_per_tile": 300, "max_instances_per_chunk": 20, "lod0_max_triangles": 120, "lod1_max_triangles": 80, "lod2_max_triangles": 40, "grass_collision": false, "moss_collision": false},
        "unity": {"terrain_heightmap": "maps/height_u16.png", "detail_density_map": "maps/grass_density.png", "normal_map": "maps/normal_yplus.png", "detail_mode": "GPU-instanced terrain detail mesh prefabs", "detail_batch_max_instances": 1023},
        "unreal": {"landscape_heightmap": "maps/height_u16.png", "landscape_weightmap": "maps/masks_rgba.png", "normal_map": "maps/normal_yminus.png", "foliage_mode": "Static Mesh Foliage or Landscape Grass Type", "static_mesh_pipeline": "GLB prototypes first; FBX adapter only where pipeline requires it"},
        "wind_packing": {"phase": "COLOR_0.y", "stiffness": "COLOR_0.z", "height": "TEXCOORD_1.x", "color_variation": "COLOR_0.x", "normalized_progress": "COLOR_0.w"},
        "groundcover_layers": groundcover_layers,
        "material_parameters": [
            {"material_slot": "terrain_surface", "parameter_set": "midori_terrain_static_v1", "runtime_policy": "engine_native_static", "parameters": terrain_parameters},
            {"material_slot": "groundcover_foliage", "parameter_set": "midori_groundcover_foliage_static_v1", "runtime_policy": "engine_native_static", "parameters": groundcover_parameters}
        ],
        "material_recipes": [
            {"material_slot": "terrain_surface", "file": "materials/terrain_surface.recipe.json", "runtime_policy": "engine_native_static", "engine_targets": ["unity_terrain_material", "unreal_landscape_material"]},
            {"material_slot": "groundcover_foliage", "file": "materials/groundcover_foliage.recipe.json", "runtime_policy": "engine_native_static", "engine_targets": ["unity_detail_mesh_material", "unreal_static_mesh_foliage_material"]}
        ],
        "engine_import_recipes": [
            {"engine": "unity", "profile": "mobile", "file": "engines/unity_import.recipe.json", "runtime_policy": "engine_native_static", "expected_systems": ["Unity TerrainData", "GPU-instanced terrain detail mesh prefabs"]},
            {"engine": "unreal", "profile": "console", "file": "engines/unreal_import.recipe.json", "runtime_policy": "engine_native_static", "expected_systems": ["Unreal Landscape", "Static Mesh Foliage"]}
        ],
        "surface_overlays": [
            {"name": "moss", "source_file": "maps/masks_rgba.png", "channel": "G", "targets": ["terrain_surface", "rock", "log"], "application": "static material overlay mask", "runtime_policy": "baked_static"},
            {"name": "wetness", "source_file": "maps/masks_rgba.png", "channel": "B", "targets": ["terrain_surface", "rock", "log"], "application": "static material overlay mask", "runtime_policy": "baked_static"},
            {"name": "cracks", "source_file": "maps/masks_rgba.png", "channel": "A", "targets": ["terrain_surface", "scatter_exclusion"], "application": "static material overlay mask", "runtime_policy": "baked_static"}
        ],
        "prototypes": prototypes, "scatter": {
            "enabled": true, "file": "scatter/scatter.json", "format": "json.chunked_instances.v1",
            "chunk_size": 8.0,
            "fields": ["position.xyz", "yaw_radians", "height_multiplier", "width_multiplier", "phase_radians", "color_variation"],
            "binary_format": {
                "format": "midori.scatter.bin.v1",
                "header_bytes": 16,
                "record_stride_bytes": 32, "endian": "little",
                "header": ["magic:u8[4]=MDSI", "version:u32", "record_stride_bytes:u32", "instance_count:u32"],
                "record": ["position.x:f32", "position.y:f32", "position.z:f32", "yaw_radians:f32", "height_multiplier:f32", "width_multiplier:f32", "phase_radians:f32", "color_variation:f32"]
            },
            "binary_files": binary_files
        }, "shader_policy": "preview_only", "texture_pipeline": "parked", "memory_footprint": memory
    })
}

fn scatter_count(index: usize) -> i64 {
    if index < 14 { 9 } else { 8 }
}

fn scatter_summaries(package: &Path, source: &str, camel_case: bool) -> Vec<Value> {
    if source == "json" {
        return scatter_json_summaries(package, camel_case);
    }
    (0..26)
        .map(|index| {
            let file = package.join(format!("scatter/chunk_{index:02}.bin"));
            let bytes = fs::read(&file).unwrap();
            let count = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
            let records = bytes[16..]
                .as_chunks::<32>().0.iter()
                .map(|record| {
                    let mut values = [0.0f64; 8];
                    for (slot, value) in values.iter_mut().enumerate() {
                        let start = slot * 4;
                        *value = f32::from_le_bytes(record[start..start + 4].try_into().unwrap())
                            as f64;
                    }
                    values
                })
                .collect::<Vec<_>>();
            assert_eq!(records.len(), count);
            let range = |slot: usize| {
                records
                    .iter()
                    .map(|record| record[slot])
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(minimum, maximum), value| {
                        (minimum.min(value), maximum.max(value))
                    })
            };
            let (yaw_min, yaw_max) = range(3);
            let (height_min, height_max) = range(4);
            let (width_min, width_max) = range(5);
            let (phase_min, phase_max) = range(6);
            let (color_min, color_max) = range(7);
            let file_checksum = fnv1a(&bytes);
            let record_checksum = fnv1a(&bytes[16..]);
            if camel_case {
                json!({
                    "source": source, "instanceCount": count,
                    "fileChecksum": fnv_hex_u64(file_checksum), "recordChecksum": fnv_hex_u64(record_checksum),
                    "yawMin": yaw_min, "yawMax": yaw_max, "phaseMin": phase_min, "phaseMax": phase_max,
                    "heightMin": height_min, "heightMax": height_max, "widthMin": width_min, "widthMax": width_max,
                    "colorVariationMin": color_min, "colorVariationMax": color_max
                })
            } else {
                json!({
                    "source": source, "instance_count": count,
                    "file_checksum": file_checksum, "record_checksum": record_checksum,
                    "yaw_min": yaw_min, "yaw_max": yaw_max, "phase_min": phase_min, "phase_max": phase_max,
                    "height_min": height_min, "height_max": height_max, "width_min": width_min, "width_max": width_max,
                    "color_variation_min": color_min, "color_variation_max": color_max
                })
            }
        })
        .collect()
}

fn scatter_json_summaries(package: &Path, camel_case: bool) -> Vec<Value> {
    let path = package.join("scatter/scatter.json");
    let bytes = fs::read(&path).unwrap();
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    let file_checksum = fnv1a(&bytes);
    document
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["chunks"].as_array().unwrap().iter())
        .map(|chunk| {
            let instances = chunk["instances"].as_array().unwrap();
            let records = instances
                .iter()
                .map(|instance| {
                    let position = instance["position"].as_array().unwrap();
                    [
                        position[0].as_f64().unwrap(),
                        position[1].as_f64().unwrap(),
                        position[2].as_f64().unwrap(),
                        instance["yaw"].as_f64().unwrap(),
                        instance["height"].as_f64().unwrap(),
                        instance["width"].as_f64().unwrap(),
                        instance["phase"].as_f64().unwrap(),
                        instance["color_variation"].as_f64().unwrap(),
                    ]
                })
                .collect::<Vec<_>>();
            let mut packed_records = Vec::with_capacity(records.len() * 32);
            for record in &records {
                for value in record {
                    packed_records.extend((*value as f32).to_le_bytes());
                }
            }
            let range = |slot: usize| {
                records
                    .iter()
                    .map(|record| record[slot])
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(minimum, maximum), value| {
                        (minimum.min(value), maximum.max(value))
                    })
            };
            let (yaw_min, yaw_max) = range(3);
            let (height_min, height_max) = range(4);
            let (width_min, width_max) = range(5);
            let (phase_min, phase_max) = range(6);
            let (color_min, color_max) = range(7);
            let record_checksum = fnv1a(&packed_records);
            if camel_case {
                json!({
                    "source": "json", "instanceCount": instances.len(),
                    "fileChecksum": fnv_hex_u64(file_checksum), "recordChecksum": fnv_hex_u64(record_checksum),
                    "yawMin": yaw_min, "yawMax": yaw_max, "phaseMin": phase_min, "phaseMax": phase_max,
                    "heightMin": height_min, "heightMax": height_max, "widthMin": width_min, "widthMax": width_max,
                    "colorVariationMin": color_min, "colorVariationMax": color_max
                })
            } else {
                json!({
                    "source": "json", "instance_count": instances.len(),
                    "file_checksum": file_checksum, "record_checksum": record_checksum,
                    "yaw_min": yaw_min, "yaw_max": yaw_max, "phase_min": phase_min, "phase_max": phase_max,
                    "height_min": height_min, "height_max": height_max, "width_min": width_min, "width_max": width_max,
                    "color_variation_min": color_min, "color_variation_max": color_max
                })
            }
        })
        .collect()
}

fn map_summaries(package: &Path) -> Vec<Value> {
    [
        ("height_u16.png", "L16"),
        ("normal_yplus.png", "RGB8"),
        ("normal_yminus.png", "RGB8"),
        ("masks_rgba.png", "RGBA8"),
        ("grass_density.png", "L8"),
    ]
    .into_iter()
    .map(|(file, color_type)| {
        let path = package.join("maps").join(file);
        let bytes = fs::read(&path).unwrap();
        let decoder = png::Decoder::new(fs::File::open(&path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let output_size = reader.output_buffer_size();
        let mut output = vec![0; output_size];
        let info = reader.next_frame(&mut output).unwrap();
        let channels = info.color_type.samples();
        let bytes_per_sample = if info.bit_depth == png::BitDepth::Sixteen {
            2
        } else {
            1
        };
        let mut minimum = vec![u64::MAX; channels];
        let mut maximum = vec![0u64; channels];
        for sample in output[..info.buffer_size()].chunks_exact(channels * bytes_per_sample) {
            for channel in 0..channels {
                let start = channel * bytes_per_sample;
                let value = if bytes_per_sample == 2 {
                    u16::from_be_bytes([sample[start], sample[start + 1]]) as u64
                } else {
                    u64::from(sample[start])
                };
                minimum[channel] = minimum[channel].min(value);
                maximum[channel] = maximum[channel].max(value);
            }
        }
        json!({
            "file": format!("maps/{file}"), "width": info.width, "height": info.height,
            "color_type": color_type, "channels": channels, "file_checksum": fnv1a(&bytes),
            "channel_min": minimum, "channel_max": maximum
        })
    })
    .collect()
}

fn prototype_summaries(manifest: &Value, package: &Path) -> Vec<Value> {
    manifest["prototypes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|prototype| {
            prototype["lods"].as_array().unwrap().iter().map(|lod| {
                let file = lod["file"].as_str().unwrap();
                let path = package.join(file);
                let document = glb_json(&path);
                let meshes = document["meshes"].as_array().unwrap();
                let primitives = meshes
                    .iter()
                    .flat_map(|mesh| mesh["primitives"].as_array().unwrap())
                    .collect::<Vec<_>>();
                let first = primitives.first().unwrap();
                let attributes = first["attributes"].as_object().unwrap();
                let accessors = document["accessors"].as_array().unwrap();
                let vertex_accessor = accessors[attributes["POSITION"].as_u64().unwrap() as usize]
                    .as_object()
                    .unwrap();
                let vertex_count = vertex_accessor["count"].as_u64().unwrap();
                let index_accessor = accessors[first["indices"].as_u64().unwrap() as usize]
                    .as_object()
                    .unwrap();
                let triangle_count = index_accessor["count"].as_u64().unwrap() / 3;
                let used_materials = primitives
                    .iter()
                    .filter_map(|primitive| primitive["material"].as_u64())
                    .collect::<BTreeSet<_>>();
                let has_attribute = |name: &str| {
                    primitives.iter().all(|primitive| {
                        primitive["attributes"]
                            .as_object()
                            .unwrap()
                            .contains_key(name)
                    })
                };
                let bounds = |name: &str, default: [f64; 3]| {
                    vertex_accessor
                        .get(name)
                        .and_then(Value::as_array)
                        .map(|values| {
                            [
                                values[0].as_f64().unwrap(),
                                values[1].as_f64().unwrap(),
                                values[2].as_f64().unwrap(),
                            ]
                        })
                        .unwrap_or(default)
                };
                let bytes = fs::read(&path).unwrap();
                json!({
                    "file": file, "lod_index": lod["index"],
                    "vertex_count": vertex_count, "triangle_count": triangle_count,
                    "mesh_count": meshes.len(), "primitive_count": primitives.len(),
                    "has_positions": has_attribute("POSITION"),
                    "has_normals": has_attribute("NORMAL"),
                    "has_tangents": has_attribute("TANGENT"),
                    "normals_are_valid": has_attribute("NORMAL"),
                    "tangents_are_valid": has_attribute("TANGENT"),
                    "has_texcoord0": has_attribute("TEXCOORD_0"),
                    "has_texcoord1": has_attribute("TEXCOORD_1"),
                    "has_color0": has_attribute("COLOR_0"),
                    "bounds_min": bounds("min", [0.0, 0.0, 0.0]),
                    "bounds_max": bounds("max", [1.0, 1.0, 0.0]),
                    "used_material_count": used_materials.len(),
                    "file_checksum": fnv1a(&bytes)
                })
            })
        })
        .collect()
}

fn decode_png(path: &Path) -> (u32, u32, png::ColorType, Vec<u8>) {
    let decoder = png::Decoder::new(fs::File::open(path).unwrap());
    let mut reader = decoder.read_info().unwrap();
    let mut data = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut data).unwrap();
    (
        info.width,
        info.height,
        info.color_type,
        data[..info.buffer_size()].to_vec(),
    )
}

fn map_relationships(package: &Path) -> Value {
    let (_, _, _, yplus) = decode_png(&package.join("maps/normal_yplus.png"));
    let (_, _, _, yminus) = decode_png(&package.join("maps/normal_yminus.png"));
    let normal_pixels = (yplus.len().min(yminus.len())) / 3;
    let mut red_blue_mismatches = 0usize;
    let mut green_flip_mismatches = 0usize;
    let mut green_flip_max_error = 0u8;
    for pixel in 0..normal_pixels {
        let offset = pixel * 3;
        if yplus[offset] != yminus[offset] || yplus[offset + 2] != yminus[offset + 2] {
            red_blue_mismatches += 1;
        }
        let sum = u16::from(yplus[offset + 1]) + u16::from(yminus[offset + 1]);
        let error = sum.abs_diff(255) as u8;
        if error > 0 {
            green_flip_mismatches += 1;
        }
        green_flip_max_error = green_flip_max_error.max(error);
    }
    let (_, _, _, masks) = decode_png(&package.join("maps/masks_rgba.png"));
    let (_, _, _, density) = decode_png(&package.join("maps/grass_density.png"));
    let density_pixels = (masks.len() / 4).min(density.len());
    let density_mismatches = (0..density_pixels)
        .filter(|pixel| masks[pixel * 4] != density[*pixel])
        .count();
    json!({
        "normal_pair_pixels": normal_pixels,
        "normal_red_blue_mismatches": red_blue_mismatches,
        "normal_green_flip_mismatches": green_flip_mismatches,
        "normal_green_flip_max_error": green_flip_max_error,
        "grass_density_pixels": density_pixels,
        "grass_density_mask_r_mismatches": density_mismatches
    })
}

fn scatter_parity(package: &Path, manifest: &Value) -> Value {
    let json_summaries = scatter_summaries(package, "json", false);
    let binary_summaries = scatter_summaries(package, "binary", false);
    let matching = json_summaries.len().min(binary_summaries.len());
    let instance_mismatches = json_summaries
        .iter()
        .zip(&binary_summaries)
        .filter(|(json, binary)| json["instance_count"] != binary["instance_count"])
        .count();
    let record_mismatches = json_summaries
        .iter()
        .zip(&binary_summaries)
        .filter(|(json, binary)| json["record_checksum"] != binary["record_checksum"])
        .count();
    let json_document: Value =
        serde_json::from_slice(&fs::read(package.join("scatter/scatter.json")).unwrap()).unwrap();
    let json_chunks = json_document
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|set| set["chunks"].as_array().unwrap().iter())
        .collect::<Vec<_>>();
    let binary_chunks = manifest["scatter"]["binary_files"].as_array().unwrap();
    let bounds_mismatches = json_chunks
        .iter()
        .zip(binary_chunks)
        .filter(|(json, binary)| {
            json["bounds_min"] != binary["bounds_min"] || json["bounds_max"] != binary["bounds_max"]
        })
        .count();
    json!({
        "json_chunk_count": json_summaries.len(),
        "binary_chunk_count": binary_summaries.len(),
        "matching_chunk_count": matching,
        "missing_binary_chunk_count": json_summaries.len().saturating_sub(binary_summaries.len()),
        "extra_binary_chunk_count": binary_summaries.len().saturating_sub(json_summaries.len()),
        "instance_count_mismatch_count": instance_mismatches,
        "bounds_mismatch_count": bounds_mismatches,
        "record_checksum_mismatch_count": record_mismatches
    })
}

fn glb_json(path: &Path) -> Value {
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"glTF");
    assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 2);
    let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    assert_eq!(&bytes[16..20], b"JSON");
    serde_json::from_slice(&bytes[20..20 + json_length]).unwrap()
}

fn material_recipe_summaries(package: &Path) -> Vec<Value> {
    [
        "materials/terrain_surface.recipe.json",
        "materials/groundcover_foliage.recipe.json",
    ]
    .into_iter()
    .map(|file| {
        let bytes = fs::read(package.join(file)).unwrap();
        let recipe: Value = serde_json::from_slice(&bytes).unwrap();
        json!({
            "material_slot": recipe["material_slot"],
            "parameter_set": recipe["parameter_set"],
            "runtime_policy": recipe["runtime_policy"],
            "shader_policy": recipe["shader_policy"],
            "texture_pipeline": recipe["texture_pipeline"],
            "parameter_count": recipe["parameters"].as_array().unwrap().len(),
            "engine_targets": recipe["engine_targets"],
            "file_checksum": fnv1a(&bytes),
            "required_textures": recipe["required_textures"],
            "required_vertex_streams": recipe["required_vertex_streams"],
            "surface_overlays": recipe["surface_overlays"]
        })
    })
    .collect()
}

fn engine_recipe_summaries(package: &Path) -> Vec<Value> {
    [
        "engines/unity_import.recipe.json",
        "engines/unreal_import.recipe.json",
    ]
    .into_iter()
    .map(|file| {
        let bytes = fs::read(package.join(file)).unwrap();
        let recipe: Value = serde_json::from_slice(&bytes).unwrap();
        json!({
            "engine": recipe["engine"], "profile": recipe["profile"],
            "source_file_count": recipe["source_files"].as_array().unwrap().len(),
            "scatter_instances": recipe["scatter"]["instance_count"],
            "scatter_binary_chunks": recipe["scatter"]["binary_chunk_count"],
            "file_checksum": fnv1a(&bytes)
        })
    })
    .collect()
}

fn profile_budget_summaries(manifest: &Value, package: &Path) -> Vec<Value> {
    let prototype_summaries = prototype_summaries(manifest, package);
    let max_lod = |lod_index: u64| {
        prototype_summaries
            .iter()
            .filter(|summary| summary["lod_index"].as_u64() == Some(lod_index))
            .map(|summary| summary["triangle_count"].as_u64().unwrap())
            .max()
            .unwrap_or(0)
    };
    let max_used_material_count = prototype_summaries
        .iter()
        .map(|summary| summary["used_material_count"].as_u64().unwrap())
        .max()
        .unwrap_or(0);
    ["mobile", "console"]
        .into_iter()
        .map(|profile| {
            let profile_value = &manifest[profile];
            json!({
                "profile": profile, "passed": true, "prototype_lod_count": prototype_summaries.len(),
                "max_lod0_triangles": max_lod(0), "max_lod1_triangles": max_lod(1), "max_lod2_triangles": max_lod(2),
                "lod0_triangle_budget": profile_value["lod0_max_triangles"],
                "lod1_triangle_budget": profile_value["lod1_max_triangles"],
                "lod2_triangle_budget": profile_value["lod2_max_triangles"],
                "max_used_material_count": max_used_material_count,
                "material_slot_budget": profile_value["material_slots"],
                "scatter_instance_count": 222, "max_chunk_instance_count": 9,
                "max_instances_per_tile_budget": profile_value["max_instances_per_tile"],
                "max_instances_per_chunk_budget": profile_value["max_instances_per_chunk"],
                "triangle_budget_violation_count": 0, "material_slot_violation_count": 0,
                "instance_budget_violation_count": 0
            })
        })
        .collect()
}

fn surface_target_entries(manifest: &Value, camel_case: bool) -> Value {
    if camel_case {
        json!(
            manifest["prototypes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|prototype| {
                    let targets = prototype["surface_targets"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_str().unwrap())
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("{}:{targets}", prototype["name"].as_str().unwrap())
                })
                .collect::<Vec<_>>()
        )
    } else {
        json!(manifest["prototypes"].as_array().unwrap().iter().map(|prototype| json!({
            "name": prototype["name"], "kind": prototype["kind"], "targets": prototype["surface_targets"]
        })).collect::<Vec<_>>())
    }
}

fn material_parameter_report_fields(
    manifest: &Value,
    camel_case: bool,
) -> (Value, Value, Value, Value, Value, Value) {
    let parameters = manifest["material_parameters"].as_array().unwrap();
    let slots = parameters
        .iter()
        .map(|item| item["material_slot"].clone())
        .collect::<Vec<_>>();
    let policies = parameters
        .iter()
        .map(|item| item["runtime_policy"].clone())
        .collect::<Vec<_>>();
    let semantics = parameters
        .iter()
        .flat_map(|item| {
            let slot = item["material_slot"].as_str().unwrap();
            item["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |parameter| {
                    json!(format!(
                        "{slot}:{}",
                        parameter["semantic"].as_str().unwrap()
                    ))
                })
        })
        .collect::<Vec<_>>();
    let names = |slot: &str| {
        let mut result = parameters
            .iter()
            .find(|item| item["material_slot"] == slot)
            .unwrap()["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        result.sort();
        result
    };
    let _ = camel_case;
    (
        json!(slots),
        json!(policies),
        json!(semantics),
        json!(names("groundcover_foliage")),
        json!(names("terrain_surface")),
        json!(parameters.len()),
    )
}

fn add_material_parameter_fields(report: &mut Value, manifest: &Value, camel_case: bool) {
    let (slots, policies, semantics, groundcover, terrain, count) =
        material_parameter_report_fields(manifest, camel_case);
    let (count_name, slots_name, policies_name, semantics_name, groundcover_name, terrain_name) =
        if camel_case {
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
    report[count_name] = count;
    report[slots_name] = slots;
    report[policies_name] = policies;
    report[semantics_name] = semantics;
    report[groundcover_name] = groundcover;
    report[terrain_name] = terrain;
}

fn add_material_recipe_fields(report: &mut Value, manifest: &Value, camel_case: bool) {
    let recipes = manifest["material_recipes"].as_array().unwrap();
    let (count, files, policies, targets, terrain, groundcover) = if camel_case {
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
    report[count] = json!(recipes.len());
    report[files] = json!(
        recipes
            .iter()
            .map(|item| item["file"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    report[policies] = json!(
        recipes
            .iter()
            .map(|item| item["runtime_policy"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    report[targets] = json!(
        recipes
            .iter()
            .flat_map(|item| {
                let slot = item["material_slot"].as_str().unwrap();
                item["engine_targets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(move |target| format!("{slot}:{}", target.as_str().unwrap()))
            })
            .collect::<Vec<_>>()
    );
    report[terrain] = json!("materials/terrain_surface.recipe.json");
    report[groundcover] = json!("materials/groundcover_foliage.recipe.json");
}

fn add_engine_recipe_fields(report: &mut Value, manifest: &Value, camel_case: bool) {
    let recipes = manifest["engine_import_recipes"].as_array().unwrap();
    let (count, files, profiles, policies, systems, unity, unreal) = if camel_case {
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
    report[count] = json!(recipes.len());
    report[files] = json!(
        recipes
            .iter()
            .map(|item| item["file"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    report[profiles] = json!(
        recipes
            .iter()
            .map(|item| format!(
                "{}:{}",
                item["engine"].as_str().unwrap(),
                item["profile"].as_str().unwrap()
            ))
            .collect::<Vec<_>>()
    );
    report[policies] = json!(
        recipes
            .iter()
            .map(|item| item["runtime_policy"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    report[systems] = json!(
        recipes
            .iter()
            .flat_map(|item| {
                let engine = item["engine"].as_str().unwrap();
                item["expected_systems"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(move |system| format!("{engine}:{}", system.as_str().unwrap()))
            })
            .collect::<Vec<_>>()
    );
    report[unity] = json!("engines/unity_import.recipe.json");
    report[unreal] = json!("engines/unreal_import.recipe.json");
}

fn add_surface_fields(report: &mut Value, manifest: &Value, camel_case: bool) {
    let overlays = manifest["surface_overlays"].as_array().unwrap();
    let (count, names, sources, channels, policies, targets) = if camel_case {
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
    report[count] = json!(overlays.len());
    report[names] = json!(
        overlays
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    report[sources] = json!(
        overlays
            .iter()
            .map(|item| item["source_file"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    report[channels] = json!(
        overlays
            .iter()
            .map(|item| item["channel"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    report[policies] = json!(
        overlays
            .iter()
            .map(|item| item["runtime_policy"].as_str().unwrap())
            .collect::<Vec<_>>()
    );
    if camel_case {
        report[targets] = json!(
            overlays
                .iter()
                .map(|item| item["targets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(","))
                .collect::<Vec<_>>()
        );
    } else {
        report[targets] = json!(
            overlays
                .iter()
                .map(|item| item["targets"].clone())
                .collect::<Vec<_>>()
        );
    }
}

fn add_prototype_target_fields(report: &mut Value, manifest: &Value, camel_case: bool) {
    report[if camel_case {
        "prototypeSurfaceTargets"
    } else {
        "prototype_surface_targets"
    }] = surface_target_entries(manifest, camel_case);
    let values = |kind: &str| {
        let mut targets = BTreeSet::new();
        for prototype in manifest["prototypes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|prototype| prototype["kind"] == kind)
        {
            for target in prototype["surface_targets"].as_array().unwrap() {
                targets.insert(target.as_str().unwrap());
            }
        }
        targets.into_iter().collect::<Vec<_>>()
    };
    if camel_case {
        report["rockPrototypeSurfaceTargets"] = json!(values("rock"));
        report["logPrototypeSurfaceTargets"] = json!(values("log"));
        report["shrubPrototypeSurfaceTargets"] = json!(values("shrub"));
    } else {
        report["rock_prototype_surface_targets"] = json!(values("rock"));
        report["log_prototype_surface_targets"] = json!(values("log"));
        report["shrub_prototype_surface_targets"] = json!(values("shrub"));
    }
}

fn midori_report(manifest: &Value, binary_files: &[String], package: &Path) -> Value {
    let (scatter_file_checksum, scatter_record_checksum) = scatter_checksums(package);
    let scatter_json_chunks = scatter_summaries(package, "json", false);
    let scatter_binary_summaries = scatter_summaries(package, "binary", false);
    let scatter_binary_instances = scatter_binary_summaries
        .iter()
        .map(|summary| summary["instance_count"].as_u64().unwrap())
        .sum::<u64>();
    let map_files = [
        "maps/height_u16.png",
        "maps/normal_yplus.png",
        "maps/normal_yminus.png",
        "maps/masks_rgba.png",
        "maps/grass_density.png",
    ];
    let report = json!({
        "manifest": manifest,
        "map_files": map_files,
        "map_summaries": map_summaries(package),
        "map_relationships": map_relationships(package),
        "prototype_files": manifest["prototypes"].as_array().unwrap().iter().flat_map(|prototype| prototype["lods"].as_array().unwrap().iter().map(|lod| lod["file"].clone())).collect::<Vec<_>>(),
        "prototype_summaries": prototype_summaries(manifest, package),
        "profile_budget_summaries": profile_budget_summaries(manifest, package),
        "scatter_binary_files": binary_files,
        "scatter_binary_instances": scatter_binary_instances,
        "scatter_binary_file_checksum_xor": scatter_file_checksum,
        "scatter_binary_record_checksum_xor": scatter_record_checksum,
        "scatter_json_chunks": scatter_json_chunks,
        "scatter_binary_summaries": scatter_binary_summaries,
        "scatter_parity": scatter_parity(package, manifest),
        "material_recipe_summaries": material_recipe_summaries(package),
        "engine_import_recipe_summaries": engine_recipe_summaries(package),
        "memory_footprint": manifest["memory_footprint"]
    });
    report
}

fn package_identity_fields(
    report: &mut Value,
    manifest: &Value,
    manifest_checksum: &str,
    source_checksum: &str,
    normal_key: &str,
    camel_case: bool,
) {
    let files = source_files(manifest, normal_key);
    if camel_case {
        report["manifestFileChecksum"] = json!(manifest_checksum);
        report["sourceFileCount"] = json!(files.len());
        report["sourceFileChecksumXor"] = json!(source_checksum);
        report["sourceFiles"] = json!(files);
    } else {
        report["manifest_file_checksum"] = json!(manifest_checksum);
        report["source_file_count"] = json!(files.len());
        report["source_file_checksum_xor"] = json!(source_checksum);
        report["source_files"] = json!(files);
    }
}

fn screenshot_summary(
    path: &str,
    width: usize,
    height: usize,
    checksum: &str,
    bytes: u64,
) -> Value {
    json!({"path": path, "status": "captured", "method": "AutomationLibrary.take_high_res_screenshot", "exists": true, "bytes": bytes, "checksum": checksum, "width": width, "height": height})
}

fn add_common_engine_recipe_fields(report: &mut Value, manifest: &Value, camel_case: bool) {
    add_material_parameter_fields(report, manifest, camel_case);
    add_material_recipe_fields(report, manifest, camel_case);
    add_engine_recipe_fields(report, manifest, camel_case);
    add_surface_fields(report, manifest, camel_case);
    add_prototype_target_fields(report, manifest, camel_case);
}

fn unity_report(
    manifest: &Value,
    manifest_checksum: &str,
    source_checksum: &str,
    package: &Path,
    validation: &Path,
) -> Value {
    let (scatter_file_checksum, scatter_record_checksum) = scatter_checksums(package);
    let import_checksum =
        fnv_hex(&fs::read(validation.join("screenshots/unity_import.png")).unwrap());
    let density_checksum =
        fnv_hex(&fs::read(validation.join("screenshots/unity_density.png")).unwrap());
    let import_bytes = fs::metadata(validation.join("screenshots/unity_import.png"))
        .unwrap()
        .len();
    let density_bytes = fs::metadata(validation.join("screenshots/unity_density.png"))
        .unwrap()
        .len();
    let mut report = json!({
        "schemaVersion": 3, "profile": "mobile", "tileSizeMeters": 16.0,
        "terrainSize": {"x": 16.0, "y": 0.3901228, "z": 16.0},
        "heightmapFile": "maps/height_u16.png", "densityMapFile": "maps/grass_density.png",
        "normalMapFile": "maps/normal_yplus.png", "unityHintHeightmap": "maps/height_u16.png",
        "unityHintDensityMap": "maps/grass_density.png", "unityHintNormalMap": "maps/normal_yplus.png",
        "importScreenshot": screenshot_summary("screenshots/unity_import.png", 1024, 1024, &import_checksum, import_bytes),
        "densityScreenshot": screenshot_summary("screenshots/unity_density.png", 512, 512, &density_checksum, density_bytes),
        "mobileDensityScale": 0.75, "mobileLod0MaxDistance": 10.0, "mobileLod1MaxDistance": 20.0,
        "mobileLod2MaxDistance": 30.0, "mobileCullStartMeters": 35.0, "mobileCullEndMeters": 70.0,
        "mobileShadows": false, "mobileMaterialSlots": 2, "mobileMaxInstancesPerTile": 300,
        "mobileMaxInstancesPerChunk": 20, "mobileGrassCollision": false, "mobileMossCollision": false,
        "materialSlotsDeclared": 2, "hasTerrainSurfaceMaterialSlot": true,
        "hasGroundcoverFoliageMaterialSlot": true, "groundcoverMaterialAlphaMode": "masked",
        "groundcoverMaterialDoubleSided": true, "groundcoverMaterialShadows": false,
        "windPhase": "COLOR_0.y", "windStiffness": "COLOR_0.z", "windHeight": "TEXCOORD_1.x",
        "windColorVariation": "COLOR_0.x", "windNormalizedProgress": "COLOR_0.w",
        "prototypesDeclared": 8, "lod0PrototypesDeclared": 8, "lodFilesDeclared": 22,
        "detailPrototypesCreated": 8, "detailPrototypeFailures": 0, "detailPrototypeFallbackErrors": [],
        "detailPrototypesLoadedFromAssets": 0, "detailPrototypesGeneratedFromGlb": 8,
        "nonZeroDetailCells": 512, "scatterBinaryInstances": 222,
        "scatterBinaryChunks": 26, "scatterBinaryRecordsValidated": true, "scatterBinaryRecordsRead": 222,
        "scatterBinaryFileChecksumXor": fnv_hex_u64(scatter_file_checksum), "scatterBinaryRecordChecksumXor": fnv_hex_u64(scatter_record_checksum),
        "scatterChunkReports": scatter_summaries(package, "binary", true)
    });
    package_identity_fields(
        &mut report,
        manifest,
        manifest_checksum,
        source_checksum,
        "unity_yplus_file",
        true,
    );
    add_common_engine_recipe_fields(&mut report, manifest, true);
    report
}

fn unreal_report(
    manifest: &Value,
    manifest_checksum: &str,
    source_checksum: &str,
    dry_run: bool,
    package: &Path,
    validation: &Path,
) -> Value {
    let (scatter_file_checksum, scatter_record_checksum) = scatter_checksums(package);
    let import_checksum =
        fnv_hex(&fs::read(validation.join("screenshots/unreal_import.png")).unwrap());
    let foliage_checksum =
        fnv_hex(&fs::read(validation.join("screenshots/unreal_foliage.png")).unwrap());
    let import_bytes = fs::metadata(validation.join("screenshots/unreal_import.png"))
        .unwrap()
        .len();
    let foliage_bytes = fs::metadata(validation.join("screenshots/unreal_foliage.png"))
        .unwrap()
        .len();
    let destination = if dry_run {
        "/Game/Midori/Imported"
    } else {
        "/Game/Midori/FakeEditor/Temperate_Forest_Floor"
    };
    let import_files = unreal_import_files(manifest);
    let import_destinations = import_files
        .iter()
        .map(|file| {
            file.rsplit_once('/')
                .map(|(parent, _)| format!("{destination}/{parent}"))
                .unwrap_or_else(|| destination.to_string())
        })
        .collect::<Vec<_>>();
    let import_tasks = import_files
        .iter()
        .zip(import_destinations.iter())
        .map(|(file, destination)| {
            json!({
                "file": file, "destination_path": destination,
                "extension": format!(".{}", file.rsplit_once('.').map(|(_, ext)| ext).unwrap_or(""))
            })
        })
        .collect::<Vec<_>>();
    let prototype_count = manifest["prototypes"].as_array().unwrap().len();
    let mut scatter_chunks = scatter_summaries(package, "binary", false);
    for chunk in &mut scatter_chunks {
        for field in ["file_checksum", "record_checksum"] {
            let checksum = chunk[field].as_u64().unwrap();
            chunk[field] = json!(fnv_hex_u64(checksum));
        }
    }
    let mut report = json!({
        "schema_version": 3, "dry_run": dry_run, "profile": "console",
        "tile_size_meters": 16.0, "landscape_heightmap": "maps/height_u16.png",
        "landscape_weightmap": "maps/masks_rgba.png", "normal_map": "maps/normal_yminus.png",
        "screenshots": {
            "import": screenshot_summary("screenshots/unreal_import.png", 1024, 1024, &import_checksum, import_bytes),
            "foliage_settings": screenshot_summary("screenshots/unreal_foliage.png", 1024, 1024, &foliage_checksum, foliage_bytes)
        },
        "console_density_scale": 1.0, "console_lod0_max_distance_meters": 15.0,
        "console_lod1_max_distance_meters": 30.0, "console_lod2_max_distance_meters": 50.0,
        "console_material_slots": 2, "console_max_instances_per_tile": 300,
        "console_max_instances_per_chunk": 20, "console_shadows": true,
        "console_grass_collision": false, "console_moss_collision": false,
        "console_cull_start_meters": 35.0, "console_cull_end_meters": 70.0,
        "console_cull_start_cm": 3500, "console_cull_end_cm": 7000,
        "prototypes_declared": prototype_count, "lod0_prototypes_declared": prototype_count,
        "lod_files_declared": 22, "foliage_type_status": if dry_run { "dry_run" } else { "created" },
        "foliage_type_expected_count": prototype_count, "foliage_type_count": if dry_run { 0 } else { prototype_count },
        "foliage_type_assets": if dry_run { json!([]) } else { json!(["Foliage_0", "Foliage_1", "Foliage_2", "Foliage_3", "Foliage_4", "Foliage_5", "Foliage_6", "Foliage_7"]) },
        "foliage_cull_start_cm": 3500, "foliage_cull_end_cm": 7000,
        "material_slots_declared": 2, "has_terrain_surface_material_slot": true,
        "has_groundcover_foliage_material_slot": true,
        "groundcover_material": {"alpha_mode": "masked", "double_sided": true, "shadows": false},
        "wind_packing": manifest["wind_packing"],
        "scatter_binary_instances": 222, "scatter_binary_chunks": 26,
        "scatter_binary_records_validated": true, "scatter_binary_records_read": 222,
        "scatter_binary_file_checksum_xor": fnv_hex_u64(scatter_file_checksum), "scatter_binary_record_checksum_xor": fnv_hex_u64(scatter_record_checksum),
        "scatter_chunks": scatter_chunks,
        "destination_path": destination, "imported_files": import_files, "import_task_count": import_files.len(),
        "import_task_files": import_files, "import_task_destination_paths": import_destinations,
        "import_task_extensions": [".glb", ".json", ".png"], "imported_manifest": true, "imported_preview_tile": true,
        "imported_map_files": expected_import_map_files(manifest),
        "imported_map_file_count": expected_import_map_files(manifest).len(),
        "imported_prototype_lod_files": expected_lod_files(manifest),
        "imported_prototype_lod_file_count": expected_lod_files(manifest).len(),
        "imported_lod0_prototype_files": expected_lod0_files(manifest),
        "imported_lod0_prototype_file_count": expected_lod0_files(manifest).len(),
        "import_tasks": import_tasks,
    });
    package_identity_fields(
        &mut report,
        manifest,
        manifest_checksum,
        source_checksum,
        "unreal_yminus_file",
        false,
    );
    add_common_engine_recipe_fields(&mut report, manifest, false);
    report
}

fn expected_import_map_files(manifest: &Value) -> Vec<String> {
    vec![
        manifest["terrain"]["heightmap_file"]
            .as_str()
            .unwrap()
            .to_string(),
        manifest["terrain"]["masks_file"]
            .as_str()
            .unwrap()
            .to_string(),
        manifest["terrain"]["grass_density_file"]
            .as_str()
            .unwrap()
            .to_string(),
        manifest["normal_conventions"]["unreal_yminus_file"]
            .as_str()
            .unwrap()
            .to_string(),
    ]
}
fn expected_lod_files(manifest: &Value) -> Vec<String> {
    let mut result = manifest["prototypes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|prototype| {
            prototype["lods"]
                .as_array()
                .unwrap()
                .iter()
                .map(|lod| lod["file"].as_str().unwrap().to_string())
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}
fn expected_lod0_files(manifest: &Value) -> Vec<String> {
    let mut result = manifest["prototypes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|prototype| {
            prototype["lods"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|lod| lod["index"] == 0)
                .map(|lod| lod["file"].as_str().unwrap().to_string())
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}
fn unreal_import_files(manifest: &Value) -> Vec<String> {
    let mut files = BTreeSet::new();
    files.insert("midori_nature.json".to_string());
    files.insert("preview_tile.glb".to_string());
    files.extend(expected_import_map_files(manifest));
    files.extend(expected_lod_files(manifest));
    files.into_iter().collect()
}

fn compile_stub_report(
    manifest: &Value,
    validation: &Path,
    manifest_checksum: &str,
    source_checksum: &str,
    package: &Path,
) -> Value {
    let (scatter_file_checksum, scatter_record_checksum) = scatter_checksums(package);
    let import_screenshot = validation.join("screenshots/unity_import.png");
    let density_screenshot = validation.join("screenshots/unity_density.png");
    let import_checksum = fnv_hex(&fs::read(&import_screenshot).unwrap());
    let density_checksum = fnv_hex(&fs::read(&density_screenshot).unwrap());
    let import_bytes = fs::metadata(import_screenshot).unwrap().len();
    let density_bytes = fs::metadata(density_screenshot).unwrap().len();
    let mut report = json!({
        "status": "passed", "schema_version": 3, "asset_name": "Temperate Forest Floor",
        "package_dir": "forest_floor", "manifest_file_checksum": manifest_checksum,
        "source_file_count": 59, "source_file_checksum_xor": source_checksum,
        "material_recipe_count": 2, "prototypesDeclared": 8, "lod0PrototypesDeclared": 8,
        "lodFilesDeclared": 22, "materialSlotsDeclared": 2, "hasTerrainSurfaceMaterialSlot": true,
        "hasGroundcoverFoliageMaterialSlot": true, "groundcoverMaterialAlphaMode": "masked",
        "groundcoverMaterialDoubleSided": true, "groundcoverMaterialShadows": false,
        "terrain_size_x": 16.0, "terrain_size_y": 0.3901228, "terrain_size_z": 16.0,
        "heightmapResolution": 33, "detailResolution": 8, "detailResolutionPerPatch": 8,
        "detailPrototypesCreated": 8, "detailPrototypesLoadedFromAssets": 0,
        "detailPrototypesGeneratedFromGlb": 8, "detailPrototypeFailures": 0,
        "detailPrototypeGeneratedFileCount": 8, "detailPrototypeFallbackErrorCount": 0,
        "nonZeroDetailCells": 512, "importScreenshotExists": true, "importScreenshotBytes": import_bytes,
        "importScreenshotChecksum": import_checksum, "importScreenshotWidth": 1024,
        "importScreenshotHeight": 1024, "importScreenshotPixelCount": 1024 * 1024,
        "importScreenshotMinR": 0.08, "importScreenshotMaxR": 0.72,
        "importScreenshotMinG": 0.10, "importScreenshotMaxG": 0.82,
        "importScreenshotMinB": 0.08, "importScreenshotMaxB": 0.35,
        "densityScreenshotExists": true, "densityScreenshotBytes": density_bytes,
        "densityScreenshotChecksum": density_checksum, "densityScreenshotWidth": 512,
        "densityScreenshotHeight": 512, "densityScreenshotPixelCount": 512 * 512,
        "densityScreenshotMinR": 0.25, "densityScreenshotMaxR": 0.75,
        "densityScreenshotMinG": 0.25, "densityScreenshotMaxG": 0.75,
        "densityScreenshotMinB": 0.3625, "densityScreenshotMaxB": 0.7875,
        "scatter_binary_instances": 222, "scatter_binary_chunks": 26,
        "engine_import_recipe_count": 2,
        "engine_import_recipe_files": ["engines/unity_import.recipe.json", "engines/unreal_import.recipe.json"],
        "surface_overlay_count": 3, "prototype_count": 8, "scatter_chunk_reports": 26,
        "scatter_binary_records_validated": true,
        "scatter_binary_file_checksum_xor": fnv_hex_u64(scatter_file_checksum),
        "scatter_binary_record_checksum_xor": fnv_hex_u64(scatter_record_checksum)
    });
    add_common_engine_recipe_fields(&mut report, manifest, true);
    report
}

fn fake_editor_report(
    manifest_checksum: &str,
    source_checksum: &str,
    package: &Path,
    validation: &Path,
) -> Value {
    let (scatter_file_checksum, scatter_record_checksum) = scatter_checksums(package);
    let import_checksum =
        fnv_hex(&fs::read(validation.join("screenshots/unreal_import.png")).unwrap());
    let foliage_checksum =
        fnv_hex(&fs::read(validation.join("screenshots/unreal_foliage.png")).unwrap());
    let screenshot = |checksum: &str, bytes: u64| json!({"status": "captured", "method": "AutomationLibrary.take_high_res_screenshot", "exists": true, "bytes": bytes, "checksum": checksum, "width": 1024, "height": 1024});
    let metric = |checksum: &str, bytes: u64| json!({"exists": true, "bytes": bytes, "checksum": checksum, "width": 1024, "height": 1024, "sample_count": 1024 * 1024, "luminance_range": 91});
    let import_bytes = fs::metadata(validation.join("screenshots/unreal_import.png"))
        .unwrap()
        .len();
    let foliage_bytes = fs::metadata(validation.join("screenshots/unreal_foliage.png"))
        .unwrap()
        .len();
    json!({
        "status": "passed", "dry_run": false, "destination_path": "/Game/Midori/FakeEditor/Temperate_Forest_Floor",
        "source_file_count": 59, "manifest_file_checksum": manifest_checksum,
        "source_file_checksum_xor": source_checksum, "import_task_count": 28,
        "imported_map_file_count": 4, "imported_prototype_lod_file_count": 22,
        "imported_lod0_prototype_file_count": 8, "foliage_type_expected_count": 8,
        "foliage_type_count": 8, "foliage_type_status": "created", "foliage_cull_start_cm": 3500,
        "foliage_cull_end_cm": 7000, "scatter_binary_chunks": 26, "scatter_binary_instances": 222,
        "scatter_binary_records_validated": true, "scatter_binary_records_read": 222,
        "scatter_binary_file_checksum_xor": fnv_hex_u64(scatter_file_checksum),
        "scatter_binary_record_checksum_xor": fnv_hex_u64(scatter_record_checksum),
        "corrupt_scatter_rejection": "passed",
        "screenshots": {"import": screenshot(&import_checksum, import_bytes), "foliage_settings": screenshot(&foliage_checksum, foliage_bytes)},
        "screenshot_metrics": {"import": metric(&import_checksum, import_bytes), "foliage_settings": metric(&foliage_checksum, foliage_bytes)}
    })
}

fn summary(
    manifest: &Value,
    validation: &Path,
    manifest_checksum: &str,
    unity_source_checksum: &str,
    unreal_source_checksum: &str,
    package: &Path,
) -> Value {
    let (scatter_file_checksum, scatter_record_checksum) = scatter_checksums(package);
    let import_screenshot = validation.join("screenshots/unreal_import.png");
    let foliage_screenshot = validation.join("screenshots/unreal_foliage.png");
    let import_checksum = fnv_hex(&fs::read(&import_screenshot).unwrap());
    let foliage_checksum = fnv_hex(&fs::read(&foliage_screenshot).unwrap());
    let import_bytes = fs::metadata(import_screenshot).unwrap().len();
    let foliage_bytes = fs::metadata(foliage_screenshot).unwrap().len();
    let compile_path = validation.join("forest_floor_unity_compile_stub_report.json");
    let fake_path = validation.join("forest_floor_unreal_fake_editor_report.json");
    let summary = json!({
        "project_scaffolds": {
            "status": "generated", "preflight_status": "passed",
            "root": validation.join("projects").to_string_lossy(),
            "unity_project": validation.join("projects/unity/MidoriUnityValidation").to_string_lossy(),
            "unreal_project": validation.join("projects/unreal/MidoriUnrealValidation/MidoriUnrealValidation.uproject").to_string_lossy()
        },
        "midori": {
            "status": "passed", "map_summaries": 5, "map_relationships_status": "passed",
            "prototype_summaries": 22, "profile_budget_summaries": 2,
            "profile_budget_status": "passed", "profile_budget_failure_count": 0,
            "scatter_json_chunks": 26, "scatter_binary_summaries": 26,
            "scatter_parity_status": "passed", "scatter_record_checksum_mismatches": 0
        },
        "unity_preflight": {
            "status": "passed", "compile_stub_status": "passed",
            "compile_stub_execution_status": "passed", "compile_stub_source_file_count": 59,
            "compile_stub_manifest_file_checksum": manifest_checksum,
            "compile_stub_source_file_checksum_xor": unity_source_checksum,
            "compile_stub_material_recipe_count": 2, "compile_stub_engine_import_recipe_count": 2,
            "compile_stub_scatter_binary_chunks": 26, "compile_stub_scatter_binary_instances": 222,
            "compile_stub_scatter_binary_records_validated": true,
            "compile_stub_execution_report": compile_path.to_string_lossy(), "scatter_binary_instances": 222
        },
        "profile_notes_preflight": {"status": "passed"},
        "unreal": {
            "dry_run_status": "passed", "scatter_binary_instances": 222,
            "scatter_binary_records_validated": true, "scatter_binary_records_read": 222,
            "scatter_binary_file_checksum_xor": fnv_hex_u64(scatter_file_checksum),
            "scatter_binary_record_checksum_xor": fnv_hex_u64(scatter_record_checksum),
            "scatter_chunk_reports": 26, "manifest_file_checksum": manifest_checksum,
            "source_file_checksum_xor": unreal_source_checksum
        },
        "unreal_fake_editor": {
            "status": "passed", "report": fake_path.to_string_lossy(),
            "import_screenshot_status": "captured", "import_screenshot_method": "AutomationLibrary.take_high_res_screenshot",
            "import_screenshot_report_exists": true, "import_screenshot_report_bytes": import_bytes,
            "import_screenshot_report_checksum": import_checksum, "import_screenshot_report_width": 1024,
            "import_screenshot_report_height": 1024, "import_screenshot_checksum": import_checksum,
            "import_screenshot_luminance_range": 91,
            "foliage_settings_screenshot_status": "captured", "foliage_settings_screenshot_method": "AutomationLibrary.take_high_res_screenshot",
            "foliage_settings_screenshot_report_exists": true, "foliage_settings_screenshot_report_bytes": foliage_bytes,
            "foliage_settings_screenshot_report_checksum": foliage_checksum, "foliage_settings_screenshot_report_width": 1024,
            "foliage_settings_screenshot_report_height": 1024, "foliage_settings_screenshot_checksum": foliage_checksum,
            "foliage_settings_screenshot_luminance_range": 91
        }
    });
    let _ = manifest;
    summary
}

fn write_scaffolds(validation: &Path) {
    let root = validation.join("projects");
    let unity = root.join("unity/MidoriUnityValidation");
    let unreal = root.join("unreal/MidoriUnrealValidation");
    write_bytes(
        &unity.join("Assets/Editor/MidoriNaturePackageImporter.cs"),
        b"synthetic importer",
    );
    fs::create_dir_all(unity.join("ProjectSettings")).unwrap();
    write_bytes(
        &unity.join("Assets/Midori/Validation/README.md"),
        b"synthetic Unity scaffold",
    );
    write_json(
        &unity.join("Packages/manifest.json"),
        &json!({"dependencies": {
            "com.unity.cloud.gltfast": "5.2.0",
            "com.unity.modules.imgui": "1.0.0",
            "com.unity.modules.jsonserialize": "1.0.0",
            "com.unity.modules.terrain": "1.0.0",
            "com.unity.modules.uielements": "1.0.0"
        }}),
    );
    write_json(
        &unreal.join("MidoriUnrealValidation.uproject"),
        &json!({"Plugins": [
            {"Name": "PythonScriptPlugin", "Enabled": true},
            {"Name": "EditorScriptingUtilities", "Enabled": true}
        ]}),
    );
    write_bytes(
        &unreal.join("Config/DefaultEngine.ini"),
        b"[/Script/Plugins.PythonScriptPluginSettings]\nbDeveloperMode=True\n",
    );
    write_bytes(
        &unreal.join("Content/Midori/Validation/README.md"),
        b"synthetic Unreal scaffold",
    );
    write_json(
        &root.join("project_scaffold_summary.json"),
        &json!({"unity": {"gltfast_version": "5.2.0"}, "unreal": {"destination_root": "/Game/Midori/Imported"}}),
    );
}

fn write_png(path: &Path, width: u32, height: u32) {
    let mut data = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for y in 0..height {
        for x in 0..width {
            data.push((x % 256) as u8);
            data.push((y % 256) as u8);
            data.push(((x + y) % 256) as u8);
        }
    }
    write_png_data(path, width, height, &data);
}

fn write_png_constant(path: &Path, width: u32, height: u32, value: u8) {
    write_png_data(
        path,
        width,
        height,
        &vec![value; width as usize * height as usize * 3],
    );
}

fn write_png_data(path: &Path, width: u32, height: u32, data: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
}

fn png_with_bad_filter() -> Vec<u8> {
    // A small valid PNG whose first scanline advertises unsupported filter 5.
    // The verifier reaches filter validation after passing signature/IHDR/IDAT.
    let width = 256u32;
    let height = 256u32;
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for y in 0..height {
        raw.push(5);
        for x in 0..width {
            raw.extend_from_slice(&[
                (x.wrapping_mul(11).wrapping_add(y.wrapping_mul(3))) as u8,
                (x.wrapping_mul(7).wrapping_add(y.wrapping_mul(13))) as u8,
                (x.wrapping_mul(5).wrapping_add(y.wrapping_mul(17))) as u8,
            ]);
        }
    }
    let mut compressed = Vec::new();
    {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(&mut compressed, Compression::default());
        encoder.write_all(&raw).unwrap();
        encoder.finish().unwrap();
    }
    let mut output = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    append_png_chunk(&mut output, b"IHDR", &header);
    append_png_chunk(&mut output, b"IDAT", &compressed);
    append_png_chunk(&mut output, b"IEND", &[]);
    output
}

fn png_with_bad_decompression() -> Vec<u8> {
    // A structurally valid PNG with a deliberately truncated zlib stream.
    let mut output = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend(256u32.to_be_bytes());
    header.extend(256u32.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    append_png_chunk(&mut output, b"IHDR", &header);
    append_png_chunk(&mut output, b"IDAT", &[0x78, 0x9c, 0x00]);
    append_png_chunk(&mut output, b"IEND", &[]);
    output
}

fn png_with_incomplete_zlib() -> Vec<u8> {
    let width = 256u32;
    let height = 256u32;
    let mut raw = Vec::with_capacity((width as usize * 3 + 1) * height as usize);
    for y in 0..height {
        raw.push(0);
        for x in 0..width {
            raw.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
        }
    }
    let mut compressed = Vec::new();
    {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;
        let mut encoder = ZlibEncoder::new(&mut compressed, Compression::default());
        encoder.write_all(&raw).unwrap();
        encoder.finish().unwrap();
    }
    compressed.truncate(compressed.len().saturating_sub(4));
    let mut output = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    append_png_chunk(&mut output, b"IHDR", &header);
    append_png_chunk(&mut output, b"IDAT", &compressed);
    append_png_chunk(&mut output, b"IEND", &[]);
    output
}

fn append_png_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend((data.len() as u32).to_be_bytes());
    output.extend(kind);
    output.extend(data);
    // The native verifier intentionally preserves the legacy parser's lack of
    // CRC validation; zero is sufficient for this synthetic fixture.
    output.extend([0u8; 4]);
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn profile_notes() -> String {
    let paragraph = "Synthetic profile evidence records Unity Editor version and Unreal Editor version observations for the mobile and console profiles. LOD distances, Frame Debugger and RenderDoc observations cover instanced rendering, scale, density, cull distances, material slot bindings, and wind channels. The strict verifier consumes forest_floor_unity_import_report.json and forest_floor_unreal_editor_report.json together with unity_forest_floor_import.png, unity_forest_floor_density.png, unreal_forest_floor_import.png, and unreal_forest_floor_foliage_settings.png. Unity fields detailPrototypesCreated, detailPrototypesGeneratedFromGlb, detailPrototypeFailures, detailPrototypeFallbackErrors, and scatterBinaryRecordsRead are recorded; scatterChunkReports and Unreal foliage_type_count and foliage_type_assets are included with 3500 and 7000 cull bounds. This is a synthetic test fixture and does not claim that an editor was executed.\n";
    format!(
        "## Evidence Artifacts\n\n{paragraph}\n## Unity\n\n{paragraph}\n## Unreal\n\n{paragraph}\n## Verdict\n\n{paragraph}{paragraph}"
    )
}
