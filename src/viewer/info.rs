//! Overlay info in the Viewer, at its top right: how fast it draws, how much
//! memory Imago holds, the camera looked through, what is selected, and how
//! the render goes. Chosen in File > Settings > Viewer > Overlays, kept in
//! `viewer_overlays.txt` beside the other settings.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Which overlays are shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub fps: bool,
    pub memory: bool,
    pub camera: bool,
    pub selection: bool,
    pub render: bool,
}

impl Shown {
    pub fn any(self) -> bool {
        self.fps || self.memory || self.camera || self.selection || self.render
    }

    fn to_text(self) -> String {
        [
            ("fps", self.fps),
            ("memory", self.memory),
            ("camera", self.camera),
            ("selection", self.selection),
            ("render", self.render),
        ]
        .iter()
        .map(|(k, v)| format!("{k}={}\n", u8::from(*v)))
        .collect()
    }

    fn from_text(text: &str) -> Self {
        let on = |key: &str| {
            text.lines()
                .filter_map(|l| l.split_once('='))
                .any(|(k, v)| k.trim() == key && v.trim() == "1")
        };
        Shown {
            fps: on("fps"),
            memory: on("memory"),
            camera: on("camera"),
            selection: on("selection"),
            render: on("render"),
        }
    }
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn file() -> Option<std::path::PathBuf> {
    crate::file_browser::config_path("viewer_overlays.txt")
}

/// The overlays shown, as last saved.
pub fn shown() -> Shown {
    let mut s = SHOWN.lock().unwrap();
    *s.get_or_insert_with(|| {
        file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .map(|t| Shown::from_text(&t))
            .unwrap_or_default()
    })
}

/// Show these overlays, and keep the choice.
pub fn set(shown: Shown) {
    *SHOWN.lock().unwrap() = Some(shown);
    if let Some(f) = file() {
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(f, shown.to_text());
    }
}

/// The Settings page's part: File > Settings > Viewer > Overlays.
pub fn settings(ui: &mut egui::Ui) {
    let mut s = shown();
    let before = s;
    ui.checkbox(&mut s.fps, "FPS")
        .on_hover_text("How many times a second the Viewer is drawn, and how long the last frame took. The Viewer draws only when something changes, so it reads low while nothing moves");
    ui.checkbox(&mut s.memory, "Memory")
        .on_hover_text("The memory Imago holds (its resident size), as the system counts it");
    ui.checkbox(&mut s.camera, "Camera")
        .on_hover_text("The camera looked through, its focal length and aperture (or the Viewer's own and its field of view), and the frame");
    ui.checkbox(&mut s.selection, "Selection")
        .on_hover_text("How many prims are selected, and the meshes, points and triangles in them and what is below them");
    ui.checkbox(&mut s.render, "Render")
        .on_hover_text("While Arnold renders in the Viewer: how it goes (pass, done) and the time since it started");
    if s != before {
        set(s);
    }
}

/// The Viewer's frames over the last second.
#[derive(Default)]
pub struct Frames {
    times: VecDeque<Instant>,
}

impl Frames {
    /// A frame drawn: frames a second over the last second.
    pub fn tick(&mut self) -> usize {
        let now = Instant::now();
        self.times.push_back(now);
        while self.times.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1)) {
            self.times.pop_front();
        }
        self.times.len()
    }
}

/// The memory Imago holds, in bytes, read at most twice a second.
pub fn resident() -> Option<u64> {
    static LAST: Mutex<Option<(Instant, Option<u64>)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if let Some((at, bytes)) = *last {
        if at.elapsed() < Duration::from_millis(500) {
            return bytes;
        }
    }
    let bytes = read_resident();
    *last = Some((Instant::now(), bytes));
    bytes
}

#[cfg(target_os = "linux")]
fn read_resident() -> Option<u64> {
    // statm: sizes in pages; the second is what is resident.
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = text.split_whitespace().nth(1)?.parse().ok()?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    Some(pages * page.max(0) as u64)
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn read_resident() -> Option<u64> {
    let mut info: libc::mach_task_basic_info = unsafe { std::mem::zeroed() };
    let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
    let r = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            &mut info as *mut _ as libc::task_info_t,
            &mut count,
        )
    };
    (r == libc::KERN_SUCCESS).then_some(info.resident_size)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn read_resident() -> Option<u64> {
    None
}

/// "1.84 GB", "512 MB".
fn bytes(n: u64) -> String {
    let gb = n as f64 / (1u64 << 30) as f64;
    if gb >= 1.0 {
        format!("{gb:.2} GB")
    } else {
        format!("{} MB", n >> 20)
    }
}

