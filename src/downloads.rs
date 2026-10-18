//! Example files fetched when a graph asks for one that is not there.
//!
//! A template or a saved graph may name a file under `examples/` that does
//! not ship with the program (see `examples::SOURCES`): too big for the
//! repository, or published under a licence that asks for a credit beside
//! it. When such a graph is opened, a dialog says what the file is, where
//! it comes from and under which licence, and offers to download it into the
//! examples folder, to open its page for a download by hand, or to leave it.
//! The download runs on a thread of its own; the loaders find the file by
//! its date, so nothing else has to be told.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::examples::{self, Source};
use crate::node_graph::NodeGraphState;
use crate::types::NodeType;

type File = (&'static str, &'static str, u64);

/// A download in progress, read by the dialog each frame.
pub struct Job {
    files:   Vec<&'static File>,
    /// Files finished so far, and bytes of the one being fetched.
    done:    AtomicUsize,
    got:     AtomicU64,
    total:   AtomicU64,
    result:  Mutex<Option<Result<usize, String>>>,
}

impl Job {
    fn finished(&self) -> Option<Result<usize, String>> { self.result.lock().ok()?.clone() }
}

#[derive(Default)]
pub struct Downloads {
    /// Files the user chose not to fetch, until the next graph is opened.
    declined: HashSet<&'static str>,
    /// What the dialog offers now.
    offer:    Vec<(&'static Source, &'static File)>,
    job:      Option<Arc<Job>>,
    /// Said under the graph's buttons when a download ends.
    pub message: Option<String>,
}

impl Downloads {
    /// A graph was opened or added: files declined before may be asked
    /// about again.
    pub fn graph_opened(&mut self) { self.declined.clear(); }
}

/// The files a graph reads: what its loaders point at.
pub fn file_paths(graph: &NodeGraphState) -> Vec<String> {
    graph.nodes.iter().filter_map(|n| match &n.node_type {
        NodeType::LoadFbx { path, .. } | NodeType::LoadFbxMesh { path } | NodeType::LoadUsd { path, .. } => Some(path.clone()),
        _ => None,
    }).filter(|p| !p.trim().is_empty()).collect()
}

fn megabytes(bytes: u64) -> String {
    if bytes >= 1_000_000 { format!("{:.0} MB", bytes as f64 / 1e6) } else { format!("{:.1} MB", bytes as f64 / 1e6) }
}

