mod app;
mod batch;
mod core;
mod curve_editor;
mod downloads;
mod edits;
mod eframe_app;
mod examples;
mod examples_stage;
mod fbx_loader;
mod fbx_writer;
mod file_browser;
mod graph_io;
mod graph_ports;
mod history;
mod ice;
/// The Image Viewer pane: renders and images.
mod image_viewer;
mod keys_ui;
mod layout;
mod materialx;
mod menus;
mod modelling;
mod node_graph;
mod prim_inspector;
mod properties;
mod ragdoll;
mod recent;
#[cfg(test)]
mod regress;
mod scene_explorer;
mod strands;
mod templates;
mod theme;
mod timeline;
mod timings;
mod type_colours;
mod types;
mod usd_loader;
mod usd_scene;
mod usd_stage;
mod usd_write;
mod usda_text;
mod uv_canvas;
mod uv_editor;
mod variables;
mod viewer;
mod viewport;

use app::{AppState, ViewportRect};
use batch::BatchState;
use file_browser::{BrowseMode, BrowseTarget, FileBrowser};
use ice::{
    ui::{draw_breadcrumb, draw_subnet_graph},
    GraphNavigation, IceGraphStore,
};
use layout::{Layout, Pane};
use node_graph::ui::draw_node_graph;
use node_graph::NodeGraphState;
use prim_inspector::ui::draw_prim_inspector;
use properties::ui::draw_properties_panel;
use properties::ui::{AnimContext, PanelAction, PanelIo};
use scene_explorer::ui::{draw_operator_stack, draw_scene_explorer};
use scene_explorer::OperatorStack;
use timeline::{resolve_source, ui::draw_timeline, Playback, TimelineState};
use types::NodeType;
use types::{IceGraphId, MeshData, PrimInspectorState, SceneHierarchy};

fn main() {
    if let Err(e) = eframe_app::run() {
        eprintln!("{e}");
    }
}

// Icon

