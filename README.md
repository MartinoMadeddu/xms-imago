# About XMS | Imago

![The XMS | Imago icon](assets/icon.png)

XMS | Imago is a node-based 3D application written in Rust. A scene is one
graph of operators on USD, and nothing is baked: change any node and
everything after it follows.

Developed to have a fast, easy to build application that can run on all main
OSs, non destructive procedural workflow with USD scene at its core with
advanced crossdata manipulation access via the Integrated Creation Engine (ICE)
and fast mocap processing tools.

![XMS | Imago with shot 03_035_A of Spring: the forest seen through the shot's
camera in the Viewer, its stage in the Scene Explorer, and Load USD, Prune
Primitives, Reparent, Set Variable, Render Commons and Render Settings in the
Node Graph](docs/xms_front.png)

The scene is shot 03_035_A of [Spring](https://studio.blender.org/projects/spring/pages/about/),
© Blender Studio, under CC BY 4.0, as USD: from the [USD Working Group's
Sample Assets](https://lf-aswf.atlassian.net/wiki/spaces/WGUSD/pages/11274584/Sample+Assets),
downloaded from
[download.blender.org/institute/sybren/usd](https://download.blender.org/institute/sybren/usd/).

## Structure

One program, a few crates of its own, and open libraries under them.

- **The Scene Graph:** operators on a USD scene, each cooked when what it reads
  changes and kept between frames.
- **ICE:** graphs per point, face or object, compiled into plans and run over
  attribute columns in parallel batches.
- **Edit Poly:** a whole modelling session in one node.
- **Rigging-Mocap:** procedural rigging and dynamic/ragdoll mo-cap clean up and
  process.
- **The Viewer and the Image Viewer:** wgpu, colour managed with OpenColorIO.
- **Rendering:** UsdRender prims from the graph; Arnold renders in Imago itself
  (IPR, every AOV bucket by bucket into the Image Viewer, or live in the
  Viewer), Karma and others through their command lines.

Its crates:

- `xms-scene`: the scene, USD-shaped, path-keyed and time-sampled, with change
  tracking.
- `xms-usd`: USD stages into that scene.
- `xms-geo`: the geometry, a topology and typed attribute columns, shared
  copy-on-write.
- `xms-ice`: the ICE engine.
- `xms-matx`: the MaterialX node libraries.
- `xms-image`: renders and images as layers of float channels, and the
  snapshots both viewers share.
- `xms-render`: rendering in Imago: the renderer interface, live edits by prim
  path, the pixels as they come, and the Arnold backend.
- `xms-alembic`: Alembic caches.
- `xms-colour`: colour management.
- `ragdoll`: collision cleanup for motion capture.
- `xms-docs`: these pages.

Built on:

- [egui](https://github.com/emilk/egui) for the interface, and
  [wgpu](https://wgpu.rs) for the Viewer.
- [openusd](https://github.com/MartinoMadeddu/openusd), pure-Rust OpenUSD
  (Imago's fork).
- [ocio](https://crates.io/crates/ocio), pure-Rust OpenColorIO.
- [rayon](https://github.com/rayon-rs/rayon), to use every core.
- [ufbx](https://github.com/ufbx/ufbx) for FBX, and
  [image](https://github.com/image-rs/image) for PNG, JPEG, OpenEXR and more.
- [Arnold](https://www.autodesk.com/products/arnold), loaded from your SDK
  when Imago runs (nothing to build against it), to render in Imago.
- Houdini's husk, to render with Karma.

## Get it

- **Website:** [martinomadeddu.github.io/xms-imago](https://martinomadeddu.github.io/xms-imago/)
- **Download:** binaries for Linux, macOS and Windows on the [Releases
  page](https://github.com/MartinoMadeddu/xms-imago/releases): the latest
  release (a version, for example 0.2.0), or the newest build of `main` (a
  pre-release, "Build N"). See [Builds and releases](docs/releases.md) and the
  [changelog](CHANGELOG.md).
- **Documentation:** [docs/](docs/README.md), or **Help > Documentation** in
  Imago.


## The command line

```
xms                                        the window
xms scene.xms                              the window, the scene opened
xms --headless scene.xms --render 1-100    render with Arnold in Imago
xms --headless scene.xms --write 1-100     write the scene (Write USD nodes)
xms --headless scene.xms --run 1-100       run the Render Command nodes
xms --headless scene.xms --tx              make .tx files of the textures
```

`--render` cooks the graph frame by frame, ICE deformers included, and renders
each frame with Arnold in Imago's process, loaded once: no scene description
is kept, the deformed points go to Arnold in memory, and the images are the
Render Settings' products. `--write` and `--run` write the scene for husk,
kick or a farm through the graph's Write USD and Render Command nodes. Frames
are `12`, `1-100` or `1-100x2`; `--node NAME` picks one node. For a farm it
prints `Imago progress: 40%` lines and ends with code 1 when anything failed.
See [The xms command line](docs/render.md#the-xms-command-line).

## How to build

With [Rust](https://rustup.rs) 1.95 or newer (`rustup update` brings it):

```
git clone https://github.com/MartinoMadeddu/xms-imago
cd xms-imago
cargo run --release
```

On Ubuntu or Debian, first:

```
sudo apt install build-essential pkg-config libasound2-dev libudev-dev libx11-dev libxkbcommon-x11-0
```

Export Movie needs [ffmpeg](https://ffmpeg.org) on the PATH. `cargo test
--release` runs the tests, and `cargo run -p xms-docs` makes the HTML
documentation in `docs/html`.

## Credits

XMS | Imago was created by [Martino Madeddu](https://github.com/MartinoMadeddu)
and developed by [Simon Legrand](https://github.com/srlegrand) and [Martino
Madeddu](https://github.com/MartinoMadeddu).

The example files' authors and licences are in
[examples/CREDITS.md](examples/CREDITS.md).
