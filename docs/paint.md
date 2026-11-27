# Paint (plan)

**Planned work for the future: nothing here is built yet.** Written 10
November 2026 from Martino's brief, with the changes agreed after reviewing
it; updated 27 November 2026: environment work first (density and IDs for
scatters), one brush shared with [Groom](groom.md), and the environment
workflow below. Painting is the next large piece of work.

## What it is for

A Paint operator and its brush: values painted on geometry in the Viewer,
read by whatever samples the surface. Environment work, scatters, groom,
crowds and deformers read the same painted map at their own positions
(scatter points, curve roots, crowd agents): one map, several readers.

## Two ways to store it

- **Point mode:** one value per point, on any primitive with points (meshes,
  curves, point clouds). It is an ordinary point attribute: ICE reads it as
  any other, and Write USD writes it as a `vertex` primvar every program
  reads. On a mesh it is Mesh Colors at resolution 0, so it shares their
  file format.
- **Mesh Colors mode** (Yuksel, Keyser, House 2010): values per face in the
  face's own parameters, no UVs and no images. Samples sit on the corners,
  the edges and inside the faces; neighbouring faces share their corner and
  edge samples, so the map has no seams. It is the default once it is built,
  and point mode is the toggle.

Mesh Colors fit Imago's surface places (`Loc`, what Closest Point on Surface
and Scatter on Surface give): both use barycentric coordinates on a triangle
and a bilinear patch on a quad. A painted value is read at a place the way an
attribute is today.

## Mesh Colors data

- **Resolution per face:** `R = 2^r − 1`. Where neighbouring faces differ,
  the extra edge samples of the finer face are interpolated. The resolution
  is chosen from the face's area in world space and a target density (a
  large ground polygon gets more samples), then stored, never chosen again.
- **Sample positions:** on a triangle, sample `(i, j)` is at barycentric
  `(i/R, j/R, 1 − i/R − j/R)`; on a quad, a bilinear grid. A face of five
  corners or more is a ring of triangles around a sample at its middle, as
  its places are a fan around the average of its corners.
