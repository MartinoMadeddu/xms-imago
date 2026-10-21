use crate::core::anim::AnimData;
use crate::types::{array_text, InspectorTable, MeshData, PrimInspectorState, PrimInspectorTab, PrimVarInterp};
use crate::node_graph::NodeGraphState;

const ROW_H:      f32 = 20.0;
const IDX_COL_W:  f32 = 40.0;
const DATA_COL_W: f32 = 90.0;
const LABEL_COL_W: f32 = 230.0;
const HEADER_H:   f32 = 24.0;
const MAX_ROWS:   usize = 200; // max rows rendered at once (virtual scroll)
/// Width of one character of a 10 pt monospace array cell, and the widest an
/// array column grows to before its longer cells are cut off.
const ARRAY_CHAR_W:    f32 = 6.2;
const ARRAY_COL_MAX_W: f32 = 420.0;

mod xsi {
    // Light-theme colours. `theme::c` returns the dark counterpart in dark mode.
    #![allow(non_snake_case)]
    use egui::Color32;
    pub fn BG() -> Color32 { crate::theme::c(72, 72, 72) }
    pub fn HEADER_BG() -> Color32 { crate::theme::c(58, 58, 58) }
    pub fn ROW_EVEN() -> Color32 { crate::theme::c(72, 72, 72) }
    pub fn ROW_ODD() -> Color32 { crate::theme::c(66, 66, 66) }
    pub fn BORDER() -> Color32 { crate::theme::c(50, 50, 50) }
    pub fn TEXT() -> Color32 { crate::theme::c(228, 228, 228) }
    pub fn TEXT_DIM() -> Color32 { crate::theme::c(210, 210, 210) }
    /// Row numbers: a muted blue. On the light greys of Light, a dark navy,
    /// so they read against the rows.
    pub fn TEXT_IDX() -> Color32 {
        if crate::theme::is_dark() { crate::theme::c(120, 130, 145) } else { Color32::from_rgb(28, 52, 104) }
    }
    pub fn TAB_ACTIVE() -> Color32 { crate::theme::c(80, 95, 115) }
    pub fn TAB_BG() -> Color32 { crate::theme::c(58, 58, 58) }
    pub fn BREADCRUMB() -> Color32 { crate::theme::c(160, 165, 170) }
    pub fn PATH_BG() -> Color32 { crate::theme::c(45, 45, 45) }
}

pub fn draw_prim_inspector(
    ui:       &mut egui::Ui,
    graph:    &NodeGraphState,
    state:    &mut PrimInspectorState,
    // The operator with the view flag, and the row selected in the Scene
    // Explorer: whose scene is shown, and which part of it.
    source:   super::Source,
    // Graph revision: the data is cooked again only when this or the
    // source changes, not on every frame.
    revision: u64,
    // Playhead, for the pose a clip is listed in.
    time:     f64,
    get_parts: &dyn Fn(&NodeGraphState) -> Option<Vec<MeshData>>,
    // The counts of that geometry, without making it.
    get_summary: &dyn Fn(&NodeGraphState) -> crate::prim_inspector::Summary,
    get_clip: &dyn Fn(&NodeGraphState) -> Option<std::sync::Arc<AnimData>>,
    // The Properties tab: the selected prims themselves.
    get_props: &dyn Fn(&NodeGraphState) -> Vec<super::PropRow>,
) -> Option<crate::types::NodeId> {
    // The operator a Properties row's "Go to Operator" asked for.
    let mut go_to = None;
    egui::Frame::NONE
        .fill(xsi::BG())
        .show(ui, |ui| {
            let paths = &source.paths;
            if !state.is_cache_valid(source.node, paths, revision) {
                let clip = get_clip(graph);
                let summary = if clip.is_none() { get_summary(graph) } else { Default::default() };
                let props = if clip.is_none() { get_props(graph) } else { vec![] };
                state.update_cache(source.node, paths, summary, clip, props, revision);
            }
            // The geometry is made only for a geometry tab: on Properties a
            // big scene costs nothing more to show.
            use PrimInspectorTab as Tab;
            let geometry_tab = matches!(state.active_tab, Tab::Vertex | Tab::Edge | Tab::Uniform | Tab::FaceVarying | Tab::Constant);
            if geometry_tab && state.summary().any() && !state.parts_loaded() && state.cached_clip().is_none() {
                state.set_parts(get_parts(graph).unwrap_or_default());
            }

            // ── Path breadcrumb ───────────────────────────────────────────────
            // The selected prim (or all of the scene), and the operator whose
            // scene it is.
            let path_str = match source.node.and_then(|id| graph.nodes.iter().find(|n| n.id == id)) {
                Some(n) => {
                    let what = match paths.as_slice() { [] => "/".to_string(), [one] => one.clone(), many => format!("{} prims", many.len()) };
                    format!("{what}   at {}", n.name)
                }
                None => String::new(),
            };

            egui::Frame::NONE
                .fill(xsi::PATH_BG())
                .inner_margin(egui::vec2(8.0, 4.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(
                        egui::RichText::new(&path_str)
                            .color(xsi::BREADCRUMB())
                            .monospace()
                            .size(10.0),
                    );
                });

            let clip = state.cached_clip();
            let summary = state.summary();
            let mesh = state.cached_parts().filter(|parts| parts.iter().any(|m| m.point_count() > 0));
            let props = state.cached_props();
            if clip.is_none() && !summary.any() && props.is_empty() {
                ui.add_space(12.0);
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("No mesh or clip in the scene at the view flag.")
                        .color(xsi::TEXT_DIM()));
                });
                return;
            }

            // ── Tabs ─────────────────────────────────────────────────────────
            // A clip lists joints and bones; a mesh lists its components.
            let clip_tab = matches!(state.active_tab, Tab::Joint | Tab::Bone);
            if clip.is_some() && !clip_tab { state.set_active_tab(Tab::Joint); }
            if clip.is_none() && clip_tab { state.set_active_tab(Tab::Properties); }
            // A group, a camera or a light alone: only its properties.
            if clip.is_none() && !summary.any() { state.set_active_tab(Tab::Properties); }
            // Geometry with no properties listed: its points.
            if clip.is_none() && summary.any() && props.is_empty() && state.active_tab == Tab::Properties { state.set_active_tab(Tab::Vertex); }

            egui::Frame::NONE
                .fill(xsi::TAB_BG())
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if let Some(c) = &clip {
                            tab_btn(ui, state, Tab::Joint, &format!("Joint ({})", c.joints.len()));
                            tab_btn(ui, state, Tab::Bone, &format!("Bone ({})", bones(c).len()));
                        } else if summary.any() {
                            // Edges and Object rows are counted once the
                            // geometry is made (a geometry tab opened).
                            let loaded = |n: usize| if state.parts_loaded() { format!(" ({n})") } else { String::new() };
                            let (edges, objects) = (loaded(state.edge_count), loaded(mesh.as_deref().map_or(0, |p| object_rows(p))));
                            tab_btn(ui, state, Tab::Vertex, &format!("Point ({})", summary.points));
                            tab_btn(ui, state, Tab::Edge, &format!("Edge{edges}"));
                            tab_btn(ui, state, Tab::Uniform, &format!("Face ({})", summary.faces));
                            tab_btn(ui, state, Tab::FaceVarying, &format!("Corner ({})", summary.corners));
                            tab_btn(ui, state, Tab::Constant, &format!("Object{objects}"));
                        }
                        if clip.is_none() && !props.is_empty() {
                            let prims = props.iter().fold(Vec::<&str>::new(), |mut v, r| { if v.last() != Some(&r.path.as_str()) { v.push(&r.path); } v }).len();
                            tab_btn(ui, state, Tab::Properties, &format!("Properties ({prims})"));
                        }
                    });
                });

            ui.separator();

            // ── Spreadsheet ───────────────────────────────────────────────────
            // The open tab is built once and kept until the data, the tab or
            // (for a clip) the frame changes. Each frame draws the visible rows.
            let tab = state.active_tab.clone();
            if tab == Tab::Properties {
                go_to = draw_properties(ui, &props);
                return;
            }
            // A geometry tab just opened: its geometry is made next frame.
            if clip.is_none() && !state.parts_loaded() {
                ui.ctx().request_repaint();
                ui.add_space(12.0);
                ui.label(egui::RichText::new("Reading the geometry...").color(xsi::TEXT_DIM()));
                return;
            }
            let frame = clip.as_ref().map(|c| c.index_at(time)).unwrap_or(0);
            if !state.table_is_for(&tab, frame) {
                let table = match (&clip, &mesh) {
                    (Some(c), _) => clip_table(c, &tab, frame),
                    (None, Some(parts)) => parts_table(parts, &tab),
                    _ => InspectorTable::default(),
                };
                state.set_table(tab.clone(), frame, table);
            }
            let table = state.take_table();
            if table.cols.is_empty() {
                ui.add_space(12.0);
                let what = match tab {
                    Tab::FaceVarying => "No Corner attributes. A UV Unwrap node adds UVs.",
                    Tab::Constant    => "No Object attributes.",
                    _                => "No data.",
                };
                ui.label(egui::RichText::new(what).color(xsi::TEXT_DIM()));
            } else {
                draw_spreadsheet(ui, state, &table);
            }
            state.put_table(table);
        });
    go_to
}

