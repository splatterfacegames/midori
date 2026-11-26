//! Midori - Procedural Nature Generator CLI
//!
//! Command-line interface for the Midori procedural nature generator.
//! Generates 3D tree meshes from species TOML files and nature packages
//! from NaturePatch TOML files.
//!
//! ## Usage
//!
//! ```bash
//! midori generate -s species/oak.toml -o forest/oak.glb
//! midori generate -s species/pine.toml -n 10 --seed 42
//! midori nature -p presets/nature/temperate_forest_floor.toml -o forest_floor_midori
//! midori info -s species/oak.toml
//! ```
//!
//! ## Exit codes
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | success |
//! | 2    | usage error (bad/missing arguments — clap parse failures) |
//! | 3    | input error (species/patch parse or validation, generation budget) |
//! | 4    | IO error (file read/write failures) |
//! | 1    | other internal errors |
//!
//! When `--seed` is combined with `-n`, variant `i` is generated with
//! `seed + i`, so a run of `-n 10 --seed 42` covers seeds 42 through 51.

use clap::{Parser, Subcommand, ValueEnum};
use midori_core::{
    ExportConfig, ExportFormat, ExportMetadata, FoliageRenderMode, GenerationBudget,
    GenerationError, LodGenerationConfig, NaturePackageConfig, NaturePatch, NaturePatchError,
    NatureStand, Species, SpeciesError, StandBudget, StandError, StandExportOptions, StandRaster,
    StandTileInputs, TextureSet, estimate_tree, export_lod_meshes, export_mesh,
    generate_lod_meshes_with_foliage_mode, generate_stand_tiles, generate_tree_with_budget,
    validate_nature_package, validate_stand_package,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Midori - Procedural Nature Generator
#[derive(Parser)]
#[command(name = "midori")]
#[command(author, version, about = "Procedural Nature Generator", long_about = None)]
struct Cli {
    /// Suppress progress output (errors still go to stderr)
    #[arg(long, global = true)]
    quiet: bool,

    /// Emit machine-readable JSON results and errors for scripted pipelines
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate tree mesh(es) from species definition
    Generate {
        /// Species TOML file
        #[arg(short, long)]
        species: PathBuf,

        /// Named state from the species file's `[states.<name>]` overlays
        /// (seasonal or condition variants, e.g. `flowering`)
        #[arg(long)]
        state: Option<String>,

        /// Output path (default: tree.glb)
        #[arg(short, long, default_value = "tree.glb")]
        output: PathBuf,

        /// Number of tree variants to generate (variant i uses seed + i)
        #[arg(short = 'n', long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
        count: u32,

        /// Random seed (default: random)
        #[arg(long)]
        seed: Option<u64>,

        /// LOD level(s) to export
        #[arg(long, default_value = "all")]
        lod: LodOption,

        /// Output format
        #[arg(long, default_value = "glb")]
        format: OutputFormat,

        /// LOD preset to use
        #[arg(long, default_value = "balanced")]
        lod_preset: LodPreset,

        /// Foliage rendering mode: alpha-cutout leaf cards (`cutout`), closed
        /// opaque low-poly leaf-cluster geometry (`opaque`, zero alpha), or 2026/2027
        /// 100% opaque 3D individual geometric leaves/twigs/blossoms (`geo3d`, zero alpha)
        #[arg(long, default_value = "cutout")]
        foliage: FoliageModeArg,

        /// Embed material maps (procedural or file-slot overrides) into the export
        #[arg(long)]
        textures: bool,

        /// Override the default stem budget (0 = unbounded)
        #[arg(long)]
        max_stems: Option<u32>,

        /// Override the default leaf budget (0 = unbounded)
        #[arg(long)]
        max_leaves: Option<u32>,

        /// Override the default total vertex budget across LODs (0 = unbounded)
        #[arg(long)]
        max_vertices: Option<u32>,

        /// Override the default total triangle budget across LODs (0 = unbounded)
        #[arg(long)]
        max_triangles: Option<u32>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Write the species' material maps (bark albedo, bark normal, leaf card) as PNGs
    Maps {
        /// Species TOML file
        #[arg(short, long)]
        species: PathBuf,

        /// Named state from the species file's `[states.<name>]` overlays
        /// (seasonal or condition variants, e.g. `flowering`)
        #[arg(long)]
        state: Option<String>,

        /// Output directory (default: ./maps)
        #[arg(short, long, default_value = "maps")]
        output: PathBuf,

        /// Filename prefix (default: species name in snake_case)
        #[arg(long)]
        prefix: Option<String>,
    },

    /// Show information about a species file
    Info {
        /// Species TOML file
        #[arg(short, long)]
        species: PathBuf,

        /// Named state from the species file's `[states.<name>]` overlays
        /// (seasonal or condition variants, e.g. `flowering`)
        #[arg(long)]
        state: Option<String>,
    },

    /// Generate a nature package from a NaturePatch definition
    Nature {
        /// NaturePatch TOML file
        #[arg(short, long)]
        patch: PathBuf,

        /// Output package directory
        #[arg(short, long, default_value = "nature_midori")]
        output: PathBuf,

        /// Baked map resolution in pixels
        #[arg(long, default_value_t = 64)]
        map_resolution: u32,

        /// Preview terrain mesh resolution
        #[arg(long, default_value_t = 32)]
        preview_resolution: u32,

        /// Scatter chunk size in world units
        #[arg(long, default_value_t = 8.0)]
        scatter_chunk_size: f32,

        /// Skip preview_tile.glb output
        #[arg(long)]
        no_preview: bool,

        /// Skip prototype GLB outputs
        #[arg(long)]
        no_prototypes: bool,

        /// Skip all scatter outputs
        #[arg(long)]
        no_scatter: bool,

        /// Skip binary scatter buffer output while keeping JSON scatter
        #[arg(long)]
        no_scatter_binary: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Validate an exported Midori nature package
    ValidateNature {
        /// Nature package directory containing midori_nature.json
        #[arg(short, long)]
        input: PathBuf,

        /// Optional JSON validation report path for engine import preflight
        #[arg(long)]
        report: Option<PathBuf>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Generate a volumetric nature stand (plant library + seamless tile packages)
    Stand {
        /// NatureStand TOML preset (`[asset] kind = "nature_stand"`)
        #[arg(short, long)]
        preset: PathBuf,

        /// Output stand package directory
        #[arg(short, long, default_value = "stand_midori")]
        output: PathBuf,

        /// Square tile side length in metres
        #[arg(long, default_value_t = 128.0)]
        tile_size: f32,

        /// Single world tile coordinate `x,z` (may be negative, e.g. `12,-7`)
        #[arg(long, allow_hyphen_values = true, default_value = "0,0")]
        tile: String,

        /// Inclusive tile coordinate box `x0,z0,x1,z1` (overrides `--tile`)
        #[arg(long, allow_hyphen_values = true)]
        region: Option<String>,

        /// Optional tile-space greyscale PNG mask (0 = no stand, 255 = full stand)
        #[arg(long)]
        mask: Option<PathBuf>,

        /// Optional tile-space hard-exclusion PNG (roads, paths, water, buildings)
        #[arg(long)]
        exclude: Option<PathBuf>,

        /// Optional tile-space greyscale heightmap PNG
        #[arg(long)]
        height: Option<PathBuf>,

        /// World elevation (metres) corresponding to heightmap pixel value 0
        #[arg(long, allow_hyphen_values = true, default_value_t = 0.0)]
        height_min: f32,

        /// World elevation (metres) corresponding to heightmap pixel value 1
        #[arg(long, allow_hyphen_values = true, default_value_t = 32.0)]
        height_max: f32,

        /// Per-stratum density multipliers, e.g. `canopy=1.2,understory=0.8`
        #[arg(long)]
        density: Option<String>,

        /// Maximum individual plant instances per tile (remainder merged into Stand LOD 0)
        #[arg(long, default_value_t = 300)]
        max_instances_per_tile: usize,

        /// Maximum total triangles across Stand LOD 0 stratum meshes per tile
        #[arg(long, default_value_t = 40_000)]
        max_triangles_lod0: u32,

        /// Maximum triangles for Stand LOD 1 canopy mesh per tile
        #[arg(long, default_value_t = 4_000)]
        max_triangles_lod1: u32,

        /// Maximum triangles for Stand LOD 2 far canopy mesh per tile
        #[arg(long, default_value_t = 200)]
        max_triangles_lod2: u32,

        /// Foliage rendering mode (`cutout` or `opaque`)
        #[arg(long, default_value = "cutout")]
        foliage: FoliageModeArg,

        /// Tile raster resolution in pixels (for canopy_height/cover/density/color PNGs)
        #[arg(long, default_value_t = 64)]
        raster_resolution: u32,

        /// Skip writing `library/*.glb` files (useful when reusing an existing library)
        #[arg(long)]
        skip_library: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Validate an exported volumetric nature stand package
    ValidateStand {
        /// Stand package directory containing midori_stand.json
        #[arg(short, long)]
        input: PathBuf,

        /// Optional JSON validation report path
        #[arg(long)]
        report: Option<PathBuf>,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Verify profile-note completeness only; not full engine evidence
    VerifyProfileNotes {
        #[arg(
            long,
            default_value = "docs/validation/midori-nature-engine-profile-notes.md"
        )]
        profile_notes: PathBuf,
        /// Optional profile-only JSON report path
        #[arg(long)]
        output: Option<PathBuf>,
        /// Permit missing notes, never failed checks
        #[arg(long)]
        allow_pending: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum LodOption {
    /// Export all LOD levels
    All,
    /// Export only LOD 0 (highest quality)
    #[value(name = "0")]
    Lod0,
    /// Export only LOD 1
    #[value(name = "1")]
    Lod1,
    /// Export only LOD 2
    #[value(name = "2")]
    Lod2,
    /// Export only LOD 3
    #[value(name = "3")]
    Lod3,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat {
    /// Binary glTF (.glb)
    Glb,
    /// JSON glTF with separate binary (.gltf + .bin)
    Gltf,
}

#[derive(Clone, Copy, ValueEnum)]
enum LodPreset {
    /// Ultra quality (5 LOD levels)
    Ultra,
    /// High quality (4 LOD levels)
    #[value(name = "high_quality")]
    HighQuality,
    /// Balanced quality (3 LOD levels)
    Balanced,
    /// Mobile optimized (3 LOD levels)
    Mobile,
    /// Minimal (2 LOD levels)
    Minimal,
    /// 2-LOD Nanite/meshlet-ready opaque 3D flora (Hero3D 25-55k + Mid3D 6-14k tris)
    Nanite,
    /// Use the species file's own `[lod]` section (preset or custom levels)
    Species,
}

#[derive(Clone, Copy, ValueEnum)]
enum FoliageModeArg {
    /// Alpha-cutout leaf cards (default)
    Cutout,
    /// Closed low-poly opaque leaf-cluster volumes (zero alpha)
    Opaque,
    /// 100% opaque 3D individual geometric leaves, twigs & blossoms (zero alpha)
    Geo3d,
}

impl From<FoliageModeArg> for FoliageRenderMode {
    fn from(value: FoliageModeArg) -> Self {
        match value {
            FoliageModeArg::Cutout => FoliageRenderMode::Cutout,
            FoliageModeArg::Opaque => FoliageRenderMode::Opaque,
            FoliageModeArg::Geo3d => FoliageRenderMode::Geo3d,
        }
    }
}

fn exit_code(err: &(dyn std::error::Error + 'static)) -> i32 {
    // 3: input — invalid user documents and resource-limit rejections.
    // 4: IO.  1: anything else. (2 is clap's usage-error exit before main runs.)
    if let Some(e) = err.downcast_ref::<SpeciesError>() {
        return match e {
            SpeciesError::Io(_) => 4,
            SpeciesError::Parse(_) | SpeciesError::Validation(_) => 3,
        };
    }
    if let Some(e) = err.downcast_ref::<NaturePatchError>() {
        return match e {
            NaturePatchError::Io(_) => 4,
            _ => 3,
        };
    }
    if let Some(e) = err.downcast_ref::<StandError>() {
        return match e {
            StandError::Io(_) => 4,
            _ => 3,
        };
    }
    if err.downcast_ref::<GenerationError>().is_some() {
        return 3;
    }
    if err.downcast_ref::<std::io::Error>().is_some() {
        return 4;
    }
    // A wrapped cause may carry the real kind (e.g. io::Error boxed).
    match err.source() {
        Some(source) => exit_code(source),
        None => 1,
    }
}

fn main() {
    let cli = Cli::parse();
    let quiet = cli.quiet;
    let json = cli.json;

    let result = match cli.command {
        Commands::Generate {
            species,
            state,
            output,
            count,
            seed,
            lod,
            format,
            lod_preset,
            foliage,
            textures,
            max_stems,
            max_leaves,
            max_vertices,
            max_triangles,
            verbose,
        } => run_generate(&GenerateOptions {
            species_path: &species,
            state: state.as_deref(),
            output_path: &output,
            count,
            seed,
            lod,
            format,
            lod_preset,
            foliage: foliage.into(),
            textures,
            budget: budget_arg(max_stems, max_leaves, max_vertices, max_triangles),
            quiet,
            json,
            verbose,
        }),
        Commands::Maps {
            species,
            state,
            output,
            prefix,
        } => run_maps(&species, state.as_deref(), &output, prefix.as_deref()),
        Commands::Info { species, state } => run_info(&species, state.as_deref()),
        Commands::Nature {
            patch,
            output,
            map_resolution,
            preview_resolution,
            scatter_chunk_size,
            no_preview,
            no_prototypes,
            no_scatter,
            no_scatter_binary,
            verbose,
        } => run_nature(&NatureOptions {
            patch_path: &patch,
            output_path: &output,
            map_resolution,
            preview_resolution,
            scatter_chunk_size,
            no_preview,
            no_prototypes,
            no_scatter,
            no_scatter_binary,
            verbose,
        }),
        Commands::ValidateNature {
            input,
            report,
            verbose,
        } => run_validate_nature(&input, report.as_ref(), verbose),
        Commands::Stand {
            preset,
            output,
            tile_size,
            tile,
            region,
            mask,
            exclude,
            height,
            height_min,
            height_max,
            density,
            max_instances_per_tile,
            max_triangles_lod0,
            max_triangles_lod1,
            max_triangles_lod2,
            foliage,
            raster_resolution,
            skip_library,
            verbose,
        } => run_stand(&StandCliOptions {
            preset_path: &preset,
            output_path: &output,
            tile_size,
            tile: &tile,
            region: region.as_deref(),
            mask: mask.as_deref(),
            exclude: exclude.as_deref(),
            height: height.as_deref(),
            height_min,
            height_max,
            density: density.as_deref(),
            budget: StandBudget {
                max_instances_per_tile,
                max_triangles_lod0,
                max_triangles_lod1,
                max_triangles_lod2,
                max_materials: 4,
            },
            foliage: foliage.into(),
            raster_resolution,
            skip_library,
            quiet,
            json,
            verbose,
        }),
        Commands::ValidateStand {
            input,
            report,
            verbose,
        } => run_validate_stand(&input, report.as_deref(), quiet, json, verbose),
        Commands::VerifyProfileNotes {
            profile_notes,
            output,
            allow_pending,
        } => match run_verify_profile_notes(&profile_notes, output.as_deref(), allow_pending) {
            Ok(code) => std::process::exit(code),
            Err(error) => Err(error),
        },
    };

    if let Err(e) = result {
        if json {
            eprintln!(
                "{}",
                serde_json::json!({"error": {"kind": error_kind(&*e), "message": e.to_string()}})
            );
        } else {
            eprintln!("Error: {e}");
        }
        std::process::exit(exit_code(&*e));
    }
}

// Budget flags: absent = documented default, 0 = unbounded.
fn budget_arg(
    stems: Option<u32>,
    leaves: Option<u32>,
    vertices: Option<u32>,
    triangles: Option<u32>,
) -> GenerationBudget {
    let defaults = GenerationBudget::default();
    let unbounded = |v: Option<u32>, default: u32| match v {
        None => default,
        Some(0) => u32::MAX,
        Some(v) => v,
    };
    GenerationBudget {
        max_stems: unbounded(stems, defaults.max_stems),
        max_leaves: unbounded(leaves, defaults.max_leaves),
        max_vertices: unbounded(vertices, defaults.max_vertices),
        max_triangles: unbounded(triangles, defaults.max_triangles),
    }
}

fn error_kind(err: &(dyn std::error::Error + 'static)) -> &'static str {
    if err.downcast_ref::<SpeciesError>().is_some()
        || err.downcast_ref::<NaturePatchError>().is_some()
        || err.downcast_ref::<StandError>().is_some()
    {
        "input"
    } else if err.downcast_ref::<GenerationError>().is_some() {
        "generation"
    } else if err.downcast_ref::<std::io::Error>().is_some() {
        "io"
    } else {
        "internal"
    }
}

struct GenerateOptions<'a> {
    species_path: &'a Path,
    state: Option<&'a str>,
    output_path: &'a Path,
    count: u32,
    seed: Option<u64>,
    lod: LodOption,
    format: OutputFormat,
    lod_preset: LodPreset,
    foliage: FoliageRenderMode,
    textures: bool,
    budget: GenerationBudget,
    quiet: bool,
    json: bool,
    verbose: bool,
}

fn run_generate(options: &GenerateOptions) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let species_path = options.species_path;
    let output_path = options.output_path;
    let count = options.count;
    let seed = options.seed;
    let verbose = options.verbose && !options.quiet;
    let mut outputs: Vec<serde_json::Value> = Vec::new();

    // Load species
    if verbose {
        println!("Loading species from {species_path:?}...");
    }
    let species = Species::from_file_with_state(species_path, options.state)?;
    if !options.quiet {
        println!(
            "Species: {} ({})",
            species.species.name,
            species.latin_name()
        );
    }

    // Cheap pre-flight: refuse species whose worst-case shape exceeds the
    // budget before spending any time generating.
    let estimate = estimate_tree(&species);
    if let Err(e) = options.budget.check_estimate(&estimate) {
        eprintln!("{e}");
        eprintln!("Raise a limit with --max-stems/--max-leaves (0 = unbounded).");
        return Err(e.into());
    }

    // Get LOD config
    let lod_config = match options.lod_preset {
        LodPreset::Ultra => LodGenerationConfig::ultra(),
        LodPreset::HighQuality => LodGenerationConfig::high_quality(),
        LodPreset::Balanced => LodGenerationConfig::balanced(),
        LodPreset::Mobile => LodGenerationConfig::mobile(),
        LodPreset::Minimal => LodGenerationConfig::minimal(),
        LodPreset::Nanite => LodGenerationConfig::nanite_2lod(),
        LodPreset::Species => LodGenerationConfig::from_species(&species),
    };

    // Resolve material maps when embedding: file-slot paths in the species
    // resolve relative to the species file's directory; empty slots generate
    // procedurally.
    let textures = if options.textures {
        let dir = species_path.parent().unwrap_or_else(|| Path::new("."));
        Some(TextureSet::resolve(&species, dir)?)
    } else {
        None
    };

    // Get export config
    let export_config = ExportConfig {
        format: match options.format {
            OutputFormat::Glb => ExportFormat::Glb,
            OutputFormat::Gltf => ExportFormat::GlTf,
        },
        draco: false,
        textures,
        pivot_painter_extras: true,
        leaf_roughness: species.canopy.roughness,
        organ_roughness: species.inflorescence.as_ref().and_then(|i| i.roughness),
        opaque_foliage: options.foliage.is_opaque(),
        double_sided_opaque_foliage: options.foliage == FoliageRenderMode::Geo3d,
        metadata: None,
    };

    // Generate trees
    for i in 0..count {
        let tree_seed = seed.unwrap_or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0)
                + i as u64
        }) + i as u64;

        if verbose {
            println!("Generating tree {} with seed {}...", i + 1, tree_seed);
        }

        let gen_start = Instant::now();
        let tree = generate_tree_with_budget(&species, tree_seed, &options.budget)?;

        if verbose {
            println!("  Tree generated in {:?}", gen_start.elapsed());
            println!(
                "  Stems: {}, Leaves: {}",
                tree.stems.len(),
                tree.leaves.len()
            );
        }

        // Determine output path
        let tree_output = if count > 1 {
            let stem = output_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("tree");
            let ext = output_path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("glb");
            output_path.with_file_name(format!("{stem}_{i}.{ext}"))
        } else {
            output_path.to_path_buf()
        };

        // Generate LOD meshes
        let mesh_start = Instant::now();
        let lod_meshes =
            generate_lod_meshes_with_foliage_mode(&tree, &species, &lod_config, options.foliage);
        let lod_stats = lod_meshes.total_stats();
        options
            .budget
            .check_mesh_stats(
                lod_stats.vertex_count as usize,
                lod_stats.triangle_count as usize,
            )
            .map_err(|e| {
                eprintln!("{e}");
                eprintln!("Raise a limit with --max-vertices/--max-triangles (0 = unbounded).");
                e
            })?;
        outputs.push(serde_json::json!({
            "output": tree_output,
            "seed": tree_seed,
            "stems": tree.stems.len(),
            "leaves": tree.leaves.len(),
            "lods": lod_meshes.meshes.iter().map(|m| serde_json::json!({
                "name": m.name,
                "vertices": m.stats.vertex_count,
                "triangles": m.stats.triangle_count,
            })).collect::<Vec<_>>(),
        }));

        if verbose {
            println!("  LOD meshes generated in {:?}", mesh_start.elapsed());
            for lod_mesh in &lod_meshes.meshes {
                println!(
                    "    {}: {} vertices, {} triangles",
                    lod_mesh.name, lod_mesh.stats.vertex_count, lod_mesh.stats.triangle_count
                );
            }
        }

        let mut tree_export_config = export_config.clone();
        tree_export_config.metadata = Some(ExportMetadata {
            species_name: species.species.name.clone(),
            scientific_name: species.latin_name().to_string(),
            seed: Some(tree_seed),
            lod_screen_heights: lod_meshes
                .meshes
                .iter()
                .map(|lod_mesh| lod_mesh.screen_height)
                .collect(),
        });

        // Export based on LOD option
        let export_start = Instant::now();
        match options.lod {
            LodOption::All => {
                export_lod_meshes(&lod_meshes, &tree_output, &tree_export_config)?;
            }
            LodOption::Lod0 | LodOption::Lod1 | LodOption::Lod2 | LodOption::Lod3 => {
                let level = match options.lod {
                    LodOption::Lod0 => 0,
                    LodOption::Lod1 => 1,
                    LodOption::Lod2 => 2,
                    LodOption::Lod3 => 3,
                    _ => 0,
                };
                if let Some(lod_mesh) = lod_meshes.get(level) {
                    export_mesh(&lod_mesh.mesh, &tree_output, &tree_export_config)?;
                } else {
                    return Err(format!("LOD level {level} not available").into());
                }
            }
        }

        if verbose {
            println!("  Exported in {:?}", export_start.elapsed());
        }

        if !options.quiet {
            println!("Exported: {tree_output:?}");
        }
    }

    let elapsed = start.elapsed();
    if options.json {
        println!(
            "{}",
            serde_json::json!({
                "command": "generate",
                "species": species.species.name,
                "count": count,
                "elapsed_ms": elapsed.as_millis() as u64,
                "estimate": {
                    "max_stems": estimate.max_stems,
                    "max_leaves": estimate.max_leaves,
                },
                "outputs": outputs,
            })
        );
    } else if !options.quiet {
        println!("Generated {count} tree(s) in {elapsed:?}");
    }

    Ok(())
}

fn run_maps(
    species_path: &Path,
    state: Option<&str>,
    output_dir: &Path,
    prefix: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let species = Species::from_file_with_state(species_path, state)?;
    println!(
        "Species: {} ({})",
        species.species.name, species.species.scientific
    );

    let dir = species_path.parent().unwrap_or_else(|| Path::new("."));
    let textures = TextureSet::resolve(&species, dir)?;

    std::fs::create_dir_all(output_dir)?;
    let prefix = prefix
        .map(|p| p.to_string())
        .unwrap_or_else(|| species.species.name.to_lowercase().replace(' ', "_"));

    for (name, tex) in [
        ("bark_albedo", &textures.bark_albedo),
        ("bark_normal", &textures.bark_normal),
        ("leaf_card", &textures.leaf_card),
    ] {
        let path = output_dir.join(format!("{prefix}_{name}.png"));
        std::fs::write(&path, tex.to_png()?)?;
        println!("Wrote {:?} ({}x{})", path, tex.width, tex.height);
    }
    for (name, tex) in [
        ("upper_bark_albedo", textures.upper_bark_albedo.as_ref()),
        ("upper_bark_normal", textures.upper_bark_normal.as_ref()),
        ("organ_albedo", textures.organ_albedo.as_ref()),
    ] {
        if let Some(tex) = tex {
            let path = output_dir.join(format!("{prefix}_{name}.png"));
            std::fs::write(&path, tex.to_png()?)?;
            println!("Wrote {:?} ({}x{})", path, tex.width, tex.height);
        }
    }

    Ok(())
}

fn run_info(species_path: &Path, state: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let species = Species::from_file_with_state(species_path, state)?;

    println!(
        "Species: {} ({})",
        species.species.name,
        species.latin_name()
    );
    println!("  Generator: {:?}", species.generator.family);
    if !species.species.biome.is_empty() {
        println!("  Biome: {}", species.species.biome);
    }
    if !species.species.tags.is_empty() {
        println!("  Tags: {}", species.species.tags.join(", "));
    }
    if !species.materials.bark.is_empty() || !species.materials.foliage.is_empty() {
        println!("  Materials:");
        if !species.materials.bark.is_empty() {
            println!("    Bark: {}", species.materials.bark);
        }
        if !species.materials.foliage.is_empty() {
            println!("    Foliage: {}", species.materials.foliage);
        }
    }
    println!();
    println!("Trunk:");
    println!(
        "  Height: {:.2}m (+/-{:.0}%)",
        species.trunk.height,
        species.trunk.height_variance * 100.0
    );
    println!("  Radius: {:.3}m", species.trunk.radius);
    println!("  Taper: {:.0}%", species.trunk.taper * 100.0);
    println!("  Segments: {}", species.trunk.segments);
    if species.trunk.char_height > 0.0 {
        println!(
            "  Charred base: {:.2}m ({:.0}%)",
            species.trunk.char_height,
            species.trunk.char_strength * 100.0
        );
    }
    println!();

    println!("Branches:");
    for level in 1..=3 {
        if let Some(params) = species.get_branch_level(level) {
            println!("  Level {level}:");
            println!("    Count: {} (+/-{})", params.count, params.count_variance);
            println!(
                "    Length: {:.2}m (+/-{:.0}%)",
                params.length,
                params.length_variance * 100.0
            );
            println!(
                "    Angle: {:.0} deg (+/-{:.0} deg)",
                params.angle, params.angle_variance
            );
        }
    }
    println!();

    println!("Crown:");
    println!("  Shape: {:?}", species.crown.shape);
    println!("  Offset: {:.0}%", species.crown.offset * 100.0);
    println!();

    println!("Leaves:");
    println!("  Count: {}", species.leaves.count);
    println!(
        "  Size: {:.3}m (+/-{:.0}%)",
        species.leaves.size,
        species.leaves.size_variance * 100.0
    );
    println!("  Distribution: {:?}", species.leaves.distribution);
    println!("  Geometry: {:?}", species.leaves.geometry);
    println!();

    if let Some(rosette) = &species.rosette {
        println!("Rosette:");
        println!("  {}", midori_core::rosette::describe(rosette));
        println!("  Blade card: {:?}", rosette.blade_card);
        println!(
            "  Droop: {:.2}, stiffness: {:.2}, keel: {:.2}",
            rosette.droop, rosette.stiffness, rosette.keel
        );
        if let Some(spike) = &rosette.spike {
            println!(
                "  Spike: {:?} x{} (chance {:.0}%), scape {:.2}m + {:.2}m",
                spike.style,
                spike.count,
                spike.chance * 100.0,
                spike.scape_length,
                spike.length
            );
        }
        println!();
    }
    if let Some(inf) = &species.inflorescence {
        println!("Inflorescence:");
        println!("  Kind: {:?}", inf.kind);
        println!("  Count: {}", inf.count);
        println!("  Size: {:.3}m x {:.3}m", inf.length, inf.width);
        println!(
            "  Placement: {:?}, orientation: {:?}",
            inf.placement, inf.orientation
        );
        println!("  Aged: {:.0}%", inf.aged_fraction * 100.0);
        println!();
    }
    if !species.states.is_empty() {
        println!("States: {}", species.state_names().join(", "));
        if let Some(state) = state {
            println!("  Applied: {state}");
        }
        println!();
    }

    println!("Textures:");
    let t = &species.textures;
    println!("  Resolution: {}px", t.resolution);
    println!(
        "  Seed: {}",
        t.seed
            .map(|s| s.to_string())
            .unwrap_or_else(|| "from species name".to_string())
    );
    println!("  Bark style: {:?}", t.bark_style);
    println!("  Leaf shape: {:?}", t.leaf_shape);
    println!("  Leaf card: {:?}", t.leaf_card);
    if species.uses_foliage_atlas() {
        println!("  Foliage atlas: 2x2 (leaf card + organ cells)");
    }
    if !t.bark_patch_colors.is_empty() {
        println!("  Bark patch colours: {}", t.bark_patch_colors.len());
    }
    if t.bark_kino > 0.0 {
        println!("  Bark kino: {:.2}", t.bark_kino);
    }
    for (label, slot) in [
        ("bark_albedo", &t.bark_albedo),
        ("bark_normal", &t.bark_normal),
        ("leaf_albedo_alpha", &t.leaf_albedo_alpha),
    ] {
        if !slot.is_empty() {
            println!("  {label}: {slot}");
        }
    }

    Ok(())
}

struct NatureOptions<'a> {
    patch_path: &'a Path,
    output_path: &'a Path,
    map_resolution: u32,
    preview_resolution: u32,
    scatter_chunk_size: f32,
    no_preview: bool,
    no_prototypes: bool,
    no_scatter: bool,
    no_scatter_binary: bool,
    verbose: bool,
}

fn run_nature(options: &NatureOptions) -> Result<(), Box<dyn std::error::Error>> {
    let patch_path = options.patch_path;
    let output_path = options.output_path;
    let map_resolution = options.map_resolution;
    let preview_resolution = options.preview_resolution;
    let scatter_chunk_size = options.scatter_chunk_size;
    let no_preview = options.no_preview;
    let no_prototypes = options.no_prototypes;
    let no_scatter = options.no_scatter;
    let no_scatter_binary = options.no_scatter_binary;
    let verbose = options.verbose;

    if map_resolution < 2 {
        return Err("map resolution must be at least 2".into());
    }
    if !no_preview && preview_resolution < 2 {
        return Err("preview resolution must be at least 2".into());
    }
    if scatter_chunk_size <= 0.0 || !scatter_chunk_size.is_finite() {
        return Err("scatter chunk size must be a positive finite number".into());
    }

    let start = Instant::now();
    if verbose {
        println!("Loading nature patch from {:?}...", patch_path);
    }
    let patch = NaturePatch::from_file(patch_path)?;
    println!("Nature patch: {}", patch.asset.name);
    println!("  Biome: {}", patch.patch.biome);
    println!("  Tile: {:.2} {}", patch.patch.size, patch.asset.units);
    println!("  Groundcover layers: {}", patch.groundcover.layers.len());

    let config = NaturePackageConfig {
        map_resolution,
        preview_resolution,
        scatter_chunk_size,
        write_preview_mesh: !no_preview,
        write_prototype_meshes: !no_prototypes,
        write_scatter_json: !no_scatter,
        write_scatter_binary: !no_scatter && !no_scatter_binary,
    };

    if verbose {
        println!("Writing package to {:?}...", output_path);
        println!("  Map resolution: {}", config.map_resolution);
        println!("  Preview resolution: {}", config.preview_resolution);
        println!("  Scatter chunk size: {:.2}", config.scatter_chunk_size);
    }

    let summary = patch.write_package(output_path, &config)?;

    println!("Exported nature package: {:?}", output_path);
    println!("  Manifest: {:?}", summary.manifest_path);
    println!("  Maps: {:?}", summary.maps_directory);
    println!("  Map checksum: {:#018x}", summary.map_checksum);
    if let Some(path) = &summary.preview_mesh_path {
        println!("  Preview mesh: {:?}", path);
    }
    println!("  Prototype meshes: {}", summary.prototype_meshes.len());
    if let Some(path) = &summary.scatter_path {
        println!("  Scatter: {:?}", path);
        println!("  Scatter instances: {}", summary.scatter_instance_count);
    }
    if !summary.scatter_binary_paths.is_empty() {
        println!(
            "  Scatter binary chunks: {}",
            summary.scatter_binary_paths.len()
        );
    }
    println!("Generated nature package in {:?}", start.elapsed());

    Ok(())
}

fn run_validate_nature(
    package_path: &PathBuf,
    report_path: Option<&PathBuf>,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    if verbose {
        println!("Validating nature package at {:?}...", package_path);
    }

    let report = validate_nature_package(package_path)?;

    println!("Validated nature package: {:?}", package_path);
    println!("  Asset: {}", report.manifest.asset_name);
    println!("  Schema: {}", report.manifest.schema_version);
    println!("  Maps: {}", report.map_files.len());
    println!("  Prototype meshes: {}", report.prototype_files.len());
    if report.scatter_json_path.is_some() {
        println!(
            "  Scatter JSON instances: {}",
            report.scatter_json_instances
        );
    }
    println!(
        "  Scatter binary chunks: {}",
        report.scatter_binary_files.len()
    );
    println!(
        "  Scatter binary instances: {}",
        report.scatter_binary_instances
    );
    if let Some(path) = report_path
        && let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(path) = report_path {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, &report)?;
        println!("  Report: {:?}", path);
    }
    println!("Validation completed in {:?}", start.elapsed());

    Ok(())
}

struct StandCliOptions<'a> {
    preset_path: &'a Path,
    output_path: &'a Path,
    tile_size: f32,
    tile: &'a str,
    region: Option<&'a str>,
    mask: Option<&'a Path>,
    exclude: Option<&'a Path>,
    height: Option<&'a Path>,
    height_min: f32,
    height_max: f32,
    density: Option<&'a str>,
    budget: StandBudget,
    foliage: FoliageRenderMode,
    raster_resolution: u32,
    skip_library: bool,
    quiet: bool,
    json: bool,
    verbose: bool,
}

fn parse_tile_coord(raw: &str) -> Result<[i32; 2], StandError> {
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    if parts.len() != 2 {
        return Err(StandError::Validation(format!(
            "invalid --tile {:?}: expected `x,z` (e.g. `12,-7`)",
            raw
        )));
    }
    let x = parts[0]
        .parse::<i32>()
        .map_err(|_| StandError::Validation(format!("invalid tile x {:?}", parts[0])))?;
    let z = parts[1]
        .parse::<i32>()
        .map_err(|_| StandError::Validation(format!("invalid tile z {:?}", parts[1])))?;
    Ok([x, z])
}

fn parse_tile_region(raw: &str) -> Result<Vec<[i32; 2]>, StandError> {
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(StandError::Validation(format!(
            "invalid --region {:?}: expected `x0,z0,x1,z1`",
            raw
        )));
    }
    let x0 = parts[0]
        .parse::<i32>()
        .map_err(|_| StandError::Validation(format!("invalid region x0 {:?}", parts[0])))?;
    let z0 = parts[1]
        .parse::<i32>()
        .map_err(|_| StandError::Validation(format!("invalid region z0 {:?}", parts[1])))?;
    let x1 = parts[2]
        .parse::<i32>()
        .map_err(|_| StandError::Validation(format!("invalid region x1 {:?}", parts[2])))?;
    let z1 = parts[3]
        .parse::<i32>()
        .map_err(|_| StandError::Validation(format!("invalid region z1 {:?}", parts[3])))?;
    let (lo_x, hi_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    let (lo_z, hi_z) = if z0 <= z1 { (z0, z1) } else { (z1, z0) };
    let count = (hi_x - lo_x + 1) as i64 * (hi_z - lo_z + 1) as i64;
    if count > 256 {
        return Err(StandError::Validation(format!(
            "--region spans {} tiles (maximum 256 per invocation)",
            count
        )));
    }
    let mut out = Vec::with_capacity(count as usize);
    for z in lo_z..=hi_z {
        for x in lo_x..=hi_x {
            out.push([x, z]);
        }
    }
    Ok(out)
}

fn parse_density_overrides(raw: &str) -> Result<BTreeMap<String, f32>, StandError> {
    let mut map = BTreeMap::new();
    for item in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let Some((k, v)) = item.split_once('=') else {
            return Err(StandError::Validation(format!(
                "invalid --density entry {:?}: expected `stratum=multiplier`",
                item
            )));
        };
        let mul = v.trim().parse::<f32>().map_err(|_| {
            StandError::Validation(format!("invalid density multiplier {:?} for {:?}", v, k))
        })?;
        if !mul.is_finite() || mul < 0.0 {
            return Err(StandError::Validation(format!(
                "density multiplier for {:?} must be finite and >= 0",
                k
            )));
        }
        map.insert(k.trim().to_string(), mul);
    }
    Ok(map)
}

fn run_stand(options: &StandCliOptions) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let verbose = options.verbose && !options.quiet && !options.json;

    if verbose {
        println!(
            "Loading volumetric stand preset {:?}...",
            options.preset_path
        );
    }
    let stand = NatureStand::from_file(options.preset_path)?;
    let base_dir = options
        .preset_path
        .parent()
        .unwrap_or_else(|| Path::new("."));

    let tiles = if let Some(region_str) = options.region {
        parse_tile_region(region_str)?
    } else {
        vec![parse_tile_coord(options.tile)?]
    };

    let mut inputs = StandTileInputs::default();
    if let Some(p) = options.mask {
        inputs.mask = Some(StandRaster::from_png_normalized(p)?);
    }
    if let Some(p) = options.exclude {
        inputs.exclude = Some(StandRaster::from_png_normalized(p)?);
    }
    if let Some(p) = options.height {
        inputs.height = Some(StandRaster::from_png_height(
            p,
            options.height_min,
            options.height_max,
        )?);
    }
    if let Some(d) = options.density {
        inputs.density_overrides = parse_density_overrides(d)?;
    }

    let export_opts = StandExportOptions {
        tile_size_m: options.tile_size,
        foliage_mode: options.foliage,
        budget: options.budget,
        write_library: !options.skip_library,
        raster_resolution: options.raster_resolution,
    };

    let (manifest, library, tile_stats) = generate_stand_tiles(
        &stand,
        base_dir,
        options.output_path,
        &tiles,
        &inputs,
        &export_opts,
    )?;

    let elapsed_ms = start.elapsed().as_millis() as u64;
    if options.json {
        println!(
            "{}",
            serde_json::json!({
                "command": "stand",
                "name": manifest.name,
                "biome": manifest.biome,
                "output": options.output_path,
                "tile_size_m": manifest.tile_size_m,
                "foliage_mode": manifest.foliage_mode,
                "library_variants": library.variants.len(),
                "library_build_ms": library.build_ms,
                "elapsed_ms": elapsed_ms,
                "tiles": tile_stats,
            })
        );
    } else if !options.quiet {
        println!(
            "Generated volumetric stand: {} ({})",
            manifest.name, manifest.biome
        );
        println!("  Output: {:?}", options.output_path);
        println!(
            "  Plant library: {} variant(s) built in {} ms",
            library.variants.len(),
            library.build_ms
        );
        for st in &tile_stats {
            println!(
                "  Tile [{}, {}] ({:.0} m): {} stems ({} instanced, {} merged), cover {:.0}% (target {:.0}%), tris LOD0={} LOD1={} LOD2={}, {} ms",
                st.tile[0],
                st.tile[1],
                st.tile_size_m,
                st.total_stems,
                st.instanced_count,
                st.merged_count,
                st.achieved_total_cover * 100.0,
                st.requested_total_cover * 100.0,
                st.triangles_lod0,
                st.triangles_lod1,
                st.triangles_lod2,
                st.generation_ms
            );
        }
        println!("Completed in {:?}", start.elapsed());
    }

    Ok(())
}

