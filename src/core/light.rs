//! Lights, as UsdLux has them: a type (Dome, Rect, Sphere, Disk, Cylinder,
//! Distant) and values by attribute name (`inputs:intensity`,
//! `inputs:texture:file`, `poleAxis`).
//!
//! What each type has, its fallbacks, labels and groups, come from
//! `assets/usdlux.json`, written from OpenUSD's schemas: LightAPI's
//! attributes for every light, then the type's own. A light keeps only
//! the values that differ from them; the rest are the schema's.
//!
//! A light is a primitive with no geometry (`Prim::light`), as a camera is:
//! a file's, read by Load USD, or the Light operator's. Its placement says
//! where it is and which way it faces: Rect, Disk and Distant lights emit
//! along their -Z.

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::Vec3;

use crate::core::properties::Value;
use crate::core::render::Param;

/// A UsdLux light type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LightType {
    #[default]
    Dome,
    Rect,
    Sphere,
    Disk,
    Cylinder,
    Distant,
}

impl LightType {
    pub const ALL: [LightType; 6] = [LightType::Dome, LightType::Rect, LightType::Sphere, LightType::Disk, LightType::Cylinder, LightType::Distant];

    /// Its schema's name, the prim's type in USD.
    pub fn usd(self) -> &'static str {
        match self {
            LightType::Dome => "DomeLight", LightType::Rect => "RectLight", LightType::Sphere => "SphereLight",
            LightType::Disk => "DiskLight", LightType::Cylinder => "CylinderLight", LightType::Distant => "DistantLight",
        }
    }

    /// The type of a prim type name: `DomeLight` and `DomeLight_1` are Dome.
    pub fn from_usd(name: &str) -> Option<Self> {
        let base = name.split('_').next().unwrap_or(name);
        LightType::ALL.into_iter().find(|t| t.usd() == base)
    }
}

/// A light: its type, and the values set that are not the schema's.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Light {
    pub kind:   LightType,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
}

/// The UsdLux definitions, read once.
#[derive(serde::Deserialize)]
struct Definitions {
    common: Vec<Param>,
    types:  Vec<TypeDef>,
    /// Fallbacks a type has of its own for a common attribute (a distant
    /// light's intensity is 50000).
    #[serde(default)]
    defaults: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

#[derive(serde::Deserialize)]
struct TypeDef {
    name:   String,
    label:  String,
    params: Vec<Param>,
}

fn definitions() -> &'static Definitions {
    static DEFS: std::sync::OnceLock<Definitions> = std::sync::OnceLock::new();
    DEFS.get_or_init(|| serde_json::from_str(include_str!("../../assets/usdlux.json")).expect("the UsdLux definitions Imago comes with read"))
}

fn type_def(kind: LightType) -> &'static TypeDef {
    definitions().types.iter().find(|t| t.name == kind.usd()).expect("every light type is defined")
}

impl Light {
    pub fn new(kind: LightType) -> Self { Light { kind, values: BTreeMap::new() } }

    /// A point light: a sphere light treated as a point.
    pub fn point() -> Self {
        let mut l = Light::new(LightType::Sphere);
        l.values.insert("treatAsPoint".into(), Value::Bool(true));
        l
    }

    pub fn is_point(&self) -> bool { self.kind == LightType::Sphere && self.value("treatAsPoint") == Some(Value::Bool(true)) }

