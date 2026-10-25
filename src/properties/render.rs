//! The render operators' parameters: Render Commons, Render Settings
//! (a renderer's settings from its definition, and its AOVs), Render
//! Variables. See `core::render`.

use crate::core::properties::{Value, ValueType};
use crate::core::render::{self, Context, Definition, Extra, Param, RendererSettings, SceneSettings, VarRow, Variables, CUSTOM};
use crate::types::NodeId;

use super::form::{self, group, group_with, ink, FieldExt, Rows, Status};

// ── Shared ─────────────────────────────────────────────────────────────────

/// A value's field, filling the value column. `range`: the lowest and
/// highest a number may be. Whether it changed.
pub(super) fn value_field(ui: &mut egui::Ui, v: &mut Value, range: Option<(f64, f64)>) -> bool {
    let h = ui.spacing().interact_size.y;
    let w = ui.available_width();
    let speed = |r: Option<(f64, f64)>| r.map_or(0.01, |(a, b)| ((b - a) / 300.0).clamp(0.001, 10.0));
    let fields = |ui: &mut egui::Ui, v: &mut [f32]| -> bool {
        ui.horizontal(|ui| {
            let gap = ui.spacing().item_spacing.x;
            let n = v.len() as f32;
            let w = ((ui.available_width() - (n - 1.0) * gap) / n).max(30.0);
            let mut changed = false;
            for x in v.iter_mut() { changed |= ui.field_sized([w, h], egui::DragValue::new(x).speed(0.01).max_decimals(4)).changed(); }
            changed
        }).inner
    };
    match v {
        Value::Bool(b) => form::check(ui, b).changed(),
        Value::Int(i) => {
            let mut d = egui::DragValue::new(i).speed(0.1);
            if let Some((a, b)) = range { d = d.range(a.round() as i32..=b.round() as i32); }
            ui.field_sized([w, h], d).changed()
        }
        Value::Float(x) => {
            let mut d = egui::DragValue::new(x).speed(speed(range)).max_decimals(4);
            if let Some((a, b)) = range { d = d.range(a as f32..=b as f32); }
            ui.field_sized([w, h], d).changed()
        }
        Value::Double(x) => {
            let mut d = egui::DragValue::new(x).speed(speed(range)).max_decimals(6);
            if let Some((a, b)) = range { d = d.range(a..=b); }
            ui.field_sized([w, h], d).changed()
        }
        Value::String(s) | Value::Token(s) | Value::Asset(s) => {
            let r = ui.field(egui::TextEdit::singleline(&mut *s).desired_width(w));
            r.changed() | crate::variables::complete(ui, &r, s, &[])
        }
        Value::Float2(a) => fields(ui, a),
        Value::Float3(a) => fields(ui, a),
        Value::Float4(a) => fields(ui, a),
        Value::Color3f(c) => ui.horizontal(|ui| {
            let picked = ui.color_edit_button_rgb(c).changed();
            fields(ui, c) | picked
        }).inner,
    }
}

/// Rows of settings typed in: a name (put under `namespace` when it has
/// none), a type and a value each, to remove; and a button to add one.
/// `presets`: names to pick from, with a value and what they are.
fn extra_rows(ui: &mut egui::Ui, title: &str, hint: &str, extras: &mut Vec<Extra>, namespace: Option<&str>, presets: &[(&str, Value, &str)]) {
    let mut remove = None;
    group(ui, title, Some(hint), |f| {
        for (k, e) in extras.iter_mut().enumerate() {
            if k > 0 { f.ui.separator(); }
            let id = f.ui.id().with(("extra", k));
            let name_hint = match namespace {
                Some(ns) if !ns.is_empty() => format!("Written as {ns}:name, unless the name has its own namespace (a:b)"),
                _ => "The attribute's full name, namespace and all (driver:parameters:aov:format)".to_string(),
            };
            f.row("Name", Some(name_hint.as_str()), |ui| {
                ui.horizontal(|ui| {
                    let menu_w = if presets.is_empty() { 0.0 } else { 44.0 };
                    let w = (ui.available_width() - 30.0 - menu_w).max(60.0);
                    let hint = match namespace { Some(ns) if !ns.is_empty() => format!("{ns}:…"), _ => "name".into() };
                    ui.field(egui::TextEdit::singleline(&mut e.name).desired_width(w).hint_text(hint).font(egui::TextStyle::Monospace));
                    if !presets.is_empty() {
                        crate::menus::menu(ui, "Pick", |ui| {
                            for (name, value, about) in presets {
                                if ui.selectable_label(e.name == *name, *name).on_hover_text(*about).clicked() {
                                    e.name = name.to_string();
                                    e.value = value.clone();
                                    ui.close();
                                }
                            }
                        });
                    }
                    if ui.small_button("🗑").on_hover_text("Remove this setting").clicked() { remove = Some(k); }
                });
            });
            let options: Vec<(ValueType, &str)> = ValueType::ALL.iter().map(|t| (*t, t.usd())).collect();
            let mut ty = e.value.kind();
            f.row("Type", None, |ui| {
                if form::choice_widget(ui, id.with("type"), &mut ty, &options) { e.value = ty.convert(&e.value); }
            });
            f.row("Value", None, |ui| { value_field(ui, &mut e.value, None); });
        }
        f.buttons("Add", None, |ui| {
            if ui.button("+ Setting").clicked() { extras.push(Extra { name: String::new(), value: Value::Int(0) }); }
        });
    });
    if let Some(k) = remove { extras.remove(k); }
}

