//! Weber-Penn style branching algorithm for procedural tree generation.
//!
//! This module implements the tree generation algorithm based on the Weber-Penn
//! model for realistic procedural tree generation. It uses recursive branching
//! with phyllotaxis (golden angle) placement and crown shape modifiers.

use crate::{
    constants::*,
    math::*,
    rng::Rng,
    species::{
        BranchParams, BranchRadiusModel, CrownShape, GeneratorFamily, Species, TaperProfile,
    },
    tree::*,
};
use glam::Vec3;

/// Resource limits enforced during tree generation.
///
/// Procedural parameters are user input, so a pathological species (huge
/// branch counts, deep segment counts, extreme leaf counts) must fail fast
/// instead of hanging a CLI, a browser tab, or an engine editor. Exceeding
/// a limit is an explicit [`GenerationError::BudgetExceeded`], not a silent
/// truncation.
///
/// `max_vertices`/`max_triangles` are checked by callers after LOD mesh
/// generation (see [`GenerationBudget::check_mesh_stats`]); the CLI's
/// `--max-*` flags and the wasm bindings apply them there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationBudget {
    /// Maximum stems (trunk + branches) a generated tree may contain.
    pub max_stems: u32,
    /// Maximum leaves a generated tree may contain.
    pub max_leaves: u32,
    /// Maximum vertices across all generated LOD meshes.
    pub max_vertices: u32,
    /// Maximum triangles across all generated LOD meshes.
    pub max_triangles: u32,
}

impl Default for GenerationBudget {
    fn default() -> Self {
        Self {
            max_stems: MAX_STEMS,
            max_leaves: 250_000,
            max_vertices: MAX_VERTICES,
            max_triangles: 1_000_000,
        }
    }
}

/// Cheap pre-generation upper-bound estimate for a species.
///
/// Bounds are worst-case: branch `count + count_variance` is an upper bound
/// on children per parent, so the stem total assumes every parent spawns its
/// maximum. Dichotomous generators emit terminal forks on top of level
/// children, so their estimate is doubled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GenerationEstimate {
    /// Upper bound on generated stems.
    pub max_stems: u64,
    /// Upper bound on generated leaves.
    pub max_leaves: u64,
}

/// Estimate a species' worst-case generation cost without building geometry.
pub fn estimate_tree(species: &Species) -> GenerationEstimate {
    let level_max = |params: &Option<BranchParams>| {
        params
            .as_ref()
            .map(|p| (p.count as u64).saturating_add(p.count_variance as u64))
            .unwrap_or(0)
    };
    let l1 = level_max(&species.branches.level1);
    let l2 = level_max(&species.branches.level2);
    let l3 = level_max(&species.branches.level3);
    let mut max_stems = 1u64
        .saturating_add(l1)
        .saturating_add(l1.saturating_mul(l2))
        .saturating_add(l1.saturating_mul(l2).saturating_mul(l3));
    if matches!(species.generator.family, GeneratorFamily::Dichotomous) {
        max_stems = max_stems.saturating_mul(2);
    }
    GenerationEstimate {
        max_stems,
        max_leaves: species.leaves.count as u64,
    }
}

/// Error returned when generation exceeds a [`GenerationBudget`].
#[derive(Debug)]
pub enum GenerationError {
    /// A resource limit was exceeded. `actual` is the observed or estimated
    /// count that crossed `limit`.
    BudgetExceeded {
        /// The budgeted resource (e.g. "stems", "leaves", "triangles").
        resource: &'static str,
        /// The configured limit.
        limit: u64,
        /// Observed or estimated amount.
        actual: u64,
    },
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerationError::BudgetExceeded {
                resource,
                limit,
                actual,
            } => write!(
                f,
                "generation budget exceeded: {actual} {resource} (limit {limit})"
            ),
        }
    }
}

impl std::error::Error for GenerationError {}

impl GenerationBudget {
    /// Refuse an estimate that would exceed the budget before generating.
    pub fn check_estimate(&self, estimate: &GenerationEstimate) -> Result<(), GenerationError> {
        if estimate.max_stems > self.max_stems as u64 {
            return Err(GenerationError::BudgetExceeded {
                resource: "stems",
                limit: self.max_stems as u64,
                actual: estimate.max_stems,
            });
        }
        if estimate.max_leaves > self.max_leaves as u64 {
            return Err(GenerationError::BudgetExceeded {
                resource: "leaves",
                limit: self.max_leaves as u64,
                actual: estimate.max_leaves,
            });
        }
        Ok(())
    }

    /// Check a generated tree against the budget.
    pub fn check_tree(&self, tree: &Tree) -> Result<(), GenerationError> {
        if tree.stems.len() > self.max_stems as usize {
            return Err(GenerationError::BudgetExceeded {
                resource: "stems",
                limit: self.max_stems as u64,
                actual: tree.stems.len() as u64,
            });
        }
        if tree.leaves.len() > self.max_leaves as usize {
            return Err(GenerationError::BudgetExceeded {
                resource: "leaves",
                limit: self.max_leaves as u64,
                actual: tree.leaves.len() as u64,
            });
        }
        Ok(())
    }

    /// Check generated mesh totals (e.g. `LodMeshSet::total_stats()`).
    pub fn check_mesh_stats(
        &self,
        vertex_count: usize,
        triangle_count: usize,
    ) -> Result<(), GenerationError> {
        if vertex_count > self.max_vertices as usize {
            return Err(GenerationError::BudgetExceeded {
                resource: "vertices",
                limit: self.max_vertices as u64,
                actual: vertex_count as u64,
            });
        }
        if triangle_count > self.max_triangles as usize {
            return Err(GenerationError::BudgetExceeded {
                resource: "triangles",
                limit: self.max_triangles as u64,
                actual: triangle_count as u64,
            });
        }
        Ok(())
    }
}

/// Parameters describing a single branch to generate
struct BranchSpec<'a> {
    parent_id: u32,
    parent_offset: f32,
    position: Vec3,
    direction: Vec3,
    params: &'a BranchParams,
    length_modifier: f32,
    level: u8,
    sibling_count: u32,
}

/// Generator context for tree creation
pub struct TreeGenerator<'a> {
    species: &'a Species,
    seed: u64,
    rng: Rng,
    tree: Tree,
    next_stem_id: u32,
    budget: GenerationBudget,
    over_budget: bool,
}

impl<'a> TreeGenerator<'a> {
    /// Create a new tree generator with the given species, seed, and budget.
    pub fn new(species: &'a Species, seed: u64, budget: GenerationBudget) -> Self {
        Self {
            species,
            seed,
            rng: Rng::from_seed(seed),
            tree: Tree::new(species.species.name.clone(), seed),
            next_stem_id: 0,
            budget,
            over_budget: false,
        }
    }

    /// Generate a complete tree
    pub fn generate(mut self) -> Result<Tree, GenerationError> {
        // Generate trunk
        let trunk = self.generate_trunk();
        let trunk_id = self.tree.add_stem(trunk);

        // Generate branches recursively
        self.generate_children(trunk_id, 1);
        if self.over_budget {
            return Err(GenerationError::BudgetExceeded {
                resource: "stems",
                limit: self.budget.max_stems as u64,
                actual: self.tree.stems.len() as u64,
            });
        }

        // Update bounding box
        self.tree.update_bounds();

        // Generate leaves
        crate::leaves::add_leaves_to_tree(&mut self.tree, self.species, self.seed);
        self.budget.check_tree(&self.tree)?;

        Ok(self.tree)
    }

