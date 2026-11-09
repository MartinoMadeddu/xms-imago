//! Imago without its window: `xms --headless scene.xms --render 1-100`. The
//! graph is opened and cooked frame by frame as in the window, ICE deformers
//! and every operator included, and Arnold renders each frame in Imago's
//! process, loaded once: the first frame loads the scene, each one after is
//! sent as live edits (moved points, transforms, lights, settings) with the
//! frame, so nothing is cached to disk between them. The images are the
//! RenderSettings' products, written by Arnold as it finishes each frame
//! (relative paths are from the scene's folder).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::ice::IceGraphStore;
use crate::node_graph::NodeGraphState;

/// What was asked for on the command line.
#[derive(Debug, PartialEq)]
pub struct Job {
    pub scene: PathBuf,
    /// The frames, first to last; `None`: the scene's range.
    pub frames: Option<(i64, i64, i64)>,
}

const USAGE: &str = "xms --headless scene.xms [--render 1-100 | 12 | 1-100x2]";

/// Whether Imago was asked to run without its window: the job, or why the
/// command line does not say one.
pub fn job(args: &[String]) -> Option<Result<Job, String>> {
    let at = args.iter().position(|a| a == "--headless")?;
    let mut scene = None;
    let mut frames = None;
    let mut rest = args[at + 1..].iter();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--render" => match rest.next().map(|r| range(r)) {
                Some(Ok(r)) => frames = Some(r),
                Some(Err(e)) => return Some(Err(e)),
                None => return Some(Err(format!("--render needs frames: {USAGE}"))),
            },
            s if !s.starts_with("--") && scene.is_none() => scene = Some(PathBuf::from(s)),
            other => return Some(Err(format!("Not an option: {other}. {USAGE}"))),
        }
    }
    Some(
        scene
            .map(|scene| Job { scene, frames })
            .ok_or_else(|| format!("Which scene? {USAGE}")),
    )
}

/// `12`, `1-100`, `1-100x2`: the first and last frames and the step.
fn range(text: &str) -> Result<(i64, i64, i64), String> {
    let bad = || format!("Not frames: {text} (12, 1-100 or 1-100x2)");
    let (span, step) = match text.split_once('x') {
        Some((s, k)) => (s, k.trim().parse::<i64>().map_err(|_| bad())?),
        None => (text, 1),
    };
    // A first frame below zero is written -5-10.
    let split = span.char_indices().skip(1).find(|(_, c)| *c == '-').map(|(i, _)| i);
    let (a, b) = match split {
        Some(i) => (&span[..i], &span[i + 1..]),
        None => (span, span),
    };
    let (a, b) = (
        a.trim().parse::<i64>().map_err(|_| bad())?,
        b.trim().parse::<i64>().map_err(|_| bad())?,
    );
    if step < 1 || b < a {
        return Err(bad());
    }
    Ok((a, b, step))
}

