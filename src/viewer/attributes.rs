//! Display Attribute (the Viewer's Attributes menu): attributes of the prims
//! shown drawn over them, each as its type is, as Houdini's visualizers and
//! Gaffer's viewer show them:
//!
//! - a float (or a boolean) colours the surface through a ramp, its range the
//!   values shown unless set;
//! - a colour colours the surface itself; a 2D vector (UVs) colours it red by
//!   u and green by v;
//! - a 3D vector or a normal is a line from each element along it;
//! - an integer, text, a list, anything else is a label over each element.
//!
//! Where an element is drawn goes by the attribute's context: a point at the
//! point, a face at its centre (or the whole face, coloured), a corner a
//! little towards its face's centre, the object at its box's centre. Several
//! attributes may be shown: the first of the list that colours a prim colours
//! it, lines and labels add up. Intrinsics are read by their names
//! (`intrinsic:ptIndex`).
//!
//! On the selected prims (Scope: Selected, the default) or every prim shown
//! (All), and within limits: past them the Viewer says what it left out.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use glam::{Mat4, Vec3};

use crate::core::geo::{Attr, Column, Context, Geo, Role, Topology};

/// Lines drawn for vectors at most, every prim's.
pub const LINE_LIMIT: usize = 1_000_000;
/// Labels made at most, every prim's: the Viewer draws those on screen,
/// nearest first, up to `LABELS_DRAWN`.
pub const LABEL_LIMIT: usize = 200_000;
pub const LABELS_DRAWN: usize = 5_000;
/// Triangles coloured at most: past it, the rest are drawn as they are.
pub const COLOURED_LIMIT: usize = 20_000_000;
/// Triangles hiding labels at most: past it, labels are not hidden.
pub const OCCLUDER_LIMIT: usize = 2_000_000;
/// Labels tested for what hides them in a frame at most, nearest first.
const LABELS_TESTED: usize = 10_000;

/// Which prims attributes are shown on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Scope {
    /// The prims selected (in the Scene Explorer or the Viewer), and those
    /// below them.
    #[default]
    Selected,
    All,
}

/// The colours a float goes through, low to high.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Ramp {
    /// Black to white.
    #[default]
    Grey,
    /// Black, red, yellow, white.
    Heat,
}

/// An attribute shown.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Shown {
    /// Its name, as ICE reads it: `density`, `N`, `intrinsic:ptIndex`.
    pub name: String,
    #[serde(default = "yes")]
    pub on: bool,
    /// A float's range, low and high: `None`, the values shown.
    #[serde(default)]
    pub min: Option<f32>,
    #[serde(default)]
    pub max: Option<f32>,
    #[serde(default)]
    pub ramp: Ramp,
    /// A vector's line: its length times this.
    #[serde(default = "one")]
    pub scale: f32,
}

fn yes() -> bool {
    true
}
fn one() -> f32 {
    1.0
}

impl Shown {
    pub fn new(name: &str) -> Shown {
        Shown {
            name: name.to_string(),
            on: true,
            min: None,
            max: None,
            ramp: Ramp::Grey,
            scale: 1.0,
        }
    }
}

/// What the Attributes menu says: whether attributes are shown, on which
/// prims, and which.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub on: bool,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub list: Vec<Shown>,
}

impl Settings {
    /// The attributes shown now: none while it is off.
    pub fn shown(&self) -> impl Iterator<Item = &Shown> {
        self.list
            .iter()
            .filter(|s| self.on && s.on && !s.name.trim().is_empty())
    }
}

static SETTINGS: Mutex<Option<Settings>> = Mutex::new(None);
static REVISION: AtomicU64 = AtomicU64::new(0);

/// What the menu says: off, with nothing listed, whenever Imago starts; kept
/// neither with the graph nor between sessions.
pub fn settings() -> Settings {
    SETTINGS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(Settings::default)
        .clone()
}

/// Change what is shown, and have the Viewer build its scene again.
pub fn set(new: Settings) {
    if settings() == new {
        return;
    }
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some(new);
    REVISION.fetch_add(1, Ordering::Relaxed);
}

/// Changes whenever the settings do.
pub fn revision() -> u64 {
    REVISION.load(Ordering::Relaxed)
}

/// How an attribute is drawn, by its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Ramp,
    Colour,
    Uv,
    Vector,
    Label,
}