    /// Generate the trunk stem
    fn generate_trunk(&mut self) -> Stem {
        let trunk = &self.species.trunk;

        // Calculate actual height with variance
        let height = trunk.height * self.rng.variance_mul(trunk.height_variance);
        let segment_count = trunk.segments.max(1);
        let segment_length = height / segment_count as f32;

        let mut stem = Stem::new(self.next_stem_id(), 0);
        let mut position = Vec3::ZERO;
        let mut direction = Vec3::Y; // Start pointing up
        let rich_trunk =
            segment_count >= 10 && self.species.crown.offset < 0.88 && trunk.curve_variance >= 4.0;
        let init_az = if rich_trunk {
            (self.seed as f32 * 1.618_034) % TAU
        } else {
            PI * 0.5
        };
        let mut bend_axis = Vec3::new(init_az.cos(), 0.0, init_az.sin());
        let mut radius = trunk.radius;
        let segs_f = segment_count as f32;
        let crook_t = 0.20 + 0.26 * ((self.seed as f32 * 0.381_966).fract());
        let crook_sign = if (self.seed & 1) == 0 {
            1.0_f32
        } else {
            -1.0_f32
        };

        for i in 0..segment_count {
            let t = i as f32 / segs_f;

            // Calculate curve for this segment (normalized by segment count)
            let mut curve_amount = self.calculate_curve(
                t,
                trunk.curve,
                trunk.curve_variance,
                trunk.curve_back,
                segment_count,
            );

            // Balanced 3D helical S-curve + zero-net-tilt localized crook + gravitropic posture recovery
            bend_axis = transport_axis(bend_axis, direction);
            let roll = if rich_trunk {
                let u_crook = (t - crook_t) / 0.11;
                // Antisymmetric S-jog: kinks sharply sideways then elbows back upright without tipping the tree over
                let crook_jog = crook_sign
                    * u_crook
                    * (-u_crook * u_crook).exp()
                    * radians(trunk.curve_variance * 2.35)
                    / (segs_f * 0.14).max(1.0);
                let serpent = radians(trunk.curve_variance * 0.95)
                    * (TAU * 1.45 / segs_f)
                    * (t * TAU * 1.45 + init_az).cos();
                curve_amount += crook_jog + serpent;
                radians(118.0) / segs_f
                    + radians(trunk.curve_variance * 1.5) * (t * TAU * 1.25).sin() / segs_f
                    + self.rng.variance_add(radians(trunk.curve_variance * 0.36)) / segs_f.sqrt()
            } else {
                let harmonic_roll =
                    radians(trunk.curve_variance * 1.15) * (t * TAU * 1.35).sin() / segs_f;
                harmonic_roll
                    + self.rng.variance_add(radians(trunk.curve_variance * 0.32)) / segs_f.sqrt()
            };
            direction = rotate_around_axis(direction, bend_axis, curve_amount);
            if rich_trunk {
                // Natural gravitropism: prevent lower lean from tipping the tree onto the ground,
                // and gently recover skyward through the upper crown
                let min_up = if t > 0.52 { 0.74 } else { 0.64 };
                if direction.y < min_up {
                    direction = (direction + Vec3::Y * ((min_up - direction.y) * 0.95))
                        .try_normalize()
                        .unwrap_or(Vec3::Y);
                }
            }
            if roll.abs() > f32::EPSILON {
                bend_axis = rotate_around_axis(bend_axis, direction, roll);
            }

            // Calculate taper
            let next_t = (i + 1) as f32 / segment_count as f32;
            let next_radius = tapered_radius(
                trunk.radius,
                radius,
                trunk.taper,
                next_t,
                trunk.taper_profile,
            );

            // Create segment
            let end = position + direction * segment_length;
            stem.segments.push(Segment {
                start: position,
                end,
                start_radius: radius,
                end_radius: next_radius,
                direction,
            });

            position = end;
            radius = next_radius;
        }

        stem
    }

    /// Generate child branches for a parent stem
    fn generate_children(&mut self, parent_id: u32, level: u8) {
        if self.over_budget || level > MAX_BRANCH_LEVELS as u8 {
            return;
        }

        let branch_params = match self.species.get_branch_level(level as u32) {
            Some(params) => params.clone(),
            None => return,
        };

        let parent = match self.tree.get_stem(parent_id) {
            Some(p) => p.clone(),
            None => return,
        };

        let parent_length = parent.length();
        if parent_length < MIN_LENGTH {
            return;
        }

        // Calculate branch count with variance
        let count = (branch_params.count as i32
            + self.rng.variance_add(branch_params.count_variance as f32) as i32)
            .max(0) as u32;

        // Crown offset applies to primary scaffold branches off the trunk; deeper
        // sub-branches start after a short collar offset along their parent limb.
        let start_offset = if level == 1 {
            self.species.crown.offset.clamp(0.0, 0.96)
        } else {
            0.18
        };

        // Detect N-fold whorled phyllotaxis when `rotation` divides 360 deg into 3..=10
        // equal spokes (e.g. 72 deg -> 5-branch conifer whorl, 45 deg -> 8-frond palm whorl);
        // otherwise use spiral phyllotaxis (e.g. 137.5 deg golden angle).
        let rot_deg = branch_params.rotation.abs();
        let approx_spokes = if rot_deg > 1.0 {
            (360.0 / rot_deg).round()
        } else {
            1.0
        };
        let whorl_size = if (3.0..=10.0).contains(&approx_spokes)
            && (approx_spokes * rot_deg - 360.0).abs() < 1.5
        {
            approx_spokes as u32
        } else {
            1
        };
        let num_whorls = if count > 0 {
            count.div_ceil(whorl_size)
        } else {
            0
        };

        let mut spawn_slots = Vec::with_capacity(count as usize);
        for i in 0..count {
            let is_apical_leader = whorl_size == 1
                && count >= 3
                && level <= 2
                && self.species.crown.offset < 0.85
                && i == count - 1;

            let (crown_t, rotation_angle) = if is_apical_leader {
                let rot = i as f32 * radians(branch_params.rotation)
                    + self.rng.variance_add(radians(18.0));
                (1.0, rot)
            } else if whorl_size > 1 {
                let w_idx = i / whorl_size;
                let in_whorl = i % whorl_size;
                let this_whorl_n = (count - w_idx * whorl_size).min(whorl_size).max(1);
                let base_frac = if num_whorls <= 1 {
                    0.5
                } else {
                    (w_idx as f32 + 0.30) / (num_whorls as f32 - 0.35)
                };
                let jitter = self.rng.variance_add(0.06 / num_whorls.max(1) as f32);
                let ct = (base_frac + jitter).clamp(0.0, 0.985);
                let spoke_step = crate::constants::TAU / this_whorl_n as f32;
                // Alternate adjacent whorls by half a spoke so tiers interlock naturally
                let whorl_phase = (w_idx as f32) * spoke_step * 0.5;
                let spoke = spoke_step * (in_whorl as f32);
                let rot = whorl_phase + spoke + self.rng.variance_add(radians(9.0));
                (ct, rot)
            } else {
                // Lateral branches span smoothly up to ~0.95, leaving the apex for the leader
                let lateral_n = if count >= 3 && level <= 2 && self.species.crown.offset < 0.85 {
                    count - 1
                } else {
                    count
                };
                let base_frac = if lateral_n <= 1 {
                    0.5
                } else {
                    (i as f32 + 0.30) / (lateral_n as f32 - 0.10)
                };
                let jitter = self.rng.variance_add(0.22 / lateral_n.max(1) as f32);
                let ct = (base_frac + jitter).clamp(0.0, 0.96);
                let rot = i as f32 * radians(branch_params.rotation)
                    + self.rng.variance_add(radians(22.0));
                (ct, rot)
            };
            let t = if is_apical_leader {
                1.0
            } else {
                start_offset + (1.0 - start_offset) * crown_t
            };
            spawn_slots.push((t, crown_t, rotation_angle, is_apical_leader));
        }

        // Spawn from base to tip along the parent so each upper branch inherits
        // the exact live position and rotated tangent produced by lower sympodial forks.
        if whorl_size == 1 && spawn_slots.len() > 1 {
            spawn_slots.sort_by(|a, b| a.0.total_cmp(&b.0));
        }

        for (t, crown_t, rotation_angle, is_apical_leader) in spawn_slots {
            if self.tree.stems.len() >= self.budget.max_stems as usize {
                self.over_budget = true;
                return;
            }

            // Read live parent pose (reflecting sympodial deflection & sapwood division from lower boughs)
            let (spawn_pos, parent_dir, parent_seg_count) = match self.tree.get_stem(parent_id) {
                Some(live_parent) => (
                    live_parent.point_at(t),
                    live_parent.direction_at(t),
                    live_parent.segments.len(),
                ),
                None => return,
            };

            // Calculate branch direction: near crown apex on level 1, narrow insertion angle
            // so upper branches and sympodial leaders flow organically out of their parent stem.
            let apex_angle_scale = if is_apical_leader {
                0.38
            } else if level == 1 {
                if whorl_size > 1 {
                    lerp(1.05, 0.48, crown_t.powf(1.25))
                } else {
                    lerp(1.0, 0.64, crown_t * crown_t)
                }
            } else {
                lerp(1.02, 0.78, crown_t)
            };
            let branch_angle = radians(
                branch_params.angle * apex_angle_scale
                    + self.rng.variance_add(branch_params.angle_variance),
            );

            let branch_dir =
                self.calculate_branch_direction(parent_dir, branch_angle, rotation_angle);

            // Apply crown envelope modifier at level 1; taper sub-branches gently along parent
            let length_mod = if level == 1 {
                self.crown_length_modifier(crown_t)
            } else {
                lerp(1.06, 0.54, crown_t)
            };

            // Generate the branch stem
            let branch = self.generate_branch(BranchSpec {
                parent_id,
                parent_offset: t,
                position: spawn_pos,
                direction: branch_dir,
                params: &branch_params,
                length_modifier: length_mod,
                level,
                sibling_count: count,
            });

            // Only add branch if it has segments
            if branch.segments.is_empty() {
                continue;
            }

            let branch_base_r = branch.base_radius();
            let branch_id = self.tree.add_stem(branch);

            // Add child reference to parent
            if let Some(parent) = self.tree.get_stem_mut(parent_id) {
                parent.child_ids.push(branch_id);
            }

            // Multi-level sympodial fork reaction & da Vinci sapwood division across trunk, L1, and L2!
            if !is_apical_leader
                && level <= 3
                && whorl_size == 1
                && parent_seg_count >= 5
                && branch_params.radius_model == BranchRadiusModel::Ratio
                && self.species.crown.offset < 0.85
            {
                self.apply_scaffold_fork_reaction(parent_id, t, branch_dir, branch_base_r);
            }

            // Recursively generate children
            self.generate_children(branch_id, level + 1);
        }
    }

