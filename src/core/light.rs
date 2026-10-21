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

}
