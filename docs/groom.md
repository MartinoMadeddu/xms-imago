# Groom (plan)

**Planned work for the future: nothing here is built yet.** Written 27
November 2026 from Martino's brief and the workshop that followed (two
mockups, XGen Interactive Groom, Softimage's Hair and ICE strands, Houdini's
groom). It is the direction to take after [Paint](paint.md) and the bases
listed under "Before it starts".

## The aim

Procedural as Houdini, artist friendly and interactive as XGen. A groom is
built from a few chunky operators an artist reads at a glance, each opening
into more control when wanted: brushes where an artist wants their hands,
ICE where a TD wants the wiring.

## Curves are curves

There are no "guides" and no "hair": every groom is BasisCurves, and what
a set of curves does depends on the input it is wired into. Four hundred
styled curves wired into Hair Generate's Guides input guide; the same curves
wired into a Clump effect are the clump centres; 200,000 generated curves
can be brushed as directly as 400. A brush works on any curves.

## The operators

| Operator | What it does | Built as |
|---|---|---|
| **Paint** | the maps on the skin: density, length, clump, masks ([Paint](paint.md)) | native operator, its own file |
| **Groom Session** | plants curves and brushes them, in layers; owns its data | native operator (it keeps data) |
| **Hair Generate** | grows curves on the skin from the painted density, shaped by guide curves | native operator (it makes curves) |
| **Groom Modify** | the procedural effects, in order, in one pass over the points | one operator holding an ICE graph of compounds |
| **Plant Curves** | curves planted from a density map, alone | native operator |

A head reads: Skin, Paint, Groom Session (plant and style), Hair Generate,
Groom Modify, Groom Session (touch-up), Material. One short column per
region (scalp, brows, lashes, body), merged before the material.

### Groom Session

As the Edit Session, interactive while used and procedural after.

- **Its curves:** planted by its own Plant brush (on the skin, by the
  painted density or freely), brought in on its input, or both.
- **Brushes,** on the shared brush of [Paint](paint.md): Comb, Length, Cut,
  Smooth, Lift (puff), Clump, Twist, Plant, Delete; radius, strength, a
  falloff curve, Mirror X, on the surface. They act on all curves, the
  selected ones, or those inside a painted mask.
- **Layers** of strokes, listed in its Parameters as the Edit Session's
  steps: each can be turned off, weighted, reordered and keyed. **Freeze**
  merges them into one; **Explode** makes each a Sculpt Layer operator in the
  graph.
- **A layer keeps offsets, not curves:** each curve's change relative to its
  root, in the skin's local frame at that root, by curve ID. A change
  upstream (more density, a repainted map, an animated skin) shows through,
  and the styling stays on the curves that remain.
- **Curves that are new** (added upstream) have no strokes. As an option,
  they take those of their nearest neighbours at rest, so raising the
  density does not leave unbrushed curves among brushed ones.
- **No effects stack inside it.** Procedural effects are Groom Modify's.
- **Its data** is a file beside the graph, as Paint's, so the graph stays
  light.

### Hair Generate

- **Inputs:** the skin (its main input, with its painted maps) and Guides
  (any curves); more as they come (a part mask, an exclusion).
- **Parameters:** count, or a density with the painted map; length and width
  by map; how curves follow the guides (nearest N, weighted by distance,
  within a radius, across a part only where the mask allows); points per
  curve; seed.
- **Stable IDs (a hard rule):** each curve's ID comes from where its root
  sits on the skin, not its order. Changing the count keeps the IDs of the
  roots that stay; added roots get new ones; removed roots take their strokes
  with them. Without this, a touch-up Groom Session after it would put every
  stroke on the wrong curve.

### Groom Modify

One scene operator holding an ICE graph of effect compounds, run in one
pass over the points: 15 effects cost one copy of the groom, not 15.

- **For an artist:** its Parameters list the effects in order (on, mix,
  up, down, remove) and, below, each effect's promoted parameters. **+
  Effect** adds one of the shipped compounds; nothing needs wiring.
- **For a TD:** **Open ICE Graph** shows the compounds wired in order, to
  change or add to.
- **The shipped effects,** each an ICE compound with ramps and painted maps
  as inputs: Clump (by clump curves on an input, by size, a root-to-tip
  profile), Frizz, Cut, Length, Curl, Noise, Scraggle, Flyaways, Bend.

## Speed while brushing

The curves a brush touches, and a light preview of what is grown from them
(about one in ten), update with every dab; the rest updates when the stroke
ends. XGen grows every hair on the GPU while brushing; that is for later.
Nothing below the session re-cooks during a stroke.

## Animation

Curves are bound to rest places on the skin (a face and its barycentric
place) and follow it as it deforms, with a frame along each curve so a
twist or a lift turns with the skin.

## In USD and in renders

The groom goes to USD as BasisCurves with widths and the attributes the
effects leave (for shading: root and tip colour, a random per curve), and to
Arnold as curves. Groom Sessions' and Paint's files stay beside the graph;
only what they made reaches USD.

## Before it starts

- **[Paint](paint.md)** with its shared brush: hit, falloff, symmetry,
  strokes, undo, colours in the Viewer.
- **ICE, along curves:** each point's nearest curve and a place along it
  (for Clump); a frame along a curve (for Curl and Twist); neighbours along
  a curve ([ICE](ice.md), Not yet).
- **ICE that makes geometry** (curves and points), later: then Hair Generate
  can be a compound too, and TDs can write their own generators.

## Order to do it in

1. Groom Session: planting and the brushes on curves, layers with offsets
   by ID, Explode and Freeze, its file.
2. Hair Generate with stable IDs and guide interpolation.
3. Groom Modify and the first effects: Clump, Frizz, Cut, Length, Noise.
4. Binding to an animated skin.
5. The touch-up case: brushing generated curves, strokes carried to new
   curves by their neighbours.
6. More effects, and a GPU preview while brushing if it is needed.

## Left out for now

- **Simulation.**
- **Cards and feathers** from the same tools.
- **An effects stack inside the Groom Session:** Groom Modify is where
  effects go.

## References

- Autodesk, Maya: XGen Interactive Grooming (brushes, sculpt layers,
  modifier stacks).
- Autodesk, Softimage 2014: Hair (guide and render hairs, styling
  operators, freezing the stack); ICE strands and Melena (hair as ICE
  compounds).
- SideFX, Houdini: Guide Groom, Hair Generate, Guide Process, Guide Deform.
