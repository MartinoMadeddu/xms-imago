# Changelog

What each release of XMS | Imago brought, newest first. A release is a tag
on `main` (`v0.2.0`); see [docs/releases.md](docs/releases.md). Day-to-day
changes, one line each, are in [LOGS.md](LOGS.md); between releases, note the
ones worth announcing under **Unreleased**, and move them under the new
version's heading when you tag it.

## [Unreleased]

## [0.5.1] - 2026-11-27

### Documentation

- The README shows the new interface: two Viewers, Arnold live in one, and
  a move kept in an Edit Session. New key points: editing in the Viewer and
  the Primitive Inspector.

## [0.5.0] - 2026-11-27

### Interface

- A Viewer can be open more than once, each with its own camera,
  look-through, shading and Arnold render (one renders at a time). The
  overlays are shared.
- The Primitive Inspector's Properties list a prim's Local Transform, Parent
  to World and Local to World, the first with the operator that last set it.
- Cube, Sphere, Grid, Camera and Light have a Transform group: Translate,
  Rotate, Rotate Order, Scale (not for a camera, dome or distant light) and
  Pivot. A light's size stays its width, height or radius.
- Moving a prim made in Imago from the Viewer or the Selection bar writes
  into the operator that made it, while nothing after it moved the prim;
  else into the Edit Session, as before. Edit Session only, in the bar, puts
  every move there. Several prims dragged at once go the same way, each
  where it is held.
- Shift+Enter over the Viewer: an Edit Primitive on the selected prims, as
  Enter in the Scene Explorer.
- The Viewer's handles are drawn three pixels wide.
- The handles turn with the prim in Local space (the Selection bar's Global
  and Local); Scale is always along its own axes.
- Maya's middle-button drag: with Move, Rotate or Scale, a middle drag
  anywhere in the Viewer moves by the handle last dragged, drawn yellow.
- A right-click menu in the Viewer: on a prim, editing, its operators,
  selecting around it, Hide, Show, Isolate, Reset Transform, Move Pivot,
  Bind Material, Look Through, Light Links, Frame; on empty space, framing,
  selecting, Show All, Shading, Camera, and Create at the clicked point.

### Fixes

- Forms that look at the scene with ICE left out (Edit Primitive, Rename,
  Reparent, Variant, Rig, Material Binding's lists, the Light Linker, the
  Image Viewer) no longer leave that scene in the cook cache for the Viewer
  to show: ICE instances could vanish after selecting such an operator.
- The template test counts points rather than merging every instance into
  one mesh, which ran out of memory on ICE: instancing.

### Changed

- A Cube, Sphere or Grid is placed by its transform, its points in its own
  space, as a loaded prim is: a Transform after it moves its placement, and
  Write USD writes an `xformOp:transform` rather than moved points. Where it
  is drawn is the same.

## [0.4.0] - 2026-11-26

### Interface

- **The Viewer starts in Hidden Line Removal.**
- **Viewer overlays**: FPS, Memory, Camera, Selection and Render at the
  Viewer's top right, chosen in File > Settings > Viewer > Overlays.
- **Settings**: Parameters' tabs inside its pages, and the Theme on its
  Interface page.
- **File > Settings** opens the Settings window directly.
- **Selection bar closer to XSI's**: the big Select arrow with what a click
  picks beside it; the tools; Translate, Rotate and Scale stacked, X, Y and
  Z down each beside a button for its tool, always shown.
- **xms_xsi_layout is the default layout**, with the graph's Parameters (P)
  shown; a layout keeps whether they are.
- **Select modes as usdview's**: Prims, Models, Instances and Prototypes,
  and Imago's Top.
- **P's Parameters over a MaterialX graph** too.
- **Tidy lays materials out left to right** into a Material Binding, as
  they are wired, instead of above it.

### Rendering

- **Arnold's clamp Max Value is 0 for no clamping**, not 1e30: at 0
  nothing is written and Arnold's own default applies.

### Fixes

- **Prims dropped from the Scene Explorer onto an operator** (Material
  Binding, Light Linker, any with Prims Location) are taken again.
- **ICE instances no longer vanish** from the Viewer and the render after
  selecting.
- **The Viewer comes back from a render** at once, not on the next camera
  move.
- **xms_main_layout is now xms_3split_layout.**

## [0.3.11] - 2026-11-26

