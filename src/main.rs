mod app;
mod batch;
mod colour;
mod compounds;
mod core;
mod curve_editor;
mod docs;
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
mod headless;
mod history;
mod ice;
mod icons;
/// The Image Viewer pane: renders and images.
mod image_gpu;
mod image_viewer;
mod inside_compound;
mod keys_ui;
mod layout;
mod light_linker;
mod live;
mod materialx;
mod menus;
mod modelling;
mod node_graph;
mod prim_inspector;
mod properties;
mod ragdoll;
mod ramp;
mod recent;
#[cfg(test)]
mod regress;
mod scene_explorer;
mod selection_bar;
mod settings;
mod strands;
mod templates;
mod theme;
mod timeline;
mod timings;
mod tools;
mod type_colours;
mod types;
mod ui_scale;
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
use layout::{Follows, Layout, Pane, Tab};
use node_graph::ui::draw_node_graph;
use node_graph::NodeGraphState;
use prim_inspector::ui::draw_prim_inspector;
use properties::ui::draw_properties_panel;
use properties::ui::{AnimContext, PanelAction, PanelIo};
use scene_explorer::ui::draw_scene_explorer;
use timeline::{resolve_source, ui::draw_timeline, Playback, TimelineState};
use types::NodeType;
use types::{IceGraphId, MeshData, PrimInspectorState, SceneHierarchy};