/// A path field with a menu of paths to pick from, and what an empty one means.
fn path_pick(f: &mut Rows, label: &str, hint: &str, value: &mut String, placeholder: &str, choices: &[String]) {
    f.row(label, Some(hint), |ui| {
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 48.0).max(60.0);
            ui.field(egui::TextEdit::singleline(value).desired_width(w).hint_text(placeholder));
            ui.add_enabled_ui(!choices.is_empty(), |ui| {
                crate::menus::menu(ui, "Pick", |ui| {
                    for c in choices {
                        if ui.selectable_label(value.trim() == c, c.as_str()).clicked() { *value = c.clone(); ui.close(); }
                    }
                });
            });
        });
    });
}

// ── Render Commons ───────────────────────────────────────────────────

pub fn scene_settings_rows(ui: &mut egui::Ui, s: &mut SceneSettings, ctx: &Context) {
    let path = s.path();
    group(ui, "Primitive", Some("The RenderSettings prim. Render Settings and Render Variables find it without a wire: the layer names it (renderSettingsPrimPath)"), |f| {
        f.text_vars("Primitive Location", Some("A USD path, such as /Render/Settings. Variables are put in"), &mut s.location, "/Render/Settings", false);
        if path != s.location.trim() { f.status(Status::Info, format!("Used as {path}")); }
    });
    group(ui, "Camera", Some("The camera rendered through (the camera relationship)"), |f| {
        path_pick(f, "Camera", "Its path. Empty: the first camera coming in", &mut s.camera, "first camera", &ctx.cameras);
        if s.camera.trim().is_empty() {
            match ctx.cameras.first() {
                Some(c) => f.status(Status::Info, format!("The first camera coming in: {c}")),
                None => f.status(Status::Bad, "No camera coming in: wire a scene with one, or type its path"),
            }
        } else {
            let c = crate::types::prim_location(&s.camera, "/camera");
            if !ctx.cameras.contains(&c) { f.status(Status::Info, format!("{c} is not among the cameras coming in")); }
        }
    });
    group(ui, "Image", Some("What the image is: resolution, pixel aspect, how it fits the camera's gate, and overscan"), |f| {
        f.row("Presets", None, |ui| {
            let current = render::RESOLUTIONS.iter().find(|(_, r)| *r == s.resolution).map(|(n, _)| *n).unwrap_or("Custom");
            egui::ComboBox::from_id_salt("render_resolution").width(ui.available_width() - 8.0).selected_text(current).show_ui(ui, |ui| {
                for (name, r) in render::RESOLUTIONS {
                    if ui.selectable_label(current == name, format!("{name}   {} × {}", r[0], r[1])).clicked() { s.resolution = r; }
                }
            });
        });
        f.row("Resolution", Some("Width and height, in pixels"), |ui| {
            ui.horizontal(|ui| {
                let h = ui.spacing().interact_size.y;
                let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(30.0);
                ui.field_sized([w, h], egui::DragValue::new(&mut s.resolution[0]).speed(1.0).range(1..=65536).prefix("W "));
                ui.field_sized([w, h], egui::DragValue::new(&mut s.resolution[1]).speed(1.0).range(1..=65536).prefix("H "));
            });
        });
        f.drag("Pixel Aspect", Some("The width of a pixel over its height: 1 for square pixels"), &mut s.pixel_aspect, 0.01, "");
        let options: Vec<(String, &str)> = render::CONFORM.iter().map(|(t, l)| (t.to_string(), *l)).collect();
        f.choice("Aspect Conform", Some("When the image's aspect is not the camera gate's: grow the gate, crop it, change its width or height, or the pixel aspect (aspectRatioConformPolicy)"), &mut s.conform, &options);
        f.drag("Overscan", Some("How much is rendered beyond the frame on every side, as a share of it: the data window (dataWindowNDC) runs from -overscan to 1 + overscan"), &mut s.overscan, 0.005, "");
        s.overscan = s.overscan.clamp(0.0, 1.0);
        if s.overscan > 0.0 {
            let o = s.overscan;
            f.value("Data window", format!("({:.3}, {:.3}) to ({:.3}, {:.3})   {} × {} pixels", -o, -o, 1.0 + o, 1.0 + o,
                (s.resolution[0] as f32 * (1.0 + 2.0 * o)).round(), (s.resolution[1] as f32 * (1.0 + 2.0 * o)).round()));
        }
    });
    group(ui, "Render", None, |f| {
        f.toggle("Motion Blur", Some("Off: disableMotionBlur, rendered at the shutter's open"), &mut s.motion_blur);
        f.toggle("Depth of Field", Some("Off: disableDepthOfField, everything in focus"), &mut s.depth_of_field);
        f.row("Purposes", Some("The purposes rendered (includedPurposes). Prims of the others are not"), |ui| {
            ui.horizontal_wrapped(|ui| {
                for p in render::PURPOSES {
                    let mut on = s.purposes.iter().any(|x| x == p);
                    if ui.checkbox(&mut on, p).changed() {
                        s.purposes.retain(|x| x != p);
                        if on { s.purposes.push(p.to_string()); }
                    }
                }
            });
        });
    });
    let product = s.product_path();
    group(ui, "Output", Some("The RenderProduct: one image file, holding the RenderVars (AOVs) Render Settings and Render Variables add"), |f| {
        f.text_vars("Product", Some("The RenderProduct's path. Variables are put in"), &mut s.product, "/Render/Products/beauty", false);
        if product != s.product.trim() { f.status(Status::Info, format!("Used as {product}")); }
        f.text_vars("Output Image", Some("The file it writes (productName). Imago's variables are put in ($JOB, $GNAME, $OPNAME); the frame's ($F, $F4) are left for the renderer to put in per frame (husk does)"), &mut s.output, "render/beauty.$F4.exr", true);
    });
}

