# Compounds

A **compound** packages a piece of graph as one reusable operator (in the scene
graph) or node (in an ICE graph), with an interface of its own: ports,
**promoted** parameters, a name and a category in the add menu. What a compound
is made from is its **definition**, an `.icd` file ("Imago compound
definition"); one placed in a graph is an **instance**.

Agreed on 25 October 2026 and revised on 27 October 2026 (Martino). The design
is below; **Status** says what is built.

## Why

- **Groom tools** (Groom Grow, Clump, Frizz, Cut): scene-graph operators that
  are ICE graphs inside, used without opening ICE.
- **Shared rigs:** light rigs, lookdev setups, render-pass setups, reused across
  shots and edited in one place.
- **ICE compounds** inside ICE graphs.

It brings two features planned anyway: **parameter references** (Copy as
Reference, Paste as Reference) and a **Ramp** parameter type.

## Status

| Step | What | State |
|---|---|---|
| 1 | Definitions, `.icd` files, the library and its search path, context checks | Built: `src/compounds.rs` |
| 2 | Scene compound instances: the Compound, Compound Input and Compound Output operators, cooking, Make Compound, entering, Make Editable, Save as Definition, promoted parameters in the Parameters pane | Built: `src/node_graph/compound.rs`, `src/inside_compound.rs` |
| 3 | Promotion and references: links on operators, applied with keys; Promote and Copy / Paste as Reference from the Keys list | Built: `src/node_graph/links.rs`, `src/keys_ui.rs` |
| 4 | The right-click menu on every number's row in the parameters | Built: `properties::form` finds a row's number by its path (a probe), `keys_ui::row_menu` |
| 5 | ICE compounds, flattened into the engine graph; the ICE Parameter node | Built: `src/ice/compound.rs` ([ICE](ice.md#compounds)) |
| 6 | Ramp: the type, its row, Ramp Lookup | Later: not needed yet (Martino) |
| 7 | Viewer tools inside compounds | |
| 8 | The first shipped compounds: the groom set, Scatter and Instance | |

## Using them

- **Make one:** select operators, open the add menu (right-click or Tab,
  over the canvas or an operator; an operator right-clicked is selected) and
  choose **Organise > Make Compound**, or type to search for it: it is found
  like the operators (and the library's compounds are too), ↓ ↑ and Enter
  included. They are packed into a Compound operator in their
  place, the wires that crossed the selection now on its ports: a **Compound
  Input** inside for each output wired in from outside, a **Compound Output**
  for each output wired out (the viewed operator's, or the lowest's, when none
  is). It is **editable**: its definition is its own, kept in the graph.
- **Go inside:** double-click it, select it and press ↓, or **Go Inside** in its
  parameters. Every pane shows the graph inside: the Node Graph, the Viewer
  (its Compound Output viewed), the Scene Explorer, the Parameters. Its
  Compound Inputs give what came into the instance when you went in. The bar
  over the Node Graph shows the way back: **Root**, the instances entered, **↑
  Up** (or ↑). Undo inside undoes what was done inside; leaving an editable
  compound is one step of the graph it is in.
- **Leaving an editable one** keeps the graph inside as its definition: its
  ports are its Compound Input and Compound Output operators' names (renaming
  one renames the port; its wires follow by name), its promoted values the
  values at their targets.
- **Save as Definition…** (in its parameters, with its Name, Category, Icon,
  Help and Author) writes it as an `.icd` file, in the user's compounds folder
  to start with. Saved in a library folder, the instance then follows the file,
  **locked**, a version after the library's of that name; others of that name
  are brought up to date.
- **Placing one:** the add menu's **Compounds** lists the library's scene
  compounds by category, with where each came from. A locked instance shows its
  definition's name, version and file; **Make Editable** gives it its own copy,
  and **Revert to Latest Definition** drops that copy: it follows the library's
  definition of its name again, locked. Inside a locked one the graph is drawn
  at half strength, a change is put back at once, and Make Editable is on the
  bar.
- **Promote:** inside an editable compound, right-click a number's row in an
  operator's parameters (sliders, number fields, X Y Z, U V), or the number in
  its **Keys** list, and choose **Promote…**:
  give it a label, a group and a range, or make it another target of a
  parameter already promoted. Its value now is the default. The compound's
  parameters take it when you leave the compound; **Remove Promotion** takes
  it back. A promoted number is marked ◆ in the list.
- **Promoted numbers follow:** inside the compound, a promoted number is drawn
  orange, as a reference is: it follows the compound's parameter. Changing it
  there changes the parameter (when you leave), and changing the parameter
  changes it. A compound inside a compound: promote the inner instance's
  parameter row, and the outer one drives it, however deep (a value an inner
  instance has not saved yet is made). Promotion works in every editable
  compound, saved or not, and in ICE graphs (see [ICE](ice.md#compounds)).
  Text promotes as numbers do (a path, a name), its row's right-click menu
  the same; switches and lists do not yet. Promotions name their operator:
  renaming it inside the compound keeps them. An operator's name is not a
  parameter, and is never promoted.
- **Promoted parameters** show in the instance's parameters by group: numbers,
  booleans, text and vectors. A value set back to the default is the default
  again. Each instance keeps its own.
- **References:** right-click any number's row (or the number in the Keys
  list), **Copy as Reference**; on another number, **Paste as Reference**: it follows the first,
  as it cooks (keyed, it follows its animation). Both stay joined: changing
  either changes both, since a value typed (or dragged, or moved with a
  handle) over the number that follows goes to the number it follows, at the
  end of its chain, keyed there when that one is keyed. Two numbers of the
  same operator can follow each other this way (translate X and scale Z of
  one Transform). A number that follows is drawn on desaturated orange, in its
  row and in the Keys list, with a 🔗 in place of its key diamond;
  **Remove Reference** makes it its own again. A keyed number cannot follow
  another, nor one that would make a loop (a number following one that
  follows it). Each reference reads the number at the end of its chain once,
  so many references on one operator stay quick. References are saved with
  the graph, and pasted operators follow each other, or the operator copied
  when pasted alone.
- **How a row is found:** a right-click on a number's row puts a probe value
  in its place for the moment the parameters are drawn; the operator's
  parameters are searched for it (or, for a row showing its number another
  way, such as angles in degrees, for what changed), which gives the number's
  path, and the parameters are put back before anything cooks, whether the
  number was found or not. What a change to it would set off waits (a
  Merge's Inputs would make that many inputs). Every number field and slider of an operator's
  parameters has the menu (`form::number`, `form::slider`); a right-click
  never moves a slider. While an operator has references, its rows are found
  the same way, one a frame while the pointer and keys are still, until each
  is known (kept until the operator or its references change), so the rows
  of the numbers that follow are drawn orange. Text, switches and lists do
  not have the menu yet.
- **Not yet:** Promote is not a step of the undo history inside the compound
  (leaving it is).
- **In an ICE graph** it is the same: select nodes, **Compounds > Make
  Compound** in the add menu, double-click or ↓ to go inside, ↑ to come out,
  Save as Definition, Make Editable. Inside a locked scene compound, the ICE
  graphs of its operators are locked too. Its ports are its Compound Input and
  Compound Output nodes, and its input ports hold values as any ICE node's
  inputs do: those are what an ICE compound offers to set, rather than
  promoted parameters. See [ICE](ice.md#compounds).
- **A missing definition** keeps the operator, its ports, wires and values: what
  comes into its first input goes on, and its parameters say which definition
  is missing.
- **Saving the graph, opening another or closing** while inside first goes back
  out to the root scene graph.

## Decisions

1. **One compound system for both graphs.** A definition records its
   **context**: `Scene` (its inside is operators) or `Ice` (its inside is ICE
   nodes). A scene compound cannot be placed in an ICE graph, nor the reverse:
   the add menus list compounds of their own context, and pasting or loading one
   of the other context is refused with a message.
2. **Definitions are `.icd` files**, JSON inside like `.xms`.
3. **Instances are locked by default:** they follow their definition. **Make
   Editable** copies the definition into the instance, for that instance only.
4. **Promotion and references are one mechanism:** a target parameter follows a
   source. Both are on the parameter's right-click menu.
5. **Words** (`VOCABULARY.md`): compound, definition, instance, **Promote**
   (promoted parameters and ports), reference, locked and editable. Reading
   across ICE contexts is **Resample**, not Promote.

## Concepts

- **Definition:** a piece of graph and its interface, in an `.icd` file, or in
  an instance made editable.
- **Instance:** an operator (scene graph) or a node (ICE graph) naming a
  definition, holding its own parameter values.
- **Interface:**
  - **Ports:** named inputs and outputs. A scene compound usually has `Scene` in
    and `Scene` out, and named extra inputs (`Clump Guides`). An ICE
    compound's ports also have a data type.
  - **Promoted parameters:** inner parameters given a label, type, default,
    range, group and help.
  - **Metadata:** name, category (the add menu's group), icon, colour, help,
    version, author.
  - **A tool,** optionally: the inner operator the Viewer's tools act on.

## The `.icd` file

```json
{
  "format": "imago-compound",
  "format_version": 1,
  "context": "Scene",
  "name": "Groom Clump",
  "version": 3,
  "category": "Groom",
  "icon": "🌀",
  "help": "Pulls each strand toward its clump guide",
  "author": "Martino",
  "inputs":  [{ "name": "Scene" }, { "name": "Clump Guides" }],
  "outputs": [{ "name": "Scene" }],
  "parameters": [
    { "name": "strength", "label": "Strength", "type": "Float", "default": 0.8,
      "range": [0, 1], "group": "Clump",
      "targets": [{ "node": "Clump", "path": "params/strength" }] }
  ],
  "tool": null,
  "graph": { "...": "the inner graph, as a .xms graph holds it (scene), or an ICE graph (ICE)" }
}
```

- **`format_version`** is the file layout's, for reading older files;
  **`version`** is the definition's own.
- **`targets`** say where a promoted value goes: an inner node, by its name in
  the definition, and the **serde path** of the parameter in it, the paths keys
  already use (`translation/0`, `Channel::path`). One promoted parameter can
  drive several targets. In an ICE compound, a target is an input port or an ICE
  Parameter node.
- **`graph`** is the inner graph in the JSON a `.xms` file uses
  (`graph_io::to_json`), its ICE graphs with it; for an ICE compound, an ICE
  graph. Written with the keys in a stable order, so a definition diffs well.

## The library

Definitions are found, in this order, the first of a name winning:

1. `$JOB/compounds/`, the project's
2. the user's: `compounds/` in the config folder (`~/.config/xms/compounds` on
   Linux, `%APPDATA%\xms\compounds` on Windows)
3. every folder in `XMS_COMPOUNDS` (a path list, like `PATH`)
4. the ones Imago ships: `compounds/` beside the program

**Shipping them as files.** Houdini installs its digital assets as files in its
own folder (`$HFS/houdini/otls`) and finds them through `HOUDINI_PATH`, the
user's and the job's folders before its own; Gaffer ships its resources beside
the program and finds startup files through `GAFFER_STARTUP_PATHS`; Katana
reads macros from `KATANA_RESOURCES`. Imago does the same: the release copies
`assets/compounds/` beside the program as `compounds/`, as it does with
`examples/`, and a build run from the repository finds `assets/compounds/`
there. Shipped definitions stay files that can be opened, read and copied into
a project to change. (Renderer definitions and layouts are built into the
program instead: Imago cannot work without them.)

## Instances

Stored in a `.xms` graph as an operator of a new kind:

```rust
NodeType::Compound {
    definition: String,               // its name, as found in the library
    version:    u32,                  // the definition's, when last saved
    hash:       u64,                  // of the definition, when last saved
    inputs:     Vec<String>,          // its ports, when last saved
    outputs:    Vec<String>,
    values:     BTreeMap<String, serde_json::Value>, // promoted values, by name
    editable:   Option<Box<Definition>>,             // made editable: its own
}
```

ICE instances get the matching `IceGraphNodeType::Compound`.

- **Values by name.** A parameter the definition gains takes its default; one
  it loses is dropped, and the Parameters pane says so once.
- **Ports from the interface,** rebuilt when the definition changes; wires kept
  by port name where they can be, then by number. The instance keeps its port
  names, so its wires are kept while its definition cannot be found.
- **A missing definition** keeps the instance as it is, drawn as missing with a
  message naming the compound: opening and saving the graph keeps the
  reference, and it cooks again once the definition is back. What comes into
  its first input goes out.
- **Locked,** the default: only the reference and the values are saved. Its
  inside opens read-only, with a banner: locked, **Make Editable**, **Open
  Definition**.
- **Make Editable** copies the definition into `editable`; the instance no
  longer follows the library. **Save as Definition** writes it to a library
  folder as a new version, or under a new name.
- **Updates:** a locked instance uses the definition found. When its version
  or hash differs from the instance's, the Parameters pane says so ("Updated
  from v2 to v3", or "Its definition changed"). Pinning to a version, and
  **Collect Compounds** (copying the definitions a graph uses into
  `$JOB/compounds`, so it can be sent on), come later.

### Compound Input and Compound Output

Inside a scene compound, a **Compound Input** operator stands for each input
port, by its name, giving what comes into that port; a **Compound Output**
operator takes what goes out of its port. Make Compound puts them where wires
crossed the selection.

## Promotion and references

One link: **a target parameter follows a source.** The source is another
operator's parameter (a reference) or the compound's promoted parameter
(promotion).

- **Kept on the target operator,** beside its keys (`GraphNode::links`, as
  `GraphNode::channels`): a link copies, pastes, deletes and undoes with its
  operator. Pasting renumbers operators and their links' sources with them.
- **Applied where keys are,** in `animated_at`: the operator's parameters as it
  cooks are its own with its keyed numbers and its linked ones set. The graph
  is not written to, so a link adds no step to the history, and the cook key,
  made from those parameters, follows the source. A link to a keyed parameter
  follows its animation.
- **A keyed parameter is not linked,** nor a linked one keyed: the menu says
  which it is.
- **Linked rows** show a link mark and a tooltip naming their source; editing
  one replaces the link with its own value.
- **Cycles are refused** when the link is made. **Types match:** a number to a
  number, a vector to a vector, one channel (`translation/1`) to a number.

On the right-click menu of a parameter row: **Copy as Reference**, **Paste as
Reference**, **Remove Reference**; inside an editable compound, **Promote…**,
which asks for the label, group and range and adds the parameter to the
interface, this row its target. Promoting a promoted row adds it as another
target. Until every row knows its serde path (step 4), these are on the rows of
the Keys list, which already names every number by its path.

## Cooking

- **A scene compound instance cooks its inner graph:** its inputs fed to the
  Compound Input operators, its result read from the Compound Output.
- **Cooked results are shared** where they can be: the cook cache is keyed by
  content (parameters, and the keys of what is wired in), so two instances with
  the same values and inputs give the same result and cook once. The per-cook
  cache, keyed by operator id, is one per instance, since inner ids repeat
  those outside.
- **ICE compounds are flattened** into their parent ICE graph when it becomes
  the engine's graph (`ice::compound::flatten`): the engine never sees a
  compound. What reads a Compound Input reads what is wired to the instance's
  port (or the port's value); what reads the instance's output reads what feeds
  the Compound Output. Values reach the engine as parameters through ICE
  Parameter nodes (`Op::Parameter`; the values `xms_ice::run_in` takes), set on
  the ICE operator (`NodeType::Ice::params`), which a scene compound can
  promote.
- **Inside an ICE compound** the editor holds the definition's graph, the graph
  the instance is in kept with it (`IceGraph::outer`); what is run, saved and
  hashed is that graph with the inside folded back into its instance
  (`IceGraph::composed`), so the result follows edits made inside.
- **Nesting** is allowed, 32 deep, with a check on definition names against
  cycles.
- **The ICE graphs of a definition** are kept with the definition, not in the
  graph's ICE graph store: a locked instance's ICE graphs are not saved in the
  `.xms`.

### Variables inside

- `$OPNAME` is each inner operator's own name, as everywhere.
- The instance gives its inner graph `{instance}`, its own name, with what
  comes down the wires to it.
- Two instances of a definition make the same primitive paths unless their
  locations say otherwise: shipped definitions put `{instance}` in theirs
  (`/lights/{instance}/key`).

## The ICE operator inside scene compounds

- Its extra inputs are numbered (`Reference 1, 2…`) on purpose: inside a
  definition they are wired from the named Compound Input operators (`Clump
  Guides` to Reference 1).
- Promoted values reach its ICE graph as parameters, by name.

## Viewer tools inside compounds

Interactive operators (Edit Poly, the planned Paint and Groom Guides brushes)
can live inside a compound.

- **The definition's `tool`** names one inner operator. While the instance is
  selected, the Viewer's tools act on it as if it were.
- **Its parameters belong to the instance:** Edit Poly's operations are its
  parameters, so the tool operator's parameters are kept in the instance's
  values, as if all of them were promoted. A locked instance takes them.
- **Files beside the graph** (a paint operator's strokes) are named per
  instance: `$GDIR/{instance}_$OPNAME.paint`.

## Ramp

For the groom compounds (a clump's profile, width from root to tip, frizz along
the length):

- **The value:** keys of a position and a value, and how they are joined
  (linear, smooth, constant): the curves keys already have (`core::keys`), the
  position in place of the frame, so the Curve Editor can shape a ramp later.
- **A row** in the Parameters pane: a small curve to add, move and delete keys.
- **Ramp Lookup,** an ICE node: the ramp's value at an input (usually `u`, the
  place along a curve); and ICE inputs that hold a ramp.

## Interface

- **The add menus:** the scene graph's lists scene compounds by category, with
  the operators; the ICE graph's lists ICE compounds; each says where it came
  from (shipped, user, project).
- **Making one:** select operators, right-click > **Make Compound**: wires
  crossing the selection become its ports. Promote parameters inside, then
  **Save as Definition…**: name, category, icon, help, folder.
- **Using one:** double-click an instance to go inside (with the path at the
  top, as for ICE graphs). The Operator Stack shows it as one entry, opening to
  its inner operators. The Parameters pane shows its promoted parameters,
  grouped, with their help; its definition's name, version and folder; and Make
  Editable, Open Definition, Save as Definition.

## Undo, copy and paste, files

- Every compound action is a step of the history: Make Compound, Promote, Make
  Editable, values and links. Writing an `.icd` is not undone.
- Copying an instance copies its reference, values and `editable`.
- Graphs without compounds are saved as they were.

## Rules

- `LLM_instructions.md`, `VOCABULARY.md`, `ARCHITECTURE.md`.
- Tests, a line in `DESIGN_PHILOSOPHY.md`'s log and these docs with every step.
- Operators that make or work on a scene take a `Scene` and give a `Scene`.
- Graphs and `.icd` files stay readable JSON, stable across versions.