pub(crate) fn decode_png(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .ok()?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

// UI

/// The whole interface for one frame: menus, the dock and every pane, the file
/// browser and the Viewer's navigation menu. The same in any shell. `root`: the
/// window's whole Ui, which the top bar and the dock divide. `viewer`: the
/// Viewer's 3D view (`None` leaves the pane empty, as in tests).
pub(crate) fn draw_ui(
    root: &mut egui::Ui,
    app: &mut AppState,
    poly_tool: &mut modelling::PolyTool,
    nav_settings: &mut viewport::nav::NavSettings,
    dt: f64,
    viewer: Option<&mut viewer::Viewer>,
) {
    let ctx = &root.ctx().clone();
    let AppState {
        graph,
        stack,
        hierarchy,
        subnets,
        nav,
        vp_rect,
        prim_state,
        playback,
        browser,
        graph_file,
        layout,
        revision,
        uv_state,
        history,
        recent,
        save_first,
        confirm,
        batch,
        curve_editor,
        image_viewer,
        ..
    } = app;
    // Keyed parameters cook at the playhead.
    graph.time = playback.time;
    // Local Animations show the parameters of the operator before them.
    graph.sync_inherited();
    // S, tapped: key the selected operator at the playhead.
    if !browser.is_open() {
        if let Some(m) = keys_ui::s_key(ctx, graph) {
            graph_file.message = m;
        }
    }

    // Path chosen in the file browser
    if let Some((target, path)) = browser.take_result() {
        let text = path.to_string_lossy().to_string();
        match target {
            BrowseTarget::Node(id) => {
                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id) {
                    match &mut node.node_type {
                        NodeType::LoadUsd { path, .. }
                        | NodeType::WriteUsd { path }
                        | NodeType::LoadFbx { path, .. }
                        | NodeType::LoadFbxMesh { path } => *path = text,
                        NodeType::LoadFbxDir { dir, index, .. } => {
                            *dir = text;
                            *index = 0;
                        }
                        // A folder was picked: keep the file name pattern.
                        NodeType::WriteFbx { path, .. } => {
                            let name = path
                                .rsplit(['/', '\\'])
                                .next()
                                .filter(|n| !n.is_empty())
                                .unwrap_or("{file}_{char}.fbx")
                                .to_string();
                            *path = std::path::Path::new(&text).join(name).to_string_lossy().to_string();
                        }
                        _ => {}
                    }
                }
            }
            // A texture file for one input of a Material node.
            BrowseTarget::Texture(id, input) => {
                if let Some(NodeType::Material { surface }) =
                    graph.nodes.iter_mut().find(|n| n.id == id).map(|n| &mut n.node_type)
                {
                    surface.textures.insert(input.to_string(), text);
                }
            }
            // A file for one of a Light operator's values.
            BrowseTarget::LightFile(id, name) => {
                if let Some(NodeType::Light { light, .. }) =
                    graph.nodes.iter_mut().find(|n| n.id == id).map(|n| &mut n.node_type)
                {
                    light.set(name, crate::core::properties::Value::Asset(text));
                }
            }
            // A file name input of a node in a MaterialX graph.
            BrowseTarget::MatxFile(op, node, input) => {
                if let Some(NodeType::MaterialX { graph: g }) =
                    graph.nodes.iter_mut().find(|n| n.id == op).map(|n| &mut n.node_type)
                {
                    let name = g
                        .def(materialx::lib(), node)
                        .and_then(|d| d.inputs.get(input as usize))
                        .map(|i| i.name.clone());
                    if let Some(name) = name {
                        g.set_value(node, &name, xms_matx::Value::String(text));
                    }
                }
            }
            // A MaterialX operator's graph from a .mtlx file: the material of
            // the operator's name, else the first.
            BrowseTarget::MatxLoad(op) => {
                graph_file.message = match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| xms_matx::import::import(&t, materialx::lib(), path.parent()))
                {
                    Err(e) => format!("Could not read {}: {e}", path.display()),
                    Ok(mut got) => match graph.nodes.iter_mut().find(|n| n.id == op) {
                        Some(node) if !got.materials.is_empty() => {
                            let own = crate::core::material::prim_name(&node.name);
                            let k = got
                                .materials
                                .iter()
                                .position(|(n, _)| crate::core::material::prim_name(n) == own)
                                .unwrap_or(0);
                            let (name, g) = got.materials.swap_remove(k);
                            if let NodeType::MaterialX { graph } = &mut node.node_type {
                                *graph = g;
                            }
                            materialx::set_view(ctx, op, materialx::View::default());
                            let mut m = format!("{name} read from {}", path.display());
                            if !got.materials.is_empty() {
                                m.push_str(&format!(
                                    " ({} other materials in the file: File > Import > MaterialX adds them all)",
                                    got.materials.len()
                                ));
                            }
                            if !got.notes.is_empty() {
                                m.push_str(&format!(". {}", got.notes.join(". ")));
                            }
                            m
                        }
                        _ => format!("No material read from {}. {}", path.display(), got.notes.join(". ")),
                    },
                };
            }
            // A MaterialX operator's graph written as a .mtlx file.
            BrowseTarget::MatxSave(op) => {
                let path = if path.extension().is_some_and(|e| e == "mtlx") {
                    path.clone()
                } else {
                    path.with_extension("mtlx")
                };
                if let Some(NodeType::MaterialX { graph: g }) =
                    graph.nodes.iter().find(|n| n.id == op).map(|n| &n.node_type)
                {
                    graph_file.message = match std::fs::write(&path, xms_matx::document::write(g, materialx::lib())) {
                        Ok(()) => format!("Wrote {}", path.display()),
                        Err(e) => format!("Could not write {}: {e}", path.display()),
                    };
                }
            }
            // A MaterialX operator for each material of a .mtlx file.
            BrowseTarget::ImportMtlx => {
                graph_file.message = match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| xms_matx::import::import(&t, materialx::lib(), path.parent()))
                {
                    Err(e) => format!("Could not read {}: {e}", path.display()),
                    Ok(got) => {
                        let n = got.materials.len();
                        for (name, g) in got.materials {
                            import_node(&mut *graph, name, NodeType::MaterialX { graph: g });
                        }
                        let mut m = format!(
                            "Imported {n} material{} from {}",
                            if n == 1 { "" } else { "s" },
                            path.display()
                        );
                        if !got.notes.is_empty() {
                            m.push_str(&format!(". {}", got.notes.join(". ")));
                        }
                        m
                    }
                };
            }
            BrowseTarget::OpenGraph => save_first.load(Source::File(path.clone())),
            BrowseTarget::OpenLayout => {
                graph_file.message = match layout.load_from(&path) {
                    Ok(()) => format!("Layout loaded from {}", path.display()),
                    Err(e) => format!("Could not load layout {}: {e}", path.display()),
                };
            }
            BrowseTarget::SaveLayout => {
                graph_file.message = match layout.save_as(&path) {
                    Ok(()) => format!("Layout saved to {}", path.display()),
                    Err(e) => format!("Could not save layout {}: {e}", path.display()),
                };
            }
            BrowseTarget::SaveGraph => {
                save_graph(&graph, &subnets, &path, &mut *graph_file, &mut *history, &mut *recent);
            }
            BrowseTarget::ImportGraph => {
                graph_file.message = add_graph_file(&mut *graph, &mut *subnets, &path, &mut *recent);
            }
            BrowseTarget::ImportFbx => {
                let name = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Load FBX".into());
                import_node(&mut *graph, name, NodeType::LoadFbx { path: text, take: 0 });
                graph_file.message = format!("Imported {}", path.display());
            }
            BrowseTarget::ImportUsd => {
                let name = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Load USD".into());
                import_node(&mut *graph, name, NodeType::load_usd(text));
                graph_file.message = format!("Imported {}", path.display());
            }
        }
    }

    // File shortcuts (Cmd on macOS). Ctrl+Shift+S is checked before Ctrl+S,
    // which would match it too.
    let mut command = None;
    if !ctx.egui_wants_keyboard_input() && !browser.is_open() && !save_first.busy() && confirm.pending.is_none() {
        use egui::{Key, Modifiers};
        command = ctx.input_mut(|i| {
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
                Some(FileCommand::SaveAs)
            } else if i.consume_key(Modifiers::COMMAND, Key::S) {
                Some(FileCommand::Save)
            } else if i.consume_key(Modifiers::COMMAND, Key::N) {
                Some(FileCommand::New)
            } else if i.consume_key(Modifiers::COMMAND, Key::O) {
                Some(FileCommand::Open)
            } else if i.consume_key(Modifiers::COMMAND, Key::Q) {
                Some(FileCommand::Quit)
            } else {
                None
            }
        });
    }

    // Range, rate and timecode come from the clip of the selected node.
    let timeline_state = resolve_source(&graph);
    let keys_free = !ctx.egui_wants_keyboard_input() && !browser.is_open();

    // Clips around the selected node, for the properties panel.
    let selected_input = graph
        .selected_node
        .and_then(|id| graph.nodes.iter().find(|n| n.id == id))
        .and_then(|n| n.inputs.first())
        .and_then(|i| i.connected_output)
        .and_then(|(src, out)| graph.eval_anim_out(src, out));
    let anim_ctx = AnimContext {
        time: playback.time,
        output: match &timeline_state {
            TimelineState::Source(s) if s.from_selection => Some(s.clip.clone()),
            _ => None,
        },
        input: selected_input,
    };

    // Panes
    // Every pane is a tab of one dock area: drag a tab to move it, stack it
    // with another, or pull it out as a window.
    vp_rect.0 = None; // set again by the viewport pane if it is showing
    let locked = layout.locked;
    // Top bar
    // Fixed line above the panes, for things that belong to the whole
    // program. It is not a pane: it cannot be moved, floated or closed.
    egui::Panel::top("top_bar")
        .exact_size(28.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(theme::c(58, 58, 58)).inner_margin(egui::Margin::symmetric(8, 0)))
        .show(root, |ui| {
            let bar = ui.max_rect();
            ui.horizontal_centered(|ui| {
                // The wordmark: the logo and the name, dimmed, nothing to
                // click. A thin divider sets it apart from the menus.
                draw_logo(ui);
                ui.add_space(2.0);
                ui.add(egui::Label::new(egui::RichText::new("XMS | Imago").size(12.0).color(theme::c(165, 165, 165))).selectable(false));
                ui.add_space(10.0);
                top_bar_divider(ui);
                ui.add_space(4.0);
                // The menus: flat titles that light up under the mouse and
                // stay lit while open, as a menu bar does.
                ui.scope(|ui| {
                    menu_title_style(ui);
                    if let Some(c) = file_menu(ui, &recent, &graph_file) { command = Some(c); }
                    edit_menu(ui, &mut *graph, &mut *subnets, &mut *history, &mut *graph_file);
                    crate::menus::menu(ui, "Window", |ui| window_menu(ui, &mut *layout, &mut *browser, locked));
                });
                let menus_end = ui.cursor().min.x;
                let mut lock_start = bar.right();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The lock sits at the far right, a button like any other.
                    let tip = if locked {
                        "Layout locked: panes cannot be moved, floated or closed. Dividers still resize. Click to unlock"
                    } else {
                        "Lock the layout"
                    };
                    let r = lock_button(ui, locked).on_hover_text(tip);
                    if r.clicked() { layout.locked = !locked; }
                    lock_start = r.rect.left();
                });
                // The graph's file, centred in the bar as a window title is,
                // with a dot before it while it has unsaved changes. Plain
                // text, not a button: nothing happens on a click.
                let doc = graph_file.path.as_deref().map(file_name).unwrap_or_else(|| "Untitled".into());
                let unsaved = history.is_dirty();
                let title = egui::RichText::new(if unsaved { format!("• {doc}") } else { doc.clone() })
                    .size(13.0).color(theme::c(225, 225, 225));
                let galley = ui.painter().layout_no_wrap(title.text().to_string(), egui::FontId::proportional(13.0), egui::Color32::WHITE);
                let w = galley.size().x;
                let free = egui::Rect::from_min_max(egui::pos2(menus_end + 16.0, bar.top()), egui::pos2(lock_start - 16.0, bar.bottom()));
                let x = (bar.center().x - w / 2.0).max(free.left()).min((free.right() - w).max(free.left()));
                let rect = egui::Rect::from_min_max(egui::pos2(x, bar.top()), egui::pos2(x + w.min(free.width().max(0.0)), bar.bottom()));
                ui.put(rect, egui::Label::new(title).selectable(false).truncate())
                    .on_hover_text(match (&graph_file.path, unsaved) {
                        (Some(p), true)  => format!("{}\nChanged since it was saved. Ctrl+S saves", p.display()),
                        (Some(p), false) => format!("{}\nSaved", p.display()),
                        (None, _)        => "Not saved to a file yet. Ctrl+S saves".into(),
                    });
            });
        });
    theme::editor(ctx);
    variables::window(ctx, &mut graph.variables);
    if let Some(c) = command {
        match c {
            FileCommand::New => save_first.load_new(),
            FileCommand::Open => {
                browser.open(BrowseTarget::OpenGraph, BrowseMode::File, "Open graph", GRAPH_FILES, "");
            }
            FileCommand::OpenRecent(p) => save_first.load(Source::File(p)),
            FileCommand::Template(i) => save_first.load(Source::Template(i)),
            FileCommand::ClearRecent => recent.clear(),
            FileCommand::Save => {
                save_or_ask(
                    &graph,
                    &subnets,
                    &mut *graph_file,
                    &mut *history,
                    &mut *recent,
                    &mut *browser,
                );
            }
            FileCommand::SaveAs => ask_where_to_save(&graph_file, &mut *browser),
            FileCommand::ImportGraph => browser.open(
                BrowseTarget::ImportGraph,
                BrowseMode::File,
                "Import graph",
                GRAPH_FILES,
                "",
            ),
            FileCommand::ImportFbx => {
                browser.open(BrowseTarget::ImportFbx, BrowseMode::File, "Import FBX", &["fbx"], "")
            }
            FileCommand::ImportUsd => browser.open(
                BrowseTarget::ImportUsd,
                BrowseMode::File,
                "Import USD",
                &["usd", "usda", "usdc", "usdz"],
                "",
            ),
            FileCommand::ImportMtlx => browser.open(
                BrowseTarget::ImportMtlx,
                BrowseMode::File,
                "Import MaterialX",
                &["mtlx"],
                "",
            ),
            FileCommand::Variables => variables::open(ctx),
            FileCommand::Quit => save_first.quit = true,
        }
    }
    // Writing over files that exist: Replace or Cancel.
    if let Some((existing, _)) = &confirm.pending {
        let existing = existing.clone();
        dim_behind(ctx);
        let mut choice = None;
        egui::Window::new("Replace files?")
            .order(egui::Order::Tooltip)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(if existing.len() == 1 {
                    "This already exists:"
                } else {
                    "These already exist:"
                });
                for e in existing.iter().take(8) {
                    ui.label(egui::RichText::new(e).monospace().small());
                }
                if existing.len() > 8 {
                    ui.label(format!("and {} more", existing.len() - 8));
                }
                ui.label(egui::RichText::new("Writing replaces what is there.").small().weak());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Replace").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(false);
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(false);
        }
        if let Some(go) = choice {
            if let Some((_, action)) = confirm.pending.take() {
                if go {
                    run_action(&graph, &subnets, &batch, action);
                }
            }
        }
    }

    let mut toggle_float = None;
    let mut dock_rect = ctx.content_rect();
    let mut tab_pressed = None;
    {
        let mut panes = Panes {
            graph: &mut *graph,
            stack: &mut *stack,
            hierarchy: &mut *hierarchy,
            subnets: &mut *subnets,
            nav: &mut *nav,
            vp_rect: &mut *vp_rect,
            prim_state: &mut *prim_state,
            playback: &mut *playback,
            browser: &mut *browser,
            graph_file: &mut *graph_file,
            poly_tool: &mut *poly_tool,
            batch: &batch,
            anim_ctx: &anim_ctx,
            timeline_state: &timeline_state,
            dt,
            keys_free,
            viewer,
            nav_settings: *nav_settings,
            space_plays: nav_settings.style != viewport::nav::NavStyle::Houdini,
            revision: revision.0,
            locked,
            toggle_float: &mut toggle_float,
            tab_pressed: &mut tab_pressed,
            uv_state: &mut *uv_state,
            history: &mut *history,
            confirm: &mut *confirm,
            curve_editor: &mut *curve_editor,
            image_viewer: &mut *image_viewer,
        };
        let style = theme::dock_style(root.style());
        layout.size_new_windows();
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
            // What is left under the top bar.
            let screen = ui.max_rect();
            dock_rect = screen;
            // Locked: tabs stay where they are. Dividers resize either way.
            egui_dock::DockArea::new(&mut layout.dock)
                .style(style)
                .window_bounds(screen)
                .draggable_tabs(!locked)
                .show_close_buttons(!locked)
                // A pane is closed by the cross on its tab: no close-all or
                // collapse buttons on the tab bars (egui_dock 0.15 added them).
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .tab_context_menus(!locked)
                .show_inside(ui, &mut panes);
        });
    }

    if let Some(pane) = toggle_float {
        layout.toggle_float(pane);
    }

    // Docking along a whole edge of the window
    // The dock area only offers places beside single panes. While a tab is
    // dragged, the four edges of the window are drop places too: the pane
    // then spans that whole side, as the timeline does by default.
    // A drag is the pointer travelling, not a long press: holding a tab
    // still never docks it.
    let (down, moving, let_go) = ctx.input(|i| {
        let travelled = match (i.pointer.press_origin(), i.pointer.interact_pos()) {
            (Some(a), Some(b)) => a.distance(b) > 8.0,
            _ => false,
        };
        (
            i.pointer.primary_down(),
            i.pointer.is_decidedly_dragging() && travelled,
            i.pointer.primary_released(),
        )
    });
    if let Some(pane) = tab_pressed {
        if layout.tab_drag.is_none() {
            layout.tab_drag = Some((pane, false));
        }
    }
    if let Some((pane, moved)) = layout.tab_drag {
        let released = let_go || !down;
        if released {
            layout.tab_drag = None;
        } else if moving {
            layout.tab_drag = Some((pane, true));
        }
        let moved = moved || moving;
        let screen = dock_rect;
        let edge = ctx
            .input(|i| i.pointer.interact_pos())
            .and_then(|p| layout::Edge::near(screen, p, 26.0));
        if !moved {
            // A click on a tab, not a drag.
        } else if released {
            if let Some(edge) = edge {
                layout.dock_to_edge(pane, edge);
            }
        } else {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("edge_dock")));
            let tint = egui::Color32::from_rgb(120, 170, 255);
            for e in [
                layout::Edge::Left,
                layout::Edge::Right,
                layout::Edge::Top,
                layout::Edge::Bottom,
            ] {
                // A thin bar marks each edge; the one under the cursor shows where the pane will land.
                let mut bar = screen;
                match e {
                    layout::Edge::Left => bar.set_right(screen.left() + 5.0),
                    layout::Edge::Right => bar.set_left(screen.right() - 5.0),
                    layout::Edge::Top => bar.set_bottom(screen.top() + 5.0),
                    layout::Edge::Bottom => bar.set_top(screen.bottom() - 5.0),
                }
                painter.rect_filled(bar, 0.0, tint.gamma_multiply(0.6));
                if edge == Some(e) {
                    painter.rect_filled(e.strip(screen), 0.0, tint.gamma_multiply(0.35));
                    painter.rect_stroke(
                        e.strip(screen).shrink(1.0),
                        0.0,
                        egui::Stroke::new(2.0_f32, tint),
                        egui::StrokeKind::Middle,
                    );
                }
            }
        }
    }

    // Not while a button is down: a drag changes the layout on every frame.
    if !ctx.input(|i| i.pointer.any_down()) {
        layout.save_if_changed();
    }

    // File browser (on top of everything)
    browser.show(ctx);

    // Viewport navigation menu and help
    let Some(vp) = vp_rect.0 else { return };
    let before = *nav_settings;
    let mut chosen = before;
    // The key list is hidden until the question mark is clicked.
    let help_id = egui::Id::new("viewport_nav_help");
    let mut nav_help = ctx.data(|d| d.get_temp::<bool>(help_id)).unwrap_or(false);
    // Background order: floating panes go over the menu, not under it.
    egui::Area::new("viewport_nav_menu".into())
        .order(egui::Order::Background)
        .fixed_pos(vp.min + egui::vec2(10.0, 8.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme::c(90, 90, 90).gamma_multiply(0.92))
                .stroke(egui::Stroke::new(1.0_f32, theme::c(70, 70, 70)))
                .corner_radius(4.0)
                .inner_margin(3.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        crate::menus::menu(ui, format!("Navigation: {} ⏷", chosen.style.label()), |ui| {
                            for style in viewport::nav::NavStyle::ALL {
                                if ui.radio_value(&mut chosen.style, style, style.label()).clicked() { ui.close(); }
                            }
                            ui.separator();
                            ui.checkbox(&mut chosen.invert_zoom, "Invert zoom drag");
                        });
                        if ui.selectable_label(nav_help, "?").on_hover_text("Show the navigation keys").clicked() {
                            nav_help = !nav_help;
                        }
                        let shown = viewport::display::mode();
                        crate::menus::menu(ui, format!("{} ⏷", shown.label()), |ui| {
                            for m in viewport::display::DisplayMode::ALL {
                                if ui.radio(shown == m, m.label()).clicked() { viewport::display::set_mode(m); ui.close(); }
                            }
                        }).response.on_hover_text("How meshes are drawn");
                        // Through a camera of the scene, or the Viewer's own.
                        let looking = viewer::camera::looking_through();
                        let name = looking.as_deref().map(|p| p.rsplit('/').next().unwrap_or(p).to_string()).unwrap_or_else(|| "Perspective".into());
                        crate::menus::menu(ui, format!("🎥 {name} ⏷"), |ui| {
                            if ui.radio(looking.is_none(), "Perspective").clicked() { viewer::camera::look_through(None); ui.close(); }
                            // The scene's cameras, looked for only while the menu is open.
                            let subnets_ref = &*subnets;
                            let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| -> crate::core::geo::Geo {
                                subnets_ref.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
                            };
                            let cameras: Vec<String> = match graph.evaluate_for_viewport_packed(&eval) {
                                Some(types::EvalResult::Named(prims)) => prims.iter().filter(|p| p.camera.is_some()).map(|p| p.path.clone()).collect(),
                                _ => vec![],
                            };
                            ui.separator();
                            if cameras.is_empty() { ui.weak("No cameras in the scene"); }
                            for c in cameras {
                                if ui.radio(looking.as_deref() == Some(c.as_str()), c.as_str()).clicked() { viewer::camera::look_through(Some(c)); ui.close(); }
                            }
                        }).response.on_hover_text("Look through a camera of the scene, or the Viewer's own (Perspective). Navigating leaves the camera, where it was");
                        let loading = viewport::textures::pending();
                        if loading > 0 {
                            ui.label(egui::RichText::new(format!("loading {loading}")).small());
                            ui.ctx().request_repaint();
                        }
                    });
                });
        });
    if chosen != before {
        *nav_settings = chosen;
        chosen.save();
    }

    ctx.data_mut(|d| d.insert_temp(help_id, nav_help));
    if nav_help {
        egui::Area::new("viewport_label".into())
            .order(egui::Order::Background)
            .fixed_pos(vp.min + egui::vec2(10.0, 40.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::c(90, 90, 90).gamma_multiply(0.86))
                    .stroke(egui::Stroke::new(1.0_f32, theme::c(70, 70, 70)))
                    .corner_radius(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        let play = if chosen.style == viewport::nav::NavStyle::Houdini {
                            "Tap Space: play  |  Left/Right: step"
                        } else {
                            "Space: play  |  Left/Right: step"
                        };
                        let nav_lines = chosen.style.help();
                        for line in nav_lines.iter().chain(["Scroll: zoom", "F: focus", play].iter()) {
                            ui.label(egui::RichText::new(*line).color(egui::Color32::from_rgb(190, 190, 190)));
                        }
                    });
            });
    }
}

