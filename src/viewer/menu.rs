//! The Viewer's right-click menu. A right-click that does not drag opens it
//! (a drag with the navigation keys moves the camera); on a prim it selects
//! that prim first if it is not selected. On a prim: editing it, the
//! operators it comes from, selecting around it, hiding and isolating,
//! resetting its transform and moving its pivot, its material, looking
//! through it, framing it. On empty space: framing, selecting, showing
//! everything, shading, cameras, and making a shape, light or camera where
//! the ground was clicked.

use glam::{Vec2, Vec3};

use crate::core::poly::PickView;
use crate::types::{EvalResult, NodeId, NodeType, Prim};

use super::{Viewer, ViewerInput};

/// A pane the menu asks to be brought to the front (Show in Scene Explorer,
/// Inspect, Light Links): taken by the program, which holds the layout.
pub static PANE: std::sync::Mutex<Option<crate::layout::Pane>> = std::sync::Mutex::new(None);

fn bring(pane: crate::layout::Pane) {
    if let Ok(mut p) = PANE.lock() {
        *p = Some(pane);
    }
}

/// What the menu was opened on.
#[derive(Clone, Debug, Default)]
pub struct Opened {
    /// The prim right-clicked, as the Selection bar's Pick has it.
    pub prim: Option<String>,
    /// Where the ground (y = 0) was clicked, for Create.
    pub ground: Option<Vec3>,
}

/// A prim's type, as the Primitive Inspector says it.
fn kind(p: &Prim) -> String {
    if p.shading.is_some() {
        "Material".into()
    } else if p.camera.is_some() {
        "Camera".into()
    } else if let Some(l) = &p.light {
        l.kind.usd().into()
    } else if p.instancing.is_some() {
        "PointInstancer".into()
    } else if p.volume.is_some() {
        "Volume".into()
    } else {
        match p.geo.as_ref().map(|g| &g.topology) {
            Some(crate::core::geo::Topology::Curves { .. }) => "BasisCurves".into(),
            Some(crate::core::geo::Topology::Points) => "Points".into(),
            _ => "Mesh".into(),
        }
    }
}

/// The node `id`'s name.
fn name_of(graph: &crate::node_graph::NodeGraphState, id: NodeId) -> String {
    graph.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_default()
}

/// Select the operator `id`.
fn select_node(graph: &mut crate::node_graph::NodeGraphState, id: NodeId) {
    graph.selected_node = Some(id);
    graph.selected_nodes = vec![id];
}

/// Every path the scene has: its prims and the groups of its stages.
fn all_paths(prims: &[Prim]) -> Vec<String> {
    let mut out: Vec<String> = prims.iter().map(|p| p.path.clone()).collect();
    let mut trees: Vec<&std::sync::Arc<crate::usd_scene::StageTree>> = vec![];
    for p in prims {
        if let Some(t) = &p.stage {
            if !trees.iter().any(|x| std::sync::Arc::ptr_eq(x, t)) {
                trees.push(t);
            }
        }
    }
    for t in trees {
        out.extend(t.nodes.iter().map(|n| n.path.clone()));
    }
    out.sort();
    out.dedup();
    out
}

/// The parent of a path, none for a root prim.
fn parent_of(path: &str) -> Option<String> {
    let (up, _) = path.rsplit_once('/')?;
    (!up.is_empty()).then(|| up.to_string())
}

/// `node`'s output into the scene viewed: a Merge put in after the viewed
/// operator, the flag on it, `node` into its second input. The first one
/// viewed when nothing is.
fn into_scene(graph: &mut crate::node_graph::NodeGraphState, node: NodeId) {
    match graph.view_flag.filter(|v| *v != node) {
        Some(view) => {
            graph.selected_node = Some(view);
            let merge = graph.add_after_selected(NodeType::merge());
            graph.add_connection(node, 0, merge, 1);
            select_node(graph, node);
        }
        None => graph.set_view_flag(node),
    }
}

