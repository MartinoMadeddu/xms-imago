# ICE graphs

An **ICE** operator holds an **ICE graph**: a graph of its own that runs on every primitive coming in, in the primitive's own space, and sets its attributes. Loosely modelled after Softimage ICE and Houdini VOPs. Double-click the ICE operator, or select it and press ↓, to open its graph; ↑ comes back.

## How a graph runs

Every ICE graph ends in an **ICE Tree** node. Its inputs are **Execute ports**, run in order, top to bottom; a free one is always there for the next. The tree runs **over every element of its context**: Point by default, or Face, Corner or Object (set it in the tree's parameters).

- **Get Data** reads an attribute of the geometry: `P` is the points, any other name is an attribute (`N`, `Cd`, your own). Read in the tree's context, it is one value per element; an Object attribute is one value for all of them.
- **Set Data** writes an attribute, for every element. Its Execute output goes into the ICE Tree. A Get Data of an attribute set by an earlier port reads the new values.
- **Constants** (Scalar, Integer, 3D Vector, Boolean) and **Math** (Add, Subtract, Multiply, Divide, Minimum, Maximum, Linear Interpolate, Negate, Absolute Value, Sine, Cosine, Dot Product, Cross Product, Length, Normalize) work on what they are given. Add takes two floats or two vectors; Multiply also scales a vector by a float.

Right-click the canvas to add nodes. Drag from an output to an input to wire them.

**Inputs hold values.** An input with nothing wired to it uses its own value, set in the Parameters pane (with its type: Scalar, Integer, 3D Vector, Boolean). A value meets the type of what it is combined with: a 1 added to a vector is the vector (1, 1, 1).

**Errors show on the node.** A graph that does not fit together (a vector added to a boolean, an attribute that is not there, a per-face attribute read in a per-point tree) leaves the primitive unchanged; the node where it broke gets a red ring, and why is written along the top of the canvas.

## Fast

The graph is compiled, then run in parallel batches of 4,096 elements:

- only the attributes the graph reads are read, a batch at a time;
- only the attributes it sets get new values; every other attribute stays shared with what came in;
- what does not change from element to element (constants, Object attributes, maths on those) is worked out once.

A million points moved by a graph like P + N × heat × amount take about a millisecond.

## Try it

Templates → ICE → **ICE graph: spherify**: Get Data P → Normalize → Multiply (B = 1.2) → Set Data P, run by the ICE Tree. Change Multiply's B in its parameters.

## Not yet

- Scattering points and copying geometry onto points (the next step of the refactor)
- Reading another context's attributes (a face's corners, a point's neighbours)
- Snippets: code in a node, compiled to the same instructions
- ICE graphs saved in graph files (copied nodes carry them)

## Tests

`cargo test -p xms-ice` (the engine), `cargo test ice::` (the editor's graph on the engine), `cargo test -p xms-ice --release ice_engine_speed -- --ignored --nocapture` (its speed).
