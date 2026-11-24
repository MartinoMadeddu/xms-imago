//! Imago's icons: Phosphor (phosphoricons.com, MIT: see
//! `assets/fonts/Phosphor-LICENSE.txt`), in two weights, Regular and Light,
//! fonts added after egui's (`install`). Each icon is one of their characters,
//! the same in both, named here as Phosphor names it. Draw an icon with these,
//! as text: never a glyph typed in place. Any other Phosphor icon is a
//! constant away: its code point is in the fonts (`@phosphor-icons/web`,
//! `src/regular/style.css`). File > Settings > Interface chooses the weight
//! (`settings`), kept in the config folder.

// Some not used yet: the interface's own icons move to them next.
#![allow(dead_code)]

use std::sync::Mutex;

/// The fonts, Phosphor 2.1.
pub const REGULAR: &[u8] = include_bytes!("../assets/fonts/Phosphor-Regular.ttf");
pub const LIGHT: &[u8] = include_bytes!("../assets/fonts/Phosphor-Light.ttf");

/// The weight icons are drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weight {
    #[default]
    Regular,
    Light,
}

impl Weight {
    pub const ALL: [Weight; 2] = [Weight::Regular, Weight::Light];

    pub fn label(self) -> &'static str {
        match self {
            Weight::Regular => "Regular",
            Weight::Light => "Light",
        }
    }

    /// Its font's name among egui's, and a family of it alone (for showing
    /// the two side by side).
    fn font(self) -> &'static str {
        match self {
            Weight::Regular => "phosphor-regular",
            Weight::Light => "phosphor-light",
        }
    }

    pub fn family(self) -> egui::FontFamily {
        egui::FontFamily::Name(self.font().into())
    }
}

static CHOSEN: Mutex<Option<Weight>> = Mutex::new(None);

fn file() -> Option<std::path::PathBuf> {
    crate::file_browser::config_path("icons.txt")
}

fn parse(t: &str) -> Weight {
    match t.trim() {
        "light" => Weight::Light,
        _ => Weight::Regular,
    }
}

fn text(w: Weight) -> &'static str {
    match w {
        Weight::Regular => "regular\n",
        Weight::Light => "light\n",
    }
}

/// The weight chosen, read from the config folder once: Regular unless
/// Light was chosen.
pub fn chosen() -> Weight {
    let mut g = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
    *g.get_or_insert_with(|| {
        file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| parse(&t))
            .unwrap_or_default()
    })
}

/// Draw icons in `w` from now on, and keep it.
pub fn choose(ctx: &egui::Context, w: Weight) {
    if let Ok(mut g) = CHOSEN.lock() {
        *g = Some(w);
    }
    if let Some(f) = file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, text(w));
    }
    install(ctx);
}

/// Both weights added to egui's fonts, each a family of its own; the one
/// chosen after egui's own fonts in every family, so a character they lack
/// (Phosphor's are in the private-use area) is drawn from it.
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (w, bytes) in [(Weight::Regular, REGULAR), (Weight::Light, LIGHT)] {
        fonts
            .font_data
            .insert(w.font().into(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
        fonts.families.insert(w.family(), vec![w.font().into()]);
    }
    let shown = chosen().font();
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push(shown.into());
    }
    ctx.set_fonts(fonts);
}

/// File > Settings > Interface: the weight of the icons, each shown.
pub fn settings(ui: &mut egui::Ui) {
    ui.strong("Icons");
    let now = chosen();
    let mut pick = now;
    let sample = [
        CUBE,
        SPHERE,
        VIDEO_CAMERA,
        LIGHTBULB,
        STACK,
        GRAPH,
        BONE,
        TRASH,
        FOLDER_OPEN,
        GEAR,
    ];
    for w in Weight::ALL {
        ui.horizontal(|ui| {
            ui.radio_value(&mut pick, w, w.label());
            ui.add_space(8.0);
            let font = egui::FontId::new(16.0, w.family());
            ui.label(egui::RichText::new(sample.join("  ")).font(font));
        });
    }
    if pick != now {
        choose(ui.ctx(), pick);
    }
    let dim = crate::properties::form::ink::DIM();
    ui.label(
        egui::RichText::new("Phosphor icons, on operators and buttons. Light's strokes are thinner")
            .small()
            .color(dim),
    );
}

