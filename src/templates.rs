//! Ready-made graphs, one for each area of the program. Picking one from
//! the Templates menu replaces the current graph.

use glam::Vec3;

use crate::core::poly::{ExtrudeMode, PolyMesh, PolyOp, PolyOpKind, PolySelection, SelSource, SubLevel};
use crate::examples;
use crate::ice::IceGraphStore;
use crate::node_graph::NodeGraphState;
use crate::types::{MergeMode, NodeId, NodeType, RetimeMode, IceGraphNodeType, DEFAULT_WRITE_PATH};

pub struct Template {
    /// Whose work the template shows: the first level of the Templates menu.
    pub by:    &'static str,
    pub group: &'static str,
    pub name:  &'static str,
    pub hint:  &'static str,
    /// Builds the graph and returns a line telling the user what to look at.
    pub build: fn(&mut NodeGraphState, &mut IceGraphStore) -> String,
}

pub const MARTINO: &str = "Martino";
pub const SIMON:   &str = "Simon";
pub const XMS:     &str = "XMS";

/// The Templates menu: whose work, then the groups of each.
pub const MENU: [(&str, &[&str]); 3] = [
    (MARTINO, &["USD stages", "ICE"]),
    (SIMON,   &["Basics", "Modelling", "UV", "USD", "Animation & Mocap", "FBX and characters"]),
    (XMS,     &["Lighting & Rendering"]),
];

pub const TEMPLATES: &[Template] = &[
    Template { by: MARTINO, group: "USD stages", name: "Composition: a street", build: street,
        hint: "A stage composed from four files: a sublayer, references, a variant set and an override in the root layer" },
    Template { by: MARTINO, group: "USD stages", name: "Curves and points", build: strands,
        hint: "BasisCurves of every basis (linear, Bezier, Catmull-Rom, a periodic B-spline), 600 strands of grass and a spiral of points of growing width" },
    Template { by: MARTINO, group: "USD stages", name: "Point instancer: a forest", build: forest,
        hint: "4,900 pines and stones from one PointInstancer, drawn by GPU instancing. The Transform after it moves the placements, not the shared meshes" },
    Template { by: MARTINO, group: "USD stages", name: "Native instances: a hall of tables", build: hall,
        hint: "64 instanceable references to one table: five meshes, shared by every table" },
    Template { by: MARTINO, group: "USD stages", name: "Purposes: street lamps", build: lamps,
        hint: "Lamps with render, proxy and guide geometry. Switch them with the render, proxy and guide toggles of the Scene Explorer" },
    Template { by: MARTINO, group: "ICE", name: "ICE graph: spherify", build: spherify,
        hint: "A subdivided cube pushed onto a sphere by an ICE graph. Select the ICE node and press the down arrow to go inside, the up arrow to come back" },
    Template { by: SIMON, group: "Basics", name: "Shapes", build: primitives,
        hint: "Cube, sphere and grid, each moved with a Transform and joined with Merge" },
    Template { by: SIMON, group: "Basics", name: "USD import", build: usd,
        hint: "Two example USD files loaded, moved and merged" },
    Template { by: SIMON, group: "Modelling", name: "Edit Poly: tower", build: tower,
        hint: "Inset, extrude and bevel stacked on the top face of a cube" },
    Template { by: SIMON, group: "Modelling", name: "Edit Poly: panels", build: panels,
        hint: "Every cell of a grid inset and extruded on its own" },
    Template { by: SIMON, group: "Modelling", name: "Edit Poly: goblet", build: goblet,
        hint: "Edge loops, transforms and subdivision, with the early operations collapsed" },
    Template { by: SIMON, group: "Modelling", name: "Edit Poly: bridge", build: bridge,
        hint: "Two cubes joined with Bridge, the selection made by a box rule" },
    Template { by: SIMON, group: "Modelling", name: "Edit Poly: sea mine", build: sea_mine,
        hint: "Eleven operations with three rounds of subdivision: about ten thousand polygons from one cube" },
    Template { by: SIMON, group: "Modelling", name: "Edit Poly: bolt", build: bolt,
        hint: "Chamfer, slice, hinge, outline and vertex extrude: the second set of Edit Poly operations" },
    Template { by: SIMON, group: "UV", name: "Unwrap the sea mine", build: uv_mine,
        hint: "Conformal (LSCM) unwrap of ten thousand polygons. Open the UV Editor pane to see the charts" },
    Template { by: SIMON, group: "UV", name: "Edit UV islands", build: uv_edit,
        hint: "A box unwrapped, then islands moved, turned and scaled in a UV Edit node" },
    Template { by: SIMON, group: "UV", name: "UDIM tiles", build: uv_udim,
        hint: "A body and two hands made of separate pieces, unwrapped over three UDIM tiles: each hand keeps its fingers on its own tile" },
    Template { by: SIMON, group: "USD", name: "USD: heavy model", build: usd_heavy,
        hint: "A 460,000 triangle model as 43 primitives. Select the Load USD node for its materials and textures; open the UV Editor for its UVs" },
    Template { by: SIMON, group: "USD", name: "USD: pick and edit", build: usd_pick,
        hint: "One wheel picked out of the model and moved. Only the picked primitive goes through the Transform and Edit Poly nodes" },
    Template { by: SIMON, group: "USD", name: "Open assets: teapot and car kit", build: open_assets,
        hint: "Two CC0 models from the USD Working Group's assets: the PolyHaven teapot with its materials, and the mini car kit's vehicles, merged. Files that ship with the program" },
    Template { by: SIMON, group: "USD", name: "Open assets: carbon frame bike (download)", build: open_bike,
        hint: "A 135,648-triangle bike with 31 materials, CC BY-SA, fetched into the examples folder when the template opens, with its licence shown first" },
    Template { by: SIMON, group: "USD", name: "Open assets: elephant with monochord (download)", build: open_elephant,
        hint: "A small textured model from Songs of Cultures, CC BY-SA, fetched into the examples folder when the template opens" },
    Template { by: SIMON, group: "USD", name: "USD: prune", build: usd_prune,
        hint: "The model cut down to its wheels and callipers with a path pattern" },
    Template { by: MARTINO, group: "USD stages", name: "USD: set attribute", build: usd_set_attribute,
        hint: "Values set on prims: the wheels made invisible, the model's root made a component asset. Add a Write USD node to see them written" },
    Template { by: MARTINO, group: "USD stages", name: "Cameras", build: cameras,
        hint: "Three shapes and two cameras, one perspective, one orthographic from above. Choose one in the Viewer's camera menu to look through it" },
    Template { by: MARTINO, group: "USD stages", name: "Material bindings", build: material_bindings,
        hint: "Four shapes in one Merge, three materials bound to them by row under /root/materials, and a second Material Binding giving a fourth shape the textured ground material by Location. Write USD is viewed" },
    Template { by: MARTINO, group: "USD stages", name: "MaterialX: three shaders", build: materialx_shaders,
        hint: "Three spheres with MaterialX materials: a gold standard_surface, a red open_pbr_surface, and a standard_surface reading a texture through an image node. Double-click a MaterialX node to open its graph. Write USD is viewed" },
    Template { by: MARTINO, group: "USD stages", name: "USD: material binding", build: usd_material_binding,
        hint: "Two Material operators wired into a Material Binding: car paint on the whole model, chrome on the wheels (the later row wins). Textured mode shows them" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Keys: a bouncing ball", build: keys_bounce,
        hint: "A sphere keyed to bounce along the grid: open the Graph Editor to see its curves, press Play" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Local Animation and Merge Animation", build: local_animation,
        hint: "Two animations on frames of their own, merged as they are and lined up in time" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Clip basics", build: clip_basics,
        hint: "Test clip renamed, trimmed, retimed and given a start timecode. Select each node to see the timeline follow" },
    Template { by: SIMON, group: "Animation & Mocap", name: "FBX import", build: fbx_import,
        hint: "A real two-character mocap take loaded from FBX" },
    Template { by: SIMON, group: "Animation & Mocap", name: "T-pose and export", build: tpose,
        hint: "Auto T-pose, a manual fix, proxy skin and Write FBX" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Mocap tools", build: mocap_tools,
        hint: "The example take split, pruned, smoothed, held in place, mirrored and looped" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Retarget", build: retarget,
        hint: "A character of the example take driving the test skeleton, which rests in another pose" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Body Collide: into the car", build: body_collide,
        hint: "A captured actor walks through a car and sits in it for six minutes, kept out of the seat, the floor and himself. Opens solved" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Body Collide: obstacle course", build: body_collide_course,
        hint: "The first 20 seconds of the car take on a course of boxes, each one a case the solver handles: a kerb, a pole, a wall, a door walked through, a seat and a ledge. Opens solved" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Mocap split (example takes)", build: mocap_example,
        hint: "The mocap split graph, pointed at the example folder with a two-character take" },
    Template { by: SIMON, group: "Animation & Mocap", name: "Mocap split", build: mocap_blank,
        hint: "Folder of takes, split per character, animation and skinned T-pose written per character" },
    Template { by: SIMON, group: "FBX and characters", name: "Load FBX: what a file holds", build: fbx_studio,
        hint: "A small studio: two meshes with their materials and UVs, a camera that dollies in, a light. Select Load FBX to see them listed" },
    Template { by: SIMON, group: "FBX and characters", name: "Novasci clip onto the character", build: novasci_retarget,
        hint: "A CC0 mocap clip on the Mixamo skeleton (ComeHere1, 93 frames) retargeted onto the 342-joint character. The other clips of the pack are fetched when a template asks for one" },
    Template { by: SIMON, group: "FBX and characters", name: "Novasci clip: thumbs up (download)", build: novasci_download,
        hint: "A clip of the pack that is not in the repository: the program offers to fetch it, saying where from and under which licence" },
    Template { by: SIMON, group: "FBX and characters", name: "Retarget onto a character", build: retarget_character,
        hint: "The example take on a 342-joint character in the pose its skin was bound in, then Reach: feet where the actor's are, within human limits" },
    Template { by: SIMON, group: "FBX and characters", name: "Capture and Skin", build: capture_skin,
        hint: "The character's mesh weighted again by proximity to its bones, and its skin put on the retargeted take by joint name" },
    Template { by: SIMON, group: "FBX and characters", name: "Mocap folder: retarget, skin, collide, write", build: mocap_pipeline,
        hint: "A folder of raw takes, each actor placed and retargeted onto a skinned character, kept out of the set by Body Collide, and written as skinned FBX. Write FBX with All files solves and writes every take" },
    Template { by: SIMON, group: "FBX and characters", name: "Human Limits", build: human_limits,
        hint: "An elbow turned the wrong way by Fix Pose, put back by the Body Collide node's human pass" },
    Template { by: XMS, group: "Lighting & Rendering", name: "DeLorean: lit and rendered with Karma", build: delorean_render,
        hint: "The DeLorean under a dome (a sky HDR) and a blue rect light, a camera, RenderScene Settings, Karma's Render Settings and AOVs, Write USD and a husk Render Command" },
];

