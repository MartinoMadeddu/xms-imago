//! The rig operators' Parameters.

use std::sync::Arc;

use super::form::{group, Rows, Status};
use crate::core::anim::AnimData;
use crate::core::rig::build::BuildSettings;
use crate::core::rig::presets::{Axis, Preset};
use crate::core::rig::{self, Component, Finger, RotOrder, Side};
use crate::node_graph::NodeGraphState;
use crate::types::{EvalResult, NodeId, NodeType, Prim};

/// What reaches a rig operator and what it gives, read before its
/// parameters are borrowed.
#[derive(Default)]
pub struct RigIn {
    /// The clip on the first port that carries one: the guides coming into a
    /// Rig Guide, the guides of a Build Skeleton, the skeleton of a Range of
    /// Motion; the skeleton (second port) of the skin operators.
    pub clip: Option<Arc<AnimData>>,
    /// What it gives, as a clip.
    pub out_clip: Option<Arc<AnimData>>,
    /// What it gives, as primitives.
    pub out_prims: Option<Vec<Prim>>,
    /// A mesh is wired.
    pub mesh: bool,
    pub frame: f64,
}

pub fn rig_in(graph: &NodeGraphState, id: NodeId) -> RigIn {
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else {
        return RigIn::default();
    };
    if !rig::node::is_rig(&node.node_type) {
        return RigIn::default();
    }
    let pass = |_: crate::types::IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
    let port = |k: usize| node.inputs.get(k).and_then(|i| i.connected_output);
    let clip_port = match node.node_type {
        NodeType::RigGuide { .. } => 1,
        NodeType::BuildSkeleton { .. } | NodeType::RangeOfMotion { .. } => 0,
        _ => 1,
    };
    let clip = port(clip_port).and_then(|(src, out)| graph.eval_anim_out(src, out));
    let out = graph.eval_node_out(id, 0, &mut Default::default(), &pass);
    let (out_clip, out_prims) = match out {
        Some(EvalResult::Anim(a)) => (Some(a), None),
        Some(EvalResult::Named(p)) => (None, Some(p)),
        _ => (None, None),
    };
    RigIn {
        clip,
        out_clip,
        out_prims,
        mesh: matches!(node.node_type, NodeType::RigGuide { .. }) && port(0).is_some(),
        frame: graph.frame(),
    }
}

/// The Viewer's choice while a skin operator is viewed: regions, or one
/// joint's weights.
fn show_row(f: &mut Rows, joints: &[String]) {
    let mut shown = rig::view::shown();
    f.row("Show", Some("What the Viewer paints on the geometry while this operator is viewed: each joint's region in a colour of its own, or one joint's weights, dark blue (none) to yellow (all). It changes only the Viewer"), |ui| {
        egui::ComboBox::from_id_salt("rig_show")
            .width(ui.available_width())
            .selected_text(shown.clone().unwrap_or_else(|| "Regions".into()))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut shown, None, "Regions");
                for j in joints {
                    ui.selectable_value(&mut shown, Some(j.clone()), j);
                }
            });
    });
    if shown != rig::view::shown() {
        rig::view::show(shown);
    }
}

fn joints_of(rig_in: &RigIn) -> Vec<String> {
    rig_in.clip.as_deref().map(rig::node::deforming).unwrap_or_default()
}

/// Weights on primitives: points, and how many joints weigh on each.
fn weight_summary(f: &mut Rows, prims: Option<&Vec<Prim>>, sk: Option<&Arc<AnimData>>) {
    let (Some(prims), Some(sk)) = (prims, sk) else {
        f.status(
            Status::Info,
            "Connect the geometry, then the skeleton from Build Skeleton",
        );
        return;
    };
    let mut points = 0;
    let mut used = std::collections::HashSet::new();
    let mut blended = 0;
    for p in prims {
        if let Some(w) = rig::bind::read_weights(p, sk) {
            points += w.len();
            for x in &w {
                used.insert(x[0].0);
                if x[1].1 > 0.01 {
                    blended += 1;
                }
            }
        }
    }
    if points == 0 {
        f.status(
            Status::Bad,
            "No weights: the geometry has no polygons, or the skeleton is not the one it was weighted to",
        );
        return;
    }
    f.value("Points", format!("{points}, {blended} blended"));
    f.value("Regions", format!("{} joints", used.len()));
}