pub const ARROW_COUNTER_CLOCKWISE: &str = "\u{e038}";
pub const ARROW_DOWN: &str = "\u{e03e}";
pub const ARROW_FAT_LINE_UP: &str = "\u{e522}";
pub const ARROW_LINE_DOWN: &str = "\u{e05c}";
pub const ARROW_SQUARE_IN: &str = "\u{e5dc}";
pub const ARROW_SQUARE_OUT: &str = "\u{e5de}";
pub const ARROW_U_UP_LEFT: &str = "\u{e08a}";
pub const ARROW_U_UP_RIGHT: &str = "\u{e08c}";
pub const ARROW_UP: &str = "\u{e08e}";
pub const ARROWS_CLOCKWISE: &str = "\u{e094}";
pub const ARROWS_HORIZONTAL: &str = "\u{eb06}";
pub const ARROWS_IN_LINE_HORIZONTAL: &str = "\u{e530}";
pub const ARROWS_MERGE: &str = "\u{ed3e}";
pub const ARROWS_OUT: &str = "\u{e0a2}";
pub const ARROWS_OUT_CARDINAL: &str = "\u{e0a4}";
pub const ARROWS_OUT_LINE_HORIZONTAL: &str = "\u{e534}";
pub const ARROWS_OUT_SIMPLE: &str = "\u{e0a6}";
pub const ARROWS_SPLIT: &str = "\u{ed3c}";
pub const BACKSPACE: &str = "\u{e0ae}";
pub const BEZIER_CURVE: &str = "\u{eb00}";
pub const BONE: &str = "\u{e7f2}";
pub const BRACKETS_CURLY: &str = "\u{e860}";
pub const BRACKETS_SQUARE: &str = "\u{e85e}";
pub const CAMERA: &str = "\u{e10e}";
pub const CAR_SIMPLE: &str = "\u{e114}";
pub const CARET_DOWN: &str = "\u{e136}";
pub const CARET_LEFT: &str = "\u{e138}";
pub const CARET_RIGHT: &str = "\u{e13a}";
pub const CARET_UP: &str = "\u{e13c}";
pub const CHECK: &str = "\u{e182}";
pub const CIRCLE: &str = "\u{e18a}";
pub const CIRCLE_DASHED: &str = "\u{e602}";
pub const CIRCLE_HALF: &str = "\u{e18c}";
pub const CIRCLES_THREE: &str = "\u{e192}";
pub const CLOCK: &str = "\u{e19a}";
pub const CLOUD: &str = "\u{e1aa}";
pub const CODE: &str = "\u{e1bc}";
pub const COMPASS: &str = "\u{e1c8}";
pub const COMPASS_TOOL: &str = "\u{ea0e}";
pub const COPY: &str = "\u{e1ca}";
pub const CROP: &str = "\u{e1d4}";
pub const CROSSHAIR: &str = "\u{e1d6}";
pub const CROSSHAIR_SIMPLE: &str = "\u{e1d8}";
pub const CUBE: &str = "\u{e1da}";
pub const CUBE_TRANSPARENT: &str = "\u{ec7c}";
pub const CURSOR_CLICK: &str = "\u{e7c8}";
pub const CYLINDER: &str = "\u{e8fc}";
pub const DIAMOND: &str = "\u{e1ec}";
pub const DICE_FIVE: &str = "\u{e1ee}";
pub const DOTS_NINE: &str = "\u{e1fc}";
pub const DROP: &str = "\u{e210}";
pub const DROP_HALF: &str = "\u{e566}";
pub const ERASER: &str = "\u{e21e}";
pub const EXPORT: &str = "\u{eaf0}";
pub const EYE: &str = "\u{e220}";
pub const FADERS: &str = "\u{e228}";
pub const FAST_FORWARD: &str = "\u{e6a6}";
pub const FILE: &str = "\u{e230}";
pub const FILE_ARROW_DOWN: &str = "\u{e232}";
pub const FILM_SLATE: &str = "\u{e8c2}";
pub const FILM_STRIP: &str = "\u{e792}";
pub const FIRE: &str = "\u{e242}";
pub const FLAG_CHECKERED: &str = "\u{ea38}";
pub const FLASHLIGHT: &str = "\u{e246}";
pub const FLIP_HORIZONTAL: &str = "\u{ed6a}";
pub const FLOPPY_DISK: &str = "\u{e248}";
pub const FLOPPY_DISK_BACK: &str = "\u{eaf4}";
pub const FOLDER: &str = "\u{e24a}";
pub const FOLDER_OPEN: &str = "\u{e256}";
pub const FOLDER_SIMPLE: &str = "\u{e25a}";
pub const FOLDERS: &str = "\u{e260}";
pub const GAUGE: &str = "\u{e628}";
pub const GEAR: &str = "\u{e270}";
pub const GIT_BRANCH: &str = "\u{e278}";
pub const GIT_FORK: &str = "\u{e27e}";
pub const GIT_MERGE: &str = "\u{e280}";
pub const GRADIENT: &str = "\u{eb42}";
pub const GRAINS: &str = "\u{ec68}";
pub const GRAPH: &str = "\u{eb58}";
pub const GRID_FOUR: &str = "\u{e296}";
pub const HAND_GRABBING: &str = "\u{e57c}";
pub const HOUSE: &str = "\u{e2c2}";
pub const IMAGE: &str = "\u{e2ca}";
pub const INTERSECT: &str = "\u{e2d2}";
pub const INTERSECT_SQUARE: &str = "\u{e87a}";
pub const KNIFE: &str = "\u{e636}";
pub const LIGHTBULB: &str = "\u{e2dc}";
pub const LIGHTNING: &str = "\u{e2de}";
pub const LINK: &str = "\u{e2e2}";
pub const LINK_SIMPLE: &str = "\u{e2e6}";
pub const LIST_NUMBERS: &str = "\u{e2f6}";
pub const LIST_PLUS: &str = "\u{e2f8}";
pub const LOCK: &str = "\u{e2fa}";
pub const LOCK_SIMPLE: &str = "\u{e308}";
pub const MAP_PIN: &str = "\u{e316}";
pub const MAP_TRIFOLD: &str = "\u{e31a}";
pub const MATH_OPERATIONS: &str = "\u{e31e}";
pub const NUT: &str = "\u{e38c}";
pub const PACKAGE: &str = "\u{e390}";
pub const PAINT_BUCKET: &str = "\u{e392}";
pub const PALETTE: &str = "\u{e6c8}";
pub const PATH: &str = "\u{e39c}";
pub const PAUSE: &str = "\u{e39e}";
pub const PENCIL_SIMPLE: &str = "\u{e3b4}";
pub const PERSON_ARMS_SPREAD: &str = "\u{ecfe}";
pub const PERSON_SIMPLE_TAI_CHI: &str = "\u{ed5c}";
pub const PERSON_SIMPLE_WALK: &str = "\u{e73a}";
pub const PLACEHOLDER: &str = "\u{e650}";
pub const PLAY: &str = "\u{e3d0}";
pub const PLUS: &str = "\u{e3d4}";
pub const POLYGON: &str = "\u{e6d0}";
pub const PROHIBIT: &str = "\u{e3de}";
pub const PUSH_PIN: &str = "\u{e3e2}";
pub const REPEAT: &str = "\u{e3f6}";
pub const RESIZE: &str = "\u{ed6e}";
pub const REWIND: &str = "\u{e6a8}";
pub const ROWS: &str = "\u{e5a2}";
pub const SCISSORS: &str = "\u{eae0}";
pub const SELECTION_ALL: &str = "\u{e746}";
pub const SHUFFLE: &str = "\u{e422}";
pub const SIGN_IN: &str = "\u{e428}";
pub const SIGN_OUT: &str = "\u{e42a}";
pub const SKIP_BACK: &str = "\u{e5a4}";
pub const SKIP_FORWARD: &str = "\u{e5a6}";
pub const SLIDERS: &str = "\u{e432}";
pub const SLIDERS_HORIZONTAL: &str = "\u{e434}";
pub const SPHERE: &str = "\u{ee66}";
pub const SQUARE: &str = "\u{e45e}";
pub const SQUARES_FOUR: &str = "\u{e464}";
pub const STACK: &str = "\u{e466}";
pub const STACK_MINUS: &str = "\u{edf4}";
pub const STOP: &str = "\u{e46c}";
pub const SWATCHES: &str = "\u{e5b8}";
pub const T_SHIRT: &str = "\u{e670}";
pub const TAG: &str = "\u{e478}";
pub const TARGET: &str = "\u{e47c}";
pub const TERMINAL_WINDOW: &str = "\u{eae8}";
pub const TEXT_AA: &str = "\u{e6ee}";
pub const TEXTBOX: &str = "\u{eb0a}";
pub const TIMER: &str = "\u{e492}";
pub const TRASH: &str = "\u{e4a6}";
pub const TREE_STRUCTURE: &str = "\u{e67c}";
pub const TRIANGLE: &str = "\u{e4b0}";
pub const USER_FOCUS: &str = "\u{e6fc}";
pub const USER_SWITCH: &str = "\u{e756}";
pub const VIDEO_CAMERA: &str = "\u{e4da}";
pub const WAVE_SINE: &str = "\u{ea9a}";
pub const WRENCH: &str = "\u{e5d4}";
pub const X: &str = "\u{e4f6}";
pub const X_CIRCLE: &str = "\u{e4f8}";

