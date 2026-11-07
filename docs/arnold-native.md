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
  without them every material is Arnold's fallback.

### Step 2: the renderer interface and the Arnold backend in Imago

- The interface (load, render, edit by path, reload, stop, pixels), with the
  live-or-reload rule per kind of edit.
- The Arnold backend: the binding grown from the prototype's (only the
  functions used), `libai` found through `ARNOLD_PATH` in the renderer
  definitions.
- **Tests:** CI has no Arnold, so they run against a stand-in renderer
  implementing the interface. Arnold itself is tested by hand.

### Step 3: Live in the Image Viewer

- **A Live mode:** the first render loads and starts the session; after that,
  each change to the graph sends edits by prim path, or reloads when it must.
- **The current renders keep working:** the kick and husk command lines stay
  as they are, side by side with Live.

### Step 4: pixels in memory

- An in-memory display driver, replacing the EXR file, so passes reach the
  Image Viewer without disk writes. The hardest step: a driver node is
  registered from Rust as a table of functions, fiddly and dependent on
  Arnold's version.

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