fn main() {
    #[cfg(target_os = "linux")]
    arnold_library_path();
    // `--headless`: a scene rendered without the window.
    let args: Vec<String> = std::env::args().collect();
    if let Some(job) = headless::job(&args) {
        let done = job.and_then(|j| headless::run(&j));
        live::end();
        if let Err(e) = done {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = eframe_app::run() {
        eprintln!("{e}");
    }
    live::end();
}

/// Linux: Arnold's plugins (its USD reader) find `libai.so` only on the
/// library path, as `kick` has it; Imago loads Arnold by its full path, so
/// they would not load ("unknown format"). With Arnold's folder known (File >
/// Settings > Renderers, else `$ARNOLD_PATH`) and its `bin` not on
/// `LD_LIBRARY_PATH`, Imago starts itself again with it there, once: the
/// library path is read only when a program starts.
#[cfg(target_os = "linux")]
fn arnold_library_path() {
    use std::os::unix::process::CommandExt;
    const AGAIN: &str = "XMS_IMAGO_ARNOLD_LIBRARY_PATH";
    if std::env::var_os(AGAIN).is_some() {
        return;
    }
    let Some(folder) = crate::core::renderers::folder_var("ARNOLD_PATH")
        .or_else(|| std::env::var("ARNOLD_PATH").ok())
        .filter(|f| !f.trim().is_empty())
    else {
        return;
    };
    let bin = std::path::PathBuf::from(folder.trim()).join("bin");
    let now = std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
    if !bin.join("libai.so").is_file() || std::env::split_paths(&now).any(|p| p == bin) {
        return;
    }
    let paths = std::iter::once(bin).chain(std::env::split_paths(&now));
    let (Ok(joined), Ok(me)) = (std::env::join_paths(paths), std::env::current_exe()) else {
        return;
    };
    let failed = std::process::Command::new(me)
        .args(std::env::args_os().skip(1))
        .env("LD_LIBRARY_PATH", joined)
        .env(AGAIN, "1")
        .exec();
    // Only when it could not start again: on without (Arnold's USD reader
    // will say why it does not load).
    eprintln!("Imago could not start again with Arnold's libraries on the library path: {failed}");
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
        inside,
        tabs,
        ..
    } = app;
    // Inside a locked compound its ICE graphs cannot be changed either.
    crate::ice::ui::set_scene_locked(ctx, graph.inside.locked);
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

    // What became of a movie being exported.
    if let Some(m) = viewer::movie::take_message() {
        graph_file.message = m;
    }

    // Path chosen in the file browser
    if let Some((target, path)) = browser.take_result() {
        let text = path.to_string_lossy().to_string();
        match target {
            BrowseTarget::Node(id) => {
                if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id) {
                    match &mut node.node_type {
                        // One frame of a sequence picked: the sequence ($F4).
                        NodeType::LoadImage { path, .. } | NodeType::LoadVdb { path, .. } => {
                            *path = crate::core::image_file::as_sequence(&text)
                        }
                        NodeType::LoadUsd { path, .. }
                        | NodeType::WriteImage { path, .. }
                        | NodeType::WriteUsd { path, .. }
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
            // Export Movie: made from the next frame, in the Viewer.
            BrowseTarget::ExportMovie => {
                viewer::movie::request(viewer::movie::with_extension(&path), viewer::movie::Settings::load());
            }
            // A compound's definition written as an .icd file.
            BrowseTarget::SaveDefinition(op) => {
                let path = if path.extension().is_some_and(|e| e == compounds::EXTENSION) {
                    path.clone()
                } else {
                    path.with_extension(compounds::EXTENSION)
                };
                graph_file.message = graph
                    .save_definition(op, &path)
                    .unwrap_or_else(|e| format!("Could not save the definition: {e}"));
            }
            BrowseTarget::SaveIceDefinition(sid, id) => {
                let path = if path.extension().is_some_and(|e| e == compounds::EXTENSION) {
                    path.clone()
                } else {
                    path.with_extension(compounds::EXTENSION)
                };
                graph_file.message = match subnets.get_mut(sid) {
                    Some(sg) => crate::ice::compound::save_definition(sg, id, &path)
                        .unwrap_or_else(|e| format!("Could not save the definition: {e}")),
                    None => "That ICE graph is gone".into(),
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
                // Which build this is, for bug reports: copied by a click.
                let build = version();
                if ui.add(egui::Label::new(egui::RichText::new(&build).size(10.0).color(theme::c(130, 130, 130))).sense(egui::Sense::click()))
                    .on_hover_text("This build of Imago: click to copy it, for a bug report")
                    .clicked()
                {
                    ui.ctx().copy_text(format!("XMS | Imago {build}"));
                }
                ui.add_space(10.0);
                top_bar_divider(ui);
                ui.add_space(4.0);
                // The menus: flat titles that light up under the mouse and
                // stay lit while open, as a menu bar does.
                ui.scope(|ui| {
                    menu_title_style(ui);
                    if let Some(c) = file_menu(ui, &recent, &graph_file) { command = Some(c); }
                    edit_menu(ui, &mut *graph, &mut *subnets, &mut *history, &mut *graph_file);
                    crate::menus::menu(ui, "Window", |ui| window_menu(ui, &mut *layout, &mut *browser, locked, &mut graph_file.message));
                    crate::menus::menu(ui, "Help", |ui| help_menu(ui, &mut graph_file.message));
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
    settings::window(ctx, &mut graph.scene, nav_settings);
    // What saves, replaces or closes the graph is done on the root scene
    // graph: out of any compound first.
    if command.as_ref().is_some_and(|c| {
        !matches!(
            c,
            FileCommand::ClearRecent | FileCommand::Variables | FileCommand::Settings(_)
        )
    }) && inside.is_inside()
    {
        if let Some(m) = inside_compound::leave_all(&mut *graph, &mut *subnets, &mut *history, &mut *nav, &mut *inside)
        {
            graph_file.message = m;
        }
    }
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
                &["usd", "usda", "usdc", "usdz", "abc"],
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
            FileCommand::Settings(page) => settings::open(ctx, page),
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

    // Side bars
    // The Tools down the left and the Selection bar down the right, when the
    // layout has them: fixed, as the top bar is, in the panes' grey. Not
    // tabs: they are not moved, sized or closed but from Window.
    let bar = egui::Frame::NONE
        .fill(root.visuals().panel_fill)
        .inner_margin(egui::Margin::symmetric(6, 6));
    if layout.bars.tools {
        egui::Panel::left("tools_bar")
            .exact_size(TOOLS_BAR)
            .resizable(false)
            .frame(bar)
            .show(root, |ui| fixed(ui, |ui| tools::draw(ui, &mut *graph)));
    }
    if layout.bars.selection {
        egui::Panel::right("selection_bar")
            .exact_size(SELECTION_BAR)
            .resizable(false)
            .frame(bar)
            .show(root, |ui| {
                fixed(ui, |ui| selection_bar::draw(ui, &mut *graph, &mut *hierarchy, &mut *poly_tool))
            });
    }

    let mut toggle_float = None;
    let mut dock_rect = ctx.content_rect();
    let mut tab_pressed = None;
    // Tabs asked for from a tab bar (its + or a tab's New Tab): added once the
    // dock area is drawn.
    let mut new_tabs: Vec<(Pane, Option<egui_dock::NodePath>)> = vec![];
    let open_tabs = layout.tabs();
    tabs.tidy(&open_tabs, graph, nav);
    {
        let mut panes = Panes {
            graph: &mut *graph,
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
            new_tabs: &mut new_tabs,
            open_tabs: &open_tabs,
            parameters_drawn: false,
            uv_state: &mut *uv_state,
            history: &mut *history,
            confirm: &mut *confirm,
            curve_editor: &mut *curve_editor,
            tabs: &mut *tabs,
            image_viewer: &mut *image_viewer,
        };
        let style = theme::dock_style(root.style());
        // File > New: the default layout, its tabs in front as saved.
        if layout::take_default_request() {
            layout.reset();
        }
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
                // The + of each tab bar: a tab of any pane beside the others.
                .show_add_buttons(!locked)
                .show_add_popup(!locked)
                .show_inside(ui, &mut panes);
        });
    }

    for (pane, at) in new_tabs {
        layout.add_tab(pane, at);
    }
    if let Some(tab) = toggle_float {
        layout.toggle_float_tab(tab);
    }
    // The Timeline's pane is as tall as its rows, no taller.
    if let Some(need) = timeline::ui::need(ctx) {
        let m = theme::dock_style(&ctx.global_style()).tab.tab_body.inner_margin;
        layout.fit_timeline(need, (m.top + m.bottom) as f32);
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
    if let Some(tab) = tab_pressed {
        if layout.tab_drag.is_none() {
            layout.tab_drag = Some((tab, false));
        }
    }
    if let Some((tab, moved)) = layout.tab_drag {
        let released = let_go || !down;
        if released {
            layout.tab_drag = None;
        } else if moving {
            layout.tab_drag = Some((tab, true));
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
                layout.dock_tab_to_edge(tab, edge);
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

    // The Viewer's menus, and the navigation keys (the style is File >
    // Settings > Viewer's).
    let Some(vp) = vp_rect.0 else { return };
    let style = nav_settings.style;
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
                        if ui.selectable_label(nav_help, "?").on_hover_text(format!("Show the navigation keys ({}; File > Settings > Viewer chooses)", style.label())).clicked() {
                            nav_help = !nav_help;
                        }
                        let shown = viewport::display::mode();
                        crate::menus::menu(ui, format!("{} {}", shown.label(), crate::icons::CARET_DOWN), |ui| {
                            for m in viewport::display::DisplayMode::ALL {
                                if ui.radio(shown == m, m.label()).clicked() { viewport::display::set_mode(m); ui.close(); }
                            }
                            ui.separator();
                            let mut wire = viewport::display::wire_over();
                            if ui.checkbox(&mut wire, "Wireframe on Shaded").on_hover_text("The polygons' edges over the surfaces, in Shaded and Textured").changed() { viewport::display::set_wire_over(wire); }
                            let mut points = viewport::display::points();
                            if ui.checkbox(&mut points, "Points").on_hover_text("Every point as a dot, hidden behind what is in front").changed() { viewport::display::set_points(points); }
                        }).response.on_hover_text("How meshes are drawn");
                        // Through a camera of the scene, or the Viewer's own.
                        let looking = viewer::camera::looking_through();
                        let name = looking.as_deref().map(|p| p.rsplit('/').next().unwrap_or(p).to_string()).unwrap_or_else(|| "Perspective".into());
                        crate::menus::menu(ui, format!("{} {name} {}", crate::icons::VIDEO_CAMERA, crate::icons::CARET_DOWN), |ui| {
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
                        // Export Movie: the timeline's range as an MP4, through
                        // the camera looked through.
                        match viewer::movie::progress() {
                            Some((done, all)) => {
                                ui.label(egui::RichText::new(format!("Movie {done}/{all}")).small());
                                if ui.button("Stop").on_hover_text("Stop the movie: the frames so far are kept").clicked() {
                                    viewer::movie::stop();
                                }
                            }
                            None => {
                                let mut m = viewer::movie::Settings::load();
                                let before = m;
                                crate::menus::menu(ui, format!("Movie {}", crate::icons::CARET_DOWN), |ui| {
                                    ui.weak("Codec");
                                    for c in viewer::movie::Codec::ALL {
                                        ui.radio_value(&mut m.codec, c, c.label());
                                    }
                                    ui.separator();
                                    ui.weak("Size");
                                    for z in viewer::movie::Size::ALL {
                                        ui.radio_value(&mut m.size, z, z.label());
                                    }
                                    ui.separator();
                                    ui.weak("Quality");
                                    for q in viewer::movie::Quality::ALL {
                                        ui.radio_value(&mut m.quality, q, q.label());
                                    }
                                    ui.separator();
                                    if ui.button("Export Movie…").clicked() {
                                        browser.open(BrowseTarget::ExportMovie, BrowseMode::Save, "Export movie", &["mp4"], "movie.mp4");
                                        ui.close();
                                    }
                                }).response.on_hover_text("Export Movie: the timeline's frames as seen in the Viewer, as an MP4 (H.264 or H.265), by ffmpeg");
                                if m != before {
                                    m.save();
                                }
                            }
                        }
                        let loading = viewport::textures::pending();
                        if loading > 0 {
                            ui.label(egui::RichText::new(format!("loading {loading}")).small());
                            ui.ctx().request_repaint();
                        }
                    });
                });
        });
    // Top right: drawn, or rendered live with Arnold through the view.
    egui::Area::new("viewport_renderer_menu".into())
        .order(egui::Order::Background)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(egui::pos2(vp.max.x - 10.0, vp.min.y + 8.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme::c(90, 90, 90).gamma_multiply(0.92))
                .stroke(egui::Stroke::new(1.0_f32, theme::c(70, 70, 70)))
                .corner_radius(4.0)
                .inner_margin(3.0)
                .show(ui, |ui| {
                    // One size whichever is chosen: the area keeps last
                    // frame's width, and a narrower label would wrap.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.set_min_width(96.0);
                    let rendered = crate::live::in_viewer();
                    crate::menus::menu(ui, format!("{} {}", if rendered { "Arnold" } else { "Viewport" }, crate::icons::CARET_DOWN), |ui| {
                        if ui.radio(!rendered, "Viewport").on_hover_text("The scene drawn by Imago's viewport (wgpu: Metal on macOS, Vulkan or DirectX 12 elsewhere), as you work").clicked() { crate::live::set_in_viewer(false); ui.close(); }
                        let why = crate::live::why_not("arnold");
                        if ui.add_enabled(why.is_none(), egui::RadioButton::new(rendered, "Arnold")).on_hover_text("Rendered live (IPR) through the Viewer's camera, each change and move sent to Arnold").on_disabled_hover_text(format!("Arnold does not render in Imago here: {}", why.unwrap_or_default())).clicked() { crate::live::set_in_viewer(true); ui.close(); }
                    }).response.on_hover_text("Draw the scene, or render it with Arnold");
                });
        });

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
                        let play = if style == viewport::nav::NavStyle::Houdini {
                            "Tap Space: play  |  Left/Right: step"
                        } else {
                            "Space: play  |  Left/Right: step"
                        };
                        let nav_lines = style.help();
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

/// A side bar's contents in its rectangle and no wider: what does not fit is
/// cut off, and the bar keeps its width whatever it shows.
fn fixed(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let rect = ui.max_rect();
    let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
    inner.set_clip_rect(rect.intersect(ui.clip_rect()));
    add(&mut inner);
}

/// The widths of the side bars: the Tools, the Selection bar.
const TOOLS_BAR: f32 = 142.0;
const SELECTION_BAR: f32 = 200.0;

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
    /// The tab double-clicked this frame.
    toggle_float: &'a mut Option<Tab>,
    /// The tab the mouse button went down on this frame.
    tab_pressed: &'a mut Option<Tab>,
    /// Tabs asked for this frame: a pane, beside the tabs of a tab bar.
    new_tabs: &'a mut Vec<(Pane, Option<egui_dock::NodePath>)>,
    /// The tabs open as the frame began.
    open_tabs: &'a [Tab],
    /// A Parameters tab drawn this frame: what is drawn over every pane (a
    /// row's menu, Promote Group) only by the first.
    parameters_drawn: bool,
    uv_state: &'a mut uv_editor::UvEditorState,
    curve_editor: &'a mut curve_editor::CurveEditor,
    /// What tabs beyond the first of their pane keep of their own.
    tabs: &'a mut layout::TabStates,
    image_viewer: &'a mut image_viewer::ImageViewerState,
    history: &'a mut history::History,
    confirm: &'a mut Confirm,
    /// The Viewer's 3D view, when the interface draws it (eframe).
    viewer: Option<&'a mut viewer::Viewer>,
    nav_settings: viewport::nav::NavSettings,
}

/// A tab following something other than the selection, drawn: the operator
/// it shows put as the selection, and the selection as it was (`unfollow`).
struct Followed {
    own: bool,
    shown: Option<types::NodeId>,
    kept: (Option<types::NodeId>, Vec<types::NodeId>),
}

impl Panes<'_> {
    /// Before drawing a tab that follows (`layout::Follows`): the operator it
    /// shows put as the selection, while it is drawn. `None`, said in the tab:
    /// nothing to show (no display flag, a pin gone or in another compound).
    fn follow(&mut self, ui: &mut egui::Ui, tab: &mut Tab) -> Option<Followed> {
        let level = layout::level_of(&self.graph.inside.path);
        let follows = tab.follows().unwrap_or(Follows::Selection);
        let own = follows == Follows::Selection;
        let shown = shown_by(self.graph, follows, level);
        if !own && shown.is_none() {
            ui.add_space(8.0);
            ui.label(match follows {
                Follows::Display => "No operator has the display flag in this graph.",
                _ => "The operator pinned is gone, or in another compound.",
            });
            if ui.button("Follow the selection").clicked() {
                tab.follows = Some(Follows::Selection);
            }
            return None;
        }
        let kept = (self.graph.selected_node, self.graph.selected_nodes.clone());
        if !own {
            self.graph.selected_node = shown;
            self.graph.selected_nodes = shown.into_iter().collect();
        }
        Some(Followed { own, shown, kept })
    }

    /// After: the selection back as it was, unless the tab chose another.
    fn unfollow(&mut self, f: Followed) {
        if !f.own && self.graph.selected_node == f.shown {
            (self.graph.selected_node, self.graph.selected_nodes) = f.kept;
        }
    }

    /// For a Scene Explorer or Primitive Inspector tab: the operator whose
    /// scene it shows, `None` for the one at the display flag (the program's
    /// own, which the Viewer shows too). `Err`, said in the tab (with a way
    /// back to the display flag): nothing to show.
    fn scene_of_tab(&self, ui: &mut egui::Ui, tab: &mut Tab) -> Result<Option<types::NodeId>, ()> {
        let follows = tab.follows().unwrap_or(Follows::Display);
        if follows == Follows::Display {
            return Ok(None);
        }
        let level = layout::level_of(&self.graph.inside.path);
        match shown_by(self.graph, follows, level) {
            Some(id) if Some(id) == self.graph.view_flag => Ok(None),
            Some(id) => Ok(Some(id)),
            None => {
                ui.add_space(8.0);
                ui.label(match follows {
                    Follows::Selection => "No operator is selected.",
                    _ => "The operator pinned is gone, or in another compound.",
                });
                if ui.button("Follow the display flag").clicked() {
                    tab.follows = Some(Follows::Display);
                }
                Err(())
            }
        }
    }

    /// Whether `tab` is the first open of its pane: it uses the program's
    /// state, the others their own (`TabStates`).
    fn first_of(&self, tab: &Tab) -> bool {
        self.open_tabs
            .iter()
            .find(|t| t.pane == tab.pane)
            .is_none_or(|t| t.id == tab.id)
    }

    /// The Parameters down the Node Graph's right (P over it): its first
    /// width, and the narrowest it goes.
    const PARAMETERS_OVER_WIDTH: f32 = 380.0;
    const PARAMETERS_OVER_MIN: f32 = 220.0;
    /// The shortest they go.
    const PARAMETERS_OVER_LOW: f32 = 120.0;

    /// A Node Graph tab: drawn with its own view (`GraphView`) and graph
    /// (ICE, MaterialX) swapped in, unless it is the one last used, whose are
    /// the program's. Pressed in, it becomes that one.
    fn node_graph_tab(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        let active = *self.tabs.active_graph.get_or_insert(tab.id);
        if active == tab.id {
            self.toggle_parameters_over(ui);
            self.draw_tab(ui, tab);
            return;
        }
        let rect = ui.max_rect();
        let pressed =
            ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p)));
        let (mut view, mut at) = self
            .tabs
            .graphs
            .remove(&tab.id)
            .unwrap_or_else(|| (self.graph.view(), self.nav.clone()));
        self.graph.swap_view(&mut view);
        std::mem::swap(&mut *self.nav, &mut at);
        self.draw_tab(ui, tab);
        if pressed {
            // Its view stays; the last one's is kept for its tab.
            self.tabs.active_graph = Some(tab.id);
            self.tabs.graphs.insert(active, (view, at));
        } else {
            self.graph.swap_view(&mut view);
            std::mem::swap(&mut *self.nav, &mut at);
            self.tabs.graphs.insert(tab.id, (view, at));
        }
    }

    /// P over the Node Graph in use: its Parameters down its right side, or
    /// not.
    fn toggle_parameters_over(&mut self, ui: &egui::Ui) {
        let rect = ui.max_rect();
        let over = ui.input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p)));
        let p = ui.input(|i| i.key_pressed(egui::Key::P) && i.modifiers.is_none());
        if over && p && self.keys_free {
            self.tabs.parameters_over = !self.tabs.parameters_over;
        }
    }

    /// Where the Node Graph tab `id`'s Parameters go (P, the tab in use):
    /// over its canvas, at the top right, as wide and as tall as they were
    /// sized, within the canvas.
    fn parameters_rect(&self, id: u32, canvas: egui::Rect) -> Option<egui::Rect> {
        if !self.tabs.parameters_over || self.tabs.active_graph != Some(id) {
            return None;
        }
        let fit = |v: f32, first: f32, least: f32, most: f32| {
            let v = if v > 0.0 { v } else { first };
            v.clamp(least.min(most), most)
        };
        let w = fit(
            self.tabs.parameters_width,
            Self::PARAMETERS_OVER_WIDTH,
            Self::PARAMETERS_OVER_MIN,
            canvas.width() - 40.0,
        );
        let h = fit(
            self.tabs.parameters_height,
            canvas.height(),
            Self::PARAMETERS_OVER_LOW,
            canvas.height(),
        );
        Some(egui::Rect::from_min_size(
            egui::pos2(canvas.right() - w, canvas.top()),
            egui::vec2(w, h),
        ))
    }

    /// The Node Graph's Parameters (P over it), as Houdini's network editor
    /// has them: over the graph at its top right, in a thin border, no
    /// title; the selection's, as a Parameters pane draws them. Their left
    /// edge, bottom edge and bottom left corner drag to size them. On a layer
    /// of their own, so the graph under them takes no clicks or wheel.
    fn parameters_over(&mut self, ui: &egui::Ui, tab: u32, side: egui::Rect) {
        let visuals = ui.visuals().clone();
        let id = egui::Id::new(("parameters_over", tab));
        egui::Area::new(id)
            .order(egui::Order::Middle)
            .fixed_pos(side.min)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                ui.set_clip_rect(side);
                // The whole rectangle is theirs: the graph under it is not
                // reached.
                ui.allocate_rect(side, egui::Sense::click());
                ui.painter().rect_filled(side, 0.0, visuals.panel_fill);
                let mut inner = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(id.with("inner"))
                        .max_rect(side.shrink2(egui::vec2(8.0, 6.0)))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                inner.set_clip_rect(side.shrink(1.0));
                // A Parameters tab of its own, following the selection.
                let mut shown = Tab {
                    id: u32::MAX,
                    pane: Pane::Parameters,
                    follows: None,
                };
                self.draw_tab(&mut inner, &mut shown);
                // The edges that size them, over what they hold.
                let grip = 4.0;
                let left = egui::Rect::from_min_max(
                    side.left_top() - egui::vec2(grip, 0.0),
                    side.left_bottom() + egui::vec2(grip, -grip),
                );
                let bottom = egui::Rect::from_min_max(
                    side.left_bottom() + egui::vec2(grip, -grip),
                    side.right_bottom() + egui::vec2(0.0, grip),
                );
                let corner = egui::Rect::from_center_size(side.left_bottom(), egui::vec2(3.0 * grip, 3.0 * grip));
                let mut lit = false;
                for (rect, x, y, cursor) in [
                    (left, true, false, egui::CursorIcon::ResizeHorizontal),
                    (bottom, false, true, egui::CursorIcon::ResizeVertical),
                    (corner, true, true, egui::CursorIcon::ResizeSouthWest),
                ] {
                    let r = ui.interact(rect, id.with((x, y)), egui::Sense::drag());
                    if r.hovered() || r.dragged() {
                        ui.ctx().set_cursor_icon(cursor);
                        lit = true;
                    }
                    if r.dragged() {
                        let d = r.drag_delta();
                        if x {
                            self.tabs.parameters_width = side.width() - d.x;
                        }
                        if y {
                            self.tabs.parameters_height = side.height() + d.y;
                        }
                    }
                }
                let stroke = if lit {
                    visuals.widgets.hovered.bg_stroke
                } else {
                    visuals.widgets.noninteractive.bg_stroke
                };
                ui.painter().rect_stroke(side, 0.0, stroke, egui::StrokeKind::Inside);
            });
    }

    /// On a tab that follows (`Pane::follows`), at its right before its close
    /// cross: the letter of what it follows (`follows_letter`) in a thin
    /// circle, a menu of the choices (`follow_choices`). Drawn over the tab,
    /// just above its layer, so a window over the tab bar still covers it.
    fn follows_mark(&mut self, tab: &mut Tab, response: &egui::Response) {
        let Some(now) = tab.follows() else {
            return;
        };
        let ctx = response.ctx.clone();
        let rect = response.rect;
        let r = (rect.height() * 0.32).clamp(5.0, 9.0);
        let right = if self.locked {
            rect.right() - 4.0
        } else {
            rect.right() - rect.height()
        };
        let centre = egui::pos2(right - r - 3.0, rect.center().y);
        let level = layout::level_of(&self.graph.inside.path);
        let graph = &*self.graph;
        let shown = egui::Area::new(egui::Id::new(("follows_mark", tab.id)))
            .order(response.layer_id.order)
            .fixed_pos(centre - egui::vec2(r + 1.0, r + 1.0))
            .constrain(false)
            .show(&ctx, |ui| {
                // A button of the letter alone: no frame, the circle drawn.
                ui.spacing_mut().button_padding = egui::Vec2::ZERO;
                ui.spacing_mut().interact_size = egui::vec2(2.0 * r + 2.0, 2.0 * r + 2.0);
                let v = ui.visuals_mut();
                for w in [
                    &mut v.widgets.inactive,
                    &mut v.widgets.hovered,
                    &mut v.widgets.active,
                    &mut v.widgets.open,
                ] {
                    w.weak_bg_fill = egui::Color32::TRANSPARENT;
                    w.bg_fill = egui::Color32::TRANSPARENT;
                    w.bg_stroke = egui::Stroke::NONE;
                }
                let dim = ui.visuals().widgets.inactive.fg_stroke.color;
                let lit = ui.visuals().widgets.hovered.fg_stroke.color;
                let letter = egui::RichText::new(follows_letter(now)).size(r * 1.3).color(dim);
                let mut chosen = None;
                let menu = ui.menu_button(letter, |ui| {
                    chosen = follow_choices(ui, graph, now, level);
                    if chosen.is_some() {
                        ui.close();
                    }
                });
                let b = &menu.response;
                let ink = if b.hovered() { lit } else { dim };
                ui.painter()
                    .circle_stroke(b.rect.center(), r, egui::Stroke::new(1.0, ink));
                b.clone().on_hover_text(follows_hint(now));
                chosen
            });
        ctx.set_sublayer(response.layer_id, shown.response.layer_id);
        if let Some(f) = shown.inner {
            tab.follows = Some(f);
        }
    }

    /// The panes a tab can be added of, beside the tabs at `path` (a tab
    /// bar): a click asks for one (`new_tabs`). One of a kind open is greyed.
    fn pane_list(&mut self, ui: &mut egui::Ui, path: egui_dock::NodePath) {
        for p in Pane::ALL.into_iter().filter(|p| !p.is_bar()) {
            let free = layout::may_add(self.open_tabs, p);
            let r = ui
                .add_enabled(free, egui::Button::new(p.title()))
                .on_disabled_hover_text("Open in another tab: there is one of it");
            if r.clicked() {
                self.new_tabs.push((p, Some(path)));
                ui.close();
            }
        }
    }
}