/// The program's mark: two crossing wires with a node at each end, for the X of
/// XMS and the node graph it is built on.
fn draw_logo(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
    // The program icon, loaded once and kept by egui.
    let id = egui::Id::new("logo_texture");
    let texture = ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(id)).or_else(|| {
        let (rgba, w, h) = decode_png(include_bytes!("../assets/icon.png"))?;
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        let texture = ui.ctx().load_texture("logo", image, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|d| d.insert_temp(id, texture.clone()));
        Some(texture)
    });
    if let Some(texture) = texture {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    response
}

/// A thin vertical line between parts of the top bar.
fn top_bar_divider(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 16.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0, theme::c(92, 92, 92)),
    );
}

/// Menu titles in the top bar: no frame at rest, a lit plate under the mouse
/// and while the menu is open, so a title reads as something to click and the
/// rest of the bar does not.
fn menu_title_style(ui: &mut egui::Ui) {
    let v = ui.visuals_mut();
    v.button_frame = true;
    v.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    v.widgets.inactive.fg_stroke.color = theme::c(236, 236, 236);
    let lit = theme::accent();
    for w in [&mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.weak_bg_fill = lit;
        w.bg_fill = lit;
        w.bg_stroke = egui::Stroke::NONE;
        w.expansion = 0.0;
        w.corner_radius = egui::CornerRadius::same(3);
        w.fg_stroke.color = egui::Color32::from_rgb(250, 250, 250);
    }
    ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
}

/// Padlock button, drawn by hand: closed when locked, shackle swung open when
/// not. The emoji font's padlock is unreadable at this size.
fn lock_button(ui: &mut egui::Ui, locked: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 18.0), egui::Sense::click());
    let painter = ui.painter();
    if locked || response.hovered() {
        let fill = if locked {
            theme::c(80, 95, 115)
        } else {
            theme::raised(100, 100, 100)
        };
        painter.rect_filled(rect, 3.0, fill);
    }
    let ink = if locked {
        egui::Color32::from_rgb(240, 240, 240)
    } else {
        theme::c(233, 233, 233)
    };
    let c = rect.center();
    let body = egui::Rect::from_center_size(c + egui::vec2(0.0, 3.0), egui::vec2(10.0, 7.0));
    painter.rect_filled(body, 1.5, ink);
    // Shackle: a half ring above the body. Open: one leg lifted clear of it.
    let (r, top) = (3.0, body.top() - 3.0);
    let lift = if locked { 0.0 } else { 3.0 };
    let mut pts = vec![egui::pos2(c.x - r, body.top())];
    for k in 0..=8 {
        let a = std::f32::consts::PI * (1.0 - k as f32 / 8.0);
        pts.push(egui::pos2(c.x + r * a.cos(), top - r * a.sin() * 0.9));
    }
    pts.push(egui::pos2(c.x + r, body.top() - lift));
    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.6_f32, ink)));
    response
}

/// Everything the panes draw from, borrowed for one frame.
struct Panes<'a> {
    graph: &'a mut NodeGraphState,
    stack: &'a mut OperatorStack,
    hierarchy: &'a mut SceneHierarchy,
    subnets: &'a mut IceGraphStore,
    nav: &'a mut GraphNavigation,
    vp_rect: &'a mut ViewportRect,
    prim_state: &'a mut PrimInspectorState,
    playback: &'a mut Playback,
    browser: &'a mut FileBrowser,
    graph_file: &'a mut GraphFile,
    poly_tool: &'a mut modelling::PolyTool,
    batch: &'a BatchState,
    anim_ctx: &'a AnimContext,
    timeline_state: &'a TimelineState,
    dt: f64,
    keys_free: bool,
    space_plays: bool,
    revision: u64,
    locked: bool,
    /// Pane whose tab was double-clicked this frame.
    toggle_float: &'a mut Option<Pane>,
    /// Pane whose tab the mouse button went down on this frame.
    tab_pressed: &'a mut Option<Pane>,
    uv_state: &'a mut uv_editor::UvEditorState,
    curve_editor: &'a mut curve_editor::CurveEditor,
    image_viewer: &'a mut image_viewer::ImageViewerState,
    history: &'a mut history::History,
    confirm: &'a mut Confirm,
    /// The Viewer's 3D view, when the interface draws it (eframe).
    viewer: Option<&'a mut viewer::Viewer>,
    nav_settings: viewport::nav::NavSettings,
}