    /// What it is called: Dome Light, Point Light…
    pub fn label(&self) -> &'static str {
        if self.is_point() { return "Point Light"; }
        &type_def(self.kind).label
    }

    /// Every attribute it has, LightAPI's first, then its type's.
    pub fn params(&self) -> Vec<&'static Param> {
        definitions().common.iter().chain(type_def(self.kind).params.iter()).collect()
    }

    /// The schema's fallback for an attribute of this type.
    pub fn fallback(&self, p: &Param) -> Value {
        match definitions().defaults.get(self.kind.usd()).and_then(|d| d.get(&p.name)) {
            Some(j) => crate::core::render::value_of(p.value_type(), j),
            None => p.default_value(),
        }
    }

    /// An attribute's value: set, else the schema's. `None` for one it does not have.
    pub fn value(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.values.get(name) { return Some(v.clone()); }
        self.params().into_iter().find(|p| p.name == name).map(|p| self.fallback(p))
    }

    /// Sets an attribute: kept when it differs from the schema's, dropped when not.
    pub fn set(&mut self, name: &str, v: Value) {
        let fallback = self.params().into_iter().find(|p| p.name == name).map(|p| self.fallback(p));
        if fallback.as_ref() == Some(&v) { self.values.remove(name); } else { self.values.insert(name.to_string(), v); }
    }

    /// Changes its type: the values the new type has too are kept.
    pub fn set_kind(&mut self, kind: LightType) {
        let old = std::mem::take(&mut self.values);
        self.kind = kind;
        for (k, v) in old { if self.params().iter().any(|p| p.name == k) { self.set(&k, v); } }
        self.operator_defaults();
    }

    /// What the Light operator starts with where UsdLux's fallback is not
    /// what is wanted: a dome drawn 1 m across (`guideRadius` 1, its
    /// fallback a kilometre).
    pub fn operator_defaults(&mut self) {
        if self.kind == LightType::Dome && !self.values.contains_key("guideRadius") {
            self.values.insert("guideRadius".into(), Value::Float(1.0));
        }
    }

    /// A float attribute, or its fallback.
    pub fn float(&self, name: &str) -> f32 {
        match self.value(name) { Some(Value::Float(x)) => x, Some(Value::Double(x)) => x as f32, Some(Value::Int(i)) => i as f32, _ => 0.0 }
    }

    /// Its attributes as `.usda` writes them. `all`: every one, as the Light
    /// operator writes a light it makes; else those set. An empty file path
    /// is not written.
    pub fn usda_lines(&self, all: bool) -> Vec<String> {
        let mut out = vec![];
        for p in self.params() {
            let Some(v) = self.values.get(&p.name).cloned().or_else(|| all.then(|| self.fallback(p))) else { continue };
            if matches!(&v, Value::Asset(s) if s.trim().is_empty()) { continue; }
            out.push(format!("{}{} {} = {}", if p.uniform { "uniform " } else { "" }, v.kind().usd(), p.name, v.usda()));
        }
        // Values a file had that the definitions do not list (shaping, shadow).
        for (k, v) in &self.values {
            if !self.params().iter().any(|p| &p.name == k) { out.push(format!("{} {k} = {}", v.kind().usd(), v.usda())); }
        }
        out
    }

    /// Its values as the Primitive Inspector lists them: name, value.
    pub fn properties(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self.params().into_iter().map(|p| {
            let v = self.values.get(&p.name).cloned().unwrap_or_else(|| self.fallback(p));
            (p.name.clone(), v.display())
        }).collect();
        for (k, v) in &self.values {
            if !out.iter().any(|(n, _)| n == k) { out.push((k.clone(), v.display())); }
        }
        out
    }
}

// ── Drawn ──────────────────────────────────────────────────────────────────

/// How far a dome is drawn at most, in metres: a file's dome at UsdLux's
/// guide radius (1 km) would be past the Viewer's far plane.
pub const DOME_DRAWN: f32 = 12.0;

/// Points of a circle of radius `r` in a plane, `n` segments, as lines.
fn circle(centre: Vec3, a: Vec3, b: Vec3, r: f32, n: usize, out: &mut Vec<[Vec3; 2]>) {
    let at = |k: usize| { let t = k as f32 / n as f32 * std::f32::consts::TAU; centre + (a * t.cos() + b * t.sin()) * r };
    for k in 0..n { out.push([at(k), at(k + 1)]); }
}

/// A letter of the compass, `s` high, at `at` in the ground plane: its
/// strokes in a unit box, right along +X, up along -Z (north), so it reads
/// from above.
fn letter(c: char, at: Vec3, s: f32, out: &mut Vec<[Vec3; 2]>) {
    let strokes: &[&[(f32, f32)]] = match c {
        'N' => &[&[(-0.5, -0.5), (-0.5, 0.5), (0.5, -0.5), (0.5, 0.5)][..]],
        'E' => &[&[(0.5, 0.5), (-0.5, 0.5), (-0.5, -0.5), (0.5, -0.5)][..], &[(-0.5, 0.0), (0.3, 0.0)][..]],
        'S' => &[&[(0.5, 0.5), (-0.5, 0.5), (-0.5, 0.0), (0.5, 0.0), (0.5, -0.5), (-0.5, -0.5)][..]],
        'W' => &[&[(-0.5, 0.5), (-0.25, -0.5), (0.0, 0.2), (0.25, -0.5), (0.5, 0.5)][..]],
        _ => &[],
    };
    let at2 = |(u, v): (f32, f32)| at + (Vec3::X * u * 0.6 - Vec3::Z * v) * s;
    for line in strokes { for w in line.windows(2) { out.push([at2(w[0]), at2(w[1])]); } }
}

