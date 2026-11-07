//! Step 1 of `docs/arnold-native.md`, outside Imago: Arnold, loaded at run
//! time from the SDK at `$ARNOLD_PATH`, loads a layer Imago wrote (USD, read
//! by Arnold itself), renders it progressively into the files its Render
//! Products name, and takes a light's intensity live, the light found by its
//! prim path. It prints what it finds: which nodes Arnold made and their
//! names, when each image was written, and how long the edit took to show.
//!
//! ```text
//! arnold-prototype scene.usda [--light /lights/key] [--frame 1]
//!     [--edit-after 4] [--seconds 40] [--licence]
//! ```

mod arnold;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use arnold::{Api, Ptr};

const USAGE: &str =
    "arnold-prototype scene.usda [--light /lights/key] [--frame 1] [--edit-after 4] [--seconds 40] [--licence]
  --light       the light's prim path (else the first light Arnold made)
  --frame       the frame read from the layer (1)
  --edit-after  seconds after the render starts to change the light's intensity (4)
  --seconds     seconds before stopping (40)
  --licence     look for an Arnold licence (else none: watermarked, no wait)
ARNOLD_PATH is the Arnold SDK's folder (the one with bin/ and plugins/).";

struct Args {
    scene: PathBuf,
    light: Option<String>,
    frame: f32,
    edit_after: f64,
    seconds: f64,
    licence: bool,
}

fn args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        scene: PathBuf::new(),
        light: None,
        frame: 1.0,
        edit_after: 4.0,
        seconds: 40.0,
        licence: false,
    };
    while let Some(arg) = it.next() {
        let mut value = || it.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--light" => a.light = Some(value()?),
            "--frame" => a.frame = value()?.parse().map_err(|_| "--frame: a number")?,
            "--edit-after" => a.edit_after = value()?.parse().map_err(|_| "--edit-after: seconds")?,
            "--seconds" => a.seconds = value()?.parse().map_err(|_| "--seconds: seconds")?,
            "--licence" => a.licence = true,
            "-h" | "--help" => return Err(String::new()),
            _ if a.scene.as_os_str().is_empty() && !arg.starts_with('-') => a.scene = arg.into(),
            _ => return Err(format!("not an option: {arg}")),
        }
    }
    if a.scene.as_os_str().is_empty() {
        return Err("which layer? (scene.usda)".into());
    }
    Ok(a)
}

fn main() {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("{e}\n");
            }
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = run(&a) {
        eprintln!("\nstopped: {e}");
        std::process::exit(1);
    }
}

/// Seconds since `t0`, for the log.
fn at(t0: Instant) -> String {
    format!("{:7.2}s", t0.elapsed().as_secs_f64())
}

