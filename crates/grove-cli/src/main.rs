//! Grove - Procedural Tree Generator CLI
//!
//! Command-line interface for the grove procedural tree generator.
//! Generates 3D tree meshes from species TOML files with support for
//! LOD levels, multiple output formats, and batch generation.
//!
//! ## Usage
//!
//! ```bash
//! grove generate -s species/oak.toml -o forest/oak.glb
//! grove generate -s species/pine.toml -n 10 --seed 42
//! grove info -s species/oak.toml
//! ```

use clap::{Parser, Subcommand, ValueEnum};
use grove_core::{
    ExportConfig, ExportFormat, LodGenerationConfig, Species, TextureSet, export_lod_meshes,
    export_mesh, generate_lod_meshes_with_config, generate_tree,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Grove - Procedural Tree Generator
#[derive(Parser)]
#[command(name = "grove")]
#[command(author, version, about = "Procedural Tree Generator", long_about = None)]
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

        /// Embed material maps (procedural or file-slot overrides) into the export
        #[arg(long)]
        textures: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Write the species' material maps (bark albedo, bark normal, leaf card) as PNGs
    Maps {
        /// Species TOML file
        #[arg(short, long)]
        species: PathBuf,

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
            textures,
            verbose,
        } => run_generate(&GenerateOptions {
            species_path: &species,
            output_path: &output,
            count,
            seed,
            lod,
            format,
            lod_preset,
            textures,
            verbose,
        }),
        Commands::Maps {
            species,
            output,
            prefix,
        } => run_maps(&species, &output, prefix.as_deref()),
        Commands::Info { species } => run_info(&species),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

struct GenerateOptions<'a> {
    species_path: &'a Path,
    output_path: &'a Path,
    count: u32,
    seed: Option<u64>,
    lod: LodOption,
    format: OutputFormat,
    lod_preset: LodPreset,
    textures: bool,
    verbose: bool,
}

fn run_generate(options: &GenerateOptions) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let species_path = options.species_path;
    let output_path = options.output_path;
    let count = options.count;
    let seed = options.seed;
    let verbose = options.verbose;

    // Load species
    if verbose {
        println!("Loading species from {species_path:?}...");
    }
    let species = Species::from_file(species_path)?;
    println!(
        "Species: {} ({})",
        species.species.name, species.species.scientific
    );

    // Get LOD config
    let lod_config = match options.lod_preset {
        LodPreset::Ultra => LodGenerationConfig::ultra(),
        LodPreset::HighQuality => LodGenerationConfig::high_quality(),
        LodPreset::Balanced => LodGenerationConfig::balanced(),
        LodPreset::Mobile => LodGenerationConfig::mobile(),
        LodPreset::Minimal => LodGenerationConfig::minimal(),
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
            output_path.with_file_name(format!("{stem}_{i}.{ext}"))
        } else {
            output_path.to_path_buf()
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

        // Export based on LOD option
        let export_start = Instant::now();
        match options.lod {
            LodOption::All => {
                export_lod_meshes(&lod_meshes, &tree_output, &export_config)?;
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
                    export_mesh(&lod_mesh.mesh, &tree_output, &export_config)?;
                } else {
                    return Err(format!("LOD level {level} not available").into());
                }
            }
        }

        if verbose {
            println!("  Exported in {:?}", export_start.elapsed());
        }

        println!("Exported: {tree_output:?}");
    }

    let elapsed = start.elapsed();
    println!("Generated {count} tree(s) in {elapsed:?}");

    Ok(())
}

fn run_maps(
    species_path: &Path,
    output_dir: &Path,
    prefix: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let species = Species::from_file(species_path)?;
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

    Ok(())
}

fn run_info(species_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let species = Species::from_file(species_path)?;

    println!(
        "Species: {} ({})",
        species.species.name, species.species.scientific
    );
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