impl Viewer {
    /// The right-click menu: opened on a right-click that does not drag.
    pub(super) fn context_menu(
        &mut self,
        response: &egui::Response,
        rect: egui::Rect,
        view: &PickView,
        input: &mut ViewerInput,
    ) {
        let eval = |sid: crate::types::IceGraphId, mesh: &crate::core::geo::Geo, t: &[crate::ice::Reference]| {
            input.subnets.get(sid).map(|sg| sg.evaluate(mesh, t)).unwrap_or_else(|| mesh.clone())
        };
        if response.secondary_clicked() {
            let origin = Vec2::new(rect.min.x, rect.min.y);
            let at = response.interact_pointer_pos().map(|p| Vec2::new(p.x, p.y) - origin);
            let prims = match input.graph.evaluate_for_viewport_packed(&eval) {
                Some(EvalResult::Named(p)) => p,
                _ => vec![],
            };
            let prim = at
                .and_then(|px| super::pick::prim_at_drawn(view, px, &prims, &self.scene.standins))
                .map(|p| input.hierarchy.pick.picked(&p, &prims));
            if let Some(p) = &prim {
                if !input.hierarchy.selected_prims().contains(p) {
                    input.hierarchy.click(p, false, false, &[]);
                }
            }
            let ground = at.and_then(|px| {
                let (o, d) = view.ray(px);
                (d.y.abs() > 1e-6).then(|| -o.y / d.y).filter(|t| *t > 0.0).map(|t| o + d * t)
            });
            self.menu = Opened { prim, ground };
        }
        let opened = self.menu.clone();
        response.context_menu(|ui| {
            ui.set_min_width(210.0);
            let prims = match input.graph.evaluate_for_viewport_packed(&eval) {
                Some(EvalResult::Named(p)) => p,
                _ => vec![],
            };
            let chosen = input.hierarchy.selected_prims().to_vec();
            if opened.prim.is_some() && !chosen.is_empty() {
                self.prim_menu(ui, input, &prims, &chosen);
            } else {
                self.empty_menu(ui, input, &prims, opened.ground);
            }
        });
    }

    fn prim_menu(&mut self, ui: &mut egui::Ui, input: &mut ViewerInput, prims: &[Prim], chosen: &[String]) {
        use crate::icons;
        let first = chosen[0].clone();
        let prim = prims.iter().find(|p| p.path == first);
        let tree = prim.and_then(|p| p.stage.clone()).or_else(|| {
            prims
                .iter()
                .filter_map(|p| p.stage.as_ref())
                .find(|t| t.nodes.iter().any(|n| n.path == first))
                .cloned()
        });
        // Its path and type.
        let heading = match chosen {
            [one] => one.clone(),
            many => format!("{} prims", many.len()),
        };
        ui.add(egui::Label::new(egui::RichText::new(heading).strong()).truncate());
        let ty = prim.map(kind).or_else(|| {
            tree.as_ref()
                .and_then(|t| t.nodes.iter().find(|n| n.path == first).map(|n| n.type_name.clone()))
        });
        if let Some(ty) = ty {
            ui.weak(ty);
        }
        ui.separator();

        if ui.button(format!("{} Edit Primitive\tShift+Enter", icons::PENCIL_SIMPLE)).clicked() {
            input.graph.edit_primitives(chosen);
            ui.close();
        }
        if let Some(view) = input.graph.view_flag {
            let maker = crate::prim_inspector::maker(input.graph, view, &first, tree.as_deref());
            let moved = crate::prim_inspector::xform_by(input.graph, view, &first, tree.as_deref()).map(|x| x.node);
            if let Some(m) = maker {
                if ui.button(format!("Go to Operator: {}", name_of(input.graph, m))).clicked() {
                    select_node(input.graph, m);
                    ui.close();
                }
            }
            if let Some(m) = moved.filter(|m| Some(*m) != maker) {
                if ui.button(format!("Go to what moved it: {}", name_of(input.graph, m))).clicked() {
                    select_node(input.graph, m);
                    ui.close();
                }
            }
        }
        if ui.button("Show in Scene Explorer").clicked() {
            input.hierarchy.reveal_selected();
            bring(crate::layout::Pane::SceneExplorer);
            ui.close();
        }
        if ui.button("Inspect").on_hover_text("The Primitive Inspector, on what is selected").clicked() {
            bring(crate::layout::Pane::PrimInspector);
            ui.close();
        }
        ui.menu_button("Select", |ui| {
            let paths = all_paths(prims);
            let pick = |input: &mut ViewerInput, to: Vec<String>| {
                if !to.is_empty() {
                    input.hierarchy.select_boxed(to, false, false);
                }
            };
            if ui.button("Parent").clicked() {
                let mut up: Vec<String> = chosen.iter().filter_map(|p| parent_of(p)).collect();
                up.dedup();
                pick(input, up);
                ui.close();
            }
            if ui.button("Children").clicked() {
                let below: Vec<String> = paths
                    .iter()
                    .filter(|p| chosen.iter().any(|c| parent_of(p).as_deref() == Some(c.as_str())))
                    .cloned()
                    .collect();
                pick(input, below);
                ui.close();
            }
            for (mode, text) in [
                (crate::types::PickMode::Model, "Model"),
                (crate::types::PickMode::Prototype, "Prototype"),
            ] {
                if ui.button(text).clicked() {
                    let mut to: Vec<String> = chosen.iter().map(|c| mode.picked(c, prims)).collect();
                    to.dedup();
                    pick(input, to);
                    ui.close();
                }
            }
        });
        ui.separator();

        let set = |input: &mut ViewerInput, verb: &str, name: &'static str, v: serde_json::Value| {
            let values: Vec<_> = chosen.iter().map(|p| (p.clone(), name, v.clone())).collect();
            input.graph.record_edits(verb, &values);
        };
        if ui.button(format!("{} Hide", icons::EYE_SLASH)).clicked() {
            set(input, "Hide", "visibility", serde_json::json!("invisible"));
            ui.close();
        }
        if ui.button(format!("{} Show", icons::EYE)).clicked() {
            set(input, "Show", "visibility", serde_json::json!("inherited"));
            ui.close();
        }
        if ui
            .button("Isolate")
            .on_hover_text("Only these (and what is below them) drawn in this Viewer. Nothing in the scene changes; Show All on empty space ends it")
            .clicked()
        {
            self.isolate = chosen.to_vec();
            ui.close();
        }
        ui.separator();