impl egui_dock::TabViewer for Panes<'_> {
    type Tab = Tab;

    // By its number: two tabs of one pane are two, each with the state it
    // keeps (scroll positions).
    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(("pane", tab.id))
    }
    // A pinned tab names its operator: "Parameters · Transform1". A tab that
    // follows has room at its end for the letter of what it follows, drawn
    // there (`follows_mark`).
    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        let title = tab.pane.title();
        match tab.follows() {
            Some(f @ Follows::Pinned { node, .. }) => {
                let level = layout::level_of(&self.graph.inside.path);
                let name = shown_by(self.graph, f, level)
                    .and_then(|_| self.graph.nodes.iter().find(|n| n.id == node))
                    .map_or("elsewhere".to_string(), |n| n.name.clone());
                format!("{title} · {name}{MARK_ROOM}").into()
            }
            Some(_) => format!("{title}{MARK_ROOM}").into(),
            None => title.into(),
        }
    }
    // Closing takes the tab away. The Panes menu, or a tab bar's +, opens one.
    fn is_closeable(&self, _tab: &Tab) -> bool {
        !self.locked
    }
    // Right-click a tab: Pane, what it shows instead; New Tab, another beside
    // it. A pane of one of a kind (`Pane::multiple`) open elsewhere is greyed.
    fn context_menu(&mut self, ui: &mut egui::Ui, tab: &mut Tab, path: egui_dock::NodePath) {
        let now = *tab;
        ui.menu_button("Pane", |ui| {
            for p in Pane::ALL.into_iter().filter(|p| !p.is_bar()) {
                let free = layout::may_become(self.open_tabs, now, p);
                let r = ui
                    .add_enabled_ui(free, |ui| ui.selectable_label(p == now.pane, p.title()))
                    .inner
                    .on_disabled_hover_text("Open in another tab: there is one of it");
                if r.clicked() && p != now.pane {
                    tab.pane = p;
                    ui.close();
                }
            }
        });
        ui.menu_button("New Tab", |ui| self.pane_list(ui, path));
        ui.separator();
    }
    // A tab bar's +: a tab of any pane that can be added, beside the others.
    fn add_popup(&mut self, ui: &mut egui::Ui, path: egui_dock::NodePath) {
        self.pane_list(ui, path);
    }
    // Double-click a tab to float it, or to dock it back.
    fn on_tab_button(&mut self, tab: &mut Tab, response: &egui::Response) {
        self.follows_mark(tab, response);
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
    fn scroll_bars(&self, _tab: &Tab) -> [bool; 2] {
        [false, false]
    }
    // The 3D view is drawn by the camera behind the interface.
    fn clear_background(&self, tab: &Tab) -> bool {
        tab.pane != Pane::Viewer || self.viewer.is_some()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        if tab.pane == Pane::NodeGraph {
            self.node_graph_tab(ui, tab);
        } else {
            self.draw_tab(ui, tab);
        }
    }
}

