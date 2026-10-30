# Component libraries

MitCad's component libraries hold fasteners and other standard components.
Each component library and the community library are plain git
repositories. No server is necessary. To publish a library, push it to a
git host or to a shared folder. A community index is one more
repository. It lists libraries.

This page gives the formats, so that you can make and publish a library.
The application's side is in the
[user guide](user-guide.md#component-libraries). The commands are in
[commands.md](../core/model-api/src/commands.md#component-libraries).

## A library repository

```
my-parts/                        a git repository, also a MitCad project
  mitcad-library.json            the manifest (required)
  LICENSE                        the licence's text or a note naming it
  README.md
  SOURCES.md                     where data such as dimension tables came from
  .mitcad/project.json           MitCad's project marker
  screws/iso4762.mitcad          the components' designs
  previews/iso4762.png           preview images (PNG, at most 512 KB)
```

The designs are ordinary MitCad designs (`.mitcad` files). A design can
have a configuration table
([Configuration tables](#configuration-tables)). Then one design stands
for a family of sizes.

A library holds data only:

- MitCad runs nothing in it.
- MitCad refuses symbolic links.
- Paths stay in the repository.

### `mitcad-library.json`

```json
{
  "format": "mitcad-library",
  "version": 1,
  "id": "mitcad-parts",
  "name": "Mitcad parts",
  "description": "ISO metric screws, nuts and washers, and the Build tab's components.",
  "library_version": "2.0.0",
  "license": "MIT",
  "authors": ["Mitcad contributors"],
  "homepage": "",
  "sources": ["ISO 4762", "ISO 4032", "Tabulated for this library; see SOURCES.md"],
  "units": "mm",
  "categories": [{"id": "screws", "name": "General fasteners/Screws"},
                 {"id": "nuts", "name": "General fasteners/Nuts"}],
  "components": [
    {"id": "iso4762", "path": "screws/iso4762.mitcad", "category": "screws",
     "name": "Hexagon socket head cap screw", "standard": "ISO 4762",
     "keywords": ["socket head", "cap screw", "DIN 912"],
     "preview": "previews/iso4762.png", "license": "CC0-1.0", "designation": "ISO 4762 {Size}x{Length}",
     "description": "190 sizes, M1.6x2.5 to M24x200."}
  ]
}
```

| Field | Meaning |
|---|---|
| `format`, `version` | `mitcad-library`, 1 |
| `id` | 2 to 64 characters of `a-z`, `0-9` and `-`. It is the same for every version |
| `name`, `description` | For people |
| `library_version` | The library's own version. It is the same as its tag (`v1.0.0`) |
| `license` | An SPDX licence expression for every component without a licence of its own |
| `authors`, `homepage`, `sources` | For the attribution and the data's origin |
| `units` | `mm` |
| `categories` | `id` and `name` of the groups that components belong to. Optional: `parent` ([Groups](#groups)) |
| `components` | Required: `id`, `path` (a `.mitcad` file) and `name`. Optional: `category` (one id or a list, [Groups](#groups)), `categories`, `standard`, `description`, `keywords` (or `tags`), `preview` (a PNG), `row_previews`, `license` (instead of the library's), `designation`, `supplier`, `part_number`, `supplier_url`, `rows` (the rows' standards and suppliers' articles, [Standards and suppliers of rows](#standards-and-suppliers-of-rows)), `build` and `mounts` ([Mount frames](#mount-frames)) |
| `suppliers` | Optional: the suppliers the rows' articles name, each `id`, `name` and `url` ([Standards and suppliers of rows](#standards-and-suppliers-of-rows)) |
| `replaces` | Optional: the ids the library had before (`["mitcad-fasteners"]`). An index's library of such an id is not listed beside this one ([A community index](#a-community-index)) |
| `materials`, `printers`, `processes` | Optional: the library's materials and print profiles, each `id` and `path` ([Materials and print profiles](#materials-and-print-profiles)) |

Three fields tell who sells the component:

- `supplier`: who sells the component (`Motedis`)
- `part_number`: the supplier's article number
- `supplier_url`: the product page. It must be an https address.

A configuration row can have its own `part_number` and `supplier_url`.
The row's values come first. For example, a sheet sold in widths has a
number and a page for each width. MitCad shows these fields where you
select and list the component: Insert from Library, Library Parts, the parts
list and the Build tab's cut list. `build` makes the component a part of
the Build tab ([Build parts](#build-parts)).

`row_previews` gives single configuration rows a picture of their own:
`{"<row name>": "previews/<id>-<row>.png"}`. For example, each timber
size shows its section from its end. A row without a picture of its own
shows `preview`. The Build tab's pickers list every row as an entry of
its own. A picker shows a row's picture in its tile (`library_preview`
with `config`).

`library check` makes sure that every key of `row_previews` names a
configuration row of the design. It also makes sure that every path
names a PNG image in the folder.

`designation` is the name of a part in a bill of materials. MitCad
replaces these placeholders in braces: `{standard}`, `{name}`,
`{config}` and the options' names (`{Size}`: the option's value, a
number without its unit). Without `designation`, the name is
`{standard} {config}`. The own `designation` of the row of the part's
combination comes first; a combination that is no row of an open table
takes the pattern, with `{config}` empty.

MitCad ignores unknown fields. Thus newer libraries stay readable.

### Standards and suppliers of rows

One component can stand for a fastener of several standards. A screw's
length is often in two standards (M6x10 is ISO 4762 and DIN 912), only
in one (M6x8 is DIN 912 only), or in none (a supplier's own length). The
component's `rows` tags each configuration row with its standards and
the suppliers' articles of it. The library's `suppliers` names the
suppliers once:

```json
"suppliers": [{"id": "wurth", "name": "Würth", "url": "https://www.wurth.fi"},
              {"id": "etra", "name": "Etra", "url": "https://www.etra.fi"}],
"components": [
  {"id": "iso4762", "path": "screws/iso4762.mitcad", "name": "Hexagon socket head cap screw",
   "standard": "ISO 4762 / DIN 912", "designation": "{standard} {Size}x{Length}",
   "rows": {
     "M6x6":  {"standards": ["non-standard"],
               "supplier_parts": [{"supplier": "wurth", "part_number": "0084 6 6", "material": "8.8",
                                   "finish": "zinc-plated", "checked": "2026-10-30"}]},
     "M6x8":  {"standards": ["DIN 912"]},
     "M6x10": {"standards": ["ISO 4762", "DIN 912"],
               "supplier_parts": [{"supplier": "wurth", "part_number": "0084 6 10", "material": "8.8",
                                   "finish": "zinc-plated", "checked": "2026-10-30"}]}}}]
```

| Field | Meaning |
|---|---|
| `suppliers` | The library's suppliers. `id`: 2 to 64 characters of `a-z`, `0-9` and `-`, the same in every version (the rows refer to it). `name`: for people. `url`: the supplier's own site (https) |
| `rows` | A component's rows by the row's name (as the design's table names them). Each has `standards` and `supplier_parts`, both optional |
| `standards` | The standards the row belongs to, in the component's order (`["ISO 4762", "DIN 912"]`). `["non-standard"]`: a length outside every standard's table |
| `supplier_parts` | One entry per article: a supplier sells one size in several materials and finishes. `supplier`: an id of `suppliers`. `part_number`: in the supplier's own form (below). `material` (`8.8`, `A2`), `finish` (`zinc-plated`, `plain`). `checked`: the date the entry was read from the supplier's catalogue (`YYYY-MM-DD`). No price and no address |

**Part numbers.** A part number is written as the supplier's catalogue
writes it, so that an address made from it later finds the article:

| Supplier | Form |
|---|---|
| Würth (`wurth`) | The article number in its groups, separated by single spaces: `0084 6 10` |
| Etra (`etra`) | The product code as the catalogue writes it, without spaces added or left out |

**Designations.** In a pattern, `{standard}` is the standard the row is
named by: the standard chosen in the Standard filter when the row has
it, else the row's first (`ISO 4762 M6x10`, `DIN 912 M6x8`). A row of no
standard is named without one and marked: `M6x6 (non-standard)`. A row
without `standards` takes the component's `standard`, as before. The
part records the standard it is named by and its row's articles
(commands.md, [Standards and suppliers of library rows](../core/model-api/src/commands.md#standards-and-suppliers-of-library-rows)).

**Filters.** Insert from Library, Library Parts, Edit Options and the
Build tab's part picker show a Standard and a Supplier filter above the
size choosers when the rows name standards or articles. They narrow the
sizes to the rows that match. The part grid's Supplier filter hides the
components that the supplier sells in no size
([the user guide](user-guide.md#component-libraries)).

**What MitCad shows.** The same three dialogs show the chosen row's
articles behind a button, **Supplier Articles (n)…**, shown only when
the row has articles. It opens a table with one line per article and
the columns Supplier (the supplier's `name`, a link to its `url`), Part
number, Material, Finish, Checked and Product page (the row's or the
component's `supplier_url`). A column that no article fills is hidden,
so the table shows what the library gives and nothing more. When the
Supplier filter names a supplier, its articles come first; the others
follow. The parts lists keep one text per part in their column
Supplier part numbers (`Würth 0084 6 10 (8.8 zinc-plated); …`).

**Checks.** `library check` refuses an article of a supplier that
`suppliers` does not list, a `checked` that is no date, an article
without a part number, a supplier without an id or a name, and a row
name that the design's table does not have. MitCad of before mitcad#949
ignores `rows` and `suppliers`, and reads the library as before.

### Groups

The categories make the tree of groups. Insert from Library and the
Build tab's part picker show this tree:

```json
"categories": [
  {"id": "wood", "name": "Wood"},
  {"id": "dimensioned-timber", "name": "Dimensioned timber", "parent": "wood"},
  {"id": "sheets", "name": "Wood/Sheets"},
  {"id": "slot-screws", "name": "Slot profile/Screws"},
  {"id": "general-screws", "name": "General fasteners/Screws"}],
"components": [
  {"id": "hammer-screw-b8", "path": "slot-screws/hammer-screw-b8.mitcad", "name": "Hammer-head screw B8",
   "category": ["slot-screws", "general-screws"]}]
```

**Parents and paths.** A category's `parent` is the `id` of another
category in the same library. A category's `name` can be a path of
names, such as `Wood/Sheets`. The names between the slashes are
trimmed. A category with both puts the path under the parent. A
category with neither is a group at the top. Thus libraries made
before the tree, and your own libraries, show as they did.

**Groups are known by their names.** The `Wood` of two libraries is
one group. This is so when one library names it by a parent and the
other by a path. It is also so when the two spellings differ in case.
The first spelling shows. A group lists its own components and those of its
subgroups.

**A component's categories.** A component's `category` is one id or a
list of ids. It can also be `category` and a list `categories`. The
first category is the component's home. The home is the folder that
`library add` puts the component in. It is also the group that a view
of one group shows the component in. Each of the other categories
lists the component too. The component keeps one tile with one star and one count of uses. It is in Favourites and in In this
design once.

**The order of the groups.** The groups show in the order of the
libraries' `categories`, then by name. A group has the place of the
first category that names it. A category that the list lacks is a
group of its id at the top. It comes after the others.

**What `library check` warns of.** `library check` warns of:

- A `parent` that is no category of the library. MitCad shows the
  group at the top.
- Categories that are each other's parents, a cycle. MitCad cuts the
  path where it closes.
- A component's category that the list lacks.

These are warnings. The library stays usable.

**The commands.** `library_show` and `library_search` give each
component its `category` (the home), its `categories` (every one, the
home first) and its `groups`. `groups` has one entry for each
category: `{"category", "path": [names from the top], "order": [each
name's place]}`. `library_search`'s `category` finds a component by any
of its categories. `library_add` takes `category` as one id or a list.
For a new home category, it takes `category_name` (a path name) and
`category_parent`.

### Configuration tables

A design's top-level `configurations` block holds the table. The
`set_configurations` command writes it:

```json
"configurations": {
  "options": ["Size", "Length"],
  "closed": true,
  "parameters": ["d", "dk", "k", "s", "t", "L"],
  "default": "M5x16",
  "rows": [
    {"name": "M5x16",
     "values": {"Size": "'M5'", "Length": "'16'", "d": "5 mm", "dk": "8.5 mm",
                "k": "5 mm", "s": "4 mm", "t": "2.5 mm", "L": "16 mm"}}
  ]
}
```

The options (Size, then Length) are user parameters of the design (here
text parameters) in the order the application offers them. A row is a
preset: it sets the options' values and, in a closed table, the
expressions of the table's other parameters, usually user parameters
that the design's features use. A closed table allows only its rows'
combinations, each unique. An open table (`"closed": false`, the
default) sets options only and allows every combination its rules
allow; its rows are named presets. When MitCad applies a row, it changes
parameter expressions only, as Change Parameters does. Rules, the
cascade and the `configure` command are in
[commands.md](../core/model-api/src/commands.md#configurations-options-rows-and-rules)
and [configurations.md](configurations.md).

A table written before mitcad#798 has `selectors` and rows with
`select` (`{"Size": "M5", "Length": "16"}`). MitCad still reads it, as
a closed table whose options are text parameters made from the
selectors, and writes the form above. Library components keep working
without being published again.

### Mount frames

Any component can say where it mounts. Examples: a light fitting on a
ceiling or a wall, a screw in a hole, a bin on the cells of a
baseplate. Its design marks a Joint Origin as a **mount frame**. In
the application, this is Joint Origin's Mount Frame check
([app/COMMANDS.md](../app/COMMANDS.md#joints)). In the commands, it is
the `mount` of a `joint_origin`
([commands.md](../core/model-api/src/commands.md#mount-frames)). The
mark gives the frame's name, what it mounts on and how the component can
move after you put it. A joint origin named `mount:<name>`, as Build's
components name theirs ([Build parts](#build-parts)), is a face frame of
that name. It is fixed.

| `kind` | The frame | Placed on | `joint` (how it moves) |
|---|---|---|---|
| `face` | On the face that the component lies with. Its z points out of the component | A flat face or a plane, where you click, face to face | `rigid` (fixed), `planar` (free on the face: it slides and turns), `revolute` (it turns) |
| `shaft` | On a screw's or a pin's axis, at the face that bears on the hole's rim (under a head). Its z goes along the shank towards the tip | A hole: its round face or its rim, at the end towards the viewer, coaxial | `rigid`, `revolute`, `cylindrical` (it turns and slides), `slider` (it slides along the axis) |
| `hole` | On the axis of the component's hole. Its z points out of the face that goes on | A shaft: its round face or its end rim | As `shaft` |
| `grid` | At a corner of a grid of cells on the component's face. Its x and y go along the cells. Its z points out of the component. It has `pitch` (two lengths) and `cells` (two counts) | Other components stand on it | `rigid` |
| `foot` | In the middle of the face with which a component stands on a grid. Its z points out of the component. It has `footprint` (the cells that it covers along its x and y) and optionally `pitch` (the grid that it fits) | A grid's cells: those under the click, with its footprint turned in quarter turns | `rigid` |

**Publishing writes them.** `library_add` (Publish to Library,
`mitcad-cli library add`) writes the design's mount frames into the
component's `mounts`. It does not write them when the command gives
them. It does not write them for a Build part, whose frames are
`build.frames`. The design stays what counts. The block tells the
application before it reads the design:

```json
{"id": "bin1x1", "path": "bins/bin1x1.mitcad", "name": "Bin 1x1",
 "mounts": [{"name": "foot", "kind": "foot", "joint": "rigid", "footprint": [1, 1], "pitch": [42, 42]}]}
{"id": "baseplate", "path": "baseplates/baseplate.mitcad", "name": "Baseplate 3x2",
 "mounts": [{"name": "grid", "kind": "grid", "joint": "rigid", "pitch": [42, 42], "cells": [3, 2]}]}
{"id": "lamp", "path": "lamps/lamp.mitcad", "name": "Ceiling lamp",
 "mounts": [{"name": "ceiling", "kind": "face", "joint": "planar"},
            {"name": "wall", "kind": "face", "joint": "rigid"}]}
```

A length is a number in mm or a text with a unit, as in Build's
blocks. `library check` checks the block:

- Each name (letters, digits, `_`, `-`, `+`, `.`) occurs once.
- The kind is known.
- The joint is one that the kind takes.
- A grid has `pitch` and `cells`. A foot has `footprint`. The counts
  are 1 or more.
- No other kind has `cells` or `footprint`.

It also checks the design:

- Every listed frame is a mount frame of the design that says the
  same: kind, joint, cells, footprint and pitch.
- Every mount of the design is a sound one.
- A mount frame of the design that the block leaves out is a warning.

**Placing a component.** Insert from Library drops a component in as it always
did: at the origin. You join it later with Joint. A component's mount
frames are then its joint origins, ready for Joint. A
component with mount frames opens Place Component instead. There, a click on a
face, a hole or a grid snaps the selected frame to it and makes the
joint that the frame asks for. Enter without a click still drops the
component in ([app/COMMANDS.md](../app/COMMANDS.md#component-libraries)).
On a grid, the foot goes on the cells under the click. Rotate turns
its footprint in quarter turns. MitCad refuses a cell that another
component stands on, and names that component.

**Gridfinity** is an open system of bins on baseplates, with 42 mm
cells and 7 mm height units. It is made this way: a baseplate's top
has a `grid` frame at the corner of its first cell, with `pitch`
[42, 42] and `cells` its size. Each bin's bottom has a `foot` frame in
its middle, with `footprint` its size in cells and `pitch` [42, 42].
The tests use synthetic parts of those sizes. Ready-made Gridfinity
models are third-party files. They stay out of MitCad's repositories.

### Build parts

A component with a `build` block is placed by the Build tab.
It is a slot profile, a connector, a nut, a panel or a linear part. The
design stays an ordinary design. The `build` block says how MitCad cuts the
component, where it mounts and what it fits. `library check` checks its
rules. Example:

```json
{"id": "p30x30-b8", "path": "profiles/p30x30-b8.mitcad", "name": "Profile 30x30 B-type slot 8",
 "category": "profiles", "supplier": "Motedis", "part_number": "P30_N8_B",
 "supplier_url": "https://www.motedis.com/en/Profile-30x30-B-Type-Slot-8",
 "designation": "Profile 30x30 B8 L={L}",
 "build": {
   "role": "profile",
   "system": {"type": "B", "slot": 8, "grid": 30},
   "length": {"parameter": "L", "min": "10 mm", "max": "6000 mm", "stock": ["6000 mm"]},
   "section": {"width": "30 mm", "height": "30 mm"},
   "slot": {"depth": "9 mm", "opening": "8.2 mm", "inner_width": "16.5 mm"},
   "mass_per_metre": "0.9 kg",
   "frames": [
     {"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
     {"name": "core", "kind": "bore", "diameter": "6.8 mm"},
     {"name": "slot_x+_1", "kind": "slot"}, {"name": "slot_x-_1", "kind": "slot"},
     {"name": "slot_y+_1", "kind": "slot"}, {"name": "slot_y-_1", "kind": "slot"}]}}
```

A value is a number (mm, kg) or a text with a unit: `"9 mm"`, `"6 m"`,
`"0.9 kg"`, `"900 g"`. MitCad keeps unknown fields and ignores them.

The fields of the block:

- `role`: `profile`, `connector`, `fastener`, `panel`, `linear` or
  `cap`. Every role but `fastener` has `frames`.
- `system`: `type` (`I` or `B`), `slot` (the slot width in mm: 5, 6, 8,
  10) and `grid` (the series: 20, 30, 40, 45 and others). A profile with
  slot frames has `system`. A profile without slots (a steel tube,
  mitcad#873) has no `system` and no `slot`. A connector, a nut or a cap lists the systems that it fits
  in `systems`. `systems` is an array of the same objects. A system
  without `grid` fits every series of its type and slot.
- `length`: for cut-to-length components (profiles, rails, shafts, screws,
  racks, cut plates). `parameter` is the design's user parameter that
  sets the length. `min` and `max` are its limits. `stock` lists the
  stock lengths that the component is sold in. `default` (optional) is the
  length that the design has. A profile has `length`.
- `length.pitch`: for rails and racks,
  `{"distance": "60 mm", "first": "20 mm"}`. It is the hole spacing,
  kept from end A. The design's holes are a pattern that starts at
  `first` and repeats at `distance`, with as many holes as fit the
  length.
- Profiles have `section` (the outer size), `slot` (the depth from the
  face, the opening and the inner width) and `mass_per_metre`. MitCad
  uses `slot` to size panels and to fit nuts.
- `frames` names the design's **mount frames**. Each frame is a
  `joint_origin` feature of the design, named `mount:<name>`
  (`mount:slot_x+_1`), or one with a `mount` of that name
  ([Mount frames](#mount-frames)). A frame is where another component mates.
  Its x axis goes along the slot or the rail. Its z axis points as the
  rules below give. `name` can have letters, digits, `_`, `-`, `+` and
  `.`. `kind` is one of `end`, `slot`, `bore`, `face`, `rail`, `shaft`,
  `hole`, `tongue` and `groove`. `tongue` and `groove` are a profiled
  board's edges ([Timber components](#timber-components)). `accepts` (optional)
  lists the roles that can mate there
  (`["connector", "panel", "linear"]`). By default, everything that
  fits the system can mate there. `diameter` (optional) is a bore's or
  a hole's diameter.
- Caps have `section` (`width`, `height`). It is the same as their
  profile's section.
- Nuts (role `fastener`) list `systems` without `grid`. A nut fits every
  series of its slot. A nut has one `slot` frame.

**The direction of a frame's z axis.** Two frames that mate face each
other. Their z axes are opposite. Thus every joint that Build makes
between them is flipped
([Joints Build makes](../core/model-api/src/commands.md#placement)).

- These frames point out of their component, towards the component that mates
  there:
  - A profile's `end_a`, `end_b` and slot frames
  - A bracket's or a plate's `face` frames
  - A cap's `end` frame
  - A carriage's `rail` frame
  - A rail's `rail` frame
- A component that sits inside a slot points into the profile. Examples: a
  nut, an inner bracket, a slot-insert arm of a connector. Its frame is
  opposite the slot frame that it mates with, as a bracket's face frame
  is.
- A `bore` or `shaft` frame lies on its axis. A profile's `core` points
  out of end A. A bearing's or a nut's bore points along the axis about
  which the component turns or slides.

**Profile designs** obey one layout. Thus MitCad places and cuts every
profile the same way:

- The cross-section's sketch lies on the XY plane. The profile's centre
  is at the origin.
- One extrusion of `L` along +Z makes the body. End A is at z = 0. End B
  is at z = L.
- Slot frames sit on the face, at the slot's centre line, at z = 0.
  Their names are `slot_<side>_<n>`. `side` is the outward direction
  (`x+`, `x-`, `y+`, `y-`). `n` counts the slots of that side from the
  side's lower coordinate. A square profile has 1. The long sides of a
  30x60 have 1 and 2. A closed side has no frame.
- `end_a` points along −Z at the origin. `end_b` points along +Z at
  z = L. Both have x along +X.
- `core` is the central bore's axis at z = 0. It points out of end A, as
  `end_a` does.
- A profile without a central bore names its bores `core_1`, `core_2`
  and so on. A 40x80 has two. MitCad counts them from the lower y, then
  from the lower x.
- `end_b` follows the length. Its joint origin lies on a construction
  plane offset from XY by `L`. `end_a` is XY turned over. The slot frames
  are fixed planes at z = 0.

**Connectors** add a `connector` block:

```json
"connector": {
  "junctions": ["L", "T"],
  "space": "inner_corner",
  "mates": [{"frame": "mount_a", "to": "slot"}, {"frame": "mount_b", "to": "slot"}],
  "fasteners": [{"frame": "mount_a", "nut": {"library": "mitcad-parts", "component": "tnut-b8", "config": "M6"},
                 "screw": {"library": "mitcad-parts", "component": "iso4762", "config": "M6x12"}}],
  "machining": [{"on": "b", "frame": "end", "hole": {"diameter": "6.8 mm", "depth": "15 mm", "thread": "M8"}}]}
```

- `junctions`: where the connector can go. `L` is a corner. `T` is an
  end on a side. `cross`, `inline` (end to end on one axis) and
  `surface` (on a face, for panels' clamps and plates) are the others.
- `space`: `inner_corner` (inside the angle of an L or a T), `outer` (on
  the outside faces), `end` (in the profile's end, as an automatic
  connector) or `through` (through both profiles, as a cube connector's
  arms).
- `mates`: which of the connector's frames mate with which frame kind of
  the two profiles. Profile `a` is the one whose end meets. Profile `b`
  is the other.
- `fasteners`: the nut and the screw that each mate brings, and
  optionally a `washer`. They are library references. A configuration
  row is given by its name.
- `machining`: what the connector must have cut into a profile. `on` is
  `a` or `b`. `frame` is its `end` or `slot` frame. The cut is a hole
  with `diameter`, `depth` and an optional `thread`.
- `envelope` (optional): `{"x", "y", "z"}`, the room that an
  `inner_corner` connector takes. A junction whose inner corner is
  smaller does not take the connector.

`library_build_candidates`
([commands.md](../core/model-api/src/commands.md#library-commands))
offers connectors to a junction. A connector is offered when:

- Its `junctions` hold the junction's kind.
- Its `systems` fit both profiles' systems.
- Its `envelope` fits the junction's free room.

A connector that fails one of these conditions comes apart as
`refused`. Each refused entry is `{"id", "name", "library", "reason"}`.
The list shows it greyed. Two kinds of connector come first: a connector
that names the profiles' series, and a connector made for one kind of
junction only.

**Panels** add a `panel` block:

- `mount`: `slot` (in the slots of the frame around the panel) or
  `surface` (on the faces, held by clamps).
- `thickness`: a value, or the parameter that sets it.
- `width` and `height`: the parameters that size the panel. A fixed
  width is a configuration row's value and not a parameter. For
  example, a sheet is sold in widths.
- `clearance`: the room left in the slot's depth. The default is
  0.5 mm.
- `clamp`: a library reference to the clamp that a surface panel uses.

**Linear parts** add a `linear` block:

- `kind`: `rail`, `carriage`, `shaft`, `bearing`, `screw`, `nut`,
  `pulley`, `gear`, `rack`, `belt` or `plate`.
- `mate`: for a component that moves on another component,
  `{"frame": "rail", "on": "rail", "joint": "slider", "axis": "x"}`.
  `joint` and `axis` are the joint kind and its axis, as
  [joints](../core/model-api/src/commands.md#kinds-and-frames) have
  them.
- `size` (a text or a number): what components that go together share.
  Examples:
  - A carriage and its rail (`HGW15`)
  - A bearing and its shaft (`8 mm`)
  - A lead screw nut and its spindle (`Tr8x1.5`)
  - A pulley and its belt (`GT2`)

  Two sizes are the same when both are lengths of the same value (`8`,
  `"8 mm"`). Otherwise two sizes are the same when they are the same
  text, without case.
- Drive data: `lead` and `pitch` for screws (lengths), `module` (a
  length) and `teeth` (a count) for gears, racks and pulleys.

Rules for linear parts:

- A component whose `mate.on` is `slot` (a slider, a motor plate) goes on a
  profile's side, as a profile does. It goes on the slot of the side
  that you click, at the click's snaps.
- A lead screw nut mates with a `cylindrical` joint for now. It slides
  and turns on its spindle.
- A rack is a solid of its length, not a cut-to-length component. Its teeth
  cannot be made from an end section.

`library_build_candidates` and `library_search` take a picker's `kind`
and `size`. A carriage for a rail is
`{"role": "linear", "kind": "carriage", "size": "HGW15"}`. A part of
another size comes apart with the reason. A component without a size fits
any size.

**What `library check` checks.** `library check` checks the rules above.
In the design, it makes sure that:

- The parameters that the block names (`length.parameter`, a panel's
  `thickness`, `width` and `height`) are user parameters.
- Every listed frame is a `mount:<name>` joint origin. An unlisted joint
  origin is a warning.
- The rows' own `supplier_url` values are https addresses.
- A profile obeys the layout:
  - An extrusion by its length along +Z of a section on XY
  - `end_a`, `end_b` and the slot frames where the layout puts them
  - `end_b` on a plane parallel to XY, offset by the length

A library reference must name a component (and row) of the library
itself, or of a fetched library. A reference to a library that is not
fetched is a warning.

**Not expressible yet.** These components are in the library in the nearest
form:

- A profile with a different system on one side. Example: a 30x45
  B-type slot 8 with one slot-10 side. `system` is one for the whole
  profile.
- A junction of three profiles at a corner (a cube connector 3D, a
  corner connector 3D). Junctions are of two profiles, so these
  connectors list `L`.
- Slot frames on angled faces (the 45° sides of an R40/80). The layout
  names the `x+`, `x-`, `y+` and `y-` sides only. Automatic frames find
  only those.

### Timber components

Timber construction uses Build parts of its own:

- Members: sawn, planed and dimensioned timber
- Wood screws and nails
- Brackets, joist hangers and post bases, fitted by the members'
  sections
- Sheets laid on a member's face

None of them has a slot system. The slot path does not offer them. The
pickers list them only where timber is asked for (refer to **Pickers**
below). How the Build tab places and lists them is in
[commands.md](../core/model-api/src/commands.md#build-timber-construction).

```json
{"id": "timber-dimensioned", "path": "dimensioned-timber/timber-dimensioned.mitcad", "name": "Dimensioned timber",
 "category": "dimensioned-timber", "license": "MIT", "designation": "Dimensioned timber {Thickness}x{Width} L={L}",
 "build": {
   "role": "profile", "kind": "timber",
   "material": {"species": "spruce", "finish": "dimensioned", "grade": "", "density": 450, "moisture": 20},
   "availability": "standard",
   "section": {"width": "98 mm", "height": "48 mm"}, "nominal": {"width": "98 mm", "height": "48 mm"},
   "tolerance": {"thickness": [-1.0, 1.0], "width": [-1.0, 1.0]},
   "length": {"parameter": "L", "min": 100, "max": 6000, "default": 3000,
              "stock": [2700, 3000, 3300, 3600, 3900, 4200, 4500, 4800, 5100, 5400]},
   "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
              {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
              {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}],
   "rows": {"48x98": {"section": {"width": "98 mm", "height": "48 mm"}, "availability": "standard", "...": "..."},
            "48x248": {"section": {"width": "248 mm", "height": "48 mm"},
                       "nominal": {"width": "248 mm", "height": "48 mm"},
                       "availability": "less common", "material": {"species": "spruce", "density": 450},
                       "tolerance": {"thickness": [-1.0, 1.0], "width": [-1.5, 1.5]}}}}}
```

**Configuration rows' own blocks.** A component sold in sizes of different
data gives each row a block of its own in `build.rows`:
`{"<row name>": {a partial build block}}`. For example, a timber
family's sizes differ in section, species and availability. A wood
screw's sizes differ in length. A reader of a row merges its block over
the component's block: objects key by key, other values (numbers, texts,
lists) replaced. The component's own fields are those of its default row.
Thus a reader that knows nothing of rows sees a whole component.

`library check` makes sure that every key names a configuration row of
the design. It checks the rows' fields as the component's.

**Values.** A length or a mass is given as elsewhere in the block. A
`density` is in kg/m³ (a number, or a text with the unit `kg/m3`). A
`moisture` is in %.

**A member** (`role: "profile"`, `kind: "timber"`) has the layout of a
profile ([Build parts](#build-parts)):

- The section lies on XY, centred at the origin. Its width W goes along
  X. Its thickness T goes along Y. The extrusion by `L` goes along +Z.
- `section` `{width, height}` is the actual size (W, T). `nominal` is
  the nominal size, at 20 % moisture. A re-sawn size is 2 mm narrower
  than its nominal width.
- `material`: `species` (`spruce`, `pine` and others), `finish` (`sawn`,
  `planed`, `dimensioned`), `grade` (`""` by default, `C24` and others),
  `density` and `moisture`.
- `availability`: `standard` or `less common`. The picker lists the
  standard sizes first.
- `tolerance` `{thickness, width}`: each `[minus, plus]` in mm.
- `length`: as a profile's. Its `stock` lists the lengths sold.
- Frames: `end_a`, `end_b`, and a `face` frame on each side: `face_xp`,
  `face_xn`, `face_yp`, `face_yn` (the side that faces +X, −X, +Y, −Y).
  A face frame is at z = 0 on the side's centre line. Its z points out
  of the side. Its x goes along +Z, as a profile's slot frames. The face
  frames follow the width and the thickness of the row.
- A member has no `system`, no `slot`, no slot frames and no
  `mass_per_metre`. Its mass is the density times the volume.

**A profiled board** is a member as above whose section is shaped. It
has a `board` block. Each row has its own, through `build.rows`:

```json
"board": {"profile": "tongue_and_groove", "cover": "112 mm", "area": 3123.5,
          "tongue": {"length": "8 mm", "thickness": "8 mm"}}
```

- `profile` is the shape (the table below).
- `cover` is the width that the board covers in a floor, a wall or a
  deck. Without it, the cover is the board's width.
- `area` is the section's area in mm². The board's mass and volume come
  from it.
- `tongue` `{length, thickness}`, `overlap`, `edge_radius` and `grooves`
  (a count) are the sizes of its joint or its edges.

The section lies as a member's: its box is centred on the origin.
`section.width` is the full width, over a tongue, along X. The thickness
goes along Y. The visible face points towards +Y. The face frames are on
the box's sides.

A board that joins the next board has two more frames:

- `groove` (kind `groove`) on the box's −X side, with z along −X
- `tongue` (kind `tongue`), the cover width on from `groove`, with z
  along +X

Both are at y = 0, with x along +Z. The edge with the tongue goes under
the next board. Thus a board whose `groove` frame mates another board's
`tongue` frame lies beside it, tongue into groove. A row of boards steps
by the cover width.

The profiles are MitCad's own parametric sections. No manufacturer's
drawing is used. Their sizes and cover widths are those of public
product lists:

| `profile` | The section | Joins the next |
|---|---|---|
| `tongue_and_groove` | A tongue `TL` long and `TT` thick on the +X edge. Its centre is `TO` from the board's middle (below the middle in a floorboard). A groove on the −X edge, `GC` (1 mm) deeper and `GW` (0.5 mm) wider. Panel boards have a V-joint: the face's edges are chamfered `VB` wide at the angle `VA` to the face. Cover: W − TL | Yes |
| `shiplap` | Rebated. A lip `LT` thick and `LW` wide on the back of the +X edge. The −X edge's front laps over the lip of the board before it. Cover: W − LW | Yes |
| `feather_edge` | A wedge from the thick edge `T` to the thin edge `TN`. It overlaps the board under it by `O`. The section lies as the board is laid on a wall: turned by the angle at which its thick edge rests on the face of the board under it. Thus the boards of a row touch along that line only. They are the cover width apart: `sqrt((W - O)² + v²)`, where v is the board's thickness `O` from its thin edge | Yes |
| `deck` | The four long edges are rounded by `R`. A grooved board has `grooves` grooves `GG` wide and `GD` deep on the face, evenly over the width. Cover: its width | No. A deck keeps a gap |

**A wood fastener** (`role: "fastener"`) has:

- `wood`
  `{kind: "screw" | "nail", head: "countersunk" | "flat" | "pan" | "washer", diameter, length}`.
  Each row's diameter and length are in its own block.
- `material` `{product, density}`.
- The frame `axis` (kind `shaft`). It is on the axis at the head's
  bearing surface. Its z goes along the shank towards the tip. The tip
  is at z = `length`. A countersunk head lies under that surface
  (z > 0). The other heads lie on it (z < 0).

Timber's generated fasteners seat by `wood.seat`:

- `head_top`: the default for wood screws and nails
- `head_underside`: the default for pan, hex, washer, button and socket
  heads
- A named mount frame whose z points along the shaft

A `head_top` frame, when present, is exact even for an oblique source.
An older axis-aligned source does not need a new frame. Its body bounds
supply its head top. `head_underside` uses the axis bearing frame that
the source already has, unless the source lists a frame with that name.
When you select a row, the row can replace `seat`.

A head behind the axis drives the tip deeper by its height. The nominal
length still refers to the axis-to-tip distance. Metal connector hole
frames keep their bearing placement. Generic non-wood bolts and washers
use the connector frames that they already have, unchanged.

`wood.default: true` marks the wood fastener that the tools start
with when you have not selected one. `library check` accepts true or
false. The tools are Timber's frames and spacing sets, and a timber
junction's Connector. The wood picker (`library_build_candidates` with
`kind: wood`) lists the marked fastener first. MitCad's library marks
**Wood screw, countersunk**. It is driven flush (`seat: head_top`).
The library does not mark the pan-head **Connector screw**. That screw
bears on a metal connector's plate. On bare wood it stands proud.

Without a mark, the tools take the first screw that is driven flush
(`seat: head_top`, or a countersunk or bugle head without a seat).
Then they take the first screw, then the first wood fastener, in the
picker's order. A fastener that you selected stays the tool's while
the application runs.

**A bracket, a joist hanger or a post base** (`role: "connector"`) has:

- `connector.placement: "outer"`: on the members' outer faces. `space`
  is `outer`.
- `junctions`: as other connectors (`L`, `T`). A post base has
  `surface`.
- `fit` `{kind, section, width}`. `kind` is `angle`, `hanger` or
  `post_base`. `section` `[a, b]` is the actual section of the member
  that it carries (the joist, the post), either way round. `width`
  `[min, max]` is the range of that member's smaller dimension (a
  bracket).
- No `systems`.
- `mates`: its first frame, `mount`, places it. The design is built in
  that frame's coordinates. The frame is the design's origin and axes:
  - A hanger: at the middle of the joist's end, on the carrier's face.
    Its z points into the carrier. Its x goes up the joist's section.
    The seat is half the joist's height below, at −x.
  - An angle bracket: on the carrier's face, at the middle of the edge
    where the selected side of member a meets it. Its z points into the
    carrier. Its x is that side's outward normal. One arm lies on the
    carrier in the plane z = 0, towards +x. One arm lies on member a in
    the plane x = 0, towards −z.
  - A post base: at the middle of the post's end. Its z points into the
    post, along its axis. Its x is the post's +X. Its base plate is
    under it, at z < 0.
- `fasteners`:
  `[{"on": "a" | "b", "frames": [hole frames], "count": n, "screw" (or "nail"): {library reference with its row}}]`.
  There is one entry for each member that the fasteners go into. `a` is
  the member whose end meets (the joist, the post). `b` is the carrier.
  A hole frame (kind `hole`, with its `diameter`) lies on the
  connector's surface that the fastener's head bears on. Its z points
  out of that surface, towards the head. Thus it faces the fastener's
  `axis` frame.

**A sheet** (`role: "panel"`) has:

- `panel.mount: "face"`: laid on a member's face.
- `thickness`: its parameter.
- `width` and `height`: the parameters that the Build tab sets from the
  faces that the sheet covers.
- `stock`: `[[width, height], …]`, the sheet sizes sold, in mm.
- `material`
  `{product: "plywood" | "osb" | "chipboard", species, finish, grade, density}`.
- Thicknesses as rows.
- The design is a box from the origin. Its width goes along X, its
  height along Y and its thickness along +Z. Its back is on z = 0. Its
  `face` frame is at the origin, with z along +Z.

**Pickers.** `library_search` and `library_build_candidates` take `kind`
for timber too:

- `timber`: members
- `wood`: wood fasteners
- `outer`, `angle`, `hanger`, `post_base`: outer connectors
- `mount` `face`: sheets

A timber component is listed only when it is asked for:

- `role: "profile"` without `kind` (the Profile picker) lists no timber.
- `role: "panel"` without `mount` lists no sheets.
- `role: "fastener"` without `kind` lists no wood fasteners.
- A slot system's picker or junction lists none of them.

A junction of two members is offered wood fasteners and outer
connectors. `build_junctions` gives such a junction `wood`, with the
members' `sections`. An outer connector is offered when its `junctions`
hold the junction's kind and its `fit` holds member a's section (with a
tolerance of 1 mm) or its smaller dimension. The `fit` can come from the
component or from one of its rows. A connector that does not fit is greyed,
with the reason (`it fits 48 x 148, the member is 48 x 123`).
`mitcad-cli library search` and `library candidates` take `--kind` and
`--mount`.

### Versions

A library's versions are its commits. Released versions are tags
(`v1.0.0`). MitCad lists the tags, newest first, and the default
branch's tip as "latest, unreleased".

A design records, for each component, the commit of the version that you
selected. When you open the design, MitCad reads that commit, also after
the library has moved on. Only Library Parts › Update changes it. Keep
old tags. Designs refer to their commits.

## A community index

```
community-index/
  mitcad-index.json              {"format": "mitcad-index", "version": 1, "name": "..."}
  libraries/<id>.json            one entry per library
```

```json
{
  "format": "mitcad-index-entry", "version": 1,
  "id": "my-parts", "name": "My parts", "description": "...",
  "url": "https://example.org/you/my-parts.git",
  "homepage": "", "license": "CC-BY-4.0", "maintainers": ["You"],
  "tags": ["brackets"],
  "components": [{"id": "bracket-20", "name": "Bracket 20", "tags": ["corner"]}],
  "reviewed": [{"rev": "<40 hex digits>", "label": "v1.0.0", "date": "2026-10-05"}]
}
```

`components` lets MitCad find a library before it is fetched.
`reviewed` lists the versions that the index's maintainers looked at.

MitCad does not list an index's library when an enabled source has its
repository under another id, or when a fetched library `replaces` its
id (mitcad#949). The same repository is the same URL, or a source whose
clone has the library's newest commit or a version the index reviewed
(an address that redirects to a source's repository). The log says so
once: `Library index: mitcad-fasteners left out: same repository as
https://github.com/mitcad/parts.git`. Thus an index that still names a
library's former id shows no second group of the same parts.
An index only lists. The libraries stay where their authors keep them.
An index's checks, if it has any, only check entries.
`mitcad-cli library index-entry <folder> --url <url>` writes an entry
for a library.

## Licences

Every item must have a licence. MitCad hides items without a licence,
unless you ask for them. A community index accepts these licences. This
is not legal advice.

| SPDX | What you must know |
|---|---|
| CC0-1.0 | No conditions |
| CC-BY-4.0 | Credit the authors when you share designs that contain it |
| CC-BY-SA-4.0 | Credit the authors. Designs that you share with it can need the same licence |
| MIT, BSD-2-Clause, BSD-3-Clause | Keep the copyright and licence notice |
| Apache-2.0 | Keep the copyright, licence and notice files |
| CERN-OHL-P-2.0, -W-2.0, -S-2.0 | Permissive, weakly and strongly reciprocal hardware licences |

MitCad records the licence, the library, its URL and the version with
each component in the design, also for a copied component. Library Parts › Components
List shows them.

## Making and publishing a library

In the application, Tools › Libraries › Publish to Library does these
steps:

1. It adds the open design to a library folder of your own. When the
   folder is new, it makes the folder's manifest, a licence note and a
   git repository.
2. It makes an image of the view as the design's preview.
3. It records the design as a version.
4. It pushes the library to a remote that you give.
5. It writes the entry for a community index.

From the command line:

```
mitcad-cli library init my-parts --id my-parts --name "My parts" --license CC-BY-4.0 --author "You"
mitcad-cli library add my-parts bracket.mitcad --id bracket-20 --name "Bracket 20" --category brackets \
    --preview bracket.png
mitcad-cli library check my-parts
mitcad-cli version save my-parts/mitcad-library.json my-parts/brackets/bracket-20.mitcad ...
git -C my-parts tag v1.0.0
mitcad-cli remote add my-parts https://example.org/you/my-parts.git
mitcad-cli library index-entry my-parts --url https://example.org/you/my-parts.git
```

## MitCad's parts library

`mitcad-parts` ("MitCad components", MIT) is the one library of the components
that MitCad offers. It is kept in a repository of its own
(`https://github.com/mitcad/parts.git`). It holds:

- The metric fasteners, one component per kind, each for its ISO
  standard and the DIN standard that the ISO one replaced (mitcad#949):
  - Hexagon socket head cap screw, ISO 4762 / DIN 912, M1.6 to M24
  - Hexagon head screw, thread to the head, ISO 4017 / DIN 933, M1.6 to
    M24
  - Hexagon socket countersunk head screw, ISO 10642 / DIN 7991, M3 to
    M20
  - Hexagon nut, ISO 4032 / DIN 934, M1.6 to M24
  - Plain washer, ISO 7089 / DIN 125, M1.6 to M24

  The component ids stay (`iso4762`, `iso4017`, `iso10642`, `iso4032`,
  `iso7089`), and so do the rows of earlier versions. Each row names its
  standards in the component's `rows` ([Standards and suppliers of
  rows](#standards-and-suppliers-of-rows)):
  - A length in both standards' tables names both (`M6x10`: ISO 4762,
    DIN 912).
  - The DIN tables' own lengths are rows tagged DIN only (`M6x8`:
    DIN 912; `M5x7`: DIN 933).
  - A DIN size whose dimensions differ from the ISO one is a size of its
    own, its rows tagged DIN only: DIN 933 at M10, M12 and M14 (across
    flats 17, 19, 22 against ISO 4017's 16, 18, 21; size `M10 DIN 933`,
    row `M10x20 DIN 933`) and DIN 934 from M5 (lower nuts, and 17, 19, 22
    across flats at M10, M12, M14; row `M10 DIN 934`). The ISO size keeps
    its rows and their names, tagged ISO only.
  - The suppliers' lengths outside every table are rows tagged
    `non-standard` (socket head and hexagon head screws: `M6x6`), with
    the supplier's head (`M12x220 DIN 933`). The earlier rows that are in
    no table stay the same way (ISO 4017 `M10x16`, `M12x20`).
  - DIN 7991's head gives the countersink of ISO 10642's theoretical head,
    which the library models: the countersunk screw's rows are ISO 10642's,
    tagged DIN 7991 too where its table has the length. DIN 125-1 washers
    are ISO 7089's in every size.

  The ISO lengths are the standards' preferred ones. ISO 4017 has M16,
  M20 and M24 up to 200 mm (ISO 4017:2011 Table 1, product grade B over
  150 mm; up to 150 before mitcad#949). ISO 10642 M14 has the standard's
  head, dk 30.8 and k 8.4.

  The rows list Würth's and Etra's articles (`supplier_parts`): every
  article of the groups read from their catalogues on 2026-10-30, in
  steel 8.8, 10.9 and 12.9 zinc-plated or plain and stainless A2 and A4
  (screws), class 6 and 8, A2 and A4 (nuts), steel 200 HV, A2 and A4
  (washers). The articles are kept in
  `tools/libraries/fastener_suppliers.py` with the catalogue page of each
  group. A countersunk screw's length that no row has is left out. The
  part numbers are in each supplier's form:
  - Würth: the article number in its groups, separated by single spaces
    (`0084 6 8`). They are the groups of the article key of Würth's shop
    (`00846  8`, in the product address `.../00846%20%208.sku/...`): the
    first four digits, the next three characters without their filling
    spaces, then the rest. Back to the key: the middle group filled with
    spaces to three characters when a third group follows.
  - Etra: the product code as Etra's catalogue writes it, digits only
    (`701200206008`).

  The fasteners are simplified for assemblies: cosmetic threads, the heads' top
  edges rounded or chamfered, and no other chamfers or fillets
  ([Previews](#previews)). Each names CC0-1.0 as its own licence.
- The Build tab's slot profiles, connectors, nuts, caps, panels and
  linear parts. They are made parametric from the supplier's CAD files.
  Every component names its `supplier` and, when it has one, its
  `part_number`. Components that the supplier sells without a CAD file are
  modelled from their dimensions:
  - Sheets and plates
  - The round precision shafts h6 (6 to 25 mm)
  - The trapezoidal lead screws (Tr8x1.5 to Tr16x4)
  - The threaded rods DIN 976 (M5 to M12)

  The screws and the rods have their threads as cosmetic thread
  features.
- The timber components ([Timber components](#timber-components)). They are MitCad's
  own, in industry-standard sizes, and name no supplier:
  - Sawn, planed and dimensioned timber. Each is one component, with its
    sizes as rows. The less common sizes are marked.
  - Wood screws, connector screws, ring shank nails and structural
    screws
  - Angle brackets, joist hangers and post bases
  - Spruce and birch plywood, OSB and chipboard sheets

  Their group files' notes give the grounds of the densities.
- The profiled boards. They are MitCad's own parametric sections, in
  industry-standard sizes and cover widths, and name no supplier:
  - Tongue and groove floorboards and panel boards with a V-joint
    (category `tongue-and-groove`)
  - Shiplap and feather-edge cladding boards (`cladding`)
  - Deck boards with rounded edges, smooth or grooved (`decking`)

  Each is one component with its sizes as rows.
- The steel tubes (mitcad#873). They are MitCad's own, in the sizes of
  the standards, and name no supplier:
  - Round tubes to EN 10219-2 (outside diameters of EN 10220 series 1),
    21.3x2 to 323.9x16
  - Square tubes to EN 10219-2, 20x20x2 to 300x300x16
  - Square furniture tubes to EN 10305-5 Table 6, 15x15x1 to 100x100x4
  - Rectangular tubes to EN 10219-2, 40x20x2 to 400x300x16 (mitcad#892)
  - Rectangular furniture tubes to EN 10305-5 Table 6, 20x10x1 to
    120x60x4 (mitcad#892)
  - Round stainless steel tubes in the sizes of EN ISO 1127 Table 3
    (EN 10216-5 seamless, EN 10217-7 welded), 6x1 to 323.9x12.5, at the
    standard's conventional density 7970 kg/m³ (mitcad#892)

  Each is one component with its outside size and wall thickness as rows.
  The section is exact, with the standard's corner radii. Thus a row's
  mass per metre is the standard's. The Build tab cuts them to length as
  profiles.
- The base library of materials and print profiles (mitcad#640,
  [Materials and print profiles](#materials-and-print-profiles)): the
  generic filaments PLA, PETG, ABS, ASA, TPU and PA, the common Bambu Lab
  and QIDI printers with a 0.4 mm nozzle, and four process profiles for
  that nozzle.

Its categories are those of both kinds of component, in seven main groups
([Groups](#groups)), in this order:

- **Wood**: sawn, planed and dimensioned timber, tongue and groove,
  cladding and deck boards, sheets, wood fasteners and timber
  connectors
- **Slot profile**: profiles, connectors, mounting plates, panels,
  caps, covers, tubes and spacers, accessories, and the nuts and
  screws made for the slot (`slot-nuts`, `slot-screws`)
- **Steel**: the steel tubes (`steel-tubes`, `Steel/Tubes`)
- **Motion**: the linear parts (rails, carriages, shafts, lead screws
  and threaded rods, belts, gears), ball bearings and motors
- **General fasteners**: the ISO screws, nuts and washers (`screws`,
  `nuts`, `washers`)
- **Hardware**: handles and knobs, feet and base plates, castors and
  wheels
- **Workshop**

`make-profile-library.py` names each category under its main group
(`MAIN_GROUPS`: `Wood/Dimensioned timber`) and lists them in that
order. `make-fastener-library.py` names its own `General fasteners/…`.
`LICENSE` says which licence applies to what. `SOURCES.md` says where
both kinds of component come from.

**The fastener library before it.** Up to 1.0.0, the repository was the
fastener library `mitcad-fasteners`
(`https://github.com/mitcad/fasteners.git`). That address redirects to
the components repository. At 2.0.0, the repository became the parts library,
with the new id.

Designs made with `mitcad-fasteners` continue to work. MitCad reads a
component at the commit that its design recorded. It reads it from any clone
that has that commit, with the recorded id in its manifest there. Thus
a clone of the components repository reads them under their old id and
address. Library Parts › Update moves such a component to the components
library's versions.

Three generators make the library with `mitcad-cli`:

```
tools/libraries/make-parts-library.py --cli <mitcad-cli> --source <folder of the supplier's files>
    --out <clone of the parts repository> [--groups <folder>] [--version 2.9.0] [--work <folder>]
    [--render-worker <mitcad-render>] [--no-render] [--rerender] [--commit]
tools/libraries/make-fastener-library.py --cli <mitcad-cli> --out <folder> [--sizes M3,M5]
    [--standards iso4762,iso4032] [--version 1.0.0] [--render-worker <mitcad-render>] [--no-render]
    [--rerender] [--merge] [--commit]
tools/libraries/make-profile-library.py --cli <mitcad-cli> --source <folder of the supplier's files>
    --out <folder> [--groups <folder>] [--only id,id] [--version 1.0.0] [--work <folder>]
    [--render-worker <mitcad-render>] [--no-render] [--rerender] [--merge] [--commit]
tools/libraries/profiles/*.json       the BUILD parts, one file per group
tools/libraries/library_previews.py   the fasteners' studio and finishes (both generators use it)
```

- `make-parts-library.py` makes the library: first the fasteners, then
  the Build parts into the same manifest (`--merge`), then one
  `LICENSE`, `README.md` and `SOURCES.md`. Then it checks the folder.
  `--out` is a clone of the components repository, so that its versions
  stay. `--commit` records the result on top of them and tags it. The
  default tag is `v2.9.0`, the next version to publish.
- `make-fastener-library.py` builds the fasteners from the standards'
  dimension tables. Each fastener has its finish as its appearance and
  a rendered preview ([Previews](#previews)).
- `make-profile-library.py` makes the Build parts
  ([The Build parts](#the-build-parts)).
- Each of these two makes a library `mitcad-parts` of its own components on
  its own. Their tests do this. With `--merge`, each keeps the other's
  components in the manifest at `--out`.

### Previews

This section tells how the library's pictures are made.

**Fasteners.** The fasteners are the ISO screws, nuts and washers, and
the Build parts whose `build.role` is `fastener` or whose category is
one of nuts, screws, washers, bolts, nails or fasteners (such as
`nuts`, `screws` and `wood-fasteners`). The generators render them
with `mitcad-cli render` ([rendering.md](rendering.md#final-render))
in one studio setup, `STUDIO` in `tools/libraries/library_previews.py`.

The studio turns a copy of the design so that the component's axis is Z,
with its head up. The axis goes along a long component's longest extent, or
across a flat component's thinnest. The head is on the side that the component's
mass lies on. A group file's `"preview": {"up": [x, y, z]}` names the
axis instead. The camera sees the component in perspective from three times
its size away, from the front right and above: 25 degrees for a long
component, 35 for a compact one, 50 for a nut or a washer. A component at least
2.2 times longer than wide lies across the tile.

The render settings are:

- The `studio` environment at twice its strength
- Three softboxes. They are area lights in the component's frame. The component
  sizes them. Their power is set so that each gives the same light on
  every component. Each is placed where the camera sees it mirrored: a
  broad key streak along the left of a shank and a head's side, and a
  narrow rim streak near their right edge. These come from lights
  below the component, as a cylinder seen from above mirrors. A small
  softbox is mirrored in part of a head's top.
- No ground
- The `neutral` view transform, a quarter stop brighter
- 64 samples, denoised, with Cycles' fixed seed and no time limit
- 512 x 512 pixels with a transparent background

The generator lays the render over the tiles' background (sRGB 246,
247, 249, that of the drawn pictures). It crops the render to the component
with 6 % around it. Then it averages the picture down to 256 x 256, an
RGB PNG of a few tens of KB.

**Finishes.** A fastener's surface is its `appearance`. The ISO socket
screws (ISO 4762, ISO 10642) are black oxide. The other ISO components are
zinc plated steel. A Build fastener without an `appearance` gets one
by what its name, part number or id says: `stainless`, `A2` or `A4`
give stainless steel, `brass` gives brass, `black` or `oxide` give
black oxide. Otherwise it gets zinc plated steel. An `appearance` can
name a finish:

- `zinc_plated`: the design's `zinc_plated`, a light silver, roughness
  0.27
- `stainless`: the design's `stainless_steel`, roughness 0.22
- `black_oxide`: the design's `black_oxide`, a dark grey metal,
  roughness 0.22
- `brass`: the design's `brass_polished`

The designs keep the finish. Thus a placed component shows it too.

**Threads and heads.** Every screw's design has a cosmetic thread
feature on its shank (`modeled: false`). The `threads` query lists it.
The view shows it. It has no geometry. The thread's cone chamfers the
tip. That is the thread's `chamfer`, at the free end only: about the
thread's depth at 45 degrees. The head's top edge is rounded or
chamfered. Each is one feature. Thus assemblies stay light:

| Screw | Thread | Head |
|---|---|---|
| ISO 4762 | ISO metric coarse, over b = 2d + 12 mm from the tip (the whole shank when it is shorter) | The top edge rounded, radius 0.06 dk |
| ISO 10642 | ISO metric coarse, over b = 2d + 12 mm, at most the shank under the head | The top edge rounded, radius 0.03 dk |
| ISO 4017 | ISO metric coarse, to the head | The hexagon's six top edges chamfered 0.06 s |
| Wood screws, connector and structural screws | The nearest ISO 1478 forming screw thread (`ST4.8` for a 5 mm screw). It is a stand-in for the wood screw's own thread, which the thread feature has no profile of. It goes over 60 % of the length from the tip (the connector screw's to the head) | The head's outer edge rounded, radius 0.08 of its diameter |
| The slot systems' screws (made from the supplier's solids) | ISO metric coarse of the size in the name (`M8x20`), on the solid's cylindrical faces of that diameter. A self-forming screw (`S7x25`) gets the metric coarse thread of its diameter as a stand-in. A part whose solid models its thread itself (`"thread": "source"` in its group file: the `S12x30` self-forming screw's shank is thread rings, no plain cylinder) keeps that thread and gets none of its own | The supplier's |

A design's thread has its default row's size. A configuration row sets
parameters only. Thus another row's shank follows its size but keeps
that designation. The generator leaves out a feature that it cannot
make (an edge or a face not found) and warns. The component stays as it
was. The tip's chamfer diameter is an expression of the row's own
size. For ISO threads it is `d - 1.182532 * P`, the minor diameter
less a tenth of a pitch. For wood screws it is `0.58 * D`. Thus the
chamfer follows the rows that the designation does not. Nails, nuts
and washers have no thread.

The **previews** make the component's cosmetic thread modelled on the
rendered copy only (`edit_feature` with `modeled: true`: the same
faces, size, length and chamfer). Thus the picture shows the thread's
profile. When the copy cannot model the thread, the generator prints
`rendered without its thread (why)`. With `--strict` (all three
generators), a generator fails after it has made the library when a
fastener's preview was drawn, was rendered without its thread, or a
thread or a rounding was left out. The render-step test runs with
`--strict`.

**Only what changed is rendered.** A rendered PNG carries a `tEXt`
chunk `Mitcad preview` with the key that it was made from. The key is
a digest of the studio setup and the design file. A generator keeps
the picture in `--out` whose key matches. It renders the others.
`--rerender` renders them all.

**Without a render worker.** With `--no-render`, or when no worker is
found (`mitcad-cli render --list-devices` fails), the generators say
so once. Then they draw the pictures as before: an ISO fastener's
outline from the side, drawn from its dimensions, and a Build
fastener's bounds as a box. When a render fails, the generator draws
that component's picture and warns, with the component's name.

**Other parts.** A section component's picture is its section seen from end
B, drawn from its curves. The generator renders a `board`'s rows from
their ends (`row_previews`, [The Build parts](#the-build-parts)). It
renders the other Build parts from the iso view with the design's own
render settings. Fasteners have no `row_previews`. Their rows differ
in size only.

**The render worker.** The render worker is `--render-worker`. Without
it, the worker is `MITCAD_RENDER_WORKER`. Without that, it is
`mitcad-render` next to `mitcad-cli` (a package). In a build folder,
it is `app/mitcad-render` beside `tools/cli/mitcad-cli`. To rebuild
the library with rendered previews, run this from a build folder and
a clone of the components repository (on Windows, elsewhere the same
without `.exe`):

```
python tools\libraries\make-parts-library.py --cli build\dev\tools\cli\mitcad-cli.exe
    --render-worker build\dev\app\mitcad-render.exe --source <folder of the supplier's files>
    --out <clone of the parts repository> --work <folder outside it>
```

It prints `preview <id>: rendered|kept|drawn (<finish>)` for every
fastener. It ends with the library check. `--commit` records and tags
the result. To publish a version is the maintainers' step.

### The Build parts

`make-profile-library.py` makes them from the group files.

The supplier's files are not part of MitCad. `--source` points at a copy
of them. Only the designs made from them go into the library.

A group file is a JSON array of components. The generator also reads
`{"categories": [{"id", "name"}], "parts": [...], "left_out": [...]}`.
`categories` names the categories for the manifest, under their main
group ([MitCad's parts library](#mitcads-parts-library)). A name with
a slash is kept as it is. `left_out` lists
the supplier's files that the group makes no part of, each
`{"source", "reason"}`. Examples: an assembly, a copy of another component's
file, a file that holds another product.

The generator reads the files in the order of their names. Every
supplier file is the `source` of a component or of a row, or it is left out
with its reason. Each entry is one library component:

```json
{"id": "p30x30-b8", "name": "Profile 30x30 B-type slot 8", "category": "profiles",
 "source": "aluminium-profiles/Profile-30x30-B-Type-Slot-8/P30_N8_B.stp",
 "make": "section",
 "supplier": "Motedis", "part_number": "P30_N8_B",
 "supplier_url": "https://www.motedis.com/en/Profile-30x30-B-Type-Slot-8",
 "designation": "Profile 30x30 B8 L={L}",
 "rows": [],
 "build": {"role": "profile", "system": {"type": "B", "slot": 8, "grid": 30}, "...": "as above"},
 "frames": {"auto": "profile"}}
```

**The fields of an entry:**

- `id`, `name` and `make` are required.
- `category`: the folder and the category. The default is the role.
- `source`: a path in `--source`. It is required, except for `model`.
  For `model`, it names the file that the dimensions come from, and the
  generator does not import it.
- `supplier`, `part_number`, `supplier_url`, `designation`,
  `description`, `keywords`, `standard` and `license` go to the manifest
  as they are.
- `build` goes to the manifest with what the generator fills in (refer
  to **What the generator fills in** below).

**`make: "section"`** (profiles and other cut-to-length components: rails,
shafts, belts, spindles):

- The component is the cross-section at the source's end face, put on XY at
  the centre of its bounds. The section is a sketch of fixed lines,
  arcs, circles and splines. The generator extrudes it by the length
  parameter along +Z.
- `place` `{"z": [0, 1, 0], "x": [0, 0, 1]}` names the source's
  directions that become the component's z and x. The component's z is the
  profile's axis, along a model axis. The component's y is z × x.
- Without `place`, the component goes along the source's longest extent, or
  along `axis` (`x`, `y`, `z`). Then x and y go along the source's next
  axes in turn (y and z for an x axis).
- The generator takes the section at the end where the component's z starts:
  the planar faces lowest along z that face −z. It projects several
  faces into one sketch, each as its own region. It finds them without
  an assumption about their area. It extrudes the section along the
  component's whole length.
- A component with `build.length.pitch` (a rail) keeps its holes. The
  generator finds the holes across the component nearest end A in the
  source. These are circular edges whose axes are square to the component's
  axis, through or to their depth. A counterbore's two diameters and
  depth are among them. They become `hole` features at `first` from
  end A, and a rectangular pattern along Z at `distance`. The pattern's count is
  `max(1, floor((L - first) / distance) + 1)`. Thus a shorter component keeps
  the holes that fit. A counterbore without depth (a chamfer's edge) is
  a plain hole. When the source has no holes, or holes that cannot be
  placed (one that cuts nothing), the generator makes the component without
  them and warns.
- The generator warns when the design's volume per length and the
  source's differ. Then the design does not have holes or machining that
  the source has.
- `place` can also turn the centred section about Z (`rotate`, degrees)
  and move it (`shift`, [x, y]).
- The length parameter (`build.length.parameter`, default `L`) is the
  source's length, unless `build.length.default` gives another.

**`make: "solid"`** (connectors, nuts, caps, carriages, racks and
others): the source's bodies as a base feature, in the source's
coordinates, or moved by `place`. `place` is one of:

- `{"z", "x"}` as for a section
- `{"matrix": [[...], [...], [...]]}`, rows of a rotation and a
  translation
- `{"rotate": {"axis": "z" or [x, y, z], "angle": degrees}}`

Then `"translate": [x, y, z]` moves it.

**`make: "model"`** (no source: sheets, plates, shafts and screws sold
without a CAD file): `dimensions` gives `shape` (`plate`, `angle`, `rod`
or `none`) and its sizes. Each size is a parameter named in
`parameters`, or a value. A value becomes a user parameter `W`, `T`,
`L`, `A` or `B`. `parameters` (`{name: default}`) are user parameters,
such as a sheet's `W` that its rows set. The shapes:

- A `plate` (`width`, `thickness`, `length`) is centred on x = 0 across
  its width. It goes from y = 0 to its thickness and from z = 0 to its
  length.
- An `angle` (`legs` [a, b], `width`, `thickness`) is centred on x = 0
  across its width. Its first leg is in the XZ plane up to z = a, with
  its thickness along +Y. Its second leg is in the XY plane up to y = b,
  with its thickness along +Z.
- A `rod` (`diameter`, `length`) is a cylinder on the Z axis from z = 0
  to its length. This is the layout of a section component. Thus its `end_a`,
  `end_b` and `shaft` frames are those of a shaft. With `thread`, the
  rod has a cosmetic thread over its whole side. `thread` is a thread
  size as the
  [thread feature](../core/model-api/src/commands.md#thread) takes it:
  `standard`, `designation`, `class`, `right_handed`. The thread follows
  the length. The `threads` query lists it. Examples:
  `{"standard": "trapezoidal", "designation": "Tr8x1.5", "class": "7e"}`
  for a lead screw, `{"designation": "M8"}` for a threaded rod.
- `commands` (optional) add more. They are MitCad commands as
  `mitcad-cli run` takes them.

The [timber components](#timber-components)' shapes:

- A `board` (`width`, `thickness`, `length`) is a rectangle centred on
  XY, with its width along X. The generator sketches it with its sides
  dimensioned by the parameters. It extrudes it by the length along +Z
  (the profile layout). Without `frames`, the board has `end_a`, `end_b`
  and the four face frames. The face frames lie on planes offset by
  `W / 2` and `T / 2`. Thus they follow a row's size.
- A `board` with a `profile` is a profiled board
  ([Timber components](#timber-components)). `profile` is `tongue_and_groove`
  (with `"v_joint": true` for panel boards), `shiplap`, `feather_edge`
  or `deck` (with `"grooves": n` for a grooved face). Its `width` and
  `thickness` are the parameters `W` and `T`. Its profile's parameters
  are user parameters with defaults of their own:
  - `TL`, `TT`, `TO`, `GC`, `GW`, `VB` and `VA` for tongue and groove
  - `LW` and `LT` for shiplap
  - `TN` and `O` for feather edge
  - `R`, `GG` and `GD` for deck

  `parameters` can replace the defaults (an angle as `"45 deg"`). A
  feather-edge board's turn, cover and box parameters (`FA`, `COV`,
  `HX`, `HY`, `YC`) are derived from them.

  The section is a sketch of its points, and of a deck board's corner
  arcs. Each point is dimensioned from a fixed point at the origin by
  signed horizontal and vertical distances. Their values are expressions
  of those parameters. Thus a row, or a changed tongue or radius,
  reshapes the section.

  Its face frames follow the box. Its `groove` and `tongue` frames
  follow the cover width. The generator checks the design's volume
  against the area that it calculates for the `board` block.
- A `fastener` (`diameter`, `length`, `head_diameter`, `head_height`,
  `head`: `countersunk`, `flat`, `pan`, `washer`) is a shank on the Z
  axis from z = 0 to its length, and a cylindrical head. The head is
  under z = 0. A countersunk head is in z > 0. The `axis` frame is at
  the origin.
- A `sheet` (`width`, `height`, `thickness`) is a box from the origin,
  with a `face` frame there.
- A `bracket` (`legs` [on the carrier, on the member], `width`,
  `thickness`, `hole`, `holes` `{"b": [[x, y], …], "a": [[depth, y], …]}`),
  a `hanger` (`width` and `height` of the joist, `hanger_height`,
  `depth`, `flange`, `thickness`, `hole`) and a `post_base` (`width` of
  the post, `height`, `plate`, `plate_thickness`, `side`, `thickness`,
  `hole`) are steel plates of those fixed sizes (boxes joined). They are
  built in their `mount` frame's coordinates. Their screw holes are cut.
  The holes are made frames `hole_a_<n>` (into member a) and
  `hole_b_<n>` (into b). A frame's origin can also name a parameter in
  its x or y, when its z is along X or Y.

The steel tubes' shapes (mitcad#873). Each size names a parameter of the
design, and the rows set them:

- A `round_tube` (`diameter`, `thickness`, `length`) is a circle of the
  diameter on XY, extruded by the length along +Z. Its bore, of the
  diameter less twice the thickness, is cut along all of it. Its frames
  are `end_a`, `end_b` and `axis` (kind `shaft`, on the Z axis).
- A `square_tube` (`size`, `thickness`, `outer_radius`, `inner_radius`,
  `length`) is a square of the size centred on XY, its corners rounded by
  the outer radius, extruded by the length along +Z. Its bore is a square
  of the size less twice the thickness, its corners rounded by the inner
  radius, cut along all of it. An inner radius of 0 is a sharp corner.
  When some rows have sharp corners and others rounded ones, the design
  has both bores. A condition of the inner radius suppresses the one that
  a row does not use ([Bindings](configurations.md#bindings-appearance-material-suppression-visibility)).
  Its frames are `end_a`, `end_b` and the four face frames, on planes
  offset by half the size. Thus they follow a row.
- A `rect_tube` (`width`, `height`, `thickness`, `outer_radius`,
  `inner_radius`, `length`; mitcad#892) is a `square_tube` of the width
  along X and the height along Y. Its face frames are offset by half the
  width (`face_xp`, `face_xn`) and half the height (`face_yp`, `face_yn`).
- The section's points are dimensioned from a fixed point at the origin,
  as a profiled board's are. The generator checks the design's volume
  against the exact area of the section in its default row. With both
  bores, it also checks a row of the other kind of corner.

**`sizes`** (instead of `rows`, for a `board`, a `fastener`, a `sheet`
or a tube) makes the rows from a compact table:

- A board's
  `{"species": {name: density}, "species_default", "resawn": 2, "tolerance": {"thickness": [[up_to, minus, plus], …], "width": …}, "rows": [{"thickness", "widths": [...], "species", "availability", "resawn": true}]}`
  makes a row `<thickness>x<width>` for each width. The selectors are
  Thickness and Width. The values are W and T. Each row has its own
  block: section, nominal, availability, material and tolerance. The
  tolerance is the first rule whose `up_to` is the size or more, or
  null.
- A profiled board's rows also take `profile` and `covers`. `profile`
  holds its profile's parameters, in `sizes` for every row and in a row
  for its widths. `covers` has one entry for each width: a tongue and
  groove board's or a shiplap's cover width. The cover sets `TL` or `LW`
  to the width less the cover. Every profile parameter is a value of the
  row. The generator calculates the row's `board` block (profile, cover,
  area, joint) from them. It refuses sizes that make no section:
  - A tongue over half the width
  - A groove outside the thickness
  - A V-joint that reaches the groove
  - A radius over half the thickness
  - Grooves that do not fit
- A fastener's
  `{"rows": [{"diameter", "head": [diameter, height], "lengths": [...]}]}`
  makes a row `<d>x<length>`. The selectors are Diameter and Length. The
  values are D, L, DK and K, and `wood.diameter` and `wood.length`.
- A sheet's `{"rows": [{"thickness", "availability"}]}` makes a row of
  each thickness (T).
- A round tube's `{"rows": [{"diameter", "thicknesses": [...]}]}` makes a
  row `<D>x<T>` for each wall thickness. The options are Diameter and
  Thickness.
- A square tube's
  `{"radii": [[up_to, outer, inner], …], "tolerance": {"thickness": [[up_to, plus_minus, factor], …]}, "rows": [{"size", "thicknesses": [...], "tolerance"}]}`
  makes a row `<B>x<B>x<T>` for each wall thickness. The options are Size
  and Thickness. The corner radii are the first rule of `radii` whose
  `up_to` is the thickness or more, or null. `outer` and `inner` are
  factors of the thickness: `[[6, 2.0, 1.0], [10, 2.5, 1.5], [null, 3.0, 2.0]]`
  for EN 10219-2. An inner factor of 0 makes a sharp corner. The
  thickness tolerance is plus or minus `plus_minus`, or `factor` times
  the thickness when that is smaller. A row's `tolerance` is plus or
  minus that on its size.
- A rectangular tube's sizes are a square tube's whose rows have
  `width` and `height` for `size`. They make a row `<W>x<H>x<T>`; the
  options are Size (`<W>x<H>`) and Thickness. A row's `tolerance` is on
  both sides.
- A tube's rows are sorted by size, then by wall. The values are its
  sizes' parameters (a square tube's radii too). Each row has its own
  block: `section`, `mass_per_metre` (the exact area times the density,
  which the component or its group file names), `tolerance` and `tube`
  (`shape`, `thickness`, a square tube's `outer_radius` and
  `inner_radius`, and `area` in mm²).

A row's `build`, as `sizes` makes it or as written, is the row's own
block: the manifest's `build.rows`. The component's block takes its default
row's.

**`density`** (a component's or the group file's, kg/m³): a section's mass
per metre is its volume per length times the density. When neither
names a density, it is aluminium's, 2.7 g/cm³. A component's
`build.material.density` defaults to it.

**`appearance`**: a library appearance's id (`steel_satin`), a
finish's name (`zinc_plated`, `stainless`, `black_oxide` or `brass`,
[Previews](#previews)), or one of the design's own
`{"id", "name", "based_on", "base_color", "roughness", "texture", "size"}`.
A fastener without an `appearance` gets a finish.
The generator draws and embeds the texture: `grain` (wood, with the
grain along the image's height), `strands` (OSB) or `particles`
(chipboard). The texture repeats every `size` mm. It is projected as a
box in the body's coordinates. Thus a board's grain goes along its
length and a sheet's along its height. Every body of the design takes
the appearance.

A `board` with a `grain` gets a second appearance of its own,
`<id>-end`, on the two caps of its extrusion. It shows growth rings (a
400 mm tile, with the pith off the section). The members placed from
the board show the rings on their ends.

**`data_source`** (a part of no supplier and no source file): what its
sizes are, for SOURCES.md (`industry-standard sizes of sawn timber`).

**`rows`**: configuration rows (`name`, `select`, `values`,
`part_number`, `supplier_url`, `designation`, `source`) for components sold
in sizes. `values` can be empty: one solid for every thread size, or a
rail's stock lengths that only carry their part numbers. The row's
`part_number`, `supplier_url` and `designation` go into the design's
table.

A row's `source` is the supplier's file of that row's product: another
thread, colour or stock length of the same component. The design is made
from the component's `source` and stands for every row. SOURCES.md lists the
row's file. `default` names the default row. Without it, the first row
is the default.

**`frames`**:

- `{"auto": "profile"}` (the default for `section`) finds the ends,
  bores and slots of a profile from its section. It names them by the
  layout above. A slot is a gap between the straight pieces of the
  outline on a side. A bore is a circle inside the outline.
- `{"auto": "profile", "add": [...]}` adds frames to those found.
- A frame found that `build.frames` does not list, or a frame listed and
  not found, is reported as a warning.
- Otherwise `frames` is a list of `{"name", "kind", "origin", "z", "x"}`
  in the component's coordinates after `place`. `x` defaults to +X, or to +Z
  when `z` is along X or Y. `diameter` and `accepts` are as in the
  manifest.
- The origin is in mm. Its z can instead name the length parameter
  (`[0, 0, "L"]` for `end_b`, `[10, 0, "L / 2"]`), when the frame's z is
  along ±Z. Then the frame lies on a plane offset by that expression
  from XY, or from a plane through its x and y. Thus it follows the
  length.
- `end_a` and `end_b` without an origin, in a component with a length, are XY
  turned over and the plane at the length.
- The manifest's `build.frames` are the frames made in the design, no
  more and no fewer. A warning names the frames that the group file's
  `build.frames` listed and the design does not have, and the frames
  that it adds. `accepts`, `diameter` and `kind` of a frame listed in
  `build.frames` are kept for the frame of the same name.

**What the generator fills in:**

- `build.frames`.
- For a profile, `build.section` (the section's bounds),
  `build.slot.opening` (the narrowest slot found) and
  `build.mass_per_metre` (the design's volume per length in aluminium,
  2.7 g/cm³). Each only when the group file has none.
- `build.length.parameter` for a section.
- `build.rows` (the rows' own blocks) and the default row's fields.
- An outer connector's `fasteners` `frames` and `count`, when an entry
  names only the member (`on`) that they go into.
- A profiled board's `build.board`: its rows', and without rows, the
  component's.
- A tube's rows' `section`, `mass_per_metre`, `tolerance` and `tube`,
  from its `sizes`.

**The designs.** The designs hold their features in the root, as a
component: no components, occurrences, active component or root policy. Each
starts from a blank design that `mitcad-cli` makes, not from MCP's
`document_create`, whose root is an assembly. A design that has them is
left out. Their timelines:

- A section component has `Section` (the sketch) and `Profile` (the
  extrusion). With a pitch, it also has `Hole plane <n>`, `Hole <n>` and
  `Holes at the pitch`.
- A solid has its base feature and `Placed` (the move, with `place`).
- A model has its boxes and `Centred`. A rod has its `Rod` and, with a
  thread, `Thread`. A tube has `Section` and `Tube` (its outside), then
  `Bore section` and `Bore`, and `Sharp bore section` and `Sharp bore`.
- Then the joint origins come: `mount:end_a`, `End B plane` and
  `mount:end_b`, then the others in their order. A joint origin that
  follows a parameter comes after its plane (`Plane of <name>`).
- The configuration table comes last.

**How the generator runs.** The generator imports a source with
`mitcad-cli import`. It models with `mitcad-cli mcp --workspace`:
commands, and the queries whose answers it needs (the end face, the
section's curves, its regions). It works in a temporary folder in
`--work`. The default is the folder above `--out`, not in a MitCad
project.

The generator draws a section's preview as the section seen from end B.
It renders a fastener's preview in the studio ([Previews](#previews)).
It renders the other previews with `mitcad-cli render`. With
`--no-render`, or when a render fails, it draws their bounds as a box.
It renders a `board`'s rows each from beyond its end B, as a piece 1.2
times its larger side long. Thus the tiles show the sections
(`row_previews`). The component's picture is its default row's.

Then the generator writes the manifest, an MIT `LICENSE`, `README.md`
and `SOURCES.md` (the source file, supplier, part number and URL of each
component). It checks the folder with `library check`. Errors stop it. With
`--commit`, it records and tags the result (`v<version>`).

A component that the generator cannot make is reported by id on one line
(`left out <id>: <reason>`) and left out. The causes:

- A file that is not there
- A source that `mitcad-cli import` cannot read
- No end face
- A section with an ellipse
- Regions that do not make the end faces' areas
- A body that fails

A program's failure is reported with its exit status and the last lines
of its output. The run fails only on a broken group file:

- Not JSON
- A component without `id`, `name` or a known `make`
- An id used twice
- A frame without a name, kind or numbers

MitCad lists the library among its default libraries (Libraries…), as
MitCad's one library of components. MitCad fetches it when you ask.

**Tests:**

- ctest `cli.profile-library` (`tools/cli/profile-library-test.cmake`)
  runs the generator on these components:
  - Two slot profiles. One is turned to go along Y and placed back,
    with a frame at half its length.
  - A bracket that the test makes and writes as STEP
  - A rail with two counterbored holes at a pitch. The test counts its
    holes at three lengths by the volume.
  - A panel and an angle modelled from their dimensions
  - A shaft and a Tr8x1.5 lead screw modelled as rods. The test checks
    their length, end B and thread at two lengths.

  It has a row's own file and a file left out in SOURCES.md.
- ctest `build.profiles` (`tools/cli/build-profiles-test.cmake`) runs on
  the generated library that `MITCAD_PROFILES_LIBRARY` names. It
  places:
  - A profile of each system
  - Linear parts on a frame
  - Slot nuts in their M5 rows and a cover cap
  - A precision shaft with its bearing and support
  - A lead screw with its nut
  - The steel tubes in the standards' check sizes. Their cut-list masses
    are within 1 % of the standards'. A square tube goes on another's
    face.
- ctest `cli.timber-library` (`tools/cli/timber-library-test.cmake`)
  runs the generator on small timber group files: a board family with a
  re-sawn and a less common size, wood screws, a bracket, a hanger, a
  post base and a sheet. It checks:
  - Their manifest fields and rows' blocks
  - The board's section and face frames that follow a row
  - The fastener's head
  - The connectors' holes
  - The pickers

  It also runs on profiled boards: tongue and groove with and without a
  V-joint, shiplap, feather-edge and a grooved deck board. It checks
  their `board` blocks per row, their volumes, and their `tongue` and
  `groove` frames that follow a row and a changed parameter.

  It also runs on a round, a square and a furniture tube, a rectangular
  tube and a rectangular furniture tube (mitcad#892). It checks
  their rows' corner radii, masses per metre and tolerances, and their
  volumes against the exact areas and the standards' masses. It checks
  that the furniture tube's rows switch its bores, and that the tubes are
  among the profiles in the pickers.
- ctest `build.timber` (`tools/cli/build-timber-test.cmake`, on
  `MITCAD_PROFILES_LIBRARY`) checks every timber part of the generated
  library:
  - The sizes and their marking
  - The sections, densities and masses of rows of each family
  - The fasteners' and connectors' frames
  - The profiled boards' rows, covers, areas (the designs' volumes) and
    frames

  It places a floorboard on another's tongue and a shiplap row that
  steps by its cover.
- ctest `cli.library-previews` (`tools/cli/library-previews-test.py`)
  renders two screws in the studio: an ISO 4762 screw (black oxide, by
  the fastener generator) and the countersunk wood screw of
  `timber-fasteners.json` (zinc plated, by the profile generator). It
  checks:
  - Each is a 256 x 256 PNG within 512 KB, and not blank.
  - Each reads as metal. The zinc plated screw is a light silvery grey
    with near-white highlights. The black oxide one is a dark grey
    with visible highlights. Only its thread grooves in deep shadow
    can be black. Neither is flat (the lumas inside their edges).
  - `library check` passes.
  - A second run keeps the picture.
  - Without a worker, the generator draws it.

  The test is skipped where the render worker cannot render.

## Materials and print profiles

Materials and printer and process profiles of 3D printing (mitcad#640)
are items of libraries too: one JSON file each, in MitCad's own format.

```
materials/pla.json                    a material: density, appearance, filament
materials/acme/petg-hf.json           a vendor's material, id acme/petg-hf
print/printers/bambu-lab-a1.json      a printer profile
print/processes/standard-0-20.json    a process profile
```

```json
{"format": "mitcad-print-item", "version": 1,
 "material": {"id": "petg", "name": "PETG", "class": "plastic", "density": 1.27, "appearance": "plastic_clear",
              "filament": {"filament_type": "PETG", "temperature": 240, "bed_temperature": 75},
              "version": 1}}
{"format": "mitcad-print-item", "version": 1,
 "profile": {"id": "bambu-lab-a1", "name": "Bambu Lab A1 (0.4 nozzle)", "kind": "printer",
             "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]], "max_print_height": 256,
                        "nozzle_diameter": 0.4, "filament_slots": 4,
                        "printer_settings_id": {"bambu_studio": "Bambu Lab A1 0.4 nozzle"}},
             "version": 1}}
```

- An item is one `material` (with an `appearance` object beside it when
  its appearance is not one of MitCad's library appearances) or one
  `profile`. The fields are those of the design's materials and profiles
  ([commands.md](../core/model-api/src/commands.md#print-profiles)):
  values of the print settings' key map by MitCad's keys.
- A material's `class` (`metal`, `plastic`, `other`) is the group it is
  listed in; a file without one is grouped as MitCad works it out (a
  metal by its id or the built-in it is based on, a plastic by its
  filament settings).
- The file is named after the item's id in its kind's folder; a vendor's
  item `<vendor>/<product>` is in the vendor's folder.
- `version` (an integer) counts up with each change of the item. A
  design keeps a complete copy of the items it uses with the version it
  copied (the app never shows the copy as such, mitcad#655). An edit made
  in the design writes the item's file and the copy together, one undo
  step that writes the file back too. When another design changed the
  item (a newer version with other values), the design asks once: it
  takes the change, or keeps its values and remembers that version as
  declined until the item changes again.
- Values are a library's own: a printer carries what MitCad uses (the
  bed, the printable height, the nozzle, the filament slots, the printer
  model and the slicers' preset names), never a slicer's machine
  settings or system presets.

Three libraries hold them; the same kind and id in several: the nearest
wins.

| Library | Where | |
|---|---|---|
| project | `print-library/` in the project's folder | versioned with the project |
| user | `print-library` in the platform's data folder under `Mitcad/Mitcad`, beside `libraries` (`MITCAD_PRINT_LIBRARY_DIR` replaces it) | the user's own |
| base | MitCad's parts library, its newest version fetched | read only |

The user's and the project's library are managed in the application
without a design: Tools › My Library… ([user guide](user-guide.md#my-library),
mitcad#704) makes, duplicates (also from the base library and the
built-in materials), renames, edits and deletes their items, and
imports a slicer's project or preset into them. It runs the library
commands of [commands.md](../core/model-api/src/commands.md#print-profiles)
(*Items where they live*, `import_print_profile` with `into`) on a model
of its own, so an open design changes only through its own question
about an item changed elsewhere. A library can also be written by hand
in this format, or with `mitcad-cli run` and the same commands.

A library repository lists its items in its manifest, each with its id
and path:

```json
"materials": [{"id": "pla", "path": "materials/pla.json"}],
"printers": [{"id": "bambu-lab-a1", "path": "print/printers/bambu-lab-a1.json"}],
"processes": [{"id": "standard-0-20", "path": "print/processes/standard-0-20.json"}]
```

The check of a manifest refuses paths outside their folders and an id
listed twice. MitCad's parts library takes its items from
`tools/libraries/print-library/` (with their `SOURCES.md`):
`make-parts-library.py` copies them and lists them in the manifest.

## Where libraries are kept

The application and CLI share fetched bare clones in the platform's data
folder under `Mitcad/Mitcad/libraries`: `%LOCALAPPDATA%` on Windows,
`~/Library/Application Support` on macOS, and `$XDG_DATA_HOME` (or
`~/.local/share`) on Linux. `MITCAD_LIBRARIES_DIR` replaces that folder.
The default configuration also reads the earlier CLI cache under
`mitcad/libraries`, preserving versions already fetched there without moving
or deleting them; new fetches use the shared folder. Explicit overrides do
not add this legacy cache. There is
one clone for each URL. MitCad makes and updates the clones with the
system's git only when you ask, or at start-up for a source with
Auto-update (the check box in Libraries). These commands ask for it:

- Libraries › Fetch
- Update in the library browser
- Library Parts › Check for Newer Versions
- Get Library

A source with Auto-update is fetched in the background when MitCad
starts, without a dialog. The setting `autoUpdate` is kept with the
source's `url` and `enabled` (`libraries/sources`) and is off when it is
missing. The CLI and the MCP server do not fetch at start-up.

Updates add branches and tags. They remove nothing. Git's automatic
garbage collection is off there. Thus the versions that designs use
stay readable.

A clone may have at most 200 MB, a new one and an updated one alike
(mitcad#938). An update fetches the branches and tags under
`refs/incoming/` first. Only when the clone is 200 MB or less then do
they replace `refs/heads/` and `refs/tags/`. A larger clone gives the
error `too_large` ("the versions fetched before stay"): the fetched
references and the object files the fetch brought are removed again, so
the clone has the versions it had before, and the next fetch (also an
Auto-update at start-up) tries again.

A design whose library is not on the computer keeps the bodies saved
with it, and says so. Library Parts › Get Missing Libraries shows their
addresses and then fetches them.
