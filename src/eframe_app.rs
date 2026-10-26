//! The program's shell: a window run by eframe, the interface drawn with egui,
//! and the Viewer's 3D view on wgpu (`crate::viewer`).

use std::time::{Duration, Instant};

use crate::app::AppState;
use crate::{modelling, theme, viewport};

/// Multisampling of the interface's pass; the Viewer's picture is laid into it,
/// so its pipeline has to match.
const EGUI_SAMPLES: u16 = 4;

/// The program under eframe.
pub struct ImagoApp {
    app: AppState,
    poly_tool: modelling::PolyTool,
    nav_settings: viewport::nav::NavSettings,
    viewer: crate::viewer::Viewer,
    /// The graph's content hash last seen (revision tracking).
    last_hash: Option<u64>,
    /// The revision the Operator Stack and Scene Explorer were last built at.
    seen_stack: u64,
    seen_scene: u64,
    /// When the last frame was drawn, for the timeline's step.
    last_frame: Instant,
    /// Closing was agreed to: let the window close.
    closing: bool,
}

impl ImagoApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::init(&cc.egui_ctx);
        // Render folders an Imago that crashed left behind.
        crate::image_viewer::clean_up_left_behind();
        if let Some(render_state) = cc.wgpu_render_state.as_ref() {
            crate::viewer::install(render_state, EGUI_SAMPLES as u32);
        }
        // A slightly wider feathering than egui's own: softer edges on
        // wires, outlines and frames. Text keeps egui's own weight.
        cc.egui_ctx
            .tessellation_options_mut(|t| t.feathering_size_in_pixels = 1.4);
        Self {
            app: AppState::default(),
            poly_tool: modelling::PolyTool::default(),
            nav_settings: viewport::nav::NavSettings::default(),
            viewer: crate::viewer::Viewer::default(),
            last_hash: None,
            seen_stack: 0,
            seen_scene: 0,
            last_frame: Instant::now(),
            closing: false,
        }
    }
}

impl eframe::App for ImagoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64().min(0.25);
        self.last_frame = now;
        let ctx = ui.ctx().clone();

        // The interface, undo, what changed, questions before closing, then
        // what the panes show.
        crate::draw_ui(
            ui,
            &mut self.app,
            &mut self.poly_tool,
            &mut self.nav_settings,
            dt,
            Some(&mut self.viewer),
        );
        crate::history_frame(&ctx, &mut self.app);
        // The graph's variables ($GNAME, $GDIR, $F…), for paths and command
        // lines. A change other than the frame cooks again what reads paths.
        let a = &self.app;
        let file = a.graph_file.path.as_deref();
        let globals = crate::core::vars::Globals {
            gname: file
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            gdir: file
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            frame: a.graph.scene.frame(a.playback.time).round() as i64,
            start: a.graph.scene.start,
            end: a.graph.scene.end,
            fps: a.graph.scene.fps(),
            user: a.graph.variables.clone(),
        };
        if crate::core::vars::set_globals(globals) {
            self.app.revision.0 = self.app.revision.0.wrapping_add(1);
        }
        crate::track_revision_frame(&ctx, &mut self.app, &mut self.last_hash);

        // Closing the window asks first when there are unsaved changes.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.closing {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if crate::save_first_frame(&ctx, &mut self.app, close_requested && !self.closing) {
            self.closing = true;
            // Renders started from Render Command end with the program.
            crate::core::command::stop_all();
            // The Image Viewer's temporary layers and renders go with it.
            crate::image_viewer::clean_up();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Example files the graph names that are not there: offered, fetched.
        if !self.app.save_first.busy() {
            crate::downloads::frame(&ctx, &mut self.app.downloads, &self.app.graph);
            if let Some(m) = self.app.downloads.message.take() {
                self.app.graph_file.message = m;
            }
        }

        crate::refresh_operator_stack(&mut self.app, &mut self.seen_stack);
        crate::refresh_scene_hierarchy(&mut self.app, &mut self.seen_scene);

        // A new frame when something happens; while something runs on its
        // own (playback, textures decoding, a batch), keep drawing.
        let busy = self.app.playback.playing
            || viewport::textures::pending() > 0
            || self.app.batch.0.lock().map(|p| p.running).unwrap_or(false)
            || crate::core::command::any_running();
        if busy {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

/// Open the window and run the program until it closes.
pub fn run() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("XMS | Imago")
        .with_app_id("xms-imago")
        .with_inner_size([1600.0, 1000.0])
        .with_maximized(true);
    if let Some((rgba, width, height)) = crate::decode_png(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(egui::IconData { rgba, width, height });
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        // Smoother wires and outlines, as under Bevy (4 samples there).
        multisampling: EGUI_SAMPLES,
        ..Default::default()
    };
    eframe::run_native("XMS | Imago", options, Box::new(|cc| Ok(Box::new(ImagoApp::new(cc)))))
}
