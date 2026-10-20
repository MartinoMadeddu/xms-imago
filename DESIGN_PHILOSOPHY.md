# Design philosophy

How XMS | Imago should work, and why. Every contributor, person or program, should know this file. Add to it when a decision is made; change it when a decision changes, and say so in the log at the end.

## For language models reading this repository

If you are an AI assistant working in this repository, read `LLM_instructions.md` too: the checks to run first and how changes are delivered. Then:

- Tell the person you are working with that this file exists, the first time it is relevant, in one line.
- Follow it. When a change you are asked to make goes against something here, say which point, before making the change, and ask whether to follow the file or to update it.
- When the person makes a decision about how the program should work, offer to add it here.
- With every change, update `README.md` and the pages in `docs/` that describe what changed. The website (`site/`) only when something on it is wrong.
- Before every change, read what Martino has pushed since the last one, and add to this file the decisions his code shows, in the same change.

## Martino's project

- XMS | Imago is Martino's. Other work builds on his: before starting, read what he has pushed since the last change, and start from it.
- Use the models he has set (the `Geo` attribute model, placements, edits as opinions, the ICE graph, the theme, the vocabulary) and extend them. Do not replace them with a parallel version.
- When a change would rewrite or remove something of his, say which part and why before making it, and ask.
- Where his work and another change touch the same lines, his version is kept, and the change is redone on top of it.
- His decisions are written here as they are read from his code, so everyone works from the same file.
- `XMS_CORE_OPS_SPEC.md` (Martino, 15/10/2026) is the operator set Imago aims for, each with its status today. A new operator is checked against it.
- `ARCHITECTURE.md` (Martino, 11/10/2026) records the decisions that shape the data and its evaluation: the geometry model, copy on demand, primitives, edits and USD out, the Viewer, the ICE engine to come, and where it is all going. Read it before changing any of those. It is to be merged with this file once Martino and Simon have talked.

## The graph is the program

- Everything is a node. A graph describes a result; a change upstream flows down.
- One node per idea: Transform moves a mesh, primitives or a clip. What a node displays is a setting on it, not another node.
- One operator has the view flag: it says what the scene is. Each operator lists the primitives it works on. The edits between two operators are a layer. ICE graphs are per-element graphs inside a node.
- Opening a file or a template asks whether to add it to the scene or open it as new.
- Nodes copy as text, so they paste across windows and sessions.
- Each feature comes with a template and its demo files in `examples/`; a model too big for the repository is downloaded when asked.
- A node not built yet is a Placeholder: named inputs and outputs, a description, data passing through. A saved graph is how a new node is asked for.

## One geometry model