#[cfg(test)]
mod tests {
    /// The glyph of `c` in a TrueType font, by its `cmap` table's first
    /// subtable (format 4, as Phosphor's): 0 when it has none.
    fn glyph(font: &[u8], c: u16) -> u16 {
        let u16at = |i: usize| u16::from_be_bytes([font[i], font[i + 1]]);
        let u32at = |i: usize| u32::from_be_bytes([font[i], font[i + 1], font[i + 2], font[i + 3]]);
        let tables = u16at(4) as usize;
        let cmap = (0..tables)
            .map(|k| 12 + 16 * k)
            .find(|&r| &font[r..r + 4] == b"cmap")
            .map(|r| u32at(r + 8) as usize)
            .expect("a cmap table");
        let sub = cmap + u32at(cmap + 8) as usize;
        assert_eq!(u16at(sub), 4, "a format 4 subtable");
        let segs = u16at(sub + 6) as usize / 2;
        let ends = sub + 14;
        let starts = ends + 2 * segs + 2;
        let deltas = starts + 2 * segs;
        let ranges = deltas + 2 * segs;
        for s in 0..segs {
            if c > u16at(ends + 2 * s) {
                continue;
            }
            let start = u16at(starts + 2 * s);
            if c < start {
                return 0;
            }
            let delta = u16at(deltas + 2 * s);
            let range = u16at(ranges + 2 * s) as usize;
            if range == 0 {
                return c.wrapping_add(delta);
            }
            let g = u16at(ranges + 2 * s + range + 2 * (c - start) as usize);
            return if g == 0 { 0 } else { g.wrapping_add(delta) };
        }
        0
    }

