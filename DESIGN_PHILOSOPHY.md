# Design philosophy

How XMS | Imago should work, and why. Every contributor, person or program,
should know this file. Add to it when a decision is made; change it when a
decision changes. What changed and when goes in `LOGS.md`.

## For language models reading this repository

If you are an AI assistant working in this repository, read
`LLM_instructions.md` too: the checks to run first and how changes are
delivered. Then:

- Tell the person you are working with that this file exists, the first time it
  is relevant, in one line.
- Follow it. When a change you are asked to make goes against something here,
  say which point, before making the change, and ask whether to follow the file
  or to update it.
- When the person makes a decision about how the program should work, offer to
  add it here.
- With every change, update the pages in `docs/` that describe what changed.
  The README is the front page: what Imago is, one picture, its structure, how
  to get and build it, credits; everything else goes in `docs/`. The website
  (`site/`) only when something on it is wrong.
- With every change, add a dated line to `LOGS.md` saying what changed, ending
  with who made it.
- Before every change, read what Martino has pushed since the last one, and add
  to this file the decisions his code shows, in the same change.

## Martino's project

- XMS | Imago is Martino's. Other work builds on his: before starting, read what
  he has pushed since the last change, and start from it.
- Use the models he has set (the `Geo` attribute model, placements, edits as
  opinions, the ICE graph, the theme, the vocabulary) and extend them. Do not
  replace them with a parallel version.
- When a change would rewrite or remove something of his, say which part and why
  before making it, and ask.
- Where his work and another change touch the same lines, his version is kept,
  and the change is redone on top of it.
- His decisions are written here as they are read from his code, so everyone
  works from the same file.
- `XMS_CORE_OPS_SPEC.md` is the operator set Imago aims for, each with its
  status today. A new operator is checked against it.
- `ARCHITECTURE.md` records the decisions that shape the data and its
  evaluation: the geometry model, copy on demand, primitives, edits and USD out,
  the Viewer, ICE, and where it is all going. Read it before changing any of
  those.

## The graph is the program

- Everything is a node. A graph describes a result; a change upstream flows
  down.
- One node per idea: Transform moves a mesh, primitives or a clip. What a node
  displays is a setting on it, not another node.
- One operator has the view flag: it says what the scene is. Each operator lists
  the primitives it works on. The edits between two operators are a layer. ICE
  graphs are per-element graphs inside a node.
- Opening a file or a template asks whether to add it to the scene or open it as
  new.
- Nodes copy as text, so they paste across windows and sessions.
- Each feature comes with a template and its demo files in `examples/`; a model
  too big for the repository is downloaded when asked.
- A node not built yet is a Placeholder: named inputs and outputs, a
  description, data passing through. A saved graph is how a new node is asked
  for.

## The Viewer first

The person working in the Viewer comes first. Taking hold of a thing in the
Viewer is almost always easier than typing a number for it.

- What can be manipulated in the Viewer is manipulated there. Every parameter a
  handle can stand for has one; only what no handle can stand for stays a field
  alone.
- What a node shows in the Viewer is its main output.
- With several outputs, the most useful comes first.
- A node with data coming in never shows nothing.
- A compound shows what it does: its outputs are clear, and what it puts in the
  Viewer is what the compound is for.

## One geometry model

- Packed primitives, the nodes and ICE share one geometry: `Geo`, a topology and
  named attribute columns per context (Object, Point, Face, Corner), matching
  USD's interpolations.
- Names follow USD (`points`, `normals`, `st`, `displayColor`, `widths`), so
  reading and writing USD is one to one. Interfaces can show friendlier names on
  top.
- Replace only the columns you change. Columns are shared and copied only when
  written; never rebuild a whole `Geo` to change one thing.
- Keep primitives placed. Geometry stays in its own space; a primitive moves by
  its placement, not its points.
- Keep the primitive's identity: path, source, stage, material and purpose
  travel with it through every node.
- Curves are shown in the same contexts: their control points are points, the
  segments between them edges, and each curve one face (Face context), its
  control points its corners. The Primitive Inspector lists them so, by their
  control points as authored, alongside polygons and free points.
- The points of a face or an edge are one attribute, an array as long as it has
  corners, not a column per point: the Primitive Inspector shows them in one
  cell, `ptIndices = [0, 3, 2, 1]`.