### Interface

- **F over the Scene Explorer** brings the selected prim into view, opening
  the rows above it.
- **P's Parameters** open at half the graph's height.
- **Edit Session steps** fit the Parameters, their names all as wide.

### Viewer

- **Selecting no longer slows Imago**: the selection's places are kept
  between frames, not looked up through the whole scene each frame.
- **The Viewer is drawn only when its picture changes**: panning the graph
  or typing in a pane no longer redraws a dense wireframe.

### Rendering

- **Arnold in the Viewer keeps rendering** when the scene brings its own
  OCIO colour manager: that note had stopped it, as if it were an error.

## [0.3.10] - 2026-11-25

### Interface

- **Tools pane**: the operators' categories as coloured buttons down a
  column, after XSI's tool bars; a click opens a box of the category's
  operators, to drag onto the Node Graph (onto a wire to go into it), as in
  Shake, or click to add after the selected operator. New layout
  **xms_xsi_layout** has it down the left.
- **P over the Node Graph** shows the selected operator's Parameters over
  the graph at its top right, in a thin border, as Houdini's network editor
  does; drag their left edge, bottom edge or corner to size them.
- **Tools and Selection bars** down the window's sides: fixed, not tabs.
  Window shows and hides them; layouts keep them.
- **Selection bar**: what is selected in the Viewer, after XSI's main command
  panel: what a click picks (prim, model, top), the move, rotate and scale
  tools, the selection's scale, rotation and translation (local or global)
  to type in, and Edit Selected, Hide, Show, Deactivate. Down the right of
  xms_xsi_layout.
- **Prims selected get the Viewer's handles** when the selected operator has
  none of its own: drag to move, rotate or scale them, each about its own
  pivot.

### USD

- **Edit Session**: what you do to prims from the Viewer and the Selection
  bar goes into one operator, put in after the one viewed on the first edit,
  as steps: one per selection edited, absolute values, the last to set one
  wins. Switch steps off, rename, reorder or delete them; Freeze makes them
  one, Explode puts them in the graph as Edit Primitives.

## [0.3.9] - 2026-11-24

### USD

- **Layers with nothing to draw are read**: a layer of materials and `over`s
  (a look-dev layer for another's prims) showed nothing; it is now listed
  whole in the Scene Explorer, as Solaris lists it. What is under an `over`
  root is listed but not drawn, as USD draws only defined prims.
- **Payloads**: Load USD loads all of a stage's payloads, or none (Payloads >
  Load None), and a Scene Explorer row's right-click loads or unloads one,
  kept as a rule on the Load USD. An unloaded payload is drawn as the box of
  its `extentsHint`, and nothing below it is read: a set of payloaded assets
  opens in a fraction of the time.
- **Render meshes are read when they are needed**: the Scene Explorer's
  render toggle now starts off, as in usdview, and while it is off a stage's
  render-purpose meshes are kept as their boxes and not read. On a production
  set, reading those meshes was more than half of the load. Render geometry
  without a proxy is drawn as its box, so nothing disappears; prims with no
  purpose are drawn as always. Turning render on, rendering, or Write USD
  reads them.

### Interface

- **Phosphor icons everywhere**: the buttons, menus, History, Timeline
  transport, file browser and Add Node categories now use the same Phosphor
  icons as the operators, in place of emoji and Unicode symbols. Phosphor's
  Fill weight ships too: the view flag's eye and a key at the playhead are
  drawn filled in.
- **The Scene Explorer and the Primitive Inspector follow**, as Parameters
  does: the display flag (D, the default), the selection (S) or a pin (P).
  Two Scene Explorers can list two operators' scenes side by side; a prim
  selected in either is selected everywhere.

### Viewer

- **Box select is fast on big scenes**: a box over a scatter of a million
  instances takes a moment, not seconds, and the selection no longer slows
  every frame after it. Past 2,000 outlines, one box goes around each
  instancer's selected instances.

### Node graph

- **The wheel zooms over a node too**: in the Node Graph, ICE and MaterialX
  graphs it stopped while the pointer was on a node.

## [0.3.8] - 2026-11-24

### Interface

- **Show hidden** in the file browser: files and folders whose names start
  with a dot.
- **The Operator Stack is gone**: the node graph is the chain of operators.
  Layouts saved with it load, its tabs left out.
