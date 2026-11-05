# .solidcraft import

Part of [mitcad-import-solidcraft](../README.md); the other imports'
documents are indexed in [core/import](../../import/README.md#documents).

SolidCraft designs (mitcad#450) are read by `core/solidcraft` (plain JSON
or the container; every file format upgraded to 27 with the changes
pending after it by SolidCraft's own steps, `third_party/solidcraft`) and
built again as MitCad features by `core/import-solidcraft` on the `Kernel` trait.
The file keeps definitions only, not bodies: nothing is checked against
stored results, and nothing falls back to them. A feature that does not
map or fails is a suppressed placeholder with the reason, and the timeline
goes on. Command, options and report: [commands.md](../../model-api/src/commands.md#solidcraft-import).

| SolidCraft | MitCad |
|---|---|
| parameters, sketch dimensions' and feature inputs' parameters | parameters of the same names and expressions; a dimension's or input's is its slot's (`params.rs`) |
| sketch (`sketch.rs`): lines, circles, arcs, ellipses, splines (control or fit points), conics; constraints and dimensions | `sketch`: the same curves (a conic as a rational quadratic spline), constraints and dimensions; an angle as the sector between the rays; a point placed by expressions by signed distances from the origin. Left out: pattern relations, line angles and arc sweeps (other than level or upright lines), slot overall lengths, offset distances; projected and linked curves fixed where they are |
| origin, explicit, placed, offset, angled, construction, flipped, face, midplane, two-edge, three-point, tangent, along-path and perpendicular planes (`planes.rs`) | origin planes, construction planes (fixed for an explicit plane, a placed one, a turn about a line outside the plane and a perpendicular one), a face; a sketch's own frame in the plane |
| extrude (distance, two sides, symmetric per side or in total, through all, to and from an object, start offset, taper); thin extrude along open chains and closed loops of sketch curves (centre, inside, outside, two sides; mitcad#463) | `extrude`; a thin one with `thin.curves` and no profiles: inside is side 2 (the area a closed chain encloses, the side an open one bends toward), outside side 1, two sides `both` (the outside thickness on side 1) |
| revolve (angle, two angles, symmetric, full, to an object; the newer convention's turn towards the sketch normal) | `revolve` |
| fillet (constant, chord, curvature, variable, spline radius), chamfer (equal, two distances, distance and angle, the first face the one facing up) | `fillet`, `chamfer` (one per body); a spline radius (mitcad#464) one `fillet` per edge, as the file rounds them one at a time, its `spline` size from the edge's end nearest the file's start point |
| box, cylinder, sphere, torus | `box`, `cylinder`, `sphere`, `torus` on the plane across their axis through their point |
| combine, pattern (rectangular, circular, along sketch curves: one or a chain, mitcad#464; flipped from the path's end), mirror, move (turn then move), scale, align, split, split face, offset/delete/replace face, remove | `combine`, `rectangular_pattern`, `circular_pattern`, `path_pattern`, `mirror`, `move` (a rotate and a translate), `scale`, `align`, `split_body`, `split_face`, `offset_face`, `delete_face`, `replace_face`, `remove_body` |
| hole (at a point on a face or at sketch points; simple, drilled, counterbore, countersink; depth, through all, to an object), thread | `hole`, `thread`; a modelled thread's `ends` `run_out` (mitcad#1302): SolidCraft runs its groove a pitch past an end of the face through what lies there (a cone or countersink too) and stops it square at an end inside the face; its threads have no end chamfers |
| shell, draft, loft (ruled), sweep (orientation, twist, end scale, one or two rails; mitcad#464; the rails' points where they cross the plane square to the path, mitcad#469), pipe, coil, rib and web | `shell`, `draft`, `loft`, `sweep`, `pipe`, `coil`, `rib`, `web` |
| emboss (from the sketch plane, or wrapped onto the cylindrical wall its face picks; deboss) | `emboss` (mitcad#458): the wall found by its persistent name or at its point, `sink` for a deboss |
| construction plane, axis, point | `construction_plane`, `construction_axis`, `construction_point` |
| import (STEP), mesh import | `base` features of the bodies the kernel reads |
| body transfer (bodies by name, from one component into another; `imports.rs`) | `body_transfer` in the destination, linked at the components' first placements (mitcad#465) |
| components, occurrences (placements, grounded), joints (rigid, revolute, slider, cylindrical, pin slot, planar, ball; `assembly.rs`) | components, occurrences, `joint` where it holds at the file's placements (driven at the file's values, else free), else `as_built_joint` |
| sheet rules, base flange, edge flange, hem, fold, unfold, refold (`sheet.rs`) | `sheet_rules`, `sheet_face`, `sheet_flange` (a hem a 180-degree flange), `sheet_fold`, `sheet_unfold`, `sheet_refold` |
| contact sets (occurrences, suppressed; contact off, all contact; `assembly.rs`) | contact sets of the occurrences' paths (mitcad#454): suppressed as in the file, all of them with contact off; all contact a set `All Contact` of every occurrence |
| motion studies (steps, keys: joint, value index, `[step, value]` points; `assembly.rs`) | motion studies: a key on the `joint` feature the file's joint became, the index the kind's motion in SolidCraft's order (cylindrical and pin slot: the turn first); keys of joints that came in as `as_built_joint` or not at all left out with a note |
| contour flange, convert to sheet metal, perforation (`sheet.rs`, mitcad#461) | `sheet_contour` (the material right of travel, left with `flip`; the width against the normal with `reverse`), `sheet_convert` (the face and edges at its points; rules of the thickness the file takes from the body: a plate's volume over its face's area, a shell's wall, else the rule's), `sheet_perforate` (the newest sheet without a body; the holes cut through the sheet, the grid in its first panel's axes) |
| motion links (`a`/`ia`, `b`/`ib`, `ratio`, coupling: gear, rack and pinion, screw by their sizes as numbers, expressions or picked circle centres, cam by its sketch or picked edges, knife, roller or flat follower and tip; `assembly.rs`) | `motion_link` features (mitcad#466) between the `joint` (or `as_built_joint`) features the file's joints became, the value indices as the kinds' motions; a plain link a `ratio`; a screw's pitch its lead; a cam's sketch profile the sketch's curves, its picked edges the planar face of the cam whose plane holds them; the offset MitCad's own (`as_built`: the joints stay where the file places the parts). A link whose joints did not come in, that does not fit or that would move the parts is left out with a note (the report's `links`) |
| bounding solid (`bodies`, none: every body; `margin`, 0 when the file has none; mitcad#460) | `bounding_solid` over the same bodies (none: every body of the feature's component, with a note when bodies of other components are left out), the margin its input's parameter; the box named as the file names it (its stored body name, else `Bounding`, made unique). The box is that of the exact geometry, where SolidCraft bounds the display mesh: on curved bodies the file's box can be a little smaller |
| boundary fill (tools, a point in each cell, operation, `remove_tools`; `features.rs`) | `boundary_fill` (mitcad#457): the same tools and points; a join, cut or intersection works on the feature's participants (`*` or none: all bodies of its component), the tools it removes left out of them. Two points in one cell fill it once (the file makes two bodies there); a join of cells that touch no participant fails (the file adds them to its first body as lumps of their own) |
| mesh conversion (`imports.rs`, mitcad#462) | `mesh_convert` of the body by its name, the tolerance its input's parameter (the body keeps its name) |
| patch (sketch regions; a closed loop of a body's edges picked by points, its continuity and points; `surfaces.rs`), stitch, thicken (one side or symmetric, new body or joined, cut or intersected with its targets; mitcad#456) | `patch` (a region's surface body each; the loop the named edges at the picks, or the body's opening the picks lie on, by the faces it runs along; single edges' continuity where they have names; the crown left out with a note), `stitch` (the first body keeps its name; a tolerance of 0 MitCad's own precision), `thicken` (the surfaces stay) |
| surface extrude, surface revolve, surface sweep (profiles, none without a selection; curves; `surface_modelling.rs`, mitcad#455) | `extrude`, `revolve`, `sweep` with `surface` and `curves`, mapped as the solid ones (extent, angles turned towards the sketch normal, path); a sweep of curves with a twist, an end scale or rails is not mapped (MitCad sweeps curves without them) |
| surface loft (sections: a profile or curves of a sketch; end continuity, crown) | `loft` with `surface`: `profile` or `curves` sections, ruled between two sections and smooth through more; continuity and crown beyond G0 left out with a note; a mix of profile and curve sections, and sections on a body's edges, are not mapped |
| offset surface (faces picked, or a body; chain; tolerance), ruled surface (edges picked, mode, angle, direction, flip, corners, the face beside), extend (edges picked, distance, mode, neighbours) | `offset_surface` (the faces at the picks; every face of the body without; the tolerance not needed: the offset is exact), `ruled_surface`, `extend_surface`: picked edges are the named edges at the picks or, on an open boundary (free edges, which have no names), the opening the picks lie on with the faces the picked edges bound; an extend up to a plane, a face or the bounds is not mapped, one without `neighbours` notes that its sides follow the surface |
| surface trim (a plane, a tool face picked or sketch curves with a direction; keep points; a mutual trim `with`), surface untrim (the face picked; holes, all or picked; outer; in place), unstitch, reverse normal (faces picked, none: every face) | `trim_surface` (the plane as a reference, the face at its pick, the sketch's curves; every keep point), `untrim_surface`, `unstitch`, `reverse_normal`; a mutual trim, a trim by every curve of a sketch and an untrim of some of a face's holes are not mapped |
| surface blend (two chains of edges picked on bodies; continuity and bulge per end; align; stitch), boundary patch (two to four curves of a sketch; orders; crown), network (curves one way and crossing: sketch curves, edges picked, a point; continuity; new body or join), match (a face picked or the first; the target's edges picked; order), fair (faces picked or every face; strength, iterations, keep edges, tolerance) | `blend_surface` (each end's chain from its picks, its continuity up to G2 and bulge; the alignment points left out with a note; a stitched blend sewn into the first body, which keeps its name), `boundary_surface` (three or four curves a loop, two a loft of them: the curves lie in the sketch's plane, so every order holds; a crowned patch is not mapped), `boundary_surface` with `u` and `v` (a joined one followed by a `stitch` named `<name> Join` into the body of its first curve on edges; continuity on network curves is not mapped), `match_surface` (one face; several are not mapped), `fair_surface` (the tolerance its input's parameter); G3 is not mapped. Free edges picked on a surface are the opening the pick lies on, the faces the picked edges bound and the picks (`at`) |
| boss, lip and groove, snap fit, rest (`plastic.rs`, mitcad#459); the plastic rule assigned to the feature's body, else the active one (a design's own rule before the library's, `third_party/solidcraft/plastic.rs`) | `boss`, `lip` or `groove` (the file's height a groove's depth), `snap_fit`, `rest`, each at its pick's face with the file's direction; the rule a `plastic_rule` feature (every expression, `Thickness` its own thickness parameter) made before the first feature that needs it, named in their `rule` |

Every SolidCraft feature kind has a MitCad counterpart; options MitCad has not
are named in each row (such a feature is a placeholder with the reason).
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
