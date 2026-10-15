# Templates and example files

The Templates menu, in the menu bar at the top left, holds ready-made graphs. Picking one asks whether to add it to the current graph, beside the nodes already there, or open it as a new graph; opening as new asks to save unsaved changes first (see [Interface](interface.md#files)). On an empty graph it opens straight away. A line under the header then says what to look at.

The menu is in two parts, by whose work the templates show: **Martino** (composed USD stages, ICE) and **Simon** (everything else).

![The Viewer of twelve of the templates](xms_templates.png)

## Martino: USD stages

![The six templates of Martino's part of the menu](xms_martino_templates.png)

Each loads one stage from `examples/stage/` with Load USD. The Scene Explorer lists the stage; a closed prim is drawn as the box around what is below it until it is opened, or until Show all geometry is ticked.

| Template | Shows |
|---|---|
| Composition: a street | A stage from four files: the road and pavements from a sublayer, three tables referenced from `table.usda`, the shapes on them in the variant `cafe` of the variant set `dressing`, and an override in the root layer that moves the south pavement |
| Curves and points | BasisCurves of each basis (linear, Bezier, Catmull-Rom, a periodic B-spline), 600 strands of grass, and points whose width grows along a spiral |
| Point instancer: a forest | 4,900 pines and stones from one PointInstancer, each turned and scaled, drawn by GPU instancing. A Transform after it turns the forest by moving the placements; the three shared meshes are not copied |
| Native instances: a hall of tables | 64 instanceable references to `table.usda`: five meshes read once and placed 64 times |
| Purposes: street lamps | Three lamps, each with render geometry, a proxy of blocks and a guide (its cone of light as lines). The render, proxy and guide toggles of the Scene Explorer switch them |
| Material bindings | Martino's graph: a cube, two spheres and a UV-unwrapped grid at Primitive Locations under `/root/asset`, joined by one Merge, two moved by Transforms naming them. MaterialBinding puts basePlastic, baseMetal, ground (textured with Imago's splash) and test_placeHolder under `/root/materials`, and binds the first three to the cube, the sphere and the grid by row; test_placeHolder stays unbound in the library. MaterialBinding2 binds ground to the second sphere by Location. Write USD, viewed, writes to the temporary folder |
| USD: material binding | The DeLorean with two Material operators, CarPaint and Chrome, wired into a Material Binding: row 1 (`/*`) binds input 1 to the whole model, row 2 (`/**/*Wheel*`) input 2 to the wheels, the later row winning. The materials go under `/materials`. Textured mode shows them; a Write USD node after it writes them |
| USD: set attribute | The DeLorean with two Set Attribute nodes: `visibility` invisible on `/**/*Wheel*`, and `kind` and a `displayColor` on `/*`, the root prim, which is a group: one opinion on it, not one per mesh. A Write USD node after them shows them written |

## Martino: ICE

| Template | Shows |
|---|---|
| ICE graph: spherify | A cube subdivided three times, then an ICE graph: Normalize sets every point to unit length, Multiply scales by 1.2. Select the Spherify node and press the down arrow to go inside, the up arrow to come back |

## Simon: Basics

| Template | Shows |
|---|---|
| Shapes | Cube, sphere and grid, each moved with a Transform and joined with Merge |
| ICE graph | Points scattered on a grid and a small sphere copied onto each, inside an ICE graph. Double-click the ICE node, or select it and press ↓, to open it. Its Merge is set to Flatten: both inputs come from the same grid, at the same path |
| USD import | Two USD files loaded with Load USD, one moved onto the other, merged |

## Simon: Modelling

| Template | Shows |
|---|---|
| Edit Poly: tower | Inset, extrude and bevel stacked on the top face of a cube, all live |
| Edit Poly: panels | By-polygon inset and extrude on every cell of a grid, a bevel on every other one |
| Edit Poly: goblet | Edge loops with Connect, a Transform, extrudes and Subdivide. The first three operations are collapsed |
| Edit Poly: bridge | Two cubes joined with Bridge. The selection is a box rule, so it follows changes upstream |
| Edit Poly: bolt | Chamfer, slice, hinge and the other newer operations on one cube |
| Edit Poly: sea mine | Eleven operations on one cube, with one round of subdivision at the start and two at the end: about 9,600 polygons. The hull is collapsed, the horns and ports are live |

## Simon: UV

| Template | Shows |
|---|---|
| Unwrap the sea mine | UV Unwrap on the sea mine, shown in the UV Editor |
| UDIM tiles | A body and two hands made of separate pieces, unwrapped over three tiles |
| Edit UV islands | A box unwrapped, the layout scaled with UV Transform, two islands moved with UV Edit |

## Simon: USD

| Template | Shows |
|---|---|
| USD: heavy model | A 461,595 triangle model as 43 primitives, with its materials, textures and UVs |
| USD: pick and edit | One wheel picked by a path pattern, moved, and handed to an Edit Poly node. The other primitives pass through |
| USD: prune | The model cut down to its wheels and callipers, listed in the Prune node's Prims Location tab as `/**/*Wheel*` and `/**/*Calliper*` |

## Simon: Animation & Mocap

| Template | Shows |
|---|---|
| Clip basics | Test Clip, Rename Joints, Trim, Retime and Set Timecode in a chain. Select each node to see the timeline follow |
| FBX import | The example take loaded with Load FBX and trimmed by a second at each end |
| T-pose and export | Auto T-Pose, Fix Pose lowering the arms, Proxy Skin and Write FBX |
| Mocap tools | A character of the example take through the clip tool nodes |
| Retarget | A character of the example take driving the test skeleton, which rests in another pose |
| Body Collide: obstacle course | The first 20 seconds of the car take on a course of boxes, one case each: a kerb stood on, a pole and a wall the arms swing into, a door walked through, a block stood and sat on, a ledge the leg rests on. Opens solved. See [Body Collide](body-collide.md#obstacle-course) |
| Body Collide: into the car | A captured actor walks through a car and sits in it, kept out of the seat, the floor and himself, within what a human body can do. A Characterize node shows which joint is which part. Opens solved. See [Body Collide](body-collide.md) |
| Mocap split (example takes) | The mocap split graph on the example folder: one take of two characters |
| Mocap split | The same graph with no folder set |

## Simon: FBX and characters

| Template | Shows |
|---|---|
| Load FBX: what a file holds | `examples/fbx/studio.fbx`, written by hand: a floor and a crate with their materials and UVs on the Geometry output, which is the one viewed, a camera that dollies in and a point light listed in the parameters |
| Retarget onto a character | The example take's first actor on the 342-joint character of `ragdoll/split/into_the_car_.fbx`, taken in the pose its skin was bound in (Load FBX's Rest output), then Reach: feet where the actor's are, within human limits |
| Capture and Skin | The same, with the character's mesh weighted again by proximity (Capture) and its skin put on the retargeted take by joint name (Skin). Bypass Capture to use the file's own weights |
| Mocap folder: retarget, skin, collide, write | The example takes folder, each actor placed by a Transform and retargeted onto the character, feet set by Reach, kept out of a stand-in set by Body Collide, written as skinned FBX to `{dir}/retargeted`. All files on a Write node solves and writes the whole folder |
| Human Limits | Fix Pose bends the test walk's left elbow 80 degrees the wrong way; Human Limits, against the walk before it, bends it back |

## Example files

In the `examples` folder:

| File | Content |
|---|---|
| `takes/S3_12point5_002.fbx` | A real motion capture take: two characters ("Skeleton 001" and "Skeleton 002"), 136 bones, 728 frames at 120 fps |
| `DeLorean.usdz` | A production-size model: 43 mesh prims, 461,595 triangles, 30 materials, 40 textures. By VTX (https://sketchfab.com/VTX_car), CC BY-NC-SA 4.0: no commercial use. See `examples/CREDITS.md` |
| `shapes.usda` | Three meshes: pyramid, prism, octahedron |
| `table.usda` | A table: a top and four legs, five meshes |
| `stage/street.usda`, `stage/street_base.usda` | A street: the base as a sublayer, references to `table.usda` and `shapes.usda`, a variant set, an override |
| `stage/strands.usda` | BasisCurves of each basis, grass strands, a Points prim with widths |
| `stage/forest.usda` | A PointInstancer of 4,900 copies of two prototypes |
| `stage/tables.usda` | 64 instanceable references to `table.usda` |
| `stage/lamps.usda` | Three lamps with render, proxy and guide purposes |

The FBX file holds skeletons and animation only, no meshes.

The program looks for the folder in this order: the folder named by the `XMS_EXAMPLES` environment variable, next to the program, the root of the source tree when run with `cargo run`, the working folder. If it is not found, the template still loads and its message says so.

The USD files are generated by the program. To make them again:

    cargo test write_example_files -- --ignored
