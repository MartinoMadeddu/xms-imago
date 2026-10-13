//! The Viewer's camera: a position looking at a target, orbited, panned
//! and zoomed about it, as under Bevy.

use glam::{Mat4, Quat, Vec2, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub eye:    Vec3,
    pub target: Vec3,
    /// Vertical field of view, in radians.
    pub fov:    f32,
}

impl Default for Camera {
    fn default() -> Self {
        // As Bevy's: from (5, 5, 5) towards the origin, 45 degrees.
        Camera { eye: Vec3::splat(5.0), target: Vec3::ZERO, fov: std::f32::consts::FRAC_PI_4 }
    }
}

impl Camera {
    fn forward(&self) -> Vec3 { (self.target - self.eye).normalize_or_zero() }

    pub fn right(&self) -> Vec3 {
        let r = self.forward().cross(Vec3::Y);
        if r.length_squared() < 1e-12 { Vec3::X } else { r.normalize() }
    }

    pub fn up(&self) -> Vec3 { self.right().cross(self.forward()) }

    /// World to clip space, for a view `aspect` wide per unit of height.
    /// Reversed depth, as Bevy draws: 1 at the near plane, towards 0 far
    /// away, with no far plane. Precision then stays even with distance, so
    /// a grid under a model does not show through it.
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let dist = self.eye.distance(self.target).max(1e-4);
        let near = (dist * 0.01).clamp(1e-3, 0.1);
        let proj = Mat4::perspective_infinite_reverse_rh(self.fov, aspect.max(1e-4), near);
        proj * Mat4::look_at_rh(self.eye, self.target, Vec3::Y)
    }

    /// Turn about the target: left-right about the world's up, up-down
    /// about the camera's right. `delta` in points.
    pub fn orbit(&mut self, delta: Vec2) {
        let off = self.eye - self.target;
        let yaw = Quat::from_rotation_y(-delta.x * 0.01);
        let pitch = Quat::from_axis_angle(self.right(), -delta.y * 0.01);
        let turned = pitch * (yaw * off);
        // Not over the top: looking straight down or up has no right.
        if turned.normalize_or_zero().dot(Vec3::Y).abs() < 0.999 { self.eye = self.target + turned; }
        else { self.eye = self.target + yaw * off; }
    }

    /// Slide the camera and its target, faster further away, so the scene
    /// follows the cursor. `sign` flips the direction for styles that drag
    /// the camera rather than the scene.
    pub fn pan(&mut self, delta: Vec2, sign: f32) {
        let k = (self.eye.distance(self.target) * 0.0015).max(0.0005) * sign;
        let d = self.right() * -delta.x * k + self.up() * delta.y * k;
        self.eye += d;
        self.target += d;
    }

    /// Move towards the target by a fraction of the distance, so zooming
    /// slows close up and never passes through the target.
    pub fn zoom(&mut self, amount: f32) {
        let off = self.eye - self.target;
        let dist = (off.length() * (-amount).exp()).clamp(0.02, 1.0e5);
        self.eye = self.target + off.normalize_or_zero() * dist;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_in_the_middle_of_the_view() {
        let c = Camera::default();
        let p = c.view_proj(1.5) * c.target.extend(1.0);
        assert!((p.x / p.w).abs() < 1e-5 && (p.y / p.w).abs() < 1e-5);
        assert!((0.0..=1.0).contains(&(p.z / p.w)));
    }

    #[test]
    fn zooming_never_passes_the_target_and_orbiting_keeps_the_distance() {
        let mut c = Camera::default();
        let d = c.eye.distance(c.target);
        c.orbit(Vec2::new(40.0, 25.0));
        assert!((c.eye.distance(c.target) - d).abs() < 1e-3);
        for _ in 0..200 { c.zoom(1.0); }
        assert!(c.eye.distance(c.target) >= 0.02 - 1e-6);
    }
}