        // Into the operator holding it, else the Edit Session.
        let sources = super::gizmo::sources(input.graph, input.subnets, chosen);
        if ui.button("Reset Transform").clicked() {
            let mut rest = vec![];
            for (path, src) in chosen.iter().zip(&sources) {
                match src {
                    Some(id) => input.graph.edit_animated(*id, |nt| {
                        if let Some((x, _)) = nt.xform_mut() {
                            x.translate = [0.0; 3];
                            x.rotate = [0.0; 3];
                            x.scale = [1.0; 3];
                        }
                    }),
                    None => rest.push(path.clone()),
                }
            }
            for (name, v) in [("translate", [0.0; 3]), ("rotate", [0.0; 3]), ("scale", [1.0; 3])] {
                let values: Vec<_> = rest.iter().map(|p| (p.clone(), name, serde_json::json!(v))).collect();
                if !values.is_empty() {
                    input.graph.record_edits("Reset", &values);
                }
            }
            ui.close();
        }
        // A pivot is an operator's: only for prims whose operator holds them.
        let held: Vec<(NodeId, &Prim)> = chosen
            .iter()
            .zip(&sources)
            .filter_map(|(path, s)| Some(((*s)?, prims.iter().find(|p| &p.path == path)?)))
            .filter(|(_, p)| !p.mesh.vertices.is_empty() || !p.mesh.points.is_empty())
            .collect();
        ui.add_enabled_ui(!held.is_empty(), |ui| {
            ui.menu_button("Move Pivot", |ui| {
                for (text, bottom) in [("To Centre", false), ("To Bottom", true)] {
                    if ui.button(text).clicked() {
                        for (id, p) in &held {
                            // Its box in its own space: where its points are.
                            let pts = p.mesh.vertices.iter().chain(p.mesh.points.iter());
                            let (lo, hi) = pts.fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
                                let v = Vec3::from(*v);
                                (lo.min(v), hi.max(v))
                            });
                            let mut at = (lo + hi) * 0.5;
                            if bottom {
                                at.y = lo.y;
                            }
                            input.graph.edit_animated(*id, |nt| {
                                if let Some((x, _)) = nt.xform_mut() {
                                    x.set_pivot(at.to_array());
                                }
                            });
                        }
                        ui.close();
                    }
                }
            })
        })
        .response
        .on_disabled_hover_text("A pivot is the Transform group's, of the operator that made the prim");
        ui.separator();

        // Materials: those in the scene to bind, the one it wears.
        let materials: Vec<String> = prims.iter().filter(|p| p.shading.is_some()).map(|p| p.path.clone()).collect();
        ui.add_enabled_ui(!materials.is_empty(), |ui| {
            ui.menu_button("Bind Material", |ui| {
                for m in &materials {
                    if ui.button(m.as_str()).clicked() {
                        bind(input.graph, chosen, m);
                        ui.close();
                    }
                }
            })
        });
        if let (Some(bound), Some(view)) = (prim.and_then(|p| p.material.clone()), input.graph.view_flag) {
            let name = bound.rsplit('/').next().unwrap_or(&bound).to_string();
            if let Some((id, n)) = crate::prim_inspector::material_of(input.graph, view, &name) {
                if ui.button(format!("Go to Material: {n}")).clicked() {
                    select_node(input.graph, id);
                    ui.close();
                }
            }
        }
        if prim.is_some_and(|p| p.camera.is_some()) && ui.button("Look Through").clicked() {
            super::camera::look_through(Some(first.clone()));
            ui.close();
        }
        if prim.is_some_and(|p| p.light.is_some()) && ui.button("Light Links").clicked() {
            bring(crate::layout::Pane::LightLinker);
            ui.close();
        }
        ui.separator();
        if ui.button("Frame Selected\tF").clicked() {
            self.frame_request = Some(true);
            ui.close();
        }
    }

    fn empty_menu(&mut self, ui: &mut egui::Ui, input: &mut ViewerInput, prims: &[Prim], ground: Option<Vec3>) {
        if ui.button("Frame All\tA").clicked() {
            self.frame_request = Some(false);
            ui.close();
        }
        if ui.button("Select All").clicked() {
            let roots: Vec<String> = all_paths(prims).into_iter().filter(|p| parent_of(p).is_none()).collect();
            input.hierarchy.select_boxed(roots, false, false);
            ui.close();
        }
        if ui.button("Select None").clicked() {
            input.hierarchy.clear_selection();
            ui.close();
        }
        if ui
            .button("Show All")
            .on_hover_text("Ends Isolate, and shows every hidden prim (into the Edit Session)")
            .clicked()
        {
            self.isolate.clear();
            let hidden: Vec<_> = prims
                .iter()
                .filter(|p| crate::core::properties::invisible(p))
                .map(|p| (p.path.clone(), "visibility", serde_json::json!("inherited")))
                .collect();
            if !hidden.is_empty() {
                input.graph.record_edits("Show", &hidden);
            }
            ui.close();
        }
        ui.separator();
        ui.menu_button("Shading", |ui| {
            use crate::viewport::display::{mode, set_mode, DisplayMode};
            for (m, text) in [
                (DisplayMode::Wireframe, "Wireframe"),
                (DisplayMode::HiddenLine, "Hidden Line Removal"),
                (DisplayMode::Shaded, "Shaded"),
                (DisplayMode::Textured, "Textured"),
            ] {
                if ui.radio(mode() == m, text).clicked() {
                    set_mode(m);
                    ui.close();
                }
            }
        });
        ui.menu_button("Camera", |ui| {
            let looking = super::camera::looking_through();
            if ui.radio(looking.is_none(), "Perspective").clicked() {
                super::camera::look_through(None);
                ui.close();
            }
            for c in prims.iter().filter(|p| p.camera.is_some()) {
                if ui.radio(looking.as_deref() == Some(c.path.as_str()), c.path.as_str()).clicked() {
                    super::camera::look_through(Some(c.path.clone()));
                    ui.close();
                }
            }
        });
        ui.separator();
        ui.menu_button("Create", |ui| {
            let at = ground.unwrap_or(Vec3::ZERO);
            let mut made: Option<NodeType> = None;
            for (text, nt) in [
                ("Cube", NodeType::cube(1.0)),
                ("Sphere", NodeType::sphere(0.5, 32)),
                ("Grid", NodeType::grid(10, 10, 2.0)),
            ] {
                if ui.button(text).clicked() {
                    made = Some(nt);
                }
            }
            ui.separator();
            use crate::core::light::{Light, LightType};
            for (text, light) in [
                ("Rect Light", Light::new(LightType::Rect)),
                ("Point Light", Light::point()),
                ("Sphere Light", Light::new(LightType::Sphere)),
                ("Disk Light", Light::new(LightType::Disk)),
                ("Dome Light", Light::new(LightType::Dome)),
                ("Distant Light", Light::new(LightType::Distant)),
            ] {
                if ui.button(text).clicked() {
                    made = Some(NodeType::light(light));
                }
            }
            ui.separator();
            if ui.button("Camera").clicked() {
                made = Some(NodeType::camera());
            }
            if let Some(mut nt) = made {
                // Where the ground was clicked: a shape stands on it, a light
                // or camera keeps its height and offset from it.
                let up = match &nt {
                    NodeType::CreateCube { size, .. } => size * 0.5,
                    NodeType::CreateSphere { radius, .. } => *radius,
                    _ => 0.0,
                };
                if let Some((x, _)) = nt.xform_mut() {
                    let t = Vec3::from(x.translate) + Vec3::Y * up;
                    x.translate = (at + t).to_array();
                }
                let id = input.graph.add_node(
                    crate::node_graph::ui::default_name(&nt),
                    nt,
                    egui::pos2(0.0, 0.0),
                );
                place_beside_view(input.graph, id);
                into_scene(input.graph, id);
                ui.close();
            }
        });
    }
}

