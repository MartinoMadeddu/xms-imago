# Imago vocabulary

One word for one thing. **Primitives** are objects in the stage. The **scene
graph** is the network of **operators**. Geometry carries **attributes**. Nodes
have **parameters**.

## Terms

| Concept | Name | Why |
| --- | --- | --- |
| An object in the USD stage, and Imago's piece for it | **primitive** (prim for short) | One word for the stage object and the piece that carries it. |
| The top-level graph | **scene graph** (the **root scene graph**) | Names the graph that builds the scene. In USD, "scene graph" often means the prim hierarchy, which in Imago is the Scene Explorer: the docs say so once. |
| The operator holding an ICE graph | **ICreationEngine** (ICE for short) | Its name in the add menu, on the node and in its parameters, after Softimage's ICE, the Interactive Creative Environment. |
| The graph inside an ICE node | **ICE graph** | Matches "scene graph". |
| A node of the scene graph | **operator** | Each takes the scene coming in and changes it, hence the Operator Stack. "Node" stays the generic word for any graph element. |
| A step inside Edit Poly | **operation** | Distinct from operator: an operator holds operations. |
| A node's settings | **parameters** | Not to be confused with USD properties (a prim's attributes and relationships). |
| Text and number fields drawn dark grey with light text | **Dark fields** (`DARK_FIELD_BG`) | A switch in the colour editor, not a colour of the scheme: on in the Light preset, off in Dark and ADHD; Reset keeps it. |
| Per-element data on geometry | **attributes** | One word everywhere: UI, docs, code, ICE. Attributes are written to USD as primvars, with their interpolation. |
| Where attribute values live (UI, ICE and code) | **Object, Point, Face, Corner** | One set of names everywhere. "Primitive" means a stage object, and one value per primitive is the Object context, so the per-polygon context is Face. Face also covers curves (one value per curve). |
| Cube, sphere, grid | **shapes** | Keeps "primitive" for stage objects. |
| Where an operator puts the primitive it makes | **Primitive Location** (a parameter) | As in Solaris: a USD path such as `/cube` or `/set/props/box1`. The parents on it are Xforms. |
| The primitives an operator works on | **Primitive Locations**, in the operator's **Prims Location** tab | As Solaris' Primitives parameter on a LOP: each operator says what it works on. Paths with `*`, `?` and `**`; a path takes everything below it; empty means everything. |
| The operator editing prims already in the scene as what they are | **Edit Primitive**, with a **Type** (Xform, Mesh, Curves, Points, Camera, Light, RenderSettings) | One operator for every kind of prim, its form that kind's. A value changed there is an **override** (✎); the rest follows the prim. Only Edit Primitive has overrides: every other operator's parameters are its values, with no per-parameter create or edit state. |
| Moving prims under another prim | **Reparent**, its **Parent Location** (the Parent), **Move** Primitives or Whole scene | Whole scene is every root prim. Keep World Position is on by default, as Maya's parenting keeps it. |
| How the scene's colours become a display's | **View** and **Display** (two menus, OpenColorIO's), from the **config** (`$OCIO`'s, or Imago's own); ACEScg the **working space** | OpenColorIO's own words, as Houdini, Gaffer and Nuke show them. The scene's colours are as USD authors them, linear Rec.709, turned into the working space on the way. |
| Where Imago's own settings are | Preferences | **Settings** (File > Settings), a **page** per part of the program | One window for every part; a part's page is named after it (Viewer). |
| What a Set Attribute row's value is in USD | **Object** (a constant primvar) or **Property** (the prim's own attribute or metadata) | One operator sets every value on a prim; the context says how it is written. Object is the context of the same name, one value per primitive; per point, face and corner values are ICE's. |
| How a Merge joins its inputs | **Auto**, **Primitives**, **Flatten** (Merge's Mode) | Auto goes by the first input. "Flatten" is what Unpack does: everything as one plain mesh. |
| What the Viewer and the Scene Explorer show | **view flag** | One operator always has it. |
| An operator kept at its result while what is above it changes | **freeze** (a **frozen** operator) | Says what it does to the cooking. Bypass, freeze, ghost and view are the operator's **flags**. |
| A number of an operator's parameters set by frame | **keyed** number; its curve is a **channel**, its points **keys** | A channel is named by its place in the parameters (`translation/0`). The pane that shapes channels is the **Curve Editor**, not "Graph Editor": a graph is the scene graph or an ICE graph. |
| An operator's parameters keyed on frames of its own | **Local Animation** (the operator), its **local frames** from its **Start** to its **End** | Keys the operator wired into it; that operator is not changed. |
| The frames an operator's output spans | its **time range** | Local Animations, clips and keys in and above it, put together: a Merge's is the widest. |
| Inputs lined up in time | **Merge Animation**, its **Align**: Frame to frame, First frame, Last frame, Middle, Stretch to the longest, Shrink to the shortest | Against input A for first, last and middle. |
| The frames of a graph that is not a clip | the **scene range** (first frame, last frame, rate) | Saved with the graph; keys are set on its frames. |
| An operator's output drawn grey beside the scene | **ghost** (a **ghosted** operator) | "Template" is a ready-made graph in the File menu: one word for one thing. |
| An operator making a material | **Material** (the operator), its output a **material** | It puts out a material, not a scene; named by the operator. The quick form: one UsdPreviewSurface. |
| An operator making a material from a graph of shading nodes | **MaterialX** (the operator), its graph a **MaterialX graph** | Its nodes are MaterialX's, by MaterialX's names (`add`, `image`, `standard_surface`). A node's **definition** is MaterialX's `nodedef` (`ND_add_color3`): one node has one per set of types, chosen by what is connected. |
| The operator binding materials | **Material Binding** | USD's word (`material:binding`). Its rows are Primitive Locations, each with a Material column, on its **Bindings** tab. A row bound by **Location** names a material of the scene by its path. |
| What the Viewer shows when it looks through a camera | **looking through** a camera (the Viewer's 🎥 menu); **Perspective** is the Viewer's own camera | As usdview, Maya, Houdini. A camera's **filmback** (its gate: `horizontalAperture`, `verticalAperture`), picked from **Filmback Presets**. |
| How Load USD brings its file in | **Sublayer**, **Reference**, **Payload** (Load USD's Mode) | USD's own words. Reference and Payload put the file's default prim at a Primitive Location. |
| Where Material Binding puts the materials plugged into it | **Material Library Location** (a parameter, `/materials` by default) | A Primitive Location for materials: each goes under it by its operator's name. |
| How a Get Data reading another context makes the values an element touches one | **Resample** (resampling): Average, Sum, Min, Max (Get Data's and Set Data's parameter) | Reading or writing across contexts resamples, from points to faces, faces to points (Martino). Graphs saved with `reduce` still read. A face touches several points. Arrays (Get Array Data) will keep them as a list. |
| What a primitive has from its topology and points rather than stored | **intrinsic**, every one named `intrinsic:…` (`intrinsic:ptIndex`, `intrinsic:corners`, `intrinsic:area`, `intrinsic:pointCount`…) | One list (`xms_geo::intrinsics`), one name wherever it is shown or read: ICE, the Primitive Inspector, the docs. Read only. Point, Face, Corner and Object are Imago's contexts; USD's interpolations (vertex, uniform, faceVarying, constant) are named only where USD is read or written. |
| The colour a port and its wires get from what they carry | **data type colour** (`type_colours::DataType`) | One table for every graph: see docs/interface.md, Data type colours. |
| The operator making a light | **Light**, with a **Type**: Dome, Rect, Point, Sphere, Disk, Cylinder, Distant | UsdLux's types by their schema names; the add menu lists one entry per type, all the one operator. A **point light** is a sphere light treated as a point, as USD has it. |
| A node that hands on what comes in, unchanged, to lay a graph out | **Pass Through** | Says what it does. |
| The operator making the RenderSettings prim and its product | **Render Commons** | What is rendered, whatever renders it: camera, resolution, overscan, motion blur, purposes, the output image. |
| The operator setting a renderer's own settings | **Render Settings** | The name artists know for a renderer's settings. Its **Renderer** is Karma, Arnold, one from a **renderer definition** (a JSON file), or **Custom**, whose settings you name under its **namespace** (`rray:spp`). |
| One AOV a renderer writes | **RenderVar** (USD's word), made by **Render Variables**; Render Settings' **AOVs** tab ticks a renderer's standard ones | USD's `UsdRenderVar`. "AOV" stays in the UI for the standard ones, the word artists use. |
| The image file a render makes | **RenderProduct** (USD's word); its file is its **Output Image** (`productName`) |  |
| The operator running a render command on the system | **Render Command** | Made for rendering the layer (`$USD`); a general Shell or System Command can follow. It runs when **Run** is pressed, never on cook. |
| A texture of one file per UV tile | **UDIM tiles**, named by a **`<UDIM>` path** (`body.<UDIM>.exr`; `<udim>` read too) | The tag USD, Mari and Substance write. |
| Files numbered alike, listed as one | **sequence** (`shot.####.exr`) | One word, shown in the file browser with Collapse sequences. |
| An ICE graph's end node | **Execution Ports** (the node named ExecutionPorts) | Softimage's ICETree: one root with ordered Execute ports. Named for what it is. The geometry is not wired in: Get Data reads its attributes, Set Data writes them. |
| A place on a mesh's surface (or a curve) in an ICE graph | **Polygon** and **Param UV** | Closest Point on Surface's and Scatter on Surface's outputs, read by Get Data and Get Array Data wired to them. The Polygon is a face's or curve's number, the Param UV Houdini's parametric coordinates on it. Not "Primitive": in Imago a primitive is a prim (a USD stage object), and Face is to be called Polygon too. Not "location": a location is a prim path. |
| A piece of graph packaged as one reusable operator (scene graph) or node (ICE graph) | **compound**; what it is made from, its **definition** (an `.icd` file); one placed in a graph, an **instance** | Softimage's word, for both graphs. A definition records its **context**: Scene or ICE. "Definition" is also MaterialX's word for a `nodedef`: the context tells them apart. See [compounds](docs/compounds.md). |
| Giving an inner parameter or port to a compound's interface | **Promote** (Promote…, a **promoted** parameter or port) | The word artists know from Houdini and Katana (Martino). Not reading across contexts: that is **Resample**. |
| A parameter following another | **reference** (Copy as Reference, Paste as Reference, Remove Reference) | The same link as promotion: a target follows a source. |
| A compound instance that follows its definition, or has its own copy | **locked** (the default), **editable** (after **Make Editable**) |  |
| A value an ICE operator gives its graph by name | **Parameter** (an ICE node); its values the ICE operator's **Parameters** | Set in the ICE operator's parameters, so a scene compound can promote it. |

| One part of a rig placed on a mesh (a spine, an arm, a leg…) | **guide** (a **Rig Guide** operator: Spine Guide, Arm Guide…); a set of them chained, the **guides** | mGear's word. A component is the kind (Spine, Arm…), named `{component}_{side}{index}` (`arm_L0`). |
| A point of a guide | **locator** (`arm_L0_elbow`) | Maya's word for a point you place. `pole` is where a hinge points. |
| Placing the locators by measuring the mesh | **autofit** | On by default; the locators are moved by hand after. |
| The skeleton a rig deforms the mesh with | **deform skeleton**, made by **Build Skeleton** for a **target** (Unreal, HumanIK, Maya) | Naming, axes, rotation order and root joint come with the target and can each be changed. |
| A joint's frame the rig reasons in, whatever the target's axes | **canonical frame**: X to the child, Y to the pole, Z = X×Y | Limits and the range of motion are written in it. |
| Weights of 0 or 1, each point to one joint | **hard bind** (Hard Bind), its points per joint a **region** | Changed by **Envelope**: slide, tilt, grow. |
| A machine's piece bound whole to one joint | **Rigid Bind** (binding), **Rigid Skin** (moving it by its placement) | Its geometry and materials stay as they are. |
| Simple shapes standing in for a mesh | **Rig Proxy** | One primitive per joint, `/proxy/<joint>`. |
| A machine part's or a chain's own word in its name | the part's **Name** (`aileron` makes `aileron_L0`) | Parts of one name on both sides move together in the range of motion. |
| Every joint moved through what it can do | **range of motion** (Range of Motion) | Sweeps of each joint, then poses, as fractions of the joints' **limits**. |

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
from the topology. A face's and an edge's points are its
**intrinsic:ptIndices**; a corner's point is its **intrinsic:ptIndex**.

## Panes

| Name | Why |
| --- | --- |
| **Viewer** | The 3D scene: where the scene is looked at, through any camera. Renders and images have the Image Viewer. |
| **Image Viewer** | Renders and images: layers (AOVs), channels, exposure. It renders the scene itself (RenderSettings and camera menus, Render, Stop); rray will render into it. Its crate is `xms-image`. |
| **Node Graph** | The pane shows the root scene graph or an ICE graph, so "Scene Graph" would be wrong half the time. |
| **Scene Explorer** | The stage's primitives as a hierarchy. |
| **Parameters** | The selected node's parameters, matching the term above. |
| **Primitive Inspector** | Click a primitive in the Scene Explorer and inspect its contents. Tabs: Object, Point, Edge, Face, Corner, Properties (and Joint, Bone for clips). Properties is USD's sense of the word: a prim's attributes and metadata, not an operator's parameters. |
| **Operator Stack** | The chain of operators feeding the viewed one. |
| **Timeline** | Playback, the scene range and the selected operator's keys. |
| **UV Editor** | The UVs of the primitives shown. |
| **History** | The scene's undo history. |

## Node and menu names

| Name | Why |
| --- | --- |
| Pick Primitives, Prune Primitives, Unpack | "Primitive" is exactly what these act on. |
| **Primitives** (the add menu's category) | As above. |
| **Shapes** group under "Create" | "Primitive" is kept for stage objects. |
| **Execution Ports** (root) | The ICE graph's root: Get Data reads the geometry and Set Data writes it, with no mesh wired in or out. |
