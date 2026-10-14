# Interface

![Default layout: the top bar, and the panes under it](xms_layout.png)

## Top bar

A fixed line across the top of the window, for things that belong to the whole program. It holds the logo and the name, the menus, the name of the graph's file, and the layout padlock at the right. It is not a pane: it cannot be moved, floated or closed.

![The File menu](xms_menu_bar.png)

The menus, from the left, as in other programs:

| Menu | Holds |
|---|---|
| File | New (Ctrl+N), Open... (Ctrl+O), Open Recent, Save (Ctrl+S), Save As... (Ctrl+Shift+S), Import (Graph..., FBX..., USD...), Quit (Ctrl+Q). See [Files](#files) |
| Edit | Undo, Redo, Copy, Paste, Delete, Select All, for the operators of the root scene graph |
| Templates | The ready-made graphs, by author, then by area. See [Templates](templates.md) |
| Window | The panes (tick to show), Dock floating panes, Reset layout, Save layout..., Load layout..., Theme |

On macOS the shortcuts take Cmd in place of Ctrl.

## Panes

Nine panes: Viewer, Node Graph, UV Editor, Scene Explorer, Operator Stack, History, Parameters, Primitive Inspector, Timeline. Each one is a tab. The default layout has the Scene Explorer in a narrow column at the left, the Viewer beside it with the UV Editor as a tab behind it (see [UV](uv.md)), and the Primitive Inspector under both; then the Node Graph, then Parameters over the Operator Stack. The Timeline runs along the bottom of those, and History (see [Undo](undo.md)) down the whole right side. A layout saved before the History pane existed does not have it: tick it in the Window menu, or "Reset layout" for this default.

| To | Do this |
|---|---|
| Move a pane | Drag its tab by the title. Drop it on one of the five squares that appear over the pane under the cursor: the middle one stacks it as a tab, the others split that pane on that side |
| Float a pane | Drag its tab and drop it anywhere that is not one of the squares, or double-click the tab |
| Dock along a whole side of the window | Drag the tab to within a few pixels of that side. Blue bars mark the four sides while a tab is dragged, and a blue strip shows where the pane will land. This is how the timeline goes back across the bottom |
| Dock a floating pane | Drag its tab (the title inside the window, not the bar above it) onto one of the squares, or double-click the tab, or use "Dock floating panes" in the Window menu |
| Move a floating window | Drag the bar at its top |
| Close a pane | The cross on its tab, docked or floating |
| Show a closed pane | Tick it in the Window menu. It comes back as a tab of the main area |
| Resize | Drag the line between two panes, or the corner of a floating window |
| Start over | "Reset layout" in the Window menu |
| Keep a layout | "Save layout..." in the Window menu writes it to a file. A file that exists is replaced only after Replace |
| Bring one back | "Load layout..." in the Window menu. A file that is not a layout is refused and the current layout stays |

Right-clicking a tab offers the same close and float actions.

![A tab dragged to the bottom edge: the strip shows where it will dock](xms_edge_dock.png)

### Lock

The padlock at the right of the top bar locks the layout. While it is locked, tabs cannot be moved, floated or closed, and the layout items of the Window menu are greyed out. The lines between panes still resize. Click the padlock again to unlock.

### Saved layout

The layout, the lock and the place of every floating window are saved when they change and restored at the next start. They are kept in `layout.json` in the config folder (`~/.config/xms` on Linux, `%APPDATA%\xms` on Windows). A file that cannot be read is ignored and the default layout is used.

When the Viewer pane is closed or hidden behind another tab, the 3D view is not drawn.

## Files

The graph is saved as a JSON file. The top bar shows its name, and "(unsaved)" while it differs from the file.

| To | Do this |
|---|---|
| Start an empty graph | File > New (Ctrl+N). Asks to save unsaved changes first |
| Open a graph | File > Open... (Ctrl+O) |
| Open one used lately | File > Open Recent: the last ten graphs opened or saved. Files no longer there are greyed out. "Clear the list" empties it |
| Save | File > Save or Ctrl+S (Cmd+S on macOS): to the current file, or asks where when there is none yet |
| Save to a new file | File > Save As... (Ctrl+Shift+S) |
| Add a saved graph to this one | File > Import > Graph... |
| Add a node reading a file | File > Import > FBX... or USD...: a Load FBX or Load USD node with the file set, right of the nodes there, selected |
| Close | File > Quit (Ctrl+Q), or the window's close button. Asks to save unsaved changes first |

### Replacing files

As in other programs, nothing is written over a file that exists without asking:

- **Save As, Save layout:** choosing a name that is taken shows "*name* already exists. Replace it?" in the browser. Replace writes over it; Cancel goes back to choosing. Save (Ctrl+S) writes to the graph's own file without asking, as elsewhere.

  ![Save As over a file that exists](xms_replace.png)

- **Write FBX, Write USD:** when what the node would write is there already, a question lists it: Replace or Cancel (Esc). For one file, the files themselves; for All files of a folder, the folders that already hold FBX files and how many, since the names come from files not read yet.

Opening a graph is a step in the [history](undo.md): undo goes back to the graph before it.

Adding a graph puts its nodes to the right of the ones there, selected, as one step in the history. The graph open stays the current file. If its Output is not wired, it takes what the added graph's Output was wired to; if nothing is viewed, the added graph's viewed node is. ICE graphs are not saved in graph files yet, so an ICE node added from a file comes with an empty tree (from a template, or copied and pasted, it keeps its tree).

Opening a graph (Open or Open Recent) or picking a template asks first whether to **Add** it to the graph, beside the nodes already there, or **Open as new** in place of the graph. On a graph with nothing but Output it opens straight away.

![Picking a template over a graph with nodes](xms_add_or_open.png)

Anything that replaces the whole graph asks first when it has unsaved changes: closing the window, and opening a graph or a template as new. The choice is Save, Don't save, or Cancel (Esc). Save writes to the current file, or opens the browser for a graph never saved; the file or template opens, or the program closes, once it is written. Cancel keeps the graph as it is. With nothing unsaved it goes ahead straight away. Adding replaces nothing, so it never asks about saving. File > Import > Graph... adds without asking.

![Loading a template over unsaved changes](xms_unsaved_template.png)

![Closing with unsaved changes](xms_unsaved.png)

The list of recent files is kept in `recent.json` in the config folder.

## Node graph

| To | Do this |
|---|---|
| Add a node | Right-click or press Tab on the canvas. The node is centred on the place you clicked |
| Add a node after another | Right-click an output socket. The node you pick goes under that node, wired to the socket. If the place is taken it steps to the right |
| Draw a wire | Drag from an output socket with the left button |
| Remove a wire | Right-click it |
| See every node | "Frame" in the header, or F or A over the graph |
| Tidy the graph | "Tidy" in the header lays the nodes out in rows, top to bottom, Output last, each node above the first node that uses it |
| Copy nodes | Ctrl+C (Cmd+C on macOS) over the graph: the selected nodes, their parameters, the wires between them and the trees of their ICE nodes. Output is not copied |
| Paste nodes | Ctrl+V (Cmd+V) over the graph: new nodes, under the pointer, selected. Each ICE node gets a tree of its own |

Copied nodes go to the system clipboard as text, and to `clipboard.json` in the config folder. They paste in another window of the program, after it was closed and started again, or on another computer from text sent over. When the clipboard holds something else, Ctrl+V pastes the nodes copied last.

![The ICE spherify template, then the same template added to it: the added nodes sit beside the first ones, and Output stays with the first](xms_copy_paste_add.png)

A new node is never left out of sight: the view moves just enough to show it. Templates are laid out with Tidy and framed when they load.

### Placeholder: a node that does not exist yet

When a graph needs a node the program does not have, put a **Placeholder** in its place: give it a name, as many inputs and outputs as the node would have, each named, and describe what it should do in **Description of node functionality for LLM**. Input 1 goes to output 1 unchanged, input 2 to output 2, and so on, so the rest of the graph keeps working. Save the graph and hand the file over: the name, the sockets and the description are in it, and an AI assistant reading it builds the node from them (see `LLM_instructions.md`).

### The add menu

Seven categories, each a submenu, each item a node. A node has one name, the same in the menu, on the node and at the top of its parameters. Hover an item for a line on what it does.

| Category | Nodes |
|---|---|
| Placeholder | Placeholder |
| Create | Cube, Sphere, Grid, Test Clip |
| File | Load USD, Write USD, Load FBX, Load FBX Folder, Write FBX |
| Modify | Transform, Edit Poly, Merge, ICE |
| Primitives | Pick Primitives, Prune Primitives, Unpack |
| UV | UV Unwrap, UV Transform, UV Edit |
| Animation | Rename Joints, Trim, Retime, Set Timecode, Time Warp, Blend, Loop, Mirror, Smooth, In Place, Floor, Prune Joints |
| Mocap | Split Characters, Characterize, Retarget, Reach, Human Limits, Capture, Skin, Auto T-Pose, Fix Pose, Proxy Skin, Body Collide |

Transform moves anything: a mesh, primitives (the picked ones, or each of them), or a clip. A clip moves by its top joints, so a skinned mesh follows its skeleton once and is not moved a second time.

Cube, Sphere and Grid each make a primitive at their **Primitive Location**, `/cube`, `/sphere` and `/grid` unless another path is typed (see [USD](usd.md#shapes-are-primitives)).

Merge joins its two inputs by its **Mode**. Auto goes by the first input: primitives are merged as primitives, each keeping its path, and on a path both inputs have the second is the stronger; a plain mesh or a clip is flattened into one mesh. Primitives and Flatten force one or the other.

Scatter Points and Copy to Points are done in an ICE graph. A graph saved with them opens with a Placeholder in their place, wires kept.

![The add menu, Animation open](xms_add_menu.png)

### ICE graphs

An ICE node holds a graph of its own. Double-click it, or select it and press ↓, to open it; ↑ or the path at the top goes back up to the scene graph. Inside, the graph is drawn after Softimage ICE: nodes coloured by kind, each with its name over its type, a row per port, ports and wires coloured by the type of data they carry.

| | |
|---|---|
| Add | Right-click the canvas |
| Wire | Drag from an output to an input |
| Delete | Delete or Backspace on the selected node, or right-click its title. The graph's input and output stay |
| Collapse | The box at the right of a node's title |
| Pan | Shift or middle mouse button, and drag |
| Zoom | Mouse wheel, towards the pointer |
| Frame | F or A |

A graph runs on each primitive coming in, in the primitive's own space, as an ICE tree runs on an object in Softimage.

## Parameters

Every node's parameters are laid out the same way. The first group is the node's type and its name. Then its parameters, in titled groups. In every group a row is a label, right-aligned in a column of one width, and its value, which fills the rest: sliders, fields and lists line up down the pane. Clip nodes end with a Clip group: what comes In and what goes Out.

Explanations are in tooltips, on a group's title or a row's label, so the pane holds the parameters and the state of the node and little else.

The same words mean the same thing everywhere: Translate, Rotate and Scale on every transform (Transform, UV Transform, UV Edit, Fix Pose, Edit Poly's Transform); Path for every file; In and Out for what passes through. Units are in the value: m, cm, °, cm/s.

![A template after loading: laid out in rows and framed](xms_graph_layout.png)

## Node buttons

Each node in the graph has two buttons in its title.

| Button | Where | Does |
|---|---|---|
| Bypass | Left, a ring | Switches the node off. Its first input passes through unchanged. The ring turns orange with a bar through it and the node is greyed. A bypassed node with nothing wired to it gives nothing. Output cannot be bypassed |
| View | Right | Shows that node in the Viewer |

Bypass is saved with the graph.

![Smooth is bypassed. Prune Joints shows its picker](xms_bypass_pick.png)

## Name patterns

Fields that choose things by name take patterns instead of exact names:

- Comma-separated regular expressions. Case is ignored
- A pattern matches anywhere in the name, so a plain word means "contains this word". Use `^` and `$` to anchor
- Text that is not a valid expression is matched as plain text, and the field says so
- "Pick" lists the names coming into the node, with a filter box. Clicking a name adds or removes it. Names the pattern already matches are highlighted. "Use the filter as a pattern" adds what you typed in the filter
- Under the field: how many names match

Used by Prune Joints, Fix Pose and the Find field of Rename Joints.

## Framing

With the cursor over the Viewer:

| Key | Frames |
|---|---|
| F, G, Z, . | The selected node. With an Edit Poly node selected and vertices, edges or polygons selected in it, those components |
| A, H | Everything shown |

## Theme

The Theme menu in the top bar has three schemes. The choice is kept.

| Scheme | Look |
|---|---|
| Light | The original grey theme. The default |
| Dark | Neutral dark greys with strong text contrast |
| ADHD | Dark blue surfaces. Orange marks whatever is selected or active, and nothing else |

![The ADHD scheme, with the colour editor](xms_theme_adhd.png)

![The dark scheme](xms_theme_dark.png)

### Colour editor

"Edit colours..." in the Theme menu opens it. A scheme is six colours:

| Colour | Used for |
|---|---|
| Darkest surface | Canvases, text fields |
| Lightest surface | Nodes, buttons. Every other surface sits between the two |
| Text | Names, values |
| Dim text | Labels, hints |
| Accent | Selection, active flags, highlights |
| Outline | Node borders, window edges |
| Text weight | How heavy all text is drawn |

Changes show at once and are saved as you edit, in `theme.json` in the config folder. "Start from" loads a preset, "Reset" undoes the edits to the current one. Colours that carry meaning (axes, wires, warnings) are the same in every scheme. The Viewer's background follows the scheme: mid grey in Light, dark grey in Dark and ADHD.

### Text

Text is drawn to look as it did before the move to eframe, on screens of ordinary density (Full HD and below) as well as on high-density ones: glyphs are not hinted (hinting makes small text blocky at 1x) and sit on whole pixels, and text sizes are those of before (body text 12.5, code 12). How heavy text looks is the scheme's **Text weight**, a slider in the colour editor (0 thinnest, 100 heaviest; 50 in Light, 80 in Dark and ADHD, set by eye against the earlier shell on a Full HD screen). It is one setting for all text of a scheme, dark text on light surfaces and light text on dark ones alike: node titles are dark on light in every scheme.

### Selection and tabs

What is selected shows in one blue everywhere: rows in the Scene Explorer and the Operator Stack, the ring around a selected node in the Node Graph and in ICE graphs, and in Light the box dragged to select nodes or Edit Poly components. The ring follows the zoom but stays between 3 and 6 pixels wide, so a selected node can be told apart however far the graph is zoomed out. In Light, the tabs behind sit darker than the panel and the tab in front is the lightest.

### Menus

A menu stays open while it is used: ticking panes in Window, Invert zoom in Navigation, typing in the filter of a Pick list. Choosing a command closes it, and so does a click outside.

## Scene Explorer

The scene explorer lists what each node in view gives: a mesh, the joints of a clip, or a USD stage as its hierarchy. For a stage, opening and closing prims also decides what the Viewer draws as geometry and what as boxes, and the guide, proxy and render toggles in the header choose which purposes are drawn. See [USD](usd.md#the-scene-explorer-and-the-Viewer). Only the rows in view are laid out, so a stage of tens of thousands of prims scrolls as easily as a cube.

## Operator Stack

One row per node, from Output down to the sources. A chain of single inputs stays in one column however long it is. Only a node with several inputs, such as a Merge, steps its branches in. The triangle at the left of a row folds what is under it: it shows at branch points, on folded rows, and under the cursor. The type of the node is at the right when there is room. A bypassed node is struck through.

## Splash and icon

At start a splash shows for three seconds: a borderless window of its own, always on top, in the middle of the primary screen. The main window is hidden until the splash closes, then opens maximised on the primary screen. A click or a key in the splash closes it early. Setting the environment variable `XMS_NO_SPLASH` skips it.

![The splash on an otherwise empty screen](xms_splash.png)

The icon is the wireframe cube of the splash. It is the logo in the top bar and the icon of the window, shown in the task bar on X11 and Windows. On Wayland and macOS the desktop takes the icon from an installed application entry or bundle, which the program does not ship. The icon of the file itself in Windows Explorer is not set. Both images are in `assets/`.

## Primitive Inspector

The attributes of the selected node, or of the primitive selected in the Scene Explorer, as a spreadsheet.

| Tab | Rows |
|---|---|
| Point | Points: polygon vertices, curve control points and free points. Position `P`, normal `N` for polygon vertices, `widths` for free points |
| Edge | Polygon edges, then curve segments (between control points that follow each other, and the last back to the first on a periodic curve): `ptIndices`, its two points as one array, and the length |
| Face | Polygons, then curves, one row each: the number of corners (a curve's control points), `ptIndices`, every point of the face as one array, and the normal and area of a polygon or the length of a curve. A polygon's `firstCorner` is the Corner row its corners start at. A curve's root point is the first number of its `ptIndices` |
| Corner | Polygon corners, polygon after polygon (24 on a cube, not the 36 corners of the triangles it is drawn with): `ptIndex`, the point each corner belongs to (several corners share a point), `faceIndex`, the face it is a corner of, `localCornerIndex`, its place in that face (0 for the first, as Houdini's vertex `0:0, 0:1…`), and the UV when the mesh has them. The row number is the corner's own number, counted across the whole mesh as USD counts faceVarying values. Curves have none |
| Object | One row per primitive, with its Object attributes |
| Joint | Joints of a clip: name, parent, position and rotation at the current frame |
| Bone | Bones of a clip: from one joint to another, with length |

Nodes that give geometry show the first five tabs, clip nodes the last two.

Each primitive is listed where it is drawn, one after the other, and points are numbered across all of them, so the point numbers in the Edge and Face tabs are rows of the Point tab. An edge's or a face's points are one value, an array as long as it has corners, shown in one cell as `[0, 3, 2, 1]`; the column is as wide as its widest cell, up to a limit past which longer arrays are cut off at the cell's edge. A column one primitive does not have (a normal on a curve point, the area of a curve) shows `-` on its rows. Curves are listed by their control points as authored, whatever their basis, not by the lines they are drawn as. With the Strands template, Point lists 3430 points (3030 control points, then the spiral's 400), Edge 2427 segments, Face 604 curves.

The mesh is cooked and the table built only when the graph changes or another node is selected. Moving the camera, or anything else in the Viewer that leaves the graph alone, does not refresh it. Each frame draws only the rows in view.

## When the graph is cooked

The Viewer meshes, the scene explorer, the operator stack and the primitive inspector are rebuilt when the content of the graph changes: a node added or removed, a parameter edited, a wire changed, the view flag or the selection moved. Moving nodes about, panning the graph and moving the camera do not cook anything. Animated meshes are also rebuilt when the playhead moves.

Each operator's result is kept, as in Houdini, until the operator or something above it changes. It is found by a key made of the operator's parameters and the keys of what is wired into it, so:

- Selecting a node, redrawing the window, playing or scrubbing the timeline cooks nothing again: a skinned character is posed at the playhead, its clip is not made again.
- Changing a parameter cooks that operator and what is below it; what is above it, and other branches, are not cooked again. Changing it back finds the earlier result.
- Picking polygons in Edit Poly does not cook Edit Poly again: only its operations make its result. The mesh it works on is built once and kept while its input stays the same.
- A file read is read again when it is written again (its date and size are part of the key). Body Collide is asked again when a solve finishes or is cleared.
- ICE operators, and what comes after them, are cooked each time asked, as before.

Results are kept up to about 3 GB, the least recently used going first. Copies of the graph (undo steps, Write FBX over a folder) share them.

Measured on the example take (728 frames, 342 joints) and the car (1.7 million triangles), on 2 cores: a Body Collide node with the car upstream took 2.3 to 6.6 seconds every time it was asked for (its panel, the timeline and the Viewer each ask on every frame); it now takes 1 to 2 ms after its first cook. Retarget takes 25 to 45 ms for the take, Reach 250 ms; both work on every core, so on a machine with more cores they take less. Edit Poly on the car takes 2.8 seconds when an operation changes, once.

While an ICE graph is open, every click and key press counts as a change.

The window draws a new frame when something happens (input, a change in the graph, playback, a texture arriving), not all the time, so an idle window costs almost nothing and a heavy Viewer does not slow the panes down while nothing moves.

## Not done yet

- The Primitive Inspector shows curves' positions only: their widths and other attributes are not listed yet. Nor are the attributes of a primitive placed from a stage, apart from positions, normals, UVs and points' widths.
- The Edit menu acts on the root scene graph, not inside an ICE graph.
- Paste in the Edit menu pastes the nodes copied last; Ctrl+V over the canvas also takes nodes from the system clipboard, under the pointer.
- No question before Write FBX or Write USD overwrites a file in a batch run started some other way than the node's buttons (the tests, say).