// ── Render Settings ────────────────────────────────────────────────────────

pub fn renderer_rows(ui: &mut egui::Ui, r: &mut RendererSettings, ctx: &Context, node: NodeId) {
    let defs = render::definitions();
    let mut options: Vec<(String, String)> = defs.iter().map(|d| (d.name.clone(), d.label.clone())).collect();
    options.push((CUSTOM.to_string(), "Custom".to_string()));
    let options: Vec<(String, &str)> = options.iter().map(|(k, l)| (k.clone(), l.as_str())).collect();
    let def = (r.renderer != CUSTOM).then(|| render::definition(&r.renderer)).flatten();

    group(ui, "Renderer", Some("Whose settings: a definition file each (Karma and Arnold come with Imago; more go in the renderers folder), or Custom: settings you name"), |f| {
        f.row("Renderer", None, |ui| {
            let current = options.iter().find(|(k, _)| *k == r.renderer).map(|(_, l)| *l).unwrap_or(r.renderer.as_str()).to_string();
            egui::ComboBox::from_id_salt(("renderer", node.0)).width(ui.available_width() - 8.0).selected_text(current).show_ui(ui, |ui| {
                for (key, label) in &options {
                    if ui.selectable_label(r.renderer == *key, *label).clicked() { r.renderer = key.clone(); }
                }
            });
        });
        if r.renderer == CUSTOM {
            f.text("Name", Some("The renderer's name: its settings are written under it (rray: spp is rray:spp)"), &mut r.custom_name, "type the renderer's name");
            let ns = r.namespace();
            if ns.is_empty() { f.status(Status::Bad, "No name yet: type the renderer's name (rray). Until then its settings are not written"); }
            else { f.status(Status::Ok, format!("Settings written as {ns}:name")); }
        } else if def.is_none() {
            f.status(Status::Bad, format!("No definition named \"{}\": its settings are kept, not written", r.renderer));
        }
        if let Some(d) = &def { if !d.about.is_empty() { f.status(Status::Info, d.about.clone()); } }
        for e in render::definition_errors() { f.status(Status::Bad, e.clone()); }
    });
    group(ui, "Primitive", Some("Where the settings go: the RenderSettings prim the scene names, else the first one coming in, else one made here"), |f| {
        match &ctx.settings {
            Some(found) => f.status(Status::Ok, found.describe()),
            None => {
                f.text_vars("Primitive Location", Some("Where the RenderSettings prim is made when none comes in. Variables are put in"), &mut r.location, "/Render/Settings", false);
                f.status(Status::Info, render::Found::Made(r.path()).describe());
            }
        }
    });

    // Settings and AOVs: one tab at a time.
    let tab_id = egui::Id::new(("render_settings_tab", node.0));
    let mut aovs_tab = ui.data(|d| d.get_temp::<bool>(tab_id)).unwrap_or(false);
    let ticked = def.as_ref().map(|d| r.ticked(d).len()).unwrap_or(0);
    ui.horizontal(|ui| {
        if ui.selectable_label(!aovs_tab, "Settings").clicked() { aovs_tab = false; }
        if ui.selectable_label(aovs_tab, format!("AOVs ({ticked})")).on_hover_text("The renderer's standard AOVs: those ticked become RenderVars of the product").clicked() { aovs_tab = true; }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, aovs_tab));
    ui.add_space(4.0);

    if aovs_tab { aov_rows(ui, r, def.as_deref(), ctx); } else { settings_rows(ui, r, def.as_deref()); }
}