/// An arrow from `from` along `dir`, `len` long, with a small head.
fn arrow(from: Vec3, dir: Vec3, side: Vec3, len: f32, out: &mut Vec<[Vec3; 2]>) {
    let tip = from + dir * len;
    out.push([from, tip]);
    let h = len * 0.25;
    out.push([tip, tip - dir * h + side * h * 0.5]);
    out.push([tip, tip - dir * h - side * h * 0.5]);
}

impl Light {
    /// Its shape as lines in its own space, before its placement: a dome,
    /// half a sphere over a ground circle with the compass's points (at its
    /// guide radius, at most `DOME_DRAWN`; its pole on Z when `poleAxis` is), a rectangle or a
    /// disk with the way it emits (-Z), a sphere, a point's star, a
    /// cylinder along X, a distant light's parallel arrows.
    pub fn glyph_local(&self) -> Vec<[Vec3; 2]> {
        let mut out = vec![];
        let o = Vec3::ZERO;
        let (x, y, z) = (Vec3::X, Vec3::Y, Vec3::Z);
        match self.kind {
            LightType::Dome => {
                let r = self.float("guideRadius").clamp(0.05, DOME_DRAWN);
                // The ground circle, and the half above: two arches across
                // it and a ring at 45 degrees.
                circle(o, x, z, r, 64, &mut out);
                let arch = |a: Vec3, out: &mut Vec<[Vec3; 2]>| {
                    let at = |k: usize| { let t = k as f32 / 32.0 * std::f32::consts::PI; (a * t.cos() + y * t.sin()) * r };
                    for k in 0..32 { out.push([at(k), at(k + 1)]); }
                };
                arch(x, &mut out);
                arch(z, &mut out);
                let (s, c) = std::f32::consts::FRAC_PI_4.sin_cos();
                circle(y * r * s, x, z, r * c, 48, &mut out);
                out.push([y * r, y * r * 1.08]);
                // The compass: north along -Z, east along +X, a tick out of
                // the circle at each and its letter beyond.
                for (c, d) in [('N', -z), ('E', x), ('S', z), ('W', -x)] {
                    let tick = if c == 'N' { 0.16 } else { 0.08 };
                    out.push([d * r, d * r * (1.0 + tick)]);
                    letter(c, d * r * 1.28, r * 0.12, &mut out);
                }
                if self.value("poleAxis") == Some(Value::Token("Z".into())) {
                    let up = glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                    for l in &mut out { *l = [up * l[0], up * l[1]]; }
                }
            }
            LightType::Rect => {
                let (w, h) = (self.float("inputs:width") * 0.5, self.float("inputs:height") * 0.5);
                let c = [Vec3::new(-w, -h, 0.0), Vec3::new(w, -h, 0.0), Vec3::new(w, h, 0.0), Vec3::new(-w, h, 0.0)];
                for k in 0..4 { out.push([c[k], c[(k + 1) % 4]]); }
                out.push([c[0], c[2]]);
                out.push([c[1], c[3]]);
                arrow(o, -z, x, (w.max(h) * 0.8).max(0.2), &mut out);
            }
            LightType::Sphere if self.is_point() => {
                let s = 0.12;
                for d in [x, y, z, (x + y).normalize(), (x - y).normalize(), (y + z).normalize(), (y - z).normalize()] { out.push([-d * s, d * s]); }
            }
            LightType::Sphere => {
                let r = self.float("inputs:radius").max(0.01);
                circle(o, x, y, r, 48, &mut out);
                circle(o, x, z, r, 48, &mut out);
                circle(o, y, z, r, 48, &mut out);
            }
            LightType::Disk => {
                let r = self.float("inputs:radius").max(0.01);
                circle(o, x, y, r, 48, &mut out);
                arrow(o, -z, x, (r * 1.6).max(0.2), &mut out);
            }
            LightType::Cylinder => {
                let (l, r) = (self.float("inputs:length") * 0.5, self.float("inputs:radius").max(0.01));
                circle(x * l, y, z, r, 32, &mut out);
                circle(-x * l, y, z, r, 32, &mut out);
                for d in [y, -y, z, -z] { out.push([x * l + d * r, -x * l + d * r]); }
            }
            LightType::Distant => {
                let r = 0.25;
                circle(o, x, y, r, 32, &mut out);
                for side in [x * r * 0.6, -x * r * 0.6, y * r * 0.6, -y * r * 0.6, o] { arrow(side, -z, x, 0.8, &mut out); }
            }
        }
        out
    }

