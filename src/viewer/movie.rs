//! Export Movie: the Viewer's picture, frame by frame over the timeline's
//! range, written as an MP4 (H.264 or H.265) by ffmpeg.
//!
//! The picture is drawn again for each frame into a texture of the movie's
//! size (not the pane's), through the Viewer's display transform, read back,
//! and piped to ffmpeg as raw RGBA, which encodes it on a thread of its own.
//! The playhead steps one frame per interface frame and goes back where it was
//! when the movie is done. ffmpeg must be on the PATH, built with libx264
//! (H.264) or libx265 (H.265).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    H265,
}

impl Codec {
    pub const ALL: [Codec; 2] = [Codec::H264, Codec::H265];
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::H265 => "H.265",
        }
    }
    fn encoder(self) -> &'static str {
        match self {
            Codec::H264 => "libx264",
            Codec::H265 => "libx265",
        }
    }
}

/// The movie's size: the Viewer's own, or a width with the height from the
/// camera looked through (its film's aspect), else 16:9.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Viewer,
    W1280,
    W1920,
    W3840,
}

impl Size {
    pub const ALL: [Size; 4] = [Size::Viewer, Size::W1280, Size::W1920, Size::W3840];
    pub fn label(self) -> &'static str {
        match self {
            Size::Viewer => "Viewer size",
            Size::W1280 => "1280 wide (720p)",
            Size::W1920 => "1920 wide (1080p)",
            Size::W3840 => "3840 wide (UHD)",
        }
    }
    fn width(self) -> Option<u32> {
        match self {
            Size::Viewer => None,
            Size::W1280 => Some(1280),
            Size::W1920 => Some(1920),
            Size::W3840 => Some(3840),
        }
    }
    /// Pixels, even both ways (4:2:0 needs it). `viewer`: the pane in
    /// pixels; `aspect`: the camera's film, when looking through one.
    pub fn pixels(self, viewer: [u32; 2], aspect: Option<f32>) -> [u32; 2] {
        let even = |v: f32| ((v.round() as u32) & !1).max(2);
        match self.width() {
            None => [even(viewer[0] as f32), even(viewer[1] as f32)],
            Some(w) => {
                let a = aspect.filter(|a| a.is_finite() && *a > 0.0).unwrap_or(16.0 / 9.0);
                [w, even(w as f32 / a)]
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    High,
    Medium,
    Small,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::High, Quality::Medium, Quality::Small];
    pub fn label(self) -> &'static str {
        match self {
            Quality::High => "High",
            Quality::Medium => "Medium",
            Quality::Small => "Small file",
        }
    }
    /// ffmpeg's constant rate factor for the codec: lower is better.
    fn crf(self, codec: Codec) -> u32 {
        match (codec, self) {
            (Codec::H264, Quality::High) => 16,
            (Codec::H264, Quality::Medium) => 21,
            (Codec::H264, Quality::Small) => 27,
            (Codec::H265, Quality::High) => 18,
            (Codec::H265, Quality::Medium) => 24,
            (Codec::H265, Quality::Small) => 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub codec: Codec,
    pub size: Size,
    pub quality: Quality,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            codec: Codec::H264,
            size: Size::W1920,
            quality: Quality::High,
        }
    }
}

static CHOSEN: Mutex<Option<Settings>> = Mutex::new(None);

impl Settings {
    fn to_text(self) -> String {
        format!(
            "codec={}\nsize={}\nquality={}\n",
            Codec::ALL.iter().position(|c| *c == self.codec).unwrap_or(0),
            Size::ALL.iter().position(|c| *c == self.size).unwrap_or(0),
            Quality::ALL.iter().position(|c| *c == self.quality).unwrap_or(0),
        )
    }
    fn from_text(t: &str) -> Self {
        let mut s = Self::default();
        for line in t.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let Ok(i) = v.trim().parse::<usize>() else { continue };
            match k.trim() {
                "codec" => s.codec = Codec::ALL.get(i).copied().unwrap_or(s.codec),
                "size" => s.size = Size::ALL.get(i).copied().unwrap_or(s.size),
                "quality" => s.quality = Quality::ALL.get(i).copied().unwrap_or(s.quality),
                _ => {}
            }
        }
        s
    }
    /// The settings last chosen, read from the config folder once.
    pub fn load() -> Self {
        let mut g = CHOSEN.lock().unwrap_or_else(|e| e.into_inner());
        *g.get_or_insert_with(|| {
            crate::file_browser::config_path("movie.txt")
                .and_then(|f| std::fs::read_to_string(f).ok())
                .map(|t| Self::from_text(&t))
                .unwrap_or_default()
        })
    }
    pub fn save(self) {
        if let Ok(mut g) = CHOSEN.lock() {
            *g = Some(self);
        }
        let Some(file) = crate::file_browser::config_path("movie.txt") else { return };
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(file, self.to_text());
    }
}

