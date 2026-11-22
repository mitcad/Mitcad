# .solidcraft import

Part of [mitcad-import-solidcraft](../README.md); the other imports'
documents are indexed in [core/import](../../import/README.md#documents).

SolidCraft designs (mitcad#450) are read by `core/solidcraft` (plain JSON
or the container; every file format upgraded to 63, SolidCraft 0.45.0's,
by SolidCraft's own steps, `third_party/solidcraft`, mitcad#1331 and
mitcad#1994 and mitcad#2153; a released format has every change of its number, a design
saved while changes were pending only those it lists) and built again as
MitCad features by `core/import-solidcraft` on the `Kernel` trait. A
SolidCraft drawing (`solidcraft-drawing/N`, a file of its own) is refused
with a message saying so.

A design a newer SolidCraft saved (a format past 63, or changes in its
`format_pending` this import does not know) opens with a warning
(mitcad#1994, as SolidCraft's own `newer_file.rs` opens one): "Saved by a
newer SolidCraft (format 64; this import reads SolidCraft designs up to
format 63): what this import reads came in, features it cannot read are
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
| box, cylinder, sphere, torus; cone (format 40) | `box`, `cylinder`, `sphere`, `torus` and `cone` (mitcad#2239: its `radius`, `top_radius` and `height` the file's, a negative height the other way with the base kept) on the plane across their axis through their point. A cylinder placed by distances from edges (format 41) is not mapped |
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
| 3D curves of a sketch: drawn points, lines, arcs and fit splines; a linked curve's exact pieces; format 58's 3D relations and dimensions (`spatial.rs`, mitcad#2157) | a 3D sketch (`spatial_sketch`) on the component's XY plane: the sketch itself when it has nothing else, else `<name> 3D` right after it ([below](#3d-curves-and-frame-members-mitcad2157)) |
| frame members (format 35; `locate` and standard sizes, format 56; `frames.rs`, mitcad#2157) | one `pipe` per line for a round bar or tube along its centre, else per line a construction plane `along_path` at the line's start, the section sketched on it and a `sweep` along the line ([below](#3d-curves-and-frame-members-mitcad2157)) |
| Repair Part (format 49) | none: a placeholder with the reason (MitCad has no such feature) |
| point clouds (formats 43, 45) | none: named in the report as not mapped |
| display and browser state (the `render-*` changes, browser views and nested groups, print faces, identification colours, history visibility, timeline folders' joints; formats 35 to 47) | read and ignored |

The feature kinds of format 27 all have MitCad counterparts; the path
flanges, frame end caps and Repair Part of the later formats have none and
are placeholders with the reason (the cone of format 40 is MitCad's Cone
since mitcad#2239).

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
| | `3d-wire-point-nodes` (52) | a reference to a 3D curve's node (`w3:1`) is the 3D sketch's point, a 3D curve of one piece its curve and, as a path, the typed 3D path (mitcad#2157) |
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
| Frames, sheet metal, primitives | `frame-end-caps` (52) | placeholders: MitCad has no end cap of a member |
| | `frame-member-locate` and `frame-standard-sizes` (56) | read: the profile point `locate` from the profile's own origin; a standard size by the dimensions saved with it, its name what the members are named after (mitcad#2157) |
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

## Formats 58 to 63 (SolidCraft 0.44.0 to 0.45.0, mitcad#2153)

The steps of these formats (`third_party/solidcraft/README.md`) run on an
older design as it opens; a design of format 63 opens as it is.

| Group | Change (format) | MitCad |
|---|---|---|
| Extrude, fillet, move, rib, split | `extrude-from-slanted-face` (58) | From Object on a slanted flat face starts on the face, as MitCad's start object does; an older design's extrude (`from_level`) keeps it with a note: the file starts on a plane parallel to the sketch at the level where the face meets the middle of the profiles |
| | `extrude-one-sided-direction` (59) | read and ignored (a cap's answer to `start`) |
| | `fillet-edge-radii` (60) | `fillet` with a set per edge of its own constant radius (each radius its input's parameter, `Edge radius`), one fillet per body; with any other option (tangent chains, a fit, corners, setbacks or a turn of their own) it fails, as SolidCraft refuses it |
| | `move-rotate-about-edge` (58) | the move's `rotate` about the edge or round face where MitCad finds it on every recompute, turned the file's way |
| | `rib-extend-ends` (58) | the rib's `extend_ends` |
| | `split-planar-tool` (58) | nothing to do: MitCad's split by a face extends a flat face to its plane |
| | `pattern-placements` (58) | a pattern of bodies where a placement of their component shows them is a placeholder |
| | `sweep-path-across-sketches` (63) | a sweep whose path joins curves of several sketches is a placeholder (MitCad's sweep path is curves of one sketch) |
| | `surface-sweep-exact-spline-path` (58) | a surface sweep along a 3D sketch curve sweeps along the 3D curve itself (mitcad#2157); an older design's (`spline_as_samples`, the lines between the spline's samples in the file) with a note |
| | `bounding-solid-side-margins` (58) | six margins of one expression are MitCad's one margin (the first side's input parameter); six of their own a placeholder |
| | `bounding-solid-contents` (63) | a component's envelope is MitCad's bounding solid of no bodies (the component's own) where the component has no subcomponents; else a placeholder |
| Planes and references | `plane-at-angle-axis-origin-expressions` (59) | the axis where its origin's expressions put it now, with a note (a placement, not an input) |
| | `sketch-reference-placements` (58) | a construction plane of another component is where that component stands now, with a note where it stood elsewhere when picked; linked curves fixed where they are, as before |
| | `projected-curve-edge-names`, `closed-control-splines`, `freeform-chain-offsets` (63); `sketch-3d-constraints`, `sketch-3d-dimensions`, `sketch-3d-relations`, `whole-profile-holes-share-a-curve` (58) | linked curves fixed where they are; 3D curves and their relations and dimensions as the 3D sketch of [3D curves and frame members](#3d-curves-and-frame-members-mitcad2157); the rest of a sketch as before |
| Sheet metal | `sheet-hem-gap` (62) | a hem's inside radius is half its gap (the space between its leg and the face it folds over); a hem without a gap takes half its rule's `hem_gap` (SolidCraft's default 0.01 mm without a rule). An older design's gaps are doubled by the step, so its hems keep the radius their gap gave them, and a rule without `hem_gap` gives `0.02 mm` |
| | `sheet-edge-names`, `sheet-face-names` (62) | a base flange's `F<b>:top`, `F<b>:bottom` and `F<b>:side:<line>` translate to the sheet's panel surfaces and the wall along that line, the pick telling top from bottom; edges between them so too. Flanges' own names (`F<f>:far:…`) and a converted sheet's are found at their picks |
| | `sheet-surface-convert`, `sheet-recognize` (62) | Recognize Sheet Metal of a plate from a picked face (an older design's conversion of a plate becomes one) is MitCad's `sheet_convert` of the plate, its thickness the plate's as before, with a note; other recognitions (the largest face, faces left out, bend edges) and Convert to Sheet Metal of a whole body from its faces are placeholders. A conversion of picked faces with more than one face, with engravings or with the material `outside` is a placeholder |
| | `sheet-rule-material` (63) | read and ignored (a label) |
| Imports and assemblies | `import-records`, `import-record-copies`, `import-record-paint`, `import-record-meshes` (60 to 63) | an import kept as B-rep records is a placeholder |
| | `step-import-solid-items` (58) | read and ignored (the STEP text's bodies come in as the kernel reads them) |
| | `joint-lateral-offsets` (58) | a joint's offsets along its first origin's x and y are left out with a note (MitCad's joint offset is along z) |
| | `rigid-groups` (62) | named in the report as not mapped; the parts stay where the file places them |
| | `occurrence-bom-type`, `bom-item-numbers`, `bom-suppressed-parts` (59), `link-inherited-metadata` (63) | read and ignored (bills of materials, linked properties) |
| No geometry | `drawings` (59) | a plain JSON design's drawings named in the report as not mapped |
| | `names-count-characters` (58), `named-view-roll-and-up` (59), `plugin-data-documents` (60), `canvas-anchor` (63) | read and ignored |

## 3D curves and frame members (mitcad#2157)

**3D curves** (`spatial.rs`). A sketch's 3D curves (SolidCraft's
`wires`, `crates/sketch/src/wire.rs`) become one 3D sketch on its
component's XY plane, the points in component coordinates: a curve in
world coordinates (`space` `world`, the default) through the inverse of
the component's placement (its first occurrence's, with a note when the
component has more). A sketch with nothing but 3D curves (no planar curve
or point but the origin, no pattern, offset or equation, links only of
3D curves) is the 3D sketch itself, with its name; one with planar
geometry too keeps its planar sketch and gets the 3D sketch right after
it, `<name> 3D`.

| SolidCraft | MitCad |
|---|---|
| a drawn point (one fit point) | a point |
| a drawn line (two fit points) | `line` |
| an exact arc (`arc`: centre, normal, start, end; its fit points start, through, end) | `arc` (centre, start, end, normal); the through point only where a relation names it, a point on the arc |
| a fit spline (more fit points; one ending where it starts: closed) | `fitted_spline` through the fit points, with a note: MitCad fits its own cubic through them, where the file's curve is a centripetal Catmull-Rom curve |
| a linked curve's exact pieces (`exact`): lines, circles, ellipses, polylines, splines, rational splines | fixed curves: lines, arcs (their normals held by `normal_along`), a full circle and every ellipse as an exact rational quadratic `spline`, a polyline's segments as lines, a piece of a spline cut out of it by knot insertion |
| a linked curve known only by its points (projected onto a face, iso curves), an intersection curve | left out with a note |

Ends that meet (within 1e-6 mm) share one point; in a sketch whose curves
were joined by relations (format 58's `wires_joined`) only its coincident
relations join them, so a joint deleted in the file stays apart. A
design before format 58 has no 3D relations: every drawn point is fixed
where the file has it.

**3D relations and dimensions** (`wire_constraints`, format 58, read by
`mitcad_solidcraft::sketch::WireConstraint`; a format-58 design opens
without the newer-design warning since mitcad#2153):

| SolidCraft | MitCad's 3D sketch |
|---|---|
| `coincident` | one point |
| `point_on_line` | `point_on` where the point lies between the line's ends, else a construction line from the line's start to it held `parallel` to the line |
| `fix` | `fix`; with `exprs`, `offset` dimensions from a point attached to the origin, an expression's coordinate its expression, another its number (mitcad#1912) |
| `along_axis` | `align` with the sense the line has |
| `distance` | a line's `length` between its ends, else a construction line's |
| `axis_distance` | `offset` along the axis (negated where `reversed`) |
| `angle` | `angle` (180° less it where one line is reversed) |
| `radius` | `radius` |
| `parallel`, `perpendicular`, `equal` | the same |
| `collinear` | `parallel` and the second line's start on the first's line |
| `tangent` | `tangent` at the arc's end |
| `on_plane` | `on_plane` of the frame plane (`xy`: `z`) |
| `plane_distance` | `offset` from the origin point along the plane's normal |
| `parallel_to_plane`, `plane_angle`, a kind of a later format | left out with a note naming its kind; the curves stay where the file solved them |

A driving dimension's value is its parameter (`d3 = expression`, the
file's name), a driven one is driven. A relation to the world's axes or
planes in a component placed elsewhere is left out with the reason. When
the relations do not solve, the 3D sketch comes in with every point fixed
and a note. A reference to a 3D curve's node (format 52) is the 3D
sketch's point; a sweep's or pipe's path and rails along 3D curves are the
typed 3D path.

**Frame members** (`frames.rs`, SolidCraft's `eval_frame.rs` and
`frame.rs`). The lines: those named (`lines`, planar lines or straight 3D
curves), else every planar line that is not construction, a centre line
or linked, and every unlinked 3D curve that runs straight (within
0.001 mm), each with a length. The profile's axes are SolidCraft's: its y
up towards +Z as far as the line allows, x across (x × y along the line;
a vertical line's x is +X), turned by `angle` about the line and mirrored
across its y axis by `mirror`; the profile point on the line is `locate`
from the profile's own origin, else the `anchor` (the bounding box's
centre for `origin` and `center` of a built-in profile, its centroid,
a corner or an edge's middle).

- A `round_bar` or `round_tube` whose line runs through its centre: one
  `pipe` per line, `section` `circular`, `size` the diameter and
  `thickness` the wall, each the file's input with its parameter (shared
  by the feature's pipes), square at both ends of its line.
- Every other section (`bar`, `rect_tube`, `angle`, `channel` with or
  without tapered flanges, `i_beam`, a sketch's profiles, a round one off
  its centre): per line a hidden construction plane `along_path` at
  fraction 0 of the line, a sketch on it (`<feature> Section`, hidden)
  whose frame is the member's (the profile point at the line's start) and
  a `sweep` (`orientation` `perpendicular`, a new body) of its material
  along the line. A built-in profile is drawn fixed at its dimensions now
  (SolidCraft's defaults for radii it leaves out), with a note; a sketch's
  profiles are a copy of that sketch with its relations and dimensions
  (their parameters the sketch's), mirrored where the member is, all of
  them the material as nested loops alternate (an outline, a hole, an
  island). A turn given by an expression of parameters is fixed at its
  value, with a note.

Each member follows its line: the plane, the section and the sweep run
along the line's current geometry (a 3D sketch's dimension changed moves
them; after a change of the line's direction the section keeps its turn
from the plane's own x axis). Members are features `<name>`, `<name>
(2)`, …; their bodies are named as the file names them: `names` by line,
else `body_name` or the profile's description (`Rect Tube 40x40x3`, a
standard size's name, the profile sketch's name), `(2)`, `(3)`, … where
taken. A member whose line or section does not build is left out with a
note, the others stay; end caps (`frame_end_cap`) are placeholders, and
Rotate Member turns are not in the formats this import reads.

A reported design (mitcad#2175, GitHub #526's reduced pipe network): a
format-63 file of three sketches of nothing but 3D lines whose ends stop
short of each other (`wires_joined` without relations) and three round
tube members that name their inputs' parameters (`param_names`) and their
bodies by line. It comes in whole: three 3D sketches with every point
fixed where the file has it, one `pipe` per line with each member's
diameter and wall as the parameters the file names, every feature
parametric and every body valid. The reporter's earlier release read it
before formats 58 to 63 (mitcad#2153) and frame members (mitcad#2157)
were read, hence its newer-design warning, partial sketches and
placeholders. Its test is the ctest `cli.solidcraft_pipe_network_reported`
(the file under `MITCAD_MITCAD_REPORTED`, skipped without it); the same
shape written by hand is
`frames_tests::a_pipe_network_of_format_63_comes_in_whole`.

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
