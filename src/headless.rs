//! Imago without its window, for a farm or a script:
//!
//! ```text
//! xms --headless shot.xms --render 1-100   Arnold in Imago, frame by frame
//! xms --headless shot.xms --write 1-100    the Write USD nodes write
//! xms --headless shot.xms --run 1-100      the Render Command nodes run
//! xms --headless shot.xms --tx             .tx files of the scene's textures
//! ```
//!
//! - **`--render`:** the graph cooked frame by frame as in the window (ICE
//!   deformers and every operator), Arnold rendering in Imago's process,
//!   loaded once. The renderer is given a light layer, in the temp folder: the
//!   scene's files referenced, Imago's changes over them, but not the points
//!   deformers moved, which go to Arnold in memory, as each frame after the
//!   first does. No scene description is kept. The images are the
//!   RenderSettings' products, written by Arnold, relative ones from the
//!   scene's folder.
//! - **`--write`, `--run`:** as pressing Write on each Write USD, Run on each
//!   Render Command (its Write USD written first when it says so): the scene
//!   description written where Write USD says, for husk, kick or a farm.
//!   The frames given are the scene's range for them.
//! - **`--tx`:** each texture Arnold reads in the scene made a `.tx` by
//!   Arnold's `maketx` (only when the texture is newer), once before a farm
//!   job, so the machines do not all make them.
//! - **`--node NAME`:** only the Write USD or Render Command of that name.
//!
//! What it does is printed line by line, with a progress line a farm manager
//! reads (`Imago progress: 40%`); it ends with code 1 when anything failed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::ice::IceGraphStore;
use crate::node_graph::NodeGraphState;
use crate::types::NodeType;

/// What it is asked to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Render,
    Write,
    Run,
    Tx,
}

/// What was asked for on the command line.
#[derive(Debug, PartialEq)]
pub struct Job {
    pub scene: PathBuf,
    pub mode: Mode,
    /// The frames, first to last and the step; `None`: the scene's range.
    pub frames: Option<(i64, i64, i64)>,
    /// Only the node of this name (`--write`, `--run`).
    pub node: Option<String>,
}

pub const USAGE: &str =
    "xms --headless scene.xms (--render | --write | --run | --tx) [1-100 | 12 | 1-100x2] [--node NAME]";

/// Whether Imago was asked to run without its window: the job, or why the
/// command line does not say one.
pub fn job(args: &[String]) -> Option<Result<Job, String>> {
    let at = args.iter().position(|a| a == "--headless")?;
    Some(read_job(&args[at + 1..]))
}

