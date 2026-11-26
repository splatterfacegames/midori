//! Bark styles and colour layers for barks that strip, tessellate, shed in
//! patches or peel in sheets (`stringy`, `tessellated`, `patchy`, `papery`),
//! plus the optional patch-colour and kino layers.
//!
//! Heightfields are tileable in both directions like the original styles:
//! every noise call uses integer frequencies equal to its period.
//! See `docs/botanical-generators.md`.

use crate::{
    math::lerp,
    species::{BarkStyle, TextureParams},
    textures::{RgbaTexture, fbm, smooth, value_noise, voronoi_f1_f2},
};

/// Large-scale shed-patch field in `[0, 1)` shared by the `patchy` relief
/// (flake lips) and the patch-colour layer, so colour borders sit on the lips.
fn patch_field(u: f32, v: f32, seed: u64) -> f32 {
    let warp_u = (fbm(seed ^ 0x71, u * 6.0, v * 4.0, 6, 4, 3) - 0.5) * 0.10;
    let warp_v = (fbm(seed ^ 0x72, u * 6.0, v * 4.0, 6, 4, 3) - 0.5) * 0.14;
    let a = fbm(seed ^ 0x73, (u + warp_u) * 5.0, (v + warp_v) * 3.0, 5, 3, 4);
    let b = fbm(
        seed ^ 0x74,
        (u - warp_u) * 11.0,
        (v + warp_v) * 5.0,
        11,
        5,
        3,
    );
    // Stretch the noise histogram so every zone gets real area.
    ((a * 0.72 + b * 0.28 - 0.5) * 2.3 + 0.5).clamp(0.0, 0.9999)
}

/// Zone index (`0` = the unshed base bark) and distance to the nearest zone
/// border in field units for `zones` zones.
fn patch_zone(field: f32, zones: usize) -> (usize, f32) {
    // The base bark keeps a wider share than each exposed patch.
    let base_share = 0.42;
    if field < base_share || zones <= 1 {
        return (0, (base_share - field).abs());
    }
    let rest = (field - base_share) / (1.0 - base_share);
    let n = (zones - 1) as f32;
    let k = ((rest * n) as usize).min(zones - 2);
    let fr = rest * n - k as f32;
    let edge = fr.min(1.0 - fr) / n * (1.0 - base_share);
    (
        k + 1,
        if k == 0 {
            edge.min(field - base_share)
        } else {
            edge
        },
    )
}

/// Band coordinate of `papery` bark: the integer part is the sheet index,
/// the fraction runs from a sheet's ragged lower lip (0) up to where the
/// next sheet overlaps it (1).
fn paper_band(u: f32, v: f32, seed: u64) -> f32 {
    let rag = (fbm(seed ^ 0x81, u * 7.0, v * 16.0, 7, 16, 3) - 0.5) * 2.4
        + (fbm(seed ^ 0x82, u * 40.0, v * 16.0, 40, 16, 2) - 0.5) * 0.5;
    v * 16.0 + rag
}

