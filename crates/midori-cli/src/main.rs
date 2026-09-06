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

use clap::{Parser, Subcommand, ValueEnum};
use midori_core::{
    ExportConfig, ExportFormat, ExportMetadata, LodGenerationConfig, NaturePackageConfig,
    NaturePatch, Species, export_lod_meshes, export_mesh, generate_lod_meshes_with_config,
    generate_tree, validate_nature_package,
};
use std::path::PathBuf;
use std::time::Instant;

/// Midori - Procedural Nature Generator
#[derive(Parser)]
#[command(name = "midori")]
#[command(author, version, about = "Procedural Nature Generator", long_about = None)]
struct Cli {
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

        /// Output path (default: tree.glb)
        #[arg(short, long, default_value = "tree.glb")]
        output: PathBuf,

        /// Number of tree variants to generate
        #[arg(short = 'n', long, default_value_t = 1)]
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

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show information about a species file
    Info {
        /// Species TOML file
        #[arg(short, long)]
        species: PathBuf,
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

#[derive(Clone, ValueEnum)]
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

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    /// Binary glTF (.glb)
    Glb,
    /// JSON glTF with separate binary (.gltf + .bin)
    Gltf,
}

#[derive(Clone, ValueEnum)]
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
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Generate {
            species,
            output,
            count,
            seed,
            lod,
            format,
            lod_preset,
            verbose,
        } => run_generate(
            &species, &output, count, seed, lod, format, lod_preset, verbose,
        ),
        Commands::Info { species } => run_info(&species),
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
        } => run_nature(
            &patch,
            &output,
            map_resolution,
            preview_resolution,
            scatter_chunk_size,
            no_preview,
            no_prototypes,
            no_scatter,
            no_scatter_binary,
            verbose,
        ),
        Commands::ValidateNature {
            input,
            report,
            verbose,
        } => run_validate_nature(&input, report.as_ref(), verbose),
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
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run_generate(
    species_path: &PathBuf,
    output_path: &PathBuf,
    count: u32,
    seed: Option<u64>,
    lod: LodOption,
    format: OutputFormat,
    lod_preset: LodPreset,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();

    // Load species
    if verbose {
        println!("Loading species from {:?}...", species_path);
    }
    let species = Species::from_file(species_path)?;
    println!(
        "Species: {} ({})",
        species.species.name,
        species.latin_name()
    );

    // Get LOD config
    let lod_config = match lod_preset {
        LodPreset::Ultra => LodGenerationConfig::ultra(),
        LodPreset::HighQuality => LodGenerationConfig::high_quality(),
        LodPreset::Balanced => LodGenerationConfig::balanced(),
        LodPreset::Mobile => LodGenerationConfig::mobile(),
        LodPreset::Minimal => LodGenerationConfig::minimal(),
    };

    // Get export config
    let export_config = ExportConfig {
        format: match format {
            OutputFormat::Glb => ExportFormat::Glb,
            OutputFormat::Gltf => ExportFormat::GlTf,
        },
        draco: false,
        embed_textures: false,
        pivot_painter_extras: true,
        metadata: None,
    };

    // Generate trees
    for i in 0..count {
        let tree_seed = seed.unwrap_or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
                + i as u64
        }) + i as u64;

        if verbose {
            println!("Generating tree {} with seed {}...", i + 1, tree_seed);
        }

        let gen_start = Instant::now();
        let tree = generate_tree(&species, tree_seed);

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
            output_path.with_file_name(format!("{}_{}.{}", stem, i, ext))
        } else {
            output_path.clone()
        };

        // Generate LOD meshes
        let mesh_start = Instant::now();
        let lod_meshes = generate_lod_meshes_with_config(&tree, &species, &lod_config);

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
        match lod {
            LodOption::All => {
                export_lod_meshes(&lod_meshes, &tree_output, &tree_export_config)?;
            }
            LodOption::Lod0 | LodOption::Lod1 | LodOption::Lod2 | LodOption::Lod3 => {
                let level = match lod {
                    LodOption::Lod0 => 0,
                    LodOption::Lod1 => 1,
                    LodOption::Lod2 => 2,
                    LodOption::Lod3 => 3,
                    _ => 0,
                };
                if let Some(lod_mesh) = lod_meshes.get(level) {
                    export_mesh(&lod_mesh.mesh, &tree_output, &tree_export_config)?;
                } else {
                    return Err(format!("LOD level {} not available", level).into());
                }
            }
        }

        if verbose {
            println!("  Exported in {:?}", export_start.elapsed());
        }

        println!("Exported: {:?}", tree_output);
    }

    let elapsed = start.elapsed();
    println!("Generated {} tree(s) in {:?}", count, elapsed);

    Ok(())
}

fn run_info(species_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let species = Species::from_file(species_path)?;

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
    println!();

    println!("Branches:");
    for level in 1..=3 {
        if let Some(params) = species.get_branch_level(level) {
            println!("  Level {}:", level);
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

    Ok(())
}

fn run_nature(
    patch_path: &PathBuf,
    output_path: &PathBuf,
    map_resolution: u32,
    preview_resolution: u32,
    scatter_chunk_size: f32,
    no_preview: bool,
    no_prototypes: bool,
    no_scatter: bool,
    no_scatter_binary: bool,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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
    if let Some(path) = report_path {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, &report)?;
        println!("  Report: {:?}", path);
    }
    println!("Validation completed in {:?}", start.elapsed());

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
}