impl Template {
    /// Build the template, lay its graph out and bring it into view.
    pub fn load(&self, graph: &mut NodeGraphState, subnets: &mut IceGraphStore) -> String {
        let message = (self.build)(graph, subnets);
        let (selected, many) = (graph.selected_node, graph.selected_nodes.clone());
        graph.auto_layout();
        graph.selected_node = selected;
        graph.selected_nodes = many;
        message
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn p(x: f32, y: f32) -> egui::Pos2 { egui::pos2(x, y) }

/// View the last operator: its output is the scene. Select `focus`.
fn finish(graph: &mut NodeGraphState, last: NodeId, focus: NodeId) {
    // Merge and Material Binding keep an input free, as when wired by hand.
    let ids: Vec<NodeId> = graph.nodes.iter().map(|n| n.id).collect();
    for id in ids { graph.keep_an_input_free(id); }
    graph.view_flag      = Some(last);
    graph.view_output    = 0;
    graph.selected_node  = Some(focus);
    graph.selected_nodes = vec![focus];
}

/// View and select a clip operator.
fn view(graph: &mut NodeGraphState, node: NodeId) {
    finish(graph, node, node);
}

fn transform(t: [f32; 3], r: [f32; 3], s: [f32; 3]) -> NodeType {
    NodeType::Transform { translation: Vec3::from_array(t), rotation: Vec3::from_array(r), scale: Vec3::from_array(s) }
}

fn edit_poly(ops: Vec<PolyOp>, pending: PolySelection) -> NodeType {
    NodeType::EditPoly { ops, pending, edit: None, auto_collapse: false }
}

fn picked(polys: Vec<u32>) -> PolySelection { PolySelection::picked_polys(polys) }

fn missing_examples() -> &'static str {
    if examples::dir().is_some() { "" } else { " The examples folder was not found: put it next to the program." }
}

// ── Basics ───────────────────────────────────────────────────────────────────

fn primitives(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let cube   = g.add_node("Cube".into(), NodeType::cube(1.0), p(40.0, 20.0));
    let sphere = g.add_node("Sphere".into(), NodeType::sphere(0.6, 24), p(250.0, 20.0));
    let grid   = g.add_node("Grid".into(), NodeType::grid(8, 8, 5.0), p(460.0, 20.0));
    let cube_x = g.add_node("MoveCube".into(), transform([-1.2, 0.5, 0.0], [0.0, 30f32.to_radians(), 0.0], [1.0, 1.0, 1.0]), p(40.0, 120.0));
    let sph_x  = g.add_node("MoveSphere".into(), transform([1.2, 0.6, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]), p(250.0, 120.0));
    let m1     = g.add_node("Merge".into(), NodeType::merge(), p(145.0, 220.0));
    let m2     = g.add_node("MergeFloor".into(), NodeType::merge(), p(300.0, 320.0));
    g.add_connection(cube, 0, cube_x, 0);
    g.add_connection(sphere, 0, sph_x, 0);
    g.add_connection(cube_x, 0, m1, 0);
    g.add_connection(sph_x, 0, m1, 1);
    g.add_connection(m1, 0, m2, 0);
    g.add_connection(grid, 0, m2, 1);
    finish(g, m2, cube_x);
    "Shapes: select a Transform node and change its values in Parameters.".into()
}

/// A ball keyed to bounce: four keys of height, cycled; forward motion on
/// two keys, cycled with offset so it keeps going; a squash on contact.
fn keys_bounce(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::keys::{Infinity, Tangent};
    crate::graph_io::clear(g);
    g.scene = crate::node_graph::SceneRange { start: 1, end: 96, fps_num: 24, fps_den: 1 };
    let ball  = g.add_node("Ball".into(), NodeType::sphere(0.4, 24), p(40.0, 20.0));
    let grid  = g.add_node("Floor".into(), NodeType::grid(8, 8, 12.0), p(260.0, 20.0));
    let mv    = g.add_node("Bounce".into(), transform([-4.0, 0.4, 0.0], [0.0; 3], [1.0; 3]), p(40.0, 120.0));
    let merge = g.add_node("Merge".into(), NodeType::merge(), p(150.0, 220.0));
    g.add_connection(ball, 0, mv, 0);
    g.add_connection(mv, 0, merge, 0);
    g.add_connection(grid, 0, merge, 1);
    // Height: down on 1 and 25, up on 13. Sharp at the floor, eased at the top.
    for (f, v) in [(1.0, 0.4), (13.0, 2.6), (25.0, 0.4)] { g.set_key_value(mv, "translation/1", f, v); }
    // Forward: a metre a bounce, on and on.
    for (f, v) in [(1.0, -4.0), (25.0, -3.0)] { g.set_key_value(mv, "translation/0", f, v); }
    // Squash on contact: flat on 1 and 25, round by 4 and 22.
    for (f, v) in [(1.0, 0.7), (4.0, 1.0), (22.0, 1.0), (25.0, 0.7)] { g.set_key_value(mv, "scale/1", f, v); }
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == mv) {
        for c in &mut n.channels {
            match c.path.as_str() {
                "translation/1" => {
                    c.post = Infinity::Cycle;
                    for k in &mut c.keys {
                        if k.value < 1.0 { k.in_type = Tangent::Linear; k.out_type = Tangent::Linear; k.broken = true; }
                        else { k.in_type = Tangent::Flat; k.out_type = Tangent::Flat; }
                    }
                }
                "translation/0" => {
                    c.post = Infinity::CycleOffset;
                    for k in &mut c.keys { k.in_type = Tangent::Linear; k.out_type = Tangent::Linear; }
                }
                _ => c.post = Infinity::Cycle,
            }
        }
    }
    finish(g, merge, mv);
    "Keys: Bounce's translate X and Y and scale Y are keyed. Press Play, or open the Graph Editor (Window menu) with Bounce selected: Y is cycled, X cycled with offset so the ball keeps going. Select a key there and pick a tangent type, or drag it.".into()
}

/// Two Local Animations of different lengths at different times, merged
/// as they are (Merge: the widest range) and lined up (Merge Animation).
fn local_animation(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::keys::Tangent;
    crate::graph_io::clear(g);
    g.scene = crate::node_graph::SceneRange { start: 1, end: 72, fps_num: 24, fps_den: 1 };
    let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 0.8, location: "/cube".into() }, p(40.0, 20.0));
    let place_c = g.add_node("PlaceCube".into(), transform([-1.5, 0.4, 0.0], [0.0; 3], [1.0; 3]), p(40.0, 120.0));
    let rise = g.add_node("Rise".into(), NodeType::LocalAnimation { start: 1, end: 24, inherited: serde_json::Value::Null }, p(40.0, 220.0));
    let ball = g.add_node("Ball".into(), NodeType::CreateSphere { radius: 0.4, segments: 24, location: "/ball".into() }, p(300.0, 20.0));
    let place_b = g.add_node("PlaceBall".into(), transform([1.0, 0.4, 0.0], [0.0; 3], [1.0; 3]), p(300.0, 120.0));
    let roll = g.add_node("Roll".into(), NodeType::LocalAnimation { start: 49, end: 60, inherited: serde_json::Value::Null }, p(300.0, 220.0));
    g.add_connection(cube, 0, place_c, 0);
    g.add_connection(place_c, 0, rise, 0);
    g.add_connection(ball, 0, place_b, 0);
    g.add_connection(place_b, 0, roll, 0);
    let merge = g.add_node("AsTheyAre".into(), NodeType::merge(), p(40.0, 330.0));
    g.add_connection(rise, 0, merge, 0);
    g.add_connection(roll, 0, merge, 1);
    let lined = g.add_node("LinedUp".into(), NodeType::MergeAnimation { align: crate::types::AnimAlign::First, match_rate: false, inputs: 2 }, p(300.0, 330.0));
    g.add_connection(rise, 0, lined, 0);
    g.add_connection(roll, 0, lined, 1);
    g.sync_inherited();
    // The cube rises 2 m over its 23 frames; the ball rolls 3 m over 11.
    for (f, v) in [(0.0, 0.4), (23.0, 2.4)] { g.set_key_value(rise, "inherited/translation/1", f, v); }
    for (f, v) in [(0.0, 1.0), (11.0, 4.0)] { g.set_key_value(roll, "inherited/translation/0", f, v); }
    for (f, v) in [(0.0, 0.0), (11.0, -7.5)] { g.set_key_value(roll, "inherited/rotation/2", f, v); }
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == roll) {
        for c in &mut n.channels { for k in &mut c.keys { k.in_type = Tangent::Linear; k.out_type = Tangent::Linear; } }
    }
    finish(g, lined, lined);
    "Rise keys the cube's Transform on frames 1 to 24, Roll the ball's on 49 to 60, each on frames of its own. AsTheyAre merges them as they are: select it, the timeline shows frames 1 to 60. LinedUp (viewed) starts both on frame 1; set its Align to Stretch and the ball rolls over the cube's 23 frames.".into()
}

fn usd(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let shapes = g.add_node("Shapes".into(), NodeType::load_usd(examples::path(examples::SHAPES_USD)), p(40.0, 20.0));
    let table  = g.add_node("Table".into(), NodeType::load_usd(examples::path(examples::TABLE_USD)), p(300.0, 20.0));
    let lift   = g.add_node("OnTable".into(), transform([0.0, 0.77, 0.0], [0.0, 0.0, 0.0], [0.35, 0.35, 0.35]), p(40.0, 120.0));
    let merge  = g.add_node("Merge".into(), NodeType::merge(), p(170.0, 220.0));
    g.add_connection(shapes, 0, lift, 0);
    g.add_connection(lift, 0, merge, 0);
    g.add_connection(table, 0, merge, 1);
    finish(g, merge, shapes);
    format!("USD import: three shapes from one file, a table from another.{}", missing_examples())
}

// ── Modelling ────────────────────────────────────────────────────────────────

fn tower(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    // Polygon 3 is the top face of the cube; it keeps its index through
    // inset, extrude and bevel.
    let top = || picked(vec![3]);
    let group = ExtrudeMode::Group;
    let ops = vec![
        PolyOp::new(top(), PolyOpKind::Inset { amount: 0.12, by_polygon: false }),
        PolyOp::new(top(), PolyOpKind::Extrude { height: 0.7, mode: group }),
        PolyOp::new(top(), PolyOpKind::Bevel { height: 0.15, outline: 0.12, mode: group }),
        PolyOp::new(top(), PolyOpKind::Extrude { height: 0.5, mode: group }),
        PolyOp::new(top(), PolyOpKind::Bevel { height: 0.45, outline: -0.3, mode: group }),
        PolyOp::new(top(), PolyOpKind::Inset { amount: 0.05, by_polygon: false }),
        PolyOp::new(top(), PolyOpKind::Extrude { height: -0.2, mode: group }),
    ];
    let cube = g.add_node("Cube".into(), NodeType::cube(1.0), p(180.0, 20.0));
    let ep   = g.add_node("Tower".into(), edit_poly(ops, top()), p(180.0, 130.0));
    g.add_connection(cube, 0, ep, 0);
    finish(g, ep, ep);
    "Edit Poly tower: seven live operations on the top face. Change their values in Parameters.".into()
}

fn panels(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    // The 36 cells keep their indices; new polygons are added after them.
    let cells = || picked((0..36).collect());
    let every_other = || picked((0..36).filter(|c| (c / 6 + c % 6) % 2 == 0).collect());
    let ops = vec![
        PolyOp::new(cells(), PolyOpKind::Inset { amount: 0.05, by_polygon: true }),
        PolyOp::new(cells(), PolyOpKind::Extrude { height: 0.08, mode: ExtrudeMode::ByPolygon }),
        PolyOp::new(every_other(), PolyOpKind::Bevel { height: 0.25, outline: -0.08, mode: ExtrudeMode::ByPolygon }),
    ];
    let grid = g.add_node("Grid".into(), NodeType::grid(6, 6, 3.0), p(180.0, 20.0));
    let ep   = g.add_node("Panels".into(), edit_poly(ops, every_other()), p(180.0, 130.0));
    g.add_connection(grid, 0, ep, 0);
    finish(g, ep, ep);
    "Edit Poly panels: by-polygon inset and extrude on every cell, a bevel on every other one.".into()
}

fn goblet(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let cube = PolyMesh::from_mesh(&crate::node_graph::nodes::create_cube(1.0));
    // The four upright edges: one of them, grown into its ring.
    let up = cube.edges().into_iter()
        .find(|e| (cube.verts[e[0] as usize] - cube.verts[e[1] as usize]).y.abs() > 0.9)
        .unwrap_or([0, 1]);
    let mut ring = PolySelection { level: SubLevel::Edge, edges: vec![up], ..Default::default() };
    ring.expand_edges(&cube, true);
    // Connect renumbers the polygons: find the top face on the mesh it
    // leaves. From there on the face keeps its index.
    let connect = PolyOp::new(ring.clone(), PolyOpKind::Connect { segments: 2 });
    let mut cut = cube.clone();
    connect.apply(&mut cut);
    let top_index = (0..cut.polys.len()).find(|q| cut.normal(*q).y > 0.9).unwrap_or(0) as u32;
    let top = || picked(vec![top_index]);
    let group = ExtrudeMode::Group;
    // Foot and stem, collapsed.
    let mut ops = vec![
        connect,
        PolyOp::new(top(), PolyOpKind::Transform { translate: [0.0; 3], rotate: [0.0, 0.0, 0.0, 1.0], scale: [0.3, 1.0, 0.3], falloff: 0.0 }),
        PolyOp::new(top(), PolyOpKind::Extrude { height: 0.7, mode: group }),
    ];
    crate::core::poly::collapse_all(&mut ops);
    // The cup, live.
    ops.push(PolyOp::new(top(), PolyOpKind::Bevel { height: 0.6, outline: 0.5, mode: group }));
    ops.push(PolyOp::new(top(), PolyOpKind::Extrude { height: 0.5, mode: group }));
    ops.push(PolyOp::new(top(), PolyOpKind::Inset { amount: 0.1, by_polygon: false }));
    ops.push(PolyOp::new(top(), PolyOpKind::Extrude { height: -0.9, mode: group }));
    ops.push(PolyOp::new(PolySelection { source: SelSource::All, ..Default::default() }, PolyOpKind::Subdivide { iterations: 2 }));
    let node = g.add_node("Cube".into(), NodeType::cube(1.0), p(180.0, 20.0));
    let ep   = g.add_node("Goblet".into(), edit_poly(ops, top()), p(180.0, 130.0));
    g.add_connection(node, 0, ep, 0);
    finish(g, ep, ep);
    "Edit Poly goblet: three collapsed operations for the foot and stem, five live ones for the cup. Restore brings the collapsed ones back.".into()
}

fn bridge(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    // The two faces that look at each other: their centres lie in this box.
    let facing = PolySelection {
        source: SelSource::InBox { min: [-0.1, 0.4, -0.1], max: [0.1, 2.1, 0.1] },
        ..Default::default()
    };
    let ops = vec![
        PolyOp::new(facing, PolyOpKind::Bridge),
        PolyOp::new(PolySelection { level: SubLevel::Edge, source: SelSource::InBox { min: [-0.6, 0.4, -0.6], max: [0.6, 2.1, 0.6] }, ..Default::default() },
            PolyOpKind::Connect { segments: 3 }),
    ];
    // Two cubes, each a primitive of its own.
    let a    = g.add_node("Lower".into(), NodeType::CreateCube { size: 1.0, location: "/lower".into() }, p(60.0, 20.0));
    let b    = g.add_node("Upper".into(), NodeType::CreateCube { size: 1.0, location: "/upper".into() }, p(300.0, 20.0));
    let lift = g.add_node("Lift".into(), transform([0.0, 2.5, 0.0], [0.0, 45f32.to_radians(), 0.0], [1.0, 1.0, 1.0]), p(300.0, 120.0));
    let m    = g.add_node("Merge".into(), NodeType::merge(), p(180.0, 220.0));
    let ep   = g.add_node("Bridge".into(), edit_poly(ops, Default::default()), p(180.0, 320.0));
    g.add_connection(b, 0, lift, 0);
    g.add_connection(a, 0, m, 0);
    g.add_connection(lift, 0, m, 1);
    g.add_connection(m, 0, ep, 0);
    finish(g, ep, ep);
    "Edit Poly bridge: the selection is a box rule, so it still works when the Lift node turns the upper cube.".into()
}