/// Tileable heightfield in `[0, 1]` for the layered styles.
pub(crate) fn height_at(style: BarkStyle, u: f32, v: f32, seed: u64) -> f32 {
    let u = u.rem_euclid(1.0);
    let v = v.rem_euclid(1.0);
    match style {
        BarkStyle::Stringy => {
            // Jarrah / WA Peppermint: deep long fibrous strands that part in interlacing
            // vertical canyons with shredded outer wood-fibre contrast.
            let warp = (fbm(seed ^ 0x51, u * 5.0, v * 2.0, 5, 2, 3) - 0.5) * 0.065;
            let (f1, f2, cell) = voronoi_f1_f2(seed ^ 0x52, (u + warp) * 32.0, v * 2.0, 32, 2);
            let furrow = smooth(((f2 - f1) / 0.19).clamp(0.0, 1.0));
            let strand = fbm(seed ^ 0x53, (u + warp) * 112.0, v * 2.0, 112, 2, 3);
            let strand_ridge = (1.0 - (2.0 * strand - 1.0).abs()).powf(1.45);
            let fibre = fbm(seed ^ 0x54, (u + warp * 0.5) * 256.0, v * 8.0, 256, 8, 3);
            let fibre_ridge = (1.0 - (2.0 * fibre - 1.0).abs()).powf(1.2);
            let fray = value_noise(seed ^ 0x55, u * 320.0, v * 64.0, 320, 64);
            let slab = 0.68 + 0.32 * cell;
            let h =
                (0.20 + 0.80 * furrow) * slab * (0.36 + 0.38 * strand_ridge + 0.26 * fibre_ridge)
                    + (fray - 0.5) * 0.085;
            h.clamp(0.0, 1.0)
        }
        BarkStyle::Tessellated => {
            // Marri / Tuart / Banksia box bark: crisp interlocking squarish plates, deep dark
            // fissure cracks, secondary sub-splits, and weathered rough cork micro-pitting.
            let warp_u = (fbm(seed ^ 0x61, u * 8.0, v * 6.0, 8, 6, 3) - 0.5) * 0.032;
            let warp_v = (fbm(seed ^ 0x62, u * 8.0, v * 6.0, 8, 6, 3) - 0.5) * 0.032;
            let uw = u + warp_u;
            let vw = v + warp_v;
            let (f1, f2, cell) = voronoi_f1_f2(seed ^ 0x63, uw * 22.0, vw * 15.0, 22, 15);
            let crack = smooth(((f2 - f1) / 0.16).clamp(0.0, 1.0));
            let (g1, g2, sub) = voronoi_f1_f2(seed ^ 0x64, uw * 44.0, vw * 30.0, 44, 30);
            let sub_crack = if cell > 0.40 {
                0.64 + 0.36 * smooth(((g2 - g1) / 0.11).clamp(0.0, 1.0))
            } else {
                1.0
            };
            let plate = 0.54 + 0.32 * cell + 0.14 * sub;
            let dome = 1.0 - 0.25 * f1.clamp(0.0, 1.0);
            let grain = fbm(seed ^ 0x65, u * 192.0, v * 112.0, 192, 112, 3);
            let pit = value_noise(seed ^ 0x66, u * 320.0, v * 224.0, 320, 224);
            let h = crack * sub_crack * plate * dome * (0.77 + 0.27 * grain) + (pit - 0.5) * 0.075;
            h.clamp(0.0, 1.0)
        }
        BarkStyle::Patchy => {
            // Smooth Australian gum bark: exfoliating flakes with curled peeling ribbon edges,
            // subtle recessed scribbly-gum tracks, and silky living epidermis porosity.
            let field = patch_field(u, v, seed);
            let (zone, edge) = patch_zone(field, 4);
            let level = 0.58 - 0.085 * zone as f32;
            let lip = (-(edge / 0.013).powi(2)).exp() * 0.22;
            let lip_under = (-((edge - 0.018) / 0.008).powi(2)).exp() * 0.09;
            let swell = fbm(seed ^ 0x75, u * 6.0, v * 3.0, 6, 3, 3);
            let stria = fbm(seed ^ 0x76, u * 112.0, v * 10.0, 112, 10, 3);
            let stria_line = (1.0 - (2.0 * stria - 1.0).abs()).powf(2.2);
            // Subtle winding scribbly-gum moth tracks etched into smooth pale gum plates
            let scrib_a = fbm(seed ^ 0x78, u * 20.0, v * 12.0, 20, 12, 3);
            let scrib_mask = fbm(seed ^ 0x79, u * 6.0, v * 4.0, 6, 4, 2);
            let scrib = if scrib_mask > 0.58 && (scrib_a - 0.50).abs() < 0.011 {
                0.08 * (1.0 - (scrib_a - 0.50).abs() / 0.011)
            } else {
                0.0
            };
            let skin = value_noise(seed ^ 0x77, u * 288.0, v * 144.0, 288, 144);
            let h = level + lip - lip_under - scrib
                + 0.11 * (swell - 0.5)
                + 0.06 * stria_line
                + 0.05 * (skin - 0.5);
            h.clamp(0.0, 1.0)
        }
        BarkStyle::Papery => {
            // Paperbark (Melaleuca): thick multi-layered overlapping parchment sheets, each
            // with a ragged curled free edge, deep shadow crease, and fibrous torn vertical splits.
            let band = paper_band(u, v, seed);
            let fr = band.rem_euclid(1.0);
            let sheet = 0.30 + 0.52 * fr.powf(0.62);
            let lip_curl = (-((fr - 0.07) / 0.035).powi(2)).exp() * 0.14;
            let lip_shadow = (-(fr / 0.055).powi(2)).exp() * 0.36;
            let wrinkle = fbm(seed ^ 0x83, u * 22.0, v * 168.0, 22, 168, 3);
            let split = fbm(seed ^ 0x84, u * 38.0, v * 6.0, 38, 6, 3);
            let split_cut = if (split - 0.5).abs() < 0.016 {
                0.28 * (1.0 - (split - 0.5).abs() / 0.016)
            } else {
                0.0
            };
            let fuzz = value_noise(seed ^ 0x85, u * 320.0, v * 320.0, 320, 320);
            let h = sheet + lip_curl - lip_shadow + 0.12 * (wrinkle - 0.5) - split_cut
                + 0.06 * (fuzz - 0.5);
            h.clamp(0.0, 1.0)
        }
        BarkStyle::PalmDiamond => {
            // Persistent criss-crossing diamond leaf-base scars of Canary Island
            // date palm and cotton fan palm: diagonal lattice of raised truncated
            // petiole bosses separated by deep dark fibrous diamond seams.
            let du = u * 10.0;
            let dv = v * 14.0;
            // Two diagonal coordinates forming crisp interlocking diamonds
            let a = (du + dv).rem_euclid(1.0);
            let b = (du - dv).rem_euclid(1.0);
            let edge_a = a.min(1.0 - a);
            let edge_b = b.min(1.0 - b);
            let seam = smooth((edge_a.min(edge_b) / 0.14).clamp(0.0, 1.0));
            // Each diamond boss steps outward slightly toward its upper truncated lip
            let lip = smooth((b / 0.85).clamp(0.0, 1.0));
            let fibre = fbm(seed ^ 0xA1, u * 96.0, v * 48.0, 96, 48, 3);
            let scar = value_noise(seed ^ 0xA2, u * 192.0, v * 192.0, 192, 192);
            let h = seam * (0.48 + 0.34 * lip + 0.14 * fibre) + (scar - 0.5) * 0.08;
            h.clamp(0.0, 1.0)
        }
        // The original styles are evaluated in `textures.rs`.
        BarkStyle::Furrowed | BarkStyle::Plated | BarkStyle::Smooth => 0.5,
    }
}

