# Imago vocabulary

One word for one thing. **Primitives** are objects in the stage. The **scene
graph** is the network of **operators**. Geometry carries **attributes**. Nodes
have **parameters**.

## Terms

| Concept | Old name | New name | Why |
| --- | --- | --- | --- |
| An object in the USD stage, and Imago's piece for it | prim / packed primitive | **primitive** (prim for short) | One word for the stage object and the piece that carries it. "Packed" was an implementation detail. |
| The top-level graph | node graph / scene network | **scene graph** (the **root scene graph**) | Names the graph that builds the scene. In USD, "scene graph" often means the prim hierarchy, which in Imago is the Scene Explorer: the docs say so once. |
| The operator holding an ICE graph | ICE | **ICreationEngine** (ICE for short) | Its name in the add menu, on the node and in its parameters, after Softimage's ICE, the Interactive Creative Environment. |
| The graph inside an ICE node | subnet / ICE tree | **ICE graph** | Matches "scene graph". "Subnet" was a code term. |
| A node of the scene graph | node | **operator** | Each takes the scene coming in and changes it, hence the Operator Stack. "Node" stays the generic word for any graph element. |
| A step inside Edit Poly | operation | **operation** (unchanged) | Distinct from operator: an operator holds operations. |
| A node's settings | properties | **parameters** | Not to be confused with USD properties (a prim's attributes and relationships). |
| Text and number fields drawn dark grey with light text | dark field background | **Dark fields** (`DARK_FIELD_BG`) | A switch in the colour editor, not a colour of the scheme: on in the Light preset, off in Dark and ADHD; Reset keeps it. The variable keeps the name it was asked for under. |
| Per-element data on geometry | attributes / primvars | **attributes** | One word everywhere: UI, docs, code, ICE. Attributes are written to USD as primvars, with their interpolation. |
| Where attribute values live (UI, ICE and code) | Constant, Vertex, Polygon, FaceVarying (UI) · Object, Point, Primitive, Corner (code) | **Object, Point, Face, Corner** | One set of names everywhere. "Primitive" now means a stage object, and one value per primitive is the Object context, so the per-polygon context becomes Face. Face also covers curves (one value per curve). |
| Cube, sphere, grid | primitives | **shapes** | Frees "primitive" for stage objects. |
| Where an operator puts the primitive it makes | path / prim path | **Primitive Location** (a parameter) | As in Solaris: a USD path such as `/cube` or `/set/props/box1`. The parents on it are Xforms. |
| The primitives an operator works on | Pick Primitives upstream | **Primitive Locations**, in the operator's **Prims Location** tab | As Solaris' Primitives parameter on a LOP: each operator says what it works on. Paths with `*`, `?` and `**`; a path takes everything below it; empty means everything. |
| What a Set Attribute row's value is in USD | Set Primvar, Set Metadata, Set Active, Set Kind… (separate operators) | **Object** (a constant primvar) or **Property** (the prim's own attribute or metadata) | One operator sets every value on a prim; the context says how it is written. Object is the context of the same name, one value per primitive; per point, face and corner values are ICE's. |
| How a Merge joins its inputs | (none: always one mesh) | **Auto**, **Primitives**, **Flatten** (Merge's Mode) | Auto goes by the first input. "Flatten" is what Unpack does: everything as one plain mesh. |
| What the Viewer and the Scene Explorer show | Output node | **view flag** | One operator always has it. |
| An operator kept at its result while what is above it changes | lock flag | **freeze** (a **frozen** operator) | Says what it does to the cooking. Bypass, freeze, ghost and view are the operator's **flags**. |
| A number of an operator's parameters set by frame | animated parameter, attribute curve | **keyed** number; its curve is a **channel**, its points **keys** | A channel is named by its place in the parameters (`translation/0`). The pane that shapes channels is the **Curve Editor**, not "Graph Editor": a graph is the scene graph or an ICE graph. |
| An operator's parameters keyed on frames of its own | local animation, animation clip | **Local Animation** (the operator), its **local frames** from its **Start** to its **End** | Keys the operator wired into it; that operator is not changed. |
| The frames an operator's output spans | frame range | its **time range** | Local Animations, clips and keys in and above it, put together: a Merge's is the widest. |
| Inputs lined up in time | time align | **Merge Animation**, its **Align**: Frame to frame, First frame, Last frame, Middle, Stretch to the longest, Shrink to the shortest | Against input A for first, last and middle. |
| The frames of a graph that is not a clip | playback range, time slider range | the **scene range** (first frame, last frame, rate) | Saved with the graph; keys are set on its frames. |
| An operator's output drawn grey beside the scene | template flag | **ghost** (a **ghosted** operator) | "Template" is a ready-made graph in the File menu: one word for one thing. |
| An operator making a material | Create Material | **Material** (the operator), its output a **material** | It puts out a material, not a scene; named by the operator. The quick form: one UsdPreviewSurface. |
| An operator making a material from a graph of shading nodes | material compound, shader network | **MaterialX** (the operator), its graph a **MaterialX graph** | Its nodes are MaterialX's, by MaterialX's names (`add`, `image`, `standard_surface`). A node's **definition** is MaterialX's `nodedef` (`ND_add_color3`): one node has one per set of types, chosen by what is connected. |
| The operator binding materials | Material Assign, Mat Bindings | **Material Binding** | USD's word (`material:binding`). Its rows are Primitive Locations, each with a Material column, on its **Bindings** tab. A row bound by **Location** names a material of the scene by its path. |
| What the Viewer shows when it looks through a camera | camera view | **looking through** a camera (the Viewer's 🎥 menu); **Perspective** is the Viewer's own camera | As usdview, Maya, Houdini. A camera's **filmback** (its gate: `horizontalAperture`, `verticalAperture`), picked from **Filmback Presets**. |
| How Load USD brings its file in | (always sublayered) | **Sublayer**, **Reference**, **Payload** (Load USD's Mode) | USD's own words. Reference and Payload put the file's default prim at a Primitive Location. |
| Where Material Binding puts the materials plugged into it | material library | **Material Library Location** (a parameter, `/materials` by default) | A Primitive Location for materials: each goes under it by its operator's name. |
| How a Get Data reading another context makes the values an element touches one | promote, aggregate | **Several**: Average, Sum, Min, Max (Get Data's and Set Data's parameter) | Plain words: a face touches several points. Arrays (Get Array Data) will keep them as a list. |
| What ICE reads from the topology without it being stored | intrinsic, built-in attribute | the Primitive Inspector's names: `corners`, `firstCorner`, `N`, `area` (Face); `ptIndex`, `faceIndex`, `localCornerIndex` (Corner) | One name for a column wherever it is shown or read. |
| The colour a port and its wires get from what they carry | wire colour, socket colour | **data type colour** (`type_colours::DataType`) | One table for every graph: see docs/interface.md, Data type colours. |
| The operator making a light | Dome Light, Area Light, Point Light (separate nodes) | **Light**, with a **Type**: Dome, Rect, Point, Sphere, Disk, Cylinder, Distant | UsdLux's types by their schema names; the add menu lists one entry per type, all the one operator. A **point light** is a sphere light treated as a point, as USD has it. |
| A node that hands on what comes in, unchanged, to lay a graph out | Null (Houdini), Dot (Nuke), Reroute | **Pass Through** | Says what it does. |
| The operator making the RenderSettings prim and its product | Render Settings (Solaris), Scene Options | **Render Commons** (was RenderScene Settings) | What is rendered, whatever renders it: camera, resolution, overscan, motion blur, purposes, the output image. |
| The operator setting a renderer's own settings | Karma Render Properties, Arnold Render Settings, renderer options | **Render Settings** | The name artists know for a renderer's settings. Its **Renderer** is Karma, Arnold, one from a **renderer definition** (a JSON file), or **Custom**, whose settings you name under its **namespace** (`rray:spp`). |
| One AOV a renderer writes | AOV, render pass, output variable | **RenderVar** (USD's word), made by **Render Variables**; Render Settings' **AOVs** tab ticks a renderer's standard ones | USD's `UsdRenderVar`. "AOV" stays in the UI for the standard ones, the word artists use. |
| The image file a render makes | output, render product, image plane | **RenderProduct** (USD's word); its file is its **Output Image** (`productName`) | |
| The operator running a render command on the system | Shell (Houdini ROP), SystemCommand (Gaffer), Terminal | **Render Command** | Made for rendering the layer (`$USD`); a general Shell or System Command can follow. It runs when **Run** is pressed, never on cook. |
| A texture of one file per UV tile | UDIM set, tiled texture | **UDIM tiles**, named by a **`<UDIM>` path** (`body.<UDIM>.exr`; `<udim>` read too) | The tag USD, Mari and Substance write. |
| Files numbered alike, listed as one | image sequence, file sequence | **sequence** (`shot.####.exr`) | One word, shown in the file browser with Collapse sequences. |
| An ICE graph's end node | SubnetInput / SubnetOutput, GraphInput / GraphOutput, ICE Tree | **Execution Ports** (the node named ExecutionPorts) | Softimage's ICETree: one root with ordered Execute ports. Renamed so it says what it is. The geometry is not wired in: Get Data reads its attributes, Set Data writes them. |
| A place on a mesh's surface in an ICE graph (a face, a triangle of it, where in it) | Location, PointLocator (Softimage), primuv (Houdini) | **Surface Sample** | Scatter on Surface's third output, read by Get Data at Samples. Not "location": in Imago a location is a prim path. Not a USD type: positions and normals are what reach USD. |

### Contexts and USD

| Context | Values per | USD interpolation |
| --- | --- | --- |
| Object | primitive | `constant` |
| Point | point | `vertex` |
| Face | polygon or curve | `uniform` |
| Corner | polygon corner | `faceVarying` |

![The parts of a primitive, with Imago's words for
them](docs/imago_geo_parts_v3.png)

The Primitive Inspector has a tab for each, and one for **Edge**, which is not a
context: an edge is two points that follow each other around a face, worked out
from the topology. A face's and an edge's points are its **ptIndices**; a
corner's point is its **ptIndex**.

## Panes

| Old name | New name | Why |
| --- | --- | --- |
| Viewport | **Viewer** | The 3D scene. "Viewer" over "Viewport": it is where the scene is looked at, through any camera. Renders and images have the Image Viewer. |
| (none) | **Image Viewer** | Renders and images: layers (AOVs), channels, exposure. It renders the scene itself (RenderSettings and camera menus, Render, Stop); rray will render into it. Its crate is `xms-image`. |
| Node Graph | **Node Graph** (unchanged) | The pane shows the root scene graph or an ICE graph, so "Scene Graph" would be wrong half the time. |
| Scene Explorer | **Scene Explorer** (unchanged) | The stage's primitives as a hierarchy. |
| Properties | **Parameters** | The selected node's parameters, matching the term above. |
| Primitive Inspector | **Primitive Inspector** (unchanged) | Click a primitive in the Scene Explorer and inspect its contents. Tabs: Object, Point, Edge, Face, Corner, Properties (and Joint, Bone for clips). Properties is USD's sense of the word: a prim's attributes and metadata, not an operator's parameters. |
| Operator Stack | **Operator Stack** (unchanged) | The chain of operators feeding the viewed one. |
| Timeline | Timeline (unchanged) | |
| UV Editor | UV Editor (unchanged) | |
| History | History (unchanged) | |

## Node and menu names

| Old name | New name | Why |
| --- | --- | --- |
| Pick Primitives, Prune Primitives, Unpack | unchanged | "Primitive" now means exactly what these act on. |
| "Primitives" menu category | Primitives (unchanged) | As above. |
| Cube, Sphere, Grid under "Create" | **Shapes** group under "Create" | "Primitive" is no longer used for shapes. |
| SubnetInput, SubnetOutput (ICE); ICE Tree | **Execution Ports** (root) | The new engine: Get Data and Set Data instead of a mesh going in and out. |

## Code renames

| Old | New | When |
| --- | --- | --- |
| `Context::Primitive` | `Context::Face` | With the vocabulary patch |
| `src/scene_graph/` (the Scene Explorer's code) | `src/scene_explorer/` | With the vocabulary patch: "scene graph" now means the operator network |
| `Pane::Properties` | `Pane::Parameters` | With the vocabulary patch; saved layouts with the old name still load |
| `Pane::PrimInspector` | unchanged | The pane keeps its name |
| `Pane::Viewport` | `Pane::Viewer` | With the vocabulary patch; saved layouts with the old name still load. The `src/viewport/` module (the 3D view inside the Viewer) is renamed during the eframe rework, when the Viewer gains its modes |
| `NamedMesh` | `Prim` | Done (eframe rework) |
| `Subnet*` (`SubnetGraph`, `SubnetNode`, `SubnetNodeType`, `SubnetStore`, `SubnetId`…), `NodeType::Subnet` | `IceGraph`, `IceGraphNode`, `IceGraphNodeType`, `IceGraphStore`, `IceGraphId`…, `NodeType::Ice` | Done (eframe rework); graphs saved with `Subnet` still load |
