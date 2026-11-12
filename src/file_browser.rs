//! In-app file browser.
//!
//! Opens where the user last navigated, in this session or a previous one. The
//! location is kept in a small text file in the user's config folder.
//!
//! **Collapse sequences** (on by default) lists files numbered alike as one
//! entry (`shot.####.exr  1-120`); picking it gives its first file. With
//! **UDIM** on too, a sequence numbered 1001 to 1999 is one `<UDIM>` path
//! (`body.<UDIM>.exr  1001-1012`), which picking gives as it is: what a texture
//! input takes for UDIM tiles (`core::sequences`).
//!
//! Each entry shows its size (a sequence's, all its files), when it was last
//! changed, and its owner. A click picks, a double-click opens: a folder, or a
//! file. The window opens in the middle and can be moved.

use std::path::{Path, PathBuf};

use crate::types::NodeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowseMode {
    /// Pick an existing file.
    File,
    /// Pick a folder.
    Folder,
    /// Pick a folder and type a file name.
    Save,
}

/// Who asked for the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowseTarget {
    /// The path parameter of a node.
    Node(NodeId),
    /// The texture file feeding one input of a Material node.
    Texture(NodeId, &'static str),
    /// A file a Light operator's value names (a dome's environment image, a
    /// rect's texture), by the attribute's name.
    LightFile(NodeId, &'static str),
    /// A file name input of a node in a MaterialX operator's graph: the
    /// operator, the node, the input's place in its definition.
    MatxFile(NodeId, u32, u16),
    /// A MaterialX operator's graph, read from a `.mtlx` file.
    MatxLoad(NodeId),
    /// A MaterialX operator's graph, written as a `.mtlx` file.
    MatxSave(NodeId),
    /// File > Import: a MaterialX operator for each material of a `.mtlx` file.
    ImportMtlx,
    OpenGraph,
    SaveGraph,
    OpenLayout,
    SaveLayout,
    /// File > Import: add a saved graph to the one open.
    ImportGraph,
    /// File > Import: add a Load FBX node reading this file.
    ImportFbx,
    /// File > Import: add a Load USD node reading this file.
    ImportUsd,
    /// A compound instance's definition, written as an `.icd` file.
    SaveDefinition(NodeId),
    /// An ICE compound instance's definition (in the ICE graph), likewise.
    SaveIceDefinition(crate::types::IceGraphId, NodeId),
    /// Export Movie: the Viewer's animation as an MP4.
    ExportMovie,
}

struct Request {
    target: BrowseTarget,
    mode: BrowseMode,
    title: String,
    /// Lower-case extensions shown in File and Save mode. Empty shows all.
    exts: Vec<String>,
    selected: Option<PathBuf>,
    file_name: String,
    path_edit: String,
    all_files: bool,
    /// Save mode: the chosen file exists, and the browser asks whether to
    /// replace it.
    replacing: Option<PathBuf>,
}

pub struct FileBrowser {
    dir: PathBuf,
    request: Option<Request>,
    result: Option<(BrowseTarget, PathBuf)>,
    /// Files numbered alike listed as one entry. Kept from one browse to the
    /// next, for the session.
    collapse: bool,
    /// With `collapse`: a sequence of UDIM tiles listed as its `<UDIM>` path.
    udim: bool,
}

impl Default for FileBrowser {
    fn default() -> Self {
        Self {
            dir: load_last_dir(),
            request: None,
            result: None,
            collapse: true,
            udim: true,
        }
    }
}

/// Whether choosing `path` has to be confirmed first: saving over a file that
/// exists asks before replacing it, as in any other program.
pub fn asks_to_replace(mode: BrowseMode, path: &Path) -> bool {
    mode == BrowseMode::Save && path.is_file()
}

/// A file in the application's config folder.
pub fn config_path(file: &str) -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    Some(base.join("xms").join(file))
}

fn config_file() -> Option<PathBuf> {
    config_path("last_dir.txt")
}

fn home_dir() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn load_last_dir() -> PathBuf {
    config_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|s| PathBuf::from(s.trim()))
        .filter(|p| p.is_dir())
        .unwrap_or_else(home_dir)
}

fn save_last_dir(dir: &Path) {
    let Some(file) = config_file() else { return };
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(file, dir.to_string_lossy().as_bytes());
}

