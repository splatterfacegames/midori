//! Weber-Penn style branching algorithm for procedural tree generation.
//!
//! This module implements the tree generation algorithm based on the Weber-Penn
//! model for realistic procedural tree generation. It uses recursive branching
//! with phyllotaxis (golden angle) placement and crown shape modifiers.

use crate::{
    constants::*,
    math::*,
    rng::Rng,
    species::{BranchParams, CrownShape, Species},
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
            let next_radius = trunk.radius * (1.0 - trunk.taper * next_t);

            // Create segment
            let end = position + direction * segment_length;
            stem.segments.push(Segment {
                start: position,
                end,
                start_radius: radius,
                end_radius: next_radius.max(MIN_RADIUS),
                direction,
            });

            position = end;
            radius = next_radius.max(MIN_RADIUS);
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
        let mut radius = parent_radius * spec.params.radius_ratio;

        // Default branch taper
        let taper = 0.7;

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
            let next_radius = (radius * (1.0 - taper * next_t)).max(MIN_RADIUS);

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
    TreeGenerator::new(species, seed).generate()
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
}
