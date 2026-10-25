//! Variables in the interface (see `core::vars` for what they are):
//!
//! - **File > Variables**: one table of the graph's own variables (saved
//!   with it) and the built-in ones: variable, result, description.
//! - **The list on `$` and `{`**: typing `$` (or `${`) in a path or command
//!   line lists the built-in variables it can take, with their values;
//!   typing `{` in an operator's line lists its own (`{usd}`). A click puts
//!   one in.

use crate::core::vars;
use crate::properties::form::FieldExt;

fn open_id() -> egui::Id { egui::Id::new("variables_window_open") }

/// Show File > Variables.
pub fn open(ctx: &egui::Context) { ctx.data_mut(|d| d.insert_temp(open_id(), true)); }

/// The table's columns: the variable, what it is now, what it is for.
const NAME_W: f32 = 150.0;
const RESULT_W: f32 = 260.0;
const ABOUT_W: f32 = 220.0;

/// File > Variables, while it is open: one table, the graph's own variables
/// then the built-in ones, each a row of the variable, its result and what
/// it is for. Changes apply at once and are steps of the history, as any
/// change to the graph.
pub fn window(ctx: &egui::Context, v: &mut vars::Variables) {
    let mut open = ctx.data(|d| d.get_temp::<bool>(open_id())).unwrap_or(false);
    if !open { return; }
    let dim = crate::properties::form::ink::DIM();
    let bad = crate::properties::form::ink::BAD();
    egui::Window::new("Variables").open(&mut open).collapsible(false).resizable(true).default_width(NAME_W + RESULT_W + ABOUT_W + 60.0).show(ctx, |ui| {
        ui.label(egui::RichText::new("Write them as $NAME in paths, locations, values and command lines. Type $ in a field for the list.").small().color(dim));
        ui.add_space(4.0);
        let mut remove = None;
        let mut add = false;
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
            egui::Grid::new("variables_table").num_columns(3).spacing([10.0, 6.0]).striped(true).show(ui, |ui| {
                let head = |ui: &mut egui::Ui, t: &str, w: f32| { ui.add_sized([w, 16.0], egui::Label::new(egui::RichText::new(t).small().strong().color(dim))); };
                head(&mut *ui, "Variable", NAME_W); head(&mut *ui, "Result", RESULT_W); head(&mut *ui, "Description", ABOUT_W);
                ui.end_row();

                // The graph's own: saved with it.
                section(ui, "Graph", "Saved with the graph. A value can hold variables: $JOB/assets");
                let names: Vec<String> = v.list.iter().map(|x| x.name.clone()).collect();
                for (k, var) in v.list.iter_mut().enumerate() {
                    let h = ui.spacing().interact_size.y;
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("$").monospace());
                            ui.field_sized([NAME_W - 16.0, h], egui::TextEdit::singleline(&mut var.name).hint_text("NAME").font(egui::TextStyle::Monospace));
                        });
                        let taken = names.iter().take(k).any(|n| *n == var.name);
                        match vars::check_name(&var.name) {
                            Err(e) => { ui.label(egui::RichText::new(e).small().color(bad)); }
                            Ok(()) if taken => { ui.label(egui::RichText::new("a variable above has this name").small().color(bad)); }
                            Ok(()) => {}
                        }
                    });
                    ui.vertical(|ui| {
                        let r = ui.field_sized([RESULT_W, h], egui::TextEdit::singleline(&mut var.value).hint_text("value"));
                        complete(ui, &r, &mut var.value, &[]);
                        // With variables in it: what it comes to.
                        if var.value.contains('$') && vars::check_name(&var.name).is_ok() {
                            ui.label(egui::RichText::new(format!("→ {}", vars::expand(&format!("${{{}}}", var.name), &[]))).small().color(dim));
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.field_sized([ABOUT_W - 30.0, h], egui::TextEdit::singleline(&mut var.description).hint_text("what it is for"));
                        if ui.small_button("🗑").on_hover_text("Remove this variable").clicked() { remove = Some(k); }
                    });
                    ui.end_row();
                }
                // $JOB: built in, but typed here with the graph's own.
                ui.label(egui::RichText::new("$JOB").monospace());
                ui.vertical(|ui| {
                    let hint = std::env::var("JOB").map(|j| format!("{j} (from the environment)")).unwrap_or_else(|_| "the graph's folder".into());
                    let r = ui.field_sized([RESULT_W, ui.spacing().interact_size.y], egui::TextEdit::singleline(&mut v.job).hint_text(hint));
                    complete(ui, &r, &mut v.job, &[]);
                    if v.job.contains('$') { ui.label(egui::RichText::new(format!("→ {}", vars::expand("$JOB", &[]))).small().color(dim)); }
                });
                ui.label(egui::RichText::new("the project folder: typed here, else JOB from the environment, else $GDIR").small().color(dim));
                ui.end_row();
                if ui.button("+ Variable").on_hover_text("A variable of the graph's own").clicked() { add = true; }
                ui.label("");
                ui.label("");
                ui.end_row();

                // Built in: what the graph's file, frames and operators give.
                section(ui, "Built-in", "Set by the graph's file, the scene's frames and the operator reading the text");
                for (name, what) in vars::BUILTINS.into_iter().filter(|(n, _)| *n != "JOB") {
                    ui.label(egui::RichText::new(format!("${name}")).monospace());
                    let result = if name == "OPNAME" { "each operator's name".to_string() } else { vars::expand(&format!("${{{name}}}"), &[]) };
                    let shown = if result.is_empty() { egui::RichText::new("(empty)").color(dim) } else { egui::RichText::new(result) };
                    ui.add_sized([RESULT_W, 16.0], egui::Label::new(shown).truncate());
                    ui.label(egui::RichText::new(what).small().color(dim));
                    ui.end_row();
                }
            });
        });
        if let Some(k) = remove { v.list.remove(k); }
        if add {
            let n = (1..).map(|i| format!("VAR{i}")).find(|n| !v.list.iter().any(|x| x.name == *n)).unwrap_or_default();
            v.list.push(vars::Variable::new(&n, ""));
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new("$F2 to $F9 pad the frame as $F4 does. An operator's own are written in braces ({usd} in a Render Command); the environment's ($HOME…) come last.").small().color(dim));
    });
    ctx.data_mut(|d| d.insert_temp(open_id(), open));
}