/// The Properties tab: one row per property of each prim listed, its value,
/// and where the value comes from. Several prims: a Path column first.
/// A row is selected by clicking it; right-click it for "Go to Operator",
/// which gives the operator the value comes from.
fn draw_properties(ui: &mut egui::Ui, rows: &[super::PropRow]) -> Option<crate::types::NodeId> {
    // Rows are painted, not laid out as widgets: each column is as wide as
    // its widest cell, and hovering or selecting a row changes its colour
    // only, never a size, so no column edge moves.
    const ROW: f32 = 18.0;
    const PAD: f32 = 8.0;
    let font = egui::FontId::monospace(10.5);
    let several = rows.first().is_some_and(|f| rows.iter().any(|r| r.path != f.path));
    let sel_id = egui::Id::new("prim_inspector_props_selected");
    let mut selected: Option<usize> = ui.data(|d| d.get_temp::<Option<usize>>(sel_id)).flatten();
    let mut go_to = None;

    // The cells: the path once per prim, then name, value and where from.
    let mut heads: Vec<&str> = if several { vec!["Path"] } else { vec![] };
    heads.extend(["Name", "Value", "From"]);
    let mut last = "";
    let cells: Vec<Vec<(String, egui::Color32)>> = rows.iter().map(|r| {
        let mut c = vec![];
        if several {
            c.push((if r.path != last { r.path.clone() } else { String::new() }, xsi::TEXT_DIM()));
            last = r.path.as_str();
        }
        c.push((r.name.clone(), xsi::TEXT()));
        c.push((r.value.clone(), xsi::TEXT()));
        c.push((r.from.clone(), xsi::TEXT_DIM()));
        c
    }).collect();
    let measure = |t: &str| ui.painter().layout_no_wrap(t.to_string(), font.clone(), egui::Color32::WHITE).size().x;
    let mut widths: Vec<f32> = heads.iter().map(|h| measure(h)).collect();
    for row in &cells {
        for (w, (text, _)) in widths.iter_mut().zip(row) { *w = w.max(measure(text)); }
    }
    let widths: Vec<f32> = widths.into_iter().map(|w| w + 2.0 * PAD).collect();
    let total: f32 = widths.iter().sum();

    egui::ScrollArea::both().id_salt("prim_inspector_props").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let w = total.max(ui.available_width());
        let line = |painter: &egui::Painter, rect: egui::Rect, texts: &[(String, egui::Color32)]| {
            let mut x = rect.left();
            for ((text, colour), cw) in texts.iter().zip(&widths) {
                painter.text(egui::pos2(x + PAD, rect.center().y), egui::Align2::LEFT_CENTER, text, font.clone(), *colour);
                x += cw;
            }
        };
        // Header.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, ROW + 4.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, xsi::HEADER_BG());
        let head_cells: Vec<(String, egui::Color32)> = heads.iter().map(|h| (h.to_string(), xsi::TEXT_IDX())).collect();
        line(ui.painter(), rect, &head_cells);
        // Rows.
        for (k, (r, texts)) in rows.iter().zip(&cells).enumerate() {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ROW), egui::Sense::click());
            let bg = if selected == Some(k) { xsi::TAB_ACTIVE() }
                else if resp.hovered() { xsi::HEADER_BG() }
                else if k % 2 == 0 { xsi::ROW_EVEN() } else { xsi::ROW_ODD() };
            ui.painter().rect_filled(rect, 0.0, bg);
            line(ui.painter(), rect, texts);
            if resp.clicked() || resp.secondary_clicked() { selected = Some(k); }
            resp.context_menu(|ui| {
                let tip = match r.node {
                    Some(_) => format!("Select the operator the value comes from ({})", r.from),
                    None => "No operator to go to: the value was not made by one".to_string(),
                };
                if ui.add_enabled(r.node.is_some(), egui::Button::new("Go to Operator")).on_hover_text(tip).clicked() {
                    go_to = r.node;
                    ui.close();
                }
            });
        }
    });
    ui.data_mut(|d| d.insert_temp(sel_id, selected));
    go_to
}