    /// Generate a single branch stem
    fn generate_branch(&mut self, spec: BranchSpec<'_>) -> Stem {
        let mut stem = Stem::new(self.next_stem_id(), spec.level);
        stem.parent_id = Some(spec.parent_id);
        stem.parent_offset = spec.parent_offset;

        // Calculate branch length with variance and crown modifier
        let length = spec.params.length
            * self.rng.variance_mul(spec.params.length_variance)
            * spec.length_modifier;

        if length < MIN_LENGTH {
            return stem;
        }

        // Get parent radius at spawn point
        let parent = match self.tree.get_stem(spec.parent_id) {
            Some(p) => p,
            None => return stem,
        };
        let parent_radius = parent.radius_at(spec.parent_offset);

        let segment_count = spec.params.segments.max(2);
        let segment_length = length / segment_count as f32;

        let mut pos = spec.position;
        let mut dir = spec.direction;
        // Outward vertical-plane pitch axis so positive curve arches outward/downward smoothly
        let mut bend_axis = Vec3::Y
            .cross(dir)
            .try_normalize()
            .unwrap_or_else(|| create_basis(dir).0);
        let mut radius = branch_base_radius(parent_radius, spec.params, spec.sibling_count);
        let base_radius = radius;
        let gnarl_phase_a = (stem.id as f32 * 2.399_963 + self.seed as f32 * 0.618_034) % TAU;
        let gnarl_phase_b = (stem.id as f32 * 3.883_222 + 1.17) % TAU;
        let segs_f = segment_count as f32;
        let rich_gnarl = segment_count >= 6
            && spec.params.curve_variance >= 6.0
            && self.species.crown.offset < 0.88;
        // Localized crooked knuckles (old storm/growth elbows) at 1-2 spots along the limb
        let k1_t = 0.24 + 0.22 * ((stem.id as f32 * 0.618_034).fract());
        let k2_t = 0.58 + 0.22 * ((stem.id as f32 * 0.754_877).fract());
        let k1_sign = if (stem.id.wrapping_mul(2654435761) & 2) == 0 {
            1.0_f32
        } else {
            -1.0_f32
        };
        let k2_sign = -k1_sign;

        for i in 0..segment_count {
            let t = i as f32 / segs_f;

            // True integrated 3D serpentine S-meander + localized crooked elbows
            let (d_pitch_gnarl, d_yaw_gnarl) = if rich_gnarl {
                let pitch_amp = radians(spec.params.curve_variance * 1.18);
                let yaw_amp = radians(spec.params.curve_variance * 1.72);
                // Derivative of sin(t * TAU * f + phi) wrt segment index is (TAU * f / segs) * cos(...)
                let s_pitch =
                    pitch_amp * (TAU * 1.08 / segs_f) * (t * TAU * 1.08 + gnarl_phase_a).cos()
                        + (pitch_amp * 0.42)
                            * (TAU * 2.25 / segs_f)
                            * (t * TAU * 2.25 - gnarl_phase_b).cos();
                let s_yaw =
                    yaw_amp * (TAU * 1.15 / segs_f) * (t * TAU * 1.15 + gnarl_phase_b).cos()
                        + (yaw_amp * 0.48)
                            * (TAU * 2.40 / segs_f)
                            * (t * TAU * 2.40 + gnarl_phase_a).cos();

                let bell1 = (-((t - k1_t) / 0.088).powi(2)).exp() / (segs_f * 0.155).max(1.0);
                let bell2 = (-((t - k2_t) / 0.088).powi(2)).exp() / (segs_f * 0.155).max(1.0);
                let knuck_amp = radians((spec.params.curve_variance * 0.95).clamp(10.0, 22.0));
                let k_pitch = knuck_amp * (0.75 * k1_sign * bell1 + 0.62 * k2_sign * bell2);
                let k_yaw = knuck_amp * (0.95 * k2_sign * bell1 - 0.85 * k1_sign * bell2);
                (s_pitch + k_pitch, s_yaw + k_yaw)
            } else {
                (0.0, 0.0)
            };

            // Add S-scoop inflection curve_back on rich crooked limbs so boughs sweep outward then scoop skyward
            let branch_curve_back = if rich_gnarl {
                spec.params.curve * 1.42
            } else {
                0.0
            };
            let curve_amount = self.calculate_curve(
                t,
                spec.params.curve,
                spec.params.curve_variance,
                branch_curve_back,
                segment_count,
            ) + d_pitch_gnarl;
            let lateral_wander = radians(self.rng.variance_add(spec.params.curve_variance * 0.58))
                / segs_f.sqrt()
                + d_yaw_gnarl;

            // Apply gravity sag through mid-bough + apical phototropic skyward lift + ground clearance
            let is_weeping = spec.params.gravity <= -0.60;
            let mut photo_lift = if segment_count >= 6 && !is_weeping && t > 0.38 && dir.y < 0.38 {
                (t - 0.38).powf(1.22) * 0.62 * (0.38 - dir.y).clamp(0.0, 1.1) * segment_length
            } else {
                0.0
            };
            if rich_gnarl && !is_weeping {
                let min_clear_y = (self.species.trunk.height * 0.18).clamp(0.85, 1.55);
                if pos.y < min_clear_y + 1.15 && dir.y < 0.26 {
                    let urgency = ((min_clear_y + 1.15 - pos.y) / 1.15).clamp(0.0, 1.6);
                    photo_lift += urgency * 0.58 * segment_length;
                }
            }
            let gravity_influence = spec.params.gravity * segment_length + photo_lift;

            bend_axis = transport_axis(bend_axis, dir);
            dir = self.apply_curve_and_gravity(
                dir,
                bend_axis,
                curve_amount,
                lateral_wander,
                gravity_influence,
            );
            if rich_gnarl && !is_weeping {
                let floor_y = (self.species.trunk.height * 0.14).clamp(0.65, 1.25);
                if pos.y < floor_y && dir.y < 0.08 {
                    dir = Vec3::new(dir.x, 0.08, dir.z).try_normalize().unwrap_or(dir);
                }
            } else if is_weeping && pos.y < 0.22 && dir.y < -0.05 {
                dir = Vec3::new(dir.x, -0.05, dir.z)
                    .try_normalize()
                    .unwrap_or(dir);
            }

            // Taper radius
            let next_t = (i + 1) as f32 / segment_count as f32;
            let next_radius = tapered_radius(
                base_radius,
                radius,
                spec.params.taper,
                next_t,
                spec.params.taper_profile,
            );

            let end = pos + dir * segment_length;
            stem.segments.push(Segment {
                start: pos,
                end,
                start_radius: radius,
                end_radius: next_radius,
                direction: dir,
            });

            pos = end;
            radius = next_radius;
        }

        stem
    }

