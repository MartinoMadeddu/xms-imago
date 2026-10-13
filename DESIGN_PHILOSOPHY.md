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
- `ARCHITECTURE.md` (Martino, 11/10/2026) records the decisions that shape the data and its evaluation: the geometry model, copy on demand, primitives, edits and USD out, the Viewer, the ICE engine to come, and where it is all going. Read it before changing any of those. It is to be merged with this file once Martino and Simon have talked.

## The graph is the program

- Everything is a node. A graph describes a result; changing a number upstream changes everything below it.
- One node per idea. A node that moves things moves anything: Transform takes a mesh, primitives or a clip. What a node shows in the Viewer is a setting on that node, not another node (Body Collide's Skin, Pieces and Hulls display, which replaced the separate Calamari node).
- Opening a file or a template asks every time whether to add it to the scene or open it as new. Added nodes land beside what is there. (One question replaced the + buttons first tried, the same day.)
- Copied nodes are text, kept after the program closes: they paste in another window, in a later session, or from a message.
- Every area of the program has templates: working graphs that load laid out and framed, with a line saying what to look at. Templates are grouped by whose work they show.
- A feature comes with its template and the demo files it needs, packaged in `examples/`. A model too big for the repository (the Moana Island scene, say) is not packaged: its template asks whether to download it from where it is published, and says where from.
- A node that does not exist yet is sketched in the graph: a Placeholder with named inputs and outputs and a description of what it should do, data going through unchanged. The saved graph is how a new node is asked for.
- An ICE node runs its ICE graph on each primitive coming in, in the primitive's own space, as an ICE tree runs on an object in Softimage.
- Where a known tool already has a convention, follow it: ICE graphs drawn and navigated after Softimage ICE (loosely modelled after Softimage ICE and Houdini VOPs), edits between two nodes as stacked layers as in Solaris. The Viewer drawing what the Scene Explorer opens is Imago's own, not credited to another program (Martino).

## One geometry model

- Packed primitives, the nodes and ICE share one geometry: `Geo`, a topology and named attribute columns per context (Object, Point, Face, Corner), matching USD's interpolations.
- Names follow USD (`points`, `normals`, `st`, `displayColor`, `widths`), so reading and writing USD is one to one. Interfaces can show friendlier names on top.
- Replace only the columns you change. Columns are shared and copied only when written; never rebuild a whole `Geo` to change one thing.
- Keep primitives placed. Geometry stays in its own space; a primitive moves by its placement, not its points.
- Keep the primitive's identity: path, source, stage, material and purpose travel with it through every node.

## Nothing typed that can be picked

- Joint names, prim paths and file paths are picked from lists, browsed or matched by pattern, with a live count of what matches. Typing is the fallback, not the way.
- Ask the program for what it already knows: a skeleton's parts are read from its names, the user only sets what could not be read. A prefix every bone shares (`Skeleton 001_`) says nothing and is left out.

## The same words for the same things

- `VOCABULARY.md` (Martino, 12/10/2026) is the list of words: primitive (an object in the stage, "packed" dropped), scene graph (the network of operators; the Scene Explorer shows the stage's hierarchy), ICE graph, operator, parameters, attributes, the contexts Object, Point, Face, Corner, shapes for cube, sphere and grid, and the panes Viewer and Parameters. Interface, docs and code use those words; old names in saved graphs and layouts still load.
- Translate, Rotate, Scale for every transform. Path for every file. In and Out for what passes through.
- The parameters of every node share one layout: titled groups, labels right-aligned in one column, values filling the width. Explanations go in tooltips; the panel shows parameters and the state of the node: what came in, what goes out, what went wrong.
- The light theme is the default. The Viewer's background follows the colour scheme.
- Nodes are coloured by their add-menu category; the ICE node has its own teal, after Softimage's ICETree.
- Imago keeps its own way of working. Softimage XSI and other programs are references for conventions, not a look or a layout to copy.
- How it looks is a theme, chosen by the user. A button style setting has been proposed but not built: from soft, rounded XSI buttons to the square IRIX Motif look. Colour schemes can be worked out with a palette tool such as [Adobe Color](https://color.adobe.com/create/color-wheel).

## Like any other program

- The program works with files as other programs do. File, Edit, Templates and Window menus at the top left, with the usual shortcuts (New, Open, Save, Save As, Quit). A node graph pane holds what belongs to the graph only.
- Nothing is lost without asking: closing, New, or opening a file or template as new over unsaved changes asks to save first.
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
- Skin weights live on the geometry and go by joint name (after Houdini's KineFX): the same weights fit any skeleton with those names, and the rest pose is a skeleton of its own.
- The rest pose is the skeleton. Moving a clip moves its animation, never its rest pose, so the move carries through every node after it (Retarget, Reach).
- Meshes, characters and takes come in as FBX (or USD). The program has no mesh or clip format of its own: one way in is clearer than two. Example data too big for GitHub is cut or written lighter, still as FBX.
- A USD file is read composed, as usdview shows it. Nothing is baked into the points: up axis and unit become a correction on the placement.
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

## One shell

- The program runs in an eframe window: the interface in egui, the Viewer's 3D view in wgpu (`src/viewer/`). Bevy is gone (Martino, branch `core/uiframework`). The program's state is one struct, `AppState` (`app.rs`), owned by the app: new state goes there. Code outside `eframe_app.rs` and `viewer/` does not depend on the window or the GPU.

## Say what is true

- Documentation states what the program does, measured where it can be. Every page has a "Not done yet" list.
- Each feature comes with tests; each change says what was tested and what was not.
- Credit: XMS was created by Martino Madeddu. Additional development by Simon Legrand.

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
