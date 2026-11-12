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

For a horizontal bore, give `hole` a `print_shape`: `teardrop` (45°
sides meeting at a point) or `flat_roof` (the sides cut short by a short
bridge `roof_sag` above the circle, by default the printed fit
`hole_roof_sag`, 0.4 mm). The roof points away from the body's print bed
face, or up the design's Z without one, so set the bed face first; holes
whose axis is not parallel to the bed stay round. The lower arc keeps the
diameter, and the hole keeps its nominal size for drawings and callouts.
It does not combine with `press_fit`, `taper` or a modelled thread; for
those, or a bore that is not a hole, sketch the section and cut it with
`extrude`. Check the usable diameter, not just the outline's overall
height. Vertical holes can print smaller than drawn; expose any
compensation as a parameter rather than changing an unrelated nominal
screw size.

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

For a heat-set insert, `hole_sizes` gives the series' `insert` (hole
diameter and depth per screw size, M2 to M6): make a `hole` with those
values and `insert` `{"size": "M3"}`; use the supplier's values for another
insert, and leave enough wall round it. For a captive nut, `hole_sizes`
gives `nut_pocket` (ISO 4032's nut plus a clearance, `nut_clearance` to
change it): give the `hole` on the face where the screw comes out a
`kind` `nut_pocket` with them; a `slot` lets the nut slide in from the side
and an `offset` buries the pocket below the face. Keep material against
pull-out.

Where a dedicated feature is absent, keep the recipe editable:

| Need | MitCad construction |
|---|---|
| Adjustable mounting | Constrain a closed slot in a sketch; cut it with `extrude`. Dimension travel and end radii separately. |
| Press fit | Sketch a hexagonal bore, or add small sacrificial inward ribs to a round bore. Constrain the contact size and rib interference independently, then cut or join with `extrude`. Validate insertion on a coupon. |

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
leaves an unsupported ceiling. Give the `hole` a `print_bridging`:
`sacrificial` (a membrane one layer thick over the bore, drilled out
after printing) or `stepped` (two layers of orthogonal bridges before the
bore). It also bridges a nut pocket at the hole's start. The layers are the
design's layer height (its print process, else 0.2 mm), and only holes
whose recess opens toward the body's print bed face (with the design's
−Z down without one) are bridged, so set the bed face first. Plan how to
remove the membrane.

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
  surfaces and keep them as separate bodies. `print_min_gap` lists the
  pairs closer than the gap (the design's `print_in_place_gap`, else
  0.3 mm) where they do not touch, also beside a contact (a peg in its
  hole, `touches: true`); a nonzero `unmeasured` means touching pairs it
  could not check. Check unintended overlaps with
  `interference`. A gap above the threshold still needs the slicer
  preview to prove it prints open.
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
   For bodies printed in place, run `print_min_gap` (by `plate` or
   `bodies`) and widen every narrow gap it reports.
4. Check `build_volume` against the selected printer and `print_plates`
   for the intended grouping. An item is on one plate at most:
   `place_on_plate` moves one from another plate, and its result's
   `moved` names the plate it left and its count. Without a printer, obtain its actual bed
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