fn component_defaults(c: Component) -> (u32, u32) {
    match c {
        Component::Spine => (5, 2),
        Component::Neck => (2, 2),
        _ => (1, 2),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn rig_rows(ui: &mut egui::Ui, t: &mut NodeType, r: &RigIn) {
    match t {
        NodeType::RigGuide {
            component,
            side,
            index,
            attach,
            autofit,
            segments,
            twists,
            fingers,
            metacarpals,
            edits,
        } => {
            group(ui, "Component", Some("One part of a rig: its guide locators, added to the guides that come into the second input. Chain one Rig Guide after another to build up a body; Build Skeleton makes the joints from them all"), |f| {
                let was = *component;
                f.dropdown("Component", None, component, &Component::ALL.map(|c| (c, c.label())));
                if *component != was {
                    let (s, _) = component_defaults(*component);
                    *segments = s;
                    edits.clear();
                    if component.sided() && *side == Side::Centre {
                        *side = Side::Left;
                    }
                }
                if component.sided() {
                    f.choice("Side", Some("Left is +X, the character facing +Z"), side, &[(Side::Left, "Left"), (Side::Right, "Right")]);
                }
                f.drag_u32("Index", Some("Tells apart two components of one kind on one side: arm_L0, arm_L1"), index, 0..=9);
                let fallback = rig::guides::default_attach(*component, *side, *index).unwrap_or_default();
                f.text("Attach to", Some("The locator this component hangs from, by its full name (spine_C0_chest). Empty: the usual one, shown greyed"), attach, &fallback);
            });
            group(ui, "Joints", None, |f| match component {
                Component::Spine => {
                    f.drag_u32("Spine joints", Some("Joints from the pelvis to the chest, along the curve through the hips, mid and chest locators"), segments, 1..=9);
                }
                Component::Neck => {
                    f.drag_u32(
                        "Neck joints",
                        Some("Joints from the neck locator to the head"),
                        segments,
                        1..=9,
                    );
                }
                Component::Arm | Component::Leg => {
                    f.drag_u32(
                        "Twist joints",
                        Some("Per bone: along the upper arm and the forearm, or the thigh and the calf"),
                        twists,
                        0..=5,
                    );
                }
                Component::Hand => {
                    for fi in Finger::ALL {
                        let mut on = fingers.contains(&fi);
                        let label = match fi {
                            Finger::Thumb => "Thumb",
                            Finger::Index => "Index",
                            Finger::Middle => "Middle",
                            Finger::Ring => "Ring",
                            Finger::Pinky => "Pinky",
                        };
                        if f.toggle(label, None, &mut on).changed() {
                            if on {
                                fingers.push(fi);
                                fingers.sort_by_key(|x| Finger::ALL.iter().position(|y| y == x));
                            } else {
                                fingers.retain(|x| *x != fi);
                            }
                        }
                    }
                    f.toggle(
                        "Metacarpals",
                        Some("A joint in the palm for each finger but the thumb"),
                        metacarpals,
                    );
                }
            });
            group(ui, "Fit", Some("Autofit measures the mesh wired to the first input (standing on the floor, Y up, facing +Z, in an A or T pose) and places the locators on it. Without a mesh, or with autofit off, they go where a reference body has them. Either way, each locator can be moved after"), |f| {
                f.toggle("Autofit", None, autofit);
                if !r.mesh {
                    f.status(Status::Info, "No mesh wired: the reference body");
                } else if !*autofit {
                    f.status(Status::Info, "Autofit off: the reference body");
                }
            });
            let part = rig::node::part_info(
                *component,
                *side,
                *index,
                attach,
                *segments,
                *twists,
                fingers,
                *metacarpals,
            )
            .name();
            let placed = r
                .out_clip
                .as_deref()
                .map(|g| rig::node::locators_of(g, &part))
                .unwrap_or_default();
            group(ui, "Locators", Some("Where each locator is, and how far it was moved from where the fit put it, in centimetres. The skeleton follows when a locator moves"), |f| {
                if placed.is_empty() {
                    f.status(Status::Info, "Nothing yet");
                }
                for (name, at) in &placed {
                    let mut off = edits.iter().find(|e| &e.locator == name).map(|e| e.offset).unwrap_or([0.0; 3]).map(|v| v * 100.0);
                    let hint = format!("At {:.1}, {:.1}, {:.1} cm", at.x * 100.0, at.y * 100.0, at.z * 100.0);
                    if f.xyz(name, Some(&hint), &mut off, 0.1, " cm") {
                        rig::node::edit_to(edits, name, off.map(|v| v / 100.0));
                    }
                }
                if !edits.is_empty() {
                    f.buttons("", None, |ui| {
                        if ui.button("Back to the fit").on_hover_text("Every locator where the fit puts it").clicked() {
                            edits.clear();
                        }
                    });
                }
            });
        }
        NodeType::BuildSkeleton { settings } => {
            group(ui, "Target", Some("What the joints are called and how their axes lie. Choosing a target sets its axes, rotation order and root; each can be changed after"), |f| {
                let was = settings.preset;
                f.dropdown("Target", None, &mut settings.preset, &Preset::ALL.map(|p| (p, p.label())));
                if settings.preset != was {
                    let limits = settings.limits;
                    *settings = BuildSettings::of(settings.preset);
                    settings.limits = limits;
                }
            });
            group(ui, "Axes", Some("As the target lays them out, or one rule for every joint: the axis along the bone, the axis toward the pole (the way an elbow or a knee points), and the right side mirrored in behaviour (its two axes reversed)"), |f| {
                f.choice("Axes", None, &mut settings.custom_axes, &[(false, "As the target"), (true, "Custom")]);
                f.enabled(settings.custom_axes, |f| {
                    let axes = Axis::ALL.map(|a| (a, a.label()));
                    f.choice("Along the bone", None, &mut settings.aim, &axes);
                    f.choice("Toward the pole", None, &mut settings.up, &axes);
                    f.toggle("Mirror the right", None, &mut settings.mirror);
                });
                if settings.aim.index() == settings.up.index() {
                    f.status(Status::Bad, "The two axes are the same: the joints keep their own frames");
                }
                f.dropdown("Rotation order", Some("Kept with each joint, and used by Range of Motion. Write FBX does not write it yet"), &mut settings.order, &RotOrder::ALL.map(|o| (o, o.label())));
                f.toggle("Root joint", Some("A joint on the floor under the pelvis"), &mut settings.root);
                f.toggle("Limits", Some("Each joint's range of motion, as a human's. Off, Range of Motion turns every joint a right angle either way"), &mut settings.limits);
            });
            group(ui, "Skeleton", None, |f| match (&r.clip, &r.out_clip) {
                (None, _) => f.status(Status::Info, "Connect guides from a Rig Guide"),
                (Some(g), None) => match rig::build::build(g, settings) {
                    Err(e) => f.status(Status::Bad, e),
                    Ok(_) => f.status(Status::Info, "Cooking"),
                },
                (_, Some(s)) => {
                    f.value("Joints", format!("{}", s.joints.len()));
                    let h = crate::core::human::Human::of(s);
                    f.value(
                        "Human",
                        format!("{} of {} parts found", h.found(), crate::core::human::SLOTS),
                    );
                    f.value(
                        "First",
                        s.joints
                            .iter()
                            .take(4)
                            .map(|j| j.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
            });
        }
        NodeType::HardBind => {
            group(ui, "Hard Bind", Some("Every point to one joint, weight one. Each bone claims the points close to it, then the regions grow along the surface until they meet, so a hand resting on a thigh stays the hand's. Envelope moves where they meet; Bake Weights makes them smooth"), |f| {
                weight_summary(f, r.out_prims.as_ref(), r.clip.as_ref());
                show_row(f, &joints_of(r));
            });
        }
        NodeType::Envelope { edits } => {
            let joints = joints_of(r);
            group(ui, "Envelope", Some("Where a joint's region meets the one before it (its parent's) is a plane across its bone at the joint: slide it along the bone, tilt it, or grow or shrink the region along the surface. The weights stay hard"), |f| {
                weight_summary(f, r.out_prims.as_ref(), r.clip.as_ref());
                show_row(f, &joints);
                f.row("Add", Some("Edit one more joint's region"), |ui| {
                    egui::ComboBox::from_id_salt("envelope_add").width(ui.available_width()).selected_text("Joint…").show_ui(ui, |ui| {
                        for j in &joints {
                            if !edits.iter().any(|e| &e.joint == j) && ui.selectable_label(false, j).clicked() {
                                edits.push(rig::bind::EnvelopeEdit { joint: j.clone(), shift: 0.0, tilt: [0.0; 2], grow: 0.0 });
                                rig::view::show(Some(j.clone()));
                            }
                        }
                    });
                });
            });
            let mut gone = None;
            for (k, e) in edits.iter_mut().enumerate() {
                let title = e.joint.clone();
                group(ui, &title, None, |f| {
                    f.slider(
                        "Slide",
                        Some("Along the bone, a fraction of its length: + toward its end"),
                        &mut e.shift,
                        -0.5..=0.5,
                        "",
                    );
                    f.slider(
                        "Tilt",
                        Some("About the joint's pole axis, degrees"),
                        &mut e.tilt[0],
                        -60.0..=60.0,
                        "°",
                    );
                    f.slider(
                        "Tilt across",
                        Some("About the joint's third axis, degrees"),
                        &mut e.tilt[1],
                        -60.0..=60.0,
                        "°",
                    );
                    let mut cm = e.grow * 100.0;
                    if f.slider(
                        "Grow",
                        Some("Along the surface, centimetres: + takes points from its neighbours, - gives them back"),
                        &mut cm,
                        -10.0..=10.0,
                        " cm",
                    )
                    .changed()
                    {
                        e.grow = cm / 100.0;
                    }
                    f.buttons("", None, |ui| {
                        if ui.button("Show").clicked() {
                            rig::view::show(Some(e.joint.clone()));
                        }
                        if ui.button("Remove").clicked() {
                            gone = Some(k);
                        }
                    });
                });
            }
            if let Some(k) = gone {
                edits.remove(k);
            }
        }
        NodeType::RangeOfMotion { settings } => {
            group(ui, "Range of Motion", Some("Every joint of the skeleton swept alone about each axis, to its limits both ways (both sides at once), then poses a body takes: arms up, a squat, a lunge, bending and twisting, a fist. Every angle is a share of the joint's limits"), |f| {
                f.toggle("Each joint", None, &mut settings.sweeps);
                f.toggle("Poses", None, &mut settings.poses);
                f.drag_u32("Frames", Some("From one key pose to the next"), &mut settings.frames, 2..=60);
                f.slider("Amount", Some("How far into the limits"), &mut settings.amount, 0.1..=1.0, "");
                match (&r.clip, r.out_clip.as_ref().and_then(rig::node::rom_report)) {
                    (None, _) => f.status(Status::Info, "Connect a skeleton from Build Skeleton"),
                    (Some(_), None) => f.status(Status::Bad, "The skeleton has no rig information: build it with Build Skeleton"),
                    (_, Some(rep)) => {
                        f.value("Frames", format!("{}", rep.frames));
                        f.value("Parts", format!("{}", rep.parts.len()));
                    }
                }
            });
            if let Some(rep) = r.out_clip.as_ref().and_then(rig::node::rom_report) {
                group(ui, "Parts", None, |f| {
                    for (name, a, b) in &rep.parts {
                        f.value(name, format!("{a} – {b}"));
                    }
                });
            }
        }
        NodeType::DeltaMush { settings } => {
            group(ui, "Delta Mush", Some("The geometry skinned by its weights to the clip at the playhead, smoothed, and its detail put back on the smoothed surface: the seams of a hard skin smoothed out, the shape kept. Inputs: the weighted geometry, the skeleton it was weighted to, and a clip of that skeleton (a Range of Motion)"), |f| {
                f.drag_u32("Smoothing", Some("Passes: more spreads the smoothing further"), &mut settings.iterations, 1..=200);
                f.slider("Step", Some("How far each pass moves a point toward its neighbours"), &mut settings.step, 0.05..=1.0, "");
                f.value("Frame", format!("{}", r.frame.round()));
                show_row(f, &joints_of(r));
            });
        }
        NodeType::BakeWeights { settings, samples } => {
            group(ui, "Bake Weights", Some("The hard skin moved through the clip and delta-mushed, at frames spread over it; then for each point the blend of up to four of the joints around it that follows the mushed skin best over all those frames. Inputs as Delta Mush's; the smoothing should match it"), |f| {
                f.drag_u32("Smoothing", Some("Passes, as Delta Mush's"), &mut settings.iterations, 1..=200);
                f.slider("Step", None, &mut settings.step, 0.05..=1.0, "");
                f.drag_u32("Frames used", Some("Frames of the clip fitted to, spread evenly over it"), samples, 1..=2000);
                match r.out_prims.as_deref().and_then(rig::node::bake_report) {
                    Some(rep) => {
                        f.value("Fitted", format!("{} points over {} frames", rep.points, rep.frames));
                        f.value("Off the mush", format!("{:.2} mm mean, {:.1} mm most{}", rep.mean * 1000.0, rep.max * 1000.0, rep.worst.as_deref().map(|w| format!(" ({w})")).unwrap_or_default()));
                        f.value("Hard skin was", format!("{:.2} mm mean, {:.1} mm most", rep.hard_mean * 1000.0, rep.hard_max * 1000.0));
                    }
                    None => f.status(Status::Info, "Connect the hard-bound geometry, the skeleton and a Range of Motion"),
                }
                weight_summary(f, r.out_prims.as_ref(), r.clip.as_ref());
                show_row(f, &joints_of(r));
            });
        }
        _ => {}
    }
}