/// What the menu asked for and what it shows, between the interface and the
/// Viewer.
#[derive(Default)]
struct Shared {
    /// A movie to start, written here.
    request: Option<(PathBuf, Settings)>,
    /// Frames written and to write, while a movie is being made.
    progress: Option<(usize, usize)>,
    stop: bool,
    /// What happened, for the status line.
    message: Option<String>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Shared::default))
}

/// Make a movie at `path` with these settings, from the next frame.
pub fn request(path: PathBuf, settings: Settings) {
    with(|s| s.request = Some((path, settings)));
}
/// Frames written and to write, while a movie is being made.
pub fn progress() -> Option<(usize, usize)> {
    with(|s| s.progress)
}
pub fn stop() {
    with(|s| s.stop = true);
}
/// What became of the last movie, once.
pub fn take_message() -> Option<String> {
    with(|s| s.message.take())
}
fn say(m: String) {
    with(|s| s.message = Some(m));
}

/// The file name with .mp4.
pub fn with_extension(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")) {
        path.to_path_buf()
    } else {
        path.with_extension("mp4")
    }
}

/// ffmpeg's arguments: raw RGBA frames in on stdin (the display's colours, as
/// the Viewer shows them), an MP4 out, Rec. 709 at video levels.
pub fn ffmpeg_args(size: [u32; 2], rate: (u32, u32), s: Settings, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-s",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(format!("{}x{}", size[0], size[1]));
    a.push("-framerate".into());
    a.push(format!("{}/{}", rate.0.max(1), rate.1.max(1)));
    a.extend(["-i", "-", "-an", "-c:v", s.codec.encoder()].map(String::from));
    a.push("-crf".into());
    a.push(s.quality.crf(s.codec).to_string());
    a.extend(
        [
            "-preset",
            "medium",
            "-vf",
            "scale=out_color_matrix=bt709:out_range=tv",
            "-pix_fmt",
            "yuv420p",
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_range",
            "tv",
            "-movflags",
            "+faststart",
        ]
        .map(String::from),
    );
    if s.codec == Codec::H265 {
        // QuickTime and Apple's players play H.265 only tagged hvc1.
        a.extend(["-tag:v", "hvc1", "-x265-params", "log-level=error"].map(String::from));
    }
    a.push(out.to_string_lossy().into_owned());
    a
}

/// ffmpeg, fed frames from a channel on a thread of its own.
pub struct Writer {
    tx: Mutex<Option<SyncSender<Vec<u8>>>>,
    thread: Mutex<Option<std::thread::JoinHandle<Result<(), String>>>>,
}

impl Writer {
    fn start(size: [u32; 2], rate: (u32, u32), s: Settings, out: &Path) -> Result<Writer, String> {
        let mut child: Child = Command::new("ffmpeg")
            .args(ffmpeg_args(size, rate, s, out))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Could not start ffmpeg ({e}): it must be installed and on the PATH"))?;
        let mut stdin = child.stdin.take().ok_or("ffmpeg has no input")?;
        let mut stderr = child.stderr.take();
        // A few frames ahead of ffmpeg at most.
        let (tx, rx) = sync_channel::<Vec<u8>>(4);
        let thread = std::thread::spawn(move || {
            let mut broken = false;
            for frame in rx {
                if !broken && stdin.write_all(&frame).is_err() {
                    broken = true;
                }
            }
            drop(stdin);
            let mut err = String::new();
            if let Some(e) = stderr.as_mut() {
                let _ = e.read_to_string(&mut err);
            }
            match child.wait() {
                Ok(st) if st.success() && !broken => Ok(()),
                Ok(st) => Err(format!(
                    "ffmpeg failed ({st}): {}",
                    err.trim().lines().last().unwrap_or("no message")
                )),
                Err(e) => Err(format!("ffmpeg: {e}")),
            }
        });
        Ok(Writer {
            tx: Mutex::new(Some(tx)),
            thread: Mutex::new(Some(thread)),
        })
    }

    /// One frame, tightly packed RGBA rows, top first.
    pub fn send(&self, frame: Vec<u8>) {
        if let Some(tx) = self.tx.lock().ok().and_then(|g| g.clone()) {
            let _ = tx.send(frame);
        }
    }

    /// No more frames: ffmpeg finishes the file, waited for on a thread of
    /// its own, and says how it went.
    fn finish(&self, done: String) {
        if let Ok(mut g) = self.tx.lock() {
            g.take();
        }
        let thread = self.thread.lock().ok().and_then(|mut g| g.take());
        std::thread::spawn(move || {
            let r = thread.map(|t| t.join().unwrap_or_else(|_| Err("ffmpeg's writer stopped".into())));
            say(match r {
                Some(Ok(())) | None => done,
                Some(Err(e)) => e,
            });
        });
    }
}