    /// Its shape where it is: `glyph_local` through its placement, the
    /// dome's size kept (its scale is the environment's, not a size).
    pub fn glyph(&self, world: glam::Mat4) -> Vec<[Vec3; 2]> {
        let m = if self.kind == LightType::Dome {
            let (_, r, t) = world.to_scale_rotation_translation();
            glam::Mat4::from_rotation_translation(r, t)
        } else { world };
        self.glyph_local().into_iter().map(|[a, b]| [m.transform_point3(a), m.transform_point3(b)]).collect()
    }

    /// The colour it is drawn in: its colour, times its temperature's when
    /// that is on, brought to full brightness so a dim light still shows.
    pub fn tint(&self) -> [f32; 3] {
        let c = match self.value("inputs:color") { Some(Value::Color3f(c)) => c, _ => [1.0; 3] };
        let c = match (self.value("inputs:enableColorTemperature"), self.value("inputs:colorTemperature")) {
            (Some(Value::Bool(true)), Some(Value::Float(k))) => { let t = blackbody(k); [c[0] * t[0], c[1] * t[1], c[2] * t[2]] }
            _ => c,
        };
        let top = c[0].max(c[1]).max(c[2]);
        if top <= 1e-6 { [0.3; 3] } else { [c[0] / top, c[1] / top, c[2] / top] }
    }
}

/// An approximate blackbody colour for a temperature in kelvin, white at
/// 6500 K: enough to tint what is drawn.
pub fn blackbody(kelvin: f32) -> [f32; 3] {
    // Tanner Helland's fit, divided by 6500 K's.
    fn raw(k: f32) -> [f32; 3] {
        let t = (k / 100.0).clamp(10.0, 400.0);
        let r = if t <= 66.0 { 255.0 } else { 329.698_73 * (t - 60.0).powf(-0.133_204_76) };
        let g = if t <= 66.0 { 99.470_8 * t.ln() - 161.119_57 } else { 288.122_16 * (t - 60.0).powf(-0.075_514_85) };
        let b = if t >= 66.0 { 255.0 } else if t <= 19.0 { 0.0 } else { 138.517_73 * (t - 10.0).ln() - 305.044_8 };
        [r.clamp(0.0, 255.0) / 255.0, g.clamp(0.0, 255.0) / 255.0, b.clamp(0.0, 255.0) / 255.0]
    }
    let (c, w) = (raw(kelvin), raw(6500.0));
    [c[0] / w[0].max(1e-3), c[1] / w[1].max(1e-3), c[2] / w[2].max(1e-3)]
}

/// A primitive that is a light, at a placement.
pub fn light_prim(path: &str, light: Light, place: crate::types::Placement) -> crate::types::Prim {
    crate::types::Prim { place, light: Some(Arc::new(light)), ..crate::types::Prim::new(path.to_string(), crate::types::MeshData::default()) }
}

