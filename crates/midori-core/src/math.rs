//! Math utilities for deterministic cross-platform generation.
//! Uses glam with libm feature for consistent results.

use glam::{Quat, Vec3};

/// Create perpendicular basis vectors from a direction
pub fn create_basis(direction: Vec3) -> (Vec3, Vec3) {
    let up = if direction.y.abs() < 0.99 {
        Vec3::Y
    } else {
        Vec3::X
    };
    let tangent = direction.cross(up).normalize();
    let bitangent = direction.cross(tangent);
    (tangent, bitangent)
}

/// Rotate a vector around an axis by angle (radians)
pub fn rotate_around_axis(v: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    Quat::from_axis_angle(axis, angle) * v
}

/// Spherical linear interpolation between two vectors
pub fn slerp_vec3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    let dot = a.dot(b).clamp(-1.0, 1.0);
    let theta = dot.acos() * t;
    let relative = (b - a * dot).normalize_or_zero();
    a * theta.cos() + relative * theta.sin()
}

/// Linearly interpolate between two values
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Clamp value to range
pub fn clamp(v: f32, min: f32, max: f32) -> f32 {
    v.max(min).min(max)
}

/// Convert degrees to radians
pub fn radians(degrees: f32) -> f32 {
    degrees * crate::constants::PI / 180.0
}

/// Convert radians to degrees
pub fn degrees(radians: f32) -> f32 {
    radians * 180.0 / crate::constants::PI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_basis_up() {
        let direction = Vec3::Y;
        let (tangent, bitangent) = create_basis(direction);

        // Tangent and bitangent should be perpendicular to direction
        assert!(tangent.dot(direction).abs() < 0.001);
        assert!(bitangent.dot(direction).abs() < 0.001);

        // Tangent and bitangent should be perpendicular to each other
        assert!(tangent.dot(bitangent).abs() < 0.001);

        // Both should be unit vectors
        assert!((tangent.length() - 1.0).abs() < 0.001);
        assert!((bitangent.length() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_create_basis_forward() {
        let direction = Vec3::Z;
        let (tangent, bitangent) = create_basis(direction);

        assert!(tangent.dot(direction).abs() < 0.001);
        assert!(bitangent.dot(direction).abs() < 0.001);
        assert!(tangent.dot(bitangent).abs() < 0.001);
    }

    #[test]
    fn test_rotate_around_axis() {
        let v = Vec3::X;
        let axis = Vec3::Y;
        let angle = crate::constants::PI / 2.0; // 90 degrees

        let rotated = rotate_around_axis(v, axis, angle);

        // X rotated 90 degrees around Y should be approximately -Z
        assert!((rotated.x).abs() < 0.001);
        assert!((rotated.y).abs() < 0.001);
        assert!((rotated.z - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_lerp() {
        assert!((lerp(0.0, 10.0, 0.0) - 0.0).abs() < 0.001);
        assert!((lerp(0.0, 10.0, 1.0) - 10.0).abs() < 0.001);
        assert!((lerp(0.0, 10.0, 0.5) - 5.0).abs() < 0.001);
        assert!((lerp(5.0, 15.0, 0.25) - 7.5).abs() < 0.001);
    }

    #[test]
    fn test_clamp() {
        assert!((clamp(5.0, 0.0, 10.0) - 5.0).abs() < 0.001);
        assert!((clamp(-5.0, 0.0, 10.0) - 0.0).abs() < 0.001);
        assert!((clamp(15.0, 0.0, 10.0) - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_radians() {
        assert!((radians(0.0) - 0.0).abs() < 0.001);
        assert!((radians(180.0) - crate::constants::PI).abs() < 0.001);
        assert!((radians(360.0) - crate::constants::TAU).abs() < 0.001);
        assert!((radians(90.0) - crate::constants::PI / 2.0).abs() < 0.001);
    }

    #[test]
    fn test_degrees() {
        assert!((degrees(0.0) - 0.0).abs() < 0.001);
        assert!((degrees(crate::constants::PI) - 180.0).abs() < 0.001);
        assert!((degrees(crate::constants::TAU) - 360.0).abs() < 0.001);
        assert!((degrees(crate::constants::PI / 2.0) - 90.0).abs() < 0.001);
    }

    #[test]
    fn test_radians_degrees_roundtrip() {
        let original = 45.0;
        let result = degrees(radians(original));
        assert!((result - original).abs() < 0.001);
    }

    #[test]
    fn test_slerp_vec3_endpoints() {
        let a = Vec3::X;
        let b = Vec3::Y;

        let at_zero = slerp_vec3(a, b, 0.0);
        let at_one = slerp_vec3(a, b, 1.0);

        assert!((at_zero - a).length() < 0.001);
        assert!((at_one - b).length() < 0.001);
    }

    #[test]
    fn test_slerp_vec3_midpoint() {
        let a = Vec3::X;
        let b = Vec3::Y;

        let midpoint = slerp_vec3(a, b, 0.5);

        // Midpoint should be on the unit sphere
        assert!((midpoint.length() - 1.0).abs() < 0.001);

        // Should be equidistant from both endpoints (in angle)
        let angle_a = a.dot(midpoint).acos();
        let angle_b = b.dot(midpoint).acos();
        assert!((angle_a - angle_b).abs() < 0.001);
    }
}
