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
| Per-element data on geometry | attributes / primvars | **attributes** | One word everywhere: UI, docs, code, ICE. Attributes are written to USD as primvars, with their interpolation. |
| Where attribute values live (UI, ICE and code) | Constant, Vertex, Polygon, FaceVarying (UI) · Object, Point, Primitive, Corner (code) | **Object, Point, Face, Corner** | One set of names everywhere. "Primitive" now means a stage object, and one value per primitive is the Object context, so the per-polygon context becomes Face. Face also covers curves (one value per curve). |
| Cube, sphere, grid | primitives | **shapes** | Frees "primitive" for stage objects. |
| ICE graph input and output nodes | SubnetInput / SubnetOutput | **GraphInput / GraphOutput** | Follows "ICE graph". "Graph" also fits ICE compounds (graphs inside an ICE graph), which will use the same terminals. |

### Contexts and USD

| Context | Values per | USD interpolation |
| --- | --- | --- |
| Object | primitive | `constant` |
| Point | point | `vertex` |
| Face | polygon or curve | `uniform` |
| Corner | polygon corner | `faceVarying` |

## Panes

| Old name | New name | Why |
| --- | --- | --- |
| Viewport | **Viewer** | One pane for whatever is being looked at: the 3D scene today, and renders, images and other 2D views as they arrive, through the same pane instead of separate ones. "Viewer" says that; "Viewport" says 3D. |
| Node Graph | **Node Graph** (unchanged) | The pane shows the root scene graph or an ICE graph, so "Scene Graph" would be wrong half the time. |
| Scene Explorer | **Scene Explorer** (unchanged) | The stage's primitives as a hierarchy. |
| Properties | **Parameters** | The selected node's parameters, matching the term above. |
| Primitive Inspector | **Primitive Inspector** (unchanged) | Click a primitive in the Scene Explorer and inspect its contents. Tabs: Object, Point, Face, Corner (and Joint, Bone for clips). |
| Operator Stack | **Operator Stack** (unchanged) | The chain of operators feeding the selected one. |
| Timeline | Timeline (unchanged) | |
| UV Editor | UV Editor (unchanged) | |
| History | History (unchanged) | |

## Node and menu names

| Old name | New name | Why |
| --- | --- | --- |
| Pick Primitives, Prune Primitives, Unpack | unchanged | "Primitive" now means exactly what these act on. |
| "Primitives" menu category | Primitives (unchanged) | As above. |
| Cube, Sphere, Grid under "Create" | **Shapes** group under "Create" | "Primitive" is no longer used for shapes. |
| SubnetInput, SubnetOutput (ICE) | **GraphInput, GraphOutput** | Follows "ICE graph". |

## Code renames

| Old | New | When |
| --- | --- | --- |
| `Context::Primitive` | `Context::Face` | With the vocabulary patch |
| `src/scene_graph/` (the Scene Explorer's code) | `src/scene_explorer/` | With the vocabulary patch: "scene graph" now means the operator network |
| `Pane::Properties` | `Pane::Parameters` | With the vocabulary patch; saved layouts with the old name still load |
| `Pane::PrimInspector` | unchanged | The pane keeps its name |
| `Pane::Viewport` | `Pane::Viewer` | With the vocabulary patch; saved layouts with the old name still load. The `src/viewport/` module (the 3D view inside the Viewer) is renamed during the eframe rework, when the Viewer gains its modes |
| `NamedMesh` | `Prim` | During the eframe rework |
| `Subnet*` (`SubnetGraph`, `SubnetNode`, `SubnetStore`…) | `IceGraph*` | During the eframe rework; saved graphs with the old node type names still load |