impl FileBrowser {
    pub fn is_open(&self) -> bool {
        self.request.is_some()
    }

    /// Open the browser in the last visited folder.
    pub fn open(&mut self, target: BrowseTarget, mode: BrowseMode, title: &str, exts: &[&str], file_name: &str) {
        if !self.dir.is_dir() {
            self.dir = home_dir();
        }
        self.request = Some(Request {
            target,
            mode,
            title: title.to_string(),
            exts: exts.iter().map(|e| e.to_lowercase()).collect(),
            selected: None,
            file_name: file_name.to_string(),
            path_edit: self.dir.to_string_lossy().to_string(),
            all_files: false,
            replacing: None,
        });
    }

    /// The chosen path, once. Returned the frame after the user confirms.
    pub fn take_result(&mut self) -> Option<(BrowseTarget, PathBuf)> {
        self.result.take()
    }

    /// A path has been chosen and not yet taken.
    pub fn has_result(&self) -> bool {
        self.result.is_some()
    }

    /// Start the next browse in this folder.
    pub fn set_dir(&mut self, dir: &Path) {
        if dir.is_dir() {
            self.dir = dir.to_path_buf();
        }
    }

    fn go(&mut self, dir: PathBuf) {
        if !dir.is_dir() {
            return;
        }
        self.dir = dir;
        save_last_dir(&self.dir);
        if let Some(r) = &mut self.request {
            r.selected = None;
            r.replacing = None;
            r.path_edit = self.dir.to_string_lossy().to_string();
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let Some(req) = self.request.as_mut() else { return };

        // Folder contents: folders first, then files that match the filter.
        let mut dirs: Vec<PathBuf> = vec![];
        let mut files: Vec<PathBuf> = vec![];
        let mut read_error = None;
        match std::fs::read_dir(&self.dir) {
            Ok(rd) => {
                for e in rd.filter_map(|e| e.ok()) {
                    let p = e.path();
                    let hidden = p
                        .file_name()
                        .map(|n| n.to_string_lossy().starts_with('.'))
                        .unwrap_or(false);
                    if hidden {
                        continue;
                    }
                    if p.is_dir() {
                        dirs.push(p);
                    } else if req.mode != BrowseMode::Folder {
                        let ext = p
                            .extension()
                            .map(|x| x.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if req.all_files || req.exts.is_empty() || req.exts.contains(&ext) {
                            files.push(p);
                        }
                    }
                }
            }
            Err(e) => read_error = Some(e.to_string()),
        }
        let key = |p: &PathBuf| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        };
        dirs.sort_by_key(key);
        files.sort_by_key(key);
        // Sequences as one entry each, UDIM tiles as their <UDIM> path.
        let entries: Vec<crate::core::sequences::Entry> = if self.collapse {
            crate::core::sequences::collapse(&files, self.udim)
        } else {
            files.iter().cloned().map(crate::core::sequences::Entry::File).collect()
        };
        let (mut collapse, mut udim) = (self.collapse, self.udim);

        let mut go_to: Option<PathBuf> = None;
        let mut confirm: Option<PathBuf> = None;
        let mut cancel = false;
        let mut open = true;

        egui::Window::new(req.title.clone())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([780.0, 480.0])
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .constrain(true)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                // Location bar
                ui.horizontal(|ui| {
                    if ui.button("⬆ Up").clicked() {
                        if let Some(p) = self.dir.parent() { go_to = Some(p.to_path_buf()); }
                    }
                    if ui.button("🏠 Home").clicked() { go_to = Some(home_dir()); }
                    let edit = ui.add(egui::TextEdit::singleline(&mut req.path_edit).desired_width(f32::INFINITY));
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go_to = Some(PathBuf::from(req.path_edit.trim()));
                    }
                });
                if cfg!(windows) {
                    ui.horizontal_wrapped(|ui| {
                        for letter in b'A'..=b'Z' {
                            let drive = PathBuf::from(format!("{}:\\", letter as char));
                            if drive.is_dir() && ui.small_button(format!("{}:", letter as char)).clicked() {
                                go_to = Some(drive);
                            }
                        }
                    });
                }
                ui.separator();

                // Entries
                let footer = if req.mode == BrowseMode::Save { 64.0 } else { 36.0 };
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height((ui.available_height() - footer).max(80.0))
                    .show(ui, |ui| {
                        if let Some(e) = &read_error {
                            ui.colored_label(egui::Color32::from_rgb(220, 140, 140), e);
                        }
                        egui::Grid::new("file_browser_entries")
                            .num_columns(4)
                            .striped(true)
                            .min_col_width(60.0)
                            .spacing([16.0, 2.0])
                            .show(ui, |ui| {
                                let weak = |ui: &mut egui::Ui, t: &str| { ui.weak(t); };
                                ui.strong("Name");
                                ui.strong("Size");
                                ui.strong("Modified");
                                ui.strong("Owner");
                                ui.end_row();
                                let name_width = (ui.available_width() - 420.0).max(220.0);
                                for d in &dirs {
                                    let name = d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                    let sel = req.selected.as_ref() == Some(d);
                                    let resp = name_cell(ui, name_width, sel, format!("📁 {name}"));
                                    if resp.clicked() { req.selected = Some(d.clone()); }
                                    if resp.double_clicked() { go_to = Some(d.clone()); }
                                    let i = info(std::slice::from_ref(d));
                                    weak(ui, "folder");
                                    weak(ui, &i.modified_text());
                                    weak(ui, i.owner.as_deref().unwrap_or(""));
                                    ui.end_row();
                                }
                                for entry in &entries {
                                    let f = entry.path();
                                    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                    let sel  = req.selected.as_ref() == Some(&f);
                                    let icon = match entry { crate::core::sequences::Entry::File(_) => "📄", _ => "🎞" };
                                    let resp = name_cell(ui, name_width, sel, format!("{icon} {}", entry.label()));
                                    if resp.clicked() {
                                        req.selected  = Some(f.clone());
                                        req.file_name = name.clone();
                                    }
                                    if resp.double_clicked() { confirm = Some(f.clone()); }
                                    let i = info(&entry_files(entry));
                                    weak(ui, &i.size.map(size_text).unwrap_or_default());
                                    weak(ui, &i.modified_text());
                                    weak(ui, i.owner.as_deref().unwrap_or(""));
                                    ui.end_row();
                                }
                            });
                        if dirs.is_empty() && files.is_empty() && read_error.is_none() {
                            ui.weak("Nothing to show here.");
                        }
                    });
                ui.separator();

                // Confirm
                if req.mode == BrowseMode::Save {
                    ui.horizontal(|ui| {
                        ui.label("File name:");
                        let edit = ui.add(egui::TextEdit::singleline(&mut req.file_name).desired_width(f32::INFINITY));
                        if edit.changed() { req.replacing = None; }
                    });
                }
                // Saving over a file that exists: Replace or keep it.
                if let Some(existing) = req.replacing.clone() {
                    ui.horizontal(|ui| {
                        let name = existing.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        ui.colored_label(ui.visuals().warn_fg_color, format!("⚠ {name} already exists. Replace it?"));
                        if ui.button("Replace").clicked() { confirm = Some(existing.clone()); }
                        if ui.button("Cancel").clicked() { req.replacing = None; }
                    });
                    return;
                }
                ui.horizontal(|ui| {
                    match req.mode {
                        BrowseMode::File => {
                            let picked = req.selected.clone();
                            if ui.add_enabled(picked.is_some(), egui::Button::new("Open")).clicked() {
                                // A folder picked opens; a file is the choice.
                                match picked {
                                    Some(p) if p.is_dir() => go_to = Some(p),
                                    p => confirm = p,
                                }
                            }
                        }
                        BrowseMode::Folder => {
                            // The folder picked in the list, else the one open.
                            let picked = req.selected.clone().filter(|p| p.is_dir());
                            let label = match &picked {
                                Some(p) => format!("Use {}", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
                                None => "Use this folder".to_string(),
                            };
                            if ui.button(label).clicked() { confirm = Some(picked.unwrap_or_else(|| self.dir.clone())); }
                        }
                        BrowseMode::Save => {
                            let name = req.file_name.trim().to_string();
                            if ui.add_enabled(!name.is_empty(), egui::Button::new("Save")).clicked() {
                                let mut p = self.dir.join(&name);
                                if p.extension().is_none() {
                                    if let Some(ext) = req.exts.first() { p.set_extension(ext); }
                                }
                                confirm = Some(p);
                            }
                        }
                    }
                    if ui.button("Cancel").clicked() { cancel = true; }
                    if req.mode != BrowseMode::Folder && !req.exts.is_empty() {
                        ui.checkbox(&mut req.all_files, "All files");
                        ui.weak(format!("showing .{}", req.exts.join(" .")));
                    }
                    if req.mode != BrowseMode::Folder {
                        ui.checkbox(&mut collapse, "Collapse sequences")
                            .on_hover_text("Files numbered alike as one entry (shot.####.exr); picking it gives its first file");
                        ui.add_enabled_ui(collapse, |ui| {
                            ui.toggle_value(&mut udim, "UDIM")
                                .on_hover_text("A sequence numbered 1001 to 1999 as one <UDIM> path (body.<UDIM>.exr), for a texture of UDIM tiles");
                        });
                    }
                });
            });

