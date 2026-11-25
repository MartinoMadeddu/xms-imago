//! Dockable panes: which panes exist, where they sit, and the layout file.
//!
//! Every pane is a tab of one dock area. Tabs can be dragged to another place,
//! stacked, or pulled out as floating windows.
//!
//! - **The default layout:** the one saved with Window > Layout > Save current
//!   as default, else `xms_main_layout`. Imago opens with it, its tabs in
//!   front as saved, and File > New goes back to it.
//! - **The layouts it comes with** (`PRESETS`, Window > Layout), and any saved
//!   to a file (Save layout..., Load layout...).
//! - **The lock** is kept from one run to the next (`layout.json`).
//! - **Tabs are instances** (`Tab`): a pane and a number of its own, so some
//!   panes can be open twice (`Pane::multiple`), each a tab. A tab's right-click
//!   menu changes what it shows (Pane) or opens another beside it (New Tab), as
//!   does the + of each tab bar. Layouts saved before name the panes alone: they
//!   load, each tab given its number.

use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    /// Was "Viewport": layouts saved with that name still load.
    #[serde(alias = "Viewport")]
    Viewer,
    NodeGraph,
    SceneExplorer,
    /// A pane Imago no longer has: the Operator Stack (the chain feeding the
    /// viewed operator, which the graph shows). Read from layouts saved with
    /// it, its tabs taken away as they load (`retired`); never shown.
    #[serde(alias = "OperatorStack")]
    Retired,
    /// Was "Properties".
    #[serde(alias = "Properties")]
    Parameters,
    PrimInspector,
    Timeline,
    UvEditor,
    History,
    /// The curves of the selected operator's keys. Layouts saved with
    /// "GraphEditor" still load.
    #[serde(alias = "GraphEditor")]
    CurveEditor,
    /// Renders and images: layers (AOVs), exposure, the values under the
    /// pointer.
    ImageViewer,
    /// The Light Linker's table, big, with the scene's lights beside it.
    LightLinker,
    /// Every operator by category, to click after the selected one or drag
    /// onto the Node Graph (`tools`).
    Tools,
    /// What is selected in the Viewer, its transform and edits, after XSI's
    /// main command panel (`selection_bar`).
    Selection,
}

impl Pane {
    pub const ALL: [Pane; 13] = [
        Pane::Viewer,
        Pane::NodeGraph,
        Pane::SceneExplorer,
        Pane::Parameters,
        Pane::PrimInspector,
        Pane::Timeline,
        Pane::UvEditor,
        Pane::History,
        Pane::CurveEditor,
        Pane::ImageViewer,
        Pane::LightLinker,
        Pane::Tools,
        Pane::Selection,
    ];

    /// How a tab of this pane chooses the operator it shows, unless told
    /// otherwise (`Tab::follows`). `None`: it shows no one operator.
    pub fn follows(self) -> Option<Follows> {
        match self {
            Pane::Parameters | Pane::CurveEditor | Pane::UvEditor => Some(Follows::Selection),
            // The scene the Viewer shows, unless told otherwise.
            Pane::SceneExplorer | Pane::PrimInspector => Some(Follows::Display),
            _ => None,
        }
    }

    /// Panes that can be open more than once, each a tab of its own. The
    /// others are one of a kind: what they show, and how, is the program's
    /// (one Viewer, one Timeline, one History), or they share what they draw
    /// with (the Image Viewer's image on the GPU).
    pub fn multiple(self) -> bool {
        !matches!(
            self,
            Pane::Viewer
                | Pane::Timeline
                | Pane::History
                | Pane::Retired
                | Pane::ImageViewer
                | Pane::Tools
                | Pane::Selection
        )
    }

    pub fn title(self) -> &'static str {
        match self {
            Pane::Viewer => "Viewer",
            Pane::NodeGraph => "Node Graph",
            Pane::SceneExplorer => "Scene Explorer",
            Pane::Retired => "Retired",
            Pane::Parameters => "Parameters",
            Pane::PrimInspector => "Primitive Inspector",
            Pane::Timeline => "Timeline",
            Pane::UvEditor => "UV Editor",
            Pane::History => "History",
            Pane::CurveEditor => "Curve Editor",
            Pane::ImageViewer => "Image Viewer",
            Pane::LightLinker => "Light Linker",
            Pane::Tools => "Tools",
            Pane::Selection => "Selection",
        }
    }
}

/// What a tab showing an operator shows (`Pane::follows`): the operator
/// selected, the one with the display flag, or one it is pinned to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Follows {
    Selection,
    Display,
    /// `node`, of the graph at `level` (`level_of` the compounds entered):
    /// shown only there.
    Pinned {
        node: crate::types::NodeId,
        level: u64,
    },
}

/// The compounds entered (`Inside::path`), as one number a pin keeps.
pub fn level_of(path: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish()
}

/// One open pane: which, and its own number (unique in the layout), so two
/// of a kind are two tabs; what it follows, for a pane that follows
/// (`None`: the pane's own way, `Pane::follows`). Written `{"id": 3, "pane":
/// "Parameters"}`; read from the pane's name alone too, as layouts saved
/// before had it (numbered when the layout loads, `numbered`). Two tabs are
/// the same by their number.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(from = "TabFile")]
pub struct Tab {
    pub id: u32,
    pub pane: Pane,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follows: Option<Follows>,
}

impl PartialEq for Tab {
    fn eq(&self, other: &Tab) -> bool {
        self.id == other.id
    }
}

impl Eq for Tab {}

impl std::hash::Hash for Tab {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.id.hash(h);
    }
}

/// A tab as a layout file has it: today's, or a pane's name.
#[derive(Deserialize)]
#[serde(untagged)]
enum TabFile {
    Pane(Pane),
    Tab {
        id: u32,
        pane: Pane,
        #[serde(default)]
        follows: Option<Follows>,
    },
}

impl From<TabFile> for Tab {
    fn from(t: TabFile) -> Tab {
        match t {
            // Numbered once the whole layout is read.
            TabFile::Pane(pane) => Tab {
                id: 0,
                pane,
                follows: None,
            },
            TabFile::Tab { id, pane, follows } => Tab { id, pane, follows },
        }
    }
}

/// The next tab's number: above every one in a layout loaded.
static NEXT_TAB: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

impl Tab {
    /// A new tab of `pane`, a number of its own.
    pub fn new(pane: Pane) -> Tab {
        Tab {
            id: NEXT_TAB.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            pane,
            follows: None,
        }
    }

    /// What it follows: its own choice, else its pane's way. `None` for a
    /// pane that follows nothing.
    pub fn follows(&self) -> Option<Follows> {
        let own = self.pane.follows()?;
        Some(self.follows.unwrap_or(own))
    }
}

