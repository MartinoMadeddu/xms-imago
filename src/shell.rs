//! What ties the program to its current shell, Bevy: the components the
//! Viewer's entities carry, the Viewer's screen rectangle, and the program's
//! state registered as Bevy resources.
//!
//! Everything outside `main.rs`, `modelling.rs`, `viewport/` and this file is
//! free of Bevy. The eframe rework replaces this file with plain fields on
//! the app (see the "eframe" stages in ARCHITECTURE.md).

use bevy::prelude::*;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct GeneratedMesh;

/// A generated mesh that follows the playhead: rebuilt when time moves,
/// while the others stay as they are.
#[derive(Component)]
pub struct PosedMesh;

#[derive(Component)]
pub struct GroundGrid;

/// Tracks the screen rect available for the 3D viewport (excluding egui panels).
#[derive(Resource, Default)]
pub struct ViewportRect(pub Option<egui::Rect>);

// The program's state, held by Bevy as resources for now.
impl Resource for crate::node_graph::NodeGraphState {}
impl Resource for crate::node_graph::GraphRevision {}
impl Resource for crate::scene_explorer::OperatorStack {}
impl Resource for crate::types::SceneHierarchy {}
impl Resource for crate::types::PrimInspectorState {}
impl Resource for crate::ice::SubnetStore {}
impl Resource for crate::ice::GraphNavigation {}
impl Resource for crate::timeline::Playback {}
impl Resource for crate::layout::Layout {}
impl Resource for crate::recent::Recent {}
impl Resource for crate::file_browser::FileBrowser {}
impl Resource for crate::batch::BatchState {}
impl Resource for crate::history::History {}
impl Resource for crate::uv_editor::UvEditorState {}