- **In memory:** one flat buffer of values, offsets per face and the
  resolutions (as USD's `faceVertexCounts`), no `Vec` per element.
- **Channels:**
  - float: density, weights, length;
  - colour;
  - int (ID): region masks, variants, agent types. Read at the nearest
    sample, never interpolated (between regions 3 and 7 is not region 5),
    and never smoothed;
  - direction (combing, flow): in the surface's local frame, so it follows
    a deformation. The frames are shared with the curve deformer.

## Reading it (ICE)

The face and its place come from the closest point on the triangle BVH; the
small triangle of samples holding the place gives the value, interpolated
from its three samples. ICE reads it as an attribute: at a place, or at a
point (the corner's sample).

## The brush

One brush, shared: Paint first, then the Groom Session's brushes and the
Scatter Session's, later sculpting. What it does (hit, falloff, symmetry,
strokes, undo, what it shows) is written once, so all of them are as fast.

- **Speed:** while a stroke is painted, only the painted values and their
  colours in the Viewer change, on the GPU; nothing below the Paint operator
  cooks. What reads the paint (a scatter, a groom) cooks when the stroke ends,
  or, as an option, at a few frames a second while it is painted.
- **Hit:** a ray against the triangle BVH.
- **Samples under it:** each sample's position in the world, made from its
  face's corners, in a k-d tree (`KdPointTree`), searched within the radius.
- **Operations:** add, subtract, set, smooth, flood, erase.
- **Controls:** a falloff curve, symmetry, a radius in the world or on the
  screen, masking by the angle of the normal or by another attribute.
- **Shown** in the Viewer as colours while painting, with the value under
  the cursor.
- **Undo:** each stroke's changes, kept in memory.

## The Paint operator

- Its result is the geometry with the attribute: an operator like any
  other, its output an override.
- **It keeps its own layer, not the result:** the values painted and how
  strongly each sample was painted. Over an attribute made upstream (an ICE
  base map: procedural base, painted correction), the two are blended when
  it cooks, so a change upstream still shows through. The strength is also
  what tells "painted 0" from "not painted" for set and erase.
- **Blend modes:** replace (by the strength), add, subtract, multiply, min,
  max.
- It keeps values, not the strokes that made them.

## Its file

One file per Paint operator, its layer:

- **`.mcol`:** a header (format version, channel type and count, a hash of
  the topology), the resolutions per face, the values, the strengths, and
  the samples' positions at rest (for resampling), compressed with
  `lz4_flex` (already one of Imago's dependencies; read on every load, so
  fast over small). Points on curves or point clouds are the same file with
  resolution 0 and no faces.
- **Named by its content:** a hash in the name, so the same paint is the
  same file, two machines never clash, and a file a farm render may be
  reading is never written over. The operator keeps the path it was given
  when it was made (`$GDIR/paint/<operator>.<attr>.<hash>.mcol`), so
  renaming the operator does not lose its file. Files no saved graph names
  are cleaned up.
- **Written** to a temporary file then renamed (never half written), when
  the graph is saved, not with every stroke: strokes go into memory.
- **The topology hash** is of the faces' counts and indices, not the
  points', which move when deformed or animated. When it does not match the
  mesh, the values are resampled to the new mesh from the stored positions
  at rest, through the closest point: never applied as they are.

## In USD

- **Point mode:** a `vertex` primvar, as any attribute Imago writes.
- **Mesh Colors:** `xms:paint:<attr>`, an asset path to the `.mcol`
  (resolved by the USD asset resolver, so relative paths work), its name and
  channel type; and the values at the points as an ordinary primvar, so
  another program has an approximation rather than nothing.
- The graph file and the layers stay light: neither holds the painted
  values.

## Rendering it

What paint is mostly for (scatter density, groom, crowds) is read in Imago,
and only what it made reaches USD. A painted shading mask is baked for the
renderers at export (Ptex or a UV texture, by the renderer's definition):
renderers do not read `.mcol`.

## The Viewer

First, each painted face drawn split into its grid of samples, with colours
at its points: exact, simple, fast enough for an asset being painted. Mesh
Color Textures (Yuksel 2017) only if that proves slow: the colours packed in
a texture the GPU filters (4D texture coordinates, triangles in pairs, mip
levels down to point colours), in a float or half-float format, so the
border texel of a diagonal edge (`c3 = c1 + c2 − c0`) is stored as it is,
values never changed to fit 0 to 1.

## The environment workflow

A ground dressed with a meadow reads: terrain (Load USD), Surface Masks,
Paint, Scatter (the prototypes from a library on its second input), Scatter
Session, Merge with the terrain, its material.

- **Surface Masks:** the procedural base, as ICE compounds: slope, height,
  curvature, cavity, noise, distance to a curve. Attributes on the ground a
  Paint channel can be laid over.
- **Paint:** its channels on the ground. **density** (Float) over the slope
  mask, multiplied; **biome** (ID: 1 grass, 2 fern, 3 rock, shown as a
  palette); others as wanted (wetness, a variant). The Viewer shows one
  channel at a time: a float as a heat map, an ID in its palette colours.
  Masking by the slope (the normal's angle) and by another channel.
- **Scatter:** one operator holding an ICE graph of compounds, shown as a
  list in its Parameters, as Groom Modify is: Scatter on Surface by density,
  the prototype by the biome ID, random scale, rotation and align to the
  normal, keep apart, cull by camera. Its output a PointInstancer. Each
  instance's ID comes from its place on the ground (the same hard rule as
  the groom's curves), so a repainted density keeps the instances that stay
  where they were.
- **Scatter Session:** placing by hand, as the Groom Session is for curves:
  brushes to place, move, rotate, scale, swap and delete instances, in
  layers by instance ID, with Explode and Freeze. Edits stay on the
  instances that remain when the scatter upstream changes.

## Order to do it in

1. **The shared brush and Paint in point mode:** a float and an ID channel;
   add, subtract, set, smooth, flood and erase, falloff and symmetry,
   masking by slope; the colours in the Viewer (a heat map, an ID palette);
   the `.mcol` file named by its content and written atomically; written to
   USD as primvars.
2. **Scatter reading it:** density and the biome ID into Scatter on Surface
   and the Instancer (both built), as the list-of-compounds operator; stable
   instance IDs.
3. **Layering** over an upstream attribute, with the blend modes, and the
   Surface Masks compounds.
4. **Scatter Session:** placing, moving and deleting instances by brush.
5. **Mesh Colors:** the data, reading at places in ICE, the brush on them,
   the split faces in the Viewer.
6. **The other channels** (colour, direction), **Mesh Color Textures** if
   needed, **baking** for the renderers.

## Left for later, or to find out

- **Distance along the surface** (geodesic), to stop paint crossing thin
  gaps: a project of its own. Masking by the normal's angle stops most of it.
- **Tablet pressure:** winit may not pass a pen's pressure on macOS; to
  check before it is promised.

## References

- Yuksel, Keyser, House. "Mesh Colors." ACM Transactions on Graphics 29(2),
  2010.
- Yuksel. "Mesh Color Textures." HPG 2017.
  <https://www.cemyuksel.com/research/meshcolors/mesh_color_textures.pdf>