/// Where the Light operator puts a light of this kind unless told: `/lights/dome`.
pub fn default_location(light: &Light) -> String {
    if light.is_point() { return "/lights/point".into(); }
    format!("/lights/{}", light.kind.usd().trim_end_matches("Light").to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_is_defined_with_lightapi_first() {
        for t in LightType::ALL {
            let l = Light::new(t);
            let names: Vec<&str> = l.params().iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names[0], "inputs:intensity", "{t:?}");
            assert!(names.contains(&"inputs:color") && names.contains(&"inputs:specular"), "{t:?}");
            for p in l.params() {
                assert_eq!(l.fallback(p).kind(), p.value_type(), "{t:?} {}", p.name);
                if !p.choices.is_empty() { assert!(p.choices.contains(&l.fallback(p).display()), "{t:?} {}", p.name); }
            }
            assert_eq!(LightType::from_usd(t.usd()), Some(t));
        }
        assert_eq!(LightType::from_usd("DomeLight_1"), Some(LightType::Dome));
        assert_eq!(LightType::from_usd("Camera"), None);
    }

    #[test]
    fn values_are_kept_only_where_they_differ() {
        let mut l = Light::new(LightType::Distant);
        assert_eq!(l.value("inputs:intensity"), Some(Value::Float(50000.0)), "a distant light's own fallback");
        assert_eq!(l.value("inputs:angle"), Some(Value::Float(0.53)));
        l.set("inputs:intensity", Value::Float(50000.0));
        assert!(l.values.is_empty());
        l.set("inputs:intensity", Value::Float(3.0));
        assert_eq!(l.usda_lines(false), vec!["float inputs:intensity = 3".to_string()]);
        // Every attribute, as the operator writes them; no empty texture.
        let dome = Light::new(LightType::Dome);
        let lines = dome.usda_lines(true);
        assert!(lines.contains(&"token inputs:texture:format = \"automatic\"".to_string()), "{lines:#?}");
        assert!(lines.contains(&"uniform token poleAxis = \"scene\"".to_string()));
        assert!(lines.contains(&"color3f inputs:color = (1, 1, 1)".to_string()));
        assert!(!lines.iter().any(|l| l.contains("texture:file")));
    }

    #[test]
    fn a_point_is_a_sphere_and_types_keep_what_they_share() {
        let p = Light::point();
        assert!(p.is_point() && p.kind == LightType::Sphere);
        assert_eq!((p.label(), default_location(&p).as_str()), ("Point Light", "/lights/point"));
        let mut l = Light::new(LightType::Rect);
        l.set("inputs:intensity", Value::Float(8.0));
        l.set("inputs:width", Value::Float(2.0));
        l.set_kind(LightType::Disk);
        assert_eq!(l.value("inputs:intensity"), Some(Value::Float(8.0)), "kept");
        assert!(!l.values.contains_key("inputs:width"), "a disk has no width");
        assert_eq!(default_location(&l), "/lights/disk");
    }

    #[test]
    fn every_type_has_a_shape_in_its_place() {
        for t in LightType::ALL {
            let l = Light::new(t);
            assert!(l.glyph_local().len() >= 4, "{t:?}");
        }
        assert!(Light::point().glyph_local().len() < Light::new(LightType::Sphere).glyph_local().len(), "a point is a star, not a sphere");
        // A rect of 2 by 1: its corners at ±1, ±0.5, emitting along -Z.
        let mut r = Light::new(LightType::Rect);
        r.set("inputs:width", Value::Float(2.0));
        let g = r.glyph_local();
        assert!(g.iter().any(|[a, _]| (*a - Vec3::new(-1.0, -0.5, 0.0)).length() < 1e-6));
        assert!(g.iter().any(|[_, b]| b.z < -0.5), "the arrow points down -Z");
        // Placed: moved; a dome keeps its size whatever the scale.
        let at = glam::Mat4::from_scale_rotation_translation(Vec3::splat(100.0), glam::Quat::IDENTITY, Vec3::new(0.0, 3.0, 0.0));
        let dome = Light::new(LightType::Dome).glyph(at);
        let far = dome.iter().map(|[a, _]| (*a - Vec3::new(0.0, 3.0, 0.0)).length()).fold(0.0f32, f32::max);
        assert!(far <= DOME_DRAWN * 1.5, "{far}");
        // A dome is half a sphere, on the ground: nothing below it.
        let mut d = Light::new(LightType::Dome);
        d.set("guideRadius", Value::Float(2.0));
        let g = d.glyph_local();
        assert!(g.iter().flatten().all(|p| p.y > -1e-4), "nothing below the ground circle");
        assert!(g.iter().flatten().any(|p| (p.y - 2.0).abs() < 1e-3), "its top at the radius");
        assert!(g.iter().flatten().any(|p| p.z < -2.4), "north's letter past the circle, along -Z");
        // Its pole on Z: the top along +Z.
        d.set("poleAxis", Value::Token("Z".into()));
        assert!(d.glyph_local().iter().flatten().any(|p| (p.z - 2.0).abs() < 1e-3));
    }

    #[test]
    fn the_operator_draws_a_dome_a_metre_across() {
        let mut d = Light::new(LightType::Dome);
        assert_eq!(d.float("guideRadius"), 100000.0, "UsdLux's fallback, a file's dome");
        d.operator_defaults();
        assert_eq!(d.float("guideRadius"), 1.0);
        let mut r = Light::new(LightType::Rect);
        r.set_kind(LightType::Dome);
        assert_eq!(r.float("guideRadius"), 1.0, "a Light turned into a dome too");
        r.set("guideRadius", Value::Float(5.0));
        r.set_kind(LightType::Dome);
        assert_eq!(r.float("guideRadius"), 5.0, "one set is kept");
    }

    #[test]
    fn temperature_tints() {
        let w = blackbody(6500.0);
        assert!(w.iter().all(|c| (c - 1.0).abs() < 1e-3), "{w:?}");
        let warm = blackbody(3000.0);
        assert!(warm[0] > warm[2], "warm is red: {warm:?}");
        let mut l = Light::new(LightType::Rect);
        l.set("inputs:enableColorTemperature", Value::Bool(true));
        l.set("inputs:colorTemperature", Value::Float(3000.0));
        let t = l.tint();
        assert!(t[0] > t[2] && (t[0] - 1.0).abs() < 1e-6, "{t:?}");
    }
}
