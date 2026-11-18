# A contact's end at a vertex within that vertex's own tolerance: 1730

mitcad#1926, GitHub #371's Chamfer3: the 0.5 chamfer of a pit's bottom
loop failed on the body the geometry kernel's Fillet1 made, and built on
the body the file stored. Back to the [port's README](../README.md).

## The case

The pit's bottom loop is ten tangent edges at z = -550: a straight edge
along the pit's 45 degree face, the bottom's ellipses with the two rounds
of 1 between that face and the pit's walls, the walls' short straight
edges, and the drafted curved part. Fillet1 rounds the 45 degree face's
edges with the walls with 1; above the bottom those rounds end in a corner
patch with the wall's rounding of 0.8 along the plate and the plate's
rounding of 5 along the 45 degree face. In the kernel's result the patch's
vertices have a tolerance of about 0.54 (the patch lies up to 0.25 off its
neighbours; the stored body's corner is tight).

The chamfer's piece along the 45 degree face is analytic (plane and
plane, ChFiKPart). Its contact on that face ends at 0.5 from the bottom's
vertex where the face, the round of 1 and the bottom meet, on the straight
edge between the face and the round, whose other vertex is a vertex of the
corner patch.

## The fault

Patch [0692](../opencascade/0692-chfi3d-kpart-contact-end-within-a-vertex-tolerance-at-the-vertex.patch)
puts an analytic contact's end found on an arc at a vertex of that arc
when it lies within the vertex's tolerance (a contact running exactly
through the vertex where two edges of the face meet). It took the larger
tolerance of the arc's two vertices and compared every vertex with it: the
patch's vertex (0.54) drew the contact to the bottom's vertex, 0.5 away.
The next piece then started at a vertex, SearchFace took the round of 1
beyond it for an obstacle (no tangent face along the spine at that
vertex), and the walk asked for a chamfer rolling on an edge, which
`ChFi3d_ChBuilder` does not have ("PerformSurf Not Implemented"): the
stripe failed (status 1) at the bottom's vertex at the wall of x -275.43.

## 1730-chfi3d-kpart-contact-end-at-a-vertex-only-within-its-own-tolerance

- `FillSD`: `SnapToVertex` is called with `OwnTolerance`, so that each of
  the arc's vertices is compared with its own tolerance. The snap of a
  contact hatched on its face's boundary (`HatchOnBoundary`) keeps the
  tolerance it is given.

Result: Chamfer3 builds on the kernel's Fillet1 result, valid, its twelve
pieces as on the stored body. It takes 4.36 mm3 away there against 4.62 on
the stored body; the difference is Fillet1's (the corner patches beside
the pit, see [1728](fillet-ride-on-stripe-at-end-1728.md)).

Left: the corner patch's tolerance of 0.54 itself (the wall's rounding of
0.8 meeting the rounds of 1 and 5), which is Fillet1's deviation from the
stored body at the 0.8 arm.