fn read_job(args: &[String]) -> Result<Job, String> {
    let (mut scene, mut mode, mut frames, mut node) = (None, None, None, None);
    let mut rest = args.iter().peekable();
    while let Some(a) = rest.next() {
        let asked = match a.as_str() {
            "--render" => Some(Mode::Render),
            "--write" => Some(Mode::Write),
            "--run" => Some(Mode::Run),
            "--tx" => Some(Mode::Tx),
            "--node" => {
                node = Some(rest.next().ok_or(format!("--node needs a name: {USAGE}"))?.clone());
                None
            }
            s if !s.starts_with("--") && scene.is_none() => {
                scene = Some(PathBuf::from(s));
                None
            }
            other => return Err(format!("Not an option: {other}. {USAGE}")),
        };
        if let Some(m) = asked {
            if mode.replace(m).is_some() {
                return Err(format!("One of --render, --write, --run or --tx: {USAGE}"));
            }
            // Its frames, when they follow it.
            if let Some(r) = rest.peek().map(|r| range(r)).filter(|r| r.is_ok()) {
                frames = r.ok();
                rest.next();
            }
        }
    }
    let scene = scene.ok_or_else(|| format!("Which scene? {USAGE}"))?;
    let mode = mode.ok_or_else(|| format!("What to do with it? {USAGE}"))?;
    Ok(Job {
        scene,
        mode,
        frames,
        node,
    })
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

/// The frames of a range, first to last.
fn frames_of((first, last, step): (i64, i64, i64)) -> Vec<i64> {
    (first..=last).step_by(step.max(1) as usize).collect()
}

/// The progress line a farm manager reads: `Imago progress: 40%`.
fn progress(done: usize, of: usize) -> String {
    format!("Imago progress: {}%", (done * 100) / of.max(1))
}

/// Run the job. What went wrong, if anything (Imago then ends with a code
/// other than 0).
pub fn run(job: &Job) -> Result<(), String> {
    let t0 = Instant::now();
    let say = move |s: &str| println!("[imago {:8.2}s] {s}", t0.elapsed().as_secs_f64());
    let scene = job
        .scene
        .canonicalize()
        .map_err(|e| format!("{}: {e}", job.scene.display()))?;
    let mut graph = NodeGraphState::default();
    let mut subnets = IceGraphStore::default();
    crate::graph_io::load(&mut graph, &mut subnets, &scene)?;
    let range = job.frames.unwrap_or((graph.scene.start, graph.scene.end, 1));
    // The frames given are the scene's range, for the nodes that use it.
    graph.scene.start = range.0;
    graph.scene.end = range.1;
    say(&format!(
        "{} ({:?}): frames {} to {}, every {}",
        scene.display(),
        job.mode,
        range.0,
        range.1,
        range.2
    ));
    let result = match job.mode {
        Mode::Render => render(&mut graph, &subnets, &scene, &frames_of(range), &say),
        Mode::Write | Mode::Run => nodes(&mut graph, &subnets, &scene, job, range.0, &say),
        Mode::Tx => tx(&mut graph, &subnets, &scene, range.0, &say),
    };
    crate::image_viewer::clean_up();
    match &result {
        Ok(()) => say("done"),
        Err(e) => say(&format!("failed: {e}")),
    }
    result
}

/// `--render`: each frame rendered by Arnold in Imago, loaded once.
fn render(
    graph: &mut NodeGraphState,
    subnets: &IceGraphStore,
    scene: &Path,
    frames: &[i64],
    say: &dyn Fn(&str),
) -> Result<(), String> {
    let renderer = arnold()?;
    // The layer in the temp folder; the products from the scene's folder.
    let outputs = scene.parent().map(Path::to_path_buf);
    let mut session = crate::live::Session::start(&renderer, &crate::image_viewer::render_dir(), outputs)?;
    for (k, frame) in frames.iter().enumerate() {
        let started = Instant::now();
        let prims = cook(graph, subnets, scene, *frame)?;
        let key = (0, (*frame as f64).to_bits());
        if k == 0 {
            session.render(prims, key, *frame as f64, false, None)?;
        } else {
            session.frame(prims, key, *frame as f64);
        }
        let images = session.finish(Duration::from_secs(48 * 3600), |n| say(n))?;
        if images.is_empty() {
            return Err("No render product: nothing would be written (Render Settings make one)".into());
        }
        let written: Vec<String> = images.iter().map(|p| p.display().to_string()).collect();
        if let Some(missing) = images.iter().find(|p| !p.exists()) {
            return Err(format!("Frame {frame}: {} was not written", missing.display()));
        }
        say(&format!(
            "frame {frame} done in {:.2}s: {}",
            started.elapsed().as_secs_f64(),
            written.join(", ")
        ));
        println!("{}", progress(k + 1, frames.len()));
    }
    Ok(())
}

/// `--write`, `--run`: each Write USD written, or each Render Command run and
/// waited for, its lines printed as it prints them.
fn nodes(
    graph: &mut NodeGraphState,
    subnets: &IceGraphStore,
    scene: &Path,
    job: &Job,
    first: i64,
    say: &dyn Fn(&str),
) -> Result<(), String> {
    // Every stage read before anything is written.
    cook(graph, subnets, scene, first)?;
    let wanted = |t: &NodeType| match job.mode {
        Mode::Write => matches!(t, NodeType::WriteUsd { .. }),
        _ => matches!(t, NodeType::RenderCommand { .. }),
    };
    let ids: Vec<(crate::types::NodeId, String)> = graph
        .nodes
        .iter()
        .filter(|n| wanted(&n.node_type) && job.node.as_ref().is_none_or(|w| *w == n.name))
        .map(|n| (n.id, n.name.clone()))
        .collect();
    if ids.is_empty() {
        let what = if job.mode == Mode::Write {
            "Write USD"
        } else {
            "Render Command"
        };
        return Err(match &job.node {
            Some(n) => format!("No {what} named {n}"),
            None => format!("No {what} in the scene"),
        });
    }
    for (k, (id, name)) in ids.iter().enumerate() {
        if job.mode == Mode::Write {
            crate::write_usd(graph, subnets, *id);
            match crate::usd_write::last(*id) {
                Some(Ok(report)) => say(&format!("{name}: written ({} frames)", report.frames.max(1))),
                Some(Err(e)) => return Err(format!("{name}: {e}")),
                None => return Err(format!("{name}: nothing written")),
            }
        } else {
            say(&format!("{name}: run"));
            crate::run_command(graph, subnets, *id);
            wait_for(id.0, name, say)?;
        }
        println!("{}", progress(k + 1, ids.len()));
    }
    Ok(())
}

/// A Render Command's run, to its end: each line it prints, and its own
/// progress (husk's percentages).
fn wait_for(key: usize, name: &str, say: &dyn Fn(&str)) -> Result<(), String> {
    use crate::core::command::{snapshot, State};
    let mut shown = 0;
    let mut told = None;
    loop {
        let Some(s) = snapshot(key, usize::MAX) else {
            return Err(format!("{name}: did not start"));
        };
        // The lines since the last look (the run keeps the last few thousand).
        for line in s.lines.iter().skip(shown.min(s.lines.len())) {
            println!("{line}");
        }
        shown = s.lines.len();
        if let Some(p) = s.progress.map(|p| p.round() as i64).filter(|p| Some(*p) != told) {
            println!("Imago progress: {p}%");
            told = Some(p);
        }
        match s.state {
            State::Running => std::thread::sleep(Duration::from_millis(200)),
            State::Done => {
                say(&format!("{name}: done in {:.1}s", s.elapsed.as_secs_f64()));
                return Ok(());
            }
            State::Failed(why) => return Err(format!("{name}: {why}")),
            State::Stopped => return Err(format!("{name}: stopped")),
        }
    }
}

/// `--tx`: the scene's textures, as Arnold loads it at the first frame, made
/// `.tx` files by `maketx`, each only when it is newer than its `.tx`.
fn tx(
    graph: &mut NodeGraphState,
    subnets: &IceGraphStore,
    scene: &Path,
    first: i64,
    say: &dyn Fn(&str),
) -> Result<(), String> {
    let renderer = arnold()?;
    let dir = crate::image_viewer::render_dir();
    let config = crate::live::arnold_config(&renderer, &dir, scene.parent().map(Path::to_path_buf))?;
    let prims = cook(graph, subnets, scene, first)?;
    let layer = dir.join("textures.usda");
    crate::usd_write::write_override(&prims, &layer).map_err(|e| format!("The scene could not be written: {e}"))?;
    let textures = xms_render::arnold::textures(&config, &layer, first as f64)?;
    let maketx = xms_render::arnold::maketx(&config);
    say(&format!("{} textures, made with {}", textures.len(), maketx.display()));
    for (k, t) in textures.iter().enumerate() {
        let out = std::process::Command::new(&maketx)
            .args(["-u", "--oiio"])
            .arg(t)
            .output()
            .map_err(|e| format!("{}: {e}", maketx.display()))?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr);
            return Err(format!("{}: {}", t.display(), why.trim()));
        }
        say(&format!("{}: .tx", t.display()));
        println!("{}", progress(k + 1, textures.len()));
    }
    Ok(())
}