        self.collapse = collapse;
        self.udim = udim;
        if let Some(dir) = go_to {
            self.go(dir);
        }
        // A file that exists is replaced only once Replace is clicked.
        if let (Some(path), Some(req)) = (confirm.as_ref(), self.request.as_mut()) {
            if asks_to_replace(req.mode, path) && req.replacing.as_ref() != Some(path) {
                req.replacing = Some(path.clone());
                confirm = None;
            }
        }
        if let Some(path) = confirm {
            let target = self.request.as_ref().map(|r| r.target);
            if let Some(target) = target {
                self.result = Some((target, path));
            }
            save_last_dir(&self.dir);
            self.request = None;
        } else if cancel || !open {
            self.request = None;
        }
    }
}

// What the listing shows of an entry

/// An entry's name, in a column `width` wide, cut short when longer.
fn name_cell(ui: &mut egui::Ui, width: f32, selected: bool, text: String) -> egui::Response {
    ui.scope(|ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        ui.selectable_label(selected, text)
    })
    .inner
}

/// An entry's size (a sequence's, all its files), when it last changed (its
/// newest file), and its owner (its first file's).
struct Info {
    size: Option<u64>,
    modified: Option<std::time::SystemTime>,
    owner: Option<String>,
}

impl Info {
    fn modified_text(&self) -> String {
        self.modified.map(date_text).unwrap_or_default()
    }
}

