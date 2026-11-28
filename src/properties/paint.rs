//! The Paint operator's Parameters: its channels, the brush, its file.

use super::form::group;
use crate::core::paint::{self, Channel, ChannelKind, Op, PaintData};

/// The colour of an ID in the Viewer and here: 0 (none) grey, then a
/// palette that tells neighbours apart.
pub fn id_colour(id: i32) -> egui::Color32 {
    const PALETTE: [[u8; 3]; 8] = [
        [0x7F, 0xB8, 0x5E], // green
        [0x3F, 0x7F, 0x4A], // dark green
        [0x9A, 0x96, 0x8E], // stone
        [0xD8, 0xB2, 0x4A], // ochre
        [0x5E, 0x8C, 0xC8], // blue
        [0xB8, 0x6A, 0x9E], // plum
        [0xC8, 0x6E, 0x4A], // rust
        [0x6E, 0xC0, 0xB8], // teal
    ];
    if id <= 0 {
        return egui::Color32::from_gray(70);
    }
    let [r, g, b] = PALETTE[(id as usize - 1) % PALETTE.len()];
    egui::Color32::from_rgb(r, g, b)
}

/// The brush's ops, in the order of their square buttons.
pub const TOOLS: [Op; 6] = [Op::Add, Op::Subtract, Op::Set, Op::Smooth, Op::Flood, Op::Erase];

pub fn icon(op: Op) -> &'static str {
    use crate::icons as i;
    match op {
        Op::Add => i::PLUS,
        Op::Subtract => i::MINUS,
        Op::Set => i::EQUALS,
        Op::Smooth => i::WAVE_SINE,
        Op::Erase => i::ERASER,
        Op::Flood => i::PAINT_BUCKET,
    }
}

pub fn hint(op: Op) -> &'static str {
    match op {
        Op::Add => "Add: raise the values under the brush towards 1",
        Op::Subtract => "Subtract: lower the values under the brush towards 0",
        Op::Set => "Set: the values under the brush towards the Value (an ID: to its ID)",
        Op::Smooth => "Smooth: the values towards the average around them (floats only). Shift while painting smooths too",
        Op::Erase => "Erase: back to the channel's default",
        Op::Flood => "Flood: click a prim to set all of it to the Value (an ID: its ID), within the slope limit",
    }
}

/// Smooth has no meaning on IDs.
pub fn enabled(op: Op, kind: Option<ChannelKind>) -> bool {
    !(op == Op::Smooth && kind == Some(ChannelKind::Id))
}

/// A square button: an icon, and its label under it when `label` is
/// given. Used for the brush in the Parameters and in the Viewer.
pub fn square(ui: &mut egui::Ui, icon: &str, label: Option<&str>, selected: bool, enabled: bool, size: egui::Vec2) -> egui::Response {
    let r = ui.add_enabled(enabled, egui::Button::new("").selected(selected).min_size(size));
    let visuals = ui.style().interact_selectable(&r, selected);
    let mut ink = visuals.text_color();
    if !enabled {
        ink = ink.gamma_multiply(0.4);
    }
    let p = ui.painter();
    let rect = r.rect;
    match label {
        Some(text) => {
            p.text(rect.center() - egui::vec2(0.0, 7.0), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(16.0), ink);
            p.text(rect.center_bottom() - egui::vec2(0.0, 4.0), egui::Align2::CENTER_BOTTOM, text, egui::FontId::proportional(10.5), ink);
        }
        None => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, icon, egui::FontId::proportional(16.0), ink);
        }
    }
    r
}

/// The brush's ops as square icon buttons, laid out by the caller's `ui`.
pub fn tool_buttons(ui: &mut egui::Ui, b: &mut paint::Brush, kind: Option<ChannelKind>, size: egui::Vec2) {
    for op in TOOLS {
        if square(ui, icon(op), None, paint::brush_on() && b.op == op, enabled(op, kind), size)
            .on_hover_text(hint(op))
            .clicked()
        {
            b.op = op;
            paint::set_brush_on(true);
        }
    }
}

/// A swatch of an ID's colour.
fn swatch(ui: &mut egui::Ui, id: i32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(11.0, 11.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, id_colour(id));
    ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(60)), egui::StrokeKind::Inside);
}