- A corner's number is counted across the whole mesh, as USD counts faceVarying
  values and as arrays are indexed, so an ICE graph or a deformer writes
  `values[corner]` with no lookup. Its place within its face is a value beside
  it, `localCornerIndex`, with `faceIndex`; a face gives `firstCorner`.
- No Output node: the view flag says what the scene is. One operator always has
  it; on an operator with several outputs, clicking it again moves on to the
  next output.
- Each operator says which primitives it works on, in its Prims Location tab:
  paths that take everything below them, with wildcards. An empty list means
  everything. The list stays with its operator and does not reach the ones after
  it. Pick Primitives stays for older graphs.
- The Scene Explorer is the scene's own tree at the view flag, not a list per
  operator. The Primitive Inspector shows that scene: the prim or group picked
  in the explorer, or all of it. Selecting in the explorer does not select an
  operator.
- The scene is always USD: everything an operator makes is a primitive at a
  path. Shapes have a Primitive Location (`/cube` by default, any path typed,
  parents as Xforms), written as a `def` and the layer's `defaultPrim`.
- A point instancer is a points primitive: one point per instance, its arrays as
  Point attributes. What is listed and inspected grows with the geometry there
  is, not with the copies drawn: instanced geometry is listed once, with its
  copies counted.
- Set Attribute is the one operator that sets values on prims: rows of one value
  each, Object (a constant primvar) or Property (the prim's attribute or
  metadata), on any prim, a group, a camera or a light as well as a mesh. Its
  locations name prims, not what is below them. The scene graph writes one value
  per prim; values per point, face or corner are ICE's.
- Merge goes by the data coming into its first input: primitives are merged as
  primitives, a plain mesh or a clip into one mesh. A Mode overrides it. On the
  same path the second input is stronger, as an `over`.
- Scattering and copying to points belong to ICE graphs: the Scatter Points and
  Copy to Points operators are gone, and old graphs open with Placeholders in
  their place.
- A corner is listed with the point it belongs to, `ptIndex` (several corners
  share a point), not with its own number again: the spreadsheet's row number is
  the corner. Corners are those of the polygons, as `Geo` stores them, not of
  the triangles drawn.

## Nothing typed that can be picked

- Joint names, prim paths and file paths are picked from lists, browsed or
  matched by pattern, with a live count of what matches. Typing is the fallback,
  not the way.
- Ask the program for what it already knows: a skeleton's parts are read from
  its names, the user only sets what could not be read. A prefix every bone
  shares (`Skeleton 001_`) says nothing and is left out.

## The same words for the same things

