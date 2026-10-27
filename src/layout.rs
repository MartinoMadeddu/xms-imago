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

use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Pane {
    /// Was "Viewport": layouts saved with that name still load.
    #[serde(alias = "Viewport")]
    Viewer,
    NodeGraph,
    SceneExplorer,
    OperatorStack,
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
}

impl Pane {
    pub const ALL: [Pane; 11] = [
        Pane::Viewer,
        Pane::NodeGraph,
        Pane::SceneExplorer,
        Pane::OperatorStack,
        Pane::Parameters,
        Pane::PrimInspector,
        Pane::Timeline,
        Pane::UvEditor,
        Pane::History,
        Pane::CurveEditor,
        Pane::ImageViewer,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Pane::Viewer => "Viewer",
            Pane::NodeGraph => "Node Graph",
            Pane::SceneExplorer => "Scene Explorer",
            Pane::OperatorStack => "Operator Stack",
            Pane::Parameters => "Parameters",
            Pane::PrimInspector => "Primitive Inspector",
            Pane::Timeline => "Timeline",
            Pane::UvEditor => "UV Editor",
            Pane::History => "History",
            Pane::CurveEditor => "Curve Editor",
            Pane::ImageViewer => "Image Viewer",
        }
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
    pub dock: DockState<Pane>,
    /// Locked: tabs cannot be moved, floated or closed. Dividers still resize.
    pub locked: bool,
    /// The layout as last written to disk.
    saved: String,
    /// Floating windows that have been given their starting size.
    sized: Vec<usize>,
    /// Pane whose tab the mouse went down on, and whether it has moved since.
    pub tab_drag: Option<(Pane, bool)>,
}

/// The layouts Imago comes with, in Window > Layout: name (its file's),
/// the file. The first is the default until another is saved as default.
pub const PRESETS: [(&str, &str); 3] = [
    (
        "xms_main_layout",
        include_str!("../assets/layouts/xms_main_layout.json"),
    ),
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
pub fn preset(name: &str) -> Option<DockState<Pane>> {
    PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, text)| from_json(text))
        .map(|(dock, _)| dock)
}

/// `xms_main_layout`: the default when none is saved.
pub fn main_dock() -> DockState<Pane> {
    preset(PRESETS[0].0).unwrap_or_else(built_dock)
}

/// The file the layout saved as default is kept in.
fn default_file() -> Option<std::path::PathBuf> {
    crate::file_browser::config_path("default_layout.json")
}

/// The layout saved as default, if one is and it reads.
fn saved_default() -> Option<DockState<Pane>> {
    let text = std::fs::read_to_string(default_file()?).ok()?;
    from_json(&text).map(|(dock, _)| dock)
}

/// Whether a layout has been saved as default.
pub fn has_saved_default() -> bool {
    saved_default().is_some()
}

/// The layout Imago opens with, and File > New and Reset go back to: the
/// one saved as default, else `xms_main_layout`.
pub fn default_dock() -> DockState<Pane> {
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
/// inspector under both. Then the node graph, then the properties over the
/// operator stack. Timeline along the bottom of those, History down the whole
/// right side. The Curve Editor is a tab behind the node graph. Built in
/// code: what is used if `xms_main_layout` could not be read.
pub fn built_dock() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Viewer, Pane::UvEditor, Pane::ImageViewer]);
    let s = dock.main_surface_mut();
    let [work, _history] = s.split_right(NodeIndex::root(), 0.928, vec![Pane::History]);
    let [top, _timeline] = s.split_below(work, 0.862, vec![Pane::Timeline]);
    let [left, right] = s.split_right(top, 0.476, vec![Pane::NodeGraph, Pane::CurveEditor]);
    let [view, _prim] = s.split_below(left, 0.72, vec![Pane::PrimInspector]);
    s.split_left(view, 0.163, vec![Pane::SceneExplorer]);
    let [_graph, side] = s.split_right(right, 0.648, vec![Pane::Parameters]);
    s.split_below(side, 0.718, vec![Pane::OperatorStack]);
    dock
}

/// A layout is usable when no pane is in it twice. Panes that are missing are
/// closed, and can be shown again from the Panes menu.
pub fn is_valid(dock: &DockState<Pane>) -> bool {
    let tabs: Vec<Pane> = dock.iter_all_tabs().map(|(_, t)| *t).collect();
    Pane::ALL.iter().all(|p| tabs.iter().filter(|t| *t == p).count() <= 1)
}

