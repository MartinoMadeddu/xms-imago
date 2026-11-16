//! Light linking: which prims each light lights and shadows, as USD has it:
//! the light's `lightLink` and `shadowLink` collections (`core::collections`).
//! A light without them lights and shadows everything (`includeRoot`).
//!
//! The Light Linker operator is a table, a row per link: its lights (a path
//! taking the lights at and below it, a pattern or `%collection`), its prims
//! (the same, a prim with what is below it), and Light and Shadow, each on,
//! off or as it comes in. On adds the prims to the light's collection, off
//! takes them out; the nearest of them above a prim decides, so `/set` off and
//! `/set/hero` on lights the hero alone of the set. Rows are applied in order:
//! a later row wins on the same prim.

use crate::core::collections::{self, Collection, Held};
use crate::core::locations;
use crate::types::{EvalResult, Prim};

/// The collection a light lights by.
pub const LIGHT_LINK: &str = "lightLink";
/// The collection a light casts shadows by.
pub const SHADOW_LINK: &str = "shadowLink";

/// One link: a row of the table, its lights' and prims' text, and what it
/// says (`None`: as it comes in).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Link {
    pub light: String,
    pub prim: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lights: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadows: Option<bool>,
}

impl Link {
    /// A new row: Light and Shadow on, to be turned off.
    pub fn new(light: &str, prim: &str) -> Link {
        Link {
            light: light.to_string(),
            prim: prim.to_string(),
            lights: Some(true),
            shadows: Some(true),
        }
    }

    /// Whether it changes anything.
    pub fn says_something(&self) -> bool {
        !self.light.trim().is_empty()
            && !self.prim.trim().is_empty()
            && (self.lights.is_some() || self.shadows.is_some())
    }
}

/// A link for a prim dropped on the operator: in the first row without
/// prims, else a new row, its lights left to fill in. None for one it has.
pub fn add_prim(links: &mut Vec<Link>, prim: &str) {
    if links.iter().any(|l| l.prim == prim) {
        return;
    }
    match links.iter_mut().find(|l| l.prim.trim().is_empty()) {
        Some(l) => l.prim = prim.to_string(),
        None => links.push(Link::new("", prim)),
    }
}

/// The paths of the scene's lights: those Imago made, and those of its stages.
pub fn light_paths(prims: &[Prim]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut trees: Vec<&std::sync::Arc<crate::usd_scene::StageTree>> = vec![];
    for p in prims {
        if let Some(t) = &p.stage {
            if !trees.iter().any(|x| std::sync::Arc::ptr_eq(x, t)) {
                trees.push(t);
            }
        }
    }
    for n in trees.iter().flat_map(|t| t.nodes.iter()) {
        if is_light_type(&n.type_name) && !out.contains(&n.path) {
            out.push(n.path.clone());
        }
    }
    for p in prims.iter().filter(|p| p.light.is_some()) {
        if !out.contains(&p.path) {
            out.push(p.path.clone());
        }
    }
    out
}

/// A USD light's type: `RectLight`, `DomeLight_1`, a renderer's own
/// (`ArnoldMeshLight`, `PluginLight`). Not a light filter.
pub fn is_light_type(type_name: &str) -> bool {
    let base = type_name.split('_').next().unwrap_or(type_name);
    base.ends_with("Light") && !base.ends_with("LightFilter")
}

/// The light's collection `name` as it comes in, or everything.
fn coming_in(held: &[Held], light: &str, name: &str) -> Collection {
    held.iter()
        .find(|h| h.holder == light && h.collection.name == name)
        .map(|h| h.collection.clone())
        .unwrap_or_else(|| Collection {
            name: name.to_string(),
            includes: vec!["/".into()],
            excludes: vec![],
            explicit_only: false,
        })
}

/// A prim put in (`on`) or taken out of a collection.
fn link(c: &mut Collection, path: &str, on: bool) {
    let (into, from) = if on {
        (&mut c.includes, &mut c.excludes)
    } else {
        (&mut c.excludes, &mut c.includes)
    };
    from.retain(|p| p != path);
    if !into.iter().any(|p| p == path) {
        into.push(path.to_string());
    }
}

