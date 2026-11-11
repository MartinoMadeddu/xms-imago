# ISL functions

Every function of [ISL](isl.md), by kind. Where OSL has the function, it has
OSL's name and arguments; ISL's own are named after the ICE node they match,
their arguments in the order of its ports. A type `T` takes any of `int`,
`float` and the vectors, the same for every `T` of a call; a function given a
list works item by item, and gives a list.

| Kind | Functions |
|---|---|
| [Constructors](#constructors) | [point, vector, normal, color](#point-vector-normal-color), [vector2, vector4](#vector2-vector4), [quat](#quat), [matrix](#matrix), [float](#float) |
| [Maths](#maths) | [abs](#abs), [sign](#sign), [floor](#floor), [ceil](#ceil), [round](#round), [sqrt](#sqrt), [pow](#pow), [exp](#exp), [log](#log), [mod](#mod), [fmod](#fmod), [min](#min), [max](#max), [clamp](#clamp) |
| [Trigonometry](#trigonometry) | [sin](#sin), [cos](#cos), [tan](#tan), [asin](#asin), [acos](#acos), [atan2](#atan2), [radians](#radians), [degrees](#degrees) |
| [Interpolation](#interpolation) | [mix](#mix), [step](#step), [smoothstep](#smoothstep) |
| [Vectors and matrices](#vectors-and-matrices) | [dot](#dot), [cross](#cross), [length](#length), [distance](#distance), [normalize](#normalize), [transform](#transform), [rotate](#rotate), [inverse](#inverse), [transpose](#transpose) |
| [Colour](#colour) | [luminance](#luminance) |
| [Noise](#noise) | [noise](#noise-1), [snoise](#snoise), [cellnoise](#cellnoise) |
| [Random](#random) | [random](#random-1), [random_orientation](#random_orientation) |
| [Lists](#lists) | [arraylength](#arraylength), [sum](#sum), [average](#average) |
| [Geometry](#geometry) | [get_data](#get_data), [get_array_data](#get_array_data), [near_points](#near_points), [closest_point_on_surface](#closest_point_on_surface), [get_reference](#get_reference), [index](#index) |

## Constructors

### point, vector, normal, color

```c
point point(float x)
point point(float x, float y, float z)
```

Three numbers: all the same, or each given. `vector`, `normal` and `color`
are written the same way; their role decides what [transform](#transform)
does.

```c
point origin = point(0);
color red = color(1, 0, 0);
@P = point(@P.x, 0, @P.z);       // flattened onto the ground
```

### vector2, vector4

```c
vector2 vector2(float u, float v)
vector4 vector4(float x, float y, float z, float w)
```

Two or four numbers; one number gives all the parts. A `vector2` is a Param
UV.

```c
vector2 middle = vector2(0.5, 0.5);
```

### quat

```c
quat quat(float x, float y, float z, float w)
```

A rotation by its parts (w last: `quat(0, 0, 0, 1)` turns nothing).

### matrix

```c
matrix matrix(float k)
```

The identity times `k`, known when compiling: `matrix(1)` is the identity.

```c
snippet move (matrix m = matrix(1)) { @P = transform(m, @P); }
```

### float

```c
float float(int i)
```

An int as a float.

```c
float half = float(@intrinsic:ptIndex) / 2;
```

## Maths

### abs

```c
T abs(T x)
```

The size of `x`, each part's for a vector.

```c
float far = abs(@P.y);
```

### sign

```c
float sign(float x)
```

-1, 0 or 1, by the sign of `x` (0 at 0, as OSL's).

### floor

```c
float floor(float x)
```

The whole number at or below `x`.

```c
@P.y = floor(@P.y * 4) / 4;      // steps a quarter high
```

### ceil

```c
float ceil(float x)
```

The whole number at or above `x`.

### round

```c
float round(float x)
```

The nearest whole number; halves away from 0.

### sqrt

```c
float sqrt(float x)
```

The square root.

### pow

```c
float pow(float x, float y)
```

`x` to the power of `y`.

```c
float falloff = pow(1 - t, 3);
```

### exp

```c
float exp(float x)
```

e to the power of `x`.

### log

```c
float log(float x)
```

The natural logarithm.

### mod

```c
T mod(T a, T b)
```

The remainder of `a` divided by `b`, with `b`'s sign: never negative for a
positive `b` (OSL's `mod`; `a % b` is the same). Of ints or floats.

```c
int every_third = mod(@intrinsic:ptIndex, 3) == 0;
```

### fmod

```c
float fmod(float a, float b)
```

[mod](#mod) of floats.

### min

```c
T min(T a, T b)
```

The smaller, each part's for vectors. (Of a list: not yet.)

### max

```c
T max(T a, T b)
```

The larger, each part's for vectors.

```c
@P.y = max(@P.y, 0);             // nothing below the ground
```

### clamp

```c
T clamp(T x, T lo, T hi)
```

`x`, kept between `lo` and `hi`.

```c
float w = clamp(@weight, 0, 1);
```

## Trigonometry

Angles are in radians.

### sin

```c
float sin(float a)
```

The sine.

```c
@P.y += 0.1 * sin(@P.x * 6.283);   // a wave a unit long
```

### cos

```c
float cos(float a)
```

The cosine.

### tan

```c
float tan(float a)
```

The tangent.

### asin

```c
float asin(float x)
```

The angle whose sine is `x`.

### acos

```c
float acos(float x)
```

The angle whose cosine is `x`.

```c
float angle = acos(dot(normalize(@N), vector(0, 1, 0)));   // from up
```

### atan2

```c
float atan2(float y, float x)
```

The angle of the direction (`x`, `y`), from -π to π.

```c
float around = atan2(@P.z, @P.x);
```

### radians

```c
float radians(float degrees)
```

Degrees as radians.

### degrees

```c
float degrees(float radians)
```

Radians as degrees.

## Interpolation

### mix

```c
T mix(T a, T b, float t)
```

From `a` (at 0) to `b` (at 1), by `t`.

```c
@P = mix(@P, hit, weight);
@Cd = mix(color(0, 0, 1), color(1, 0, 0), t);
```

### step

```c
float step(float edge, float x)
```

0 where `x` is below `edge`, 1 from it.

### smoothstep

```c
float smoothstep(float edge0, float edge1, float x)
```

0 below `edge0`, 1 above `edge1`, and a smooth S between.

```c
@P += normalize(@N) * 0.2 * smoothstep(0.4, 0.6, noise(@P * 3));
```

## Vectors and matrices

### dot

```c
float dot(vector a, vector b)
```

The dot product (of `vector2`s and `vector4`s too).

```c
float facing = dot(normalize(@N), vector(0, 1, 0));   // 1: facing up
```

### cross

```c
vector cross(vector a, vector b)
```

The direction at right angles to both, as long as the area they span.

### length

```c
float length(vector v)
```

How long `v` is.

### distance

```c
float distance(point a, point b)
```

How far apart.

```c
float d = distance(@P, point(0));
```

### normalize

```c
vector normalize(vector v)
```

`v` one long (zero stays zero).

### transform

```c
point transform(matrix m, point p)
vector transform(matrix m, vector v)
normal transform(matrix m, normal n)
```

Through a matrix, by the role: a point is moved, turned and scaled; a vector
turned and scaled, not moved; a normal turned by the inverse transpose, and
made one long again. `@P` is a point and `@N` a normal.

```c
@P = transform(m, @P);
@N = transform(m, @N);
```

### rotate

```c
vector rotate(quat q, vector v)
```

`v` turned by `q`. (OSL's angle-and-axis form: not yet.)

```c
@N = rotate(random_orientation(@N, 1), vector(0, 1, 0));
```

### inverse

```c
matrix inverse(matrix m)
```

The matrix undoing `m`.

### transpose

```c
matrix transpose(matrix m)
```

`m`, rows as columns.

## Colour

### luminance

```c
float luminance(color c)
```

How bright `c` is, by ACEScg's weights (Imago's working space).

```c
float @bright = luminance(@Cd);
```

## Noise

ISL's noises are OSL's, ported: at the same point they give the value an OSL
shader gets. They take a point (or a vector).

### noise

```c
float noise(point p)
float noise(string kind, point p)
```

`noise(p)` is OSL's: Perlin's, from 0 to 1. `kind` chooses, by OSL's names:

| Kind | Gives |
|---|---|
| `"perlin"` | Perlin's, signed: about -1 to 1, 0 at whole numbers |
| `"uperlin"` | Perlin's, from 0 to 1 (`noise(p)`) |
| `"simplex"` | simplex noise, signed: smoother, without Perlin's grid |
| `"usimplex"` | simplex noise, from 0 to 1 |
| `"cell"` | one value from 0 to 1 per unit cube, the same all through it |

```c
float n = noise("perlin", @P * 3);
@P += normalize(@N) * n * 0.1;
```

### snoise

```c
float snoise(point p)
```

Perlin's, signed: `noise("perlin", p)`.

### cellnoise

```c
float cellnoise(point p)
```

One value from 0 to 1 per unit cube: `noise("cell", p)`.

```c
@Cd = color(cellnoise(@P * 4));   // a colour per cube
```

## Random

### random

```c
T random(T min, T max)
T random(T min, T max, int seed)
```

A value between `min` and `max`, the same each time for the element and the
seed (the Random Value node). A `float`, a vector or a colour, as `min` and
`max` are.

```c
float @scale = random(0.5, 1.5, 7);
color @Cd = random(color(0), color(1), 3);
```

### random_orientation

```c
quat random_orientation(normal n)
quat random_orientation(normal n, int seed)
```

A rotation turning +Y along `n`, then a random turn about it (the Random
Orientation node).

## Lists

### arraylength

```c
int arraylength(T a[])
```

How many items (OSL's name).

```c
int sides = arraylength(get_array_data(@P, "point"));   // per face
```

### sum

```c
T sum(T a[])
```

Every item added (comparisons count as 1).

```c
int @above = sum(get_array_data(@P, "point").y > 0);
```

### average

```c
T average(T a[])
```

Every item's mean.

```c
point middle = average(get_array_data(@P, "point"));
```

## Geometry

The ICE nodes that read geometry, as functions. Their first argument, `@x`,
names the attribute read (its type, from the geometry or a declaration); it
is not read in the snippet's context. A context is a string written there:
`"point"`, `"face"`, `"corner"` or `"object"`. A [geometry](isl.md#references-other-prims),
when given, comes last and reads on those prims instead of the snippet's own.

### get_data

```c
T get_data(@x, string context)
T get_data(@x, string context, string resample)
T get_data(@x, int index)
T get_data(@x, int face, vector2 uv)
T get_data(@x, geometry g)
```

The Get Data node:

| Written | Reads |
|---|---|
| `get_data(@x, "point")` | the values the element touches in that context, made one by their average |
| `get_data(@x, "point", "max")` | the same, made one by `"average"`, `"sum"`, `"min"` or `"max"` |
| `get_data(@x, i)` | at element `i` of `x`'s own context (VEX's `point(0, "P", i)`); a list of numbers gives a list |
| `get_data(@x, face, uv)` | at a place on a face: a face's number and its Param UV |
| `get_data(@x, g)` | element by element on `g`, of the same topology (a rest mesh) |

Each but the last takes a geometry last too: `get_data(@P, i, rest)`.

```c
point @centre = get_data(@P, "point");          // a face's centre
vector d = @P - get_data(@P, rest);             // moved from rest
point mean = average(get_data(@P, near));       // at near_points' numbers
```

### get_array_data

```c
T[] get_array_data(@x, string context)
T[] get_array_data(@x, int index)
T[] get_array_data(@x, int face, vector2 uv)
```

The Get Array Data node: the values the element touches in a context, as a
list, in order (a face's points' positions); the list element `index` holds;
the list at a place. A geometry may come last.

```c
point pts[] = get_array_data(@P, "point");
float edge = distance(pts[0], pts[1]);
```

### near_points

```c
int near_points(point origin, int count, float radius,
                output int points[], output float distances[])
int near_points(point origin, int count, float radius, geometry g,
                output int points[], output float distances[])
```

The Near Points node: the nearest points to `origin`, up to `count` (0: all),
within `radius` (0: anywhere), nearest first, the snippet's own points or
`g`'s. Their numbers go in `points`, their distances in `distances`; it gives
how many it found.

```c
int near[];
float d[];
int found = near_points(@P, 6, 0, near, d);
point mean = average(get_data(@P, near));
```

### closest_point_on_surface

```c
int closest_point_on_surface(point origin, geometry g,
                             output int face, output vector2 uv, output point position)
```

The Closest Point on Surface node: the nearest place on `g`'s meshes (and
curves) to `origin`: its face's number, its Param UV and its position. It
gives 1 when it found one. Read anything there with `get_data(@x, face, uv,
g)`.

```c
int face; vector2 uv; point hit;
if (closest_point_on_surface(@P, ground, face, uv, hit)) {
    @P = hit;
    @N = get_data(@N, face, uv, ground);
}
```

### get_reference

```c
geometry get_reference(int input, string path, …)
```

The Get Reference node: the prims on reference input `input` (1, the first
after the scene) the paths name (`*`, `?`, `**`); no path, all of them. Most
often a geometry parameter's default.

```c
snippet s (geometry rocks = get_reference(1, "/set/rocks*")) { … }
```

### index

```c
int index()
```

The element's number in the snippet's context: a point's, a face's; 0 per
object.

```c
@Cd = mod(index(), 2) == 0 ? color(1) : color(0);
```