/// Every tab a number of its own: those read without one (0), or with one
/// another has, get a new one; new tabs are numbered above them all.
/// The tabs of panes Imago no longer has (`Pane::Retired`) taken away; the
/// panes beside them take their room.
fn retired(dock: &mut DockState<Tab>) {
    while let Some(at) = dock.find_tab_from(|t| t.pane == Pane::Retired) {
        dock.remove_tab(at);
    }
}

fn numbered(dock: &mut DockState<Tab>) {
    use std::sync::atomic::Ordering;
    let top = dock.iter_all_tabs().map(|(_, t)| t.id).max().unwrap_or(0);
    NEXT_TAB.fetch_max(top + 1, Ordering::Relaxed);
    let mut seen = std::collections::HashSet::new();
    for (_, t) in dock.iter_all_tabs_mut() {
        if t.id == 0 || !seen.insert(t.id) {
            t.id = NEXT_TAB.fetch_add(1, Ordering::Relaxed);
            seen.insert(t.id);
        }
    }
}

/// Where the first tab of `pane` is in a tree of a surface: its node and its
/// place among the node's tabs.
fn find_in(tree: &egui_dock::Tree<Tab>, pane: Pane) -> Option<(NodeIndex, usize)> {
    tree.iter().enumerate().find_map(|(i, node)| {
        let k = node.tabs()?.iter().position(|t| t.pane == pane)?;
        Some((NodeIndex(i), k))
    })
}

/// Whether, with `open` the tabs open, a tab of `pane` can be added: a pane
/// that can be open twice, or one of a kind while none is open.
pub fn may_add(open: &[Tab], pane: Pane) -> bool {
    pane.multiple() || !open.iter().any(|t| t.pane == pane)
}

/// Whether, with `open` the tabs open, tab `tab` can show `pane` instead: its
/// own, one that can be open twice, or one of a kind no other tab shows.
pub fn may_become(open: &[Tab], tab: Tab, pane: Pane) -> bool {
    tab.pane == pane || pane.multiple() || !open.iter().any(|t| t.pane == pane && t.id != tab.id)
}

/// What tabs keep of their own, beyond the first of their pane (which uses
/// the program's): a Node Graph tab's view and the graph it is in (ICE,
/// MaterialX), a Curve Editor's, a UV Editor's. Forgotten when the tab closes.
#[derive(Default)]
pub struct TabStates {
    /// The Parameters down the right of the Node Graph in use (P over it),
    /// and their width (0: the first one).
    pub parameters_over: bool,
    pub parameters_width: f32,
    /// Node Graph tabs not the one last used: their views, swapped into the
    /// graph while each is drawn.
    pub graphs: std::collections::HashMap<u32, (crate::node_graph::GraphView, crate::ice::GraphNavigation)>,
    /// The Node Graph tab last used: its view is the graph's own, its graph
    /// the one Parameters shows the nodes of.
    pub active_graph: Option<u32>,
    pub curves: std::collections::HashMap<u32, crate::curve_editor::CurveEditor>,
    pub uvs: std::collections::HashMap<u32, crate::uv_editor::UvEditorState>,
    /// Scene Explorer tabs following another operator than the one with the
    /// display flag: its rows, and what they were listed for (the graph's
    /// revision, the operator).
    pub explorers: std::collections::HashMap<u32, ((u64, crate::types::NodeId), crate::types::Rows)>,
}

impl TabStates {
    /// Each kept only while its tab is open. The Node Graph tab last used
    /// closed, the first open takes its place, its view put into the graph.
    pub fn tidy(
        &mut self,
        open: &[Tab],
        graph: &mut crate::node_graph::NodeGraphState,
        nav: &mut crate::ice::GraphNavigation,
    ) {
        let is_open = |id: u32| open.iter().any(|t| t.id == id);
        if self.active_graph.is_some_and(|a| !is_open(a)) {
            self.active_graph = open.iter().find(|t| t.pane == Pane::NodeGraph).map(|t| t.id);
            if let Some((mut view, mut at)) = self.active_graph.and_then(|id| self.graphs.remove(&id)) {
                graph.swap_view(&mut view);
                std::mem::swap(nav, &mut at);
            }
        }
        self.graphs.retain(|id, _| is_open(*id));
        self.curves.retain(|id, _| is_open(*id));
        self.uvs.retain(|id, _| is_open(*id));
        self.explorers.retain(|id, _| is_open(*id));
    }
}

/// A side of the program window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    /// The edge within `reach` of a point, if any. The nearest one wins.
    pub fn near(screen: egui::Rect, p: egui::Pos2, reach: f32) -> Option<Edge> {
        [
            (Edge::Left, p.x - screen.left()),
            (Edge::Right, screen.right() - p.x),
            (Edge::Top, p.y - screen.top()),
            (Edge::Bottom, screen.bottom() - p.y),
        ]
        .into_iter()
        .filter(|(_, d)| *d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e)
    }

    /// Share of the window a pane docked on this edge starts with.
    pub fn share(self) -> f32 {
        match self {
            Edge::Left | Edge::Right => 0.22,
            Edge::Top | Edge::Bottom => 0.16,
        }
    }

    /// The strip of the window a pane docked on this edge will take.
    pub fn strip(self, screen: egui::Rect) -> egui::Rect {
        let (w, h) = (screen.width() * self.share(), screen.height() * self.share());
        let mut r = screen;
        match self {
            Edge::Left => r.set_right(screen.left() + w),
            Edge::Right => r.set_left(screen.right() - w),
            Edge::Top => r.set_bottom(screen.top() + h),
            Edge::Bottom => r.set_top(screen.bottom() - h),
        }
        r
    }
}

pub struct Layout {
    pub dock: DockState<Tab>,
    /// Locked: tabs cannot be moved, floated or closed. Dividers still resize.
    pub locked: bool,
    /// The layout as last written to disk.
    saved: String,
    /// Floating windows that have been given their starting size.
    sized: Vec<usize>,
    /// The tab the mouse went down on, and whether it has moved since.
    pub tab_drag: Option<(Tab, bool)>,
}

/// The layouts Imago comes with, in Window > Layout: name (its file's),
/// the file. The first is the default until another is saved as default.
pub const PRESETS: [(&str, &str); 4] = [
    (
        "xms_main_layout",
        include_str!("../assets/layouts/xms_main_layout.json"),
    ),
    ("xms_xsi_layout", include_str!("../assets/layouts/xms_xsi_layout.json")),
    (
        "xms_2split_layout",
        include_str!("../assets/layouts/xms_2split_layout.json"),
    ),
    (
        "xms_4split_layout",
        include_str!("../assets/layouts/xms_4split_layout.json"),
    ),
];

/// A layout Imago comes with, by name. Its lock is not taken: choosing a
/// layout does not lock or unlock.
pub fn preset(name: &str) -> Option<DockState<Tab>> {
    PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, text)| from_json(text))
        .map(|(dock, _)| dock)
}