/// Each frame: look for missing files the graph names, offer them, show the
/// download running. Returns true while a dialog is showing.
pub fn frame(ctx: &egui::Context, downloads: &mut Downloads, graph: &NodeGraphState) -> bool {
    // A download running, or just finished.
    if let Some(job) = downloads.job.clone() {
        match job.finished() {
            Some(Ok(n)) => {
                downloads.job = None;
                downloads.message = Some(format!("Downloaded {n} example file{} into {}", if n == 1 { "" } else { "s" },
                    examples::dir().unwrap_or_else(|| "examples".into()).display()));
            }
            Some(Err(e)) => {
                downloads.job = None;
                downloads.message = Some(format!("Download failed: {e}"));
                for f in &job.files { downloads.declined.insert(f.0); }
            }
            None => {
                crate::dim_behind(ctx);
                let (done, got, total) = (job.done.load(Ordering::Relaxed), job.got.load(Ordering::Relaxed), job.total.load(Ordering::Relaxed));
                let current = job.files.get(done).map(|f| f.0).unwrap_or("");
                egui::Window::new("Downloading example files")
                    .order(egui::Order::Tooltip).collapsible(false).resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.set_min_width(360.0);
                        ui.label(format!("{} of {}: {current}", done + 1, job.files.len()));
                        let frac = if total > 0 { (got as f32 / total as f32).min(1.0) } else { 0.0 };
                        ui.add(egui::ProgressBar::new(frac).text(format!("{} of {}", megabytes(got), megabytes(total))));
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
                return true;
            }
        }
    }
    // Nothing offered yet: look at what the graph names.
    if downloads.offer.is_empty() {
        let paths = file_paths(graph);
        downloads.offer = examples::missing_downloads(&paths).into_iter()
            .filter(|(_, f)| !downloads.declined.contains(f.0)).collect();
        if downloads.offer.is_empty() { return false; }
    }
    // The question: what, from where, under which licence; fetch, open the
    // page, or not now.
    crate::dim_behind(ctx);
    let mut choice: Option<u8> = None;
    let offer = downloads.offer.clone();
    let total: u64 = offer.iter().map(|(_, f)| f.2).sum();
    egui::Window::new("Example files to download")
        .order(egui::Order::Tooltip).collapsible(false).resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_max_width(520.0);
            ui.label(format!("This graph reads {} example file{} that {} not in the examples folder. {} free to use and to pass on, but not in the repository.",
                offer.len(), if offer.len() == 1 { "" } else { "s" }, if offer.len() == 1 { "is" } else { "are" }, if offer.len() == 1 { "It is" } else { "They are" }));
            ui.add_space(4.0);
            // Grouped by source, so the licence is read once.
            let mut sources: Vec<&'static Source> = vec![];
            for (s, _) in &offer { if !sources.iter().any(|x| std::ptr::eq(*x, *s)) { sources.push(s); } }
            for s in sources {
                let files: Vec<&&'static File> = offer.iter().filter(|(x, _)| std::ptr::eq(*x, s)).map(|(_, f)| f).collect();
                ui.group(|ui| {
                    ui.label(egui::RichText::new(s.name).strong());
                    ui.label(format!("By {}. Licence: {}.", s.by, s.licence));
                    for f in &files { ui.label(egui::RichText::new(format!("{}  ({})", f.0, megabytes(f.2))).monospace().small()); }
                    ui.horizontal(|ui| {
                        if ui.small_button("Open the page").on_hover_text(s.page).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(s.page));
                        }
                        ui.label(egui::RichText::new(s.page).small().weak());
                    });
                });
            }
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("Downloading puts them under {}.", examples::dir().unwrap_or_else(|| "examples".into()).display())).small().weak());
            ui.horizontal(|ui| {
                if ui.button(format!("Download ({})", megabytes(total))).clicked() { choice = Some(0); }
                if ui.button("Not now").clicked() { choice = Some(1); }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) { choice = Some(1); }
    match choice {
        Some(0) => {
            let files: Vec<&'static File> = offer.iter().map(|(_, f)| *f).collect();
            downloads.offer.clear();
            let job = Arc::new(Job { files, done: AtomicUsize::new(0), got: AtomicU64::new(0), total: AtomicU64::new(0), result: Mutex::new(None) });
            let worker = job.clone();
            std::thread::spawn(move || {
                let mut n = 0;
                let result = (|| {
                    for (k, f) in worker.files.iter().enumerate() {
                        worker.done.store(k, Ordering::Relaxed);
                        worker.got.store(0, Ordering::Relaxed);
                        worker.total.store(f.2, Ordering::Relaxed);
                        examples::download(f, &|got, total| { worker.got.store(got, Ordering::Relaxed); worker.total.store(total, Ordering::Relaxed); })
                            .map_err(|e| format!("{}: {e}", f.0))?;
                        n += 1;
                    }
                    Ok(n)
                })();
                *worker.result.lock().unwrap() = Some(result);
            });
            downloads.job = Some(job);
            true
        }
        Some(_) => {
            for (_, f) in &offer { downloads.declined.insert(f.0); }
            downloads.offer.clear();
            false
        }
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A graph naming an example file that is not there, and is in the
    /// table of sources, is offered it; a file that is there, or that the
    /// program knows nothing about, is not.
    #[test]
    fn missing_files_the_program_can_fetch_are_offered() {
        let base = examples::dir().unwrap_or_else(|| "examples".into());
        let mut g = NodeGraphState::default();
        let missing = base.join("mocap/novasci/NodYes1.fbx");
        let _ = std::fs::remove_file(&missing);
        g.add_node("A".into(), NodeType::LoadFbx { path: missing.to_string_lossy().to_string(), take: 0 }, egui::pos2(0.0, 0.0));
        g.add_node("B".into(), NodeType::LoadFbx { path: base.join("mocap/novasci/ComeHere1.fbx").to_string_lossy().to_string(), take: 0 }, egui::pos2(0.0, 80.0));
        g.add_node("C".into(), NodeType::LoadFbx { path: "/nowhere/else.fbx".into(), take: 0 }, egui::pos2(0.0, 160.0));
        let offered = examples::missing_downloads(&file_paths(&g));
        let names: Vec<&str> = offered.iter().map(|(_, f)| f.0).collect();
        assert_eq!(names, ["mocap/novasci/NodYes1.fbx"]);
        assert_eq!(offered[0].0.licence, "CC0 1.0 (public domain)");
        // Every entry of the table has a licence, an author and a page.
        for s in examples::SOURCES {
            assert!(!s.licence.is_empty() && !s.by.is_empty() && s.page.starts_with("https://"), "{}", s.name);
            for f in s.files { assert!(f.1.starts_with("https://") && f.2 > 0, "{}", f.0); }
        }
    }
}
