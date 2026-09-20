//! Mathematical constants and safety limits for tree generation.

use std::f32::consts::PI as STD_PI;

/// Pi constant (3.14159...)
pub const PI: f32 = STD_PI;

/// Tau constant (2 * PI, full circle in radians)
pub const TAU: f32 = PI * 2.0;

/// Golden angle in radians (137.5 degrees)
/// Used for phyllotaxis - natural leaf/branch arrangement patterns
pub const GOLDEN_ANGLE: f32 = 2.399_963_2;

/// Golden ratio (phi)
pub const GOLDEN_RATIO: f32 = 1.618_034;

// Safety limits

/// Minimum branch radius in meters (1mm)
pub const MIN_RADIUS: f32 = 0.001;

/// Minimum segment length in meters (1cm)
pub const MIN_LENGTH: f32 = 0.01;

/// Maximum number of stems/branches (safety limit)
pub const MAX_STEMS: u32 = 50_000;

/// Maximum number of vertices in generated mesh (safety limit)
pub const MAX_VERTICES: u32 = 500_000;

/// Maximum branch recursion levels (trunk + 3 branch levels)
pub const MAX_BRANCH_LEVELS: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tau_equals_two_pi() {
        assert!((TAU - 2.0 * PI).abs() < f32::EPSILON);
    }

    #[test]
    fn test_golden_angle_degrees() {
        // Golden angle should be approximately 137.5 degrees
        let degrees = GOLDEN_ANGLE * 180.0 / PI;
        assert!((degrees - 137.5).abs() < 0.1);
    }

    #[test]
    fn test_golden_ratio() {
        // Golden ratio phi satisfies: phi^2 = phi + 1
        let phi_squared = GOLDEN_RATIO * GOLDEN_RATIO;
        let phi_plus_one = GOLDEN_RATIO + 1.0;
        assert!((phi_squared - phi_plus_one).abs() < 0.001);
    }
}
