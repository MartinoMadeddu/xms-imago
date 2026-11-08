# Native Arnold rendering (plan)

*Agreed 7 November 2026. For whoever builds it: read this whole page, then
`LLM_instructions.md`, `docs/render.md` and `ARCHITECTURE.md`.*

## The aim

Interactive rendering in Imago with Arnold, **free to run** (no Houdini, no
Maya) and **without a C++ build in Imago**:

- **Arnold reads the scene itself.** Imago writes USD as it already does;
  Arnold loads it with its own USD support. No translation of geometry or
  materials.
- **In Imago's own process.** Imago loads Arnold's library (`libai`) from the
  user's SDK install at run time, and drives an interactive render through
  Arnold's API.
- **Edits without reloading.** Arnold names the nodes it makes from USD after
  their prim paths, so an edit to a prim becomes a few API calls on that node
  while the render keeps going.

This is the first backend of a **renderer interface** that rray will fit next.

## Decisions

- **No Hydra server, no Houdini or Maya:** hosting Hydra in `hython` or
  `mayapy` needs a licence, which is a no-go.
- **No OpenUSD or C++ build:** Arnold brings its own USD reader in its SDK.
- **Arnold first:** it reads USD natively in its own process. Without a
  licence, renders carry Arnold's watermark.
- **Imago stays pure Rust:** a small binding to the functions used, `libai`
  opened by the system's loader at run time (`dlopen`, `LoadLibraryW`), never
  linked at build time. The binding is our own declarations of those
  functions; no Autodesk header or binary is shipped.
- **Karma is out of scope:** it only exists inside Houdini.

## How it works

### 1. Load: through USD

1. Imago writes the layer as it does for renders today: the scene, Render
   Settings, products and MaterialX materials. The products' frame tokens
   (`$F4`), left for husk to fill in, are put in by Imago: Arnold does not.
2. Imago starts Arnold in process (`libai` from the SDK at `ARNOLD_PATH`),
   checking its version: Arnold 7 or later (render sessions).
3. Arnold loads the layer with `AiSceneLoad` (its USD scene format), which
   makes its nodes straight in the universe, not inside a procedural: a USD
   procedural's nodes belong to it and are made again when it expands, so
   they are not the ones to edit. Composition, instancing, MaterialX
   `standard_surface`, lights, cameras and Render Settings are Arnold's to
   read, as kick does today.
4. Without a licence, `skip_license_check` is set: the render starts at once,
   watermarked, rather than waiting for a licence server.

### 2. Render progressively

- **Arnold's interactive render session** renders in progressive passes.
- **One thread owns Arnold** (it is started once per process): the session
  lives there, and Imago sends it edits.
- **Pixels, first:** the EXR the layer's Render Product names, which the Image
  Viewer already watches, so it needs no change. How often Arnold writes it
  during a progressive render is a question for Step 1.
- **Pixels, later:** an in-memory display driver installed from Imago, for
  pixels straight into the Image Viewer.

### 3. Edits by prim path

Imago knows what changed since the last render (the edit tracking Write USD
already uses). Nodes are changed only while the render is paused (after
`AiRenderInterrupt`, or between passes), then it restarts. For each change:

| Change | Applied live as |
|---|---|
| A transform | the world matrix of every shape below the prim (Arnold keeps world matrices) |
| A light's parameters | the light node's intensity, colour, exposure… |
| A material's or shader's parameters | the shader node, by the shader prim's path |
| Visibility | the node's visibility |
| The camera | the camera node's matrix and lens |
| Render Settings (samples, resolution) | the options node |
| Moved points (animation, ICE deformers) | the mesh's point list |

Each change says what kind it is, and a renderer answers whether it can apply
that kind live. Anything it can't apply falls back to a reload, including
instanced prims, point instancers, and transforms with motion blur keys.

Live edits repeat a small part of Arnold's USD translation (a light's units
and normalising, a camera's field of view from its focal length and
aperture). Each kind is checked once against a reload, so that a live edit
and a reload give the same image. The list stays short.

### 4. Structural changes: reload

Added or removed prims, changed topology, a new material network, another
frame: Imago writes the layer again and Arnold loads it again. Still in
process and in the same session, so it is quicker than starting kick; whether
textures stay loaded across loads is a question for Step 1. On a heavy scene,
changing the frame is the slow part.