/// A section's title row in the table.
fn section(ui: &mut egui::Ui, title: &str, about: &str) {
    ui.label(egui::RichText::new(title).strong());
    ui.label(egui::RichText::new(about).small().color(crate::properties::form::ink::DIM()));
    ui.label("");
    ui.end_row();
}

/// After a text field (`r`, over `text`): while a variable's name is being
/// typed in it (`$`, `$GD`, `${F`; `{`, `{us`), the variables it can take,
/// with their values, below it; a click puts the name in. After `$`: the
/// graph's own, the built-ins, the environment's. After `{`: `locals`, the
/// operator's own (name in lower case, what it is, value). Returns whether
/// `text` changed.
pub fn complete(ui: &egui::Ui, r: &egui::Response, text: &mut String, locals: &[(&str, &str, String)]) -> bool {
    let ctx = ui.ctx().clone();
    let popup = r.id.with("variables_list");
    // A press on the list takes the focus from the field: the list stays
    // while the pointer is over where it was.
    let kept: Option<egui::Rect> = ctx.data(|d| d.get_temp(popup));
    let over = kept.is_some_and(|rect| ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p)));
    let close = |ctx: &egui::Context| ctx.data_mut(|d| d.remove::<egui::Rect>(popup));
    if !r.has_focus() && !over { if kept.is_some() { close(&ctx); } return false; }
    let Some(state) = egui::TextEdit::load_state(&ctx, r.id) else { return false };
    let Some(range) = state.cursor.char_range() else { return false };
    let chars: Vec<char> = text.chars().collect();
    let cursor = range.primary.index.0.min(chars.len());
    // The name before the cursor, and what comes before it: `$`, `${`, or
    // `{` (an operator's own, when it has any).
    let start = chars[..cursor].iter().rposition(|c| !(c.is_ascii_alphanumeric() || *c == '_')).map_or(0, |k| k + 1);
    let opened = start >= 1 && chars[start - 1] == '{';
    let brace = opened && start >= 2 && chars[start - 2] == '$';
    let own = opened && !brace && !locals.is_empty();
    let dollar = !opened && start >= 1 && chars[start - 1] == '$' && !(start >= 2 && chars[start - 2] == '$');
    if !brace && !dollar && !own { if kept.is_some() { close(&ctx); } return false; }
    let typed: String = chars[start..cursor].iter().collect();
    let low = typed.to_lowercase();
    let mut rows: Vec<(String, String, String)> = if own {
        locals.iter().filter(|(n, _, _)| n.starts_with(&low)).map(|(n, what, v)| (n.to_string(), v.clone(), what.to_string())).collect()
    } else {
        vars::available(&[]).into_iter().filter(|(n, _, _)| n.to_lowercase().starts_with(&low)).collect()
    };
    // The environment's, once a letter is typed: there are many.
    if !own && !typed.is_empty() {
        let more: Vec<(String, String, String)> = vars::environment("").into_iter()
            .filter(|(n, _)| n.to_lowercase().starts_with(&low) && !rows.iter().any(|r| r.0 == *n))
            .take(8).map(|(n, v)| (n, v, "from the environment".into())).collect();
        rows.extend(more);
    }
    rows.truncate(16);
    // Nothing to offer, or the name typed in full.
    if rows.is_empty() || (rows.len() == 1 && rows[0].0 == typed) { if kept.is_some() { close(&ctx); } return false; }

    let mut picked: Option<String> = None;
    let width = r.rect.width().max(280.0);
    let area = egui::Area::new(popup).order(egui::Order::Foreground).fixed_pos(r.rect.left_bottom() + egui::vec2(0.0, 2.0)).show(&ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_max_width(width);
            for (name, value, what) in &rows {
                ui.horizontal(|ui| {
                    let shown = if own { format!("{{{name}}}") } else { format!("${name}") };
                    if ui.selectable_label(false, egui::RichText::new(shown).monospace()).on_hover_text(what.as_str()).clicked() {
                        picked = Some(name.clone());
                    }
                    let short: String = if value.chars().count() > 60 { format!("{}…", value.chars().take(60).collect::<String>()) } else { value.clone() };
                    ui.add(egui::Label::new(egui::RichText::new(short).small().color(crate::properties::form::ink::DIM())).truncate());
                });
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(popup, area.response.rect));
    let Some(name) = picked else { return false };
    // The name typed so far becomes the one picked; `${` and `{` get their `}`.
    let mut out: String = chars[..start].iter().collect();
    out.push_str(&name);
    let mut at = start + name.chars().count();
    let mut rest = &chars[cursor..];
    if brace || own {
        out.push('}');
        at += 1;
        if rest.first() == Some(&'}') { rest = &rest[1..]; }
    }
    out.extend(rest.iter());
    *text = out;
    let mut state = state;
    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
    state.store(&ctx, r.id);
    ctx.memory_mut(|m| m.request_focus(r.id));
    close(&ctx);
    true
}
