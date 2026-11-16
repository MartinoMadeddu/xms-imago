//! A Collection's parameters (`core::collections`).

use crate::core::collections::{self, Held};

use super::form::{group, Status};
use super::ui::{locations_tab, TabKind};

/// What a Collection operator holds, to edit.
pub struct Rows<'a> {
    pub name: &'a mut String,
    pub location: &'a mut String,
    pub includes: &'a mut Vec<String>,
    pub excludes: &'a mut Vec<String>,
    pub explicit_only: &'a mut bool,
}

/// Its name, Collection Location and rule, then its includes and excludes as Prims
/// Location lists. `paths`: every prim of the scene coming in; `selected`: the
/// Scene Explorer's; `held`: the collections coming in.
pub fn rows(ui: &mut egui::Ui, r: &mut Rows, paths: &[String], selected: &[String], held: &[Held]) {
    group(
        ui,
        "Collection",
        Some("A named set of prims, written as a USD collection on the prim at its Collection Location. %name in any Prims Location list takes its members"),
        |f| {
            f.text(
                "Name",
                Some("What %name calls it. Letters, digits and _; $OPNAME is the operator's name"),
                r.name,
                "characters",
            );
            let clean = collections::clean_name(&crate::core::vars::expand(r.name.as_str(), &[]));
            if clean.is_empty() {
                f.status(Status::Bad, "No name: no collection is made");
            } else {
                f.status(Status::Info, format!("Taken as %{clean}"));
            }
            f.text_vars(
                "Collection Location",
                Some("The prim it is written on, made (a Scope) where the scene has none: /collections, or any prim. Variables are put in"),
                r.location,
                collections::DEFAULT_LOCATION,
                false,
            );
            let used = collections::holder_path(r.location);
            if used != r.location.trim() {
                f.status(Status::Info, format!("Used as {used}"));
            }
            f.toggle(
                "Explicit Only",
                Some("Off (Expand Prims): an include takes its prim and everything below it. On: the prims named alone"),
                r.explicit_only,
            );
            if held.iter().any(|h| {
                h.collection.name == clean && h.holder == collections::holder_path(r.location)
            }) {
                f.status(Status::Info, "Replaces the collection of this name coming in");
            }
        },
    );
    locations_tab(
        ui,
        r.includes,
        paths,
        selected,
        TabKind::Collects,
        None,
        Some("Includes"),
        held,
    );
    locations_tab(
        ui,
        r.excludes,
        paths,
        selected,
        TabKind::Collects,
        None,
        Some("Excludes"),
        held,
    );
}