/// The definition's settings by group, then those typed in.
fn settings_rows(ui: &mut egui::Ui, r: &mut RendererSettings, def: Option<&Definition>) {
    if let Some(def) = def {
        let mut groups: Vec<&str> = vec![];
        for p in &def.params { if !groups.contains(&p.group.as_str()) { groups.push(&p.group); } }
        for g in groups {
            group(ui, if g.is_empty() { "Settings" } else { g }, None, |f| {
                for p in def.params.iter().filter(|p| p.group == g) { param_row(f, r, p); }
            });
        }
        group(ui, "Defaults", None, |f| {
            let changed = def.params.iter().filter(|p| r.values.contains_key(&p.name)).count();
            f.buttons("Reset", Some("Every setting of this renderer back to its definition's default"), |ui| {
                if ui.add_enabled(changed > 0, egui::Button::new("Reset to defaults")).clicked() {
                    for p in &def.params { r.values.remove(&p.name); }
                }
            });
            f.status(Status::Info, if changed == 0 { "Every setting is its default; all are written".to_string() } else { format!("{changed} changed from the defaults; all are written") });
        });
    }
    let ns = r.namespace();
    let hint = if r.renderer == CUSTOM {
        "The renderer's settings, named as you like: spp, bounce… are written under its name (rray:spp), with the type and value given"
    } else {
        "Settings the definition does not list, written under the renderer's namespace unless the name has one"
    };
    let title = if r.renderer == CUSTOM { "Settings" } else { "Extra Settings" };
    extra_rows(ui, title, hint, r.extras_mut(), Some(ns.as_str()), &[]);
}

/// One setting of a definition: changed values are kept, defaults are not.
fn param_row(f: &mut Rows, r: &mut RendererSettings, p: &Param) {
    let mut v = r.value(p);
    let before = v.clone();
    let mut hint = String::new();
    if !p.help.is_empty() { hint.push_str(&p.help); hint.push_str("\n\n"); }
    hint.push_str(&p.name);
    if p.unconfirmed { hint.push_str("\n(Name not confirmed against the renderer: check with its list of properties)"); }
    let default = p.default_value();
    if v != default { hint.push_str(&format!("\nDefault: {}", default.display())); }
    let label = if v != default { format!("{} •", p.shown()) } else { p.shown().to_string() };
    match &mut v {
        Value::Token(t) if !p.choices.is_empty() => {
            let mut choices: Vec<(String, &str)> = p.choices.iter().map(|c| (c.clone(), c.as_str())).collect();
            if !p.choices.contains(t) { choices.push((t.clone(), t.as_str())); }
            let mut picked = t.clone();
            f.choice(&label, Some(hint.as_str()), &mut picked, &choices);
            *t = picked;
        }
        Value::Bool(b) => { f.toggle(&label, Some(hint.as_str()), b); }
        other => { f.row(&label, Some(hint.as_str()), |ui| value_field(ui, other, p.min.zip(p.max))); }
    }
    if v != before {
        if v == default { r.values.remove(&p.name); } else { r.values.insert(p.name.clone(), v); }
    }
}