/// What a Light Linker puts out: the scene coming in with each row's lights
/// linked to its prims, as it says, row after row.
pub fn link_lights(input: &EvalResult, links: &[Link]) -> EvalResult {
    let EvalResult::Named(scene) = input else {
        return input.clone();
    };
    if !links.iter().any(Link::says_something) {
        return input.clone();
    }
    let held = collections::gather(scene);
    let known = crate::node_graph::scene_paths(scene);
    let all_lights = light_paths(scene);
    // Each light's collections as they become, in the order first changed.
    let mut made: Vec<(String, Collection)> = vec![];
    for l in links.iter().filter(|l| l.says_something()) {
        let read = collections::resolve(std::slice::from_ref(&l.light), &held).into_owned();
        // A `%collection` of prims links what it includes; what it excludes is
        // left as it was.
        let (on, _) = collections::expand_rows(std::slice::from_ref(&l.prim), &known, &held);
        for light in all_lights.iter().filter(|p| locations::matches(&read[0], p)) {
            for (name, says) in [(LIGHT_LINK, l.lights), (SHADOW_LINK, l.shadows)] {
                let Some(v) = says else { continue };
                let k = match made.iter().position(|(p, c)| p == light && c.name == name) {
                    Some(k) => k,
                    None => {
                        made.push((light.clone(), coming_in(&held, light, name)));
                        made.len() - 1
                    }
                };
                for p in &on {
                    link(&mut made[k].1, p, v);
                }
            }
        }
    }
    let mut out = scene.clone();
    for (light, mut c) in made {
        collections::simplify(&mut c);
        collections::put(&mut out, &light, c);
    }
    EvalResult::Named(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MeshData;

    fn prim(path: &str) -> Prim {
        Prim::new(path.to_string(), MeshData::default())
    }

    fn light(path: &str) -> Prim {
        Prim {
            light: Some(std::sync::Arc::new(crate::core::light::Light::point())),
            ..prim(path)
        }
    }

    fn collection_of(out: &EvalResult, light: &str, name: &str) -> Option<Collection> {
        let EvalResult::Named(p) = out else { return None };
        collections::gather(p)
            .into_iter()
            .find(|h| h.holder == light && h.collection.name == name)
            .map(|h| h.collection)
    }

    #[test]
    fn a_link_off_takes_the_prim_out_of_what_the_light_lights() {
        let scene = EvalResult::Named(vec![light("/lights/key"), prim("/set/a"), prim("/set/b")]);
        let links = vec![Link {
            lights: Some(false),
            shadows: None,
            ..Link::new("/lights/key", "/set/b")
        }];
        let out = link_lights(&scene, &links);
        let c = collection_of(&out, "/lights/key", LIGHT_LINK).expect("a lightLink");
        assert!(c.has("/set/a") && !c.has("/set/b"));
        assert!(
            collection_of(&out, "/lights/key", SHADOW_LINK).is_none(),
            "shadows as they came"
        );
    }

    #[test]
    fn a_later_row_on_below_an_earlier_off_is_lit_alone() {
        let scene = EvalResult::Named(vec![light("/lights/rim"), prim("/set/hero"), prim("/set/wall")]);
        let off = Link {
            lights: Some(false),
            shadows: Some(false),
            ..Link::new("/lights/*", "/set")
        };
        let links = vec![off, Link::new("/lights", "/set/hero")];
        let out = link_lights(&scene, &links);
        for name in [LIGHT_LINK, SHADOW_LINK] {
            let c = collection_of(&out, "/lights/rim", name).unwrap();
            assert!(
                c.has("/set/hero") && !c.has("/set/wall") && c.has("/elsewhere"),
                "{name}: {c:?}"
            );
        }
    }

    #[test]
    fn only_lights_are_linked() {
        let mut scene = vec![prim("/set/a")];
        let links = vec![Link {
            lights: Some(false),
            ..Link::new("/lights/sun", "/set/a")
        }];
        let EvalResult::Named(out) = link_lights(&EvalResult::Named(scene.clone()), &links) else {
            panic!()
        };
        assert_eq!(out.len(), 1, "no light by that path: nothing linked");
        scene.push(light("/lights/sun"));
        let out = link_lights(&EvalResult::Named(scene), &links);
        assert!(!collection_of(&out, "/lights/sun", LIGHT_LINK).unwrap().has("/set/a"));
        // A row without its prims says nothing.
        assert!(!Link::new("/lights/sun", " ").says_something());
    }

    #[test]
    fn a_prim_dropped_fills_the_empty_row_then_adds_one() {
        let mut links = vec![Link::new("", "")];
        add_prim(&mut links, "/set/a");
        add_prim(&mut links, "/set/b");
        add_prim(&mut links, "/set/a");
        let prims: Vec<&str> = links.iter().map(|l| l.prim.as_str()).collect();
        assert_eq!(prims, ["/set/a", "/set/b"]);
        assert!(links
            .iter()
            .all(|l| l.light.is_empty() && l.lights == Some(true) && l.shadows == Some(true)));
    }

    #[test]
    fn light_types() {
        assert!(is_light_type("RectLight") && is_light_type("DomeLight_1") && is_light_type("PluginLight"));
        assert!(!is_light_type("Mesh") && !is_light_type("PluginLightFilter"));
    }
}
