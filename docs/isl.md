# ISL: the ICE Scripting Language

ISL is Imago's language for ICE: code written in a **Snippet**, run once per
point, face, corner or object, as an ICE graph is. It has **OSL's syntax,
types and standard library**, with **ICE's model of geometry**: attributes
(`@P`), contexts, and reading along topology and on other prims. Its own
functions are named after the ICE nodes they match (`get_data`,
`near_points`), so a snippet and the graph of nodes doing the same read
alike.

It is for what is awkward as nodes: a formula, a condition, a chain of maths
too long to wire.

```c
snippet inflate (
    float amount = 0.1 [[ float min = 0, float max = 2, string label = "Amount" ]],
    float freq   = 4   [[ string label = "Noise frequency" ]],
    output float pushed = 0
)
{
    pushed = amount * noise(@P * freq);   // OSL's noise: 0 to 1
    @P += normalize(@N) * pushed;
}
```

The functions: [ISL functions](isl-functions.md).

## Where it runs

- **The Snippet node,** in an ICE graph (the add menu's Snippets): its
  parameters are its input ports, its `output` parameters its output ports,
  then an **Execute** for the root, the attributes it sets. Wire into a port
  and the wire takes the parameter's place.
- **ISnippet,** in the scene graph: an ICE operator holding one snippet,
  locked inside. Its code, Run Over, References and parameters are in its
  Parameters tab; its reference inputs are what its `geometry` parameters
  read.

In both, **Run Over** chooses the context the body runs over (the graph's
own unless set). The code is applied with **Ctrl+Enter** (Cmd+Enter on
macOS) or by leaving the editor; what is wrong shows under it, with its line
and column, and on the node. When its parameters change, the wires of those
still there stay, by name.

**Make Compound** makes a snippet a compound, its free parameters its ports,
to save as a definition and share. **Make ICE Graph** (an ISnippet's) puts an
ICreationEngine beside it holding the nodes the snippet runs, wired as it is.

## How it runs

A snippet is not run by an interpreter. It is parsed, type-checked and
**lowered into ICE's own nodes**, compiled with the rest of the graph, and
run as any ICE graph is: in batches, in parallel, reading only the attributes
it uses and writing only those it sets. A snippet costs what the same nodes
cost; Make ICE Graph shows exactly those nodes.

So:

- **Both sides of an `if` are worked out** for every element, and an If
  picks between them, element by element. An expensive side costs the same
  whether taken or not.
- **A loop is worked through when compiling,** as many times as it runs: its
  count must be known then (at most 1,024).
- **A function is worked through where it is called.**

## A snippet

```c
float twice(float x) { return x * 2; }    // functions, before it

snippet name [[ metadata ]] ( parameters )
{
    statements
}
```

### Parameters

`type name = default [[ metadata ]]`, separated by commas.

| Parameter | Is | Example |
|---|---|---|
| `type name = value` | an input port, its default the port's value | `float amount = 0.1` |
| `output type name = value` | an output port, a value per element | `output int big = 0` |
| `geometry name = n` | a Geometry port: the prims on reference input `n` (1 when not written) | `geometry ground = 1` |
| `geometry name = get_reference(n, "paths"…)` | the same, some of them | `geometry rocks = get_reference(1, "/set/rocks*")` |

Every input but a geometry needs its default. Two parameters have two names.

### Metadata

As OSL writes it: `[[ type name = value, … ]]`, after the parameter (or the
snippet's name).

| Metadata | On | Does |
|---|---|---|
| `string label = "Amount"` | a parameter | its name in the parameters |
| `float min = 0`, `float max = 1` | a parameter | its range |
| `string help = "…"` | a parameter | its tooltip |
| `string context = "face"` | the snippet | pins its context: Run Over set to another is an error |

### Contexts

The body runs once per element of its context: **point**, **face**,
**corner** or **object** (once for the primitive). `@` reads and writes the
element's attributes in it; `index()` is the element's number.

## Data types

| Type | Is | Notes |
|---|---|---|
| `int` | a whole number | an int meeting a float is a float |
| `float` | a number | |
| `point`, `vector`, `normal`, `color` | three numbers | distinct to `transform`: a point is moved, a vector turned, a normal turned by the inverse transpose |
| `vector2`, `vector4` | two, four numbers | `vector2` is a Param UV |
| `quat` | a rotation | |
| `matrix` | a 4×4 matrix | |
| `geometry` | prims on a reference input | parameters only |
| `string` | text | only as an argument: a context, a resample, a noise's name, a path |
| `type name[]` | a list | per element (what `get_array_data` gives), or one in an object snippet |

There is no boolean: a comparison is an `int`, 1 or 0, as in OSL; a number
is true where it is not 0.

### Constructors

| Written | Gives |
|---|---|
| `point(0)`, `color(0.5)` | all three parts the same |
| `point(x, y, z)`, `color(r, g, b)` | three parts |
| `vector2(u, v)`, `vector4(x, y, z, w)` | two, four parts |
| `quat(x, y, z, w)` | a rotation, its parts |
| `matrix(1)` | the identity (times the number) |
| `float(i)` | an int as a float |

### Parts and items

| Written | Reads |
|---|---|
| `p.x`, `p.y`, `p.z`, `p.w` | a part of a vector |
| `c.r`, `c.g`, `c.b`, `c.a` | the same, by a colour's names |
| `p[0]`, `p[1]`, `p[2]` | a part, by a number written there |
| `a[i]` | an item of a list |

A part is set as a variable is: `p.y = 0;`, `@P.y += 1;`.

### Lists

Maths on a list works item by item: `pts * 2`, `r > 0.5` (a list of 1 and 0),
`distance(pts, @P)`. A single value meeting a list goes with every item.
This is ISL's, not OSL's, where arrays take no maths. A list is summed,
averaged or counted with `sum`, `average` and `arraylength`.

## Variables

```c
float x = 1, y;          // y is 0
point pts[] = get_array_data(@P, "point");
int near[];              // a list to be given (near_points' output)
```

A variable lives in its block (`{ … }`) and the blocks inside it. An
assignment converts to the variable's type: an int into a float, a number
into a vector (all its parts).

## Attributes

| Written | Does |
|---|---|
| `@P` | the element's `P` |
| `@P = v;`, `@P += v;` | sets it, for this element |
| `vector @vel;` | declares it: its type, for an attribute made here or not on the geometry |
| `float @mask = 0;` | declares it and sets it |
| `@intrinsic:area` | an intrinsic, read only |

- **Types come from the geometry,** or from a declaration. A declaration that
  disagrees with the geometry is an error, as is reading an attribute that is
  neither there nor declared.
- **A new attribute's type is its value's,** but `@kept = 1;` is an error:
  1 could be an int or a float. Declare it: `int @kept;`, `float @kept;`.
- **Declared and not on the geometry,** an attribute reads as zero until
  set. Set in only one side of an `if`, it keeps that value elsewhere.
- **Intrinsics** are read by their names (`@intrinsic:ptIndex`,
  `@intrinsic:area`, `@intrinsic:ptIndices`); setting one is an error.
- **When a write is seen:** the element's own reads after it see it (`@P +=
  …;` then `@P` is the moved point). Reads of other elements (`get_data`,
  `get_array_data`, `near_points`) see the values from before the snippet
  ran: every element's writes land together at the end, as Set Data's do.
- **Another context's attribute** is read with `get_data` or
  `get_array_data`, never by `@`: `@N` in a face snippet, where `N` is per
  point, is an error saying `get_data(@N, "point")`.
- **A snippet writes in its own context,** to its own element. It does not
  add or remove elements, nor write another's: Scatter on Surface, Set Data
  into another context and the operators do that.

## References: other prims

A `geometry` parameter reads prims on one of the operator's numbered
**Reference** inputs (these are ICE's references, not USD's reference arcs):

```c
snippet stick (geometry ground = 1, float weight = 1)
{
    int face; vector2 uv; point hit;
    if (closest_point_on_surface(@P, ground, face, uv, hit))
        @P = mix(@P, hit, weight);
}
```

- **Its default is its input:** `geometry ground = 1`, `geometry anim = 2`;
  1 when not written. Only that input's prims are searched and read, though
  another input has prims at the same paths.
- **Its References rows,** in the parameters, name some of its prims, as Get
  Reference's rows do (`*`, `?`, `**`); none, all of them.
- **Wired,** from a Get Reference (or anything giving paths), the wire
  decides.
- **In the body, its name is the reference,** passed last to the functions
  that read other prims: `get_data(@P, rest)`, `get_data(@P, "point", i,
  rest)`, `closest_point_on_surface(@P, ground, …)`, `near_points(@P, 8, 0,
  crowd, …)`.
- **Several prims:** the searches (`closest_point_on_surface`,
  `near_points`) search them all; reading element by element (`get_data(@P,
  rest)`, the same topology) needs one, and as many elements as the snippet's
  geometry; reading at an index reads the first.
- **Positions, normals and vectors** read on another prim come into the
  graph's space.

## Operators

Highest first.

| Operators | Are |
|---|---|
| `f(…)`, `a[i]`, `p.x` | call, item, part |
| `-x`, `!x` | negation, not |
| `*`, `/`, `%` | times, divided by, remainder (as `mod`) |
| `+`, `-` | plus, minus |
| `<`, `<=`, `>`, `>=` | comparisons of numbers |
| `==`, `!=` | equal, not equal (numbers, vectors) |
| `&&` | and |
| `\|\|` | or |
| `c ? a : b` | a where c holds, else b |
| `=`, `+=`, `-=`, `*=`, `/=` | assignment (statements) |
| `x++`, `x--` | one more, one less (statements) |

- **An int with a float** is a float; **a number with a vector** is spread
  over it (`@P + 1` adds 1 to each part); vectors of different sizes do not
  mix.
- **Roles:** a point less a point is a vector; a point plus a vector is a
  point.
- **`&&` and `||` work out both sides:** nothing has a side effect, so the
  result is the same.
- **Division of ints** is an int's; by 0, 0.

## Statements

```c
x = 1;  x += 2;  x++;
if (c) a; else b;
if (c) { … } else if (d) { … }
for (int i = 0; i < 4; i++) { … }
return x;       // in a function
// a comment
/* a comment */
```

- **`for`'s count must be known when compiling:** written in the text (`i <
  4`), not a parameter, which is a port a wire can change. Loops over a count
  per element, and `while`, are not in ISL yet.
- **Functions** are written before the snippet, OSL style, with `output`
  parameters for what they give back besides their value:

  ```c
  float bump(float x) { return x * x * (3 - 2 * x); }
  void scale(output vector v, float k) { v = v * k; }
  ```

  `return` ends a function at its last statement (not inside an `if`).
  A function calling itself is an error.

## Reserved words

`snippet`, `output`, `if`, `else`, `for`, `while`, `return`, and the types:
`int`, `float`, `point`, `vector`, `normal`, `color`, `vector2`, `vector4`,
`quat`, `matrix`, `string`, `geometry`, `void`.

## Errors

Every error names a line and a column, and what is wrong: an unknown
attribute, a type that does not fit, a context that is not one, a loop whose
count is not known. They show under the code and on the node; an error the
engine finds in what a snippet became is shown at the line it came from.

## Examples

A face snippet, its context pinned, reading its points and an intrinsic:

```c
snippet face_centres [[ string context = "face" ]] (
    float threshold = 0.5,
    color paint = color(1, 0, 0),
    output int big = 0
)
{
    point @centre = get_data(@P, "point");          // its points, averaged
    point pts[] = get_array_data(@P, "point");      // its points, as a list
    float @spread = average(distance(pts, @centre)); // item by item
    big = @intrinsic:area > threshold;
    if (big)
        @Cd = paint;
}
```

Each point moved towards the middle of its nearest points:

```c
snippet relax (float weight = 0.5, int count = 6, float radius = 0)
{
    int near[];
    float d[];
    near_points(@P, count, radius, near, d);
    point mean = average(get_data(@P, near));   // P at each of their numbers
    @P = mix(@P, mean, weight);
}
```

A loop worked through when compiling, and a condition:

```c
snippet steps (float k = 0.5)
{
    float total = 0;
    for (int i = 0; i < 3; i++)
        total += k;
    if (@P.x > 1.5)
        @P.y += total;
    else
        @P.z = noise("perlin", @P);
}
```

The add menu has more (Snippet: roughen, relax, stick), and Templates >
Martino > ICE two scenes of ISnippets.

## Not in ISL yet

- loops over a count per element, and `while`;
- noise of one, two or four numbers, and vector noise;
- `min` and `max` of a list, `range`, lists made in a snippet;
- `int()` of a float, and a corner's own number (`index()` per corner).

## Credits

ISL's noises are ported from [Open Shading
Language](https://github.com/AcademySoftwareFoundation/OpenShadingLanguage)
(BSD-3-Clause, Contributors to the Open Shading Language project).
