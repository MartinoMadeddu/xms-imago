//! Cameras: a UsdGeomCamera's values, as Imago reads, shows and writes them.
//!
//! A camera in the scene is a primitive with no geometry whose
//! `Prim::camera` holds these values; where it is and where it looks is its
//! placement, as for any primitive (USD cameras look down their -Z axis,
//! with +Y up). Load USD gives the cameras of a file, the Camera operator
//! makes one, Transform moves them, Write USD writes them. The Viewer looks
//! through one (`view_proj`) and draws the others (`glyph`).
//!
//! Lengths are USD's: focal length and apertures in tenths of a scene unit
//! (millimetres in a centimetre scene: a 50 mm lens on a 36 mm wide gate is
//! focal length 50, horizontal aperture 36), clipping range and focus
//! distance in scene units. The field of view depends on their ratio only.

use glam::{Mat4, Vec3};

/// How a camera projects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Projection {
    #[default]
    Perspective,
    Orthographic,
}

impl Projection {
    pub fn token(self) -> &'static str {
        match self { Projection::Perspective => "perspective", Projection::Orthographic => "orthographic" }
    }
}

/// A UsdGeomCamera's values. Defaults are USD's.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Camera {
    pub projection:                 Projection,
    pub focal_length:               f32,
    pub horizontal_aperture:        f32,
    pub vertical_aperture:          f32,
    pub horizontal_aperture_offset: f32,
    pub vertical_aperture_offset:   f32,
    /// Near and far, in scene units.
    pub clipping_range:             [f32; 2],
    /// 0: no depth of field.
    pub f_stop:                     f32,
    pub focus_distance:             f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            projection: Projection::Perspective,
            focal_length: 50.0,
            horizontal_aperture: 20.955,
            vertical_aperture: 15.2908,
            horizontal_aperture_offset: 0.0,
            vertical_aperture_offset: 0.0,
            clipping_range: [1.0, 1_000_000.0],
            f_stop: 0.0,
            focus_distance: 0.0,
        }
    }
}

/// Filmbacks (the gate's width and height, in millimetres), offered by the
/// Camera operator's Filmback Presets.
pub const FILMBACKS: &[(&str, f32, f32)] = &[
    ("USD default", 20.955, 15.2908),
    ("Full frame", 36.0, 24.0),
    ("Super 35", 24.89, 18.66),
    ("ALEXA Mini 16:9", 23.76, 13.37),
    ("ALEXA Mini Open Gate", 28.25, 18.17),
    ("ALEXA Mini LF Open Gate", 36.70, 25.54),
];

/// The preset a filmback is, if it is one.
pub fn filmback_name(width: f32, height: f32) -> Option<&'static str> {
    FILMBACKS.iter().find(|(_, w, h)| (w - width).abs() < 1e-3 && (h - height).abs() < 1e-3).map(|(n, _, _)| *n)
}

/// Its eye, the way it looks and its up, from its placement, and how many
/// metres a scene unit is there (the placement's scale: a centimetre
/// stage's cameras are placed at 0.01).
pub fn frame(world: Mat4) -> (Vec3, Vec3, Vec3, f32) {
    let eye = world.w_axis.truncate();
    let forward = (-world.z_axis.truncate()).normalize_or(Vec3::NEG_Z);
    let up = world.y_axis.truncate().normalize_or(Vec3::Y);
    let unit = world.x_axis.truncate().length().max(1e-9);
    (eye, forward, up, unit)
}

impl Camera {
    /// Width over height of its film gate.
    pub fn gate_aspect(&self) -> f32 { self.horizontal_aperture / self.vertical_aperture.max(1e-6) }

    /// The part of a view `aspect` wide per unit of height that its gate
    /// fills, as fractions of the view's width and height: the whole gate
    /// is shown, touching the view on two sides.
    pub fn gate_in_view(&self, aspect: f32) -> (f32, f32) {
        let gate = self.gate_aspect();
        if aspect >= gate { (gate / aspect.max(1e-6), 1.0) } else { (1.0, aspect / gate.max(1e-6)) }
    }