/// The renderer's standard AOVs, by group, to tick.
fn aov_rows(ui: &mut egui::Ui, r: &mut RendererSettings, def: Option<&Definition>, ctx: &Context) {
    let Some(def) = def else {
        group(ui, "AOVs", None, |f| {
            f.status(Status::Info, "A Custom renderer has no standard AOVs: add its RenderVars with Render Variables");
        });
        return;
    };
    let ticked = r.ticked(def);
    group(ui, "Product", Some("The ticked AOVs become RenderVars beside the product (Products/Vars/<name>), in its orderedVars"), |f| {
        let product = ctx.settings.as_ref().and_then(|_| ctx.products.first().cloned());
        match product {
            Some(p) => f.status(Status::Info, format!("Added to {p}")),
            None => f.status(Status::Info, format!("Added to {}, made beside the RenderSettings prim", render::default_product(&r.path()))),
        }
        f.buttons("Tick", None, |ui| {
            if ui.button("All").clicked() { r.aovs.insert(def.name.clone(), def.aovs.iter().map(|a| a.name.clone()).collect()); }
            if ui.button("None").clicked() { r.aovs.insert(def.name.clone(), vec![]); }
            if ui.button("Defaults").on_hover_text("The beauty").clicked() { r.aovs.remove(&def.name); }
        });
    });
    let mut groups: Vec<&str> = vec![];
    for a in &def.aovs { if !groups.contains(&a.group.as_str()) { groups.push(&a.group); } }
    for g in groups {
        group(ui, if g.is_empty() { "AOVs" } else { g }, None, |f| {
            for a in def.aovs.iter().filter(|a| a.group == g) {
                let mut on = ticked.contains(&a.name);
                let label = if a.label.is_empty() { a.name.as_str() } else { a.label.as_str() };
                let mut hint = format!("RenderVar {}\nsourceName {}  ({})\ndataType {}", a.name, a.source_name, a.source_type, a.data_type);
                if !a.format.is_empty() { hint.push_str(&format!("\nformat {}", a.format)); }
                if a.unconfirmed { hint.push_str("\n(Source not confirmed against the renderer)"); }
                let changed = f.wide(|ui| ui.horizontal(|ui| {
                    let c = form::check(ui, &mut on).changed();
                    let l = ui.add(egui::Label::new(egui::RichText::new(label).color(ink::LABEL())).sense(egui::Sense::click()));
                    ui.label(egui::RichText::new(format!("{}  {}", a.name, a.data_type)).color(ink::DIM()).small());
                    let clicked = l.clicked();
                    l.on_hover_text(hint.as_str());
                    if clicked { on = !on; }
                    c || clicked
                }).inner);
                if changed { r.tick(def, &a.name, on); }
            }
        });
    }
}

// ── Render Variables ───────────────────────────────────────────────────────