impl Look {
    /// It colours the surface (or, on points and curves, their points).
    pub fn colours(self) -> bool {
        matches!(self, Look::Ramp | Look::Colour | Look::Uv)
    }
}

/// How an attribute of this column, role and list size is drawn.
pub fn look_of(column: &Column, role: Role, element_size: usize) -> Look {
    if element_size > 1 {
        return Look::Label;
    }
    match column {
        Column::Float(_) | Column::Bool(_) => Look::Ramp,
        Column::Vec2(_) => Look::Uv,
        Column::Vec3(_) | Column::Vec4(_) if role == Role::Color => Look::Colour,
        Column::Vec3(_) if matches!(role, Role::Normal | Role::Vector | Role::None) => Look::Vector,
        _ => Look::Label,
    }
}

/// An attribute of a prim's geometry, as it is read to be drawn: its
/// context, its values, the value of each element (indices undone), how many
/// values an element has.
pub struct Found {
    pub context: Context,
    pub column: Column,
    pub role: Role,
    indices: Option<std::sync::Arc<Vec<u32>>>,
    pub element_size: usize,
}

impl Found {
    /// The values of element `e`, one or `element_size` of them.
    fn at(&self, e: usize) -> Option<usize> {
        match &self.indices {
            Some(ix) => ix.get(e).map(|&i| i as usize),
            None => Some(e),
        }
    }

    pub fn look(&self) -> Look {
        look_of(&self.column, self.role, self.element_size)
    }

    /// Element `e`'s value as a float (a ramp's), if it is one.
    pub fn float(&self, e: usize) -> Option<f32> {
        let i = self.at(e)?;
        match &self.column {
            Column::Float(v) => v.get(i).copied(),
            Column::Bool(v) => v.get(i).map(|b| *b as u8 as f32),
            _ => None,
        }
    }

    /// Element `e`'s value as up to four floats (a colour's, UV's or vector's).
    pub fn floats(&self, e: usize) -> Option<[f32; 4]> {
        let i = self.at(e)?;
        match &self.column {
            Column::Vec2(v) => v.get(i).map(|x| [x[0], x[1], 0.0, 1.0]),
            Column::Vec3(v) => v.get(i).map(|x| [x[0], x[1], x[2], 1.0]),
            Column::Vec4(v) | Column::Quat(v) => v.get(i).copied(),
            _ => None,
        }
    }

    /// Element `e`'s value as a label shows it: a list as `[a, b]`.
    pub fn text(&self, e: usize) -> Option<String> {
        let n = self.element_size.max(1);
        let first = self.at(e)? * n;
        let one = |i: usize| -> Option<String> {
            Some(match &self.column {
                Column::Bool(v) => (*v.get(i)? as u8).to_string(),
                Column::Int(v) => v.get(i)?.to_string(),
                Column::Float(v) => short(*v.get(i)?),
                Column::Vec2(v) => list(v.get(i)?.iter().map(|x| short(*x))),
                Column::Vec3(v) => list(v.get(i)?.iter().map(|x| short(*x))),
                Column::Vec4(v) | Column::Quat(v) => list(v.get(i)?.iter().map(|x| short(*x))),
                Column::Mat4(v) => list(v.get(i)?.iter().map(|x| short(*x as f32))),
                Column::Token(v) => v.get(i)?.to_string(),
            })
        };
        if n == 1 {
            return one(first);
        }
        Some(list((first..first + n).filter_map(one)))
    }
}

/// A number as a label shows it: three decimals at most, no trailing zeros.
fn short(x: f32) -> String {
    let t = format!("{x:.3}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    match t {
        "-0" | "" => "0".into(),
        t => t.to_string(),
    }
}

fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(", "))
}

/// The attribute `name` of `geo`: its own, else an intrinsic of that name.
pub fn find(geo: &Geo, name: &str) -> Option<Found> {
    let name = name.trim();
    if let Some((context, a)) = geo.find(name) {
        let Attr {
            column,
            indices,
            role,
            element_size,
        } = a.clone();
        return Some(Found {
            context,
            column,
            role,
            indices,
            element_size,
        });
    }
    let context = xms_geo::intrinsics::context_of(geo, name)?;
    let column = xms_geo::intrinsics::column(geo, context, name, None)?;
    let role = if name.ends_with(":N") { Role::Normal } else { Role::None };
    Some(Found {
        context,
        column,
        role,
        indices: None,
        element_size: 1,
    })
}