pub fn rows(ui: &mut egui::Ui, channels: &mut Vec<Channel>, active: &mut usize, data: &mut PaintData) {
    // Channels: the active one is what the brush paints and the Viewer shows.
    group(
        ui,
        "Channels",
        Some("A point attribute each, on every prim painted: a float from 0 to 1, or an ID (a whole number). Click one to paint it"),
        |f| {
            let mut remove = None;
            let mut renamed: Option<(String, String)> = None;
            for (i, c) in channels.iter_mut().enumerate() {
                f.wide(|ui| {
                    ui.horizontal(|ui| {
                        if ui.radio(*active == i, "").on_hover_text("Paint this channel").clicked() {
                            *active = i;
                        }
                        let before = c.name.clone();
                        let r = ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(96.0));
                        if r.lost_focus() && c.name != before && !c.name.trim().is_empty() {
                            renamed = Some((before, c.name.clone()));
                        } else if r.lost_focus() && c.name.trim().is_empty() {
                            c.name = before;
                        }
                        egui::ComboBox::from_id_salt(("paint_kind", i))
                            .width(56.0)
                            .selected_text(match c.kind {
                                ChannelKind::Float => "Float",
                                ChannelKind::Id => "ID",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut c.kind, ChannelKind::Float, "Float");
                                ui.selectable_value(&mut c.kind, ChannelKind::Id, "ID");
                            });
                        if ui
                            .small_button(crate::icons::X)
                            .on_hover_text("Remove the channel (what was painted on it is kept until saved)")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                    });
                    if c.kind == ChannelKind::Id {
                        // Its IDs' names, from 1, with their colours.
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(24.0);
                            for (k, l) in c.labels.iter().enumerate() {
                                swatch(ui, k as i32 + 1);
                                ui.label(egui::RichText::new(format!("{} {l}", k + 1)).small());
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            let mut text = c.labels.join(", ");
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut text)
                                        .desired_width(200.0)
                                        .hint_text("names of IDs 1, 2, 3…"),
                                )
                                .changed()
                            {
                                c.labels = text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                            }
                        });
                    }
                });
            }
            if let Some(i) = remove {
                channels.remove(i);
                *active = (*active).min(channels.len().saturating_sub(1));
            }
            if let Some((from, to)) = renamed {
                // What was painted goes with the name.
                for p in data.prims.values_mut() {
                    if let Some(v) = p.values.remove(&from) {
                        p.values.insert(to.clone(), v);
                    }
                }
                data.rehash();
            }
            f.wide(|ui| {
                ui.horizontal(|ui| {
                    let mut add = |kind: ChannelKind, base: &str| {
                        let mut n = 1;
                        let mut name = base.to_string();
                        while channels.iter().any(|c| c.name == name) {
                            n += 1;
                            name = format!("{base}{n}");
                        }
                        channels.push(match kind {
                            ChannelKind::Float => Channel::float(&name),
                            ChannelKind::Id => Channel::id(&name, &[]),
                        });
                        *active = channels.len() - 1;
                    };
                    if ui.button(format!("{} Float", crate::icons::PLUS)).clicked() {
                        add(ChannelKind::Float, "weight");
                    }
                    if ui.button(format!("{} ID", crate::icons::PLUS)).clicked() {
                        add(ChannelKind::Id, "region");
                    }
                });
            });
        },
    );

    let channel = channels.get(*active).cloned();
    let mut b = paint::brush();
    let before = b;
    group(
        ui,
        "Brush",
        Some("Paint in the Viewer with the left button over a prim, with this operator selected. The brush is the Viewer's, not saved with the graph"),
        |f| {
            f.buttons("", None, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                tool_buttons(ui, &mut b, channel.as_ref().map(|c| c.kind), egui::vec2(24.0, 22.0));
            });
            f.drag("Radius", Some("In the scene's units. [ and ] in the Viewer change it"), &mut b.radius, 0.01, "");
            b.radius = b.radius.max(0.001);
            f.slider("Strength", Some("How much of the change one dab makes"), &mut b.strength, 0.0..=1.0, "");
            match channel.as_ref().map(|c| c.kind) {
                Some(ChannelKind::Id) => {
                    let c = channel.as_ref().unwrap();
                    f.row("ID", Some("The ID Set paints"), |ui| {
                        ui.horizontal(|ui| {
                            let mut id = b.value.round().max(0.0) as i32;
                            swatch(ui, id);
                            let name = c.labels.get(id as usize - 1).cloned().unwrap_or_default();
                            egui::ComboBox::from_id_salt("paint_id")
                                .selected_text(format!("{id} {name}"))
                                .show_ui(ui, |ui| {
                                    for k in 1..=(c.labels.len().max(1) as i32 + 1) {
                                        let n = c.labels.get(k as usize - 1).cloned().unwrap_or_default();
                                        ui.selectable_value(&mut id, k, format!("{k} {n}"));
                                    }
                                });
                            b.value = id as f32;
                        });
                    });
                }
                _ => {
                    f.slider("Value", Some("The value Set paints and Flood fills with"), &mut b.value, 0.0..=1.0, "");
                }
            }
            f.toggle("Mirror X", Some("Also on the other side of the prim's own X = 0"), &mut b.mirror);
            f.slider(
                "Slope Limit",
                Some("Only where the surface is no steeper than this: 90° paints everywhere"),
                &mut b.max_slope,
                0.0..=90.0,
                "°",
            );
        },
    );
    if b != before {
        paint::set_brush(b);
    }

    group(ui, "File", Some("The values are kept beside the graph, in a file named by what it holds"), |f| {
        let points: usize = data.prims.values().map(|p| p.rest.len()).sum();
        match data.file_name() {
            Some(name) => {
                f.value("Painted", format!("{} prims, {points} points", data.prims.len()));
                f.value("File", name);
            }
            None => f.value("Painted", "nothing yet"),
        }
        if data.missing {
            f.status(
                super::form::Status::Bad,
                "Its file is not beside the graph: what was painted is lost",
            );
        }
    });
}