impl egui_dock::TabViewer for Panes<'_> {
    type Tab = Pane;

    // By its title, as egui_dock named tabs before it asked for an id: the
    // state a pane keeps (scroll positions) carries over.
    fn id(&mut self, tab: &mut Pane) -> egui::Id {
        egui::Id::new(tab.title())
    }
    fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
        tab.title().into()
    }
    // Closing hides the pane. The Panes menu shows it again.
    fn is_closeable(&self, _tab: &Pane) -> bool {
        !self.locked
    }
    // Double-click a tab to float it, or to dock it back.
    fn on_tab_button(&mut self, tab: &mut Pane, response: &egui::Response) {
        if self.locked {
            return;
        }
        if response.double_clicked() {
            *self.toggle_float = Some(*tab);
        }
        // The dock area takes over the tab while it is dragged, so only the
        // press is seen here. The drag is followed from the pointer.
        if response.is_pointer_button_down_on() {
            *self.tab_pressed = Some(*tab);
        }
    }
    // Panes scroll their own content where they need to.
    fn scroll_bars(&self, _tab: &Pane) -> [bool; 2] {
        [false, false]
    }
    // The 3D view is drawn by the camera behind the interface.
    fn clear_background(&self, tab: &Pane) -> bool {
        *tab != Pane::Viewer || self.viewer.is_some()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Pane) {
        match tab {
            Pane::Viewer => {
                self.vp_rect.0 = Some(ui.max_rect());
                if let Some(v) = self.viewer.as_deref_mut() {
                    v.show(
                        ui,
                        viewer::ViewerInput {
                            graph: &mut *self.graph,
                            subnets: &*self.subnets,
                            hierarchy: &mut *self.hierarchy,
                            playback: &mut *self.playback,
                            revision: self.revision,
                            nav: self.nav_settings,
                            tool: &mut *self.poly_tool,
                        },
                    );
                }
            }

            Pane::Timeline => {
                draw_timeline(
                    ui,
                    self.playback,
                    self.timeline_state,
                    self.graph,
                    self.dt,
                    self.keys_free,
                    self.space_plays,
                );
            }

            Pane::Parameters => {
                // A scroll area that never shrinks keeps the pane at the size
                // it was given: text wraps, anything still too big scrolls.
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    let poly_input = self.graph.selected_node.and_then(|id| {
                        let subnets = &*self.subnets;
                        let eval = |sid: IceGraphId,
                                    mesh: &crate::core::geo::Geo,
                                    template: &[crate::ice::Reference]|
                         -> crate::core::geo::Geo {
                            subnets
                                .get(sid)
                                .map(|sg| sg.evaluate(mesh, template))
                                .unwrap_or_else(|| mesh.clone())
                        };
                        let is_edit_poly = self
                            .graph
                            .nodes
                            .iter()
                            .any(|n| n.id == id && matches!(n.node_type, NodeType::EditPoly { .. }));
                        if is_edit_poly {
                            modelling::input_mesh(self.graph, id, &eval)
                        } else {
                            None
                        }
                    });
                    let mut tool = self.poly_tool.tool;
                    let scene_paths = self.hierarchy.selected_prim_paths();
                    let mut io = PanelIo {
                        browser: self.browser,
                        batch: self.batch,
                        action: None,
                        poly_input,
                        tool: &mut tool,
                        scene_paths,
                        add: vec![],
                    };
                    // A keyed node shows its parameters at the playhead; a
                    // keyed number changed here is keyed there.
                    let editing = self
                        .graph
                        .selected_node
                        .and_then(|id| self.graph.begin_edit(id).map(|own| (id, own)));
                    draw_properties_panel(
                        ui,
                        self.graph,
                        &*self.stack,
                        self.subnets,
                        self.nav,
                        self.anim_ctx,
                        &mut io,
                    );
                    if let Some((id, own)) = editing {
                        self.graph.end_edit(id, own);
                    }
                    if let Some(id) = self.graph.selected_node {
                        crate::keys_ui::channel_box(ui, self.graph, id);
                    }
                    // Operators the panel asked for: a MaterialX operator from a stage's material.
                    for (name, t) in std::mem::take(&mut io.add) {
                        import_node(self.graph, name, t);
                    }
                    // Writing over files that exist asks first.
                    if let Some(action) = io.action {
                        let existing = existing_outputs(self.graph, &action);
                        if existing.is_empty() {
                            run_action(self.graph, &*self.subnets, self.batch, action);
                        } else {
                            self.confirm.pending = Some((existing, action));
                        }
                    }
                    if tool != self.poly_tool.tool {
                        self.poly_tool.tool = tool;
                    }
                });
            }

            Pane::SceneExplorer => {
                egui::ScrollArea::both()
                    .id_salt("scene_explorer_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| draw_scene_explorer(ui, self.hierarchy));
            }

            Pane::CurveEditor => {
                curve_editor::draw(ui, self.curve_editor, self.graph, self.playback);
            }

            Pane::History => {
                history::draw(ui, self.history, self.graph.selected_node);
            }

            Pane::ImageViewer => {
                image_viewer::draw(ui, self.image_viewer, self.graph, &*self.subnets, self.revision);
            }

            Pane::OperatorStack => {
                egui::ScrollArea::both()
                    .id_salt("operator_stack_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| draw_operator_stack(ui, self.stack, self.graph));
            }

            Pane::UvEditor => {
                let subnets = &*self.subnets;
                let get_mesh = |g: &NodeGraphState| -> Option<MeshData> {
                    let id = g.selected_node?;
                    let eval = |sid: IceGraphId,
                                mesh: &crate::core::geo::Geo,
                                template: &[crate::ice::Reference]|
                     -> crate::core::geo::Geo {
                        subnets
                            .get(sid)
                            .map(|sg| sg.evaluate(mesh, template))
                            .unwrap_or_else(|| mesh.clone())
                    };
                    g.eval_node(id, &mut std::collections::HashMap::new(), &eval)
                        .map(|r| r.into_mesh())
                };
                uv_editor::draw_uv_editor(ui, self.graph, self.uv_state, self.revision, &get_mesh);
            }

            Pane::PrimInspector => {
                // The scene at the view flag: the rows selected in the Scene
                // Explorer and what is below them, or all of it.
                let subnets = &*self.subnets;
                let paths = self.hierarchy.selected_prim_paths();
                let get_parts = |g: &NodeGraphState| -> Option<Vec<MeshData>> {
                    let eval = |sid: IceGraphId,
                                mesh: &crate::core::geo::Geo,
                                template: &[crate::ice::Reference]|
                     -> crate::core::geo::Geo {
                        subnets
                            .get(sid)
                            .map(|sg| sg.evaluate(mesh, template))
                            .unwrap_or_else(|| mesh.clone())
                    };
                    let _t = crate::timings::span("primitive inspector: geometry for a tab");
                    let (_, _, scene) = g.evaluate_scene(&eval)?;
                    Some(prim_inspector::scene_parts(scene, &paths))
                };
                let get_summary = |g: &NodeGraphState| -> prim_inspector::Summary {
                    let eval = |sid: IceGraphId,
                                mesh: &crate::core::geo::Geo,
                                template: &[crate::ice::Reference]|
                     -> crate::core::geo::Geo {
                        subnets
                            .get(sid)
                            .map(|sg| sg.evaluate(mesh, template))
                            .unwrap_or_else(|| mesh.clone())
                    };
                    let _t = crate::timings::span("primitive inspector: counts");
                    g.evaluate_scene(&eval)
                        .map(|(_, _, scene)| prim_inspector::scene_summary(&scene, &paths))
                        .unwrap_or_default()
                };
                let get_clip =
                    |g: &NodeGraphState| g.display_source().and_then(|id| g.eval_anim_out(id, g.viewed_output()));
                let get_props = |g: &NodeGraphState| -> Vec<prim_inspector::PropRow> {
                    let eval = |sid: IceGraphId,
                                mesh: &crate::core::geo::Geo,
                                template: &[crate::ice::Reference]|
                     -> crate::core::geo::Geo {
                        subnets
                            .get(sid)
                            .map(|sg| sg.evaluate(mesh, template))
                            .unwrap_or_else(|| mesh.clone())
                    };
                    let _t = crate::timings::span("primitive inspector: properties");
                    match g.evaluate_scene(&eval) {
                        Some((view, _, scene)) => prim_inspector::scene_properties(g, view, &scene, &paths),
                        None => vec![],
                    }
                };
                let source = prim_inspector::Source {
                    node: self.graph.display_source(),
                    paths: self.hierarchy.selected_prim_paths(),
                };
                let go_to = draw_prim_inspector(
                    ui,
                    self.graph,
                    self.prim_state,
                    source,
                    self.revision,
                    self.playback.time,
                    &get_parts,
                    &get_summary,
                    &get_clip,
                    &get_props,
                );
                // "Go to Operator" on a Properties row: select the operator the value comes from.
                if let Some(id) = go_to {
                    self.graph.selected_node = Some(id);
                    self.graph.selected_nodes = vec![id];
                }
            }

            // A MaterialX operator's graph.
            Pane::NodeGraph if self.nav.materialx.is_some() => {
                let op = self.nav.materialx.unwrap_or(types::NodeId(usize::MAX));
                let found = self
                    .graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == op)
                    .and_then(|n| match &mut n.node_type {
                        NodeType::MaterialX { graph } => Some((n.name.clone(), graph)),
                        _ => None,
                    });
                match found {
                    Some((name, g)) => {
                        if draw_breadcrumb(ui, &name) {
                            self.nav.materialx = None;
                        } else {
                            ui.label("Right-click or Tab: add  |  Drag an output to an input, or onto a node  |  Right-click a wire: take it off  |  Del: delete  |  Shift/middle-drag: pan  |  F: frame  |  ↑: back to the scene graph");
                            ui.separator();
                            let rect = ui.available_rect_before_wrap();
                            let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
                            canvas.set_clip_rect(rect.intersect(ui.clip_rect()));
                            let up = materialx::ui::draw(&mut canvas, op, g);
                            ui.allocate_rect(rect, egui::Sense::hover());
                            if up {
                                self.nav.materialx = None;
                            }
                        }
                    }
                    None => self.nav.materialx = None,
                }
            }
            Pane::NodeGraph => match self.nav.current_subnet {
                Some(sid) => {
                    if let Some(sg) = self.subnets.get_mut(sid) {
                        let subnet_name = sg.name.clone();
                        if draw_breadcrumb(ui, &subnet_name) {
                            self.nav.current_subnet = None;
                        } else {
                            ui.label("Right-click: add  |  Del: delete  |  Shift/middle-drag: pan  |  Wheel: zoom  |  F: frame  |  ↑: back to the scene graph  |  Esc: cancel wire");
                            ui.separator();
                            // Canvas in its own child Ui: see the note below.
                            let rect = ui.available_rect_before_wrap();
                            let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
                            canvas.set_clip_rect(rect.intersect(ui.clip_rect()));
                            let up = draw_subnet_graph(&mut canvas, sg);
                            ui.allocate_rect(rect, egui::Sense::hover());
                            if up {
                                self.nav.current_subnet = None;
                            }
                        }
                    } else {
                        self.nav.current_subnet = None;
                    }
                }
                None => {
                    // Wraps when the pane is narrow.
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Tidy").on_hover_text("Lay the nodes out in rows, top to bottom, and bring them into view").clicked() {
                            self.graph.auto_layout();
                        }
                        if ui.button("Frame").on_hover_text("Bring every node into view (F or A over the graph)").clicked() {
                            self.graph.frame_request = true;
                        }
                        ui.small_button("?").on_hover_text(
                            "Right-click or Tab: add a node\nCtrl+C, Ctrl+V: copy and paste nodes\nShift+drag: pan\nEsc: cancel a wire\nDouble-click an ICE node, or ↓: open its ICE graph\nRight-click an output: add a node under it, wired\nF or A: frame every node\nRing at the left of a node: bypass\nEye at the right: show in the Viewer\nSnowflake: freeze, keep the result as it is\nSquare: ghost, drawn grey in the Viewer\nMiddle button held on a node: what it puts out");
                    });
                    if !self.graph_file.message.is_empty() {
                        ui.label(
                            egui::RichText::new(&self.graph_file.message)
                                .small()
                                .color(theme::c(230, 230, 230)),
                        );
                    }
                    ui.separator();

                    // The canvas gets its own child Ui. Nodes are widgets placed
                    // at arbitrary positions; drawn straight into the pane, a
                    // node dragged or panned past the edge would stretch it.
                    let rect = ui.available_rect_before_wrap();
                    let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
                    canvas.set_clip_rect(rect.intersect(ui.clip_rect()));
                    let dive = draw_node_graph(&mut canvas, self.graph);
                    ui.allocate_rect(rect, egui::Sense::hover());
                    {
                        let subnets = &*self.subnets;
                        let eval = |sid: IceGraphId,
                                    mesh: &crate::core::geo::Geo,
                                    template: &[crate::ice::Reference]|
                         -> crate::core::geo::Geo {
                            subnets
                                .get(sid)
                                .map(|sg| sg.evaluate(mesh, template))
                                .unwrap_or_else(|| mesh.clone())
                        };
                        crate::node_graph::ui::info_popup(ui.ctx(), self.graph, rect, &eval);
                    }

                    // Ctrl+C, Ctrl+V (Cmd on macOS) over the canvas: copy the
                    // selected nodes, paste them under the pointer.
                    let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p));
                    if pointer.is_some() && !ui.ctx().egui_wants_keyboard_input() && !self.browser.is_open() {
                        // egui gives Cmd+V (Ctrl+V) as a Paste event holding
                        // the clipboard's text, not as a key; a V key with
                        // the command modifier still counts, for backends
                        // that send one.
                        let (copy, paste, text) = ui.input(|i| {
                            let copy = i.events.iter().any(|e| matches!(e, egui::Event::Copy));
                            let paste = i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))
                                || matches!(e, egui::Event::Key { key: egui::Key::V, pressed: true, modifiers, .. } if modifiers.command));
                            let text = i.events.iter().find_map(|e| match e {
                                egui::Event::Text(t) | egui::Event::Paste(t) if t.trim_start().starts_with('{') => Some(t.clone()), _ => None });
                            (copy, paste, text)
                        });
                        if copy {
                            if let Some(m) = copy_nodes(ui.ctx(), self.graph, self.subnets) {
                                self.graph_file.message = m;
                            }
                        }
                        if paste && text.as_deref().is_some_and(graph_io::is_ice_clip) {
                            self.graph_file.message = "These are ICE nodes: paste them in an ICE graph".into();
                        } else if paste {
                            let (pan, zoom) = (self.graph.pan_offset, self.graph.zoom);
                            let at = pointer.map(|p| {
                                egui::pos2((p.x - rect.min.x - pan.x) / zoom, (p.y - rect.min.y - pan.y) / zoom)
                            });
                            self.graph_file.message = paste_nodes(self.graph, self.subnets, text.as_deref(), at)
                                .unwrap_or_else(|| "Nothing copied to paste".into());
                        }
                    }

                    for node in self.graph.nodes.iter_mut() {
                        if let NodeType::Ice { id, name, .. } = &mut node.node_type {
                            if *id == IceGraphId(usize::MAX) {
                                let new_id = self.subnets.create_subnet(name.clone());
                                *id = new_id;
                            }
                        }
                    }
                    match dive {
                        Some(node_graph::ui::Dive::Ice(sid)) if sid != IceGraphId(usize::MAX) => {
                            self.nav.current_subnet = Some(sid)
                        }
                        Some(node_graph::ui::Dive::MaterialX(op)) => self.nav.materialx = Some(op),
                        _ => {}
                    }
                }
            },
        }
    }
}

