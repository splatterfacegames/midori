//! Mesh builder for generating branch geometry.

use crate::{
    constants::{GOLDEN_RATIO, MAX_BRANCH_LEVELS, TAU},
    math::create_basis,
    mesh::{MaterialType, Mesh, Submesh, Vertex},
    tree::{Stem, Tree},
};
use glam::{Vec2, Vec3, Vec4};
use std::collections::HashMap;

const RING_T_EPSILON: f32 = 0.0001;
const NORMAL_SMOOTH_POSITION_SCALE: f32 = 100_000.0;

/// Configuration for mesh generation
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Ring resolution per branch level [trunk, level1, level2, level3]
    pub ring_resolution: [u32; 4],
    /// Texture V scale (repeats per meter)
    pub texture_v_scale: f32,
    /// Enable Pivot Painter data encoding
    pub pivot_painter: bool,
    /// Branch collar swelling factor (1.0 = no swelling, 1.5 = 50% larger at joints)
    pub branch_collar_swell: f32,
    /// How far the swelling extends (0.0-1.0, as fraction of branch length)
    pub collar_falloff: f32,
    /// Trunk base flare multiplier at ground level (1.0 = no flare)
    pub trunk_base_flare: f32,
    /// How far trunk flare extends (0.0-1.0, as fraction of trunk length)
    pub trunk_base_flare_height: f32,
    /// Branch base swell multiplier at parent attachment (1.0 = no swell)
    pub branch_base_swell: f32,
    /// How far branch base swell extends (0.0-1.0, as fraction of branch length)
    pub branch_base_falloff: f32,
    /// Strength of the trunk's root-buttress flutes and exposed surface roots
    /// (1.0 = the built-in look, 0.0 = a plain column with no surface roots,
    /// above 1.0 = heavier buttresses). Set from `trunk.buttress`.
    pub trunk_buttress: f32,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            // Higher resolution for production-quality trees
            // Trunk needs 24+ segments for smooth appearance at close range
            ring_resolution: [24, 16, 10, 6],
            texture_v_scale: 1.0,
            pivot_painter: true,
            branch_collar_swell: 1.24,
            collar_falloff: 0.085,
            trunk_base_flare: 1.38,
            trunk_base_flare_height: 0.18,
            branch_base_swell: 1.32,
            branch_base_falloff: 0.13,
            trunk_buttress: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RingFrame {
    tangent: Vec3,
    bitangent: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct RingSample {
    position: Vec3,
    radius: f32,
    direction: Vec3,
    v_coord: f32,
    t_norm: f32,
}

#[derive(Debug, Clone, Copy)]
struct ChildAttachment {
    offset_t: f32,
    radial_dir: Vec3,
    weight: f32,
}

impl RingFrame {
    fn from_direction(direction: Vec3) -> Self {
        let direction = normalized_or_up(direction);
        let (tangent, bitangent) = create_basis(direction);
        Self {
            tangent,
            bitangent: bitangent.normalize_or_zero(),
        }
    }

    fn transported(self, direction: Vec3) -> Self {
        let direction = normalized_or_up(direction);
        let projected = self.tangent - direction * self.tangent.dot(direction);
        let tangent = if projected.length_squared() > 0.0001 {
            projected.normalize()
        } else {
            create_basis(direction).0
        };
        let bitangent = direction.cross(tangent).normalize_or_zero();

        Self { tangent, bitangent }
    }
}

/// Build mesh from generated tree
pub struct MeshBuilder<'a> {
    tree: &'a Tree,
    config: MeshConfig,
    mesh: Mesh,
    max_branch_level: Option<u32>,
    cap_center_indices: Vec<u32>,
}

impl<'a> MeshBuilder<'a> {
    /// Create a new mesh builder for the given tree
    pub fn new(tree: &'a Tree, config: MeshConfig) -> Self {
        Self {
            tree,
            config,
            mesh: Mesh::new(),
            max_branch_level: None,
            cap_center_indices: Vec::new(),
        }
    }

    /// Build complete branch mesh
    pub fn build_branches(mut self) -> Mesh {
        for stem in &self.tree.stems {
            self.build_stem(stem);
        }

        self.smooth_coincident_normals();

        // Add bark submesh for all branch geometry
        if !self.mesh.indices.is_empty() {
            self.mesh.submeshes.push(Submesh {
                index_start: 0,
                index_count: self.mesh.indices.len() as u32,
                material: MaterialType::Bark,
            });
        }

        self.mesh
    }

    /// Build branches up to max_level depth
    ///
    /// This is useful for LOD generation where lower LOD levels
    /// should exclude smaller branches.
    ///
    /// # Arguments
    ///
    /// * `max_level` - Maximum branch level to include (0 = trunk only, 1 = trunk + primary branches, etc.)
    pub fn build_branches_to_level(mut self, max_level: u32) -> Mesh {
        self.max_branch_level = Some(max_level);

        for stem in &self.tree.stems {
            if stem.level as u32 <= max_level {
                self.build_stem(stem);
            }
        }

        self.smooth_coincident_normals();

        // Add bark submesh for all branch geometry
        if !self.mesh.indices.is_empty() {
            self.mesh.submeshes.push(Submesh {
                index_start: 0,
                index_count: self.mesh.indices.len() as u32,
                material: MaterialType::Bark,
            });
        }

        self.mesh
    }

    /// Build stems at or below min_level depth — the branches a LOD cut
    /// away. The impostor baker rasterizes these into the crown atlas so
    /// the baked card still shows the twiggy interior.
    ///
    /// # Arguments
    ///
    /// * `min_level` - Minimum branch level to include
    pub fn build_stems_from_level(mut self, min_level: u32) -> Mesh {
        for stem in &self.tree.stems {
            if stem.level as u32 >= min_level {
                self.build_stem(stem);
            }
        }

        // Add bark submesh for all branch geometry
        if !self.mesh.indices.is_empty() {
            self.mesh.submeshes.push(Submesh {
                index_start: 0,
                index_count: self.mesh.indices.len() as u32,
                material: MaterialType::Bark,
            });
        }

        self.mesh
    }

    fn build_stem(&mut self, stem: &Stem) {
        if stem.segments.is_empty() {
            return;
        }

        // Get average radius for adaptive resolution calculation
        let avg_radius = stem
            .segments
            .iter()
            .map(|s| (s.start_radius + s.end_radius) / 2.0)
            .sum::<f32>()
            / stem.segments.len() as f32;

        // Use adaptive resolution based on branch thickness
        let ring_res = self.adaptive_ring_resolution(stem.level, avg_radius);
        if ring_res < 3 {
            return; // Too few vertices to make a cylinder
        }

        // Calculate total stem length for position calculations
        let total_length: f32 = stem
            .segments
            .iter()
            .map(|s| (s.end - s.start).length())
            .sum();

        let attachments = self.included_child_attachments(stem);
        let term_children = self.included_terminal_child_count(stem);
        let samples = self.ring_samples(stem, &attachments, term_children, total_length, ring_res);
        if samples.len() < 2 {
            return;
        }
        let parent_weld_info = self.child_weld_info(stem, total_length);

        // Generate ring vertices for each segment joint.
        // Keep the ground-level trunk ring horizontal so tilted trunks sit flush on y = 0.
        let mut rings: Vec<Vec<u32>> = Vec::new();
        let initial_dir = if stem.level == 0 {
            Vec3::Y
        } else {
            samples[0].direction
        };
        let mut frame = RingFrame::from_direction(initial_dir);

        for (sample_idx, sample) in samples.iter().enumerate() {
            if sample_idx > 0 {
                frame = frame.transported(sample.direction);
            }
            let ring = self.create_ring(
                sample,
                frame,
                ring_res,
                stem,
                &attachments,
                term_children,
                parent_weld_info,
            );
            rings.push(ring);
        }

        // Connect adjacent rings with faces
        for i in 0..rings.len() - 1 {
            self.connect_rings(&rings[i], &rings[i + 1]);
        }

        // Cap the end unless a single included child continues directly from the tip.
        // Multi-child terminal forks (e.g., dichotomous Y-splits) are capped so wide parent
        // rims never leave open annular shelves around angled child collars.
        if term_children != 1 && !rings.is_empty() {
            let tip_normal = normalized_or_up(stem.segments.last().unwrap().direction);
            self.cap_ring(rings.last().unwrap(), tip_normal, stem);
        }

        // Sculptural 3D exposed surface roots (nebari) flowing out of trunk buttress flutes
        let base_r = stem.segments[0].start_radius;
        let tip_r = stem.segments.last().unwrap().end_radius;
        if stem.level == 0
            && ring_res >= 16
            && stem.segments.len() >= 10
            && base_r >= 0.25
            && tip_r < base_r * 0.44
            && self.config.trunk_buttress > 0.0
        {
            self.build_trunk_roots(stem, ring_res);
        }
    }

    fn included_terminal_child_count(&self, stem: &Stem) -> usize {
        stem.child_ids
            .iter()
            .filter_map(|&child_id| self.tree.stems.iter().find(|s| s.id == child_id))
            .filter(|child| self.includes_stem(child))
            .filter(|child| child.parent_offset.clamp(0.0, 1.0) >= 1.0 - RING_T_EPSILON * 2.0)
            .count()
    }