- **The scene graph's grid** is twice as dense.
- **Phosphor icons**: every operator's, ICE node's and Scene Explorer row's
  icon is now one of Phosphor's, one drawing style throughout, shipped with
  Imago as fonts. File > Settings > Interface > Icons: Regular (the default)
  or Light. The interface's own buttons follow.

## [0.3.7] - 2026-11-23

### USD

- **Layer Break**, as Solaris': a Write USD below it writes only what the
  operators after it change. The opinions above it, and the files of the
  stages loaded above it, are left out; prims gone since are deactivated. The
  Viewer and renders still see everything.

### Interface

- **Tabs are instances**: right-click a tab for **Pane** (what it shows
  instead) and **New Tab** (another beside it), or use the **+** of a tab bar.
  Parameters, Scene Explorer and Primitive Inspector can be open in several
  tabs. Layouts saved before load as they were.
- **Most panes open twice**: Node Graph tabs each with their own view and
  graph (the one last clicked in drives the Parameters), Curve Editor and UV
  Editor tabs each with their own view and Follows, Light Linker tabs each
  their own operator. One of a kind: Viewer, Timeline, History, Operator
  Stack, Image Viewer.
- **A Parameters tab follows** the selection, the display flag, or is pinned
  to one operator: its tab shows which, S, D or P in a circle before its
  cross; click the letter to change it. A pinned tab names its
  operator ("Parameters · Transform1").