fn luminance(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// Apply the optional colour layers to a finished bark albedo: exposed patch
/// colours (`patchy`), sheet-edge underlayers (`papery`) and kino streaks.
///
/// A no-op unless `bark_patch_colors` is non-empty on a style that uses it or
/// `bark_kino` is above zero, so existing species are untouched.
pub(crate) fn apply_layers(
    albedo: &mut RgbaTexture,
    height: &[f32],
    params: &TextureParams,
    seed: u64,
) {
    let style = params.bark_style;
    let patches = &params.bark_patch_colors;
    let use_patches = !patches.is_empty() && matches!(style, BarkStyle::Patchy | BarkStyle::Papery);
    let kino = params.bark_kino.clamp(0.0, 1.0);
    if !use_patches && kino <= 0.0 {
        return;
    }
    let base = params.bark_color.unwrap_or([0.42, 0.34, 0.26]);
    let base_lum = luminance(base).max(0.05);
    let res = albedo.width;
    let inv = 1.0 / res as f32;
    for y in 0..res {
        let v = (y as f32 + 0.5) * inv;
        for x in 0..res {
            let u = (x as f32 + 0.5) * inv;
            let i = (y * res + x) as usize;
            let px = albedo.get(x, y);
            let mut rgb = [
                px[0] as f32 / 255.0,
                px[1] as f32 / 255.0,
                px[2] as f32 / 255.0,
            ];

            if use_patches && style == BarkStyle::Patchy {
                let zones = patches.len() + 1;
                let (zone, edge) = patch_zone(patch_field(u, v, seed), zones);
                if zone > 0 {
                    // Keep the painted relief shading, swap the pigment.
                    let shade = (luminance(rgb) / base_lum).clamp(0.55, 1.35);
                    let target = patches[zone - 1];
                    let blend = (edge / 0.004).clamp(0.0, 1.0);
                    for ch in 0..3 {
                        rgb[ch] = lerp(rgb[ch], (target[ch] * shade).clamp(0.0, 1.0), blend);
                    }
                }
            } else if use_patches && style == BarkStyle::Papery {
                // Under-sheets show in the shadow just above each ragged lip.
                let band = paper_band(u, v, seed);
                let fr = band.rem_euclid(1.0);
                let sheet_index = band.floor() as i64;
                let target = patches[sheet_index.rem_euclid(patches.len() as i64) as usize];
                let reveal = (1.0 - fr / 0.22).clamp(0.0, 1.0).powf(1.4) * 0.85;
                let shade = (luminance(rgb) / base_lum).clamp(0.55, 1.25);
                for ch in 0..3 {
                    rgb[ch] = lerp(rgb[ch], (target[ch] * shade).clamp(0.0, 1.0), reveal);
                }
            }

            if kino > 0.0 {
                // Gum bleeds from cracks and runs down the trunk in narrow streaks.
                let column = fbm(seed ^ 0x91, u * 30.0, v * 2.0, 30, 2, 3);
                let run = fbm(seed ^ 0x92, u * 30.0, v * 6.0, 30, 6, 3);
                let bead = value_noise(seed ^ 0x93, u * 128.0, v * 64.0, 128, 64);
                let threshold = lerp(0.74, 0.56, kino);
                let streak = smooth(((column - threshold) / 0.08).clamp(0.0, 1.0));
                let in_crack = (1.0 - height[i] / 0.55).clamp(0.0, 1.0);
                let mask = (streak * (0.35 + 0.65 * run) * (0.55 + 0.45 * in_crack)
                    + in_crack * kino * 0.25 * bead)
                    .clamp(0.0, 1.0)
                    * (0.55 + 0.45 * kino);
                let gum = [0.30 - 0.08 * bead, 0.075, 0.045];
                for ch in 0..3 {
                    rgb[ch] = lerp(rgb[ch], gum[ch], mask * 0.9);
                }
            }

            albedo.set(
                x,
                y,
                [
                    (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
                    255,
                ],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layered_heights_tile_in_both_directions() {
        for style in [
            BarkStyle::Stringy,
            BarkStyle::Tessellated,
            BarkStyle::Patchy,
            BarkStyle::Papery,
        ] {
            for k in 0..12 {
                let t = k as f32 / 12.0 + 0.031;
                let a = height_at(style, 0.0005, t, 7);
                let b = height_at(style, 0.9995, t, 7);
                assert!((a - b).abs() < 0.2, "{style:?} u seam at v={t}: {a} vs {b}");
                let c = height_at(style, t, 0.0005, 7);
                let d = height_at(style, t, 0.9995, 7);
                assert!((c - d).abs() < 0.2, "{style:?} v seam at u={t}: {c} vs {d}");
            }
        }
    }

    #[test]
    fn layered_heights_have_relief() {
        for style in [
            BarkStyle::Stringy,
            BarkStyle::Tessellated,
            BarkStyle::Patchy,
            BarkStyle::Papery,
        ] {
            let mut lo = 1.0f32;
            let mut hi = 0.0f32;
            for y in 0..64 {
                for x in 0..64 {
                    let h = height_at(style, x as f32 / 64.0, y as f32 / 64.0, 99);
                    assert!((0.0..=1.0).contains(&h));
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            assert!(hi - lo > 0.2, "{style:?} is flat: {lo}..{hi}");
        }
    }

    #[test]
    fn patch_zones_cover_every_colour() {
        let mut seen = [0u32; 4];
        for y in 0..96 {
            for x in 0..96 {
                let f = patch_field(x as f32 / 96.0, y as f32 / 96.0, 3);
                seen[patch_zone(f, 4).0] += 1;
            }
        }
        for (zone, n) in seen.iter().enumerate() {
            assert!(*n > 96 * 96 / 40, "zone {zone} covers only {n} texels");
        }
    }
}
