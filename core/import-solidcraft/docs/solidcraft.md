# .solidcraft import

Part of [mitcad-import-solidcraft](../README.md); the other imports'
documents are indexed in [core/import](../../import/README.md#documents).

SolidCraft designs (mitcad#450) are read by `core/solidcraft` (plain JSON
or the container; every file format upgraded to 57, SolidCraft 0.43.0's,
by SolidCraft's own steps, `third_party/solidcraft`, mitcad#1331 and
mitcad#1994; a released format has every change of its number, a design
saved while changes were pending only those it lists) and built again as
MitCad features by `core/import-solidcraft` on the `Kernel` trait. A
SolidCraft drawing (`solidcraft-drawing/N`, a file of its own) is refused
with a message saying so.

A design a newer SolidCraft saved (a format past 57, or changes in its
`format_pending` this import does not know) opens with a warning
(mitcad#1994, as SolidCraft's own `newer_file.rs` opens one): "Saved by a
newer SolidCraft (format 58; this import reads SolidCraft designs up to
format 57): what this import reads came in, features it cannot read are
placeholders with the reason, and what it does not know is left out.
Features may build differently from the newer SolidCraft, which can give a
known option a new meaning." It is the first warning of the result's
notices and the report's `newer` (`format`, `reads_up_to`,
`unknown_changes`, `left_out`, `warning`). What the reader reads is read: a
feature of a kind it does not know is a placeholder ("saved by a newer
SolidCraft: a `…` feature, which this import does not know"); a section of
the design it does not know or cannot read, and a parameter it cannot read,
are left out and named in `left_out`. A field it does not know inside a
feature is not named (the reader cannot tell it from a field it ignores).
Only a file that is not a design is refused.
The file keeps definitions only, not bodies: nothing is checked against
stored results, and nothing falls back to them. A feature that does not
map or fails is a suppressed placeholder with the reason, and the timeline
goes on. Command, options and report: [commands.md](../../model-api/src/commands.md#solidcraft-import).

| SolidCraft | MitCad |
|---|---|
| parameters, sketch dimensions' and feature inputs' parameters | parameters of the same names and expressions; a dimension's or input's is its slot's (`params.rs`) |
| sketch (`sketch.rs`): lines, circles, arcs, ellipses, splines (control or fit points; a control spline's own knots and weights, format 49), conics; constraints and dimensions | `sketch`: the same curves (a conic as a rational quadratic spline, a control spline with its knots and weights), constraints and dimensions; an angle as the sector between the rays; a point placed by expressions by signed distances from the origin. Left out: pattern relations, line angles and arc sweeps (other than level or upright lines), slot overall lengths, offset distances, the distances to a circle's edge from a line, a point or another circle (formats 28 and 36), constraints of a kind a later format brings; an equation curve's formulas (format 44: the curve stays the spline it placed); projected and linked curves fixed where they are |
| origin, explicit, placed, offset, angled (about a line, or an edge or round face of the model: format 39), construction (by feature id, format 40), flipped, face, midplane, two-edge, axis and point (format 47), through a point (format 36), three-point, tangent, along-path and perpendicular planes (`planes.rs`) | origin planes, construction planes (fixed for an explicit plane, a placed one, a turn about a line outside the plane and a perpendicular one; `offset_through_point`, `edge_and_point`, an angle plane's line the edge or face), a face; a sketch's own frame in the plane. A tangent plane turned from a reference plane (format 48) is not mapped |
| extrude (distance, two sides, symmetric per side or in total, through all, to and from an object, start offset, taper and its corner rule, format 49; a Join To Object that keeps the body it runs to apart, format 49 and every design before format 40); thin extrude along open chains and closed loops of sketch curves (centre, inside, outside, two sides; mitcad#463) | `extrude`; a thin one with `thin.curves` and no profiles: inside is side 2 (the area a closed chain encloses, the side an open one bends toward), outside side 1, two sides `both` (the outside thickness on side 1); a join keeping the body it runs to apart joins its targets (none: its component's bodies) but that one, and is a new body with a note when that leaves none (mitcad#1603); the taper's corner rule is MitCad's own, with a note |
| revolve (angle, two angles, symmetric, full, to an object; the newer convention's turn towards the sketch normal; about a construction axis, format 31, or an edge or round face of the model, format 39) | `revolve`, its axis the construction axis or the edge or face where MitCad finds it |
| fillet (constant, chord, curvature, variable, spline radius; only the picked edges, format 49), chamfer (equal, two distances, distance and angle, the first face the one facing up; only the picked edges) | `fillet`, `chamfer` (one per body; `tangent_chain` as the file's); a spline radius (mitcad#464) one `fillet` per edge, as the file rounds them one at a time, its `spline` size from the edge's end nearest the file's start point. A fillet whose corners are set back by distances of their own is not mapped |
| box, cylinder, sphere, torus; cone (format 40) | `box`, `cylinder`, `sphere`, `torus` on the plane across their axis through their point. A cylinder placed by distances from edges (format 41) and a cone (MitCad has no cone primitive) are not mapped |
| combine, pattern (rectangular, circular about a fixed line, an origin of expressions (format 48), a construction axis (format 31) or an edge or round face (format 39), along sketch curves: one or a chain, mitcad#464; flipped from the path's end), mirror, move (turn then move), scale, align (points and normals; the turn after the move, format 50), split, split face, offset/delete/replace face, remove. The copied features of a pattern or mirror are feature ids (format 40) | `combine`, `rectangular_pattern`, `circular_pattern` (the axis the construction axis or the edge or face, else fixed where the origin's expressions put it now, with a note), `path_pattern`, `mirror`, `move` (a rotate and a translate), `scale`, `align` (its `angle`), `split_body`, `split_face`, `offset_face`, `delete_face`, `replace_face`, `remove_body`. An align by planes (format 29), by frames (format 50) or along some axes only (format 46) is not mapped |
| hole (at a point on a face or at sketch points; simple, drilled, counterbore, countersink, the last two with a drill point, format 36; depth, through all, to an object; a cosmetic or modelled tapped thread with its depth and hand, formats 36 and 49), thread (length; the end it starts from; form, hand and starts; modelled with its end chamfers and runout, format 49) | `hole`, `thread`. A cosmetic tapped thread is a note (the hole keeps its diameter); a modelled one MitCad's tapped `thread` (ISO metric, Unified, G, Rc, NPT; its depth the thread's `length`, its ends `flat`, the bore the size's minor diameter, with notes). A thread's form is MitCad's standard (metric, trapezoidal, `bspp` Whitworth, `bspt` ISO taper, NPT), left-hand `right_handed: false`, its starts `starts`, its chamfered tips `chamfer` (`start`, `both`) at MitCad's default size, its runout left out with a note; a modelled thread's `ends` `run_out` (mitcad#1302): SolidCraft runs its groove a pitch past an end of the face through what lies there (a cone or countersink too) and stops it square at an end inside the face. Not mapped: a hole placed by distances from edges (format 35), a thread up to a face or plane, of the acme, buttress, square or Rp forms, or of both hands |
| shell, draft, loft (ruled), sweep (orientation, twist, end scale, one or two rails; mitcad#464; the rails' points where they cross the plane square to the path, mitcad#469), pipe, coil, rib and web | `shell`, `draft`, `loft`, `sweep`, `pipe`, `coil`, `rib`, `web`. A loft along guide rails (format 28) and a coil whose section is a sketch's profile (format 36) are not mapped |
| emboss (from the sketch plane, or wrapped onto the cylindrical wall its face picks; deboss) | `emboss` (mitcad#458): the wall found by its persistent name or at its point, `sink` for a deboss |
| construction plane, axis, point | `construction_plane`, `construction_axis`, `construction_point` |
| import (STEP), mesh import | `base` features of the bodies the kernel reads |
| body transfer (bodies by name, from one component into another; `imports.rs`) | `body_transfer` in the destination, linked at the components' first placements (mitcad#465) |
| components, occurrences (placements, grounded), joints (rigid, revolute, slider, cylindrical, pin slot, planar, ball; `assembly.rs`); joint origins on faces, circles, frames, points (with their x, format 28) and edges (with a fixed z, format 38) | components, occurrences, `joint` where it holds at the file's placements (driven at the file's values, else free), else `as_built_joint`; relations (format 28) have no counterpart, noted |
| sheet rules, base flange, edge flange (inside, outside, middle; material outside and tangent, format 37; heights from the outer or the inner virtual sharp, format 37), hem, fold, unfold, refold (`sheet.rs`) | `sheet_rules`, `sheet_face`, `sheet_flange` (outside `edge`, material outside `outside`, tangent `side_tangent`; the length `outer` or `inner`; a hem a 180-degree flange), `sheet_fold`, `sheet_unfold`, `sheet_refold`. A flange whose heights are measured from the tangent line, and a path flange (format 36), are not mapped |
| contact sets (occurrences, suppressed; contact off, all contact; `assembly.rs`) | contact sets of the occurrences' paths (mitcad#454): suppressed as in the file, all of them with contact off; all contact a set `All Contact` of every occurrence |
| motion studies (steps, keys: joint, value index, `[step, value]` points; `assembly.rs`) | motion studies: a key on the `joint` feature the file's joint became, the index the kind's motion in SolidCraft's order (cylindrical and pin slot: the turn first); keys of joints that came in as `as_built_joint` or not at all left out with a note |
| contour flange, convert to sheet metal, perforation (`sheet.rs`, mitcad#461) | `sheet_contour` (the material right of travel, left with `flip`; the width against the normal with `reverse`), `sheet_convert` (the face and edges at its points; rules of the thickness the file takes from the body: a plate's volume over its face's area, a shell's wall, else the rule's), `sheet_perforate` (the newest sheet without a body; the holes cut through the sheet, the grid in its first panel's axes); a field bounded by a sketch's profiles (format 48) is not mapped |
| motion links (`a`/`ia`, `b`/`ib`, `ratio`, coupling: gear, rack and pinion, screw by their sizes as numbers, expressions or picked circle centres, cam by its sketch or picked edges, knife, roller or flat follower and tip; `assembly.rs`) | `motion_link` features (mitcad#466) between the `joint` (or `as_built_joint`) features the file's joints became, the value indices as the kinds' motions; a plain link a `ratio`; a screw's pitch its lead; a cam's sketch profile the sketch's curves, its picked edges the planar face of the cam whose plane holds them; the offset MitCad's own (`as_built`: the joints stay where the file places the parts). A link whose joints did not come in, that does not fit or that would move the parts is left out with a note (the report's `links`) |
| bounding solid (`bodies`, none: every body; `margin`, 0 when the file has none; mitcad#460) | `bounding_solid` over the same bodies (none: every body of the feature's component, with a note when bodies of other components are left out), the margin its input's parameter; the box named as the file names it (its stored body name, else `Bounding`, made unique). The box is that of the exact geometry, where SolidCraft bounds the display mesh: on curved bodies the file's box can be a little smaller |
| boundary fill (tools, a point in each cell, operation, `remove_tools`; `features.rs`) | `boundary_fill` (mitcad#457): the same tools and points; a join, cut or intersection works on the feature's participants (`*` or none: all bodies of its component), the tools it removes left out of them. Two points in one cell fill it once (the file makes two bodies there); a join of cells that touch no participant fails (the file adds them to its first body as lumps of their own) |
| mesh conversion (`imports.rs`, mitcad#462) | `mesh_convert` of the body by its name, the tolerance its input's parameter (the body keeps its name) |
| patch (sketch regions; a closed loop of a body's edges picked by points, its continuity and points; `surfaces.rs`), stitch, thicken (one side or symmetric, new body or joined, cut or intersected with its targets; mitcad#456) | `patch` (a region's surface body each; the loop the named edges at the picks, or the body's opening the picks lie on, by the faces it runs along; single edges' continuity where they have names; the crown left out with a note), `stitch` (the first body keeps its name; a tolerance of 0 MitCad's own precision), `thicken` (the surfaces stay) |
| surface extrude, surface revolve, surface sweep (profiles, none without a selection; curves; `surface_modelling.rs`, mitcad#455) | `extrude`, `revolve`, `sweep` with `surface` and `curves`, mapped as the solid ones (extent, angles turned towards the sketch normal, path); a sweep of curves with a twist, an end scale or rails is not mapped (MitCad sweeps curves without them) |
| surface loft (sections: a profile or curves of a sketch; end continuity, crown) | `loft` with `surface`: `profile` or `curves` sections, ruled between two sections and smooth through more; continuity and crown beyond G0 left out with a note; a mix of profile and curve sections, sections on a body's edges and guide rails (format 28) are not mapped |
| offset surface (faces picked, or a body; chain; tolerance), ruled surface (edges picked, mode, angle, direction, flip, corners, the face beside), extend (edges picked, distance, mode, neighbours) | `offset_surface` (the faces at the picks; every face of the body without; the tolerance not needed: the offset is exact), `ruled_surface`, `extend_surface`: picked edges are the named edges at the picks or, on an open boundary (free edges, which have no names), the opening the picks lie on with the faces the picked edges bound; an extend up to a plane, a face or the bounds is not mapped, one without `neighbours` notes that its sides follow the surface |
| surface trim (a plane, a tool face picked or sketch curves with a direction; keep points; a mutual trim `with`), surface untrim (the face picked; holes, all or picked; outer; in place), unstitch, reverse normal (faces picked, none: every face) | `trim_surface` (the plane as a reference, the face at its pick, the sketch's curves; every keep point), `untrim_surface`, `unstitch`, `reverse_normal`; a mutual trim, a trim by every curve of a sketch and an untrim of some of a face's holes are not mapped |
| surface blend (two chains of edges picked on bodies; continuity and bulge per end; align; stitch), boundary patch (two to four curves of a sketch; orders; crown), network (curves one way and crossing: sketch curves, edges picked, a point; continuity; new body or join), match (a face picked or the first; the target's edges picked; order), fair (faces picked or every face; strength, iterations, keep edges, tolerance) | `blend_surface` (each end's chain from its picks, its continuity up to G2 and bulge; the alignment points and the seam points (format 28) left out with a note; a stitched blend sewn into the first body, which keeps its name), `boundary_surface` (three or four curves a loop, two a loft of them: the curves lie in the sketch's plane, so every order holds; a crowned patch is not mapped), `boundary_surface` with `u` and `v` (a joined one followed by a `stitch` named `<name> Join` into the body of its first curve on edges; continuity on network curves is not mapped), `match_surface` (one face; several are not mapped), `fair_surface` (the tolerance its input's parameter); G3 is not mapped. Free edges picked on a surface are the opening the pick lies on, the faces the picked edges bound and the picks (`at`) |
| boss, lip and groove, snap fit, rest (`plastic.rs`, mitcad#459); the plastic rule assigned to the feature's body, else the active one (a design's own rule before the library's, `third_party/solidcraft/plastic.rs`) | `boss`, `lip` or `groove` (the file's height a groove's depth), `snap_fit`, `rest`, each at its pick's face with the file's direction; the rule a `plastic_rule` feature (every expression, `Thickness` its own thickness parameter) made before the first feature that needs it, named in their `rule`. A snap fit with a base below the face or as a body of its own is not mapped |
| frame members (format 35), Repair Part (format 49) | none: placeholders with the reason (MitCad has no such features) |
| point clouds (formats 43, 45) | none: named in the report as not mapped |
| display and browser state (the `render-*` changes, browser views and nested groups, print faces, identification colours, history visibility, timeline folders' joints; formats 35 to 47) | read and ignored |

The feature kinds of format 27 all have MitCad counterparts; the cone, frame
members, path flanges and Repair Part of the later formats have none and are
placeholders with the reason.

## Formats 51 to 57 (SolidCraft 0.41 to 0.43.0, mitcad#1994)

| Group | Change (format) | MitCad |
|---|---|---|
| Holes and threads | `bridged-counterbore` (51) | the counterbore's bridge layers are MitCad's stepped `print_bridging`, with a note: MitCad's layers are the print's layer height and only where the counterbore opens toward the print bed |
| | `hole-direction-own-faces` (51) | nothing saved: a hole drills along the file's direction |
| | `thread-continuous-flanks` (52), `thread-single-face-selection` (57) | read and ignored (how the faces are named, the dialog) |
| | `hole-sketch-plane-start` (53) | holes at sketch points start at the sketch's plane; an older design's hole at points of a sketch on a derived plane (`old_surface_start`) has a note: the file moves a point inside a body back to the body's surface |
| | `hole-thread-class` (57) | read and ignored (a drawing's callout) |
| | `linked-cosmetic-threads` (57) | read and ignored (a linked import's records) |
| | `thread-multiple-faces` (57) | one `thread` per face, on the body `target_bodies` names; the others named `<name> (2)`, … |
| Extrude, revolve, sweep, fillet | `two-sided-extrude-fixed-sides` (51) | a two-sided extrude of entered distances is MitCad's signed `two_sides` (Direction fixes the sides, a sign turns its own side over); an older design's (`second_follows_first`) the unsigned sides, flipped where its first distance sends side one now |
| | `revolve-axis-origin-expressions` (51) | the fixed axis where its origin's expressions put it now, with a note |
| | `fillet-turn` (51) | a swept turn round a sharp side edge is a note: the rounds meet as MitCad's fillet makes them |
| | `transferred-profile-frame` (51) | nothing saved |
| | `revolve-axis-sketch` (52) | the axis line in the sketch the file names |
| | `two-sided-cut-cap-names` (52) | a cap named `top` or `bottom` is found by its height over the sketch, else at its pick |
| | `sweep-path-on-body-edges` (53) | the sweep's path is the body's edges at the picks (`path_body`, `path_edges`) |
| | `extrude-to-object-keeps-picked-placements` (57) | the extrude runs to its object where the components stand; a note where they stood elsewhere when the object was picked |
| | `sweep-walls-named-along-the-path` (57) | read (the step drops sketch links' names; the import fixes linked curves where they are) |
| Sketches and 3D wires | `intersection-curve-sketch-curves` (51), `sketch-link-construction-point` (52), `sketch-vertex-link-names` (55) | linked curves fixed where they are, as before |
| | `sketch-point-along` (51) | the point on its curve (a coincidence), its share along the curve left out with a note |
| | `3d-wire-point-nodes` (52) | a reference to a 3D wire's node fails (3D curves are not brought in) |
| | `sketch-text-vertical-align` (52) | texts are left out, as before |
| | `sketch3d-profiles` (52) | a patch of 3D sketch loops is a placeholder |
| | `spline-tangent-handles` (55) | a fit spline's dragged tangent handles are left out with a note |
| | `split-face-loops` (55) | a split face along joined loops: `extend` off |
| Planes and references | `plane-at-angle-direction-only` (51) | the plane turns about the axis direction projected onto the base, through the pivot (a point, the origin or a vertex) or the base's origin, fixed where it is now |
| | `nested-face-name` (52) | face names that hold another's in parentheses (`F5:blend(…)`, `F3:inner(…)`) read as the older form |
| | `component-origin-datums` (52) | a component's origin plane, axis or point: this component's own where it is its own, else fixed where the placement puts it now, with a note |
| | `construct-snap-points` (55) | a reference to a snap point fails with the reason |
| | `plane-at-angle-follows-named-axis` (57) | the construction axis the plane turns about |
| | `saved-face-points` (57) | read and ignored (a model cache) |
| Frames, sheet metal, primitives | `frame-end-caps` (52), `frame-member-locate` and `frame-standard-sizes` (56) | placeholders, as frame members are |
| | `sheet-flange-segments` (52) | a flange whose edge is split into stretches is a placeholder |
| | `box-centred` (54) | MitCad's box from the corner its center and size give now, its height both ways (`symmetric`), with a note |
| Assemblies, placement and patterns | `component-origin-visibility`, `joint-captured-positions`, `joint-constraint-tag` (52) | read and ignored |
| | `joint-origin-inside` (52) | the joint origin's path goes down through the occurrences it is inside |
| | `joint-plane-and-axis-snaps` (52) | an origin on an origin or explicit plane, a world axis or a fixed line: a point there with the plane's normal or the axis as z; other planes and axes fail with the reason |
| | `solid-move-origin` (55) | the move turns about its point, where its expressions put it now |
| | `occurrence-move-expressions` (56), `placement-expressions` (57) | placed where the file saved them, a note naming them |
| | `pattern-names-by-source` (56) | read (copies are named in the order of the stored names) |
| | `move-copy-placements` (57) | a move whose copies take their sources' placements in another component is a placeholder |
| New feature kinds | `cell-grid-feature` (56), `cell-grid-*` (57) | placeholders: MitCad has no cell grid |
| | `point-cloud-*` (51) | point clouds named in the report as not mapped |
| | `notes` (52) | named in the report as not mapped |
| | `configuration-row-ids` (53), `linked-configuration` (57) | read and ignored (configurations are not mapped) |
| | `blend-definition-faces`, `plugin-data-namespace` (55) | read and ignored |
| Display only | `canvas-lock`, `cpu-output-presentation`, `embedded-surface-textures`, `glass-material-models` (51), `opus-quality`, `render-indirect-clamp`, `render-quality-controls` (52) | read and ignored | Options MitCad has not are named in each row
(such a feature is a placeholder with the reason). What a join makes of
pieces that touch only along an edge or at a point (`touching`, format 49)
is MitCad's own.
Body transfers between components are `body_transfer`
features in their destinations (mitcad#465; the file's recorded
conversion kept as the feature's `transform` only where the components'
placements give another). Exploded views are read and kept by
`core/solidcraft` and named in the report as not mapped.

References (`refs.rs`, `names.rs`): SolidCraft resolves a pick by its
persistent name first, by the point only when the name finds nothing; the
points stay where the face was when it was picked, so a design saved after
a parameter change keeps picks off their faces. The import translates the
names it can into the faces they may mean and takes the one nearest the
pick, else the face or edge at the pick (within 0.05 mm, the picks of
curved faces lying on their triangles). The faces and edges found are
then MitCad's topological names, which follow later edits.

Tests: see [commands.md](../../model-api/src/commands.md#solidcraft-import).
The test designs (`core/solidcraft/tests/designs`) are written by hand;
their expected values are worked out from SolidCraft's definitions
(`core/solidcraft/tests/designs/README.md`). SolidCraft itself is not built
or run for them.