pub fn variables_rows(ui: &mut egui::Ui, v: &mut Variables, ctx: &Context) {
    group(ui, "Product", Some("The RenderProduct whose RenderVars these are"), |f| {
        path_pick(f, "Product", "Its path. Empty: the RenderSettings prim's first product, else the first coming in", &mut v.product, "the scene's product", &ctx.products);
        if v.product.trim().is_empty() {
            match ctx.products.first() {
                Some(p) => f.status(Status::Info, format!("On {p}")),
                None => f.status(Status::Info, "No product coming in: makes /Render/Products/beauty"),
            }
        }
        f.toggle("Replace", Some("On: the product holds these RenderVars alone, in this order. Off: they are added to its own, replacing those of the same name"), &mut v.replace);
    });
    let mut remove = None;
    let mut up = None;
    for (k, row) in v.rows.iter_mut().enumerate() {
        let title = format!("{}  {}", k + 1, if row.name.trim().is_empty() { "(no name)" } else { row.name.trim() });
        group_with(ui, &title, Some("A RenderVar: Products/Vars/<name> beside the product"), |ui| {
            if ui.small_button("🗑").on_hover_text("Remove this RenderVar").clicked() { remove = Some(k); }
            if k > 0 && ui.small_button("⏶").on_hover_text("Move up").clicked() { up = Some(k); }
        }, |f| {
            f.toggle("On", Some("Off: not written, and taken off the product"), &mut row.on);
            f.text("Name", Some("The RenderVar's name: its prim, and the AOV's name in the image"), &mut row.name, "diffuse");
            f.text("Source Name", Some("What the renderer gives: an AOV name for raw, a light path expression for lpe, a primvar's name. Empty: the name"), &mut row.source_name, "the name");
            let types: Vec<(String, &str)> = render::SOURCE_TYPES.iter().map(|t| (t.to_string(), *t)).collect();
            f.choice("Source Type", Some("raw: a renderer's own output; lpe: a light path expression; primvar: a primvar of the geometry; intrinsic: a renderer's built-in value"), &mut row.source_type, &types);
            let mut data: Vec<(String, &str)> = render::DATA_TYPES.iter().map(|t| (t.to_string(), *t)).collect();
            if !render::DATA_TYPES.contains(&row.data_type.as_str()) { data.push((row.data_type.clone(), row.data_type.as_str())); }
            let mut ty = row.data_type.clone();
            f.choice("Data Type", Some("The AOV's type (dataType)"), &mut ty, &data);
            row.data_type = ty;
        });
        let extras_title = format!("{}  {}: extras", k + 1, if row.name.trim().is_empty() { "(no name)" } else { row.name.trim() });
        if !row.extras.is_empty() {
            extra_rows(ui, &extras_title, "What the renderer reads on the RenderVar beyond its source and type: a format, a filter. The full name, namespace and all", &mut row.extras, None, &render::var_extra_presets());
        } else if ui.small_button("+ Extras (format, filter…)").clicked() {
            row.extras.push(Extra { name: String::new(), value: Value::Token(String::new()) });
        }
    }
    if let Some(k) = remove { v.rows.remove(k); }
    if let Some(k) = up { v.rows.swap(k, k - 1); }
    group(ui, "RenderVars", None, |f| {
        if v.rows.is_empty() { f.status(Status::Info, "No rows: the product is left as it comes"); }
        f.buttons("Add", Some("An empty RenderVar, or a renderer's standard AOV filled in"), |ui| {
            if ui.button("+ RenderVar").clicked() { v.rows.push(VarRow::default()); }
            crate::menus::menu(ui, "+ From a renderer", |ui| {
                for d in render::definitions() {
                    crate::menus::menu(ui, d.label.as_str(), |ui| {
                        for a in &d.aovs {
                            let label = if a.label.is_empty() { a.name.clone() } else { format!("{}   ({})", a.label, a.name) };
                            if ui.button(label).clicked() {
                                v.rows.push(VarRow { name: a.name.clone(), source_name: a.source_name.clone(), source_type: a.source_type.clone(), data_type: a.data_type.clone(), ..Default::default() });
                                ui.close();
                            }
                        }
                    });
                }
            });
        });
    });
}

// ── Render Command ─────────────────────────────────────────────────────────

/// What a Render Command's panel shows of the graph: the Write USD above
/// it (its name and file), and the scene's frames.
#[derive(Clone, Debug, Default)]
pub struct CommandIn {
    pub writer: Option<(String, String)>,
    pub scene:  (i64, i64),
}

pub fn command_rows(ui: &mut egui::Ui, c: &mut render::CommandSettings, node: NodeId, name: &str, input: &CommandIn, io: &mut super::ui::PanelIo) {
    use crate::core::command::{self, State};
    let run = command::snapshot(node.0, 400);
    let running = run.as_ref().is_some_and(|r| r.state == State::Running);
    group(ui, "Run", None, |f| {
        f.buttons("", None, |ui| {
            if ui.add_enabled(!running && !c.line.trim().is_empty(), egui::Button::new("▶ Run")).clicked() {
                io.action = Some(super::ui::PanelAction::RunCommand(node));
            }
            if ui.add_enabled(running, egui::Button::new("⏹ Stop")).on_hover_text("Ends the command and what it started").clicked() { command::stop(node.0); }
            if ui.add_enabled(run.is_some() && !running, egui::Button::new("Clear")).clicked() { command::clear(node.0); }
        });
        let Some(r) = &run else { f.status(Status::Info, "Not run yet"); return };
        let secs = r.elapsed.as_secs_f64();
        let time = if secs < 60.0 { format!("{secs:.1} s") } else { format!("{}m {:02}s", (secs / 60.0) as u64, secs as u64 % 60) };
        let of = if r.at.1 > 1 { format!(", run {} of {}", r.at.0, r.at.1) } else { String::new() };
        match &r.state {
            State::Running => {
                f.status(Status::Info, format!("Running{of}: {time}"));
                if !r.command.is_empty() {
                    f.wide(|ui| ui.label(egui::RichText::new(format!("$ {}", r.command)).monospace().small().color(ink::DIM())));
                }
            }
            State::Done => f.status(Status::Ok, format!("Done in {time}")),
            State::Stopped => f.status(Status::Info, format!("Stopped after {time}")),
            State::Failed(why) => f.status(Status::Bad, format!("{why}{of}")),
        }
        if let Some(p) = r.progress.filter(|_| running) {
            f.wide(|ui| ui.add(egui::ProgressBar::new(p / 100.0).show_percentage()));
        }
    });


    // Its parameters, or what it prints: one tab at a time. A run starting
    // shows the Console.
    let tab_id = egui::Id::new(("render_command_tab", node.0));
    let mut console = ui.data(|d| d.get_temp::<bool>(tab_id)).unwrap_or(false);
    if io.action.as_ref().is_some_and(|a| matches!(a, super::ui::PanelAction::RunCommand(n) if *n == node)) { console = true; }
    ui.horizontal(|ui| {
        if ui.selectable_label(!console, "Parameters").clicked() { console = false; }
        let lines = run.as_ref().map(|r| r.lines.len()).unwrap_or(0);
        let label = if lines == 0 { "Console".to_string() } else { format!("Console ({lines})") };
        if ui.selectable_label(console, label).on_hover_text("What the command prints").clicked() { console = true; }
    });
    ui.data_mut(|d| d.insert_temp(tab_id, console));
    ui.add_space(4.0);
    if console { console_view(ui, node, run.as_ref()); } else { command_params(ui, c, node, name, input); }
}

