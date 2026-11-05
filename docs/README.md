# XMS documentation

- [Interface](interface.md): dockable panes, saved layout, theme, primitive
  inspector, the file browser (sequences, UDIM), wires and selection in every
  graph, data type colours, Export Movie (the Viewer as an MP4)
- [Undo](undo.md): what is undone, steps, branches, the History pane
- [Templates and example files](templates.md): ready-made graphs for every area
  of the program
- [Edit Poly](edit-poly.md): polygon modelling in one node
- [UV](uv.md): unwrapping, island editing, the UV Editor pane
- [USD](usd.md): composed stages, primitives and instancing, the Scene Explorer
  and the Viewer, Write USD
- [ICE graphs](ice.md): Get Data, Set Data and maths run by the Execution Ports
  over every point, face or corner, compiled and run in parallel
- [MaterialX](materialx.md): the MaterialX node library and the MaterialX
  operator
- [Compounds](compounds.md): a piece of graph as one reusable operator, made
  from a selection with Make Compound, gone inside, saved as an `.icd` file
  definition in a library; promotion, references and ICE compounds being built
- [Rendering](render.md): Render Commons, Render Settings (Karma, Arnold,
  Custom, renderer definition files, AOVs), Render Variables, as UsdRender prims
  Write USD writes
- [Colour](colour.md): OpenColorIO, `$OCIO`'s config or Imago's own, the
  Viewer's and the Image Viewer's displays and views
- [Geometry](geometry.md): how geometry is held and passed between nodes, and
  how edits are found. For anyone writing a node
- [Viewer navigation](navigation.md): the seven navigation styles, framing keys
- [Animation and mocap](mocap.md): clips, the timeline, batch export to FBX
- [Keys and the Curve Editor](keys.md): keying any operator's numbers, the
  scene's range on the timeline, curves, tangents and infinity, Local Animation
  and Merge Animation
- [Body Collide](body-collide.md): keeping a captured character out of a set and
  out of itself
- [Rigging](rigging.md): guides fitted to a mesh, a deform skeleton for Unreal,
  HumanIK or Maya with joint limits, hard bind, envelopes, range of motion,
  delta mush and baked weights
- [Builds and releases](releases.md): where the binaries come from

How the program should work and why is in
[DESIGN_PHILOSOPHY.md](../DESIGN_PHILOSOPHY.md), at the root of the repository;
the operators it has and plans in
[XMS_CORE_OPS_SPEC.md](../XMS_CORE_OPS_SPEC.md). How to work on it with an AI
assistant is in [LLM_instructions.md](../LLM_instructions.md).

**Help > Documentation** opens these pages and the reference as HTML in your
browser, [overview.md](overview.md) first, made from these files (`cargo run -p xms-docs` makes them in
`docs/html`). A link here to another page is to its `.md` file, and to a
heading by its anchor as GitHub writes it (`usd.md#big-stages-boxes-first`), so
it works on GitHub and in the HTML alike. The name at the top is in Anisette
when it is installed, or put in `site/fonts` as `anisette.woff2` or
`anisette.otf` (it is not shipped: its licence does not allow it), else in
Michroma.

These pages describe
[MartinoMadeddu/xms-imago](https://github.com/MartinoMadeddu/xms-imago). They
are updated with every change.
