# Imago architecture

The decisions that shape Imago's data and evaluation, and why. Read this before changing the geometry model, a node that touches geometry, the USD path, or ICE. Details live in `docs/geometry.md`, `docs/usd.md` and the code.

## 1. Shape of the program

- **Scene graph:** a node graph, flowing top to bottom. Each node cooks its inputs into an `EvalResult` (a mesh, primitives, an animation clip). The graph is cooked when its content changes, not every frame.
- **ICE graphs:** a graph inside an ICE node, flowing left to right, run on the geometry coming in. Loosely modelled after Softimage ICE and Houdini VOPs.
- **USD is the scene description.** Stages come in through Load USD and go back out through Write USD as opinions over the original file. Imago never needs to own the whole stage.

## 2. USD in

```
file ──openusd (composition)──> xms-usd ──> xms-scene (scene layer) ──usd_stage──> primitives + StageTree
```

- **Composition is `openusd`'s** (pure Rust): sublayers, references, payloads, inherits, specializes, variants.
- **`xms-usd` and `xms-scene`** are forks of rray's `rray-usd` and `rray-scene`. They stay separate crates, ready to merge with rray's into one shared crate when rray becomes a render delegate.
- **openusd's `Stage` is `!Send`.** Translation runs on one thread; only plain data crosses threads.
- **`StageTree`** is the stage's hierarchy, shared by every primitive of the stage. Each node holds its path, type, own transform, and whether it's hidden, an instance proxy or an instancer prototype. The tree also records the stage's root correction (up axis, unit) and source file. It drives the Scene Explorer, viewport boxes and writing.

## 3. Geometry: `Geo`

One model for primitives, nodes and ICE (`core::geo`).

| `Context` | values per | USD interpolation | ICE |
|---|---|---|---|
| `Object` | prim | `constant` | per object |
| `Point` | point | `vertex` | per point |
| `Face` | polygon / curve | `uniform` | per face |
| `Corner` | polygon corner | `faceVarying` | per sample |

- **`Column`:** a typed array, `Arc<Vec<T>>` for `T` in `bool, i32, f32, [f32;2], [f32;3], [f32;4], quat, [f64;16], token`. This is the generalised `Array<T>`: every attribute in every context is one.
- **`Attr`:** a column, plus optional indices (USD indexed primvars) and a role (point, normal, vector, colour, texCoord) for writing the right USD type.
- **`Topology`:** `Points`, `Mesh` (counts, indices, orientation, subdivision scheme) or `Curves` (counts, basis, wrap). The arrays are `Arc` too.
- **Names follow USD:** `points`, `normals`, `st`, `widths`, `displayColor`, a primvar's own name. Interfaces may alias them (ICE's `P`).
- **`validate()`:** attributes cover their context, indices are in range, topology references existing points.

## 4. Copy on demand

- **Cloning a `Geo` copies pointers.**
- **Writing goes through `Arc::make_mut`:** only the written column is copied, and only if shared.
- **Consequences:**
  - A node changing UVs on a 10M-point mesh copies one column.
  - Untouched data (normals, primvars, subsets, UV indices) survives every node.
  - **Identity is information:** a column still pointing at the loaded `Arc` is, by that fact, unedited.

## 5. Primitives

`NamedMesh`: one per mesh, curves or points prim, passed between nodes without copying.

