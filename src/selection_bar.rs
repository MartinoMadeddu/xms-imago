//! The Selection bar (after XSI's main command panel): what is selected in the
//! Viewer or the Scene Explorer, not an operator. Where its edits go, what a
//! click picks, the move, rotate and scale tools, the selected prims' scale,
//! rotation and translation to type in, and edits of them. Every edit goes
//! into the Edit Session at the display flag, one put in after the operator
//! viewed if need be (`core::edit_session`), as a drag of the handles does.

use glam::{EulerRot, Mat4, Quat, Vec3};

use crate::core::edit_session::Place;
use crate::core::manip::Tool;
use crate::icons;
use crate::modelling::PolyTool;
use crate::node_graph::NodeGraphState;
use crate::properties::form::{group, FieldExt, Status};
use crate::types::{PickMode, SceneHierarchy};
use crate::viewer::gizmo;

/// The space the fields show: the prim's own transform under its parents, or
/// where it is in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Space {
    #[default]
    Local,
    Global,
}

/// A transform's translate, rotate (degrees, X then Y then Z) and scale.
fn parts(m: Mat4) -> [[f32; 3]; 3] {
    let (s, r, t) = m.to_scale_rotation_translation();
    let (x, y, z) = r.to_euler(EulerRot::XYZ);
    [
        t.to_array(),
        [x.to_degrees(), y.to_degrees(), z.to_degrees()],
        s.to_array(),
    ]
}

fn compose(p: [[f32; 3]; 3]) -> Mat4 {
    let r = p[1].map(f32::to_radians);
    Mat4::from_scale_rotation_translation(
        Vec3::from(p[2]),
        Quat::from_euler(EulerRot::XYZ, r[0], r[1], r[2]),
        Vec3::from(p[0]),
    )
}

/// A place's values in `space`.
pub fn shown(p: &Place, space: Space) -> [[f32; 3]; 3] {
    let v = match space {
        Space::Local => p.trs,
        Space::Global => parts(p.parents * compose(p.trs)),
    };
    // No "-0.0" in a field.
    v.map(|row| row.map(|x| x + 0.0))
}

/// `part` (0 translate, 1 rotate, 2 scale) of each place with its `axis` set
/// to `v` in `space`: what goes into the Edit Session, the prim's own value.
pub fn typed(
    places: &[Place],
    space: Space,
    part: usize,
    axis: usize,
    v: f32,
) -> Vec<(String, &'static str, serde_json::Value)> {
    let name = ["translate", "rotate", "scale"][part];
    places
        .iter()
        .map(|p| {
            let mut now = shown(p, space);
            now[part][axis] = v;
            let own = match space {
                Space::Local => now,
                Space::Global => parts(p.parents.inverse() * compose(now)),
            };
            (p.path.clone(), name, serde_json::json!(own[part]))
        })
        .collect()
}

/// A toggle in a row of them, as the Scene Explorer's purpose chips.
fn chip(ui: &mut egui::Ui, on: bool, text: &str, hint: &str) -> bool {
    ui.selectable_label(on, text).on_hover_text(hint).clicked()
}

