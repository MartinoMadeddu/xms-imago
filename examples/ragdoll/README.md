# Files of the Body Collide template

| File | What |
|---|---|
| `split/into_the_car_.fbx` | The take: a skinned character of 342 joints and 32,334 vertices, frames 1200 to 9199 of the capture at 30 fps. An actor walks through a car and sits in it |
| `car.fbx` | The set: the car, 1,743,007 triangles, without normals |
| `solved/` | The take solved against the car with the default settings |

The take and the car are Simon Legrand's. The whole capture as FBX is 205 MB and the car with normals 99 MB, more than GitHub takes in one file: the take is cut to 8,000 frames, and the car is written without normals (25 MB). Normals are worked out again when the car is drawn.

To solve the take again after a change to the solver, and put the result in `solved/` in place of the one there:

    cargo test --release solve_the_template -- --ignored --nocapture