/// Intrinsics the menu offers, with what they show.
pub const INTRINSICS: [(&str, &str); 8] = [
    ("intrinsic:ptIndex", "point numbers"),
    ("intrinsic:faceIndex", "face numbers"),
    ("intrinsic:localCornerIndex", "corner numbers, in their face"),
    ("intrinsic:N", "face normals"),
    ("intrinsic:area", "face areas"),
    ("intrinsic:ptIndices", "each face's points"),
    ("intrinsic:curveu", "how far along its curve"),
    ("intrinsic:length", "curve lengths"),
];

/// A prim's faces as its corners: where each face's corners start, and the
/// point of each corner. A curve's corners are its control points.
pub fn faces(geo: &Geo) -> (Vec<usize>, Vec<u32>) {
    match &geo.topology {
        Topology::Mesh { counts, indices, .. } => (starts(counts), indices.to_vec()),
        Topology::Curves { counts, .. } => (starts(counts), (0..geo.point_count() as u32).collect()),
        Topology::Points => (vec![0], vec![]),
    }
}

/// Where each face's corners start, and where the last ends.
fn starts(counts: &[u32]) -> Vec<usize> {
    let mut out = Vec::with_capacity(counts.len() + 1);
    let mut at = 0usize;
    out.push(0);
    for c in counts {
        at += *c as usize;
        out.push(at);
    }
    out
}

/// Where each element of `ctx` is drawn, in the prim's own space, and which
/// way it faces when that is known (a face's normal, its corners').
pub fn places(geo: &Geo, ctx: Context) -> Vec<(Vec3, Option<Vec3>)> {
    let pts = geo.points();
    let p = |i: u32| Vec3::from_array(pts.get(i as usize).copied().unwrap_or_default());
    let (start, corner_point) = faces(geo);
    let mesh = matches!(geo.topology, Topology::Mesh { .. });
    let face = |f: usize| -> (Vec3, Option<Vec3>) {
        let cs = &corner_point[start[f]..start[f + 1]];
        let centre = cs.iter().map(|&i| p(i)).sum::<Vec3>() / cs.len().max(1) as f32;
        let normal = mesh.then(|| {
            let mut n = Vec3::ZERO;
            for k in 0..cs.len() {
                n += p(cs[k]).cross(p(cs[(k + 1) % cs.len()]));
            }
            n.normalize_or_zero()
        });
        (centre, normal.filter(|n| *n != Vec3::ZERO))
    };
    match ctx {
        Context::Point => pts.iter().map(|x| (Vec3::from_array(*x), None)).collect(),
        Context::Face => (0..start.len() - 1).map(face).collect(),
        // A little towards its face's centre, so neighbouring corners part.
        Context::Corner => (0..start.len() - 1)
            .flat_map(|f| {
                let (centre, n) = face(f);
                corner_point[start[f]..start[f + 1]]
                    .iter()
                    .map(move |&i| (p(i).lerp(centre, 0.2), n))
                    .collect::<Vec<_>>()
            })
            .collect(),
        Context::Object => {
            let at = geo
                .bounds()
                .map(|(lo, hi)| (Vec3::from_array(lo) + Vec3::from_array(hi)) * 0.5)
                .unwrap_or(Vec3::ZERO);
            vec![(at, None)]
        }
    }
}

/// The range of a float attribute's values on the prims shown, low and high,
/// skipping those that are not numbers.
pub fn range(values: impl Iterator<Item = f32>) -> Option<(f32, f32)> {
    values.filter(|v| v.is_finite()).fold(None, |r, v| {
        Some(r.map_or((v, v), |(lo, hi): (f32, f32)| (lo.min(v), hi.max(v))))
    })
}

/// A float's colour through `ramp`, between `lo` and `hi`, as sRGB (0 to 1).
/// All one value (`lo` = `hi`): the middle of the ramp.
pub fn ramp_colour(ramp: Ramp, v: f32, lo: f32, hi: f32) -> [f32; 3] {
    let t = if hi > lo {
        ((v - lo) / (hi - lo)).clamp(0.0, 1.0)
    } else {
        0.5
    };
    match ramp {
        Ramp::Grey => [t, t, t],
        // Black to red over the first third, to yellow over the second, to
        // white over the last.
        Ramp::Heat => [
            (t * 3.0).min(1.0),
            (t * 3.0 - 1.0).clamp(0.0, 1.0),
            (t * 3.0 - 2.0).clamp(0.0, 1.0),
        ],
    }
}