/// The command's parameters: what it renders, its line, its frames, where it runs.
fn command_params(ui: &mut egui::Ui, c: &mut render::CommandSettings, node: NodeId, name: &str, input: &CommandIn) {
    use crate::core::command;
    let usd = input.writer.as_ref().map(|(_, p)| p.clone()).unwrap_or_default();
    group(ui, "Scene", Some("What {usd} is: the layer the Write USD above this operator writes, as it comes down the wires"), |f| {
        match &input.writer {
            Some((w, p)) if !p.is_empty() => f.status(Status::Ok, format!("{{usd}} is {p}, from {w}")),
            Some((w, _)) => f.status(Status::Bad, format!("{{usd}} from {w} is empty: it has no file set")),
            None => f.status(Status::Bad, "Nothing above gives {usd}: wire this operator after a Write USD, or type the file in place of {usd}"),
        }
        f.toggle("Write First", Some("Write the layer before running, so the render sees the scene as it is now"), &mut c.write_first);
        f.toggle("Make Output Folders", Some("Make the folders the scene's RenderProducts write into (their Output Image's folder, next to the layer when relative) before running: renderers do not"), &mut c.make_folders);
    });

    let tokens: String = command::TOKENS.iter().map(|(t, what)| format!("{t}: {what}")).collect::<Vec<_>>().join("\n");
    let globals: String = crate::core::vars::GLOBALS_LIST.iter().map(|(t, what)| format!("{t}: {what}")).collect::<Vec<_>>().join("\n");
    group(ui, "Command", Some("Run through the system's shell (sh, or cmd on Windows) when Run is pressed: never while the graph cooks"), |f| {
        f.row("Presets", None, |ui| {
            egui::ComboBox::from_id_salt(("command_presets", node.0)).width(ui.available_width() - 8.0).selected_text("Fill in…").show_ui(ui, |ui| {
                for (label, line, about) in render::COMMAND_PRESETS {
                    if ui.selectable_label(c.line == line, label).on_hover_text(about).clicked() {
                        c.line = line.to_string();
                        // A line of one frame ($F, not the range's) runs once a frame.
                        c.per_frame = crate::core::render::one_frame_a_run(line);
                    }
                }
            });
        });
        f.wide(|ui| {
            let r = ui.add(egui::TextEdit::multiline(&mut c.line).code_editor().desired_rows(3).desired_width(f32::INFINITY))
                .on_hover_text(format!("Variables, put in when it runs. This operator's own, in braces, and the built-ins with the run's frames:\n{tokens}\n\nThe other built-ins:\n{globals}\n\n${{NAME}} sets a name apart; $$ is a $; a name that is none of these is left for the shell. Type {{ or $ for the list"));
            // Its own, for the list on `{`: what it is now.
            let locals = [("usd", command::TOKENS[0].1, usd.clone())];
            crate::variables::complete(ui, &r, &mut c.line, &locals);
        });
        let lines = c.lines(&usd, name, input.scene);
        if let Some(first) = lines.first() {
            let more = if lines.len() > 1 { format!("\n… and {} more, one per frame", lines.len() - 1) } else { String::new() };
            f.wide(|ui| ui.label(egui::RichText::new(format!("{first}{more}")).monospace().small().color(ink::DIM())));
        }
    });

    group(ui, "Frames", Some("The frames $FSTART, $FEND, $N and $F stand for"), |f| {
        let mut custom = c.custom_range;
        f.choice("Range", None, &mut custom, &[(false, "Scene range"), (true, "Custom")]);
        if custom && !c.custom_range { c.start = input.scene.0; c.end = input.scene.1; }
        c.custom_range = custom;
        if c.custom_range {
            f.row("Frames", Some("First and last"), |ui| {
                ui.horizontal(|ui| {
                    let h = ui.spacing().interact_size.y;
                    let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(30.0);
                    ui.field_sized([w, h], egui::DragValue::new(&mut c.start).speed(0.2).prefix("first "));
                    ui.field_sized([w, h], egui::DragValue::new(&mut c.end).speed(0.2).prefix("last "));
                });
            });
        } else {
            f.value("Frames", format!("{} to {}", input.scene.0, input.scene.1));
        }
        f.toggle("Run Per Frame", Some("Off: one run for the whole range (husk renders ranges itself). On: one run per frame, $F that frame, one after the other"), &mut c.per_frame);
        f.status(Status::Info, "The layer holds the scene at the current frame: animation is not written yet, so every frame renders the same");
    });

    group(ui, "Environment", Some("Where it runs, and variables it runs with on top of Imago's (HOUDINI_PATH, ARNOLD_PLUGIN_PATH, licences)"), |f| {
        f.text_vars("Folder", Some("Where it runs, its variables put in. Empty: the layer's folder"), &mut c.folder, "the layer's folder", false);
        let mut remove = None;
        for (k, e) in c.env.iter_mut().enumerate() {
            f.row(&format!("Variable {}", k + 1), None, |ui| {
                ui.horizontal(|ui| {
                    let h = ui.spacing().interact_size.y;
                    let w = ((ui.available_width() - 30.0 - ui.spacing().item_spacing.x) / 2.0).max(40.0);
                    ui.field_sized([w, h], egui::TextEdit::singleline(&mut e.name).hint_text("NAME").font(egui::TextStyle::Monospace));
                    let r = ui.field_sized([w, h], egui::TextEdit::singleline(&mut e.value).hint_text("value"));
                    crate::variables::complete(ui, &r, &mut e.value, &[]);
                    if ui.small_button("🗑").clicked() { remove = Some(k); }
                });
            });
        }
        if let Some(k) = remove { c.env.remove(k); }
        f.buttons("Add", None, |ui| { if ui.button("+ Variable").clicked() { c.env.push(Default::default()); } });
    });

}

