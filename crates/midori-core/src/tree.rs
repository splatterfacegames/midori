//! Tree data structures for procedural generation.
//!
//! This module provides the core data structures for representing generated trees,
//! including stems (trunk and branches), segments, leaves, and bounding boxes.

use glam::{Quat, Vec3};

/// A complete generated tree
#[derive(Debug, Clone)]
pub struct Tree {
    /// Name of the species used to generate this tree
    pub species_name: String,
    /// Seed used for random generation
    pub seed: u64,
    /// All stems (trunk and branches) in the tree
    pub stems: Vec<Stem>,
    /// All leaves in the tree
    pub leaves: Vec<Leaf>,
    /// Axis-aligned bounding box containing the entire tree
    pub bounds: BoundingBox,
}

/// A single stem (trunk or branch)
#[derive(Debug, Clone)]
pub struct Stem {
    /// Unique identifier for this stem
    pub id: u32,
    /// Branch level: 0 = trunk, 1-3 = branches
    pub level: u8,
    /// ID of the parent stem (None for trunk)
    pub parent_id: Option<u32>,
    /// Position along parent where this stem branches (0-1)
    pub parent_offset: f32,
    /// Segments making up this stem
    pub segments: Vec<Segment>,
    /// IDs of child stems branching from this stem
    pub child_ids: Vec<u32>,
}

/// A segment of a stem (a tapered cylinder)
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    /// Start position of the segment
    pub start: Vec3,
    /// End position of the segment
    pub end: Vec3,
    /// Radius at the start of the segment
    pub start_radius: f32,
    /// Radius at the end of the segment
    pub end_radius: f32,
    /// Direction vector of the segment (normalized)
    pub direction: Vec3,
}

/// A leaf instance
#[derive(Debug, Clone, Copy)]
pub struct Leaf {
    /// Position in world space
    pub position: Vec3,
    /// Rotation as a quaternion
    pub rotation: Quat,
    /// Scale factor relative to base leaf size
    pub scale: f32,
    /// ID of the stem this leaf is attached to
    pub stem_id: u32,
}

/// Axis-aligned bounding box
#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    /// Minimum corner of the bounding box
    pub min: Vec3,
    /// Maximum corner of the bounding box
    pub max: Vec3,
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self::new()
    }
}

impl BoundingBox {
    /// Create a new empty bounding box (inverted bounds)
    pub fn new() -> Self {
        Self {
            min: Vec3::splat(f32::MAX),
            max: Vec3::splat(f32::MIN),
        }
    }

    /// Expand the bounding box to include a point
    pub fn expand(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    /// Get the center of the bounding box
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Get the size (dimensions) of the bounding box
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Get the maximum dimension (width, height, or depth)
    pub fn max_dimension(&self) -> f32 {
        let size = self.size();
        size.x.max(size.y).max(size.z)
    }

    /// Check if the bounding box is valid (has been expanded at least once)
    pub fn is_valid(&self) -> bool {
        self.min.x <= self.max.x && self.min.y <= self.max.y && self.min.z <= self.max.z
    }
}

impl Stem {
    /// Create a new stem with the given ID and level
    pub fn new(id: u32, level: u8) -> Self {
        Self {
            id,
            level,
            parent_id: None,
            parent_offset: 0.0,
            segments: Vec::new(),
            child_ids: Vec::new(),
        }
    }

    /// Get the tip position (end of last segment)
    pub fn tip(&self) -> Vec3 {
        self.segments.last().map(|s| s.end).unwrap_or(Vec3::ZERO)
    }

    /// Get the base position (start of first segment)
    pub fn base(&self) -> Vec3 {
        self.segments.first().map(|s| s.start).unwrap_or(Vec3::ZERO)
    }

    /// Get total length of stem (sum of all segment lengths)
    pub fn length(&self) -> f32 {
        self.segments
            .iter()
            .map(|s| (s.end - s.start).length())
            .sum()
    }

    /// Get radius at the base (start of first segment)
    pub fn base_radius(&self) -> f32 {
        self.segments.first().map(|s| s.start_radius).unwrap_or(0.0)
    }

    /// Get position along stem (t in 0..1)
    pub fn point_at(&self, t: f32) -> Vec3 {
        if self.segments.is_empty() {
            return Vec3::ZERO;
        }

        let t = t.clamp(0.0, 1.0);
        let total_length = self.length();

        if total_length < f32::EPSILON {
            return self.base();
        }

        let target_length = t * total_length;
        let mut accumulated = 0.0;

        for seg in &self.segments {
            let seg_length = (seg.end - seg.start).length();
            if accumulated + seg_length >= target_length {
                let local_t = (target_length - accumulated) / seg_length.max(f32::EPSILON);
                return seg.start + (seg.end - seg.start) * local_t;
            }
            accumulated += seg_length;
        }

        self.tip()
    }