/// A new operator put beside the one viewed.
fn place_beside_view(graph: &mut crate::node_graph::NodeGraphState, id: NodeId) {
    let at = graph
        .view_flag
        .and_then(|v| graph.nodes.iter().find(|n| n.id == v))
        .map(|n| n.position + egui::vec2(240.0, 0.0));
    if let (Some(at), Some(n)) = (at, graph.nodes.iter_mut().find(|n| n.id == id)) {
        n.position = at;
    }
}

/// `chosen` bound to the material at `material`: a row of the Material
/// Binding nearest upstream of the viewed operator, one put in after it when
/// there is none.
fn bind(graph: &mut crate::node_graph::NodeGraphState, chosen: &[String], material: &str) {
    let Some(view) = graph.view_flag else { return };
    let binding = crate::prim_inspector::upstream(graph, view, |n| matches!(n.node_type, NodeType::MaterialBinding { .. }))
        .map(|n| n.id);
    let id = match binding {
        Some(id) => id,
        None => {
            graph.selected_node = Some(view);
            graph.add_after_selected(NodeType::MaterialBinding {
                library: crate::types::material_library(),
                inputs: 1,
                bindings: vec![],
                options: vec![],
            })
        }
    };
    if let Some(n) = graph.nodes.iter_mut().find(|n| n.id == id) {
        if let NodeType::MaterialBinding { bindings, .. } = &mut n.node_type {
            // A row per prim, each bound to the material by its path: the
            // rows before keep what they bind.
            while bindings.len() < n.locations.len() {
                bindings.push(Default::default());
            }
            for p in chosen {
                n.locations.push(p.clone());
                bindings.push(crate::core::material::BindTo::Path(material.to_string()));
            }
        }
    }
    select_node(graph, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bind Material: a row per prim on the Material Binding upstream, bound
    /// to the material by path; one put in after the viewed operator when
    /// there is none. Create: the new operator in the scene through a Merge.
    #[test]
    fn bind_and_create_go_into_the_graph() {
        let mut g = crate::node_graph::NodeGraphState::default();
        let cube = g.add_node("Cube".into(), NodeType::cube(1.0), egui::pos2(0.0, 0.0));
        g.set_view_flag(cube);
        bind(&mut g, &["/cube".to_string()], "/materials/red");
        let mb = g.nodes.iter().find(|n| matches!(n.node_type, NodeType::MaterialBinding { .. })).unwrap();
        assert_eq!(mb.locations, vec!["/cube".to_string()]);
        let NodeType::MaterialBinding { bindings, .. } = &mb.node_type else { panic!() };
        assert_eq!(bindings, &vec![crate::core::material::BindTo::Path("/materials/red".into())]);
        assert_eq!(g.view_flag, Some(mb.id), "the flag moved on to it");
        let mb = mb.id;
        // Again: the same node, one more row.
        bind(&mut g, &["/cube".to_string()], "/materials/blue");
        assert_eq!(g.nodes.iter().filter(|n| matches!(n.node_type, NodeType::MaterialBinding { .. })).count(), 1);
        assert_eq!(g.nodes.iter().find(|n| n.id == mb).unwrap().locations.len(), 2);

        let light = g.add_node("Key".into(), NodeType::light(crate::core::light::Light::point()), egui::pos2(0.0, 0.0));
        into_scene(&mut g, light);
        let view = g.view_flag.unwrap();
        assert!(matches!(g.nodes.iter().find(|n| n.id == view).unwrap().node_type, NodeType::Merge { .. }));
        assert!(g.connections.iter().any(|c| c.from_node == light && c.to_node == view));
        assert!(g.connections.iter().any(|c| c.from_node == mb && c.to_node == view));
    }
}
