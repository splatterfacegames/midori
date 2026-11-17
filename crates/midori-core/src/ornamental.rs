//! Procedural generators for sheared topiary hedges, ornamental floral planters
//! and hanging cascades, wall-climbing woody vines, and potted tropical clumps.

use crate::{
    constants::{PI, TAU},
    generation::{GenerationBudget, GenerationError, GenerationEstimate},
    math::{create_basis, lerp, radians},
    rng::Rng,
    species::{FloralHabit, GeneratorFamily, Species, TopiaryParams, TopiaryShape},
    tree::{Leaf, Segment, Stem, Tree},
};
use glam::{Quat, Vec2, Vec3};

/// Worst-case stem & leaf estimate for ornamental generator families.
pub(crate) fn estimate_ornamental(species: &Species) -> GenerationEstimate {
    match species.generator.family {
        GeneratorFamily::Topiary => {
            let p = species.topiary.clone().unwrap_or_default();
            let roots = ((p.length / 0.40).ceil() as u64).clamp(1, 24);
            GenerationEstimate {
                max_stems: 1 + roots * 16,
                max_leaves: (species.leaves.count as u64).saturating_add(64),
            }
        }
        GeneratorFamily::FloralShrub => {
            let p = species.floral.clone().unwrap_or_default();
            let base_stems = ((p.length * p.width * 36.0).ceil() as u64).clamp(16, 64);
            let peduncle_stems = (species.leaves.count as u64 / 2).max(128);
            let cascade_stems = if p.habit == FloralHabit::Cascade {
                64
            } else {
                0
            };
            GenerationEstimate {
                max_stems: 8 + base_stems + peduncle_stems + cascade_stems,
                max_leaves: (species.leaves.count as u64).saturating_add(128),
            }
        }
        GeneratorFamily::ClimbingVine => {
            let p = species.vine.clone().unwrap_or_default();
            let mains = p.main_vines.max(1) as u64;
            let runners = p.hanging_runners as u64;
            GenerationEstimate {
                max_stems: 1 + mains * 18 + runners * 2,
                max_leaves: (species.leaves.count as u64).saturating_add(64),
            }
        }
        GeneratorFamily::TropicalClump => {
            let l1 = species
                .branches
                .level1
                .as_ref()
                .map(|b| (b.count + b.count_variance) as u64)
                .unwrap_or(22)
                .max(8);
            GenerationEstimate {
                max_stems: 4 + l1,
                max_leaves: l1.max(species.leaves.count as u64),
            }
        }
        _ => GenerationEstimate {
            max_stems: 1,
            max_leaves: species.leaves.count as u64,
        },
    }
}

/// Dispatch ornamental generator family under `budget`.
pub(crate) fn generate_ornamental(
    species: &Species,
    seed: u64,
    budget: GenerationBudget,
) -> Result<Tree, GenerationError> {
    budget.check_estimate(&estimate_ornamental(species))?;
    let mut tree = match species.generator.family {
        GeneratorFamily::Topiary => generate_topiary(species, seed),
        GeneratorFamily::FloralShrub => generate_floral_shrub(species, seed),
        GeneratorFamily::ClimbingVine => generate_climbing_vine(species, seed),
        GeneratorFamily::TropicalClump => generate_tropical_clump(species, seed),
        _ => Tree::new(species.species.name.clone(), seed),
    };
    tree.update_bounds();
    budget.check_tree(&tree)?;
    Ok(tree)
}

