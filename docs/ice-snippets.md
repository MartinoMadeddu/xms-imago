# XMS | Imago: ICE snippets (design and build instructions, v4)

*v1 agreed 8 November 2026; v2 10 November 2026, after a review against the
engine as it now is (Near Points, list attributes, intrinsics); v3 the same
day: references (other geometry) and two snippets, a node in ICE graphs and an
operator in the scene graph, with prototypes of both; v4 calls other geometry
by Imago's own word, references (it was "templates" in v3). What changed is
listed at the end. For the agent building it: read this whole file, then
`docs/ice.md`, `docs/compounds.md`, `LLM_instructions.md` and
`docs/vocabulary.md` in the repository, and the engine in `crates/xms-ice`
(`graph.rs`, `compile.rs`, `exec.rs`) before writing code. Put a copy in the
repository as `docs/ice-snippets.md`.*

## Where it is

Being built on the `core/ice-snippets` branch.

- **Built:**
  - the engine's Logic (comparisons, And, Or, Not, If) and OSL's maths
    (Square Root, Power, Exponential, Logarithm, Floor, Ceiling, Round, Sign,
    Modulo, Tangent, Arcsine, Arccosine, Arctangent of Y over X), nodes too;
  - the language (`crates/xms-ice/src/lang`): the lexer, the parser, and the
    lowering into the engine's own nodes, with the types, the attributes, `if`
    joined by If nodes, constant loops worked through, functions inlined, the
    ICE nodes as functions, and errors with their line and column;
  - the **Snippet node** in ICE graphs: its ports from its signature (wires
    kept by name when it changes), Run Over, the text with line numbers,
    applied with Ctrl+Enter (or Cmd+Enter), its errors under it; a
    **References** row per unwired `geometry` parameter (a Get Reference's:
    Reference 1, all its prims, unless set); **Make Compound**, its free
    parameters the compound's ports;
  - **noise** matched to OSL's: `noise("perlin" | "uperlin" | "simplex" |
    "usimplex" | "cell", p)`, `noise(p)` (OSL's default, uperlin), `snoise(p)`,
    `cellnoise(p)`; Perlin and cell checked against OSL's own expected values.
    Nodes too: Perlin Noise, Simplex Noise, Cell Noise;
  - **ISnippet**, the snippet operator in the scene graph (named as
    ICreationEngine is; yellow, as the Snippet node is): one Snippet node,
    locked inside, its Parameters tab the snippet's (Run Over, code,
    References, parameters). **Make ICE Graph** puts an ICreationEngine
    beside it, wired as it is, holding the nodes the snippet runs (each
    parameter a Constant named as it, each reference a Get Reference). This
    replaces v4's "Open as ICE graph" and its flag: an ISnippet is never
    opened, so it stays a snippet;
  - examples: Snippet: roughen (noise), relax (near points), stick (closest
    point), in the ICE add menu; Templates > Martino > ICE > Snippets: roughen
    and relax, stick to a surface.
- **Not yet:**
  - loops over a count per element, and `while` (build step 6: when real
    snippets need them);
  - noise of a number, of two or of four (OSL's 1D, 2D and 4D), and vector
    noise;
  - `min` and `max` of a list, `range`, arrays made in a snippet;
  - an attribute of another context read by `@` where each element touches
    one (a corner's point): `get_data(@x, "point")` reads it;
  - roles carried into what Set Data writes (an attribute keeps the geometry's
    role, or none when new);
  - a corner's own number (`index()` per corner), `int()` of a float.
- **Noise names are OSL's:** `"perlin"` and `"simplex"` are signed (about
  -1 to 1), `"uperlin"` and `"usimplex"` from 0 to 1; the design's examples
  written before this used `"perlin"` as if from 0 to 1.
- **Masked stores are not an engine instruction:** an attribute set under an
  `if` is an If of the new and the old value, made by the lowering. Nothing
  per element is added that the nodes would not add.

## The aim

A small **inline language for ICE graphs**, written as text in a **Snippet**,
which comes two ways: a **node in ICE graphs**, and an **operator in the scene
graph** (Houdini's Attribute Wrangle, Imago's way). The language is **OSL's syntax, types and standard library**, with **ICE's model of
geometry** added (attributes, contexts, reading along topology), its
functions named and shaped as the ICE nodes are. It is for what is awkward as
nodes: a formula, a condition, a chain of maths too long to wire.

**A snippet is not a second engine.** It is parsed, type-checked and
**lowered to the same engine graph** the nodes make, then compiled and run by
`xms-ice` as any ICE graph is: batched, parallel, reading only the attributes
it uses and writing only those it sets.

## Decisions (settled)

1. **OSL's syntax and types.** A header with typed parameters, defaults and
   `[[ ]]` metadata; `output` parameters; C-like statements; `point`,
   `vector`, `normal`, `color` as distinct types; comparisons give an `int`
   (OSL has no `bool`).
2. **Where functions get their names**, in this order:
   - **OSL's name and signature** where OSL has the function (`noise`,
     `smoothstep`, `arraylength`, `transform`).
   - **The ICE node's name**, in snake case, with its inputs as arguments in
     the node's order, where ICE has it and OSL does not (`get_data`,
     `get_array_data`, `near_points`, `random`). A snippet and a graph of
     nodes then read the same, and the docs of one serve the other.
   - **VEX names nothing.** It lends ideas only: `@` for attributes, typed
     attribute declarations, Run Over on the node.
3. **ICE's model of geometry:**
   - **`@name` is a geometry attribute** of the snippet's context, read or
     written anywhere in the body.
   - **The context is the node's**, as a Wrangle's Run Over: the body runs
     once per element of it.
   - **Reading from another context is always explicit**, through
     `get_data` and `get_array_data`, never a silent average.
   - **Intrinsics** are read as attributes, by their registry names
     (`@intrinsic:area`).
4. **The signature is the interface:** parameters become the Snippet node's
   input ports (with their defaults and metadata); `output` parameters become
   its output ports. Attributes are not in the signature.
5. **Other geometry comes in as ICE's does,** never by number: a `geometry`
   parameter is a Geometry port, wired from Get Reference or filled in from
   the node's own rows, and named in the text (`get_data(@P, rest)`). The
   operator's numbered Reference inputs are where references come from.
6. **Two snippets, one language:** the Snippet node in ICE graphs, and the
   Snippet operator in the scene graph, which is an ICreationEngine operator
   holding one Snippet node, made ready to use.
7. **Lowered to the engine graph.** No interpreter, no second runtime. The
   engine gains what the language needs (below), which serves the nodes too.

## The language

### A snippet

```c
snippet <name> [[ <metadata> ]] ( <parameters> )
{
    <statements>
}
```

- **The context** is the Snippet node's **Run Over** dropdown (Point, Face,
  Corner, Object), defaulting to the graph's own. The same text can run over
  points or faces. A snippet that only makes sense in one context pins it in
  its metadata, OSL style: `snippet inflate [[ string context = "point" ]]
  (…)`; the dropdown is then fixed to it. A snippet saved as a compound always
  carries its context.
- **Parameters:** `type name = default [[ metadata ]]`, separated by commas.
  - **An input** becomes an input port, its default the port's own value.
  - **`output type name = default`** becomes an output port, a value per
    element of the context.
  - **Metadata,** as OSL writes it: `[[ float min = 0, float max = 1, string
    label = "Amount", string help = "…" ]]`.

### Types

| Type | Engine type | Notes |
|---|---|---|
| `int`, `float` | `Int`, `Float` | an `int` meeting a `float` becomes a `float`; comparisons give an `int` |
| `point`, `vector`, `normal`, `color` | `Vec3` with a role | distinct to the type checker; `transform` treats each as OSL does |
| `vector2`, `vector4` | `Vec2`, `Vec4` | as ICE's 2D and 4D vectors |
| `quat` | `Quat` | rotations, as the engine's |
| `matrix` | `Matrix` | from the start: the engine has it (Transform Point, Invert, Transpose) |
| `string` | `Token` | names, paths, and the context and resample arguments |
| `geometry` | an array of paths | prims to read from (a reference): Get Reference's output; parameters only, never made in the body except by `get_reference` |
| `T name[]` | a list of `T` | a list per element (what Get Array Data gives), or one array in an Object snippet |

- **Constructors:** `point(0)`, `point(x, y, z)`, `color(1, 0, 0)`,
  `matrix(1)`.
- **Components:** `p[0]`, and `p.x`, `p.y`, `p.z` (`c.r`, `c.g`, `c.b` for
  colours).
- **Arrays:** `a[i]`, `arraylength(a)` (OSL's name).
- **Arrays in maths, a departure from OSL:** maths on arrays works item by
  item (`r > 0.5` on a `float[]` gives an `int[]`), as the engine already does
  for lists. OSL does not allow it; the snippet language does, on purpose.

### Attributes

- **`@name`** reads the element's value of an attribute of the snippet's
  context; assigning to it sets it per element (`@P += n * 0.1`).
- **Declaring an attribute's type** (VEX's idea, OSL's spelling), in the
  body: `vector @vel;`, or `float @mask = 0;`, which also sets it, as VEX's
  `f@mask = 0` does. Declared and not on the geometry, an attribute reads as
  zero until set.
  - **Optional when reading**: the type then comes from the geometry, as
    `output_types` does now.
  - **Required when making an attribute whose type is ambiguous** from its
    expression: `@kept = 1` is an error asking for `int @kept` or
    `float @kept` (VEX's int-or-float trap, refused rather than guessed).
  - **A declaration that disagrees with the geometry** is an error naming the
    attribute, the declared and the found type, at its line.
- **Reading an attribute that doesn't exist**, undeclared, is an error naming
  it, at its line.
- **Intrinsics** by their names in the registry (`xms_geo::intrinsics`):
  `@intrinsic:ptIndex`, `@intrinsic:area`, `@intrinsic:pointCount`. Read
  only: assigning to one is an error, as Set Data refuses them.
- **When a write is seen:**
  - **The element's own reads** after a write see the new value (`@P += …;`
    then `@P` is the moved point).
  - **Reads of other elements** (`get_data`, `get_array_data`, `near_points`)
    see the values from before the snippet ran: every element's writes land
    together at the end, as Set Data's do.
- **An attribute of another context** is not read by `@` directly. `@N` in a
  face snippet, where `N` is per point, is an error that suggests
  `get_data(@N, "point")`. The one exception: where each element touches
  exactly one element of the other context (a corner's point, a corner's
  face), `@` reads it directly; nothing is combined there.

### Reading geometry: the ICE nodes as functions

Each is the ICE node of the same name. The attribute argument is written
`@name`: it names the attribute (and its declared type), it is not read in
the snippet's context. A context argument is a string literal of a context
name (`"point"`, `"face"`, `"corner"`, `"object"`); a resample argument one of
`"average"` (the default), `"sum"`, `"min"`, `"max"`. Both must be literals, so
they are checked when the snippet compiles.

| Function | ICE node | Gives |
|---|---|---|
| `get_data(@x, "point")` | Get Data, its context set | the values the element touches in that context, made one by Average |
| `get_data(@x, "point", "max")` | Get Data, context and Resample set | the same, made one by Max (or Sum, Min) |
| `get_data(@x, int index)` | Get Data at an Index | the value of the element numbered `index` in `x`'s own context (VEX's `point(0, "P", i)`) |
| `get_data(@x, int face, vector2 uv)` | Get Data at a Face and Param UV | the value at a place on a face |
| `get_array_data(@x, "point")` | Get Array Data, its context set | the values it touches, as a list, in order (a face's points' positions) |
| `get_array_data(@x, int index)` | Get Array Data at an Index | the list the numbered element holds |
| `near_points(point origin, int count, float radius, [geometry of,] output int points[], output float distances[])` | Near Points | the nearest points, up to `count`, within `radius` (0: anywhere); returns how many it found |
| `closest_point_on_surface(point origin, [geometry on,] output int face, output vector2 uv, output point position)` | Closest Point on Surface | the nearest place on the graph's own surface, or on `on`'s meshes and curves; returns 1 if found |
| `get_reference(int input, string paths…)` | Get Reference | a `geometry`: the prims on Reference input `input` its paths or patterns name (`"/assets/rock*"`) |
| `random(min, max, int seed)` | Random | a value between `min` and `max`, the same each time for the element and seed; `float`, `vector` and `color` overloads |
| `random_orientation(normal n, int seed)` | Random Orientation | a `quat` turning +Y along `n` |

- **Arguments follow the node's input ports**, in order; outputs beyond the
  first are OSL `output` arguments, as OSL returns several values.
- **Element numbers** come from intrinsics: a face's points are
  `get_array_data(@intrinsic:ptIndex, "point")`, or the face's own
  `@intrinsic:ptIndices`.
- **A Geometry input** is a last argument, a `geometry` value: `get_data(@P,
  rest)`, `get_data(@P, "point", i, rest)`, `closest_point_on_surface(@P,
  ground, …)`. See References, below.
- **Neighbours along edges** have no ICE node yet. When the node comes, so
  does its function, by the node's name.

### Built-in functions

**OSL's standard library names and signatures**, where they make sense on
geometry:
- **Maths:** `abs`, `sign`, `floor`, `ceil`, `round`, `mod`, `fmod`, `pow`,
  `exp`, `log`, `sqrt`, `min`, `max`, `clamp`, `mix`, `step`, `smoothstep`,
  `radians`, `degrees`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan2`.
