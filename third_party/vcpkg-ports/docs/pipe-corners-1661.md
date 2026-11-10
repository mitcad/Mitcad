# Pipe shells' corners between curved pieces: 1661 to 1666

mitcad#1559, the follow-up of mitcad#1555 (GitHub mitcad/Mitcad#208): a
pipe along an intersection curve whose pieces meet at angles came out with
its section leaning from square and pinched. mitcad#1564, its follow-up:
the same with rounded corners. Back to the
[port's README](../README.md).

## Overview

`BRepOffsetAPI_MakePipeShell` builds a corner of its spine, mitred
(`BRepBuilderAPI_RightCorner`) or rounded (`BRepBuilderAPI_RoundCorner`),
by sweeping each side past the corner and trimming the two against each
other (`BRepFill_TrimShellCorner`). Where that fails, MitCad's pipes try
the other kind and, last, `BRepBuilderAPI_Transformed`, which carries the
section unturned through the corner, so that along the next piece it leans
from square by the corner's angle (`geometry/src/spine.cpp`, `pipe_shell`).

- **1661** ends each side's seam lines where they cross the other side
  when the two sides' seams do not cross each other (sides swept along
  curves other than lines and circles), and matches the ends of the
  section edges' curves on the faces in 3D within the edges' tolerance.
- **1662** makes a turn of the spine's tangent from 1e-4 rad on a mitred
  corner (it was 1e-2 rad): through a smaller one the section keeps its
  frame and leans by the turn. Rounded corners keep 1e-2 rad.
- **1663** cuts a mitre along the bisector plane where the intersection
  of the two sides breaks up (sides of one section touch at two points of
  the corner, and meet at a shallow angle along a small one), joining the
  two sides' cuts along edges in their middle; and keeps `ChooseSection`
  from asking parallel edges for their extremum's points.
- **1664** extends a B-spline side past a mitred corner as the prism of
  its end section (it was extrapolated from the approximated surface's
  derivatives).
- **1665** keeps `BOPDS_DS::Paves` from writing past its array for an edge
  whose pave blocks do not chain (a heap corruption the corners' trims
  reached).
- **1666** cuts a rounded corner's sides along the bisector plane where
  they overlap inside the corner and their intersection breaks up, and
  keeps each side's end where it does not reach the plane, for the round
  to turn about the corner.

## 1661-brepfill-trim-shell-corner-each-seam-ends-where-it-crosses-the-other-side