fn sea_mine(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let all = || PolySelection { source: SelSource::All, ..Default::default() };
    // One round of subdivision turns the cube into 24 quads. They keep the
    // indices 0 to 23 through every inset, extrude and bevel that follows.
    let plates = || picked((0..24).collect());
    let long   = || picked((0..24).filter(|q| q % 2 == 0).collect());
    let short  = || picked((0..24).filter(|q| q % 2 == 1).collect());
    let each = ExtrudeMode::ByPolygon;
    // The hull, collapsed.
    let mut ops = vec![
        PolyOp::new(all(), PolyOpKind::Subdivide { iterations: 1 }),
        PolyOp::new(all(), PolyOpKind::Transform { translate: [0.0, 1.2, 0.0], rotate: [0.0, 0.0, 0.0, 1.0], scale: [2.4, 2.4, 2.4], falloff: 0.0 }),
        PolyOp::new(plates(), PolyOpKind::Inset { amount: 0.06, by_polygon: true }),
        PolyOp::new(plates(), PolyOpKind::Extrude { height: -0.05, mode: each }),
        PolyOp::new(plates(), PolyOpKind::Inset { amount: 0.07, by_polygon: true }),
    ];
    crate::core::poly::collapse_all(&mut ops);
    // Horns and ports, live.
    ops.push(PolyOp::new(plates(), PolyOpKind::Bevel { height: 0.12, outline: -0.05, mode: each }));
    ops.push(PolyOp::new(long(), PolyOpKind::Bevel { height: 0.55, outline: -0.07, mode: each }));
    ops.push(PolyOp::new(long(), PolyOpKind::Bevel { height: 0.06, outline: 0.05, mode: each }));
    ops.push(PolyOp::new(short(), PolyOpKind::Inset { amount: 0.04, by_polygon: true }));
    ops.push(PolyOp::new(short(), PolyOpKind::Extrude { height: -0.12, mode: each }));
    ops.push(PolyOp::new(all(), PolyOpKind::Subdivide { iterations: 2 }));
    let cube = g.add_node("Cube".into(), NodeType::cube(1.0), p(180.0, 20.0));
    let ep   = g.add_node("SeaMine".into(), edit_poly(ops, long()), p(180.0, 130.0));
    g.add_connection(cube, 0, ep, 0);
    finish(g, ep, ep);
    "Edit Poly sea mine: about ten thousand polygons. Change the height of the horns in operation 7, or lower the last Subdivide to see the cage.".into()
}

fn bolt(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let top = || picked(vec![3]);
    let all_edges = PolySelection { level: SubLevel::Edge, source: SelSource::All, ..Default::default() };
    let top_corners = PolySelection { level: SubLevel::Vertex, source: SelSource::InBox { min: [-2.0, 1.7, -2.0], max: [2.0, 3.0, 2.0] }, ..Default::default() };
    let ops = vec![
        // Head: a slab, its edges taken off.
        PolyOp::new(PolySelection { source: SelSource::All, ..Default::default() },
            PolyOpKind::Transform { translate: [0.0, 0.2, 0.0], rotate: [0.0, 0.0, 0.0, 1.0], scale: [1.6, 0.4, 1.6], falloff: 0.0 }),
        PolyOp::new(all_edges, PolyOpKind::Chamfer { amount: 0.06 }),
        // Shank: inset the top, pull it up, cut it into rings.
        PolyOp::new(top(), PolyOpKind::Inset { amount: 0.35, by_polygon: false }),
        PolyOp::new(top(), PolyOpKind::Extrude { height: 1.5, mode: ExtrudeMode::Group }),
        PolyOp::new(Default::default(), PolyOpKind::Slice { axis: 1, offset: 0.8 }),
        PolyOp::new(Default::default(), PolyOpKind::Slice { axis: 1, offset: 1.2 }),
        PolyOp::new(Default::default(), PolyOpKind::Slice { axis: 1, offset: 1.6 }),
        // Tip: narrower, a spike on each corner, and a lid hinged open.
        PolyOp::new(top(), PolyOpKind::Outline { amount: -0.08 }),
        PolyOp::new(top_corners, PolyOpKind::ExtrudeVertex { height: 0.15, width: 0.08 }),
        PolyOp::new(top(), PolyOpKind::Hinge { angle: 50.0, segments: 5, edge: 0 }),
    ];
    let cube = g.add_node("Cube".into(), NodeType::cube(1.0), p(180.0, 20.0));
    let ep   = g.add_node("Bolt".into(), edit_poly(ops, top()), p(180.0, 130.0));
    g.add_connection(cube, 0, ep, 0);
    finish(g, ep, ep);
    "Edit Poly bolt: chamfer, slice, outline, vertex extrude and hinge. Change the hinge angle in the last operation.".into()
}

// ── Lighting & Rendering ─────────────────────────────────────────────────────

/// Martino's DeLorean scene: the car, a camera, a dome lit by the examples'
/// sky and a blue rect light, merged; RenderScene Settings and Karma's
/// Render Settings after them, then Write USD and a husk Render Command.
/// The layer and the frames go to the temporary folder.
fn delorean_render(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::camera::Camera;
    use crate::core::light::{Light, LightType};
    use crate::core::properties::Value;
    use crate::core::render::{CommandSettings, RendererSettings, SceneSettings};
    let car = load_delorean(g);
    let camera = g.add_node("Camera".into(), NodeType::Camera { location: "/scene/camera".into(), translate: [0.0, 3.0, 11.6], rotate: [-12.0, 0.0, 0.0],
        camera: Camera { focal_length: 50.0, horizontal_aperture: 23.76, vertical_aperture: 13.37, clipping_range: [1.0, 1_000_000.0], ..Default::default() } }, p(420.0, 20.0));
    let shot = g.add_node("Merge".into(), NodeType::Merge { mode: MergeMode::Auto, inputs: 3 }, p(300.0, 120.0));

    let mut dome = Light::new(LightType::Dome);
    dome.set("inputs:intensity", Value::Float(0.1));
    dome.set("inputs:texture:file", Value::Asset(examples::path(examples::SKY_HDR)));
    dome.set("inputs:texture:format", Value::Token("latlong".into()));
    let dome = g.add_node("DomeLight".into(), NodeType::light(dome), p(640.0, 120.0));
    let mut rect = Light::new(LightType::Rect);
    rect.set("inputs:color", Value::Color3f([0.2502537, 0.46810228, 0.63512963]));
    rect.set("inputs:exposure", Value::Float(5.0));
    rect.set("inputs:normalize", Value::Bool(true));
    let rect = g.add_node("RectLight".into(), NodeType::Light { location: "/lights/rect".into(),
        translate: [3.2311358, 2.9668941, 2.390841], rotate: [-55.98229, 52.89807, 49.75959], light: rect }, p(860.0, 120.0));
    let lit = g.add_node("Merge2".into(), NodeType::Merge { mode: MergeMode::Auto, inputs: 4 }, p(420.0, 240.0));

    let dir = std::env::temp_dir().join("xms_lighting_rendering");
    let at = |f: &str| dir.join(f).to_string_lossy().to_string();
    let scene = g.add_node("RenderSceneSettings".into(), NodeType::RenderSceneSettings { scene: SceneSettings {
        motion_blur: false, depth_of_field: false, output: at("render/beauty.$F4.exr"), ..Default::default() } }, p(420.0, 360.0));
    let mut karma = RendererSettings::default();
    for (k, v) in [
        ("karma:global:convergence_mode", Value::Token("Path Traced".into())),
        ("karma:global:imagemode", Value::Token("Bucket".into())),
        ("karma:global:pathtracedsamples", Value::Int(32)),
        ("karma:global:samplesperpixel", Value::Int(3)),
        ("karma:object:diffuselimit", Value::Float(2.0)),
        ("karma:object:refractlimit", Value::Float(8.0)),
        ("karma:object:varianceaa_maxsamples", Value::Int(12)),
        ("karma:object:varianceaa_minsamples", Value::Int(3)),
    ] { karma.values.insert(k.into(), v); }
    karma.aovs.insert("karma".into(), ["C", "combineddiffuse", "combinedglossyreflection", "glossytransmission", "P", "depth", "N", "Ng", "UV", "combinedemission"]
        .iter().map(|s| s.to_string()).collect());
    let settings = g.add_node("RenderSettings".into(), NodeType::RenderSettings { renderer: karma }, p(420.0, 460.0));
    let write = g.add_node("WriteUSD".into(), NodeType::WriteUsd { path: at("delorean_karma.usda") }, p(420.0, 560.0));
    let render = g.add_node("RenderCommand".into(), NodeType::RenderCommand { command: CommandSettings {
        custom_range: true, start: 1, end: 1, ..Default::default() } }, p(420.0, 660.0));

    g.add_connection(car, 0, shot, 0);
    g.add_connection(camera, 0, shot, 1);
    g.add_connection(shot, 0, lit, 0);
    g.add_connection(dome, 0, lit, 1);
    g.add_connection(rect, 0, lit, 2);
    g.add_connection(lit, 0, scene, 0);
    g.add_connection(scene, 0, settings, 0);
    g.add_connection(settings, 0, write, 0);
    g.add_connection(write, 0, render, 0);
    finish(g, settings, render);
    format!("The DeLorean lit by a dome (the examples' sky) and a blue rect light, through Camera at 1920 by 1080. Render on the Render Command runs husk (Houdini's bin folder on the PATH; pick your own in its presets) and writes {}; the Image Viewer renders it too.{}",
        at("render/beauty.0001.exr"), missing_examples())
}

// ── USD ──────────────────────────────────────────────────────────────────────

fn load_delorean(g: &mut NodeGraphState) -> NodeId {
    crate::graph_io::clear(g);
    g.add_node("DeLorean".into(), NodeType::load_usd(examples::path(examples::DELOREAN_USD)), p(180.0, 20.0))
}

fn usd_heavy(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    let load = load_delorean(g);
    finish(g, load, load);
    format!("USD: 43 primitives, 461,595 triangles. The Parameters of the Load USD node list its materials and textures. The UV Editor tab shows its UVs.{}", missing_examples())
}

fn usd_pick(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    let load = load_delorean(g);
    let pick = g.add_node("PickWheel".into(), NodeType::PickPrims { pattern: "Wheel_Front_L".into() }, p(180.0, 110.0));
    let lift = g.add_node("PullOut".into(), transform([0.6, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]), p(180.0, 200.0));
    let edit = g.add_node("EditWheel".into(), edit_poly(vec![], PolySelection::default()), p(180.0, 290.0));
    g.add_connection(load, 0, pick, 0);
    g.add_connection(pick, 0, lift, 0);
    g.add_connection(lift, 0, edit, 0);
    finish(g, edit, pick);
    format!("USD pick: the front left wheel is picked by a path pattern, pulled out by the Transform, and is what the Edit Poly node edits. The other 42 primitives pass through untouched.{}", missing_examples())
}

fn usd_prune(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    let load = load_delorean(g);
    let prune = g.add_node("WheelsOnly".into(), NodeType::PrunePrims { pattern: String::new(), keep: true }, p(180.0, 110.0));
    // What it keeps is in its Prims Location tab: every prim with Wheel or
    // Calliper in its name, at any depth.
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == prune) { n.locations = vec!["/**/*Wheel*".into(), "/**/*Calliper*".into()]; }
    g.add_connection(load, 0, prune, 0);
    finish(g, prune, prune);
    format!("USD prune: only the primitives its Prims Location tab lists are kept. Switch Matches to \"Remove\" for the car without its wheels.{}", missing_examples())
}

fn usd_set_attribute(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::properties::{SetContext, Value};
    use crate::core::set_attribute::SetRow;
    let load = load_delorean(g);
    // Every prim with Wheel in its name, at any depth: invisible.
    let hide = g.add_node("HideWheels".into(), NodeType::SetAttribute { rows: vec![
        SetRow { context: SetContext::Property, name: "visibility".into(), value: Value::Token("invisible".into()) },
    ] }, p(180.0, 110.0));
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == hide) { n.locations = vec!["/**/*Wheel*".into()]; }
    // The root prim, a group with no geometry: one opinion on it.
    let asset = g.add_node("Asset".into(), NodeType::SetAttribute { rows: vec![
        SetRow { context: SetContext::Property, name: "kind".into(), value: Value::Token("component".into()) },
        SetRow { context: SetContext::Object, name: "displayColor".into(), value: Value::Color3f([0.75, 0.75, 0.78]) },
    ] }, p(180.0, 200.0));
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == asset) { n.locations = vec!["/*".into()]; }
    g.add_connection(load, 0, hide, 0);
    g.add_connection(hide, 0, asset, 0);
    finish(g, asset, asset);
    format!("USD set attribute: HideWheels makes every prim with Wheel in its name invisible; Asset sets kind and a displayColor on the root prim, a group: one opinion each, on the group. Add a Write USD node after Asset to see them written.{}", missing_examples())
}