/// What is selected, as the Selection overlay counts it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counted {
    pub prims: usize,
    pub meshes: usize,
    pub points: usize,
    pub triangles: usize,
}

/// The selected prims (`chosen`) and what is below them, counted in `prims`.
pub fn count(prims: &[crate::types::Prim], chosen: &[String]) -> Counted {
    let mut c = Counted {
        prims: chosen.len(),
        ..Default::default()
    };
    if chosen.is_empty() {
        return c;
    }
    let set: std::collections::HashSet<&str> = chosen
        .iter()
        .map(|p| crate::core::instancing::instance_path(p).map_or(p.as_str(), |(i, _)| i))
        .collect();
    for p in prims {
        let under = set.contains(p.path.as_str())
            || p.path.match_indices('/').any(|(i, _)| i > 0 && set.contains(&p.path[..i]));
        if under && (p.mesh.num_faces() > 0 || p.mesh.point_count() > 0) {
            c.meshes += 1;
            c.points += p.mesh.point_count();
            c.triangles += p.mesh.num_faces();
        }
    }
    c
}

/// 12345678 as "12.3M", 4321 as "4.3k".
fn short(n: usize) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 10_000 => format!("{:.0}k", n as f64 / 1e3),
        n if n >= 1_000 => format!("{:.1}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

/// What the overlay's lines are made from, beyond the time.
#[derive(Default)]
pub struct Facts {
    /// The camera: its line, as made by the Viewer.
    pub camera: Option<String>,
    pub selection: Option<Counted>,
    /// How the render in the Viewer goes, and since when.
    pub render: Option<(String, std::time::Duration)>,
}

/// The overlay's lines, as shown: `fps` the frames over the last second,
/// `ms` the last frame's time.
pub fn lines(shown: Shown, fps: usize, ms: f32, facts: &Facts) -> Vec<String> {
    let mut out = vec![];
    if shown.fps {
        out.push(format!("{fps} fps  {ms:.1} ms"));
    }
    if shown.memory {
        out.push(match resident() {
            Some(n) => format!("Memory {}", bytes(n)),
            None => "Memory: not known here".into(),
        });
    }
    if shown.camera {
        if let Some(c) = &facts.camera {
            out.push(c.clone());
        }
    }
    if shown.selection {
        match facts.selection {
            Some(c) if c.prims > 0 => out.push(format!(
                "Selected {} prim{}  {} mesh{}  {} pts  {} tris",
                c.prims,
                if c.prims == 1 { "" } else { "s" },
                c.meshes,
                if c.meshes == 1 { "" } else { "es" },
                short(c.points),
                short(c.triangles)
            )),
            _ => out.push("Nothing selected".into()),
        }
    }
    if shown.render {
        if let Some((status, since)) = &facts.render {
            out.push(format!("{status}  {:.1} s", since.as_secs_f32()));
        }
    }
    out
}

/// The lines in a dark box at the Viewer's top right, under its menus.
pub fn draw(ui: &egui::Ui, rect: egui::Rect, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let painter = ui.painter_at(rect);
    let galley = painter.layout(
        lines.join("\n"),
        egui::FontId::monospace(11.0),
        egui::Color32::from_gray(235),
        f32::INFINITY,
    );
    let at = egui::pos2(rect.max.x - 12.0 - galley.size().x, rect.min.y + 44.0);
    painter.rect_filled(
        egui::Rect::from_min_size(at, galley.size()).expand(5.0),
        3.0,
        egui::Color32::from_black_alpha(150),
    );
    painter.galley(at, galley, egui::Color32::from_gray(235));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_are_kept_as_text_and_memory_is_read() {
        let s = Shown {
            fps: true,
            selection: true,
            ..Default::default()
        };
        assert_eq!(Shown::from_text(&s.to_text()), s);
        assert_eq!(Shown::from_text(""), Shown::default());
        let facts = Facts {
            selection: Some(Counted {
                prims: 1,
                meshes: 2,
                points: 12_345,
                triangles: 2_500_000,
            }),
            ..Default::default()
        };
        let l = lines(s, 24, 12.0, &facts);
        assert!(l[0].starts_with("24 fps"));
        assert_eq!(l[1], "Selected 1 prim  2 meshes  12k pts  2.5M tris");
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        assert!(read_resident().is_some_and(|n| n > 1 << 20));
        assert_eq!(bytes(3 << 29), "1.50 GB");
        assert_eq!(bytes(512 << 20), "512 MB");
    }
}
