//! The rigging templates: a human and its parts on the example character, the
//! DeLorean and the mini car kit's tractor on their own models, and machines,
//! a quadruped and a tentacle on shapes Rig Proxy makes for them.

use glam::Vec3;

use super::{examples, finish, missing_examples, p};
use crate::core::rig::guides::{reference, GuideEdit};
use crate::core::rig::node::{guide, part_of};
use crate::core::rig::presets::{self, Preset};
use crate::core::rig::rigid::RigidRow;
use crate::core::rig::{Component, MechParams, Role, Side};
use crate::ice::IceGraphStore;
use crate::node_graph::NodeGraphState;
use crate::types::{NodeId, NodeType};

/// A Rig Guide set up for a template.
pub(super) struct Part {
    name: String,
    t: NodeType,
}

/// A part with its locators where `at` puts them (the others where the
/// reference has them): the edits are how far each is from the reference.
fn placed(
    name: impl Into<String>,
    c: Component,
    side: Side,
    index: u32,
    label: &str,
    attach: &str,
    at: &[(&str, [f32; 3])],
    set: impl FnOnce(&mut MechParams, &mut u32),
) -> Part {
    let mut t = guide(c, side);
    if let NodeType::RigGuide {
        index: i,
        label: l,
        attach: a,
        autofit,
        mech,
        segments,
        ..
    } = &mut t
    {
        *i = index;
        *l = label.into();
        *a = attach.into();
        *autofit = false;
        set(mech, segments);
    }
    let part = part_of(&t).expect("a guide");
    let base = reference(&part);
    if let NodeType::RigGuide { edits, .. } = &mut t {
        for (n, q) in at {
            let r = base.get(*n).copied().unwrap_or(Vec3::ZERO);
            edits.push(GuideEdit {
                locator: (*n).into(),
                offset: (Vec3::from_array(*q) - r).to_array(),
            });
        }
    }
    Part { name: name.into(), t }
}

/// A part fitted to the primitives `fit` names.
fn fitted(
    name: &str,
    c: Component,
    side: Side,
    index: u32,
    label: &str,
    fit: &str,
    set: impl FnOnce(&mut MechParams),
) -> Part {
    let mut t = guide(c, side);
    if let NodeType::RigGuide {
        index: i,
        label: l,
        mech,
        ..
    } = &mut t
    {
        *i = index;
        *l = label.into();
        mech.fit = fit.into();
        set(mech);
    }
    Part { name: name.into(), t }
}

/// Mirrored to the right.
fn r(p: [f32; 3]) -> [f32; 3] {
    [-p[0], p[1], p[2]]
}

/// The guides chained, the mesh (if any) into each: the last, and all.
fn guides(g: &mut NodeGraphState, mesh: Option<(NodeId, usize)>, parts: &[Part]) -> (NodeId, Vec<NodeId>) {
    let mut ids = vec![];
    for (k, part) in parts.iter().enumerate() {
        let n = g.add_node(part.name.clone(), part.t.clone(), p(300.0, 20.0 + 70.0 * k as f32));
        if let Some((m, o)) = mesh {
            g.add_connection(m, o, n, 0);
        }
        if let Some(prev) = ids.last() {
            g.add_connection(*prev, 0, n, 1);
        }
        ids.push(n);
    }
    (*ids.last().expect("a guide"), ids)
}

/// The guides packed into a compound named `name`, once what they feed is
/// wired: the compound gives their last's output.
fn pack(g: &mut NodeGraphState, subnets: &mut IceGraphStore, ids: &[NodeId], name: &str) -> Option<NodeId> {
    let c = g.make_compound(subnets, ids, name).ok()?;
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == c) {
        n.name = name.replace(' ', "");
    }
    Some(c)
}

/// The scene's frames: the range of motion of these guides' skeleton (as
/// many frames wherever the locators are).
fn scene_frames(g: &mut NodeGraphState, parts: &[Part]) {
    let mut built: Option<crate::core::anim::AnimData> = None;
    for part in parts {
        let info = part_of(&part.t).expect("a guide");
        built = Some(crate::core::rig::guides::add_part(
            built.as_ref(),
            &info,
            None,
            None,
            &[],
            false,
            &[],
        ));
    }
    let frames = built
        .and_then(|b| crate::core::rig::build::build(&b, &Default::default()).ok())
        .and_then(|sk| crate::core::rig::rom::range_of_motion(&sk, &Default::default()))
        .map(|(rom, _)| rom.frames as i64)
        .unwrap_or(120);
    g.scene = crate::node_graph::SceneRange {
        start: 1,
        end: frames,
        fps_num: 24,
        fps_den: 1,
    };
}