/// Undo and redo for one frame: the shortcuts, and a step recorded when the
/// graph changed and nothing is being dragged or typed.
pub(crate) fn history_frame(ctx: &egui::Context, app: &mut AppState) {
    let AppState {
        graph,
        history,
        nav,
        browser,
        subnets,
        ..
    } = app;
    history.set_ice(subnets.content_hash());
    let typing = ctx.egui_wants_keyboard_input();
    if !typing && !browser.is_open() {
        let (redo, undo) = ctx.input_mut(|i| {
            use egui::{Key, Modifiers};
            let redo = i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || i.consume_key(Modifiers::COMMAND, Key::Y);
            let undo = !redo && i.consume_key(Modifiers::COMMAND, Key::Z);
            (redo, undo)
        });
        if redo {
            history.request = Some(history::Request::Redo);
        }
        if undo {
            history.request = Some(history::Request::Undo);
        }
    }
    let busy = typing || browser.is_open() || ctx.egui_is_using_pointer() || ctx.input(|i| i.pointer.any_down());
    // Look without touching: only an actual change marks the graph changed.
    if history.update(graph, busy) {
        // Leave a subnet whose node is gone.
        if let Some(sid) = nav.current_subnet {
            let there = graph
                .nodes
                .iter()
                .any(|n| matches!(&n.node_type, NodeType::Ice { id, .. } if *id == sid));
            if !there {
                nav.current_subnet = None;
            }
        }
        // And a MaterialX graph whose operator is gone.
        if let Some(op) = nav.materialx {
            let there = graph
                .nodes
                .iter()
                .any(|n| n.id == op && matches!(n.node_type, NodeType::MaterialX { .. }));
            if !there {
                nav.materialx = None;
            }
        }
    }
}

/// Move the graph's revision when its content changed. `last`: the content hash
/// seen last time.
pub(crate) fn track_revision_frame(ctx: &egui::Context, app: &mut AppState, last: &mut Option<u64>) {
    let AppState {
        graph,
        subnets,
        revision,
        ..
    } = app;
    // The scene graph and what its ICE graphs compute: moving, panning or
    // selecting in an ICE graph changes neither, so nothing is cooked again.
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (graph.content_hash(), subnets.content_hash()).hash(&mut h);
        h.finish()
    };
    let _ = ctx;
    // A solve that finished on its thread changes what its node puts out.
    // So does a stage read in the background, as its boxes then all of it arrive.
    if *last != Some(hash) || ragdoll::take_changed() | usd_scene::take_changed() {
        *last = Some(hash);
        revision.0 = revision.0.wrapping_add(1);
    }
}

/// Feedback line for graph open / save, and the file the graph is in.
#[derive(Default)]
pub(crate) struct GraphFile {
    message: String,
    /// Last opened from or saved to. None: never saved, or a template.
    path: Option<std::path::PathBuf>,
}

/// Imago's graph files: `.xms` (JSON inside); `.json` graphs saved before still
/// open.
const GRAPH_FILES: &[&str] = &["xms", "json"];

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Open a saved graph. It replaces the scene as one undoable step, becomes the
/// current file, and goes to the top of Recent.
fn open_graph(
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    path: &std::path::Path,
    file: &mut GraphFile,
    history: &mut history::History,
    recent: &mut recent::Recent,
) {
    // Its ICE graphs in a store of their own, taking the old one's place
    // once the file has been read.
    let mut fresh = IceGraphStore::default();
    match graph_io::load(graph, &mut fresh, path) {
        Ok(()) => {
            *subnets = fresh;
            history::note(format!("Open {}", file_name(path)));
            history.mark_clean();
            recent.add(path);
            file.path = Some(path.to_path_buf());
            file.message = format!("Opened {}", path.display());
        }
        Err(e) => file.message = format!("Could not open {}: {e}", path.display()),
    }
}

// Menu bar

/// What a File menu item or its shortcut asks for.
#[derive(Clone, Debug, PartialEq)]
enum FileCommand {
    New,
    Open,
    OpenRecent(std::path::PathBuf),
    /// A template, by its index in `templates::TEMPLATES`.
    Template(usize),
    ClearRecent,
    Save,
    SaveAs,
    ImportGraph,
    ImportFbx,
    ImportUsd,
    ImportMtlx,
    Variables,
    Quit,
}

/// A menu item with its shortcut written at the right, as menus show them.
fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let clicked = ui.add(egui::Button::new(label).shortcut_text(shortcut)).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

const CMD: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };

