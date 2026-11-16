# Fillet results that were invalid or failed: 0150–0153, 0200, 0230–0241, 0270, 0271, 0300

Roundings whose result OCCT's topological build made invalid, or whose
computation failed (mitcad#239 and its later rounds). The cases are rebuilt
with OCCT alone in `geometry/tests/test_fillet_results.cpp`. Back to the
[port's README](../README.md).

## Overview

- **0150 to 0159** make the fillet build roundings whose results were
  invalid or failed (mitcad#239).
- **0200** lets a walked blend turn about a contact that stands still
  (mitcad#239).
- **0230** keeps a blend's open contact in the adjacent face's parametric
  period.
- **0240 and 0241** keep the seam of a blend's extension out of the ball's
  way and let two roundings meet across a notch in the rim they round
  (mitcad#239).
- **0270** lets two roundings meet on one contact line of a face that goes
  on past the line's ends, and **0271** cuts two roundings by each other
  where their contacts cross part of the way (mitcad#239).
- **0300** gives the pieces of an edge prolonged at a rounding's end across
  a seam their p-curves on the right face (mitcad#239).

## 0150 to 0153: invalid or failed results

The patches 0150 to 0159 make OCCT's fillet build roundings whose result
the topological build made invalid, or whose computation failed
(mitcad#239); `geometry/tests/test_fillet_results.cpp` rebuilds the cases
with OCCT alone, each test failing without its patch.

### 0150-chfi3d-onsame-corner-beside-a-split-face

The corner where a rounding ends against a face (OnSame) when the opposite
face is split, at the corner's vertex, into two faces of one surface along
an edge that lies in the face at the end (a slot cut into the side of a bar
whose side is two faces split at the slot's back wall). The missing end of
the prolonged edge went on the opposite face, which does not hold that edge,
and the split edge stayed whole; the build kept its pieces under the
rounding (an unorientable shell). Where the opposite contact ends on such an
edge, the missing end is computed on the other face of the prolonged edge,
and the split edge is cut where the contact ends.

### 0151-chfi3d-corner-on-an-end-face-split-in-one-plane

The corner where a rounding ends (OnDiff) on a face at the end that is two
faces of one plane, each holding one of the arcs the ends lie on (a pad on a
wall with a foot face of its own; the concave edge at the pad's bevelled end
rounded wider than the bevel, the ball rolling on the pad's edge). No face
held both arcs, and the intersection at the end built a curve from the wrong
edge's geometry. Where exactly one arc holds the corner's vertex and both
faces are planes of one plane facing the same way, the cap goes on the face
of the other arc and the part of the first arc between its point and the
vertex becomes the edge between the two faces
(`ChFi3d_CoplanarSplitEndFace`); a point at a vertex is first given the edge
of that vertex on the coplanar face.

### 0152-chfi3d-closed-rolling-start-on-a-seam-vertex

A closed rounding rolling on an edge (0011) from that edge's vertex, where a
face's seam starts too (a disc's bottom edge rounded wider than the disc's
side and bevel, the ball rolling on the bevel's top edge; disc, bevel and
neck made of separate solids). The first point was found on the seam, and
`ChFi3d_FilDS` cut the seam there; the build lost the faces above. A point
at a vertex on a side rolling on an edge no longer cuts its arc at the first
surface data, as at the last one already.

### 0153-chfi3d-roundings-meeting-on-one-contact-line

Two roundings whose contact lines on the face between their edges are one
line, the face exactly as wide as the two together (a plate 1.5 thick, both
top edges rounded with 0.75). 0013 and 0016 handle contacts that overlap;
coinciding ones failed in the corner (the two caps tangent at their common
end point, taken as crossing) and in `ChFi3d_StripeEdgeInter`. Such a pair
on a strip face between their edges is recorded as 0016's ride (the face
standing for the support), so that the blends meet on the line and the face
vanishes; caps tangent at their own end are not taken as crossing.
Not where another rounding reaches a vertex of the two edges (mitcad#1883,
in 0270's part of the code): a box 100 wide whose top's two edges and two
vertical edges at their ends are rounded with 50 (OCCT's bug119) came out
invalid, its corners built round the top that vanishes. Without the ride
the corners cut the roundings by each other: valid, 809366.2 against the
two roundings' 809366.23 computed apart (`test_fillet_occt_cases.cpp`).

## 0200: a walked blend about a standing contact

The patch 0200 lets a walked blend between two faces turn about a contact
that stands still (mitcad#239).

### 0200-chfi3d-walked-blend-about-a-standing-contact

Where a rounding runs past a corner rounded with the ball's own radius (a
box's vertical edge rounded with r, then its top edges with r; a pocket's
floor whose inner corners are rounded with r), the ball's centre stays on
the corner's axis: its contact on the top (the floor) stands at one point
while the other sweeps round the corner, and the blend there is a piece of
the ball's sphere whose side on the top is a point. On elementary faces OCCT
builds that sphere analytically; on other faces (stored splines) the walk
does it, and there the line got no transition on the standing side (its
contact has no direction: `Standard_DomainError` from
`BRepBlend_Line::TransitionOnS1/S2` in `StoreData` or `ComputeData`), the
standing side's twist flag was the sign of numerical noise (the piece was
then replaced by a filling surface), and `StoreData` took the blend's
orientation from its normal at the point side, which is null, and failed. A
contact whose tangent is below a thousandth of the other's now stands still:
it is not twisted, and its transition is taken with the guide's tangent for
its direction (`BRepBlend_Walking::TestArret`); where the blend's normal at
its side 1 is null, the orientation is taken a hundredth of the section
inside. The point side is stored as 0171 stores a ball turning about the end
of an edge: a degenerated edge of the blend's face.

## 0230: a contact in the adjacent face's period

### 0230-chfi3d-contact-in-the-adjacent-face-period

The patch 0230 keeps a blend's open contact in the adjacent face's
parametric period. Projecting the contact onto a cylinder can place the
whole arc one period beyond the face bounds: its 3D curve is correct, but
the wire does not close in 2D. The existing period adjustment now applies
to every newly projected contact, before the closed-contact seam handling.
The lug-in-bore chamfer and fillet cases in
`geometry/tests/test_chamfer_overflow.cpp` cover this failure.

## 0240 and 0241: the third round

The patches 0240 and 0241 go on with roundings whose computation failed
(mitcad#239, third round).

### 0240-chfi3d-extension-on-the-faces-own-surface-away-from-its-seam

Past the end of a spine, where a contact leaves its face with no face
beyond, the blend rolls on the face's own surface (0076), a face made of the
whole surface within its natural bounds, whose seam is an edge without
vertices. Where the ball heads for that seam close by (a small convex arc
whose circle starts 0.08 radians before the arc, after a kink from a big
arc, both rounded: the small arc's blend is extended back past the kink for
the corner), the walk stopped on the seam (`Standard_NullObject` in
`ChFi3d_FilCommonPoint`, then `FindFromKey` in the search for the faces
around it). Where the ball, leaving the face, heads for a seam of the
natural bounds less than a quarter period away, the bounds are now taken
half a period on (`ChFi3d_BuildPlane`); other extensions keep the face they
had, since moved bounds would part the extension's parameters from the
walk's on the face before it.

### 0241-chfi3d-roundings-meeting-across-a-notch

A rim rounded on both sides of a small notch whose edges are not rounded (an
arc broken at its lowest point by a V notch narrower than the rounding,
whose contact on the top passes above the notch): the edge chain stops on
both sides, one stripe ends at each side or both ends of one stripe meet
there. The blends are one torus, over the notch the ball rolls on and the
notch's faces cut the blend, and the files' bodies have one torus face
there. OCCT ran each end's contact on the top on to the end of its domain on
that face (SplitKPart's extension for the corner, here round the whole other
half of the outline), the corner did not cut it back, and the two contacts
overlapped (`ChFi3d_StripeEdgeInter`) or the wires intersected themselves.
`ChFi3d_JoinAcrossNotches`, after the stripes and before the corners, finds
such pairs of ends (each the only end at its vertex, analytic blends of one
surface, contacts on one line of a common face that stays inside it over the
gap, the other contacts on arcs of two faces of one surface), cuts the
contacts on the common face back to the ends' sections, runs one end's blend
on over the gap cut by the notch's faces in turn (the pieces of
`ChFi3d_KPartChainPieces`, which now takes given ends where the piece before
or after is missing), and makes the two ends free ends that share the
meeting section's points. The blends are one face where their surfaces are
one (the joined end's pieces take the other's surface, their p-curves mapped
where the tori are parametrized otherwise); the two ends of one stripe share
the section's edge (one face all round would have no seam). In
`ChFi3d_KPartCutPiece` an edge of the face beyond with the other contact's
face, from a vertex where it meets the contact's face too, no longer counts
as an edge ending under the blend: it lies between the cut and the other
contact and goes with both faces.

## 0270 and 0271: roundings meeting on one contact line

The patches 0270 and 0271 go on with 0153 (mitcad#239).

### 0270-chfi3d-roundings-meeting-on-one-contact-line-of-a-face-going-on

Two roundings whose contact lines on the face between their edges are one
line, where that face goes on past the line's ends (a rib as wide as the two
roundings standing out of a plate or a ring, its top one face with theirs; a
slot as wide in a wall, its floor one face with the floor in front of the
wall). 0153 recorded the pair only where the face is a strip that vanishes
whole, so here `ChFi3d_StripeEdgeInter` failed. The pair is now recorded
where neither stripe is closed; `ApplyRides` makes the blends meet on the
line when each end of it is on one arc of the face for both (as 0153) or a
point inside the face where both corners end, having cut the face off there
(the edge of the face at the end prolonged across the root). At such a point
each corner made its own point: the later stripe's is made the earlier one's
throughout the data structure, so that the face's wire closes there. The
face keeps its part past the line's ends. Neither pair is recorded where
another stripe reaches a vertex of the two edges (see 0153, mitcad#1883).

### 0271-chfi3d-roundings-cut-by-each-other-where-their-contacts-cross

Two roundings whose contacts on the face between their edges cross each
other twice, the face narrowing to less than their two widths part of the
way (a block whose long sides are concave arcs, the top 5.8 wide in the
middle, both top edges rounded with 4; a channel whose walls bulge in, its
floor edges rounded). The files' bodies cut the two blends by each other
between the crossings and keep the face in two pieces; OCCT failed in
`ChFi3d_StripeEdgeInter`, or, where 0013 took the contacts for overlapping
all along, in the stripes computed again. After the corners,
`trimCrossingStripes` finds the blends' intersection between the crossings
(one blend's sections cut by the other's surface, interpolated; the blends
are tangent to the face at the crossings, a node of the surfaces'
intersection), splits each SurfData in three on its surface, and gives the
middle piece the intersection on no face, shared by both stripes;
`meetOnCurve` makes the two stripes' points at the crossings one once both
are in the data structure. A pair that 0013 computed again is computed on
the faces of its edges once more when its contacts as first computed crossed
twice and it failed or still crosses. Open stripes with no spine end in
common only; other pairs are left as they were.

## 0300: a prolonged end edge across a seam

The patch 0300 goes on with 0150 to 0153 (mitcad#239).

### 0300-chfi3d-prolonged-end-edge-across-the-opposite-faces-seam

A concave rounding of a rib's foot on a rod, its end cut by the rib's end
face: the end face grows down to the rounding's contact on the rod, its edge
with the rod prolonged from the vertex to the contact (`PerformOneCorner`).
Where the rod's seam runs between the foot and the contact, the prolonged
edge is cut at the seam, and the two pieces' p-curves on the end face were
computed on the rod instead (edges of several mm tolerance, an unorientable
end face). They now take the intersection's own p-curve on the end face,
trimmed at the seam.

## Reports and tests

- 0150 to 0159: mitcad#239 and `geometry/tests/test_fillet_results.cpp`.
- 0200: mitcad#239 and the standing-contact tests in
  `geometry/tests/test_fillet_results.cpp`, which fail without it.
- 0230: the lug-in-bore chamfer and fillet cases in
  `geometry/tests/test_chamfer_overflow.cpp`.
- 0240 and 0241: mitcad#239 and the seam and notch tests in
  `geometry/tests/test_fillet_results.cpp`, which fail without them.
- 0270 and 0271: mitcad#239 and the ribs and the slot rounded to a full
  round and the narrowing top and channel in
  `geometry/tests/test_fillet_results.cpp`, which fail without them.
- 0300: mitcad#239 and the rib's foot rounded across the rod's seam in
  `geometry/tests/test_fillet_results.cpp`, which fails without it.