## The renderer interface

The same for every renderer, designed with Arnold and checked against rray:

1. **Load a layer.**
2. **Render progressively,** with Render Settings, a camera and outputs.
3. **Apply an edit by prim path,** if this renderer can apply that kind live.
4. **Reload** after a structural change.
5. **Stop,** and hand over pixels (a file at first, memory later).

A new renderer that reads USD natively implements these five. It can start
with reloads only and gain live edits one kind at a time.

The interface does not assume where the renderer runs. In Imago's process, a
crash in Arnold takes Imago and the unsaved graph with it; if that happens in
practice, the Arnold backend moves into a small helper process behind the same
interface, with nothing changing for the user.

## Steps

### Step 1: a prototype outside Imago

`prototypes/arnold`: a small Rust program with no dependencies, its own
workspace, not part of Imago's build:

1. Load `libai` from the SDK at run time, and check its version.
2. Load a `scene.usda` Imago wrote with `AiSceneLoad`.
3. Render progressively into the files its Render Products name.
4. Change one light's intensity, found by its prim path, while it renders, and
   check that the render updates without reloading.

**It answers:**
- Does the SDK's USD support load the layers Imago writes, as kick does?
- Are the nodes it makes named by prim path, shaders inside MaterialX
  networks too, and can they be found and changed during an interactive
  render with this SDK version?
- How often is the EXR written during a progressive render?
- Which edits does Arnold take live, and which need a reload?

**Running it** (CI has no Arnold: it runs on a machine with the SDK):

```
cd prototypes/arnold
cargo build --release
export ARNOLD_PATH=/path/to/Arnold-7.x-sdk
./target/release/arnold-prototype /path/to/scene.usda --light /lights/key
```

The layer is one Write USD wrote (the DeLorean render template's, for one).
Its images are written where its Render Products say, relative paths from
the folder it is run in, the frame's tokens (`$F4`) put in and the folder
made. It prints the nodes Arnold made and their names, the
images and when each was written, and the edit and how long it took to show.
`--frame`, `--edit-after`, `--seconds` and `--licence` are in `--help`.

**Found so far** (Arnold 7.5.0.0, macOS arm64):
- The binding works: `libai` opened at run time, every function found.
- Arnold loads Imago's layers with `AiSceneLoad` in a fraction of a second:
  camera, meshes, Render Products, AOVs and their filters, resolution.
- Its nodes are named by prim path (`/root/statue/statue`,
  `/root/shotCamera`, `/Render/Products/beauty`); the others are Arnold's
  own (`options`, `root`, `_fallbackShader`).
- Progressive passes run from -4 AA samples up, each after a "fast scene
  update" of the few nodes that changed, not a reload.
- The EXR is written through each pass, not only at its end: the Image Viewer
  is to read it once a pass is done, not when the file changes.
- The products' frame tokens (`$F4`) are not Arnold's to put in.
- MaterialX materials need MaterialX's libraries on
  `PXR_MTLX_STDLIB_SEARCH_PATHS` (Imago has them, `crates/xms-matx/libraries`):
  without them every material is Arnold's fallback. With them, a material's
  shaders are nodes named by their prim paths (`standard_surface`, and `osl`
  nodes for the rest of its network).
- A light's intensity changed while it renders shows in the next pass, about
  0.03 s after: Arnold updates only the nodes changed, no reload.
- Pausing (`AiRenderInterrupt`, blocking) waits for the pass running to end.
- Arnold makes `.tx` files of the textures as it first meets them, which can
  hold the first passes for many seconds; once made, they stay beside the
  textures. Whether Live makes them or uses only those made is to decide.
- A point instancer was read as its prototype only: whether its copies render
  is still to see.

### Step 2: the renderer interface and the Arnold backend in Imago

`crates/xms-render`, the standard library only:

- **The interface:** `Renderer` (load, render, live, edit, poll, stop), and
  `Change`, each change by kind: a transform, a light's or shader's
  attributes, the Render Settings', visibility, points, or structure (always a
  reload).
