# Changelog

What each release of XMS | Imago brought, newest first. A release is a tag
on `main` (`v0.2.0`); see [docs/releases.md](docs/releases.md). Day-to-day
changes, one line each, are in [LOGS.md](LOGS.md); between releases, note the
ones worth announcing under **Unreleased**, and move them under the new
version's heading when you tag it.

## [Unreleased]

- **Composition as USD has it:** a later input of a Merge is the stronger in the layers Imago writes too (its stage sublayered first), whichever order the inputs come in, and a prim another stage's prim took the place of is no longer deactivated (which took both away).
- **The scene's up axis, unit and rate** (File > Settings > Scene, saved with the graph): every layer says them, each stage put where Imago shows it by a transform before its root prims' own (`xformOp:transform:toScene`); a stage of another rate is read at the same second, as USD retimes it, with a warning in Load USD, Write USD and the render's Log.
- **Arnold on Linux:** Imago starts with Arnold's `bin` on the library path, so Arnold's USD reader loads (it was "unknown format"); Arnold's plugin loading is in the Log.

## [0.2.8] - 2026-11-09

- **Frame rates:** Imago's USD layers repeat their files' `timeCodesPerSecond`, so a 25 fps file renders (and is written) at the frames Imago shows.
- **Arnold's log in Imago:** its warnings and errors in the Image Viewer's Log (printed headless), and the reason when a scene does not load; the Log also says which changes went to Arnold live and which made it load the scene again.
- **Arnold from the Arnold Folder in Settings** (nothing in the environment, as on Linux): `ARNOLD_PATH` and its plugins (the USD reader) set for Arnold before it starts.
- **Hair in Arnold:** curves deformed in ICE render as Imago shows them: their points sent live, written with their counts, and widths set per curve written per point and no longer blocked by the file's. Curves' widths are listed in the Primitive Inspector.

## [0.2.7] - 2026-11-09

- **The xms command line, headless:** `--render` (Arnold in Imago: the graph cooked each frame, ICE deformers too, Arnold loaded once, deformed points sent in memory, no scene description kept, the products written), `--write` (the Write USD nodes), `--run` (the Render Command nodes), `--tx` (`.tx` files of the scene's textures), `--node`; progress lines for farm managers and an exit code.
- **Rendering in Imago writes a light layer:** points that only moved (deformers, animation) go to Arnold in memory, not into the layer.
- **Auto-generate TX** is off by default in Arnold's settings.
- **Project renderer definitions:** `$JOB/renderers`, between the built-in ones and yours, so a show pins its own.

## [0.2.6] - 2026-11-09

- **Arnold in the Viewer:** the menu at the Viewer's top right, Viewport or Arnold: Arnold renders live (IPR) through the Viewer's camera at the pane's size, each change and each move of the view sent as you work, the wireframe and handles drawn over it.
- **Snapshots in both viewers:** one set for the program. The Viewer's bar (📷, bottom left, hidden to start with) snaps its render and shows any snapshot; compare them in the Image Viewer.
- **Renderers found on this machine:** each, each way it runs (Arnold in Imago with its version; husk, kick as command lines), shown in the renderer menu, the Render Settings panel and File > Settings > Renderers, never hidden. **Arnold Folder** and **Houdini Folder** there set `$ARNOLD_PATH` and `$HFS` inside Imago, for programs opened from the Finder or the Dock.
- **Your renderer definitions:** Make editable copy, Revert to built-in, Reload; what a later built-in definition has that yours lacks; settings kept by name. The definition is checked against Arnold's own options, and a new Arnold's changes are listed.
- **Arnold settings as Houdini's ROP:** indirect specular blur, low light threshold, nested dielectrics, indirect clamp, max subdivisions, textures (auto TX and more), bucket scanning, more Ignore toggles.
- **Viewer:** its colour as the Image Viewer's by default (File > Settings > Viewer), where its navigation style now is too.
- **Render Settings:** a Log tab for the renderer as found and its definition; under the renderer only its version, and a red line when the Log has something to say.
- **Image Viewer:** with IPR off, the buckets being rendered are outlined in orange.
- **Stop** ends everything a command line started, even with Arnold loaded.
- **Arnold's Max Value** (sample clamp) defaults to Arnold's own: no clamping.

## [0.2.5] - 2026-11-08

- **Rendering in Imago with Arnold:** Render renders in Imago itself, Arnold loaded from your SDK (`ARNOLD_PATH`) at run time, nothing to build. The image comes into the Image Viewer bucket by bucket, every AOV of it, through Imago's own display driver in Arnold; the products' files are written as before.
- **IPR**, on by default: in passes, and each change to the graph (a light, a MaterialX shader's inputs, a move, hiding a prim, moved points, Render Settings) goes to Arnold while it renders, the scene written again only when it must be. Off: once, each bucket at the Render Settings' samples.
- **Bit depth:** Arnold's AOVs carry a format, half for colour, float for data, as Karma's.
- **Image Viewer:** RGB, R, G, B, A, L in one menu; exposure and gamma as E and G; snapshots of a render in Imago keep what they took.

## [0.2.3] - 2026-11-07

- **Image operators:** Image Merge (B with each A over it: Over, Plus, Multiply, Screen, In, Out and more), Colour Correct, Shuffle, Delete Channels, Constant, Crop, Reformat, Transform 2D, Blur, Premultiply and Unpremultiply, and Write Image (EXR with every channel, or PNG, JPEG, TIFF; a frame or a range). They work in ACEScg with premultiplied alpha.
- **Image Viewer on the GPU:** a change of colour space, view, exposure or channel shows at once, at any image size.
- **Colour:** changing an image's colour space reads nothing again; Raw goes through the view chosen, and the Display menu is greyed while the Raw view is on.

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