The trim intersects the two sides (`BOPAlgo_PaveFiller`) and, for each
pair of faces, replaces the face's end by the section edges between the
vertices where its seam lines (U-edges) end. `SplitUEdges` ended the seams
of both sides at the vertex where they cross each other. Swept along lines
and circles, the sides are cylinders and tori whose corresponding seams
cross exactly. Swept along other curves (B-splines, an ellipse's arc) the
sweep's frame turns along the piece and its surfaces are approximated: the
seams pass each other by 1e-4 to 0.2 mm (0.126 mm for three straight
segments as cubic B-splines, which as lines give exact mitres), no vertex
is found, the face keeps its end and the corner comes out invalid. Each
seam now ends where it crosses the other side's faces when the two have
no common vertex: of the vertices the intersection put on it, the one
nearest to the bisector plane. Each side's loop round the section then
starts at its own seam's vertex, on section edges both sides share.

`CheckAndOrientEdges`, walking the section edges from vertex to vertex,
decided which end of an edge's curve on the face lies at the seam by
comparing points on the face within the resolution of the vertex's
tolerance there. The curves of approximated section edges on swept faces
end up to 1.2e-6 off in the face's parameters (4.7e-6 mm on a circle of
4 mm, where the vertex's tolerance was 1e-5 mm), so no loop was accepted.
The ends are now compared in 3D within twice the vertex's or the edge's
tolerance, whichever is wider, and on the face only for which of the two
points they are nearer (the two curves of a closed face's seam are one
line in 3D).

## 1662-brepoffsetapi-pipe-shell-mitred-corners-from-a-ten-thousandth-radian

`BRepOffsetAPI_MakePipeShell::SetTransitionMode` passed
`BRepFill_PipeShell::SetTransition`'s default angles. Under its `Angmin`,
`BRepFill_LocationLaw::TransformInCompatibleLaw` joins two edges' laws by
turning the second's whole frame onto the first's end frame, and the
sweep builds no corner there: through a kink of up to 1e-2 rad (0.57
degrees) the section kept the first edge's frame and leaned by the kink
all along the second edge. For mitred corners the angle is now 1e-4 rad
(0.0057 degrees). The pieces of an intersection curve of chains joined
tangentially meet at some 2e-6 rad, from the approximation of the
crossing, and stay joined as before.

Rounded corners keep 1e-2 rad. The turn of a section about a slight kink
is a sliver that does not close: an IFC file's closed stirrup of lines and
arcs, whose arcs meet two of its lines at 0.12 degrees (their plane turned
by that), came out invalid where MitCad's pipe fell back to rounded
corners (its mitre between an arc's torus and the closing line's cylinder
does not close). The first version of this patch changed the default of
`SetTransition` itself, for both.

## 1663-brepfill-trim-shell-corner-mitre-along-the-bisector-plane-where-the-sides-intersection-breaks-up

Two sides of one section touch where their normals coincide, at the two
points of the corner's sides, and at a small corner they meet at a shallow
angle all round. There the intersection of the approximated sides comes in
pieces with gaps of 1e-4 to 0.1 mm and small loops of its own, and the
trim along it finds no loop round the section (the sine as ten lines, and
kinks of 3, 0.3 and 0.05 degrees between two B-spline pieces).

When the trim along the intersection fails for a mitred corner of a
closed section, `PerformByBisectorPlane` cuts each side where it crosses
the bisector plane, which both cross well apart from each other. Where the
pieces are straight near the corner the two cuts are one (the sides are
then the prisms of their end sections, 1664); elsewhere they lie within
the bend over the mitre's depth of each other. The loop round the section
is broken where the U-edges of either side cross the plane (one vertex
where both sides' meet: their seams can be turned against each other, by
2.9 degrees at the 3 degree kink); each edge between two breaks lies on
one face of each side, runs through the middles of the two cuts, and has
each side's cut as its curve on that face. The U-edges end at the breaks
and each face's bound is replaced by its edges, as the trim along the
intersection makes them.

The mitre is refused where the two cuts lie further apart than 5e-4 of the
section's extent. That keeps the corners of about 60 degrees between the
curved pieces of the sine as four lines (cuts 0.011 mm apart on an 8 mm
circle, 9e-4 of its extent) to the trim along the intersection of the
section in two halves, which MitCad's pipes try next and which makes them
exactly (1661); at the corners of the tests mitred this way the cuts lay
at most 3.2e-3 mm apart (2.9e-4 of the extent).

With 1664, the halved section's trim met two parallel section edges in
`ChooseSection`, whose `BRepExtrema_ExtCC` has a distance but no points:
asking for them crashed. Parallel edges are now taken as edges without an
extremum.

## 1664-brepfill-sweep-sides-extended-past-a-corner-as-the-prisms-of-their-end-sections

For a mitred corner `BRepFill_Sweep::BuildShell` extends each side past the
corner by `EvalExtrapol`'s length with `GeomLib::ExtendSurfByLength` at C1:
from the approximated surface's derivatives at its end. Along a piece
whose frame turns about it those lean: the 0.22 mm extensions of the two
sides of the 3 degree kink passed each other by up to 0.026 mm at the
bisector plane, too far for 1663's mitre. A B-spline side is now extended
straight along the normal of its section at the law's end: rows of poles
of the end row translated along it, with the end row's weights, so that
the extension is the prism of the end section, joined at C0. Other
surfaces (the analytic ones of lines and circles) are extended as before.
The cuts of that corner then lie 2.7e-5 mm apart.

## 1665-bopds-paves-of-an-edge-whose-blocks-do-not-chain

`BOPDS_DS::Paves` collects the distinct paves of an edge's pave blocks in
an array of the blocks' number plus one, as a chain of n blocks has n + 1
paves, and only asserts that in debug builds. Blocks that do not chain
(two ending at one vertex at parameters that differ, as on the edges of a
corner whose two sides `BRepFill_TrimShellCorner` intersects) have more,
and the loop wrote past the array's end. The stirrup above, mitred,
corrupted the heap ("corrupted size vs. prev_size"; valgrind: an invalid
write of 8 bytes just after a block of 64, in `Paves` called from
`MakeFacesSec` and `FindFromVEdge`). The array now holds every distinct
pave, counted first.

## 1666-brepfill-trim-shell-corner-round-along-the-bisector-plane-where-the-sides-intersection-breaks-up

A rounded corner (`BRepFill_Round`) does not extend the sides past the
corner: each ends square at it. `BRepFill_Sweep::PerformCorner` turns
each side's end section (its bound) about the axis through the corner
square to both tangents, from the first side's end to the second side's
start, where the bound does not reach the bisector plane (outside the
corner); inside the corner the two sides overlap and the trim cuts them
against each other along their intersection. The two sides touch where
the bounds cross the bisector plane (on the axis of the turn), and at a
small corner they meet at a shallow angle all along the overlap, so their
intersection breaks up as a mitre's does (1663). The trim then failed,
and `PerformCorner`, which only stops a mitre there, turned the whole
bounds about the corner over the untrimmed sides: a shell through itself.

When the trim along the intersection fails for a rounded corner of a
closed section, `PerformRoundByBisectorPlane` cuts each side where it
crosses the bisector plane, only where its bound lies past the plane, at
the first crossing from the bound along the path. Where the pieces are
straight near the corner the two sides are mirror images in the plane
(the second side's end section is the first's turned about the axis,
which the reflection in the plane makes too), and their intersection
lies in it; elsewhere the two cuts lie within the bend over the overlap's
depth of each other. The bounds cross the plane at the section's points
on the axis, the same on both sides: one vertex of both (in the middle of
the two). The loop round the section is broken there and where the
U-edges that reach past the plane cross it (as 1663 breaks it); each edge
between two breaks past the plane lies on one face of each side, runs
through the middles of the two cuts and has each side's cut as its curve
on that face. Each bound is kept where it does not reach the plane, as
pieces with the bound's curve between the breaks and its own ends. The
round in `PerformCorner` turns those pieces, as it turns the pieces of the
trim along the intersection (`KeepEdge` finds them by the bound's curve):
their ends at the breaks lie on the axis, and its turns close there in
degenerate edges.

A mitre's sides are straight near the bisector plane (1664's prisms); a
round's overlap lies on the curved sides themselves, up to r tan(a / 2)
deep (1 mm for the 28 degree corners of the sine as 10 lines), where the
sides' intersection lies off the plane by their bend over that depth.
Each point of the cut and each U-edge's break is moved from the plane to
where the first side's line there (a U-edge's own side's) meets the other
side, by Newton's method from the two cuts' points; where that does not
converge (near the bounds' crossings, where the sides meet nearly
tangentially and the two cuts nearly meet anyway) it stays in the middle
of the two cuts.

The round itself (`BRepFill_Sweep`'s `Filling`) gives the second side's
end its curve on the turn by the first side's end turned, its parameters
mapped linearly; the two lie apart by the approximation of the sides and
of where their pieces end, and the edge kept the turn's 1e-7 mm
tolerance: likely the edge the checker refused in the halved section's round of the
sine as 10 lines (its volume within 0.00004 % of the round's, one edge's
curve on a face refused). The edge's tolerance now covers how far its
curve lies from the turn's up to twice the sweep's tolerance (each side's
end is approximated within it); further off the piece is not the turned
end (8 mm, a diameter, off at the whole circle's 26 and 28 degree corners,
where the trim along the intersection kept the other half of one side's
end) and no turn is made, so that the try fails instead of building a
solid through itself. A row's turn reuses the edge the previous row's
turn made at their common end with that row's angle, so that the two
meet there (each row's angle is measured at its own farthest point, and
they differ by the sides' approximation). `Oriente`, which orients a
turn's edges by the bound of its surface they lie at, took a degenerated
edge at the turn's first end for one at its last where the turn's surface
is trimmed on a periodic profile (the halved section's second half, whose
parameters run past 2 pi: the trimmed surface's bounds move into the
first period, the curves on it keep the profile's parameters) and
reversed it, so the turn's wire did not close on its face (the halved
section's one refused face). Where an iso-line lies at neither bound, on
a surface whose basis is periodic that way, it is now at the bound it is
nearer to over the period (a `Geom_RectangularTrimmedSurface` trimmed in
a direction is not itself periodic that way, which the first version of
this change asked, so that it changed nothing). A turn of a point next to the axis between
two vertices apart (where the trim along the intersection leaves the
sides' ends so) that cannot be made an edge, its arc no longer than the
distance between them, is a degenerated edge as for a point on the axis:
`BRepLib_MakeEdge`'s `StdFail_NotDone` likely stopped the sweep of the whole
circle at the sine's 26 degree corner. `BuildFace`'s test of whether a
turn is planar no longer asks edges that make no wire for their wire.

With `MITCAD_TRACE_PIPE_CORNERS` set, `PerformCorner` and the trim write
each corner's angle and kind, whether the trim along the intersection was
done, where the round's cut was refused (the line of the patched file) or
how far apart its two sides lay, the pieces of the two ends each row
turns and whether each turn was built, on the standard error.

MitCad's `pipe_shell` (`geometry/src/spine.cpp`) traces its tries of the
corners under the same variable: for each, what it threw, or the
checker's first problem with what it built and its volume.

Refused, so that the corner is left as before, where the two cuts lie
further apart than 5e-4 of the section's extent (as 1663), where the
bounds do not cross the plane, where a bound crosses it within 1e-6 of a
face's width of a U-edge, and for a section that is not closed.

## Tests

`geometry/tests/test_pipe_square_section.cpp` (`geometry.pipe_square_section`)
sweeps an 8 mm circle along intersection curves of a sine wave on XZ and an
arc on XY and checks the section square to the path within 0.01 degrees
at 20 points and the volume, the circle's area times the path's length,
within 0.1 %.

Measured (the worst of 20 cuts; before: the section carried through the
corner):

| Path (8 mm circle) | Before | With 1661 and 1662 | With 1661 to 1664 |
|---|---|---|---|
| sine as 4 lines × bent arc | 58.8°, −23.9 % | 0.000005°, −0.00001 % | the same |
| sine as 10 lines × bent arc | 78.7°, −32.6 % | unchanged | 0.000001°, −0.0004 % |
| kink of 3°, nearly planar arc | 3.0°, −0.07 % | unchanged | 0.0002°, +0.00001 % |
| kink of 1° (both arcs) | square (rounded) | square | square |
| kink of 0.3°, bent arc | 0.3° | unchanged | 0.00009°, +0.00001 % |
| kink of 0.05°, nearly planar arc | 0.05° | unchanged | 0.00013°, +0.00001 % |

The test checks all of these. The existing pipe, sweep, thread and helix
tests (`geometry.suite.sweeps.*`, `geometry.suite.thread.*`,
`geometry.pipe_corners`, `geometry.helical_joins`) pass unchanged.

The stirrup (`test_stirrup`, the IFC file's 12 mm bar) is a valid solid
within 0.2 % of the circle's area times its length: its two corners at the
closing line come out rounded (the trim along the intersection of the
arc's torus and the line's cylinder does not close, their cuts by the
bisector plane lie 0.31 mm apart), 0.11 % short, as before 1662.

A mitre between pieces curved near the corner is cut where the sides cross
the bisector plane only where their intersection breaks up; its edges lie
up to half the cuts' distance (at most 2.5e-4 of the section's extent)
off each side.

Rounded corners (`BRepBuilderAPI_RoundCorner`, 1666): the same test
sweeps the circle with rounded corners along the sine as 10 lines × the
bent arc and along the kink of 3 degrees × the nearly planar arc. It
checks the faces that turn the section about the corners (their cuts one
way all centred on a corner): at least one at each corner of the path
from 1e-2 rad on, spanning half the circle at each together (the outside
of the corner; a turn of the whole bound spans all of it); the section
square to the path within 0.01 degrees at 20 points elsewhere; and the
volume within 0.1 % of the circle's area times the path's length plus,
for each corner turning by a, (a - 2 tan(a / 2)) 2 r^3 / 3 (the two
overlapping wedges of the legs inside it less, the turn of the outer half
circle more; geometry.pipe_corners has it for a quarter turn). That
correction is 0.03 % for the ten lines' corners: the volume alone does
not tell a round from a mitre, the turning faces do. The patch was
written without a build; the figures of its first run belong here.