    /// Every icon is a character of both fonts: drawn, not a box.
    #[test]
    fn every_icon_is_in_the_font() {
        for font in [super::REGULAR, super::LIGHT] {
            for icon in super::ALL {
                let c = icon.chars().next().unwrap() as u32;
                assert!(c <= 0xffff && glyph(font, c as u16) != 0, "{c:x} is not in the font");
            }
            // And a character it has not, none.
            assert_eq!(glyph(font, 'A' as u16), 0);
        }
        // The weight chosen reads back as written.
        for w in super::Weight::ALL {
            assert_eq!(super::parse(super::text(w)), w);
        }
        assert_eq!(super::parse("nonsense"), super::Weight::Regular);
    }
}

/// Every icon named here, for the test.
#[cfg(test)]
const ALL: [&str; 150] = [
    ARROW_COUNTER_CLOCKWISE,
    ARROW_DOWN,
    ARROW_FAT_LINE_UP,
    ARROW_LINE_DOWN,
    ARROW_SQUARE_IN,
    ARROW_SQUARE_OUT,
    ARROW_U_UP_LEFT,
    ARROW_U_UP_RIGHT,
    ARROW_UP,
    ARROWS_CLOCKWISE,
    ARROWS_HORIZONTAL,
    ARROWS_IN_LINE_HORIZONTAL,
    ARROWS_MERGE,
    ARROWS_OUT,
    ARROWS_OUT_CARDINAL,
    ARROWS_OUT_LINE_HORIZONTAL,
    ARROWS_OUT_SIMPLE,
    ARROWS_SPLIT,
    BACKSPACE,
    BEZIER_CURVE,
    BONE,
    BRACKETS_CURLY,
    BRACKETS_SQUARE,
    CAMERA,
    CAR_SIMPLE,
    CARET_DOWN,
    CARET_LEFT,
    CARET_RIGHT,
    CARET_UP,
    CHECK,
    CIRCLE,
    CIRCLE_DASHED,
    CIRCLE_HALF,
    CIRCLES_THREE,
    CLOCK,
    CLOUD,
    CODE,
    COMPASS,
    COMPASS_TOOL,
    COPY,
    CROP,
    CROSSHAIR,
    CROSSHAIR_SIMPLE,
    CUBE,
    CUBE_TRANSPARENT,
    CURSOR_CLICK,
    CYLINDER,
    DIAMOND,
    DICE_FIVE,
    DOTS_NINE,
    DROP,
    DROP_HALF,
    ERASER,
    EXPORT,
    EYE,
    FADERS,
    FAST_FORWARD,
    FILE,
    FILE_ARROW_DOWN,
    FILM_SLATE,
    FILM_STRIP,
    FIRE,
    FLAG_CHECKERED,
    FLASHLIGHT,
    FLIP_HORIZONTAL,
    FLOPPY_DISK,
    FLOPPY_DISK_BACK,
    FOLDER,
    FOLDER_OPEN,
    FOLDER_SIMPLE,
    FOLDERS,
    GAUGE,
    GEAR,
    GIT_BRANCH,
    GIT_FORK,
    GIT_MERGE,
    GRADIENT,
    GRAINS,
    GRAPH,
    GRID_FOUR,
    HAND_GRABBING,
    HOUSE,
    IMAGE,
    INTERSECT,
    INTERSECT_SQUARE,
    KNIFE,
    LIGHTBULB,
    LIGHTNING,
    LINK,
    LINK_SIMPLE,
    LIST_NUMBERS,
    LIST_PLUS,
    LOCK,
    LOCK_SIMPLE,
    MAP_PIN,
    MAP_TRIFOLD,
    MATH_OPERATIONS,
    NUT,
    PACKAGE,
    PAINT_BUCKET,
    PALETTE,
    PATH,
    PAUSE,
    PENCIL_SIMPLE,
    PERSON_ARMS_SPREAD,
    PERSON_SIMPLE_TAI_CHI,
    PERSON_SIMPLE_WALK,
    PLACEHOLDER,
    PLAY,
    PLUS,
    POLYGON,
    PROHIBIT,
    PUSH_PIN,
    REPEAT,
    RESIZE,
    REWIND,
    ROWS,
    SCISSORS,
    SELECTION_ALL,
    SHUFFLE,
    SIGN_IN,
    SIGN_OUT,
    SKIP_BACK,
    SKIP_FORWARD,
    SLIDERS,
    SLIDERS_HORIZONTAL,
    SPHERE,
    SQUARE,
    SQUARES_FOUR,
    STACK,
    STACK_MINUS,
    STOP,
    SWATCHES,
    T_SHIRT,
    TAG,
    TARGET,
    TERMINAL_WINDOW,
    TEXT_AA,
    TEXTBOX,
    TIMER,
    TRASH,
    TREE_STRUCTURE,
    TRIANGLE,
    USER_FOCUS,
    USER_SWITCH,
    VIDEO_CAMERA,
    WAVE_SINE,
    WRENCH,
    X,
    X_CIRCLE,
];
