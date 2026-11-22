//! Rosette generator (`generator.family = "rosette"`).
//!
//! A rosette plant is an optional trunk carrying one or more crowns of long
//! blades: grass trees (*Xanthorrhoea*), cycads, strap-leaf clumps such as
//! kangaroo paw and sedges, and feather palms. The trunk may fork into arms,
//! each crown may wear a skirt of dead blades, and may carry a flower spike.
//!
//! Output is the shared [`Tree`]: stems for trunk and arms, live blades as
//! leaves (drawn as curved ribbons, see [`generate_blade_ribbons`]), dead
//! blades and spikes as organs. See `docs/botanical-generators.md`.

use crate::{
    botanical_cards::{CELL_AGED, CELL_FLOWER, CELL_ORGAN},
    constants::*,
    generation::{GenerationBudget, GenerationError, GenerationEstimate},
    leaves::LeafConfig,
    math::*,
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    organs::{Organ, OrganKind, ribbon_spine, ribbon_tangent},
    rng::Rng,
    species::{BladeCard, RosetteParams, Species, SpikeStyle},
    tree::{Leaf, Segment, Stem, Tree},
};
use glam::{Quat, Vec2, Vec3};

/// Ribbon parameters the leaf mesher needs to draw rosette blades.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BladeRibbon {
    /// Ribbon width in metres.
    pub width: f32,
    /// Gravity bend (`0` straight .. `1` strongly arching).
    pub droop: f32,
    /// Rigidity (`0` limp .. `1` rigid).
    pub stiffness: f32,
    /// V fold along the midrib (`0` flat .. `1` deep).
    pub keel: f32,
    /// Rows of quads along the blade.
    pub rows: usize,
    /// Width multiplier that keeps coverage when a LOD thins the blades.
    pub widen: f32,
}

impl BladeRibbon {
    /// Ribbon settings for `species` at full detail, or `None` when the
    /// species is not a rosette.
    pub fn from_species(species: &Species) -> Option<Self> {
        if species.generator.family != crate::species::GeneratorFamily::Rosette {
            return None;
        }
        let r = species.rosette.clone().unwrap_or_default();
        Some(Self {
            width: r.blade_width,
            droop: r.droop,
            stiffness: r.stiffness,
            keel: r.keel,
            rows: 6,
            widen: 1.0,
        })
    }
}

/// Worst-case generation cost of a rosette species.
pub(crate) fn estimate_rosette(species: &Species) -> GenerationEstimate {
    let r = species.rosette.clone().unwrap_or_default();
    let crowns = r.crowns.max(1) as u64;
    let spike_organs = r
        .spike
        .as_ref()
        .map(|s| {
            let per = match s.style {
                SpikeStyle::Plume | SpikeStyle::Panicle => 2 + s.flowers.max(6) as u64,
                _ => 2 + s.flowers as u64,
            };
            per * s.count.max(1) as u64
        })
        .unwrap_or(0);
    let runner_blades = (r.runners as u64) * 14;
    GenerationEstimate {
        max_stems: 1 + crowns + r.runners as u64,
        max_leaves: crowns * (r.blades as u64 + r.skirt_blades as u64 + spike_organs)
            + runner_blades,
    }
}