/// Run the job: each frame rendered and written. What went wrong, if
/// anything (Imago then ends with a code other than 0).
pub fn run(job: &Job) -> Result<(), String> {
    let t0 = Instant::now();
    let say = |s: &str| println!("[imago {:7.2}s] {s}", t0.elapsed().as_secs_f64());
    let scene = job
        .scene
        .canonicalize()
        .map_err(|e| format!("{}: {e}", job.scene.display()))?;
    let mut graph = NodeGraphState::default();
    let mut subnets = IceGraphStore::default();
    crate::graph_io::load(&mut graph, &mut subnets, &scene)?;
    let (first, last, step) =
        job.frames
            .unwrap_or((graph.scene.start, graph.scene.end, 1));
    say(&format!("{}: frames {first} to {last}, every {step}", scene.display()));

    let renderer = crate::live::renderer_for("arnold")
        .ok_or_else(|| crate::live::why_not("arnold").unwrap_or_else(|| "Arnold does not render in Imago".into()))?;
    let mut session = crate::live::Session::start(&renderer, &crate::image_viewer::render_dir())?;
    // The scene's layer beside it, so relative products are from its folder.
    let stem = scene
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    session.layer = scene.with_file_name(format!(".{stem}.imago-{}.usda", std::process::id()));

    let result = (|| {
        let mut frame = first;
        let mut loaded = false;
        while frame <= last {
            let started = Instant::now();
            let prims = cook(&mut graph, &subnets, &scene, frame)?;
            let key = (0, (frame as f64).to_bits());
            if loaded {
                session.frame(prims, key, frame as f64);
            } else {
                session.render(prims, key, frame as f64, false, None)?;
                loaded = true;
            }
            let images = session.finish(Duration::from_secs(24 * 3600), |n| say(&format!("note: {n}")))?;
            if images.is_empty() {
                return Err("No render product: nothing would be written (Render Settings make one)".into());
            }
            let written: Vec<String> = images.iter().map(|p| p.display().to_string()).collect();
            say(&format!(
                "frame {frame} done in {:.2}s{}",
                started.elapsed().as_secs_f64(),
                if written.is_empty() {
                    String::new()
                } else {
                    format!(": {}", written.join(", "))
                }
            ));
            if images.iter().any(|p| !p.exists()) {
                return Err(format!("Frame {frame}: not written: {}", written.join(", ")));
            }
            frame += step;
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&session.layer);
    crate::image_viewer::clean_up();
    match &result {
        Ok(()) => say("done"),
        Err(e) => say(&format!("failed: {e}")),
    }
    result
}

/// The graph cooked at `frame`, every stage read: the scene's prims; with
/// none (no view flag), why.
fn cook(
    graph: &mut NodeGraphState,
    subnets: &IceGraphStore,
    scene: &Path,
    frame: i64,
) -> Result<Vec<crate::types::Prim>, String> {
    graph.time = graph.scene.seconds(frame as f64);
    crate::core::vars::set_globals(crate::core::vars::Globals {
        gname: scene
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        gdir: scene.parent().map(|p| p.display().to_string()).unwrap_or_default(),
        frame,
        start: graph.scene.start,
        end: graph.scene.end,
        fps: graph.scene.fps(),
        user: graph.variables.clone(),
    });
    // Stages are read on threads of their own: cooked again once they are.
    let mut prims = crate::live::scene(graph, subnets);
    while crate::usd_scene::any_reading() {
        std::thread::sleep(Duration::from_millis(50));
        prims = crate::live::scene(graph, subnets);
    }
    prims.ok_or_else(|| "The view flag shows no scene: set it on the node to render".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn a_headless_job_is_read_from_the_command_line() {
        assert_eq!(job(&args("xms scene.xms")), None, "the window, as usual");
        assert_eq!(
            job(&args("xms --headless shot.xms --render 1-100")),
            Some(Ok(Job {
                scene: "shot.xms".into(),
                frames: Some((1, 100, 1))
            }))
        );
        assert_eq!(
            job(&args("xms --headless shot.xms")).unwrap().unwrap().frames,
            None,
            "the scene's range"
        );
        assert!(job(&args("xms --headless"))
            .unwrap()
            .unwrap_err()
            .starts_with("Which scene?"));
        assert!(job(&args("xms --headless a.xms --render")).unwrap().is_err());
        assert!(job(&args("xms --headless a.xms --fast"))
            .unwrap()
            .unwrap_err()
            .contains("--fast"));
    }

    #[test]
    fn frames_are_one_a_range_or_a_range_by_steps() {
        assert_eq!(range("12"), Ok((12, 12, 1)));
        assert_eq!(range("1-100"), Ok((1, 100, 1)));
        assert_eq!(range("1-100x2"), Ok((1, 100, 2)));
        assert_eq!(range("-5-10"), Ok((-5, 10, 1)));
        assert!(range("10-1").is_err() && range("1-10x0").is_err() && range("a").is_err());
    }
}