    /// Get direction at position along stem (t in 0..1)
    pub fn direction_at(&self, t: f32) -> Vec3 {
        if self.segments.is_empty() {
            return Vec3::Y;
        }

        let t = t.clamp(0.0, 1.0);
        let total_length = self.length();

        if total_length < f32::EPSILON {
            return self
                .segments
                .first()
                .map(|s| s.direction)
                .unwrap_or(Vec3::Y);
        }

        let target_length = t * total_length;
        let mut accumulated = 0.0;

        for seg in &self.segments {
            let seg_length = (seg.end - seg.start).length();
            if accumulated + seg_length >= target_length {
                return seg.direction;
            }
            accumulated += seg_length;
        }

        self.segments.last().map(|s| s.direction).unwrap_or(Vec3::Y)
    }

    /// Get radius at position along stem (t in 0..1)
    pub fn radius_at(&self, t: f32) -> f32 {
        if self.segments.is_empty() {
            return 0.0;
        }

        let t = t.clamp(0.0, 1.0);
        let total_length = self.length();

        if total_length < f32::EPSILON {
            return self.segments.first().map(|s| s.start_radius).unwrap_or(0.0);
        }

        let target_length = t * total_length;
        let mut accumulated = 0.0;

        for seg in &self.segments {
            let seg_length = (seg.end - seg.start).length();
            if accumulated + seg_length >= target_length {
                let local_t = (target_length - accumulated) / seg_length.max(f32::EPSILON);
                return seg.start_radius + (seg.end_radius - seg.start_radius) * local_t;
            }
            accumulated += seg_length;
        }

        self.segments.last().map(|s| s.end_radius).unwrap_or(0.0)
    }
}

impl Tree {
    /// Create a new empty tree
    pub fn new(species_name: String, seed: u64) -> Self {
        Self {
            species_name,
            seed,
            stems: Vec::new(),
            leaves: Vec::new(),
            bounds: BoundingBox::new(),
        }
    }

    /// Add a stem to the tree and return its ID
    pub fn add_stem(&mut self, stem: Stem) -> u32 {
        let id = stem.id;
        self.stems.push(stem);
        id
    }

    /// Get a reference to a stem by ID
    pub fn get_stem(&self, id: u32) -> Option<&Stem> {
        self.stems.iter().find(|s| s.id == id)
    }

    /// Get a mutable reference to a stem by ID
    pub fn get_stem_mut(&mut self, id: u32) -> Option<&mut Stem> {
        self.stems.iter_mut().find(|s| s.id == id)
    }

    /// Recalculate bounding box from all stems and leaves
    pub fn update_bounds(&mut self) {
        self.bounds = BoundingBox::new();

        // Include all stem segments
        for stem in &self.stems {
            for seg in &stem.segments {
                // Expand for segment endpoints with radius
                self.bounds
                    .expand(seg.start - Vec3::splat(seg.start_radius));
                self.bounds
                    .expand(seg.start + Vec3::splat(seg.start_radius));
                self.bounds.expand(seg.end - Vec3::splat(seg.end_radius));
                self.bounds.expand(seg.end + Vec3::splat(seg.end_radius));
            }
        }

        // Include all leaves
        for leaf in &self.leaves {
            // Leaves have some size, estimate as scale
            self.bounds.expand(leaf.position - Vec3::splat(leaf.scale));
            self.bounds.expand(leaf.position + Vec3::splat(leaf.scale));
        }
    }

    /// Get the trunk stem (level 0)
    pub fn trunk(&self) -> Option<&Stem> {
        self.stems.iter().find(|s| s.level == 0)
    }

    /// Get all stems at a given level
    pub fn stems_at_level(&self, level: u8) -> impl Iterator<Item = &Stem> {
        self.stems.iter().filter(move |s| s.level == level)
    }

    /// Get total number of stems
    pub fn stem_count(&self) -> usize {
        self.stems.len()
    }

    /// Get total number of leaves
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box_new() {
        let bb = BoundingBox::new();
        assert!(!bb.is_valid());
    }

