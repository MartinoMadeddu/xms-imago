//! Light linking: which prims each light lights and shadows, as USD has it:
//! the light's `lightLink` and `shadowLink` collections (`core::collections`).
//! A light without them lights and shadows everything (`includeRoot`).
//!
//! The Light Linker operator is a grid: a row per light (a path taking the
//! lights at and below it, a pattern or `%collection`), a column per prim (the same), and in each cell Light and
//! Shadow, each on, off or as it comes in. On adds the prim to the light's
//! collection, off takes it out; the nearest of them above a prim decides, so
//! `/set` off and `/set/hero` on lights the hero alone of the set.

use crate::core::collections::{self, Collection, Held};
use crate::core::locations;
use crate::types::{EvalResult, Prim};

/// The collection a light lights by.
pub const LIGHT_LINK: &str = "lightLink";
/// The collection a light casts shadows by.
pub const SHADOW_LINK: &str = "shadowLink";

/// One cell of the grid: a light row's and a prim column's text, and what it
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
    /// Whether it says nothing.
    pub fn is_empty(&self) -> bool {
        self.lights.is_none() && self.shadows.is_none()
    }
}

/// The cell of a row and a column, if it says something.
pub fn cell<'a>(links: &'a [Link], light: &str, prim: &str) -> Option<&'a Link> {
    links.iter().find(|l| l.light == light && l.prim == prim)
}

/// The cell of a row and a column to change, made where there is none.
pub fn cell_mut<'a>(links: &'a mut Vec<Link>, light: &str, prim: &str) -> &'a mut Link {
    match links.iter().position(|l| l.light == light && l.prim == prim) {
        Some(k) => &mut links[k],
        None => {
            links.push(Link {
                light: light.to_string(),
                prim: prim.to_string(),
                ..Default::default()
            });
            links.last_mut().expect("just pushed")
        }
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

/// What a Light Linker puts out: the scene coming in with the lights its rows
/// take linked to the prims its columns take, cell by cell, as each says.
pub fn link_lights(input: &EvalResult, lights: &[String], prims: &[String], links: &[Link]) -> EvalResult {
    let EvalResult::Named(scene) = input else {
        return input.clone();
    };
    if links.iter().all(Link::is_empty) {
        return input.clone();
    }
    let held = collections::gather(scene);
    let known = crate::node_graph::scene_paths(scene);
    let all_lights = light_paths(scene);
    // Each column as the paths it takes (and what its `%other` takes out).
    let columns: Vec<(Vec<String>, Vec<String>)> = prims
        .iter()
        .map(|c| collections::expand_rows(std::slice::from_ref(c), &known, &held))
        .collect();
    let mut out = scene.clone();
    for row in locations::rows(lights) {
        let read = collections::resolve(std::slice::from_ref(&row.to_string()), &held).into_owned();
        let taken: Vec<&String> = all_lights.iter().filter(|p| locations::matches(&read[0], p)).collect();
        for light in taken {
            for (name, says) in [
                (LIGHT_LINK, (|l: &Link| l.lights) as fn(&Link) -> Option<bool>),
                (SHADOW_LINK, |l: &Link| l.shadows),
            ] {
                let mut c = coming_in(&held, light, name);
                let mut changed = false;
                // A `%collection` column links what it includes; what it
                // excludes is left as it was.
                for (column, (on, _)) in prims.iter().zip(&columns) {
                    let Some(v) = cell(links, row, column).and_then(says) else {
                        continue;
                    };
                    for p in on {
                        link(&mut c, p, v);
                    }
                    changed = true;
                }
                if changed {
                    collections::simplify(&mut c);
                    collections::put(&mut out, light, c);
                }
            }
        }
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

    fn rows(r: &[&str]) -> Vec<String> {
        r.iter().map(|s| s.to_string()).collect()
    }

    fn collection_of(out: &EvalResult, light: &str, name: &str) -> Option<Collection> {
        let EvalResult::Named(p) = out else { return None };
        collections::gather(p)
            .into_iter()
            .find(|h| h.holder == light && h.collection.name == name)
            .map(|h| h.collection)
    }

    #[test]
    fn a_cell_off_takes_the_prim_out_of_what_the_light_lights() {
        let scene = EvalResult::Named(vec![light("/lights/key"), prim("/set/a"), prim("/set/b")]);
        let links = vec![Link {
            light: "/lights/key".into(),
            prim: "/set/b".into(),
            lights: Some(false),
            shadows: None,
        }];
        let out = link_lights(&scene, &rows(&["/lights/key"]), &rows(&["/set/b"]), &links);
        let c = collection_of(&out, "/lights/key", LIGHT_LINK).expect("a lightLink");
        assert!(c.has("/set/a") && !c.has("/set/b"));
        assert!(
            collection_of(&out, "/lights/key", SHADOW_LINK).is_none(),
            "shadows as they came"
        );
    }

    #[test]
    fn a_prim_on_below_a_group_off_is_lit_alone() {
        let scene = EvalResult::Named(vec![light("/lights/rim"), prim("/set/hero"), prim("/set/wall")]);
        let cells = |light: &str, prim: &str, on: bool| Link {
            light: light.into(),
            prim: prim.into(),
            lights: Some(on),
            shadows: Some(on),
        };
        let links = vec![cells("/lights/*", "/set", false), cells("/lights/*", "/set/hero", true)];
        let out = link_lights(&scene, &rows(&["/lights/*"]), &rows(&["/set", "/set/hero"]), &links);
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
            light: "/lights/sun".into(),
            prim: "/set/a".into(),
            lights: Some(false),
            shadows: None,
        }];
        let EvalResult::Named(out) = link_lights(
            &EvalResult::Named(scene.clone()),
            &rows(&["/lights/sun"]),
            &rows(&["/set/a"]),
            &links,
        ) else {
            panic!()
        };
        // No light by that path: nothing linked.
        assert_eq!(out.len(), 1);
        // One made in Imago: linked.
        scene.push(light("/lights/sun"));
        let out = link_lights(
            &EvalResult::Named(scene),
            &rows(&["/lights/sun"]),
            &rows(&["/set/a"]),
            &links,
        );
        assert!(!collection_of(&out, "/lights/sun", LIGHT_LINK).unwrap().has("/set/a"));
    }

    #[test]
    fn light_types() {
        assert!(is_light_type("RectLight") && is_light_type("DomeLight_1") && is_light_type("PluginLight"));
        assert!(!is_light_type("Mesh") && !is_light_type("PluginLightFilter"));
    }
}