- **`Live`:** a renderer on a thread of its own. It takes edits with the
  layer as it is with them, applies them live when the renderer can, else
  loads that layer; edits that come in together are one (a slider dragged).
  Its `Event`s: loaded, each pass done (its images whole, to read), finished,
  edits applied, failures.
- **The Arnold backend** (`arnold`): the prototype's binding, grown. Lights'
  attributes go to the parameters Arnold's reader sets from them (intensity,
  exposure, colour, diffuse, specular, normalize), shaders' to the
  parameters of the same name, Render Settings' `arnold:global:` attributes
  and resolution to the options; each only where the node has that
  parameter, of that type. A shape with motion blur keys, points of another
  count, an attribute not mapped, a prim with no node: a reload. Relative
  image paths are from the layer's folder.
- **Tests:** CI has no Arnold: `Live` is tested against a stand-in renderer,
  and the mapping on its own. Arnold by hand:

```
ARNOLD_PATH=/path/to/Arnold-7.x-SDK cargo run --release -p xms-render \
    --example arnold_live -- scene.usda --light /lights/key --move /a/shape
```

  It edits the light while the render runs and again once it has finished,
  moves the shape, and asks for a reload, printing each event.
- Still to do in this step: `ARNOLD_PATH` from the renderer definitions
  (with Step 3, where Imago starts it).

### Step 3: Live in the Image Viewer

- **A Live mode:** the first render loads and starts the session; after that,
  each change to the graph sends edits by prim path, or reloads when it must.
- **The current renders keep working:** the kick and husk command lines stay
  as they are, side by side with Live.

Built: **● Live** in the Image Viewer's bar, on by default, Render and Stop
starting and stopping it (`docs/render.md`).

- `crate::live::Session` is a live render that belongs to no pane: the scene
  sent, edits as it changes, its status, each pass's image. The Image Viewer
  drives one; the Viewer can drive one the same way (rendering in the 3D
  viewport), with pixels in memory (Step 4) drawn as its texture.
- Before Step 4, each pass's EXR was copied whole at the end of the pass
  (Arnold writes the file all through a pass): replaced by pixels in memory.
- `src/live.rs` works out the edits between two cooks of the scene, prim by
  prim, by pointer first, then by value (an operator cooked again makes its
  prims anew): a placement becomes its world matrix in the layer's axes
  (`usd_write::layer_axes`), a light's values its attributes, a MaterialX
  graph's input values its shaders' (`<material>/<node>`), `visibility` alone
  a visibility, points alone (the same topology and other attributes) the
  points, a RenderSettings prim's attributes the settings. Anything else is a
  change of structure.
- The layer is written only when a reload needs it: `Live::edit` takes the
  writing as a closure, called on the render thread if the edits cannot be
  applied live.
- The renderer definition says how a renderer renders live
  (`"live": { "backend": "arnold", "sdk": "$ARNOLD_PATH" }`, and `licence`).
- MaterialX's libraries, which Imago carries inside it, are written once into
  the render folder for Arnold's USD reader.
- Not yet: the Image Viewer's choice of RenderSettings and camera (Live
  renders the layer's own), and a "making textures" status from Arnold itself
  (the bar says so when the first pass is long in coming).

### Step 4: pixels in memory

- An in-memory display driver, replacing the EXR file, so passes reach the
  Image Viewer without disk writes. The hardest step: a driver node is
  registered from Rust as a table of functions, fiddly and dependent on
  Arnold's version.

Built: `xms_render::Pixels`, the image a renderer fills bucket by bucket, in
memory, read from any thread when its generation changes; `Live::pixels`
gives it, and `crate::live::Session::pixels` the frame when buckets came in.

- **Imago's driver** (`arnold/driver.rs`): a node type of its own,
  `xms_imago_pixels`, installed with `AiNodeEntryInstall` once Arnold starts:
  its common and driver methods as `repr(C)` tables of `extern "C"`
  functions, in the order `ai_node_entry.h` and `ai_drivers.h` declare them.
  It takes RGB and RGBA; each bucket, on the render thread that did it
  (`DriverProcessBucket`), goes into the image where it is in the frame.
- **Attached at each load:** a node of it, `/__xms_imago_pixels`, and every
  output line again (each AOV once, of its own type), to it, added to the
  options' `outputs`; it takes several outputs (`AiDriverInitialize`), RGBA,
  RGB, vectors, floats and integers, each a layer of `Pixels` (one channel
  held grey). The products' drivers still write their files.