- Packed primitives, the nodes and ICE share one geometry: `Geo`, a topology and named attribute columns per context (Object, Point, Face, Corner), matching USD's interpolations.
- Names follow USD (`points`, `normals`, `st`, `displayColor`, `widths`), so reading and writing USD is one to one. Interfaces can show friendlier names on top.
- Replace only the columns you change. Columns are shared and copied only when written; never rebuild a whole `Geo` to change one thing.
- Keep primitives placed. Geometry stays in its own space; a primitive moves by its placement, not its points.
- Keep the primitive's identity: path, source, stage, material and purpose travel with it through every node.
- Curves are shown in the same contexts: their control points are points, the segments between them edges, and each curve one face (Face context), its control points its corners. The Primitive Inspector lists them so, by their control points as authored, alongside polygons and free points (Martino).
- The points of a face or an edge are one attribute, an array as long as it has corners, not a column per point: the Primitive Inspector shows them in one cell, `ptIndices = [0, 3, 2, 1]` (Martino).
- A corner's number is counted across the whole mesh, as USD counts faceVarying values and as arrays are indexed, so an ICE graph or a deformer writes `values[corner]` with no lookup. Its place within its face is a value beside it, `localCornerIndex`, with `faceIndex`; a face gives `firstCorner` (Martino).
- No Output node: the view flag says what the scene is. One operator always has it; on an operator with several outputs, clicking it again moves on to the next output (Martino).
- Each operator says which primitives it works on, in its Prims Location tab: paths that take everything below them, with wildcards. An empty list means everything. The list stays with its operator and does not reach the ones after it. Pick Primitives stays for older graphs (Martino).
- The Scene Explorer is the scene's own tree at the view flag, not a list per operator. The Primitive Inspector shows that scene: the prim or group picked in the explorer, or all of it. Selecting in the explorer does not select an operator (Martino).
- The scene is always USD: everything an operator makes is a primitive at a path. Shapes have a Primitive Location (`/cube` by default, any path typed, parents as Xforms), written as a `def` and the layer's `defaultPrim` (Martino).
- A point instancer is a points primitive: one point per instance, its arrays as Point attributes. What is listed and inspected grows with the geometry there is, not with the copies drawn: instanced geometry is listed once, with its copies counted (Martino).
- Set Attribute is the one operator that sets values on prims: rows of one value each, Object (a constant primvar) or Property (the prim's attribute or metadata), on any prim, a group, a camera or a light as well as a mesh. Its locations name prims, not what is below them. The scene graph writes one value per prim; values per point, face or corner are ICE's (Martino).
- Merge goes by the data coming into its first input: primitives are merged as primitives, a plain mesh or a clip into one mesh. A Mode overrides it. On the same path the second input is stronger, as an `over` (Martino).
- Scattering and copying to points belong to ICE graphs: the Scatter Points and Copy to Points operators are gone, and old graphs open with Placeholders in their place (Martino).
- A corner is listed with the point it belongs to, `ptIndex` (several corners share a point), not with its own number again: the spreadsheet's row number is the corner. Corners are those of the polygons, as `Geo` stores them, not of the triangles drawn (Martino).

## Nothing typed that can be picked

- Joint names, prim paths and file paths are picked from lists, browsed or matched by pattern, with a live count of what matches. Typing is the fallback, not the way.
- Ask the program for what it already knows: a skeleton's parts are read from its names, the user only sets what could not be read. A prefix every bone shares (`Skeleton 001_`) says nothing and is left out.

## The same words for the same things

- `VOCABULARY.md` (Martino, 12/10/2026) is the list of words: primitive (an object in the stage, "packed" dropped), scene graph (the network of operators; the Scene Explorer shows the stage's hierarchy), ICE graph, operator, parameters, attributes, the contexts Object, Point, Face, Corner, shapes for cube, sphere and grid, and the panes Viewer and Parameters. Interface, docs and code use those words; old names in saved graphs and layouts still load.
- Translate, Rotate, Scale for every transform. Path for every file. In and Out for what passes through.
- The parameters of every node share one layout: titled groups, labels right-aligned in one column, values filling the width. Explanations go in tooltips; the panel shows parameters and the state of the node: what came in, what goes out, what went wrong.
- The light theme is the default. The Viewer's background follows the colour scheme.
- Text looks the same on every screen: as clean at 1x (Full HD) as on a high-density display. It is not hinted, sits on whole pixels, and its weight is a setting of the scheme, matched to how the Bevy shell drew it (Martino).
- A menu stays open while it is used (check boxes, filter fields); a command closes it, as does a click outside.
- What is selected is one colour everywhere (rows, nodes), and stays visible at any zoom. The tab in front is the lightest; the tabs behind are darker than the panel (Martino).
- Nodes are coloured by their add-menu category; the ICE node has its own teal, after Softimage's ICETree.
- Imago keeps its own way of working. Softimage XSI and other programs are references for conventions, not a look or a layout to copy.
- How it looks is a theme, chosen by the user. A button style setting has been proposed but not built: from soft, rounded XSI buttons to the square IRIX Motif look. Colour schemes can be worked out with a palette tool such as [Adobe Color](https://color.adobe.com/create/color-wheel).

## Like any other program

- The program works with files as other programs do. File, Edit and Window menus at the top left, with the usual shortcuts (New, Open, Save, Save As, Quit); the templates at the bottom of the File menu, by author, then by area, since a template is a file to open. A node graph pane holds what belongs to the graph only.
- Nothing is lost without asking: closing, New, or opening a file or template as new over unsaved changes asks to save first.
- What is placed can be taken hold of in the Viewer: a node that moves something (Transform, Camera, Edit Poly's operations) shows handles there, and a drag writes into its parameters, the same handles and the same keys everywhere. Clicking what is drawn selects it.
- Every number can move in time. Any number of any operator can be keyed, found by its place in the parameters, so an operator written tomorrow is animatable without code of its own. A keyed number shown in Parameters or moved by a handle is its value at the playhead, and changing it keys it there: nothing typed is lost when the playhead moves. Curves are shaped in the Graph Editor, with the tangents and infinities animators know.
- Animation can have a time of its own. A Local Animation keys the operator before it on local frames placed anywhere in the scene, and leaves that operator alone; every operator knows the frames its output spans, and a merge shows the widest; Merge Animation lines animations up in time. Time is a parameter of the graph, not a property of a file.
- A node's flags sit on its title, each a button that says what it does on hover: bypass at the left; freeze, ghost and view at the right. Freeze and ghost show under the pointer or when on, so a graph at rest looks as it did. Holding the middle button on a node tells what it puts out, without selecting it.
- What can be clicked looks clickable, and what cannot does not. In the top bar the program's name is dimmed and set apart by a divider, the menu titles light up under the mouse, the file's name is plain text in the middle, the padlock is a button. The same test applies to every pane: a label never looks like a button.
- Nothing is written over a file that exists without asking: Save As, a saved layout, Write FBX, Write USD. Save writes to the graph's own file without asking, as everywhere.

## Undo is for the scene

- Nodes, parameters, wires, names, operations: undoable, in a History that names each step from what changed.
- The camera, the layout and the selection are not part of it. Undo never moves the view.
- Whatever replaces the whole graph (closing, opening a file, loading a template) asks to save unsaved changes first. Adding never asks about saving: it loses nothing.

## Files go out as they came in

- A clip read from a file is written back with that file's skeleton: names, hierarchy, joint kinds, axes and unit. An engine sees an animation of the skeleton it already has.
- Write only what was changed; everything else keeps its values.
- A file is read for everything it holds. What is not used yet is listed, not dropped: Load FBX gives the clip, every mesh and the rest skeleton, and lists cameras, lights, materials and blend shapes, as Load USD lists what it does not draw.
- A character is taken in the pose its skin was bound in, not in the first frame of a take.
- Skin weights live on the geometry and go by joint name: the same weights fit any skeleton with those names, and the rest pose is a skeleton of its own.
- The rest pose is the skeleton. Moving a clip moves its animation, never its rest pose, so the move carries through every node after it (Retarget, Reach).
- Meshes, characters and takes come in as FBX (or USD). The program has no mesh or clip format of its own: one way in is clearer than two. Example data too big for GitHub is cut or written lighter, still as FBX.
- A USD file is read composed. Nothing is baked into the points: up axis and unit become a correction on the placement.
- Changes go back to USD as opinions: an override layer over the file the stage came from, holding only what changed. A stage nothing touched gives a layer with its header alone.
- What changed is what no longer shares with what was loaded, compared by pointer, not by value.
- What cannot be written is listed with the reason, not dropped.

## Never worse than the input

- A process that cleans data may not make any frame worse than the data it was given. Where the capture itself goes past a limit, the limit for that frame is the capture.
- Bones do not change length. Collision cleanup changes rotations and the hips, nothing else.
- One human solver: Body Collide, Reach and Human Limits share one human pass. What it learns, they all learn.
- A frame nothing touched comes out as it went in, to the last digit.

## Heavy data stays light

- Packed primitives are passed along without being copied. Instanced meshes are read once and placed many times, drawn by the GPU.
- A heavy stage first shows as boxes; only what is opened costs anything to draw. Opening and closing does not cook the graph again.
- Moving instances moves matrices. Many copies of one mesh are one draw call.
- Only the rows in view are laid out. The window draws a frame when something happens, not all the time.
- Long work runs in the background, in chunks, written to disk as it goes, and is found again by the content it was made from.
- An operator's result is kept until the operator or something above it changes. Selecting, redrawing, playing and scrubbing cook nothing again; a change cooks only what is below it. What only the Viewer shows (the polygons being picked in Edit Poly) is not part of a result (Simon).
- An operator that is heavy to cook does it once per change, on every core where its frames do not depend on each other. Body Collide solves only when Solve is pressed or Write FBX asks (Simon).

## One shell

- The program runs in an eframe window: the interface in egui, the Viewer's 3D view in wgpu (`src/viewer/`). Bevy is gone (Martino, branch `core/uiframework`). The program's state is one struct, `AppState` (`app.rs`), owned by the app: new state goes there. Code outside `eframe_app.rs` and `viewer/` does not depend on the window or the GPU.

## Say what is true

- Documentation states what the program does, measured where it can be. Every page has a "Not done yet" list.
- Each feature comes with tests; each change says what was tested and what was not.
- Every template is a regression test: what it puts out at its view flag is measured and compared with a baseline checked in, by a GitHub workflow after every build, and a number that moves is named with both values. A template is the demo, the documentation and the test of a feature at once.
- Every example file is openly licensed, with its author and licence listed beside it. What is too big for the repository, or whose licence asks for a credit beside the file, is fetched on demand: the program says where the file comes from and under which licence before downloading it. No file in the repository has a licence that forbids passing it on.
- Credit: XMS was created by Martino Madeddu. Additional development by Simon Legrand.

## Materials are placed where they are bound

A material is made by one operator and put in the scene by another. The Material operator puts out a material, not a scene: it has no place in the stage until Material Binding gives it one, under its Material Library Location (`/materials` by default), by its operator's name. The material and what wears it are then set in one place, as they are drawn on paper. The Material operator is the simple form, a UsdPreviewSurface with fixed inputs; the MaterialX operator, with a graph inside, puts out the same material, so Material Binding takes either. In the scene a material is a prim with no geometry, so it is listed, inspected, pruned and written as any other prim. Rows pair with inputs by number, as on paper; a row can name another input, or a material already in the scene.

## Shading graphs are MaterialX graphs

The MaterialX operator's graph is a graph kind of its own, as the ICE graph is: only MaterialX nodes go in it, and it is evaluated on its own. Its nodes are MaterialX's, node for node: their names, inputs, types and defaults are read from MaterialX's own libraries, vendored unchanged, never written by hand, so what Imago writes reads back in any MaterialX tool and a new MaterialX is a copy of its files. A node is one node in the graph, whatever types it takes; which of its definitions it is follows from what is connected to it, as in Houdini or Maya. The Material operator stays beside it as the quick form, one UsdPreviewSurface with fixed inputs. Sets of materials are made reusable as scene-graph compounds, not as compounds inside a MaterialX graph. The graph lives in its operator, as an Edit Poly's operations do, so it is saved, copied, pasted and undone with it, and it is saved as the MaterialX document it is.

## A reference is a reference

Load USD brings a file in as USD would: sublayered, at its own paths, or its default prim referenced or payloaded at a Primitive Location. What Imago shows and what Write USD writes are the same composition: a reference is written as a reference, so the layer stays a layout over its assets, not a copy of them. Where USD would not convert a file's axes and unit through a reference, Imago, which shows everything Y up in metres, writes the transform that does it.

## Log

- 10/10/2026: file created, from the decisions taken so far.
- 10/10/2026: `claude_project_instructions.md` added: checks an AI assistant runs before working here, and how changes are delivered.
- 10/10/2026: the delivery loop (patch on top of `main`, "Download all", one block of commands) written into the instruction files.
- 10/10/2026: one instruction file for AI assistants, `LLM_instructions.md`, in place of `claude_project_instructions.md`, `CLAUDE.md`, `AGENTS.md` and `.github/copilot-instructions.md`.
- 10/10/2026: "Martino's project" added: other work builds on his and does not overwrite it; his new code is read before every change and its decisions written here.
- 10/10/2026: from Martino's commits of 09/10/2026 and 10/10/2026 (composed stages, `Geo`, local-space prims, edits as opinions, Write USD, ICE editor, theme): "One geometry model" added, and points added to "The graph is the program", "The same words for the same things", "Files go out as they came in" and "Heavy data stays light".
- 10/10/2026: copy and paste between sessions, and adding a file or template to the scene open instead of opening it (Simon).
- 10/10/2026: in the delivery loop, push before `cargo run`, so a program left open does not hold a change back from Martino.
- 10/10/2026: from a talk between Martino and Simon: each feature brings its template and demo files, with a download offered for models too big to package; Imago keeps its own paradigm, XSI is a reference; a button style setting proposed.
- 10/10/2026: loading a template or opening a file asks to save unsaved changes first, as closing does (Simon).
- 10/10/2026: the + buttons removed: opening a file or template asks whether to add or open as new; opening as new still asks to save first (Simon).
- 11/10/2026: read from Martino's commits: `ARCHITECTURE.md` added (pointed to under "Martino's project"); nodes coloured by add-menu category, the ICE node teal.
- 12/10/2026: Load FBX reads everything a file holds; skin weights by joint name, after KineFX; characters taken in their bind pose; one human solver shared by Body Collide, Reach and Human Limits (Simon).
- 12/10/2026: the Placeholder node, for asking for a node in the graph itself; Write FBX solves Body Collide upstream for each file it writes (Simon).
- 12/10/2026: read from Martino's commits: `VOCABULARY.md` and the vocabulary applied across the program and docs; pointed to under "The same words for the same things", and this file brought into it.
- 12/10/2026: "Like any other program" added: a menu bar at the top left, and questions before losing changes or writing over a file (Simon). FBX only for meshes and characters, the `.xmsclip` and `.xmsmesh` formats removed; the rest pose is the skeleton, so a Transform carries through Retarget (Simon).
- 13/10/2026: from Martino's eframe rework, stage 0: "One shell, being replaced" added; Bevy kept to the shell files. The Gaffer credit removed and the contexts named Object, Point, Face, Corner, as Martino decided (11/10 and 12/10/2026).
- 13/10/2026: eframe rework, stage 0b: the program's state gathered into `AppState`, which Bevy holds as one resource; the template-opens-solved test removed (a known mismatch with the shipped solve, Martino).
- 13/10/2026: eframe rework, stage 1: the interface and the per-frame state work are plain functions; an eframe shell runs them (`XMS_SHELL=eframe`), Bevy still the default until the Viewer is drawn with wgpu.
- 13/10/2026: eframe rework, stage 2a: the Viewer's 3D view on wgpu in the eframe shell (grid, shaded, wireframe, hidden line, boxes, curves and points, instancing, navigation); textures, overlays and picking to follow.
- 13/10/2026: Viewer on wgpu: reversed, infinite depth as Bevy had, so the grid stays behind models; polygon edges drawn as instanced lines at every copy, so Wireframe and Hidden Line work on instancers too.
- 13/10/2026: eframe rework, stage 2b: Textured mode on wgpu (colour and emissive textures with their mip chains, opacity, cut-out), on instanced copies too; F/G/Z/. and A/H framing and the origin label in the eframe Viewer.
- 13/10/2026: eframe rework, stage 3: Edit Poly's picking, box selection, Q/W/E/R manipulators and Delete in the wgpu Viewer, with its wireframe and selection overlay; clip skeletons drawn over everything. The eframe shell now does everything the Bevy one does.
- 13/10/2026: eframe rework done: Bevy removed (its systems, `shell.rs`, the old viewport drawing, the dependencies); eframe is the only shell. `NamedMesh` renamed `Prim`, the `Subnet*` types `IceGraph*`, the ICE node `NodeType::Ice` (graphs saved with `Subnet` still load).
- 13/10/2026: curves in the Primitive Inspector: control points as points, segments as edges, one face per curve (Martino).
- 13/10/2026: a face's or an edge's points are one array attribute, `indices`, in the Primitive Inspector (Martino).
- 13/10/2026: the Corner tab lists polygon corners with `ptIndex`, the point each belongs to (Martino).
- 14/10/2026: egui and eframe 0.36, egui_dock 0.21, wgpu 30. Text as clean at 1x as under Bevy; menus stay open while used (Martino).
- 14/10/2026: the scheme is egui's style for the system's light and dark modes alike (Imago started white otherwise); text unhinted, its weight a setting of the scheme (Martino).
- 14/10/2026: text weight 50 in Light, 80 in Dark and ADHD; one selection blue, also around selected nodes, at least 3 pixels wide at any zoom; in Light, tabs behind darker than the panel (Martino).
- 14/10/2026: in Light, the box dragged to select (nodes, Edit Poly components) in the selection blue (Martino).
- 14/10/2026: shapes are primitives at a Primitive Location, written to USD as `def`s under Xforms with a `defaultPrim`; Merge merges primitives when its first input is primitives (Auto, Primitives, Flatten), the second input stronger on the same path; Scatter Points and Copy to Points removed for ICE (Martino).
- 14/10/2026: the Corner tab gives each corner's `faceIndex` and `localCornerIndex`, the Face tab each polygon's `firstCorner`; corners stay numbered across the mesh, as in USD (Martino).
- 14/10/2026: the points of an edge or a face are `ptIndices` in the Primitive Inspector, not `indices`: what they index is in the name, as in `ptIndex` (Martino).
- 14/10/2026: results kept between evaluations, keyed by parameters and what is wired in; Edit Poly keyed by its operations only; Retarget and Reach on every core (Simon).
- 14/10/2026: the Scene Explorer lists the scene at the view flag as its tree, with no row per operator; the Primitive Inspector shows the row selected there, or the whole scene at the view flag (Martino).
- 14/10/2026: the Output node removed; one operator always has the view flag, which picks the output too; the Operator Stack starts at the viewed operator; graphs with Output open with the view on what was wired into it (Martino).
- 14/10/2026: the diagram of a primitive's parts (`docs/imago_geo_parts_v3`) shown in the README, ARCHITECTURE and VOCABULARY, its labels brought to `ptIndices` (Martino).
- 14/10/2026: Primitive Locations in a Prims Location tab on Transform, Edit Poly and Prune: paths with `*`, `?` and `**`, a count per row, filled from the Scene Explorer by + Add Location or by dragging prims onto the operator (Martino).
- 14/10/2026: several rows selected in the Scene Explorer (Ctrl/Cmd, Shift), dragged onto operators together and shown together in the Primitive Inspector; Primitive Location fields grey, in the code font (Martino).
- 15/10/2026: `XMS_CORE_OPS_SPEC.md` (v0.3) added at the root, pointed to under "Martino's project": the operator set Imago aims for, with Set Attribute (Object and Property rows) the one operator that sets values on primitives. "The graph is the program" in fewer words, and the principles without the names of other programs (Martino).
- 15/10/2026: Set Attribute, on branch `core/usd-scene`: Object and Property rows, the common Properties offered with their values, on the prims its Prims Location tab names (any prim of a stage); values on prims without geometry kept as opinions in the stage tree; Write USD writes them all (Martino).
- 15/10/2026: Set Attribute on the Xforms Imago makes above a shape (`/root/asset/geo`), and what is set on a group inherited below it in Imago as in USD: visibility, active, purpose, Object values (Martino).
- 15/10/2026: a Properties tab in the Primitive Inspector: the selected prims themselves (groups, cameras and lights too), Name, Value and From, From saying whether a value comes from the file, Imago, a Set Attribute operator or a group above (Martino).
- 15/10/2026: Go to Operator on a Properties row: the operator a value comes from (the Load USD of the file, the shape that made the prim, the Set Attribute that set it, on the prim or on the group above) is selected (Martino).
- 15/10/2026: what a file authors beyond geometry is read and listed: `kind`, `instanceable`, single-valued attributes, from openusd through the scene layer to the stage tree. Purpose follows USD: the nearest prim that has one wins, whether the file authors it or Set Attribute set it; only visibility and purpose are inherited among Properties (Martino).
- 15/10/2026: point instancers read as points primitives (one point per instance, `orient`, `scale`, `protoIndex`, `id`); the Primitive Inspector lists instanced geometry once, in its own space, with its copies counted (Martino).
- 15/10/2026: a point instancer's points are the source of its copies: what moves them moves the copies, and Write USD writes its arrays (`positions`, `orientations`, `scales`, `protoIndices`, `ids`). The Scene Explorer lists an instancer once (Martino).
- 15/10/2026: the Scene Explorer shows each prim's type in a column at the right; the Primitive Inspector's scroll bar is dragged (Martino).
- 15/10/2026: the Material operator, on branch `core/usd-scene`: a UsdPreviewSurface with fixed inputs and a texture file per input, named by the operator, drawn on a sphere when viewed; its output a material for Material Binding, which puts it under `/materials` (Martino).
- 15/10/2026: Material Binding: materials on numbered inputs on its left, put under its Material Library Location as Material prims; each Prims Location row binds the input of its number or a chosen one, the later row winning; written as `def Material`s under Scopes and `material:binding` (Martino).
- 15/10/2026: a Material's output on its right, material sockets purple; Material Binding's rows on a Bindings tab, its Material column listing the materials wired in and Location, a material of the scene by its path on a line under the row (Martino).
- 15/10/2026: Merge takes any number of inputs, one always free, as Material Binding; the Material output in the middle of its right side; the "Material bindings" template, from Martino's graph (Martino).
- 16/10/2026: Load USD modes, on branch `core/usd-scene`: Sublayer, Reference and Payload; the file's default prim (or a named one) at a Primitive Location, `/root` by default, its paths rebased; Write USD writes the reference or payload on that prim with the axis and unit transform where needed (Martino).
- 16/10/2026: cameras: a camera is a primitive with no geometry holding a UsdGeomCamera's values; Load USD gives a file's, the Camera operator makes one, Transform moves them, Write USD writes them; the Viewer draws them and looks through one from its 🎥 menu, the whole gate shown; navigating leaves the camera where it was (Martino).
- 16/10/2026: the camera's name larger, at the top of the gate; Filmback Presets a drop-down, with the ALEXA Mini 16:9, Mini Open Gate and Mini LF Open Gate filmbacks (Martino).
- 16/10/2026: the Camera operator has a colour of its own, dark grey, as the ICE node has teal: cameras are not shapes (Martino).
- 16/10/2026: nested point instancers (in another instancer's prototype) drawn through the outer one at each of its copies, down to four deep, following its points; Moana's corals and grass come in whole (Martino).
- 16/10/2026: an instancer's prototypes are listed where the hierarchy has them, also inside native instances, so the Scene Explorer no longer lists part of a stage twice; drawn prims the hierarchy does not have are reported in "Not read in full" (Martino).
- 16/10/2026: a prim of the scene (an instancer whose prototypes are not drawn, a camera) is always listed in its stage's tree, with the groups above it: Moana's island is listed once (Martino).
- 16/10/2026: `core/props.rs` split in two, named for what each is: `core/properties.rs` (values, the properties Imago knows, opinions, inheritance), read by the Viewer, Load and Write USD and the Primitive Inspector, and `core/set_attribute.rs`, the operator (Martino).
- 16/10/2026: Delete and Backspace delete the selected nodes only when no text is being typed and the pointer is over the graph itself: typing a file name in the file browser no longer deletes the Write USD or Write FBX node (Martino).
- 16/10/2026: on branch `core/gui-work`: the Parameters pane's values dark grey with light, almost white text (fields, drop-downs, checkboxes, the buttons beside them), labels and titles as the theme has them (Martino).
- 16/10/2026: on `core/gui-work`: dark fields only where you type (text and number fields, through `ui.field(..)`); buttons, drop-downs and checkboxes back to the theme's look (Martino).
- 16/10/2026: on `core/gui-work`: dark fields are a choice, off by default: "Dark fields" in the colour editor (saved in `theme.json`, kept across presets and Reset), `DARK_FIELD_BG=1` or `0` to override it for a run. Off, the Parameters pane looks as before (Martino).
- 16/10/2026: on `core/materialx`: shading graphs are MaterialX graphs: a MaterialX operator with a graph kind of its own, of MaterialX's nodes read from its libraries (vendored in `crates/xms-matx`, MaterialX 1.39); the Material operator stays as the quick UsdPreviewSurface; compounds come after, at the scene-graph level (Martino).
- 16/10/2026: on `core/materialx`: the MaterialX operator: its graph inside it (saved as its `.mtlx` document), edited as the ICE graph is, connecting picking the nodes' definitions, parameters from the library, written by Write USD as a MaterialX network, shown in the Viewer by its surface shader (Martino).
- 16/10/2026: on `core/materialx`: a node with a graph inside (ICE, MaterialX) sits on a dark grey plate with squarer corners and an arrow, so it reads as one you can go into; the add menu opens with a search line; in a MaterialX graph wires are selected and deleted, maths are a neutral grey, the material dark grey, a node's colour never follows its types (Martino).
- 16/10/2026: on `core/materialx`: the plate under nodes with a graph inside replaced by a light grey band as wide as the selection's, the selection going around it; the arrow kept (Martino).
- 16/10/2026: on `core/materialx`: MaterialX read from anywhere: `.mtlx` files imported (node graphs laid out flat, fileprefix and includes followed, tested on every MaterialX example) and written; a stage's material made into a MaterialX operator with → MaterialX; the UsdPreviewSurface family written as USD has it (Martino).
- 16/10/2026: on `core/materialx`: the add menus' search, in the scene graph and in MaterialX graphs alike: ↓ and ↑ move a highlight through what is found, Enter adds it (Martino).
- 17/10/2026: ICE refactor, stage 1 (Martino, branch `core/ice-refactor`): the engine as a library (`src/ice/engine/`): graph, compiler, batched parallel executor. A graph reads only the attribute columns it uses and writes new columns only for the attributes it sets; values that do not vary per element are computed once; parameters change without compiling again.
- 17/10/2026: the geometry model and the ICE engine are crates: `crates/xms-geo` (`Geo`, depending on nothing) and `crates/xms-ice` (on `xms-geo` only), both fully optimised in development builds. The app reaches the model as `core::geo` and converts it with `GeoMesh`. The engine stays free of the app: no interface or node-editor code goes into it (Martino).
- 17/10/2026: ICE refactor, stage 2 (Martino): the ICE operator runs on the engine. ICE graphs end in an ICE Tree (ordered Execute ports, a context); Get Data and Set Data read and write attributes by name; inputs hold their own values; errors show on the node. The old engine is gone, and old ICE graphs are not converted (Martino: the old ICE is not kept). Scatter and copy to points come back as structural nodes.
- 17/10/2026: on `core/ice-refactor`: the ICE operator takes Primitive Locations; ICE ports and wires coloured by the types the last run found; nodes named after what they are; the ICE graph's add menu with the search and arrow keys of every graph; editing in an ICE graph cooks only when what it computes changes (Martino).
- 17/10/2026: on `core/ice-refactor`: Get Data reads across contexts (its Context: Auto, Point, Face, Corner, Object; Several: Average, Sum, Min, Max), in the engine, with the topology's maps made once per run; Get Data lists the attributes of the geometry coming in. Get Array Data next (Martino).
- 17/10/2026: on `core/ice-refactor`: Get Data reads what the topology gives under the Primitive Inspector's names (a face's `corners`, `firstCorner`, `N`, `area`; a corner's `ptIndex`, `faceIndex`, `localCornerIndex`), made when run; putting a node down that reaches nothing no longer cooks; port types and the Attributes list are worked out by the editor from the last run's geometry, the list only when opened (Martino: placing a Get Data lagged on the DeLorean).
- 17/10/2026: on `core/ice-refactor`: Set Data has a Context too (Auto, Point, Face, Corner, Object) and Several, writing into another context the way Get Data reads across; a name is held in one context; boolean attributes reach the primvars as 0 and 1 (Martino). What the topology gives stays derived, not stored: it is the topology, and a stored copy would go stale when the topology changes; a graph that wants it kept writes it with Set Data.
- 17/10/2026: on `core/ice-refactor`: a placed primitive keeps its primvars where it is drawn (`place_mesh` dropped them), so attributes ICE writes on a placed prim (the DeLorean's) show in the Primitive Inspector; corner values turn with a mirrored polygon (Martino: Set Data `foo` did not show).
- 18/10/2026: Templates moved to the bottom of the File menu, under the usual commands, with its categorised submenu (Simon).
- 18/10/2026: openly licensed example files: four Novasci clips (CC0) and the USD Working Group's teapot and mini car kit in the repository; the other 17 clips, the carbon frame bike and the elephant (CC BY-SA 4.0) fetched on demand by a dialog that names the source, the licence and the size (Simon: "ask if we want to download it").
- 18/10/2026: the templates as the regression suite, with a baseline in `tests/regression/templates.json`, run by a GitHub workflow after every build with a report of what to fix (Simon).
- 18/10/2026: the regression suite skips the templates whose example files are fetched on demand when they are not there (the release build's tests ran without them and failed); the release workflow fetches them first, as the test workflow does.
- 18/10/2026: the top bar laid out so the program's name, the menus and the file's name read apart: what is clickable looks it (Simon: "everything is in the same font and looks clickable").
- 18/10/2026: handles in the Viewer for the selected Transform and Camera nodes, on Edit Poly's manipulator maths; clicking a primitive selects it in the Scene Explorer (Simon: "viewer gizmos for everything that should have one"). The Shapes and lift templates had their rotations written in degrees where radians were read: fixed.
- 18/10/2026: the Viewer draws curves and points as one instanced line mesh per primitive (held once however many copies), and splits any mesh or list of copies into buffers under 128 MB, so a big groom or mesh no longer crashes it. A texture larger than the GPU takes starts from its first smaller copy that fits.
- 18/10/2026: the Primitive Inspector opens on Properties each session and makes the geometry only for a geometry tab (counts from the primitives as they are until then); a stage's primitives keep their attributes in their geometry, the mesh view made lean (`to_mesh_lean`), the rows made when asked; `XMS_TIMINGS=1` times the steps of a load (Martino: Moana takes over a minute to show as boxes).
- 18/10/2026: the Viewer's box of each mesh and each point instancer is measured once and remembered; the remembered boxes are swept for meshes that are gone only when they have doubled (sweeping on every insert made Moana's first frame quadratic, 19 s). `XMS_TIMINGS` splits opening the stage (openusd) from translating it (xms-usd).
- 18/10/2026: a stage's geometry is made after its walk, once per source prim and on every core (Moana: 240,000 meshes), instead of prim by prim as the walk meets them.
- 18/10/2026: a USD stage is read in the background in two passes over one open stage: boxes first (each mesh, curves or points as the box of its `extent`, or of its points when it has none), then the whole stage, which replaces them and cooks everything downstream again. The program stays in use meanwhile; Write USD waits until it is done (Martino: "rare that within the first 2 minutes you need the meshes").
- 18/10/2026: the program waits while a stage's first pass (prims, properties, boxes) is read, as a load always did, with no panel of its own; it is in use again once the boxes show, the geometry following in the background (Martino).
- 18/10/2026: read Martino's commits of 18/10/2026 (USD read in two passes, lean meshes, buffers under 128 MB, boxes remembered); his log lines are his. Node graph: freeze and ghost flags on the title (ghost, not template: a template is a ready-made graph), middle button held on a node for its information, cook time per node; only the left button moves nodes (Simon: the node graph "Houdini-like").
- 18/10/2026: keys on any number of any operator, by its path in the saved parameters; the scene range on the timeline, saved with the graph; the Graph Editor (tangent types, break and unify, weighted handles, infinity, insert, copy, paste, snap); S keys the selected operator; a keyed number edited in Parameters or with a handle is keyed at the playhead (Simon: "animatable parameters... a curve editor copying Maya's").
- 18/10/2026: Local Animation (keys on the operator before it, on local frames from Start to End), time ranges carried through the graph (a Merge shows the widest), Merge Animation (first, last, middle, frame to frame, stretch, shrink, match rate) (Simon: "local animation... inherited through the flow").
- 19/10/2026: one set of data type colours for every graph (`type_colours`): float teal-green (Houdini VOPs'), integer and boolean blue, vector yellow, vector 4 / quaternion / rotation dark yellow, colour orange, matrix light purple (lighter than materials' purple), string desaturated orange-pink (Houdini VOPs'), material and shader purple, closures pink, execute and unknown dark grey (Martino).
- 19/10/2026: float and string colours taken from Houdini VOPs (picked off Martino's screenshot): float 85, 210, 197; string 204, 178, 153 (OpInput's).
- 19/10/2026: the ICE graph's root is ICE Execution Ports (its type), named ICEExectPorts, drawn dark grey as MaterialX's material, where a graph ends; a root named ICE Tree is renamed when read (Martino).
- 19/10/2026: one way to work with wires and selection in every graph (`graph_ports`): click a port with wires, then the empty canvas, to unplug it; box select and Shift+click in ICE and MaterialX graphs, the selection moved and deleted together; a wire selected and deleted in ICE graphs as in MaterialX's; the string colour a little more saturated, nearer Houdini's (Martino).
- 19/10/2026: wires unplugged or moved in one motion in every graph: dragging from an input picks its wire up, its end following the pointer, into another input or, let go of over the canvas, nowhere; Ctrl+drag from an output picks up all its wires, a plain drag from an output still drawing a new one, since an output feeds any number of inputs; a dragged wire let go of over no input is gone (Martino).
- 19/10/2026: in the scene graph, a wire is picked up from an input found by where the press was (the node's title under the socket took the drag and moved the node) (Martino).
- 19/10/2026: Pass Through, an organising operator: one input handed on unchanged; narrower, square-cornered and without an icon, so it reads as layout rather than as a step (Martino).
- 19/10/2026: a node's name is one word: a space typed becomes an underscore, and names made by the menu, templates or older files have theirs made underscores; scene and ICE graphs (Martino: the name is the operator's name in the graph).
- 19/10/2026: UDIM textures: a `<UDIM>`/`<udim>` path reads its tiles, laid out in one image as in UV space, the Viewer scaling each material's UVs to it; OpenEXR, TIFF, TGA and HDR read, float images encoded from linear light; the file browser collapses sequences into one entry and, with UDIM on, UDIM tiles into their `<UDIM>` path (`core::sequences`) (Martino).
- 19/10/2026: README, docs index, UV, USD, ops spec and vocabulary brought up to date with the day's work: UDIM tiles and image formats, the file browser's sequences, wires and selection in every graph, data type colours, one-word node names.
- 20/10/2026: rendering as USD has it, three operators: RenderScene Settings (what is rendered: the RenderSettings prim, named by the layer, and a RenderProduct), Render Settings (a renderer's settings from a JSON definition, Karma and Arnold to start, or Custom with settings named under the renderer's namespace, `rray:spp`; its standard AOVs ticked as RenderVars) and Render Variables (RenderVars row by row). Render Settings finds the RenderSettings prim in the scene rather than through a wire: named, else first, else a file's (an `over`), else made. Filtering and bit depth stay per RenderVar or the renderer's (Martino: "familiarity, people are used to have the renderer settings in render settings").
- 20/10/2026: Render Settings' Renderer is a drop-down; a Custom renderer's settings are written only once it has a name, the rows kept as the name is typed; Arnold's clamp Max Value starts at 10 (Martino).
- 20/10/2026: Render Command runs a command line on the system (husk on the layer the Write USD above writes, `$USD`): only when Run is pressed, never on cook, since cooking happens all the time; in the background with Stop ending everything it started, its output and progress in its panel. Named for rendering; a general Shell Command can come later (Martino). The image streamed into the Viewer comes later.
- 20/10/2026: Render Command's output in a Console tab beside its Parameters, black as a terminal, opened by Run; a Console pane of its own can come later (Martino). Every RenderVar carries `driver:parameters:aov:name`, its name in the image: husk reports a RenderVar without one (Martino's husk run).
- 20/10/2026: `driver:parameters:aov:name` is written as a `string`, as Houdini writes it: husk did not find it as a `token` (Martino's husk run).
- 20/10/2026: Render Command makes the folders the products write into before running (Make Output Folders, on): husk rendered but could not write into a `render/` that did not exist (Martino).