/// `xms_main_layout`: the default when none is saved.
pub fn main_dock() -> DockState<Tab> {
    preset(PRESETS[0].0).unwrap_or_else(built_dock)
}

/// The file the layout saved as default is kept in.
fn default_file() -> Option<std::path::PathBuf> {
    crate::file_browser::config_path("default_layout.json")
}

/// The layout saved as default, if one is and it reads.
fn saved_default() -> Option<DockState<Tab>> {
    let text = std::fs::read_to_string(default_file()?).ok()?;
    from_json(&text).map(|(dock, _)| dock)
}

/// Whether a layout has been saved as default.
pub fn has_saved_default() -> bool {
    saved_default().is_some()
}

/// The layout Imago opens with, and File > New and Reset go back to: the
/// one saved as default, else `xms_main_layout`.
pub fn default_dock() -> DockState<Tab> {
    saved_default().unwrap_or_else(main_dock)
}

static TO_DEFAULT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ask for the default layout on the next frame (File > New).
pub fn ask_for_default() {
    TO_DEFAULT.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the default layout was asked for since the last call.
pub fn take_default_request() -> bool {
    TO_DEFAULT.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// Scene Explorer in a narrow column at the far left, the viewport beside it
/// with the UV editor and the Image Viewer as tabs behind, the primitive
/// inspector under both. Then the node graph, then the Parameters. Timeline
/// along the bottom of those, History down the whole right side. The Curve
/// Editor is a tab behind the node graph, the Light Linker behind the
/// primitive inspector. Built in code: what is used if `xms_main_layout`
/// could not be read.
pub fn built_dock() -> DockState<Tab> {
    let tabs = |panes: &[Pane]| panes.iter().map(|p| Tab::new(*p)).collect::<Vec<Tab>>();
    let mut dock = DockState::new(tabs(&[Pane::Viewer, Pane::UvEditor, Pane::ImageViewer]));
    let s = dock.main_surface_mut();
    let [work, _history] = s.split_right(NodeIndex::root(), 0.928, tabs(&[Pane::History]));
    let [top, _timeline] = s.split_below(work, 0.862, tabs(&[Pane::Timeline]));
    let [left, right] = s.split_right(top, 0.476, tabs(&[Pane::NodeGraph, Pane::CurveEditor]));
    let [view, _prim] = s.split_below(left, 0.72, tabs(&[Pane::PrimInspector, Pane::LightLinker]));
    s.split_left(view, 0.163, tabs(&[Pane::SceneExplorer]));
    s.split_right(right, 0.648, tabs(&[Pane::Parameters]));
    dock
}

/// After XSI: the Tools in a narrow column down the whole left side, the
/// Selection bar down the whole right side, then the Viewer over the Scene
/// Explorer (the Primitive Inspector, Light Linker
/// and History behind it), and the Node Graph, wide (UV and Curve Editors
/// behind): its Parameters float over it, on P. The Timeline along the
/// bottom. No Parameters pane: Window > Parameters docks one. What
/// `xms_xsi_layout.json` was written from.
pub fn xsi_dock() -> DockState<Tab> {
    let tabs = |panes: &[Pane]| panes.iter().map(|p| Tab::new(*p)).collect::<Vec<Tab>>();
    let mut dock = DockState::new(tabs(&[Pane::Viewer, Pane::ImageViewer]));
    let s = dock.main_surface_mut();
    // The Tools the whole height, the Timeline beside them.
    let [work, _tools] = s.split_left(NodeIndex::root(), 0.05, tabs(&[Pane::Tools]));
    let [work, _selection] = s.split_right(work, 0.915, tabs(&[Pane::Selection]));
    let [work, _timeline] = s.split_below(work, 0.871, tabs(&[Pane::Timeline]));
    let [view, _graph] = s.split_right(work, 0.45, tabs(&[Pane::NodeGraph, Pane::UvEditor, Pane::CurveEditor]));
    s.split_below(
        view,
        0.63,
        tabs(&[
            Pane::SceneExplorer,
            Pane::PrimInspector,
            Pane::LightLinker,
            Pane::History,
        ]),
    );
    dock
}

/// A layout is usable when no tab number is in it twice, nor a pane of one
/// of a kind (`Pane::multiple`). Panes that are missing are closed, and can be
/// shown again from the Panes menu.
pub fn is_valid(dock: &DockState<Tab>) -> bool {
    let tabs: Vec<Tab> = dock.iter_all_tabs().map(|(_, t)| *t).collect();
    let ids: std::collections::HashSet<u32> = tabs.iter().map(|t| t.id).collect();
    ids.len() == tabs.len()
        && Pane::ALL
            .iter()
            .filter(|p| !p.multiple())
            .all(|p| tabs.iter().filter(|t| t.pane == *p).count() <= 1)
}

/// The layout as text. Rectangles of panes that have not been laid out yet are
/// infinite, which JSON cannot hold: they are written as zero. They are worked
/// out again on the first frame anyway.
pub fn to_json(dock: &DockState<Tab>, locked: bool) -> Option<String> {
    fn fix(v: &mut serde_json::Value, in_rect: bool) {
        match v {
            serde_json::Value::Null if in_rect => *v = serde_json::json!(0.0),
            serde_json::Value::Array(a) => {
                for x in a {
                    fix(x, in_rect);
                }
            }
            serde_json::Value::Object(o) => {
                for (k, x) in o.iter_mut() {
                    fix(x, in_rect || k == "rect" || k == "viewport");
                }
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(dock).ok()?;
    fix(&mut value, false);
    serde_json::to_string(&serde_json::json!({ "locked": locked, "dock": value })).ok()
}

/// Layout and lock state from text. Files written before the lock existed hold
/// the layout alone.
pub fn from_json(json: &str) -> Option<(DockState<Tab>, bool)> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let (dock, locked) = match value.get("dock") {
        Some(d) => (
            d.clone(),
            value.get("locked").and_then(|l| l.as_bool()).unwrap_or(false),
        ),
        None => (value, false),
    };
    let mut dock: DockState<Tab> = serde_json::from_value(dock).ok()?;
    retired(&mut dock);
    numbered(&mut dock);
    is_valid(&dock).then_some((dock, locked))
}

/// Height of the bar with the close button at the top of a floating window.
const CLOSE_BAR: f32 = 17.0;

fn file() -> Option<std::path::PathBuf> {
    crate::file_browser::config_path("layout.json")
}

impl Default for Layout {
    /// The default layout, with its tabs in front as saved; locked if Imago
    /// was locked when it closed.
    fn default() -> Self {
        let last = file().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
        let locked = from_json(&last).map(|(_, locked)| locked).unwrap_or(false);
        let mut layout = Self {
            dock: default_dock(),
            locked,
            saved: String::new(),
            sized: vec![],
            tab_drag: None,
        };
        layout.restore_windows();
        layout
    }
}

impl Layout {
    /// Back to the default layout (Reset layout, File > New).
    pub fn reset(&mut self) {
        self.set_dock(default_dock());
    }

    /// One of the layouts Imago comes with, by name.
    pub fn use_preset(&mut self, name: &str) -> Result<(), String> {
        let dock = preset(name).ok_or_else(|| format!("no layout {name}"))?;
        self.set_dock(dock);
        Ok(())
    }

    /// This layout as the default: Imago opens with it, File > New and Reset
    /// go back to it.
    pub fn save_as_default(&self) -> Result<(), String> {
        let f = default_file().ok_or("no config folder")?;
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        self.save_as(&f)
    }

    /// Forget the layout saved as default: `xms_main_layout` is again.
    pub fn forget_default() -> Result<(), String> {
        match default_file() {
            Some(f) if f.exists() => std::fs::remove_file(f).map_err(|e| e.to_string()),
            _ => Ok(()),
        }
    }

    /// Another layout in place of this one, its floating windows where it
    /// has them.
    fn set_dock(&mut self, dock: DockState<Tab>) {
        self.dock = dock;
        self.sized.clear();
        self.restore_windows();
    }

    /// Content rectangle of a floating window, once it has been drawn.
    fn window_rect(&self, surface: usize) -> Option<egui::Rect> {
        let tree = self.dock.get_surface(egui_dock::SurfaceIndex(surface))?.node_tree()?;
        let r = tree.root_node()?.rect()?;
        (r.is_finite() && r.width() > 1.0 && r.height() > 1.0).then_some(r)
    }

    /// Surfaces that are floating windows right now.
    fn windows(&self) -> Vec<usize> {
        (1..self.dock.surfaces_count())
            .filter(|i| {
                self.dock
                    .get_surface(egui_dock::SurfaceIndex(*i))
                    .and_then(|s| s.node_tree())
                    .is_some()
            })
            .collect()
    }

    /// Put floating windows back where the layout file says they were. The file
    /// holds the rectangle of each window's content; the window itself starts
    /// above and left of it by its frame and its close bar.
    fn restore_windows(&mut self) {
        for i in self.windows() {
            let Some(r) = self.window_rect(i) else { continue };
            if let Some(state) = self.dock.get_window_state_mut(egui_dock::SurfaceIndex(i)) {
                state.set_position(r.min - egui::vec2(7.0, 24.0));
                state.set_size(r.size() + egui::vec2(0.0, CLOSE_BAR));
            }
            self.sized.push(i);
        }
    }

    /// Give a window that was just pulled out a workable size. Left alone it
    /// takes the size of the pane it came from, which can be taller than the
    /// screen. Call once a frame, before the panes are drawn.
    pub fn size_new_windows(&mut self) {
        let now = self.windows();
        for i in &now {
            if self.sized.contains(i) {
                continue;
            }
            if let Some(state) = self.dock.get_window_state_mut(egui_dock::SurfaceIndex(*i)) {
                state.set_size(egui::vec2(440.0, 480.0));
            }
        }
        self.sized = now;
    }

    /// The Timeline, docked above or below other panes, is kept to the height
    /// it needs: `need` points of its own (see `timeline::ui::need`) and
    /// `margin`, its body's top and bottom margins, under its tab bar. An
    /// absolute height, not a correction of the last one, so it settles at
    /// once. Not while it shares its place with other tabs, nor floating.
    pub fn fit_timeline(&mut self, need: f32, margin: f32) {
        let tree = &mut self.dock[egui_dock::SurfaceIndex::main()];
        let Some((node, _)) = find_in(tree, Pane::Timeline) else {
            return;
        };
        let (rect, viewport) = match &tree[node] {
            egui_dock::Node::Leaf(l) if l.tabs.len() == 1 => (l.rect, l.viewport),
            _ => return,
        };
        if !rect.is_finite() || !viewport.is_finite() {
            return;
        }
        let want = (rect.height() - viewport.height()) + need + margin;
        if (rect.height() - want).abs() < 1.5 {
            return;
        }
        let mut at = node;
        while let Some(p) = at.parent() {
            if let egui_dock::Node::Vertical(split) = &mut tree[p] {
                let h = split.rect.height();
                if !h.is_finite() || h <= want + 40.0 {
                    return;
                }
                let share = want / h;
                split.fraction = if p.left() == at { share } else { 1.0 - share };
                return;
            }
            at = p;
        }
    }

    /// The first tab of `pane`, if one is open.
    pub fn first(&self, pane: Pane) -> Option<Tab> {
        self.dock.iter_all_tabs().map(|(_, t)| *t).find(|t| t.pane == pane)
    }

    /// Whether any tab of `pane` is open.
    pub fn is_open(&self, pane: Pane) -> bool {
        self.first(pane).is_some()
    }

    /// True when a tab of `pane` sits in a floating window.
    pub fn is_floating(&self, pane: Pane) -> bool {
        self.dock
            .iter_all_tabs()
            .any(|(at, t)| t.pane == pane && !at.surface.is_main())
    }

    fn tab_floats(&self, tab: Tab) -> bool {
        self.dock.find_tab(&tab).is_some_and(|at| !at.surface.is_main())
    }

    pub fn any_floating(&self) -> bool {
        Pane::ALL.iter().any(|p| self.is_floating(*p))
    }

    /// Close every tab of a pane. It can be shown again with `show`.
    pub fn hide(&mut self, pane: Pane) {
        while let Some(at) = self.dock.find_tab_from(|t| t.pane == pane) {
            self.dock.remove_tab(at);
        }
    }

    fn remove(&mut self, tab: Tab) {
        if let Some(at) = self.dock.find_tab(&tab) {
            self.dock.remove_tab(at);
        }
    }

    /// Show a closed pane, as a tab of the main area.
    pub fn show(&mut self, pane: Pane) {
        if self.is_open(pane) {
            return;
        }
        self.dock.main_surface_mut().push_to_focused_leaf(Tab::new(pane));
    }

    /// Whether a tab of `pane` can be added (`add_tab`): see `may_add`.
    pub fn may_add(&self, pane: Pane) -> bool {
        may_add(&self.tabs(), pane)
    }

    /// Whether tab `tab` can show `pane` instead: see `may_become`.
    pub fn may_become(&self, tab: Tab, pane: Pane) -> bool {
        may_become(&self.tabs(), tab, pane)
    }

    /// Every open tab.
    pub fn tabs(&self) -> Vec<Tab> {
        self.dock.iter_all_tabs().map(|(_, t)| *t).collect()
    }

    /// A new tab of `pane` beside the tabs at `at` (a tab bar), in front;
    /// anywhere when that place is gone. Nothing when `may_add` says no.
    pub fn add_tab(&mut self, pane: Pane, at: Option<egui_dock::NodePath>) {
        if !self.may_add(pane) {
            return;
        }
        let tab = Tab::new(pane);
        match at.and_then(|at| self.dock.leaf_mut(at).ok()) {
            Some(leaf) => leaf.append_tab(tab),
            None => self.dock.main_surface_mut().push_to_focused_leaf(tab),
        }
    }

    /// Bring a floating tab back into the main area.
    pub fn dock_tab(&mut self, tab: Tab) {
        if !self.tab_floats(tab) {
            return;
        }
        self.remove(tab);
        self.dock.main_surface_mut().push_to_focused_leaf(tab);
    }

    /// Pull a docked tab out as a floating window.
    pub fn float_tab(&mut self, tab: Tab) {
        if self.dock.find_tab(&tab).is_none() || self.tab_floats(tab) {
            return;
        }
        self.remove(tab);
        self.dock.add_window(vec![tab]);
    }

    pub fn toggle_float_tab(&mut self, tab: Tab) {
        if self.tab_floats(tab) {
            self.dock_tab(tab);
        } else {
            self.float_tab(tab);
        }
    }

    /// Bring a pane's floating tabs back into the main area.
    pub fn dock_pane(&mut self, pane: Pane) {
        let floating: Vec<Tab> = self
            .dock
            .iter_all_tabs()
            .filter(|(at, t)| t.pane == pane && !at.surface.is_main())
            .map(|(_, t)| *t)
            .collect();
        for t in floating {
            self.dock_tab(t);
        }
    }

    /// Pull a pane's first tab out as a floating window.
    pub fn float_pane(&mut self, pane: Pane) {
        if let Some(t) = self.first(pane) {
            self.float_tab(t);
        }
    }

    pub fn toggle_float(&mut self, pane: Pane) {
        if let Some(t) = self.first(pane) {
            self.toggle_float_tab(t);
        }
    }

    /// Dock a tab along a whole side of the window, next to everything else:
    /// the place the timeline has in the default layout.
    pub fn dock_tab_to_edge(&mut self, tab: Tab, edge: Edge) {
        use egui_dock::{Node, Split};
        self.remove(tab);
        let tree = self.dock.main_surface_mut();
        if tree.root_node().map(|n| n.is_empty()).unwrap_or(true) {
            tree.push_to_first_leaf(tab);
            return;
        }
        // The fraction is the share of the left or upper part.
        let (split, fraction) = match edge {
            Edge::Left => (Split::Left, edge.share()),
            Edge::Top => (Split::Above, edge.share()),
            Edge::Right => (Split::Right, 1.0 - edge.share()),
            Edge::Bottom => (Split::Below, 1.0 - edge.share()),
        };
        tree.split(NodeIndex::root(), split, fraction, Node::leaf(tab));
    }

    /// A pane's first tab (a new one when none is open) along a side.
    pub fn dock_to_edge(&mut self, pane: Pane, edge: Edge) {
        let tab = self.first(pane).unwrap_or_else(|| Tab::new(pane));
        self.dock_tab_to_edge(tab, edge);
    }

    pub fn dock_all(&mut self) {
        for pane in Pane::ALL {
            self.dock_pane(pane);
        }
    }

    /// Write the layout to a file of the user's choosing.
    pub fn save_as(&self, path: &std::path::Path) -> Result<(), String> {
        let text = to_json(&self.dock, self.locked).ok_or("the layout could not be written as text")?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }

    /// Replace the layout with one read from a file. The current layout is kept
    /// when the file is not a layout.
    pub fn load_from(&mut self, path: &std::path::Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let (dock, locked) = from_json(&text).ok_or("not a layout file")?;
        self.locked = locked;
        self.set_dock(dock);
        Ok(())
    }

    /// Write the layout if it changed since the last write.
    pub fn save_if_changed(&mut self) {
        let Some(text) = to_json(&self.dock, self.locked) else {
            return;
        };
        if text == self.saved {
            return;
        }
        if let Some(f) = file() {
            if let Some(dir) = f.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(f, &text);
        }
        self.saved = text;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            dock: main_dock(),
            locked: false,
            saved: String::new(),
            sized: vec![],
            tab_drag: None,
        }
    }
    /// The panes open in the default layout: every one but the Tools and the
    /// Selection bar.
    const ALL: usize = Pane::ALL.len() - 2;
    /// The first tab of `pane`, wherever it is.
    fn find(dock: &DockState<Tab>, pane: Pane) -> Option<egui_dock::TabPath> {
        dock.find_tab_from(|t| t.pane == pane)
    }
    fn open(l: &Layout) -> usize {
        Pane::ALL.iter().filter(|p| l.is_open(**p)).count()
    }

    #[test]
    fn layout_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("xms_layout_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("mine.json");
        let mut a = layout();
        a.hide(Pane::UvEditor);
        a.locked = true;
        a.save_as(&file).unwrap();
        let mut b = layout();
        b.load_from(&file).unwrap();
        assert!(b.locked && !b.is_open(Pane::UvEditor) && open(&b) == ALL - 1);
        // Something that is not a layout leaves the layout alone.
        std::fs::write(&file, "{\"nodes\": []}").unwrap();
        assert!(b.load_from(&file).is_err());
        assert!(b.locked && open(&b) == ALL - 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_layout_has_every_pane_once() {
        let l = layout();
        assert!(is_valid(&l.dock));
        assert_eq!(open(&l), ALL);
        assert!(!l.any_floating());
    }

    #[test]
    fn layout_and_lock_survive_the_file() {
        let mut l = layout();
        // Stack the properties with the node graph, close the timeline, float the inspector.
        let at = find(&l.dock, Pane::Parameters).unwrap();
        let tab = l.dock.remove_tab(at).unwrap();
        let at = find(&l.dock, Pane::NodeGraph).unwrap();
        l.dock.leaf_mut(at.node_path()).unwrap().append_tab(tab);
        l.hide(Pane::Timeline);
        l.hide(Pane::PrimInspector);
        l.dock.add_window(vec![Tab::new(Pane::PrimInspector)]);
        assert!(is_valid(&l.dock));
        assert!(l.is_floating(Pane::PrimInspector) && !l.is_open(Pane::Timeline));

        let json = to_json(&l.dock, true).unwrap();
        let (back, locked) = from_json(&json).expect("layout loads");
        assert!(locked);
        assert_eq!(to_json(&back, true).unwrap(), json);
        let back = Layout {
            dock: back,
            locked,
            saved: json,
            sized: vec![],
            tab_drag: None,
        };
        let a = find(&back.dock, Pane::Parameters).unwrap();
        let b = find(&back.dock, Pane::NodeGraph).unwrap();
        assert_eq!((a.surface, a.node), (b.surface, b.node));
        assert!(back.is_floating(Pane::PrimInspector));
        assert!(!back.is_open(Pane::Timeline));
        assert_eq!(open(&back), ALL - 1);
    }

    #[test]
    fn panes_close_reopen_and_dock_back() {
        let mut l = layout();
        l.hide(Pane::Viewer);
        l.hide(Pane::Viewer);
        assert_eq!(open(&l), ALL - 1);
        l.show(Pane::Viewer);
        l.show(Pane::Viewer);
        assert_eq!(open(&l), ALL);
        assert!(is_valid(&l.dock));

        l.hide(Pane::Parameters);
        l.dock.add_window(vec![Tab::new(Pane::Parameters)]);
        l.hide(Pane::NodeGraph);
        l.dock.add_window(vec![Tab::new(Pane::NodeGraph)]);
        assert!(l.is_floating(Pane::Parameters) && l.any_floating());
        l.dock_pane(Pane::Parameters);
        assert!(!l.is_floating(Pane::Parameters) && l.is_floating(Pane::NodeGraph));
        l.dock_all();
        assert!(!l.any_floating());
        l.toggle_float(Pane::Timeline);
        assert!(l.is_floating(Pane::Timeline));
        l.toggle_float(Pane::Timeline);
        assert!(!l.is_floating(Pane::Timeline));
        assert_eq!(open(&l), ALL);

        // Everything closed, then one shown again.
        for p in Pane::ALL {
            l.hide(p);
        }
        assert_eq!(open(&l), 0);
        l.show(Pane::NodeGraph);
        assert!(l.is_open(Pane::NodeGraph) && !l.is_floating(Pane::NodeGraph));
        // Back to the main layout (`reset` takes the one saved as default).
        l.set_dock(main_dock());
        assert_eq!(open(&l), ALL);
    }

    #[test]
    fn panes_dock_along_a_whole_edge() {
        use egui::{pos2, Rect};
        let screen = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
        assert_eq!(Edge::near(screen, pos2(500.0, 795.0), 30.0), Some(Edge::Bottom));
        assert_eq!(Edge::near(screen, pos2(4.0, 400.0), 30.0), Some(Edge::Left));
        assert_eq!(Edge::near(screen, pos2(990.0, 20.0), 30.0), Some(Edge::Right));
        assert_eq!(Edge::near(screen, pos2(500.0, 400.0), 30.0), None);
        assert_eq!(Edge::Bottom.strip(screen).top(), 800.0 - 800.0 * 0.16);

        for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
            let mut l = layout();
            // Float the timeline, then put it back on an edge.
            l.float_pane(Pane::Timeline);
            l.dock_to_edge(Pane::Timeline, edge);
            assert!(is_valid(&l.dock) && !l.is_floating(Pane::Timeline));
            assert_eq!(open(&l), ALL);
            // It is a child of the root: it spans the whole side.
            let (node, _) = find_in(&l.dock[egui_dock::SurfaceIndex::main()], Pane::Timeline).unwrap();
            assert_eq!(node.parent(), Some(NodeIndex::root()), "{edge:?}");
            let first = matches!(edge, Edge::Left | Edge::Top);
            assert_eq!(node == NodeIndex::root().left(), first, "{edge:?}");
            // And the file still loads.
            assert!(from_json(&to_json(&l.dock, false).unwrap()).is_some());
        }
        // Onto an empty main area.
        let mut l = layout();
        for p in Pane::ALL {
            l.hide(p);
        }
        l.dock_to_edge(Pane::Viewer, Edge::Left);
        assert!(l.is_open(Pane::Viewer) && !l.is_floating(Pane::Viewer));
    }

    #[test]
    fn broken_layout_files_are_refused() {
        assert!(from_json("").is_none());
        assert!(from_json("{ nonsense").is_none());
        // The same pane twice.
        let mut dock = main_dock();
        dock.add_window(vec![Tab::new(Pane::Timeline)]);
        assert!(!is_valid(&dock));
        assert!(from_json(&to_json(&dock, false).unwrap()).is_none());
        // A file from before the lock: the layout alone.
        let old = serde_json::to_string(
            &serde_json::from_str::<serde_json::Value>(&to_json(&main_dock(), false).unwrap()).unwrap()["dock"],
        )
        .unwrap();
        let (dock, locked) = from_json(&old).unwrap();
        assert!(is_valid(&dock) && !locked);
    }

    /// Where each pane sits in a layout: tab order in its place, and the
    /// fractions of the splits above it, rounded.
    fn shape(dock: &DockState<Tab>) -> Vec<(Vec<Pane>, Vec<i32>)> {
        let tree = &dock[egui_dock::SurfaceIndex::main()];
        let mut out = vec![];
        for (i, node) in tree.iter().enumerate() {
            if let Some(tabs) = node.tabs() {
                let mut fr = vec![];
                let mut at = NodeIndex(i);
                while let Some(p) = at.parent() {
                    let f = match &tree[p] {
                        egui_dock::Node::Horizontal(split) | egui_dock::Node::Vertical(split) => Some(split.fraction),
                        _ => None,
                    };
                    if let Some(f) = f {
                        // The fraction is the share of the first (left or top) child.
                        let first = p.left() == at;
                        fr.push(((if first { f } else { 1.0 - f }) * 100.0).round() as i32);
                    }
                    at = p;
                }
                out.push((tabs.iter().map(|t| t.pane).collect(), fr));
            }
        }
        out.sort_by_key(|(t, _)| format!("{t:?}"));
        out
    }

    /// The default is the layout Simon arranged, saved from the program.
    #[test]
    fn the_default_layout_is_the_arranged_one() {
        let Ok(text) = std::fs::read_to_string(std::env::var("XMS_LAYOUT_FILE").unwrap_or_default()) else {
            return;
        };
        let (theirs, _) = from_json(&text).unwrap();
        assert_eq!(shape(&main_dock()), shape(&theirs));
    }

    /// Panes saved under the names they had: the Curve Editor as "GraphEditor".
    #[test]
    fn panes_load_under_their_earlier_names() {
        for (saved, pane) in [
            ("Viewport", Pane::Viewer),
            ("Properties", Pane::Parameters),
            ("GraphEditor", Pane::CurveEditor),
        ] {
            assert_eq!(serde_json::from_str::<Pane>(&format!("\"{saved}\"")).unwrap(), pane);
        }
        assert_eq!(serde_json::to_string(&Pane::CurveEditor).unwrap(), "\"CurveEditor\"");
    }

    /// A layout file written by the egui_dock of Imago before the egui 0.36
    /// upgrade (0.13), with the pane names of that time: it still loads, with
    /// its splits, tabs, floating window and lock.
    #[test]
    fn layouts_saved_before_the_upgrade_still_load() {
        let r = r#"{"min":{"x":0.0,"y":0.0},"max":{"x":0.0,"y":0.0}}"#;
        let leaf =
            |tabs: &str| format!(r#"{{"Leaf":{{"rect":{r},"viewport":{r},"tabs":[{tabs}],"active":0,"scroll":0.0}}}}"#);
        let main = format!(
            r#"{{"Main":{{"nodes":[{{"Horizontal":{{"rect":{r},"fraction":0.6}}}},{},{}],"focused_node":1}}}}"#,
            leaf(r#""Viewport","UvEditor""#),
            leaf(r#""NodeGraph","SceneExplorer","OperatorStack","PrimInspector","Timeline","History""#)
        );
        let window = format!(
            r#"{{"Window":[{{"nodes":[{}],"focused_node":null}},{{"screen_rect":null,"dragged":false,"next_position":null,"next_size":null,"new":false}}]}}"#,
            leaf(r#""Properties""#)
        );
        let json = format!(
            r#"{{"locked":true,"dock":{{"surfaces":[{main},{window}],"focused_surface":0,
            "translations":{{"tab_context_menu":{{"close_button":"Close","eject_button":"Eject"}},"window":{{"close_button_tooltip":"Close"}}}}}}}}"#
        );
        let (dock, locked) = from_json(&json).expect("a layout saved before the upgrade loads");
        assert!(locked);
        let l = Layout {
            dock,
            locked,
            saved: json,
            sized: vec![],
            tab_drag: None,
        };
        // Every pane of that time; the Curve Editor, the Image Viewer and the
        // Light Linker came later, and are ticked in the Window menu.
        assert_eq!(open(&l), ALL - 3);
        assert!(!l.is_open(Pane::CurveEditor) && !l.is_open(Pane::ImageViewer) && !l.is_open(Pane::LightLinker));
        // Its Operator Stack, a pane no longer, taken away; the others of
        // its place kept.
        assert!(!l.is_open(Pane::Retired) && l.is_open(Pane::History) && l.is_open(Pane::NodeGraph));
        assert!(l.is_floating(Pane::Parameters) && !l.is_floating(Pane::Viewer));
        let tree = &l.dock[egui_dock::SurfaceIndex::main()];
        match &tree[NodeIndex::root()] {
            egui_dock::Node::Horizontal(split) => assert!((split.fraction - 0.6).abs() < 1e-6),
            other => panic!("the root split was lost: {other:?}"),
        }
        let (node, _) = find_in(&l.dock[egui_dock::SurfaceIndex::main()], Pane::UvEditor).unwrap();
        assert_eq!(node, NodeIndex::root().left());
        // And it is written back in today's form, which loads again.
        assert!(from_json(&to_json(&l.dock, true).unwrap()).is_some());
    }

    /// The layouts Imago comes with read, each pane once at most; the main
    /// one has every pane.
    #[test]
    fn the_layouts_it_comes_with_read() {
        for (name, _) in PRESETS {
            let dock = preset(name).unwrap_or_else(|| panic!("{name} does not read"));
            assert!(is_valid(&dock), "{name}");
        }
        // Every pane but the Tools and the Selection bar, which the XSI
        // layout has.
        assert_eq!(main_dock().iter_all_tabs().count(), Pane::ALL.len() - 2);
        assert_eq!(preset("xms_2split_layout").unwrap().iter_all_tabs().count(), 7);
        // Every pane but the Parameters, which float over the Node Graph.
        let xsi = preset("xms_xsi_layout").unwrap();
        assert_eq!(xsi.iter_all_tabs().count(), Pane::ALL.len() - 1);
        assert!(xsi.find_tab_from(|t| t.pane == Pane::Parameters).is_none());
        // The Tools alone in their column, the whole height: the root's
        // first split is theirs.
        let tree = &xsi[egui_dock::SurfaceIndex::main()];
        let (leaf, _) = find_in(tree, Pane::Tools).unwrap();
        assert_eq!(find_in(tree, Pane::Viewer).map(|f| f.0 != leaf), Some(true));
        assert!(leaf.parent() == Some(NodeIndex::root()), "{leaf:?}");
        // The Selection bar down the right, beside the Timeline's column.
        let (bar, _) = find_in(tree, Pane::Selection).unwrap();
        assert_ne!(Some(bar), find_in(tree, Pane::Timeline).map(|f| f.0));
        assert_eq!(xsi.iter_all_tabs().count(), xsi_dock().iter_all_tabs().count());
        assert!(preset("nope").is_none());
        // Choosing one keeps the lock as it is.
        let mut l = layout();
        l.locked = true;
        l.use_preset("xms_2split_layout").unwrap();
        // Its Primitive Inspector a tab behind the Scene Explorer, bottom left.
        {
            let tree = &l.dock[egui_dock::SurfaceIndex::main()];
            let (scene, front) = find_in(tree, Pane::SceneExplorer).unwrap();
            assert_eq!(find_in(tree, Pane::PrimInspector).unwrap().0, scene);
            match &tree[scene] {
                egui_dock::Node::Leaf(leaf) => assert_eq!(leaf.active.0, front),
                _ => panic!("not a leaf"),
            }
        }
        assert!(l.locked && !l.is_open(Pane::History) && l.is_open(Pane::Viewer));
    }

    /// The tabs in front are the layout's own: Prim Inspector's leaf shows
    /// the Scene Explorer (the Light Linker a tab beside them, bottom left),
    /// History alone in its own, in the main one.
    #[test]
    fn the_main_layout_opens_with_its_tabs_in_front() {
        let dock = main_dock();
        let tree = &dock[egui_dock::SurfaceIndex::main()];
        let front = |p: Pane| {
            let (node, tab) = find_in(tree, p).unwrap();
            match &tree[node] {
                egui_dock::Node::Leaf(leaf) => leaf.active.0 == tab,
                _ => false,
            }
        };
        assert!(front(Pane::SceneExplorer) && !front(Pane::PrimInspector) && !front(Pane::LightLinker));
        let leaf_of = |p: Pane| find_in(tree, p).unwrap().0;
        assert_eq!(leaf_of(Pane::LightLinker), leaf_of(Pane::PrimInspector));
        assert_eq!(leaf_of(Pane::LightLinker), leaf_of(Pane::SceneExplorer));
        assert!(front(Pane::History) && front(Pane::Viewer) && front(Pane::NodeGraph));
    }

    /// The layout built in code when the file cannot be read: the Light
    /// Linker a tab behind the Primitive Inspector, bottom left.
    #[test]
    fn the_built_layout_has_the_light_linker_bottom_left() {
        let dock = built_dock();
        let tree = &dock[egui_dock::SurfaceIndex::main()];
        let leaf_of = |p: Pane| find_in(tree, p).unwrap().0;
        assert_eq!(leaf_of(Pane::LightLinker), leaf_of(Pane::PrimInspector));
        assert!(is_valid(&dock) && dock.iter_all_tabs().count() == Pane::ALL.len() - 2);
    }

    #[test]
    fn the_default_layout_has_every_pane_once() {
        let d = main_dock();
        assert!(is_valid(&d));
        // Every pane but the Tools and the Selection bar (`xms_xsi_layout`).
        assert_eq!(d.iter_all_tabs().count(), Pane::ALL.len() - 2);
    }

    /// Some panes open twice, each a tab of its own number; one of a kind
    /// never; a tab turns into another pane only where that keeps the rule.
    /// Saved and read again, the numbers stay.
    #[test]
    fn tabs_are_instances_some_of_them_twice() {
        let mut l = layout();
        let at = find(&l.dock, Pane::Parameters).map(|p| p.node_path());
        l.add_tab(Pane::Parameters, at);
        l.add_tab(Pane::Timeline, at);
        let tabs: Vec<Tab> = l.dock.iter_all_tabs().map(|(_, t)| *t).collect();
        assert_eq!(tabs.iter().filter(|t| t.pane == Pane::Parameters).count(), 2);
        assert_eq!(tabs.iter().filter(|t| t.pane == Pane::Timeline).count(), 1);
        assert!(is_valid(&l.dock));
        // Beside the first, in the same tab bar.
        let both: Vec<egui_dock::TabPath> = l
            .dock
            .iter_all_tabs()
            .filter(|(_, t)| t.pane == Pane::Parameters)
            .map(|(at, _)| at)
            .collect();
        assert_eq!((both[0].surface, both[0].node), (both[1].surface, both[1].node));
        let params = l.first(Pane::Parameters).unwrap();
        assert!(l.may_become(params, Pane::SceneExplorer) && !l.may_become(params, Pane::Viewer));
        let viewer = l.first(Pane::Viewer).unwrap();
        assert!(l.may_become(viewer, Pane::Viewer));
        // Through the file: the same numbers.
        let (back, _) = from_json(&to_json(&l.dock, false).unwrap()).unwrap();
        let ids = |d: &DockState<Tab>| {
            let mut v: Vec<u32> = d.iter_all_tabs().map(|(_, t)| t.id).collect();
            v.sort_unstable();
            v
        };
        assert_eq!(ids(&back), ids(&l.dock));
        // What a tab follows, kept through the file; a tab the same by its
        // number whatever it follows.
        let pin = Follows::Pinned {
            node: crate::types::NodeId(7),
            level: level_of(&["Rig".to_string()]),
        };
        for (_, t) in l.dock.iter_all_tabs_mut() {
            if t.id == params.id {
                t.follows = Some(pin);
            }
        }
        assert!(l.dock.find_tab(&params).is_some());
        let (back, _) = from_json(&to_json(&l.dock, false).unwrap()).unwrap();
        let kept = back.iter_all_tabs().find(|(_, t)| t.id == params.id).unwrap().1;
        assert_eq!(kept.follows(), Some(pin));
        assert_eq!(Tab::new(Pane::Parameters).follows(), Some(Follows::Selection));
        assert_eq!(Tab::new(Pane::Timeline).follows(), None);
        assert_ne!(level_of(&[]), level_of(&["Rig".to_string()]));
        // Closing the pane closes both.
        l.hide(Pane::Parameters);
        assert!(!l.is_open(Pane::Parameters));
    }

    /// A layout naming panes alone (as saved before tabs had numbers), and
    /// two tabs of one number: each tab is given one of its own, new tabs
    /// numbered above them; two Viewers are refused.
    #[test]
    fn tabs_read_without_numbers_are_numbered() {
        let dock = main_dock();
        let mut ids: Vec<u32> = dock.iter_all_tabs().map(|(_, t)| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert!(ids.len() == Pane::ALL.len() - 2 && ids[0] > 0);
        assert!(Tab::new(Pane::Viewer).id > *ids.last().unwrap());
        let mut twice = main_dock();
        let first = twice.iter_all_tabs().map(|(_, t)| *t).next().unwrap();
        twice.add_window(vec![Tab {
            id: first.id,
            pane: Pane::Parameters,
            follows: None,
        }]);
        let (back, _) = from_json(&to_json(&twice, false).unwrap()).expect("renumbered");
        assert!(is_valid(&back));
        let mut viewers = main_dock();
        viewers.add_window(vec![Tab::new(Pane::Viewer)]);
        assert!(from_json(&to_json(&viewers, false).unwrap()).is_none());
    }

    /// Which panes open twice: not the Viewer, Timeline, History, Operator
    /// Stack or Image Viewer. Which follow: Parameters, Curve and UV Editors.
    #[test]
    fn most_panes_open_twice() {
        let twice: Vec<Pane> = Pane::ALL.into_iter().filter(|p| p.multiple()).collect();
        assert_eq!(
            twice,
            [
                Pane::NodeGraph,
                Pane::SceneExplorer,
                Pane::Parameters,
                Pane::PrimInspector,
                Pane::UvEditor,
                Pane::CurveEditor,
                Pane::LightLinker
            ]
        );
        let follow: Vec<Pane> = Pane::ALL.into_iter().filter(|p| p.follows().is_some()).collect();
        assert_eq!(
            follow,
            [
                Pane::SceneExplorer,
                Pane::Parameters,
                Pane::PrimInspector,
                Pane::UvEditor,
                Pane::CurveEditor
            ]
        );
        assert_eq!(Tab::new(Pane::SceneExplorer).follows(), Some(Follows::Display));
    }

    /// What tabs keep is forgotten as they close; the Node Graph tab last
    /// used closed, the next one open takes its place, its view put into the
    /// graph.
    #[test]
    fn tab_states_follow_their_tabs() {
        let (a, b) = (Tab::new(Pane::NodeGraph), Tab::new(Pane::NodeGraph));
        let curve = Tab::new(Pane::CurveEditor);
        let mut graph = crate::node_graph::NodeGraphState::default();
        let mut nav = crate::ice::GraphNavigation::default();
        let mut states = TabStates::default();
        states.active_graph = Some(a.id);
        let mut b_view = graph.view();
        b_view.zoom = 2.5;
        states.graphs.insert(b.id, (b_view, Default::default()));
        states.curves.insert(curve.id, Default::default());
        states.tidy(&[a, b, curve], &mut graph, &mut nav);
        assert_eq!((states.graphs.len(), states.curves.len()), (1, 1));
        // a closed: b's view is the graph's.
        states.tidy(&[b], &mut graph, &mut nav);
        assert_eq!(states.active_graph, Some(b.id));
        assert_eq!(graph.zoom, 2.5);
        assert!(states.graphs.is_empty() && states.curves.is_empty());
    }
}
