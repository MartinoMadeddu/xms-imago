//! File > Settings: Imago's settings in one window, a page for each part of
//! the program, listed on its left. Each setting is kept where its part keeps
//! it (the Viewer's colour in `colour`, its navigation in `navigation.txt`).

/// A page of the Settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Viewer,
    Interface,
}

impl Page {
    pub const ALL: [Page; 2] = [Page::Viewer, Page::Interface];

    pub fn label(self) -> &'static str {
        match self {
            Page::Viewer => "Viewer",
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

/// The Settings window, while it is open. `nav`: the Viewer's navigation,
/// saved when it changes.
pub fn window(ctx: &egui::Context, nav: &mut crate::viewport::nav::NavSettings) {
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
                    Page::Viewer => viewer(ui, nav),
                    Page::Interface => crate::ui_scale::settings(ui),
                });
            });
        });
    ctx.data_mut(|d| d.insert_temp(open_id(), open.then_some(page)));
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