impl Panes<'_> {
    /// A tab's pane, drawn.
    fn draw_tab(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab.pane {
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
                // What it shows (`layout::Follows`): the operator selected,
                // the one with the display flag, or the one it is pinned to,
                // put as the selection while it is drawn, at the scene's level
                // (not inside an ICE or MaterialX graph open).
                let Some(followed) = self.follow(ui, tab) else {
                    return;
                };
                let own = followed.own;
                let plain = GraphNavigation::default();
                let nav: &GraphNavigation = if own { &*self.nav } else { &plain };
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
                        ice_params: vec![],
                        ice_settings: vec![],
                        snippet_here: false,
                    };
                    // A keyed node shows its parameters at the playhead; a
                    // keyed number changed here is keyed there.
                    let editing = self
                        .graph
                        .selected_node
                        .and_then(|id| self.graph.begin_edit(id).map(|own| (id, own)));
                    // A number's row right-clicked shows its menu, and the rows
                    // of numbers that follow others are found to draw them
                    // orange: only on an operator's own parameters, not an ICE
                    // or MaterialX node's.
                    let own_parameters = nav.current_subnet.is_none() && nav.materialx.is_none();
                    let selected = editing
                        .as_ref()
                        .and_then(|(id, _)| self.graph.nodes.iter().find(|n| n.id == *id));
                    // Its numbers that follow another, and those promoted (inside
                    // a compound): each follows a number elsewhere.
                    let (whose, linked): (u64, Vec<String>) = selected
                        .map(|n| {
                            use std::hash::{Hash, Hasher};
                            let promoted =
                                self.graph.inside.parameters.iter().flat_map(|q| {
                                    q.targets.iter().filter(|t| t.node == n.name).map(|t| t.path.clone())
                                });
                            let linked: Vec<String> = n.links.iter().map(|l| l.path.clone()).chain(promoted).collect();
                            let mut h = std::collections::hash_map::DefaultHasher::new();
                            (n.id, &linked).hash(&mut h);
                            (h.finish(), linked)
                        })
                        .unwrap_or_default();
                    // The parameters as they were, for a probed row to be found
                    // by what it changed, and put back.
                    let before = selected
                        .filter(|_| {
                            !linked.is_empty()
                                || ui.input(|i| i.pointer.secondary_clicked())
                                || crate::properties::form::seeking()
                        })
                        .cloned();
                    // An ICE graph's nodes are probed by the ICE panel, rows of its own.
                    if own_parameters {
                        crate::properties::form::probing(editing.is_some(), whose, linked);
                    } else {
                        crate::properties::form::probed();
                    }
                    // Each operator's fields are its own: a value being typed
                    // when another operator is selected is not put in its field.
                    ui.push_id(("parameters_of", self.graph.selected_node.map(|n| n.0)), |ui| {
                        draw_properties_panel(ui, self.graph, self.subnets, nav, self.anim_ctx, &mut io)
                    });
                    // An ICE operator's parameters, for those it keeps only once set.
                    let ice_settings = io.ice_settings.clone();
                    use crate::properties::form::Probe;
                    match (crate::properties::form::probed(), &editing) {
                        (Some(Probe::Clicked(old, at)), Some((id, _))) => {
                            if let Some((path, value)) = crate::keys_ui::found_row(self.graph, *id, old, before) {
                                // A text row's value is its text.
                                let value = match crate::properties::form::probed_text() {
                                    Some(t) => serde_json::json!(t),
                                    None => serde_json::json!(value),
                                };
                                let (value, choices) = crate::keys_ui::kept_default(
                                    &ice_settings,
                                    &path,
                                    value,
                                    crate::properties::form::row_choices(None),
                                );
                                crate::keys_ui::open_row_menu(ui.ctx(), *id, path, value, choices, at);
                            }
                        }
                        (Some(Probe::Finding(row)), Some((id, _))) => {
                            let path = crate::keys_ui::found_row(self.graph, *id, 0.0, before).map(|f| f.0);
                            crate::properties::form::found(row, path);
                        }
                        _ => {}
                    }
                    // What is drawn over every pane, by the first Parameters
                    // tab of the frame alone.
                    let first = !self.parameters_drawn;
                    self.parameters_drawn = true;
                    // A group's title right-clicked: Promote Group.
                    if own_parameters && first {
                        let id = editing.as_ref().map(|(id, _)| *id);
                        if let Some(m) = crate::keys_ui::group_menu(ui.ctx(), self.graph, id, &ice_settings) {
                            self.graph_file.message = m;
                        }
                    }
                    if let Some((id, own)) = editing {
                        self.graph.end_edit(id, own);
                    }
                    if let Some(id) = self.graph.selected_node {
                        if let Some(m) = crate::keys_ui::channel_box(ui, self.graph, id) {
                            self.graph_file.message = m;
                        }
                    }
                    // A number's menu from its row, and the Promote window.
                    if first {
                        if let Some(m) = crate::keys_ui::row_menu(ui.ctx(), self.graph) {
                            self.graph_file.message = m;
                        }
                    }
                    // Operators the panel asked for: a MaterialX operator from a stage's material.
                    for (name, t) in std::mem::take(&mut io.add) {
                        import_node(self.graph, name, t);
                    }
                    // Writing over files that exist asks first.
                    if let Some(PanelAction::SceneRange(r)) = io.action {
                        // The range and rate; the axes and unit stay the scene's.
                        self.graph.scene = crate::node_graph::SceneRange {
                            up_axis: self.graph.scene.up_axis,
                            meters_per_unit: self.graph.scene.meters_per_unit,
                            ..r
                        };
                        self.graph_file.message = format!(
                            "Scene frames {} to {} at {} fps, from the stage",
                            r.start,
                            r.end,
                            r.fps()
                        );
                    } else if let Some(action) = io.action {
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
                self.unfollow(followed);
            }

            Pane::SceneExplorer => {
                // What it lists (`layout::Follows`): the scene at the display
                // flag, the program's; another operator's, its own rows,
                // listed again as the graph changes, swapped in while it is
                // drawn. The selection, rows opened and draw modes are the
                // program's, by path, whichever it lists.
                let Ok(shown) = self.scene_of_tab(ui, tab) else {
                    return;
                };
                let mut own = shown.map(|id| {
                    let key = (self.revision, id);
                    match self.tabs.explorers.remove(&tab.id) {
                        Some((k, rows)) if k == key => (key, rows),
                        _ => {
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
                            let _t = crate::timings::span("scene explorer: list another operator's scene");
                            (key, self.hierarchy.rows_of(self.graph.evaluate_scene_at(id, &eval)))
                        }
                    }
                });
                if let Some((_, rows)) = own.as_mut() {
                    self.hierarchy.swap_rows(rows);
                }
                egui::ScrollArea::both()
                    .id_salt(("scene_explorer_scroll", tab.id))
                    .auto_shrink([false, false])
                    .show(ui, |ui| draw_scene_explorer(ui, self.hierarchy));
                if let Some((key, mut rows)) = own {
                    self.hierarchy.swap_rows(&mut rows);
                    self.tabs.explorers.insert(tab.id, (key, rows));
                }
                // Enter or Edit Selected: an Edit Primitive on the selection.
                if let Some(paths) = self.hierarchy.edit_request.take() {
                    self.graph.edit_primitives(&paths);
                }
                // Variant Select on a row's right-click.
                if let Some(choice) = self.hierarchy.variant_request.take() {
                    self.graph.choose_variant(choice);
                }
                // Load or Unload Payload on a row's right-click: its Load USD
                // keeps it.
                if let Some((paths, load)) = self.hierarchy.payload_request.take() {
                    set_payloads(self.graph, self.subnets, shown, &paths, load);
                }
            }

            Pane::CurveEditor => {
                let Some(followed) = self.follow(ui, tab) else {
                    return;
                };
                let ed = if self.first_of(tab) {
                    &mut *self.curve_editor
                } else {
                    self.tabs.curves.entry(tab.id).or_default()
                };
                curve_editor::draw(ui, ed, self.graph, self.playback);
                self.unfollow(followed);
            }

            Pane::History => {
                history::draw(ui, self.history, self.graph.selected_node);
            }

            Pane::Tools => tools::draw(ui, self.graph),

            Pane::Selection => selection_bar::draw(ui, self.graph, self.hierarchy, self.poly_tool),

            Pane::LightLinker => {
                let selected = self.hierarchy.selected_prim_paths();
                light_linker::draw(ui, self.graph, &selected);
            }

            Pane::ImageViewer => {
                image_viewer::draw(ui, self.image_viewer, self.graph, &*self.subnets, self.revision);
            }

            // A pane no longer: its tabs are taken away as a layout loads.
            Pane::Retired => {}

            Pane::UvEditor => {
                let Some(followed) = self.follow(ui, tab) else {
                    return;
                };
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
                let state = if self.first_of(tab) {
                    &mut *self.uv_state
                } else {
                    self.tabs.uvs.entry(tab.id).or_default()
                };
                uv_editor::draw_uv_editor(ui, self.graph, state, self.revision, &get_mesh);
                self.unfollow(followed);
            }

            Pane::PrimInspector => {
                // The scene at the view flag, or of the operator it follows
                // (`layout::Follows`): the rows selected in the Scene Explorer
                // and what is below them, or all of it.
                let Ok(shown) = self.scene_of_tab(ui, tab) else {
                    return;
                };
                let scene_of = |g: &NodeGraphState,
                                eval: &dyn Fn(
                    IceGraphId,
                    &crate::core::geo::Geo,
                    &[crate::ice::Reference],
                ) -> crate::core::geo::Geo| match shown {
                    None => g.evaluate_scene(&eval),
                    Some(id) => g.evaluate_scene_at(id, &eval),
                };
                let subnets = &*self.subnets;
                let paths = self.hierarchy.selected_prim_paths();
                let get_parts = |g: &NodeGraphState| -> Option<Vec<prim_inspector::Part>> {
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
                    let (_, _, scene) = scene_of(g, &eval)?;
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
                    scene_of(g, &eval)
                        .map(|(_, _, scene)| prim_inspector::scene_summary(&scene, &paths))
                        .unwrap_or_default()
                };
                let get_clip = |g: &NodeGraphState| {
                    let id = shown.or(g.display_source())?;
                    let output = if g.view_flag == Some(id) { g.viewed_output() } else { 0 };
                    g.eval_anim_out(id, output)
                };
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
                    match scene_of(g, &eval) {
                        Some((view, _, scene)) => prim_inspector::scene_properties(g, view, &scene, &paths),
                        None => vec![],
                    }
                };
                let source = prim_inspector::Source {
                    node: shown.or(self.graph.display_source()),
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
                // Enter or Edit Selected on a Properties row: an Edit Primitive on its prim.
                if let Some(paths) = self.prim_state.edit_request.take() {
                    self.graph.edit_primitives(&paths);
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
                        let (root, told) = crate::ice::ui::draw_ice_bar(ui, sg);
                        if let Some(m) = told {
                            self.graph_file.message = m;
                        }
                        if root {
                            self.nav.current_subnet = None;
                        } else {
                            ui.label("Right-click: add  |  Del: delete  |  Shift/middle-drag: pan  |  Wheel: zoom  |  F: frame  |  ↓: inside a compound  |  ↑: back out  |  Esc: cancel wire");
                            ui.separator();
                            // Canvas in its own child Ui: see the note below.
                            let rect = ui.available_rect_before_wrap();
                            let side = self.parameters_rect(tab.id, rect);
                            let Some(sg) = self.subnets.get_mut(sid) else { return };
                            let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
                            canvas.set_clip_rect(rect.intersect(ui.clip_rect()));
                            let (up, told) = draw_subnet_graph(&mut canvas, sg);
                            if let Some(m) = told {
                                self.graph_file.message = m;
                            }
                            ui.allocate_rect(rect, egui::Sense::hover());
                            if up {
                                self.nav.current_subnet = None;
                            }
                            if let Some(side) = side {
                                self.parameters_over(ui, tab.id, side);
                            }
                        }
                    } else {
                        self.nav.current_subnet = None;
                    }
                }
                None => {
                    // Inside a compound: the way back, and whether it is
                    // locked.
                    if !self.graph.inside.path.is_empty() {
                        compound_bar(ui, self.graph);
                    }
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
                    let side = self.parameters_rect(tab.id, rect);
                    let mut canvas = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
                    canvas.set_clip_rect(rect.intersect(ui.clip_rect()));
                    let dive = draw_node_graph(&mut canvas, self.graph);
                    ui.allocate_rect(rect, egui::Sense::hover());
                    if let Some(side) = side {
                        self.parameters_over(ui, tab.id, side);
                    }
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
                            } else if *id == ice::NEW_SNIPPET {
                                *id = self.subnets.create_snippet(name.clone(), ice::SNIPPET);
                            }
                        }
                    }
                    match dive {
                        Some(node_graph::ui::Dive::Ice(sid))
                            if sid != IceGraphId(usize::MAX) && sid != ice::NEW_SNIPPET =>
                        {
                            self.nav.current_subnet = Some(sid)
                        }
                        Some(node_graph::ui::Dive::MaterialX(op)) => self.nav.materialx = Some(op),
                        Some(node_graph::ui::Dive::Compound(op)) => {
                            self.graph.inside.request = Some(node_graph::compound::Request::Enter(op))
                        }
                        _ => {}
                    }
                    // Make Compound, from the add menu: the operators and their
                    // ICE graphs packed into an editable instance.
                    if let Some(ids) = self.graph.make_compound_request.take() {
                        self.graph_file.message = match self.graph.make_compound(self.subnets, &ids, "Compound") {
                            Ok(_) => format!(
                                "{} operator{} made a compound: double-click it to go inside",
                                ids.len(),
                                if ids.len() == 1 { "" } else { "s" }
                            ),
                            Err(e) => e,
                        };
                    }
                }
            },
        }
    }
}