/// What the command printed, as a terminal shows it: light text on black,
/// the command lines run in green, errors and warnings in amber.
fn console_view(ui: &mut egui::Ui, node: NodeId, run: Option<&crate::core::command::Snapshot>) {
    // The pane's height when it has one; inside a scrolling panel, a fixed one.
    let h = ui.available_height() - 40.0;
    let height = if h.is_finite() { h.clamp(240.0, 900.0) } else { 420.0 };
    console(ui, ("command_console", node.0), run, height, "Nothing yet: press Run");
}

/// A run's output as a terminal shows it, `height` high: light text on
/// black, the command lines run in green, errors and warnings in amber, a
/// Copy button above. Render Command's Console tab, the Image Viewer's log.
pub fn console(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, run: Option<&crate::core::command::Snapshot>, height: f32, empty: &str) {
    use egui::Color32;
    let black = Color32::from_rgb(12, 12, 12);
    let text = Color32::from_rgb(208, 208, 208);
    let prompt = Color32::from_rgb(120, 220, 120);
    let warn = Color32::from_rgb(240, 180, 80);
    let lines: &[String] = run.map(|r| r.lines.as_slice()).unwrap_or(&[]);
    ui.horizontal(|ui| {
        if ui.add_enabled(!lines.is_empty(), egui::Button::new("Copy")).on_hover_text("Every line kept, to the clipboard").clicked() {
            ui.ctx().copy_text(lines.join("\n"));
        }
        ui.label(egui::RichText::new("The last 400 lines").small().color(ink::DIM()));
    });
    egui::Frame::NONE.fill(black).corner_radius(3.0).inner_margin(8.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::both().id_salt(id).max_height(height).min_scrolled_height(height)
            .stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                if lines.is_empty() {
                    ui.label(egui::RichText::new(empty).monospace().color(Color32::from_gray(120)));
                }
                for l in lines {
                    let lower = l.to_ascii_lowercase();
                    let colour = if l.starts_with("$ ") { prompt }
                        else if ["error", "missing", "warning", "failed", "cannot", "could not"].iter().any(|w| lower.contains(w)) { warn }
                        else { text };
                    ui.add(egui::Label::new(egui::RichText::new(l).monospace().size(11.5).color(colour)).extend());
                }
            });
    });
}