/// A UV's colour: red by u, green by v, each its place in its tile.
pub fn uv_colour(u: f32, v: f32) -> [f32; 3] {
    let tile = |x: f32| if (0.0..=1.0).contains(&x) { x } else { x - x.floor() };
    [tile(u), tile(v), 0.0]
}

/// A linear colour packed as the Viewer's vertices hold it: four bytes, red
/// first, in a float's bits (read by the GPU as `Unorm8x4`, never as a
/// number).
pub fn packed(c: [f32; 4]) -> f32 {
    let b = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    f32::from_bits(u32::from_le_bytes([b(c[0]), b(c[1]), b(c[2]), b(c[3])]))
}

/// White, as packed: a vertex not coloured.
pub fn white() -> f32 {
    packed([1.0; 4])
}

/// A label over the picture: where, what, and which way its element faces.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub at: Vec3,
    pub text: String,
    pub facing: Option<Vec3>,
}

/// What attributes give a prim's drawing, in the world.
#[derive(Default)]
pub struct Drawn {
    /// The colour of each corner, linear, for its surface: a mesh's.
    pub corners: Option<Vec<[f32; 4]>>,
    /// Points coloured: position, linear colour and radius in pixels (the
    /// Viewer's dots), for points and curves.
    pub dots: Vec<f32>,
    /// Lines for vectors: x, y, z and a linear colour per vertex, in pairs.
    pub lines: Vec<f32>,
    pub labels: Vec<Label>,
    /// What was left out, past a limit.
    pub left_out: Vec<String>,
}

/// Each float attribute's range over the prims shown: the menu's, else the
/// values'.
pub fn ranges<'a>(shown: &[Shown], geos: impl Iterator<Item = &'a Geo> + Clone) -> Vec<Option<(f32, f32)>> {
    shown
        .iter()
        .map(|s| {
            let found = geos.clone().filter_map(|g| find(g, &s.name)).collect::<Vec<_>>();
            let auto = range(found.iter().flat_map(|f| {
                let n = f.column.len() / f.element_size.max(1);
                (0..n).filter_map(|e| f.float(e))
            }));
            match (s.min, s.max, auto) {
                (Some(lo), Some(hi), _) => Some((lo, hi)),
                (lo, hi, Some((a, b))) => Some((lo.unwrap_or(a), hi.unwrap_or(b))),
                (Some(lo), None, None) => Some((lo, lo)),
                (None, Some(hi), None) => Some((hi, hi)),
                (None, None, None) => None,
            }
        })
        .collect()
}

