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
}

impl<'a> TreeGenerator<'a> {
    /// Create a new tree generator with the given species and seed
    pub fn new(species: &'a Species, seed: u64) -> Self {
        Self {
            species,
            seed,
            rng: Rng::from_seed(seed),
            tree: Tree::new(species.species.name.clone(), seed),
            next_stem_id: 0,
        }
    }

    /// Generate a complete tree
    pub fn generate(mut self) -> Tree {
        // Generate trunk
        let trunk = self.generate_trunk();
        let trunk_id = self.tree.add_stem(trunk);

        // Generate branches recursively
        self.generate_children(trunk_id, 1);

        // Update bounding box
        self.tree.update_bounds();

        // Generate leaves
        crate::leaves::add_leaves_to_tree(&mut self.tree, self.species, self.seed);

        self.tree
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
        let mut radius = trunk.radius;

        for i in 0..segment_count {
            let t = i as f32 / segment_count as f32;

            // Calculate curve for this segment
            let curve_amount =
                self.calculate_curve(t, trunk.curve, trunk.curve_variance, trunk.curve_back);

            // Apply curve rotation
            direction = self.apply_curve(direction, curve_amount, 0.0);

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
        if level > MAX_BRANCH_LEVELS as u8 {
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

        // Crown offset - start branches at this fraction of parent
        let crown_offset = self.species.crown.offset;

        // Generate branches using phyllotaxis (golden angle)
        for i in 0..count {
            if self.tree.stems.len() >= MAX_STEMS as usize {
                break;
            }

            // Position along parent (crown offset to tip)
            let t = crown_offset + (1.0 - crown_offset) * (i as f32 / count.max(1) as f32);

            // Rotation around parent using golden angle
            let rotation_angle =
                i as f32 * radians(branch_params.rotation) + self.rng.variance_add(radians(15.0));

            // Get parent position and direction at this point
            let spawn_pos = parent.point_at(t);
            let parent_dir = parent.direction_at(t);

            // Calculate branch direction
            let branch_angle =
                radians(branch_params.angle + self.rng.variance_add(branch_params.angle_variance));

            let branch_dir =
                self.calculate_branch_direction(parent_dir, branch_angle, rotation_angle);

            // Apply crown shape modifier
            let length_mod = self.crown_length_modifier(t);

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

            let branch_id = self.tree.add_stem(branch);

            // Add child reference to parent
            if let Some(parent) = self.tree.get_stem_mut(parent_id) {
                parent.child_ids.push(branch_id);
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
        let mut radius = branch_base_radius(parent_radius, spec.params, spec.sibling_count);
        let base_radius = radius;

        for i in 0..segment_count {
            let t = i as f32 / segment_count as f32;

            // Calculate curve with variance
            let curve_amount =
                self.calculate_curve(t, spec.params.curve, spec.params.curve_variance, 0.0);

            // Apply gravity influence
            let gravity_influence = spec.params.gravity * segment_length;

            dir = self.apply_curve_and_gravity(dir, curve_amount, gravity_influence);

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

    /// Calculate curve amount for a segment
    fn calculate_curve(&mut self, t: f32, curve: f32, variance: f32, curve_back: f32) -> f32 {
        let base = radians(curve * t);
        let var = radians(self.rng.variance_add(variance));

        // S-curve: bend back at end
        let back = if t > 0.5 && curve_back != 0.0 {
            radians(curve_back * (t - 0.5) * 2.0)
        } else {
            0.0
        };

        base + var - back
    }

    /// Apply curve rotation to a direction vector
    fn apply_curve(&self, direction: Vec3, curve: f32, roll: f32) -> Vec3 {
        if curve.abs() < f32::EPSILON && roll.abs() < f32::EPSILON {
            return direction;
        }

        // Create perpendicular axis for rotation
        let (tangent, _) = create_basis(direction);

        // Apply curve rotation around tangent
        let rotated = rotate_around_axis(direction, tangent, curve);

        if roll.abs() > f32::EPSILON {
            rotate_around_axis(rotated, direction, roll)
        } else {
            rotated
        }
    }

    /// Apply curve and gravity to a direction vector
    fn apply_curve_and_gravity(&self, direction: Vec3, curve: f32, gravity: f32) -> Vec3 {
        let curved = self.apply_curve(direction, curve, 0.0);

        if gravity.abs() > 0.001 {
            // Gravity pulls toward or away from Y axis
            let gravity_dir = Vec3::new(0.0, -gravity.signum(), 0.0);
            let blend = gravity.abs().min(1.0);
            (curved + gravity_dir * blend).normalize()
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

    /// Calculate length modifier based on crown shape
    fn crown_length_modifier(&self, height_fraction: f32) -> f32 {
        let crown = &self.species.crown;

        let shape_mod = match crown.shape {
            CrownShape::Spherical => {
                // Max length in middle, shorter at top and bottom
                let centered = (height_fraction - 0.5) * 2.0; // -1 to 1
                1.0 - centered.abs()
            }
            CrownShape::Conical => {
                // Longest at bottom, shortest at top
                1.0 - height_fraction
            }
            CrownShape::Hemispherical => {
                // Circular profile (half sphere)
                (1.0 - height_fraction * height_fraction).sqrt()
            }
            CrownShape::Flame => {
                // Parabolic, max at 0.5 (like a candle flame)
                height_fraction * (1.0 - height_fraction) * 4.0
            }
            CrownShape::Columnar => {
                // Constant length (vertical column)
                1.0
            }
        };

        // Apply minimum, density, and width ratio
        shape_mod.max(0.1) * crown.density * crown.width_ratio
    }

    /// Get the next stem ID and increment the counter
    fn next_stem_id(&mut self) -> u32 {
        let id = self.next_stem_id;
        self.next_stem_id += 1;
        id
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
}

impl<'a> DichotomousGenerator<'a> {
    /// Create a new dichotomous generator with the given species and seed.
    pub fn new(species: &'a Species, seed: u64) -> Self {
        Self {
            species,
            seed,
            rng: Rng::from_seed(seed),
            tree: Tree::new(species.species.name.clone(), seed),
            next_stem_id: 0,
        }
    }

    /// Generate a complete forked plant.
    pub fn generate(mut self) -> Tree {
        let trunk = self.generate_trunk();
        let trunk_id = self.tree.add_stem(trunk);

        self.generate_terminal_forks(trunk_id, 1);

        self.tree.update_bounds();
        crate::leaves::add_leaves_to_tree(&mut self.tree, self.species, self.seed);

        self.tree
    }

    fn generate_trunk(&mut self) -> Stem {
        let trunk = &self.species.trunk;
        let height = trunk.height * self.rng.variance_mul(trunk.height_variance);
        let segment_count = trunk.segments.max(1);
        let segment_length = height / segment_count as f32;

        let mut stem = Stem::new(self.next_stem_id(), 0);
        let mut position = Vec3::ZERO;
        let mut direction = Vec3::Y;
        let mut radius = trunk.radius;

        for i in 0..segment_count {
            let t = i as f32 / segment_count as f32;
            let curve_amount =
                self.calculate_curve(t, trunk.curve, trunk.curve_variance, trunk.curve_back);
            direction = self.apply_curve(direction, curve_amount, 0.0);

            let next_t = (i + 1) as f32 / segment_count as f32;
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
        if level > MAX_BRANCH_LEVELS as u8 {
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
        let length_mod = self.crown_length_modifier(level as f32 / MAX_BRANCH_LEVELS as f32);

        for i in 0..fork_count {
            if self.tree.stems.len() >= MAX_STEMS as usize {
                break;
            }

            let branch_angle = radians(params.angle + self.rng.variance_add(params.angle_variance));
            let rotation_angle = self.fork_rotation(i, fork_count, &params, level);
            let branch_dir =
                self.calculate_branch_direction(parent_dir, branch_angle, rotation_angle);
            let branch = self.generate_branch(BranchSpec {
                parent_id,
                parent_offset: 1.0,
                position: spawn_pos,
                direction: branch_dir,
                params: &params,
                length_modifier: length_mod,
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
        index as f32 * spread + level_offset + self.rng.variance_add(radians(10.0))
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
        let segment_length = length / segment_count as f32;
        let mut pos = spec.position;
        let mut dir = spec.direction;
        let mut radius = branch_base_radius(parent_radius, spec.params, spec.sibling_count);
        let base_radius = radius;

        for i in 0..segment_count {
            let t = i as f32 / segment_count as f32;
            let curve_amount =
                self.calculate_curve(t, spec.params.curve, spec.params.curve_variance, 0.0);
            let gravity_influence = spec.params.gravity * segment_length;
            dir = self.apply_curve_and_gravity(dir, curve_amount, gravity_influence);

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

    fn calculate_curve(&mut self, t: f32, curve: f32, variance: f32, curve_back: f32) -> f32 {
        let base = radians(curve * t);
        let var = radians(self.rng.variance_add(variance));
        let back = if t > 0.5 && curve_back != 0.0 {
            radians(curve_back * (t - 0.5) * 2.0)
        } else {
            0.0
        };

        base + var - back
    }

    fn apply_curve(&self, direction: Vec3, curve: f32, roll: f32) -> Vec3 {
        if curve.abs() < f32::EPSILON && roll.abs() < f32::EPSILON {
            return direction;
        }

        let (tangent, _) = create_basis(direction);
        let rotated = rotate_around_axis(direction, tangent, curve);

        if roll.abs() > f32::EPSILON {
            rotate_around_axis(rotated, direction, roll)
        } else {
            rotated
        }
    }

    fn apply_curve_and_gravity(&self, direction: Vec3, curve: f32, gravity: f32) -> Vec3 {
        let curved = self.apply_curve(direction, curve, 0.0);

        if gravity.abs() > 0.001 {
            let gravity_dir = Vec3::new(0.0, -gravity.signum(), 0.0);
            let blend = gravity.abs().min(1.0);
            (curved + gravity_dir * blend).normalize()
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
/// let tree = generate_tree(&species, 12345);
///
/// assert!(!tree.stems.is_empty());
/// ```
pub fn generate_tree(species: &Species, seed: u64) -> Tree {
    match species.generator.family {
        GeneratorFamily::Dichotomous => DichotomousGenerator::new(species, seed).generate(),
        GeneratorFamily::WeberPenn
        | GeneratorFamily::Cactus
        | GeneratorFamily::PadChain
        | GeneratorFamily::Custom => TreeGenerator::new(species, seed).generate(),
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
        let tree = generate_tree(&species, 12345);

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

        let tree1 = generate_tree(&species, 12345);
        let tree2 = generate_tree(&species, 12345);

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

        let tree1 = generate_tree(&species, 12345);
        let tree2 = generate_tree(&species, 54321);

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
        let tree = generate_tree(&species, 42);

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
        let tree = generate_tree(&species, 42);

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
        let tree = generate_tree(&species, 42);
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

        let linear_tree = generate_tree(&species_for_profile("linear"), 42);
        let smooth_tree = generate_tree(&species_for_profile("smooth"), 42);
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
        let tree = generate_tree(&species, 42);

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
        let tree = generate_tree(&species, 42);

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

        let tree_spherical = generate_tree(&spherical, 42);
        let tree_conical = generate_tree(&conical, 42);

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
        let tree = generate_tree(&species, 42);

        // Should still generate a valid trunk
        assert!(tree.trunk().is_some());
        assert!(!tree.trunk().unwrap().segments.is_empty());
    }

    #[test]
    fn test_max_stems_limit() {
        // Test that generation respects MAX_STEMS limit
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
        let tree = generate_tree(&species, 42);

        // Should not exceed MAX_STEMS
        assert!(tree.stems.len() <= MAX_STEMS as usize);
    }

    #[test]
    fn test_species_name_preserved() {
        let toml = r#"
[species]
name = "My Custom Tree"

[trunk]
"#;

        let species = Species::from_toml(toml).unwrap();
        let tree = generate_tree(&species, 42);

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
        let tree = generate_tree(&species, 99999);

        assert_eq!(tree.seed, 99999);
    }

    #[test]
    fn test_dichotomous_generator_family_dispatch() {
        let species = Species::from_toml(DICHOTOMOUS_SPECIES_TOML).unwrap();
        assert_eq!(species.generator.family, GeneratorFamily::Dichotomous);

        let tree = generate_tree(&species, 42);

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
        let tree = generate_tree(&species, 7);
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