/// Generate a rosette plant under `budget`.
pub(crate) fn generate_rosette(
    species: &Species,
    seed: u64,
    budget: GenerationBudget,
) -> Result<Tree, GenerationError> {
    budget.check_estimate(&estimate_rosette(species))?;
    let params = species.rosette.clone().unwrap_or_default();
    let mut rng = Rng::from_seed(seed ^ 0x805E_77E5_0BAD_5EED);
    let mut tree = Tree::new(species.species.name.clone(), seed);

    let trunk = &species.trunk;
    let height = trunk.height * rng.variance_mul(trunk.height_variance);
    let crowns = params.crowns.max(1);
    let forked = crowns > 1;
    let trunk_len = if forked {
        (height * params.fork_height.clamp(0.05, 0.95)).max(MIN_LENGTH * 2.0)
    } else {
        height
    };

    // Trunk: a plain column that leans by `curve` degrees over its length.
    let lean_az = rng.range(0.0, TAU);
    let lean_axis = Vec3::new(lean_az.cos(), 0.0, lean_az.sin());
    let segments = trunk.segments.max(2);
    let total_bend = radians(trunk.curve + rng.variance_add(trunk.curve_variance));
    let taper = trunk.taper.clamp(0.0, 0.95);
    let frac = trunk_len / height.max(MIN_LENGTH);
    let mut stem = Stem::new(0, 0);
    let mut pos = Vec3::ZERO;
    let mut dir = Vec3::Y;
    for i in 0..segments {
        let t0 = i as f32 / segments as f32;
        let t1 = (i + 1) as f32 / segments as f32;
        dir = rotate_around_axis(dir, lean_axis, total_bend * frac / segments as f32);
        let end = pos + dir * (trunk_len / segments as f32);
        stem.segments.push(Segment {
            start: pos,
            end,
            start_radius: (trunk.radius * (1.0 - taper * t0 * frac)).max(MIN_RADIUS),
            end_radius: (trunk.radius * (1.0 - taper * t1 * frac)).max(MIN_RADIUS),
            direction: dir,
        });
        pos = end;
    }
    let trunk_tip = pos;
    let trunk_dir = dir;
    let trunk_tip_r = stem
        .segments
        .last()
        .map(|s| s.end_radius)
        .unwrap_or(trunk.radius);
    tree.add_stem(stem);

    // Crown sites: the trunk tip, or the tips of the arms it forks into.
    struct Site {
        stem_id: u32,
        tip: Vec3,
        axis: Vec3,
        radius: f32,
    }
    let mut sites = Vec::with_capacity(crowns as usize);
    if forked {
        let arm_len = (height - trunk_len).max(MIN_LENGTH * 4.0);
        let az0 = rng.range(0.0, TAU);
        let (tan, bit) = create_basis(trunk_dir);
        for c in 0..crowns {
            let az = az0 + c as f32 / crowns as f32 * TAU + rng.range(-0.25, 0.25);
            let out = tan * az.cos() + bit * az.sin();
            let angle = radians(params.fork_angle) * rng.range(0.8, 1.2);
            let mut d = (trunk_dir * angle.cos() + out * angle.sin())
                .try_normalize()
                .unwrap_or(trunk_dir);
            let len = arm_len * rng.range(0.75, 1.1);
            let arm_segments = (segments / 2).max(3);
            let r0 = trunk_tip_r * 0.82;
            let r1 = r0 * (1.0 - taper * (1.0 - frac)).max(0.5);
            let id = tree.stems.len() as u32;
            let mut arm = Stem::new(id, 1);
            arm.parent_id = Some(0);
            arm.parent_offset = 1.0;
            let mut p = trunk_tip;
            for i in 0..arm_segments {
                let t0 = i as f32 / arm_segments as f32;
                let t1 = (i + 1) as f32 / arm_segments as f32;
                // Arms leave at the fork angle, then turn back up.
                d = (d + Vec3::Y * (0.9 / arm_segments as f32))
                    .try_normalize()
                    .unwrap_or(d);
                let end = p + d * (len / arm_segments as f32);
                arm.segments.push(Segment {
                    start: p,
                    end,
                    start_radius: lerp(r0, r1, t0).max(MIN_RADIUS),
                    end_radius: lerp(r0, r1, t1).max(MIN_RADIUS),
                    direction: d,
                });
                p = end;
            }
            tree.add_stem(arm);
            if let Some(trunk_stem) = tree.get_stem_mut(0) {
                trunk_stem.child_ids.push(id);
            }
            sites.push(Site {
                stem_id: id,
                tip: p,
                axis: d,
                radius: r1,
            });
        }
    } else {
        sites.push(Site {
            stem_id: 0,
            tip: trunk_tip,
            axis: trunk_dir,
            radius: trunk_tip_r,
        });
    }

    let roll_range = match params.blade_card {
        BladeCard::Tuft => PI * 0.5,
        BladeCard::Single => 0.6,
        BladeCard::Pinnate => 0.18,
        BladeCard::Fan => 0.14,
    };
    let waver = (1.0 - params.stiffness.clamp(0.0, 1.0)) * radians(9.0);

    for site in &sites {
        let (tan, bit) = create_basis(site.axis);
        let az0 = rng.range(0.0, TAU);
        // Older blades leave the stem a little lower: the crown is a short
        // cylinder of leaf bases, not a point.
        let crown_depth = (site.radius * 2.5).clamp(0.01, 0.5);
        let n = params.blades.max(1);
        for i in 0..n {
            let rank = (i as f32 + 0.5) / n as f32;
            let polar = radians(lerp(params.spread_min, params.spread_max, rank.powf(0.85)))
                + rng.variance_add(waver);
            let az = az0 + i as f32 * GOLDEN_ANGLE + rng.variance_add(0.12);
            let out = tan * az.cos() + bit * az.sin();
            let launch = (site.axis * polar.cos() + out * polar.sin())
                .try_normalize()
                .unwrap_or(site.axis);
            let position = site.tip + out * (site.radius * 0.7) - site.axis * (crown_depth * rank);
            let young = 0.72 + 0.28 * (rank / 0.25).clamp(0.0, 1.0);
            let length =
                params.blade_length * young * rng.variance_mul(params.blade_length_variance);
            let rotation = blade_rotation(launch, site.axis, rng.variance_add(roll_range));
            tree.leaves.push(Leaf {
                position,
                rotation,
                scale: length,
                stem_id: site.stem_id,
            });
        }

        // Dead skirt: blades hanging down the stem below the crown.
        if params.skirt > 0.0 && params.skirt_blades > 0 {
            let stem_len = tree
                .get_stem(site.stem_id)
                .map(|s| s.length())
                .unwrap_or(0.0);
            let span = (stem_len * params.skirt.clamp(0.0, 1.0)).max(crown_depth);
            for _ in 0..params.skirt_blades {
                let down = rng.next_f32().powf(1.4) * span;
                let t = if stem_len > MIN_LENGTH {
                    (1.0 - down / stem_len).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let (p, axis, r) = tree
                    .get_stem(site.stem_id)
                    .map(|s| (s.point_at(t), s.direction_at(t), s.radius_at(t)))
                    .unwrap_or((site.tip, site.axis, site.radius));
                let axis = axis.try_normalize().unwrap_or(Vec3::Y);
                let (st, sb) = create_basis(axis);
                let az = rng.range(0.0, TAU);
                let out = st * az.cos() + sb * az.sin();
                let polar = radians(lerp(
                    params.spread_max.max(120.0),
                    172.0,
                    rng.next_f32().sqrt(),
                ));
                let launch = (axis * polar.cos() + out * polar.sin())
                    .try_normalize()
                    .unwrap_or(Vec3::NEG_Y);
                let position = p + out * (r * 0.9);
                let mut length = params.blade_length * rng.range(0.6, 0.95);
                // Never poke through the ground.
                if launch.y < -0.05 {
                    length = length.min(((position.y - 0.02) / -launch.y).max(0.05));
                }
                tree.organs.push(Organ {
                    position,
                    rotation: blade_rotation(launch, axis, rng.variance_add(PI * 0.5)),
                    length,
                    width: params.blade_width * 1.15,
                    kind: OrganKind::Blade,
                    cell: CELL_AGED,
                    v_range: [1.0, 0.0],
                    bend: 0.25,
                    stem_id: site.stem_id,
                });
            }
        }

        // Flower spike(s) from the centre of the crown.
        if let Some(spike) = &params.spike {
            for _ in 0..spike.count.max(1) {
                if rng.next_f32() >= spike.chance {
                    continue;
                }
                let az = rng.range(0.0, TAU);
                let lean = radians(spike.lean) * rng.next_f32().sqrt();
                let out = tan * az.cos() + bit * az.sin();
                let axis = (site.axis * lean.cos() + out * lean.sin())
                    .try_normalize()
                    .unwrap_or(site.axis);
                let rotation = Quat::from_rotation_arc(Vec3::Y, axis);
                let base = site.tip + out * (site.radius * 0.3 * rng.next_f32());
                let organ = |position, length, width, kind, cell, v_range| Organ {
                    position,
                    rotation,
                    length,
                    width,
                    kind,
                    cell,
                    v_range,
                    bend: 0.0,
                    stem_id: site.stem_id,
                };
                match spike.style {
                    SpikeStyle::Cylinder => {
                        let scape = spike.scape_length * rng.range(0.85, 1.1);
                        if scape > MIN_LENGTH {
                            tree.organs.push(organ(
                                base,
                                scape,
                                spike.scape_radius * 2.0,
                                OrganKind::Cylinder,
                                CELL_ORGAN,
                                [1.0, 0.78],
                            ));
                        }
                        tree.organs.push(organ(
                            base + axis * scape,
                            spike.length * rng.range(0.8, 1.15),
                            spike.radius * 2.0,
                            OrganKind::Cylinder,
                            CELL_ORGAN,
                            [0.72, 0.0],
                        ));
                    }
                    SpikeStyle::Cone => {
                        tree.organs.push(organ(
                            base,
                            spike.length * rng.range(0.85, 1.1),
                            spike.radius * 2.0,
                            OrganKind::Ovoid,
                            CELL_ORGAN,
                            [1.0, 0.0],
                        ));
                    }
                    SpikeStyle::Raceme => {
                        let scape = (spike.scape_length + spike.length) * rng.range(0.85, 1.1);
                        tree.organs.push(organ(
                            base,
                            scape,
                            spike.scape_radius * 2.0,
                            OrganKind::Cylinder,
                            CELL_ORGAN,
                            [1.0, 0.0],
                        ));
                        // A one-sided fan of tubular flowers at the top.
                        let (at, ab) = create_basis(axis);
                        let face = rng.range(0.0, TAU);
                        let head = spike.length.min(scape * 0.5);
                        for f in 0..spike.flowers {
                            let k = (f as f32 + 0.5) / spike.flowers.max(1) as f32;
                            let a = face + (k - 0.5) * 1.3 + rng.variance_add(0.12);
                            let side = at * a.cos() + ab * a.sin();
                            let polar = radians(lerp(95.0, 35.0, k)) + rng.variance_add(0.12);
                            let launch = (axis * polar.cos() + side * polar.sin())
                                .try_normalize()
                                .unwrap_or(axis);
                            tree.organs.push(Organ {
                                position: base + axis * (scape - head * (1.0 - k)),
                                rotation: blade_rotation(launch, axis, rng.variance_add(0.5)),
                                length: spike.flower_length * rng.range(0.85, 1.15),
                                width: spike.flower_length * 0.42,
                                kind: OrganKind::Blade,
                                cell: CELL_FLOWER,
                                v_range: [1.0, 0.0],
                                bend: 0.2,
                                stem_id: site.stem_id,
                            });
                        }
                    }
                    SpikeStyle::Plume => {
                        // Slender arching culm topped by a loose, airy spray of small
                        // crossed-card panicle branchlets (rushes, sedges, coastal spinifex).
                        let scape =
                            (spike.scape_length + spike.length * 0.5) * rng.range(0.82, 1.15);
                        tree.organs.push(organ(
                            base,
                            scape,
                            spike.scape_radius * 2.0,
                            OrganKind::Cylinder,
                            CELL_ORGAN,
                            [1.0, 0.72],
                        ));
                        let heads = spike.flowers.max(5);
                        let (at, ab) = create_basis(axis);
                        let head_span = spike.length.max(0.12);
                        for h in 0..heads {
                            let k = (h as f32 + 0.5) / heads as f32;
                            let a = h as f32 * GOLDEN_ANGLE + rng.range(-0.3, 0.3);
                            let side = at * a.cos() + ab * a.sin();
                            let splay = lerp(0.55, 0.18, k) + rng.range(-0.08, 0.08);
                            let launch = (axis * splay.cos() + side * splay.sin())
                                .try_normalize()
                                .unwrap_or(axis);
                            let sz = spike.flower_length.max(spike.radius * 3.0)
                                * lerp(1.15, 0.75, k)
                                * rng.range(0.82, 1.18);
                            tree.organs.push(Organ {
                                position: base
                                    + axis * (scape - head_span * (1.0 - k) * 0.7)
                                    + side * (head_span * 0.18 * (1.0 - k)),
                                rotation: Quat::from_axis_angle(launch, rng.range(0.0, TAU))
                                    * Quat::from_rotation_arc(Vec3::Y, launch),
                                length: sz,
                                width: sz,
                                kind: OrganKind::Card,
                                cell: CELL_FLOWER,
                                v_range: [1.0, 0.0],
                                bend: 0.0,
                                stem_id: site.stem_id,
                            });
                        }
                    }
                    SpikeStyle::Panicle => {
                        // Drooping branched fruit/flower panicles hanging out from beneath
                        // the crown (Canary Island date palm, queen palm).
                        let hang_dir = (out * 0.55 - Vec3::Y * 0.84)
                            .try_normalize()
                            .unwrap_or(Vec3::NEG_Y);
                        let stalk_base =
                            site.tip + out * (site.radius * 0.85) - site.axis * (site.radius * 0.9);
                        let stalk_len = spike.scape_length.max(0.35) * rng.range(0.85, 1.15);
                        tree.organs.push(Organ {
                            position: stalk_base,
                            rotation: Quat::from_rotation_arc(Vec3::Y, hang_dir),
                            length: stalk_len,
                            width: spike.scape_radius * 2.4,
                            kind: OrganKind::Cylinder,
                            cell: CELL_ORGAN,
                            v_range: [1.0, 0.75],
                            bend: 0.0,
                            stem_id: site.stem_id,
                        });
                        let clusters = spike.flowers.max(6);
                        for c_i in 0..clusters {
                            let k = (c_i as f32 + 0.5) / clusters as f32;
                            let p = stalk_base + hang_dir * (stalk_len * (0.35 + 0.65 * k));
                            let sz = spike.length * lerp(0.55, 0.35, k) * rng.range(0.85, 1.15);
                            let hang_q = Quat::from_axis_angle(Vec3::NEG_Y, rng.range(0.0, TAU))
                                * Quat::from_rotation_arc(Vec3::Y, Vec3::NEG_Y);
                            tree.organs.push(Organ {
                                position: p,
                                rotation: hang_q,
                                length: sz,
                                width: sz,
                                kind: OrganKind::Card,
                                cell: CELL_FLOWER,
                                v_range: [1.0, 0.0],
                                bend: 0.0,
                                stem_id: site.stem_id,
                            });
                        }
                    }
                }
            }
        }
    }

    // Opt-in creeping stolon runners with secondary tufts (coastal spinifex).
    if params.runners > 0 {
        let base_az = rng.range(0.0, TAU);
        for r_idx in 0..params.runners {
            let az =
                base_az + (r_idx as f32 / params.runners as f32) * TAU + rng.range(-0.35, 0.35);
            let out = Vec3::new(az.cos(), 0.0, az.sin());
            let perp = Vec3::new(-az.sin(), 0.0, az.cos());
            let run_len = params.runner_length * rng.range(0.75, 1.25);
            let segs = 5usize;
            let stem_id = tree.stems.len() as u32;
            let mut runner = Stem::new(stem_id, 1);
            runner.parent_id = Some(0);
            runner.parent_offset = 0.0;
            let mut p = out * (trunk.radius * 0.5);
            p.y = 0.03;
            let mut nodes = Vec::new();
            for s in 0..segs {
                let u = (s + 1) as f32 / segs as f32;
                let wiggle = (u * PI * 1.6 + r_idx as f32).sin() * 0.14 * run_len;
                let end = Vec3::new(
                    out.x * (trunk.radius * 0.5 + run_len * u) + perp.x * wiggle,
                    0.02 + 0.025 * (u * PI * 2.0).sin().abs(),
                    out.z * (trunk.radius * 0.5 + run_len * u) + perp.z * wiggle,
                );
                let d = (end - p).try_normalize().unwrap_or(out);
                runner.segments.push(Segment {
                    start: p,
                    end,
                    start_radius: 0.010,
                    end_radius: 0.008,
                    direction: d,
                });
                if s >= 1 {
                    nodes.push(end);
                }
                p = end;
            }
            tree.add_stem(runner);
            for (n_i, node_pos) in nodes.into_iter().enumerate() {
                let blades_here = 3usize;
                for b in 0..blades_here {
                    let ba = (b as f32 * GOLDEN_ANGLE) + (n_i as f32 * 1.1) + rng.range(-0.4, 0.4);
                    let bd = Vec3::new(ba.cos(), 0.0, ba.sin());
                    let polar = radians(rng.range(18.0, 62.0));
                    let launch = (Vec3::Y * polar.cos() + bd * polar.sin())
                        .try_normalize()
                        .unwrap_or(Vec3::Y);
                    tree.leaves.push(Leaf {
                        position: node_pos,
                        rotation: blade_rotation(launch, Vec3::Y, rng.variance_add(0.8)),
                        scale: params.blade_length * rng.range(0.32, 0.58),
                        stem_id,
                    });
                }
            }
        }
    }

    tree.update_bounds();
    budget.check_tree(&tree)?;
    Ok(tree)
}

/// Orientation of a blade: local +Y along `launch`, local +Z (the ribbon
/// face) turned toward the crown `axis` side of the blade, then rolled.
fn blade_rotation(launch: Vec3, axis: Vec3, roll: f32) -> Quat {
    let base = Quat::from_rotation_arc(Vec3::Y, launch);
    let face = (axis - launch * axis.dot(launch)).try_normalize();
    let align = match face {
        Some(face) => {
            let bz = base * Vec3::Z;
            let bx = base * Vec3::X;
            face.dot(bx).atan2(face.dot(bz))
        }
        None => 0.0,
    };
    Quat::from_axis_angle(launch, align + roll) * base
}

/// Draw rosette blades (`leaves`) as curved, optionally keeled ribbons that
/// bend under gravity. Vertices are laid out as `(rows + 1) * 3` per blade so
/// the canopy shading pass can address them per leaf.
pub(crate) fn generate_blade_ribbons(
    leaves: &[Leaf],
    config: &LeafConfig,
    ribbon: &BladeRibbon,
    tree: &Tree,
) -> Mesh {
    let mut mesh = Mesh::new();
    let rows = ribbon.rows.max(1);
    let half_w = ribbon.width * ribbon.widen * 0.5;
    let keel = ribbon.keel.clamp(0.0, 1.0);
    let limp = 1.0 - ribbon.stiffness.clamp(0.0, 1.0);
    for (idx, leaf) in leaves.iter().enumerate() {
        let hash = ((idx as f32 * 0.618_034) + tree.seed as f32 * 0.137).fract();
        // Limp blades arch a little more and twist along their length.
        let bend = ribbon.droop * (1.0 + 0.5 * limp * (hash - 0.3));
        let twist = limp * 1.4 * (hash - 0.5);
        let (points, sides) =
            ribbon_spine(leaf.position, leaf.rotation, leaf.scale, bend, twist, rows);
        let base = mesh.vertices.len() as u32;
        for r in 0..=rows {
            let u = r as f32 / rows as f32;
            let tang = ribbon_tangent(&points, r);
            let up = sides[r].cross(tang).try_normalize().unwrap_or(Vec3::Y);
            for xn in [-1.0f32, 0.0, 1.0] {
                // V fold: the margins rise off the midrib.
                let lift = keel * 0.7 * half_w * xn.abs();
                let pos = points[r] + sides[r] * (xn * half_w) + up * lift;
                let normal = (up - sides[r] * (xn * keel * 0.7))
                    .try_normalize()
                    .unwrap_or(up);
                let uv = Vec2::new(xn * 0.5 + 0.5, 1.0 - u);
                let mut vertex = Vertex::new(pos, normal, uv);
                if config.pivot_painter {
                    crate::leaves::encode_leaf_pivot_painter(&mut vertex, leaf, tree);
                    vertex.color.w = ribbon.stiffness.clamp(0.0, 1.0);
                }
                mesh.vertices.push(vertex);
            }
        }
        for r in 0..rows as u32 {
            for c in 0..2u32 {
                let i00 = base + r * 3 + c;
                let i10 = i00 + 1;
                let i01 = i00 + 3;
                let i11 = i01 + 1;
                mesh.indices
                    .extend_from_slice(&[i00, i10, i11, i00, i11, i01]);
            }
        }
    }
    if !mesh.indices.is_empty() {
        mesh.submeshes.push(Submesh {
            index_start: 0,
            index_count: mesh.indices.len() as u32,
            material: MaterialType::Leaves,
        });
    }
    mesh
}

/// Summary line for `midori info`.
pub fn describe(params: &RosetteParams) -> String {
    format!(
        "{} crown(s) x {} blades, {:.2}m long, spread {:.0}-{:.0} deg, skirt {} blades",
        params.crowns,
        params.blades,
        params.blade_length,
        params.spread_min,
        params.spread_max,
        params.skirt_blades
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExportConfig, export_lod_meshes_to_bytes, generate_lod_meshes, generation::generate_tree,
    };

    /// Fixture with 120 blades spreading to 150 degrees.
    fn rosette(extra: &str) -> Species {
        rosette_with(120, 150.0, extra)
    }

    fn rosette_with(blades: u32, spread_max: f32, extra: &str) -> Species {
        Species::from_toml(&format!(
            r#"
[species]
name = "Rosette Fixture"

[generator]
family = "rosette"

[trunk]
height = 1.5
radius = 0.15
taper = 0.1
segments = 6

[rosette]
blades = {blades}
spread_max = {spread_max:.1}
blade_length = 1.0
{extra}
"#
        ))
        .unwrap()
    }

    #[test]
    fn single_crown_generates_blades_on_the_trunk_tip() {
        let species = rosette("");
        let tree = generate_tree(&species, 42).unwrap();
        assert_eq!(tree.stem_count(), 1);
        assert_eq!(tree.leaf_count(), 120);
        assert!(tree.organs.is_empty());
        assert!(tree.bounds.is_valid());
        let tip = tree.stems[0].tip();
        assert!(
            tree.leaves
                .iter()
                .all(|l| (l.position - tip).length() < 0.6)
        );
        // The crown reaches well beyond the trunk in every direction.
        assert!(tree.bounds.size().x > 1.2 && tree.bounds.max.y > 2.0);
    }

    #[test]
    fn rosette_is_deterministic() {
        let species = rosette("skirt = 0.5\nskirt_blades = 30\n[rosette.spike]\nchance = 1.0");
        let a = generate_tree(&species, 7).unwrap();
        let b = generate_tree(&species, 7).unwrap();
        assert_eq!(a.leaves.len(), b.leaves.len());
        for (x, y) in a.leaves.iter().zip(&b.leaves) {
            assert_eq!(x.position, y.position);
            assert_eq!(x.rotation, y.rotation);
        }
        for (x, y) in a.organs.iter().zip(&b.organs) {
            assert_eq!(x.position, y.position);
        }
        let c = generate_tree(&species, 8).unwrap();
        assert_ne!(a.leaves[5].position, c.leaves[5].position);
    }

    #[test]
    fn forked_trunk_carries_one_crown_per_arm() {
        let species = rosette("crowns = 3\nfork_height = 0.5");
        let tree = generate_tree(&species, 3).unwrap();
        assert_eq!(tree.stem_count(), 4);
        assert_eq!(tree.leaf_count(), 360);
        assert_eq!(tree.stems[0].child_ids.len(), 3);
        for arm in &tree.stems[1..] {
            assert_eq!(arm.level, 1);
            assert!(
                arm.tip().y > tree.stems[0].tip().y,
                "arms rise above the fork"
            );
        }
    }

    #[test]
    fn skirt_hangs_below_the_crown_and_stays_above_ground() {
        let species = rosette("skirt = 0.8\nskirt_blades = 60");
        let tree = generate_tree(&species, 5).unwrap();
        assert_eq!(tree.organs.len(), 60);
        let tip_y = tree.stems[0].tip().y;
        for o in &tree.organs {
            assert_eq!(o.kind, OrganKind::Blade);
            assert_eq!(o.cell, CELL_AGED);
            assert!(o.position.y <= tip_y + 1e-3);
            assert!((o.rotation * Vec3::Y).y < 0.0, "skirt blades point down");
            let end = o.position + (o.rotation * Vec3::Y) * o.length;
            assert!(end.y > -0.05, "skirt blade ends below ground: {}", end.y);
        }
    }

    #[test]
    fn spike_styles_emit_their_organs() {
        let cyl = generate_tree(&rosette("[rosette.spike]\nstyle = \"cylinder\""), 1).unwrap();
        assert_eq!(cyl.organs.len(), 2, "scape + spike");
        assert!(cyl.organs[1].position.y > cyl.organs[0].position.y);
        let cone = generate_tree(&rosette("[rosette.spike]\nstyle = \"cone\""), 1).unwrap();
        assert_eq!(cone.organs.len(), 1);
        assert_eq!(cone.organs[0].kind, OrganKind::Ovoid);
        let raceme = generate_tree(
            &rosette("[rosette.spike]\nstyle = \"raceme\"\nflowers = 8\ncount = 3"),
            1,
        )
        .unwrap();
        assert_eq!(raceme.organs.len(), 3 * (1 + 8));
        let none = generate_tree(&rosette("[rosette.spike]\nchance = 0.0"), 1).unwrap();
        assert!(none.organs.is_empty());
    }

    #[test]
    fn droop_pulls_blade_tips_down() {
        let stiff = rosette_with(120, 70.0, "droop = 0.0\nstiffness = 1.0");
        let limp = rosette_with(120, 70.0, "droop = 1.0\nstiffness = 1.0");
        let lowest = |species: &Species| {
            let tree = generate_tree(species, 9).unwrap();
            crate::leaves::generate_leaf_mesh(
                &tree.leaves,
                &LeafConfig::from_species(species),
                &tree,
            )
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MAX, f32::min)
        };
        assert!(lowest(&limp) < lowest(&stiff) - 0.1);
    }

    #[test]
    fn rosette_lods_shrink_and_export() {
        let species = rosette("skirt = 0.6\nskirt_blades = 40\n[rosette.spike]\nchance = 1.0");
        let tree = generate_tree(&species, 42).unwrap();
        let lods = generate_lod_meshes(&tree, &species);
        assert_eq!(lods.meshes.len(), 3);
        let tris: Vec<u32> = lods.meshes.iter().map(|l| l.stats.triangle_count).collect();
        assert!(
            tris[0] > tris[1] && tris[1] > tris[2],
            "LODs must shrink: {tris:?}"
        );
        for lod in &lods.meshes {
            assert!(!lod.mesh.is_empty());
            let n = lod.mesh.vertices.len() as u32;
            assert!(lod.mesh.indices.iter().all(|i| *i < n));
            assert!(lod.mesh.vertices.iter().all(|v| v.position.is_finite()));
        }
        let glb = export_lod_meshes_to_bytes(&lods, &ExportConfig::default()).unwrap();
        assert_eq!(&glb[0..4], b"glTF");
    }

    #[test]
    fn rosette_respects_the_leaf_budget() {
        let species = rosette_with(5000, 150.0, "");
        let budget = GenerationBudget {
            max_leaves: 1000,
            ..GenerationBudget::default()
        };
        let err = crate::generation::generate_tree_with_budget(&species, 1, &budget).unwrap_err();
        assert!(err.to_string().contains("leaves"));
    }

    #[test]
    fn validation_reports_rosette_fields() {
        let err = Species::from_toml(
            r#"
[species]
name = "Bad Rosette"
[generator]
family = "rosette"
[trunk]
height = 1.0
[rosette]
blades = 0
blade_length = -1.0
spread_min = 100.0
spread_max = 50.0
droop = 2.0
"#,
        )
        .unwrap_err()
        .to_string();
        for field in [
            "rosette.blades",
            "rosette.blade_length",
            "rosette.spread_max",
            "rosette.droop",
        ] {
            assert!(err.contains(field), "missing {field} in {err}");
        }
    }
}