/// What `shown` (each with its range, from `ranges`) draws on `geo`, placed at
/// each of `places`. `budget`: lines and labels left to make, taken from.
pub fn drawn(
    geo: &Geo,
    shown: &[Shown],
    ranges: &[Option<(f32, f32)>],
    placed: &[Mat4],
    budget: &mut (usize, usize),
) -> Drawn {
    let mut out = Drawn::default();
    let mesh = matches!(geo.topology, Topology::Mesh { .. });
    for (s, r) in shown.iter().zip(ranges) {
        let Some(f) = find(geo, &s.name) else { continue };
        let look = f.look();
        let count = geo.count(f.context);
        let colour = |e: usize| -> Option<[f32; 4]> {
            let c = match look {
                Look::Ramp => {
                    let (lo, hi) = r.unwrap_or((0.0, 1.0));
                    ramp_colour(s.ramp, f.float(e)?, lo, hi)
                }
                Look::Uv => {
                    let v = f.floats(e)?;
                    uv_colour(v[0], v[1])
                }
                Look::Colour => {
                    let v = f.floats(e)?;
                    return Some([v[0], v[1], v[2], 1.0]);
                }
                _ => return None,
            };
            let l = super::scene::linear_f(c);
            Some([l[0], l[1], l[2], 1.0])
        };
        if look.colours() {
            // The first attribute that colours a prim colours it.
            if out.corners.is_some() || !out.dots.is_empty() {
                continue;
            }
            if mesh {
                let (start, corner_point) = faces(geo);
                let grey = [0.5, 0.5, 0.5, 1.0];
                let mut corners = Vec::with_capacity(corner_point.len());
                for face in 0..start.len() - 1 {
                    for c in start[face]..start[face + 1] {
                        let e = match f.context {
                            Context::Point => corner_point[c] as usize,
                            Context::Face => face,
                            Context::Corner => c,
                            Context::Object => 0,
                        };
                        corners.push(colour(e).unwrap_or(grey));
                    }
                }
                out.corners = Some(corners);
            } else if f.context == Context::Point || f.context == Context::Object {
                for m in placed {
                    for (e, x) in geo.points().iter().enumerate() {
                        let e = if f.context == Context::Object { 0 } else { e };
                        let Some(c) = colour(e) else { continue };
                        let at = m.transform_point3(Vec3::from_array(*x));
                        out.dots
                            .extend_from_slice(&[at.x, at.y, at.z, c[0], c[1], c[2], c[3], 4.0]);
                    }
                }
            }
            continue;
        }
        let at = places(geo, f.context);
        match look {
            Look::Vector => {
                for m in placed {
                    for (e, (p, _)) in at.iter().enumerate().take(count) {
                        let Some(v) = f.floats(e) else { continue };
                        if budget.0 == 0 {
                            out.left_out.push(format!("{}: past {LINE_LIMIT} lines", s.name));
                            break;
                        }
                        budget.0 -= 1;
                        let v = Vec3::new(v[0], v[1], v[2]);
                        let (a, b) = (m.transform_point3(*p), m.transform_point3(*p + v * s.scale));
                        // Coloured by which way it points.
                        let d = v.normalize_or_zero().abs();
                        let c = [0.25 + 0.75 * d.x, 0.25 + 0.75 * d.y, 0.25 + 0.75 * d.z, 1.0];
                        out.lines.extend_from_slice(&[a.x, a.y, a.z, c[0], c[1], c[2], c[3]]);
                        out.lines.extend_from_slice(&[b.x, b.y, b.z, c[0], c[1], c[2], c[3]]);
                    }
                }
            }
            _ => {
                for m in placed {
                    for (e, (p, n)) in at.iter().enumerate().take(count) {
                        let Some(text) = f.text(e) else { continue };
                        if budget.1 == 0 {
                            out.left_out.push(format!("{}: past {LABEL_LIMIT} labels", s.name));
                            break;
                        }
                        budget.1 -= 1;
                        out.labels.push(Label {
                            at: m.transform_point3(*p),
                            text,
                            facing: n.map(|n| m.transform_vector3(n).normalize_or_zero()),
                        });
                    }
                }
            }
        }
    }
    out.left_out.dedup();
    out
}

/// Whether `path` is shown under `scope`: every prim, or the selected ones and
/// those below them.
pub fn in_scope(scope: Scope, path: &str, selected: &[String]) -> bool {
    match scope {
        Scope::All => true,
        Scope::Selected => selected.iter().any(|s| {
            let s = s.trim_end_matches('/');
            path == s || path.strip_prefix(s).is_some_and(|rest| rest.starts_with('/'))
        }),
    }
}

/// A mesh's polygons as triangles in the world, placed at `m`: what hides
/// labels behind them. None for points and curves.
pub fn world_triangles(geo: &Geo, m: &Mat4) -> Vec<[Vec3; 3]> {
    if !matches!(geo.topology, Topology::Mesh { .. }) {
        return vec![];
    }
    let (start, corner_point) = faces(geo);
    let pts: Vec<Vec3> = geo
        .points()
        .iter()
        .map(|p| m.transform_point3(Vec3::from_array(*p)))
        .collect();
    let p = |c: usize| pts.get(corner_point[c] as usize).copied().unwrap_or_default();
    let mut out = Vec::with_capacity(corner_point.len());
    for f in 0..start.len() - 1 {
        for k in start[f] + 1..start[f + 1].saturating_sub(1) {
            out.push([p(start[f]), p(k), p(k + 1)]);
        }
    }
    out
}

