# Imago architecture

The decisions that shape Imago's data and evaluation. Read this before changing
the geometry model, a node that touches geometry, the USD path or ICE. Details
are in `docs/` and the code; the operators are in `XMS_CORE_OPS_SPEC.md`.

## 1. Shape

- **Scene graph:** operators flowing top to bottom, each cooking its inputs
  into an `EvalResult` (primitives, a mesh, a clip, a material). A result is
  kept until something it reads changes (its cook key), not remade every frame.
- **ICE graphs:** per-element graphs inside an ICE operator, flowing left to
  right, compiled and run by `crates/xms-ice`.
- **USD is the scene.** Load USD brings stages in; Write USD writes opinions
  over them. Imago never owns the whole stage.
- **One scene at the view flag.** There is no Output node: one operator always
  has the flag, and the Viewer, Scene Explorer and Primitive Inspector all show
  its result.

## 2. USD in

```
file ──openusd──> xms-usd ──> xms-scene ──usd_stage──> primitives + StageTree
```

- Composition is `openusd`'s (pure Rust, Imago's fork). Its `Stage` is `!Send`:
  translation runs on one thread, only plain data crosses threads.
- `xms-usd` and `xms-scene` come from rray, kept separate to merge back when
  rray becomes a render delegate.
- `StageTree`: the stage's hierarchy, shared by all its primitives: paths,
  types, transforms, what the file authors, the root correction (up axis,
  unit), and what operators did to prims without geometry (opinions, moves).
- Time samples: a stage that changes over time cooks again each frame. When
  only transforms and geometry move, `usd_stage::Quick` remakes only those
  prims; otherwise the stage is walked again.
- A big stage is read boxes first, then in full, each value read once.
- Alembic opens as a USD layer (`crates/xms-alembic`).

## 3. Geometry: `Geo`

One model for primitives, operators and ICE (`crates/xms-geo`, no
dependencies).

| `Context` | values per | USD interpolation |
|---|---|---|
| `Object` | prim | `constant` |
| `Point` | point | `vertex` |
| `Face` | polygon / curve | `uniform` |
| `Corner` | polygon corner | `faceVarying` |

![The parts of a primitive: points, edges, faces, corners and the primitive
itself, each with what it holds](docs/imago_geo_parts_v3.png)

- An attribute is a typed `Column` (`Arc<Vec<T>>`), with optional indices and a
  role. Edges are not a context: they are worked out from the topology.