/// Arnold, as found here: rendering in Imago needs it.
fn arnold() -> Result<crate::core::render::LiveRenderer, String> {
    crate::live::renderer_for("arnold")
        .ok_or_else(|| crate::live::why_not("arnold").unwrap_or_else(|| "Arnold does not render in Imago".into()))
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
                mode: Mode::Render,
                frames: Some((1, 100, 1)),
                node: None,
            }))
        );
        let j = job(&args("xms --headless --run 5 shot.xms --node husk_beauty"))
            .unwrap()
            .unwrap();
        assert_eq!(
            (j.mode, j.frames, j.node.as_deref()),
            (Mode::Run, Some((5, 5, 1)), Some("husk_beauty"))
        );
        let j = job(&args("xms --headless shot.xms --write")).unwrap().unwrap();
        assert_eq!((j.mode, j.frames), (Mode::Write, None), "the scene's range");
        assert_eq!(
            job(&args("xms --headless shot.xms --tx")).unwrap().unwrap().mode,
            Mode::Tx
        );
        let err = |line: &str| job(&args(line)).unwrap().unwrap_err();
        assert!(err("xms --headless --render").starts_with("Which scene?"));
        assert!(err("xms --headless a.xms").starts_with("What to do"));
        assert!(err("xms --headless a.xms --render --write").starts_with("One of"));
        assert!(err("xms --headless a.xms --render --node").contains("--node needs a name"));
        assert!(err("xms --headless a.xms --fast").contains("--fast"));
    }

    #[test]
    fn frames_are_one_a_range_or_a_range_by_steps() {
        assert_eq!(range("12"), Ok((12, 12, 1)));
        assert_eq!(range("1-100"), Ok((1, 100, 1)));
        assert_eq!(range("1-100x2"), Ok((1, 100, 2)));
        assert_eq!(range("-5-10"), Ok((-5, 10, 1)));
        assert!(range("10-1").is_err() && range("1-10x0").is_err() && range("a").is_err());
        assert_eq!(frames_of((1, 7, 3)), [1, 4, 7]);
    }

    #[test]
    fn progress_is_a_line_a_farm_manager_reads() {
        assert_eq!(progress(1, 4), "Imago progress: 25%");
        assert_eq!(progress(4, 4), "Imago progress: 100%");
        assert_eq!(progress(0, 0), "Imago progress: 0%");
    }
}