/// The bar over the Node Graph pane inside a compound: the instances entered
/// (Root goes back out of all of them, ↑ Up out of one), and, when locked,
/// Make Editable.
fn compound_bar(ui: &mut egui::Ui, graph: &mut NodeGraphState) {
    use node_graph::compound::Request;
    let (text, dim) = (theme::c(235, 235, 235), theme::c(200, 200, 200));
    ui.horizontal_wrapped(|ui| {
        if ui
            .link(egui::RichText::new("Root").color(text))
            .on_hover_text("Back to the root scene graph")
            .clicked()
        {
            graph.inside.request = Some(Request::LeaveAll);
        }
        for name in &graph.inside.path {
            ui.label(egui::RichText::new(" › ").color(dim));
            ui.label(egui::RichText::new(name).strong().color(text));
        }
        if graph.inside.locked {
            ui.label(
                egui::RichText::new("  locked: it follows its definition")
                    .small()
                    .color(dim),
            );
            if ui
                .button("Make Editable")
                .on_hover_text("A copy of the definition for this instance alone, to change here")
                .clicked()
            {
                graph.inside.request = Some(Request::MakeEditable);
            }
        }
        if ui
            .button("↑ Up")
            .on_hover_text("Back out to the graph the compound is in (↑)")
            .clicked()
        {
            graph.inside.request = Some(Request::Leave);
        }
    });
    ui.separator();
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
    crate::ice::compound::keep_params(graph, subnets);
    // What each spare parameter that follows another followed last frame.
    thread_local! {
        static FOLLOWED: std::cell::RefCell<crate::node_graph::links::Followed> = Default::default();
    }
    FOLLOWED.with(|f| graph.follow_kept(&mut f.borrow_mut()));
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
    // ICE operators are kept by what their graphs compute now.
    graph.know_ice(subnets);
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

/// Which build this is: the version, and on the build machines the build's
/// number and commit (`0.2.0 · build 155 (a1b2c3d)`); built anywhere else,
/// `0.2.0 (local)`. `XMS_BUILD` and `GITHUB_SHA` are set by the release
/// workflow when it compiles.
pub(crate) fn version() -> String {
    let v = env!("CARGO_PKG_VERSION");
    match (option_env!("XMS_BUILD"), option_env!("GITHUB_SHA")) {
        (Some(n), Some(sha)) => format!("{v} · build {n} ({})", &sha[..sha.len().min(7)]),
        _ => format!("{v} (local)"),
    }
}

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
    /// File > Settings, at a page.
    Settings(settings::Page),
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
        crate::menus::menu(ui, "Settings", |ui| {
            for page in settings::Page::ALL {
                if ui.button(format!("{}...", page.label())).clicked() {
                    c = Some(FileCommand::Settings(page));
                    ui.close();
                }
            }
        });
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

/// Window > Layout: the layouts Imago comes with, and the default.
fn layout_menu(ui: &mut egui::Ui, layout: &mut Layout, locked: bool, message: &mut String) {
    ui.set_min_width(220.0);
    for (name, _) in layout::PRESETS {
        if ui.add_enabled(!locked, egui::Button::new(name)).clicked() {
            *message = match layout.use_preset(name) {
                Ok(()) => format!("Layout: {name}"),
                Err(e) => format!("Could not use layout {name}: {e}"),
            };
            ui.close();
        }
    }
    ui.separator();
    if ui.button("Save current as default")
        .on_hover_text("Imago opens with this layout, its tabs in front as they are now; File > New and Reset layout go back to it")
        .clicked()
    {
        *message = match layout.save_as_default() {
            Ok(()) => "This layout is the default now".into(),
            Err(e) => format!("Could not save the default layout: {e}"),
        };
        ui.close();
    }
    if ui
        .add_enabled(layout::has_saved_default(), egui::Button::new("Forget saved default"))
        .on_hover_text("xms_main_layout is the default again")
        .clicked()
    {
        *message = match Layout::forget_default() {
            Ok(()) => "xms_main_layout is the default again".into(),
            Err(e) => format!("Could not forget the default layout: {e}"),
        };
        ui.close();
    }
    if locked {
        ui.label(egui::RichText::new("Unlock the layout to change it.").small());
    }
}

/// Panes, layout and theme.
/// Help: the documentation, in the browser.
fn help_menu(ui: &mut egui::Ui, message: &mut String) {
    ui.set_min_width(200.0);
    if ui
        .button("Documentation")
        .on_hover_text("Imago's documentation in your browser: the user guide and the reference, with a search")
        .clicked()
    {
        if let Err(e) = crate::docs::open() {
            *message = e;
        }
        ui.close();
    }
}

fn window_menu(ui: &mut egui::Ui, layout: &mut Layout, browser: &mut FileBrowser, locked: bool, message: &mut String) {
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
    if ui
        .add_enabled(!locked, egui::Button::new("Reset layout"))
        .on_hover_text("Back to the default layout: the one saved as default, else xms_main_layout")
        .clicked()
    {
        layout.reset();
        ui.close();
    }
    crate::menus::menu(ui, "Layout", |ui| layout_menu(ui, layout, locked, message));
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
                NodeType::WriteUsd { path, .. }
                    if std::path::Path::new(&graph.expand_at(n.id, path.trim())).is_file() =>
                {
                    Some(graph.expand_at(n.id, path.trim()))
                }
                _ => None,
            })
            .into_iter()
            .collect(),
        PanelAction::WriteImage(id) => graph
            .nodes
            .iter()
            .find(|n| n.id == *id)
            .map(|n| image_frames(graph, n).into_iter().map(|(_, p)| p).filter(|p| std::path::Path::new(p).is_file()).collect())
            .unwrap_or_default(),
        // Rendering writes its layer again, as asked: no question.
        PanelAction::RunCommand(_) | PanelAction::SceneRange(_) => vec![],
    }
}

