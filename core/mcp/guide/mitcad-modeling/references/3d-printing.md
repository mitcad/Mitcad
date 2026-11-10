# Design for 3D printing

<!-- SPDX-License-Identifier: MIT -->

Read this when a component is to be FDM/FFF printed. Start with its loads,
mating geometry and print orientation, then build the design. The
dimensions below are starting assumptions, not printer accuracy promises.
Use the actual printer, material and process when known. Otherwise plan
for a 0.4 mm nozzle and 0.2 mm layers; confirm critical fits with a small
test print. Keep fit and wall dimensions as named parameters.

## Orientation and strength

Arrange the layers so the main tensile load acts along them rather than
pulling them apart. If no orientation suits every load, use `split_body`
and design the joining geometry. Prefer load-bearing walls and generous
sections over solving a thin section by increasing infill alone.

In MitCad, assign the chosen planar face with `set_print_bed_face`.
This records an export orientation; it does not rotate the design.
Inspect the body's bed-face result with `bodies` after later geometry
edits. A saved face may be missing or no longer planar. Make separate
components when pieces have independent assembly placement; keep pieces
that must print assembled in their intended relative positions.

Use `print_settings` to inspect wall and infill values. A geometric
`shell` changes the CAD body; slicer walls and infill describe how that
body is filled. Neither substitutes for the other.

## Edges and holes

Prefer `chamfer` on edges parallel to the bed and `fillet` on vertical
edges, so finishing the edge does not create a poorly supported underside.
Select stable edges after the main shape is complete. Inspect the
result in the chosen print orientation.

For a horizontal bore, consider a teardrop or flat roof. MitCad's `hole`
has no special print-hole shape: sketch the section with `sketch.arc`
and `sketch.add_line`, constrain its size and joins, then `extrude` with
a cut operation. A teardrop retains the lower circular bearing surface
and replaces the upper arc with slopes; a flat roof leaves a short
bridge above the required circular envelope. Check the usable diameter,
not just the outline's overall height. Vertical holes can print smaller
than drawn; expose any compensation as a parameter rather than changing
an unrelated nominal screw size.

## Fits and fastening

Allow roughly 0.2–0.3 mm clearance as an initial fit allowance, with
about ±0.1 mm possible deviation per surface under the assumed process.
These are different quantities: state whether clearance is across the
diameter or between each pair of mating surfaces. Two opposed surfaces
can accumulate their errors. Slots make adjustment possible when a
precise rigid fit is unnecessary.

Use `printed_fit_parameters` to discover MitCad's fit names and defaults;
use `parameters` to check any existing values before using the names in
expressions. Use
`hole_sizes` for standard screw clearances and recesses; those dimensions
are not a calibration result for the printer.

Where a dedicated feature is absent, keep the recipe editable:

| Need | MitCad construction |
|---|---|
| Adjustable mounting | Constrain a closed slot in a sketch; cut it with `extrude`. Dimension travel and end radii separately. |
| Press fit | Sketch a hexagonal bore, or add small sacrificial inward ribs to a round bore. Constrain the contact size and rib interference independently, then cut or join with `extrude`. Validate insertion on a coupon. |
| Heat-set insert | Sketch and cut a bore using the insert supplier's dimensions, with a lead-in and enough surrounding material. Keep bore diameter, depth and wall thickness as parameters. |
| Captive nut | Sketch a constrained hexagon by its across-flats size plus clearance, then cut with `extrude`. Add a side-entry slot only when insertion needs it, and retain material against pull-out. |

Use the longest screw that fits without bottoming or colliding. For a
tapped printed hole, represent the intended thread with `hole` and its
thread settings; arrange a suitable pilot bore for the physical tap.
For a thread-forming screw, look up `thread_sizes` with the `forming_screw`
standard and preserve the pilot-hole and wall requirements. A cosmetic
thread adds no helical geometry to a printed mesh. If a thread is to be
printed, explicitly request its modeled geometry through `hole` or
`thread`; M8 and larger is a useful starting range to try, not a guarantee
of a working printed thread. Prefer inserts or captive nuts for joints
that must be repeatedly dismantled.