    fn parent_terminal_fork_info(&self, stem: &Stem) -> Option<(Vec3, Vec3, f32)> {
        if stem.level == 0 || stem.parent_offset <= 0.94 {
            return None;
        }
        let parent_id = stem.parent_id?;
        let parent = self.tree.stems.iter().find(|s| s.id == parent_id)?;
        if self.included_terminal_child_count(parent) < 2 || parent.segments.is_empty() {
            return None;
        }
        let p_len = parent.length();
        let p_avg_r = parent
            .segments
            .iter()
            .map(|s| (s.start_radius + s.end_radius) * 0.5)
            .sum::<f32>()
            / parent.segments.len() as f32;
        let p_res = self.adaptive_ring_resolution(parent.level, p_avg_r);
        let (tip_pos, tip_dir) = if p_res >= 12 && parent.segments.len() >= 4 {
            self.smooth_stem_pose(parent, 1.0, p_len)
        } else {
            (
                parent.tip(),
                normalized_or_up(parent.segments.last().unwrap().direction),
            )
        };
        let tip_r = parent.radius_at(1.0);
        Some((tip_pos, tip_dir, tip_r))
    }

    /// Returns `(parent_outward_dir, t_junction, parent_radius, is_terminal_fork)` for child collar welding.
    fn child_weld_info(&self, stem: &Stem, total_length: f32) -> Option<(Vec3, f32, f32, bool)> {
        if stem.level == 0 || total_length <= f32::EPSILON {
            return None;
        }
        let parent_id = stem.parent_id?;
        let parent = self.tree.stems.iter().find(|s| s.id == parent_id)?;
        if stem.parent_offset > 0.94 {
            if self.included_terminal_child_count(parent) >= 2 && !parent.segments.is_empty() {
                let p_dir = normalized_or_up(parent.segments.last().unwrap().direction);
                let p_r = parent.radius_at(1.0);
                return Some((p_dir, 0.0, p_r, true));
            }
            return None;
        }
        let p_dir = parent.direction_at(stem.parent_offset);
        let p_r = parent.radius_at(stem.parent_offset);
        let c_dir = stem
            .segments
            .first()
            .map(|s| normalized_or_up(s.direction))
            .unwrap_or(Vec3::X);
        let sin_ang = (1.0 - p_dir.dot(c_dir).clamp(-0.98, 0.98).powi(2))
            .max(0.16)
            .sqrt();
        let t_exit = ((p_r * 0.92) / sin_ang / total_length).clamp(0.025, 0.28);
        let radial = (c_dir - p_dir * c_dir.dot(p_dir))
            .try_normalize()
            .or_else(|| c_dir.try_normalize())?;
        Some((radial, t_exit, p_r, false))
    }

    fn included_child_attachments(&self, stem: &Stem) -> Vec<ChildAttachment> {
        let mut list: Vec<ChildAttachment> = stem
            .child_ids
            .iter()
            .filter_map(|&child_id| self.tree.stems.iter().find(|s| s.id == child_id))
            .filter(|child| self.includes_stem(child))
            .map(|child| {
                let offset_t = child.parent_offset.clamp(0.0, 1.0);
                let p_dir = stem.direction_at(offset_t);
                let c_dir = child
                    .segments
                    .first()
                    .map(|s| s.direction)
                    .unwrap_or(Vec3::X);
                let radial_dir = (c_dir - p_dir * c_dir.dot(p_dir))
                    .try_normalize()
                    .unwrap_or_else(|| normalized_or_up(c_dir));
                let weight = if stem.segments.len() >= 4 {
                    let pr = stem.radius_at(offset_t).max(0.01);
                    (child.base_radius() / pr * 1.45).clamp(0.22, 1.0)
                } else {
                    1.0
                };
                ChildAttachment {
                    offset_t,
                    radial_dir,
                    weight,
                }
            })
            .collect();

        list.sort_by(|a, b| a.offset_t.total_cmp(&b.offset_t));
        list
    }

    fn includes_stem(&self, stem: &Stem) -> bool {
        self.max_branch_level
            .map(|max_level| stem.level as u32 <= max_level)
            .unwrap_or(true)
    }

