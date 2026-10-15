pub mod ui;

use crate::types::{EvalResult, MeshData, NodeId, Prim, PrimVar, PrimVarInterp};

/// What the Primitive Inspector shows: the scene at the operator with the
/// view flag, all of it, or the rows selected in the Scene Explorer and what
/// is below them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub node:  Option<NodeId>,
    pub paths: Vec<String>,
}

/// The primitives of a scene at or below any of `paths` (all of them with
/// none), each where it is drawn. `/set` takes `/set` and `/set/a`, not
/// `/settle`.
pub fn scene_parts(scene: EvalResult, paths: &[String]) -> Vec<MeshData> {
    match scene {
        EvalResult::Named(prims) => {
            let under = |q: &str| paths.is_empty() || paths.iter().any(|p| {
                let p = p.trim_end_matches('/');
                q == p || q.strip_prefix(p).is_some_and(|rest| rest.starts_with('/'))
            });
            prims.iter().filter(|x| under(&x.path)).map(part).collect()
        }
        scene => scene.into_parts(),
    }
}

/// One primitive where it is drawn, with the Object values it inherits
/// from the prims above it (Set Attribute on a group) among its Object
/// attributes.
fn part(p: &Prim) -> MeshData {
    let mut m = p.world().into_owned();
    for (name, value) in crate::core::props::inherited_primvars(p) {
        if let Some(n) = value.numbers() {
            m.primvars.push(PrimVar { name, interp: PrimVarInterp::Constant, values: vec![n] });
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Prim;

    #[test]
    fn a_selected_row_shows_what_is_at_or_below_it() {
        let cube = || crate::node_graph::nodes::create_cube(1.0);
        let scene = || EvalResult::Named(["/set/a", "/set/grp/b", "/settle", "/other"].iter()
            .map(|p| Prim::new(p.to_string(), cube())).collect());
        let sel = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(scene_parts(scene(), &[]).len(), 4, "nothing selected: the whole scene");
        assert_eq!(scene_parts(scene(), &sel(&["/set"])).len(), 2, "a group: what is below it, not /settle");
        assert_eq!(scene_parts(scene(), &sel(&["/set/grp/b"])).len(), 1, "a prim: itself");
        assert_eq!(scene_parts(scene(), &sel(&["/set/a", "/other"])).len(), 2, "several rows: all of them");
        assert!(scene_parts(scene(), &sel(&["/gone"])).is_empty());
        // A plain mesh has no path: always all of it.
        assert_eq!(scene_parts(EvalResult::Single(cube()), &sel(&["/set"])).len(), 1);
    }
}