/// The labels on screen drawn over the picture, nearest the camera first,
/// those behind a surface (`occluders`) or on one turned away left out, at
/// most `LABELS_DRAWN`. What was left out, when a limit was reached.
pub fn draw_labels(
    ui: &egui::Ui,
    rect: egui::Rect,
    view_proj: Mat4,
    eye: Vec3,
    labels: &[Label],
    occluders: Option<&super::occlusion::Occluders>,
) -> Option<String> {
    if labels.is_empty() {
        return None;
    }
    let mut on_screen: Vec<(f32, egui::Pos2, &Label)> = labels
        .iter()
        .filter(|l| l.facing.is_none_or(|n| n.dot(eye - l.at) >= 0.0))
        .filter_map(|l| {
            let p = view_proj * l.at.extend(1.0);
            if p.w <= 0.0 {
                return None;
            }
            let at = egui::pos2(
                rect.min.x + (p.x / p.w * 0.5 + 0.5) * rect.width(),
                rect.min.y + (0.5 - p.y / p.w * 0.5) * rect.height(),
            );
            rect.contains(at).then(|| (l.at.distance_squared(eye), at, l))
        })
        .collect();
    on_screen.sort_by(|a, b| a.0.total_cmp(&b.0));
    let painter = ui.painter_at(rect);
    let font = egui::FontId::monospace(10.0);
    let (mut drawn, mut tested) = (0, 0);
    for (_, at, l) in &on_screen {
        if drawn == LABELS_DRAWN || tested == LABELS_TESTED {
            return Some(format!(
                "Labels: the nearest {drawn} shown: select fewer prims or come closer"
            ));
        }
        tested += 1;
        if occluders.is_some_and(|o| o.hides(eye, l.at)) {
            continue;
        }
        drawn += 1;
        let galley = painter.layout_no_wrap(l.text.clone(), font.clone(), egui::Color32::WHITE);
        let frame = egui::Rect::from_center_size(*at, galley.size() + egui::vec2(3.0, 1.0));
        painter.rect_filled(frame, 1.5, egui::Color32::from_black_alpha(130));
        painter.galley(frame.min + egui::vec2(1.5, 0.5), galley, egui::Color32::WHITE);
    }
    None
}

/// The Attributes button at the Viewer's top left, under its bar: a square
/// A, lit while attributes are shown, opening the Attributes menu.
pub fn button(ctx: &egui::Context, rect: egui::Rect, names: impl FnOnce() -> Vec<String>) {
    egui::Area::new(egui::Id::new("viewer_attributes"))
        .order(egui::Order::Background)
        .fixed_pos(egui::pos2(rect.min.x + 10.0, rect.min.y + 42.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(crate::theme::c(90, 90, 90).gamma_multiply(0.92))
                .stroke(egui::Stroke::new(1.0_f32, crate::theme::c(70, 70, 70)))
                .corner_radius(4.0)
                .inner_margin(3.0)
                .show(ui, |ui| {
                    let on = settings().on;
                    let text = egui::RichText::new("A").strong();
                    let a = egui::Button::new(text).selected(on).min_size(egui::vec2(22.0, 22.0));
                    // Closing only on a click outside, as Imago's menus do.
                    let config = egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                    let (r, _) = egui::containers::menu::MenuButton::from_button(a)
                        .config(config)
                        .ui(ui, |ui| menu(ui, names));
                    r.on_hover_text(
                        "Attributes: draw attributes over the prims, floats as colours, colours and UVs as themselves, vectors as lines, the rest as labels",
                    );
                });
        });
}

/// The Attributes menu, from the Viewer's A: on or off, the scope, and the
/// list, each its name (typed, or chosen from `names`, the attributes of the
/// prims shown, made only while the menu is open, and the intrinsics), its
/// range and ramp, its lines' scale.
pub fn menu(ui: &mut egui::Ui, names: impl FnOnce() -> Vec<String>) {
    let mut s = settings();
    ui.checkbox(&mut s.on, "Show attributes")
        .on_hover_text("Draw the attributes listed over the prims: floats as colours, colours and UVs as themselves, vectors as lines, the rest as labels");
    ui.horizontal(|ui| {
        ui.label("On");
        ui.radio_value(&mut s.scope, Scope::Selected, "Selected")
            .on_hover_text("The prims selected, and those below them");
        ui.radio_value(&mut s.scope, Scope::All, "All")
            .on_hover_text("Every prim shown, within the limits");
    });
    ui.separator();
    let mut gone = None;
    let mut all_names: Option<Vec<String>> = None;
    let mut make = Some(names);
    for (k, a) in s.list.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut a.on, "");
            ui.add(
                egui::TextEdit::singleline(&mut a.name)
                    .desired_width(150.0)
                    .hint_text("attribute"),
            );
            ui.menu_button("⏷", |ui| {
                let list = all_names.get_or_insert_with(|| make.take().map(|f| f()).unwrap_or_default());
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for n in list.iter() {
                        if ui.button(n).clicked() {
                            a.name = n.clone();
                            ui.close();
                        }
                    }
                    ui.separator();
                    for (n, what) in INTRINSICS {
                        if ui.button(n).on_hover_text(what).clicked() {
                            a.name = n.to_string();
                            ui.close();
                        }
                    }
                });
            });
            egui::ComboBox::from_id_salt(("attr_ramp", k))
                .selected_text(match a.ramp {
                    Ramp::Grey => "Grey",
                    Ramp::Heat => "Heat",
                })
                .width(56.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut a.ramp, Ramp::Grey, "Grey");
                    ui.selectable_value(&mut a.ramp, Ramp::Heat, "Heat");
                })
                .response
                .on_hover_text("A float's colours, low to high");
            limit(ui, "Min", &mut a.min);
            limit(ui, "Max", &mut a.max);
            ui.add(egui::DragValue::new(&mut a.scale).speed(0.01).prefix("× "))
                .on_hover_text("A vector's line: its length times this");
            if ui.small_button("✕").clicked() {
                gone = Some(k);
            }
        });
    }
    if let Some(k) = gone {
        s.list.remove(k);
    }
    if ui.button("+ Attribute").clicked() {
        s.list.push(Shown::new(""));
    }
    set(s);
}