    #[test]
    fn test_bounding_box_expand() {
        let mut bb = BoundingBox::new();
        bb.expand(Vec3::new(1.0, 2.0, 3.0));
        bb.expand(Vec3::new(-1.0, -2.0, -3.0));

        assert!(bb.is_valid());
        assert_eq!(bb.min, Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(bb.max, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn test_bounding_box_center() {
        let mut bb = BoundingBox::new();
        bb.expand(Vec3::new(0.0, 0.0, 0.0));
        bb.expand(Vec3::new(10.0, 20.0, 30.0));

        let center = bb.center();
        assert!((center.x - 5.0).abs() < f32::EPSILON);
        assert!((center.y - 10.0).abs() < f32::EPSILON);
        assert!((center.z - 15.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_bounding_box_size() {
        let mut bb = BoundingBox::new();
        bb.expand(Vec3::new(0.0, 0.0, 0.0));
        bb.expand(Vec3::new(10.0, 20.0, 30.0));

        let size = bb.size();
        assert!((size.x - 10.0).abs() < f32::EPSILON);
        assert!((size.y - 20.0).abs() < f32::EPSILON);
        assert!((size.z - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_bounding_box_max_dimension() {
        let mut bb = BoundingBox::new();
        bb.expand(Vec3::new(0.0, 0.0, 0.0));
        bb.expand(Vec3::new(10.0, 20.0, 30.0));

        assert!((bb.max_dimension() - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_stem_new() {
        let stem = Stem::new(5, 2);
        assert_eq!(stem.id, 5);
        assert_eq!(stem.level, 2);
        assert!(stem.parent_id.is_none());
        assert!(stem.segments.is_empty());
    }

    #[test]
    fn test_stem_with_segments() {
        let mut stem = Stem::new(0, 0);
        stem.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 1.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.4,
            direction: Vec3::Y,
        });
        stem.segments.push(Segment {
            start: Vec3::new(0.0, 1.0, 0.0),
            end: Vec3::new(0.0, 2.0, 0.0),
            start_radius: 0.4,
            end_radius: 0.3,
            direction: Vec3::Y,
        });

        assert_eq!(stem.base(), Vec3::ZERO);
        assert_eq!(stem.tip(), Vec3::new(0.0, 2.0, 0.0));
        assert!((stem.length() - 2.0).abs() < f32::EPSILON);
        assert!((stem.base_radius() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_stem_point_at() {
        let mut stem = Stem::new(0, 0);
        stem.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 2.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.4,
            direction: Vec3::Y,
        });

        let mid = stem.point_at(0.5);
        assert!((mid.y - 1.0).abs() < 0.001);

        let start = stem.point_at(0.0);
        assert!((start - Vec3::ZERO).length() < 0.001);

        let end = stem.point_at(1.0);
        assert!((end - Vec3::new(0.0, 2.0, 0.0)).length() < 0.001);
    }

    #[test]
    fn test_stem_radius_at() {
        let mut stem = Stem::new(0, 0);
        stem.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 2.0, 0.0),
            start_radius: 1.0,
            end_radius: 0.5,
            direction: Vec3::Y,
        });

        assert!((stem.radius_at(0.0) - 1.0).abs() < 0.001);
        assert!((stem.radius_at(1.0) - 0.5).abs() < 0.001);
        assert!((stem.radius_at(0.5) - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_tree_new() {
        let tree = Tree::new("Oak".to_string(), 12345);
        assert_eq!(tree.species_name, "Oak");
        assert_eq!(tree.seed, 12345);
        assert!(tree.stems.is_empty());
        assert!(tree.leaves.is_empty());
    }

    #[test]
    fn test_tree_add_and_get_stem() {
        let mut tree = Tree::new("Test".to_string(), 0);

        let stem = Stem::new(0, 0);
        tree.add_stem(stem);

        assert_eq!(tree.stem_count(), 1);
        assert!(tree.get_stem(0).is_some());
        assert!(tree.get_stem(1).is_none());
    }

    #[test]
    fn test_tree_trunk() {
        let mut tree = Tree::new("Test".to_string(), 0);

        let trunk = Stem::new(0, 0);
        let branch = Stem::new(1, 1);
        tree.add_stem(trunk);
        tree.add_stem(branch);

        let trunk_ref = tree.trunk().unwrap();
        assert_eq!(trunk_ref.level, 0);
    }

    #[test]
    fn test_tree_stems_at_level() {
        let mut tree = Tree::new("Test".to_string(), 0);

        tree.add_stem(Stem::new(0, 0));
        tree.add_stem(Stem::new(1, 1));
        tree.add_stem(Stem::new(2, 1));
        tree.add_stem(Stem::new(3, 2));

        let level1_count = tree.stems_at_level(1).count();
        assert_eq!(level1_count, 2);

        let level2_count = tree.stems_at_level(2).count();
        assert_eq!(level2_count, 1);
    }

    #[test]
    fn test_tree_update_bounds() {
        let mut tree = Tree::new("Test".to_string(), 0);

        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 5.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.3,
            direction: Vec3::Y,
        });
        tree.add_stem(trunk);

        tree.update_bounds();

        assert!(tree.bounds.is_valid());
        assert!(tree.bounds.max.y >= 5.0);
    }
}
