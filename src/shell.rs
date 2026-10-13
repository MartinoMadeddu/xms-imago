//! What ties the program to its current shell, Bevy: the components the
//! Viewer's entities carry, and the program's state (`app::AppState`)
//! registered as a Bevy resource.
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

// The program's state, held by Bevy as one resource for now.
impl Resource for crate::app::AppState {}
impl Resource for crate::viewport::nav::NavSettings {}