/// Bones of a skeleton: one from each joint to each bone joint below it.
pub fn bones(clip: &AnimData) -> Vec<(usize, usize)> {
    clip.joints.iter().enumerate()
        .filter(|(_, j)| j.is_bone)
        .filter_map(|(c, j)| j.parent.map(|p| (p, c)))
        .collect()
}

/// Joints or bones of a clip in the pose of one frame.
pub fn clip_table(clip: &AnimData, tab: &PrimInspectorTab, frame: usize) -> InspectorTable {
    let world = clip.world_pose(frame);
    let pos = |j: usize| world[j].w_axis.truncate();
    match tab {
        PrimInspectorTab::Bone => {
            let list = bones(clip);
            let labels = list.iter().map(|(a, b)| format!("{} > {}", clip.joints[*a].name, clip.joints[*b].name)).collect();
            InspectorTable {
                rows: list.len(),
                labels: Some(("from > to".into(), labels)),
                cols: vec![
                    ("from".into(), list.iter().map(|(a, _)| vec![*a as f32]).collect()),
                    ("to".into(), list.iter().map(|(_, b)| vec![*b as f32]).collect()),
                    ("length".into(), list.iter().map(|(a, b)| vec![pos(*a).distance(pos(*b))]).collect()),
                    ("dir".into(), list.iter().map(|(a, b)| (pos(*b) - pos(*a)).normalize_or_zero().to_array().to_vec()).collect()),
                ],
                arrays: vec![],
            }
        }
        _ => {
            let n = clip.joints.len();
            let euler = |j: usize| {
                let (x, y, z) = clip.local(j, frame).rotation.to_euler(glam::EulerRot::XYZ);
                vec![x.to_degrees(), y.to_degrees(), z.to_degrees()]
            };
            InspectorTable {
                rows: n,
                labels: Some(("joint".into(), clip.joints.iter().map(|j| j.name.clone()).collect())),
                cols: vec![
                    ("parent".into(), clip.joints.iter().map(|j| vec![j.parent.map(|p| p as f32).unwrap_or(-1.0)]).collect()),
                    ("P".into(), (0..n).map(|j| pos(j).to_array().to_vec()).collect()),
                    ("R".into(), (0..n).map(euler).collect()),
                    ("bone".into(), clip.joints.iter().map(|j| vec![j.is_bone as u8 as f32]).collect()),
                ],
                arrays: vec![],
            }
        }
    }
}

/// One component tab of a mesh.
#[cfg(test)]
pub fn mesh_table(mesh: &MeshData, tab: &PrimInspectorTab) -> InspectorTable {
    parts_table(std::slice::from_ref(mesh), tab)
}

/// Rows of the Object tab: one per primitive, when any has Object attributes.
pub fn object_rows(parts: &[MeshData]) -> usize {
    let any = parts.iter().any(|m| m.primvars.iter().any(|p| p.interp == PrimVarInterp::Constant));
    if any { parts.len() } else { 0 }
}

/// One component tab of some primitives, each where it is drawn: the rows of
/// each primitive in turn, with point, face and corner numbers counted
/// across all of them.
/// A column one primitive lacks shows `-` on its rows.
pub fn parts_table(parts: &[MeshData], tab: &PrimInspectorTab) -> InspectorTable {
    let mut rows = 0usize;
    let mut cols: Vec<(String, Vec<Vec<f32>>)> = vec![];
    let mut first = First::default();
    for m in parts {
        let (n, part_cols) = part_table(m, tab, first);
        for (name, values) in part_cols {
            let at = match cols.iter().position(|c| c.0 == name) {
                Some(at) => at,
                None => { cols.push((name, vec![vec![]; rows])); cols.len() - 1 }
            };
            cols[at].1.extend(values);
        }
        rows += n;
        for c in cols.iter_mut() { c.1.resize(rows, vec![]); }
        first.point += m.point_count();
        first.face += m.polygons().len() + m.curve_counts.len();
        first.corner += m.corner_count();
    }
    // The points of an edge or a face: one array each, `ptIndices`, as the
    // face's slice of `Topology::Mesh::indices` (USD faceVertexIndices).
    let arrays = match tab {
        PrimInspectorTab::Edge | PrimInspectorTab::Uniform => cols.iter()
            .filter(|c| c.0 == "ptIndices")
            .map(|c| (c.0.clone(), c.1.iter().map(|v| array_text(v).len()).max().unwrap_or(2)))
            .collect(),
        _ => vec![],
    };
    InspectorTable { rows, labels: None, cols, arrays }
}

/// The numbers of a primitive's first point, face and corner in the whole
/// table, when several primitives are listed one after the other.
#[derive(Clone, Copy, Default)]
struct First { point: usize, face: usize, corner: usize }

