# Changelog

What each release of XMS | Imago brought, newest first. A release is a tag
on `main` (`v0.2.0`); see [docs/releases.md](docs/releases.md). Day-to-day
changes, one line each, are in [LOGS.md](LOGS.md); between releases, note the
ones worth announcing under **Unreleased**, and move them under the new
version's heading when you tag it.

## [Unreleased]

- **ICE snippets:** a **Snippet** node in ICE graphs: code in OSL's syntax and types with ICE's attributes (`@P += normalize(@N) * amount;`), `if`, loops of a known count, functions, and the ICE nodes as functions (`get_data`, `get_array_data`, `near_points`, `closest_point_on_surface`, `random`), lowered into ICE's own nodes, so it runs as fast as they do. Its parameters are its ports; errors show with their line. ([design and status](docs/ice-snippets.md))
- **The Snippet operator:** code as a scene-graph operator (Houdini's Attribute Wrangle, Imago's way): an ICE operator holding one Snippet node, its code, Run Over, References and parameters in its own pane. Two templates: Snippets: roughen and relax, Snippets: stick to a surface.
- **Snippet node:** a `geometry` parameter left unwired reads its **References** row (a Get Reference's: Reference 1 unless set); **Make Compound** makes a snippet a compound with its parameters as ports; Ctrl+Enter applies the code on macOS too; three examples in the add menu (roughen, relax, stick).
- **Noise matched to OSL's:** Perlin Noise, Simplex Noise and Cell Noise nodes, and `noise("perlin" | "uperlin" | "simplex" | "usimplex" | "cell", p)`, `snoise(p)`, `cellnoise(p)` in snippets: ported from Open Shading Language (BSD-3-Clause), checked against its own expected values.
- **ICE logic:** Less Than, Less Than or Equal, Greater Than, Greater Than or Equal, Equal, Not Equal, And, Or, Not, and **If** (one value or another per element, by a condition), under Logic in the add menu.
- **OSL's maths as ICE nodes:** Square Root, Power, Exponential, Logarithm, Floor, Ceiling, Round, Sign, Modulo, Tangent, Arcsine, Arccosine, Arctangent of Y over X.

## [0.3.0] - 2026-11-10

A milestone: the Viewer becomes a place to look at data, not only shapes, and
the 0.2 series (rendering with Arnold in Imago, volumes, renderers' shaders,
the scene's units and rate) is gathered into one release.

### New in 0.3.0

- **Display Attribute** (the Viewer's **A** button, top left): attributes drawn
  over the prims, each as its type asks. Floats as colours through a ramp
  (automatic range, or Min and Max typed), colours and UVs as themselves,
  vectors and normals as lines, integers, text and lists as labels hidden
  behind what is in front; intrinsics (point, face and corner numbers,
  normals, areas) from a list. Several at once, on the Selected prims or All,
  within limits the Viewer says it reached. The list is saved with the graph,
  Show off when it opens.
- **Box selection in the Viewer:** drag a box to select every prim it touches
  (a mesh's triangles, a light's or camera's lines), behind others too; Ctrl
  adds, Shift takes away, as in Edit Poly.
- **Selection you can see:** a selected prim's wireframe drawn over it in a
  bright blue of the Viewer's own, whatever the shading, its box hidden behind
  what is in front; **F** frames the prims selected.
- **The Viewer starts Shaded**; its shading is no longer kept between sessions.
- **Graphs (scene graph, ICE, MaterialX):** one selection blue for selected
  nodes, wires and picked ports; the box dragged to select nodes is the same in
  all three, and see-through in the dark themes (it was a bright wash).
- **Dark themes:** Hidden Line Removal and Wireframe's wires a mid grey, not
  white; the Primitive Inspector's row numbers and intrinsics' headers a
  brighter blue.

### The 0.2 series in short

- **Rendering with Arnold in Imago** (0.2.5 to 0.2.9): loaded from your SDK at
  run time, nothing to build; IPR into the Image Viewer, every AOV bucket by
  bucket, or live in the Viewer; changes sent to Arnold as you work; the
  `xms` command line headless for farms (`--render`, `--write`, `--run`,
  `--tx`); Arnold's log in Imago; it ends cleanly when Imago quits (0.2.12).
- **Shading** (0.2.12): MaterialX graphs put out volumes as well as surfaces;
  a renderer's own MaterialX definitions (Arnold's 140 shaders, from its SDK)
  offered as nodes, turned on in File > Settings > Renderers.
- **Volumes** (0.2.12): Load VDB brings OpenVDB files and sequences in as USD
  Volumes, a field per grid, rendered by Arnold with `ai_standard_volume`.
- **The scene's up axis, unit and rate** (0.2.9), saved with the graph, and
  composition as USD has it: a Merge's later input the stronger.
- **ICE** (0.2.11): Near Points (a k-d tree over the points), Get Data at an
  index, lists stored per element and read back: capture once, deform every
  frame.
- **Images** (0.2.2, 0.2.3): Load Image and image operators (Merge, Colour
  Correct, Shuffle, Crop, Reformat, Blur, Write Image…) in ACEScg; the Image
  Viewer on the GPU.
- **Releases** (0.2.1): versioned downloads side by side, the version and
  build shown in Imago and written into saved graphs.

## [0.2.12] - 2026-11-10

- **Renderers' shaders from their SDKs:** File > Settings > Renderers > **Load Shader Definitions from SDK** (saved with the graph; `XMS_RENDERER_SHADERS=1` turns it on for every graph) offers every shader of a renderer's own MaterialX definitions in MaterialX graphs: Arnold's 140, as `ai_standard_surface`, `ai_image`…, under Arnold in the add menu. Generalised through the renderer definitions' `shaders` block, so other renderers shipping MaterialX definitions work the same way. A node whose definition is not loaded is kept, values and wires, and written out as it was.
- **Volume shading:** MaterialX graphs can put out a volume (`volumematerial`, beside a surface), and have Arnold's `standard_volume` as a node, `ai_standard_volume`, under Arnold in the add menu: written to USD as Arnold reads it (`arnold:standard_volume`, `outputs:arnold:volume`), so a Load VDB volume renders with its density, scatter and emission; read back from a stage too.
- **Load VDB:** an OpenVDB file or sequence in the scene as a USD Volume, a field per grid; its box in the Viewer, its grids in the Primitive Inspector; Write USD writes it for Arnold and any renderer to read the file (read with OpenVDB-rs, a Rust port of OpenVDB).
- **Primitive Inspector:** list attributes (ICE's lists stored with Set Data) shown, one `[a, b, c]` cell per row; the Point, Face, Corner and Object tabs count the attributes they hold, not the elements (those are intrinsics, in the Object tab).
- **Arnold ends cleanly** when Imago closes or quits (and after `--headless`): the live render stopped and let go before Arnold releases its resources, no more heap corruption ("Incorrect checksum for freed object") at exit.

## [0.2.11] - 2026-11-10

- **Near Points** (ICE): the points nearest each origin, of reference prims or the graph's own, up to a count within a radius (0: anywhere), as a list per point of their numbers and distances: neighbours for smoothing, relaxing, clumping. A k-d tree over the points (kiddo).
- **Get Data at an Index** (ICE): Get Data and Get Array Data's Polygon input is now their **Index**; wired alone, a Get Data reads at the elements it numbers (Houdini's `point()`, `prim()`: Near Points' Points), with a Param UV at a place as before. Graphs saved before keep reading at the place (0, 0). The **Cage deform by near points** template carries a sphere by a cube's points with them.
- ICE: Closest Point on Surface's and Scatter on Surface's **Polygon** output is now **Face Index** (wires and saved graphs unchanged).
- **Lists stored with Set Data** (ICE): a list per element (Near Points' lists, a Get Array Data) is kept as a list attribute, every element as many items as the longest (the shorter padded: -1 for an int, else zero), written to USD with its `elementSize`, read back with Get Array Data: a capture once, a deform every frame with no search. The Cage deform template is now two operators, CageCapture and CageDeform.
- **Get Array Data at an Index** (ICE): its Index wired alone, the list the numbered element holds.
- **Lighter render layers:** a deformed prim's points stay in memory even when the deformer also set an attribute (only that attribute is written, and the Log names it); attributes a deformer copied unchanged are no longer written.

## [0.2.10] - 2026-11-09

- 0.2.9 again: its release failed on GitHub's side.

## [0.2.9] - 2026-11-09

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
