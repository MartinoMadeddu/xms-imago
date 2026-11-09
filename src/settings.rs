//! File > Settings: Imago's settings in one window, a page for each part of
//! the program, listed on its left. Each setting is kept where its part keeps
//! it (the scene's up axis, unit and rate with the graph, the Viewer's colour
//! in `colour`, its navigation in `navigation.txt`, where renderers are
//! installed in `core::renderers`).

/// A page of the Settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Scene,
    Viewer,
    Renderers,
    Interface,
}

impl Page {
    pub const ALL: [Page; 4] = [Page::Scene, Page::Viewer, Page::Renderers, Page::Interface];

    pub fn label(self) -> &'static str {
        match self {
            Page::Scene => "Scene",
            Page::Viewer => "Viewer",
            Page::Renderers => "Renderers",
            Page::Interface => "Interface",
        }
    }
}

fn open_id() -> egui::Id {
    egui::Id::new("settings_window_open")
}

/// Show the Settings window at a page.
pub fn open(ctx: &egui::Context, page: Page) {
    ctx.data_mut(|d| d.insert_temp(open_id(), Some(page)));
}

/// The Settings window, while it is open. `scene`: the graph's, saved with
/// it; `nav`: the Viewer's navigation, saved when it changes.
pub fn window(
    ctx: &egui::Context,
    scene: &mut crate::node_graph::SceneRange,
    nav: &mut crate::viewport::nav::NavSettings,
) {
    let Some(mut page) = ctx.data(|d| d.get_temp::<Option<Page>>(open_id())).flatten() else {
        return;
    };
    let mut open = true;
    crate::menus::at_pointer(ctx, egui::Window::new("Settings"), "settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(110.0);
                    for p in Page::ALL {
                        ui.selectable_value(&mut page, p, p.label());
                    }
                });
                ui.separator();
                ui.vertical(|ui| match page {
                    Page::Scene => scene_page(ui, scene),
                    Page::Viewer => viewer(ui, nav),
                    Page::Renderers => renderers(ui),
                    Page::Interface => crate::ui_scale::settings(ui),
                });
            });
        });
    ctx.data_mut(|d| d.insert_temp(open_id(), open.then_some(page)));
}

/// Units a scene can be in, as USD states them (`metersPerUnit`).
const UNITS: [(f64, &str); 6] = [
    (1.0, "Metres"),
    (0.01, "Centimetres"),
    (0.001, "Millimetres"),
    (0.1, "Decimetres"),
    (0.0254, "Inches"),
    (0.3048, "Feet"),
];

/// The scene's up axis, unit and rate: what its layers say (Write USD's,
/// renders'), each stage put in them where Imago shows it.
fn scene_page(ui: &mut egui::Ui, scene: &mut crate::node_graph::SceneRange) {
    use crate::node_graph::UpAxis;
    let dim = crate::properties::form::ink::DIM();
    ui.strong("Scene");
    egui::Grid::new("settings_scene").num_columns(2).show(ui, |ui| {
        ui.label("Up axis");
        egui::ComboBox::from_id_salt("settings_up_axis")
            .selected_text(scene.up_axis.token())
            .show_ui(ui, |ui| {
                for a in UpAxis::ALL {
                    ui.selectable_value(&mut scene.up_axis, a, a.token());
                }
            });
        ui.end_row();
        ui.label("Unit");
        let now = UNITS
            .iter()
            .find(|u| (u.0 - scene.meters_per_unit).abs() < 1e-9)
            .map_or_else(|| format!("{} m", scene.meters_per_unit), |u| u.1.to_string());
        egui::ComboBox::from_id_salt("settings_unit")
            .selected_text(now)
            .show_ui(ui, |ui| {
                for (m, name) in UNITS {
                    ui.selectable_value(&mut scene.meters_per_unit, m, name);
                }
            });
        ui.end_row();
        ui.label("Frames a second");
        crate::timeline::ui::scene_rate(ui, scene, "settings_scene_rate");
        ui.end_row();
    });
    ui.label(
        egui::RichText::new(
            "Saved with the scene. Imago works in Y-up metres; every layer it writes (Write USD, a render) says these, \
             with each stage where Imago shows it. A stage of another rate is retimed to the scene's by seconds, as \
             USD does, and its Load USD says so",
        )
        .small()
        .color(dim),
    );
}

