//! The program's state: everything the interface, the graph and the Viewer
//! work on, in one place, owned by whatever shell runs the program.
//!
//! The eframe app owns it (`eframe_app.rs`). Nothing in it depends on the
//! shell.

use crate::batch::BatchState;
use crate::downloads::Downloads;
use crate::file_browser::FileBrowser;
use crate::history::History;
use crate::ice::{GraphNavigation, IceGraphStore};
use crate::layout::Layout;
use crate::node_graph::{GraphRevision, NodeGraphState};
use crate::recent::Recent;
use crate::scene_explorer::OperatorStack;
use crate::timeline::Playback;
use crate::types::{PrimInspectorState, SceneHierarchy};
use crate::uv_editor::UvEditorState;
use crate::{Confirm, GraphFile, SaveFirst};

/// The screen rectangle of the Viewer pane, when it is showing.
#[derive(Default)]
pub struct ViewportRect(pub Option<egui::Rect>);

#[derive(Default)]
pub struct AppState {
    /// The root scene graph, and what changed in it.
    pub graph:      NodeGraphState,
    pub revision:   GraphRevision,
    /// ICE graphs, and which one is open.
    pub subnets:    IceGraphStore,
    pub nav:        GraphNavigation,
    /// What the panes show.
    pub stack:      OperatorStack,
    pub hierarchy:  SceneHierarchy,
    pub prim_state: PrimInspectorState,
    pub uv_state:   UvEditorState,
    pub playback:   Playback,
    pub vp_rect:    ViewportRect,
    pub layout:     Layout,
    /// Files: the graph's own, the browser, recent files, questions before
    /// losing changes or writing over a file.
    pub graph_file: GraphFile,
    pub browser:    FileBrowser,
    pub recent:     Recent,
    pub save_first: SaveFirst,
    pub confirm:    Confirm,
    /// Undo and redo.
    pub history:    History,
    /// Write FBX running over a folder, in the background.
    pub batch:      BatchState,
    /// Example files a graph names that can be fetched.
    pub downloads:  Downloads,
    /// The Graph Editor's view and selection.
    pub graph_editor: crate::graph_editor::GraphEditor,
}