| Field | |
|---|---|
| `path` | prim path (instance proxies under their instance's path) |
| `geo` | geometry in its **own space** |
| `place` | `InPlace`, `One(Mat4)`, or `Many(Arc<[Mat4]>)` for point-instancer copies. Stage prims are always placed: world transform × root correction |
| `source` | the prim as loaded (geo, place, material, purpose): what edits are measured against |
| `stage` | `Arc<StageTree>` |
| `material`, `look`, `purpose`, `picked` | |
| `mesh` | `MeshData` view of `geo` for legacy consumers (viewport, Edit Poly, UV). Temporary, see §9 |

- **Instancing is kept.** Prototype geometry is held once, and copies are placements. Transform moves placements, not points.
- **Nothing is baked.** Up axis and unit live in the placement, never in the points.
- **Pick, then edit:** nodes work on picked primitives and pass the rest through. A single placed prim is edited where it's drawn and returned to its own space and placement (`map_mesh`). Several picked prims are merged (`geo: None`).

## 6. Edits and USD out

- **`edits::prim_edit`:** a primitive compared with its `source`, **by pointer**. The result covers placement, topology, attributes written or added, attributes lost, material and purpose. If `replaced` is set, the geometry is no longer traceable and must be written whole.
- **`edit_set(prims, tree)`:** a stage's changes, prims no longer arriving (to deactivate), and prims made by nodes (to add).
- **`edits_between(a, b)`:** the edit layer of the nodes between two points. This is the basis for Solaris-style stacked layers.
- **Write USD (`usd_write`):** an `.usda` that sublayers the source and holds only `over`s.
  - It repeats `upAxis` and `metersPerUnit`, since USD reads stage metadata only from the root layer.
  - Moved prims get their own `xformOp:transform` (`L' = L · loaded⁻¹ · now`), so they keep following their parents.
  - Changed attributes are written with type, interpolation and indices; lost ones are blocked with `= None`.
  - Pruned prims are deactivated; new prims are written as `def`s.
  - **Not written yet, but reported:** edits inside native instances (instance proxies are read-only in USD) and on point-instancer prototypes.
  - Values are written at the current frame.

## 7. Viewer

- **Draws what the Scene Explorer opens.** A closed prim is the box around its subtree; open prims hand down to their children. There's a per-row geometry toggle and a "Show all geometry" switch. Boxes come from the primitives as they arrive, so they follow upstream nodes.
- **Purposes:** guide/proxy/render toggles. Render geometry gives way to its asset's proxy when proxies are shown.
- **Instancing:** a mesh with more than 64 copies is one entity with a GPU instance buffer of matrices and colours, uploaded once per change. Fewer copies are drawn as entities with full materials.
- **Curves and points:** curves are evaluated to polylines at draw time (their control points are kept); points are drawn as crosses sized by width.
- **Redraws on demand:** input, a change, playback, background jobs. An idle window costs nothing.
- **Current renderer:** Bevy. It is being replaced by eframe + wgpu (§10). Bevy is already confined to the shell: `main.rs`, `modelling.rs`, `viewport/` and `shell.rs` (components, the Viewer's rectangle, the program's state registered as resources). Everything else uses `glam` and `egui` directly.

## 8. ICE

**Now:** a graph runs on each primitive, in the primitive's own space (object space), on its `Geo`. Nodes read and write whole per-point arrays (`P` = points). Writes replace only their column. The output is the primitive with its `geo` updated, so ICE results are edits like any other node's and are written to USD the same way. The node implementations are placeholders.

**Planned engine:** loosely modelled after Softimage ICE and Houdini VOPs: ICE's contexts and per-element arrays, with a compile step in the spirit of VOPs. Three layers.

1. **Graph.** Ports carry **type**, **structure** (single value, or array per element) and **context**. These are validated when you connect, and a mismatch shows as a red wire. Constants take the context they're wired into. The graph runs over a chosen context and executes its Execute ports in order.
2. **Compiler.** It turns the graph into a plan:
   - context and type resolution
   - **uniform/varying analysis**: object-context values, constants and parameters are computed once and hoisted
   - dead-code elimination and constant folding
   - compounds inlined
   - polymorphic nodes monomorphised into typed instructions
   - **read/write analysis**: only written columns are copied
   - parameter changes update a constant table and never recompile
3. **Executor.** It runs the plan:
   - **batches** of about 4–16k elements through the whole plan, with per-thread batch registers and no full-size temporaries
   - Rayon over batches; tight per-instruction loops that auto-vectorise
   - group masks: only selected elements are iterated
   - an element writes only itself; writes elsewhere go through explicit **merge modes** (set, add, min, max, append), resolved deterministically after each batch
   - spatial indices built once per evaluation and shared
- **Snippet node** (wrangle-like inline code): compiles to the **same instructions** as nodes, so it's just as fast and uses the same plan.
- **Promoted parameters,** value watches on wires, per-node timings.
- **Simulation** (state across frames) comes later, as a separate mode.
- **Keep the instruction set pointer-free** (typed column loads and stores, maths) so the plan can later be lowered to WGSL and run on the GPU.

## 9. Rules for anyone writing a node

1. **Replace only the columns you change.** Clone the `Geo`; write through `points_mut()`, `attr_mut()` or `set()`. Never rebuild a `Geo` to change one thing: every rebuilt column becomes an edit, a copy and a write.
2. **Keep primitives placed.** Move by `place` (`Placement::moved`), and work in the primitive's own space. `NamedMesh::world()` gives the drawn mesh when needed.
3. **Keep identity.** Build outputs with `NamedMesh { geo, mesh, ..p.clone() }` so `path`, `source`, `stage`, material and purpose travel along. If geometry can't stay traceable, set `geo: None`.
4. **Keep `mesh` in step with `geo`.** Rebuild the view (`geo.to_mesh()`), or clear `geo` if only `mesh` changed.

## 10. Where it's going

1. **eframe + wgpu** replaces Bevy as the app shell. egui UI code stays as it is. The viewport becomes a raster **render delegate** at parity, drawing straight from `Geo`, which retires the eager `MeshData` view.
2. **The ICE engine** of §8.
3. **rray as render delegates**: its viewport integrator and path tracer, consuming the scene layer plus edits (dirty bits). At that point `xms-scene` and `rray-scene` merge into one shared crate.
4. **Follow-ups:**
   - Write USD: time samples over a range, prototype/de-instance choice, flatten
   - instancer edits
   - LOD and culling for heavy instancers
   - implicit shapes, per-face material subsets

## Module map

| | |
|---|---|
| `src/shell.rs` | what ties the program to Bevy until the eframe rework: components, resources |
| `src/core/xform.rs` | `Transform` (translation, rotation, scale) for clips, on glam |
| `crates/xms-scene`, `crates/xms-usd` | scene layer, USD → scene layer translation (forked from rray) |
| `src/usd_stage.rs` | scene layer → `Geo` + placements + `StageTree` |
| `src/usd_scene.rs` | `UsdScene`, loading and caching, root-layer fallback |
| `src/core/geo.rs` | `Geo`, `Column`, `Attr`, `Topology`, `Context` |
| `src/types.rs` | `NamedMesh`, `Placement`, `Source`, `Purpose`, `EvalResult`, `map_mesh` |
| `src/edits.rs` | edits against the source, edit sets, layers between nodes |
| `src/usd_write.rs` | override layer writer |
| `src/node_graph/` | scene graph: nodes, evaluation, UI |
| `src/ice/` | ICE graphs: graph, execution, editor |
| `src/viewport/` | the Viewer's 3D view: bounds and boxes, GPU instancing, camera |
| `src/strands.rs` | curves and points as lines |