fn viewer(ui: &mut egui::Ui, nav: &mut crate::viewport::nav::NavSettings) {
    let dim = crate::properties::form::ink::DIM();
    ui.strong("Navigation");
    let before = *nav;
    ui.horizontal(|ui| {
        ui.label("Style");
        egui::ComboBox::from_id_salt("settings_nav_style")
            .selected_text(nav.style.label())
            .show_ui(ui, |ui| {
                for style in crate::viewport::nav::NavStyle::ALL {
                    ui.selectable_value(&mut nav.style, style, style.label());
                }
            })
            .response
            .on_hover_text("The keys and mouse buttons that move the Viewer's camera, as the program named has them");
    });
    ui.checkbox(&mut nav.invert_zoom, "Invert zoom drag");
    if *nav != before {
        nav.save();
    }
    ui.label(
        egui::RichText::new("The ? at the Viewer's top left shows the keys")
            .small()
            .color(dim),
    );
    ui.add_space(8.0);

    ui.strong("Colour");
    let mut follows = crate::colour::viewer_follows();
    if ui
        .checkbox(&mut follows, "As Image Viewer")
        .on_hover_text("The Viewer shows the view and display the Image Viewer's menus choose: one colour for both")
        .changed()
    {
        crate::colour::set_viewer_follows(follows);
    }
    ui.add_enabled_ui(!follows, |ui| {
        ui.horizontal(|ui| {
            ui.label("View and display");
            crate::colour::menus(ui, crate::colour::Pane::Viewer);
        });
    });
    ui.label(egui::RichText::new(crate::colour::about()).small().color(dim));
    ui.label(
        egui::RichText::new(
            "OpenColorIO's: the config is $OCIO's when it names one, else Imago's own (see docs/colour.md)",
        )
        .small()
        .color(dim),
    );
}

/// Each renderer: whether it is on this machine, where it is installed (for
/// one rendering in Imago), where its definition comes from; your folder.
fn renderers(ui: &mut egui::Ui) {
    use crate::core::renderers;
    use crate::properties::form::{group, Status};
    let dim = crate::properties::form::ink::DIM();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for d in renderers::definitions() {
            group(ui, &d.label, Some(d.about.as_str()), |f| {
                if let Some(folder) = &d.folder {
                    // Typed, then set when the field is left: each change
                    // looks for the renderers again.
                    let var = folder.variable.as_str();
                    let id = egui::Id::new(("renderer_folder", var));
                    let mut text = f
                        .ui
                        .ctx()
                        .data(|m| m.get_temp::<String>(id))
                        .unwrap_or_else(|| renderers::folder_var(var).unwrap_or_default());
                    let env = std::env::var(var).ok().filter(|v| !v.is_empty());
                    let hint = format!(
                        "Where {} is installed: ${var} inside Imago, its bin/ on the PATH of the command lines it runs. Empty: ${var} from the environment ({}). Programs opened from the Finder or the Dock on macOS do not see variables set in a shell's profile, so set it here",
                        d.label,
                        env.as_deref().unwrap_or("not set")
                    );
                    let shown = env.map(|e| format!("${var}: {e}")).unwrap_or_else(|| format!("${var} is not set"));
                    let r = f.text(&folder.label, Some(hint.as_str()), &mut text, &shown);
                    if r.lost_focus() {
                        renderers::set_folder(var, &text);
                        f.ui.ctx().data_mut(|m| m.remove::<String>(id));
                    } else if r.changed() {
                        f.ui.ctx().data_mut(|m| m.insert_temp(id, text));
                    }
                }
                crate::properties::render::definition_rows(f, &d);
            });
        }
        for e in renderers::errors() {
            crate::properties::form::status_label(ui, Status::Bad, &e);
        }
        ui.add_space(6.0);
        if let Some(dir) = renderers::folder() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Your definitions: {}", dir.display())).small().color(dim));
                if ui.small_button("Open").on_hover_text("Show the folder (made if it is not there)").clicked() {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = crate::docs::show(&dir);
                }
            });
        }
        ui.label(
            egui::RichText::new("$XMS_RENDERERS names another folder. See docs/render.md, Renderer definitions")
                .small()
                .color(dim),
        );
    });
}