    /// Calculate per-segment curve angle in radians, normalized by segment count
    fn calculate_curve(
        &mut self,
        t: f32,
        curve: f32,
        variance: f32,
        curve_back: f32,
        segment_count: u32,
    ) -> f32 {
        let segs = segment_count.max(1) as f32;
        let weight = 0.6 + 0.8 * t;
        let base = radians(curve * weight) / segs;
        let var = radians(self.rng.variance_add(variance)) / segs.sqrt();

        // S-curve: bend back in upper half
        let back = if t > 0.5 && curve_back != 0.0 {
            radians(curve_back * (t - 0.5) * 4.0) / segs
        } else {
            0.0
        };

        base + var - back
    }

    /// Apply curve and gravity to a direction vector
    fn apply_curve_and_gravity(
        &self,
        direction: Vec3,
        bend_axis: Vec3,
        curve: f32,
        lateral_wander: f32,
        gravity: f32,
    ) -> Vec3 {
        let mut curved = if curve.abs() > f32::EPSILON {
            rotate_around_axis(direction, bend_axis, curve)
        } else {
            direction
        };
        if lateral_wander.abs() > f32::EPSILON {
            let bitangent = curved.cross(bend_axis).normalize_or_zero();
            if bitangent.length_squared() > 0.001 {
                curved = rotate_around_axis(curved, bitangent, lateral_wander);
            }
        }

        if gravity.abs() > 0.001 {
            // Negative gravity droops (-Y); positive gravity lifts (+Y)
            let gravity_dir = Vec3::new(0.0, gravity.signum(), 0.0);
            let blend = gravity.abs().min(0.95);
            (curved + gravity_dir * blend)
                .try_normalize()
                .unwrap_or(curved)
        } else {
            curved
        }
    }

    /// Calculate branch direction from parent direction
    fn calculate_branch_direction(
        &self,
        parent_dir: Vec3,
        branch_angle: f32,
        rotation_angle: f32,
    ) -> Vec3 {
        // Rotate parent direction outward by branch angle
        let (tangent, _) = create_basis(parent_dir);
        let tilted = rotate_around_axis(parent_dir, tangent, branch_angle);

        // Then rotate around parent direction
        rotate_around_axis(tilted, parent_dir, rotation_angle)
    }

    /// Calculate length modifier based on crown-normalized height fraction [0, 1]
    fn crown_length_modifier(&self, height_fraction: f32) -> f32 {
        let crown = &self.species.crown;
        let h = height_fraction.clamp(0.0, 1.0);

        let shape_mod = match crown.shape {
            CrownShape::Spherical => {
                // Broad rounded decurrent dome: full mid-crown, graceful upper dome
                let centered = (h - 0.42) * 1.55;
                (1.0 - centered * centered).max(0.20).sqrt()
            }
            CrownShape::Conical => {
                // Classic conifer spire: widest at base, tapering crisply to leader apex
                0.15 + 0.85 * (1.0 - h).powf(1.24)
            }
            CrownShape::Hemispherical => {
                // Umbrella / dome profile: widest at base of crown, rounded top
                (1.0 - (h * 0.88).powi(2)).max(0.18).sqrt()
            }
            CrownShape::Flame => {
                // Parabolic, max near middle (like a candle flame)
                ((h + 0.12) * (1.08 - h) * 3.4).clamp(0.22, 1.0)
            }
            CrownShape::Columnar => {
                // Upright columnar profile
                0.85 + 0.15 * (1.0 - (h - 0.5).abs() * 2.0)
            }
        };

        // Apply minimum, density, and width ratio
        shape_mod.max(0.15) * crown.density * crown.width_ratio
    }

    /// Get the next stem ID and increment the counter
    fn next_stem_id(&mut self) -> u32 {
        let id = self.next_stem_id;
        self.next_stem_id += 1;
        id
    }

    /// Deflect a scaffold stem away from a heavy lateral limb and step down its remaining
    /// sapwood radius (da Vinci area division), translating any already-spawned upper children.
    fn apply_scaffold_fork_reaction(
        &mut self,
        parent_id: u32,
        fork_t: f32,
        branch_dir: Vec3,
        branch_base_r: f32,
    ) {
        let (existing_upper_children, old_positions) = match self.tree.get_stem(parent_id) {
            Some(parent) => {
                let n = parent.segments.len();
                if n < 5 {
                    return;
                }
                let parent_r = parent.radius_at(fork_t).max(MIN_RADIUS);
                let ratio = (branch_base_r / parent_r).clamp(0.0, 0.88);
                if ratio < 0.28 {
                    return;
                }
                let fork_seg = ((fork_t * n as f32).floor() as usize).min(n - 1);
                if fork_seg + 1 >= n {
                    return;
                }
                let upper: Vec<(u32, f32)> = parent
                    .child_ids
                    .iter()
                    .filter_map(|&cid| self.tree.get_stem(cid))
                    .filter(|c| c.parent_offset > fork_t + 0.001)
                    .map(|c| (c.id, c.parent_offset))
                    .collect();
                let old_pts: Vec<Vec3> = upper.iter().map(|&(_, ct)| parent.point_at(ct)).collect();
                (upper, old_pts)
            }
            None => return,
        };

        let new_positions = {
            let parent = match self.tree.get_stem_mut(parent_id) {
                Some(p) => p,
                None => return,
            };
            let n = parent.segments.len();
            let parent_r = parent.radius_at(fork_t).max(MIN_RADIUS);
            let ratio = (branch_base_r / parent_r).clamp(0.0, 0.88);
            let fork_seg = ((fork_t * n as f32).floor() as usize).min(n - 1);
            let p_dir = parent.segments[fork_seg]
                .direction
                .try_normalize()
                .unwrap_or(Vec3::Y);
            let lat = branch_dir - p_dir * branch_dir.dot(p_dir);
            let axis = p_dir.cross(lat).try_normalize();
            let twist_axis = lat.try_normalize();
            let twist_sign = if (parent.child_ids.len() & 1) == 0 {
                1.0_f32
            } else {
                -1.0_f32
            };

            let (level_gain, max_deflect) = match parent.level {
                0 => (0.44, 0.21),
                1 => (0.86, 0.35),
                _ => (0.94, 0.36),
            };
            let rad_scale = (1.0 - 0.38 * ratio * ratio).clamp(0.72, 0.98).sqrt();
            let deflect = (level_gain * ratio * ratio + 0.07 * ratio).clamp(0.08, max_deflect);

            parent.segments[fork_seg].end_radius =
                (parent.segments[fork_seg].end_radius * (0.5 + 0.5 * rad_scale)).max(MIN_RADIUS);

            // Distribute sympodial elbow smoothly across 2 segments so spline rings never self-intersect
            for k in (fork_seg + 1)..n {
                let prev_end = parent.segments[k - 1].end;
                let prev_r = parent.segments[k - 1].end_radius;
                let seg_len = (parent.segments[k].end - parent.segments[k].start).length();
                let step_frac = if k == fork_seg + 1 && fork_seg + 2 < n {
                    0.62
                } else {
                    1.0
                };
                if let Some(ax) = axis {
                    parent.segments[k].direction =
                        rotate_around_axis(parent.segments[k].direction, ax, -deflect * step_frac);
                }
                if let Some(tax) = twist_axis {
                    parent.segments[k].direction = rotate_around_axis(
                        parent.segments[k].direction,
                        tax,
                        twist_sign * deflect * 0.20 * step_frac,
                    );
                }
                // Keep primary trunk leader upright enough that cumulative forks never tip the crown over
                if parent.level == 0 && parent.segments[k].direction.y < 0.58 {
                    parent.segments[k].direction = (parent.segments[k].direction
                        + Vec3::Y * ((0.58 - parent.segments[k].direction.y) * 1.10))
                        .try_normalize()
                        .unwrap_or(Vec3::Y);
                }
                parent.segments[k].start = prev_end;
                parent.segments[k].end = prev_end + parent.segments[k].direction * seg_len;
                parent.segments[k].start_radius = prev_r;
                parent.segments[k].end_radius =
                    (parent.segments[k].end_radius * rad_scale).max(MIN_RADIUS);
            }

            existing_upper_children
                .iter()
                .map(|&(_, ct)| parent.point_at(ct))
                .collect::<Vec<Vec3>>()
        };

        for (((cid, _), old_p), new_p) in existing_upper_children
            .into_iter()
            .zip(old_positions)
            .zip(new_positions)
        {
            let delta = new_p - old_p;
            if delta.length_squared() > 1e-10 {
                self.translate_subtree(cid, delta);
            }
        }
    }

