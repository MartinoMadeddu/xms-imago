//! The Arnold backend by hand (CI has no Arnold): a layer Imago wrote, rendered
//! live, with edits sent as Imago will send them.
//!
//! ```text
//! ARNOLD_PATH=/path/to/Arnold-7.x-SDK cargo run --release -p xms-render --example arnold_live -- \
//!     scene.usda --light /lights/key [--move /root/statue/statue] [--final]
//! ```
//!
//! It loads the layer, edits the light's intensity during the first passes,
//! again once the render has finished (a restart after the end), moves a
//! shape if asked, and asks for a change Arnold cannot make live (a reload).
//! `--final`: once at the Render Settings' samples, no passes (IPR off).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use xms_render::arnold::{Arnold, Config};
use xms_render::{Change, Edit, Event, Live, Renderer, Value};

/// The layer as it is: the edits here change nothing Imago would write.
fn unchanged(layer: &Path) -> impl FnOnce() -> Result<PathBuf, String> + Send + 'static {
    let layer = layer.to_path_buf();
    move || Ok(layer)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut layer = None;
    let (mut light, mut moved, mut progressive) = (None, None, true);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--light" => light = args.next(),
            "--move" => moved = args.next(),
            "--final" => progressive = false,
            _ => layer = Some(PathBuf::from(a)),
        }
    }
    let Some(layer) = layer else {
        eprintln!("arnold_live scene.usda --light /lights/key [--move /path/to/a/shape]");
        std::process::exit(2);
    };
    let light = light.unwrap_or_else(|| "/lights/key".into());
    let t0 = Instant::now();
    let materialx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../xms-matx/libraries");
    let live = Live::start(move |pixels| {
        let config = Config {
            materialx: materialx.canonicalize().ok(),
            ..Config::from_env()?
        };
        Ok(Box::new(Arnold::new(config, pixels)?) as Box<dyn Renderer>)
    });
    // Each event as it comes, until one `wanted` accepts (a minute at most).
    let until = |wanted: fn(&Event) -> bool| {
        let end = Instant::now() + Duration::from_secs(60);
        while Instant::now() < end {
            for e in live.wait(Duration::from_millis(20), |_| true) {
                println!("{:7.2}s {e:?}", t0.elapsed().as_secs_f64());
                if e == Event::Finished {
                    // The image in memory, bucket by bucket (Imago's driver).
                    let f = live.pixels().frame();
                    let lit = f.rgba.iter().filter(|p| p[0] + p[1] + p[2] > 0.0).count();
                    let layers: Vec<String> = live.pixels().layers().iter().map(|l| l.name.clone()).collect();
                    println!(
                        "         pixels in memory: {}x{}, {lit} lit, {} buckets in; layers {layers:?}",
                        f.width,
                        f.height,
                        live.pixels().generation()
                    );
                }
                if wanted(&e) {
                    return;
                }
            }
        }
    };
    let pass: fn(&Event) -> bool = |e| matches!(e, Event::Pass { .. } | Event::Failed(_));
    let finished: fn(&Event) -> bool = |e| matches!(e, Event::Finished | Event::Failed(_));
    let intensity = |x: f32| Edit {
        path: light.clone(),
        change: Change::Attributes(vec![("inputs:intensity".into(), Value::Float(x))]),
    };

    println!("load {}", layer.display());
    live.load(&layer, 1.0, progressive, None);
    until(pass);

    println!("\nedit while it renders: {light} intensity 4");
    live.edit(vec![intensity(4.0)], unchanged(&layer));
    until(finished);

    println!("\nedit after it finished: {light} intensity 0.5");
    live.edit(vec![intensity(0.5)], unchanged(&layer));
    until(finished);

    if let Some(path) = moved {
        println!("\nmove {path}: its world matrix a translation of 0.05 in z");
        let mut m = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        m[3][2] = 0.05;
        live.edit(
            vec![Edit {
                path,
                change: Change::Transform(m),
            }],
            unchanged(&layer),
        );
        until(finished);
    }

    println!("\na change Arnold cannot make live: a reload");
    live.edit(
        vec![Edit {
            path: "/root".into(),
            change: Change::Structure,
        }],
        unchanged(&layer),
    );
    until(finished);
    println!(
        "\ndone in {:.2}s. Send this whole output back.",
        t0.elapsed().as_secs_f64()
    );
}
