//! Menus that stay open while they are used.
//!
//! Since egui 0.32 a menu closes on any click inside it. Imago's menus hold
//! check boxes (the panes in Window, Invert zoom in Navigation) and filter
//! fields (Pick), which have to stay open while they are clicked; an item
//! that should close its menu calls `ui.close()` itself, as the menus have
//! always done. So every menu here closes only on a click outside it, as
//! menus did before 0.32.

use egui::containers::menu::{is_in_menu, MenuButton, MenuConfig, SubMenuButton};

/// `ui.menu_button`, closing only on a click outside the menu (or on
/// `ui.close()`). Inside a menu it is a submenu, which follows its menu.
pub fn menu<'a, R>(
    ui: &mut egui::Ui,
    atoms: impl egui::IntoAtoms<'a>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let (response, inner) = if is_in_menu(ui) {
        SubMenuButton::new(atoms).ui(ui, add_contents)
    } else {
        let config = MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        MenuButton::new(atoms).config(config).ui(ui, add_contents)
    };
    egui::InnerResponse::new(inner.map(|i| i.inner), response)
}