fn run_action(graph: &NodeGraphState, subnets: &IceGraphStore, batch: &BatchState, action: PanelAction) {
    match action {
        PanelAction::Write { targets, all_files } => batch::start(batch, graph, targets, all_files),
        PanelAction::WriteUsd(id) => write_usd(graph, subnets, id),
        PanelAction::WriteImage(id) => write_image(graph, subnets, id),
        PanelAction::RunCommand(id) => run_command(graph, subnets, id),
        // Set on the graph where the panel asks (`Pane::Parameters`).
        PanelAction::SceneRange(_) => {}
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
            layout::ask_for_default();
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
                    if ui.button(format!("{} Save", crate::icons::FLOPPY_DISK)).clicked() {
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

/// The Layer Breaks above node `from`, not bypassed: the nearest on each way
/// up the scene's inputs (every input of a Merge, the first of any other
/// node), not those above them.
fn layer_breaks(graph: &NodeGraphState, from: types::NodeId) -> Vec<types::NodeId> {
    let mut found = vec![];
    let mut seen = std::collections::HashSet::new();
    let mut up = vec![from];
    while let Some(id) = up.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(n) = graph.nodes.iter().find(|n| n.id == id) else {
            continue;
        };
        if matches!(n.node_type, NodeType::LayerBreak) && !n.bypassed {
            found.push(id);
            continue;
        }
        let inputs = match n.node_type {
            NodeType::Merge { .. } if !n.bypassed => &n.inputs[..],
            _ => &n.inputs[..n.inputs.len().min(1)],
        };
        up.extend(inputs.iter().filter_map(|i| i.connected_output.map(|(s, _)| s)));
    }
    found.sort_by_key(|n| n.0);
    found
}

/// Payloads loaded (`load`) or unloaded at the prims of `paths`, in the scene
/// of operator `shown` (the display flag's when `None`): each kept in the
/// Payloads of the Load USD operator upstream that read its stage, at its
/// path in the file. Paths of no stage, or above where a reference puts it,
/// are passed over.
fn set_payloads(
    graph: &mut NodeGraphState,
    subnets: &IceGraphStore,
    shown: Option<types::NodeId>,
    paths: &[String],
    load: bool,
) {
    let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| {
        subnets
            .get(sid)
            .map(|sg| sg.evaluate(mesh, template))
            .unwrap_or_else(|| mesh.clone())
    };
    let scene = match shown {
        Some(id) => graph.evaluate_scene_at(id, &eval),
        None => graph.evaluate_scene(&eval),
    };
    let Some((view, _, types::EvalResult::Named(prims))) = scene else {
        return;
    };
    // Each stage once: the trees of the prims, by the file read.
    let mut trees: Vec<std::sync::Arc<usd_scene::StageTree>> = vec![];
    for t in prims.iter().filter_map(|p| p.stage.clone()) {
        if !trees.iter().any(|x| std::sync::Arc::ptr_eq(x, &t)) {
            trees.push(t);
        }
    }
    let mut rules: Vec<(types::NodeId, String)> = vec![];
    for path in paths {
        let Some(tree) = trees.iter().find(|t| t.node(path).is_some()) else {
            continue;
        };
        let (Some(file), Some(node)) = (tree.file_path(path), prim_inspector::load_of(graph, view, tree)) else {
            continue;
        };
        rules.push((node, file));
    }
    for (id, file) in rules {
        if let Some(types::NodeType::LoadUsd { payloads, .. }) =
            graph.nodes.iter_mut().find(|n| n.id == id).map(|n| &mut n.node_type)
        {
            payloads.set(&file, load);
        }
    }
}

/// The operator a tab following `f` shows in `graph`, the compounds entered
/// being `level` (`layout::level_of`): the one selected, the one with the
/// display flag, the one pinned (when it is still there, at its level).
fn shown_by(graph: &NodeGraphState, f: Follows, level: u64) -> Option<types::NodeId> {
    let there = |id: types::NodeId| graph.nodes.iter().any(|n| n.id == id);
    match f {
        Follows::Selection => graph.selected_node,
        Follows::Display => graph.view_flag.filter(|id| there(*id)),
        Follows::Pinned { node, level: at } => (at == level && there(node)).then_some(node),
    }
}

/// The choices of what a tab follows, `now` the one it has: Selection,
/// Display Flag, Pinned (to the operator it shows; chosen again, back to the
/// selection). The one clicked.
fn follow_choices(ui: &mut egui::Ui, graph: &NodeGraphState, now: Follows, level: u64) -> Option<Follows> {
    let mut out = None;
    let small = |t: &str| egui::RichText::new(t);
    if ui
        .selectable_label(now == Follows::Selection, small("S  Selection"))
        .on_hover_text("The operator selected")
        .clicked()
    {
        out = Some(Follows::Selection);
    }
    if ui
        .selectable_label(now == Follows::Display, small("D  Display Flag"))
        .on_hover_text("The operator with the display flag, whatever is selected")
        .clicked()
    {
        out = Some(Follows::Display);
    }
    let pinned = matches!(now, Follows::Pinned { .. });
    let here = shown_by(graph, now, level);
    let r = ui
        .add_enabled_ui(pinned || here.is_some(), |ui| {
            ui.selectable_label(pinned, small("P  Pinned"))
        })
        .inner
        .on_hover_text("The operator shown now, whatever is selected. Click again to follow the selection");
    if r.clicked() {
        out = match (pinned, here) {
            (true, _) => Some(Follows::Selection),
            (false, Some(node)) => Some(Follows::Pinned { node, level }),
            (false, None) => None,
        };
    }
    out
}

/// Room at the end of a tab's title for its `follows_mark`.
const MARK_ROOM: &str = "       ";

/// What a tab's letter means, and that it is a menu.
fn follows_hint(f: Follows) -> &'static str {
    match f {
        Follows::Selection => "Follows the selection. Click to follow the display flag, or to pin it",
        Follows::Display => "Follows the display flag. Click to follow the selection, or to pin it",
        Follows::Pinned { .. } => "Pinned to its operator. Click to follow the selection or the display flag",
    }
}

/// The letter of what a tab follows: Selection, Display flag, Pinned.
fn follows_letter(f: Follows) -> &'static str {
    match f {
        Follows::Selection => "S",
        Follows::Display => "D",
        Follows::Pinned { .. } => "P",
    }
}