/// Build Skeleton and Range of Motion after the guides.
fn skeleton_and_motion(g: &mut NodeGraphState, from: NodeId) -> (NodeId, NodeId) {
    let build = g.add_node(
        "Skeleton".into(),
        NodeType::BuildSkeleton {
            settings: Default::default(),
        },
        p(300.0, 620.0),
    );
    g.add_connection(from, 0, build, 0);
    let rom = g.add_node(
        "RangeOfMotion".into(),
        NodeType::RangeOfMotion {
            settings: Default::default(),
        },
        p(560.0, 720.0),
    );
    g.add_connection(build, 0, rom, 0);
    (build, rom)
}

/// Rigid Skin of `bound` on the skeleton, through the range of motion; viewed.
fn rigid_skin(g: &mut NodeGraphState, bound: NodeId, build: NodeId, rom: NodeId) -> NodeId {
    let skin = g.add_node("RigidSkin".into(), NodeType::RigidSkin, p(300.0, 960.0));
    g.add_connection(bound, 0, skin, 0);
    g.add_connection(build, 0, skin, 1);
    g.add_connection(rom, 0, skin, 2);
    skin
}

/// A rig of guides placed by hand, seen on Rig Proxy's shapes.
fn proxy_rig(g: &mut NodeGraphState, subnets: &mut IceGraphStore, parts: Vec<Part>, name: &str) -> NodeId {
    crate::graph_io::clear(g);
    let (last, ids) = guides(g, None, &parts);
    let (build, rom) = skeleton_and_motion(g, last);
    let proxy = g.add_node("RigProxy".into(), NodeType::RigProxy, p(40.0, 720.0));
    g.add_connection(build, 0, proxy, 0);
    let skin = rigid_skin(g, proxy, build, rom);
    scene_frames(g, &parts);
    let packed = pack(g, subnets, &ids, name);
    finish(g, skin, packed.unwrap_or(build));
    skin
}

// Humans

