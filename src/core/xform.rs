//! A transform as translation, rotation and scale: a joint's pose in a clip.
//! The same fields and meaning as Bevy's `Transform`, which it replaces
//! outside the shell, on plain glam.

use glam::{Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation:    Quat,
    pub scale:       Vec3,
}

impl Transform {
    pub const IDENTITY: Transform = Transform { translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE };

    pub fn from_translation(translation: Vec3) -> Self {
        Transform { translation, ..Self::IDENTITY }
    }

    /// Split a matrix into translation, rotation and scale.
    pub fn from_matrix(m: Mat4) -> Self {
        let (scale, rotation, translation) = m.to_scale_rotation_translation();
        Transform { translation, rotation, scale }
    }

    /// Scale, then rotation, then translation, as one matrix.
    pub fn compute_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

impl Default for Transform {
    fn default() -> Self { Self::IDENTITY }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matrix_goes_through_and_back() {
        let t = Transform { translation: Vec3::new(1.0, 2.0, 3.0), rotation: Quat::from_rotation_y(0.5), scale: Vec3::new(2.0, 2.0, 2.0) };
        let back = Transform::from_matrix(t.compute_matrix());
        assert!(back.translation.abs_diff_eq(t.translation, 1e-5));
        assert!(back.rotation.abs_diff_eq(t.rotation, 1e-5));
        assert!(back.scale.abs_diff_eq(t.scale, 1e-5));
    }
}
