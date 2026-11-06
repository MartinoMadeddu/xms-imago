# Changelog

What each release of XMS | Imago brought, newest first. A release is a tag
on `main` (`v0.2.0`); see [docs/releases.md](docs/releases.md). Day-to-day
changes, one line each, are in [LOGS.md](LOGS.md); between releases, note the
ones worth announcing under **Unreleased**, and move them under the new
version's heading when you tag it.

## [Unreleased]

## [0.2.2] - 2026-11-06

- **Load Image:** an image or a sequence (`plate.$F4.exr`) into the graph, brought from its colour space into ACEScg as Nuke's Read does; viewed, the Image Viewer shows it through its view, every layer of an EXR. Picking one frame of a sequence takes the sequence; Use as Scene Range sets the timeline to its frames.
- **Image colour as in Nuke:** image files are read as stored, so 8-bit images are no longer decoded twice.

## [0.2.1] - 2026-11-06

- Downloads, their folders and the program are named with the version
  (`xms-0.2.1-linux-x86_64.tar.gz`, `xms-0.2.1`), and builds of `main` with
  their number too (`xms-0.2.1-build160-…`), so versions sit side by side.
- A release is published only when every platform built.
- Imago shows its version and build beside its name (click to copy it, for a
  bug report), and writes it into saved graphs.
- The website's download buttons fetch the latest release's files.

## [0.2.0] - 2026-11-06

The first versioned release: everything up to now, in short.

- **One program, one window:** the eframe and wgpu shell, with a Viewer of its
  own (shaded, textured, wireframe, instancing, Edit Poly's tools) and an
  Image Viewer that runs renders and shows them as they arrive.
- **USD:** composed stages read in the background (boxes first, then the
  whole stage), as authored, with an option to turn them to Y-up metres; Load
  USD as sublayer, reference or payload; Set Attribute, Prune, variants
  (select and create), material binding, lights, cameras, render settings;
  Write USD writes what changed as an override layer.
- **ICE:** a compiled, batched engine (`xms-ice`) running on every core:
  Get and Set Data across contexts, maths, rotations, arrays, Random, Scatter
  on Surface, reading at locations, reference inputs, and the Instancer.
- **Compounds:** scene-graph and ICE compounds, promoted parameters, a
  shipped library.
- **Rendering:** Render Command (husk, kick), Render Settings, products and
  variables; snapshots in the Image Viewer.
- **Colour:** views and displays, as Houdini, Gaffer and Nuke show them.
- **Files:** graphs saved as `.xms` (`.json` still opens), Alembic opened as
  USD, FBX in and out, MaterialX shading graphs.
- **Animation and mocap:** clips, retargeting, capture and skin, Body
  Collide, rigging's first step.
- **Variables:** `$GNAME`, `$GDIR`, `$JOB`, the frames, `$OPNAME`, and each
  operator's own `{name}`.
- **Builds:** Linux (glibc 2.28), macOS (Apple silicon and Intel), Windows;
  the documentation as HTML in every package.
