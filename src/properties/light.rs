//! The Light operator's parameters: where it is, its type, and its UsdLux
//! values, laid out from `assets/usdlux.json` (see `core::light`).

use crate::core::light::{Light, LightType, DEFAULT_LOCATION};
use crate::core::properties::Value;
use crate::core::render::Param;

use super::form::{group, FieldExt, Rows, Status};

/// The images a light's file can be: what the Viewer and renderers read.
const TEXTURES: &[&str] = &["exr", "hdr", "png", "jpg", "jpeg", "tif", "tiff", "tga"];

/// The Type menu: the six UsdLux types and Point, a sphere treated as one.
#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Kind(LightType),
    Point,
}

pub fn light_rows(
    ui: &mut egui::Ui,
    location: &mut String,
    translate: &mut [f32; 3],
    rotate: &mut [f32; 3],
    light: &mut Light,
    io: &mut super::ui::PanelIo,
    node: crate::types::NodeId,
) {
    let path = crate::types::prim_location(location, DEFAULT_LOCATION);
    group(
        ui,
        "Primitive",
        Some("Where the light is in the scene. Its parents are Xforms"),
        |f| {
            f.text_vars(
                "Primitive Location",
                Some("A USD path, such as /lights/key. Variables are put in: /lights/$OPNAME"),
                location,
                DEFAULT_LOCATION,
                false,
            );
            if path != location.trim() {
                f.status(Status::Info, format!("Used as {path}"));
            }
        },
    );
    group(
        ui,
        "Type",
        Some("UsdLux's light types. A point light is a sphere light treated as a point"),
        |f| {
            let now = if light.is_point() {
                Pick::Point
            } else {
                Pick::Kind(light.kind)
            };
            let mut pick = now;
            let options: Vec<(Pick, &str)> = [
                (Pick::Kind(LightType::Dome), "Dome"),
                (Pick::Kind(LightType::Rect), "Rect"),
                (Pick::Point, "Point"),
                (Pick::Kind(LightType::Sphere), "Sphere"),
                (Pick::Kind(LightType::Disk), "Disk"),
                (Pick::Kind(LightType::Cylinder), "Cylinder"),
                (Pick::Kind(LightType::Distant), "Distant"),
            ]
            .into_iter()
            .collect();
            f.row("Type", None, |ui| {
                let label = options.iter().find(|(p, _)| *p == now).map(|(_, l)| *l).unwrap_or("");
                egui::ComboBox::from_id_salt("light_type")
                    .width(ui.available_width() - 8.0)
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (p, l) in &options {
                            if ui.selectable_label(*p == now, *l).clicked() {
                                pick = *p;
                            }
                        }
                    });
            });
            if pick != now {
                match pick {
                    Pick::Point => {
                        light.set_kind(LightType::Sphere);
                        light.set("treatAsPoint", Value::Bool(true));
                    }
                    Pick::Kind(LightType::Sphere) => {
                        light.set_kind(LightType::Sphere);
                        light.set("treatAsPoint", Value::Bool(false));
                    }
                    Pick::Kind(k) => light.set_kind(k),
                }
            }
            f.value("USD", light.kind.usd());
        },
    );
    let dome = light.kind == LightType::Dome;
    let hint = if dome {
        "A dome is all around: where it is does not matter, its rotation turns the environment"
    } else {
        "It looks down its -Z axis, with +Y up. A Transform after it moves it too"
    };
    group(ui, "Transform", Some(hint), |f| {
        f.xyz("Translate", None, translate, 0.05, "");
        f.xyz("Rotate", Some("Degrees, X then Y then Z"), rotate, 0.5, "°");
    });
    // Its values, by the definitions' groups, in their order.
    let params = light.params();
    let mut groups: Vec<&str> = vec![];
    for p in &params {
        if !groups.contains(&p.group.as_str()) {
            groups.push(&p.group);
        }
    }
    for g in groups {
        group(ui, g, None, |f| {
            for p in params.iter().filter(|p| p.group == g) {
                param_row(f, light, *p, io, node);
            }
        });
    }
    let set = light.values.len();
    group(ui, "Defaults", None, |f| {
        f.buttons("Reset", Some("Every value back to the schema's"), |ui| {
            if ui
                .add_enabled(set > 0, egui::Button::new("Reset to the schema's"))
                .clicked()
            {
                // A point light stays one.
                let point = light.is_point();
                light.values.clear();
                if point {
                    light.set("treatAsPoint", Value::Bool(true));
                }
            }
        });
        f.status(
            Status::Info,
            if set == 0 {
                "Every value is UsdLux's fallback; all are written".to_string()
            } else {
                format!("{set} set; all are written")
            },
        );
    });
}

/// One value: set when it differs from the schema's, dropped when not.
fn param_row(
    f: &mut Rows,
    light: &mut Light,
    p: &'static Param,
    io: &mut super::ui::PanelIo,
    node: crate::types::NodeId,
) {
    let fallback = light.fallback(p);
    let light_label = light.label();
    let mut v = light.value(&p.name).unwrap_or_else(|| fallback.clone());
    let before = v.clone();
    let mut hint = String::new();
    if !p.help.is_empty() {
        hint.push_str(&p.help);
        hint.push_str("\n\n");
    }
    hint.push_str(&p.name);
    if v != fallback {
        hint.push_str(&format!("\nUsdLux's: {}", fallback.display()));
    }
    let label = if v != fallback {
        format!("{} •", p.shown())
    } else {
        p.shown().to_string()
    };
    match &mut v {
        Value::Token(t) if !p.choices.is_empty() => {
            let mut choices: Vec<(String, &str)> = p.choices.iter().map(|c| (c.clone(), c.as_str())).collect();
            if !p.choices.contains(t) {
                choices.push((t.clone(), t.as_str()));
            }
            let mut picked = t.clone();
            f.choice(&label, Some(hint.as_str()), &mut picked, &choices);
            *t = picked;
        }
        Value::Bool(b) => {
            f.toggle(&label, Some(hint.as_str()), b);
        }
        Value::Asset(s) => {
            f.row(&label, Some(hint.as_str()), |ui| {
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - if s.is_empty() { 34.0 } else { 62.0 }).max(40.0);
                    let r = ui.field(
                        egui::TextEdit::singleline(s)
                            .hint_text("/path/to/image.exr")
                            .desired_width(w),
                    );
                    crate::variables::complete(ui, &r, s, &[]);
                    if ui.button("📂").on_hover_text("Browse").clicked() {
                        io.browser.open(
                            crate::file_browser::BrowseTarget::LightFile(node, p.name.as_str()),
                            crate::file_browser::BrowseMode::File,
                            &format!("{} for {}", p.shown(), light_label),
                            TEXTURES,
                            "",
                        );
                    }
                    if !s.is_empty() && ui.button("✕").on_hover_text("No file").clicked() {
                        s.clear();
                    }
                });
            });
            if !s.trim().is_empty() && !crate::core::sequences::texture_exists(s) {
                f.status(Status::Bad, "Not found");
            }
        }
        other => {
            f.row(&label, Some(hint.as_str()), |ui| {
                super::render::value_field(ui, other, p.min.zip(p.max))
            });
        }
    }
    if v != before {
        light.set(&p.name, v);
    }
}