    /// Evaluate smooth Catmull-Rom centerline position and tangent along a multi-segment stem.
    fn smooth_stem_pose(&self, stem: &Stem, t: f32, total_length: f32) -> (Vec3, Vec3) {
        let n = stem.segments.len();
        if n < 3 || total_length <= f32::EPSILON {
            return (
                stem.point_at(t),
                self.direction_at_ring(stem, t, total_length),
            );
        }

        let target_len = t.clamp(0.0, 1.0) * total_length;
        let mut acc = 0.0_f32;
        let mut seg_idx = n - 1;
        let mut local_u = 1.0_f32;
        for (idx, seg) in stem.segments.iter().enumerate() {
            let sl = (seg.end - seg.start).length();
            if target_len <= acc + sl || idx + 1 == n {
                seg_idx = idx;
                local_u = if sl > f32::EPSILON {
                    ((target_len - acc) / sl).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                break;
            }
            acc += sl;
        }

        let knot = |k: isize| -> Vec3 {
            if k < 0 {
                let p0 = stem.segments[0].start;
                let p1 = stem.segments[0].end;
                p0 * 2.0 - p1
            } else if k == 0 {
                stem.segments[0].start
            } else if (k as usize) <= n {
                stem.segments[k as usize - 1].end
            } else {
                let pn = stem.segments[n - 1].end;
                let pm = stem.segments[n - 1].start;
                pn * 2.0 - pm
            }
        };

        let i = seg_idx as isize;
        let p0 = knot(i - 1);
        let p1 = knot(i);
        let p2 = knot(i + 1);
        let p3 = knot(i + 2);

        let u = local_u;
        let u2 = u * u;
        let u3 = u2 * u;

        let pos = 0.5
            * ((2.0 * p1)
                + (-p0 + p2) * u
                + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * u2
                + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * u3);
        let deriv = 0.5
            * ((-p0 + p2)
                + (4.0 * p0 - 10.0 * p1 + 8.0 * p2 - 2.0 * p3) * u
                + (-3.0 * p0 + 9.0 * p1 - 9.0 * p2 + 3.0 * p3) * u2);
        let dir = deriv
            .try_normalize()
            .unwrap_or_else(|| self.direction_at_ring(stem, t, total_length));
        (pos, dir)
    }

    fn ring_samples(
        &self,
        stem: &Stem,
        attachments: &[ChildAttachment],
        term_children: usize,
        total_length: f32,
        ring_res: u32,
    ) -> Vec<RingSample> {
        let smooth_subdiv = ring_res >= 12 && stem.segments.len() >= 4;
        let mut t_values = Vec::with_capacity(stem.segments.len() * 2 + attachments.len() + 4);
        t_values.push(0.0);

        let mut accumulated_length = 0.0;
        for seg in &stem.segments {
            let prev_t = if total_length > f32::EPSILON {
                accumulated_length / total_length
            } else {
                0.0
            };
            accumulated_length += (seg.end - seg.start).length();
            let t = if total_length > f32::EPSILON {
                accumulated_length / total_length
            } else {
                1.0
            };
            if smooth_subdiv && stem.level <= 2 {
                t_values.push(((prev_t + t) * 0.5).clamp(0.0, 1.0));
            }
            t_values.push(t.clamp(0.0, 1.0));
        }

        let weld_info = self.child_weld_info(stem, total_length);
        if smooth_subdiv {
            if let Some((_, t_exit, _, is_term)) = weld_info {
                if is_term {
                    t_values.push(0.042);
                    t_values.push(0.105);
                } else {
                    t_values.push((t_exit * 0.55).clamp(0.01, 0.90));
                    t_values.push((t_exit * 0.86).clamp(0.015, 0.92));
                    t_values.push((t_exit * 1.02).clamp(0.02, 0.94));
                    t_values.push((t_exit * 1.28).clamp(0.03, 0.95));
                    t_values.push((t_exit * 1.35 + 0.045).clamp(0.04, 0.97));
                    t_values.push((t_exit * 1.35 + 0.095).clamp(0.06, 0.985));
                }
            }
            if term_children >= 2 {
                t_values.push(0.91);
                t_values.push(0.965);
            }
        }

        for att in attachments {
            t_values.push(att.offset_t);
        }
        t_values.sort_by(|a, b| a.total_cmp(b));
        t_values.dedup_by(|a, b| (*a - *b).abs() < RING_T_EPSILON);

        let base_radius = stem
            .segments
            .first()
            .map(|s| s.start_radius)
            .unwrap_or(0.15);
        // Scale axial V coordinate by stem circumference so bark texels stay sharp and 1:1 square.
        let circ = (TAU * base_radius).clamp(0.20, 1.85);
        let axial_v_repeats = total_length / circ;

        // Lock child branch start to parent's smoothed spline centerline when parent is subdivided
        let parent_anchor_delta = if smooth_subdiv && stem.level > 0 && stem.parent_offset <= 0.94 {
            stem.parent_id
                .and_then(|pid| self.tree.stems.iter().find(|s| s.id == pid))
                .map(|parent| {
                    let p_len = parent.length();
                    let p_avg_r = parent
                        .segments
                        .iter()
                        .map(|s| (s.start_radius + s.end_radius) * 0.5)
                        .sum::<f32>()
                        / parent.segments.len().max(1) as f32;
                    if self.adaptive_ring_resolution(parent.level, p_avg_r) >= 12
                        && parent.segments.len() >= 4
                    {
                        self.smooth_stem_pose(parent, stem.parent_offset, p_len).0
                            - parent.point_at(stem.parent_offset)
                    } else {
                        Vec3::ZERO
                    }
                })
                .unwrap_or(Vec3::ZERO)
        } else {
            Vec3::ZERO
        };

        let mut samples: Vec<RingSample> = t_values
            .into_iter()
            .map(|t| {
                let (mut position, direction) = if smooth_subdiv {
                    self.smooth_stem_pose(stem, t, total_length)
                } else {
                    (
                        stem.point_at(t),
                        self.direction_at_ring(stem, t, total_length),
                    )
                };
                if parent_anchor_delta.length_squared() > 1e-8 {
                    let fade = 1.0 - (t / 0.36).clamp(0.0, 1.0);
                    position += parent_anchor_delta * (fade * fade);
                }
                let raw_r = stem.radius_at(t);
                let radius = if smooth_subdiv && let Some((_, t_exit, p_r, false)) = weld_info {
                    // Lateral branch exit fillet: pinch buried centerline root (t=0) so it never
                    // pokes out the opposite/side trunk wall, and flare collar smoothly across
                    // the oblique bark exit ellipse (out to ~1.35 * t_exit + 0.09).
                    let peak = (raw_r * 1.16).min((p_r * 0.70).max(raw_r * 1.01));
                    let collar_end = (t_exit * 1.35 + 0.09).min(0.65);
                    let collar_r = if t < t_exit * 0.92 {
                        let u = (t / (t_exit * 0.92).max(0.01)).clamp(0.0, 1.0);
                        let s = u * u * (3.0 - 2.0 * u);
                        raw_r * 0.44 + (peak - raw_r * 0.44) * s
                    } else if t < collar_end {
                        let span = (collar_end - t_exit * 0.92).max(0.02);
                        let u = ((t - t_exit * 0.92) / span).clamp(0.0, 1.0);
                        let s = u * u * (3.0 - 2.0 * u);
                        peak + (raw_r - peak) * s
                    } else {
                        raw_r
                    };
                    let tip_scale = if term_children >= 2 && t > 0.84 {
                        let u = ((t - 0.84) / 0.16).clamp(0.0, 1.0);
                        1.0 - 0.26 * (u * u * (3.0 - 2.0 * u))
                    } else {
                        1.0
                    };
                    collar_r * tip_scale
                } else if smooth_subdiv && term_children >= 2 && t > 0.84 {
                    let u = ((t - 0.84) / 0.16).clamp(0.0, 1.0);
                    let tip_scale = 1.0 - 0.26 * (u * u * (3.0 - 2.0 * u));
                    raw_r * self.calculate_base_swell(stem.level, t) * tip_scale
                } else {
                    raw_r
                        * self.calculate_radius_scale(
                            stem.level,
                            stem.parent_offset,
                            t,
                            term_children,
                        )
                };
                RingSample {
                    position,
                    radius,
                    direction,
                    v_coord: t * axial_v_repeats * self.config.texture_v_scale,
                    t_norm: t,
                }
            })
            .collect();

        // Sink high-detail trunk base well below y = 0 so spreading buttresses & roots anchor cleanly into soil
        if stem.level == 0 && ring_res >= 16 && !samples.is_empty() {
            samples[0].position.y -= 0.085;
        }

        // Seamless union for dichotomous / terminal multi-branch Y-forks (e.g. Joshua Tree):
        // Child bases envelop the tapered parent tip and curve smoothly into their fork angles.
        if let Some((parent_tip, parent_dir, parent_tip_r)) = self.parent_terminal_fork_info(stem) {
            if smooth_subdiv && samples.len() >= 3 {
                let target_r0 = samples[0].radius.max(parent_tip_r * 0.77);
                samples[0].radius = target_r0;
                samples[0].position = parent_tip - parent_dir * (parent_tip_r * 0.24);
                samples[0].direction = parent_dir;
                samples[1].radius = samples[1].radius.max(parent_tip_r * 0.75);
                samples[1].direction = (parent_dir * 0.42 + samples[1].direction * 0.58)
                    .try_normalize()
                    .unwrap_or(samples[1].direction);
            } else if samples.len() >= 2 {
                samples[0].radius = samples[0].radius.min(parent_tip_r * 0.78);
                let tuck = (samples[0].radius * 0.34).min(total_length * 0.18);
                samples[0].position = parent_tip - parent_dir * tuck;
                samples[0].direction = parent_dir;
            }
        } else if !smooth_subdiv && stem.level > 0 && samples.len() >= 2 {
            let tuck = (samples[0].radius * 0.48).min(total_length * 0.22);
            let dir0 = samples[0].direction;
            samples[0].position -= dir0 * tuck;
        }

        samples
    }

    fn direction_at_ring(&self, stem: &Stem, t: f32, total_length: f32) -> Vec3 {
        if stem.segments.is_empty() {
            return Vec3::Y;
        }

        if t <= RING_T_EPSILON {
            return normalized_or_up(stem.segments[0].direction);
        }

        if t >= 1.0 - RING_T_EPSILON || total_length <= f32::EPSILON {
            return normalized_or_up(stem.segments.last().unwrap().direction);
        }

        let target_length = t.clamp(0.0, 1.0) * total_length;
        let mut accumulated = 0.0;

        for (seg_idx, seg) in stem.segments.iter().enumerate() {
            let seg_length = (seg.end - seg.start).length();
            let segment_end = accumulated + seg_length;

            if (target_length - segment_end).abs() <= total_length * RING_T_EPSILON
                && seg_idx + 1 < stem.segments.len()
            {
                let next = &stem.segments[seg_idx + 1];
                return normalized_or(seg.direction + next.direction, seg.direction);
            }

            if target_length <= segment_end {
                return normalized_or_up(seg.direction);
            }

            accumulated = segment_end;
        }

        normalized_or_up(stem.segments.last().unwrap().direction)
    }

    fn calculate_radius_scale(
        &self,
        level: u8,
        parent_offset: f32,
        t: f32,
        term_children: usize,
    ) -> f32 {
        let mut rim_scale = 1.0;
        if term_children >= 2 && t > 0.80 {
            let tip_fade = ((1.0 - t) / 0.20).clamp(0.0, 1.0);
            rim_scale *= 1.0 - 0.09 * (1.0 - tip_fade);
        }
        let base_swell = if level > 0 && parent_offset > 0.94 {
            1.0
        } else {
            self.calculate_base_swell(level, t)
        };
        rim_scale * base_swell
    }

    /// Directional anatomical branch-shoulder swelling on the side of the parent stem facing
    /// each attached child branch (eliminating symmetric 360-degree sausage bulges).
    fn directional_collar_swell(
        &self,
        t: f32,
        offset: Vec3,
        attachments: &[ChildAttachment],
        term_children: usize,
    ) -> f32 {
        if attachments.is_empty() || self.config.branch_collar_swell <= 1.0 {
            return 1.0;
        }

        let falloff = self.config.collar_falloff.max(0.001);
        let max_extra = self.config.branch_collar_swell - 1.0;
        let tip_fade = if term_children != 1 && t > 0.80 {
            ((1.0 - t) / 0.20).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let mut extra = 0.0_f32;
        for att in attachments {
            let dist = (t - att.offset_t).abs();
            if dist < falloff {
                let axial = (1.0 - dist / falloff).powi(2);
                let align = ((offset.dot(att.radial_dir) + 0.12) / 0.92).clamp(0.0, 1.0);
                let ang_smooth = align * align * (3.0 - 2.0 * align);
                extra = extra.max(max_extra * att.weight * axial * ang_smooth * tip_fade);
            }
        }

        1.0 + extra
    }

    fn calculate_base_swell(&self, level: u8, t: f32) -> f32 {
        let (max_swell, falloff) = if level == 0 {
            (
                self.config.trunk_base_flare,
                self.config.trunk_base_flare_height,
            )
        } else {
            (
                self.config.branch_base_swell,
                self.config.branch_base_falloff,
            )
        };

        if max_swell <= 1.0 || falloff <= 0.0 || t >= falloff {
            return 1.0;
        }

        let x = (1.0 - t / falloff).clamp(0.0, 1.0);
        let smooth = x * x * (3.0 - 2.0 * x);
        1.0 + (max_swell - 1.0) * smooth
    }

    #[allow(clippy::too_many_arguments)]
    fn create_ring(
        &mut self,
        sample: &RingSample,
        frame: RingFrame,
        resolution: u32,
        stem: &Stem,
        attachments: &[ChildAttachment],
        term_children: usize,
        parent_weld_info: Option<(Vec3, f32, f32, bool)>,
    ) -> Vec<u32> {
        let center = sample.position;
        let radius = sample.radius;
        let v_coord = sample.v_coord;
        let t_norm = sample.t_norm;

        // +1 for seam vertex at U=1.0 to avoid texture seam artifacts
        let mut ring_indices = Vec::with_capacity(resolution as usize + 1);
        let trunk_len = if stem.level == 0 {
            stem.length().max(0.5)
        } else {
            1.0
        };
        let h_frac = if stem.level == 0 {
            ((center.y - stem.base().y) / trunk_len).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let seed_phase = (self.tree.seed as f32 * 0.618_034).fract() * TAU;
        let stem_phase =
            ((self.tree.seed as f32 * 0.618_034 + stem.id as f32 * 0.381_966).fract()) * TAU;

        // Generate resolution+1 vertices (0 to resolution inclusive)
        // Last vertex duplicates position of first but has U=1.0 for proper UV wrap
        for i in 0..=resolution {
            let angle = if i == resolution {
                0.0
            } else {
                (i as f32 / resolution as f32) * TAU
            };
            let offset = frame.tangent * angle.cos() + frame.bitangent * angle.sin();
            let tangent_dir = -frame.tangent * angle.sin() + frame.bitangent * angle.cos();

            let shoulder_mul =
                self.directional_collar_swell(t_norm, offset, attachments, term_children);

            // Asymmetric underside reaction-wood bracket right across the oblique bark-surface exit collar
            let crotch_bracket = if let Some((_, t_exit, p_r, false)) = parent_weld_info
                && resolution >= 12
                && t_norm > t_exit * 0.55
                && t_norm < t_exit * 1.35 + 0.085
            {
                let center_t = t_exit * 1.06;
                let span = (t_exit * 0.45 + 0.085).max(0.04);
                let dist = ((t_norm - center_t).abs() / span).clamp(0.0, 1.0);
                let bell = (1.0 - dist).powi(2);
                let under = (-offset.y).clamp(0.0, 1.0);
                let raw_mul = 1.0 + 0.16 * bell * (0.20 + 0.80 * under);
                let max_mul = (p_r * 0.74 / radius.max(0.005)).max(1.0);
                raw_mul.min(max_mul)
            } else {
                1.0
            };

            let (buttress_mul, normal_twist) =
                if stem.level == 0 && resolution >= 16 && h_frac < 0.36 {
                    // 5-lobed organic asymmetric root buttress flutes aligned directly with the 5 nebari surface roots
                    let falloff = (1.0 - h_frac / 0.36).powi(2);
                    let root_base_az = seed_phase * 0.20;
                    let spiral = h_frac * 0.95;
                    let a5 = (angle - root_base_az) * 5.0 - spiral;
                    let a3 = angle * 3.0 + seed_phase + spiral * 0.6;
                    let asym = 0.78 + 0.22 * (angle * 2.0 - seed_phase).cos();
                    let raw = 0.68 * asym * a5.cos() + 0.32 * a3.cos();
                    let raw_d = -0.68 * asym * 5.0 * a5.sin() - 0.32 * 3.0 * a3.sin();
                    let toe = raw.max(0.0).powf(1.35) * 0.74 - (-raw).max(0.0) * 0.16;
                    let flute_ridge = if resolution >= 20 {
                        let m1 = angle * 4.0 + v_coord * 0.72 + stem_phase;
                        let r1 = angle * 13.0 + v_coord * 2.2 + stem_phase;
                        let r2 = angle * 21.0 - v_coord * 3.5 + seed_phase;
                        0.042 * m1.cos() + 0.028 * r1.cos() + 0.015 * r2.sin()
                    } else {
                        0.0
                    };
                    let k = self.config.trunk_buttress;
                    if k == 1.0 {
                        (1.0 + falloff * toe + flute_ridge, -raw_d * 0.15 * falloff)
                    } else {
                        (
                            1.0 + (falloff * toe + flute_ridge) * k,
                            -raw_d * 0.15 * falloff * k,
                        )
                    }
                } else if stem.level == 0 && resolution >= 8 && h_frac < 0.28 {
                    let falloff = (1.0 - h_frac / 0.28).powi(2);
                    let phase = angle * 5.0 + seed_phase + 0.38 * (angle * 2.0 - seed_phase).sin();
                    let lobe = phase.cos().max(-0.28);
                    let deriv = -phase.sin() * 0.30 * falloff;
                    let k = self.config.trunk_buttress;
                    (1.0 + 0.28 * falloff * lobe * k, deriv * k)
                } else if resolution >= 12 && stem.level <= 2 {
                    // Twisting 3D sapwood muscle cords + craggy bark furrow relief across trunks & boughs
                    let (muscle_amp, bark_amp) = match stem.level {
                        0 => (0.056, 0.034),
                        1 => (0.040, 0.022),
                        _ => (0.024, 0.012),
                    };
                    let m1 = angle * 3.0 + v_coord * 0.68 + stem_phase;
                    let m2 = angle * 5.0 - v_coord * 0.96 + seed_phase;
                    let muscle = muscle_amp * (0.62 * m1.cos() + 0.38 * m2.sin());
                    let (ridge, ridge_d) = if resolution >= 18 {
                        let p1 = angle * 11.0 + v_coord * 1.9 + stem_phase;
                        let p2 = angle * 19.0 - v_coord * 3.1 + seed_phase;
                        (
                            bark_amp * (0.66 * p1.cos() + 0.34 * p2.sin()),
                            bark_amp * (-0.66 * 11.0 * p1.sin() + 0.34 * 19.0 * p2.cos()),
                        )
                    } else {
                        (0.0, 0.0)
                    };
                    let deriv = (muscle_amp * (-0.62 * 3.0 * m1.sin() + 0.38 * 5.0 * m2.cos())
                        + ridge_d)
                        * 0.14;
                    (1.0 + muscle + ridge, deriv)
                } else {
                    (1.0, 0.0)
                };

            // Gravity reaction-wood elliptical cross-section on horizontal/angled boughs
            let ellip_mul = if stem.level >= 1 && resolution >= 12 {
                let horiz = (1.0 - sample.direction.y.abs()).clamp(0.0, 1.0);
                let vert_align = offset.y * offset.y;
                1.0 + horiz * (0.13 * vert_align - 0.05 * (1.0 - vert_align))
            } else {
                1.0
            };

            // Localized asymmetric 3D callus burls / healed branch knuckles on ancient trunks & primary limbs
            let burl_mul = if stem.level <= 1 && resolution >= 16 && stem.segments.len() >= 8 {
                let b1_t = 0.24 + 0.24 * ((stem_phase * 0.37).sin() * 0.5 + 0.5);
                let b2_t = 0.58 + 0.22 * ((stem_phase * 0.73 + seed_phase).cos() * 0.5 + 0.5);
                let b1_ang = stem_phase * 1.3 + seed_phase * 0.5;
                let b2_ang = stem_phase * 2.4 - seed_phase * 0.8;
                let d1 = (t_norm - b1_t) / 0.065;
                let d2 = (t_norm - b2_t) / 0.058;
                let a1 = (angle - b1_ang).cos().max(0.0).powi(3);
                let a2 = (angle - b2_ang).cos().max(0.0).powi(3);
                let swell = 0.16 * (-0.5 * d1 * d1).exp() * a1 + 0.14 * (-0.5 * d2 * d2).exp() * a2;
                1.0 + swell
            } else {
                1.0
            };

            let clamped_twist = normal_twist.clamp(-0.38, 0.38);
            let total_r =
                radius * shoulder_mul * crotch_bracket * buttress_mul * ellip_mul * burl_mul;
            let position = center + offset * total_r;
            let mut normal = (offset + tangent_dir * clamped_twist).normalize_or_zero();

            // Smoothly weld child collar normals into parent bark surface right at the visible exit seam
            if let Some((p_out, t_exit, _, is_term)) = parent_weld_info {
                if is_term && t_norm < 0.10 {
                    let weld = ((1.0 - t_norm / 0.10).clamp(0.0, 1.0)).powi(2) * 0.52;
                    let target = (offset + p_out * 0.35).normalize_or_zero();
                    normal = (normal * (1.0 - weld) + target * weld).normalize_or_zero();
                } else if !is_term && t_norm > t_exit * 0.55 && t_norm < t_exit * 1.35 + 0.085 {
                    let center_t = t_exit * 1.06;
                    let half_w = (t_exit * 0.45 + 0.085).max(0.04);
                    let dist = ((t_norm - center_t).abs() / half_w).clamp(0.0, 1.0);
                    let weld = (1.0 - dist * dist) * 0.78;
                    let collar_target = (p_out + offset * 0.45).normalize_or_zero();
                    normal = (normal * (1.0 - weld) + collar_target * weld).normalize_or_zero();
                }
            }

            // U goes from 0.0 to 1.0 (inclusive) for seamless texture wrap
            let uv = Vec2::new(i as f32 / resolution as f32, v_coord);

            let mut vertex = Vertex::new(position, normal, uv);

            // Add Pivot Painter data
            if self.config.pivot_painter {
                self.encode_pivot_painter(&mut vertex, stem);
            }

            let idx = self.mesh.vertices.len() as u32;
            self.mesh.vertices.push(vertex);
            ring_indices.push(idx);
        }

        ring_indices
    }

    fn connect_rings(&mut self, ring_a: &[u32], ring_b: &[u32]) {
        // Ring has resolution+1 vertices (last is seam duplicate)
        // Connect resolution quads (not wrapping, since last vertex handles the seam)
        let n = ring_a.len() - 1; // -1 because last vertex is seam duplicate

        for i in 0..n {
            let a0 = ring_a[i];
            let a1 = ring_a[i + 1];
            let b0 = ring_b[i];
            let b1 = ring_b[i + 1];

            // Two triangles per quad (CCW outward winding: tangent x bitangent = +direction)
            // First triangle
            self.mesh.indices.push(a0);
            self.mesh.indices.push(a1);
            self.mesh.indices.push(b1);

            // Second triangle
            self.mesh.indices.push(a0);
            self.mesh.indices.push(b1);
            self.mesh.indices.push(b0);
        }
    }

    fn cap_ring(&mut self, ring: &[u32], normal: Vec3, stem: &Stem) {
        // Ring has resolution+1 vertices, last is seam duplicate
        // Use only the unique vertices for the cap (exclude last seam vertex)
        let actual_count = ring.len() - 1;
        if actual_count < 3 {
            return;
        }

        // Calculate center position (exclude seam vertex)
        let avg_center: Vec3 = ring[..actual_count]
            .iter()
            .map(|&i| self.mesh.vertices[i as usize].position)
            .sum::<Vec3>()
            / actual_count as f32;
        let center = if stem.child_ids.len() >= 2 {
            let avg_r = ring[..actual_count]
                .iter()
                .map(|&i| (self.mesh.vertices[i as usize].position - avg_center).length())
                .sum::<f32>()
                / actual_count as f32;
            avg_center + normal * (avg_r * 0.28)
        } else {
            avg_center
        };

        // Add center vertex
        let uv = Vec2::new(0.5, 0.5);
        let mut vertex = Vertex::new(center, normal, uv);
        if self.config.pivot_painter {
            self.encode_pivot_painter(&mut vertex, stem);
        }
        let center_idx = self.mesh.vertices.len() as u32;
        self.mesh.vertices.push(vertex);
        self.cap_center_indices.push(center_idx);

        // Fan triangulation (use actual_count, not ring.len())
        for i in 0..actual_count {
            let next_i = (i + 1) % actual_count;
            self.mesh.indices.push(ring[i]);
            self.mesh.indices.push(ring[next_i]);
            self.mesh.indices.push(center_idx);
        }
    }

    fn encode_pivot_painter(&self, vertex: &mut Vertex, stem: &Stem) {
        // UV2: x = branch depth (normalized), y = phase offset (for animation variation)
        let depth = stem.level as f32 / MAX_BRANCH_LEVELS as f32;
        let phase = (stem.id as f32 * GOLDEN_RATIO).fract(); // Pseudo-random phase
        vertex.uv2 = Vec2::new(depth, phase);

        // Color: RGB = pivot position (normalized to bounding box), A = stiffness
        let pivot = if let Some(parent_id) = stem.parent_id {
            // Pivot is parent attachment point
            if let Some(parent) = self.tree.get_stem(parent_id) {
                parent.point_at(stem.parent_offset)
            } else {
                stem.base()
            }
        } else {
            // Trunk pivots at ground
            Vec3::ZERO
        };

        // Normalize pivot to bounding box
        let bounds = &self.tree.bounds;
        let size = bounds.size();
        let pivot_norm = if size.length() > 0.001 {
            (pivot - bounds.min) / size
        } else {
            Vec3::ZERO
        };

        // Stiffness: trunk is stiff (1.0), deeper branches are more flexible (lower values)
        let stiffness = 1.0 - depth;

        vertex.color = Vec4::new(pivot_norm.x, pivot_norm.y, pivot_norm.z, stiffness);
    }

    fn encode_custom_pivot_painter(
        &self,
        vertex: &mut Vertex,
        depth: f32,
        phase: f32,
        pivot: Vec3,
    ) {
        let clamped_depth = depth.clamp(0.0, 1.0);
        vertex.uv2 = Vec2::new(clamped_depth, phase.fract().abs());
        let bounds = &self.tree.bounds;
        let size = bounds.size();
        let pivot_norm = if size.length() > 0.001 {
            (pivot - bounds.min) / size
        } else {
            Vec3::ZERO
        };
        vertex.color = Vec4::new(
            pivot_norm.x,
            pivot_norm.y,
            pivot_norm.z,
            (1.0 - clamped_depth).clamp(0.0, 1.0),
        );
    }

    /// Emit 3D snaking & forked exposed surface buttress roots (nebari) that flow seamlessly out of
    /// the trunk's 5 buttress flutes, hug the terrain without air gaps, and match trunk bark texel scale.
    fn build_trunk_roots(&mut self, stem: &Stem, trunk_ring_res: u32) {
        let base_pos = stem.base();
        let ground_y = base_pos.y;
        let base_r = stem.segments[0].start_radius;
        let seed_f = self.tree.seed as f32;
        let seed_phase = (seed_f * 0.618_034).fract() * TAU;
        let root_base_az = seed_phase * 0.20;
        let root_res = (trunk_ring_res / 2).clamp(12, 16);
        let fork_res = (root_res * 3 / 4).clamp(8, 12);

        // Five primary 3D snaking buttress roots aligned directly with the 5 trunk buttress flute peaks
        let root_count = 5usize;
        for r_idx in 0..root_count {
            let h1 = ((seed_f * 0.317 + r_idx as f32 * 0.754_877).fract()).abs();
            let h2 = ((seed_f * 0.519 + r_idx as f32 * 0.414_213).fract()).abs();
            let h3 = ((seed_f * 0.793 + r_idx as f32 * 0.618_034).fract()).abs();

            let az =
                root_base_az + (r_idx as f32 / root_count as f32) * TAU + 0.05 + (h1 - 0.5) * 0.14;
            let rad_dir = Vec3::new(az.cos(), 0.0, az.sin());
            let side_dir = Vec3::new(-az.sin(), 0.0, az.cos());

            let reach = base_r * (2.15 + 0.95 * h2);
            let start_h = base_r * (1.28 + 0.18 * h3);
            let r_peak = base_r * (0.29 + 0.06 * h1);
            let s_sign = if r_idx % 2 == 0 { 1.0 } else { -1.0 };
            let wave_phase = h2 * TAU;

            let steps = 13usize;
            let mut spine = Vec::with_capacity(steps + 1);
            let mut radii = Vec::with_capacity(steps + 1);

            for k in 0..=steps {
                let u = k as f32 / steps as f32;
                // Start deep inside upper-base sapwood so there is zero bulbous shoulder on the trunk wall,
                // then sweep outward along the trunk buttress flute and snake across the ground.
                let radial_dist = base_r * 0.20 + (base_r * 0.34 + reach) * u.powf(0.96);
                let out_u = ((u - 0.20) / 0.80).clamp(0.0, 1.0);
                let s_bend = reach
                    * s_sign
                    * (0.19
                        * (out_u * std::f32::consts::PI * 1.75 + wave_phase * 0.30).sin()
                        * out_u.powf(1.15)
                        + 0.06 * (out_u * TAU * 1.6 - wave_phase).sin() * out_u);

                // Radius starts tucked thin inside trunk core, swells through the buttress skirt, then tapers
                let r = if u < 0.22 {
                    let su = u / 0.22;
                    r_peak * (0.52 + 0.48 * su * (2.0 - su))
                } else {
                    let tu = (u - 0.22) / 0.78;
                    (r_peak * (1.0 - 0.86 * tu.powf(0.82))).max(0.020)
                };

                // Concave buttress sweep down onto ground; once on ground, bottom 35% stays buried below y = 0!
                let flare_frac = (1.0 - (u / 0.36).min(1.0)).powf(2.35);
                let flare_y = start_h * flare_frac;
                let tip_sink = if u > 0.74 {
                    let su = (u - 0.74) / 0.26;
                    -0.055 * su * su
                } else {
                    0.0
                };
                let ground_hug_y = r * 0.18 * (1.0 - flare_frac);
                let y = ground_y + flare_y + ground_hug_y + tip_sink;

                let pos = Vec3::new(base_pos.x, y, base_pos.z)
                    + rad_dir * radial_dist
                    + side_dir * s_bend;
                spine.push(pos);
                radii.push(r);
            }

            let root_phase = (r_idx as f32 * GOLDEN_RATIO + h1).fract();
            self.emit_root_tube(&spine, &radii, root_res, base_r, root_phase);

            // Secondary forked ground-hugging root toe on 3 of the 5 primary roots
            if r_idx % 2 == 0 {
                let fork_idx = 6usize;
                let fork_origin = spine[fork_idx];
                let parent_tan = (spine[fork_idx + 1] - spine[fork_idx - 1]).normalize_or_zero();
                let parent_horiz = Vec3::new(parent_tan.x, 0.0, parent_tan.z)
                    .try_normalize()
                    .unwrap_or(rad_dir);
                let parent_side = Vec3::new(-parent_horiz.z, 0.0, parent_horiz.x);
                let fork_sign = if h3 > 0.48 { 1.0 } else { -1.0 };
                let fork_ang = fork_sign * (0.64 + 0.20 * h1);
                let fork_dir = (parent_horiz * fork_ang.cos() + parent_side * fork_ang.sin())
                    .normalize_or_zero();
                let fork_perp = Vec3::new(-fork_dir.z, 0.0, fork_dir.x);
                let fork_len = reach * (0.48 + 0.14 * h3);
                let fork_r0 = radii[fork_idx] * 0.70;

                let f_steps = 8usize;
                let mut f_spine = Vec::with_capacity(f_steps + 1);
                let mut f_radii = Vec::with_capacity(f_steps + 1);
                let tucked_xz = fork_origin - fork_dir * (radii[fork_idx] * 0.45);
                for fk in 0..=f_steps {
                    let fu = fk as f32 / f_steps as f32;
                    let f_dist = fork_len * fu;
                    let f_lat = fork_len
                        * (-fork_sign)
                        * 0.17
                        * (fu * std::f32::consts::PI * 1.45 + h2).sin()
                        * fu;
                    let fr = (fork_r0 * (1.0 - 0.82 * fu.powf(0.84))).max(0.016);
                    let f_sink = if fu > 0.70 {
                        let su = (fu - 0.70) / 0.30;
                        -0.050 * su * su
                    } else {
                        0.0
                    };
                    // Ground-anchored centerline so secondary root toes never hover above y = 0
                    let fy = ground_y + fr * 0.16 + f_sink;
                    let f_pos = Vec3::new(tucked_xz.x, fy, tucked_xz.z)
                        + fork_dir * f_dist
                        + fork_perp * f_lat;
                    f_spine.push(f_pos);
                    f_radii.push(fr);
                }
                self.emit_root_tube(
                    &f_spine,
                    &f_radii,
                    fork_res,
                    base_r,
                    (root_phase + 0.37).fract(),
                );
            }
        }
    }

    fn emit_root_tube(
        &mut self,
        spine: &[Vec3],
        radii: &[f32],
        resolution: u32,
        trunk_base_r: f32,
        pivot_phase: f32,
    ) {
        if spine.len() < 2 || spine.len() != radii.len() || resolution < 4 {
            return;
        }

        let n_pts = spine.len();
        let mut dirs = Vec::with_capacity(n_pts);
        for k in 0..n_pts {
            let raw_dir = if k == 0 {
                spine[1] - spine[0]
            } else if k + 1 == n_pts {
                spine[k] - spine[k - 1]
            } else {
                spine[k + 1] - spine[k - 1]
            };
            dirs.push(normalized_or_up(raw_dir));
        }

        let avg_r = (radii.iter().sum::<f32>() / n_pts as f32).max(0.025);
        // Match bark texel scale to trunk circumference so bark plates never shrink into miniature scales
        let trunk_circum = (TAU * trunk_base_r * 0.76).max(0.55);
        let u_span = ((avg_r * 2.5) / trunk_base_r.max(0.20)).clamp(0.28, 0.48);
        let u_base = (pivot_phase * 0.50).clamp(0.0, 1.0 - u_span);

        let mut cum_len = 0.0_f32;
        let mut frame = RingFrame::from_direction(dirs[0]);
        let mut rings = Vec::with_capacity(n_pts);
        let seed_phase = (self.tree.seed as f32 * 0.618_034).fract() * TAU + pivot_phase * TAU;

        for k in 0..n_pts {
            if k > 0 {
                cum_len += (spine[k] - spine[k - 1]).length();
                frame = frame.transported(dirs[k]);
            }
            let u_frac = k as f32 / (n_pts - 1) as f32;
            let v_coord = (cum_len / trunk_circum * self.config.texture_v_scale).max(0.0);
            let r_here = radii[k];
            let mut ring = Vec::with_capacity(resolution as usize + 1);

            for i in 0..=resolution {
                let angle = if i == resolution {
                    0.0
                } else {
                    (i as f32 / resolution as f32) * TAU
                };
                let offset = frame.tangent * angle.cos() + frame.bitangent * angle.sin();
                let tangent_dir = -frame.tangent * angle.sin() + frame.bitangent * angle.cos();

                // Wide earth-gripping buttress base + muscular dorsal root crest
                let horiz_sq = (1.0 - offset.y * offset.y).clamp(0.0, 1.0);
                let dorsal = offset.y.max(0.0);
                let width_aspect = 1.0 + 0.30 * horiz_sq;
                let crest_aspect = 1.0 + 0.24 * dorsal * dorsal * (1.0 - u_frac * 0.45);
                let f1 = angle * 3.0 + v_coord * 1.1 + seed_phase;
                let flute = 0.055 * f1.cos();
                let twist = (-0.055 * 3.0 * f1.sin() * 0.14).clamp(-0.22, 0.22);

                let shape_mul = width_aspect * crest_aspect * (1.0 + flute);
                let pos = spine[k] + offset * (r_here * shape_mul);
                let normal = (offset + tangent_dir * twist).normalize_or_zero();

                // Seamless mirrored UV wrap matching trunk bark plate scale
                let t_u = i as f32 / resolution as f32;
                let tri_u = 1.0 - (2.0 * t_u - 1.0).abs();
                let uv = Vec2::new(u_base + u_span * tri_u, v_coord);

                let mut vertex = Vertex::new(pos, normal, uv);
                if self.config.pivot_painter {
                    self.encode_custom_pivot_painter(&mut vertex, 0.0, pivot_phase, Vec3::ZERO);
                }
                let idx = self.mesh.vertices.len() as u32;
                self.mesh.vertices.push(vertex);
                ring.push(idx);
            }
            rings.push(ring);
        }

        for k in 0..rings.len() - 1 {
            self.connect_rings(&rings[k], &rings[k + 1]);
        }

        // Cap buried root tip cleanly
        if let Some(last_ring) = rings.last() {
            let actual_count = last_ring.len() - 1;
            if actual_count >= 3 {
                let tip_normal = dirs[n_pts - 1];
                let center: Vec3 = last_ring[..actual_count]
                    .iter()
                    .map(|&idx| self.mesh.vertices[idx as usize].position)
                    .sum::<Vec3>()
                    / actual_count as f32;
                let mut vertex = Vertex::new(center, tip_normal, Vec2::new(0.5, 0.5));
                if self.config.pivot_painter {
                    self.encode_custom_pivot_painter(&mut vertex, 0.0, pivot_phase, Vec3::ZERO);
                }
                let center_idx = self.mesh.vertices.len() as u32;
                self.mesh.vertices.push(vertex);
                self.cap_center_indices.push(center_idx);
                for i in 0..actual_count {
                    let next_i = (i + 1) % actual_count;
                    self.mesh.indices.push(last_ring[i]);
                    self.mesh.indices.push(last_ring[next_i]);
                    self.mesh.indices.push(center_idx);
                }
            }
        }
    }

    fn ring_resolution_for_level(&self, level: u8) -> u32 {
        self.config
            .ring_resolution
            .get(level as usize)
            .copied()
            .unwrap_or(4)
            .max(3)
    }

    /// Adaptive ring resolution based on branch radius.
    /// Thin branches don't need as many segments - reduces polygon count
    /// without visible quality loss.
    fn adaptive_ring_resolution(&self, level: u8, radius: f32) -> u32 {
        let base = self.ring_resolution_for_level(level);

        if base >= 8 {
            // High-detail LODs keep smooth round branches even on slender limbs and twigs
            if radius < 0.016 {
                (base * 2 / 3).max(6)
            } else if radius < 0.045 {
                (base * 3 / 4).max(8)
            } else {
                base
            }
        } else if radius < 0.02 {
            (base / 2).max(3)
        } else if radius < 0.05 {
            (base * 2 / 3).max(4)
        } else {
            base
        }
    }

    fn smooth_coincident_normals(&mut self) {
        if self.mesh.vertices.is_empty() {
            return;
        }

        let mut is_cap_center = vec![false; self.mesh.vertices.len()];
        for &index in &self.cap_center_indices {
            if let Some(is_cap) = is_cap_center.get_mut(index as usize) {
                *is_cap = true;
            }
        }

        let mut groups: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for (index, vertex) in self.mesh.vertices.iter().enumerate() {
            if is_cap_center[index] {
                continue;
            }
            groups
                .entry(position_key(vertex.position))
                .or_default()
                .push(index);
        }

        for indices in groups.values() {
            if indices.len() < 2 {
                continue;
            }

            let normal_sum = indices.iter().fold(Vec3::ZERO, |sum, &index| {
                sum + self.mesh.vertices[index].normal
            });
            let smoothed = normal_sum.normalize_or_zero();
            if smoothed.length_squared() <= f32::EPSILON {
                continue;
            }

            for &index in indices {
                self.mesh.vertices[index].normal = smoothed;
            }
        }
    }
}

fn normalized_or_up(value: Vec3) -> Vec3 {
    normalized_or(value, Vec3::Y)
}

fn normalized_or(value: Vec3, fallback: Vec3) -> Vec3 {
    let normalized = value.normalize_or_zero();
    if normalized.length_squared() > 0.0 {
        normalized
    } else {
        fallback.normalize_or_zero()
    }
}

fn position_key(position: Vec3) -> (i32, i32, i32) {
    (
        (position.x * NORMAL_SMOOTH_POSITION_SCALE).round() as i32,
        (position.y * NORMAL_SMOOTH_POSITION_SCALE).round() as i32,
        (position.z * NORMAL_SMOOTH_POSITION_SCALE).round() as i32,
    )
}

/// Generate mesh from tree with default configuration
pub fn build_mesh(tree: &Tree) -> Mesh {
    MeshBuilder::new(tree, MeshConfig::default()).build_branches()
}

/// Generate mesh from tree with custom configuration
pub fn build_mesh_with_config(tree: &Tree, config: MeshConfig) -> Mesh {
    MeshBuilder::new(tree, config).build_branches()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Segment;

    fn create_simple_tree() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Create a simple trunk
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 2.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.4,
            direction: Vec3::Y,
        });
        trunk.segments.push(Segment {
            start: Vec3::new(0.0, 2.0, 0.0),
            end: Vec3::new(0.0, 4.0, 0.0),
            start_radius: 0.4,
            end_radius: 0.3,
            direction: Vec3::Y,
        });
        tree.add_stem(trunk);

        tree.update_bounds();
        tree
    }