    /// The vertical field of view, in radians, of a view `aspect` wide that
    /// shows its whole gate.
    pub fn fov(&self, aspect: f32) -> f32 {
        let (_, h) = self.gate_in_view(aspect);
        let height = self.vertical_aperture / h;
        2.0 * (height * 0.5 / self.focal_length.max(1e-6)).atan()
    }

    /// World to clip space, looking through it from its placement `world`,
    /// in a view `aspect` wide per unit of height. Reversed depth, as the
    /// Viewer's own camera: 1 at the near plane.
    pub fn view_proj(&self, world: Mat4, aspect: f32) -> Mat4 {
        let (eye, forward, up, unit) = frame(world);
        let view = Mat4::look_to_rh(eye, forward, up);
        let near = (self.clipping_range[0] * unit).max(1e-4);
        match self.projection {
            Projection::Perspective => Mat4::perspective_infinite_reverse_rh(self.fov(aspect), aspect.max(1e-4), near) * view,
            Projection::Orthographic => {
                // The gate, in tenths of a scene unit, is what it sees.
                let (w, h) = self.gate_in_view(aspect);
                let width = self.horizontal_aperture * 0.1 * unit / w;
                let height = self.vertical_aperture * 0.1 * unit / h;
                let far = (self.clipping_range[1] * unit).max(near * 2.0);
                // Far and near swapped: reversed depth.
                Mat4::orthographic_rh(-width * 0.5, width * 0.5, -height * 0.5, height * 0.5, far, near) * view
            }
        }
    }

    /// The lines the Viewer draws for it, in world space, from its
    /// placement: a pyramid from its eye to its gate, as wide as its lens
    /// sees, and a triangle over the gate on the side that is up.
    pub fn glyph(&self, world: Mat4) -> Vec<[Vec3; 2]> {
        let (eye, forward, up, _) = frame(world);
        let right = forward.cross(up).normalize_or(Vec3::X);
        let up = right.cross(forward);
        let half_w = 0.12;
        let half_h = half_w / self.gate_aspect().max(1e-3);
        // As deep as the lens: long lenses make long pyramids.
        let depth = (half_w * 2.0 * self.focal_length / self.horizontal_aperture.max(1e-6)).clamp(0.12, 0.6);
        let at = |x: f32, y: f32| eye + forward * depth + right * x + up * y;
        let c = [at(-half_w, -half_h), at(half_w, -half_h), at(half_w, half_h), at(-half_w, half_h)];
        let mut lines = vec![];
        for k in 0..4 {
            lines.push([eye, c[k]]);
            lines.push([c[k], c[(k + 1) % 4]]);
        }
        let top = at(0.0, half_h * 1.7);
        lines.push([c[3], top]);
        lines.push([top, c[2]]);
        lines
    }

    /// Its attributes as a USD Camera prim has them: the projection, lens,
    /// gate and clipping range always, the rest where they are not USD's
    /// defaults.
    pub fn usda_lines(&self) -> Vec<String> {
        let mut out = vec![
            format!("token projection = \"{}\"", self.projection.token()),
            format!("float focalLength = {}", num(self.focal_length)),
            format!("float horizontalAperture = {}", num(self.horizontal_aperture)),
            format!("float verticalAperture = {}", num(self.vertical_aperture)),
            format!("float2 clippingRange = ({}, {})", num(self.clipping_range[0]), num(self.clipping_range[1])),
        ];
        if self.horizontal_aperture_offset != 0.0 { out.push(format!("float horizontalApertureOffset = {}", num(self.horizontal_aperture_offset))); }
        if self.vertical_aperture_offset != 0.0 { out.push(format!("float verticalApertureOffset = {}", num(self.vertical_aperture_offset))); }
        if self.f_stop != 0.0 { out.push(format!("float fStop = {}", num(self.f_stop))); }
        if self.focus_distance != 0.0 { out.push(format!("float focusDistance = {}", num(self.focus_distance))); }
        out
    }