fn usd_material_binding(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::material::PreviewSurface;
    let load = load_delorean(g);
    let paint = g.add_node("CarPaint".into(), NodeType::Material { surface: PreviewSurface {
        diffuse_color: [0.45, 0.02, 0.02], metallic: 0.6, roughness: 0.3, clearcoat: 1.0, ..Default::default() } }, p(-60.0, 90.0));
    let chrome = g.add_node("Chrome".into(), NodeType::Material { surface: PreviewSurface {
        diffuse_color: [0.9, 0.9, 0.9], metallic: 1.0, roughness: 0.08, ..Default::default() } }, p(-60.0, 170.0));
    let looks = g.add_node("Looks".into(), NodeType::MaterialBinding { library: crate::types::material_library(), inputs: 3, bindings: vec![] }, p(180.0, 110.0));
    // Row 1, the whole model, gets input 1; row 2, the wheels, input 2.
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == looks) { n.locations = vec!["/*".into(), "/**/*Wheel*".into()]; }
    g.add_connection(load, 0, looks, 0);
    g.add_connection(paint, 0, looks, 1);
    g.add_connection(chrome, 0, looks, 2);
    finish(g, looks, looks);
    format!("USD material binding: CarPaint and Chrome go under /materials; row 1 of the Bindings tab binds input 1 to the whole model, row 2 input 2 to the wheels, the later row winning. Switch the Viewer to Textured to see them, and add a Write USD node to write them.{}", missing_examples())
}

/// Shapes and two cameras: a perspective one at the front, an orthographic
/// one above, all in one Merge.
fn cameras(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::camera::{Camera, Projection};
    crate::graph_io::clear(g);
    let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 1.0, location: "/set/cube".into() }, p(40.0, 20.0));
    let sphere = g.add_node("Sphere".into(), NodeType::CreateSphere { radius: 0.5, segments: 32, location: "/set/sphere".into() }, p(260.0, 20.0));
    let grid = g.add_node("Grid".into(), NodeType::CreateGrid { rows: 10, cols: 10, size: 6.0, location: "/set/ground".into() }, p(480.0, 20.0));
    let place = |g: &mut NodeGraphState, id: NodeId, t: [f32; 3]| {
        let move_it = g.add_node(format!("Place{}", id.0), NodeType::Transform { translation: Vec3::from(t), rotation: Vec3::ZERO, scale: Vec3::ONE }, p(0.0, 0.0));
        g.add_connection(id, 0, move_it, 0);
        move_it
    };
    let cube_at = place(g, cube, [-1.0, 0.5, 0.0]);
    let sphere_at = place(g, sphere, [1.0, 0.5, 0.0]);
    for (id, x) in [(cube_at, 40.0), (sphere_at, 260.0)] {
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) { n.position = p(x, 110.0); }
    }
    let front = g.add_node("Front".into(), NodeType::Camera { location: "/cameras/front".into(), translate: [0.0, 1.6, 5.5], rotate: [-12.0, 0.0, 0.0],
        camera: Camera { focal_length: 35.0, horizontal_aperture: 36.0, vertical_aperture: 24.0, ..Default::default() } }, p(700.0, 20.0));
    // Orthographic: its gate, in tenths of a scene unit, is what it sees (8 m across).
    let top = g.add_node("Top".into(), NodeType::Camera { location: "/cameras/top".into(), translate: [0.0, 10.0, 0.0], rotate: [-90.0, 0.0, 0.0],
        camera: Camera { projection: Projection::Orthographic, horizontal_aperture: 80.0, vertical_aperture: 60.0, ..Default::default() } }, p(920.0, 20.0));
    let merge = g.add_node("Merge".into(), NodeType::Merge { mode: MergeMode::Auto, inputs: 5 }, p(480.0, 220.0));
    for (k, src) in [cube_at, sphere_at, grid, front, top].into_iter().enumerate() { g.add_connection(src, 0, merge, k); }
    finish(g, merge, front);
    "Cameras: Front, a 35 mm lens on a full-frame gate, and Top, orthographic, 8 m across. Choose one in the Viewer's camera menu (🎥 Perspective) or press Look through on the operator; navigating leaves it.".into()
}

/// Martino's material bindings graph: shapes at Primitive Locations under
/// /root/asset, one Merge, Transforms by location, and two Material
/// Bindings: the first puts four materials under /root/materials and binds
/// three by row, the second binds the first one's ground material by
/// Location.
fn material_bindings(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::material::{BindTo, PreviewSurface};
    use crate::core::uv::UvMethod;
    crate::graph_io::clear(g);
    let colour = |c: [f32; 3]| NodeType::Material { surface: PreviewSurface { diffuse_color: c, ..Default::default() } };
    let locate = |g: &mut NodeGraphState, id: NodeId, rows: &[&str]| {
        if let Some(n) = g.nodes.iter_mut().find(|n| n.id == id) { n.locations = rows.iter().map(|r| r.to_string()).collect(); }
    };
    let transform = |t: [f32; 3], r: [f32; 3], s: [f32; 3]| NodeType::Transform { translation: Vec3::from(t), rotation: Vec3::from(r), scale: Vec3::from(s) };

    // The shapes.
    let cube = g.add_node("Cube".into(), NodeType::CreateCube { size: 1.0, location: "/root/asset/cube".into() }, p(80.0, -620.0));
    let sphere = g.add_node("Sphere".into(), NodeType::CreateSphere { radius: 0.5, segments: 32, location: "/root/asset/sphere".into() }, p(310.0, -620.0));
    let grid = g.add_node("Grid".into(), NodeType::CreateGrid { rows: 10, cols: 10, size: 2.0, location: "/root/asset/grid".into() }, p(545.0, -620.0));
    let grid_uv = g.add_node("UVUnwrap".into(), NodeType::UvUnwrap { method: UvMethod::Conformal, angle: 66.0, margin: 0.02, axis: 1, tiles: 1 }, p(545.0, -530.0));
    let sphere2 = g.add_node("Sphere2".into(), NodeType::CreateSphere { radius: 0.5, segments: 32, location: "/root/asset/sphere_2".into() }, p(760.0, -620.0));
    let turn = g.add_node("Transform".into(), transform([-2.65, 0.0, 1.97], [0.0, -1.5009831, 0.0], [1.0; 3]), p(760.0, -530.0));
    let sphere2_uv = g.add_node("UVUnwrap2".into(), NodeType::UvUnwrap { method: UvMethod::Box, angle: 66.0, margin: 0.02, axis: 1, tiles: 1 }, p(760.0, -440.0));
    // One Merge for all four, where chaining Merges was needed before.
    let merge = g.add_node("Merge".into(), NodeType::Merge { mode: MergeMode::Auto, inputs: 4 }, p(395.0, -340.0));
    // Each Transform moves the primitive its Prims Location tab names.
    let move_sphere = g.add_node("MoveSphere".into(), transform([-2.32, 0.0, 0.0], [0.0; 3], [1.0; 3]), p(395.0, -230.0));
    locate(g, move_sphere, &["/root/asset/sphere"]);
    let move_grid = g.add_node("MoveGrid".into(), transform([0.0, 0.0, 3.0], [0.0; 3], [1.0, 1.0, 1.35]), p(395.0, -130.0));
    locate(g, move_grid, &["/root/asset/grid"]);

    // The materials, on the first Material Binding's side.
    let plastic = g.add_node("basePlastic".into(), colour([0.55, 0.18, 0.18]), p(60.0, 35.0));
    let metal = g.add_node("baseMetal".into(), colour([0.0, 0.18, 0.18]), p(60.0, 130.0));
    let ground = g.add_node("ground".into(), NodeType::Material { surface: PreviewSurface {
        textures: [("diffuseColor".to_string(), concat!(env!("CARGO_MANIFEST_DIR"), "/assets/splash.png").to_string())].into(),
        ..Default::default() } }, p(60.0, 225.0));
    let spare = g.add_node("test_placeHolder".into(), colour([0.207, 0.307, 0.097]), p(60.0, 320.0));
    // Rows 1 to 3 bind inputs 1 to 3; input 4 goes into the library unbound.
    let bind = g.add_node("MaterialBinding".into(), NodeType::MaterialBinding { library: "/root/materials".into(), inputs: 5, bindings: vec![] }, p(395.0, 115.0));
    locate(g, bind, &["/root/asset/cube", "/root/asset/sphere", "/root/asset/grid"]);
    // A material already in the scene, bound by Location.
    let bind_ground = g.add_node("MaterialBinding2".into(), NodeType::MaterialBinding {
        library: crate::types::material_library(), inputs: 1, bindings: vec![BindTo::Path("/root/materials/ground".into())] }, p(395.0, 300.0));
    locate(g, bind_ground, &["/root/asset/sphere_2"]);
    let write = g.add_node("WriteUSD".into(), NodeType::WriteUsd {
        path: std::env::temp_dir().join("xms_material_bindings.usda").to_string_lossy().to_string() }, p(395.0, 420.0));

    g.add_connection(grid, 0, grid_uv, 0);
    g.add_connection(sphere2, 0, turn, 0);
    g.add_connection(turn, 0, sphere2_uv, 0);
    for (k, src) in [cube, sphere, grid_uv, sphere2_uv].into_iter().enumerate() { g.add_connection(src, 0, merge, k); }
    g.add_connection(merge, 0, move_sphere, 0);
    g.add_connection(move_sphere, 0, move_grid, 0);
    g.add_connection(move_grid, 0, bind, 0);
    for (k, m) in [plastic, metal, ground, spare].into_iter().enumerate() { g.add_connection(m, 0, bind, k + 1); }
    g.add_connection(bind, 0, bind_ground, 0);
    g.add_connection(bind_ground, 0, write, 0);
    finish(g, write, bind);
    "Material bindings: four shapes in one Merge; MaterialBinding puts basePlastic, baseMetal, ground and test_placeHolder under /root/materials and binds the first three to the cube, the sphere and the grid by row; MaterialBinding2 binds ground to the second sphere by Location. Switch the Viewer to Textured to see them; Write USD writes them to the temporary folder.".into()
}