fn entry_files(e: &crate::core::sequences::Entry) -> Vec<PathBuf> {
    use crate::core::sequences::Entry;
    match e {
        Entry::File(p) => vec![p.clone()],
        Entry::Sequence { files, .. } => files.clone(),
        Entry::Udim { path, tiles } => {
            let text = path.to_string_lossy();
            tiles
                .iter()
                .map(|t| PathBuf::from(text.replace("<UDIM>", &t.to_string())))
                .collect()
        }
    }
}

fn info(paths: &[PathBuf]) -> Info {
    let mut out = Info {
        size: None,
        modified: None,
        owner: None,
    };
    for (k, p) in paths.iter().enumerate() {
        let Ok(m) = std::fs::metadata(p) else { continue };
        if m.is_file() {
            out.size = Some(out.size.unwrap_or(0) + m.len());
        }
        if let Ok(t) = m.modified() {
            out.modified = Some(out.modified.map_or(t, |was| was.max(t)));
        }
        if k == 0 {
            out.owner = owner(&m);
        }
    }
    out
}

/// A size as the Finder gives it: bytes, then kB, MB, GB by thousands.
fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["kB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} bytes");
    }
    let mut v = bytes as f64 / 1000.0;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if v < 10.0 {
        format!("{v:.1} {}", UNITS[u])
    } else {
        format!("{v:.0} {}", UNITS[u])
    }
}

/// A time as the local date and time to the minute (`2026-11-12 12:38`).
fn date_text(t: std::time::SystemTime) -> String {
    let secs = match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let (y, mo, d, h, mi) = local(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// Seconds since 1970 as the local year, month, day, hour and minute.
#[cfg(unix)]
fn local(secs: i64) -> (i64, u32, u32, u32, u32) {
    let t = secs as libc::time_t;
    // SAFETY: localtime_r writes the broken-down time into `tm` and reads only
    // `t`.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return utc(secs);
    }
    (
        tm.tm_year as i64 + 1900,
        tm.tm_mon as u32 + 1,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
    )
}

#[cfg(not(unix))]
fn local(secs: i64) -> (i64, u32, u32, u32, u32) {
    utc(secs)
}

/// Seconds since 1970 as UTC's year, month, day, hour and minute (Howard
/// Hinnant's days-to-civil).
fn utc(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d, (rest / 3600) as u32, (rest % 3600 / 60) as u32)
}

