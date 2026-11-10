# MaterialX libraries

The node definitions of MaterialX, copied unchanged from the MaterialX
repository:
https://github.com/AcademySoftwareFoundation/MaterialX/tree/main/libraries

- MaterialX version: 1.39
- Commit: 7a4f557 (15 October 2026)
- Licence: Apache 2.0, in `LICENSE` here. Copyright Contributors to the
  MaterialX Project.

Copied: every `.mtlx` file of `bxdf`, `cmlib`, `lights`, `nprlib`, `pbrlib` and
`stdlib`, definitions (`*_defs.mtlx`) and node graphs (`*_ng.mtlx`). Not copied:
the shader code folders (`genglsl`, `genmsl`, `genosl`, `genmdl`, `genslang`)
and `targets`.

Not MaterialX's: `arnold/`, Imago's own definitions of Arnold's shaders as
MaterialX nodes (`ai_standard_volume`), written for Imago from what Arnold
reports of them (`kick -info`). Leave it when updating.

To update: copy the same files from a newer MaterialX over these, change the
version and commit above, add a new file to `FILES` in `src/library.rs` if there
is one, and run `cargo test -p xms-matx`.