fn file_menu(ui: &mut egui::Ui, recent: &recent::Recent, file: &GraphFile) -> Option<FileCommand> {
    let mut c = None;
    crate::menus::menu(ui, "File", |ui| {
        ui.set_min_width(200.0);
        if menu_item(ui, "New", &format!("{CMD}+N")) {
            c = Some(FileCommand::New);
        }
        if menu_item(ui, "Open...", &format!("{CMD}+O")) {
            c = Some(FileCommand::Open);
        }
        crate::menus::menu(ui, "Open Recent", |ui| {
            if recent.files.is_empty() {
                ui.label(egui::RichText::new("No graphs opened or saved yet").weak());
            }
            for f in &recent.files {
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let there = f.is_file();
                let label = if there { name } else { format!("{name} (missing)") };
                if ui
                    .add_enabled(there, egui::Button::new(label))
                    .on_hover_text(f.display().to_string())
                    .on_disabled_hover_text(format!("{}\nNot found", f.display()))
                    .clicked()
                {
                    c = Some(FileCommand::OpenRecent(f.clone()));
                    ui.close();
                }
            }
            if !recent.files.is_empty() {
                ui.separator();
                if ui.button("Clear the list").clicked() {
                    c = Some(FileCommand::ClearRecent);
                    ui.close();
                }
            }
        });
        ui.separator();
        let save = ui
            .add(egui::Button::new("Save").shortcut_text(format!("{CMD}+S")))
            .on_hover_text(match &file.path {
                Some(p) => format!("Save to {}", p.display()),
                None => "Not saved to a file yet: asks where".into(),
            });
        if save.clicked() {
            c = Some(FileCommand::Save);
            ui.close();
        }
        if menu_item(ui, "Save As...", &format!("{CMD}+Shift+S")) {
            c = Some(FileCommand::SaveAs);
        }
        ui.separator();
        if ui
            .button("Variables...")
            .on_hover_text("The graph's own variables and $JOB, saved with it, for paths and command lines")
            .clicked()
        {
            c = Some(FileCommand::Variables);
            ui.close();
        }
        ui.separator();
        crate::menus::menu(ui, "Import", |ui| {
            if ui
                .button("Graph...")
                .on_hover_text("Add the nodes of a saved graph beside these")
                .clicked()
            {
                c = Some(FileCommand::ImportGraph);
                ui.close();
            }
            if ui
                .button("FBX...")
                .on_hover_text("Add a Load FBX node reading the file")
                .clicked()
            {
                c = Some(FileCommand::ImportFbx);
                ui.close();
            }
            if ui
                .button("USD...")
                .on_hover_text("Add a Load USD node reading the file")
                .clicked()
            {
                c = Some(FileCommand::ImportUsd);
                ui.close();
            }
            if ui
                .button("MaterialX...")
                .on_hover_text("Add a MaterialX operator for each material of a .mtlx file")
                .clicked()
            {
                c = Some(FileCommand::ImportMtlx);
                ui.close();
            }
        });
        ui.separator();
        if menu_item(ui, "Quit", &format!("{CMD}+Q")) {
            c = Some(FileCommand::Quit);
        }
        ui.separator();
        if let Some(i) = templates_menu(ui) {
            c = Some(FileCommand::Template(i));
        }
    });
    c
}

fn edit_menu(
    ui: &mut egui::Ui,
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    history: &mut history::History,
    file: &mut GraphFile,
) {
    crate::menus::menu(ui, "Edit", |ui| {
        ui.set_min_width(200.0);
        if ui
            .add_enabled(
                history.can_undo(),
                egui::Button::new("Undo").shortcut_text(format!("{CMD}+Z")),
            )
            .clicked()
        {
            history.request = Some(history::Request::Undo);
            ui.close();
        }
        if ui
            .add_enabled(
                history.can_redo(),
                egui::Button::new("Redo").shortcut_text(format!("{CMD}+Shift+Z")),
            )
            .clicked()
        {
            history.request = Some(history::Request::Redo);
            ui.close();
        }
        ui.separator();
        let any = !graph.selected_nodes.is_empty();
        if ui
            .add_enabled(any, egui::Button::new("Copy").shortcut_text(format!("{CMD}+C")))
            .clicked()
        {
            if let Some(m) = copy_nodes(ui.ctx(), graph, subnets) {
                file.message = m;
            }
            ui.close();
        }
        if ui
            .add(egui::Button::new("Paste").shortcut_text(format!("{CMD}+V")))
            .on_hover_text("The nodes copied last, in this window or another")
            .clicked()
        {
            file.message = paste_nodes(graph, subnets, None, None).unwrap_or_else(|| "Nothing copied to paste".into());
            ui.close();
        }
        if ui
            .add_enabled(
                any || graph.selected_connection.is_some(),
                egui::Button::new("Delete").shortcut_text("Del"),
            )
            .clicked()
        {
            graph.delete_selected();
            ui.close();
        }
        ui.separator();
        if ui.button("Select All").clicked() {
            graph.selected_nodes = graph.nodes.iter().map(|n| n.id).collect();
            ui.close();
        }
    });
}

/// Ready-made graphs, at the bottom of the File menu, by whose work they show,
/// then by area. Picking one asks whether to add it to the graph or open it as
/// new (`SaveFirst::load`). Returns the template picked.
fn templates_menu(ui: &mut egui::Ui) -> Option<usize> {
    let mut picked = None;
    crate::menus::menu(ui, "Templates", |ui| {
        for (by, groups) in templates::MENU {
            crate::menus::menu(ui, by, |ui| {
                for group in groups.iter().copied() {
                    crate::menus::menu(ui, group, |ui| {
                        for (index, t) in templates::TEMPLATES
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| t.by == by && t.group == group)
                        {
                            if ui.button(t.name).on_hover_text(t.hint).clicked() {
                                picked = Some(index);
                                ui.close();
                            }
                        }
                    });
                }
            });
        }
    });
    picked
}

/// Panes, layout and theme.
fn window_menu(ui: &mut egui::Ui, layout: &mut Layout, browser: &mut FileBrowser, locked: bool) {
    ui.set_min_width(200.0);
    for pane in Pane::ALL {
        let mut open = layout.is_open(pane);
        let label = if layout.is_floating(pane) {
            format!("{} (floating)", pane.title())
        } else {
            pane.title().to_string()
        };
        if ui.add_enabled(!locked, egui::Checkbox::new(&mut open, label)).changed() {
            if open {
                layout.show(pane);
            } else {
                layout.hide(pane);
            }
        }
    }
    ui.separator();
    if ui
        .add_enabled(
            !locked && layout.any_floating(),
            egui::Button::new("Dock floating panes"),
        )
        .clicked()
    {
        layout.dock_all();
        ui.close();
    }
    if ui.add_enabled(!locked, egui::Button::new("Reset layout")).clicked() {
        layout.reset();
        ui.close();
    }
    ui.separator();
    if ui
        .button("Save layout...")
        .on_hover_text("Write this layout to a file")
        .clicked()
    {
        browser.open(
            BrowseTarget::SaveLayout,
            BrowseMode::Save,
            "Save layout",
            &["json"],
            "layout.json",
        );
        ui.close();
    }
    if ui
        .add_enabled(!locked, egui::Button::new("Load layout..."))
        .on_hover_text("Replace this layout with one from a file")
        .clicked()
    {
        browser.open(BrowseTarget::OpenLayout, BrowseMode::File, "Load layout", &["json"], "");
        ui.close();
    }
    if locked {
        ui.label(egui::RichText::new("Unlock the layout to change it.").small());
    }
    ui.separator();
    crate::menus::menu(ui, "Theme", theme::menu);
}

/// Add a node reading an imported file, right of the nodes there, selected.
fn import_node(graph: &mut NodeGraphState, name: String, node_type: NodeType) -> types::NodeId {
    let x = graph.nodes.iter().map(|n| n.position.x).fold(f32::MIN, f32::max);
    let y = graph.nodes.iter().map(|n| n.position.y).fold(f32::MAX, f32::min);
    let at = if x == f32::MIN {
        egui::pos2(200.0, 100.0)
    } else {
        egui::pos2(x + 260.0, y)
    };
    let id = graph.add_node(name.clone(), node_type, at);
    graph.selected_node = Some(id);
    graph.selected_nodes = vec![id];
    graph.frame_request = true;
    history::note(format!("Import {name}"));
    id
}

/// Writing over files that exist asks first, as in any other program.
#[derive(Default)]
pub(crate) struct Confirm {
    /// What exists, and the write waiting on Replace.
    pending: Option<(Vec<String>, PanelAction)>,
}

/// Files a write from the properties panel would replace.
fn existing_outputs(graph: &NodeGraphState, action: &PanelAction) -> Vec<String> {
    match action {
        PanelAction::Write { targets, all_files } => batch::existing_outputs(graph, targets, *all_files),
        PanelAction::WriteUsd(id) => graph
            .nodes
            .iter()
            .find(|n| n.id == *id)
            .and_then(|n| match &n.node_type {
                NodeType::WriteUsd { path } if std::path::Path::new(&graph.expand_at(n.id, path.trim())).is_file() => {
                    Some(graph.expand_at(n.id, path.trim()))
                }
                _ => None,
            })
            .into_iter()
            .collect(),
        // Rendering writes its layer again, as asked: no question.
        PanelAction::RunCommand(_) => vec![],
    }
}

fn run_action(graph: &NodeGraphState, subnets: &IceGraphStore, batch: &BatchState, action: PanelAction) {
    match action {
        PanelAction::Write { targets, all_files } => batch::start(batch, graph, targets, all_files),
        PanelAction::WriteUsd(id) => write_usd(graph, subnets, id),
        PanelAction::RunCommand(id) => run_command(graph, subnets, id),
    }
}

