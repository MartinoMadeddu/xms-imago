//! The Edit Session's Parameters: its steps, as a table to switch off, rename,
//! reorder and delete; Freeze and Explode.

use crate::core::edit_session::{self, Step};
use crate::icons;

use super::form::{group, FieldExt, Status};

/// What the steps' buttons ask of the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Its steps as Edit Primitives in the graph, in its place.
    Explode,
}

pub fn rows(ui: &mut egui::Ui, steps: &mut Vec<Step>) -> Option<Ask> {
    let mut ask = None;
    group(
        ui,
        "Steps",
        Some("What was done to prims in the Viewer and the Selection bar, oldest first. Each step's values are absolute: a later step on the same prim wins, so switching an earlier one off changes only what no later one set"),
        |f| {
            if steps.is_empty() {
                f.status(
                    Status::Info,
                    "Nothing yet: select prims in the Viewer or the Scene Explorer, and move them with the handles or the Selection bar",
                );
                return;
            }
            let mut up: Option<usize> = None;
            let mut gone: Option<usize> = None;
            let n = steps.len();
            for (k, s) in steps.iter_mut().enumerate() {
                f.ui.horizontal(|ui| {
                    ui.checkbox(&mut s.on, "").on_hover_text("On: applied. Off: left out, the step kept");
                    ui.field(egui::TextEdit::singleline(&mut s.name).desired_width(ui.available_width() - 92.0));
                    let paths: Vec<&String> = s.prims.keys().collect();
                    let list = paths.iter().map(|p| p.as_str()).collect::<Vec<_>>().join("\n");
                    ui.label(egui::RichText::new(format!("{}", paths.len())).weak())
                        .on_hover_text(format!("Prims:\n{list}"));
                    ui.add_enabled_ui(k > 0, |ui| {
                        if ui.small_button(icons::CARET_UP).on_hover_text("Move up").clicked() {
                            up = Some(k);
                        }
                    });
                    ui.add_enabled_ui(k + 1 < n, |ui| {
                        if ui.small_button(icons::CARET_DOWN).on_hover_text("Move down").clicked() {
                            up = Some(k + 1);
                        }
                    });
                    if ui.small_button(icons::X).on_hover_text("Delete the step").clicked() {
                        gone = Some(k);
                    }
                });
            }
            if let Some(k) = up {
                steps.swap(k - 1, k);
            }
            if let Some(k) = gone {
                steps.remove(k);
            }
            let on = steps.iter().filter(|s| s.on).count();
            f.status(Status::Ok, format!("{on} of {} steps on", steps.len()));
            f.buttons("", None, |ui| {
                if ui
                    .button("Freeze")
                    .on_hover_text("The steps that are on made one, as the last to set each value left it; those off are dropped")
                    .clicked()
                {
                    *steps = edit_session::freeze(steps);
                }
                if ui
                    .button("Explode")
                    .on_hover_text("The steps that are on as Edit Primitives in the graph, in its place, one per step (prims with the same values share one)")
                    .clicked()
                {
                    ask = Some(Ask::Explode);
                }
            });
        },
    );
    ask
}