fn run_validate_stand(
    package_path: &Path,
    report_path: Option<&Path>,
    quiet: bool,
    json: bool,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    if verbose && !quiet && !json {
        println!(
            "Validating volumetric stand package at {:?}...",
            package_path
        );
    }

    let report = validate_stand_package(package_path)?;

    if let Some(path) = report_path {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, &report)?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if !quiet {
        println!("Validated volumetric stand package: {:?}", package_path);
        println!("  Stand: {}", report.name);
        println!("  Library variants: {}", report.library_variants);
        println!("  Tiles checked: {}", report.tile_count);
        println!("  Total instances: {}", report.total_instances);
        println!("  Max materials seen: {}", report.max_materials_seen);
        println!("  Opaque foliage verified: {}", report.opaque_verified);
        println!("Validation completed in {:?}", start.elapsed());
    }

    Ok(())
}

fn run_verify_profile_notes(
    path: &std::path::Path,
    output: Option<&std::path::Path>,
    allow_pending: bool,
) -> Result<i32, Box<dyn std::error::Error>> {
    use std::io::Write;

    let report = midori_cli::evidence::verify_profile_notes(path);
    let mut json = serde_json::to_vec_pretty(&report)?;
    json.push(b'\n');
    if let Some(output) = output {
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, &json)?;
    }
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(&json)?;
    handle.flush()?;
    Ok(report.exit_code(allow_pending))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nature_package_command() {
        let cli = Cli::parse_from([
            "midori",
            "nature",
            "-p",
            "presets/nature/temperate_forest_floor.toml",
            "-o",
            "out/forest_floor",
            "--map-resolution",
            "128",
            "--preview-resolution",
            "64",
            "--scatter-chunk-size",
            "4.0",
            "--no-scatter",
        ]);

        match cli.command {
            Commands::Nature {
                patch,
                output,
                map_resolution,
                preview_resolution,
                scatter_chunk_size,
                no_scatter,
                no_scatter_binary,
                ..
            } => {
                assert_eq!(
                    patch,
                    PathBuf::from("presets/nature/temperate_forest_floor.toml")
                );
                assert_eq!(output, PathBuf::from("out/forest_floor"));
                assert_eq!(map_resolution, 128);
                assert_eq!(preview_resolution, 64);
                assert_eq!(scatter_chunk_size, 4.0);
                assert!(no_scatter);
                assert!(!no_scatter_binary);
            }
            _ => panic!("expected nature command"),
        }
    }

    #[test]
    fn parses_state_option_on_generate_maps_and_info() {
        let cli = Cli::parse_from([
            "midori",
            "generate",
            "-s",
            "presets/species/perth_jacaranda.toml",
            "--state",
            "flowering",
        ]);
        match cli.command {
            Commands::Generate { state, .. } => assert_eq!(state.as_deref(), Some("flowering")),
            _ => panic!("expected generate command"),
        }
        let cli = Cli::parse_from(["midori", "info", "-s", "x.toml", "--state", "bare"]);
        match cli.command {
            Commands::Info { state, .. } => assert_eq!(state.as_deref(), Some("bare")),
            _ => panic!("expected info command"),
        }
        let cli = Cli::parse_from(["midori", "maps", "-s", "x.toml"]);
        match cli.command {
            Commands::Maps { state, .. } => assert!(state.is_none()),
            _ => panic!("expected maps command"),
        }
    }

    #[test]
    fn parses_validate_nature_command() {
        let cli = Cli::parse_from([
            "midori",
            "validate-nature",
            "-i",
            "target/midori_nature_cli_smoke",
            "--report",
            "target/midori_nature_cli_report.json",
            "--verbose",
        ]);

        match cli.command {
            Commands::ValidateNature {
                input,
                report,
                verbose,
            } => {
                assert_eq!(input, PathBuf::from("target/midori_nature_cli_smoke"));
                assert_eq!(
                    report,
                    Some(PathBuf::from("target/midori_nature_cli_report.json"))
                );
                assert!(verbose);
            }
            _ => panic!("expected validate-nature command"),
        }
    }

    #[test]
    fn parses_stand_and_validate_stand_and_opaque_generate() {
        let cli = Cli::parse_from([
            "midori",
            "stand",
            "-p",
            "presets/stands/wa_banksia_woodland.toml",
            "-o",
            "out/banksia",
            "--tile-size",
            "128",
            "--tile",
            "12,-7",
            "--foliage",
            "opaque",
            "--max-instances-per-tile",
            "300",
            "--max-triangles-lod0",
            "40000",
            "--max-triangles-lod1",
            "4000",
            "--max-triangles-lod2",
            "200",
            "--density",
            "canopy=1.1,understory=0.9",
        ]);
        match cli.command {
            Commands::Stand {
                preset,
                tile,
                tile_size,
                max_instances_per_tile,
                max_triangles_lod0,
                max_triangles_lod1,
                max_triangles_lod2,
                foliage,
                density,
                ..
            } => {
                assert_eq!(
                    preset,
                    PathBuf::from("presets/stands/wa_banksia_woodland.toml")
                );
                assert_eq!(tile, "12,-7");
                assert_eq!(parse_tile_coord(&tile).unwrap(), [12, -7]);
                assert_eq!(tile_size, 128.0);
                assert_eq!(max_instances_per_tile, 300);
                assert_eq!(max_triangles_lod0, 40_000);
                assert_eq!(max_triangles_lod1, 4_000);
                assert_eq!(max_triangles_lod2, 200);
                assert_eq!(FoliageRenderMode::from(foliage), FoliageRenderMode::Opaque);
                let dm = parse_density_overrides(density.as_deref().unwrap()).unwrap();
                assert!((dm["canopy"] - 1.1).abs() < 1e-5);
                assert!((dm["understory"] - 0.9).abs() < 1e-5);
            }
            _ => panic!("expected stand command"),
        }

        let cli = Cli::parse_from([
            "midori",
            "validate-stand",
            "-i",
            "out/banksia",
            "--report",
            "out/report.json",
        ]);
        match cli.command {
            Commands::ValidateStand { input, report, .. } => {
                assert_eq!(input, PathBuf::from("out/banksia"));
                assert_eq!(report, Some(PathBuf::from("out/report.json")));
            }
            _ => panic!("expected validate-stand command"),
        }

        let cli = Cli::parse_from([
            "midori",
            "generate",
            "-s",
            "presets/species/wa_jarrah.toml",
            "--foliage",
            "opaque",
        ]);
        match cli.command {
            Commands::Generate { foliage, .. } => {
                assert_eq!(FoliageRenderMode::from(foliage), FoliageRenderMode::Opaque);
            }
            _ => panic!("expected generate command"),
        }
    }
}