/// Run a Render Command: the Write USD above it writes its layer first when it
/// says so, then its command lines start, in the background.
fn run_command(graph: &NodeGraphState, subnets: &IceGraphStore, id: types::NodeId) {
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else {
        return;
    };
    let NodeType::RenderCommand { command } = &node.node_type else {
        return;
    };
    // `{usd}` as it comes down the wires: the layer of the Write USD above,
    // unless a Set Variable below that says otherwise.
    let above = graph.from_above(id);
    let usd_from = above.iter().find(|a| a.name == "usd");
    let usd = usd_from.map(|a| a.value.clone()).unwrap_or_default();
    let writer = usd_from.map(|a| a.from).filter(|w| {
        graph
            .nodes
            .iter()
            .any(|n| n.id == *w && matches!(n.node_type, NodeType::WriteUsd { .. }))
    });
    // `$OPNAME` in its line, folder and variables is its name; `{name}`
    // what comes down to it.
    let _operator = graph.operator(id);
    if command.write_first {
        if let Some(w) = writer {
            write_usd(graph, subnets, w);
            if let Some(Err(e)) = usd_write::last(w) {
                crate::core::command::fail(id.0, format!("Not run: the layer could not be written: {e}"));
                return;
            }
        }
    }
    // The folders the products write into: renderers do not make them.
    if command.make_folders {
        let eval = |sid: IceGraphId,
                    mesh: &crate::core::geo::Geo,
                    template: &[crate::ice::Reference]|
         -> crate::core::geo::Geo {
            subnets
                .get(sid)
                .map(|sg| sg.evaluate(mesh, template))
                .unwrap_or_else(|| mesh.clone())
        };
        let scene = node
            .inputs
            .first()
            .and_then(|i| i.connected_output)
            .and_then(|(src, out)| graph.eval_node_out(src, out, &mut std::collections::HashMap::new(), &eval));
        if let Some(types::EvalResult::Named(prims)) = scene {
            let base = std::path::Path::new(&usd)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default();
            for dir in crate::core::render::output_folders(&prims, &base) {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    crate::core::command::fail(id.0, format!("Not run: could not make {}: {e}", dir.display()));
                    return;
                }
            }
        }
    }
    let lines = command.lines(&usd, &node.name, (graph.scene.start, graph.scene.end));
    // Its variables' values, and its folder, with Imago's variables put in.
    let locals = [("usd", usd.clone())];
    let env = command
        .env
        .iter()
        .map(|e| (e.name.clone(), crate::core::vars::expand(&e.value, &locals)))
        .collect();
    crate::core::command::start(id.0, lines, command.folder_for(&usd), env);
}

/// Add a saved graph to the one open, beside its nodes, as one undoable step.
/// The current file stays the same.
fn add_graph_file(
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    path: &std::path::Path,
    recent: &mut recent::Recent,
) -> String {
    let (mut other, mut other_subnets) = (NodeGraphState::default(), IceGraphStore::default());
    match graph_io::load(&mut other, &mut other_subnets, path) {
        Ok(()) => {
            let n = graph_io::add_graph(graph, subnets, &other, &other_subnets);
            history::note(format!("Add {}", file_name(path)));
            recent.add(path);
            format!("Added {n} nodes from {}", path.display())
        }
        Err(e) => format!("Could not add {}: {e}", path.display()),
    }
}

/// Add a template to the graph open, beside its nodes.
fn add_template(graph: &mut NodeGraphState, subnets: &mut IceGraphStore, t: &templates::Template) -> String {
    // Made apart, then added: the added nodes get copies of its ICE graphs.
    let (mut other, mut other_subnets) = (NodeGraphState::default(), IceGraphStore::default());
    let message = t.load(&mut other, &mut other_subnets);
    let n = graph_io::add_graph(graph, subnets, &other, &other_subnets);
    history::note(format!("Add template: {}", t.name));
    format!("Added {n} nodes: {}. {message}", t.name)
        .trim_end_matches(". ")
        .to_string()
}

/// Copy the selected nodes to the clipboard, and to a file kept for later
/// sessions. Returns what to tell the user.
fn copy_nodes(ctx: &egui::Context, graph: &NodeGraphState, subnets: &IceGraphStore) -> Option<String> {
    let fragment = graph_io::copy(graph, subnets, &graph.selected_nodes)?;
    let text = fragment.to_text();
    graph_io::remember(&text);
    ctx.copy_text(text);
    let n = fragment.len();
    Some(format!("Copied {n} node{}", if n == 1 { "" } else { "s" }))
}

/// Paste nodes copied in this session or another: from the clipboard when it
/// holds nodes, otherwise the nodes copied last.
fn paste_nodes(
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    clipboard: Option<&str>,
    at: Option<egui::Pos2>,
) -> Option<String> {
    let fragment = clipboard
        .and_then(graph_io::Fragment::from_text)
        .or_else(graph_io::remembered)?;
    let n = fragment.len();
    graph_io::paste(graph, subnets, fragment, at);
    let what = format!("{n} node{}", if n == 1 { "" } else { "s" });
    history::note(format!("Paste {what}"));
    Some(format!("Pasted {what}"))
}

/// Write the graph to `path`. Returns whether it was written.
fn save_graph(
    graph: &NodeGraphState,
    subnets: &IceGraphStore,
    path: &std::path::Path,
    file: &mut GraphFile,
    history: &mut history::History,
    recent: &mut recent::Recent,
) -> bool {
    match graph_io::save(graph, subnets, path) {
        Ok(()) => {
            history.mark_clean();
            recent.add(path);
            file.path = Some(path.to_path_buf());
            file.message = format!("Saved {}", path.display());
            true
        }
        Err(e) => {
            file.message = format!("Could not save {}: {e}", path.display());
            false
        }
    }
}

/// Open the browser to choose where to save, starting at the current file.
fn ask_where_to_save(file: &GraphFile, browser: &mut FileBrowser) {
    // A graph opened from an older .json is offered as .xms.
    let name = file
        .path
        .as_deref()
        .map(|p| file_name(&p.with_extension("xms")))
        .unwrap_or_else(|| "graph.xms".into());
    if let Some(dir) = file.path.as_deref().and_then(|p| p.parent()) {
        browser.set_dir(dir);
    }
    browser.open(BrowseTarget::SaveGraph, BrowseMode::Save, "Save graph", &["xms"], &name);
}

/// Save to the current file, or ask where when there is none. Returns whether
/// the graph was written now.
fn save_or_ask(
    graph: &NodeGraphState,
    subnets: &IceGraphStore,
    file: &mut GraphFile,
    history: &mut history::History,
    recent: &mut recent::Recent,
    browser: &mut FileBrowser,
) -> bool {
    match file.path.clone() {
        Some(p) => save_graph(graph, subnets, &p, file, history, recent),
        None => {
            ask_where_to_save(file, browser);
            false
        }
    }
}

/// A graph to bring in: a saved file or a template.
#[derive(Clone, Debug, PartialEq)]
enum Source {
    File(std::path::PathBuf),
    /// Index in `templates::TEMPLATES`.
    Template(usize),
}

impl Source {
    fn name(&self) -> String {
        match self {
            Source::File(p) => file_name(p),
            Source::Template(i) => format!("the template {}", templates::TEMPLATES[*i].name),
        }
    }

    /// Opening it as new: what waits on unsaved changes.
    fn open(self) -> After {
        match self {
            Source::File(p) => After::Open(p),
            Source::Template(i) => After::Template(i),
        }
    }
}

/// What replaces the graph, or closes it, once unsaved changes are dealt with.
#[derive(Clone, Debug, PartialEq)]
enum After {
    Quit,
    /// An empty graph.
    New,
    Open(std::path::PathBuf),
    /// Index in `templates::TEMPLATES`.
    Template(usize),
}

impl After {
    fn question(&self, file: &GraphFile) -> String {
        let changes = match &file.path {
            Some(p) => format!("the changes to {}", file_name(p)),
            None => "this graph".into(),
        };
        match self {
            After::Quit => format!("Save {changes} before closing?"),
            After::New => format!("Save {changes} before starting a new graph?"),
            After::Open(p) => format!("Save {changes} before opening {}?", file_name(p)),
            After::Template(i) => format!(
                "Save {changes} before loading the template {}?",
                templates::TEMPLATES[*i].name
            ),
        }
    }

    fn warning(&self) -> &'static str {
        match self {
            After::Quit => "Closing without saving loses them.",
            After::New => "A new graph replaces this one: without saving, the changes are lost.",
            _ => "Opening it as new replaces the whole graph: without saving, the changes are lost.",
        }
    }
}

/// Opening a file or picking a template asks whether to add it to the graph or
/// open it as new. Anything that replaces or closes the graph then asks first
/// when it has unsaved changes: Save, Don't save or Cancel. Opening a file,
/// picking a template and closing the window all go through here.
#[derive(Default)]
pub(crate) struct SaveFirst {
    /// A file or template picked: add it, or open it as new?
    choosing: Option<Source>,
    after: Option<After>,
    /// Save was chosen for a graph with no file: the browser asks where.
    saving_as: bool,
    /// File > Quit or Ctrl+Q: close the window, asking first.
    quit: bool,
}

impl SaveFirst {
    /// Bring in a file or a template. Asks whether to add it or open it as new,
    /// unless the graph is empty.
    fn load(&mut self, source: Source) {
        *self = SaveFirst {
            choosing: Some(source),
            ..Default::default()
        };
    }

    /// File > New: an empty graph in place of this one, asking to save first.
    fn load_new(&mut self) {
        *self = SaveFirst {
            after: Some(After::New),
            ..Default::default()
        };
    }

    /// A question is showing, or waits on the browser.
    pub fn busy(&self) -> bool {
        self.choosing.is_some() || self.after.is_some()
    }

    /// Hold `after` and ask, when there is something to lose. Returns whether
    /// it is held (false: carry it out now).
    fn ask(&mut self, history: &history::History, after: &After) -> bool {
        if !history.is_dirty() {
            return false;
        }
        self.choosing = None;
        self.after = Some(after.clone());
        self.saving_as = false;
        true
    }
}

/// Open a file or load a template in place of the graph.
fn carry_out(
    after: After,
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    nav: &mut GraphNavigation,
    file: &mut GraphFile,
    history: &mut history::History,
    recent: &mut recent::Recent,
) {
    match after {
        After::Quit => {}
        After::New => {
            graph_io::clear(graph);
            subnets.subnets.clear();
            graph.frame_request = true;
            history::note("New graph");
            history.mark_clean();
            file.path = None;
            file.message = "New graph".into();
        }
        After::Open(path) => open_graph(graph, subnets, &path, file, history, recent),
        After::Template(i) => {
            let t = &templates::TEMPLATES[i];
            file.message = t.load(graph, subnets);
            // A template is not a file: saving asks where.
            file.path = None;
            history::note(format!("Template: {}", t.name));
        }
    }
    nav.current_subnet = None;
    nav.materialx = None;
}

/// Dim the window behind a question, and take the clicks meant for it.
pub(crate) fn dim_behind(ctx: &egui::Context) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("quit_dim"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click_and_drag());
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(170));
        });
}