- **Vectors and matrices:** `dot`, `cross`, `length`, `distance`,
  `normalize`, `rotate`, `transform` (by role: a `point` moves, a `vector`
  does not, a `normal` takes the inverse transpose), `inverse`, `transpose`.
- **Noise:** `noise("perlin" | "simplex" | "cell", p)`, `snoise`, matching
  OSL's implementation **bit for bit**, so a mask made in ICE matches the
  same noise in an OSL shader in the renderer. The engine has no noise yet:
  this is new work, and worth doing exactly.
- **Colour:** `luminance`.
- **Arrays:** `arraylength` (OSL); `sum`, `average` (ICE's array nodes);
  `min`, `max` over an array; `range(n)`, the list `0 … n-1` (the language's
  own: neither OSL nor ICE has it).
- **The element:** `index()`, the element's number in the snippet's context
  (the context's `intrinsic:ptIndex` or `intrinsic:faceIndex`, for text that
  runs over any context).
- **Time:** `time` and `frame`, as OSL's `time` global.

### Statements

- **Declarations and assignment:** `float x = 1;`, `x = x * 2;`, `x += 1;`,
  `-=`, `*=`, `/=`.
- **`if` / `else`**, and the ternary `c ? a : b`.
- **`for` with a constant count**, unrolled: `for (int i = 0; i < 4; i++)`.
  A loop over an array (`i < arraylength(a)`) and `while` come with the
  engine's loop instruction (build step 5), not before.
- **Comparisons and logic:** `<`, `<=`, `>`, `>=`, `==`, `!=`, `&&`, `||`,
  `!`. Both sides of `&&` and `||` are computed (nothing has side effects,
  so the result is the same).
- **User functions** defined before the snippet, OSL style (`float bump(float
  x) { … }`), inlined; recursion is an error.
- **Comments:** `//` and `/* */`.

### References: other geometry

A snippet reads other prims (a rest mesh, a ground to stick to, guides) as the
ICE nodes do, and calls them what ICE calls them: **references**, the prims
on the operator's Reference inputs (not USD's reference arcs). It reads them through a **Geometry** input: an array of paths, Get
Reference's, on one of the ICreationEngine operator's numbered Reference
inputs. A snippet adds nothing to that machinery; it gives it a name.