    fn translate_subtree(&mut self, root_id: u32, delta: Vec3) {
        let children = match self.tree.get_stem_mut(root_id) {
            Some(stem) => {
                for seg in &mut stem.segments {
                    seg.start += delta;
                    seg.end += delta;
                }
                stem.child_ids.clone()
            }
            None => return,
        };
        for cid in children {
            self.translate_subtree(cid, delta);
        }
    }
}

/// Minimal dichotomous fork-grammar generator.
///
/// This is intentionally small: it proves a second generator family can produce
/// the shared `Tree` output without adding special cases to mesh, LOD, WASM, or
/// export code.
pub struct DichotomousGenerator<'a> {
    species: &'a Species,
    seed: u64,
    rng: Rng,
    tree: Tree,
    next_stem_id: u32,
    budget: GenerationBudget,
    over_budget: bool,
}

impl<'a> DichotomousGenerator<'a> {
    /// Create a new dichotomous generator with the given species, seed, and budget.
    pub fn new(species: &'a Species, seed: u64, budget: GenerationBudget) -> Self {
        Self {
            species,
            seed,
            rng: Rng::from_seed(seed),
            tree: Tree::new(species.species.name.clone(), seed),
            next_stem_id: 0,
            budget,
            over_budget: false,
        }
    }

    /// Generate a complete forked plant.
    pub fn generate(mut self) -> Result<Tree, GenerationError> {
        let trunk = self.generate_trunk();
        let trunk_id = self.tree.add_stem(trunk);

        self.generate_terminal_forks(trunk_id, 1);
        if self.over_budget {
            return Err(GenerationError::BudgetExceeded {
                resource: "stems",
                limit: self.budget.max_stems as u64,
                actual: self.tree.stems.len() as u64,
            });
        }

        self.tree.update_bounds();
        crate::leaves::add_leaves_to_tree(&mut self.tree, self.species, self.seed);
        self.budget.check_tree(&self.tree)?;

        Ok(self.tree)
    }

    fn generate_trunk(&mut self) -> Stem {
        let trunk = &self.species.trunk;
        let height = trunk.height * self.rng.variance_mul(trunk.height_variance);
        let segment_count = trunk.segments.max(1);
        let segment_length = height / segment_count as f32;

        let mut stem = Stem::new(self.next_stem_id(), 0);
        let mut position = Vec3::ZERO;
        let mut direction = Vec3::Y;
        let rich_trunk = segment_count >= 8 && trunk.curve_variance >= 4.0;
        let init_az = if rich_trunk {
            (self.seed as f32 * 1.618_034) % TAU
        } else {
            PI * 0.5
        };
        let mut bend_axis = Vec3::new(init_az.cos(), 0.0, init_az.sin());
        let mut radius = trunk.radius;
        let segs_f = segment_count as f32;

        for i in 0..segment_count {
            let t = i as f32 / segs_f;
            let mut curve_amount = self.calculate_curve(
                t,
                trunk.curve,
                trunk.curve_variance,
                trunk.curve_back,
                segment_count,
            );
            bend_axis = transport_axis(bend_axis, direction);
            if rich_trunk {
                let u_c = (t - 0.44) / 0.13;
                let crook = u_c * (-u_c * u_c).exp() * radians(trunk.curve_variance * 2.1)
                    / (segs_f * 0.16).max(1.0);
                curve_amount += crook;
            }
            direction = rotate_around_axis(direction, bend_axis, curve_amount);
            if rich_trunk && direction.y < 0.78 {
                direction = (direction + Vec3::Y * ((0.78 - direction.y) * 1.05))
                    .try_normalize()
                    .unwrap_or(Vec3::Y);
            }
            if rich_trunk {
                let roll = radians(92.0) / segs_f
                    + radians(trunk.curve_variance * 1.2) * (t * TAU * 1.15).sin() / segs_f;
                bend_axis = rotate_around_axis(bend_axis, direction, roll);
            }

            let next_t = (i + 1) as f32 / segs_f;
            let next_radius = tapered_radius(
                trunk.radius,
                radius,
                trunk.taper,
                next_t,
                trunk.taper_profile,
            );
            let end = position + direction * segment_length;

            stem.segments.push(Segment {
                start: position,
                end,
                start_radius: radius,
                end_radius: next_radius,
                direction,
            });

            position = end;
            radius = next_radius;
        }

        stem
    }

    fn generate_terminal_forks(&mut self, parent_id: u32, level: u8) {
        if self.over_budget || level > MAX_BRANCH_LEVELS as u8 {
            return;
        }

        let params = match self.species.get_branch_level(level as u32) {
            Some(params) => params.clone(),
            None => return,
        };

        let parent = match self.tree.get_stem(parent_id) {
            Some(parent) => parent.clone(),
            None => return,
        };

        if parent.length() < MIN_LENGTH {
            return;
        }

        let count = (params.count as i32
            + self.rng.variance_add(params.count_variance as f32) as i32)
            .max(0) as u32;
        if count == 0 {
            return;
        }

        let fork_count = count.clamp(1, 4);
        let spawn_pos = parent.tip();
        let parent_dir = parent.direction_at(1.0);
        let base_length_mod = if level == 1 {
            self.crown_length_modifier(0.35)
        } else {
            1.0
        };

        for i in 0..fork_count {
            if self.tree.stems.len() >= self.budget.max_stems as usize {
                self.over_budget = true;
                return;
            }

            // Natural dominant/subordinate asymmetry across forked desert arms when variance is active
            let dom_bias = if params.length_variance >= 0.12 && fork_count >= 2 {
                if i == 0 { 1.14 } else { 0.88 }
            } else {
                1.0
            };
            let ang_bias = if params.angle_variance >= 6.0 && fork_count >= 2 {
                if i == 0 { 0.82 } else { 1.18 }
            } else {
                1.0
            };
            let branch_angle =
                radians(params.angle * ang_bias + self.rng.variance_add(params.angle_variance));
            let rotation_angle = self.fork_rotation(i, fork_count, &params, level);
            let branch_dir =
                self.calculate_branch_direction(parent_dir, branch_angle, rotation_angle);
            let branch = self.generate_branch(BranchSpec {
                parent_id,
                parent_offset: 1.0,
                position: spawn_pos,
                direction: branch_dir,
                params: &params,
                length_modifier: base_length_mod * dom_bias,
                level,
                sibling_count: fork_count,
            });

            if branch.segments.is_empty() {
                continue;
            }

            let branch_id = self.tree.add_stem(branch);
            if let Some(parent) = self.tree.get_stem_mut(parent_id) {
                parent.child_ids.push(branch_id);
            }

            self.generate_terminal_forks(branch_id, level + 1);
        }
    }

    fn fork_rotation(
        &mut self,
        index: u32,
        fork_count: u32,
        params: &BranchParams,
        level: u8,
    ) -> f32 {
        let spread = if fork_count > 1 {
            TAU / fork_count as f32
        } else {
            0.0
        };
        let level_offset = radians(params.rotation) * (level.saturating_sub(1) as f32);
        index as f32 * spread + level_offset + self.rng.variance_add(radians(18.0))
    }