## Supports and bridges

Use about 45° from the vertical as the initial unsupported-overhang
limit. Keep bridges short and accessible; reorient or split the design
when that avoids a difficult supported surface.

For a counterbore opening toward the bed, an ordinary circular recess
may leave an unsupported ceiling. Make the recess as separate sketch
cuts: leave a removable membrane about one layer thick, or use short,
successive orthogonal bridge openings before the final bore. Parameterize
the layer and bridge dimensions; inspect the section and plan how to
remove the membrane. This is a sketch-and-`extrude` recipe, not a field
of `hole`.

For corners that tend to lift, removable mouse ears can be thin discs
joined at the bed with `cylinder` and `combine`, or a constrained sketch
and `extrude`. Keep their removal away from functional mating surfaces.
Distinguish modeled ears from a slicer's brim when estimating plate space.

## Functional details

- For a cantilever clip, start with `snap_fit`; its length, thickness,
  width and catch size are explicit. Orient the arm for its bending load.
  Other flexures can be sketched and extruded with root fillets. A
  `plastic_rule` keeps common design values but does not establish FDM
  strength or validate fatigue; check which feature fields actually use it.
- For print-in-place motion, begin with around 0.3 mm between moving
  surfaces and keep them as separate bodies. Check critical separations
  with `measure` and check unintended overlaps with `interference`.
  MitCad has no general minimum-gap print query; collision-free bodies
  alone do not prove the gap will print open.
- Use `printable_bearing` for its supported bearing geometry and explicit
  radial, axial and element clearances. Preserve the feature's own
  defaults unless the task supplies a tested alternative.
- Make zip-tie channels with a constrained sketch and an `extrude` cut,
  or a `sweep` cut for a bent passage. Check the tie's insertion route,
  bend space and the remaining walls.
- `lip` and `groove` can give a mating rim a deliberate reveal or shadow
  line. For other decorative grooves, use a sketch cut rather than
  assuming these rim features apply to any face. Engrave text using
  `sketch.add_text` and an `extrude` cut; try strokes at least 0.6 mm wide.
- A vase is a slicer mode, not a CAD feature: set `spiral_vase` through
  the plate's process values with `edit_print_plate`. Build a compatible
  continuous outline and check the slicer preview; cavities and ordinary
  CAD wall thickness do not by themselves make a spiral toolpath.

## Checks

1. Confirm the body count, feature status and critical dimensions with
   `timeline`, `bodies`, `properties` and `measure`. Inspect a section for
   internal passages and recesses. Check the bed-face assignment again.
2. Run `print_overhangs` with the intended `bed_face` or explicit down
   direction. Without either, it assumes design −Z; it does not infer
   every body's export orientation. The default angle is 45°.
3. Run `print_thickness` with the required minimum wall; its default
   0.8 mm is an analysis threshold, not a strength certificate. The
   MCP summary suffices until a flagged region needs closer inspection.
4. Check `build_volume` against the selected printer and `print_plates`
   for the intended grouping. Without a printer, obtain its actual bed
   and height before claiming that the design fits. Export arrangement
   can differ from assembly placement; inspect the resulting layout too.
5. Use `print_estimate` for approximate material use. Supply realistic
   wall, layer and infill values; it is not a toolpath or print-time
   guarantee. Use `model_export` for the required mesh and inspect the
   slicer preview for islands, bridges, closed gaps and manifold errors.

Report failed or unavailable checks. Change the geometry or explicitly
state the remaining manufacturing assumption; a clean render is not
evidence that the component will print or carry its load.

## Sources

The design rules are paraphrased and adapted to MitCad. No source images
or direct quotations are reproduced.

- Main reference: Rahix, [Design for 3D-Printing](https://blog.rahix.de/design-for-3d-printing/).
- Complementary reference: Prusa Knowledge Base, [Modeling with 3D printing in mind](https://help.prusa3d.com/article/modeling-with-3d-printing-in-mind_164135).
- Complementary reference: Protolabs Network (Hubs), [How to design parts for FDM 3D printing](https://www.hubs.com/knowledge-base/how-design-parts-fdm-3d-printing/).
