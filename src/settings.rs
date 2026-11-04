//! File > Settings: Imago's settings in one window, a page for each part of
//! the program, listed on its left. Each setting is kept where its part keeps
//! it (the Viewer's colour in `colour`).

/// A page of the Settings window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Viewer,
}

impl Page {
    pub const ALL: [Page; 1] = [Page::Viewer];

    pub fn label(self) -> &'static str {
        match self {
            Page::Viewer => "Viewer",
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

/// The Settings window, while it is open.
pub fn window(ctx: &egui::Context) {
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
                    Page::Viewer => viewer(ui),
                });
            });
        });
    ctx.data_mut(|d| d.insert_temp(open_id(), open.then_some(page)));
}

fn viewer(ui: &mut egui::Ui) {
    ui.strong("Colour");
    ui.horizontal(|ui| {
        ui.label("View and display");
        crate::colour::menus(ui, crate::colour::Pane::Viewer);
    });
    let dim = crate::properties::form::ink::DIM();
    ui.label(egui::RichText::new(crate::colour::about()).small().color(dim));
    ui.label(
        egui::RichText::new(
            "OpenColorIO's: the config is $OCIO's when it names one, else Imago's own (see docs/colour.md)",
        )
        .small()
        .color(dim),
    );
}
