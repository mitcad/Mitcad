# CAM: toolpaths for CNC milling

MitCad's CAM (mitcad#1013) turns bodies and sketches into toolpaths and
writes them as G-code for the controller of a CNC router or mill. This
page is the design: what was learnt from the open-source CAM projects,
the scope by phases, the data model, the machine model with its
collision zones and tool changers, the post-processors, simulation, the
API and the UI. The first round (the crate `core/cam`, see
[Implemented](#implemented)) is on its own branch and is not merged into
features yet.

Contents: [Open-source CAM](#open-source-cam) ·
[Decisions](#decisions) · [Phases](#phases) ·
[Data model](#data-model) · [Tools](#tools) ·
[Machine model](#machine-model) · [Tool changers](#tool-changers) ·
[Spindle assembly](#spindle-assembly) ·
[Operations](#operations) · [Post-processors](#post-processors) ·
[Simulation and checks](#simulation-and-checks) · [API](#api) ·
[UI](#ui) · [Crates](#crates) · [Implemented](#implemented) ·
[Open questions](#open-questions) · [Next steps](#next-steps)

## Open-source CAM

What the main open-source projects do, which algorithms they use and
how they describe tools, machines and post-processors. Only projects
under MIT, Apache, BSD or BSL licences may be used as code (the
[licence policy](development.md#licence-policy)); GPL and LGPL projects
are read for ideas and behaviour only, never copied or ported.

| Project | Licence | What it does well | Taken as an idea |
|---|---|---|---|
| FreeCAD's CAM workbench (Path) | LGPL-2.1+ | The broadest open-source milling set: profile, pocket, adaptive clearing, facing, helix, drilling and tapping, engraving and V-carving, slot, deburr, thread milling, 3D surface and waterline, probing. A *Job* holds the model, the stock, the setup and the *tool controllers* (a tool bit with speeds and feeds); *dressups* change a finished path (dogbones, holding tags, ramps, lead in and out, boundary). Tool bits are shape files plus JSON bits and libraries. Post-processors are one script per controller (grbl, LinuxCNC, Mach3/Mach4, Smoothie, Centroid, Fanuc, a RepRapFirmware one and many more), lately moved to classes over a shared base. | The job → setup → operations structure; tool controllers; dressups as path modifiers; a shared post base with thin per-controller parts. |
| FreeCAD's adaptive clearing (in its copy of libarea, `Adaptive.cpp`) | LGPL-2.1+ | Constant-engagement roughing: each step searches the direction whose cut area matches the target, on Clipper's polygon booleans; helix entry found by shrinking the region to its middle. | The engagement-area search as the model for our own adaptive clearing (written from the published idea, not from the file). |
| Kiri:Moto (grid-apps) | MIT | Browser CAM for routers and lasers: roughing, outline, contour, pocket, trace, drill, register and flip, lathe; a *device* has its travel, maximum spindle and G-code macros for the header, footer, tool change, dwell and spindle. | Machine profiles with text macros at fixed points (our tool changer lines). |
| OpenCAMLib | LGPL-2.1 | 3D: drop-cutter and push-cutter (waterline) for flat, ball, bull, cone and composite cutters on triangle meshes. | The cutter shapes and the drop-cutter / waterline split for the 3D phase, written in Rust on the kernel's meshes. |
| PyCAM | GPL-3.0 | Early 3-axis CAM from meshes (drop-cutter, push-cutter, contour). | Ideas only. |
| BlenderCAM | GPL-3.0 | 3-axis and 4-axis strategies, simulation as a height map. | Height-map simulation for 3-axis stock. |
| cavalier_contours | MIT OR Apache-2.0 | Offsets of polylines of lines and arcs, arcs kept as arcs, with islands (shape offsets) and booleans; pure Rust. | **Used** for the tool radius compensation and offset pocketing. |
| Clipper2 | BSL-1.0 | Robust polygon booleans and offsets (lines only); the base of most adaptive clearing code. | A candidate for adaptive clearing's area booleans (or cavalier_contours' booleans). |
| OpenVoronoi | LGPL-2.1 | Voronoi diagrams and medial axes of polygons (V-carving). | Medial axis for V-carving, written in Rust when that phase comes. |

The controllers (all read for their G-code only): grbl 1.1 (GPL-3.0)
has no tool change, its M6 is an unsupported command, so a manual change
is a pause (M0) or the sender's job; grblHAL changes tools itself (M6
through its tool change modes `$341`, an ATC driver or a macro) and has a
dynamic tool length offset (G43.1); RepRapFirmware selects a tool with
`T`, which runs the tool change macros (`tfree`, `tpre`, `tpost`), takes
dwell seconds in `S` and comments after `;`; MillenniumOS (GPL-3.0, macros
for RepRapFirmware) defines tools with `M4000 P R S`, starts and stops the
spindle with `M3.9`, `M5.9` (waiting for the speed), has air blast `M7.1`,
probing cycles (G6500 to G6520, G6600) and parks with G27; Mach3 and
Mach4 run `T M6` through their M6 macros and take the tool length from
the tool table with `G43 H`; LinuxCNC likewise, with `%` around the
program, path blending `G64 P`, and a DIY changer remapped to M6 in the
machine's configuration.

Sources: [FreeCAD's CAM operations](https://github.com/FreeCAD/FreeCAD/tree/main/src/Mod/CAM/Path/Op),
[its post-processors](https://github.com/FreeCAD/FreeCAD/tree/main/src/Mod/CAM/Path/Post/scripts),
[its adaptive clearing](https://github.com/FreeCAD/FreeCAD/blob/main/src/Mod/CAM/libarea/Adaptive.cpp),
[Kiri:Moto](https://github.com/GridSpace/grid-apps) and
[its machine setup](https://docs.grid.space/kiri-moto/CAM/machines),
[OpenCAMLib](https://github.com/aewallin/opencamlib),
[cavalier_contours](https://github.com/jbuckmccready/cavalier_contours),
[Clipper2](https://github.com/AngusJohnson/Clipper2),
[grbl 1.1 commands](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Commands),
[grblHAL tool change](https://github.com/grblHAL/core/wiki/Manual,-semi-automatic-and-automatic-tool-change),
[MillenniumOS](https://github.com/MillenniumMachines/MillenniumOS) and
[its G-code reference](https://github.com/MillenniumMachines/MillenniumOS/blob/main/GCODE.md),
[LinuxCNC G-code](https://linuxcnc.org/docs/html/gcode/overview.html).

## Decisions

Defaults chosen for this design; those that are the owner's to change
are listed again under [Open questions](#open-questions).

1. **Rust, in a crate of its own below the model.** `core/cam`
   (`mitcad-cam`) knows no document and no OCCT: it takes planar outlines
   of lines and arcs in the work coordinates and gives toolpaths, findings
   and G-code. The model and the api give it its inputs from faces,
   sketches and the stock, and keep its operations. 3D operations later
   take the kernel's triangle meshes, as the print analysis does.
2. **CAM is not on the timeline.** Operations live in a CAM *setup* kept
   with the design (like drawings and print plates), not as timeline
   features: changing a toolpath must not recompute the model, and the
   model's recompute must not recompute toolpaths until asked. They
   follow the model through topological names (a pocket on
   `F7:top`), as drawings do, and are marked out of date when the bodies
   they use change.
3. **The toolpath is the tool center's.** Tool radius compensation is
   done in our code (G40 in the program), so every controller cuts the
   same, the checks see the real path and no controller's G41/G42 quirks
   matter.
4. **Arcs stay arcs.** Offsets keep lines and arcs (cavalier_contours),
   and the posts write G2/G3 with I and J relative to the start in the XY
   plane; a setting (or a dialect without arcs) writes short lines within
   a tolerance instead.
5. **One post driver, thin dialects.** One walk over the program for
   every controller; a dialect (`PostProcessor`) writes only what differs:
   header, footer, tool change, spindle, coolant, pause, dwell, park and
   comments. Every move line names its motion (G0 to G3), which every
   controller reads alike; unchanged axes and feeds are left out.
6. **Metric first.** Millimetres and mm/min everywhere inside; inches
   only in the written program (G20) when asked.
7. **Z zero on the stock's top, X and Y zero at a corner of the stock**
   by default (the usual router setup), work offset G54.
8. **Rapids never move down and across at once**: up first when rising,
   across first when sinking.
9. **The first tool is in the spindle when the program starts** (it was
   used to set the work zero): on a controller with no tool change (grbl)
   no stop is written for it.
10. **Messages say what is wrong in words** (no names, ids or
    coordinates, [UI conventions](ui-conventions.md)); the place goes into
    the finding's `at` and `operation` for the view and agents.

## Phases

1. **2.5-axis milling (this round and the next):** facing, contour
   (outside, inside, on the line), offset pocketing with islands,
   drilling with pecks; the machine model with its zones and tool
   changers; seven dialects; the program check.
2. **The model and the app:** setups, the tool library and machines in
   the design and the project, operations from faces, sketches and the
   stock through the api, toolpaths drawn in the 3D view, the CNC group
   on the Make tab, posting to a file.
3. **Better 2.5-axis:** ramps and helix entries, lead in and out, holding
   tabs, rest machining (what a bigger tool left), adaptive (constant
   engagement) clearing, chamfers and V-carving, engraving of text,
   thread milling, canned drilling cycles where a dialect has them.
4. **Simulation:** removing the tool's sweep from the stock (a height map
   for 3-axis), comparing it with the part (left material, gouges), the
   machine with its zones and the tool changer's moves animated.
5. **3-axis:** roughing by levels (pockets of the sections), finishing by
   raster (drop-cutter), waterline and scallop on the bodies' meshes;
   ball and bull cutters.
6. **Later:** 4th axis (indexed, then wrapped), probing cycles, tool
   wear and breakage checks, a feeds and speeds calculator.

## Data model

```
design
└── CAM setups
    └── setup "Top side"           stock, work offset, placement, machine
        ├── operation "Face"         tool 1, area: the stock
        ├── operation "Pocket 1"     tool 2, faces F7:top, F7:side(...)
        ├── operation "Contour 1"    tool 2, outside of the body's outline
        └── operation "Drill"        tool 5, holes picked by their faces
```

- **Setup:** one clamping of the stock: the bodies it machines, the
  stock (a box round them with margins, or a body), the work coordinate
  system (origin and axes picked on the stock: a corner and its top, the
  middle of the top, a vertex), the work offset (G54 to G59), the safe
  height, the machine and the post settings. A part machined from two
  sides is two setups. Its eye in the Browser (its toolpaths drawn) is
  kept with it (`visible`).
- **Operation:** its kind and inputs (faces, edges, sketch profiles,
  points; by topological names and sketch references, each with the
  occurrence it was picked in: the outline is placed where that
  occurrence places its component, so a component placed twice gives an
  outline per placement picked), the tool by its
  number, speeds and feeds (from the tool unless set), heights (top,
  bottom, clearance, retract, step down), side and direction, stock to
  leave, and whether it is suppressed. Inputs become planar outlines in
  the work coordinates: a planar face's loops, a sketch region's
  outline, a silhouette as the cut sheets make it (outside contours of a
  flat part), the section of the bodies at a height (pockets of a 3D
  part's levels, `Kernel::section_curves`).
- **Toolpath:** the computed moves of an operation (`Toolpath`, lines,
  arcs and rapids of the tool tip in the work coordinates). Computed
  data, kept in a cache with the inputs' fingerprint and not saved in the
  design file, like display meshes; *generate* computes what is out of
  date.
- **Program:** the setup's operations in order with their tools,
  spindle and coolant (`Program`), what a post-processor writes.

## Tools

A tool (`Tool`) has a number (`T`, its place in a changer), a name, a
shape (flat, ball, bull nose with a corner radius, drill with a point
angle, V-bit with an angle and tip), the cutting diameter, the flute
length (the deepest pass), the stick-out from the holder (the length for
the checks and the tool length offset), the number of flutes and default
speeds and feeds (`Cutting`: spindle rpm, feed, plunge feed). A
**tool library** (`ToolLibrary`) is a list of tools by number, kept as
JSON: one per machine (the tools in its rack or drawer) and the project
copies the tools its setups use, so a design machines the same after the
library changes. Holders are a cylinder for now (the spindle's nose in
the machine model); holders of their own come with the 3D phase.

## Machine model

A machine (`Machine`) is JSON, kept in the user's machine library and
copied into the project with the setups that use it:

- `controller`: the dialect's id (below);
- `travel`: where the controlled point can go, in machine coordinates.
  The controlled point is the spindle's gauge line (where tool lengths are
  measured from), so a tool's tip is its length below it;
- `spindle`: minimum and maximum rpm, the spin-up wait (seconds, written
  as a dwell where the spindle command does not wait), and the nose (the
  collet nut, holder and spindle body as one cylinder: diameter and
  length up from the gauge line);
- `max_feed` and `rapid_feed` (mm/min);
- `zones`: boxes in machine coordinates the tool and the nose must not
  touch: clamps, a vise, the tool rack, a dust shoe's brackets, a 4th
  axis, the gantry's sides. A zone is a box for now; meshes of a
  modelled machine (a design of the machine itself, its bodies placed by
  its joints at the axes' positions) come with the simulation phase;
- `tool_changer` (below) and `park`, where the spindle goes at the end.

The setup's **placement** says where the work offset is on the machine
(the machine position of the work zero), as the operator sets it up; the
check then knows where every move is in machine coordinates.

The **check** (`machine::check`) finds what the machine cannot do: a
spindle speed outside its range, a feed above its maximum, the gauge line
beyond the travel, the tool (a cylinder of its radius and length) or the
nose (a cylinder above it) inside a zone. Moves are sampled at most a
quarter of the tool's radius apart (1 mm at most). Each finding says
what in words (`"Contour 1: the tool runs into the front clamp."`), with
the operation's index and the point for the view.

## Tool changers

Three kinds (`ToolChanger`):

- **Manual:** the operator changes the tool. The dialect writes its own
  manual change: grbl stops with M0 and a comment (and the operator sets
  the new tool's Z zero); grblHAL, Mach3, Mach4 and LinuxCNC write
  `T M6`, which their manual change handles (grblHAL's `$341` modes,
  the M6 macros, LinuxCNC's manual change dialog); RepRapFirmware shows a
  message box (`M291 … S2`) and selects the tool; MillenniumOS selects the
  tool and asks the operator itself.
- **Controller:** the controller's own changer: the dialect's tool
  change command alone (`T M6`, or `T` on RepRapFirmware). A **DIY
  changer configured in the controller** is this kind: a grblHAL tool
  change macro or ATC plugin, RepRapFirmware's `tpre`/`tpost` macros,
  Mach3's or Mach4's M6 macro, a LinuxCNC M6 remap. On grbl, `T M6` is
  for a sender that handles M6.
- **Macro:** a **DIY changer driven by the program**, for controllers
  without macros (grbl) or when the program should carry the moves: a
  rack of slots (the gauge line's machine position for each tool), the
  machine Z for the moves to the rack, and two lists of G-code lines,
  `unload` (put the tool in the spindle away) and `load` (take the next).
  In the lines, `{tool}`, `{name}`, `{x}`, `{y}`, `{z}` (the slot) and
  `{safe_z}` are filled in, lengths in the program's units; an unknown
  name in braces is refused. A tool without a slot is changed by hand
  (the dialect's pause). Example for a pneumatic drawbar on an output:

  ```
  unload: G53 G0 Z{safe_z} / G53 G0 X{x} Y{y} / G53 G1 Z{z} F500 /
          M64 P0 / G4 P1 / G53 G0 Z{safe_z}
  load:   G53 G0 X{x} Y{y} / G53 G1 Z{z} F500 / M65 P0 / G4 P1 /
          G53 G0 Z{safe_z}
  ```

After any change the tool length offset follows where the dialect uses
one (`G43 H` on Mach3, Mach4 and LinuxCNC by default; grbl's and
grblHAL's `G43.1 Z` from the tool's length when the setting asks for it;
RepRapFirmware and MillenniumOS apply the tool's offsets with the tool).
The rack's slots and the rack itself belong in the zones, so that
operations keep clear of them; the changer's own moves are read back
from the posted program and checked, the rack's zones (those a slot is
in) excepted for them ([Simulation and checks](#simulation-and-checks)).

## Spindle assembly

The modelled machine, first phase (owner, 12.11.2026): the spindle and
what it carries, as a MitCad design, checked for collisions with the
stock, the fixtures and the keep-out zones along the toolpath
(`core/cam/src/spindle.rs`, `core/cam/src/collision/`).

- **The parts** (`SpindlePart`): the tool's flutes (the only part that
  cuts), its shank, the collet, the collet nut or a tool holder, the
  spindle's body and nose, its mount, a tool changer's moving parts on
  the spindle, any other part. Each is a solid of revolution about the
  spindle's axis where it is one (slices of cylinders and cones,
  `Frustum`), else its triangles; a part read from a mesh carries the
  mesh's deviation as a margin the check counts against it.
- **The spindle model** (`SpindleModel`) is the spindle without a tool,
  in its gauge line's frame: the axis along +Z, `z = 0` at the face a
  tool's stick-out is measured from. A machine without a modelled
  spindle has its nose as one cylinder (`SpindleModel::from_nose`). The
  **assembly** (`SpindleAssembly`) puts an operation's tool in it: the
  tool's tip at the origin, the flutes and the shank from the tool's
  definition (a drill's point and a V-bit's cone as cones, other ends at
  the full diameter, which holds a ball), the spindle raised by the
  stick-out.
- **The design:** `resources/cam/router-spindle.json` is a router
  spindle that ships with MitCad (an 80 mm body on a 40 mm nose, an ER20
  collet nut of 35 mm, a clamp mount, and a 6 mm end mill for the
  picture), built of MitCad's own features with its sizes as parameters.
  A spindle design names its components by what they are: a name with
  *nut*, *collet*, *holder*, *changer*, *mount* (or *bracket*, *clamp*),
  *spindle* (or *body*, *motor*) is that part; *tool* (or *cutter*,
  *bit*, *mill*) is the picture's tool, left out, since each operation's
  tool comes from its definition; any other name is another part on the
  spindle. `mitcad_model_api::cam_spindle::spindle_model` reads a
  design's bodies through the kernel's triangle meshes into the model; a
  body round about the axis (its box centred on the axis and reaching
  its largest radius on all four sides) becomes slices that hold its
  mesh (between two heights of the mesh, each triangle's largest radius
  stays under the line between its values there), the others keep their
  triangles.
- **Obstacles** (`Obstacle`) in the work coordinates: the stock (a box,
  or a body's mesh), fixtures (meshes: clamps, a vise, screws) and the
  machine's keep-out zones (`Obstacle::zones`, moved by the setup's
  placement).
- **The check** (`check_collisions`) sweeps every part of each
  operation's assembly along its moves: lines, arcs and helices at a
  feed, and rapids as the posts write them (up first when rising, across
  first when sinking); an operation starts above its first point at the
  safe height after a tool change (the change itself and the travel to
  there are `check_tool_changes`', below, against the stock whole), or
  where the last one ended with the same tool. The stock is taken as the
  program has cut it so far
  (`StockModel::AsCut`, the default since phase 3; [Stock
  simulation](#stock-simulation)): each move is checked against the
  stock's height map as the moves before it left it, then cuts it; the
  material over a node is a vertical column there. A part as wide as the
  cutter or wider (the shank, a nut, a holder) comes over any place
  before the cutter does, so the material the move itself cuts away is
  still there for it. The flutes may be in the stock at a feed; at a
  rapid they cut it when the simulation takes more than 0.001 mm off it.
  Conservatively (`StockModel::BeforeCut`, an option) the stock is taken
  as it is before the cut: the flutes may be in it at a feed, never at a
  rapid, except straight up (through the column the tool fills) or back
  down along a plunge the operation already cut (a drill's pecks), and
  every other part touching the stock is a collision. Every part touching
  a fixture or a zone at any speed is a collision. Touching counts.
- **The method:** the distance between a part and an obstacle as the tip
  moves is the distance from the tip to a convex set, so it changes no
  faster than the tip; the sweep of a convex piece along a line is
  convex, and its distance to a convex piece of an obstacle is found by
  Gilbert, Johnson and Keerthi's algorithm on their support functions:
  within 1e-10 of the true distance relative to it, never above it, no
  sampling. An arc is held by the triangles of its end tangents (prisms
  for a helix), refined where it comes near an obstacle until each piece
  is within the arc tolerance (0.001 mm by default) of the arc: the
  clearance on arcs is at most that below the true one, never above. A
  closed mesh's inside is found by a ray's crossings where an operation
  starts; after that, a part can only get inside through its surface.
  Boxes round the pieces skip what is farther than the smallest
  clearance found so far.
- **The result** (`CollisionReport`): the first contact of each part
  with each obstacle in an operation, in the program's order, with a
  message in words (`"Pocket 1: the collet nut runs into the stock."`,
  `"Pocket 1: a rapid move takes the cutter into the stock."`), the
  operation's and the move's index, the move's line in the posted program
  (`post_with_lines` gives each move's lines; `set_lines` fills them in),
  the part, the obstacle's index and the tip's position; and the
  smallest clearance of the program (0 with a collision) with its place.

## Operations

All operations work in the work coordinates, Z up, from planar outlines
(`Contour`: vertices with bulges, lines and arcs; `Region`: an outline
with islands). Heights: `top` and `bottom` of the cut, `step_down`
(passes of equal depth, at most the step), `clearance` (rapids between
features) and `retract` (rapids end here before a plunge). Flat-bottomed
tools only (flat, bull nose) for the wall operations; the step down must
fit the flutes and the depth the stick-out.

- **Facing:** rows back and forth along X across a rectangle, edge to
  edge with the tool's center on each edge, turning outside it; stepover
  60 % of the diameter by default.
- **Contour:** outside or inside closed outlines, or on the line (open
  outlines too, cut back and forth). Outside outlines are offset together
  as one shape, so parts closer than the tool's diameter are not gouged
  (their offsets merge). Climb milling by default for an M3 spindle
  (outside clockwise, inside counter-clockwise); conventional reverses.
  Stock to leave for a finishing pass.
- **Pocket (offset):** loops at equal offsets from the walls and
  islands, from the middle outwards, each cutting at most the stepover;
  the last loop is the wall's. A pocket that falls into parts (a waist
  narrower than the tool) is cleared part by part. Loops are joined by a
  feed move at the floor when the link is short (two stepovers) and
  stays inside the pocket; otherwise the tool goes up to the retract
  height (nothing inside the pocket stands above the top) and over. It
  comes in and leaves at the clearance height. Coverage:
  loops a stepover apart leave nothing while the stepover is at most the
  diameter, and at a corner of angle *a* they are `stepover / sin(a/2)`
  apart along its bisector, so the default stepover (40 %) clears corners
  down to about 47 degrees; when no further loop fits, loops at half the
  radius follow until the middle is reached. The entry is the
  operation's (below).
- **Drilling:** points, nearest first, in one go, in pecks back to the
  retract height or in pecks that back off 0.5 mm to break the chip,
  written out as moves and recorded as one drilling cycle that a post
  writes as a canned cycle when asked (below).

### Entries, tabs and leads

- **Entry** (`Entry`, contours, pockets, adaptive clearing, rest
  clean-up): `plunge` (straight down, a center-cutting end mill), `ramp`
  (down along the cut, back and forth, no steeper than `angle` degrees;
  the legs at most half the length the angle needs, as many as the depth
  takes) or `helix` (round a circle of the tool center's `radius` beside
  the cut's start, tangent to it, at most `angle` degrees, then once round
  at the depth). A helix that does not fit (it would cut a wall, an
  island or the part) falls back to the ramp, a ramp with less than the
  tool's radius to run along to the plunge. The tool comes down to the
  level above in a straight line first (that is air).
- **Holding tabs** (`Tabs`, contours): `layout` by `count` per outline or
  one about every `spacing` mm, `width` of the material left and `height`
  above the bottom. Only the passes below the tabs' top rise over them:
  the tool goes straight up at the feed before a tab, runs over it for
  its width plus the tool's diameter, and comes straight down at the
  plunge feed after it; a ramp stops short of the first tab. Tabs that do
  not fit along an outline are refused.
- **Leads** (`Lead`, closed contours outside or inside): a straight line
  of `length` square to the outline, then a quarter circle of `radius`
  tangent into the cut on the waste side; the same out of it. The loop
  starts where the tool is nearest, or else at the middle of its longest
  side where the leads fit; a lead that runs into the part everywhere is
  refused. Between passes the tool goes up to the retract height and
  back to the start of the lead in.

### Adaptive clearing

`Adaptive { max_engagement }` (degrees, 45 by default) on a pocket or a
facing replaces its loops or rows by a path that keeps the tool's load
even. The engagement is measured as the material each stretch of one
tool radius removes against a straight cut at that engagement
(`acos(1 - e / r)` for a cut `e` deep), so a short sharp turn does not
count as much as a whole stretch of it.

- **Pockets:** a helix opens the middle of each part (the point farthest
  from its walls); fronts of equal distance from it *inside the pocket*
  (a distance field on a grid finer than the step, so that they go round
  islands and into side arms) clear outwards. Small rings round the
  middle grow by the step that keeps their larger outer sweep within the
  limit; farther out each front cuts at most 90 % of the limit's depth.
  Each front cuts only what the one before left (its new stretches along
  the walls too); a closed one is entered a diameter along from where the
  tool is, so that the way in comes slantwise. Each ring of fronts is
  measured on the material the cuts before it left, and made again
  closer to the last (0.6 times the step) while a stretch of it is over
  the limit; the next ring may then grow by half again. A front that
  ends at a wall or an island also takes the strip beyond its end, and
  in a channel narrower than the tool the material across is wider than
  the way the center can go: there the fronts come closer together. The
  fronts keep two grid steps off the walls; loops of the exact offsets
  then finish the walls, each no deeper than a front and measured the
  same way (a loop over the limit, at a corner or where the fronts left
  more, gets another halfway before it). The rings are planned in their
  order and then cut side by side: once the fronts split round an
  island, each side is cut ring after ring instead of the tool crossing
  to the other side at every ring. A front waits only for the fronts
  planned before it that come within the tool's diameter of it (the
  material it cuts depends on them); of the fronts that may come next
  the nearest one the tool can reach at the feed within the limit is
  taken, measured as it is cut. Between cuts the tool stays down at the
  feed where the straight way runs through what is cleared and keeps
  clear of the walls, or else back along the last cut (which is clear)
  and then straight; it goes up to the retract height only where neither
  way is clear (in the island pocket of the tests, 7 times in 245 cuts,
  where the two sides meet again).
- **Facing:** loops round the stock from the outside in, each a step
  deeper, the corners rounded about the material's corners (a corner
  loads the tool less than a side); it comes down beside the stock and
  into each loop slantwise.
- The measure is a grid of a fortieth of the tool's radius
  (`ops::engagement`); the whole path is measured again at the end and
  made again with a smaller step (0.7 times, up to three times) where a
  stretch is still over the limit; a pocket that cannot be kept within
  it is refused in words. Pockets too large to measure finely (above six
  million cells) are planned by the step alone and not measured.

### Rest clean-up

`RestOp` with the bigger tool's `previous_diameter` (and the stock it
left): the smaller tool runs along its own wall loops only where its
center is farther than `R - r` from where the bigger tool's center could
go, that is in the corners the bigger tool rounded and round whole parts
it never entered. Each stretch is lengthened a radius at both ends into
what is cleared. A part the bigger tool missed that is wider than twice
the smaller tool's diameter keeps its middle: clear it with a pocket of
its own.

### 3-axis machining

Phase 3 (`core/cam/src/ops/surface.rs`, `zlevel.rs`, `roughing.rs`,
`raster.rs`, `waterline.rs`): freeform bodies machined from their
triangle meshes in the work coordinates (the model meshes each body
within 0.005 mm, placed by its occurrence; no OCCT in the crate), with
flat, ball and bull nose end mills.

- **The drop-cutter** (`Surface::drop_cutter`): how low a cutter comes
  down its axis at a place before it touches the mesh, exact for the
  triangles. A cutter is a cylinder whose bottom edge is rounded by its
  corner radius (`SurfaceCutter`: 0 for a flat end mill, the radius for a ball);
  it rests on the highest of its contacts with each triangle's corners
  (the end's height at the corner's distance), insides (the point of the
  corner circle toward the uphill side, down the triangle's normal by the
  corner radius, when it is inside the triangle; closed form) and edges
  (the tip's height along an edge is concave in the edge's parameter, so
  its highest point is the only one: closed form for a ball and a flat
  end mill, golden section for a bull nose). Each contact gives the
  surface's normal there, for the slope split. The triangles are binned
  in XY. Stock to leave is the cutter grown by it all round and raised by
  it.
- **Raster finishing** (`RasterOp`): passes at an angle back and forth
  across a boundary (where the tool's center goes; the box round the
  bodies, the stock's outline in the model), spread evenly at most the
  stepover apart (a tenth of the diameter by default); along a pass the
  tool is dropped at most a quarter of its radius apart and again between
  two points while the middle is off the line between them by more than
  the tolerance (0.01 mm); the points within the tolerance of a line are
  left out. Where nothing is under it the tool runs at the bottom. The
  end of a pass joins the next along the surface when it is near and
  inside the boundary; else the tool goes up to the retract height.
- **Waterline finishing** (`WaterlineOp`): at each level (a tenth of the
  diameter apart by default in the model) the loops where the tool just
  touches the surfaces with its tip at that height: the cutter is dropped
  once at the nodes of a grid round the bodies, and at a level the edges
  between free and blocked nodes (marching squares) cross each grid edge
  where the test flips, found by bisection (each point free); the grid is
  fine enough that a chord between two points of a loop as curved as the
  cutter stays within the tolerance. Climb milling runs round the bodies
  clockwise (the part on the right).
- **Slope split:** raster finishing with `max_slope` cuts only where the
  surface it rests on is at most that steep, waterline finishing with
  `min_slope` only where it is at least that steep; the same angle on
  both splits a part between them (the shallow areas by raster passes,
  the steep walls by waterlines).
- **Roughing by levels** (`Roughing3dOp`): the stock cleared level by
  level from the top down, the levels at most the step down apart and at
  each floor of the bodies raised by the stock to leave (flat floors keep
  exactly that). At a level the tool's center may be where the cylinder of
  its radius grown by the stock to leave, its end raised by it, rests no
  higher than the level, inside the boundary: those edges are found like
  waterlines, grown by the cylinder's radius into the walls of a pocket,
  and cleared by the offset pocket (or adaptive clearing) with the stock
  to leave and one and a half tolerances kept from the walls, so that the
  tool stays the stock to leave off the bodies to the side and below.
  Each level's pocket comes down from the level above, whose area holds
  it. Flat and bull nose end mills (a bull nose lies inside its
  cylinder).

## Post-processors

`post(program, dialect, settings, machine)` writes the program; the
dialect is `dialect(id)`. The driver writes: after a tape mark, the
notice that CAM is highly experimental (`EXPERIMENTAL_NOTICE`, four
comment lines, written even when comments are off; the owner's order of
13.11.2026: treat the machine code with caution, check every toolpath in a
simulator, run it first without stock, at a low feed, with a hand on the
emergency stop; plain ASCII without grbl's realtime characters `?`, `~`,
`!`, without parentheses and without a word LinuxCNC reads as a command);
the comments (the program's name, its tools), the dialect's header, a rapid to the safe height; per
operation its name as a comment, at a new tool the spindle and coolant
off, up to the safe height, the tool change (by the changer's kind) and
the length offset, up to the safe height again; the spindle (with the
spin-up dwell), the coolant and the moves; at the end the spindle and
coolant off, the safe height, the park position and the dialect's
footer.

| | grbl | grblHAL | RepRapFirmware | MillenniumOS | Mach3, Mach4 | LinuxCNC |
|---|---|---|---|---|---|---|
| Header | `G90 G94 G17`, `G21`, `G54` | as grbl | `G90`, `G21`, `G17`, `G54` | `G90 G21 G94`, `M4000` per tool, `G28`, `G6600 W` when asked, `G54` | `G90 G94 G91.1 G40 G49 G17`, `G21`, `G54` | `%`, `G17 G90 G94 G40 G49 G80`, `G21`, `G64 P<tol>`, `G54` |
| Tool change | M0 pause (manual), `T M6` (controller) | `T M6` | `M291` box (manual), `T` | `T` | `T M6` | `T M6` |
| Length offset | `G43.1 Z` when asked | `G43.1 Z` when asked | with the tool | with the tool | `G43 H` | `G43 H` |
| Spindle | `S M3` | `S M3` | `M3 S` | `M3.9 S` (waits), `M5.9` | `S M3` | `S M3` |
| Coolant | M8, M9 (no M7 in its standard build) | M7, M8, M9 | M7, M8, M9 | M7, M7.1 air, M8, M9 | M7, M8, M9 | M7, M8, M9 |
| Dwell | `G4 P` s | `G4 P` s | `G4 S` s | (none needed) | `G4 P` s | `G4 P` s |
| Canned drilling | none | `G98 G81/G83/G73`, `G80` | none | `G81/G83/G73` per hole | `G98 G81/G83/G73`, `G80` | `G98 G81/G83/G73`, `G80` |
| Comments | `( )` | `( )` | `;` | `;` | `( )` | `( )` |
| End | `M30` | `M30` | `M0` | `G27`, `M0` | `M30` | `M2`, `%` |
| File | `.nc` | `.nc` | `.gcode` | `.gcode` | `.tap` | `.ngc` |

Settings (`PostSettings`): units (millimetres or inches), arcs on or off
with a tolerance, line numbers (N10, N20, …), comments on or off, the
tool length offset on or off against the dialect's default, canned
drilling cycles (`canned_cycles`, off by default) and probing the work
zero (`probe_work_zero`).

**Inches** (G20) convert every length (axes, I and J, the safe height,
G43.1 Z, M4000 R, G64 P, cycle Z, R and Q, the tool changer's lines and
the park position) and every feed; spindle speeds, dwells and the
tools' names in the comments stay as they are. Golden files of every
dialect in inches are in `core/cam/src/post/golden/`.

**Canned drilling cycles:** a drilling toolpath records its holes as one
`DrillCycle` (G81 in one go, G83 pecks out of the hole, G73 pecks that
break the chip; Q is the peck depth, R the retract height) beside its
moves, which stay for the checks and the simulation. With
`canned_cycles` on, grblHAL, Mach3, Mach4 and LinuxCNC write it modally
(`G98` and the first hole, the other holes by X and Y, `G80`; G98 takes
the tool back to the clearance height between holes) and MillenniumOS one
whole cycle line per hole followed by `G0 Z` to the clearance height (its
cycles end at the retract height); grbl and RepRapFirmware have none and
get the moves. The controller pecks from the retract height by its own
rules, so its pecks are not exactly the written-out ones; the hole, its
depth and the heights are the same. Every move of the cycle still goes
through `Output::motion`, which writes the cycle's lines with the moves
they stand for. Golden files of peck drilling per dialect are in
`core/cam/src/post/golden/`.

**Probing the work zero** (MillenniumOS only, owner 12.11.2026): with
`probe_work_zero` the header writes `G6600 W<n>` (the program's work
offset, 1 for G54) after homing, so that the operator probes the work
zero before the program runs; the other dialects refuse the setting in
words.

**Dwell on Mach3 and Mach4** is in seconds (owner 12.11.2026): the
controller's setting that reads `G4 P` as milliseconds must be off. Numbers have
three decimals in millimetres and four in inches, feeds one and two,
without trailing zeros. Comments lose what the controller cannot read
(parentheses inside become brackets, other than ASCII becomes `?`); on
grbl and grblHAL, which act on `?`, `~` and `!` wherever they are in the
stream (a status report, a cycle start, a feed hold), those become `_`,
in comments and in a tool's name filled into a changer's line. A
command the controller does not have is refused in words (air blast and
mist on grbl; a counter-clockwise spindle on MillenniumOS).

A new dialect is a struct implementing `PostProcessor` in
`core/cam/src/post/`, listed in `dialects()`, with a golden test of the
shared two-operation program in `post/tests.rs`.

## Simulation and checks

- **Now:** the machine check above, and the toolpath's bounds, cut
  length and time (`Toolpath::bounds`, `cut_length`, `duration`).
- **The output through the controllers' own code** (verified before it
  ships, no outside dry run): `core/cam/tests/controller_sim.rs`
  posts the post tests' program and a plate (facing, outline, pocket
  with an island, pecked holes, three tools) in millimetres and inches,
  with arcs and without, with line numbers, the length offset, a
  spin-up dwell, a rack changer and a park, and runs it through grbl 1.1
  (its parser, arcs and planner on the host, ctest `core.cam_grbl_sim`,
  `MITCAD_GRBL_SIM`) and grblHAL's simulator (streamed as a sender
  does, ctest `core.cam_grblhal_sim`, `MITCAD_GRBLHAL_SIM`): every line
  accepted, no alarm, no realtime character, a hold per manual change on
  grbl, and the path's end and extents within the G-code's rounding, the
  controllers' arc chords and their resolution (32-bit floats; 1000
  steps per mm). The other dialects run on grbl but for their own
  commands, which the test lists (`T M6`, `G43 H`, `G64 P`, `%`, `M291`,
  MillenniumOS's `M4000`, `M3.9`, `M5.9`, `G27`). The simulators are GPL
  programs run apart, never linked, and the tests are skipped without
  them.
- **Stock removal** on a height map (phase 3, [Stock
  simulation](#stock-simulation)): what is left against the part (left
  material, gouges into the part beyond a tolerance), shown in the view
  with the toolpath played back; the collision check with the stock as
  cut.
- **The G-code reader** (`core/cam/src/gcode.rs`, `GcodeReader`): our
  own reader of the subset the posts write and a changer's lines use,
  turning a program back into the moves of the spindle's gauge line in
  machine coordinates (`GMove`): G0 to G3 (arcs by I, J, K or R in the
  planes G17 to G19, helices), G20 and G21, G53, G54 to G59 (each work
  offset's machine position given; one not given is refused), G90 and
  G91, G90.1 and G91.1, the canned cycles G73, G81, G82, G83 with G98,
  G99 and G80 (pecks read as one feed to the bottom, which the
  controller's pecks stay within), T and M6 (the tool's length then
  counts; T alone where the controller changes on it), F, comments, quoted
  texts, line numbers, `%` and checksums. A work move puts the tool's tip
  at the offset's position plus the coordinates, the gauge line the
  tool's length above it; G53 places the gauge line. The words without a
  move are read past (the length offset words too: the length is the
  tool's, as the placement means it). After G27, G28, G30, a probing move
  or a tool change the position is not known until the axes are named
  again. What it does not follow is refused in words with its line (work
  offsets set by G10 or G92, inverse time feeds, parameters and
  expressions, other axes, unknown commands). **Round trip:** every
  dialect's program (manual, controller and rack changers; millimetres
  and inches; arcs and lines; line numbers and the length offset) read
  back gives the program's moves within the dialect's rounding (half the
  last digit; an arc's center twice that), the arcs with their direction
  and the feeds, and no cut but the changer's own in machine
  coordinates.
- **The tool changes** (`check_tool_changes`, `core/cam/src/collision/changer.rs`;
  `check_with_tool_changes` runs it with `check_collisions` and orders the
  contacts as the program runs): the posted program
  (`post_with_lines`, whose `changes` give each change's `unload` and
  `load` lines) is read back, and the spindle assembly is swept along a
  macro changer's own moves, with the tool before the change on the
  `unload` lines and the operation's tool on the `load` lines (on the
  trip to the rack the spindle may be empty; taking the tool along is the
  safe side), against the stock, the fixtures and the zones, every part
  the cutter included; the zones a slot is in (its X and Y inside the
  box) are the rack, where the changer docks, and are not checked on its
  lines. Then **the travel after the change**, as rapids: from where a
  macro changer's lines leave the spindle, or after a change the program
  does not drive (the operator's, the controller's own) from the
  machine's park position, or without one from the top of the Z travel
  above where the spindle was, down or up to the safe height, then across
  above the operation's first point (the first operation of a program
  whose first tool is in the spindle has none). Each contact says what in
  words (`"The tool change before Drill: the cutter runs into the dust
  extractor."`, `"The rapid move to Drill after the tool change: the
  collet nut runs into the dust extractor."`), with the posted program's
  line and `change`: the changer's line (`unload` or `load`, 1-based in
  its list) or `travel`. A move from where the reader does not know the
  position (a program's first change, before its first X and Y) is not
  checked.
- **Later:** the machine model's bodies moved with the axes and checked
  against the zones.

### Stock simulation

`core/cam/src/stock.rs` (mitcad#1013, phase 3), Rust, no OCCT:

- **The stock** is a height map (dexels): a Z per node of a grid in the
  work plane, standing on the stock's bottom (a node at the bottom holds
  nothing). A box stock is flat; a stock body is its mesh seen from above
  (its highest surface over each node). The grid spans the stock's box
  with nodes on its edges, an eighth of the smallest tool's diameter
  apart and no closer than 0.01 mm, coarser until it has at most a
  million nodes (or what the caller asks). What lies between two nodes is
  not seen: a wall thinner than a step may be missed, and a sloped
  surface is within a step of its true height.
- **The tool** is its cutting end as a height above the tip at each
  distance from its axis (`Cutter`): flat, a ball, a bull nose's corner,
  a drill's point and a V-bit's cone with its flat tip. Each profile is
  convex and rises outwards, so the lowest the end reaches over a node
  along a straight move is found exactly: at an end of the stretch where
  the node is under the tool (flat ends), at the move's point nearest the
  node (level moves), else by a golden section search of the height along
  the stretch (convex there). Arcs are cut as chords within a quarter of
  the grid's step; long moves in pieces, so that only the nodes near them
  are looked at.
- **Rapids** move as the posts write them (up first when rising, across
  first when sinking); one that takes more than 0.001 mm off the stock is
  reported (`RapidCut`: "Pocket 1: a rapid move takes the cutter into the
  stock.", its operation, move, line of the move, depth and the place it
  first cuts).
- **The part** (the setup's bodies) is meshed and seen from above on the
  same grid. **What is left** (`remaining`): over the part, the thickness
  of the stock above it at each node (its maximum, mean, volume, the area
  with more than the tolerance, 0.01 mm by default, and the thickest
  places, apart by the smallest tool's diameter); a **gouge** where the
  stock is below the part's lowest height round the node (so that a node
  on a wall the tool runs along is none); beside the part, the material
  the stock keeps where the part is not.
- **Heights out** (`cam_simulate`) go as runs to the micrometre: pairs of
  a height and a count of nodes, a snapshot as its changes from the one
  before (`0, -n` for `n` nodes unchanged), so that Simulate reads the
  whole playback once.
- **In the collision check** (`StockModel::AsCut`) the same height map
  is cut move by move; the parts are checked against its columns found
  through a pyramid of the highest node per block, nearest first, and
  the flutes' contacts at a rapid come from the simulation.

## API

Commands and queries of `core/model-api` (`api/cam.rs`), documented in
[commands.md](../core/model-api/src/commands.md), "CAM: setups,
operations and machines":

| Command or query | What it does |
|---|---|
| `cam_setup_add`, `cam_setup_edit`, `cam_setup_delete`, `cam_setup_move` | A setup: bodies, stock (margins round the bodies, or a body), work coordinates (origin at a point of the stock or the model, Z and X from faces, planes or axes), work offset, safe height, machine, dialect, post settings (units), probing the work zero, the work zero's machine position. |
| `cam_operation_add`, `cam_operation_edit`, `cam_operation_delete`, `cam_operation_move` | An operation of a setup: kind (facing, contour, pocket, drill, adaptive, rest; 3D roughing, raster and waterline finishing), inputs (the stock, faces, sketch profiles, edges, sketch curves, points; bodies for the 3D kinds), tool number, depths, heights, speeds and feeds (from the tool unless set), stepover, stock to leave, direction, entry, tabs, lead, suppressed. |
| `cam_generate` | Computes the toolpaths and records each operation's input fingerprint; gives per operation its state, cut length, time and error, per setup the machine check. |
| `cam_machine_put`, `cam_machine_delete`, `cam_tool_put`, `cam_tool_delete` | The design's machines with their tool libraries. |
| `cam_machine_from_library`, `cam_machine_to_library` | Copies between the design and the user's or the project's machine library. |
| `cam_setups`, `cam_operations` (queries) | The setups with their work frames and the operations' states (up to date, out of date, not generated, suppressed). |
| `cam_toolpaths` (query) | A setup's toolpaths computed now, in work coordinates, with the machine check. |
| `cam_post` (query) | A setup's G-code with its dialect and units (or others), with the file extension. |
| `cam_machines`, `cam_dialects`, `cam_library` (queries) | The design's machines, the dialects, the library's machines. |
| `cam_simulate` (query) | A setup's program cut out of its stock on a height map, up to a move or to the end, with snapshots along the way (runs to the micrometre); the material left over the setup's bodies (thickness, gouges, the thickest places); rapids that cut ([Stock simulation](#stock-simulation)). |

Values are millimetres (mm/min, rpm, degrees) as JSON numbers, or
expressions with units that the design's parameters evaluate (`"0.25
in"`), so inch users enter inches; the program is written in inches with
`post.units`. Errors follow the api's rules: a message in words.

## UI

- The **CNC** group on the **Make** tab ([UI
  conventions](ui-conventions.md#ui-surfaces)): *New Setup*, *Facing*,
  *Contour*, *Pocket*, *Adaptive Clearing*, *Rest Machining*, *Drill*,
  *3D Roughing*, *Raster Finishing*, *Waterline Finishing*, *Generate*, *Simulate*, *Post Process…*, *Machines and Tools…*
  (app/COMMANDS.md, "The CNC group").
- The **browser** shows a *CAM* folder with the setups and their
  operations (state: up to date, out of date, failed, suppressed); a
  double click edits the operation in its command panel.
- Command panels pick faces, edges and sketch regions in the view, with
  the tool from the library and the values of the operation; the
  toolpath previews while editing.
- The **3D view** draws toolpaths: cuts in one color, plunges in
  another, rapids dashed, the tool at a chosen move; the machine's zones
  when the check finds something.
- *Simulate* plays the toolpaths back with the tool and the stock as cut
  at the slider's place (`cam_simulate` read once with snapshots); at the
  end the stock is shaded by the material it leaves over the part (grey
  nothing, teal to violet more, pink a cut into the part) with the
  material left in words and the legend under the slider.
- *Post Process…* asks for the setup's dialect (from its machine) and
  the file name, and writes the file; the dialect's settings are in the
  setup.

## Crates

| Crate | Holds |
|---|---|
| `core/cam` (`mitcad-cam`) | Tools, the tool library, outlines, toolpaths, operations, the machine model and check, the post-processors. No model, no OCCT. |
| `core/model` | The setups and operations in the design file, their references, out-of-date marks (phase 2). |
| `core/model-api` | The commands and queries above (phase 2). |
| `app/` | The CNC group, the panels, the toolpath display (phase 2). |

## Implemented

The first round (branch `feature-1013-cam`, crate `core/cam`, ctest
`core.cam`, 55 unit tests, one of them the whole way from outlines to every dialect's G-code):

- `geom`: `Contour` (lines and arcs by bulge, rectangle, circle,
  polygon, area, orientation, reverse, start nearest a point,
  containment, segments with arc centers, validation in words),
  `Region` (an outline with islands).
- `tool`: `Tool`, `ToolShape`, `Cutting`, `ToolLibrary`.
- `toolpath`: `Move` (rapid, line, arc or helix), `Toolpath` (bounds,
  cut length, time), arc sweeps and sampling, `PathBuilder`.
- `ops`: `face`, `contour` (outside, inside, on; merged outside offsets),
  `pocket` (offset loops with islands, parts, links, middle clearing),
  `drill` (pecks); `Depths`, `Heights`, `Direction`.
- `machine`: `Machine`, `Zone`, `Spindle`, `ToolChanger` (manual,
  controller, macro with a rack), `Placement`, `check`.
- `program`: `Program`, `ProgramOp`, spindle direction, coolant.
- `post`: the `PostProcessor` trait, `Output` (modal writer, units,
  arcs or lines, line numbers, comments), the driver `post`, and the
  dialects grbl, grblHAL, RepRapFirmware, MillenniumOS, Mach3, Mach4,
  LinuxCNC with golden G-code per dialect.

The offsets use cavalier_contours 0.9 (MIT OR Apache-2.0) with
static_aabb2d_index and ahash (MIT OR Apache-2.0) and zerocopy (BSD-2,
Apache-2.0 or MIT), checked by cargo-deny. Its debug checks of slice
views are off in the dev profile (`core/Cargo.toml`): they fire on an
outline offset exactly to a point, which it handles without them. When
the crate is linked into the application, the third-party notices name
these crates.

### Phase 2: the model, the file and the API

- `core/model/src/cam/`: `CamState` (the design file's `cam` section:
  machines and setups, read tolerantly), `CamSetup`, `CamOperation`
  (kinds, inputs, values as `CamValue`: numbers or expressions with
  units), `CamMachine` (a machine with its `CamToolLibrary`; `CamTool`
  with its kind: flat, ball, bull, drill, chamfer, engraver; shank
  diameter, stick-out, holder), `outline` (the kernel's curves and the
  sketch's segments as outlines of lines and arcs in the work plane,
  chained and nested into regions), `library` (machine files in the
  user's and the project's folder).
- `core/model/src/document/cam.rs`: the commands (one undo step each,
  no recompute), the work frame and stock, the operations' outlines and
  toolpaths through `mitcad-cam`, fingerprints and out-of-date states,
  the program, the machine check and the post.
- Tests: `core/model-tests/cam` (ctest `core.model.cam`) on the mock
  kernel, and `cam/outline.rs`'s own.

The spindle assembly and its collision check (phase 2, on this
branch): `spindle` (`SpindleModel`, `SpindleAssembly`, `SpindlePart`,
`PartKind`, `Frustum`, reading a design's bodies), `collision`
(`Obstacle`, `check_collisions`, `CollisionReport`, the convex distance
in `collision/gjk.rs`), `post::post_with_lines` (the lines of each move),
the router spindle's design `resources/cam/router-spindle.json` and its
reader `mitcad_model_api::cam_spindle`.

Phase 2, operations and posts: `ops::entry` (plunge,
ramp, helix with their fallbacks), tabs and leads on contours,
`ops::adaptive` (adaptive pockets and facing, `engagement`), `ops::rest`
(rest clean-up after a bigger tool), chip-breaking pecks and
`DrillCycle` on drilling toolpaths, canned cycles and probing the work
zero in the posts, golden files of every dialect in inches and of peck
drilling in `core/cam/src/post/golden/`.

### Phase 3: the stock simulation

- `core/cam/src/stock.rs`: `Grid`, `HeightMap` (a box, a mesh seen from
  above, `cut_line`, `cut_move`, runs), `Cutter` (the ends' profiles),
  `simulate` (with `RapidCut`), `remaining` (`Remaining`, `Thickness`);
  the collision check's `StockModel` (as cut by default, before the cut
  as an option) in `collision/`.
- `core/model/src/cam/simulate.rs` (the query's options) and
  `core/model/src/document/cam/simulate.rs`: `cam_simulate`
  (commands.md, "CAM: stock simulation").
- The app: Simulate draws the stock as cut and, at the end, the material
  left (`app/cam/CamStock.*`, `app/cam/MainWindowCamStock.cpp`,
  `app/view/CamStockOverlay.cpp`; the colour roles `CamStock`,
  `CamLeftThin`, `CamLeftThick`, `CamGouge`).
- Tests: `core/cam/src/stock/tests.rs` (a flat end mill's pocket floor at
  its depth, a ball's groove of its profile, a V-bit's cone, a rapid
  through the stock, a faced stock leaving nothing, a gouge),
  `core/cam/src/collision/tests.rs` (a nut through material already cut
  away, as cut and before the cut), `core/model-tests/cam`
  (`simulate_tests.rs`), `app/tests/cam_stock.cpp` and the CAM UI test.

Phase 3, the tool changes: `gcode` (`GcodeReader`, `GMove`, the round
trip of every dialect's program), `post::ChangeLines` (each change's
lines in `Posted::changes`), `collision::check_tool_changes` and
`check_with_tool_changes` (a macro changer's moves and the travel after
each change, `Collision::change`).

### Phase 3: 3-axis machining

`ops::surface` (`Surface`, `SurfaceCutter`, `Contact`: the drop-cutter on
triangle meshes), `ops::zlevel` (the grid's contours, polylines within a
tolerance), `ops::roughing` (`Roughing3dOp`), `ops::raster` (`RasterOp`)
and `ops::waterline` (`WaterlineOp`) ([3-axis machining](#3-axis-machining));
in the model the operation kinds `roughing_3d`, `raster_finish` and
`waterline_finish` with body inputs and a boundary (commands.md, "CAM:
3-axis operations"), and the CNC group's 3D Roughing, Raster Finishing and
Waterline Finishing. Tests: the drop-cutter against planes, points, edges
and a sphere's mesh (within 1e-6 mm where it rests on a corner of the
mesh), a raster over a hemisphere leaving the two balls' scallop, a
waterline round a cylinder at its radius and the tool's, a stepped block
roughed with its stock to leave.

## Open questions

For the owner; the defaults above hold until answered.

1. **Where CAM lives:** setups and operations kept with the design (like
   drawings), not on the timeline: agreed?
2. **The reporter's controllers first:** grblHAL and RepRapFirmware /
   MillenniumOS are the reference dialects for testing with the
   reporter; should their output be sent to the reporter for a dry run
   (air cutting) before phase 2 is merged?
3. **MillenniumOS's start:** the header homes (`G28`) as its example post
   does, and does not probe the work zero (`G6600`); should probing be a
   setup option?
4. **Mach3 and Mach4 dwell units:** written in seconds; Mach3 has a
   setting that reads `G4 P` as milliseconds. Keep seconds?
5. **The modelled machine:** zones as boxes now; should the machine be a
   MitCad design of its own (bodies and joints for the axes) in phase 4,
   or a simpler box model with named parts?
6. **Tool library location:** per machine in the user's settings with a
   copy in the project (proposed), or per project only?
7. **Imperial users:** inches only in the output (proposed), or tool
   libraries and feeds entered in inches too?

## Next steps

Phase 1 left on this branch, then phase 2:

- [x] Ramp and helix entries for pockets and closed contours (no straight
      plunge for non-center-cutting tools).
- [x] Holding tabs for outside contours; lead in and out arcs.
- [x] Adaptive (constant engagement) clearing on cavalier_contours'
      booleans.
- [x] Rest clean-up at the corners for a smaller tool after a bigger one.
- [ ] Rest material between pocket loops at sharp corners (a check that
      reports what the stepover leaves, then corner clean-up moves).
- [x] Canned drilling cycles (G81, G83) where the dialect has them, as a
      setting.
- [x] Setups, operations and the tool library in the model and the
      design file; operations' inputs from faces, sketch regions, edges,
      sketch curves, points and the stock; out-of-date marks (sections
      of the bodies at a height are left for the 3-axis phase).
- [x] The API commands and queries above, documented in commands.md.
- [x] The CNC group, command panels, the browser's CAM folder, toolpath
      display, Simulate, Post Process… writing a file, Machines and
      Tools… (app/COMMANDS.md, "The CNC group").
- [ ] Third-party notices for cavalier_contours and its dependencies
      when the app links the crate.
- [x] Height-map stock simulation and the comparison with the part
      (phase 3: `stock.rs`, `cam_simulate`, Simulate's stock; the
      collision check with the stock as cut).
- [x] The macro changer's lines checked against the zones (a small
      G-code reader), and the travel after each tool change.
- [ ] The tool changes' contacts in `cam_generate` and the view (the
      model and the api do not run the collision check yet).
- [x] 3-axis roughing by levels and raster and waterline finishing on
      meshes (ball and bull cutters).
- [ ] Scallop finishing (constant stepover along the surface) and pencil
      passes in the corners the finishing tools leave.