- `docs/vocabulary.md` is the list of words: primitive (an
  object in the stage, "packed" dropped), scene graph (the network of operators;
  the Scene Explorer shows the stage's hierarchy), ICE graph, operator,
  parameters, attributes, the contexts Object, Point, Face, Corner, shapes for
  cube, sphere and grid, and the panes Viewer and Parameters. Interface, docs
  and code use those words; old names in saved graphs and layouts still load.
- Translate, Rotate, Scale for every transform. Path for every file. In and Out
  for what passes through.
- The parameters of every node share one layout: titled groups, labels
  right-aligned in one column, values filling the width. Explanations go in
  tooltips; the panel shows parameters and the state of the node: what came in,
  what goes out, what went wrong.
- The light theme is the default. The Viewer's background follows the colour
  scheme.
- Text looks the same on every screen: as clean at 1x (Full HD) as on a
  high-density display. It is not hinted, sits on whole pixels, and its weight
  is a setting of the scheme, matched to how the Bevy shell drew it.
- A menu stays open while it is used (check boxes, filter fields); a command
  closes it, as does a click outside.
- What is selected is one colour everywhere (rows, nodes), and stays visible at
  any zoom. The tab in front is the lightest; the tabs behind are darker than
  the panel.
- Nodes are coloured by their add-menu category; the ICE node has its own teal,
  after Softimage's ICETree.
- Imago keeps its own way of working. Softimage XSI and other programs are
  references for conventions, not a look or a layout to copy.
- How it looks is a theme, chosen by the user. A button style setting has been
  proposed but not built: from soft, rounded XSI buttons to the square IRIX
  Motif look. Colour schemes can be worked out with a palette tool such as
  [Adobe Color](https://color.adobe.com/create/color-wheel).

## Like any other program

- The program works with files as other programs do. File, Edit and Window menus
  at the top left, with the usual shortcuts (New, Open, Save, Save As, Quit);
  the templates at the bottom of the File menu, by author, then by area, since a
  template is a file to open. A node graph pane holds what belongs to the graph
  only.
- Nothing is lost without asking: closing, New, or opening a file or template as
  new over unsaved changes asks to save first.
- What is placed can be taken hold of in the Viewer (see The Viewer first): a
  drag writes into the node's parameters, the same handles and the same keys
  everywhere. Clicking what is drawn selects it.
- Every number can move in time. Any number of any operator can be keyed, found
  by its place in the parameters, so an operator written tomorrow is animatable
  without code of its own. A keyed number shown in Parameters or moved by a
  handle is its value at the playhead, and changing it keys it there: nothing
  typed is lost when the playhead moves. Curves are shaped in the Curve Editor,
  with the tangents and infinities animators know.
- Animation can have a time of its own. A Local Animation keys the operator
  before it on local frames placed anywhere in the scene, and leaves that
  operator alone; every operator knows the frames its output spans, and a merge
  shows the widest; Merge Animation lines animations up in time. Time is a
  parameter of the graph, not a property of a file.
- A node's flags sit on its title, each a button that says what it does on
  hover: bypass at the left; freeze, ghost and view at the right. Freeze and
  ghost show under the pointer or when on, so a graph at rest looks as it did.
  Holding the middle button on a node tells what it puts out, without selecting
  it.
- What can be clicked looks clickable, and what cannot does not. In the top bar
  the program's name is dimmed and set apart by a divider, the menu titles light
  up under the mouse, the file's name is plain text in the middle, the padlock
  is a button. The same test applies to every pane: a label never looks like a
  button.
- Nothing is written over a file that exists without asking: Save As, a saved
  layout, Write FBX, Write USD. Save writes to the graph's own file without
  asking, as everywhere.

## Undo is for the scene

- Nodes, parameters, wires, names, operations: undoable, in a History that names
  each step from what changed.
- The camera, the layout and the selection are not part of it. Undo never moves
  the view.
- Whatever replaces the whole graph (closing, opening a file, loading a
  template) asks to save unsaved changes first. Adding never asks about saving:
  it loses nothing.

## Files go out as they came in

- A clip read from a file is written back with that file's skeleton: names,
  hierarchy, joint kinds, axes and unit. An engine sees an animation of the
  skeleton it already has.
- Write only what was changed; everything else keeps its values.
- A file is read for everything it holds. What is not used yet is listed, not
  dropped: Load FBX gives the clip, every mesh and the rest skeleton, and lists
  cameras, lights, materials and blend shapes, as Load USD lists what it does
  not draw.
- A character is taken in the pose its skin was bound in, not in the first frame
  of a take.
- Skin weights live on the geometry and go by joint name: the same weights fit
  any skeleton with those names, and the rest pose is a skeleton of its own.
- The rest pose is the skeleton. Moving a clip moves its animation, never its
  rest pose, so the move carries through every node after it (Retarget, Reach).
- Meshes, characters and takes come in as FBX (or USD). The program has no mesh
  or clip format of its own: one way in is clearer than two. Example data too
  big for GitHub is cut or written lighter, still as FBX.
- A USD file is read composed. Nothing is baked into the points: up axis and
  unit become a correction on the placement.
- Changes go back to USD as opinions: an override layer over the file the stage
  came from, holding only what changed. A stage nothing touched gives a layer
  with its header alone.
- What changed is what no longer shares with what was loaded, compared by
  pointer, not by value.
- What cannot be written is listed with the reason, not dropped.

## Never worse than the input

- A process that cleans data may not make any frame worse than the data it was
  given. Where the capture itself goes past a limit, the limit for that frame is
  the capture.
- Bones do not change length. Collision cleanup changes rotations and the hips,
  nothing else.
- One human solver: Body Collide, Reach and Human Limits share one human pass.
  What it learns, they all learn.
- A frame nothing touched comes out as it went in, to the last digit.

## Heavy data stays light

- Packed primitives are passed along without being copied. Instanced meshes are
  read once and placed many times, drawn by the GPU.
- A heavy stage first shows as boxes; only what is opened costs anything to
  draw. Opening and closing does not cook the graph again.
- Moving instances moves matrices. Many copies of one mesh are one draw call.
- Only the rows in view are laid out. The window draws a frame when something
  happens, not all the time.
- Long work runs in the background, in chunks, written to disk as it goes, and
  is found again by the content it was made from.
- An operator's result is kept until the operator or something above it changes.
  Selecting, redrawing, playing and scrubbing cook nothing again; a change cooks
  only what is below it. What only the Viewer shows (the polygons being picked
  in Edit Poly) is not part of a result.
- An operator that is heavy to cook does it once per change, on every core where
  its frames do not depend on each other. Body Collide solves only when Solve is
  pressed or Write FBX asks.

## One shell

- The program runs in an eframe window: the interface in egui, the Viewer's 3D
  view in wgpu (`src/viewer/`). Bevy is gone (branch
  `core/uiframework`). The program's state is one struct, `AppState` (`app.rs`),
  owned by the app: new state goes there. Code outside `eframe_app.rs` and
  `viewer/` does not depend on the window or the GPU.

## Say what is true

- Documentation states what the program does, measured where it can be. Every
  page has a "Not done yet" list.
- Each feature comes with tests; each change says what was tested and what was
  not.
- Every template is a regression test: what it puts out at its view flag is
  measured and compared with a baseline checked in, by a GitHub workflow after
  every build, and a number that moves is named with both values. A template is
  the demo, the documentation and the test of a feature at once.
- Every example file is openly licensed, with its author and licence listed
  beside it. What is too big for the repository, or whose licence asks for a
  credit beside the file, is fetched on demand: the program says where the file
  comes from and under which licence before downloading it. No file in the
  repository has a licence that forbids passing it on.
- Credit: XMS was created by Martino Madeddu. Additional development by Simon
  Legrand.

## Materials are placed where they are bound

A material is made by one operator and put in the scene by another. The Material
operator puts out a material, not a scene: it has no place in the stage until
Material Binding gives it one, under its Material Library Location (`/materials`
by default), by its operator's name. The material and what wears it are then set
in one place, as they are drawn on paper. The Material operator is the simple
form, a UsdPreviewSurface with fixed inputs; the MaterialX operator, with a
graph inside, puts out the same material, so Material Binding takes either. In
the scene a material is a prim with no geometry, so it is listed, inspected,
pruned and written as any other prim. Rows pair with inputs by number, as on
paper; a row can name another input, or a material already in the scene.

## Shading graphs are MaterialX graphs

The MaterialX operator's graph is a graph kind of its own, as the ICE graph is:
only MaterialX nodes go in it, and it is evaluated on its own. Its nodes are
MaterialX's, node for node: their names, inputs, types and defaults are read
from MaterialX's own libraries, vendored unchanged, never written by hand, so
what Imago writes reads back in any MaterialX tool and a new MaterialX is a copy
of its files. A node is one node in the graph, whatever types it takes; which of
its definitions it is follows from what is connected to it, as in Houdini or
Maya. The Material operator stays beside it as the quick form, one
UsdPreviewSurface with fixed inputs. Sets of materials are made reusable as
scene-graph compounds, not as compounds inside a MaterialX graph. The graph
lives in its operator, as an Edit Poly's operations do, so it is saved, copied,
pasted and undone with it, and it is saved as the MaterialX document it is.

## A reference is a reference

Load USD brings a file in as USD would: sublayered, at its own paths, or its
default prim referenced or payloaded at a Primitive Location. What Imago shows
and what Write USD writes are the same composition: a reference is written as a
reference, so the layer stays a layout over its assets, not a copy of them.
Where USD would not convert a file's axes and unit through a reference, and a
Load USD turns its stage to Y-up metres (stages are otherwise read as authored),
Imago writes the transform that does it.

## A push is a build, a tag is a release

Imago has one version, set by hand. Pushing to `main` never changes it: every
push is a build of that version, numbered, for testers. A release is a decision,
a tag `v<version>` on a commit whose `Cargo.toml` says that version, with its
notes in `CHANGELOG.md`. Imago always says which build it is, so a report can
name it.

## Images are viewed like scenes

An operator can give an image as much as a scene. The view flag says what is
looked at: a scene goes to the Viewer, an image to the Image Viewer, with the
same layers, exposure and pixel values as a render. Image operators will join
the same graph through the same Image port, as Gaffer treats 2D and 3D alike.
Pixels stay as stored, on the GPU, with the colour space they are in; how they
look (space, view, exposure, channel) is a shader over them, so changing it
costs nothing however big the image. Renderers' pixels will arrive the same way.