- `Topology`: `Points`, `Mesh` or `Curves`, its arrays `Arc` too.
- Names follow USD (`points`, `normals`, `st`); interfaces may alias them
  (ICE's `P`).
- Intrinsics (`intrinsic:area`, `intrinsic:pointCount`…) are one list in
  `xms_geo::intrinsics`, read by ICE and the Primitive Inspector.

## 4. Copy on demand

- Cloning a `Geo` copies pointers; writing goes through `Arc::make_mut`, so
  only the written column is copied, and only if shared.
- **Identity is information:** a column still pointing at the loaded `Arc` is
  unedited. Edits are found by comparing pointers, not values.

## 5. Primitives

A `Prim` is one mesh, curves or points prim (or a light, camera, material or
render prim with no geometry), passed between operators without copying:

- `path`, `geo` in its **own space**, and `place`: `InPlace`, one matrix, or
  many (instancer copies).
- `source`: the prim as loaded, which edits are measured against.
- `stage`: the shared `StageTree`.

Principles:

- **Nothing is baked.** A stage is read as authored; axis and unit corrections
  go in the placement, never in the points. Transform moves placements.
- **Instancing is kept.** Prototype geometry is held once; copies are
  placements. A point instancer is a points primitive with its prototypes.
- **Pick, then edit.** Operators work on the primitives their Primitive
  Locations pick and pass the rest through; the list never reaches the
  operators after them.
- **Operators that name prims** (Set Attribute, Edit Primitive, Reparent) edit
  prims of any type, without geometry too, through the stage's `opinions`.
- **Shapes are primitives** at their Primitive Location, written as `def`.
- **Materials are values** (`EvalResult::Material`), bound by Material Binding
  as prims at their library location.
- **Load modes:** sublayered stages keep their paths; referenced or payloaded
  ones are rebased to their Primitive Location.

## 6. Edits and USD out

- `edits::prim_edit` compares a primitive with its `source` by pointer:
  placement, topology, attributes written, added or lost, material, purpose.
- Write USD (`usd_write`) writes a `.usda` that sublayers the sources and holds
  only `over`s: changed attributes with their types and indices, lost ones
  blocked, pruned prims deactivated, new prims as `def`s, Reparent's moves as
  references at the new path with the old one deactivated. Empty `over`s are
  left out.
- Animation is written as `timeSamples` where it changes, one value where it
  does not.

## 7. Viewer

- wgpu inside the eframe window: its own texture (reversed depth, 4x MSAA,
  linear `Rgba16Float`), laid into the pane by egui's paint callback.
- Draws what the Scene Explorer opens: a closed prim is a box; past a triangle
  budget, the heaviest prims are boxed.
- Prims sharing geometry and material are one instanced draw; draws are
  matched to their GPU meshes by key, and a deforming mesh's points go into
  last frame's buffers.
- Colour is OpenColorIO (`crates/xms-colour`): the scene's linear Rec.709 into
  ACEScg, then a view and display, on the GPU in the Viewer and on the CPU in
  the Image Viewer.
- Redraws on demand: an idle window costs nothing.

## 8. ICE

- A graph runs on each primitive in its own space, on its `Geo`. Its output is
  the primitive with new columns for the attributes it sets, so ICE results are
  edits like any other and written to USD the same way.
- The compiler resolves types and contexts, hoists uniform values, keeps only
  what reaches the root and lists the columns read and written.
- The executor runs batches of 4,096 elements on every core (rayon), registers
  per thread, no full-size temporaries.
- Spatial indices (Closest Point's BVH) are built once per run, in parallel,
  **before** the batches start: nothing filled lazily may be built from inside
  a rayon task, which can deadlock.
- The instruction set stays pointer-free, so a plan can later run on the GPU.

## 9. Rules for anyone writing an operator

1. **Replace only the columns you change.** Clone the `Geo` and write through
   `points_mut()`, `attr_mut()` or `set()`; never rebuild it to change one
   thing.
2. **Keep primitives placed.** Move by `place`, work in the primitive's own
   space.
3. **Keep identity.** Build outputs with `Prim { geo, mesh, ..p.clone() }`; if
   geometry can't stay traceable, set `geo: None`.
4. **Keep `mesh` in step with `geo`** (`to_mesh()`, `to_mesh_lean()`), or clear
   `geo` if only `mesh` changed.

## 10. Where it's going

1. The Viewer drawing straight from `Geo`, retiring the `MeshData` view.
2. The rest of the ICE engine: arrays, merge modes on writes elsewhere, a
   Snippet node compiling to the same instructions, simulation.
3. rray as render delegate, consuming the scene layer and its edits; `xms-scene`
   and rray's merge.
4. Write USD: de-instancing, flattening, instancer edits.

## Module map

| | |
|---|---|
| `src/app.rs`, `src/eframe_app.rs` | the program's state, and the window that owns it |
| `src/node_graph/` | the scene graph: operators, cooking, compounds, references, variables down the wires |
| `src/core/` | operators' work: properties, Set Attribute, Edit Primitive, Reparent, locations, lights, rendering, command lines, variables |
| `src/types.rs`, `src/edits.rs` | `Prim`, `Placement`, `EvalResult`; edits against the source |
| `src/usd_stage.rs`, `src/usd_scene.rs`, `src/usd_write.rs` | USD in (stage to primitives, loading and caching) and out |
| `src/viewer/` | the Viewer on wgpu, Edit Poly's picking and handles, Export Movie |
| `src/scene_explorer/`, `src/prim_inspector/` | the scene as a tree; the selected prims' values |
| `src/ice/`, `src/materialx/` | the ICE and MaterialX editors |
| `src/image_viewer.rs`, `src/colour.rs`, `src/settings.rs` | renders, colour menus, File > Settings |
| `src/compounds.rs`, `src/inside_compound.rs` | compound definitions and going inside them |
| `src/docs.rs` | Help > Documentation |
| `crates/xms-geo` | `Geo`, columns, topology, intrinsics |
| `crates/xms-scene`, `crates/xms-usd` | the scene layer and USD into it |
| `crates/xms-alembic` | Alembic in pure Rust |
| `crates/xms-ice` | the ICE engine: graph, compiler, executor, spatial search |
| `crates/xms-matx` | MaterialX node libraries and documents |
| `crates/xms-image` | images as layers of float channels, OpenEXR |
| `crates/xms-colour` | OpenColorIO config, views and transforms |
| `crates/xms-docs` | the HTML documentation from the Markdown |
| `crates/ragdoll` | collision cleanup for motion capture |