- **A `geometry` parameter** (`geometry rest [[ string label = "Rest" ]]`) is
  a **Geometry input port** on the Snippet node.
  - **Wired:** from a Get Reference, or anything giving paths (Concatenate
    Arrays of two), or promoted to a compound.
  - **Unwired:** its rows are shown in the node's **References** tab, as Get
    Reference's are: the Reference input (a dropdown of the operator's), and
    paths or patterns (`*`, `?`, `**`), picked from the prims on that input,
    added from the Scene Explorer's selection, or dropped from it. Empty: the
    graph's own primitive.
- **In the text, the parameter's name is the reference,** passed as a last
  argument. `get_reference(1, "/rest")` makes one in the text, mirroring the
  node; the number appears there, as on Get Reference, and nowhere else.
- **No reference by number** (VEX's `point(1, "P", i)`): a number changes
  meaning when inputs are rewired, and nothing fails. A name is checked when
  the snippet compiles, shows on the port, and survives rewiring.
- **Several prims in a reference:**
  - **Searches** (`closest_point_on_surface`, `near_points`) search them all,
    as the nodes do; the place found says which prim (its face is numbered in
    that prim, and `get_data(@x, face, uv, on)` reads it there).
  - **Reads element by element** (`get_data(@P, rest)`: same topology) need
    exactly one prim: more is an error naming the reference and how many it
    found.
  - **Reads at an index** (`get_data(@P, i, rest)`) read the first prim, as
    Get Data does now; the doc of each function says which.
- **Same topology:** an element-by-element read from a reference with another
  count of elements in that context is an error (the counts, the reference),
  never a clamp.
- **Space:** positions, normals and vectors read from a reference come into the
  graph's space (positions moved, normals and vectors turned), as Get Data's
  do; the graph's own attributes are as they are.

### What a snippet cannot do

- **Add or remove elements** (VEX's `addpoint`, `removeprim`): the engine
  runs over a fixed set of elements. Scatter on Surface and the operators do
  that, outside the snippet.
- **Write to another element** (VEX's `setpointattrib`): an element writes
  only its own attributes.
- **Write another context's attribute**: a snippet writes in its own
  context. A Set Data node into another context does the rest.

## Examples

A point snippet (Run Over: Point):

```c
snippet inflate (
    float amount = 0.1  [[ float min = 0, float max = 2, string label = "Amount" ]],
    float freq   = 4    [[ string label = "Noise frequency" ]],
    int   seed   = 0,
    output float pushed = 0
)
{
    float n = noise("perlin", @P * freq + seed);
    pushed  = amount * smoothstep(0.3, 0.8, n);
    @P     += normalize(@N) * pushed;
}
```

A face snippet, its context pinned, reading its points and an intrinsic:

```c
snippet face_centres [[ string context = "face" ]] (
    float threshold = 0.5,
    color paint = color(1, 0, 0),
    output int big = 0
)
{
    point @centre;                                // made here: declared
    @centre = get_data(@P, "point");              // its points, averaged
    point pts[] = get_array_data(@P, "point");    // its points, as a list
    float @spread = max(distance(pts, @centre));  // item by item, no loop
    big = @intrinsic:area > threshold;
    if (big)
        @Cd = paint;
}
```

A point snippet reading its neighbours by Near Points, smoothing:

```c
snippet relax (float weight = 0.5, int count = 8, float radius = 0)
{
    int near[];
    float d[];
    near_points(@P, count, radius, near, d);
    point mean = average(get_data(@P, near));         // the found points' P
    @P = mix(@P, mean, weight);
}
```

An object snippet, with arrays as one value:

```c
snippet keep_half [[ string context = "object" ]] (int count = 1000, int seed = 1)
{
    int ids[] = range(count);
    float r[] = random(0.0, 1.0, seed + ids);
    int @kept = sum(r > 0.5);
}
```

(`get_data(@P, near)` reads at each index of a list, as the engine's Get
Data at an Index does on a list. An array meeting one value in maths takes it
for every item: `distance(pts, @centre)`, `seed + ids`.)

## How it is lowered

1. **Parse** into a syntax tree, with each node's line and column. A
   hand-written lexer and a precedence-climbing parser: no parser crate.
2. **Type-check:** resolve names, attributes (declared, or against the
   geometry when compiling), overloads of built-ins, the context and resample
   literals, the implicit int-to-float rule and roles.
3. **To SSA:** every assignment to a variable or attribute becomes a new
   value; user functions are inlined.
4. **`if`:** both branches are computed for the batch, and values that differ
   are joined by a **select** on the condition. Attributes set inside a branch
   are stored **masked**, only where the condition held. An expensive branch
   costs the same whether taken or not; the docs say so.
5. **Constant-count loops** are unrolled. (Varying loops: build step 5.)
6. **Out:** an engine graph of the existing operations, plus the additions
   below, compiled by `xms_ice::compile` with the rest of the ICE graph.
   Constant hoisting, dead code removal and read and write analysis come for
   free.

**Errors** from parsing, typing and compiling all name a **line and column**
in the snippet, shown under the text in the Snippet node's editor, with the
line marked.

## What the engine needs

In `crates/xms-ice`; each also serves the nodes. Already there since v1, and
not to be built again: **lists per element** (Get Array Data, maths item by
item, the array nodes), **matrices**, **Near Points**, **Get Data at an
Index** and at a place, **intrinsics**.

1. **Comparisons, logic and select:** `Lt`, `Le`, `Gt`, `Ge`, `Eq`, `Ne`,
   `And`, `Or`, `Not`, `Select`. As nodes too: Compare, Logic, If.
2. **Masked stores:** Set Data writes only where a mask is true, keeping the
   old value elsewhere.
3. **A table of built-in functions** replacing the growing `Math` enum. Each
   entry gives name, signatures, kernel and constant-folding rule, and serves
   the nodes' maths menu and the language alike. **`Math` is saved in graph
   files**: the table reads the old names, and a test opens a graph saved
   before it.
4. **Noise**, OSL's Perlin, simplex and cell, matched against OSL's own
   output in tests.
5. **Positions:** each engine node can carry a source position, so
   `CompileError` maps back to a line and column.
6. **Roles:** `point`, `vector`, `normal`, `color` carried through the
   compile with `Vec3`, matching `xms-geo`'s attribute roles; `transform`
   uses them.
7. **Later, the loop instruction** (build step 5): a per-element iteration
   count and a mask of active elements, ending when every element has
   finished, with a **cap on iterations** (a setting, 10 000 by default) so one
   bad snippet cannot hang a cook; reaching it is an error at the loop's line.

## The Snippet node

- **In ICE graphs,** under its own menu entry: a node holding the snippet's
  text.
- **Run Over:** Point, Face, Corner, Object, defaulting to the graph's
  context; fixed when the text pins a context.
- **Its ports come from the signature:** an input per parameter (with its
  default and range), an output per `output` parameter, and an **Execute**
  output when the body sets attributes, so it goes into the root like Set
  Data.
- **Editing:** a text editor in the Parameters pane (monospace, line numbers,
  syntax colours), errors listed under it with their lines. The snippet
  compiles when the text is applied (Ctrl+Enter or leaving the editor), not on
  every keystroke.
- **Saved** in the ICE graph with its text and Run Over. Port values and
  connections are kept by parameter name when the signature changes.

## The Snippet operator (scene graph)

**Houdini's Attribute Wrangle, Imago's way.** An operator in the scene graph
with a code box, its own inputs, and nothing to wire inside. It is **an
ICreationEngine operator** holding one Snippet node, made ready to use: one
engine, one file format, one set of rules.

- **Inputs:** **Scene**, then numbered **Reference** inputs (Reference 1,
  Reference 2…), one always free, exactly the ICreationEngine operator's.
  The snippet's `geometry` parameters read from them.
- **Primitive Locations:** the prims it runs on, as the ICreationEngine
  operator's.
- **Run Over:** Point, Face, Corner, Object.
- **The code box,** as the node's: line numbers, syntax colours, errors under
  it with their lines, applied with Ctrl+Enter.
- **References tab:** a row per `geometry` parameter, its Reference input
  and its paths (the node's References tab, at the operator).
- **Parameters tab:** the signature's inputs, with their ranges and labels,
  keyframable and promotable as any operator's parameters.
- **Outputs:** the scene, the attributes the body sets written into it.
  `output` parameters have nowhere to go at the scene level: the operator
  warns, and suggests storing them as attributes.
- **Open as ICE graph** shows the graph inside (Root ← Snippet, each
  `geometry` parameter wired from a Get Reference). Adding a node there makes
  it an ICreationEngine operator like any other; the snippet stays a node in
  it. There is no way back, and none is needed.
- **Saved** as an ICreationEngine operator with a flag that it is a Snippet
  operator (how it is drawn and which tabs it shows), so an older Imago that
  knows ICE but not the flag still cooks it.

## Prototypes

How both could look, and what they hold. Sketches, not final layouts.

### The Snippet node, in an ICE graph

An ICE graph running over the points of `/set/cloth`. `rest` is wired from a
Get Reference; `ground` is filled in on the node's References tab, so it has no
wire.

```text
 ICE graph: /set/cloth                                  Run Over: Point

 ┌──────────────────────┐     ┌────────────────────────────────┐
 │ Get Reference        │     │ ⟨/⟩ Snippet: stick_to_rest     │
 │ Reference 1  /rest   ●─────● rest                           │
 └──────────────────────┘     ○ ground   (References tab)      │
                              ● weight                   1.00  │
                              │                                │    ┌──────────┐
                              │                      Execute ●─┼────● Root     │
                              │                                │    │ (Point)  │
                              └────────────────────────────────┘    └──────────┘
```

Its Parameters pane:

```text
 ┌ Snippet ─────────────────────────────────────────────────────────────┐
 │ [ Code ]  [ References ]  [ Parameters ]                             │
 │ Run Over  [ Point ▾ ]                                                │
 │ ┌──────────────────────────────────────────────────────────────────┐ │
 │ │  1  snippet stick_to_rest (                                      │ │
 │ │  2      geometry rest   [[ string label = "Rest" ]],             │ │
 │ │  3      geometry ground [[ string label = "Ground" ]],           │ │
 │ │  4      float weight = 1 [[ float min = 0, float max = 1 ]]      │ │
 │ │  5  )                                                            │ │
 │ │  6  {                                                            │ │
 │ │  7      vector delta = @P - get_data(@P, rest);                  │ │
 │ │  8      int face; vector2 uv; point hit;                         │ │
 │ │  9      closest_point_on_surface(@P, ground, face, uv, hit);     │ │
 │ │ 10      @P = mix(@P, hit + delta, weight);                       │ │
 │ │ 11  }                                                            │ │
 │ └───────────────────────────────────────────── Ctrl+Enter: apply ──┘ │
 │ ✓ compiled: reads P (rest, ground), writes P                         │
 └──────────────────────────────────────────────────────────────────────┘
```

The References tab, for `ground` (unwired):

```text
 │ [ Code ]  [ References ]  [ Parameters ]                             │
 │ Rest     wired: Get Reference (Reference 1, /rest)                   │
 │ Ground   Reference [ 2 ▾ ]                                           │
 │            /set/ground                         [ ✕ ]                 │
 │            /set/rocks/**                       [ ✕ ]                 │
 │            [ + path ]  [ + selected ]   (or drop from the Explorer)  │
```

An error, as it would show under the code:

```text
 │  7      vector delta = @P - get_data(@P, rest);                     │
 │                             ~~~~~~~~~~~~~~~~~~                      │
 │ ✖ line 7, col 25: rest names 3 prims; a read point by point needs   │
 │   one (/rest/body, /rest/head, /rest/tail)                          │
```

### The Snippet operator, in the scene graph

```text
     ┌──────────────────────┐        ┌──────────────────────┐
     │ Load USD  shot.usd   │        │ Load USD  rest.usd   │
     └──────────●───────────┘        └──────────●───────────┘
                │                               │
                ▼                               ▼
     ┌──────────●───────────────────────────────●──────────────○─────┐
     │        Scene                       Reference 1   Reference 2  │
     │                                                       (free)  │
     │   ⟨/⟩ Snippet: stick_to_rest                          Point   │
     └───────────────────────────────●───────────────────────────────┘
                                     │
                                     ▼
                           the scene, P moved
```

Its Parameters pane:

```text
 ┌ Snippet: stick_to_rest ──────────────────────────────────────────────┐
 │ [ Code ]  [ References ]  [ Parameters ]        [ Open as ICE graph ]│
 │ Primitive Locations   /set/cloth                                     │
 │ Run Over              [ Point ▾ ]                                    │
 │ ┌──────────────────────────────────────────────────────────────────┐ │
 │ │  (the same code box as the node's)                               │ │
 │ └──────────────────────────────────────────────────────────────────┘ │
 ├──────────────────────────────────────────────────────────────────────┤
 │ References                                                           │
 │ Rest     Reference [ 1 ▾ ]  /rest                                    │
 │ Ground   Reference [ 1 ▾ ]  /set/ground                              │
 ├──────────────────────────────────────────────────────────────────────┤
 │ Parameters                                                           │
 │ Weight   [■■■■■■■■■■■■■■■■■■■■] 1.00   ◆ (keyframe)                  │
 └──────────────────────────────────────────────────────────────────────┘
```

### What the operator is, inside

Pseudo code: what a Snippet operator builds and cooks. Nothing here is new
machinery; every piece exists, or is built for the Snippet node.

```text
SnippetOperator {
    text, run_over, prim_locations,
    references: { parameter name -> (reference input, paths[]) },
    values:     { parameter name -> value or curve },
}

cook(op, scene, references[]):
    snippet = parse_and_check(op.text)              // errors: line and column
    graph   = IceGraph::new(context = op.run_over)
    node    = graph.add(SnippetNode(op.text))
    for p in snippet.parameters:
        if p.type == geometry:
            (input, paths) = op.references[p.name]
            graph.wire(graph.add(GetReference(input, paths)), node.port(p.name))
        else:
            node.set(p.name, op.values[p.name] at the current frame)
    graph.wire(node.execute, graph.root)
    return ICreationEngine::cook(graph, scene, references, op.prim_locations)
```

And the node, when its graph compiles:

```text
SnippetNode::lower(text, run_over, ports) -> engine nodes:
    ast   = parse(text)                             // positions kept
    typed = check(ast, run_over, geometry types, references)
    ssa   = to_ssa(typed)                           // user functions inlined
    for each value: emit engine Op                  // Math, GetData{geometry},
                                                    // NearPoints, Select…
    for each @attribute set: emit SetData (masked inside an if)
    -> the graph's own nodes, compiled with the rest by xms_ice::compile
```

## Snippets as compounds

A snippet can be saved as an **ICE compound definition** (`.icd`, context
`Ice`, see `docs/compounds.md`), its parameters the compound's promoted
parameters and its context its own. It is then shared and versioned like any
compound, and listed in the ICE add menu.

## Build order

Each step leaves Imago working, with tests (`cargo test --workspace`).

1. **Engine:** comparisons, logic and select; masked stores; the built-in
   table (moving the current maths into it, old graphs still opening); roles;
   source positions. Nodes: Compare, Logic, If.
2. **Noise**, matched to OSL's.
3. **The front end** (a module in `crates/xms-ice`, `lang/`):
   - the lexer and parser, the type checker, lowering to the engine graph,
     errors with positions
   - tests:
     - every example above
     - each error case (unknown attribute, undeclared ambiguous attribute,
       declaration against the geometry, another context's attribute by `@`,
       a non-literal context, writing an intrinsic)
     - `if` with masked stores
     - constant loops, unrolled
     - writes seen by the element, not by others
     - references: a read by name, an unwired reference from its rows, several
       prims searched, several prims refused where one is needed, a topology
       that does not match
4. **The Snippet node,** its editor, Run Over, ports and References tab
   (`geometry` parameters, Get Reference's rows); **snippets as compounds.**
5. **The Snippet operator** in the scene graph: an ICreationEngine operator
   with one Snippet node, its tabs, Open as ICE graph. Tests: the prototype's
   stick_to_rest, cooked through the operator, and through an ICE graph wired
   by hand, give the same points.
6. **The loop instruction**, then `for` over arrays and `while` in the
   language, when real snippets show the need.

## Rules to follow

- **The repository's `LLM_instructions.md`.** In particular:
  - Rust: clean and idiomatic, less is better, prefer the standard library
  - **no thin delegating wrappers**
- **The vocabulary in `docs/vocabulary.md`:** attributes, not primvars;
  contexts Object, Point, Face, Corner; function names from the ICE nodes'
  names.
- **Speed:** the engine stays batched and fast. The language adds nothing per
  element that the nodes wouldn't, and the benchmark (`ice_engine_speed`) must
  not get slower.
- **Every change comes with** tests, docs (`docs/ice.md`,
  `docs/ice-snippets.md`) and a line in `LOGS.md`.
- **No "Co-Authored-By: Claude" line** in any commit.

## What changed from v1

- **Function names:** OSL's where OSL has the function, else the ICE node's
  (`get_points(@P)` is now `get_array_data(@P, "point")`, `len` is
  `arraylength`); VEX lends ideas, never names.
- **The context** moved from the text (`snippet name : point`) to the node's
  Run Over, with an optional pin in the snippet's metadata.
- **Another context's attribute** is no longer averaged silently by `@`:
  `get_data` with a context, or an error.
- **Typed attribute declarations** (`vector @vel;`), required where the type
  would be a guess.
- **When writes are seen** is stated: the element's own at once, others' at
  the end.
- **The engine's geometry nodes** as functions: Get Data at an Index and at a
  place, Near Points, Closest Point on Surface, Random Orientation.
- **Intrinsics** read by their registry names; `count()` dropped for them.
- **`matrix`** from the start; `transform` by role.
- **Noise** called out as new engine work, matched bit for bit to OSL.
- **What a snippet cannot do** stated (adding, removing, writing other
  elements).
- **Build order:** what the engine has since v1 taken out; the front end
  before loops; varying loops last, capped, and only when needed; the `Math`
  table keeping old graphs opening.

## What changed from v2

- **References:** a `geometry` type; a `geometry` parameter is a Geometry
  port, wired from Get Reference or filled in on the node's References tab
  (Get Reference's rows), and named in the text as a last argument. No
  reference by number; `get_reference(input, paths)` mirrors the node.
- **Rules for references** stated: several prims searched by searches and
  refused by element-by-element reads, a topology that does not match an
  error, reads taken into the graph's space.
- **Two snippets:** the Snippet node in ICE graphs, and the Snippet operator
  in the scene graph (an ICreationEngine operator holding one Snippet node:
  Scene and Reference inputs, Primitive Locations, Code, References and
  Parameters tabs, Open as ICE graph).
- **Prototypes** of both, and pseudo code of what the operator builds and how
  the node lowers.
- **Build order:** the References tab with the node; the operator its own
  step, before loops.

## What changed from v3

- **"Templates" are "references",** Imago's own word for them (the
  ICreationEngine operator's Reference inputs, Get Reference): the
  References section, the node's and the operator's **References** tab, and
  every mention in the text, prototypes and pseudo code. Nothing else
  changed.
