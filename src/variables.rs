//! Variables in the interface (see `core::vars` for what they are):
//!
//! - **File > Variables**: the graph's own variables and `$JOB`, saved with
//!   it, and the built-in ones with what they are now.
//! - **The list on `$`**: typing `$` (or `${`) in a path or command line
//!   lists the variables it can take, with their values; a click puts one
//!   in.

use crate::core::vars;
use crate::properties::form::FieldExt;

fn open_id() -> egui::Id { egui::Id::new("variables_window_open") }

/// Show File > Variables.
pub fn open(ctx: &egui::Context) { ctx.data_mut(|d| d.insert_temp(open_id(), true)); }

/// File > Variables, while it is open. Changes apply at once and are steps
/// of the history, as any change to the graph.
pub fn window(ctx: &egui::Context, v: &mut vars::Variables) {
    let mut open = ctx.data(|d| d.get_temp::<bool>(open_id())).unwrap_or(false);
    if !open { return; }
    let dim = crate::properties::form::ink::DIM();
    let bad = crate::properties::form::ink::BAD();
    egui::Window::new("Variables").open(&mut open).collapsible(false).resizable(true).default_width(560.0).show(ctx, |ui| {
        ui.label(egui::RichText::new("Saved with the graph. Write them as $NAME in paths and command lines; a value can hold variables too ($JOB/assets). Type $ in a field for the list.").small().color(dim));
        ui.add_space(6.0);
        let name_w = 140.0;
        let value_w = (ui.available_width() - name_w - 60.0).max(160.0);
        // $JOB: set here, else the environment's, else the graph's folder.
        ui.horizontal(|ui| {
            ui.add_sized([name_w, ui.spacing().interact_size.y], egui::Label::new(egui::RichText::new("JOB").monospace()))
                .on_hover_text("The project folder. Empty: JOB from the environment, else the graph's folder ($GDIR)");
            let hint = std::env::var("JOB").map(|j| format!("{j} (from the environment)")).unwrap_or_else(|_| "the graph's folder".into());
            let r = ui.field(egui::TextEdit::singleline(&mut v.job).hint_text(hint).desired_width(value_w));
            complete(ui, &r, &mut v.job, &[]);
        });
        shown(ui, name_w, &vars::expand("$JOB", &[]));
        ui.separator();

        let mut remove = None;
        let names: Vec<String> = v.list.iter().map(|(n, _)| n.clone()).collect();
        for (k, (name, value)) in v.list.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let h = ui.spacing().interact_size.y;
                ui.field_sized([name_w, h], egui::TextEdit::singleline(name).hint_text("NAME").font(egui::TextStyle::Monospace));
                let r = ui.field(egui::TextEdit::singleline(value).hint_text("value").desired_width(value_w));
                complete(ui, &r, value, &[]);
                if ui.small_button("🗑").on_hover_text("Remove this variable").clicked() { remove = Some(k); }
            });
            let taken = names.iter().take(k).any(|n| n.as_str() == name.as_str());
            match vars::check_name(name) {
                Err(e) => { ui.label(egui::RichText::new(format!("${name}: {e}")).small().color(bad)); }
                Ok(()) if taken => { ui.label(egui::RichText::new(format!("${name}: a variable above has this name, and is the one used")).small().color(bad)); }
                Ok(()) => shown(ui, name_w, &vars::expand(&format!("${{{name}}}"), &[])),
            }
        }
        if let Some(k) = remove { v.list.remove(k); }
        if ui.button("+ Variable").clicked() {
            let n = (1..).map(|i| format!("VAR{i}")).find(|n| !v.list.iter().any(|(x, _)| x == n)).unwrap_or_default();
            v.list.push((n, String::new()));
        }

        ui.add_space(6.0);
        egui::CollapsingHeader::new("Built-in").default_open(false).show(ui, |ui| {
            egui::Grid::new("variables_builtin").num_columns(3).spacing([12.0, 3.0]).show(ui, |ui| {
                for (name, what) in vars::BUILTINS {
                    ui.label(egui::RichText::new(format!("${name}")).monospace());
                    ui.label(vars::expand(&format!("${{{name}}}"), &[]));
                    ui.label(egui::RichText::new(what).small().color(dim));
                    ui.end_row();
                }
            });
            ui.label(egui::RichText::new("$F2 to $F9 pad the frame likewise. Then anything set in the environment ($HOME…), after the graph's own. An operator's own come first ($USD, $OPNAME in a Render Command).").small().color(dim));
        });
    });
    ctx.data_mut(|d| d.insert_temp(open_id(), open));
}

/// What a variable is now, under its row.
fn shown(ui: &mut egui::Ui, indent: f32, value: &str) {
    ui.horizontal(|ui| {
        ui.add_space(indent + ui.spacing().item_spacing.x);
        let text = if value.is_empty() { "(empty)".to_string() } else { format!("→ {value}") };
        ui.label(egui::RichText::new(text).small().color(crate::properties::form::ink::DIM()));
    });
}

/// After a text field (`r`, over `text`): while a variable's name is being
/// typed in it (`$`, `$GD`, `${F`), the variables it can take, with their
/// values, below it; a click puts the name in. `locals`: the reader's own
/// (name, what it is, value), listed first. Returns whether `text` changed.
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
    // The name before the cursor, and what comes before it: `$` or `${`.
    let start = chars[..cursor].iter().rposition(|c| !(c.is_ascii_alphanumeric() || *c == '_')).map_or(0, |k| k + 1);
    let brace = start >= 2 && chars[start - 1] == '{' && chars[start - 2] == '$';
    let dollar = !brace && start >= 1 && chars[start - 1] == '$' && !(start >= 2 && chars[start - 2] == '$');
    if !brace && !dollar { if kept.is_some() { close(&ctx); } return false; }
    let typed: String = chars[start..cursor].iter().collect();
    let low = typed.to_lowercase();
    let mut rows: Vec<(String, String, String)> = vars::available(locals).into_iter()
        .filter(|(n, _, _)| n.to_lowercase().starts_with(&low)).collect();
    // The environment's, once a letter is typed: there are many.
    if !typed.is_empty() {
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
                    if ui.selectable_label(false, egui::RichText::new(format!("${name}")).monospace()).on_hover_text(what.as_str()).clicked() {
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
    // The name typed so far becomes the one picked; `${` gets its `}`.
    let mut out: String = chars[..start].iter().collect();
    out.push_str(&name);
    let mut at = start + name.chars().count();
    let mut rest = &chars[cursor..];
    if brace {
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