/// A range's end: automatic (the values shown) until ticked and typed.
fn limit(ui: &mut egui::Ui, label: &str, v: &mut Option<f32>) {
    let mut typed = v.is_some();
    if ui
        .checkbox(&mut typed, label)
        .on_hover_text(format!(
            "{label} of a float's range: ticked, as typed; else the values shown"
        ))
        .changed()
    {
        *v = typed.then_some(v.unwrap_or(0.0));
    }
    if let Some(x) = v {
        ui.add(egui::DragValue::new(x).speed(0.01));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Two quads side by side.
    fn quads() -> Geo {
        Geo::from_polygons(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [2.0, 0.0, 1.0],
            ],
            vec![4, 4],
            vec![0, 3, 4, 1, 1, 4, 5, 2],
        )
    }

    #[test]
    fn each_type_is_drawn_its_way() {
        let f = |c| look_of(&c, Role::None, 1);
        assert_eq!(f(Column::Float(Arc::new(vec![]))), Look::Ramp);
        assert_eq!(f(Column::Int(Arc::new(vec![]))), Look::Label);
        assert_eq!(f(Column::Vec2(Arc::new(vec![]))), Look::Uv);
        assert_eq!(f(Column::Vec3(Arc::new(vec![]))), Look::Vector);
        assert_eq!(look_of(&Column::Vec3(Arc::new(vec![])), Role::Color, 1), Look::Colour);
        assert_eq!(look_of(&Column::Vec3(Arc::new(vec![])), Role::Point, 1), Look::Label);
        assert_eq!(look_of(&Column::Float(Arc::new(vec![])), Role::None, 3), Look::Label);
    }

    #[test]
    fn floats_go_through_the_ramp_over_their_range() {
        assert_eq!(range([0.5, f32::NAN, 2.0, -1.0].into_iter()), Some((-1.0, 2.0)));
        assert_eq!(ramp_colour(Ramp::Grey, 0.75, 0.5, 1.0), [0.5, 0.5, 0.5]);
        assert_eq!(ramp_colour(Ramp::Grey, 3.0, 0.0, 1.0), [1.0; 3]);
        assert_eq!(ramp_colour(Ramp::Heat, 0.0, 0.0, 1.0), [0.0; 3]);
        assert_eq!(ramp_colour(Ramp::Heat, 1.0, 0.0, 1.0), [1.0; 3]);
        // All one value: the middle.
        assert_eq!(ramp_colour(Ramp::Grey, 4.0, 4.0, 4.0), [0.5; 3]);
        assert_eq!(uv_colour(0.25, 1.5), [0.25, 0.5, 0.0]);
        let typed = Shown {
            min: Some(10.0),
            ..Shown::new("heat")
        };
        let mut g = quads();
        g.set(
            Context::Point,
            "heat",
            Attr::new(Column::Float(Arc::new(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0])), Role::None),
        );
        assert_eq!(ranges(&[Shown::new("heat")], [&g].into_iter()), vec![Some((0.0, 5.0))]);
        assert_eq!(ranges(&[typed], [&g].into_iter()), vec![Some((10.0, 5.0))]);
    }

    #[test]
    fn a_face_attribute_colours_its_corners() {
        let mut g = quads();
        g.set(
            Context::Face,
            "mask",
            Attr::new(Column::Float(Arc::new(vec![0.0, 1.0])), Role::None),
        );
        let shown = [Shown::new("mask")];
        let r = ranges(&shown, [&g].into_iter());
        let mut budget = (LINE_LIMIT, LABEL_LIMIT);
        let d = drawn(&g, &shown, &r, &[Mat4::IDENTITY], &mut budget);
        let c = d.corners.unwrap();
        assert_eq!(c.len(), 8);
        assert!(c[..4].iter().all(|x| x[0] == 0.0) && c[4..].iter().all(|x| x[0] == 1.0));
        assert!(d.lines.is_empty() && d.labels.is_empty());
    }

    #[test]
    fn vectors_are_lines_and_numbers_are_labels() {
        let mut g = quads();
        g.set(
            Context::Point,
            "v",
            Attr::new(Column::Vec3(Arc::new(vec![[0.0, 1.0, 0.0]; 6])), Role::Vector),
        );
        let shown = [
            Shown {
                scale: 2.0,
                ..Shown::new("v")
            },
            Shown::new("intrinsic:faceIndex"),
            Shown::new("intrinsic:localCornerIndex"),
        ];
        let r = ranges(&shown, [&g].into_iter());
        let moved = Mat4::from_translation(Vec3::X * 10.0);
        let mut budget = (LINE_LIMIT, LABEL_LIMIT);
        let d = drawn(&g, &shown, &r, &[moved], &mut budget);
        assert_eq!(d.lines.len(), 6 * 2 * 7);
        // The first line: from point 0, placed, up by 2.
        assert_eq!(&d.lines[..3], &[10.0, 0.0, 0.0]);
        assert_eq!(&d.lines[7..10], &[10.0, 2.0, 0.0]);
        // Faces at their centres, facing their normal; corners towards them.
        assert_eq!(d.labels.len(), 2 + 8);
        assert_eq!(d.labels[0].text, "0");
        assert_eq!(d.labels[0].at, Vec3::new(10.5, 0.0, 0.5));
        assert!(d.labels[0].facing.is_some_and(|n| n.y.abs() > 0.99));
        assert!(d.labels[2].at.distance(Vec3::new(10.1, 0.0, 0.1)) < 1e-5);
        // Past the limit, what is left out is said.
        let mut little = (3, 1);
        let d = drawn(&g, &shown, &r, &[Mat4::IDENTITY], &mut little);
        assert_eq!((d.lines.len(), d.labels.len()), (3 * 2 * 7, 1));
        assert_eq!(d.left_out.len(), 3, "{:?}", d.left_out);
    }

    #[test]
    fn a_list_is_one_label_and_colours_pack_into_four_bytes() {
        let mut g = quads();
        g.set(
            Context::Point,
            "near",
            Attr::lists(Column::Int(Arc::new((0..12).collect())), 2, Role::None),
        );
        let f = find(&g, "near").unwrap();
        assert_eq!((f.look(), f.text(1).as_deref()), (Look::Label, Some("[2, 3]")));
        assert_eq!(short(0.5000), "0.5");
        assert_eq!(short(-0.0001), "0");
        assert_eq!(packed([1.0, 0.0, 0.5, 1.0]).to_bits().to_le_bytes(), [255, 0, 128, 255]);
    }

    /// A mesh's polygons as placed triangles hide labels behind them.
    #[test]
    fn labels_behind_a_surface_are_hidden() {
        let moved = Mat4::from_translation(Vec3::Y * 5.0);
        let tris = world_triangles(&quads(), &moved);
        assert_eq!(tris.len(), 4);
        assert!(tris.iter().flatten().all(|p| p.y == 5.0));
        let o = super::super::occlusion::Occluders::new(tris);
        let eye = Vec3::new(0.5, 10.0, 0.5);
        assert!(o.hides(eye, Vec3::new(0.5, 0.0, 0.5)));
        assert!(!o.hides(eye, Vec3::new(0.5, 5.0, 0.5)));
    }

    #[test]
    fn the_selected_scope_takes_what_is_below_them() {
        let sel = vec!["/set".to_string()];
        assert!(in_scope(Scope::Selected, "/set/a", &sel));
        assert!(!in_scope(Scope::Selected, "/settle", &sel));
        assert!(in_scope(Scope::All, "/settle", &sel));
    }
}
