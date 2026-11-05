//! The size of the whole interface: text, buttons, fields, panes' controls.
//! Auto, from the screen: a short screen (in the system's own points) draws it
//! smaller, so a laptop shows as much as a 4K monitor does. Or a size of the
//! user's, in File > Settings > Interface, or with Ctrl+= and Ctrl+- (Cmd on
//! macOS); Ctrl+0 goes back to Auto. Kept in `interface.txt` in the config
//! folder. egui's zoom: everything laid out in points scales with it, the
//! Viewer's picture stays sharp at the screen's own pixels.

use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    Auto,
    /// A fraction: 1.0 is 100%.
    Fixed(f32),
}

pub const MIN: f32 = 0.6;
pub const MAX: f32 = 1.6;
const STEP: f32 = 0.1;

static CHOSEN: Mutex<Option<Scale>> = Mutex::new(None);

fn file() -> Option<std::path::PathBuf> {
    crate::file_browser::config_path("interface.txt")
}

fn parse(t: &str) -> Scale {
    match t.trim().parse::<f32>() {
        Ok(f) if f.is_finite() => Scale::Fixed(f.clamp(MIN, MAX)),
        _ => Scale::Auto,
    }
}

fn text(s: Scale) -> String {
    match s {
        Scale::Auto => "auto\n".into(),
        Scale::Fixed(f) => format!("{f:.2}\n"),
    }
}

/// The size chosen, read from the config folder once.
pub fn chosen() -> Scale {
    let mut g = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    *g.get_or_insert_with(|| {
        file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| parse(&t))
            .unwrap_or(Scale::Auto)
    })
}

pub fn choose(s: Scale) {
    let s = match s {
        Scale::Fixed(f) => Scale::Fixed(((f * 100.0).round() / 100.0).clamp(MIN, MAX)),
        a => a,
    };
    if let Ok(mut g) = CHOSEN.lock() {
        *g = Some(s);
    }
    if let Some(f) = file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, text(s));
    }
}

/// Auto's size for a screen `height` points tall at the system's scale: the
/// interface is laid out for 1400 or more; shorter screens draw it smaller.
pub fn auto_for(height: Option<f32>) -> f32 {
    match height {
        Some(h) if h < 1000.0 => 0.8,
        Some(h) if h < 1400.0 => 0.9,
        _ => 1.0,
    }
}

/// The screen's height in the system's own points, if known.
fn screen_height(ctx: &egui::Context) -> Option<f32> {
    let (native, monitor) = ctx.input(|i| (i.viewport().native_pixels_per_point, i.viewport().monitor_size));
    // The monitor's size comes in points of the current zoom.
    let pixels = monitor?.y * ctx.pixels_per_point();
    Some(pixels / native.unwrap_or(1.0).max(0.1))
}

pub fn auto(ctx: &egui::Context) -> f32 {
    auto_for(screen_height(ctx))
}

/// The size the interface is drawn at now.
pub fn current(ctx: &egui::Context) -> f32 {
    match chosen() {
        Scale::Auto => auto(ctx),
        Scale::Fixed(f) => f,
    }
}

/// Every frame: the keys, then the size applied when it changed.
pub fn frame(ctx: &egui::Context) {
    let (bigger, smaller, reset) = ctx.input_mut(|i| {
        let m = egui::Modifiers::COMMAND;
        (
            i.consume_key(m, egui::Key::Equals) || i.consume_key(m, egui::Key::Plus),
            i.consume_key(m, egui::Key::Minus),
            i.consume_key(m, egui::Key::Num0),
        )
    });
    if reset {
        choose(Scale::Auto);
    } else if bigger || smaller {
        let now = current(ctx);
        let step = if bigger { STEP } else { -STEP };
        choose(Scale::Fixed(((now + step) * 10.0).round() / 10.0));
    }
    let z = current(ctx);
    if (ctx.zoom_factor() - z).abs() > 1e-3 {
        ctx.set_zoom_factor(z);
    }
}

/// File > Settings > Interface.
pub fn settings(ui: &mut egui::Ui) {
    ui.strong("Size");
    let now = chosen();
    let auto = auto(ui.ctx());
    let mut pick = now;
    ui.radio_value(
        &mut pick,
        Scale::Auto,
        format!("Auto, from the screen ({:.0}%)", auto * 100.0),
    )
    .on_hover_text("Smaller on a short screen: 80% under 1000 points of height, 90% under 1400, else 100%");
    ui.horizontal(|ui| {
        let fixed = matches!(now, Scale::Fixed(_));
        let was = match now {
            Scale::Fixed(f) => f,
            Scale::Auto => auto,
        };
        // The value being dragged, kept until the drag ends.
        let held = egui::Id::new("ui_scale_dragged");
        let mut f = ui.data(|d| d.get_temp::<f32>(held)).unwrap_or(was);
        if ui.radio(fixed, "").clicked() && !fixed {
            pick = Scale::Fixed(f);
        }
        let r = ui.add(
            egui::Slider::new(&mut f, MIN..=MAX)
                .step_by(0.05)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                .custom_parser(|t| {
                    t.trim()
                        .trim_end_matches('%')
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|v| v / 100.0)
                }),
        );
        // Applied when the drag ends: the slider moves with the interface.
        if r.dragged() {
            ui.data_mut(|d| d.insert_temp(held, f));
        } else {
            ui.data_mut(|d| d.remove::<f32>(held));
            // (A slider snapping the value it was given is not a change.)
            if (f - was).abs() > 0.005 {
                pick = Scale::Fixed(f);
            }
        }
    });
    if pick != now {
        choose(pick);
    }
    let dim = crate::properties::form::ink::DIM();
    ui.label(
        egui::RichText::new("Ctrl+= and Ctrl+- (Cmd on macOS) make it bigger or smaller, Ctrl+0 goes back to Auto")
            .small()
            .color(dim),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Short screens draw the interface smaller; a size reads back as written
    /// and stays within its range.
    #[test]
    fn auto_and_saved_sizes() {
        assert_eq!(auto_for(Some(768.0)), 0.8);
        assert_eq!(auto_for(Some(1080.0)), 0.9);
        assert_eq!(auto_for(Some(2160.0)), 1.0);
        assert_eq!(auto_for(None), 1.0);
        assert_eq!(parse(&text(Scale::Fixed(0.85))), Scale::Fixed(0.85));
        assert_eq!(parse(&text(Scale::Auto)), Scale::Auto);
        assert_eq!(parse("9"), Scale::Fixed(MAX));
        assert_eq!(parse("nonsense"), Scale::Auto);
    }
}
