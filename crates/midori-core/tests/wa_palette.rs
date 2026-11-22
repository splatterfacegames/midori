//! Automated Western Australian native foliage colour guardrail (Issue #38 §1).
//!
//! Generates procedural textures for every bundled `wa_*.toml` native species
//! preset and verifies that the opaque foliage pixels (top-left leaf cell when
//! the species uses a 2x2 atlas, full leaf card otherwise) stay within
//! botanically realistic South-West WA sclerophyll HSV bounds — muted olive,
//! grey-green, khaki, blue-green, or silvery-sage rather than European
//! saturated parkland greens.

use std::path::{Path, PathBuf};

use midori_core::{Species, TextureSet};

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta < 1e-6 {
        0.0
    } else if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if (max - g).abs() < 1e-6 {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let s = if max < 1e-6 { 0.0 } else { delta / max };
    (h, s, max)
}

fn mean_foliage_hsv(species: &Species) -> (f32, f32, f32, usize) {
    let mut fast = species.clone();
    fast.textures.resolution = 128;
    let maps = TextureSet::generate(&fast);
    let tex = &maps.leaf_card;
    let w = tex.width as usize;
    let h = tex.height as usize;

    // When a species packs a 2x2 foliage+organ atlas (rosettes or shared-material
    // inflorescences), the live foliage spray occupies the top-left quadrant.
    let (x_end, y_end) = if fast.uses_foliage_atlas() {
        (w / 2, h / 2)
    } else {
        (w, h)
    };

    let mut sum_r = 0.0_f64;
    let mut sum_g = 0.0_f64;
    let mut sum_b = 0.0_f64;
    let mut count = 0_usize;

    for y in 0..y_end {
        for x in 0..x_end {
            let idx = (y * w + x) * 4;
            let a = tex.pixels[idx + 3];
            if a >= 160 {
                sum_r += tex.pixels[idx] as f64 / 255.0;
                sum_g += tex.pixels[idx + 1] as f64 / 255.0;
                sum_b += tex.pixels[idx + 2] as f64 / 255.0;
                count += 1;
            }
        }
    }

    assert!(
        count > 20,
        "species `{}` produced too few opaque foliage pixels ({count})",
        species.species.name
    );

    let r = (sum_r / count as f64) as f32;
    let g = (sum_g / count as f64) as f32;
    let b = (sum_b / count as f64) as f32;
    let (hue, sat, val) = rgb_to_hsv(r, g, b);
    (hue, sat, val, count)
}

#[test]
fn all_native_wa_species_stay_within_sclerophyll_hsv_guardrails() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets/species");
    let mut wa_files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("wa_") && n.ends_with(".toml"))
        })
        .collect();
    wa_files.sort();

    assert_eq!(
        wa_files.len(),
        29,
        "expected all 29 native WA species presets (found {})",
        wa_files.len()
    );

    let mut failures = Vec::new();
    for path in &wa_files {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let species =
            Species::from_file(path).unwrap_or_else(|e| panic!("failed to load {stem}: {e}"));
        let (h, s, v, _count) = mean_foliage_hsv(&species);

        // Richer glossy forest understorey natives (Zamia, Bull Banksia) may reach
        // slightly higher chroma than dry sandplain/eucalypt sclerophylls, but all
        // native WA species remain well below saturated northern-hemisphere greens.
        let max_sat = match stem {
            "wa_zamia" | "wa_bull_banksia" => 0.48,
            _ => 0.42,
        };
        let min_val = 0.24;
        let max_val = 0.72;

        // Silvery woolly coastal daisybush (Olearia) can be very low chroma, so hue
        // only needs to be checked when saturation is appreciable.
        if s > 0.06 && !(50.0..=170.0).contains(&h) {
            failures.push(format!(
                "{stem}: mean foliage hue {h:.1}° outside WA olive/grey-green/blue-green band [50°, 170°] (S={s:.3}, V={v:.3})"
            ));
        }
        if s > max_sat {
            failures.push(format!(
                "{stem}: mean foliage saturation {s:.3} exceeds WA guardrail {max_sat:.2} (H={h:.1}°, V={v:.3})"
            ));
        }
        if !(min_val..=max_val).contains(&v) {
            failures.push(format!(
                "{stem}: mean foliage value {v:.3} outside [{min_val:.2}, {max_val:.2}] (H={h:.1}°, S={s:.3})"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "WA native foliage palette guardrail failed:\n{}",
        failures.join("\n")
    );
}