- **On screen, only what changed:** `Pixels` keeps the bounds of what came
  in since it was last taken (`take`); the Image Viewer keeps its live
  texture and writes only that region into it each frame
  (`image_gpu::ensure_live`), its own copy of the image (values under the
  cursor) brought up to date a few times a second. The Viewer can keep a
  texture the same way.
- **IPR off:** the same session renders once, without passes (the
  `progressive` render hint off: each bucket at the Render Settings'
  samples), the graph's changes not sent; said if Arnold does not take the
  hint.
- **Half or float** is a file's, each AOV's (`driver:parameters:aov:format`,
  as Karma's): Arnold gives a driver floats whatever the file holds, and the
  image in memory stays float. Whether Arnold's USD reader makes an output
  half from the format is checked with the prototype, which says each
  output's and EXR driver's `half_precision`.
- Tested against a stand-in `libai` calling the tables from C as Arnold's
  headers lay them out; by hand with Arnold (`examples/arnold_live` prints the
  image in memory after each render).

### Step 5: rendering in the Viewer

The menu at the Viewer's top right, Viewport or Arnold: Arnold
renders the scene live (IPR) through the Viewer's camera, in place of drawing
it, the image refining as buckets come in.

- **One session for the program** (`crate::live::with`): Arnold runs once per
  process, so the Image Viewer and the Viewer share it, the last to start it
  having it. The Image Viewer's Render takes it back to the layer's camera
  (and the Viewer back to Imago); its Stop stops the Viewer's render too.
- **The view** (`xms_render::Change::View`): a camera of Imago's own in
  Arnold, `/__xms_imago_view`, placed where the Viewer looks from (the
  Viewer's own camera, or the scene's camera looked through, without its
  scale), its horizontal field of view, and the resolution the pane's size in
  pixels; `options.camera` set to it. Sent with the load, kept through
  reloads, sent again as an edit each time the view moves or the pane changes
  size.
- **Two readers of the pixels** (`Pixels::take(layer, reader, all)`): each
  keeps what changed since it last took it, the Image Viewer reader 0, the
  Viewer reader 1. The Viewer writes only that region into its texture of the
  beauty, laid over the whole picture (`rendered.wgsl`) and shown through the
  Viewer's colour view; Edit Poly's wireframe and handles, and the
  manipulator, are drawn over it. The scene drawn is not.
- **Failing:** why, at the bottom left, and the Viewer draws again (no
  retrying each frame).
- Tested in the crate (the view kept through reloads, the second reader)
  and with a stand-in `libai`; the app part by hand with Arnold.

### Then: Imago on the farm

Imago headless (`xms --headless scene.xms --render 1-100`): the graph opened,
the scene loaded in Arnold once, then frame by frame what changed sent as live
edits (ICE deformers' and animation's points, transforms), the layer written
again only for a change of structure, each frame rendered at the Render
Settings' samples (IPR off, the licence looked for), its products written. ICE
and Imago's other tools on the farm without baking to disk first.

### Then: rray

- rray as the second backend, through the same interface, natively in Rust.

## Settings the user needs

- **The Arnold SDK installed,** from their Autodesk Account, and where it is:
  `ARNOLD_PATH` in the renderer definitions, later in File → Variables with
  "set in the environment of commands".
- **An Arnold licence** only for frames without the watermark.

## Later renderers

- **One that reads USD natively** (rray, or another): implement the five
  operations; days to a week or two.
- **One that only reaches USD through Hydra** (RenderMan, Cycles, 3Delight,
  Redshift…): either a self-built, free Hydra bundle (OpenUSD and open
  delegates such as Cycles'), or translating into its own API. To be decided
  when needed.

## Rules to follow

- **The repository's `LLM_instructions.md`.** In particular: Rust clean and
  idiomatic, less is better, the standard library first, **no thin delegating
  wrappers**.
- **Every change comes with** tests (a stand-in renderer where Arnold is
  needed), docs (`docs/render.md`, this page) and a line in `LOGS.md`.
- **No change to Imago's build:** no C or C++ compiled in for this, nothing
  linked at build time.