pub fn draw(
    ui: &mut egui::Ui,
    graph: &mut NodeGraphState,
    subnets: &crate::ice::IceGraphStore,
    hierarchy: &mut SceneHierarchy,
    tool: &mut PolyTool,
) {
    let chosen = hierarchy.selected_prims().to_vec();
    let places = gizmo::scene_places(graph, subnets, &chosen);
    let space_id = egui::Id::new("selection_bar_space");
    let mut space: Space = ui.data(|d| d.get_temp(space_id)).unwrap_or_default();

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        // Nothing in it may be wider than the bar: a wider row pushes the
        // bar off the window.
        ui.set_max_width(ui.available_width());
        // What is selected (a long path cut short), and where its edits go.
        let text = match chosen.as_slice() {
            [] => "Nothing selected".to_string(),
            [one] => one.clone(),
            many => format!("{} prims", many.len()),
        };
        ui.add(egui::Label::new(egui::RichText::new(text).strong()).truncate())
            .on_hover_text(chosen.join("\n"));
        let target = graph.session_at_view();
        ui.label(egui::RichText::new("Edits go into").weak());
        match target {
            Some(id) => {
                let name = graph.nodes.iter().find(|n| n.id == id).map(|n| n.name.clone()).unwrap_or_default();
                if ui
                    .add(egui::Button::new(format!("{} {name}", icons::ARROWS_OUT_CARDINAL)).truncate())
                    .on_hover_text("The Edit Session at the display flag: click to select it and see its steps")
                    .clicked()
                {
                    graph.selected_node = Some(id);
                    graph.selected_nodes = vec![id];
                }
            }
            None => {
                let after = graph
                    .view_flag
                    .and_then(|v| graph.nodes.iter().find(|n| n.id == v))
                    .map(|n| n.name.clone());
                let text = match after {
                    Some(n) => format!("A new Edit Session after {n}"),
                    None => "Nothing viewed".into(),
                };
                ui.label(egui::RichText::new(text).weak()).on_hover_text(
                    "The first edit puts an Edit Session in after the operator with the display flag, and moves the flag to it",
                );
            }
        }
        ui.add_space(2.0);

        // Select, after XSI's: the big arrow (the Select tool) on the left,
        // what a click picks down its right.
        group(ui, "Select", Some("What a click or a box in the Viewer selects"), |f| {
            f.ui.horizontal(|ui| {
                let h = ui.spacing().interact_size.y;
                let gap = 3.0;
                // The arrow a fixed square; the modes take the rest, one
                // line each.
                let tall = 52.0;
                let arrow = egui::Button::selectable(
                    tool.tool == Tool::Select,
                    egui::RichText::new(icons::CURSOR).size(tall * 0.5),
                );
                if ui.add_sized([tall, tall], arrow).on_hover_text("Select (Q)").clicked() {
                    tool.tool = Tool::Select;
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = gap;
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    let w = ui.available_width();
                    for (mode, text, hint) in [
                        (PickMode::Prim, "Prims", "The prim under the pointer (usdview's Prims)"),
                        (PickMode::Model, "Models", "The model it is part of (usdview's Models): the nearest prim above of kind component, else assembly or group"),
                        (PickMode::Instance, "Instances", "The instance it is part of (usdview's Instances): a point instancer's instance, or the nearest instanceable prim above"),
                        (PickMode::Prototype, "Prototypes", "The prototype it is a copy of (usdview's Prototypes): for a point instancer's instance, its prototype under the instancer"),
                        (PickMode::Top, "Top", "The prim at the top of its hierarchy"),
                    ] {
                        if ui
                            .add_sized([w, h], egui::Button::selectable(hierarchy.pick == mode, text))
                            .on_hover_text(hint)
                            .clicked()
                        {
                            hierarchy.pick = mode;
                        }
                    }
                });
            });
        });

        // Transform, after XSI's: the tools, then T, R and S, each its X, Y
        // and Z down beside a tall button that picks its tool. The fields
        // are always there: empty with nothing to show.
        group(ui, "Transform", Some("The selected prims' own transforms. Typing sets every selected prim's value; a field showing – differs between them"), |f| {
            f.ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
            let h = f.ui.spacing().interact_size.y;
            f.ui.horizontal(|ui| {
                let w = ((ui.available_width() - 9.0) / 4.0).floor();
                for (t, icon, hint) in [
                    (Tool::Select, icons::CURSOR, "Select (Q)"),
                    (Tool::Move, icons::ARROWS_OUT_CARDINAL, "Move (W)"),
                    (Tool::Rotate, icons::ARROWS_CLOCKWISE, "Rotate (E)"),
                    (Tool::Scale, icons::RESIZE, "Scale (R)"),
                ] {
                    if ui.add_sized([w, h], egui::Button::selectable(tool.tool == t, icon)).on_hover_text(hint).clicked() {
                        tool.tool = t;
                    }
                }
            });
            f.ui.add_space(3.0);
            let values: Vec<[[f32; 3]; 3]> = places.iter().map(|p| shown(p, space)).collect();
            let (axis_w, head_w) = (18.0, 22.0);
            for (part, head, t, hint, speed) in [
                (0, "T", Tool::Move, "Move (W)", 0.01),
                (1, "R", Tool::Rotate, "Rotate (E)", 0.5),
                (2, "S", Tool::Scale, "Scale (R)", 0.01),
            ] {
                f.ui.horizontal(|ui| {
                    let field_w = (ui.available_width() - axis_w - head_w - 6.0).floor();
                    ui.vertical(|ui| {
                        for (axis, name) in ["x", "y", "z"].into_iter().enumerate() {
                            ui.horizontal(|ui| {
                                if values.is_empty() {
                                    // Nothing to show: an empty field, kept.
                                    let (rect, _) = ui.allocate_exact_size(egui::vec2(field_w, h), egui::Sense::hover());
                                    let fill = if crate::theme::dark_fields() {
                                        crate::properties::form::ink::FIELD()
                                    } else {
                                        ui.visuals().extreme_bg_color
                                    };
                                    ui.painter().rect_filled(rect, 2.0, fill);
                                } else {
                                    let first = values[0][part][axis];
                                    let mixed = values.iter().any(|v| (v[part][axis] - first).abs() > 1e-4);
                                    let mut v = first;
                                    let mut drag = egui::DragValue::new(&mut v).speed(speed).max_decimals(3);
                                    if mixed {
                                        drag = drag.custom_formatter(|_, _| "–".into());
                                    }
                                    let r = ui.field_sized([field_w, h], drag);
                                    if r.changed() && v != first {
                                        let verb = crate::core::edit_session::verb(part);
                                        graph.record_edits(verb, &typed(&places, space, part, axis, v));
                                    }
                                }
                                ui.add_sized([axis_w, h], egui::Label::new(egui::RichText::new(name).weak()));
                            });
                        }
                    });
                    let tall = 3.0 * h + 6.0;
                    if ui
                        .add_sized([head_w, tall], egui::Button::selectable(tool.tool == t, head))
                        .on_hover_text(hint)
                        .clicked()
                    {
                        tool.tool = t;
                    }
                });
                f.ui.add_space(4.0);
            }
            f.ui.horizontal(|ui| {
                for (s, text, hint) in [
                    (Space::Global, "Global", "In the world: typed in, put back under its parents"),
                    (Space::Local, "Local", "Under its parents: the prim's own values"),
                ] {
                    if chip(ui, space == s, text, hint) {
                        space = s;
                    }
                }
            });
            if places.is_empty() && !chosen.is_empty() {
                let why = if chosen.len() > gizmo::MAX_PRIMS {
                    format!("More than {} prims selected", gizmo::MAX_PRIMS)
                } else {
                    "Not placed here: an instance's copies are placed by their instancer".into()
                };
                f.status(Status::Info, why);
            }
        });

        group(ui, "Edit", None, |f| {
            let any = !chosen.is_empty();
            f.ui.add_enabled_ui(any, |ui| {
                let wide = |ui: &mut egui::Ui, text: String, hint: &str| {
                    ui.add_sized([ui.available_width(), 22.0], egui::Button::new(text)).on_hover_text(hint).clicked()
                };
                if wide(
                    ui,
                    format!("{} Edit Selected", icons::PENCIL_SIMPLE),
                    "An Edit Primitive naming them after the operator viewed, for all their values (Enter in the Scene Explorer)",
                ) {
                    graph.edit_primitives(&chosen);
                }
                let set = |graph: &mut NodeGraphState, verb: &str, name: &'static str, v: serde_json::Value| {
                    let values: Vec<_> = chosen.iter().map(|p| (p.clone(), name, v.clone())).collect();
                    graph.record_edits(verb, &values);
                };
                if wide(ui, format!("{} Hide", icons::EYE_SLASH), "Visibility invisible, in the Edit Session") {
                    set(graph, "Hide", "visibility", serde_json::json!("invisible"));
                }
                if wide(ui, format!("{} Show", icons::EYE), "Visibility inherited, in the Edit Session") {
                    set(graph, "Show", "visibility", serde_json::json!("inherited"));
                }
                if wide(
                    ui,
                    format!("{} Deactivate", icons::PROHIBIT),
                    "Active off, in the Edit Session: the prim and what is below it are gone from the scene (USD's delete). Switch the step off to have them back",
                ) {
                    set(graph, "Deactivate", "active", serde_json::json!(false));
                }
            });
        });
    });
    ui.data_mut(|d| d.insert_temp(space_id, space));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_go_under_the_parents() {
        let place = Place {
            path: "/a/b".into(),
            parents: Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
            trs: [[1.0, 0.0, 0.0], [0.0; 3], [1.0; 3]],
        };
        let places = [place.clone()];
        assert_eq!(shown(&place, Space::Global)[0], [11.0, 0.0, 0.0]);
        // Local: as typed.
        let v = typed(&places, Space::Local, 0, 1, 2.0);
        assert_eq!(v[0].2, serde_json::json!([1.0, 2.0, 0.0]));
        // Global X 12 under a parent at 10: its own 2.
        let v = typed(&places, Space::Global, 0, 0, 12.0);
        assert_eq!(v[0].2, serde_json::json!([2.0, 0.0, 0.0]));
        // Each prim keeps its other axes.
        let other = Place {
            path: "/c".into(),
            parents: Mat4::IDENTITY,
            trs: [[0.0, 5.0, 0.0], [0.0; 3], [1.0; 3]],
        };
        let v = typed(&[place, other], Space::Local, 0, 0, 3.0);
        assert_eq!(v[1].2, serde_json::json!([3.0, 5.0, 0.0]));
    }

    #[test]
    fn a_pick_selects_the_model_or_the_top() {
        assert_eq!(PickMode::Top.picked("/set/table/leg", &[]), "/set");
        assert_eq!(PickMode::Prim.picked("/set/table/leg", &[]), "/set/table/leg");
        // No stage: the prim itself.
        assert_eq!(PickMode::Model.picked("/set/table/leg", &[]), "/set/table/leg");
    }
}