/// One component tab of one primitive, numbered on from `first`.
fn part_table(mesh: &MeshData, tab: &PrimInspectorTab, first: First) -> (usize, Vec<(String, Vec<Vec<f32>>)>) {
    let first_point = first.point;
    use glam::Vec3;
    let vars = |interp: PrimVarInterp| mesh.primvars.iter().filter(move |p| p.interp == interp);
    // Points, numbered as listed: polygon vertices, curve control points, free points.
    let (nv, nc) = (mesh.vertices.len(), mesh.curve_points.len());
    let at = |i: usize| -> Vec3 {
        Vec3::from_array(if i < nv { mesh.vertices[i] } else if i < nv + nc { mesh.curve_points[i - nv] } else { mesh.points[i - nv - nc] })
    };
    let num = |i: usize| (first_point + i) as f32;
    // A column with values on some rows and `-` on the others.
    let fill = |n: usize, from: usize, values: Vec<Vec<f32>>| -> Vec<Vec<f32>> {
        let mut col = vec![vec![]; n];
        for (k, v) in values.into_iter().enumerate() {
            if let Some(slot) = col.get_mut(from + k) { *slot = v; }
        }
        col
    };
    match tab {
        PrimInspectorTab::Vertex => {
            let n = mesh.point_count();
            let mut cols: Vec<(String, Vec<Vec<f32>>)> = vec![
                ("P".into(), (0..n).map(|i| at(i).to_array().to_vec()).collect()),
            ];
            if !mesh.normals.is_empty() {
                cols.push(("N".into(), fill(n, 0, mesh.normals.iter().map(|v| v.to_vec()).collect())));
            }
            for pv in vars(PrimVarInterp::Vertex) {
                if pv.name != "N" { cols.push((pv.name.clone(), fill(n, 0, pv.values.clone()))); }
            }
            // Free points' widths: one each, or one for them all.
            if !mesh.widths.is_empty() && !mesh.points.is_empty() {
                let w = |i: usize| mesh.widths.get(i).or(mesh.widths.first()).copied().unwrap_or(0.0);
                cols.push(("widths".into(), fill(n, nv + nc, (0..mesh.points.len()).map(|i| vec![w(i)]).collect())));
            }
            (n, cols)
        }
        PrimInspectorTab::Edge => {
            // Polygon edges, then the segments of each curve.
            let mut edges = crate::core::poly::PolyMesh::from_mesh(mesh).edges();
            edges.extend(mesh.curve_segments().into_iter().map(|[a, b]| [a + nv as u32, b + nv as u32]));
            (edges.len(), vec![
                ("ptIndices".into(), edges.iter().map(|e| vec![num(e[0] as usize), num(e[1] as usize)]).collect()),
                ("length".into(), edges.iter().map(|e| vec![at(e[0] as usize).distance(at(e[1] as usize))]).collect()),
            ])
        }
        PrimInspectorTab::Uniform => {
            // Polygons, then curves: a curve is one face, its control points
            // its corners (as `Topology::corner_count` counts them).
            let poly = crate::core::poly::PolyMesh::from_mesh(mesh);
            let np = poly.polys.len();
            let mut corners: Vec<Vec<usize>> = poly.polys.iter().map(|q| q.iter().map(|v| *v as usize).collect()).collect();
            let mut next = nv;
            for &c in &mesh.curve_counts {
                corners.push((next..next + c as usize).collect());
                next += c as usize;
            }
            let n = corners.len();
            let mut cols: Vec<(String, Vec<Vec<f32>>)> = vec![
                ("corners".into(), corners.iter().map(|q| vec![q.len() as f32]).collect()),
                ("ptIndices".into(), corners.iter().map(|q| q.iter().map(|v| num(*v)).collect()).collect()),
            ];
            if np > 0 {
                // Where each polygon's corners start in the Corner tab: its
                // corners are firstCorner, firstCorner + 1, … (corners).
                let starts = poly.polys.iter().scan(first.corner, |at, q| { let s = *at; *at += q.len(); Some(vec![s as f32]) });
                cols.push(("firstCorner".into(), fill(n, 0, starts.collect())));
                cols.push(("N".into(), fill(n, 0, (0..np).map(|q| poly.normal(q).to_array().to_vec()).collect())));
                cols.push(("area".into(), fill(n, 0, (0..np).map(|q| vec![poly.area_normal(q).length() * 0.5]).collect())));
            }
            if n > np {
                // Along the control points, closing a periodic curve.
                let closed = mesh.curve_wrap == crate::types::CurveWrap::Periodic;
                let length = |cv: &Vec<usize>| -> f32 {
                    let open: f32 = cv.windows(2).map(|w| at(w[0]).distance(at(w[1]))).sum();
                    if closed && cv.len() > 2 { open + at(cv[cv.len() - 1]).distance(at(cv[0])) } else { open }
                };
                cols.push(("length".into(), fill(n, np, corners[np..].iter().map(|cv| vec![length(cv)]).collect())));
            }
            for pv in vars(PrimVarInterp::Uniform) { cols.push((pv.name.clone(), fill(n, 0, pv.values.clone()))); }
            (n, cols)
        }
        PrimInspectorTab::FaceVarying => {
            // Polygon corners, polygon after polygon: the point each belongs to
            // (several corners share a point), the face it is a corner of and
            // its place in that face (0 for a face's first corner), then what
            // each corner carries. The row number is the corner's own number,
            // counted across the whole mesh as USD counts faceVarying values.
            let n = mesh.corner_count();
            let mut cols: Vec<(String, Vec<Vec<f32>>)> = vec![];
            if n > 0 {
                cols.push(("ptIndex".into(), mesh.corner_points().iter().map(|i| vec![num(*i as usize)]).collect()));
                let polys = mesh.polygons();
                let of_face = |f: usize, q: &Vec<u32>| (0..q.len()).map(move |k| (f, k));
                let (faces, local): (Vec<Vec<f32>>, Vec<Vec<f32>>) = polys.iter().enumerate().flat_map(|(f, q)| of_face(f, q))
                    .map(|(f, k)| (vec![(first.face + f) as f32], vec![k as f32])).unzip();
                cols.push(("faceIndex".into(), faces));
                cols.push(("localCornerIndex".into(), local));
            }
            if let Some(uvs) = mesh.corner_uvs() {
                cols.push(("uv".into(), uvs.iter().map(|uv| uv.to_vec()).collect()));
            }
            cols.extend(vars(PrimVarInterp::FaceVarying).map(|pv| (pv.name.clone(), fill(n, 0, pv.values.clone()))));
            (n, cols)
        }
        PrimInspectorTab::Constant => {
            // One row: this primitive's value of each.
            let cols: Vec<_> = vars(PrimVarInterp::Constant)
                .map(|pv| (pv.name.clone(), vec![pv.values.first().cloned().unwrap_or_default()]))
                .collect();
            (1, cols)
        }
        _ => (0, vec![]),
    }
}