/// A human, or a part of one, rigged on the example character's mesh: the
/// guides chained (packed into a compound for a whole human), a skeleton for
/// Unreal, a hard bind, a range of motion, weights baked through a delta
/// mush, and the skin played through the range of motion.
fn human(
    g: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    parts: &[(&'static str, Component, Side)],
    name: Option<&str>,
) {
    crate::graph_io::clear(g);
    let who = g.add_node(
        "Character".into(),
        NodeType::LoadFbx {
            path: examples::path(examples::CHARACTER_FBX),
            take: 0,
        },
        p(40.0, 20.0),
    );
    let parts: Vec<Part> = parts
        .iter()
        .map(|(name, c, s)| Part {
            name: (*name).into(),
            t: guide(*c, *s),
        })
        .collect();
    let (last, ids) = guides(g, Some((who, 1)), &parts);
    let (build, rom) = skeleton_and_motion(g, last);
    let hard = g.add_node("HardBind".into(), NodeType::HardBind, p(40.0, 720.0));
    g.add_connection(who, 1, hard, 0);
    g.add_connection(build, 0, hard, 1);
    let mush = g.add_node(
        "DeltaMush".into(),
        NodeType::DeltaMush {
            settings: Default::default(),
        },
        p(40.0, 840.0),
    );
    let bake = g.add_node(
        "BakeWeights".into(),
        NodeType::BakeWeights {
            settings: Default::default(),
            samples: 120,
        },
        p(300.0, 840.0),
    );
    for n in [mush, bake] {
        g.add_connection(hard, 0, n, 0);
        g.add_connection(build, 0, n, 1);
        g.add_connection(rom, 0, n, 2);
    }
    let skin = g.add_node("Skin".into(), NodeType::Skin, p(300.0, 960.0));
    g.add_connection(bake, 0, skin, 0);
    g.add_connection(build, 0, skin, 1);
    g.add_connection(rom, 0, skin, 2);
    scene_frames(g, &parts);
    let packed = name.and_then(|n| pack(g, subnets, &ids, n));
    finish(g, skin, packed.unwrap_or(last));
}

pub(super) fn rig_human(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    human(
        g,
        subnets,
        &[
            ("Spine", Component::Spine, Side::Centre),
            ("Neck", Component::Neck, Side::Centre),
            ("ArmL", Component::Arm, Side::Left),
            ("ArmR", Component::Arm, Side::Right),
            ("HandL", Component::Hand, Side::Left),
            ("HandR", Component::Hand, Side::Right),
            ("LegL", Component::Leg, Side::Left),
            ("LegR", Component::Leg, Side::Right),
        ],
        Some("Full Human"),
    );
    format!("Rig: full human. The example character's mesh (its own skeleton is not used) given guides for a whole human, fitted by autofit: double-click Full Human to see the eight Rig Guides inside. Skeleton builds the joints for Unreal; HardBind gives every point to one joint; RangeOfMotion sweeps each joint and poses the body; BakeWeights fits smooth weights to the hard skin delta-mushed over it. Skin plays it all: press Space. View HardBind or BakeWeights to see the weights painted.{}", missing_examples())
}

pub(super) fn rig_human_leg(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    human(
        g,
        subnets,
        &[
            ("Spine", Component::Spine, Side::Centre),
            ("LegL", Component::Leg, Side::Left),
        ],
        None,
    );
    format!("Rig: human leg. A spine and the left leg on the example character: thigh, calf, foot, ball, two twist joints per bone, the knee's pole and the heel kept for a control rig. The rest of the body goes to the spine. Skin plays the range of motion with baked weights: press Space.{}", missing_examples())
}

pub(super) fn rig_human_arm(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    human(
        g,
        subnets,
        &[
            ("Spine", Component::Spine, Side::Centre),
            ("ArmL", Component::Arm, Side::Left),
            ("HandL", Component::Hand, Side::Left),
        ],
        None,
    );
    format!("Rig: human arm and hand. A spine, the left arm (clavicle, upper arm, forearm, hand, two twist joints per bone) and the left hand (five fingers, metacarpals) on the example character. Skin plays the range of motion with baked weights, the fingers' sweeps and the fist among it: press Space.{}", missing_examples())
}

pub(super) fn rig_human_spine(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    human(
        g,
        subnets,
        &[
            ("Spine", Component::Spine, Side::Centre),
            ("Neck", Component::Neck, Side::Centre),
        ],
        None,
    );
    format!("Rig: spine and neck. Five spine joints along the curve through the hips, mid and chest locators, two neck joints and the head, on the example character. The arms and legs go with the spine. Skin plays the bending, leaning and twisting: press Space.{}", missing_examples())
}

// Vehicles on their own models

/// The joint of `role` of a part, on the Unreal target.
fn joint(role: Role, side: Side, part: &str) -> String {
    presets::name(Preset::Unreal, role, side, part)
}

/// A vehicle on its model: its parts fitted to the model's primitives, a
/// skeleton, a rigid bind by rows, a range of motion, the model moved by it
/// with its materials.
fn vehicle(
    g: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    file: &str,
    parts: Vec<Part>,
    rows: Vec<RigidRow>,
    name: &str,
) {
    crate::graph_io::clear(g);
    let model = g.add_node("Model".into(), NodeType::load_usd(examples::path(file)), p(40.0, 20.0));
    let (last, ids) = guides(g, Some((model, 0)), &parts);
    let (build, rom) = skeleton_and_motion(g, last);
    let bind = g.add_node("RigidBind".into(), NodeType::RigidBind { rows }, p(40.0, 720.0));
    g.add_connection(model, 0, bind, 0);
    g.add_connection(build, 0, bind, 1);
    let skin = rigid_skin(g, bind, build, rom);
    scene_frames(g, &parts);
    let packed = pack(g, subnets, &ids, name);
    finish(g, skin, packed.unwrap_or(build));
}

fn row(prims: &str, joint: &str) -> RigidRow {
    RigidRow {
        prims: prims.into(),
        joint: joint.into(),
    }
}

pub(super) fn rig_car(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{Body, Wheel};
    use Side::{Centre as C, Left as L, Right as R};
    // The model is in centimetres: 8 cm of suspension travel.
    let wheel = |name, side, index, fit, steer: bool| {
        fitted(name, Wheel, side, index, "", fit, move |m| {
            m.travel = 8.0;
            m.steer = steer;
            m.limits = [-32.0, 32.0];
        })
    };
    let parts = vec![
        fitted("Body", Body, C, 0, "", "Paint_Geo", |_| {}),
        wheel("WheelFL", L, 0, "Wheel_Front_L", true),
        wheel("WheelFR", R, 0, "Wheel_Front_R", true),
        wheel("WheelRL", L, 1, "Wheel_Rear_L", false),
        wheel("WheelRR", R, 1, "Wheel_Rear_R", false),
    ];
    let rows = vec![
        // The callipers steer and ride the suspension, but do not spin.
        row("Calliper_Front_L", &joint(Role::Steer, L, "wheel_L0")),
        row("Calliper_Front_R", &joint(Role::Steer, R, "wheel_R0")),
        row("Calliper_Rear_L", &joint(Role::Suspension, L, "wheel_L1")),
        row("Calliper_Rear_R", &joint(Role::Suspension, R, "wheel_R1")),
        row("Wheel_Front_L", "wheel_L0"),
        row("Wheel_Front_R", "wheel_R0"),
        row("Wheel_Rear_L", "wheel_L1"),
        row("Wheel_Rear_R", "wheel_R1"),
        // Everything else is the body.
        row(".", "body_C0"),
    ];
    vehicle(g, subnets, examples::DELOREAN_USD, parts, rows, "Car");
    format!("Rig: car. The DeLorean given a body and four wheels, each fitted to the model's own primitives (the body to its paint, each wheel to its wheel): the fronts steer, all four have suspension. RigidBind gives each of the model's 43 primitives to a joint (the callipers steer but do not spin); RigidSkin moves them, materials and all, through the range of motion: each wheel's suspension, steering, a drive forward and back, and driving while steering. Press Space; Textured shows the paint. {}{}", super::BOXES, missing_examples())
}

pub(super) fn rig_tractor(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{Body, Hinge, Wheel};
    use Side::{Centre as C, Left as L, Right as R};
    let wheel = |name, side, index, fit, steer: bool| {
        fitted(name, Wheel, side, index, "", fit, move |m| {
            m.travel = 6.0;
            m.steer = steer;
        })
    };
    let parts = vec![
        fitted("Body", Body, C, 0, "", "geo/tractor$", |_| {}),
        wheel("WheelFL", L, 0, "wheel1/", true),
        wheel("WheelFR", R, 0, "wheel4/", true),
        wheel("WheelRL", L, 1, "wheel2/", false),
        wheel("WheelRR", R, 1, "wheel3/", false),
        fitted("Shovel", Hinge, C, 0, "shovel", "tractorShovel", |m| {
            m.limits = [-35.0, 40.0]
        }),
    ];
    let rows = vec![
        row("wheel1/", "wheel_L0"),
        row("wheel4/", "wheel_R0"),
        row("wheel2/", "wheel_L1"),
        row("wheel3/", "wheel_R1"),
        row("tractorShovel", "shovel_C0"),
        row(".", "body_C0"),
    ];
    vehicle(
        g,
        subnets,
        "usd/mini_car_kit/assets/vehicles/tractor/asset/tractorFullAsset.usda",
        parts,
        rows,
        "Tractor",
    );
    format!("Rig: tractor. The mini car kit's tractor (USD Working Group assets, CC0): a body, four wheels fitted to the kit's wheels (the fronts steer), and its shovel on a hinge fitted along the shovel's back edge. The rear wheels are larger: driving turns each by its own radius. Press Space.{}", missing_examples())
}

// Machines on Rig Proxy's shapes

pub(super) fn rig_aeroplane(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{Body, Hinge, Rotor, Wheel};
    use Side::{Centre as C, Left as L, Right as R};
    let fixed = |m: &mut MechParams, _: &mut u32| m.limits = [0.0, 0.0];
    let mut parts = vec![placed(
        "Fuselage",
        Body,
        C,
        0,
        "fuselage",
        "",
        &[
            ("centre", [0.0, 1.5, 0.0]),
            ("front", [0.0, 1.5, 4.0]),
            ("side", [0.6, 1.5, 0.0]),
            ("top", [0.0, 2.1, 0.0]),
        ],
        |_, _| {},
    )];
    for (side, f) in [
        (L, (|p: [f32; 3]| p) as fn([f32; 3]) -> [f32; 3]),
        (R, r as fn([f32; 3]) -> [f32; 3]),
    ] {
        let s = if side == L { "L" } else { "R" };
        parts.push(placed(
            format!("Wing{s}"),
            Hinge,
            side,
            0,
            "wing",
            "fuselage_C0_centre",
            &[
                ("pivot", f([0.6, 1.6, -0.6])),
                ("axis", f([0.6, 1.6, 0.8])),
                ("tip", f([5.2, 1.6, -0.6])),
            ],
            fixed,
        ));
        parts.push(placed(
            format!("Aileron{s}"),
            Hinge,
            side,
            0,
            "aileron",
            "fuselage_C0_centre",
            &[
                ("pivot", f([3.2, 1.6, -0.6])),
                ("axis", f([5.0, 1.6, -0.6])),
                ("tip", f([3.2, 1.6, -1.1])),
            ],
            |m, _| m.limits = [-25.0, 25.0],
        ));
        parts.push(placed(
            format!("Flap{s}"),
            Hinge,
            side,
            0,
            "flap",
            "fuselage_C0_centre",
            &[
                ("pivot", f([0.8, 1.6, -0.6])),
                ("axis", f([3.0, 1.6, -0.6])),
                ("tip", f([0.8, 1.6, -1.1])),
            ],
            |m, _| m.limits = [0.0, 40.0],
        ));
        parts.push(placed(
            format!("Stabiliser{s}"),
            Hinge,
            side,
            0,
            "stabiliser",
            "fuselage_C0_centre",
            &[
                ("pivot", f([0.3, 1.8, -3.5])),
                ("axis", f([0.3, 1.8, -2.9])),
                ("tip", f([2.0, 1.8, -3.5])),
            ],
            fixed,
        ));
        parts.push(placed(
            format!("Elevator{s}"),
            Hinge,
            side,
            0,
            "elevator",
            "fuselage_C0_centre",
            &[
                ("pivot", f([0.3, 1.8, -3.5])),
                ("axis", f([2.0, 1.8, -3.5])),
                ("tip", f([0.3, 1.8, -3.9])),
            ],
            |m, _| m.limits = [-30.0, 30.0],
        ));
        parts.push(placed(
            format!("Gear{s}"),
            Hinge,
            side,
            0,
            "gear",
            "fuselage_C0_centre",
            &[
                ("pivot", f([1.2, 1.2, 0.2])),
                ("axis", f([1.2, 1.2, 0.5])),
                ("tip", f([1.2, 0.45, 0.2])),
            ],
            |m, _| m.limits = [0.0, 85.0],
        ));
        parts.push(placed(
            format!("GearWheel{s}"),
            Wheel,
            side,
            0,
            "",
            &format!("gear_{s}0_tip"),
            &[
                ("centre", f([1.2, 0.35, 0.2])),
                ("axle", f([1.35, 0.35, 0.2])),
                ("top", f([1.2, 0.7, 0.2])),
            ],
            |m, _| m.travel = 0.0,
        ));
    }
    parts.push(placed(
        "Fin",
        Hinge,
        C,
        0,
        "fin",
        "fuselage_C0_centre",
        &[
            ("pivot", [0.0, 2.1, -3.5]),
            ("axis", [0.0, 2.1, -2.8]),
            ("tip", [0.0, 3.4, -3.5]),
        ],
        fixed,
    ));
    parts.push(placed(
        "Rudder",
        Hinge,
        C,
        0,
        "rudder",
        "fuselage_C0_centre",
        &[
            ("pivot", [0.0, 2.1, -3.5]),
            ("axis", [0.0, 3.4, -3.5]),
            ("tip", [0.0, 2.1, -4.0]),
        ],
        |m, _| m.limits = [-30.0, 30.0],
    ));
    parts.push(placed(
        "Propeller",
        Rotor,
        C,
        0,
        "propeller",
        "fuselage_C0_centre",
        &[
            ("centre", [0.0, 1.5, 4.1]),
            ("axis", [0.0, 1.5, 4.6]),
            ("tip", [0.0, 2.6, 4.1]),
        ],
        |m, _| {
            m.count = 3;
            m.limits = [-10.0, 30.0];
        },
    ));
    proxy_rig(g, subnets, parts, "Aeroplane");
    "Rig: aeroplane. A fuselage, wings, stabilisers and a fin (hinges that do not turn), ailerons, flaps, elevators and a rudder within their limits, a three-bladed propeller whose blades pitch, and landing gear that folds up with its wheels. Rig Proxy makes a shape for each part; RigidSkin moves them through the range of motion. Press Space.".into()
}

pub(super) fn rig_helicopter(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{Body, Hinge, Rotor};
    use Side::{Centre as C, Left as L, Right as R};
    let fixed = |m: &mut MechParams, _: &mut u32| m.limits = [0.0, 0.0];
    let parts = vec![
        placed(
            "Cabin",
            Body,
            C,
            0,
            "cabin",
            "",
            &[
                ("centre", [0.0, 1.4, 0.4]),
                ("front", [0.0, 1.4, 2.3]),
                ("side", [0.85, 1.4, 0.4]),
                ("top", [0.0, 2.2, 0.4]),
            ],
            |_, _| {},
        ),
        placed(
            "Boom",
            Hinge,
            C,
            0,
            "boom",
            "cabin_C0_centre",
            &[
                ("pivot", [-0.15, 1.8, -1.4]),
                ("axis", [0.15, 1.8, -1.4]),
                ("tip", [-0.15, 1.8, -6.0]),
            ],
            fixed,
        ),
        placed(
            "SkidL",
            Hinge,
            L,
            0,
            "skid",
            "cabin_C0_centre",
            &[
                ("pivot", [0.8, 0.25, 1.6]),
                ("axis", [0.95, 0.25, 1.6]),
                ("tip", [0.8, 0.25, -1.6]),
            ],
            fixed,
        ),
        placed(
            "SkidR",
            Hinge,
            R,
            0,
            "skid",
            "cabin_C0_centre",
            &[
                ("pivot", r([0.8, 0.25, 1.6])),
                ("axis", r([0.95, 0.25, 1.6])),
                ("tip", r([0.8, 0.25, -1.6])),
            ],
            fixed,
        ),
        placed(
            "MainRotor",
            Rotor,
            C,
            0,
            "mainRotor",
            "cabin_C0_centre",
            &[
                ("centre", [0.0, 2.6, 0.2]),
                ("axis", [0.0, 3.1, 0.2]),
                ("tip", [0.0, 2.6, 5.2]),
            ],
            |m, _| {
                m.count = 4;
                m.limits = [-5.0, 15.0];
            },
        ),
        placed(
            "TailRotor",
            Rotor,
            C,
            0,
            "tailRotor",
            "boom_C0_tip",
            &[
                ("centre", [0.3, 2.0, -6.0]),
                ("axis", [0.8, 2.0, -6.0]),
                ("tip", [0.3, 2.9, -6.0]),
            ],
            |m, _| {
                m.count = 2;
                m.limits = [-15.0, 15.0];
            },
        ),
    ];
    proxy_rig(g, subnets, parts, "Helicopter");
    "Rig: helicopter. A cabin, a tail boom and skids, a four-bladed main rotor whose blades pitch (collective) and a two-bladed tail rotor. Each rotor spins a turn each way in the range of motion, then its blades pitch. Press Space.".into()
}

pub(super) fn rig_tank(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{Body, Hinge, Rotor, Tread, Wheel};
    use Side::{Centre as C, Left as L, Right as R};
    let mut parts = vec![
        placed(
            "Hull",
            Body,
            C,
            0,
            "hull",
            "",
            &[
                ("centre", [0.0, 1.05, 0.0]),
                ("front", [0.0, 1.05, 3.1]),
                ("side", [1.0, 1.05, 0.0]),
                ("top", [0.0, 1.55, 0.0]),
            ],
            |_, _| {},
        ),
        placed(
            "Turret",
            Rotor,
            C,
            0,
            "turret",
            "hull_C0_centre",
            &[
                ("centre", [0.0, 1.85, -0.2]),
                ("axis", [0.0, 2.3, -0.2]),
                ("tip", [0.0, 1.85, 1.0]),
            ],
            |m, _| m.count = 0,
        ),
        placed(
            "Gun",
            Hinge,
            C,
            0,
            "gun",
            "turret_C0_centre",
            &[
                ("pivot", [-0.12, 2.05, 0.9]),
                ("axis", [0.12, 2.05, 0.9]),
                ("tip", [-0.12, 2.05, 4.6]),
            ],
            |m, _| m.limits = [-20.0, 8.0],
        ),
    ];
    for side in [L, R] {
        let f = |p: [f32; 3]| if side == L { p } else { r(p) };
        let s = if side == L { "L" } else { "R" };
        parts.push(placed(
            format!("Tread{s}"),
            Tread,
            side,
            0,
            "",
            "hull_C0_centre",
            &[
                ("front", f([1.35, 0.45, 2.6])),
                ("rear", f([1.35, 0.45, -2.6])),
                ("top", f([1.35, 0.9, 2.6])),
            ],
            |m, _| m.count = 64,
        ));
        for k in 0..5u32 {
            let z = 2.6 - 1.3 * k as f32;
            parts.push(placed(
                format!("Wheel{s}{k}"),
                Wheel,
                side,
                k,
                "",
                "hull_C0_centre",
                &[
                    ("centre", f([1.35, 0.45, z])),
                    ("axle", f([1.6, 0.45, z])),
                    ("top", f([1.35, 0.85, z])),
                ],
                |m, _| m.travel = 0.0,
            ));
        }
    }
    proxy_rig(g, subnets, parts, "Tank");
    "Rig: tank. A hull, a turret that turns without end, a gun that elevates within its limits, five road wheels a side, and a tread of 64 links round each side's wheels: in the drive, the wheels turn and the links run round their loops by the same distance. Press Space.".into()
}

pub(super) fn rig_machine(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{Body, Gear, Piston};
    use Side::Centre as C;
    let parts = vec![
        placed(
            "Frame",
            Body,
            C,
            0,
            "frame",
            "",
            &[
                ("centre", [0.0, 1.0, -0.25]),
                ("front", [0.0, 1.0, -0.2]),
                ("side", [1.4, 1.0, -0.25]),
                ("top", [0.0, 1.9, -0.25]),
            ],
            |_, _| {},
        ),
        placed(
            "GearA",
            Gear,
            C,
            0,
            "",
            "frame_C0_centre",
            &[
                ("centre", [-0.6, 1.25, 0.0]),
                ("axis", [-0.6, 1.25, 0.3]),
                ("tip", [-0.6, 1.75, 0.0]),
            ],
            |_, _| {},
        ),
        placed(
            "GearB",
            Gear,
            C,
            1,
            "",
            "gear_C0_centre",
            &[
                ("centre", [0.15, 1.25, 0.0]),
                ("axis", [0.15, 1.25, 0.3]),
                ("tip", [0.15, 1.5, 0.0]),
            ],
            |_, _| {},
        ),
        placed(
            "GearC",
            Gear,
            C,
            2,
            "",
            "gear_C1_centre",
            &[
                ("centre", [0.15, 0.65, 0.0]),
                ("axis", [0.15, 0.65, 0.3]),
                ("tip", [0.5, 0.65, 0.0]),
            ],
            |_, _| {},
        ),
        placed(
            "Piston",
            Piston,
            C,
            0,
            "",
            "frame_C0_centre",
            &[("base", [1.0, 0.3, 0.0]), ("end", [1.0, 1.4, 0.0])],
            |m, _| m.travel = 0.3,
        ),
    ];
    proxy_rig(g, subnets, parts, "Machine");
    "Rig: gears and a piston. Three gears on a frame, each on the one before it: the first spins a turn each way, the second meshes with it and turns the other way twice as fast (half the radius), the third meshes with the second. A piston's rod slides through its stroke. Press Space.".into()
}

pub(super) fn rig_robot_arm(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{Body, Hinge, Rotor};
    use Side::{Centre as C, Left as L, Right as R};
    let parts = vec![
        placed(
            "Base",
            Body,
            C,
            0,
            "base",
            "",
            &[
                ("centre", [0.0, 0.15, 0.0]),
                ("front", [0.0, 0.15, 0.4]),
                ("side", [0.4, 0.15, 0.0]),
                ("top", [0.0, 0.3, 0.0]),
            ],
            |_, _| {},
        ),
        placed(
            "Turntable",
            Rotor,
            C,
            0,
            "turntable",
            "base_C0_centre",
            &[
                ("centre", [0.0, 0.35, 0.0]),
                ("axis", [0.0, 0.6, 0.0]),
                ("tip", [0.0, 0.35, 0.3]),
            ],
            |m, _| m.count = 0,
        ),
        placed(
            "Shoulder",
            Hinge,
            C,
            0,
            "shoulder",
            "turntable_C0_centre",
            &[
                ("pivot", [-0.12, 0.55, 0.0]),
                ("axis", [0.12, 0.55, 0.0]),
                ("tip", [-0.12, 1.35, 0.0]),
            ],
            |m, _| m.limits = [-90.0, 90.0],
        ),
        placed(
            "Elbow",
            Hinge,
            C,
            0,
            "elbow",
            "shoulder_C0_tip",
            &[
                ("pivot", [-0.1, 1.35, 0.0]),
                ("axis", [0.1, 1.35, 0.0]),
                ("tip", [-0.1, 1.35, 0.7]),
            ],
            |m, _| m.limits = [-135.0, 135.0],
        ),
        placed(
            "Wrist",
            Hinge,
            C,
            0,
            "wrist",
            "elbow_C0_tip",
            &[
                ("pivot", [-0.07, 1.35, 0.7]),
                ("axis", [0.07, 1.35, 0.7]),
                ("tip", [-0.07, 1.35, 0.88]),
            ],
            |m, _| m.limits = [-120.0, 120.0],
        ),
        placed(
            "Flange",
            Rotor,
            C,
            0,
            "flange",
            "wrist_C0_tip",
            &[
                ("centre", [0.0, 1.35, 0.9]),
                ("axis", [0.0, 1.35, 1.0]),
                ("tip", [0.0, 1.43, 0.9]),
            ],
            |m, _| m.count = 0,
        ),
        placed(
            "FingerL",
            Hinge,
            L,
            0,
            "finger",
            "flange_C0_centre",
            &[
                ("pivot", [0.05, 1.33, 0.92]),
                ("axis", [0.05, 1.37, 0.92]),
                ("tip", [0.05, 1.33, 1.08]),
            ],
            |m, _| m.limits = [-30.0, 0.0],
        ),
        placed(
            "FingerR",
            Hinge,
            R,
            0,
            "finger",
            "flange_C0_centre",
            &[
                ("pivot", r([0.05, 1.33, 0.92])),
                ("axis", r([0.05, 1.37, 0.92])),
                ("tip", r([0.05, 1.33, 1.08])),
            ],
            |m, _| m.limits = [-30.0, 0.0],
        ),
    ];
    proxy_rig(g, subnets, parts, "Robot Arm");
    "Rig: robot arm. Six axes: a turntable, shoulder, elbow and wrist hinges within their limits, a flange that turns without end, and a two-finger gripper. Press Space.".into()
}

pub(super) fn rig_quadruped(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    use Component::{AnimalLeg, Chain};
    use Side::{Centre as C, Left as L, Right as R};
    let chain = |n: u32, limits: [f32; 2]| {
        move |m: &mut MechParams, seg: &mut u32| {
            *seg = n;
            m.limits = limits;
        }
    };
    let leg = |p: [[f32; 3]; 6]| -> Vec<(&'static str, [f32; 3])> {
        ["hip", "knee", "hock", "fetlock", "hoof", "toe"]
            .into_iter()
            .zip(p)
            .collect()
    };
    let hind = [
        [0.25, 1.25, -0.65],
        [0.27, 0.85, -0.45],
        [0.27, 0.5, -0.75],
        [0.27, 0.15, -0.7],
        [0.27, 0.05, -0.62],
        [0.27, 0.0, -0.55],
    ];
    let fore = [
        [0.22, 1.2, 0.55],
        [0.24, 0.85, 0.42],
        [0.24, 0.5, 0.5],
        [0.24, 0.15, 0.5],
        [0.24, 0.05, 0.58],
        [0.24, 0.0, 0.65],
    ];
    let mirror = |p: [[f32; 3]; 6]| p.map(r);
    let parts = vec![
        placed(
            "Spine",
            Chain,
            C,
            0,
            "spine",
            "",
            &[
                ("start", [0.0, 1.3, -0.7]),
                ("mid", [0.0, 1.38, 0.0]),
                ("end", [0.0, 1.35, 0.65]),
                ("pole", [0.0, 2.0, 0.0]),
            ],
            chain(5, [-15.0, 15.0]),
        ),
        placed(
            "Neck",
            Chain,
            C,
            0,
            "neck",
            "spine_C0_end",
            &[
                ("start", [0.0, 1.4, 0.7]),
                ("mid", [0.0, 1.75, 0.95]),
                ("end", [0.0, 2.0, 1.1]),
                ("pole", [0.0, 1.7, 1.5]),
            ],
            chain(4, [-25.0, 25.0]),
        ),
        placed(
            "Head",
            Chain,
            C,
            0,
            "head",
            "neck_C0_end",
            &[
                ("start", [0.0, 2.0, 1.1]),
                ("mid", [0.0, 1.88, 1.35]),
                ("end", [0.0, 1.72, 1.6]),
                ("pole", [0.0, 2.4, 1.4]),
            ],
            chain(1, [-30.0, 30.0]),
        ),
        placed(
            "Tail",
            Chain,
            C,
            0,
            "tail",
            "spine_C0_start",
            &[
                ("start", [0.0, 1.35, -0.8]),
                ("mid", [0.0, 1.1, -1.0]),
                ("end", [0.0, 0.7, -1.1]),
                ("pole", [0.0, 1.0, -1.4]),
            ],
            chain(8, [-30.0, 30.0]),
        ),
        placed(
            "HindL",
            AnimalLeg,
            L,
            0,
            "hindLeg",
            "spine_C0_start",
            &leg(hind),
            |_, _| {},
        ),
        placed(
            "HindR",
            AnimalLeg,
            R,
            0,
            "hindLeg",
            "spine_C0_start",
            &leg(mirror(hind)),
            |_, _| {},
        ),
        placed(
            "ForeL",
            AnimalLeg,
            L,
            0,
            "foreLeg",
            "spine_C0_end",
            &leg(fore),
            |_, _| {},
        ),
        placed(
            "ForeR",
            AnimalLeg,
            R,
            0,
            "foreLeg",
            "spine_C0_end",
            &leg(mirror(fore)),
            |_, _| {},
        ),
    ];
    proxy_rig(g, subnets, parts, "Quadruped");
    "Rig: quadruped. A horse-sized animal: a spine, a neck, a head and a tail as chains, and four legs of five joints (hip, stifle, hock, fetlock, hoof; at the front shoulder, elbow, knee, fetlock, hoof), each bending the way its apex points. Rig Proxy gives each bone a box. Press Space.".into()
}

pub(super) fn rig_tentacle(g: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
    let parts = vec![placed(
        "Tentacle",
        Component::Chain,
        Side::Centre,
        0,
        "tentacle",
        "",
        &[
            ("start", [0.0, 2.0, 0.0]),
            ("mid", [0.0, 1.2, 0.5]),
            ("end", [0.0, 0.3, 0.3]),
            ("pole", [0.0, 1.4, 1.5]),
        ],
        |m, seg| {
            *seg = 16;
            m.limits = [-25.0, 25.0];
        },
    )];
    proxy_rig(g, subnets, parts, "Tentacle");
    "Rig: tentacle. A chain of sixteen joints along a curve: a tail, a tentacle, a trunk, a finger alone. It sweeps, then curls each way. Press Space.".into()
}
