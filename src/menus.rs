//! Menus that stay open while they are used.
//!
//! Since egui 0.32 a menu closes on any click inside it. Imago's menus hold
//! check boxes (the panes in Window, Invert zoom in Navigation) and filter
//! fields (Pick), which have to stay open while they are clicked; an item that
//! should close its menu calls `ui.close()` itself, as the menus have always
//! done. So every menu here closes only on a click outside it, as menus did
//! before 0.32.

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

/// The keys of an add menu's search: ↓ and ↑ move the highlight through the
/// `count` nodes found, Enter takes the one highlighted. The highlight goes
/// back to the first when the search text changes, or the menu opens. Kept
/// under `id`. Returns the highlighted row, whether it moved this frame (to
/// bring it into view), and whether Enter was pressed. The arrow keys are
/// taken, so the search line and the graph do not see them.
pub fn search_keys(ui: &mut egui::Ui, id: egui::Id, text: &str, count: usize, opened_now: bool) -> (usize, bool, bool) {
    use egui::{Key, Modifiers};
    let (mut hi, last): (usize, String) = if opened_now {
        (0, String::new())
    } else {
        ui.data(|d| d.get_temp(id)).unwrap_or_default()
    };
    if last != text {
        hi = 0;
    }
    let (down, up, enter) = ui.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.key_pressed(Key::Enter),
        )
    });
    if down {
        hi = (hi + 1).min(count.saturating_sub(1));
    }
    if up {
        hi = hi.saturating_sub(1);
    }
    let hi = hi.min(count.saturating_sub(1));
    ui.data_mut(|d| d.insert_temp(id, (hi, text.to_string())));
    (hi, down || up, enter && count > 0)
}
