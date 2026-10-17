# Imago vocabulary

One word for one thing. **Primitives** are objects in the stage. The **scene graph** is the network of **operators**. Geometry carries **attributes**. Nodes have **parameters**.

## Terms

| Concept | Old name | New name | Why |
| --- | --- | --- | --- |
| An object in the USD stage, and Imago's piece for it | prim / packed primitive | **primitive** (prim for short) | One word for the stage object and the piece that carries it. "Packed" was an implementation detail. |
| The top-level graph | node graph / scene network | **scene graph** (the **root scene graph**) | Names the graph that builds the scene. In USD, "scene graph" often means the prim hierarchy, which in Imago is the Scene Explorer: the docs say so once. |
| The graph inside an ICE node | subnet / ICE tree | **ICE graph** | Matches "scene graph". "Subnet" was a code term. |
| A node of the scene graph | node | **operator** | Each takes the scene coming in and changes it, hence the Operator Stack. "Node" stays the generic word for any graph element. |
| A step inside Edit Poly | operation | **operation** (unchanged) | Distinct from operator: an operator holds operations. |
| A node's settings | properties | **parameters** | Not to be confused with USD properties (a prim's attributes and relationships). |
| Text and number fields drawn dark grey with light text | dark field background | **Dark fields** (`DARK_FIELD_BG`) | A switch in the colour editor, not a colour of the scheme: it stays across presets. The variable keeps the name it was asked for under. |
| Per-element data on geometry | attributes / primvars | **attributes** | One word everywhere: UI, docs, code, ICE. Attributes are written to USD as primvars, with their interpolation. |
| Where attribute values live (UI, ICE and code) | Constant, Vertex, Polygon, FaceVarying (UI) · Object, Point, Primitive, Corner (code) | **Object, Point, Face, Corner** | One set of names everywhere. "Primitive" now means a stage object, and one value per primitive is the Object context, so the per-polygon context becomes Face. Face also covers curves (one value per curve). |
| Cube, sphere, grid | primitives | **shapes** | Frees "primitive" for stage objects. |
| Where an operator puts the primitive it makes | path / prim path | **Primitive Location** (a parameter) | As in Solaris: a USD path such as `/cube` or `/set/props/box1`. The parents on it are Xforms. |
| The primitives an operator works on | Pick Primitives upstream | **Primitive Locations**, in the operator's **Prims Location** tab | As Solaris' Primitives parameter on a LOP: each operator says what it works on. Paths with `*`, `?` and `**`; a path takes everything below it; empty means everything. |
| What a Set Attribute row's value is in USD | Set Primvar, Set Metadata, Set Active, Set Kind… (separate operators) | **Object** (a constant primvar) or **Property** (the prim's own attribute or metadata) | One operator sets every value on a prim; the context says how it is written. Object is the context of the same name, one value per primitive; per point, face and corner values are ICE's. |
| How a Merge joins its inputs | (none: always one mesh) | **Auto**, **Primitives**, **Flatten** (Merge's Mode) | Auto goes by the first input. "Flatten" is what Unpack does: everything as one plain mesh. |
| What the Viewer and the Scene Explorer show | Output node | **view flag** | One operator always has it. |
| An operator making a material | Create Material | **Material** (the operator), its output a **material** | It puts out a material, not a scene; named by the operator. The quick form: one UsdPreviewSurface. |
| An operator making a material from a graph of shading nodes | material compound, shader network | **MaterialX** (the operator), its graph a **MaterialX graph** | Its nodes are MaterialX's, by MaterialX's names (`add`, `image`, `standard_surface`). A node's **definition** is MaterialX's `nodedef` (`ND_add_color3`): one node has one per set of types, chosen by what is connected. |
| The operator binding materials | Material Assign, Mat Bindings | **Material Binding** | USD's word (`material:binding`). Its rows are Primitive Locations, each with a Material column, on its **Bindings** tab. A row bound by **Location** names a material of the scene by its path. |
| What the Viewer shows when it looks through a camera | camera view | **looking through** a camera (the Viewer's 🎥 menu); **Perspective** is the Viewer's own camera | As usdview, Maya, Houdini. A camera's **filmback** (its gate: `horizontalAperture`, `verticalAperture`), picked from **Filmback Presets**. |
| How Load USD brings its file in | (always sublayered) | **Sublayer**, **Reference**, **Payload** (Load USD's Mode) | USD's own words. Reference and Payload put the file's default prim at a Primitive Location. |
| Where Material Binding puts the materials plugged into it | material library | **Material Library Location** (a parameter, `/materials` by default) | A Primitive Location for materials: each goes under it by its operator's name. |
| An ICE graph's end node | SubnetInput / SubnetOutput, GraphInput / GraphOutput | **ICE Tree** | As in Softimage: one root with ordered Execute ports. The geometry is not wired in: Get Data reads its attributes, Set Data writes them. |

### Contexts and USD

| Context | Values per | USD interpolation |
| --- | --- | --- |
| Object | primitive | `constant` |
| Point | point | `vertex` |
| Face | polygon or curve | `uniform` |
| Corner | polygon corner | `faceVarying` |

![The parts of a primitive, with Imago's words for them](docs/imago_geo_parts_v3.png)

The Primitive Inspector has a tab for each, and one for **Edge**, which is not a context: an edge is two points that follow each other around a face, worked out from the topology. A face's and an edge's points are its **ptIndices**; a corner's point is its **ptIndex**.

## Panes

| Old name | New name | Why |
| --- | --- | --- |
| Viewport | **Viewer** | One pane for whatever is being looked at: the 3D scene today, and renders, images and other 2D views as they arrive, through the same pane instead of separate ones. "Viewer" says that; "Viewport" says 3D. |
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
| SubnetInput, SubnetOutput (ICE) | **ICE Tree** (root) | The new engine: Get Data and Set Data instead of a mesh going in and out. |

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
