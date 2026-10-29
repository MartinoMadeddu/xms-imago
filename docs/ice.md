# ICE graphs

An **ICreationEngine** operator (ICE for short; Modify → ICreationEngine in the
add menu) holds an **ICE graph**: a graph of its own that runs on every
primitive coming in (or those its **Prims Location** tab lists, as for
Transform), in the primitive's own space, and sets its attributes. Loosely
modelled after Softimage ICE and Houdini VOPs. Double-click the ICE operator, or
select it and press ↓, to open its graph; ↑ comes back.

## How a graph runs

Every ICE graph ends in an **Execution Ports** node, named ExecutionPorts and
drawn dark grey (as MaterialX's material, where a graph ends), Softimage's
ICETree under another name. Its inputs are **Execute ports**, run in order, top
to bottom; a free one is always there for the next. The tree runs **over every
element of its context**: Point by default, or Face, Corner or Object (set it in
the tree's parameters).

- **Get Data** reads an attribute of the geometry: `P` is the points, any other
  name is an attribute (`N`, `Cd`, your own). Read in the tree's context, it is
  one value per element; an Object attribute is one value for all of them.
- **Get Data across contexts.** A Get Data's **Context** says where the
  attribute is read from: **Auto** (the graph's own context, or Object), or
  Point, Face, Corner or Object. Read from another context, each element reads
  the elements it touches, and when it touches several, **Resample** makes them
  one value (Average, Sum, Min, Max): reading across contexts resamples, from
  points to faces, faces to points:

  | Graph runs per | Reads per | Each element gets |
  |---|---|---|
  | Corner | Point | its point's value |
  | Corner | Face | its face's value |
  | Face | Point | its points' values, made one (a face's centre: P, Average) |
  | Face | Corner | its corners' values, made one |
  | Point | Face | the faces around it, made one (Sum of a face heat; Max for the hottest) |
  | Point | Corner | the corners on it, made one (a UV seam's two UVs averaged) |
  | Object | any | every element's value, made one (the centre of all the points) |

  On curves a face is a curve and its corners are its control points. A boolean
  made one is true if any is (Min: if all are). Under the title the node shows
  where it reads from: `face.heat (sum)`.
- **Every attribute the Primitive Inspector lists can be read**: its
  **Attributes** list, in a Get Data's parameters, shows those of the geometry
  the graph last ran on, by context, with their kind; picking one sets the name
  and the context. The list is made when it is opened, not as the graph runs.
- **What the topology gives** is read like a stored attribute, under the names
  the Primitive Inspector shows (a stored attribute of the same name comes
  first):

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

  Made when a graph that reads them runs, once per run. Per point, Get Data
  `firstCorner` (Context Face, Resample Min) gives the first corner of the first
  face around each point. A face's `ptIndices` is a list, for Get Array Data.
  Short names work too: `P` the points, `N` the normals, `uv` the first texture
  coordinates. Quaternions read as their four numbers; matrices and tokens are
  not read yet.
- **Set Data** writes an attribute, for every element; a new name makes a new
  attribute. Its Execute output goes into the Execution Ports. A Get Data of an
  attribute set by an earlier port reads the new values. Its **Context** says
  where the attribute is written: **Auto** (the graph's own context), or Point,
  Face, Corner or Object. Written into another context, each element there takes
  the values of the graph's elements it touches (the same table as Get Data's,
  the other way round: a graph per point writing per face gives each face its
  points' values), made one by **Resample**. A name lives in one context: writing
  it in one takes it out of the others, as USD has one primvar of a name. The
  points stay per point. Booleans reach the primitive's primvars as 0 and 1.
- **Constants** (Scalar, Integer, 2D, 3D and 4D Vector, Boolean, Rotation,
  Matrix) and **Math** (Add, Subtract, Multiply, Divide, Minimum, Maximum,
  Linear Interpolate, Negate, Absolute Value, Sine, Cosine, Dot Product, Cross
  Product, Length, Normalize, Distance) work on what they are given.
  **Distance** is how far apart two positions (or any two vectors of one size)
  are. Add takes two floats or two vectors (an int meeting a float is taken as a
  float); Multiply also scales a vector by a float.
- **Compose** makes a vector from its parts, as its **Makes** says: Floats to
  2D, 3D or 4D Vector (X, Y, Z, W), 2D Vector and Float to 3D Vector (XY, Z), 3D
  Vector and Float to 4D Vector (XYZ, W), Floats to Rotation (I, J, K, Real;
  made unit). An int is taken as a number. Per element, one value, or the
  graph's arrays element by element. Changing what it makes changes its inputs;
  the wires into those still there stay.
- **Decompose** takes a vector apart, an output per part, as its **From** says:
  2D, 3D or 4D Vector to Floats (X, Y, Z, W), 3D Vector to 2D Vector and Float
  (XY, Z), 4D Vector to 3D Vector and Float (XYZ, W), Rotation to Floats (I, J,
  K, Real), Rotation to Axis and Angle (a unit axis, the angle in degrees). Each
  output can be wired on its own; the node is worked out once. Changing From
  changes its input and outputs; the wires to those still there stay.
- **Matrices.** A Matrix is a 4×4 transform, as USD's `matrix4d`: its columns
  are its X, Y and Z axes and its origin. **Compose Matrix** makes one from
  three axes and an origin (a frame: tangent, normal, bitangent and a position);
  **Decompose Matrix** takes one apart into them. **Multiply** combines two (the
  right one applies first), **Invert Matrix** inverts one, **Transpose Matrix**
  transposes it (for a frame whose axes are unit and at right angles, its turn's
  inverse, cheaper than inverting: take the offset from its origin first).
  **Transform Point** puts a position through a matrix (turned, scaled, moved),
  **Transform Vector** a direction (turned and scaled, not moved). Get Data and
  Set Data read and write `matrix4d` attributes (a matrix per point, say); read
  at a reference mesh's samples, a matrix is the nearest corner's, taken into
  the graph's space.
- **Rotations.** A Rotation constant, and Rotation from Axis and Angle, Rotation
  from Euler Angles (degrees, X then Y then Z) and Rotate Vector. Multiply
  combines two rotations, Linear Interpolate blends them along the shortest arc,
  Normalize makes one unit again. An attribute of USD's `quatf` type (an
  instancer's `orientations`, say) reads and writes as a rotation.
- **Random Value** gives a value between Min and Max (numbers, ints or vectors;
  ints include Max), different for each element and the same every time for the
  same Seed. With a Count, it gives an array of that many instead. Its **Seed
  input** says what each value is drawn for: unwired, the element's own number
  (`ptIndex` per point, `faceIndex` per face, its index in an array); wired, an
  int per element draws each for its seed, so elements sharing a seed share a
  value (each point of a face, with the face's index read from its corners; a
  stable `id` when points are reordered), and one int for all moves every value
  at once. The node's own Seed is mixed in, so two Random nodes on one seed
  still differ.
- **Scatter on Surface** puts a Count of places on the primitive's polygons,
  spread evenly by area, the same places for the same Seed. A Weight attribute
  (a number per face or per point) scales how many land where: 0 none there, 2
  twice as many. In an Object graph (one array for the whole primitive) it gives
  three arrays: **Positions** (where they are: what a PointInstancer's
  `positions` takes), **Normals** (the surface's there: the mesh's normals, or
  each face's own) and **Surface Samples** (each place as a face, a triangle of
  it and where in that triangle, to read anything else there with a Get Data
  wired to them). Each place is its own hash of the seed: made in parallel, the
  same every time.
- **Closest Point on Surface** finds, for each **Origin** (a 3D vector: `P` per
  point, one for all, or an array in an Object graph), the nearest place on the
  meshes of a reference input: its **Geometry** is their paths, from Get
  Reference (a group names the meshes below it; an instanced mesh counts once
  per copy). Its **Radius** limits the search (a number for all or per element;
  0 searches without a limit). The meshes are taken from where they are in the
  world into the space of the primitive the graph runs on, so what is read there
  lines up with the primitive where the Viewer shows both. It gives the
  **Surface Samples** found, and the **Distance** to each. Nothing within the
  radius: a sample of nothing, which reads as zero, at the radius's distance
  (use the Distance to blend: Linear Interpolate from `P` to the place found by
  `1 - Distance / Radius`). A BVH over the meshes' triangles (rray's builder) is
  made once per run of the graph, the first time it is searched, so a graph per
  point searches it from every thread at once. What it makes possible:
  shrink-wraps, sticking points to a body, falloffs by distance to a surface.
- **Get Data at samples.** Wire surface samples (Scatter's third output, Closest
  Point on Surface's first) into a Get Data's **Samples** input and it reads its
  attribute there instead of per element (unwired, it reads as above; graphs
  saved with the former Get Data at Samples node open with a Get Data wired so):
  `uv`, a colour, or any other, `P` and `N` too. At a reference mesh's samples
  it reads that mesh's attribute, taken into the graph's space by its role: a
  position moved, a normal turned, a vector turned. A point's or a corner's
  attribute is blended across the triangle each sample is in, a face's is that
  face's, the object's its one value. A surface sample is not a USD type and is
  not written: what reaches USD is positions, normals and what is read at them.
- **Random Orientation** gives a rotation with its +Y turned along **Normals**,
  then turned at random up to **Random Rotation** degrees about each axis,
  either way; the same for the same Seed, and with a **Seed input** as Random
  Value's. It works in any context: per point (`N` per point into Normals, and
  Set Data `orient`), per face, or, in an Object graph, as an array as long as
  the normals given (Scatter's normals) or of its Count: wire that into the
  Instancer's Orientations. Large arrays are made in parallel.
- **Reference inputs.** After its **Scene** input, the ICreationEngine operator
  has numbered **Reference** inputs (Reference 1, Reference 2…), one always
  free: wire anything that gives primitives into them (a Load USD of an asset
  file, shapes, another branch of the graph). **Get Reference** lists prims on
  one of them, by path, as an array: all of them, or those its rows name, in the
  rows' order. Its rows work as a Prims Location tab: type a path or a pattern
  (`*`, `?`, `**`), pick from the prims wired there (groups too), add the prims
  selected in the Scene Explorer, or drop them from it.
- **Instancer** (an Execute, in an Object graph) makes a point instancer, as
  USD's `PointInstancer`, from inputs named after its attributes: **Positions**
  (an array of 3D vectors: Scatter's), **Prototypes** (paths, from Get
  Reference), **Proto Indices** (an int for all, or an array; -1 picks one at
  random for each, the same for the same Seed), **Orientations** (a rotation for
  all, or an array: Random Orientation's) and **Scales** (a number or a 3D
  vector, for all or an array: Random Value with a Count gives one). Its
  **Primitive Location** is where the instancer is made: `/root/instancer` to
  start with, any path, or a name alone, beside the primitive the graph runs on
  (`scatter` on `/set/ground` is `/set/scatter`; not under it, since USD has no
  prims under a mesh); a path already there is numbered. The node shows the
  instancer's name, not its whole path. It comes out of the operator as Imago
  keeps a file's point instancer: a points prim, one point per instance with
  `protoIndex`, `orient`, `scale` and `id`, and its prototypes under it,
  `<path>/Prototypes/<name>/…`, each prim of a prototype drawn at its instances,
  sharing its geometry and material (nothing is copied). A prototype's prims
  keep their placement within it, so a prototype made at its file's origin sits
  on each instance. Write USD writes it as a `PointInstancer` with its
  `prototypes`, and its prototype prims below it. Its **Draw Mode** (Geometry,
  Bounding Box, Origin) says how viewports draw the prototypes: written as USD's
  `model:drawMode` on each prototype (with `model:applyDrawMode` and kind
  component), so usdview and Houdini draw them so too, and the Viewer draws them
  so: one box or origin per instance, around the whole prototype.
- **Arrays.** In an Object graph (one value per primitive) a value can be an
  array: Random Value with a Count makes one, and maths works on arrays element
  by element (an array times 2 doubles each element; two arrays add element by
  element). Array Size, Select in Array, Array Sum and Array Average turn one
  back into a single value. **Concatenate Arrays** joins arrays of one type one
  after the other (the paths of two Get References, the positions of two
  scatters; a single value joins as one element), keeping one input free for the
  next. Arrays do not yet meet per-point values, or get written as attributes;
  the node that tries says so. An array's ports and wires are pink-magenta
  whatever it holds; hover over a port to see what flows there ("3D vector",
  "array of surface samples", "array of paths").
- **Get Array Data** reads an attribute as a **list per element**, every value
  it touches kept rather than made one as Get Data's Resample does: per face,
  `ptIndices` (its point numbers, in order), or `P` from Point (its points'
  positions); per point, `faceIndices` (the faces around it); a corner's is a
  list of one. Maths works on a list item by item, a per-element value meeting
  each item (each point's x plus the face's own id), and lists of one context
  meet each other. Array Size, Select in Array (an index for all, or one per
  element), Array Sum and Array Average make one value of each element's list.
  In an Object graph it reads every value, as the graph's array (`P`: every
  point). A list is not written as an attribute, and does not meet Concatenate
  Arrays or Random yet.
- **Get Data from another prim.** Get Data and Get Array Data also take a
  **Geometry** input: paths from Get Reference, on one of the operator's
  reference inputs. Wired, they read from that prim instead of the geometry the
  graph runs on (the first one the paths name), taken into the graph's space
  (positions moved, normals and vectors turned):

  | Samples | Geometry | Reads |
  |---|---|---|
  | — | — | the attribute of each element, as above |
  | wired | — | at the samples, on the prim they were found on |
  | wired | wired | at the samples' faces and weights, on the Geometry's prim: samples found on a rest mesh read on its animated copy |
  | — | wired | the Geometry's attribute element by element, in the Context set (across contexts as Get Data reads across): an attribute transfer by name |

  Element by element (and the samples' faces) takes prims of the **same
  topology**: the same counts per context, the same faces (a rest mesh and its
  animated copy; a mesh and the copy a branch of the graph changed); Get Data
  says so when the counts differ. Between meshes of different topology, Closest
  Point on Surface and a Get Data at its samples transfer by place.
- **A sample's triangle.** Every surface sample is on a triangle of its face
  (the face's fan from its first corner) with a weight on each corner. Read at
  samples, two names give them whatever the mesh holds: **`bary`** (Get Data:
  the three weights as a 3D vector; Get Array Data: a list of three numbers) and
  **`triP`** (Get Array Data: the triangle's three points, in the graph's
  space). A list of `triP` and one of `bary` meet item by item: multiply them
  and Array Sum is the place on the face, as P read there is; Select in Array 0,
  1 and 2 gives the corners, for a frame's edges (tangent `p1 - p0`, normal
  `(p1 - p0) × (p2 - p0)`, both normalised, bitangent normal × tangent).
- **Get Array Data at samples.** Wired to surface samples through its
  **Samples** input, a Get Array Data reads, at each sample, the values of the
  **face** it is on, kept as a list: its points' positions (`P`), their numbers
  (`ptIndices`), its corners' UVs (`uv`); a face's own attribute is a list of
  one. Lists from the same samples meet in maths (a sample's face's `P` and
  `uv`, item by item); Array Size, Select in Array, Sum and Average make one
  value of each. One sample per element (Closest Point's, per point) gives a
  list per element; one sample for all, an array. An array of samples
  (Scatter's) would give a list per sample: not yet.
- **A Count takes an array.** Random Value's and Random Orientation's Count is
  an int, or an array wired in (Scatter's positions): as many as it holds, so
  there is no Array Size between them.

Right-click or Tab on the canvas to add a node: type to find it (↓ and ↑ through
what is found, Enter adds), or pick it from its category, as in the scene graph
and MaterialX graphs. A node is named after what it is (Get Data, Set Data,
Add); the attribute a Get or Set Data reads or writes shows under its title
(`self.P`). Drag from an output to an input to wire them. Click a wire to select
it and Delete takes it off; click a port with wires then the empty canvas to
unplug it; drag from an input to pick its wire up and move or unplug it; drag a
box on the canvas to select several nodes (see [Node
graph](interface.md#node-graph)).

**Ports and wires are coloured by type**, as everywhere in XMS (see [Data type
colours](interface.md#data-type-colours)): Scalar teal-green, Integer and
Boolean blue, 2D and 3D Vector yellow, 4D Vector (a quaternion, a rotation) dark
yellow; Execute, and what the graph has not run on yet, dark grey. The types are
worked out, as the graph is drawn, on the geometry the graph last ran on (a Get
Data is the type of the attribute it reads), for every node, wired into the
Execution Ports or not.

**Inputs hold values.** An input with nothing wired to it uses its own value,
set in the Parameters pane (with its type: Scalar, Integer, 3D Vector, Boolean).
A value meets the type of what it is combined with: a 1 added to a vector is the
vector (1, 1, 1).

**Errors show on the node.** A graph that does not fit together (a vector added
to a boolean, an attribute that is not there, a per-face attribute read in a
per-point tree) leaves the primitive unchanged; the node where it broke gets a
red ring, and why is written along the top of the canvas.

## Compounds

Nodes of an ICE graph can be packed into one **compound** node (see
[Compounds](compounds.md)): select them, open the add menu (right-click or Tab)
and choose **Compounds > Make Compound**, or type to search for it. The wires
that crossed the selection become its ports: a **Compound Input** inside for
each output wired in, holding the value of the input it fed, and a **Compound
Output** for each output wired out (from a Set Data: an Execute, for the
Execution Ports). The compound is drawn lavender; its inputs hold values, as any
node's, used while nothing is wired in. Its definition's **Name** takes
variables when entered (`$OPNAME` is the compound's own name), as a scene
compound's does.

- **Go inside:** double-click it, or select it and press ↓. The bar over the
  graph shows the way back (the ICE operator, the compounds entered, **↑ Up**,
  or ↑). The graph keeps running with the inside as it is edited.
- **Ports** are the Compound Input and Compound Output nodes inside, top to
  bottom, by name: add or rename one there, and the compound's port follows
  when you come back out. A Compound Input's value is the port's, for while
  nothing is wired to it. Right-click a node (Promote Input, then the input) or an input's rows
  in its parameters (Promote Input) to promote an input holding a value:
  inside a compound, it becomes a port of the compound (a Compound Input);
  at the top of the ICE operator's graph, a parameter of the operator (a
  Parameter node, set in the operator's parameters, and from there promoted
  or referenced in the scene graph). Both are named after the node and the
  input, hold its value and are wired in. Inside a locked compound (an ICE one,
  or the scene compound the operator is in) the entry is greyed out.
- **Save as Definition…** in its parameters writes it as an `.icd` file; saved
  in a library folder, it is listed under **Compounds** in every ICE graph's add
  menu and the instance follows the file, locked. A locked compound's inside is
  read-only (a change is put back) until **Make Editable**, in its parameters or
  the bar over its inside. **Revert to Latest Definition** drops an instance's
  own copy.
- **Run as its nodes:** before a graph runs, every compound is replaced by its
  nodes (compounds inside compounds too), so a compound costs nothing over the
  nodes it holds. A compound whose definition cannot be found, or one inside
  itself, stops the graph with a red ring on it, saying why. A node inside a
  compound that breaks the graph rings the compound; go inside, and the ring is
  on that node.

**Settings are promoted too:** right-click a node's setting in its parameters
(a seed, an Instancer's Primitive Location, an attribute's name, a count) and
**Promote**. At the top of the ICE operator's graph it becomes a parameter of
the operator, set in its parameters (and from there promoted or referenced in
the scene graph); inside a compound, a parameter of the compound, set on each
instance (a compound inside it can promote that row again, however deep). The
setting follows its parameter, as a reference follows its source, drawn
orange; **Remove Promotion** makes it its own again. A promoted seed replaces
the node's seed: two nodes following one Seed draw the same numbers, so
promote one seed for each when they should differ. Settings are promoted by
the node's name: renaming the node keeps them, two nodes of one name cannot
be promoted until one is renamed. Switches and choices promote (a choice
with its options); a row of a list (Get Reference's paths) promotes the whole
list. Right-click a group's title for **Promote Group**: each of its settings
promoted into a group of that name; **Remove Group Promotion** takes them back. A setting promoted
alone goes in a group of its own group's name too.

**Parameter** (Data Access) is a value the ICE operator gives, by name: the
operator's parameters list every Parameter node of its graph (and of its
compounds), each at its default until set there. The operator keeps a value
for each, so a scene compound holding it can promote it, and another operator
reference it, as any number of an operator's parameters.

## Fast

Moving, panning, zooming or selecting in an ICE graph cooks nothing, and neither
does putting a node down: only a change to what reaches the Execution Ports (a
value, a wire, a node wired in) runs it again, so the graph stays as quick to
work in on a heavy model as the scene graph. A run keeps the geometry it ran on
(its columns shared, not copied); the editor works out port types and Get Data's
list from it, not the run.

The graph is compiled, then run in parallel batches of 4,096 elements:

- only the attributes the graph reads are read, a batch at a time;
- only the attributes it sets get new values; every other attribute stays shared
  with what came in;
- what does not change from element to element (constants, Object attributes,
  maths on those) is worked out once.

A million points moved by a graph like P + N × heat × amount take about a
millisecond.

## Try it

Templates → ICE → **ICE graph: spherify**: Get Data P → Normalize → Multiply
(B = 1.2) → Set Data P, run by the Execution Ports. Change Multiply's B in its
parameters.

## Not yet

- Prototypes written as references to their files: today Write USD writes a
  prototype's geometry into the layer, and its material binding points at the
  material in the reference's scene, which is only in the layer when that scene
  is also in the operator's Scene
- Prototypes that are themselves instanced
- Scattering per face (a count per face, each face's places stable on their own)
- Lists per element meeting Concatenate Arrays, Random or Set Data; neighbours
  as lists (a point's neighbouring points)
- Neighbours in the same context (a point's neighbouring points, through the
  faces)
- Closest Point on the graph's own surface, and on points (a point cloud's
  nearest points, by a kd-tree); ray casts (Raycast on Surface); Get Array Data
  on an array of samples
- Reading at a reference mesh's samples in the editor's types: the port shows
  its type once the graph has run on that mesh
- Snippets: code in a node, compiled to the same instructions

## Tests

`cargo test -p xms-ice` (the engine), `cargo test ice::` (the editor's graph on
the engine),
`cargo test -p xms-ice --release ice_engine_speed -- --ignored --nocapture` (its
speed).