    fn generate_branch(&mut self, spec: BranchSpec<'_>) -> Stem {
        let mut stem = Stem::new(self.next_stem_id(), spec.level);
        stem.parent_id = Some(spec.parent_id);
        stem.parent_offset = spec.parent_offset;

        let length = spec.params.length
            * self.rng.variance_mul(spec.params.length_variance)
            * spec.length_modifier;
        if length < MIN_LENGTH {
            return stem;
        }

        let parent = match self.tree.get_stem(spec.parent_id) {
            Some(parent) => parent,
            None => return stem,
        };
        let parent_radius = parent.radius_at(spec.parent_offset);

        let segment_count = spec.params.segments.max(2);
        let segs_f = segment_count as f32;
        let segment_length = length / segs_f;
        let mut pos = spec.position;
        let mut dir = spec.direction;
        let mut bend_axis = Vec3::Y
            .cross(dir)
            .try_normalize()
            .unwrap_or_else(|| create_basis(dir).0);
        let mut radius = branch_base_radius(parent_radius, spec.params, spec.sibling_count);
        let base_radius = radius;

        let gnarl_phase = (stem.id as f32 * 2.399_963 + self.seed as f32 * 0.618_034) % TAU;
        let rich_gnarl = segment_count >= 6 && spec.params.curve_variance >= 6.0;
        let knuck_t = 0.34 + 0.30 * ((stem.id as f32 * 0.618_034).fract());
        let knuck_sign = if (stem.id & 1) == 0 {
            1.0_f32
        } else {
            -1.0_f32
        };

        for i in 0..segment_count {
            let t = i as f32 / segs_f;
            // Outward sweep in lower arm, strong upward candelabrum curve_back in outer arm
            let s_curve_back = if rich_gnarl {
                spec.params.curve * 1.65
            } else {
                0.0
            };
            let (d_pitch, d_yaw) = if rich_gnarl {
                let amp = radians(spec.params.curve_variance * 0.72);
                let sp = amp * (TAU * 1.05 / segs_f) * (t * TAU * 1.05 + gnarl_phase).cos();
                let sy = (amp * 0.85) * (TAU * 1.0 / segs_f) * (t * TAU * 1.0 - gnarl_phase).cos();
                let u_k = (t - knuck_t) / 0.11;
                // Antisymmetric Mojave yucca elbow jog so arms kink crisply without coiling into loops
                let jog = u_k * (-u_k * u_k).exp() / (segs_f * 0.15).max(1.0);
                let kp = knuck_sign * radians(15.0) * jog;
                let ky = -knuck_sign * radians(18.0) * jog;
                (sp + kp, sy + ky)
            } else {
                (0.0, 0.0)
            };
            let curve_amount = self.calculate_curve(
                t,
                spec.params.curve,
                spec.params.curve_variance * 0.55,
                s_curve_back,
                segment_count,
            ) + d_pitch;
            let upward_lift = if rich_gnarl && t > 0.35 {
                (t - 0.35) * 0.28 * segment_length
            } else {
                0.0
            };
            let gravity_influence = spec.params.gravity * segment_length + upward_lift;
            bend_axis = transport_axis(bend_axis, dir);
            if d_yaw.abs() > f32::EPSILON {
                let bitan = dir.cross(bend_axis).normalize_or_zero();
                if bitan.length_squared() > 0.001 {
                    dir = rotate_around_axis(dir, bitan, d_yaw);
                }
            }
            dir = self.apply_curve_and_gravity(dir, bend_axis, curve_amount, gravity_influence);
            // Mojave yucca arms reach skyward in candelabra silhouettes rather than drooping downward
            let min_arm_y = if spec.level == 1 { 0.10 } else { 0.22 };
            if rich_gnarl && dir.y < min_arm_y {
                dir = (dir + Vec3::Y * ((min_arm_y - dir.y) * 1.15))
                    .try_normalize()
                    .unwrap_or(Vec3::Y);
            }

            let next_t = (i + 1) as f32 / segs_f;
            let next_radius = tapered_radius(
                base_radius,
                radius,
                spec.params.taper,
                next_t,
                spec.params.taper_profile,
            );
            let end = pos + dir * segment_length;

            stem.segments.push(Segment {
                start: pos,
                end,
                start_radius: radius,
                end_radius: next_radius,
                direction: dir,
            });

            pos = end;
            radius = next_radius;
        }

        stem
    }

    fn calculate_curve(
        &mut self,
        t: f32,
        curve: f32,
        variance: f32,
        curve_back: f32,
        segment_count: u32,
    ) -> f32 {
        let segs = segment_count.max(1) as f32;
        let base = radians(curve * (0.6 + 0.8 * t)) / segs;
        let var = radians(self.rng.variance_add(variance)) / segs.sqrt();
        let back = if t > 0.5 && curve_back != 0.0 {
            radians(curve_back * (t - 0.5) * 4.0) / segs
        } else {
            0.0
        };

        base + var - back
    }

    fn apply_curve_and_gravity(
        &self,
        direction: Vec3,
        bend_axis: Vec3,
        curve: f32,
        gravity: f32,
    ) -> Vec3 {
        let curved = if curve.abs() > f32::EPSILON {
            rotate_around_axis(direction, bend_axis, curve)
        } else {
            direction
        };

        if gravity.abs() > 0.001 {
            // Negative gravity droops (-Y); positive gravity lifts (+Y)
            let gravity_dir = Vec3::new(0.0, gravity.signum(), 0.0);
            let blend = gravity.abs().min(0.95);
            (curved + gravity_dir * blend)
                .try_normalize()
                .unwrap_or(curved)
        } else {
            curved
        }
    }

    fn calculate_branch_direction(
        &self,
        parent_dir: Vec3,
        branch_angle: f32,
        rotation_angle: f32,
    ) -> Vec3 {
        let (tangent, _) = create_basis(parent_dir);
        let tilted = rotate_around_axis(parent_dir, tangent, branch_angle);
        rotate_around_axis(tilted, parent_dir, rotation_angle)
    }

    fn crown_length_modifier(&self, height_fraction: f32) -> f32 {
        let crown = &self.species.crown;

        let shape_mod = match crown.shape {
            CrownShape::Spherical => {
                let centered = (height_fraction - 0.5) * 2.0;
                1.0 - centered.abs()
            }
            CrownShape::Conical => 1.0 - height_fraction,
            CrownShape::Hemispherical => (1.0 - height_fraction * height_fraction).sqrt(),
            CrownShape::Flame => height_fraction * (1.0 - height_fraction) * 4.0,
            CrownShape::Columnar => 1.0,
        };

        shape_mod.max(0.1) * crown.density * crown.width_ratio
    }

    fn next_stem_id(&mut self) -> u32 {
        let id = self.next_stem_id;
        self.next_stem_id += 1;
        id
    }
}

fn transport_axis(axis: Vec3, direction: Vec3) -> Vec3 {
    let dir = direction.try_normalize().unwrap_or(Vec3::Y);
    let projected = axis - dir * axis.dot(dir);
    projected
        .try_normalize()
        .unwrap_or_else(|| create_basis(dir).0)
}

fn branch_base_radius(parent_radius: f32, params: &BranchParams, sibling_count: u32) -> f32 {
    let ratio_radius = parent_radius * params.radius_ratio;

    match params.radius_model {
        BranchRadiusModel::Ratio => ratio_radius.max(MIN_RADIUS),
        BranchRadiusModel::Pipe => {
            let exponent = params.pipe_exponent.clamp(1.0, 4.0);
            let siblings = sibling_count.max(1) as f32;
            let split_radius = parent_radius / siblings.powf(1.0 / exponent);
            (split_radius * params.radius_ratio).max(MIN_RADIUS)
        }
    }
}

fn tapered_radius(
    base_radius: f32,
    previous_radius: f32,
    taper: f32,
    t: f32,
    profile: TaperProfile,
) -> f32 {
    let taper = taper.clamp(0.0, 1.0);
    let t = t.clamp(0.0, 1.0);

    let radius = match profile {
        TaperProfile::Linear => base_radius * (1.0 - taper * t),
        TaperProfile::Smooth => {
            let smooth_t = t * t * (3.0 - 2.0 * t);
            base_radius * (1.0 - taper * smooth_t)
        }
        TaperProfile::Exponential => {
            let tip_ratio = (1.0 - taper).max(0.05);
            base_radius * tip_ratio.powf(t)
        }
        TaperProfile::Compound => previous_radius * (1.0 - taper * t),
    };

    radius.max(MIN_RADIUS)
}