/// Three MaterialX operators bound to three spheres: a metal
/// standard_surface, an OpenPBR plastic, and a standard_surface whose
/// colour is an image.
fn materialx_shaders(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use xms_matx::{Graph, Value};
    let lib = crate::materialx::lib();
    crate::graph_io::clear(g);
    let f = |x: f32| Value::Floats(vec![x]);
    let rgb = |r: f32, gr: f32, b: f32| Value::Floats(vec![r, gr, b]);

    // Gold: a metal standard_surface.
    let mut gold = Graph::standard_surface(lib);
    let s = gold.surface_shader().expect("a new graph has its surface");
    gold.set_value(s, "base_color", rgb(1.0, 0.766, 0.336));
    gold.set_value(s, "metalness", f(1.0));
    gold.set_value(s, "specular_roughness", f(0.25));
    // Red plastic: an OpenPBR surface.
    let mut red = Graph::new();
    let o = red.add(lib, "open_pbr_surface", [0.0, 0.0]).expect("open_pbr_surface is in the library");
    let m = red.add(lib, "surfacematerial", [320.0, 0.0]).expect("surfacematerial is in the library");
    red.connect(lib, o, "out", m, "surfaceshader").expect("a surface shader goes into a material");
    red.set_value(o, "base_color", rgb(0.6, 0.05, 0.05));
    red.set_value(o, "specular_roughness", f(0.4));
    // Textured: an image into a standard_surface's colour.
    let mut textured = Graph::standard_surface(lib);
    let s = textured.surface_shader().expect("a new graph has its surface");
    let img = textured.add(lib, "image", [-320.0, 0.0]).expect("image is in the library");
    textured.set_value(img, "file", Value::String(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/splash.png").into()));
    textured.connect(lib, img, "out", s, "base_color").expect("an image goes into a colour");
    textured.set_value(s, "specular_roughness", f(0.5));

    let sphere = |name: &str| NodeType::CreateSphere { radius: 0.5, segments: 48, location: format!("/root/{name}") };
    let shift = |x: f32| NodeType::Transform { translation: Vec3::new(x, 0.0, 0.0), rotation: Vec3::ZERO, scale: Vec3::ONE };
    let a = g.add_node("GoldSphere".into(), sphere("gold"), p(60.0, -420.0));
    let a_at = g.add_node("Left".into(), shift(-1.3), p(60.0, -330.0));
    let b = g.add_node("RedSphere".into(), sphere("red"), p(290.0, -420.0));
    let c = g.add_node("TexturedSphere".into(), sphere("textured"), p(520.0, -420.0));
    let c_uv = g.add_node("UVUnwrap".into(), NodeType::UvUnwrap { method: crate::core::uv::UvMethod::Box, angle: 66.0, margin: 0.02, axis: 1, tiles: 1 }, p(520.0, -330.0));
    let c_at = g.add_node("Right".into(), shift(1.3), p(520.0, -240.0));
    let merge = g.add_node("Merge".into(), NodeType::Merge { mode: MergeMode::Auto, inputs: 3 }, p(290.0, -140.0));
    let gold = g.add_node("Gold".into(), NodeType::MaterialX { graph: gold }, p(60.0, 35.0));
    let red = g.add_node("RedPlastic".into(), NodeType::MaterialX { graph: red }, p(60.0, 130.0));
    let textured = g.add_node("Textured".into(), NodeType::MaterialX { graph: textured }, p(60.0, 225.0));
    let bind = g.add_node("MaterialBinding".into(), NodeType::MaterialBinding { library: "/root/materials".into(), inputs: 4, bindings: vec![] }, p(290.0, 80.0));
    if let Some(n) = g.nodes.iter_mut().find(|n| n.id == bind) {
        n.locations = vec!["/root/gold".into(), "/root/red".into(), "/root/textured".into()];
    }
    let write = g.add_node("WriteUSD".into(), NodeType::WriteUsd {
        path: std::env::temp_dir().join("xms_materialx.usda").to_string_lossy().to_string() }, p(290.0, 300.0));

    g.add_connection(a, 0, a_at, 0);
    g.add_connection(c, 0, c_uv, 0);
    g.add_connection(c_uv, 0, c_at, 0);
    for (k, src) in [a_at, b, c_at].into_iter().enumerate() { g.add_connection(src, 0, merge, k); }
    g.add_connection(merge, 0, bind, 0);
    for (k, m) in [gold, red, textured].into_iter().enumerate() { g.add_connection(m, 0, bind, k + 1); }
    g.add_connection(bind, 0, write, 0);
    finish(g, write, gold);
    "MaterialX: Gold (standard_surface, metal), RedPlastic (open_pbr_surface) and Textured (an image into a standard_surface) bound to three spheres under /root/materials. Double-click a MaterialX node, or select it and press ↓, to open its graph; the Viewer shows what it can of each shader. Write USD writes them as MaterialX networks to the temporary folder.".into()
}

// ── UV ───────────────────────────────────────────────────────────────────────

fn uv_mine(g: &mut NodeGraphState, store: &mut IceGraphStore) -> String {
    sea_mine(g, store);
    let ep = g.nodes.iter().find(|n| matches!(n.node_type, NodeType::EditPoly { .. })).map(|n| n.id).expect("sea mine has an Edit Poly node");
    let uv = g.add_node("Unwrap".into(), NodeType::UvUnwrap {
        method: crate::core::uv::UvMethod::Conformal, angle: 50.0, margin: 0.01, axis: 1, tiles: 1 }, p(180.0, 240.0));
    g.add_connection(ep, 0, uv, 0);
    finish(g, uv, uv);
    "UV unwrap: open the UV Editor pane from the Panes menu. Lower the chart angle for more charts with less distortion.".into()
}

fn uv_udim(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let cube   = g.add_node("Box".into(), NodeType::cube(1.0), p(260.0, 20.0));
    let sphere = g.add_node("Ball".into(), NodeType::sphere(0.5, 16), p(40.0, 20.0));
    let body   = g.add_node("Body".into(), transform([0.0, 1.5, 0.0], [0.0; 3], [1.6, 3.0, 1.0]), p(480.0, 110.0));
    g.add_connection(cube, 0, body, 0);
    let mut last = body;
    // Two hands: a palm with three fingers beside it, well away from the body.
    for (h, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let x = side * 3.0;
        let col = 40.0 + h as f32 * 220.0;
        let palm = g.add_node(format!("Palm{}", h + 1), transform([x, 1.5, 0.0], [0.0; 3], [1.0, 1.0, 0.5]), p(col, 110.0));
        g.add_connection(sphere, 0, palm, 0);
        let mut hand = palm;
        for f in 0..3 {
            let row = 200.0 + f as f32 * 90.0;
            let finger = g.add_node(format!("Finger{}_{}", h + 1, f + 1),
                transform([x - 0.3 + f as f32 * 0.3, 2.35, 0.0], [0.0; 3], [0.2, 0.7, 0.2]), p(col + 110.0, row));
            let merge = g.add_node(format!("Hand{}_{}", h + 1, f + 1), NodeType::Merge { mode: MergeMode::Flatten, inputs: 2 }, p(col, row + 45.0));
            g.add_connection(cube, 0, finger, 0);
            g.add_connection(hand, 0, merge, 0);
            g.add_connection(finger, 0, merge, 1);
            hand = merge;
        }
        let join = g.add_node(format!("Join{}", h + 1), NodeType::Merge { mode: MergeMode::Flatten, inputs: 2 }, p(480.0, 300.0 + h as f32 * 100.0));
        g.add_connection(last, 0, join, 0);
        g.add_connection(hand, 0, join, 1);
        last = join;
    }
    let uv = g.add_node("Unwrap".into(), NodeType::UvUnwrap {
        method: crate::core::uv::UvMethod::Conformal, angle: 60.0, margin: 0.02, axis: 1, tiles: 3 }, p(480.0, 500.0));
    g.add_connection(last, 0, uv, 0);
    finish(g, uv, uv);
    "UDIM tiles: nine separate pieces over three tiles. The body has the most surface and takes 1001. Each hand keeps its palm and fingers together. Change the tile count on the Unwrap node.".into()
}

fn uv_edit(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    use crate::core::uv::{IslandEdit, UvMethod};
    crate::graph_io::clear(g);
    let cube = g.add_node("Box".into(), NodeType::cube(1.0), p(180.0, 20.0));
    let size = g.add_node("Stretch".into(), transform([0.0, 0.5, 0.0], [0.0, 0.0, 0.0], [2.0, 1.0, 1.0]), p(180.0, 110.0));
    let uv   = g.add_node("Unwrap".into(), NodeType::UvUnwrap { method: UvMethod::Conformal, angle: 45.0, margin: 0.03, axis: 1, tiles: 1 }, p(180.0, 200.0));
    let edit = g.add_node("Arrange".into(), NodeType::UvEdit { edits: vec![
        IslandEdit { island: 0, offset: [0.0, 0.0], rotate: 90.0, scale: [1.0, 1.0] },
        IslandEdit { island: 3, offset: [0.05, 0.1], rotate: 0.0, scale: [0.6, 0.6] },
    ] }, p(180.0, 290.0));
    g.add_connection(cube, 0, size, 0);
    g.add_connection(size, 0, uv, 0);
    g.add_connection(uv, 0, edit, 0);
    finish(g, edit, edit);
    "UV edit: open the UV Editor pane. Click an island to select it, drag to move it; its values are in Parameters.".into()
}

// ── Animation & mocap ────────────────────────────────────────────────────────

fn clip_basics(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let x = 180.0;
    let clip   = g.add_node("TestClip".into(), NodeType::TestClip { seconds: 4.0, fps_num: 30, fps_den: 1 }, p(x, 20.0));
    let rename = g.add_node("Rename".into(), NodeType::RenameJoints {
        find: "Left".into(), replace: "L_".into(), strip_namespace: true, prefix: "hero_".into() }, p(x, 110.0));
    let trim   = g.add_node("Trim".into(), NodeType::TrimClip { head: 15, tail: 15 }, p(x, 200.0));
    let retime = g.add_node("To24fps".into(), NodeType::Retime { fps_num: 24, fps_den: 1, mode: RetimeMode::Resample }, p(x, 290.0));
    let tc     = g.add_node("Timecode".into(), NodeType::SetTimecode { hours: 10, minutes: 0, seconds: 0, frames: 0, drop_frame: false }, p(x, 380.0));
    g.add_connection(clip, 0, rename, 0);
    g.add_connection(rename, 0, trim, 0);
    g.add_connection(trim, 0, retime, 0);
    g.add_connection(retime, 0, tc, 0);
    view(g, tc);
    g.selected_node  = Some(trim);
    g.selected_nodes = vec![trim];
    "Clip basics: select each node in turn. The timeline shows that node's range, rate and timecode. Space plays.".into()
}

fn fbx_import(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let load = g.add_node("Take".into(), NodeType::LoadFbx { path: examples::path(examples::TAKE_FBX), take: 0 }, p(180.0, 20.0));
    let trim = g.add_node("Trim".into(), NodeType::TrimClip { head: 120, tail: 120 }, p(180.0, 120.0));
    g.add_connection(load, 0, trim, 0);
    view(g, trim);
    format!("FBX import: a motion capture take of two people, 120 fps, a second trimmed off each end. Space plays.{}", missing_examples())
}

fn tpose(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let x = 180.0;
    let clip  = g.add_node("TestClip".into(), NodeType::TestClip { seconds: 2.0, fps_num: 30, fps_den: 1 }, p(x, 20.0));
    let pose  = g.add_node("TPose".into(), NodeType::AutoTPose { set_hip_height: true, hip_height: 95.0 }, p(x, 110.0));
    let arm = |joint: &str, z: f32| crate::core::anim::PoseEdit { joint: joint.into(), rotation: [0.0, 0.0, z], translation: [0.0; 3] };
    let fix   = g.add_node("FixPose".into(), NodeType::FixPose {
        edits: vec![arm("Take01:LeftArm", -20.0), arm("Take01:RightArm", 20.0)] }, p(x, 200.0));
    let skin  = g.add_node("ProxySkin".into(), NodeType::ProxySkin { thickness: 1.6 }, p(x, 290.0));
    let write = g.add_node("WriteTPose".into(), NodeType::WriteFbx { path: DEFAULT_WRITE_PATH.replace(".fbx", "_tpose.fbx"), mesh: true }, p(x, 380.0));
    g.add_connection(clip, 0, pose, 0);
    g.add_connection(pose, 0, fix, 0);
    g.add_connection(fix, 0, skin, 0);
    g.add_connection(skin, 0, write, 0);
    view(g, skin);
    g.selected_node  = Some(fix);
    g.selected_nodes = vec![fix];
    "T-pose and export: the Fix Pose node lowers both arms. Set a path on WriteTPose and press Write to export.".into()
}

fn mocap_tools(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let x = 180.0;
    let load   = g.add_node("Take".into(), NodeType::LoadFbx { path: examples::path(examples::TAKE_FBX), take: 0 }, p(x, 20.0));
    let split  = g.add_node("OneCharacter".into(), NodeType::SplitSkeleton { picks: vec![crate::types::SplitPick::Character(0)] }, p(x, 100.0));
    let prune  = g.add_node("NoFingers".into(), NodeType::PruneJoints { words: "thumb, index, middle, ring, pinky, end".into() }, p(x, 180.0));
    let trim   = g.add_node("Trim".into(), NodeType::TrimClip { head: 120, tail: 240 }, p(x, 260.0));
    let smooth = g.add_node("Smooth".into(), NodeType::SmoothClip { radius: 4, amount: 1.0, translations: true }, p(x, 340.0));
    let place  = g.add_node("InPlace".into(), NodeType::InPlace { keep_height: true, to_root: false }, p(x, 420.0));
    let floor  = g.add_node("Floor".into(), NodeType::FloorClip { height: 0.0 }, p(x, 500.0));
    let mirror = g.add_node("Mirror".into(), NodeType::MirrorClip, p(x, 580.0));
    let lp     = g.add_node("Loop".into(), NodeType::LoopClip { blend: 30 }, p(x, 660.0));
    for (a, b) in [(load, split), (split, prune), (prune, trim), (trim, smooth), (smooth, place), (place, floor), (floor, mirror), (mirror, lp)] {
        g.add_connection(a, 0, b, 0);
    }
    view(g, lp);
    g.selected_node  = Some(smooth);
    g.selected_nodes = vec![smooth];
    format!("Mocap tools: one node per step. Put the view flag on any of them to see the clip at that point.{}", missing_examples())
}

fn retarget(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let load  = g.add_node("Take".into(), NodeType::LoadFbx { path: examples::path(examples::TAKE_FBX), take: 0 }, p(40.0, 20.0));
    let split = g.add_node("OneCharacter".into(), NodeType::SplitSkeleton { picks: vec![crate::types::SplitPick::Character(0)] }, p(40.0, 110.0));
    let skel  = g.add_node("TargetSkeleton".into(), NodeType::TestClip { seconds: 1.0, fps_num: 30, fps_den: 1 }, p(300.0, 110.0));
    let ret   = g.add_node("Retarget".into(), NodeType::Retarget, p(170.0, 220.0));
    let skin  = g.add_node("ProxySkin".into(), NodeType::ProxySkin { thickness: 1.6 }, p(170.0, 320.0));
    g.add_connection(load, 0, split, 0);
    g.add_connection(split, 0, ret, 0);
    g.add_connection(skel, 0, ret, 1);
    g.add_connection(ret, 0, skin, 0);
    view(g, skin);
    g.selected_node  = Some(ret);
    g.selected_nodes = vec![ret];
    format!("Retarget: a captured performance on the 19-joint test skeleton, which rests with its arms down. Space plays.{}", missing_examples())
}

fn body_collide(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let take  = g.add_node("Take".into(), NodeType::LoadFbx { path: examples::path(examples::RAGDOLL_TAKE), take: 0 }, p(40.0, 20.0));
    // Characterize before Car: side by side above Body Collide, in the order of its inputs.
    let who   = g.add_node("Characterize".into(), NodeType::Characterize { picks: vec![] }, p(40.0, 85.0));
    let car   = g.add_node("Car".into(), NodeType::LoadFbx { path: examples::path(examples::RAGDOLL_SET), take: 0 }, p(300.0, 20.0));
    let body  = g.add_node("BodyCollide".into(), NodeType::Ragdoll { settings: Default::default(), view: Default::default(), limits: Default::default() }, p(170.0, 150.0));
    g.add_connection(take, 0, who, 0);
    g.add_connection(who, 0, body, 0);
    // The car's geometry: Load FBX's second output.
    g.add_connection(car, 1, body, 1);
    view(g, body);
    g.selected_node  = Some(body);
    g.selected_nodes = vec![body];
    format!("Body Collide: the actor walks through the car at frame 1341 and is seated by 1473. Scrub to frame 3000: hips on the seat, feet in the footwell. Bypass the node to see the capture. Display: Hulls shows the shapes that collide. Characterize shows which joint is which part.{}", missing_examples())
}

/// The obstacles of the Body Collide course: name, centre and size in
/// metres, turn about the vertical in degrees, the frames of the take it
/// meets, and the case it shows.
pub const COURSE: &[(&str, [f32; 3], [f32; 3], f32, (usize, usize), &str)] = &[
    ("kerb",    [1.56, 0.045,  0.45], [0.50, 0.09, 0.45],   0.0, (0, 45),    "standing on a kerb the capture did not have: knees bend, feet stay flat on it"),
    ("pole",    [2.03, 1.10,  -0.02], [0.05, 2.20, 0.05],   0.0, (40, 75),   "a thin pole the left arm swings into: the arm is carried round it"),
    ("wall",    [2.66, 1.00,  -0.55], [0.20, 2.00, 1.00],   0.0, (75, 100),  "a wall at a turn the right arm swings into: the arm is folded in, clear of the wall and of the body"),
    ("door",    [2.07, 1.00,  -1.25], [0.04, 2.00, 1.00], 113.0, (90, 125),  "a closed door the capture walks straight through: no way round it, so collisions fade out before it, he passes as captured, and they fade back in after, with nothing thrown"),
    ("seat",    [0.50, 0.20,  -1.15], [0.60, 0.40, 0.60],   0.0, (180, 600), "a block higher than the car seat in the capture: stood on, then sat on; feet and hips stay on top"),
    ("ledge",   [0.95, 0.66,  -1.20], [0.20, 0.08, 0.50],   0.0, (300, 600), "a ledge where the seated leg comes to rest: the leg lies on top of it instead of through it"),
];

/// Body Collide on a course of boxes, each one a case the solver handles,
/// met one after the other along the first 20 seconds of the car take.
fn body_collide_course(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let take = g.add_node("Take".into(), NodeType::LoadFbx { path: examples::path(examples::RAGDOLL_TAKE), take: 0 }, p(40.0, 20.0));
    // The walk, the climb and the first minutes seated: 600 frames.
    let trim = g.add_node("First20s".into(), NodeType::TrimClip { head: 0, tail: COURSE_TAIL }, p(40.0, 100.0));
    g.add_connection(take, 0, trim, 0);
    let mut set: Option<NodeId> = None;
    for (k, (name, centre, size, turn, _, _)) in COURSE.iter().enumerate() {
        let x = 300.0 + k as f32 * 150.0;
        let cube = g.add_node(name.to_string(), NodeType::CreateCube { size: 1.0, location: format!("/course/{name}") }, p(x, 20.0));
        let place = g.add_node(format!("Place_{name}"), transform(*centre, [0.0, turn.to_radians(), 0.0], *size), p(x, 100.0));
        g.add_connection(cube, 0, place, 0);
        set = Some(match set {
            None => place,
            Some(prev) => {
                let merge = g.add_node(format!("Course{k}"), NodeType::merge(), p(x, 180.0));
                g.add_connection(prev, 0, merge, 0);
                g.add_connection(place, 0, merge, 1);
                merge
            }
        });
    }
    let body = g.add_node("BodyCollide".into(), NodeType::Ragdoll { settings: Default::default(), view: Default::default(), limits: Default::default() }, p(170.0, 260.0));
    g.add_connection(trim, 0, body, 0);
    if let Some(set) = set { g.add_connection(set, 0, body, 1); }
    view(g, body);
    let cases: Vec<String> = COURSE.iter().map(|(n, _, _, _, (a, b), what)| format!("{n} ({}-{}): {what}", 1200 + a, 1200 + b)).collect();
    format!("Body Collide on an obstacle course. Bypass the node to see the capture go through the boxes; Display: Hulls shows the shapes that collide. Frame by frame: {}.{}", cases.join("; "), missing_examples())
}

/// Frames cut from the end of the car take for the course: 600 remain.
const COURSE_TAIL: u32 = 8000 - 600;

// ── FBX and characters ───────────────────────────────────────────────────────

fn fbx_studio(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let load = g.add_node("Studio".into(), NodeType::LoadFbx { path: examples::path(examples::STUDIO_FBX), take: 0 }, p(180.0, 20.0));
    finish(g, load, load);
    g.view_output = 1;
    format!("Load FBX: its Geometry output (the second) is viewed: a floor and a crate, with their materials. Click its view flag to go round its outputs. The properties list the meshes, materials, camera and light, and what is not read in full.{}", missing_examples())
}

/// Take, one character of it, the character file and a Retarget of the one
/// onto the other's rest pose. Returns (split, character, retarget).
fn take_onto_character(g: &mut NodeGraphState) -> (NodeId, NodeId, NodeId) {
    let take  = g.add_node("Take".into(), NodeType::LoadFbx { path: examples::path(examples::TAKE_FBX), take: 0 }, p(40.0, 20.0));
    let split = g.add_node("OneActor".into(), NodeType::SplitSkeleton { picks: vec![crate::types::SplitPick::Character(0)] }, p(40.0, 110.0));
    let who   = g.add_node("Character".into(), NodeType::LoadFbx { path: examples::path(examples::CHARACTER_FBX), take: 0 }, p(300.0, 20.0));
    let ret   = g.add_node("Retarget".into(), NodeType::Retarget, p(170.0, 220.0));
    g.add_connection(take, 0, split, 0);
    g.add_connection(split, 0, ret, 0);
    // The character in the pose its skin was bound in: its Rest output.
    g.add_connection(who, 2, ret, 1);
    (split, who, ret)
}

/// A Novasci clip (Mixamo skeleton) onto the 342-joint character.
fn novasci_onto_character(g: &mut NodeGraphState, clip: &str) -> NodeId {
    crate::graph_io::clear(g);
    let take = g.add_node("Clip".into(), NodeType::LoadFbx { path: examples::path(clip), take: 0 }, p(40.0, 20.0));
    let who  = g.add_node("Character".into(), NodeType::LoadFbx { path: examples::path(examples::CHARACTER_FBX), take: 0 }, p(300.0, 20.0));
    let ret  = g.add_node("Retarget".into(), NodeType::Retarget, p(170.0, 160.0));
    g.add_connection(take, 0, ret, 0);
    g.add_connection(who, 2, ret, 1);
    view(g, ret);
    ret
}

fn novasci_retarget(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    novasci_onto_character(g, examples::NOVASCI_CLIP);
    format!("A Novasci clip (CC0, 66 joints on the Mixamo skeleton, a T-pose on frame 1) retargeted onto the 342-joint character: the joints are matched by the part of the body they are, so the two conventions meet. Put the view flag on Clip to see the source. The clips in examples/mocap/novasci ship with the program; the rest of the pack is fetched when asked for.{}", missing_examples())
}

fn novasci_download(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    novasci_onto_character(g, "mocap/novasci/ThumbsUp1.fbx");
    format!("ThumbsUp1 is not in the repository: the program asked to fetch it from the pack's page, under CC0, into the examples folder. Once it is there, this graph reads it like any file; Not now leaves the Clip node pointing at a file that is missing.{}", missing_examples())
}

fn retarget_character(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let (split, _, ret) = take_onto_character(g);
    let reach = g.add_node("Reach".into(), NodeType::Reach { settings: Default::default() }, p(170.0, 320.0));
    g.add_connection(ret, 0, reach, 0);
    g.add_connection(split, 0, reach, 1);
    view(g, reach);
    g.selected_node = Some(reach);
    g.selected_nodes = vec![reach];
    format!("Retarget onto a character: one actor of the example take on a 342-joint character, read with its skin and bind pose from Load FBX's Rest output. Reach then puts the feet where the actor's are, scaled to the character, and the human pass keeps the knees right. Put the view flag on Retarget to compare. Space plays.{}", missing_examples())
}

fn capture_skin(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let (split, who, ret) = take_onto_character(g);
    let reach = g.add_node("Reach".into(), NodeType::Reach { settings: Default::default() }, p(170.0, 320.0));
    let cap   = g.add_node("Capture".into(), NodeType::Capture { falloff: 6.0 }, p(430.0, 220.0));
    let skin  = g.add_node("Skin".into(), NodeType::Skin, p(300.0, 420.0));
    g.add_connection(ret, 0, reach, 0);
    g.add_connection(split, 0, reach, 1);
    g.add_connection(who, 1, cap, 0);
    g.add_connection(who, 2, cap, 1);
    g.add_connection(cap, 0, skin, 0);
    g.add_connection(who, 2, skin, 1);
    g.add_connection(reach, 0, skin, 2);
    view(g, skin);
    g.selected_node = Some(skin);
    g.selected_nodes = vec![skin];
    format!("Capture and Skin: the character's mesh weighted again by proximity to its bones (Capture), then that skin put on the retargeted take by joint name (Skin). Bypass Capture to use the weights the file came with. Space plays.{}", missing_examples())
}

/// The whole mocap pipeline, per actor: a folder of raw takes, split, each
/// actor placed (a Transform, still at zero), retargeted onto a skinned
/// character in its bind pose, feet put where the
/// actor's are, kept out of the set and out of itself, and written as FBX
/// with the character's mesh and skin.
fn mocap_pipeline(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let takes = g.add_node("Takes".into(), NodeType::LoadFbxDir { dir: examples::path(examples::TAKES_DIR), index: 0, take: 0 }, p(300.0, 20.0));
    let split = g.add_node("Actors".into(), NodeType::SplitSkeleton { picks: vec![crate::types::SplitPick::Character(0), crate::types::SplitPick::Character(1)] }, p(300.0, 120.0));
    let who   = g.add_node("Character".into(), NodeType::LoadFbx { path: examples::path(examples::CHARACTER_FBX), take: 0 }, p(40.0, 20.0));
    let set   = g.add_node("Set".into(), NodeType::LoadFbx { path: examples::path(examples::STUDIO_FBX), take: 0 }, p(560.0, 20.0));
    let place_set = g.add_node("PlaceSet".into(), NodeType::Transform { translation: Vec3::ZERO, rotation: Vec3::ZERO, scale: Vec3::ONE }, p(700.0, 120.0));
    g.add_connection(takes, 0, split, 0);
    g.add_connection(set, 1, place_set, 0);
    let mut first_collide = None;
    for k in 0..2usize {
        let x = 120.0 + k as f32 * 400.0;
        let n = k + 1;
        let place = g.add_node(format!("Place{n}"), NodeType::Transform { translation: Vec3::ZERO, rotation: Vec3::ZERO, scale: Vec3::ONE }, p(x, 240.0));
        let ret   = g.add_node(format!("Retarget{n}"), NodeType::Retarget, p(x, 340.0));
        let reach = g.add_node(format!("Reach{n}"), NodeType::Reach { settings: Default::default() }, p(x, 440.0));
        let body  = g.add_node(format!("BodyCollide{n}"), NodeType::Ragdoll { settings: Default::default(), view: Default::default(), limits: Default::default() }, p(x, 540.0));
        let write = g.add_node(format!("Write{n}"), NodeType::WriteFbx { path: "{dir}/retargeted/{file}_{char}.fbx".into(), mesh: true }, p(x, 640.0));
        g.add_connection(split, k, place, 0);
        g.add_connection(place, 0, ret, 0);
        g.add_connection(who, 2, ret, 1);
        g.add_connection(ret, 0, reach, 0);
        // The feet go where the placed actor's are.
        g.add_connection(place, 0, reach, 1);
        g.add_connection(reach, 0, body, 0);
        g.add_connection(place_set, 0, body, 1);
        g.add_connection(body, 0, write, 0);
        first_collide.get_or_insert(body);
    }
    let body = first_collide.unwrap();
    view(g, body);
    g.selected_node = Some(body);
    g.selected_nodes = vec![body];
    format!("Mocap folder: each take in the Takes folder is split into its actors; each actor is placed in the set (Place1, Place2: Transform, at zero), retargeted onto Character (its Rest output: bind pose and skin), its feet put where the actor's are (Reach), kept out of Set and out of itself (Body Collide), and written with the character's mesh and skin to {{dir}}/retargeted. Set is a stand-in: point it at your own set, and move it with PlaceSet. Press Solve on a Body Collide node to see one take; Write FBX with All files solves and writes every take in the folder.{}", missing_examples())
}

fn human_limits(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let walk  = g.add_node("Walk".into(), NodeType::TestClip { seconds: 2.0, fps_num: 30, fps_den: 1 }, p(40.0, 20.0));
    let fix   = g.add_node("BendBack".into(), NodeType::FixPose { edits: vec![
        crate::core::anim::PoseEdit { joint: "Take01:LeftForeArm".into(), rotation: [80.0, 0.0, 0.0], translation: [0.0; 3] },
    ] }, p(40.0, 110.0));
    let lim   = g.add_node("HumanLimits".into(), NodeType::HumanLimits { limits: Default::default() }, p(170.0, 220.0));
    g.add_connection(walk, 0, fix, 0);
    g.add_connection(fix, 0, lim, 0);
    g.add_connection(walk, 0, lim, 1);
    view(g, lim);
    g.selected_node = Some(lim);
    g.selected_nodes = vec![lim];
    "Human Limits: Fix Pose bends the left elbow 80 degrees the wrong way; Human Limits, against the walk before it, bends it back the way a human elbow goes and keeps the hand where it was. Put the view flag on BendBack to see the broken arm.".into()
}

// ── Martino: composed USD stages and ICE ─────────────────────────────────────

/// One Load USD node on a stage example, viewed and selected.
fn stage(g: &mut NodeGraphState, name: &str, file: &str) -> NodeId {
    crate::graph_io::clear(g);
    let load = g.add_node(name.into(), NodeType::load_usd(examples::path(file)), p(180.0, 20.0));
    finish(g, load, load);
    load
}

const BOXES: &str = "The Scene Explorer lists the stage: each closed prim is drawn as its bounding box until it is opened, or tick Show all geometry.";

/// Two CC0 models from the USD Working Group's assets, merged.
fn open_assets(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let teapot = g.add_node("Teapot".into(), NodeType::load_usd(examples::path(examples::TEAPOT_USD)), p(40.0, 20.0));
    let cars   = g.add_node("CarKit".into(), NodeType::load_usd(examples::path(examples::CAR_KIT_USD)), p(300.0, 20.0));
    let place  = g.add_node("PlaceCars".into(), transform([1.5, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]), p(300.0, 120.0));
    let merge  = g.add_node("Merge".into(), NodeType::merge(), p(170.0, 220.0));
    g.add_connection(cars, 0, place, 0);
    g.add_connection(teapot, 0, merge, 0);
    g.add_connection(place, 0, merge, 1);
    finish(g, merge, merge);
    format!("Two models published under CC0 by the USD Working Group: the PolyHaven teapot (2,472 triangles, two materials with textures: Textured mode shows them) and the mini car kit (six vehicles with variants, read as the stage composes them). Both ship with the program, in examples/usd, with their licences beside them. {BOXES}{}", missing_examples())
}

fn open_bike(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    stage(g, "Bike", examples::BIKE_USDZ);
    format!("Carbon Frame Bike by Robert Schweier and prefrontal cortex, CC BY-SA 4.0: 331 meshes, 135,648 triangles, 31 materials, in a 27 MB usdz that is not in the repository. The program asked to fetch it, saying where from and under which licence; Not now leaves the node pointing at a file that is missing. {BOXES}{}", missing_examples())
}

fn open_elephant(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    stage(g, "Elephant", examples::ELEPHANT_USDC);
    format!("Vietnamese Elephant with Monochord from Songs of Cultures by A.MUSE, CC BY-SA 4.0: three meshes, two textured materials, fetched into the examples folder with its textures when the template opens. {BOXES}{}", missing_examples())
}

fn street(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    stage(g, "Street", crate::examples_stage::STREET);
    format!("A street from four files: the road and pavements from a sublayer, three tables referenced from table.usda, the shapes on them in the variant \"cafe\", and an override in the root layer that moves the south pavement. {BOXES}{}", missing_examples())
}

fn strands(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    stage(g, "Strands", crate::examples_stage::STRANDS);
    format!("Curves as USD has them: linear, Bezier, Catmull-Rom and a closed B-spline, 600 strands of grass, and points whose width grows along a spiral. {BOXES}{}", missing_examples())
}

fn forest(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let load = g.add_node("Forest".into(), NodeType::load_usd(examples::path(crate::examples_stage::FOREST)), p(180.0, 20.0));
    let turn = g.add_node("Turn".into(), transform([0.0; 3], [0.0, 0.35, 0.0], [1.0; 3]), p(180.0, 130.0));
    g.add_connection(load, 0, turn, 0);
    finish(g, turn, load);
    format!("4,900 trees and stones from one PointInstancer: three meshes, each drawn once per copy by GPU instancing. The Transform turns the whole forest by moving the placements; the shared meshes are not copied. {BOXES}{}", missing_examples())
}

fn hall(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    stage(g, "Hall", crate::examples_stage::TABLES);
    format!("64 tables, each an instanceable reference to table.usda: the five meshes of the table are read once and placed 64 times. {BOXES}{}", missing_examples())
}

fn lamps(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    stage(g, "Lamps", crate::examples_stage::LAMPS);
    format!("Each lamp has render geometry, a proxy of blocks and a guide showing its cone of light. With proxy on, the proxy is drawn in place of the render geometry; switch render, proxy and guide in the Scene Explorer. {BOXES}{}", missing_examples())
}

fn spherify(g: &mut NodeGraphState, store: &mut IceGraphStore) -> String {
    crate::graph_io::clear(g);
    let id = store.create_subnet("Spherify".into());
    if let Some(sub) = store.get_mut(id) {
        // Get P → Normalize → Multiply (by 1.2) → Set P → ICE Execution Ports.
        let root = sub.root().expect("every ICE graph has a root");
        let get = sub.add_node("P".into(), IceGraphNodeType::get("P"), p(80.0, 200.0));
        let norm = sub.add_node("Normalize".into(), IceGraphNodeType::Math { op: xms_ice::Math::Normalize }, p(220.0, 200.0));
        let grow = sub.add_node("Scale".into(), IceGraphNodeType::Math { op: xms_ice::Math::Multiply }, p(360.0, 200.0));
        let set = sub.add_node("Set P".into(), IceGraphNodeType::set("P"), p(500.0, 200.0));
        if let Some(n) = sub.nodes.iter_mut().find(|n| n.id == grow) { n.inputs[1].value = Some(xms_ice::Value::Float(1.2)); }
        sub.add_connection(get, 0, norm, 0);
        sub.add_connection(norm, 0, grow, 0);
        sub.add_connection(grow, 0, set, 0);
        sub.add_connection(set, 0, root, 0);
        if let Some(n) = sub.nodes.iter_mut().find(|n| n.id == root) { n.position = p(660.0, 200.0); }
    }
    let all = || PolySelection { source: SelSource::All, ..Default::default() };
    let cube = g.add_node("Cube".into(), NodeType::cube(1.0), p(180.0, 20.0));
    let dense = g.add_node("Subdivide".into(), edit_poly(vec![PolyOp::new(all(), PolyOpKind::Subdivide { iterations: 3 })], all()), p(180.0, 130.0));
    let tree = g.add_node("Spherify".into(), NodeType::Ice { id, name: "Spherify".into() }, p(180.0, 240.0));
    g.add_connection(cube, 0, dense, 0);
    g.add_connection(dense, 0, tree, 0);
    finish(g, tree, tree);
    "ICE graph: every point of the subdivided cube is set to unit length (Normalize), then scaled by 1.2 (Multiply), and written back as P (Set Data, run by the ICEExectPorts node). Select the Spherify node and press the down arrow to go inside; the up arrow comes back. Change Multiply's B in its parameters.".into()
}

fn mocap_example(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::mocap_split_template(g);
    let dir = examples::path(examples::TAKES_DIR);
    for n in &mut g.nodes {
        if let NodeType::LoadFbxDir { dir: d, .. } = &mut n.node_type { *d = dir.clone(); }
    }
    format!("Mocap split on the example take: two characters, split into one clip each.{}", missing_examples())
}

fn mocap_blank(g: &mut NodeGraphState, _: &mut IceGraphStore) -> String {
    crate::graph_io::mocap_split_template(g);
    "Template loaded. Select the Takes node and choose a folder.".into()
}

#[cfg(test)]
mod tests {

    /// How deep the skin goes into each box of the course, in the capture
    /// and after Body Collide, and over which frames: (frames touched, worst depth).
    fn course_depths(clip: &crate::core::anim::AnimData) -> Vec<(usize, f32, usize)> {
        let skin = clip.skin.as_ref().unwrap();
        COURSE.iter().map(|(_, c, s, turn, _, _)| {
            let (c, h) = (Vec3::from_array(*c), Vec3::from_array(*s) * 0.5);
            let back = glam::Quat::from_rotation_y(-turn.to_radians());
            let (mut frames, mut worst, mut worst_at) = (0, 0.0f32, 0);
            let head = crate::core::human::Human::of(clip).get(crate::core::human::Slot::Head);
            for f in 0..clip.frames {
                let w = clip.world_pose(f);
                let (mut pos, _) = skin.deformed(&w);
                // The body mesh has no head: its crown, about 12 cm above the head joint.
                if let Some(j) = head { pos.push(w[j].transform_point3(Vec3::ZERO) + Vec3::Y * 0.12); }
                let mut deepest = 0.0f32;
                for p in pos.iter().rev().step_by(3) {
                    let d = (back * (*p - c)).abs();
                    if d.x < h.x && d.y < h.y && d.z < h.z { deepest = deepest.max((h - d).min_element()); }
                }
                if deepest > 0.005 { frames += 1; }
                if deepest > worst { worst = deepest; worst_at = f; }
            }
            (frames, worst, worst_at)
        }).collect()
    }

    /// The course is built from its table, and finds the result shipped
    /// with the examples: it opens solved.
    #[test]
    fn the_course_opens_solved() {
        let t = TEMPLATES.iter().find(|t| t.name.ends_with("obstacle course")).unwrap();
        let (mut g, mut store) = (NodeGraphState::default(), IceGraphStore::default());
        (t.build)(&mut g, &mut store);
        let cubes = g.nodes.iter().filter(|n| matches!(n.node_type, NodeType::CreateCube { .. })).count();
        assert_eq!(cubes, COURSE.len());
        if examples::existing(examples::RAGDOLL_TAKE).is_none() { return; }
        let body = g.nodes.iter().find(|n| matches!(n.node_type, NodeType::Ragdoll { .. })).unwrap().id;
        let (clip, set) = g.ragdoll_inputs(body);
        let (clip, set) = (clip.unwrap(), set.unwrap());
        assert_eq!(clip.frames, 600);
        assert_eq!(set.indices.len() / 3, 12 * COURSE.len());
        let key = crate::ragdoll::key(&clip, Some(&set), &Default::default());
        assert!(crate::ragdoll::solved(key).is_some(), "no shipped result for {key:016x}");
    }

    /// Solves the course, prints what each box does, and with XMS_SHIP set
    /// puts the result with the examples:
    /// `cargo test --release course -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn solve_the_course() {
        let t = TEMPLATES.iter().find(|t| t.name.ends_with("obstacle course")).unwrap();
        let (mut g, mut store) = (NodeGraphState::default(), IceGraphStore::default());
        (t.build)(&mut g, &mut store);
        let body = g.nodes.iter().find(|n| matches!(n.node_type, NodeType::Ragdoll { .. })).unwrap().id;
        let (clip, set) = g.ragdoll_inputs(body);
        let (clip, set) = (clip.unwrap(), set.unwrap());
        println!("clip {} frames, set {} triangles", clip.frames, set.indices.len() / 3);
        let t0 = std::time::Instant::now();
        crate::ragdoll::solve_now(clip.clone(), Some(set.clone()), Default::default()).unwrap();
        let key = crate::ragdoll::key(&clip, Some(&set), &Default::default());
        let solved = crate::ragdoll::solved(key).unwrap();
        println!("solved in {:.0} s: {:?}", t0.elapsed().as_secs_f32(), solved.report);
        let out = g.eval_anim(body).unwrap();
        if std::env::var("XMS_PROBE").is_ok() {
            let skin = clip.skin.as_ref().unwrap();
            for f in [0usize, 120, 125, 130, 135, 140] {
                let (pos, _) = skin.deformed(&clip.world_pose(f));
                let top = pos.iter().fold(Vec3::splat(f32::MIN), |a, p| if p.y > a.y { *p } else { a });
                println!("PROBE f{f} top of head {top:?}");
            }
        }
        let (before, after) = (course_depths(&clip), course_depths(&out));
        for (k, (name, _, _, _, range, _)) in COURSE.iter().enumerate() {
            println!("{name:<8} {range:?}  capture: {} frames, {:.1} cm deep at {}  solved: {} frames, {:.1} cm deep at {}",
                before[k].0, before[k].1 * 100.0, before[k].2, after[k].0, after[k].1 * 100.0, after[k].2);
        }
        // Poses for pictures: the capture and the result at the deepest frame
        // of the capture in each box, as raw little-endian floats.
        if let Ok(dir) = std::env::var("XMS_DUMP") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            let skin = clip.skin.as_ref().unwrap();
            let faces: Vec<u8> = skin.faces.iter().flat_map(|f| {
                let tri = crate::core::anim::SkinMesh::is_tri(f);
                let mut v = vec![f[0], f[1], f[2]];
                if !tri { v.extend([f[0], f[2], f[3]]); }
                v
            }).flat_map(|i| (i as u32).to_le_bytes()).collect();
            std::fs::write(dir.join("faces.bin"), faces).unwrap();
            let head = crate::core::human::Human::of(&clip).get(crate::core::human::Slot::Head);
            let mut index = String::new();
            for f in (0..clip.frames).step_by(5) {
                for (tag, c) in [("capture", &clip), ("solved", &out)] {
                    let w = c.world_pose(f);
                    let (pos, _) = c.skin.as_ref().unwrap().deformed(&w);
                    let bytes: Vec<u8> = pos.iter().flat_map(|p| p.to_array()).flat_map(|x| x.to_le_bytes()).collect();
                    std::fs::write(dir.join(format!("{tag}_{f}.bin")), bytes).unwrap();
                    let h = head.map(|j| w[j].transform_point3(Vec3::ZERO)).unwrap_or_default();
                    index.push_str(&format!("{tag} {f} {} {} {}\n", h.x, h.y, h.z));
                }
            }
            std::fs::write(dir.join("index.txt"), index).unwrap();
        }
        if std::env::var("XMS_SHIP").is_ok() {
            let to = examples::dir().unwrap().join(examples::RAGDOLL_SOLVED);
            std::fs::copy(crate::ragdoll::file_of(key).unwrap(), to.join(format!("{key:016x}.rag"))).unwrap();
        }
    }

    use super::*;
    use crate::types::IceGraphId;

    /// The mocap folder template, run as a batch on a copy of the example
    /// take: every actor solved and written with the character's skin.
    /// Takes minutes: `cargo test --release pipeline_template -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn pipeline_template_writes_every_actor() {
        let Some(take) = examples::existing(examples::TAKE_FBX) else { return };
        let dir = std::env::temp_dir().join("xms_pipeline_template");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&take, dir.join("take.fbx")).unwrap();
        let t = TEMPLATES.iter().find(|t| t.name.starts_with("Mocap folder")).unwrap();
        let (mut g, mut store) = (NodeGraphState::default(), IceGraphStore::default());
        (t.build)(&mut g, &mut store);
        for n in &mut g.nodes {
            if let NodeType::LoadFbxDir { dir: d, .. } = &mut n.node_type { *d = dir.to_string_lossy().to_string(); }
        }
        let targets = crate::batch::write_nodes(&g);
        let progress = std::sync::Mutex::new(crate::batch::Progress::default());
        let t0 = std::time::Instant::now();
        crate::batch::run(&g, &targets, true, &progress);
        let p = progress.lock().unwrap();
        println!("{:#?}\n{:?}", p.log, t0.elapsed());
        assert_eq!((p.written, p.failed), (2, 0));
        for actor in ["Skeleton_001", "Skeleton_002"] {
            let out = dir.join("retargeted").join(format!("take_{actor}.fbx"));
            let a = crate::fbx_loader::load_fbx(out.to_str().unwrap(), 0).unwrap().anim;
            assert_eq!(a.joints.len(), 342, "{actor}");
            assert!(a.skin.as_ref().map(|s| s.positions.len()).unwrap_or(0) > 30_000, "{actor}: skinned");
            assert_eq!(a.frames, 728);
        }
    }

    /// The DeLorean render: the car, a camera and two lights merged, then
    /// the render chain in order, viewed at Render Settings; its dome drawn
    /// a metre across and lit by the sky in the examples.
    #[test]
    fn the_delorean_render_is_a_chain_to_husk() {
        let t = TEMPLATES.iter().find(|t| t.by == XMS).unwrap();
        let (mut g, mut store) = (NodeGraphState::default(), IceGraphStore::default());
        (t.build)(&mut g, &mut store);
        let lights: Vec<_> = g.nodes.iter().filter_map(|n| match &n.node_type { NodeType::Light { light, .. } => Some(light.clone()), _ => None }).collect();
        assert_eq!(lights.len(), 2);
        let dome = lights.iter().find(|l| l.kind == crate::core::light::LightType::Dome).unwrap();
        assert_eq!(dome.float("guideRadius"), 1.0);
        assert!(matches!(dome.value("inputs:texture:file"), Some(crate::core::properties::Value::Asset(f)) if f.ends_with("sky.hdr")));
        let id = |f: fn(&NodeType) -> bool| g.nodes.iter().find(|n| f(&n.node_type)).unwrap().id;
        let (scene, settings, write, render) = (id(|t| matches!(t, NodeType::RenderSceneSettings { .. })), id(|t| matches!(t, NodeType::RenderSettings { .. })),
            id(|t| matches!(t, NodeType::WriteUsd { .. })), id(|t| matches!(t, NodeType::RenderCommand { .. })));
        let above = |n: NodeId| g.nodes.iter().find(|x| x.id == n).unwrap().inputs[0].connected_output.map(|(a, _)| a);
        assert_eq!((above(settings), above(write), above(render)), (Some(scene), Some(settings), Some(write)));
        assert_eq!(g.view_flag, Some(settings));
        if examples::existing(examples::SKY_HDR).is_some() {
            let bytes = std::fs::read(examples::path(examples::SKY_HDR)).unwrap();
            assert!(bytes.starts_with(b"#?RADIANCE"));
        }
    }

    #[test]
    fn every_template_is_laid_out_in_clear_rows() {
        use crate::node_graph::ui::{NODE_HEIGHT, NODE_WIDTH};
        for t in TEMPLATES {
            let mut g = NodeGraphState::default();
            t.load(&mut g, &mut IceGraphStore::default());
            assert!(g.frame_request, "{}", t.name);
            // No node on top of another.
            for (i, a) in g.nodes.iter().enumerate() {
                for b in &g.nodes[..i] {
                    let apart = (a.position.x - b.position.x).abs() >= NODE_WIDTH + 20.0
                        || (a.position.y - b.position.y).abs() >= NODE_HEIGHT + 20.0;
                    assert!(apart, "{}: {} overlaps {}", t.name, a.name, b.name);
                }
            }
            // Every wire runs downwards.
            let at = |id| g.nodes.iter().find(|n| n.id == id).unwrap().position;
            for c in &g.connections {
                assert!(at(c.to_node).y > at(c.from_node).y, "{}: a wire runs upwards", t.name);
            }
        }
    }

    #[test]
    fn a_chain_lays_out_as_a_straight_line() {
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), NodeType::cube(1.0), p(500.0, 300.0));
        let b = g.add_node("B".into(), transform([0.0; 3], [0.0; 3], [1.0; 3]), p(-80.0, 10.0));
        let c = g.add_node("C".into(), transform([0.0; 3], [0.0; 3], [1.0; 3]), p(200.0, 40.0));
        g.add_connection(a, 0, b, 0);
        g.add_connection(b, 0, c, 0);
        g.auto_layout();
        let x: Vec<f32> = [a, b, c].iter().map(|id| g.nodes.iter().find(|n| n.id == *id).unwrap().position.x).collect();
        assert!(x.iter().all(|v| (v - x[0]).abs() < 0.5), "{x:?}");
    }

    #[test]
    fn every_template_builds_something_to_look_at() {
        // The file-based templates need the examples folder of the repository.
        assert!(examples::dir().is_some(), "examples folder not found");

        for t in TEMPLATES {
            assert!(MENU.iter().any(|(by, groups)| *by == t.by && groups.contains(&t.group)), "{}", t.name);
            let mut g = NodeGraphState::default();
            let mut store = IceGraphStore::default();
            // Twice: a template has to replace whatever was there.
            (t.build)(&mut g, &mut store);
            let message = (t.build)(&mut g, &mut store);
            assert!(!message.is_empty() && !message.contains("not found"), "{}: {message}", t.name);
            // One operator viewed, and it is there.
            assert!(g.view_flag.is_some_and(|v| g.nodes.iter().any(|n| n.id == v)), "{}", t.name);

            let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| -> crate::core::geo::Geo {
                store.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
            };
            // Every node cooks: a clip or a mesh with vertices.
            for n in &g.nodes {
                // The blank mocap template has no folder yet.
                if t.name == "Mocap split" { break; }
                // A material is not geometry: Material Binding puts it on some.
                // Nor is a camera or a light: a primitive with no geometry.
                if matches!(n.node_type, NodeType::WriteFbx { .. } | NodeType::Material { .. } | NodeType::MaterialX { .. } | NodeType::Camera { .. } | NodeType::Light { .. }) { continue; }
                let clip = (0..n.outputs.len().max(1)).any(|o| g.eval_anim_out(n.id, o).is_some());
                let mesh = (0..n.outputs.len().max(1)).map(|o| g.eval_node_out(n.id, o, &mut std::collections::HashMap::new(), &eval)
                    .map(|r| r.into_mesh()).map(|m| m.vertices.len() + m.points.len()).unwrap_or(0)).max().unwrap_or(0);
                assert!(clip || mesh > 0, "{}: node {} gives nothing", t.name, n.name);
            }
            // The scene at the view flag is something to look at.
            let v = g.view_flag.unwrap();
            let scene = g.eval_node_out(v, g.viewed_output(), &mut std::collections::HashMap::new(), &eval)
                .map(|r| match r { crate::types::EvalResult::Anim(_) => 1, r => { let m = r.into_mesh(); m.vertices.len() + m.points.len() } });
            assert!(t.name == "Mocap split" || scene.unwrap_or(0) > 0, "{}: the viewed operator gives nothing", t.name);
            assert!(g.selected_node.is_some(), "{}", t.name);
        }
    }

    #[test]
    fn uv_templates_give_layouts_in_the_unit_square() {
        for name in ["Unwrap the sea mine", "Edit UV islands"] {
            let t = TEMPLATES.iter().find(|t| t.name == name).unwrap();
            let mut g = NodeGraphState::default();
            (t.build)(&mut g, &mut IceGraphStore::default());
            let node = g.selected_node.unwrap();
            let eval = |_: IceGraphId, mesh: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| mesh.clone();
            let mesh = g.eval_node(node, &mut std::collections::HashMap::new(), &eval).unwrap().into_mesh();
            assert_eq!(mesh.uvs.len(), mesh.indices.len(), "{name}");
            assert!(mesh.uvs.iter().all(|uv| uv[0].is_finite() && uv[1].is_finite()), "{name}");
            let (_, islands) = crate::core::uv::islands(&mesh);
            if name == "Unwrap the sea mine" {
                assert!(mesh.uvs.iter().all(|uv| uv[0] > -1e-3 && uv[0] < 1.001 && uv[1] > -1e-3 && uv[1] < 1.001));
                assert!(islands > 20 && islands < 2000, "{islands} islands");
                assert!(crate::core::uv::coverage(&mesh) > 0.2, "{}", crate::core::uv::coverage(&mesh));
            } else {
                assert_eq!(islands, 6);
            }
        }
    }

    #[test]
    fn the_ice_template_puts_every_point_on_a_sphere() {
        let t = TEMPLATES.iter().find(|t| t.name == "ICE graph: spherify").unwrap();
        let (mut g, mut store) = (NodeGraphState::default(), IceGraphStore::default());
        (t.build)(&mut g, &mut store);
        let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| -> crate::core::geo::Geo {
            store.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
        };
        let mesh = g.eval_node(g.selected_node.unwrap(), &mut std::collections::HashMap::new(), &eval).unwrap().into_mesh();
        assert!(mesh.vertices.len() > 300 && !mesh.indices.is_empty());
        for v in &mesh.vertices {
            let r = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((r - 1.2).abs() < 1e-4, "{r}");
        }
    }

    /// Added to a scene that has nodes already, the ICE template keeps its
    /// tree: its copy of the tree gives the same sphere.
    #[test]
    fn the_ice_template_added_to_a_scene_still_spherifies() {
        let t = TEMPLATES.iter().find(|t| t.name == "ICE graph: spherify").unwrap();
        let mut store = IceGraphStore::default();
        let mut other = NodeGraphState::default();
        t.load(&mut other, &mut store);
        let mut g = NodeGraphState::default();
        g.add_node("Cube".into(), NodeType::cube(1.0), p(0.0, 0.0));
        let added = crate::graph_io::add_graph(&mut g, &mut store, &other);
        assert_eq!(added, other.nodes.len());
        for n in &other.nodes {
            if let NodeType::Ice { id, .. } = n.node_type { store.subnets.remove(&id); }
        }
        let ice = g.nodes.iter().find(|n| matches!(n.node_type, NodeType::Ice { .. })).unwrap().id;
        let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: Option<&crate::core::geo::Geo>| -> crate::core::geo::Geo {
            store.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
        };
        let mesh = g.eval_node(ice, &mut std::collections::HashMap::new(), &eval).unwrap().into_mesh();
        for v in &mesh.vertices {
            let r = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((r - 1.2).abs() < 1e-4, "{r}");
        }
    }

    #[test]
    fn modelling_templates_give_valid_meshes() {
        for (name, verts_at_least) in [("Edit Poly: tower", 30), ("Edit Poly: panels", 300), ("Edit Poly: goblet", 300), ("Edit Poly: bridge", 20), ("Edit Poly: sea mine", 8000), ("Edit Poly: bolt", 60)] {
            let t = TEMPLATES.iter().find(|t| t.name == name).unwrap();
            let mut g = NodeGraphState::default();
            (t.build)(&mut g, &mut IceGraphStore::default());
            let ep = g.nodes.iter().find(|n| matches!(n.node_type, NodeType::EditPoly { .. })).unwrap().id;
            let eval = |_: IceGraphId, mesh: &crate::core::geo::Geo, _: Option<&crate::core::geo::Geo>| mesh.clone();
            let mesh = g.eval_node(ep, &mut std::collections::HashMap::new(), &eval).unwrap().into_mesh();
            let poly = PolyMesh::from_mesh(&mesh);
            assert!(poly.verts.len() >= verts_at_least, "{name}: {} vertices", poly.verts.len());
            for q in &poly.polys {
                assert!(q.len() >= 3);
                assert_eq!(q.iter().collect::<std::collections::HashSet<_>>().len(), q.len(), "{name}");
            }
            if name != "Edit Poly: panels" { assert!(poly.is_closed(), "{name} is open"); }
            if name == "Edit Poly: goblet" {
                // It stands upright: taller than wide, centred on the axis.
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for v in &poly.verts { lo = lo.min(*v); hi = hi.max(*v); }
                assert!(hi.y - lo.y > 1.8 && hi.y - lo.y > hi.x - lo.x, "{lo} {hi}");
                assert!((lo.x + hi.x).abs() < 0.05 && (lo.z + hi.z).abs() < 0.05, "{lo} {hi}");
            }
        }
    }
}