/// A frame to read back, handed to the GPU with the Viewer's frame.
pub struct Capture {
    /// Its place in the movie: read only when it is the next one wanted.
    pub index: usize,
    pub size: [u32; 2],
    pub view_proj: glam::Mat4,
    pub writer: Arc<Writer>,
    /// Frames read back so far.
    pub read: Arc<AtomicUsize>,
}

impl Capture {
    pub fn wanted(&self) -> bool {
        self.read.load(Ordering::Acquire) == self.index
    }
    pub fn done(&self, pixels: Vec<u8>) {
        self.writer.send(pixels);
        self.read.store(self.index + 1, Ordering::Release);
    }
}

/// A movie being made: the playhead times of its frames.
pub struct Job {
    times: Vec<f64>,
    read: Arc<AtomicUsize>,
    writer: Arc<Writer>,
    size: [u32; 2],
    settings: Settings,
    path: PathBuf,
    /// The playhead before, given back after.
    time: f64,
    playing: bool,
}

/// What the Viewer does this frame for a movie.
pub enum Step {
    /// The playhead is on the frame: read it back.
    Frame(Capture),
    /// Waiting for the GPU (or for textures).
    Wait,
}

impl Job {
    /// A movie asked for, if any: its frames from the timeline.
    pub fn start(
        graph: &crate::node_graph::NodeGraphState,
        playback: &mut crate::timeline::Playback,
        viewer: [u32; 2],
        aspect: Option<f32>,
    ) -> Option<Job> {
        let (path, settings) = with(|s| {
            s.stop = false;
            s.request.take()
        })?;
        let (times, rate) = crate::timeline::frames(graph, playback.view);
        let size = settings.size.pixels(viewer, aspect);
        match Writer::start(size, rate, settings, &path) {
            Err(e) => {
                say(e);
                None
            }
            Ok(w) => {
                let job = Job {
                    times,
                    read: Arc::new(AtomicUsize::new(0)),
                    writer: Arc::new(w),
                    size,
                    settings,
                    path,
                    time: playback.time,
                    playing: playback.playing,
                };
                playback.playing = false;
                with(|s| s.progress = Some((0, job.times.len())));
                Some(job)
            }
        }
    }

    /// The next step, or None when the movie is done (or stopped): the
    /// playhead goes back and ffmpeg finishes the file.
    pub fn step(&mut self, playback: &mut crate::timeline::Playback, ready: bool) -> Option<Step> {
        let read = self.read.load(Ordering::Acquire);
        let stopped = with(|s| {
            s.progress = Some((read, self.times.len()));
            std::mem::take(&mut s.stop)
        });
        if stopped || read >= self.times.len() {
            playback.time = self.time;
            playback.playing = self.playing;
            with(|s| s.progress = None);
            let what = format!(
                "{} {}x{} {} frames to {}",
                self.settings.codec.label(),
                self.size[0],
                self.size[1],
                read,
                self.path.display()
            );
            self.writer.finish(if stopped {
                format!("Movie stopped: {what}")
            } else {
                format!("Movie written: {what}")
            });
            return None;
        }
        // The next frame not read back: again when the pane was not drawn.
        let index = read;
        playback.time = self.times[index];
        if !ready {
            return Some(Step::Wait);
        }
        Some(Step::Frame(Capture {
            index,
            size: self.size,
            view_proj: glam::Mat4::IDENTITY,
            writer: self.writer.clone(),
            read: self.read.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sizes are even, from the camera's aspect when there is one; settings
    /// read back as written; ffmpeg is asked for the codec, the rate and
    /// Rec. 709.
    #[test]
    fn sizes_settings_and_arguments() {
        assert_eq!(Size::Viewer.pixels([1001, 577], None), [1000, 576]);
        assert_eq!(Size::W1920.pixels([10, 10], None), [1920, 1080]);
        assert_eq!(Size::W1920.pixels([10, 10], Some(2.39)), [1920, 802]);
        let s = Settings {
            codec: Codec::H265,
            size: Size::W3840,
            quality: Quality::Small,
        };
        assert_eq!(Settings::from_text(&s.to_text()), s);
        assert_eq!(Settings::from_text("nonsense"), Settings::default());
        let a = ffmpeg_args([1920, 1080], (24000, 1001), s, Path::new("/tmp/a.mp4")).join(" ");
        assert!(a.contains("-s 1920x1080 -framerate 24000/1001 -i -"));
        assert!(a.contains("-c:v libx265 -crf 30"));
        assert!(a.contains("-tag:v hvc1"));
        assert!(a.contains("-colorspace bt709"));
        assert!(a.ends_with("/tmp/a.mp4"));
        assert_eq!(with_extension(Path::new("a/b")), PathBuf::from("a/b.mp4"));
        assert_eq!(with_extension(Path::new("a/b.MP4")), PathBuf::from("a/b.MP4"));
    }
}