/// Generate a tree from species parameters
///
/// # Example
///
/// ```
/// use midori_core::species::Species;
/// use midori_core::generation::generate_tree;
///
/// let toml = r#"
/// [species]
/// name = "Test Tree"
///
/// [trunk]
/// height = 5.0
/// radius = 0.3
/// segments = 4
///
/// [branches.level1]
/// count = 5
/// length = 2.0
/// "#;
///
/// let species = Species::from_toml(toml).unwrap();
/// let tree = generate_tree(&species, 12345).unwrap();
///
/// assert!(!tree.stems.is_empty());
/// ```
/// Generate a tree under the default [`GenerationBudget`].
///
/// Returns [`GenerationError::BudgetExceeded`] when the species would produce
/// more stems or leaves than the budget allows — callers must handle the
/// error instead of receiving a silently truncated tree. Use
/// [`generate_tree_with_budget`] for explicit limits, or [`estimate_tree`]
/// to refuse pathological inputs before generating.
pub fn generate_tree(species: &Species, seed: u64) -> Result<Tree, GenerationError> {
    generate_tree_with_budget(species, seed, &GenerationBudget::default())
}

/// Generate a tree under an explicit [`GenerationBudget`].
pub fn generate_tree_with_budget(
    species: &Species,
    seed: u64,
    budget: &GenerationBudget,
) -> Result<Tree, GenerationError> {
    match species.generator.family {
        GeneratorFamily::Dichotomous => {
            DichotomousGenerator::new(species, seed, *budget).generate()
        }
        GeneratorFamily::WeberPenn
        | GeneratorFamily::Cactus
        | GeneratorFamily::PadChain
        | GeneratorFamily::Custom => TreeGenerator::new(species, seed, *budget).generate(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SPECIES_TOML: &str = r#"
[species]
name = "Test Tree"

[trunk]
height = 5.0
height_variance = 0.1
radius = 0.3
taper = 0.7
curve = 5.0
curve_variance = 2.0
segments = 4

[branches.level1]
count = 5
count_variance = 1
length = 2.0
length_variance = 0.2
radius_ratio = 0.5
angle = 45.0
angle_variance = 10.0
rotation = 137.5
gravity = -0.1
curve = 10.0
curve_variance = 5.0
segments = 3

[branches.level2]
count = 3
length = 1.0
radius_ratio = 0.4
angle = 50.0
rotation = 137.5
segments = 2

[crown]
shape = "spherical"
offset = 0.3
density = 1.0
width_ratio = 1.0
"#;

    const DICHOTOMOUS_SPECIES_TOML: &str = r#"
[species]
name = "Dichotomous Prototype"
latin = "Yucca brevifolia"

[generator]
family = "dichotomous"

[trunk]
height = 4.0
radius = 0.32
taper = 0.55
segments = 4

[branches.level1]
count = 2
length = 2.0
radius_ratio = 0.58
angle = 38.0
rotation = 180.0
gravity = -0.05
curve = 8.0
segments = 3

[branches.level2]
count = 2
length = 1.2
radius_ratio = 0.55
angle = 42.0
rotation = 180.0
gravity = 0.0
curve = 8.0
segments = 3

[branches.level3]
count = 2
length = 0.7
radius_ratio = 0.5
angle = 45.0
rotation = 180.0
gravity = 0.05
curve = 6.0
segments = 2

[crown]
shape = "columnar"
offset = 0.75
density = 0.85
width_ratio = 0.8

[leaves]
count = 128
min_level = 2
size = 0.18
size_variance = 0.1
distribution = "endpoint"
geometry = "polygon"
shape = "needle"
up_influence = 0.8
"#;

    #[test]
    fn test_generate_tree() {
        let species = Species::from_toml(TEST_SPECIES_TOML).unwrap();
        let tree = generate_tree(&species, 12345).unwrap();

        // Should have at least a trunk
        assert!(!tree.stems.is_empty());
        assert!(tree.trunk().is_some());

        // Trunk should be level 0
        let trunk = tree.trunk().unwrap();
        assert_eq!(trunk.level, 0);
        assert!(trunk.parent_id.is_none());

        // Trunk should have segments
        assert!(!trunk.segments.is_empty());
    }

    #[test]
    fn test_deterministic_generation() {
        let species = Species::from_toml(TEST_SPECIES_TOML).unwrap();

        let tree1 = generate_tree(&species, 12345).unwrap();
        let tree2 = generate_tree(&species, 12345).unwrap();

        // Same seed should produce identical trees
        assert_eq!(tree1.stems.len(), tree2.stems.len());

        for (s1, s2) in tree1.stems.iter().zip(tree2.stems.iter()) {
            assert_eq!(s1.id, s2.id);
            assert_eq!(s1.level, s2.level);
            assert_eq!(s1.segments.len(), s2.segments.len());

            for (seg1, seg2) in s1.segments.iter().zip(s2.segments.iter()) {
                assert!((seg1.start - seg2.start).length() < f32::EPSILON);
                assert!((seg1.end - seg2.end).length() < f32::EPSILON);
            }
        }
    }

    #[test]
    fn test_different_seeds_produce_different_trees() {
        let species = Species::from_toml(TEST_SPECIES_TOML).unwrap();

        let tree1 = generate_tree(&species, 12345).unwrap();
        let tree2 = generate_tree(&species, 54321).unwrap();

        // Different seeds should produce different trees
        // (very unlikely to be identical)
        let trunk1 = tree1.trunk().unwrap();
        let trunk2 = tree2.trunk().unwrap();

        // Heights should be different due to variance
        let height1 = trunk1.length();
        let height2 = trunk2.length();

        // With variance, heights should differ
        // (extremely unlikely to be exactly equal)
        assert!((height1 - height2).abs() > f32::EPSILON);
    }

    #[test]
    fn test_branch_levels() {
        let species = Species::from_toml(TEST_SPECIES_TOML).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        // Should have trunk (level 0)
        assert!(tree.stems_at_level(0).count() > 0);

        // Should have level 1 branches
        assert!(tree.stems_at_level(1).count() > 0);

        // Should have level 2 branches (sub-branches of level 1)
        assert!(tree.stems_at_level(2).count() > 0);
    }

    #[test]
    fn test_trunk_structure() {
        let species = Species::from_toml(TEST_SPECIES_TOML).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        let trunk = tree.trunk().unwrap();

        // Trunk should have the expected number of segments
        assert_eq!(trunk.segments.len(), 4);

        // Trunk should start at origin
        assert!((trunk.base() - Vec3::ZERO).length() < f32::EPSILON);

        // Trunk segments should be connected
        for i in 1..trunk.segments.len() {
            let prev_end = trunk.segments[i - 1].end;
            let curr_start = trunk.segments[i].start;
            assert!((prev_end - curr_start).length() < f32::EPSILON);
        }

        // Trunk should taper (end radius < start radius)
        let first_seg = &trunk.segments[0];
        let last_seg = trunk.segments.last().unwrap();
        assert!(last_seg.end_radius < first_seg.start_radius);
    }

    #[test]
    fn test_pipe_model_branch_radius_split() {
        let toml = r#"
[species]
name = "Pipe Radius"

[trunk]
height = 6.0
radius = 0.5
taper = 0.0
segments = 2

[branches.level1]
count = 4
length = 1.0
radius_ratio = 1.0
radius_model = "pipe"
pipe_exponent = 2.0
taper = 0.0
taper_profile = "linear"
angle = 45.0
rotation = 90.0
segments = 2

[crown]
offset = 0.25

[leaves]
count = 0
"#;

        let species = Species::from_toml(toml).unwrap();
        let tree = generate_tree(&species, 42).unwrap();
        let branches: Vec<_> = tree.stems_at_level(1).collect();
        assert_eq!(branches.len(), 4);

        for branch in branches {
            let parent = tree.get_stem(branch.parent_id.unwrap()).unwrap();
            let parent_radius = parent.radius_at(branch.parent_offset);
            let expected = parent_radius / 4.0_f32.sqrt();
            assert!(
                (branch.base_radius() - expected).abs() < 0.001,
                "pipe radius should split parent radius across siblings: {} vs {}",
                branch.base_radius(),
                expected
            );
        }
    }

    #[test]
    fn test_taper_profile_controls_branch_radius_distribution() {
        let species_for_profile = |profile: &str| {
            let toml = format!(
                r#"
[species]
name = "Taper {profile}"

[trunk]
height = 4.0
radius = 0.4
taper = 0.0
segments = 2

[branches.level1]
count = 1
length = 2.0
radius_ratio = 0.5
taper = 0.8
taper_profile = "{profile}"
angle = 45.0
segments = 4

[crown]
offset = 0.5

[leaves]
count = 0
"#
            );
            Species::from_toml(&toml).unwrap()
        };

        let linear_tree = generate_tree(&species_for_profile("linear"), 42).unwrap();
        let smooth_tree = generate_tree(&species_for_profile("smooth"), 42).unwrap();
        let linear_branch = linear_tree.stems_at_level(1).next().unwrap();
        let smooth_branch = smooth_tree.stems_at_level(1).next().unwrap();

        let linear_first_end = linear_branch.segments[0].end_radius;
        let smooth_first_end = smooth_branch.segments[0].end_radius;

        assert!(
            smooth_first_end > linear_first_end,
            "smooth taper should retain more base thickness early along the branch"
        );
    }

    #[test]
    fn test_branches_attached_to_parent() {
        let species = Species::from_toml(TEST_SPECIES_TOML).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        for stem in &tree.stems {
            if let Some(parent_id) = stem.parent_id {
                // Branch should have a valid parent
                let parent = tree.get_stem(parent_id);
                assert!(parent.is_some());

                // Parent should list this stem as a child
                let parent = parent.unwrap();
                assert!(parent.child_ids.contains(&stem.id));
            }
        }
    }

    #[test]
    fn test_bounding_box() {
        let species = Species::from_toml(TEST_SPECIES_TOML).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        // Bounding box should be valid
        assert!(tree.bounds.is_valid());

        // Bounding box should contain all stems
        for stem in &tree.stems {
            for seg in &stem.segments {
                assert!(tree.bounds.min.x <= seg.start.x);
                assert!(tree.bounds.min.y <= seg.start.y);
                assert!(tree.bounds.min.z <= seg.start.z);
                assert!(tree.bounds.max.x >= seg.end.x);
                assert!(tree.bounds.max.y >= seg.end.y);
                assert!(tree.bounds.max.z >= seg.end.z);
            }
        }
    }

    #[test]
    fn test_crown_shape_modifiers() {
        // Test different crown shapes produce different results
        let spherical_toml = r#"
[species]
name = "Spherical"
[trunk]
height = 5.0
segments = 4
[branches.level1]
count = 8
length = 2.0
segments = 2
[crown]
shape = "spherical"
offset = 0.3
"#;

        let conical_toml = r#"
[species]
name = "Conical"
[trunk]
height = 5.0
segments = 4
[branches.level1]
count = 8
length = 2.0
segments = 2
[crown]
shape = "conical"
offset = 0.3
"#;

        let spherical = Species::from_toml(spherical_toml).unwrap();
        let conical = Species::from_toml(conical_toml).unwrap();

        let tree_spherical = generate_tree(&spherical, 42).unwrap();
        let tree_conical = generate_tree(&conical, 42).unwrap();

        // Both should generate valid trees
        assert!(!tree_spherical.stems.is_empty());
        assert!(!tree_conical.stems.is_empty());

        // They should have different bounding boxes (different crown shapes)
        // The conical tree should typically be narrower at the top
        let bounds_spherical = tree_spherical.bounds;
        let bounds_conical = tree_conical.bounds;

        // At minimum, they should both have valid bounds
        assert!(bounds_spherical.is_valid());
        assert!(bounds_conical.is_valid());
    }

    #[test]
    fn test_minimal_species() {
        // Test with minimal species definition (mostly defaults)
        let minimal_toml = r#"
[species]
name = "Minimal"

[trunk]
"#;

        let species = Species::from_toml(minimal_toml).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        // Should still generate a valid trunk
        assert!(tree.trunk().is_some());
        assert!(!tree.trunk().unwrap().segments.is_empty());
    }

    #[test]
    fn test_max_stems_limit() {
        // Species whose worst-case estimate exceeds the default budget is
        // refused before any geometry work.
        let many_branches_toml = r#"
[species]
name = "Many Branches"

[trunk]
height = 10.0
segments = 8

[branches.level1]
count = 100
length = 3.0
segments = 4

[branches.level2]
count = 50
length = 1.5
segments = 2

[branches.level3]
count = 25
length = 0.5
segments = 2

[crown]
offset = 0.1
"#;

        let species = Species::from_toml(many_branches_toml).unwrap();
        let estimate = estimate_tree(&species);
        assert!(estimate.max_stems > MAX_STEMS as u64);

        let budget = GenerationBudget::default();
        assert!(matches!(
            budget.check_estimate(&estimate),
            Err(GenerationError::BudgetExceeded {
                resource: "stems",
                ..
            })
        ));

        // Generation also refuses instead of silently truncating.
        let err = generate_tree(&species, 42).unwrap_err();
        assert!(matches!(
            err,
            GenerationError::BudgetExceeded {
                resource: "stems",
                ..
            }
        ));
    }

    #[test]
    fn test_budget_override_allows_larger_trees() {
        let species = Species::from_toml(
            r#"
[species]
name = "Over Budget"

[trunk]
height = 10.0
radius = 0.4
segments = 8

[branches.level1]
count = 60
length = 3.0
segments = 4
"#,
        )
        .unwrap();
        let tight = GenerationBudget {
            max_stems: 10,
            ..GenerationBudget::default()
        };
        assert!(generate_tree_with_budget(&species, 7, &tight).is_err());

        let loose = GenerationBudget {
            max_stems: 500,
            ..GenerationBudget::default()
        };
        assert!(generate_tree_with_budget(&species, 7, &loose).is_ok());
    }

    #[test]
    fn test_species_name_preserved() {
        let toml = r#"
[species]
name = "My Custom Tree"

[trunk]
"#;

        let species = Species::from_toml(toml).unwrap();
        let tree = generate_tree(&species, 42).unwrap();

        assert_eq!(tree.species_name, "My Custom Tree");
    }

    #[test]
    fn test_seed_preserved() {
        let toml = r#"
[species]
name = "Test"

[trunk]
"#;

        let species = Species::from_toml(toml).unwrap();
        let tree = generate_tree(&species, 99999).unwrap();

        assert_eq!(tree.seed, 99999);
    }

    #[test]
    fn test_dichotomous_generator_family_dispatch() {
        let species = Species::from_toml(DICHOTOMOUS_SPECIES_TOML).unwrap();
        assert_eq!(species.generator.family, GeneratorFamily::Dichotomous);

        let tree = generate_tree(&species, 42).unwrap();

        assert_eq!(tree.species_name, "Dichotomous Prototype");
        assert!(tree.trunk().is_some());
        assert_eq!(tree.stems_at_level(1).count(), 2);
        assert_eq!(tree.stems_at_level(2).count(), 4);
        assert_eq!(tree.stems_at_level(3).count(), 8);
        assert!(tree.leaf_count() > 0);
        assert!(tree.bounds.is_valid());

        for stem in tree.stems.iter().filter(|stem| stem.parent_id.is_some()) {
            assert!(
                (stem.parent_offset - 1.0).abs() < f32::EPSILON,
                "dichotomous forks should attach at parent tips"
            );
        }
    }

    #[test]
    fn test_dichotomous_generator_reaches_lod_and_export() {
        let species = Species::from_toml(DICHOTOMOUS_SPECIES_TOML).unwrap();
        let tree = generate_tree(&species, 7).unwrap();
        let lods = crate::generate_lod_meshes(&tree, &species);

        assert!(!lods.is_empty());
        for lod in &lods.meshes {
            assert!(!lod.mesh.is_empty(), "{} should have mesh data", lod.name);
            assert!(lod.stats.vertex_count > 0);
            assert!(lod.stats.triangle_count > 0);
        }

        let bytes = crate::export_lod_meshes_to_bytes(&lods, &crate::ExportConfig::default())
            .expect("dichotomous LOD set should export as GLB");
        assert!(bytes.starts_with(b"glTF"));
    }
}