- **Alt-drag a number of a row** (X, Y, Z, a vector's parts; Option on macOS)
  and the others move by as much. Shift, egui's finer dragging, still works
  with it.
- **A node of one input takes a wire anywhere across its top** (in an ICE
  graph, along the input's row), not only on the port's dot.
- **Enter** goes inside the selected ICE, MaterialX or compound node, as ↓ and
  a double-click do, in the scene graph and in ICE graphs.

### ICE

- **The binding deformers as snippets**: Templates > Martino > ICE has the
  point deformer, the hair deformed by guides and the rigid hair deformed by
  skin as ISnippets, code instead of nodes, carrying as their ICE graphs do.
  Their snippets are in the add menu.
- **ISL**: `matrix(x, y, z, origin)` makes a frame from its axes; a list of
  vectors times a list of numbers is item by item; intrinsics read on another
  geometry (`intrinsic:triP` on a mesh, from a point cloud) are typed by
  themselves.

## [0.3.6] - 2026-11-17

### Rendering

- **Renders in Imago's colour space**: Arnold is given Imago's OCIO config
  (`$OCIO`'s, or Imago's own), its rendering space (ACEScg) and sRGB texture
  for untagged 8-bit textures, so renders match the Viewer and Image Viewer. A
  scene with its own colour manager keeps it.

### ICE

- **Ramp** and **Colour Ramp** nodes: In, 0 to 1, through a float curve or a
  colour gradient of keys (Step, Linear, Smooth, Cubic), edited with the ramp
  widget in their parameters.

### MaterialX

- **Arrays read from USD**: a shader's array inputs (`float[]`, `color3f[]`,
  `int[]`, `token[]`) are read whole, not their first value: Arnold's ramps,
  and MaterialX's array inputs, come back from a stage as they were written.
- **Ramp widget**: MaterialX's `ramp` node edited as a gradient bar with its
  keys as handles (click to add, drag, right-click to remove), or as a float
  curve (keys as greys); its interpolation, Flip and Reset, and a row per key
  with its position and colour picker (or value). Arnold's `ai_ramp_rgb` and
  `ai_ramp_float` (SDK definitions) edited the same way, their colours written
  as `color3f[]`; Monotone Cubic interpolation (Arnold's, and ICE's ramps).
- **Texture colour spaces**: a Colour Space under a texture's file (Auto,
  `srgb_texture`, `lin_rec709`, `acescg`, `raw`, or any of the OCIO config's),
  kept from `.mtlx` files (input, node, node graph and document `colorspace`)
  and USD stages (`colorSpace`, UsdUVTexture's `sourceColorSpace`), written
  back to both, so the renderer converts each texture as it should.

## [0.3.5] - 2026-11-16

### USD

- **Collections**: the Collection operator makes a named set of prims on a
  Collection Location (`/collections`), paths or patterns included and
  excluded, written as a USD collection; files' collections are read. `%name`
  in any Prims Location list takes its members. The Scene Explorer lists them
  under the prim holding them; clicking one selects its members.
- **Light linking**: the Light Linker operator, a table of links: each row's
  lights and prims (or collections), with Light and Shadow toggles (prims
  dropped on the node get a link each), written as the lights' `lightLink` and
  `shadowLink`, which Arnold reads.
- **Light Linker pane**: the table big, beside the lights coming in; a light
  clicked shows its links alone, prims dropped on it get links for it.

### Files

- **Graph files from a newer Imago**: a newer layout is refused, saying which
  Imago saved it; an operator this Imago does not have opens as a Placeholder
  with its wires and is written back as it was on saving.

## [0.3.4] - 2026-11-12

### Rendering

- **Arnold in Imago writes nothing big**: a file's primitive a deformer or
  ICE graph changed stays the file's in the layer, its points and primvars
  sent in memory (with motion blur, as keys), on every reload too: a material
  changed, a prim added, a frame. A heavy groom no longer goes to disk.
- IPR's status says Arnold is loading and preparing the scene, not a guess at
  textures.

### Geometry

- **Tangent** in ICE (Geometry), and `@intrinsic:tangent` in snippets: the
  tangent per point (or corner), as the Tangents operator makes it.
- **Normals**: made per Point (smooth), Corner (hard past a cusp angle) or
  Face, weighed by area, angle or evenly; on curves, what ribbons face.
- **Tangents**: along U of the texture coordinates (or the edges) on meshes,
  per Point or Corner, with Bitangents; along curves.

## [0.3.3] - 2026-11-12

### USD

- **Rename** renames prims: part of the name replaced, a prefix or suffix
  added.
- **Duplicate** copies prims beside themselves, with what is below them, each
  copy a step further moved, turned and scaled.
- **Delete Attribute** and **Rename Attribute** take primvars away or rename
  them, by name pattern; Write USD blocks what went.
- **Set Attribute** sets `documentation` (the prim's `doc`) and
  `customData:<key>` entries, a `:` in the key nesting them.
- **Variant** (Create) takes a **sublayered stage**: what it has below the
  set's prim, written as references to the file's prims, since a variant
  holds no sublayer.

### Instancing

- An ICE Instancer's **prototype taken from a file** is written as a
  reference to the file's prim, not its geometry, while it is as the file has
  it.
- A **file instancer's prototypes take edits**: Edit Poly, materials,
  Set Attribute on one write an `over` on it, every copy changing.

### Rendering

- **Motion blur** for what moves in Imago: renders from Imago (Arnold, the
  Image Viewer's husk, `xms --render`) cook the scene at the camera's shutter
  open and close too, written as time samples. The Camera operator has a
  **Shutter**, -0.25 to 0.25 to start with; a Point attribute `velocities`
  or `accelerations` is written as the schema's. Render Commons' Motion Blur
  off turns blur off for everything, a file's animation included (Arnold's
  `ignore_motion_blur` too).

### ICE

- An **ISnippet** (or Snippet) set to **Run Over** Object, Face or Corner
  sets its attributes there, not per point.

### Interface

- The **file browser** shows each entry's size, date and owner; a folder
  opens on a double-click, a click picks it; the window can be moved.

### MaterialX

- Materials are **checked before Arnold renders in Imago**: a node of no
  library, a wire from nothing or between types that do not join, a loop, a
  node Arnold's MaterialX does not define. The render is not started and the
  problems are said, where Arnold could fail and take Imago with it.

### Graphs

- Shift+drag a node into or out of a wire in MaterialX graphs too, by type.

## [0.3.2] - 2026-11-11

USD work in the scene graph: Transform, Material Binding, Set Attribute, Load
USD and Write USD each do what USD lets them; and the graphs take a node into
a wire, and out of one, with Shift.

### USD

- **Transform: Rotate Order and Pivot.** USD's six orders (XYZ, about X first,
  for new Transforms; graphs saved before keep turning as they did, ZYX), and
  scale and rotation about the Pivot, where the Viewer's handles are. A mesh
  moved in place keeps its primvars; a mirroring scale turns its faces round.
- **Material Binding: purpose and strength per row.** All, Preview or Full
  (`material:binding`, `:preview`, `:full`; the Viewer draws the preview one),
  and ⬇ for `bindMaterialAs = "strongerThanDescendants"`.
- **Set Attribute: keys and arrays.** ◇ beside a row keys its value at the
  playhead, written by Write USD as `timeSamples`; arrays (`int[]`, `float[]`,
  `float3[]`, `color3f[]`, `string[]`, `token[]`) typed between commas.
- **Load USD: Population Mask.** Rows of prim paths: only those prims, what is
  below them and their parents are read; a big stage is read quicker.
- **Write USD: Flatten.** One file of the whole scene, the stages and Imago's
  changes composed into it, time samples kept, `.usda` or `.usdc`.
- **Numbers** very large or very small are written with an exponent (`1e30`).

### Graphs and the rest

- **A node into a wire, and out of one:** in the scene and ICE graphs, Shift
  held, a node dragged over a wire goes into it; dragged a few pixels, it comes
  out of the wires it is in, what fed it joined to what it fed.
- **ICE Instancer:** prototypes given by a Get Reference are that input's
  prims alone.
- **MaterialX files of an older MaterialX** load: a definition renamed since
  (1.38's `ND_normalmap`) is found by its node and types.

## [0.3.1] - 2026-11-11

ICE gets a language: **ISL**, the ICE Scripting Language, code that compiles
into ICE's own nodes, as a node in ICE graphs or an operator in the scene
graph. And the Viewer reads data better: labels as Houdini draws them, points
and wireframe over the shading, curves you can click.

### ISL and snippets

- **ISL, the ICE Scripting Language:** OSL's syntax and types with ICE's
  attributes (`@P += normalize(@N) * amount;`): `if`, loops of a known count,
  functions, and the ICE nodes as functions (`get_data`, `get_array_data`,
  `near_points`, `closest_point_on_surface`, `random`, `noise`). Not a second
  engine: a snippet is lowered into ICE's nodes and runs as fast as they do.
  Its parameters are its ports; errors show with their line. See
  [ISL](docs/isl.md) and [its functions](docs/isl-functions.md), in the HTML
  documentation too.
- **Snippet node** in ICE graphs, three examples in the add menu (roughen,
  relax, stick); **Make Compound** turns one into a compound with its
  parameters as ports; Ctrl+Enter applies the code, on macOS too.
- **ISnippet:** a snippet as a scene-graph operator (Houdini's Attribute
  Wrangle, Imago's way), yellow, its code, Run Over, References and parameters
  in its Parameters tab. **Make ICE Graph** puts beside it an ICreationEngine
  holding the nodes it runs. Two templates: Snippets: roughen and relax,
  Snippets: stick to a surface.
- **References:** a snippet's geometry parameters default to its reference
  inputs (`geometry ground = 1`, or `= get_reference(1, "/set/ground*")`).
  Get Reference on one input searches that input alone: Closest Point, Near
  Points and Get Data no longer find prims at the same paths on another.
- **New ICE nodes:** OSL's noise, ported and checked against its own values
  (Perlin, Simplex and Cell Noise; BSD-3-Clause); logic (Less Than…Not Equal,
  And, Or, Not, and **If**, a value or another per element); OSL's maths
  (Square Root, Power, Exponential, Logarithm, Floor, Ceiling, Round, Sign,
  Modulo, Tangent, Arcsine, Arccosine, Arctangent of Y over X).

### The Viewer

- **Display Attribute's labels as Houdini's:** the text alone with a dark
  shadow, no box, coloured by context as XSI's (points green, just right of
  them; faces lilac and corners amber, centred); a list as a column. Every
  label in front of a surface is drawn: a left-handed mesh no longer loses
  most of its labels, a curved face's centre is no longer hidden by its own
  face, and a dense prim in front (a groom) no longer uses up the labels
  before the rest are reached.
- **Display Attribute's A** opens into a box holding its settings, over the
  picture, and closes again, as a Snippet's code does.
- **Points** and **Wireframe on Shaded**, under the shading modes: every point
  of meshes and curves as a dot, as Houdini's Display Points, and the
  polygons' edges over Shaded and Textured.
- **Curves and points selectable:** clicked near one of their lines, or
  touched by a box, at every copy.
- **The selection** in the Viewer's own blue, a quarter less saturated, so it
  reads on the geometry without glaring.
- **README:** ISL and Display Attribute among Imago's main parts.

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