#[inline]
fn hash3(p: Vec3, seed: u64) -> f32 {
    let q = (p * 7.13).floor();
    let mut h = seed
        ^ (q.x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (q.y as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ (q.z as i64 as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    ((h ^ (h >> 29)) & 0xFF_FFFF) as f32 / 16_777_215.0
}

#[inline]
fn smooth_noise3(p: Vec3, seed: u64) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let sample = |dx: f32, dy: f32, dz: f32| {
        let cell = i + Vec3::new(dx, dy, dz);
        hash3(cell / 7.13, seed)
    };
    let c000 = sample(0.0, 0.0, 0.0);
    let c100 = sample(1.0, 0.0, 0.0);
    let c010 = sample(0.0, 1.0, 0.0);
    let c110 = sample(1.0, 1.0, 0.0);
    let c001 = sample(0.0, 0.0, 1.0);
    let c101 = sample(1.0, 0.0, 1.0);
    let c011 = sample(0.0, 1.0, 1.0);
    let c111 = sample(1.0, 1.0, 1.0);
    let x00 = lerp(c000, c100, u.x);
    let x10 = lerp(c010, c110, u.x);
    let x01 = lerp(c001, c101, u.x);
    let x11 = lerp(c011, c111, u.x);
    lerp(lerp(x00, x10, u.y), lerp(x01, x11, u.y), u.z)
}

/// Orient local `+Y` along `tip_dir` and local `+Z` toward `face_normal`.
fn orient_leaf(tip_dir: Vec3, face_normal: Vec3, roll_jitter: f32) -> Quat {
    let y_axis = tip_dir.try_normalize().unwrap_or(Vec3::Y);
    let base_rot = Quat::from_rotation_arc(Vec3::Y, y_axis);
    let in_plane = face_normal - y_axis * face_normal.dot(y_axis);
    let align_roll = if let Some(n_dir) = in_plane.try_normalize() {
        let bz = base_rot * Vec3::Z;
        let bx = base_rot * Vec3::X;
        n_dir.dot(bx).atan2(n_dir.dot(bz))
    } else {
        0.0
    };
    Quat::from_axis_angle(y_axis, align_roll + roll_jitter) * base_rot
}

fn push_polyline_stem(
    tree: &mut Tree,
    level: u8,
    parent_id: Option<u32>,
    parent_offset: f32,
    pts: &[Vec3],
    r_start: f32,
    r_end: f32,
) -> u32 {
    let id = tree.stems.len() as u32;
    let mut stem = Stem::new(id, level);
    stem.parent_id = parent_id;
    stem.parent_offset = parent_offset;
    let n = pts.len().saturating_sub(1).max(1);
    for i in 0..pts.len().saturating_sub(1) {
        let a = pts[i];
        let b = pts[i + 1];
        let dir = (b - a).try_normalize().unwrap_or(Vec3::Y);
        let t0 = i as f32 / n as f32;
        let t1 = (i + 1) as f32 / n as f32;
        stem.segments.push(Segment {
            start: a,
            end: b,
            start_radius: lerp(r_start, r_end, t0).max(0.002),
            end_radius: lerp(r_start, r_end, t1).max(0.0015),
            direction: dir,
        });
    }
    tree.add_stem(stem)
}

// ============================================================================
// 1. SHEARED BOXWOOD HEDGES & TOPIARY (`GeneratorFamily::Topiary`)
// ============================================================================

fn topiary_raw_sdf(p: Vec3, params: &TopiaryParams) -> f32 {
    let skirt = params.skirt_clearance.clamp(0.0, params.height * 0.45);
    let body_h = (params.height - skirt).max(0.15);
    let cy = skirt + body_h * 0.5;
    let dy = p.y - cy;

    match params.shape {
        TopiaryShape::Box => {
            let hx = (params.length * 0.5).max(0.08);
            let hy = (body_h * 0.5).max(0.08);
            let hz = (params.width * 0.5).max(0.08);
            let bev = params.bevel.clamp(0.0, hx.min(hy).min(hz) * 0.85);
            let q = Vec3::new(
                p.x.abs() - (hx - bev),
                dy.abs() - (hy - bev),
                p.z.abs() - (hz - bev),
            );
            q.max(Vec3::ZERO).length() + q.x.max(q.y.max(q.z)).min(0.0) - bev
        }
        TopiaryShape::BentBox => {
            let theta = radians(params.bend_angle.clamp(-175.0, 175.0));
            if theta.abs() < 0.05 {
                let mut straight = params.clone();
                straight.shape = TopiaryShape::Box;
                return topiary_raw_sdf(p, &straight);
            }
            let sign = theta.signum();
            let half_theta = theta.abs() * 0.5;
            let arc_r = (params.length / theta.abs()).max(params.width * 0.65);
            // Center arc so midpoint sits near (0, 0) and ends sweep symmetrically
            let cz = sign * arc_r * (half_theta.cos() * 0.5 + 0.5);
            let rel_x = p.x;
            let rel_z = (cz - p.z) * sign;
            let r = Vec2::new(rel_x, rel_z).length().max(1e-4);
            let ang = rel_x.atan2(rel_z);
            let s_excess = if ang.abs() <= half_theta {
                0.0
            } else {
                arc_r * (ang.abs() - half_theta)
            };
            let r_across = (r - arc_r).abs();
            let hy = (body_h * 0.5).max(0.08);
            let hz = (params.width * 0.5).max(0.08);
            let bev = params.bevel.clamp(0.0, hy.min(hz) * 0.85);
            let q_box = Vec3::new(
                arc_r * ang.abs() - (params.length * 0.5 - bev),
                dy.abs() - (hy - bev),
                r_across - (hz - bev),
            );
            let _ = s_excess;
            q_box.max(Vec3::ZERO).length() + q_box.x.max(q_box.y.max(q_box.z)).min(0.0) - bev
        }
        TopiaryShape::Sphere => {
            let rx = (params.length * 0.5).max(0.10);
            let ry = (body_h * 0.5).max(0.10);
            let rz = (params.width * 0.5).max(0.10);
            let q = Vec3::new(p.x / rx, dy / ry, p.z / rz);
            let k0 = q.length();
            if k0 < 1e-5 {
                -rx.min(ry).min(rz)
            } else {
                let k1 = Vec3::new(p.x / (rx * rx), dy / (ry * ry), p.z / (rz * rz)).length();
                k0 * (k0 - 1.0) / k1.max(1e-5)
            }
        }
        TopiaryShape::Cylinder => {
            let rx = (params.length * 0.5).max(0.10);
            let rz = (params.width * 0.5).max(0.10);
            let hy = (body_h * 0.5).max(0.08);
            let r_avg = (rx + rz) * 0.5;
            let bev = params.bevel.clamp(0.0, r_avg.min(hy) * 0.85);
            let radial = Vec2::new(p.x / rx, p.z / rz).length() * r_avg - (r_avg - bev);
            let vert = dy.abs() - (hy - bev);
            Vec2::new(radial.max(0.0), vert.max(0.0)).length() + radial.max(vert).min(0.0) - bev
        }
        TopiaryShape::Cone => {
            let t_h = ((p.y - skirt) / body_h).clamp(0.0, 1.0);
            let rx = lerp(params.length * 0.5, params.bevel.max(0.07), t_h).max(0.05);
            let rz = lerp(params.width * 0.5, params.bevel.max(0.07), t_h).max(0.05);
            let r_avg = (rx + rz) * 0.5;
            let radial = (Vec2::new(p.x / rx, p.z / rz).length() - 1.0) * r_avg;
            let vert = dy.abs() - body_h * 0.5;
            let bev = params.bevel.clamp(0.0, 0.12);
            Vec2::new((radial + bev).max(0.0), (vert + bev).max(0.0)).length()
                + (radial + bev).max(vert + bev).min(0.0)
                - bev
        }
    }
}

fn topiary_sdf(p: Vec3, params: &TopiaryParams, seed: u64) -> f32 {
    let base = topiary_raw_sdf(p, params);
    let undulate = (smooth_noise3(p * 3.2, seed ^ 0x5EA2) - 0.5) * 0.036
        + (smooth_noise3(p * 7.5, seed ^ 0x91B3) - 0.5) * 0.016;
    base + undulate * (1.15 - 0.55 * params.trim_tightness)
}

fn topiary_gradient(p: Vec3, params: &TopiaryParams, seed: u64) -> Vec3 {
    let e = 0.015;
    let dx =
        topiary_sdf(p + Vec3::X * e, params, seed) - topiary_sdf(p - Vec3::X * e, params, seed);
    let dy =
        topiary_sdf(p + Vec3::Y * e, params, seed) - topiary_sdf(p - Vec3::Y * e, params, seed);
    let dz =
        topiary_sdf(p + Vec3::Z * e, params, seed) - topiary_sdf(p - Vec3::Z * e, params, seed);
    Vec3::new(dx, dy, dz).try_normalize().unwrap_or(Vec3::Y)
}

/// Map a normalized spine coordinate `u in [-1, 1]` to a basal rootstock XZ point.
fn topiary_spine_point(u: f32, params: &TopiaryParams) -> (Vec3, Vec3) {
    match params.shape {
        TopiaryShape::Box => (Vec3::new(u * (params.length * 0.40), 0.0, 0.0), Vec3::Z),
        TopiaryShape::BentBox => {
            let theta = radians(params.bend_angle.clamp(-175.0, 175.0));
            if theta.abs() < 0.05 {
                return (Vec3::new(u * (params.length * 0.40), 0.0, 0.0), Vec3::Z);
            }
            let sign = theta.signum();
            let half_theta = theta.abs() * 0.5;
            let arc_r = (params.length / theta.abs()).max(params.width * 0.65);
            let cz = sign * arc_r * (half_theta.cos() * 0.5 + 0.5);
            let ang = u * half_theta * 0.84;
            let x = arc_r * ang.sin();
            let z = cz - sign * arc_r * ang.cos();
            let radial = Vec3::new(ang.sin(), 0.0, -sign * ang.cos()).normalize_or_zero();
            (Vec3::new(x, 0.0, z), radial)
        }
        TopiaryShape::Sphere | TopiaryShape::Cylinder | TopiaryShape::Cone => {
            let r = (params.length.min(params.width) * 0.16).min(0.24);
            let ang = (u + 1.0) * PI;
            (
                Vec3::new(ang.cos() * r * u.abs(), 0.0, ang.sin() * r * u.abs()),
                Vec3::new(ang.cos(), 0.0, ang.sin()),
            )
        }
    }
}

fn project_to_hull(
    start: Vec3,
    dir: Vec3,
    target_sdf: f32,
    params: &TopiaryParams,
    seed: u64,
) -> Vec3 {
    let mut lo = 0.0_f32;
    let mut hi = params.length.max(params.height).max(params.width) * 1.35;
    for _ in 0..14 {
        let mid = (lo + hi) * 0.5;
        if topiary_sdf(start + dir * mid, params, seed) < target_sdf {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    start + dir * ((lo + hi) * 0.5)
}

fn generate_topiary(species: &Species, seed: u64) -> Tree {
    let params = species.topiary.clone().unwrap_or_default();
    let mut rng = Rng::from_seed(seed ^ 0x7091_A271);
    let mut tree = Tree::new(species.species.name.clone(), seed);

    let skirt = params.skirt_clearance.clamp(0.0, params.height * 0.45);
    let body_h = (params.height - skirt).max(0.15);

    // Build rootstock woody stems along the basal spine (`level 0` anchor + `level 1..2` woody scaffold)
    let root_count = match params.shape {
        TopiaryShape::Box | TopiaryShape::BentBox => {
            ((params.length / 0.44).round() as usize).clamp(2, 14)
        }
        TopiaryShape::Cylinder => ((params.length / 0.55).round() as usize).clamp(3, 8),
        TopiaryShape::Sphere | TopiaryShape::Cone => 3,
    };

    let base_r = species.trunk.radius.clamp(0.018, 0.055);
    let mut clump_roots: Vec<(u32, Vec3)> = Vec::new();

    for r_idx in 0..root_count {
        let u = if root_count == 1 {
            0.0
        } else {
            (r_idx as f32 / (root_count - 1) as f32) * 2.0 - 1.0
        };
        let (root_origin, _) = topiary_spine_point(u, &params);
        let jitter_xz = Vec3::new(rng.range(-0.04, 0.04), 0.0, rng.range(-0.04, 0.04));
        let p0 = root_origin + jitter_xz;
        let top_y = (skirt + body_h * rng.range(0.68, 0.84)).min(params.height - 0.09);

        let mut leader_pts = Vec::with_capacity(6);
        for s in 0..=5 {
            let t = s as f32 / 5.0;
            let mut pt = p0
                + Vec3::new(
                    (t * PI * 1.6 + r_idx as f32).sin() * 0.045,
                    top_y * t,
                    (t * PI * 1.4 + r_idx as f32 * 1.7).cos() * 0.045,
                );
            // Pull interior leader safely inside sheared volume above skirt
            if pt.y > skirt + 0.06 {
                let d = topiary_sdf(pt, &params, seed);
                if d > -0.07 {
                    let g = topiary_gradient(pt, &params, seed);
                    pt -= g * (d + 0.075);
                }
            }
            leader_pts.push(pt);
        }

        let level = if r_idx == 0 { 0 } else { 1 };
        let parent = if r_idx == 0 { None } else { Some(0) };
        let root_id = push_polyline_stem(
            &mut tree,
            level,
            parent,
            0.0,
            &leader_pts,
            base_r * rng.variance_mul(0.15),
            base_r * 0.28,
        );
        let center_pt = leader_pts[3];
        clump_roots.push((root_id, center_pt));

        // 5 crooked woody interior twigs radiating from each rootstock toward the shear hull
        for b in 0..5 {
            let t_attach = 0.22 + 0.16 * b as f32;
            let seg_i = ((t_attach * 5.0) as usize).min(4);
            let origin = leader_pts[seg_i];
            let az = b as f32 * 2.399_963 + r_idx as f32 * 0.9 + rng.range(-0.25, 0.25);
            let el = rng.range(0.18, 0.72);
            let ray = Vec3::new(az.cos() * el.cos(), el.sin(), az.sin() * el.cos()).normalize();
            let raw_tip = project_to_hull(origin, ray, -0.065, &params, seed);
            let max_twig_len = match params.shape {
                TopiaryShape::Box | TopiaryShape::BentBox => {
                    (params.width * 0.48).max(body_h * 0.38)
                }
                _ => params.length.max(body_h) * 0.55,
            };
            let delta = raw_tip - origin;
            let tip = if delta.length() > max_twig_len {
                origin + delta.normalize_or_zero() * max_twig_len
            } else {
                raw_tip
            };
            if (tip - origin).length() > 0.08 && topiary_sdf(tip, &params, seed) < -0.03 {
                let mid = origin.lerp(tip, 0.52)
                    + Vec3::new(rng.range(-0.025, 0.025), 0.02, rng.range(-0.025, 0.025));
                let twig_id = push_polyline_stem(
                    &mut tree,
                    2,
                    Some(root_id),
                    t_attach,
                    &[origin, mid, tip],
                    base_r * 0.42,
                    base_r * 0.14,
                );
                if let Some(parent_stem) = tree.stems.iter_mut().find(|s| s.id == root_id) {
                    parent_stem.child_ids.push(twig_id);
                }
            }
        }
    }
    if root_count > 1 {
        let child_ids: Vec<u32> = clump_roots.iter().skip(1).map(|(id, _)| *id).collect();
        if let Some(trunk) = tree.stems.first_mut() {
            trunk.child_ids.extend(child_ids);
        }
    }

    // Sample surface + deep core + new-growth breakout Buxus sprigs around the sheared hull
    let total_leaves = species.leaves.count.max(64) as usize;
    let base_size = species.leaves.size.max(0.08);

    let nearest_clump_stem = |pos: Vec3| -> u32 {
        clump_roots
            .iter()
            .min_by(|a, b| {
                (pos - a.1)
                    .length_squared()
                    .partial_cmp(&(pos - b.1).length_squared())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(id, _)| *id)
            .unwrap_or(0)
    };

    for i in 0..total_leaves {
        // Choose spine anchor so elongated & curved hedges get completely uniform surface coverage
        let frac = (i as f32 + rng.range(0.15, 0.85)) / total_leaves as f32;
        let u_spine = match params.shape {
            TopiaryShape::Box | TopiaryShape::BentBox => {
                // Slightly denser near end-caps too
                (frac * 2.0 - 1.0) * 0.92
            }
            _ => 0.0,
        };
        let (spine_xz, _) = topiary_spine_point(u_spine, &params);
        let anchor = Vec3::new(spine_xz.x, skirt + body_h * 0.50, spine_xz.z);

        // Golden-sphere / stratified ray from spine anchor
        let golden_az = i as f32 * 2.399_963 + rng.range(-0.22, 0.22);
        let z_cos = match params.shape {
            TopiaryShape::Box | TopiaryShape::BentBox => {
                // Allocate ~34% to top deck, ~58% to vertical walls, ~8% to lower tuck
                let r = rng.next_f32();
                if r < 0.34 {
                    rng.range(0.55, 0.995)
                } else if r < 0.92 {
                    rng.range(-0.28, 0.55)
                } else {
                    rng.range(-0.68, -0.28)
                }
            }
            _ => rng.range(-0.58, 0.995),
        };
        let r_xy = (1.0 - z_cos * z_cos).max(0.0).sqrt();
        let mut ray = Vec3::new(golden_az.cos() * r_xy, z_cos, golden_az.sin() * r_xy);
        if matches!(params.shape, TopiaryShape::Box | TopiaryShape::BentBox) && i % 7 != 0 {
            // Keep interior spine rays predominantly transverse/upward except end-cap rays
            if u_spine.abs() < 0.80 {
                ray.x *= 0.45;
                ray = ray.try_normalize().unwrap_or(Vec3::Y);
            }
        }

        // Layer selection: 28% deep core shadow shell, 56% sheared mantle, 16% new-growth shoots
        let roll = (i * 13) % 25;
        let (target_sdf, scale_mul) = if roll < 7 {
            (rng.range(-0.125, -0.055), rng.range(1.12, 1.28))
        } else if roll < 21 {
            let jitter = (1.0 - params.trim_tightness * 0.65) * 0.024;
            (rng.range(-0.026, 0.014 + jitter), rng.range(0.92, 1.08))
        } else {
            let breakout = 0.018 + (1.0 - params.trim_tightness * 0.72) * 0.048;
            (rng.range(0.012, breakout), rng.range(0.74, 0.90))
        };

        let mut pos = project_to_hull(anchor, ray, target_sdf, &params, seed);
        pos.y = pos.y.max(skirt * 0.55 + 0.03);

        let n_hull = topiary_gradient(pos, &params, seed);
        // Side sprigs angle skyward (+Y) while presenting their glossy faces outward along n_hull
        let tip_dir = if n_hull.y > 0.72 {
            let tang = Vec3::new(golden_az.cos(), 0.55, golden_az.sin()).normalize();
            (n_hull * 0.52 + tang * 0.48).normalize()
        } else {
            (n_hull * 0.58 + Vec3::Y * (0.52 + 0.25 * species.leaves.up_influence)).normalize()
        };
        let face_nrm = (n_hull * 0.80 + Vec3::Y * 0.20).normalize();
        let rotation = orient_leaf(tip_dir, face_nrm, rng.range(-0.45, 0.45));
        let scale = base_size * scale_mul * rng.variance_mul(species.leaves.size_variance * 0.6);
        let stem_id = nearest_clump_stem(pos);

        tree.leaves.push(Leaf {
            position: pos - tip_dir * (scale * 0.36),
            rotation,
            scale,
            stem_id,
        });
    }

    tree
}

// ============================================================================
// 2. ORNAMENTAL CAFE FLORALS & HANGING CASCADES (`GeneratorFamily::FloralShrub`)
// ============================================================================

/// Orient a zygomorphic Pansy corolla so local `+Z` faces `face_normal` and local `+Y`
/// (the 2 upper banner petals) points gravity-upward (`+Y`), never upside-down.
fn orient_blossom(face_normal: Vec3, roll_jitter: f32) -> Quat {
    let mut fwd = face_normal.try_normalize().unwrap_or(Vec3::Z);
    // Real Viola gooseneck peduncles always nod blossoms at least ~28 deg off zenith
    if fwd.y > 0.88 {
        let xz = Vec2::new(fwd.x, fwd.z).try_normalize().unwrap_or(Vec2::Y);
        fwd = Vec3::new(xz.x * 0.48, 0.877, xz.y * 0.48).normalize();
    }
    let right = Vec3::Y.cross(fwd).try_normalize().unwrap_or(Vec3::X);
    let up = fwd.cross(right).normalize_or_zero();
    let base = Quat::from_mat3(&glam::Mat3::from_cols(right, up, fwd));
    Quat::from_axis_angle(fwd, roll_jitter) * base
}

fn floral_mound_point(
    idx: usize,
    hx: f32,
    hz: f32,
    h_top: f32,
    is_cascade: bool,
    habit: FloralHabit,
    rng: &mut Rng,
) -> (Vec3, Vec3, f32, f32) {
    let az = idx as f32 * 2.399_963 + rng.range(-0.20, 0.20);
    let rad_u = ((idx as f32 * 0.754_877_7).fract() * 0.88 + rng.range(0.0, 0.12)).clamp(0.0, 1.0);
    // Uniform area-preserving disk sampling (`sqrt`) keeps crown center & outer rim equally dense
    let u_rad = rad_u.sqrt() * 1.02;

    let (px, pz, edge_r) = match habit {
        FloralHabit::Trough => {
            let ux = ((idx as f32 * 0.618_034).fract() * 2.0 - 1.0) * 0.96 + rng.range(-0.06, 0.06);
            let uz =
                ((idx as f32 * 0.414_213_6).fract() * 2.0 - 1.0) * 0.96 + rng.range(-0.06, 0.06);
            let rx = ux.clamp(-1.03, 1.03) * hx;
            let rz = uz.clamp(-1.03, 1.03) * hz;
            let er = ((rx / hx).powi(2) * 0.65 + (rz / hz).powi(2))
                .sqrt()
                .clamp(0.0, 1.05);
            (rx, rz, er)
        }
        _ => {
            let rx = az.cos() * u_rad * hx;
            let rz = az.sin() * u_rad * hz;
            (rx, rz, u_rad)
        }
    };

    // Broad superellipse florist pillow: wide, gently rounded crown shoulders dropping all the
    // way to `rim_y` (`dome = 0` at `edge_r = 1.0`) — zero pointy cone top and zero rim gap!
    let dome = (1.0 - edge_r.clamp(0.0, 1.0).powf(2.35))
        .max(0.0)
        .powf(0.62);
    let rim_y = if is_cascade { -0.16 } else { 0.012 };
    let y = lerp(rim_y, h_top, dome);
    let pos = Vec3::new(px, y, pz);
    let out_nrm = Vec3::new(
        px / (hx * hx).max(0.02),
        ((y - rim_y).max(0.05)) / (h_top * h_top).max(0.02) + 0.34,
        pz / (hz * hz).max(0.02),
    )
    .try_normalize()
    .unwrap_or(Vec3::Y);

    (pos, out_nrm, edge_r, az)
}

fn generate_floral_shrub(species: &Species, seed: u64) -> Tree {
    let params = species.floral.clone().unwrap_or_default();
    let mut rng = Rng::from_seed(seed ^ 0xF102_A155);
    let mut tree = Tree::new(species.species.name.clone(), seed);

    let hx = (params.length * 0.5).max(0.12);
    let hz = (params.width * 0.5).max(0.12);
    let h_top = params.height.max(0.16);

    // Reserve exact stem IDs 0..=3 so `leaf.stem_id & 3` deterministically selects:
    // 0: Royal Violet & Pearl-Ivory Pansy, 1: Marigold & Mahogany Pansy,
    // 2: Primrose-Butter & Periwinkle Viola, 3: Pointed crenate Viola/Vinca green leaf blade!
    for q in 0..4u32 {
        let ang = q as f32 * (TAU * 0.25) + 0.4;
        let p0 = Vec3::new(ang.cos() * 0.02, 0.02, ang.sin() * 0.02);
        let p1 = Vec3::new(ang.cos() * 0.06, h_top * 0.28, ang.sin() * 0.06);
        let p2 = Vec3::new(ang.cos() * 0.10, h_top * 0.50, ang.sin() * 0.10);
        let id = push_polyline_stem(&mut tree, 0, None, 0.0, &[p0, p1, p2], 0.008, 0.003);
        debug_assert_eq!(id & 3, q);
    }

    // Branching green herbaceous stems supporting the interior of the cushion
    let mound_stems = ((params.length * params.width * 24.0).round() as usize).clamp(12, 28);
    for s_i in 0..mound_stems {
        let az = s_i as f32 * 2.399_963 + rng.range(-0.2, 0.2);
        let r_frac = ((s_i as f32 + 0.5) / mound_stems as f32).sqrt() * 0.72;
        let target_xz = match params.habit {
            FloralHabit::Trough => Vec2::new(rng.range(-0.80, 0.80) * hx, az.sin() * r_frac * hz),
            _ => Vec2::new(az.cos() * r_frac * hx, az.sin() * r_frac * hz),
        };
        let dome_y = h_top
            * (1.0 - 0.58 * (target_xz.x / hx).powi(2).max((target_xz.y / hz).powi(2)))
            * 0.66;
        let base = Vec3::new(target_xz.x * 0.20, 0.02, target_xz.y * 0.20);
        let mid = Vec3::new(target_xz.x * 0.60, dome_y * 0.62, target_xz.y * 0.60);
        let tip = Vec3::new(target_xz.x, dome_y.max(0.06), target_xz.y);
        push_polyline_stem(
            &mut tree,
            1,
            Some(3),
            0.3,
            &[base, mid, tip],
            0.0048,
            0.0018,
        );
    }

    // 15 intimate multi-variety nursery tufts (3 inner crown + 12 mid/outer ring) cycling
    // Violet (0), Marigold (1), and Primrose (2) so every 360-deg view shows all 3 varieties
    // in natural 4-7 bloom plant clusters rather than segregated half-pot blocks!
    let mut tufts: Vec<(Vec2, u32)> = Vec::with_capacity(15);
    let base_off = (seed as u32) % 3;
    for k in 0..3u32 {
        let a = (k as f32 / 3.0) * TAU + 0.25 + rng.range(-0.18, 0.18);
        let r = rng.range(0.20, 0.32);
        tufts.push((
            Vec2::new(a.cos() * r * hx, a.sin() * r * hz),
            (base_off + k) % 3,
        ));
    }
    for k in 0..12u32 {
        let a = (k as f32 / 12.0) * TAU + 0.12 + rng.range(-0.12, 0.12);
        let r = if k % 2 == 0 {
            rng.range(0.52, 0.68)
        } else {
            rng.range(0.72, 0.88)
        };
        tufts.push((
            Vec2::new(a.cos() * r * hx, a.sin() * r * hz),
            (base_off + k + 1) % 3,
        ));
    }

    let pick_variety = |px: f32, pz: f32, rng: &mut Rng| -> u32 {
        if rng.next_f32() < 0.22 {
            return rng.index(3) as u32;
        }
        let p =
            Vec2::new(px, pz) + Vec2::new(rng.range(-0.07, 0.07) * hx, rng.range(-0.07, 0.07) * hz);
        tufts
            .iter()
            .min_by(|a, b| {
                (p - a.0)
                    .length_squared()
                    .partial_cmp(&(p - b.0).length_squared())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|&(_, q)| q)
            .unwrap_or(0)
    };

    let total_organs = species.leaves.count.max(64) as usize;
    let base_size = species.leaves.size.clamp(0.045, 0.28);
    let is_cascade = params.habit == FloralHabit::Cascade && params.cascade_drop > 0.08;
    let mound_share = if is_cascade { 0.34 } else { 1.0 };
    let mound_organs = ((total_organs as f32 * mound_share).round() as usize).max(36);
    // Keep ~26% blossoms to ~74% pointed-crenate Viola leaves so lush emerald foliage frames
    // every bouquet instead of burying the mound under pancake-stacked flower wafers!
    let blossom_share = (params.blossom_density * 0.38).clamp(0.15, 0.34);
    let mound_blossoms = ((mound_organs as f32 * blossom_share).round() as usize).max(12);
    let mound_foliage = mound_organs.saturating_sub(mound_blossoms).max(24);

    // 1. Multi-layer phyllotactic 3D green Viola rosette cushion + emergent framing leaves
    for i in 0..mound_foliage {
        let (surf_pos, out_nrm, edge_r, az) =
            floral_mound_point(i, hx, hz, h_top, is_cascade, params.habit, &mut rng);
        let layer = i % 4;
        let depth_offset = if layer == 0 {
            rng.range(-0.048, -0.024) // Broad deep-core shadow-bed leaf sealing interior gaps
        } else if layer == 3 {
            rng.range(-0.004, 0.006) // Outer rosette shingle leaf (kept safely below flower corollas!)
        } else {
            rng.range(-0.020, -0.004) // Mid-rosette shingle leaf
        };
        let blade_center = surf_pos + out_nrm * depth_offset;

        let horiz_out = {
            let swirl = az + rng.range(-0.42, 0.42);
            Vec3::new(swirl.cos(), 0.0, swirl.sin())
        };
        let pitch_y = lerp(0.58, -0.26, (edge_r / 1.02).clamp(0.0, 1.0)) + rng.range(-0.12, 0.12);
        let tip_dir = (horiz_out * 0.80 + out_nrm * 0.36 + Vec3::Y * pitch_y)
            .try_normalize()
            .unwrap_or(out_nrm);
        let face_nrm = (out_nrm * 0.78 + Vec3::Y * 0.40).normalize();
        let rotation = orient_leaf(tip_dir, face_nrm, rng.range(-0.42, 0.42));

        let size_mul = if layer == 0 {
            rng.range(1.52, 1.84)
        } else if layer == 3 {
            rng.range(1.12, 1.34)
        } else {
            rng.range(1.26, 1.54)
        };
        let scale = base_size * size_mul * rng.variance_mul(species.leaves.size_variance * 0.45);

        let mut petiole_pos = blade_center - tip_dir * (scale * 0.42);
        if !is_cascade {
            petiole_pos.y = petiole_pos.y.max(0.008);
        }

        tree.leaves.push(Leaf {
            position: petiole_pos,
            rotation,
            scale,
            stem_id: 3, // Quadrant 3: pointed crenate Viola/Vinca green leaf blade
        });
    }

    // 2. Blue-noise-spaced 3D Pansy corollas on slender curved green gooseneck peduncles
    let max_mound_peduncles = 160usize;
    let peduncle_stride = (mound_blossoms / max_mound_peduncles.max(1)).max(1);
    let mut placed_blooms: Vec<Vec3> = Vec::with_capacity(mound_blossoms);
    let mut sample_cursor = 0usize;

    for b_i in 0..mound_blossoms {
        // Best-of-3 blue-noise candidate selection prevents flowers from pancake-stacking!
        let mut best_cand = floral_mound_point(
            b_i * 5 + 2,
            hx * 0.95,
            hz * 0.95,
            h_top,
            is_cascade,
            params.habit,
            &mut rng,
        );
        let mut best_d2 = -1.0_f32;
        for _ in 0..3 {
            sample_cursor = sample_cursor.wrapping_add(7);
            let cand = floral_mound_point(
                b_i * 3 + sample_cursor,
                hx * 0.95,
                hz * 0.95,
                h_top,
                is_cascade,
                params.habit,
                &mut rng,
            );
            let min_d2 = placed_blooms
                .iter()
                .rev()
                .take(48)
                .map(|p| (*p - cand.0).length_squared())
                .fold(f32::MAX, f32::min);
            if min_d2 > best_d2 {
                best_d2 = min_d2;
                best_cand = cand;
            }
        }
        let (surf_pos, out_nrm, edge_r, az) = best_cand;

        let horiz_dir = {
            let jitter_az = az + rng.range(-0.75, 0.75);
            Vec3::new(jitter_az.cos(), 0.0, jitter_az.sin())
        };
        let stalk_len = rng.range(0.038, 0.062);
        let calyx_pos = surf_pos + out_nrm * stalk_len + horiz_dir * 0.010;
        placed_blooms.push(calyx_pos);

        if b_i % peduncle_stride == 0 {
            let ped_base = (surf_pos - out_nrm * 0.022).max(Vec3::new(-10.0, -2.0, -10.0));
            let ped_mid = ped_base.lerp(calyx_pos, 0.62) + Vec3::Y * 0.012;
            push_polyline_stem(
                &mut tree,
                2,
                Some(3),
                0.5,
                &[ped_base, ped_mid, calyx_pos],
                0.0022,
                0.0013,
            );
        }

        // Natural varied gooseneck nod & roll so adjacent flowers face organic varied angles
        let nod_y = lerp(0.72, 0.24, (edge_r / 1.02).clamp(0.0, 1.0)) + rng.range(-0.18, 0.18);
        let nod_y = nod_y.clamp(0.12, 0.85);
        let face_nrm =
            (horiz_dir * (1.0 - nod_y * nod_y).max(0.12).sqrt() + out_nrm * 0.38 + Vec3::Y * nod_y)
                .normalize();
        let rotation = orient_blossom(face_nrm, rng.range(-0.52, 0.52));
        let variety_q = pick_variety(surf_pos.x, surf_pos.z, &mut rng);
        let scale = base_size
            * rng.range(0.76, 1.02)
            * rng.variance_mul(species.leaves.size_variance * 0.38);

        tree.leaves.push(Leaf {
            position: calyx_pos,
            rotation,
            scale,
            stem_id: variety_q,
        });
    }

    // 3. Lush herringbone-garland trailing vine cascades (`Paris_flowers_01b/c/d`)
    if is_cascade {
        let streamer_count = 32usize;
        let cascade_budget = total_organs.saturating_sub(tree.leaves.len()).max(160);

        let mut drops = Vec::with_capacity(streamer_count);
        let mut weight_sum = 0.0_f32;
        for st_i in 0..streamer_count {
            // Drape into 5 natural multi-vine balcony swags + shorter rim-bridging cascades
            let swag_wave = ((st_i as f32 / streamer_count as f32) * TAU * 5.0 + 0.4).cos();
            let base_drop = 0.56 + 0.30 * swag_wave + rng.range(-0.16, 0.16);
            let drop_frac = if st_i % 5 == 4 {
                rng.range(0.22, 0.42) // Dedicated upper rim-spillers bridging crown & curtain!
            } else {
                base_drop.clamp(0.32, 1.00)
            };
            let w = drop_frac.powf(0.85);
            weight_sum += w;
            drops.push((drop_frac, w));
        }

        for (st_i, &(drop_frac, w)) in drops.iter().enumerate() {
            let swag_pull = ((st_i as f32 / streamer_count as f32) * TAU * 5.0 + 0.4).sin() * 0.065;
            let ang =
                (st_i as f32 / streamer_count as f32) * TAU + swag_pull + rng.range(-0.07, 0.07);
            let start_rad = if st_i % 2 == 0 {
                rng.range(0.64, 0.84)
            } else {
                rng.range(0.80, 0.97)
            };
            let rim_x = ang.cos() * hx * start_rad;
            let rim_z = ang.sin() * hz * start_rad;
            let out_xz = Vec3::new(ang.cos(), 0.0, ang.sin());
            let side_xz = Vec3::new(-out_xz.z, 0.0, out_xz.x);

            let drop_len = params.cascade_drop * drop_frac;
            let primary_q = pick_variety(rim_x, rim_z, &mut rng);
            let secondary_q = (primary_q + 1 + (st_i as u32 % 2)) % 3;

            // Start vine inside the crown cushion (`y = +0.10..+0.24m`), arch UP-AND-OVER the
            // planter lip, then drape down in a natural catenary streamer — ZERO bald collar!
            let segs = 9usize;
            let start_y = lerp(h_top * 0.56, h_top * 0.18, start_rad);
            let mut v_pts = Vec::with_capacity(segs + 1);
            for s in 0..=segs {
                let t = s as f32 / segs as f32;
                let lip_arch = (hx.min(hz) * 0.24) * (t * PI * 0.76).sin() + 0.04 * t.sqrt();
                let sway = 0.038 * (t * TAU * 1.2 + st_i as f32 * 1.61).sin() * t;
                let crest = 0.06 * (t * PI).sin() * (1.0 - t);
                let y = lerp(start_y, -drop_len, t.powf(1.08)) + crest;
                v_pts.push(Vec3::new(rim_x, y, rim_z) + out_xz * lip_arch + side_xz * sway);
            }
            push_polyline_stem(&mut tree, 2, Some(3), 0.8, &v_pts, 0.0040, 0.0013);

            let vine_budget =
                ((cascade_budget as f32 * (w / weight_sum.max(1e-4))).round() as usize).max(10);
            let bloom_share = (params.blossom_density * 0.34).clamp(0.14, 0.26);
            let n_blooms = ((vine_budget as f32 * bloom_share).round() as usize).max(2);
            let n_leaves = vine_budget.saturating_sub(n_blooms).max(8);
            let n_pairs = (n_leaves / 2).max(4);

            // Opposite / sub-opposite herringbone pairs of pointed Vinca/Viola leaves rotating
            // gently around the vine axis so streamers read as lush 3D leafy garlands, never coins!
            for p_i in 0..n_pairs {
                let t_base = (p_i as f32 + 0.35) / n_pairs as f32;
                let twist_az = p_i as f32 * 0.92 + st_i as f32 * 0.7;
                let pair_lat = side_xz * twist_az.cos() + out_xz * (twist_az.sin() * 0.45);

                for side_s in [-1.0_f32, 1.0_f32] {
                    if tree.leaves.len() >= total_organs + 32 {
                        break;
                    }
                    let t = (t_base + side_s * (0.18 / n_pairs as f32) + rng.range(-0.01, 0.01))
                        .clamp(0.01, 0.995);
                    let f_idx = t * segs as f32;
                    let i0 = (f_idx.floor() as usize).min(segs - 1);
                    let ft = f_idx - i0 as f32;
                    let p_vine = v_pts[i0].lerp(v_pts[i0 + 1], ft);
                    let tang = (v_pts[i0 + 1] - v_pts[i0])
                        .try_normalize()
                        .unwrap_or(-Vec3::Y);

                    let lat = (pair_lat * side_s)
                        .try_normalize()
                        .unwrap_or(side_xz * side_s);
                    let splay = lerp(0.68, 0.46, t);
                    let tip_dir = (tang * 0.72 + lat * splay + out_xz * 0.30).normalize();
                    let face_out = (out_xz * 0.80 + lat * 0.36 + Vec3::Y * 0.32).normalize();
                    let rotation = orient_leaf(tip_dir, face_out, rng.range(-0.28, 0.28));

                    // Keep leaves large enough relative to node spacing so blades always shingle!
                    let taper = lerp(1.38, 0.56, t.powf(0.95));
                    let scale = base_size * taper * rng.variance_mul(0.12);

                    tree.leaves.push(Leaf {
                        position: p_vine + out_xz * 0.005 - tip_dir * (scale * 0.16),
                        rotation,
                        scale,
                        stem_id: 3,
                    });
                }
            }

            // Nodding 3D Pansy blossoms perched proud of the herringbone leaf garland
            for b_k in 0..n_blooms {
                if tree.leaves.len() >= total_organs + 48 {
                    break;
                }
                // Concentrate blooms along upper/mid 88% of vine, tapering size toward tip
                let u_b = (b_k as f32 + rng.range(0.18, 0.82)) / n_blooms as f32;
                let t = (u_b.powf(1.12) * 0.88).clamp(0.02, 0.90);
                let f_idx = t * segs as f32;
                let i0 = (f_idx.floor() as usize).min(segs - 1);
                let ft = f_idx - i0 as f32;
                let p_vine = v_pts[i0].lerp(v_pts[i0 + 1], ft);

                let side_s = if b_k % 2 == 0 { 1.0 } else { -1.0 };
                let lat = side_xz * side_s;
                let calyx = p_vine
                    + out_xz * rng.range(0.036, 0.052)
                    + lat * rng.range(0.010, 0.026)
                    + Vec3::Y * 0.006;

                let face_nrm =
                    (out_xz * 0.84 + lat * rng.range(0.18, 0.38) + Vec3::Y * lerp(0.42, 0.18, t))
                        .normalize();
                let rotation = orient_blossom(face_nrm, rng.range(-0.44, 0.44));
                let q_id = if rng.next_f32() < 0.72 {
                    primary_q
                } else {
                    secondary_q
                };
                let taper = lerp(0.92, 0.56, t.powf(1.05));
                let scale = base_size * taper * rng.variance_mul(0.12);

                tree.leaves.push(Leaf {
                    position: calyx,
                    rotation,
                    scale,
                    stem_id: q_id,
                });
            }
        }
    }

    tree
}

// ============================================================================
// 3. WALL-CLIMBING FACADE IVY (`GeneratorFamily::ClimbingVine`)
// ============================================================================

fn generate_climbing_vine(species: &Species, seed: u64) -> Tree {
    let params = species.vine.clone().unwrap_or_default();
    let mut rng = Rng::from_seed(seed ^ 0x1710_FA4E);
    let mut tree = Tree::new(species.species.name.clone(), seed);

    let half_w = (params.width * 0.5).max(0.25);
    let height = params.height.max(0.5);
    let depth = params.depth.max(0.15);
    let main_n = params.main_vines.clamp(1, 16) as usize;
    let base_r = species.trunk.radius.clamp(0.014, 0.055);

    let mut leaf_carrier_stems: Vec<u32> = Vec::new();

    for m_i in 0..main_n {
        let u = if main_n == 1 {
            0.0
        } else {
            (m_i as f32 / (main_n - 1) as f32) * 2.0 - 1.0
        };
        let x_start = u * half_w * 0.24 + rng.range(-0.06, 0.06);
        let x_target = u * half_w * 0.92 + rng.range(-half_w * 0.10, half_w * 0.10);
        // Stagger climbing arm heights so upper facade reads as organic reaching tendrils, not a flat box
        let arch_profile = 1.0 - 0.26 * u.abs().powf(1.3);
        let climb_h = height * arch_profile * rng.range(0.84, 1.02);

        let segs = 12usize;
        let mut pts = Vec::with_capacity(segs + 1);
        for s in 0..=segs {
            let t = s as f32 / segs as f32;
            let snake = (t * TAU * 1.65 + m_i as f32 * 1.4).sin() * (half_w * 0.13) * t.sqrt();
            let x = lerp(x_start, x_target, t.powf(0.82)) + snake;
            let y = climb_h * t;
            // Hug the facade wall at z = 0.025..0.075 with gnarled woody relief
            let z = 0.032 + 0.028 * (t * TAU * 2.3 + m_i as f32).cos().abs();
            pts.push(Vec3::new(x, y, z));
        }

        let level = if m_i == 0 { 0 } else { 1 };
        let parent = if m_i == 0 { None } else { Some(0) };
        let main_id = push_polyline_stem(
            &mut tree,
            level,
            parent,
            0.0,
            &pts,
            base_r * rng.variance_mul(0.16),
            base_r * 0.22,
        );
        if m_i > 0
            && let Some(tr) = tree.stems.first_mut()
        {
            tr.child_ids.push(main_id);
        }
        leaf_carrier_stems.push(main_id);

        // Lateral wall-clinging secondary shoots fanning across the masonry with open breathing gaps
        let laterals = 3usize;
        for l_i in 0..laterals {
            let t_attach = 0.22 + 0.26 * l_i as f32 + rng.range(-0.04, 0.04);
            let idx = ((t_attach * segs as f32) as usize).min(segs - 1);
            let origin = pts[idx];
            let side = if (l_i + m_i) % 2 == 0 { 1.0 } else { -1.0 };
            let reach_x = (origin.x + side * half_w * rng.range(0.18, 0.34))
                .clamp(-half_w * 0.98, half_w * 0.98);
            let reach_y = (origin.y + height * rng.range(0.08, 0.24)).min(height * 1.02);
            let mut l_pts = Vec::with_capacity(6);
            for ls in 0..=5 {
                let lt = ls as f32 / 5.0;
                let lx = lerp(origin.x, reach_x, lt);
                let ly =
                    lerp(origin.y, reach_y, lt) + 0.06 * (lt * PI).sin() * rng.range(-0.6, 1.0);
                let lz = lerp(origin.z, 0.045 + depth * 0.26 * lt, lt);
                l_pts.push(Vec3::new(lx, ly.max(0.04), lz));
            }
            let lat_id = push_polyline_stem(
                &mut tree,
                2,
                Some(main_id),
                t_attach,
                &l_pts,
                base_r * 0.46,
                base_r * 0.13,
            );
            if let Some(p_stem) = tree.stems.iter_mut().find(|s| s.id == main_id) {
                p_stem.child_ids.push(lat_id);
            }
            leaf_carrier_stems.push(lat_id);
        }
    }

    // Romantic pendant aerial ivy tendrils arching outward (+Z) & draping downward (-Y)
    let runners = params.hanging_runners.min(24) as usize;
    let carrier_snapshot = leaf_carrier_stems.clone();
    for r_i in 0..runners {
        let parent_id = carrier_snapshot[r_i % carrier_snapshot.len()];
        let Some(p_stem) = tree.get_stem(parent_id) else {
            continue;
        };
        let t_attach = rng.range(0.42, 0.88);
        let origin = p_stem.point_at(t_attach);
        if origin.y < height * 0.28 {
            continue;
        }
        let drop = (height * rng.range(0.16, 0.30)).min(origin.y - 0.12);
        let out_z = depth * rng.range(0.55, 0.95);
        let drift_x = rng.range(-0.14, 0.14);
        let mut r_pts = Vec::with_capacity(6);
        for rs in 0..=5 {
            let rt = rs as f32 / 5.0;
            let rx = origin.x + drift_x * rt + 0.03 * (rt * TAU).sin();
            let ry = origin.y + 0.04 * (rt * PI).sin() - drop * rt.powf(1.25);
            let rz = origin.z + out_z * (rt * PI * 0.65).sin();
            r_pts.push(Vec3::new(rx, ry.max(0.05), rz));
        }
        let run_id = push_polyline_stem(
            &mut tree,
            3,
            Some(parent_id),
            t_attach,
            &r_pts,
            base_r * 0.24,
            0.0022,
        );
        if let Some(ps) = tree.stems.iter_mut().find(|s| s.id == parent_id) {
            ps.child_ids.push(run_id);
        }
        leaf_carrier_stems.push(run_id);
    }

    // Place outward-shingled 5-lobed Hedera helix leaf sprays facing +Z & skyward
    let total_leaves = species.leaves.count.max(64) as usize;
    let base_size = species.leaves.size.max(0.14);
    let total_len: f32 = leaf_carrier_stems
        .iter()
        .filter_map(|&id| tree.get_stem(id))
        .map(|s| s.length().max(0.1))
        .sum();

    for &sid in &leaf_carrier_stems {
        let Some(stem) = tree.get_stem(sid).cloned() else {
            continue;
        };
        let share = (stem.length().max(0.1) / total_len.max(0.1)) * total_leaves as f32;
        let n_leaves = (share.round() as usize).max(2);
        for k in 0..n_leaves {
            if tree.leaves.len() >= total_leaves {
                break;
            }
            // Leave the bottom 12% of main woody climbing trunks partially exposed at masonry base
            let t_min = if stem.level <= 1 { 0.14 } else { 0.04 };
            let t = lerp(
                t_min,
                0.99,
                (k as f32 + rng.range(0.12, 0.88)) / n_leaves as f32,
            );
            let p_stem = stem.point_at(t);
            let s_dir = stem.direction_at(t);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };

            // Hug masonry facade closely so climbing vine branches read as an architectural lattice
            let out_proj = depth * rng.range(0.12, 0.44) * (1.0 - 0.28 * t);
            let pos = Vec3::new(
                p_stem.x + side * rng.range(0.02, 0.07),
                p_stem.y + rng.range(-0.02, 0.03),
                (p_stem.z + out_proj).max(0.055),
            );

            // Shingled rain-shedding orientation facing directly out from the facade (+Z) + skyward
            let face_nrm = Vec3::new(
                side * 0.14 + (pos.x / half_w) * 0.10,
                0.26 + rng.range(-0.08, 0.12),
                0.95,
            )
            .normalize();
            let tip_dir = if stem.level == 3 {
                (s_dir * 0.76 + Vec3::new(side * 0.20, -0.22, 0.22)).normalize()
            } else {
                (s_dir * 0.62 + Vec3::new(side * 0.52, 0.20, 0.18)).normalize()
            };
            let rotation = orient_leaf(tip_dir, face_nrm, rng.range(-0.22, 0.22));

            let height_taper = lerp(1.12, 0.72, (pos.y / height).clamp(0.0, 1.0));
            let level_taper = if stem.level == 3 { 0.74 } else { 1.0 };
            let scale = base_size
                * height_taper
                * level_taper
                * rng.variance_mul(species.leaves.size_variance * 0.7);

            tree.leaves.push(Leaf {
                position: pos - tip_dir * (scale * 0.22),
                rotation,
                scale,
                stem_id: sid,
            });
        }
    }

    tree
}

// ============================================================================
// 4. POTTED TROPICAL PADDLE CLUMP (`GeneratorFamily::TropicalClump`)
// ============================================================================

fn generate_tropical_clump(species: &Species, seed: u64) -> Tree {
    let mut rng = Rng::from_seed(seed ^ 0x572E_117A);
    let mut tree = Tree::new(species.species.name.clone(), seed);

    // Short central pseudostem collar at soil level (0, 0, 0)
    let collar_h = (species.trunk.height * 0.16).clamp(0.08, 0.26);
    let collar_r = species.trunk.radius.clamp(0.022, 0.065);
    let collar_id = push_polyline_stem(
        &mut tree,
        0,
        None,
        0.0,
        &[
            Vec3::ZERO,
            Vec3::new(0.0, collar_h * 0.5, 0.0),
            Vec3::new(0.0, collar_h, 0.0),
        ],
        collar_r,
        collar_r * 0.72,
    );

    let petiole_count = species
        .branches
        .level1
        .as_ref()
        .map(|b| b.count as usize)
        .unwrap_or(18)
        .clamp(7, 36);
    let petiole_base_len = species
        .branches
        .level1
        .as_ref()
        .map(|b| b.length)
        .unwrap_or(species.trunk.height * 0.62)
        .max(0.25);

    // Organize petioles into 3 distichous Strelitzia fan clusters rotated in azimuth
    let fans = 3usize;
    let fan_base_yaw = rng.range(0.0, TAU);

    for i in 0..petiole_count {
        let fan_idx = i % fans;
        let rank_in_fan = i / fans;
        let ranks_total = petiole_count.div_ceil(fans).max(2);
        // rank_t: 0 = youngest upright central leaf, 1 = oldest wide-sweeping outer leaf
        let rank_t = rank_in_fan as f32 / (ranks_total - 1) as f32;
        let side = if rank_in_fan.is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };

        let fan_yaw = fan_base_yaw + fan_idx as f32 * (TAU / fans as f32) + rng.range(-0.16, 0.16);
        let fan_origin = Vec3::new(fan_yaw.cos() * 0.045, 0.0, fan_yaw.sin() * 0.045);

        // Distichous alternating splay within each fan + slight 3D azimuth spread so blades never z-fight
        let az = fan_yaw + side * (0.20 + 0.34 * rank_t) + rng.range(-0.14, 0.14);
        let tilt = lerp(0.20, 0.76, rank_t) + rng.range(-0.05, 0.05); // radians from +Y vertical
        let pet_len = petiole_base_len * lerp(0.96, 0.66, rank_t) * rng.variance_mul(0.10);

        let radial_xz = Vec3::new(az.cos(), 0.0, az.sin());
        let segs = 8usize;
        let mut pts = Vec::with_capacity(segs + 1);
        let mut cur = fan_origin + Vec3::Y * (0.02 + 0.04 * (1.0 - rank_t));
        pts.push(cur);
        for s in 1..=segs {
            let t = s as f32 / segs as f32;
            // Smooth upward launch curving gracefully outward toward the blade collar
            let theta = lerp(tilt * 0.38, tilt * 1.18, t.powf(1.1));
            let dir = (Vec3::Y * theta.cos() + radial_xz * theta.sin()).normalize();
            cur += dir * (pet_len / segs as f32);
            pts.push(cur);
        }

        let r0 = (collar_r * 0.42 * lerp(0.88, 1.08, rank_t)).clamp(0.010, 0.024);
        let r1 = (r0 * 0.46).max(0.0055);
        let pet_id = push_polyline_stem(&mut tree, 1, Some(collar_id), 0.25, &pts, r0, r1);
        if let Some(tr) = tree.stems.first_mut() {
            tr.child_ids.push(pet_id);
        }

        // Anchor continuous 3D V-keeled paddle blade seamlessly at petiole collar (`t = 0.94`)
        let Some(pet_stem) = tree.get_stem(pet_id) else {
            continue;
        };
        let blade_start = pet_stem.point_at(0.94);
        let blade_dir = pet_stem.direction_at(0.98);
        let sky_in_plane = (Vec3::Y - blade_dir * Vec3::Y.dot(blade_dir))
            .try_normalize()
            .unwrap_or_else(|| create_basis(blade_dir).1);
        let rotation = orient_leaf(blade_dir, sky_in_plane, rng.range(-0.16, 0.16));
        let blade_len = species.leaves.size
            * lerp(0.94, 1.08, (rank_t * PI).sin())
            * rng.variance_mul(species.leaves.size_variance * 0.65);

        tree.leaves.push(Leaf {
            position: blade_start,
            rotation,
            scale: blade_len,
            stem_id: pet_id,
        });
    }

    tree
}

#[cfg(test)]
mod tests {
    use crate::{
        ExportConfig, Species, export_lod_meshes_to_bytes, generate_lod_meshes, generate_tree,
    };

    #[test]
    fn all_topiary_shapes_generate_and_export() {
        for shape in ["box", "bent_box", "sphere", "cylinder", "cone"] {
            let toml = format!(
                r#"
[species]
name = "Topiary {shape}"

[generator]
family = "topiary"

[trunk]
height = 1.1
radius = 0.035

[topiary]
shape = "{shape}"
length = 1.6
width = 0.7
height = 1.1
bend_angle = 90.0

[leaves]
count = 240
size = 0.30

[textures]
resolution = 128
leaf_card = "buxus"
"#
            );
            let species = Species::from_toml(&toml).unwrap();
            let tree = generate_tree(&species, 42).unwrap();
            assert!(!tree.stems.is_empty());
            assert!(!tree.leaves.is_empty());
            assert!(tree.bounds.is_valid());
            let lods = generate_lod_meshes(&tree, &species);
            let glb = export_lod_meshes_to_bytes(&lods, &ExportConfig::default()).unwrap();
            assert_eq!(&glb[0..4], b"glTF");
        }
    }

    #[test]
    fn floral_vine_and_tropical_clump_families_generate_and_export() {
        for toml in [
            r#"
[species]
name = "Floral Cascade Test"
[generator]
family = "floral_shrub"
[trunk]
height = 0.45
radius = 0.015
[floral]
habit = "cascade"
length = 0.8
width = 0.7
height = 0.45
cascade_drop = 1.2
[leaves]
count = 180
size = 0.26
[textures]
resolution = 128
leaf_card = "floral"
"#,
            r#"
[species]
name = "Ivy Wall Test"
[generator]
family = "climbing_vine"
[trunk]
height = 3.2
radius = 0.03
[vine]
width = 2.4
height = 3.2
depth = 0.3
main_vines = 4
hanging_runners = 4
[leaves]
count = 180
size = 0.36
shape = "ivy"
[textures]
resolution = 128
leaf_shape = "ivy"
"#,
            r#"
[species]
name = "Bird of Paradise Test"
[generator]
family = "tropical_clump"
[trunk]
height = 1.1
radius = 0.04
[branches.level1]
count = 12
length = 0.65
[leaves]
count = 12
size = 0.72
shape = "paddle"
[textures]
resolution = 128
leaf_shape = "paddle"
leaf_card = "single"
"#,
        ] {
            let species = Species::from_toml(toml).unwrap();
            let tree = generate_tree(&species, 7).unwrap();
            assert!(!tree.stems.is_empty());
            assert!(!tree.leaves.is_empty());
            assert!(tree.bounds.is_valid());
            let lods = generate_lod_meshes(&tree, &species);
            assert!(lods.meshes[0].stats.triangle_count > 0);
        }
    }
}
