# vcpkg overlay ports

`vcpkg.json` (`vcpkg-configuration.overlay-ports`) makes vcpkg build these
ports instead of its own, on every platform and for every use of the
manifest (the CMake presets, `tools/dev-env/build-cycles.sh`). vcpkg builds
an overlay port from source once per machine and triplet and keeps the
result in its binary cache; a change to the port rebuilds it.

## opencascade

vcpkg's `opencascade` port for OCCT 8.0.1 as of the baseline in
`vcpkg.json`, with patches of Mitcad's.

| Files | Origin | Licence |
|---|---|---|
| `portfile.cmake`, `vcpkg.json`, patches 0001 to 0005 | microsoft/vcpkg, `ports/opencascade` at the baseline; unchanged but for `port-version`, the list of patches and the SPDX line | MIT |
| patches 0006 and later | Mitcad | LGPL-2.1-only WITH OCCT-exception-1.0, as OCCT |

The patches change OCCT, so they are under OCCT's licence (the GNU LGPL
version 2.1 with the Open CASCADE exception), published here with Mitcad's
source, and OCCT stays a set of shared libraries (vcpkg's dynamic
triplets), as the [licence policy](../../docs/development.md#licence-policy)
requires; the third-party notices of the packages name them. Each patch
says at its top what it changes and why. Patches 0006 to 0009 change no
result: each removes repeated work that made an algorithm quadratic in the
edges or wires of a face, which bodies with thousands of holes reach
(mitcad#39, mitcad#72, mitcad#77, mitcad#79). Patches 0010 to 0019 make
OCCT's rolling ball fillet build roundings it failed on (mitcad#121), and
0020 to 0026 fix other failures and crashes of it (mitcad#122,
mitcad#133). Patches 0030 to 0032 make an allocation that fails throw
instead of crashing, let a caller learn of it and leave the collections
valid (mitcad#132).

- `0006-shapeupgrade-unifysamedomain-seam-search-by-vertex.patch`: the face
  merge after booleans (`ShapeUpgrade_UnifySameDomain`) looks up the edges
  it removed by vertex instead of going through all of them at each step of
  a wire across a seam.
- `0007-brepcheck-analyzer-no-search-in-face.patch`: the checker
  (`BRepCheck_Analyzer`) no longer searches a face for each of its own
  vertices and edges (`InContextOfAncestor`).
- `0008-brepcheck-wire-seam-edges-once-per-face.patch`: the checker finds
  the seam edges of a face once instead of once per wire for the 2d closure
  check, and no longer searches the face for each of its wires
  (`BRepCheck_Wire::InContextOfFace`).
- `0009-bopalgo-face-face-adaptors-once-per-face.patch`: the face/face
  intersections of a boolean build the surface adaptor of each face (its UV
  bounds go through all its edges) once instead of once per pair of faces
  (`IntTools_Context::SetSurfaceAdaptor`).
- `0010-brepblend-surfrst-line-on-both-restrictions.patch`: a blend
  rolling on an edge (`BRepBlend_SurfRstLineBuilder`) whose contacts leave
  the face and reach the end of the edge in the same step took the
  parameter of the wrong arc as its point on the edge and stopped short of
  the end; it now ends on both.
- `0011-chfi3d-fillet-edge-overflow.patch`: edge overflow of the rolling
  ball fillet. Where the ball cannot touch a face of the edge within that
  face (a face narrower than the rounding), it touches the face beyond the
  face's far edge, or, where that face does not let it, it rolls on the far
  edge (a blend on an obstacle edge, which OCCT had only where a walk met
  an edge in the middle of a rounding). OCCT found no start where the
  contact leaves a face along the whole edge; now the start can be on the
  face beyond or on the edge (`StartSolOverflow`). A blend on an edge then
  ends where the spine or the face at its end does (`ObstacleAtSpineEnd`,
  `ChFi3d_ObstacleEndOnFace`, `ChFi3d_ExtendAtSpineEnds`) or goes round a
  closed spine (a periodic guide in the edge/face `ComputeData`), and the
  narrow face it consumes vanishes (`ChFi3d_CutConsumedFaces`). Where the
  contact leaves a face near the end of the spine, OCCT cuts the rounding
  with the face beyond, as before. Fillets only; chamfers are unchanged.
- `0012-chfi3d-closed-stripe-end-on-vertex.patch`: a closed rounding
  whose start lies on a vertex but whose end was not identified with it
  gave the builder a vertex index as a point index (an exception).
- `0013-chfi3d-overflow-across-faces-and-overlapping-roundings.patch`:
  0011's overflow across several narrow faces and on both sides of the
  edge. Each side's support moves on across the edge of its face that the
  section crosses while the contact lies beyond it, also where the ball
  touches a face's surface at no point (a sliver: the face beyond the
  nearest edge, within the ball's diameter); a side rolls on the last edge
  before a face it cannot touch (not both sides: that ball touches no
  face); a start touching the faces it reached must clear the faces it
  passed and those around them. The consumed faces between the spine and
  an edge the blend rolls on can be a chain of strips
  (`ChFi3d_CutConsumedFaces`).
  Two stripes whose contacts on a face between their edges each lie beyond
  the other's are computed again overflowing that face across each
  other's edge (`OverlappingStripes`, `OverlapEdge`); a stripe whose blend
  then lies on another's stays a contour of the builder but gives no faces
  (`RemoveCoincidentStripes`). The builder class gets members for these,
  so code built against the port's headers must be rebuilt with them.
- `0014-chfi3d-contact-along-face-edge.patch`: a fillet or chamfer whose
  contact runs along the far edge of a face (a face exactly as wide as
  the contact distance). The split of an analytic blend into the domains
  of its faces found no domain for a contact line lying on the face's
  boundary, so the blend went to the walking path and failed; such a line
  now takes the domains of the same line moved a little into the face,
  its ends put at the vertices they meet (`SplitKPart`). A blend whose
  contact covers a whole edge of its face is then stored as a blend on
  that edge, as 0011's blends rolling on an edge are, so that the face it
  consumes is dropped instead of being trimmed to nothing
  (`ChFi3d_ContactsAlongFaceEdges`; a blend made 0.1 % smaller than the
  face stays inside it). Walked blends along a face's boundary still fail.
- `0015-brepblend-surfrst-smaller-step-on-failed-reframing.patch`: a
  blend rolling on an edge (`BRepBlend_SurfRstLineBuilder`) stopped at the
  first section it could not reframe on an arc; a step too large lands on
  the other branch of the solution, outside the face. The step is now
  halved first, as the walking between two faces does.
- `0016-chfi3d-rounding-rolls-on-anothers-contact-line.patch`: two
  roundings of a face narrower than both, where no ball touches the faces
  beyond it on both sides (the rims of a plate thinner than the
  roundings' diameter), so that 0013 cannot make them one: the earlier
  rounding is computed on its own faces, the later one rolls on the
  earlier one's contact line on that face (`RideOnOverlap`), the two
  blends meet on that line (`ApplyRides`, after the corners; the caps of
  open roundings meet where the line ends) and the face vanishes. Both
  roundings convex or both concave only. The builder class gets members
  for this (code built against the port's headers must be rebuilt).

The patches 0020 to 0026 fix OCCT's fillet (`TKFillet`, `ChFi3d`) where
it crashed, threw or built a wrong shape on fillets of the `.f3d` corpora
(mitcad#122, and from 0024 mitcad#133); each one changes the results only
in the cases it describes, and `geometry/tests/test_fillet_kernel.cpp`
(`test_fillet_ends.cpp` from 0024) rebuilds those it could on synthetic
shapes with OCCT alone.

- `0020-chfi3d-crash-fixes.patch`: three causes of segmentation faults. A
  stripe continued past an obstacle face that does not hold the edge the
  contact left by kept an empty restriction curve (`StartSol`); the seam
  of the face at a corner was intersected with a parallel curve, whose
  points `Extrema_ExtCC` does not give (`PerformOneCorner`,
  `IntersectMoreCorner`); `PerformMoreSurfdata` read the first stripe of
  the builder instead of the stripe at the vertex.
- `0021-chfi3d-corner-of-contour-closed-at-sharp-vertex.patch`: a contour
  of tangent edges that closes on itself at a sharp corner has both ends
  at that vertex; with more than three sharp edges there
  (`PerformMoreThreeCorner`) only its first end was found, and the
  corner threw.
- `0022-chfi3d-round-edges-asked-for-near-tangent.patch`: an edge between
  faces up to 0.1 rad apart counted as tangent and could not be rounded;
  it is refused now only when its faces come within the builder's angular
  tolerance.
- `0023-chfi3d-meeting-point-of-periodic-contact-lines.patch`: where a
  fillet along an arc meets one along a line at a shallow corner, the
  meeting point of their contact lines on the common face was taken on
  the arc's extension, far from the vertex, and the corner face was
  self-intersecting.
- `0024-chfi3d-order-of-periodic-kpart-surfaces.patch`: the analytic
  pieces of a rounding along a whole circle, cut where its contact line
  leaves a face (`SplitKPart`), are sorted along the spine
  (`PerformSetOfKPart`); the sort kept a stale piece, so one piece came out
  repeated and others lost, and the walks between the pieces threw
  (`ChFiDS_CommonPoint::Vector`) (mitcad#133).
- `0025-chfi3d-end-cut-by-faces-around-a-cylinder-corner.patch`: a
  rounding whose end is cut by several faces around a cylinder's corner
  patch (`PerformIntersectionAtEnd`): the crossing with each common edge
  is taken on that edge (two arcs of one circle gave the same one), a
  crossing beyond the edge's end prolongs the faces' common boundary along
  the edge's curve and cuts the corner patch's edge there (not where the
  rounding passes beside a face between the end's first and last faces,
  such as a narrow chamfer at a step's corner), a seam on a face
  at the end only no longer counts as an edge of the vertex, and the end's
  intersection with that face is taken across the seam, split there, and
  the seam cut (mitcad#133).
- `0026-chfi3d-onsame-end-cut-by-several-faces.patch`: the end of a
  rounding whose face runs on tangentially past the vertex (the OnSame
  state) with its contact off any edge, on either side of the rounding,
  cut by several faces (`PerformIntersectionAtEnd` gave up): the face's
  edge at the vertex is
  prolonged to the contact by a piece of its curve on both its faces, the
  spine's edge is cut at the vertex, and the ends on a periodic face are
  taken into its domain (mitcad#133).

The patches 0030 and later make OCCT handle an allocation that fails
(mitcad#132).

- `0030-standard-failed-allocation-throws.patch`:
  `Standard::AllocateOptimal`, under the collections' allocators and the
  strings, returned null when an allocation failed with the native (the
  default), TBB and jemalloc memory managers, and its callers wrote to
  the block at once: under a memory limit the shape checker crashed in
  `CSLib_Class2d`. It throws `Standard_OutOfMemory` now, as
  `Standard::Allocate` and the flexible memory manager do;
  `Standard_Failure`'s message, the one caller that checked for null,
  goes without it as before. `Standard_OutOfMemory::NbRaised` counts the
  exceptions made, so that a caller learns of a failure that the checker
  or the healing caught inside and went on from.
- `0031-osd-threadpool-failed-allocations-to-launching-thread.patch`:
  `Standard_OutOfMemory::NbRaisedInThread` counts the failures of the
  calling thread, and `OSD_ThreadPool::Launcher` hands those of the jobs
  its threads ran over to the thread that launched them (the checker and
  the booleans run on the pool), so a caller knows whether memory ran out
  under its own call rather than under another thread's at the same time.
- `0032-ncollection-containers-intact-when-allocation-fails.patch`: the
  collections stay valid when an allocation in them throws.
  `NCollection_Array1::Resize` and `NCollection_LocalArray` freed their
  block before allocating the new one and kept pointing to it (freed again
  by their destructors); `NCollection_IndexedMap` and
  `NCollection_IndexedDataMap::ReSize` moved their nodes to the new
  buckets before reallocating their index array, and when that threw
  their destructors freed nodes twice (a boolean ended the process so
  under a memory limit). `NCollection_BaseMap::BeginResize` leaked its
  first bucket array when the second failed.

Each patch has a report for OCCT, with a reproduction that uses OCCT alone
and measurements: mitcad#84 (0006), mitcad#90 (0007), mitcad#91 (0008) and
mitcad#92 (0009); for 0010 to 0019, mitcad#121 and the tests in
`geometry/tests/test_fillet_overflow.cpp`, which use OCCT's fillet alone
where they can; for 0020 to 0023, mitcad#122 and
`geometry/tests/test_fillet_kernel.cpp`; for 0024 to 0026, mitcad#133
and `geometry/tests/test_fillet_ends.cpp`; for 0030 to 0032, mitcad#132
and `geometry/tests/test_allocation.cpp`. Sending them upstream is the
maintainers' step. When the
baseline moves to a newer OCCT, check which of the changes OCCT has made;
update the port from vcpkg's and apply the remaining patches again
(`git apply` in OCCT's source tree, in order), and compare the corpus
results as for any change to the geometry. Building OCCT from the port is
described in [development.md](../../docs/development.md#faster-builds).
