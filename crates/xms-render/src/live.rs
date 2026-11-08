//! A renderer on a thread of its own: Arnold is driven from one thread, and
//! Imago's never waits on it. Commands queue; those that arrive while one is
//! being done are taken together, so a slider dragged is one edit, not many.

use std::collections::BTreeMap;
use std::mem::discriminant;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use std::sync::Arc;

use crate::{Change, Edit, Event, Pixels, Renderer};

/// How often the renderer is asked how it is doing.
const POLL: Duration = Duration::from_millis(20);

/// Writes the layer as it is with an edit's changes, and says where: called
/// only when they cannot be applied live.
pub type Layer = Box<dyn FnOnce() -> Result<PathBuf, String> + Send>;

enum Command {
    Load {
        layer: PathBuf,
        frame: f64,
        progressive: bool,
    },
    Edit {
        edits: Vec<Edit>,
        layer: Layer,
    },
    Stop,
}

/// A live render: load a layer, then send it edits; its [`Event`]s say how it
/// is doing.
pub struct Live {
    pixels: Arc<Pixels>,
    commands: Option<Sender<Command>>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Live {
    /// The renderer `make` makes, on the thread it is driven from, given the
    /// image to fill as it renders.
    pub fn start<F>(make: F) -> Live
    where
        F: FnOnce(Arc<Pixels>) -> Result<Box<dyn Renderer>, String> + Send + 'static,
    {
        let pixels = Arc::new(Pixels::default());
        let (commands, inbox) = channel();
        let (outbox, events) = channel();
        let theirs = pixels.clone();
        let thread = std::thread::Builder::new()
            .name("xms-render".into())
            .spawn(move || match make(theirs) {
                Ok(renderer) => run(renderer, inbox, outbox),
                Err(e) => {
                    let _ = outbox.send(Event::Failed(e));
                }
            })
            .ok();
        Live {
            pixels,
            commands: Some(commands),
            events,
            thread,
        }
    }

    /// Render `layer` at `frame`, in place of what was rendered: `progressive`
    /// (IPR), or once at the Render Settings' samples.
    pub fn load(&self, layer: impl Into<PathBuf>, frame: f64, progressive: bool) {
        self.send(Command::Load {
            layer: layer.into(),
            frame,
            progressive,
        });
    }

    /// Changes since the layer was loaded: applied live, or, when they cannot
    /// be, the layer `layer` writes loaded.
    pub fn edit(&self, edits: Vec<Edit>, layer: impl FnOnce() -> Result<PathBuf, String> + Send + 'static) {
        self.send(Command::Edit {
            edits,
            layer: Box::new(layer),
        });
    }

    /// Stop rendering; a load or an edit starts it again.
    pub fn stop(&self) {
        self.send(Command::Stop);
    }

    /// The image as it renders, bucket by bucket.
    pub fn pixels(&self) -> &Arc<Pixels> {
        &self.pixels
    }

    /// What happened since it was last asked.
    pub fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Wait up to `timeout` for an event `wanted` accepts; the events until
    /// then, it last.
    pub fn wait(&self, timeout: Duration, wanted: impl Fn(&Event) -> bool) -> Vec<Event> {
        let end = Instant::now() + timeout;
        let mut seen = vec![];
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            match self.events.recv_timeout(left) {
                Ok(e) => {
                    let done = wanted(&e);
                    seen.push(e);
                    if done {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        seen
    }

    fn send(&self, c: Command) {
        if let Some(tx) = &self.commands {
            let _ = tx.send(c);
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        // Closing the channel ends the thread, which stops the renderer.
        self.commands = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run(mut renderer: Box<dyn Renderer>, inbox: Receiver<Command>, outbox: Sender<Event>) {
    // The frame loaded, once a layer is, and whether it renders in passes.
    let mut loaded: Option<(f64, bool)> = None;
    let mut send = |e: Event| {
        let _ = outbox.send(e);
    };
    loop {
        let first = match inbox.recv_timeout(POLL) {
            Ok(c) => Some(c),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        if let Some(first) = first {
            let batch: Vec<Command> = std::iter::once(first).chain(inbox.try_iter()).collect();
            for c in taken_together(batch) {
                match c {
                    Command::Stop => renderer.stop(),
                    Command::Load {
                        layer,
                        frame,
                        progressive,
                    } => {
                        reload(&mut *renderer, &layer, (frame, progressive), &mut send);
                        loaded = Some((frame, progressive));
                    }
                    Command::Edit { edits, layer } => {
                        let at = loaded.unwrap_or((1.0, true));
                        let live = loaded.is_some() && edits.iter().all(|e| renderer.live(&e.change));
                        let applied = live
                            && match renderer.edit(&edits) {
                                Ok(applied) => applied,
                                Err(e) => {
                                    send(Event::Failed(e));
                                    false
                                }
                            };
                        if applied {
                            match renderer.render(at.1) {
                                Ok(()) => send(Event::Edited(edits.len())),
                                Err(e) => send(Event::Failed(e)),
                            }
                        } else {
                            match layer() {
                                Ok(layer) => reload(&mut *renderer, &layer, at, &mut send),
                                Err(e) => send(Event::Failed(e)),
                            }
                        }
                        loaded = Some(at);
                    }
                }
            }
        }
        for e in renderer.poll() {
            send(e);
        }
    }
    renderer.stop();
}

/// Load the layer at the frame `at` gives, and render it (in passes, if it
/// says).
fn reload(renderer: &mut dyn Renderer, layer: &std::path::Path, at: (f64, bool), send: &mut impl FnMut(Event)) {
    let (frame, progressive) = at;
    let t = Instant::now();
    renderer.stop();
    match renderer.load(layer, frame) {
        Ok(images) => {
            send(Event::Loaded {
                seconds: t.elapsed().as_secs_f64(),
                images,
            });
            if let Err(e) = renderer.render(progressive) {
                send(Event::Failed(e));
            }
        }
        Err(e) => send(Event::Failed(e)),
    }
}

/// Commands that came in together, as few: what a load replaces is dropped,
/// and edits after it are one, the last change of a kind to a prim winning
/// (attributes by name).
fn taken_together(batch: Vec<Command>) -> Vec<Command> {
    let mut out: Vec<Command> = vec![];
    for c in batch {
        match c {
            Command::Load { .. } => {
                out.retain(|c| matches!(c, Command::Stop));
                out.push(c);
            }
            Command::Edit { edits, layer } => match out.last_mut() {
                Some(Command::Edit {
                    edits: before,
                    layer: last,
                }) => {
                    before.extend(edits);
                    *before = merged(std::mem::take(before));
                    *last = layer;
                }
                _ => out.push(Command::Edit {
                    edits: merged(edits),
                    layer,
                }),
            },
            Command::Stop => out.push(c),
        }
    }
    out
}

/// One edit per prim and kind of change, in the order first made.
fn merged(edits: Vec<Edit>) -> Vec<Edit> {
    let mut out: Vec<Edit> = vec![];
    for e in edits {
        let same = out
            .iter_mut()
            .find(|o| o.path == e.path && discriminant(&o.change) == discriminant(&e.change));
        match (same, e.change) {
            (Some(o), Change::Attributes(new)) | (Some(o), Change::Settings(new)) => {
                if let Change::Attributes(old) | Change::Settings(old) = &mut o.change {
                    let mut by_name: BTreeMap<String, usize> =
                        old.iter().enumerate().map(|(k, (n, _))| (n.clone(), k)).collect();
                    for (n, v) in new {
                        match by_name.get(&n) {
                            Some(&k) => old[k].1 = v,
                            None => {
                                by_name.insert(n.clone(), old.len());
                                old.push((n, v));
                            }
                        }
                    }
                }
            }
            (Some(o), change) => o.change = change,
            (None, change) => out.push(Edit { path: e.path, change }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    /// A renderer that says what it was asked: CI has no Arnold.
    struct StandIn {
        calls: Arc<Mutex<Vec<String>>>,
        passes: u32,
    }

    impl Renderer for StandIn {
        fn load(&mut self, layer: &Path, frame: f64) -> Result<Vec<PathBuf>, String> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("load {} {frame}", layer.display()));
            if layer.ends_with("broken.usda") {
                return Err("no such layer".into());
            }
            Ok(vec!["render/beauty.0001.exr".into()])
        }
        fn render(&mut self, _: bool) -> Result<(), String> {
            self.calls.lock().unwrap().push("render".into());
            self.passes = 2;
            Ok(())
        }
        fn live(&self, change: &Change) -> bool {
            !matches!(change, Change::Structure)
        }
        fn edit(&mut self, edits: &[Edit]) -> Result<bool, String> {
            let mut calls = self.calls.lock().unwrap();
            for e in edits {
                calls.push(format!("edit {} {:?}", e.path, e.change));
            }
            Ok(!edits.iter().any(|e| e.path == "/missing"))
        }
        fn poll(&mut self) -> Vec<Event> {
            match self.passes {
                0 => vec![],
                n => {
                    self.passes -= 1;
                    if n == 1 {
                        vec![Event::Pass { index: 1, total: 2 }, Event::Finished]
                    } else {
                        vec![Event::Pass { index: 0, total: 2 }]
                    }
                }
            }
        }
        fn stop(&mut self) {
            self.calls.lock().unwrap().push("stop".into());
        }
    }

    fn stand_in() -> (Live, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(vec![]));
        let c = calls.clone();
        let live = Live::start(move |_| Ok(Box::new(StandIn { calls: c, passes: 0 }) as Box<dyn Renderer>));
        (live, calls)
    }

    const SECOND: Duration = Duration::from_secs(1);

    fn written(file: &'static str) -> Layer {
        Box::new(move || Ok(file.into()))
    }

    fn light(intensity: f32) -> Edit {
        Edit {
            path: "/lights/key".into(),
            change: Change::Attributes(vec![("inputs:intensity".into(), Value::Float(intensity))]),
        }
    }

    #[test]
    fn a_layer_loads_and_renders_in_passes() {
        let (live, calls) = stand_in();
        live.load("scene.usda", 1.0, true);
        let events = live.wait(SECOND, |e| *e == Event::Finished);
        assert!(matches!(&events[0], Event::Loaded { images, .. } if images[0].ends_with("beauty.0001.exr")));
        assert_eq!(
            events[1..],
            [
                Event::Pass { index: 0, total: 2 },
                Event::Pass { index: 1, total: 2 },
                Event::Finished
            ]
        );
        assert_eq!(calls.lock().unwrap()[..], ["stop", "load scene.usda 1", "render"]);
    }

    #[test]
    fn edits_it_can_apply_are_live_others_reload() {
        let (live, calls) = stand_in();
        live.load("scene.usda", 1.0, true);
        live.wait(SECOND, |e| *e == Event::Finished);
        live.edit(vec![light(4.0)], || Ok("scene.usda".into()));
        assert!(live
            .wait(SECOND, |e| *e == Event::Edited(1))
            .contains(&Event::Edited(1)));
        live.edit(
            vec![
                light(2.0),
                Edit {
                    path: "/asset".into(),
                    change: Change::Structure,
                },
            ],
            || Ok("scene2.usda".into()),
        );
        assert!(live
            .wait(SECOND, |e| matches!(e, Event::Loaded { .. }))
            .iter()
            .any(|e| matches!(e, Event::Loaded { .. })));
        // A prim the renderer has no node for: loaded again too.
        live.edit(
            vec![Edit {
                path: "/missing".into(),
                change: Change::Visible(false),
            }],
            || Ok("scene3.usda".into()),
        );
        live.wait(SECOND, |e| matches!(e, Event::Loaded { .. }));
        let calls = calls.lock().unwrap();
        assert!(calls.contains(&"load scene2.usda 1".to_string()));
        assert!(calls.contains(&"load scene3.usda 1".to_string()));
        assert!(!calls.iter().any(|c| c.contains("edit /asset")));
    }

    #[test]
    fn a_layer_that_will_not_load_says_why() {
        let (live, _) = stand_in();
        live.load("broken.usda", 1.0, true);
        let events = live.wait(SECOND, |e| matches!(e, Event::Failed(_)));
        assert_eq!(events.last(), Some(&Event::Failed("no such layer".into())));
    }

    #[test]
    fn edits_that_come_together_are_one() {
        let attrs =
            |v: &[(&str, f32)]| Change::Attributes(v.iter().map(|(n, x)| (n.to_string(), Value::Float(*x))).collect());
        let batch = vec![
            Command::Load {
                layer: "a.usda".into(),
                frame: 1.0,
                progressive: true,
            },
            Command::Edit {
                edits: vec![light(2.0)],
                layer: written("a.usda"),
            },
            Command::Edit {
                edits: vec![
                    light(3.0),
                    Edit {
                        path: "/lights/key".into(),
                        change: attrs(&[("inputs:exposure", 1.0)]),
                    },
                    Edit {
                        path: "/asset".into(),
                        change: Change::Visible(false),
                    },
                ],
                layer: written("b.usda"),
            },
        ];
        let out = taken_together(batch);
        assert_eq!(out.len(), 2);
        let Command::Edit { edits, .. } = out.into_iter().nth(1).unwrap() else {
            panic!()
        };
        // The last layer is the one written, if it must be.
        let Command::Edit { layer, .. } = taken_together(vec![
            Command::Edit {
                edits: vec![light(2.0)],
                layer: written("a.usda"),
            },
            Command::Edit {
                edits: vec![light(3.0)],
                layer: written("b.usda"),
            },
        ])
        .remove(0) else {
            panic!()
        };
        assert_eq!(layer().unwrap(), Path::new("b.usda"));
        assert_eq!(edits.len(), 2);
        assert_eq!(
            edits[0].change,
            attrs(&[("inputs:intensity", 3.0), ("inputs:exposure", 1.0)])
        );
        assert_eq!(edits[1].change, Change::Visible(false));
        // A load drops what came before it.
        let out = taken_together(vec![
            Command::Edit {
                edits: vec![light(2.0)],
                layer: written("a.usda"),
            },
            Command::Load {
                layer: "c.usda".into(),
                frame: 2.0,
                progressive: true,
            },
        ]);
        assert!(matches!(&out[..], [Command::Load { frame, .. }] if *frame == 2.0));
    }
}