fn run(a: &Args) -> Result<(), String> {
    let sdk = std::env::var_os("ARNOLD_PATH").ok_or("ARNOLD_PATH is not set: the Arnold SDK's folder")?;
    let ai = Api::load(Path::new(&sdk))?;
    let version = ai.version();
    println!("Arnold {version}");
    if version
        .split('.')
        .next()
        .and_then(|m| m.parse::<u32>().ok())
        .unwrap_or(0)
        < 7
    {
        return Err(format!(
            "Arnold {version}: the prototype needs Arnold 7 (render sessions)"
        ));
    }
    let t0 = Instant::now();
    // SAFETY: the calls below follow the API reference: one session, nodes
    // read and changed only while the render is not running.
    unsafe {
        (ai.AiBegin)(arnold::SESSION_INTERACTIVE);
        let universe = (ai.AiUniverse)();
        let options = (ai.AiUniverseGetOptions)(universe);
        if !a.licence {
            (ai.AiNodeSetBool)(options, ai.string("skip_license_check"), true);
        }
        let params = (ai.AiParamValueMap)();
        (ai.AiParamValueMapSetFlt)(params, ai.string("frame"), a.frame);
        let scene = std::ffi::CString::new(a.scene.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        let loaded = (ai.AiSceneLoad)(universe, scene.as_ptr(), params);
        (ai.AiParamValueMapDestroy)(params);
        let load_time = t0.elapsed();
        if !loaded {
            return Err(format!(
                "Arnold could not load {} (is the USD scene format in {}/plugins?)",
                a.scene.display(),
                Path::new(&sdk).display()
            ));
        }
        println!(
            "{} loaded {} in {:.2}s (frame {})",
            at(t0),
            a.scene.display(),
            load_time.as_secs_f64(),
            a.frame
        );
        // The layer's Render Settings may set the options: set it again.
        if !a.licence {
            (ai.AiNodeSetBool)(options, ai.string("skip_license_check"), true);
        }

        // Question 2: are the nodes named by prim path?
        let nodes = ai.nodes(universe, arnold::NODE_ALL);
        let by_path = nodes.iter().filter(|(_, n, _)| n.starts_with('/')).count();
        println!("\n{} nodes, {} named by a path:", nodes.len(), by_path);
        for (kind, name, _) in &nodes {
            println!("  {kind:<24} {name}");
        }

        let drivers: Vec<(String, Ptr)> = ai
            .nodes(universe, arnold::NODE_DRIVER)
            .into_iter()
            .map(|(_, _, d)| (ai.str_param(d, "filename"), d))
            .filter(|(f, _)| !f.is_empty())
            .collect();
        println!("\nimages written:");
        for (file, _) in &drivers {
            println!("  {file}");
        }
        if drivers.is_empty() {
            println!("  none: the layer's Render Products gave Arnold no file to write");
        }

        let light = match &a.light {
            Some(path) => {
                let l = (ai.AiNodeLookUpByName)(universe, ai.string(path), std::ptr::null_mut());
                if l.is_null() {
                    println!("\nno node named {path}: looked up by its prim path, Arnold did not find it");
                }
                (!l.is_null()).then(|| (path.clone(), l))
            }
            None => ai
                .nodes(universe, arnold::NODE_LIGHT)
                .into_iter()
                .next()
                .map(|(_, n, l)| (n, l)),
        };
        let intensity = ai.string("intensity");
        if let Some((name, l)) = &light {
            println!("\nlight {name}: intensity {}", (ai.AiNodeGetFlt)(*l, intensity));
        }

        // Question 1 and 3: progressive passes, then the edit while it runs.
        let session = (ai.AiRenderSession)(universe, arnold::SESSION_INTERACTIVE);
        let code = (ai.AiRenderBegin)(
            session,
            arnold::RENDER_MODE_CAMERA,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        let started = Instant::now();
        let why = arnold::ERRORS.get(code as usize).copied().unwrap_or("an error");
        if code != 0 {
            return Err(format!(
                "the render did not start: {why}. Use a layer made to render: Render Commons, a camera and a light, written by Write USD (the DeLorean render template's, say)"
            ));
        }
        println!("\n{} render started", at(t0));
        let mut written: Vec<Option<SystemTime>> = drivers.iter().map(|(f, _)| modified(f)).collect();
        let mut status = -1;
        let mut edited: Option<Instant> = None;
        let mut writes = 0;
        loop {
            let now = (ai.AiRenderGetStatus)(session);
            if now != status {
                status = now;
                let name = arnold::STATUS.get(now as usize).copied().unwrap_or("?");
                println!("{} {name}", at(t0));
            }
            for (k, (file, _)) in drivers.iter().enumerate() {
                let m = modified(file);
                if m.is_some() && m != written[k] {
                    written[k] = m;
                    writes += 1;
                    let since = match edited {
                        Some(e) => format!(", {:.2}s after the edit", e.elapsed().as_secs_f64()),
                        None => String::new(),
                    };
                    println!("{} written: {file} (write {writes}{since})", at(t0));
                }
            }
            if status == arnold::FAILED {
                return Err("the render failed: Arnold's log says why".into());
            }
            let elapsed = started.elapsed().as_secs_f64();
            if edited.is_none() && elapsed >= a.edit_after {
                if let Some((name, l)) = &light {
                    let before = (ai.AiNodeGetFlt)(*l, intensity);
                    // Change nodes only while the render is paused.
                    if status != arnold::FINISHED {
                        (ai.AiRenderInterrupt)(session, 0);
                        let wait = Instant::now();
                        while matches!((ai.AiRenderGetStatus)(session), arnold::RENDERING | arnold::RESTARTING)
                            && wait.elapsed() < Duration::from_secs(10)
                        {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                    (ai.AiNodeSetFlt)(*l, intensity, before * 4.0);
                    (ai.AiRenderRestart)(session);
                    println!(
                        "{} edit: {name} intensity {before} -> {} (no reload)",
                        at(t0),
                        (ai.AiNodeGetFlt)(*l, intensity)
                    );
                }
                edited = Some(Instant::now());
            }
            if elapsed >= a.seconds
                || (status == arnold::FINISHED && edited.is_some_and(|e| e.elapsed().as_secs_f64() > 1.0))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        (ai.AiRenderInterrupt)(session, 0);
        (ai.AiRenderEnd)(session);
        (ai.AiRenderSessionDestroy)(session);
        (ai.AiUniverseDestroy)(universe);
        (ai.AiEnd)();
        println!(
            "\n{} done: {writes} image writes; load {:.2}s. Send this whole output back.",
            at(t0),
            load_time.as_secs_f64()
        );
    }
    Ok(())
}

/// When the file was last written, if it exists.
fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}