/// Run a Write USD node: evaluate what enters it and write its edits.
fn write_usd(graph: &NodeGraphState, subnets: &IceGraphStore, id: types::NodeId) {
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else {
        return;
    };
    let NodeType::WriteUsd {
        path,
        frames,
        start,
        end,
        step,
        flatten,
    } = &node.node_type
    else {
        return;
    };
    // The frames written: the scene's, the one shown, or its own.
    let at: Vec<f64> = match frames {
        types::WriteFrames::Current => vec![graph.frame()],
        types::WriteFrames::Scene => usd_write::frame_list(graph.scene.start as f64, graph.scene.end as f64, 1.0),
        types::WriteFrames::Custom => usd_write::frame_list(*start as f64, *end as f64, *step as f64),
    };
    // `$GDIR/out/$GNAME.usda`: its variables put in.
    let path = graph.expand_at(node.id, path.trim());
    let result = (|| {
        if path.is_empty() {
            return Err("No file set".to_string());
        }
        // Never boxes in place of geometry: every stage read whole.
        usd_scene::ready_to_render()?;
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
        if at.is_empty() {
            return Err("No frames to write: the range ends before it starts".to_string());
        }
        // Each frame as it cooks then. Nothing above it changing over time
        // (the same cook key at every frame): one frame, single values.
        let seconds = |f: f64| graph.scene.seconds(f);
        let keys: Vec<Option<u64>> = at
            .iter()
            .map(|f| graph.with_time(seconds(*f), || graph.cook_key(src, out)))
            .collect();
        let still = keys.iter().all(|k| k.is_some() && *k == keys[0]);
        let at = if still { at[..at.len().min(1)].to_vec() } else { at };
        // The Layer Breaks above it, cooked at the same frames: what they
        // give is left out of the layer.
        let breaks = layer_breaks(graph, src);
        let mut cooked = vec![];
        let mut bases: Vec<Vec<(f64, Vec<crate::types::Prim>)>> = vec![vec![]; breaks.len()];
        for f in &at {
            let mut cache = std::collections::HashMap::new();
            let r = graph.with_time(seconds(*f), || graph.eval_node_out(src, out, &mut cache, &eval));
            match r {
                Some(types::EvalResult::Named(prims)) => cooked.push((*f, prims)),
                other => return Err(not_prims(other)),
            }
            for (k, b) in breaks.iter().enumerate() {
                let r = graph.with_time(seconds(*f), || graph.eval_node_out(*b, 0, &mut cache, &eval));
                if let Some(types::EvalResult::Named(prims)) = r {
                    bases[k].push((*f, prims));
                }
            }
        }
        // A stage first read by this cook: shown as boxes until it is read.
        if usd_scene::any_reading() {
            return Err("A stage is still being read: write when it is done".to_string());
        }
        let out = std::path::Path::new(&path);
        // Flattened: the override layer beside the file, composed into it,
        // then gone.
        let over = out.with_file_name(format!(
            ".{}.over.usda",
            out.file_stem().and_then(|s| s.to_str()).unwrap_or("scene")
        ));
        let layer = if *flatten { over.as_path() } else { out };
        let mut report = if !breaks.is_empty() {
            usd_write::write_frames_above(&cooked, &bases, layer, &graph.scene)?
        } else if still || cooked.len() == 1 {
            usd_write::write_override(&cooked[0].1, layer, &graph.scene)?
        } else {
            usd_write::write_frames(&cooked, layer, &graph.scene)?
        };
        if *flatten {
            let flat = xms_usd::flatten(&over, out);
            let _ = std::fs::remove_file(&over);
            flat?;
            report.notes.push("Flattened: one file, the stages and the changes composed, no sublayers or references".into());
        }
        Ok(report)
    })();
    usd_write::remember(id, result);
}

/// The frames a Write Image writes, and the file of each: the scene's, the one
/// shown, or its own range; `$F4` (or `####`) in the name is the frame.
fn image_frames(graph: &NodeGraphState, node: &crate::node_graph::GraphNode) -> Vec<(i64, String)> {
    let NodeType::WriteImage { path, frames, start, end, .. } = &node.node_type else { return vec![] };
    let at: Vec<i64> = match frames {
        types::WriteFrames::Current => vec![graph.frame().round() as i64],
        types::WriteFrames::Scene => (graph.scene.start..=graph.scene.end).collect(),
        types::WriteFrames::Custom => (*start..=*end).collect(),
    };
    at.into_iter().map(|f| (f, crate::core::image_file::path_at(path, f))).collect()
}

