# About XMS | Imago

![The XMS | Imago icon](assets/icon.png)

XMS | Imago is a node-based 3D application written in Rust. A scene is one
graph of operators on USD, and nothing is baked: change any node and
everything after it follows.

Developed to have a fast, easy to build application that can run on all main
OSs, non destructive procedural workflow with USD scene at its core with
advanced crossdata manipulation access via the Integrated Creation Engine (ICE)
and fast mocap processing tools.

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

Its crates:

- `xms-scene`: the scene, USD-shaped, path-keyed and time-sampled, with change
  tracking.
- `xms-usd`: USD stages into that scene.
- `xms-geo`: the geometry, a topology and typed attribute columns, shared
  copy-on-write.
- `xms-ice`: the ICE engine.
- `xms-matx`: the MaterialX node libraries.
- `xms-image`: renders and images as layers of float channels.
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
- Houdini's husk, to render with Karma.

## Credits

XMS | Imago was created by [Martino Madeddu](https://github.com/MartinoMadeddu):
the node graph, the Viewer, the ICE graphs, the USD loader and composed stages,
GPU instancing and the look of the interface.

Additional development by [Simon Legrand](https://github.com/srlegrand):
polygon modelling, UV, USD at production size, motion capture, Body Collide,
the dockable interface and the website.

The example files' authors and licences are in
[examples/CREDITS.md](examples/CREDITS.md).