    /// Its values as the Primitive Inspector lists them, by USD name.
    pub fn properties(&self) -> Vec<(String, String)> {
        vec![
            ("projection".into(), self.projection.token().into()),
            ("focalLength".into(), num(self.focal_length)),
            ("horizontalAperture".into(), num(self.horizontal_aperture)),
            ("verticalAperture".into(), num(self.vertical_aperture)),
            ("horizontalApertureOffset".into(), num(self.horizontal_aperture_offset)),
            ("verticalApertureOffset".into(), num(self.vertical_aperture_offset)),
            ("clippingRange".into(), format!("({}, {})", num(self.clipping_range[0]), num(self.clipping_range[1]))),
            ("fStop".into(), num(self.f_stop)),
            ("focusDistance".into(), num(self.focus_distance)),
        ]
    }
}

fn num(x: f32) -> String { if x.is_finite() { format!("{x}") } else { "0".into() } }

#[cfg(test)]
mod tests {
    use super::*;

    /// A 50 mm lens on a 36 x 24 gate sees about 39.6 degrees across and
    /// 27 degrees up and down, in a view the shape of its gate.
    #[test]
    fn the_lens_gives_the_field_of_view() {
        let c = Camera { focal_length: 50.0, horizontal_aperture: 36.0, vertical_aperture: 24.0, ..Default::default() };
        assert!((c.fov(1.5).to_degrees() - 26.99).abs() < 0.05);
        // A wider view shows the gate's full height and more on the sides.
        assert_eq!(c.gate_in_view(2.0), (0.75, 1.0));
        assert!((c.fov(2.0) - c.fov(1.5)).abs() < 1e-6);
        // A taller one shows the gate's full width.
        assert_eq!(c.gate_in_view(1.0), (1.0, 1.0 / 1.5));
        assert!(c.fov(1.0) > c.fov(1.5));
    }

    /// Looking through it: what is in front of it is in the middle of the
    /// view, in front of the near plane; it looks down its -Z.
    #[test]
    fn it_looks_down_its_minus_z() {
        let c = Camera::default();
        let world = Mat4::from_translation(Vec3::new(0.0, 1.0, 5.0));
        let vp = c.view_proj(world, 1.5);
        let p = vp * Vec3::new(0.0, 1.0, 0.0).extend(1.0);
        assert!((p.x / p.w).abs() < 1e-5 && (p.y / p.w).abs() < 1e-5);
        assert!(p.w > 0.0 && (0.0..=1.0).contains(&(p.z / p.w)));
        // Behind it: not in view.
        assert!((vp * Vec3::new(0.0, 1.0, 9.0).extend(1.0)).w < 0.0);
        // Orthographic: the same point in the middle, depth reversed.
        let ortho = Camera { projection: Projection::Orthographic, ..Default::default() };
        let q = ortho.view_proj(world, 1.5) * Vec3::new(0.0, 1.0, 0.0).extend(1.0);
        assert!((q.x / q.w).abs() < 1e-5 && (0.0..=1.0).contains(&(q.z / q.w)));
    }

    #[test]
    fn a_filmback_is_named_by_its_preset() {
        assert_eq!(filmback_name(23.76, 13.37), Some("ALEXA Mini 16:9"));
        let c = Camera::default();
        assert_eq!(filmback_name(c.horizontal_aperture, c.vertical_aperture), Some("USD default"));
        assert_eq!(filmback_name(30.0, 20.0), None);
    }

    #[test]
    fn written_as_usd_has_it() {
        let c = Camera { focus_distance: 4.5, ..Default::default() };
        let lines = c.usda_lines();
        assert!(lines.contains(&"token projection = \"perspective\"".to_string()));
        assert!(lines.contains(&"float2 clippingRange = (1, 1000000)".to_string()));
        assert!(lines.contains(&"float focusDistance = 4.5".to_string()) && !lines.iter().any(|l| l.contains("fStop")));
        assert_eq!(c.glyph(Mat4::IDENTITY).len(), 10);
    }
}