/// The file's owner, by name (filesystems keep the owner, not who last
/// changed it).
#[cfg(unix)]
fn owner(m: &std::fs::Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    thread_local! {
        static NAMES: std::cell::RefCell<std::collections::HashMap<u32, String>> = Default::default();
    }
    let uid = m.uid();
    if let Some(n) = NAMES.with(|c| c.borrow().get(&uid).cloned()) {
        return Some(n);
    }
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let mut buf = vec![0 as libc::c_char; 4096];
    // SAFETY: getpwuid_r fills `pw` with pointers into `buf`, which outlives
    // their reading here; `found` is null when there is no such user.
    let ok = unsafe { libc::getpwuid_r(uid, &mut pw, buf.as_mut_ptr(), buf.len(), &mut found) };
    let name = if ok == 0 && !found.is_null() && !pw.pw_name.is_null() {
        unsafe { std::ffi::CStr::from_ptr(pw.pw_name) }
            .to_string_lossy()
            .into_owned()
    } else {
        uid.to_string()
    };
    NAMES.with(|c| c.borrow_mut().insert(uid, name.clone()));
    Some(name)
}

#[cfg(not(unix))]
fn owner(_: &std::fs::Metadata) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_dates_and_owners_read_as_the_finder_gives_them() {
        assert_eq!(size_text(512), "512 bytes");
        assert_eq!(size_text(1_500), "1.5 kB");
        assert_eq!(size_text(45_000_000), "45 MB");
        assert_eq!(size_text(2_300_000_000), "2.3 GB");
        assert_eq!(utc(0), (1970, 1, 1, 0, 0));
        assert_eq!(utc(1_794_486_480), (2026, 11, 12, 12, 28));
        assert_eq!(utc(951_782_400), (2000, 2, 29, 0, 0), "a leap day");
        let dir = std::env::temp_dir().join("xms_file_browser_info");
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("shot.0001.exr"), dir.join("shot.0002.exr"));
        std::fs::write(&a, [0u8; 1000]).unwrap();
        std::fs::write(&b, [0u8; 500]).unwrap();
        let i = info(&[a, b]);
        assert_eq!(i.size, Some(1500), "a sequence: all its files");
        assert!(i.modified.is_some());
        if cfg!(unix) {
            assert!(i.owner.as_deref().is_some_and(|o| !o.is_empty()));
        }
    }

    #[test]
    fn reopens_in_last_visited_folder() {
        let base = std::env::temp_dir().join("xms_browser_test");
        let _ = std::fs::remove_dir_all(&base);
        let visited = base.join("takes").join("day01");
        std::fs::create_dir_all(&visited).unwrap();
        // Keep the config file inside the test folder.
        std::env::set_var(
            if cfg!(windows) { "APPDATA" } else { "XDG_CONFIG_HOME" },
            base.join("cfg"),
        );

        let mut b = FileBrowser::default();
        b.open(BrowseTarget::OpenGraph, BrowseMode::File, "Open", &["fbx"], "");
        b.go(visited.clone());
        assert_eq!(b.dir, visited);
        // Not a folder: ignored.
        b.go(base.join("missing"));
        assert_eq!(b.dir, visited);

        // A new session starts where the last one left off.
        let again = FileBrowser::default();
        assert_eq!(again.dir, visited);

        // Saving over a file that exists asks first; a new name does not.
        let there = visited.join("graph.json");
        std::fs::write(&there, "{}").unwrap();
        assert!(asks_to_replace(BrowseMode::Save, &there));
        assert!(!asks_to_replace(BrowseMode::Save, &visited.join("new.json")));
        assert!(!asks_to_replace(BrowseMode::File, &there));

        // If that folder is gone, fall back to the home folder.
        std::fs::remove_dir_all(&visited).unwrap();
        assert_ne!(FileBrowser::default().dir, visited);
    }
}