/// Write Image: its input at each of its frames, in the colour space asked
/// (alpha made straight for an 8-bit file), to its file.
fn write_image(graph: &NodeGraphState, subnets: &IceGraphStore, id: types::NodeId) {
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else { return };
    let NodeType::WriteImage { path, colour, .. } = &node.node_type else { return };
    let eval = |sid: IceGraphId, mesh: &crate::core::geo::Geo, template: &[crate::ice::Reference]| -> crate::core::geo::Geo {
        subnets.get(sid).map(|sg| sg.evaluate(mesh, template)).unwrap_or_else(|| mesh.clone())
    };
    let result = (|| {
        if path.trim().is_empty() {
            return Err("No file set".to_string());
        }
        let (src, out) = node.inputs.first().and_then(|i| i.connected_output).ok_or("Nothing connected")?;
        let frames = image_frames(graph, node);
        if frames.is_empty() {
            return Err("No frames to write: the range ends before it starts".to_string());
        }
        for (f, file) in &frames {
            let r = graph.with_time(graph.scene.seconds(*f as f64), || {
                graph.eval_node_out(src, out, &mut std::collections::HashMap::new(), &eval)
            });
            let Some(types::EvalResult::Image(img, space)) = r else {
                return Err(format!("Frame {f}: no image comes in"));
            };
            let img = crate::core::image_ops::working(&img, &space)?;
            let target = crate::core::image_file::colour_space(colour, file);
            let straight = !file.to_ascii_lowercase().ends_with(".exr");
            let img = crate::core::image_ops::to_space(&img, &target, straight)?;
            xms_image::write(&img, std::path::Path::new(file))?;
        }
        Ok(match frames.as_slice() {
            [(_, one)] => format!("Written: {one}"),
            many => format!("Written: {} frames, {} to {}", many.len(), many[0].1, many[many.len() - 1].1),
        })
    })();
    crate::core::image_ops::remember(id, result);
}

/// Why what arrives at a Write USD is not primitives to write.
fn not_prims(r: Option<types::EvalResult>) -> String {
    match r {
        Some(types::EvalResult::Single(_)) => {
            "The input is one plain mesh, with no primitive path (after Unpack, or a Merge set to Flatten)".to_string()
        }
        Some(types::EvalResult::Material(_)) => {
            "The input is a material, not primitives: plug it into Material Binding".to_string()
        }
        Some(types::EvalResult::Named(_)) => String::new(),
        Some(types::EvalResult::Image(..)) => "The input is an image, not primitives".to_string(),
        Some(_) => "The input is a clip, not primitives".to_string(),
        None => "The input did not cook".to_string(),
    }
}

// Scene setup

#[cfg(test)]
mod tests {
    use super::*;

    /// What a Parameters tab shows by what it follows: the operator
    /// selected, the one with the display flag, the one pinned while it is
    /// there at its level; nothing for a pin gone or in another compound.
    #[test]
    fn a_tab_shows_the_operator_it_follows() {
        let mut g = NodeGraphState::default();
        let a = g.add_node("A".into(), NodeType::cube(1.0), egui::pos2(0.0, 0.0));
        let b = g.add_node("B".into(), NodeType::sphere(0.5, 8), egui::pos2(0.0, 100.0));
        g.selected_node = Some(a);
        g.view_flag = Some(b);
        let top = layout::level_of(&[]);
        let inside = layout::level_of(&["Rig".to_string()]);
        assert_eq!(shown_by(&g, Follows::Selection, top), Some(a));
        assert_eq!(shown_by(&g, Follows::Display, top), Some(b));
        let pin = Follows::Pinned { node: b, level: top };
        assert_eq!(shown_by(&g, pin, top), Some(b));
        assert_eq!(shown_by(&g, pin, inside), None, "another compound's graph");
        g.view_flag = None;
        assert_eq!(shown_by(&g, Follows::Display, top), None);
        g.nodes.retain(|n| n.id != b);
        assert_eq!(shown_by(&g, pin, top), None, "gone");
    }

    /// Write USD finds the nearest Layer Break on each way up: through every
    /// input of a Merge, not one above another, not a bypassed one, and not on
    /// an ICE operator's reference input.
    #[test]
    fn a_payload_unloaded_on_a_row_is_kept_in_its_load_usd() {
        let dir = std::env::temp_dir().join("xms_payload_rows");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("rock.usda"),
            "#usda 1.0\ndef Xform \"Rock\" {\n    def Mesh \"geo\" {\n        int[] faceVertexCounts = [3]\n        int[] faceVertexIndices = [0, 1, 2]\n        point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0)]\n    }\n}\n",
        )
        .unwrap();
        let set = dir.join("set.usda");
        std::fs::write(
            &set,
            "#usda 1.0\ndef Xform \"set\" {\n    def Xform \"rock\" (\n        payload = @./rock.usda@</Rock>\n    ) {\n        float3[] extentsHint = [(0, 0, 0), (1, 1, 1)]\n    }\n}\n",
        )
        .unwrap();
        let mut g = NodeGraphState::default();
        let load = g.add_node(
            "Load".into(),
            NodeType::load_usd(set.to_string_lossy().into_owned()),
            egui::pos2(0.0, 0.0),
        );
        g.view_flag = Some(load);
        let payloads = |g: &NodeGraphState| match &g.nodes[0].node_type {
            NodeType::LoadUsd { payloads, .. } => payloads.clone(),
            _ => unreachable!(),
        };
        let store = IceGraphStore::default();
        set_payloads(&mut g, &store, None, &["/set/rock".into()], false);
        assert_eq!(payloads(&g).rules, vec![("/set/rock".to_string(), false)]);
        // Read so: the rock alone, its box, marked.
        let eval = |_: IceGraphId, m: &crate::core::geo::Geo, _: &[crate::ice::Reference]| m.clone();
        let Some((_, _, types::EvalResult::Named(prims))) = g.evaluate_scene(&eval) else {
            panic!()
        };
        assert!(!prims.iter().any(|p| p.path == "/set/rock/geo"));
        let rock = prims.iter().find(|p| p.path == "/set/rock").expect("its box");
        let node = rock.stage.as_ref().and_then(|t| t.node("/set/rock"));
        assert!(node.is_some_and(|n| n.unloaded));
        let (modes, _) = viewer::scene::draw_modes(&prims, &SceneHierarchy::default(), u64::MAX);
        let at = prims.iter().position(|p| p.path == "/set/rock").unwrap();
        assert_eq!(modes[at], types::DrawMode::Bounds, "drawn as its box");
        let mut h = SceneHierarchy::default();
        h.rebuild_scene(g.evaluate_scene(&eval));
        assert!(h.is_unloaded("/set/rock"), "its row says so");
        // Loaded again: the rule replaced, the mesh back.
        set_payloads(&mut g, &store, None, &["/set/rock".into()], true);
        assert_eq!(payloads(&g).rules, vec![("/set/rock".to_string(), true)]);
        let Some((_, _, types::EvalResult::Named(prims))) = g.evaluate_scene(&eval) else {
            panic!()
        };
        assert!(prims.iter().any(|p| p.path == "/set/rock/geo"));
    }

    #[test]
    fn write_usd_finds_the_layer_breaks_above_it() {
        let mut g = NodeGraphState::default();
        let at = |y: f32| egui::pos2(0.0, y);
        let cube = g.add_node("Cube".into(), NodeType::cube(1.0), at(0.0));
        let high = g.add_node("High".into(), NodeType::LayerBreak, at(100.0));
        let low = g.add_node("Low".into(), NodeType::LayerBreak, at(200.0));
        let sphere = g.add_node("Sphere".into(), NodeType::sphere(0.5, 8), at(0.0));
        let off = g.add_node("Off".into(), NodeType::LayerBreak, at(100.0));
        let merge = g.add_node("Merge".into(), NodeType::merge(), at(300.0));
        let write = g.add_node("Write".into(), NodeType::write_usd(""), at(400.0));
        g.add_connection(cube, 0, high, 0);
        g.add_connection(high, 0, low, 0);
        g.add_connection(sphere, 0, off, 0);
        g.add_connection(low, 0, merge, 0);
        g.add_connection(off, 0, merge, 1);
        g.add_connection(merge, 0, write, 0);
        assert_eq!(layer_breaks(&g, merge), vec![low, off]);
        g.nodes.iter_mut().find(|n| n.id == off).unwrap().bypassed = true;
        assert_eq!(layer_breaks(&g, merge), vec![low]);
        g.nodes.iter_mut().find(|n| n.id == low).unwrap().bypassed = true;
        assert_eq!(layer_breaks(&g, merge), vec![high]);
        assert!(layer_breaks(&g, sphere).is_empty());
        // On an ICE operator's reference input: not the scene's way up.
        let ice = g.add_node(
            "Ice".into(),
            NodeType::Ice {
                id: IceGraphId(0),
                name: "Ice".into(),
                references: 1,
                params: Default::default(),
                snippet: false,
            },
            at(500.0),
        );
        g.add_connection(sphere, 0, ice, 0);
        g.add_connection(low, 0, ice, 1);
        assert!(layer_breaks(&g, ice).is_empty());
    }

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