// ── Tab button ────────────────────────────────────────────────────────────────
// Update tab_btn to use the new method
fn tab_btn(ui: &mut egui::Ui, state: &mut PrimInspectorState, tab: PrimInspectorTab, label: &str) {
    let is_active = state.active_tab == tab;
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .color(if is_active { xsi::TEXT() } else { xsi::TEXT_DIM() })
            .size(11.0),
    )
    .fill(if is_active { xsi::TAB_ACTIVE() } else { egui::Color32::TRANSPARENT })
    .frame(true)
    .min_size(egui::vec2(0.0, 22.0));

    if ui.add(btn).clicked() {
        state.set_active_tab(tab);  // ✅ Use new method
    }
}



// ── Spreadsheet grid ──────────────────────────────────────────────────────────

/// `cols` is a list of (column_group_name, per_row_components).
/// Components wider than 1 are shown as name.X / name.Y / name.Z.
fn draw_spreadsheet(
    ui:    &mut egui::Ui,
    state: &mut PrimInspectorState,
    table: &InspectorTable,
) {
    let row_count = table.rows;
    let cols = &table.cols;
    // Width of the text column, when the table has one.
    let label_w = if table.labels.is_some() { LABEL_COL_W } else { 0.0 };
    if row_count == 0 || cols.is_empty() {
        ui.label(egui::RichText::new("No data.").color(xsi::TEXT_DIM()));
        return;
    }

    // Expand column headers: P with width 3 → P.X, P.Y, P.Z. An array
    // column stays one column, as wide as its widest cell.
    struct ColDef<'a> { header: String, source: &'a str, component: usize, array: bool, x: f32, w: f32 }
    let mut col_defs: Vec<ColDef> = Vec::new();
    let mut x = 0.0;
    for (name, rows) in cols {
        if let Some((_, chars)) = table.arrays.iter().find(|a| a.0 == *name) {
            let w = (*chars as f32 * ARRAY_CHAR_W + 12.0).clamp(DATA_COL_W, ARRAY_COL_MAX_W);
            col_defs.push(ColDef { header: name.to_string(), source: name, component: 0, array: true, x, w });
            x += w;
            continue;
        }
        // A row may be empty (`-`) where a primitive lacks the column.
        let width = rows.iter().map(|r| r.len()).find(|&w| w > 0).unwrap_or(1);
        if width == 1 {
            col_defs.push(ColDef { header: name.to_string(), source: name, component: 0, array: false, x, w: DATA_COL_W });
            x += DATA_COL_W;
        } else {
            let suffixes = ["X","Y","Z","W"];
            for c in 0..width {
                col_defs.push(ColDef {
                    header:    format!("{}.{}", name, suffixes.get(c).unwrap_or(&"?")),
                    source:    name,
                    component: c,
                    array:     false,
                    x,
                    w:         DATA_COL_W,
                });
                x += DATA_COL_W;
            }
        }
    }

    let total_w = IDX_COL_W + label_w + x;
    let avail_h = ui.available_height();

    // Virtual scroll: how many rows fit?
    let visible_rows = ((avail_h - HEADER_H) / ROW_H).floor() as usize;
    let visible_rows = visible_rows.min(MAX_ROWS).min(row_count);

    // Clamp row_offset
    if state.row_offset + visible_rows > row_count {
        state.row_offset = row_count.saturating_sub(visible_rows);
    }

    let scroll_area = egui::ScrollArea::horizontal()
        .id_salt("prim_inspector_hscroll")
        .auto_shrink([false, false]);

    scroll_area.show(ui, |ui| {
        let (outer_rect, _) = ui.allocate_exact_size(
            egui::vec2(total_w.max(ui.available_width()), avail_h),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(outer_rect);

        // ── Header row ────────────────────────────────────────────────────────
        let hdr_rect = egui::Rect::from_min_size(
            outer_rect.min,
            egui::vec2(outer_rect.width(), HEADER_H),
        );
        painter.rect_filled(hdr_rect, 0.0, xsi::HEADER_BG());

        // Index column header
        painter.rect_stroke(
            egui::Rect::from_min_size(hdr_rect.min, egui::vec2(IDX_COL_W, HEADER_H)),
            0.0, egui::Stroke::new(1.0, xsi::BORDER()), egui::StrokeKind::Middle
        );

        if let Some((heading, _)) = &table.labels {
            let cell = egui::Rect::from_min_size(
                egui::pos2(outer_rect.min.x + IDX_COL_W, outer_rect.min.y), egui::vec2(label_w, HEADER_H));
            painter.rect_stroke(cell, 0.0, egui::Stroke::new(1.0, xsi::BORDER()), egui::StrokeKind::Middle);
            painter.text(cell.center(), egui::Align2::CENTER_CENTER, heading, egui::FontId::proportional(11.0), xsi::TEXT());
        }

        for cd in col_defs.iter() {
            let x = outer_rect.min.x + IDX_COL_W + label_w + cd.x;
            let cell = egui::Rect::from_min_size(
                egui::pos2(x, outer_rect.min.y),
                egui::vec2(cd.w, HEADER_H),
            );
            painter.rect_stroke(cell, 0.0, egui::Stroke::new(1.0, xsi::BORDER()), egui::StrokeKind::Middle);
            painter.text(
                cell.center(),
                egui::Align2::CENTER_CENTER,
                &cd.header,
                egui::FontId::proportional(11.0),
                xsi::TEXT(),
            );
        }

        // ── Data rows ─────────────────────────────────────────────────────────
        for (ri, row_idx) in (state.row_offset..state.row_offset + visible_rows).enumerate() {
            let y = outer_rect.min.y + HEADER_H + ri as f32 * ROW_H;
            let row_col = if ri % 2 == 0 { xsi::ROW_EVEN() } else { xsi::ROW_ODD() };

            let row_rect = egui::Rect::from_min_size(
                egui::pos2(outer_rect.min.x, y),
                egui::vec2(outer_rect.width(), ROW_H),
            );
            painter.rect_filled(row_rect, 0.0, row_col);

            // Index cell
            let idx_cell = egui::Rect::from_min_size(
                egui::pos2(outer_rect.min.x, y),
                egui::vec2(IDX_COL_W, ROW_H),
            );
            painter.rect_stroke(idx_cell, 0.0, egui::Stroke::new(1.0, xsi::BORDER()), egui::StrokeKind::Middle);
            painter.text(
                idx_cell.center(),
                egui::Align2::CENTER_CENTER,
                &row_idx.to_string(),
                egui::FontId::proportional(10.0),
                xsi::TEXT_IDX(),
            );

            // Text cell
            if let Some((_, labels)) = &table.labels {
                let cell = egui::Rect::from_min_size(egui::pos2(outer_rect.min.x + IDX_COL_W, y), egui::vec2(label_w, ROW_H));
                painter.rect_stroke(cell, 0.0, egui::Stroke::new(1.0, xsi::BORDER()), egui::StrokeKind::Middle);
                painter.with_clip_rect(cell.shrink(2.0)).text(
                    egui::pos2(cell.min.x + 6.0, cell.center().y), egui::Align2::LEFT_CENTER,
                    labels.get(row_idx).map(|s| s.as_str()).unwrap_or("-"),
                    egui::FontId::proportional(10.5), xsi::TEXT());
            }

            // Data cells
            for cd in col_defs.iter() {
                let x = outer_rect.min.x + IDX_COL_W + label_w + cd.x;
                let cell = egui::Rect::from_min_size(
                    egui::pos2(x, y),
                    egui::vec2(cd.w, ROW_H),
                );
                painter.rect_stroke(cell, 0.0, egui::Stroke::new(1.0, xsi::BORDER()), egui::StrokeKind::Middle);

                let row = cols.iter()
                    .find(|(n, _)| *n == cd.source)
                    .and_then(|(_, rows)| rows.get(row_idx));
                if cd.array {
                    // The whole array in one cell, from the left; cut off at
                    // the edge when it is longer than the widest column allows.
                    let text = row.filter(|r| !r.is_empty()).map(|r| array_text(r)).unwrap_or_else(|| "-".into());
                    painter.with_clip_rect(cell.shrink(2.0)).text(
                        egui::pos2(cell.min.x + 6.0, cell.center().y),
                        egui::Align2::LEFT_CENTER,
                        &text,
                        egui::FontId::monospace(10.0),
                        xsi::TEXT(),
                    );
                    continue;
                }
                let val_str = row
                    .and_then(|r| r.get(cd.component))
                    .map(|v| format!("{:.4}", v))
                    .unwrap_or_else(|| "-".into());

                painter.text(
                    egui::pos2(cell.max.x - 6.0, cell.center().y),
                    egui::Align2::RIGHT_CENTER,
                    &val_str,
                    egui::FontId::monospace(10.0),
                    xsi::TEXT(),
                );
            }
        }

        // ── Vertical scroll bar (manual) ──────────────────────────────────────
        // At the right edge of what is in view, not of the whole table (which
        // may be wider and scrolled sideways), and dragged or clicked to move.
        if row_count > visible_rows {
            let sb_w    = 10.0;
            let right   = ui.clip_rect().max.x.min(outer_rect.max.x);
            let sb_rect = egui::Rect::from_min_size(
                egui::pos2(right - sb_w, outer_rect.min.y + HEADER_H),
                egui::vec2(sb_w, avail_h - HEADER_H),
            );
            painter.rect_filled(sb_rect, 4.0, egui::Color32::from_gray(50));

            let ratio    = visible_rows as f32 / row_count as f32;
            let thumb_h  = (sb_rect.height() * ratio).max(20.0);
            // Dragging the thumb, or clicking the bar, puts the thumb's middle
            // under the pointer.
            let bar = ui.interact(sb_rect, ui.id().with("prim_inspector_vscroll"), egui::Sense::click_and_drag());
            if bar.dragged() || bar.clicked() {
                if let Some(at) = bar.interact_pointer_pos() {
                    let travel = (sb_rect.height() - thumb_h).max(1.0);
                    let t = ((at.y - sb_rect.min.y - thumb_h * 0.5) / travel).clamp(0.0, 1.0);
                    state.row_offset = (t * (row_count - visible_rows) as f32).round() as usize;
                }
            }
            let thumb_y  = sb_rect.min.y
                + (sb_rect.height() - thumb_h)
                * (state.row_offset as f32 / (row_count - visible_rows) as f32);
            let thumb = egui::Rect::from_min_size(
                egui::pos2(sb_rect.min.x, thumb_y),
                egui::vec2(sb_w, thumb_h),
            );
            painter.rect_filled(thumb, 4.0, egui::Color32::from_gray(110));

            // Scroll on mouse wheel inside the panel
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 && outer_rect.contains(
                ui.input(|i| i.pointer.hover_pos()).unwrap_or_default()
            ) {
                let lines = (-scroll_delta / ROW_H).round() as i64;
                state.row_offset = (state.row_offset as i64 + lines)
                    .clamp(0, (row_count - visible_rows) as i64) as usize;
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::anim::{create_test_clip, FrameRate};
    use crate::node_graph::nodes::{create_cube, create_grid};

    fn col<'a>(t: &'a InspectorTable, name: &str) -> &'a Vec<Vec<f32>> { &t.cols.iter().find(|c| c.0 == name).unwrap().1 }

    #[test]
    fn a_mesh_lists_vertices_edges_and_polygons() {
        let cube = create_cube(1.0);
        let v = mesh_table(&cube, &PrimInspectorTab::Vertex);
        assert_eq!(v.rows, 8);
        assert_eq!(col(&v, "P")[1], vec![0.5, -0.5, -0.5]);
        let e = mesh_table(&cube, &PrimInspectorTab::Edge);
        assert_eq!(e.rows, 12);
        assert!(col(&e, "length").iter().all(|l| (l[0] - 1.0).abs() < 1e-6));
        // Polygons, not the triangles they are drawn with.
        let p = mesh_table(&cube, &PrimInspectorTab::Uniform);
        assert_eq!(p.rows, 6);
        assert!(col(&p, "corners").iter().all(|c| c[0] == 4.0));
        assert!(col(&p, "area").iter().all(|a| (a[0] - 1.0).abs() < 1e-6));
        assert_eq!(col(&p, "N")[3], vec![0.0, 1.0, 0.0]);
        // Corners of the polygons, not of the triangles drawn: 6 quads, 24
        // corners, each with the point it belongs to. No UVs, no uv column.
        let c = mesh_table(&cube, &PrimInspectorTab::FaceVarying);
        assert_eq!(c.rows, 24);
        let pt: Vec<f32> = col(&c, "ptIndex").iter().map(|v| v[0]).collect();
        assert_eq!(&pt[..4], &[0.0, 3.0, 2.0, 1.0], "the first face's corners");
        // Each of the 8 points is shared by three corners.
        assert!((0..8).all(|p| pt.iter().filter(|&&q| q == p as f32).count() == 3));
        assert!(c.cols.iter().all(|c| c.0 != "vtx" && c.0 != "uv"));
        // The face each corner belongs to, and its place in that face.
        let face: Vec<f32> = col(&c, "faceIndex").iter().map(|v| v[0]).collect();
        let local: Vec<f32> = col(&c, "localCornerIndex").iter().map(|v| v[0]).collect();
        assert_eq!(&face[..6], &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
        assert_eq!(&local[..6], &[0.0, 1.0, 2.0, 3.0, 0.0, 1.0]);
        // A face's first corner, which a corner of local index 0 is.
        let starts: Vec<f32> = col(&p, "firstCorner").iter().map(|v| v[0]).collect();
        assert_eq!(starts, vec![0.0, 4.0, 8.0, 12.0, 16.0, 20.0]);
        assert!(starts.iter().all(|&s| local[s as usize] == 0.0));
        // With UVs: one per polygon corner, read back from the triangles.
        // A planar unwrap gives every corner of a point the same UV.
        let mut grid = create_grid(2, 2, 2.0);
        grid.uvs = crate::core::uv::unwrap(&grid, crate::core::uv::UvMethod::Planar, 0.0, 0.0, 1, 1);
        let fv = mesh_table(&grid, &PrimInspectorTab::FaceVarying);
        assert_eq!(fv.rows, 16);
        let (pts, uvs) = (col(&fv, "ptIndex"), col(&fv, "uv"));
        assert_eq!(uvs.len(), 16);
        assert_eq!(uvs[0].len(), 2);
        for a in 0..16 {
            for b in 0..16 {
                if pts[a] == pts[b] { assert_eq!(uvs[a], uvs[b], "corners {a} and {b} share a point"); }
            }
        }
    }

    /// Two curves of three control points, one open and one periodic.
    fn two_curves(wrap: crate::types::CurveWrap) -> MeshData {
        MeshData {
            curve_points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 2.0], [3.0, 0.0, 2.0], [3.0, 4.0, 2.0]],
            curve_counts: vec![3, 3],
            curve_wrap: wrap,
            ..Default::default()
        }
    }

    #[test]
    fn curves_list_control_points_segments_and_one_face_each() {
        use crate::types::CurveWrap;
        let open = two_curves(CurveWrap::Nonperiodic);
        let v = mesh_table(&open, &PrimInspectorTab::Vertex);
        assert_eq!(v.rows, 6);
        assert_eq!(col(&v, "P")[4], vec![3.0, 0.0, 2.0]);
        // Segments between control points that follow each other, per curve.
        let e = mesh_table(&open, &PrimInspectorTab::Edge);
        assert_eq!(e.rows, 4);
        assert_eq!(col(&e, "ptIndices")[2], vec![3.0, 4.0]);
        assert_eq!(e.arrays, vec![("ptIndices".to_string(), "[0, 1]".len())]);
        assert_eq!(col(&e, "length")[3], vec![4.0]);
        // One face per curve: its control points are its corners.
        let f = mesh_table(&open, &PrimInspectorTab::Uniform);
        assert_eq!(f.rows, 2);
        assert_eq!(col(&f, "corners")[1], vec![3.0]);
        // Every point of the face, as one array: as many as it has corners.
        assert_eq!(col(&f, "ptIndices")[1], vec![3.0, 4.0, 5.0]);
        assert!(f.cols.iter().all(|c| !c.0.starts_with("vtx[")), "one ptIndices array, not a column per point");
        assert_eq!(col(&f, "length")[1], vec![7.0]);
        assert!(f.cols.iter().all(|c| c.0 != "area" && c.0 != "N"), "no polygon columns for curves");
        // A periodic curve closes: one more segment, and a longer curve.
        let ring = two_curves(CurveWrap::Periodic);
        let e = mesh_table(&ring, &PrimInspectorTab::Edge);
        assert_eq!(e.rows, 6);
        assert_eq!(col(&e, "ptIndices")[5], vec![5.0, 3.0]);
        assert_eq!(mesh_table(&ring, &PrimInspectorTab::Uniform).cols.iter().find(|c| c.0 == "length").unwrap().1[1], vec![12.0]);
        // Curves have no polygon corners.
        assert!(mesh_table(&open, &PrimInspectorTab::FaceVarying).cols.is_empty());
    }

    #[test]
    fn primitives_are_listed_one_after_the_other() {
        use crate::types::CurveWrap;
        let parts = vec![create_cube(1.0), two_curves(CurveWrap::Nonperiodic)];
        // Points: the cube's 8, then the curves', numbered on.
        let v = parts_table(&parts, &PrimInspectorTab::Vertex);
        assert_eq!(v.rows, 14);
        assert_eq!(col(&v, "P")[12], vec![3.0, 0.0, 2.0]);
        assert_eq!(col(&v, "N")[0].len(), 3);
        assert!(col(&v, "N")[8].is_empty(), "a curve point has no normal: shown as -");
        let e = parts_table(&parts, &PrimInspectorTab::Edge);
        assert_eq!(e.rows, 12 + 4);
        assert_eq!(col(&e, "ptIndices")[12], vec![8.0, 9.0]);
        let f = parts_table(&parts, &PrimInspectorTab::Uniform);
        assert_eq!(f.rows, 6 + 2);
        assert_eq!(col(&f, "corners")[6], vec![3.0]);
        assert_eq!(col(&f, "ptIndices")[6], vec![8.0, 9.0, 10.0]);
        assert_eq!(col(&f, "ptIndices")[0].len(), 4);
        // The widest cell sets the column's width: a quad, "[0, 3, 2, 1]".
        assert_eq!(f.arrays, vec![("ptIndices".to_string(), 12)]);
        assert_eq!(col(&f, "area")[0].len(), 1);
        assert!(col(&f, "area")[6].is_empty() && col(&f, "length")[0].is_empty());
        assert!(col(&f, "firstCorner")[6].is_empty(), "curves have no polygon corners");

        // Face and corner numbers run on from one primitive to the next.
        let two = vec![create_cube(1.0), create_cube(1.0)];
        let c = parts_table(&two, &PrimInspectorTab::FaceVarying);
        assert_eq!(c.rows, 48);
        assert_eq!((col(&c, "faceIndex")[24][0], col(&c, "localCornerIndex")[24][0]), (6.0, 0.0));
        assert_eq!(col(&parts_table(&two, &PrimInspectorTab::Uniform), "firstCorner")[6], vec![24.0]);
    }

    /// The curves and points example, as the Load USD node gives it.
    #[test]
    fn the_strands_example_lists_its_curves_and_points() {
        use crate::types::{EvalResult, Prim};
        let dir = std::env::temp_dir().join("xms_inspector_strands");
        let _ = std::fs::remove_dir_all(&dir);
        crate::examples::write_all(&dir).unwrap();
        let scene = crate::usd_stage::read(&dir.join(crate::examples_stage::STRANDS)).unwrap();
        let prims: Vec<Prim> = scene.meshes.iter()
            .map(|m| Prim { place: m.place.clone(), ..Prim::new(m.path.clone(), (*m.mesh).clone()) })
            .collect();
        let parts = EvalResult::Named(prims).into_parts();

        // Control points as authored, of every basis (not the polylines they
        // are drawn as), then the 400 points of the spiral.
        let curve_points = 7 + 7 + 8 + 8 + 600 * 5;
        let v = parts_table(&parts, &PrimInspectorTab::Vertex);
        assert_eq!(v.rows, curve_points + 400);
        let linear = scene.meshes.iter().position(|m| m.path.ends_with("Linear")).unwrap();
        let first: usize = parts[..linear].iter().map(|m| m.point_count()).sum();
        let p = &col(&v, "P")[first];
        assert!((p[0] - -4.0).abs() < 1e-5 && (p[1] - 0.2).abs() < 1e-5 && p[2].abs() < 1e-5, "{p:?}");
        // Widths for the spiral's points only.
        let widths = col(&v, "widths");
        assert_eq!(widths.iter().filter(|w| !w.is_empty()).count(), 400);

        // Segments: n - 1 per open curve, n for the periodic ring.
        let e = parts_table(&parts, &PrimInspectorTab::Edge);
        assert_eq!(e.rows, 6 + 6 + 7 + 8 + 600 * 4);
        // One face per curve.
        let f = parts_table(&parts, &PrimInspectorTab::Uniform);
        assert_eq!(f.rows, 4 + 600);
        assert!(col(&f, "length").iter().all(|l| l[0] > 0.0));
    }

    #[test]
    fn a_clip_lists_joints_and_bones() {
        let clip = create_test_clip(1.0, FrameRate::new(30, 1));
        let j = clip_table(&clip, &PrimInspectorTab::Joint, 0);
        assert_eq!(j.rows, 19);
        let (heading, names) = j.labels.as_ref().unwrap();
        assert_eq!(heading, "joint");
        assert_eq!(names[0], "Take01:Hips");
        assert_eq!(col(&j, "parent")[0], vec![-1.0]);
        assert_eq!(col(&j, "parent")[1], vec![0.0]);
        assert!((col(&j, "P")[0][1] - 0.97).abs() < 1e-4);
        // The pose follows the frame.
        let later = clip_table(&clip, &PrimInspectorTab::Joint, 8);
        assert_ne!(col(&j, "R")[5], col(&later, "R")[5]);

        // A bone runs from a joint to the joint below it: 18 for 19 joints.
        let b = clip_table(&clip, &PrimInspectorTab::Bone, 0);
        assert_eq!(b.rows, 18);
        let (_, names) = b.labels.as_ref().unwrap();
        assert_eq!(names[0], "Take01:Hips > Take01:Spine");
        assert_eq!((col(&b, "from")[0][0], col(&b, "to")[0][0]), (0.0, 1.0));
        assert!((col(&b, "length")[0][0] - 0.12).abs() < 1e-4);
        assert!((col(&b, "dir")[0][1] - 1.0).abs() < 1e-3);
    }
}