    fn create_tree_with_branch() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Create trunk
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 3.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.3,
            direction: Vec3::Y,
        });
        trunk.child_ids.push(1);
        tree.add_stem(trunk);

        // Create branch
        let mut branch = Stem::new(1, 1);
        branch.parent_id = Some(0);
        branch.parent_offset = 0.5;
        branch.segments.push(Segment {
            start: Vec3::new(0.0, 1.5, 0.0),
            end: Vec3::new(1.0, 2.0, 0.0),
            start_radius: 0.15,
            end_radius: 0.1,
            direction: Vec3::new(1.0, 0.5, 0.0).normalize(),
        });
        tree.add_stem(branch);

        tree.update_bounds();
        tree
    }

    #[test]
    fn test_mesh_config_default() {
        let config = MeshConfig::default();
        assert_eq!(config.ring_resolution, [24, 16, 10, 6]);
        assert!((config.texture_v_scale - 1.0).abs() < 0.001);
        assert!(config.pivot_painter);
        assert!(config.trunk_base_flare > 1.0);
        assert!(config.branch_base_swell > 1.0);
    }

    #[test]
    fn test_build_simple_mesh() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);

        // Should have vertices and triangles
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);

        // Should have one submesh (bark)
        assert_eq!(mesh.submeshes.len(), 1);
        assert_eq!(mesh.submeshes[0].material, MaterialType::Bark);
    }

    #[test]
    fn test_mesh_has_correct_topology() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            ring_resolution: [8, 6, 4, 3],
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        // With 8 vertices per ring, 3 rings (2 segments), plus cap center
        // Vertices: 8 * 3 + 1 = 25
        // Triangles: 2 quads per column * 8 columns * 2 segments + 8 cap triangles
        //          = 2 * 8 * 2 + 8 = 32 + 8 = 40 (but actually 2 rings between segments)

        // All indices should be valid
        for &idx in &mesh.indices {
            assert!(
                (idx as usize) < mesh.vertex_count(),
                "Invalid index {} >= {}",
                idx,
                mesh.vertex_count()
            );
        }

        // Indices should be multiple of 3 (triangles)
        assert_eq!(mesh.indices.len() % 3, 0);
    }

    #[test]
    fn test_trunk_base_flare_expands_base_ring() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            ring_resolution: [8, 6, 4, 3],
            trunk_base_flare: 1.5,
            trunk_base_flare_height: 0.25,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let ring_len = 9;
        let base_radius = max_ring_distance(&mesh, 0..ring_len, Vec3::ZERO);
        let next_radius =
            max_ring_distance(&mesh, ring_len..ring_len * 2, Vec3::new(0.0, 2.0, 0.0));

        assert!(base_radius > 0.70, "base ring should be visibly flared");
        assert!(
            next_radius < 0.45,
            "flare should fall off by the next trunk ring"
        );
    }

    #[test]
    fn test_branch_base_swell_expands_child_base_ring() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.6,
            branch_base_falloff: 0.4,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let branch_start = Vec3::new(0.0, 1.5, 0.0);
        let branch_base_radius = max_depth_ring_distance(&mesh, branch_start, 0.25);

        assert!(
            branch_base_radius > 0.22,
            "branch base should be larger than the raw 0.15m radius"
        );
    }

    #[test]
    fn test_child_attachment_ring_adds_parent_collar_geometry() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.5,
            collar_falloff: 0.25,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let ring_len = 9;
        let attachment_center = Vec3::new(0.0, 1.5, 0.0);
        let collar_radius = max_ring_distance(&mesh, ring_len..ring_len * 2, attachment_center);

        assert!(
            collar_radius > 0.55,
            "parent collar ring should be inserted and swollen at the child offset"
        );
    }

    #[test]
    fn test_parent_tip_is_capped_when_children_are_side_branches() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let trunk_tip = Vec3::new(0.0, 3.0, 0.0);
        let cap_center = mesh
            .vertices
            .iter()
            .find(|vertex| {
                (vertex.position - trunk_tip).length() < 0.001
                    && vertex.normal.dot(Vec3::Y) > 0.99
                    && (vertex.uv2.x - 0.0).abs() < f32::EPSILON
            })
            .expect("side-branched parent tip should have a cap center");

        assert!((cap_center.color.w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_lod_filtering_skips_excluded_child_attachment_ring() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            ring_resolution: [8, 8, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.5,
            ..Default::default()
        };

        let mesh = MeshBuilder::new(&tree, config).build_branches_to_level(0);

        // Trunk-only LOD keeps the start/end rings and a tip cap, but does not
        // add a hidden attachment ring for the excluded level-1 branch.
        assert_eq!(mesh.vertex_count(), 19);
    }

    #[test]
    fn test_seam_vertices_are_exact_duplicates_with_shared_normals() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            ring_resolution: [8, 6, 4, 3],
            trunk_base_flare: 1.0,
            branch_base_swell: 1.0,
            branch_collar_swell: 1.0,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        let ring_len = 9;
        for ring_index in 0..3 {
            let first = ring_index * ring_len;
            let seam = first + ring_len - 1;

            assert_eq!(mesh.vertices[first].position, mesh.vertices[seam].position);
            assert_eq!(mesh.vertices[first].normal, mesh.vertices[seam].normal);
        }
    }

    #[test]
    fn test_coincident_side_normals_are_averaged() {
        let tree = Tree::new("Normals".to_string(), 0);
        let mut builder = MeshBuilder::new(&tree, MeshConfig::default());
        builder
            .mesh
            .vertices
            .push(Vertex::new(Vec3::new(1.0, 2.0, 3.0), Vec3::X, Vec2::ZERO));
        builder.mesh.vertices.push(Vertex::new(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y,
            Vec2::new(1.0, 0.0),
        ));

        builder.smooth_coincident_normals();

        let expected = (Vec3::X + Vec3::Y).normalize();
        assert!((builder.mesh.vertices[0].normal - expected).length() < 0.001);
        assert!((builder.mesh.vertices[1].normal - expected).length() < 0.001);
    }

    #[test]
    fn test_cap_center_normals_are_not_smoothed_with_side_vertices() {
        let tree = Tree::new("Normals".to_string(), 0);
        let mut builder = MeshBuilder::new(&tree, MeshConfig::default());
        builder
            .mesh
            .vertices
            .push(Vertex::new(Vec3::new(1.0, 2.0, 3.0), Vec3::X, Vec2::ZERO));
        builder.mesh.vertices.push(Vertex::new(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::Y,
            Vec2::new(0.5, 0.5),
        ));
        builder.cap_center_indices.push(1);

        builder.smooth_coincident_normals();

        assert_eq!(builder.mesh.vertices[0].normal, Vec3::X);
        assert_eq!(builder.mesh.vertices[1].normal, Vec3::Y);
    }

    #[test]
    fn test_cylinder_triangles_wind_outward() {
        let tree = create_tree_with_multiple_levels();
        let mesh = build_mesh(&tree);

        for tri in mesh.indices.as_chunks::<3>().0 {
            let v0 = &mesh.vertices[tri[0] as usize];
            let v1 = &mesh.vertices[tri[1] as usize];
            let v2 = &mesh.vertices[tri[2] as usize];
            let face_normal = (v1.position - v0.position).cross(v2.position - v0.position);
            if face_normal.length_squared() <= 1e-10 {
                continue;
            }
            let avg_vertex_normal = v0.normal + v1.normal + v2.normal;
            assert!(
                face_normal.dot(avg_vertex_normal) > 0.0,
                "Triangle {:?} winds inward relative to vertex normals",
                tri
            );
        }
    }

    #[test]
    fn test_mesh_normals_point_outward() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);

        // Check that normals point outward from the trunk center axis
        for vertex in &mesh.vertices {
            if vertex.normal.length() > 0.5 {
                if vertex.normal.dot(Vec3::Y).abs() > 0.95 {
                    continue; // terminal cap center normal follows the stem axis
                }
                // Skip cap center vertex
                // For a trunk along Y axis, X and Z components of normal should be non-zero
                // and Y component should be small for side vertices
                let horizontal_mag = (vertex.normal.x.powi(2) + vertex.normal.z.powi(2)).sqrt();
                assert!(
                    horizontal_mag > 0.5,
                    "Normal should point outward: {:?}",
                    vertex.normal
                );
            }
        }
    }

    #[test]
    fn test_terminal_cap_center_uses_axial_normal_and_pivot_data() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);
        let cap_center = mesh.vertices.last().expect("terminal cap center vertex");

        assert!(
            cap_center.normal.dot(Vec3::Y) > 0.99,
            "cap center normal should follow the terminal stem direction"
        );
        assert!((cap_center.uv2.x - 0.0).abs() < f32::EPSILON);
        assert!((cap_center.color.w - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_mesh_attributes_are_finite_and_normalized() {
        let tree = create_tree_with_multiple_levels();
        let mesh = build_mesh(&tree);

        for (index, vertex) in mesh.vertices.iter().enumerate() {
            assert!(
                vertex.position.is_finite(),
                "position {} is not finite",
                index
            );
            assert!(vertex.normal.is_finite(), "normal {} is not finite", index);
            assert!(vertex.uv.is_finite(), "uv {} is not finite", index);
            assert!(vertex.uv2.is_finite(), "uv2 {} is not finite", index);
            assert!(vertex.color.is_finite(), "color {} is not finite", index);
            assert!(
                (vertex.normal.length() - 1.0).abs() < 0.001,
                "normal {} should be unit length: {:?}",
                index,
                vertex.normal
            );
            assert!(
                vertex.uv.x >= 0.0 && vertex.uv.x <= 1.0,
                "u coordinate {} out of range: {}",
                index,
                vertex.uv.x
            );
            assert!(
                vertex.color.w >= 0.0 && vertex.color.w <= 1.0,
                "stiffness {} out of range: {}",
                index,
                vertex.color.w
            );
        }
    }

    #[test]
    fn test_mesh_uvs_are_valid() {
        let tree = create_simple_tree();
        let mesh = build_mesh(&tree);

        for vertex in &mesh.vertices {
            // U should be in [0, 1)
            assert!(
                vertex.uv.x >= 0.0 && vertex.uv.x <= 1.0,
                "Invalid U coordinate: {}",
                vertex.uv.x
            );
            // V should be non-negative (increases along stem)
            assert!(vertex.uv.y >= 0.0, "Invalid V coordinate: {}", vertex.uv.y);
        }
    }

    #[test]
    fn test_pivot_painter_encoding() {
        let tree = create_tree_with_branch();
        let config = MeshConfig {
            pivot_painter: true,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        // Check that pivot painter data is encoded
        for vertex in &mesh.vertices {
            // UV2.x (depth) should be in [0, 1]
            assert!(
                vertex.uv2.x >= 0.0 && vertex.uv2.x <= 1.0,
                "Invalid depth: {}",
                vertex.uv2.x
            );
            // UV2.y (phase) should be in [0, 1)
            assert!(
                vertex.uv2.y >= 0.0 && vertex.uv2.y < 1.0,
                "Invalid phase: {}",
                vertex.uv2.y
            );
            // Color.w (stiffness) should be in [0, 1]
            assert!(
                vertex.color.w >= 0.0 && vertex.color.w <= 1.0,
                "Invalid stiffness: {}",
                vertex.color.w
            );
        }
    }

    #[test]
    fn test_pivot_painter_disabled() {
        let tree = create_simple_tree();
        let config = MeshConfig {
            pivot_painter: false,
            ..Default::default()
        };
        let mesh = build_mesh_with_config(&tree, config);

        // Check that pivot painter data is zero
        for vertex in &mesh.vertices {
            assert_eq!(vertex.uv2, Vec2::ZERO);
            assert_eq!(vertex.color, Vec4::ZERO);
        }
    }

    #[test]
    fn test_branch_mesh_generation() {
        let tree = create_tree_with_branch();
        let mesh = build_mesh(&tree);

        // Should generate geometry for both trunk and branch
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);

        // The trunk has a child, so it should NOT have a cap
        // The branch has no children, so it SHOULD have a cap
    }

    #[test]
    fn test_empty_tree() {
        let tree = Tree::new("Empty".to_string(), 0);
        let mesh = build_mesh(&tree);

        assert!(mesh.is_empty());
        assert!(mesh.submeshes.is_empty());
    }

    #[test]
    fn test_empty_stem() {
        let mut tree = Tree::new("EmptyStem".to_string(), 0);
        let stem = Stem::new(0, 0); // No segments
        tree.add_stem(stem);

        let mesh = build_mesh(&tree);

        // Should not crash, should produce no geometry
        assert!(mesh.is_empty());
    }

    #[test]
    fn test_ring_resolution_per_level() {
        let config = MeshConfig {
            ring_resolution: [12, 8, 5, 3],
            ..Default::default()
        };
        let tree = Tree::new("Test".to_string(), 0);
        let builder = MeshBuilder::new(&tree, config.clone());

        assert_eq!(builder.ring_resolution_for_level(0), 12);
        assert_eq!(builder.ring_resolution_for_level(1), 8);
        assert_eq!(builder.ring_resolution_for_level(2), 5);
        assert_eq!(builder.ring_resolution_for_level(3), 3);
        // Out of bounds should return default minimum
        assert_eq!(builder.ring_resolution_for_level(10), 4);
    }

    #[test]
    fn test_texture_v_scale() {
        let tree = create_simple_tree();

        let config1 = MeshConfig {
            texture_v_scale: 1.0,
            ..Default::default()
        };
        let mesh1 = build_mesh_with_config(&tree, config1);

        let config2 = MeshConfig {
            texture_v_scale: 2.0,
            ..Default::default()
        };
        let mesh2 = build_mesh_with_config(&tree, config2);

        // Find max V coordinate in each mesh
        let max_v1 = mesh1
            .vertices
            .iter()
            .map(|v| v.uv.y)
            .fold(0.0_f32, f32::max);
        let max_v2 = mesh2
            .vertices
            .iter()
            .map(|v| v.uv.y)
            .fold(0.0_f32, f32::max);

        // V coordinates should scale proportionally
        assert!(
            (max_v2 / max_v1 - 2.0).abs() < 0.01,
            "V scale not applied correctly: {max_v1} vs {max_v2}"
        );
    }

    fn create_tree_with_multiple_levels() -> Tree {
        let mut tree = Tree::new("Test".to_string(), 42);

        // Create trunk (level 0)
        let mut trunk = Stem::new(0, 0);
        trunk.segments.push(Segment {
            start: Vec3::ZERO,
            end: Vec3::new(0.0, 4.0, 0.0),
            start_radius: 0.5,
            end_radius: 0.3,
            direction: Vec3::Y,
        });
        trunk.child_ids.push(1);
        tree.add_stem(trunk);

        // Create level 1 branch
        let mut branch1 = Stem::new(1, 1);
        branch1.parent_id = Some(0);
        branch1.parent_offset = 0.5;
        branch1.segments.push(Segment {
            start: Vec3::new(0.0, 2.0, 0.0),
            end: Vec3::new(1.5, 2.5, 0.0),
            start_radius: 0.15,
            end_radius: 0.1,
            direction: Vec3::new(1.0, 0.3, 0.0).normalize(),
        });
        branch1.child_ids.push(2);
        tree.add_stem(branch1);

        // Create level 2 branch
        let mut branch2 = Stem::new(2, 2);
        branch2.parent_id = Some(1);
        branch2.parent_offset = 0.7;
        branch2.segments.push(Segment {
            start: Vec3::new(1.0, 2.35, 0.0),
            end: Vec3::new(1.5, 2.8, 0.3),
            start_radius: 0.05,
            end_radius: 0.03,
            direction: Vec3::new(0.5, 0.5, 0.3).normalize(),
        });
        tree.add_stem(branch2);

        tree.update_bounds();
        tree
    }

    #[test]
    fn test_build_branches_to_level() {
        let tree = create_tree_with_multiple_levels();

        // Build with all levels
        let config = MeshConfig::default();
        let mesh_all = MeshBuilder::new(&tree, config.clone()).build_branches();

        // Build with trunk only (level 0)
        let mesh_trunk = MeshBuilder::new(&tree, config.clone()).build_branches_to_level(0);

        // Build with trunk + level 1
        let mesh_level1 = MeshBuilder::new(&tree, config.clone()).build_branches_to_level(1);

        // Trunk only should have fewer vertices than trunk + level 1
        assert!(
            mesh_trunk.vertex_count() < mesh_level1.vertex_count(),
            "Trunk only ({}) should have fewer vertices than trunk + level 1 ({})",
            mesh_trunk.vertex_count(),
            mesh_level1.vertex_count()
        );

        // Level 1 should have fewer vertices than all levels
        assert!(
            mesh_level1.vertex_count() < mesh_all.vertex_count(),
            "Trunk + level 1 ({}) should have fewer vertices than all ({})",
            mesh_level1.vertex_count(),
            mesh_all.vertex_count()
        );
    }

    #[test]
    fn test_build_branches_to_level_high_limit() {
        let tree = create_tree_with_multiple_levels();
        let config = MeshConfig::default();

        // Build with max_level higher than any branch level in tree
        let mesh_high = MeshBuilder::new(&tree, config.clone()).build_branches_to_level(10);

        // Should be the same as building all branches
        let mesh_all = MeshBuilder::new(&tree, config).build_branches();

        assert_eq!(
            mesh_high.vertex_count(),
            mesh_all.vertex_count(),
            "High limit should include all branches"
        );
    }

    fn max_ring_distance(mesh: &Mesh, range: std::ops::Range<usize>, center: Vec3) -> f32 {
        range
            .map(|index| (mesh.vertices[index].position - center).length())
            .fold(0.0_f32, f32::max)
    }

    fn max_depth_ring_distance(mesh: &Mesh, center: Vec3, depth: f32) -> f32 {
        mesh.vertices
            .iter()
            .filter(|vertex| (vertex.uv2.x - depth).abs() < 0.001)
            .filter(|vertex| (vertex.position - center).length() < 1.0)
            .map(|vertex| (vertex.position - center).length())
            .fold(0.0_f32, f32::max)
    }
}