/// The layout as text. Rectangles of panes that have not been laid out yet are
/// infinite, which JSON cannot hold: they are written as zero. They are worked
/// out again on the first frame anyway.
pub fn to_json(dock: &DockState<Pane>, locked: bool) -> Option<String> {
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
pub fn from_json(json: &str) -> Option<(DockState<Pane>, bool)> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let (dock, locked) = match value.get("dock") {
        Some(d) => (
            d.clone(),
            value.get("locked").and_then(|l| l.as_bool()).unwrap_or(false),
        ),
        None => (value, false),
    };
    let dock: DockState<Pane> = serde_json::from_value(dock).ok()?;
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
    fn set_dock(&mut self, dock: DockState<Pane>) {
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

    pub fn is_open(&self, pane: Pane) -> bool {
        self.dock.find_tab(&pane).is_some()
    }

    /// True when the pane sits in a floating window.
    pub fn is_floating(&self, pane: Pane) -> bool {
        self.dock
            .find_tab(&pane)
            .map(|at| !at.surface.is_main())
            .unwrap_or(false)
    }

    pub fn any_floating(&self) -> bool {
        Pane::ALL.iter().any(|p| self.is_floating(*p))
    }

    /// Close a pane. It can be shown again with `show`.
    pub fn hide(&mut self, pane: Pane) {
        if let Some(at) = self.dock.find_tab(&pane) {
            self.dock.remove_tab(at);
        }
    }

    /// Show a closed pane, as a tab of the main area.
    pub fn show(&mut self, pane: Pane) {
        if self.is_open(pane) {
            return;
        }
        self.dock.main_surface_mut().push_to_focused_leaf(pane);
    }

    /// Bring a floating pane back into the main area.
    pub fn dock_pane(&mut self, pane: Pane) {
        if !self.is_floating(pane) {
            return;
        }
        self.hide(pane);
        self.show(pane);
    }

    /// Pull a docked pane out as a floating window.
    pub fn float_pane(&mut self, pane: Pane) {
        if !self.is_open(pane) || self.is_floating(pane) {
            return;
        }
        self.hide(pane);
        self.dock.add_window(vec![pane]);
    }

    pub fn toggle_float(&mut self, pane: Pane) {
        if self.is_floating(pane) {
            self.dock_pane(pane);
        } else {
            self.float_pane(pane);
        }
    }

    /// Dock a pane along a whole side of the window, next to everything else:
    /// the place the timeline has in the default layout.
    pub fn dock_to_edge(&mut self, pane: Pane, edge: Edge) {
        use egui_dock::{Node, Split};
        self.hide(pane);
        let tree = self.dock.main_surface_mut();
        if tree.root_node().map(|n| n.is_empty()).unwrap_or(true) {
            tree.push_to_first_leaf(pane);
            return;
        }
        // The fraction is the share of the left or upper part.
        let (split, fraction) = match edge {
            Edge::Left => (Split::Left, edge.share()),
            Edge::Top => (Split::Above, edge.share()),
            Edge::Right => (Split::Right, 1.0 - edge.share()),
            Edge::Bottom => (Split::Below, 1.0 - edge.share()),
        };
        tree.split(NodeIndex::root(), split, fraction, Node::leaf(pane));
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
    const ALL: usize = Pane::ALL.len();
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
        let at = l.dock.find_tab(&Pane::Parameters).unwrap();
        let tab = l.dock.remove_tab(at).unwrap();
        let at = l.dock.find_tab(&Pane::NodeGraph).unwrap();
        l.dock.leaf_mut(at.node_path()).unwrap().append_tab(tab);
        l.hide(Pane::Timeline);
        l.hide(Pane::PrimInspector);
        l.dock.add_window(vec![Pane::PrimInspector]);
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
        let a = back.dock.find_tab(&Pane::Parameters).unwrap();
        let b = back.dock.find_tab(&Pane::NodeGraph).unwrap();
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
        l.dock.add_window(vec![Pane::Parameters]);
        l.hide(Pane::NodeGraph);
        l.dock.add_window(vec![Pane::NodeGraph]);
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
            let (node, _) = l.dock.find_main_surface_tab(&Pane::Timeline).unwrap();
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
        dock.add_window(vec![Pane::Timeline]);
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
    fn shape(dock: &DockState<Pane>) -> Vec<(Vec<Pane>, Vec<i32>)> {
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
                out.push((tabs.to_vec(), fr));
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
        // Every pane of that time; the Curve Editor and the Image Viewer
        // came later, and are ticked in the Window menu.
        assert_eq!(open(&l), ALL - 2);
        assert!(!l.is_open(Pane::CurveEditor) && !l.is_open(Pane::ImageViewer));
        assert!(l.is_floating(Pane::Parameters) && !l.is_floating(Pane::Viewer));
        let tree = &l.dock[egui_dock::SurfaceIndex::main()];
        match &tree[NodeIndex::root()] {
            egui_dock::Node::Horizontal(split) => assert!((split.fraction - 0.6).abs() < 1e-6),
            other => panic!("the root split was lost: {other:?}"),
        }
        let (node, _) = l.dock.find_main_surface_tab(&Pane::UvEditor).unwrap();
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
        assert_eq!(main_dock().iter_all_tabs().count(), Pane::ALL.len());
        assert_eq!(preset("xms_2split_layout").unwrap().iter_all_tabs().count(), 7);
        assert!(preset("nope").is_none());
        // Choosing one keeps the lock as it is.
        let mut l = layout();
        l.locked = true;
        l.use_preset("xms_2split_layout").unwrap();
        assert!(l.locked && !l.is_open(Pane::History) && l.is_open(Pane::Viewer));
    }

    /// The tabs in front are the layout's own: Prim Inspector's leaf shows
    /// the Scene Explorer, the Operator Stack's the History, in the main one.
    #[test]
    fn the_main_layout_opens_with_its_tabs_in_front() {
        let dock = main_dock();
        let tree = &dock[egui_dock::SurfaceIndex::main()];
        let front = |p: Pane| {
            let (node, tab) = tree.find_tab(&p).unwrap();
            match &tree[node] {
                egui_dock::Node::Leaf(leaf) => leaf.active.0 == tab.0,
                _ => false,
            }
        };
        assert!(front(Pane::SceneExplorer) && !front(Pane::PrimInspector));
        assert!(front(Pane::History) && front(Pane::Viewer) && front(Pane::NodeGraph));
    }

    #[test]
    fn the_default_layout_has_every_pane_once() {
        let d = main_dock();
        assert!(is_valid(&d));
        assert_eq!(d.iter_all_tabs().count(), Pane::ALL.len());
    }
}