/// Add `what` to the graph (0), open it as new (1) or cancel (2).
fn ask_add_or_open(ctx: &egui::Context, what: &str) -> Option<u8> {
    dim_behind(ctx);
    let mut choice = None;
    egui::Window::new("Add or open")
        .order(egui::Order::Tooltip)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.label(format!("Add {what} to this graph, or open it as a new graph?"));
            ui.label(
                egui::RichText::new("Adding puts its nodes beside the ones here. Opening as new replaces the graph.")
                    .small()
                    .weak(),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Add").clicked() {
                    choice = Some(0);
                }
                if ui.button("Open as new").clicked() {
                    choice = Some(1);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(2);
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        choice = Some(2);
    }
    choice
}

/// Questions before losing changes, for one frame: closing the window
/// (`close_requested`), Quit, New, opening a file or a template. Returns true
/// when the program should close now.
pub(crate) fn save_first_frame(ctx: &egui::Context, app: &mut AppState, close_requested: bool) -> bool {
    let AppState {
        save_first,
        history,
        recent,
        graph_file,
        browser,
        graph,
        subnets,
        nav,
        downloads,
        ..
    } = app;
    // A graph brought in may name example files to fetch: ask again about
    // files declined for an earlier one.
    let before = graph.content_hash();
    let quit = save_first_step(
        ctx,
        save_first,
        history,
        recent,
        graph_file,
        browser,
        graph,
        subnets,
        nav,
        close_requested,
    );
    if graph.content_hash() != before {
        downloads.graph_opened();
    }
    quit
}

#[allow(clippy::too_many_arguments)]
fn save_first_step(
    ctx: &egui::Context,
    save_first: &mut SaveFirst,
    history: &mut history::History,
    recent: &mut recent::Recent,
    graph_file: &mut GraphFile,
    browser: &mut FileBrowser,
    graph: &mut NodeGraphState,
    subnets: &mut IceGraphStore,
    nav: &mut GraphNavigation,
    close_requested: bool,
) -> bool {
    if close_requested && !save_first.ask(&history, &After::Quit) {
        return true;
    }
    if std::mem::take(&mut save_first.quit) && !save_first.ask(&history, &After::Quit) {
        return true;
    }
    // File > New with nothing to lose: no question.
    if save_first.after == Some(After::New) && !save_first.saving_as && !history.is_dirty() {
        *save_first = SaveFirst::default();
        carry_out(
            After::New,
            &mut *graph,
            &mut *subnets,
            &mut *nav,
            &mut *graph_file,
            &mut *history,
            &mut *recent,
        );
        return false;
    }
    if let Some(source) = save_first.choosing.clone() {
        let empty = graph.nodes.is_empty();
        let choice = if empty {
            Some(1)
        } else {
            ask_add_or_open(ctx, &source.name())
        };
        match choice {
            Some(0) => {
                *save_first = SaveFirst::default();
                graph_file.message = match source {
                    Source::File(p) => add_graph_file(&mut *graph, &mut *subnets, &p, &mut *recent),
                    Source::Template(i) => add_template(&mut *graph, &mut *subnets, &templates::TEMPLATES[i]),
                };
            }
            Some(1) => {
                save_first.choosing = None;
                let after = source.open();
                if !save_first.ask(&history, &after) {
                    carry_out(
                        after,
                        &mut *graph,
                        &mut *subnets,
                        &mut *nav,
                        &mut *graph_file,
                        &mut *history,
                        &mut *recent,
                    );
                }
            }
            Some(_) => *save_first = SaveFirst::default(),
            None => {}
        }
        return false;
    }
    let Some(after) = save_first.after.clone() else {
        return false;
    };
    // Whether to go ahead with `after` this frame.
    let mut go = false;
    if save_first.saving_as {
        if !history.is_dirty() {
            go = true;
        }
        // The browser was closed without saving, or the save failed.
        else if !browser.is_open() && !browser.has_result() {
            *save_first = SaveFirst::default();
        }
    } else {
        dim_behind(ctx);
        let (what, warning) = (after.question(&graph_file), after.warning());
        let mut choice = None;
        egui::Window::new("Unsaved changes")
            .order(egui::Order::Tooltip)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(what);
                ui.label(egui::RichText::new(warning).small().weak());
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("💾 Save").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Don't save").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(2);
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(2);
        }
        match choice {
            Some(0) => {
                if save_or_ask(
                    &graph,
                    &subnets,
                    &mut *graph_file,
                    &mut *history,
                    &mut *recent,
                    &mut *browser,
                ) {
                    go = true;
                } else if browser.is_open() {
                    save_first.saving_as = true;
                } else {
                    // The save failed: its message is under the graph's buttons.
                    *save_first = SaveFirst::default();
                }
            }
            Some(1) => go = true,
            Some(2) => *save_first = SaveFirst::default(),
            _ => {}
        }
    }
    if go {
        *save_first = SaveFirst::default();
        if after == After::Quit {
            return true;
        }
        carry_out(
            after,
            &mut *graph,
            &mut *subnets,
            &mut *nav,
            &mut *graph_file,
            &mut *history,
            &mut *recent,
        );
    }
    false
}

// Systems

/// The Operator Stack, rebuilt when the graph's revision moved past `seen`.
pub(crate) fn refresh_operator_stack(app: &mut AppState, seen: &mut u64) {
    let AppState {
        graph, revision, stack, ..
    } = app;
    if std::mem::replace(seen, revision.0) != revision.0 {
        stack.rebuild(&graph.nodes, &graph.connections, graph.view_flag);
        if let Some(sel) = graph.selected_node {
            stack.selected_entry = Some(sel);
        }
    }
}

/// The Scene Explorer, rebuilt when the graph's revision moved past `seen`.
pub(crate) fn refresh_scene_hierarchy(app: &mut AppState, seen: &mut u64) {
    let AppState {
        graph,
        subnets,
        revision,
        hierarchy,
        ..
    } = app;
    if std::mem::replace(seen, revision.0) == revision.0 {
        return;
    }

    // Update the closure to accept template parameter
    let eval_subnet =
        |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| -> crate::core::geo::Geo {
            subnets
                .get(sid)
                .map(|sg| sg.evaluate(mesh, template))
                .unwrap_or_else(|| mesh.clone())
        };

    // The scene at the view flag, as its own tree.
    let _t = crate::timings::span("scene explorer: cook and list the scene");
    hierarchy.rebuild_scene(graph.evaluate_scene(&eval_subnet));
}

// Skeleton display
// Draws the clip of the viewed node (view flag, else Output) at the playhead.

/// Run a Write USD node: evaluate what enters it and write its edits.
fn write_usd(graph: &NodeGraphState, subnets: &IceGraphStore, id: types::NodeId) {
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else {
        return;
    };
    let NodeType::WriteUsd { path } = &node.node_type else {
        return;
    };
    // `$GDIR/out/$GNAME.usda`: its variables put in.
    let path = graph.expand_at(node.id, path.trim());
    let result = (|| {
        if path.is_empty() {
            return Err("No file set".to_string());
        }
        // Never boxes in place of geometry.
        if usd_scene::any_reading() {
            return Err("A stage is still being read: write when it is done".to_string());
        }
        let (src, out) = node
            .inputs
            .first()
            .and_then(|i| i.connected_output)
            .ok_or("Nothing connected")?;
        let eval = |sid: IceGraphId,
                    mesh: &crate::core::geo::Geo,
                    template: &[crate::ice::Reference]|
         -> crate::core::geo::Geo {
            subnets
                .get(sid)
                .map(|sg| sg.evaluate(mesh, template))
                .unwrap_or_else(|| mesh.clone())
        };
        match graph.eval_node_out(src, out, &mut std::collections::HashMap::new(), &eval) {
            Some(types::EvalResult::Named(prims)) => usd_write::write_override(&prims, std::path::Path::new(&path)),
            Some(types::EvalResult::Single(_)) => Err(
                "The input is one plain mesh, with no primitive path (after Unpack, or a Merge set to Flatten)"
                    .to_string(),
            ),
            Some(types::EvalResult::Material(_)) => {
                Err("The input is a material, not primitives: plug it into Material Binding".to_string())
            }
            Some(_) => Err("The input is a clip, not primitives".to_string()),
            None => Err("The input did not cook".to_string()),
        }
    })();
    usd_write::remember(id, result);
}

// Scene setup

#[cfg(test)]
mod tests {
    use super::*;

    /// Opening a file or loading a template over unsaved changes waits for the
    /// question; over a saved graph it goes ahead.
    #[test]
    fn replacing_an_unsaved_graph_asks_first() {
        let mut g = NodeGraphState::default();
        let mut h = history::History::default();
        h.update(&mut g, false);
        let mut ask = SaveFirst::default();
        let template = After::Template(0);
        assert!(!ask.ask(&h, &template), "nothing to lose: no question");
        assert!(ask.after.is_none());

        g.add_node("Cube".into(), NodeType::cube(1.0), egui::pos2(0.0, 0.0));
        h.update(&mut g, false);
        assert!(ask.ask(&h, &template));
        assert_eq!(ask.after, Some(template.clone()));
        let file = GraphFile {
            message: String::new(),
            path: Some("/tmp/scene.json".into()),
        };
        assert_eq!(
            template.question(&file),
            format!(
                "Save the changes to scene.json before loading the template {}?",
                templates::TEMPLATES[0].name
            )
        );
        assert_eq!(
            After::Open("/tmp/other.json".into()).question(&GraphFile::default()),
            "Save this graph before opening other.json?"
        );
        assert_eq!(
            After::Quit.question(&GraphFile::default()),
            "Save this graph before closing?"
        );

        assert_eq!(Source::Template(0).open(), After::Template(0));
        assert_eq!(Source::File("/tmp/a.json".into()).name(), "a.json");
        let mut picked = SaveFirst::default();
        picked.load(Source::Template(0));
        assert_eq!(
            picked.choosing,
            Some(Source::Template(0)),
            "picking a template asks add or open first"
        );

        // Don't save: the template replaces the graph.
        let (mut subnets, mut nav, mut file, mut recent) = (
            IceGraphStore::default(),
            GraphNavigation::default(),
            GraphFile::default(),
            recent::Recent::at(None),
        );
        carry_out(template, &mut g, &mut subnets, &mut nav, &mut file, &mut h, &mut recent);
        assert!(!g.nodes.iter().any(|n| n.name == "Cube"
            && matches!(n.node_type, NodeType::CreateCube { size, .. } if size == 1.0)
            && n.position == egui::pos2(0.0, 0.0)));
        assert!(file.path.is_none());
    }
}
