# ICE graphs

An **ICreationEngine** operator (ICE for short; Modify → ICreationEngine in the add menu) holds an **ICE graph**: a graph of its own that runs on every primitive coming in (or those its **Prims Location** tab lists, as for Transform), in the primitive's own space, and sets its attributes. Loosely modelled after Softimage ICE and Houdini VOPs. Double-click the ICE operator, or select it and press ↓, to open its graph; ↑ comes back.

## How a graph runs

Every ICE graph ends in an **Execution Ports** node, named ExecutionPorts and drawn dark grey (as MaterialX's material, where a graph ends), Softimage's ICETree under another name. Its inputs are **Execute ports**, run in order, top to bottom; a free one is always there for the next. The tree runs **over every element of its context**: Point by default, or Face, Corner or Object (set it in the tree's parameters).

- **Get Data** reads an attribute of the geometry: `P` is the points, any other name is an attribute (`N`, `Cd`, your own). Read in the tree's context, it is one value per element; an Object attribute is one value for all of them.
- **Get Data across contexts.** A Get Data's **Context** says where the attribute is read from: **Auto** (the graph's own context, or Object), or Point, Face, Corner or Object. Read from another context, each element reads the elements it touches, and when it touches several, **Several** makes them one value (Average, Sum, Min, Max):

  | Graph runs per | Reads per | Each element gets |
  |---|---|---|
  | Corner | Point | its point's value |
  | Corner | Face | its face's value |
  | Face | Point | its points' values, made one (a face's centre: P, Average) |
  | Face | Corner | its corners' values, made one |
  | Point | Face | the faces around it, made one (Sum of a face heat; Max for the hottest) |
  | Point | Corner | the corners on it, made one (a UV seam's two UVs averaged) |
  | Object | any | every element's value, made one (the centre of all the points) |

  On curves a face is a curve and its corners are its control points. A boolean made one is true if any is (Min: if all are). Under the title the node shows where it reads from: `face.heat (sum)`.
- **Every attribute the Primitive Inspector lists can be read**: its **Attributes** list, in a Get Data's parameters, shows those of the geometry the graph last ran on, by context, with their kind; picking one sets the name and the context. The list is made when it is opened, not as the graph runs.
- **What the topology gives** is read like a stored attribute, under the names the Primitive Inspector shows (a stored attribute of the same name comes first):

  | Context | Name | Type | What |
  |---|---|---|---|
  | Point | `ptIndex` | Integer | its own number (on points too) |
  | Face | `faceIndex` | Integer | its own number |
  | Face | `corners` | Integer | its corners (a curve's control points) |
  | Face | `firstCorner` | Integer | its first corner; the others follow it |
  | Face | `N` | 3D Vector | its normal (polygons only) |
  | Face | `area` | Scalar | its area (polygons only) |
  | Corner | `ptIndex` | Integer | the point it is at |
  | Corner | `faceIndex` | Integer | the face it is a corner of |
  | Corner | `localCornerIndex` | Integer | its place in that face (0 for the first) |

  Made when a graph that reads them runs, once per run. Per point, Get Data `firstCorner` (Context Face, Several Min) gives the first corner of the first face around each point. A face's `ptIndices` is a list, for Get Array Data. Short names work too: `P` the points, `N` the normals, `uv` the first texture coordinates. Quaternions read as their four numbers; matrices and tokens are not read yet.
- **Set Data** writes an attribute, for every element; a new name makes a new attribute. Its Execute output goes into the Execution Ports. A Get Data of an attribute set by an earlier port reads the new values. Its **Context** says where the attribute is written: **Auto** (the graph's own context), or Point, Face, Corner or Object. Written into another context, each element there takes the values of the graph's elements it touches (the same table as Get Data's, the other way round: a graph per point writing per face gives each face its points' values), made one by **Several**. A name lives in one context: writing it in one takes it out of the others, as USD has one primvar of a name. The points stay per point. Booleans reach the primitive's primvars as 0 and 1.
- **Constants** (Scalar, Integer, 2D, 3D and 4D Vector, Boolean) and **Math** (Add, Subtract, Multiply, Divide, Minimum, Maximum, Linear Interpolate, Negate, Absolute Value, Sine, Cosine, Dot Product, Cross Product, Length, Normalize) work on what they are given. Add takes two floats or two vectors (an int meeting a float is taken as a float); Multiply also scales a vector by a float.
- **Rotations.** A Rotation constant, and Rotation from Axis and Angle, Rotation from Euler Angles (degrees, X then Y then Z) and Rotate Vector. Multiply combines two rotations, Linear Interpolate blends them along the shortest arc, Normalize makes one unit again. An attribute of USD's `quatf` type (an instancer's `orientations`, say) reads and writes as a rotation.
- **Random Value** gives a value between Min and Max (numbers, ints or vectors; ints include Max), different for each element and the same every time for the same Seed. With a Count, it gives an array of that many instead. Its **Seed input** says what each value is drawn for: unwired, the element's own number (`ptIndex` per point, `faceIndex` per face, its index in an array); wired, an int per element draws each for its seed, so elements sharing a seed share a value (each point of a face, with the face's index read from its corners; a stable `id` when points are reordered), and one int for all moves every value at once. The node's own Seed is mixed in, so two Random nodes on one seed still differ.
- **Scatter on Surface** puts a Count of places on the primitive's polygons, spread evenly by area, the same places for the same Seed. A Weight attribute (a number per face or per point) scales how many land where: 0 none there, 2 twice as many. In an Object graph (one array for the whole primitive) it gives three arrays: **Positions** (where they are: what a PointInstancer's `positions` takes), **Normals** (the surface's there: the mesh's normals, or each face's own) and **Surface Samples** (each place as a face, a triangle of it and where in that triangle, to read anything else there with Get Data at Samples). Each place is its own hash of the seed: made in parallel, the same every time.
- **Get Data at Samples** reads an attribute at surface samples (Scatter's third output): `uv`, a colour, or any other, `P` and `N` too. A point's or a corner's attribute is blended across the triangle each sample is in, a face's is that face's, the object's its one value. A surface sample is not a USD type and is not written: what reaches USD is positions, normals and what is read at them.
- **Random Orientation** gives a rotation with its +Y turned along **Normals**, then turned at random up to **Random Rotation** degrees about each axis, either way; the same for the same Seed, and with a **Seed input** as Random Value's. It works in any context: per point (`N` per point into Normals, and Set Data `orient`), per face, or, in an Object graph, as an array as long as the normals given (Scatter's normals) or of its Count: wire that into the Instancer's Orientations. Large arrays are made in parallel.
- **Reference inputs.** After its **Scene** input, the ICreationEngine operator has numbered **Reference** inputs (Reference 1, Reference 2…), one always free: wire anything that gives primitives into them (a Load USD of an asset file, shapes, another branch of the graph). **Get Reference** lists prims on one of them, by path, as an array: all of them, or those its rows name, in the rows' order. Its rows work as a Prims Location tab: type a path or a pattern (`*`, `?`, `**`), pick from the prims wired there (groups too), add the prims selected in the Scene Explorer, or drop them from it.
- **Instancer** (an Execute, in an Object graph) makes a point instancer, as USD's `PointInstancer`, from inputs named after its attributes: **Positions** (an array of 3D vectors: Scatter's), **Prototypes** (paths, from Get Reference), **Proto Indices** (an int for all, or an array; -1 picks one at random for each, the same for the same Seed), **Orientations** (a rotation for all, or an array: Random Orientation's) and **Scales** (a number or a 3D vector, for all or an array: Random Value with a Count gives one). Its **Primitive Location** is where the instancer is made: `/root/instancer` to start with, any path, or a name alone, beside the primitive the graph runs on (`scatter` on `/set/ground` is `/set/scatter`; not under it, since USD has no prims under a mesh); a path already there is numbered. The node shows the instancer's name, not its whole path. It comes out of the operator as Imago keeps a file's point instancer: a points prim, one point per instance with `protoIndex`, `orient`, `scale` and `id`, and its prototypes under it, `<path>/Prototypes/<name>/…`, each prim of a prototype drawn at its instances, sharing its geometry and material (nothing is copied). A prototype's prims keep their placement within it, so a prototype made at its file's origin sits on each instance. Write USD writes it as a `PointInstancer` with its `prototypes`, and its prototype prims below it. Its **Draw Mode** (Geometry, Bounding Box, Origin) says how viewports draw the prototypes: written as USD's `model:drawMode` on each prototype (with `model:applyDrawMode` and kind component), so usdview and Houdini draw them so too, and the Viewer draws them so.
- **Arrays.** In an Object graph (one value per primitive) a value can be an array: Random Value with a Count makes one, and maths works on arrays element by element (an array times 2 doubles each element; two arrays add element by element). Array Size, Select in Array, Array Sum and Array Average turn one back into a single value. **Concatenate Arrays** joins arrays of one type one after the other (the paths of two Get References, the positions of two scatters; a single value joins as one element), keeping one input free for the next. Arrays do not yet meet per-point values, or get written as attributes; the node that tries says so. An array's ports and wires are violet whatever it holds; hover over a port to see what flows there ("3D vector", "array of surface samples", "array of paths").
- **Get Array Data** reads an attribute as a **list per element**, every value it touches kept rather than made one as Get Data's Several does: per face, `ptIndices` (its point numbers, in order), or `P` from Point (its points' positions); per point, `faceIndices` (the faces around it); a corner's is a list of one. Maths works on a list item by item, a per-element value meeting each item (each point's x plus the face's own id), and lists of one context meet each other. Array Size, Select in Array (an index for all, or one per element), Array Sum and Array Average make one value of each element's list. In an Object graph it reads every value, as the graph's array (`P`: every point). A list is not written as an attribute, and does not meet Concatenate Arrays or Random yet.
- **A Count takes an array.** Random Value's and Random Orientation's Count is an int, or an array wired in (Scatter's positions): as many as it holds, so there is no Array Size between them.

Right-click or Tab on the canvas to add a node: type to find it (↓ and ↑ through what is found, Enter adds), or pick it from its category, as in the scene graph and MaterialX graphs. A node is named after what it is (Get Data, Set Data, Add); the attribute a Get or Set Data reads or writes shows under its title (`self.P`). Drag from an output to an input to wire them. Click a wire to select it and Delete takes it off; click a port with wires then the empty canvas to unplug it; drag from an input to pick its wire up and move or unplug it; drag a box on the canvas to select several nodes (see [Node graph](interface.md#node-graph)).

**Ports and wires are coloured by type**, as everywhere in XMS (see [Data type colours](interface.md#data-type-colours)): Scalar teal-green, Integer and Boolean blue, 2D and 3D Vector yellow, 4D Vector (a quaternion, a rotation) dark yellow; Execute, and what the graph has not run on yet, dark grey. The types are worked out, as the graph is drawn, on the geometry the graph last ran on (a Get Data is the type of the attribute it reads), for every node, wired into the Execution Ports or not.

**Inputs hold values.** An input with nothing wired to it uses its own value, set in the Parameters pane (with its type: Scalar, Integer, 3D Vector, Boolean). A value meets the type of what it is combined with: a 1 added to a vector is the vector (1, 1, 1).

**Errors show on the node.** A graph that does not fit together (a vector added to a boolean, an attribute that is not there, a per-face attribute read in a per-point tree) leaves the primitive unchanged; the node where it broke gets a red ring, and why is written along the top of the canvas.

## Fast

Moving, panning, zooming or selecting in an ICE graph cooks nothing, and neither does putting a node down: only a change to what reaches the Execution Ports (a value, a wire, a node wired in) runs it again, so the graph stays as quick to work in on a heavy model as the scene graph. A run keeps the geometry it ran on (its columns shared, not copied); the editor works out port types and Get Data's list from it, not the run.

The graph is compiled, then run in parallel batches of 4,096 elements:

- only the attributes the graph reads are read, a batch at a time;
- only the attributes it sets get new values; every other attribute stays shared with what came in;
- what does not change from element to element (constants, Object attributes, maths on those) is worked out once.

A million points moved by a graph like P + N × heat × amount take about a millisecond.

## Try it

Templates → ICE → **ICE graph: spherify**: Get Data P → Normalize → Multiply (B = 1.2) → Set Data P, run by the Execution Ports. Change Multiply's B in its parameters.

## Not yet

- Prototypes written as references to their files: today Write USD writes a prototype's geometry into the layer, and its material binding points at the material in the reference's scene, which is only in the layer when that scene is also in the operator's Scene
- Prototypes that are themselves instanced
- Scattering per face (a count per face, each face's places stable on their own)
- Lists per element meeting Concatenate Arrays, Random or Set Data; neighbours as lists (a point's neighbouring points)
- Neighbours in the same context (a point's neighbouring points, through the faces)
- Snippets: code in a node, compiled to the same instructions
- ICE graphs saved in graph files (copied nodes carry them)

## Tests

`cargo test -p xms-ice` (the engine), `cargo test ice::` (the editor's graph on the engine), `cargo test -p xms-ice --release ice_engine_speed -- --ignored --nocapture` (its speed).
