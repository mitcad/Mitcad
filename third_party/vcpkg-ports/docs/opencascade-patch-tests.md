<!-- SPDX-License-Identifier: MIT -->
# The port patches' tests

The regression test of each OCCT port patch and the before/after proof of
it: one section per patch, headed by its number, holding the patch's
`Test:` paragraph. The patch's header describes the change itself (case,
cause, condition, origin) and points here. vcpkg hashes every file of the
port folder, so a test note or a proof sentence kept in a patch rebuilt
OCCT on every machine; this document lies outside the port, and a change
to it changes nothing there (mitcad#2133).

`tools/occt-port-proof.py` reads each section's `Test:` paragraph: the
ctests it names (`geometry.port.0011`, or `(ctests geometry.a.b, c and d)`)
and the test functions (`test_...`) whose failure counts for the patch;
every `test_...` word in the paragraph is such a function, so a paragraph
names other patches' tests by their cases, not by their functions. A
sentence "The test needs the later patch NNNN too" (or "the later patches
NNNN and MMMM") makes the chain mode carry those patches in both states
(mitcad#2310). Each named function must run alone: a suite part registers
it with `guarded("<name>", ...)` (check.hpp), an executable of its own
selects it by name (test_only.hpp), and no other label of the part may
contain a named label.
`--update-tests` rewrites the paragraph's sentence on the proof from the
tool's last run; that run's table is
[opencascade-before-after.md](opencascade-before-after.md).

A new patch gets a section here, in number order: the heading
`## <number>`, a line linking the patch, and one paragraph starting with
`Test:` that names the test (or `Test: none, because ...`). Its header does
not repeat it; it has the paragraph pointing here that the other headers
have instead.

The paragraphs below were moved from the patch headers unchanged on
2026-11-20 (mitcad#2133), as they stood after the proof runs of mitcad#2121;
the in-chain results of mitcad#2121's later runs (53 more patches shown
failing without their patch at its place, and the `Test: none` paragraphs
of 0012 and 1664) were written into them afterwards.

## 0001

[0001-cmake-keep-build-use-vcpkg-explicit.patch](../opencascade/0001-cmake-keep-build-use-vcpkg-explicit.patch)

Test: none, because this is one of vcpkg's own build changes of the port
(patches 0001 to 0005 come from vcpkg's opencascade port, otherwise
unchanged): it changes only which third-party libraries OCCT's build takes
from vcpkg, not what OCCT computes, so no geometry test can fail without
it; the port's build is its check (mitcad#1986).

## 0002

[0002-cmake-load-exported-package-dependencies.patch](../opencascade/0002-cmake-load-exported-package-dependencies.patch)

Test: none, because this is one of vcpkg's own build changes of the port
(patches 0001 to 0005 come from vcpkg's opencascade port, otherwise
unchanged): it changes only how OCCT's build finds its third-party
packages, not what OCCT computes, so no geometry test can fail without it;
the port's build is its check (mitcad#1986).

## 0003

[0003-image-remove-freeimage-msvc-autolink.patch](../opencascade/0003-image-remove-freeimage-msvc-autolink.patch)

Test: none, because this is one of vcpkg's own build changes of the port
(patches 0001 to 0005 come from vcpkg's opencascade port, otherwise
unchanged): it changes only which library MSVC links OCCT's image code
with, not what OCCT computes, so no geometry test can fail without it; the
port's build is its check (mitcad#1986).

## 0004

[0004-cmake-add-additional-path-extraction-for-OpenCASCADE.patch](../opencascade/0004-cmake-add-additional-path-extraction-for-OpenCASCADE.patch)

Test: none, because this is one of vcpkg's own build changes of the port
(patches 0001 to 0005 come from vcpkg's opencascade port, otherwise
unchanged): it changes only where OCCT's installed CMake package finds its
install prefix, not what OCCT computes, so no geometry test can fail
without it; the port's build is its check (mitcad#1986).

## 0005

[0005-drop-bin-letter.patch](../opencascade/0005-drop-bin-letter.patch)

Test: none, because this is one of vcpkg's own build changes of the port
(patches 0001 to 0005 come from vcpkg's opencascade port, otherwise
unchanged): it changes only how OCCT's libraries are named and where their
debug files go, not what OCCT computes, so no geometry test can fail
without it; the port's build is its check (mitcad#1986).

## 0006

[0006-shapeupgrade-unifysamedomain-seam-search-by-vertex.patch](../opencascade/0006-shapeupgrade-unifysamedomain-seam-search-by-vertex.patch)

Test: none, because the patch changes only the time the merge takes: the
removed edges are looked up by vertex and tried in the same order as
before, so every result is stock OCCT's and no result can fail without it,
and its gain shows only on bodies of hundreds of pieces across a seam
(mitcad#1986).

## 0007

[0007-brepcheck-analyzer-no-search-in-face.patch](../opencascade/0007-brepcheck-analyzer-no-search-in-face.patch)

Test: geometry/tests/port/p0007-checker-no-search-in-face.cpp (ctest
geometry.port.0007), test_small_face_is_valid and
test_face_of_forty_thousand_edges_within_eight_seconds: a face of one wire
of 40 000 edges is valid and checked without geometric controls within 8 s
(tens of seconds with the search). It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 0008

[0008-brepcheck-wire-seam-edges-once-per-face.patch](../opencascade/0008-brepcheck-wire-seam-edges-once-per-face.patch)

Test: geometry/tests/port/p0008-wire-closure-many-holes.cpp (ctest
geometry.port.0008): a plate of 2 025 round holes and a cylinder's side
with 96 holes (its seam found once) are valid, a wire of the holed side's
bottom circle alone is still not closed, and a plate of 3 600 holes of 24
edges each (86 404 edges) is checked within 10 s with topological checks
only (BRepCheck_Analyzer with its geometric controls off; going through all
edges of the face for each wire is quadratic).

## 0009

[0009-bopalgo-face-face-adaptors-once-per-face.patch](../opencascade/0009-bopalgo-face-face-adaptors-once-per-face.patch)

Test: none, because the patch changes only the time the booleans take:
each pair's context gets a shallow copy of the adaptor it built for itself
before, so every intersection is stock OCCT's and no result can fail
without it; the booleans of geometry.suite.boolean run through it
(mitcad#1986).

## 0010

[0010-brepblend-surfrst-line-on-both-restrictions.patch](../opencascade/0010-brepblend-surfrst-line-on-both-restrictions.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_concave_rounding_rolls_on_a_strips_far_edge,
test_rounding_beside_a_round_corner_ends_inside_its_faces,
test_rounding_rolls_beside_a_round_corner,
test_rounding_rolls_on_a_bevel_edge_round_the_corners,
test_roundings_on_a_low_step and test_roundings_over_several_narrow_faces:
roundings whose ball rolls on an edge of a face (a restriction) beside
other faces; without the patch they fail or come out wrong. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py). (The
rib's top edge rounded wider than its top, named before, succeeds without
the patch.)

## 0011

[0011-chfi3d-fillet-edge-overflow.patch](../opencascade/0011-chfi3d-fillet-edge-overflow.patch)

Test: geometry/tests/test_fillet_overflow.cpp (the roundings over a narrow
face of mitcad#121); geometry/tests/port/p0011-edge-overflow-start.cpp
(ctest geometry.port.0011): a block with a step 2 high whose top edge and
foot are each rounded with 3 (the contact leaves the step's face along the
whole edge: built, valid, the step's face consumed), with 1.9 (inside the
face, as stock), and a bar 2 x 2 whose edge is rounded with 5, wider than
both its faces (no ball touches a face: fails, as stock).

## 0012

[0012-chfi3d-closed-stripe-end-on-vertex.patch](../opencascade/0012-chfi3d-closed-stripe-end-on-vertex.patch)

Test: none, because no small body was found that reaches the branch at
the port's tip (mitcad#2121). It needs a closed stripe whose first common
point on a side is a vertex while the walk ends its last one, at the
same point one period on, without that vertex. The case it was written for, the foot of a round
step rounded higher than the step, the ball rolling on the closed top rim
(geometry/tests/test_fillet_overflow.cpp,
test_closed_roundings_roll_on_a_rim), starts on the rim's vertex, where
the seams of the step's side and of the foot meet, and builds without the
patch at the tip (shown 2026-11-20 by tools/occt-port-proof.py): the end
there is taken for the vertex too, or the start is not, since later
patches handle a closed rolling stripe starting on a seam vertex (0152)
and make a closed stripe's end the vertex where its contact runs along a
face's edge (0321). A rim whose vertex lies off the spine's start puts
neither end on a vertex. A search run on 2026-11-21 (OCCT at the tip,
ChFi3d_Builder_0.cxx with and without the patch) found none either: 504
bodies, the foot of a step 0.5 or 1 high and 5, 8.5 or 20 in radius on a
plate rounded with 0.6 to 1.2, the step a cylinder whose seam stood at 0
to 180 degrees, or a prism of a circle of two or three arcs whose
vertices lay 0.5 to 3.14 radians apart, all build valid with and without
the patch.

## 0013

[0013-chfi3d-overflow-across-faces-and-overlapping-roundings.patch](../opencascade/0013-chfi3d-overflow-across-faces-and-overlapping-roundings.patch)

Test: geometry/tests/port/p0013-overflow-across-faces.cpp (ctest
geometry.port.0013): a block bevelled 2 along its top front edge, both
bevel edges rounded with 5 (each fits on the bevel, the two overlap on it):
one rounding tangent to the top and the front, the bevel gone (without the
overlapping stripes computed again: not built); bevelled 1 the same; a
block's corner cut off by two strips narrower than the rounding (the second
0.75 wide or a sliver of 0.001), the first strip's top edge or the edge
between the strips rounded with 5 over both: the block's corner rounded.
The boundary: the bevel 4 wide rounded with 1, two blends as stock.

## 0014

[0014-chfi3d-contact-along-face-edge.patch](../opencascade/0014-chfi3d-contact-along-face-edge.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_contacts_along_the_far_edge_of_a_face: a step 3 high whose foot and
top edge are rounded or chamfered with 3 (its side exactly as wide as the
contact distance), and a boss's front edges ending at its corners. It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-20 by
tools/occt-port-proof.py --in-chain, with the declarations of 0030, 0031,
0170, 0180, 0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the
MitCad code uses).

Benchmark cases (mitcad#2273): geometry/tests/test_fillet_bench_cases.cpp,
fillet/l-block-inside-consumed and chamfer/l-block-inside-consumed: an
L-block's inside edge rounded with 7 or chamfered with 7, the blend's
contacts on the far edges of both its faces, which it consumes; checked
against the exact volume and area with OCCT's dressup alone and with
MitCad's (any known failure named there with why). Not a proof of
this patch: their failure without it is not shown.

## 0015

[0015-brepblend-surfrst-smaller-step-on-failed-reframing.patch](../opencascade/0015-brepblend-surfrst-smaller-step-on-failed-reframing.patch)

Test:
geometry/tests/port/p0015-rolling-walk-halves-a-step-off-the-far-branch.cpp
(ctest geometry.port.0015), test_rounding_rolls_round_a_wide_steps_rim: a
round step 0.5 high and 100 in radius on a plate, its foot rounded with
0.6, the ball rolling on the step's top rim all round: built, valid, the
exact volume, the step's side gone. The walk's steps of 4.5 to 9 degrees
start the solver, from the previous section, nearer the ball on the far
side of the rim (its centre 99.41 from the axis against 100.59), whose
plate contact under the step cannot be reframed. The second rounding of
geometry/tests/test_fillet_overflow.cpp's
test_second_rounding_rolls_on_the_first_ones_contact_line rolls inside
its rim, where such a start drifts towards the right ball, and so never
came here. test_rounding_rolls_round_a_narrow_steps_rim, the boundary: the
step 8.5 in radius, whose steps start by the right ball. It fails without
the patch at its place in the chain: OCCT with the patches before it,
without and with this one (shown 2026-11-20 by tools/occt-port-proof.py
--in-chain, with the declarations of 0030, 0031, 0170, 0180, 0260, 0320,
0330, 0531, 1662, 1701 carried, which add API the MitCad code uses).

## 0016

[0016-chfi3d-rounding-rolls-on-anothers-contact-line.patch](../opencascade/0016-chfi3d-rounding-rolls-on-anothers-contact-line.patch)

Test: geometry/tests/test_fillet_roof_slab.cpp (geometry.fillet_roof_slab);
geometry/tests/port/p0016-ride-on-overlap.cpp (ctest geometry.port.0016): a
plate 1 thick whose two top rims are rounded with 0.8, convex (the balls
cannot make one rounding over the top): built, valid, the top consumed;
the same with the slot's floor rims, concave; and a plate 2 thick (0.8
fits: two roundings as stock).

## 0017

[0017-chfi3d-contact-along-face-edge-ends-at-vertices.patch](../opencascade/0017-chfi3d-contact-along-face-edge-ends-at-vertices.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_corner_rounding_beside_a_rounded_top_edge_is_cut_by_the_top: a
corner's rounding whose contact runs along an edge of the face it rolls
on, ending at that edge's vertices. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py, reverted together with 0050, which
changes the same lines). (The boss chamfered as high as its front, named
before, succeeds without the patch.)

## 0018

[0018-chfi3d-corner-where-the-end-face-is-split-in-one-plane.patch](../opencascade/0018-chfi3d-corner-where-the-end-face-is-split-in-one-plane.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_corner_where_the_end_face_is_split_in_one_plane: It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0019

[0019-chfi3d-undone-moresurfdata-corner-fails.patch](../opencascade/0019-chfi3d-undone-moresurfdata-corner-fails.patch)

Test: none, because no synthetic body reaches the case (mitcad#1986). Its
corner needs a stripe whose last two surface data both meet the face at the
end, one with a side reduced to a point where its contact leaves its face
near the spine's end. With a probe at each of PerformMoreSurfdata's early
returns, none returned without its corner in the geometry tests, in OCCT's
blend and chamfer DRAW groups, or in 170 sizes of the pipe and rod cut by
an inclined end plane (test_fillet_overflow's body, which reaches
PerformMoreSurfdata: slopes 0.2 to 1.5, rounding radii 2 to 4, the rod's
split from 3 to 9, close to the plane too): each built its corner. The case
is verified on the stored model only.

## 0020

[0020-chfi3d-crash-fixes.patch](../opencascade/0020-chfi3d-crash-fixes.patch)

Test: geometry/tests/port/p0020-crash-fixes.cpp (ctest geometry.port.0020),
for two of the three faults. The corner curve parallel to the seam: a
chamfer and a rounding of 2 of the arc where a lug's top meets its bore's
wall, the lug ending or starting on the bore's seam (the prolongation of
the lug's end on the wall runs along the seam; the extrema in
PerformOneCorner's split of the prolongation are parallel): valid, the
blend's section turned through the lug's sweep within 1e-6; a segmentation
fault without the patch. A lug away from the seam, the boundary case,
builds the same with and without it. PerformMoreSurfdata's neighbour face:
the pipe and rod cut by an inclined end plane (test_fillet_overflow's
body), the junction rounded with 3 after another contour: after the other
junction (the first stripe's surface data at that index another blend's),
valid, the two adding what each adds alone, invalid without the patch;
after a box's edge in the same compound (the first stripe with one surface
data), valid, the junction's section between the end planes and the box
edge's loss within 1e-9, a segmentation fault without the patch. Each was
shown with TKFillet built without the patch, and without its
PerformMoreSurfdata change alone.

## 0021

[0021-chfi3d-corner-of-contour-closed-at-sharp-vertex.patch](../opencascade/0021-chfi3d-corner-of-contour-closed-at-sharp-vertex.patch)

Test: geometry/tests/test_fillet_kernel.cpp (ctest
geometry.suite.fillet_kernel), test_contour_closed_at_a_sharp_corner: It
fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py,
reverted together with 0503, which changes the same lines).

## 0022

[0022-chfi3d-round-edges-asked-for-near-tangent.patch](../opencascade/0022-chfi3d-round-edges-asked-for-near-tangent.patch)

Test: geometry/tests/test_fillet_kernel.cpp (ctest
geometry.suite.fillet_kernel), test_ridge_between_faces_at_a_small_angle:
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py,
reverted together with 0860, which changes the same lines).

Benchmark case (mitcad#2273): geometry/tests/test_fillet_bench_cases.cpp,
fillet/nearly-flat-ridge: a ridge between faces meeting at 179 degrees
rounded with 180; checked against the exact volume, area and volume
removed with OCCT's fillet alone and with MitCad's (any known failure named
there with why). Not a proof of this patch: its failure without it
is not shown.

## 0023

[0023-chfi3d-meeting-point-of-periodic-contact-lines.patch](../opencascade/0023-chfi3d-meeting-point-of-periodic-contact-lines.patch)

Test:
geometry/tests/port/p0023-meeting-point-of-periodic-contact-lines.cpp
(ctest geometry.port.0023), test_roundings_meet_near_a_shallow_corner: a
plate's outline turning by 10 degrees from a line into an arc of radius
40, both top edges rounded with 2, the arc's edge added first and then
the line's: valid, no self-interference (BOPAlgo_ArgumentAnalyzer), the
two roundings' volume, and the line's rounding running on up to the near
crossing (points it removes 4 to 10 back from the vertex are outside the
result; met at the far crossing, 13.7 back, the arc's torus runs on there
and leaves them inside). The arc's stripe first puts its periodic trace
first, the crossings sorted along it with the near one first, so the far
one, nearer the trace's end 47 degrees past the vertex, replaced it.
test_roundings_meet_at_the_only_crossing, the boundary: the plate only 10
long, the line's contact ending before the far crossing, the same checks.
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0024

[0024-chfi3d-order-of-periodic-kpart-surfaces.patch](../opencascade/0024-chfi3d-order-of-periodic-kpart-surfaces.patch)

Test: geometry/tests/test_fillet_ends.cpp (ctest
geometry.suite.fillet_ends), test_closed_rounding_across_slots: fail
without the patch and pass with it (run against the port before and after
it, mitcad#1986).

## 0025

[0025-chfi3d-end-cut-by-faces-around-a-cylinder-corner.patch](../opencascade/0025-chfi3d-end-cut-by-faces-around-a-cylinder-corner.patch)

Test: geometry/tests/test_fillet_ends.cpp (ctest
geometry.suite.fillet_ends), test_rounding_ending_in_a_cylinder_corner:
It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-20 by
tools/occt-port-proof.py --in-chain, with the declarations of 0030, 0031,
0170, 0180, 0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the
MitCad code uses).

## 0026

[0026-chfi3d-onsame-end-cut-by-several-faces.patch](../opencascade/0026-chfi3d-onsame-end-cut-by-several-faces.patch)

Test: geometry/tests/test_fillet_ends.cpp (ctest
geometry.suite.fillet_ends),
test_rounding_ending_where_its_face_turns_into_a_wall,
test_rounding_ending_where_its_second_face_turns_into_a_wall: fail without
the patch and pass with it (run against the port before and after it,
mitcad#1986).

## 0030

[0030-standard-failed-allocation-throws.patch](../opencascade/0030-standard-failed-allocation-throws.patch)

Test: geometry/tests/test_allocation.cpp (ctest
geometry.suite.allocation), test_collection_allocation_throws,
test_caught_failure_is_out_of_memory: It fails without the patch: the test
does not build without it (shown 2026-11-20 by tools/occt-port-proof.py,
reverted together with 0031, which changes the same lines).

## 0031

[0031-osd-threadpool-failed-allocations-to-launching-thread.patch](../opencascade/0031-osd-threadpool-failed-allocations-to-launching-thread.patch)

Test: geometry/tests/test_allocation.cpp (ctest
geometry.suite.allocation), test_failures_are_counted_per_thread: It fails
without the patch: the test does not build without it (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0032

[0032-ncollection-containers-intact-when-allocation-fails.patch](../opencascade/0032-ncollection-containers-intact-when-allocation-fails.patch)

Test: geometry/tests/test_allocation.cpp (ctest
geometry.suite.allocation), test_collections_intact_after_failure: It
fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0033

[0033-brepcheck-edge-curve-adaptor-per-context.patch](../opencascade/0033-brepcheck-edge-curve-adaptor-per-context.patch)

Test: geometry/tests/port/p0033-edge-checked-in-its-faces-at-once.cpp
(ctest geometry.port.0033). An edge, a planar cubic B-spline of 23 spans,
kept as the edge of five extrusions of it; in each of 2000 rounds a new
BRepCheck_Edge in the parallel mode checks it in the first face, then in
the four others on four threads at once, each evaluating the edge at 21
parameters in 21 spans, so that every evaluation rebuilds the adaptor's
cache. test_edge_of_many_spans_checked_in_its_faces_at_once: every
context is BRepCheck_NoError (without the patch the threads rebuild the
one cache under each other and a context is InvalidCurveOnSurface);
test_edge_of_many_spans_checked_one_face_after_another (the sequential
mode) and test_edge_of_one_span_checked_in_its_faces_at_once (a cache
built once and only read) are the boundary cases, valid with and without
the patch. The old witness, geometry/tests/test_sweeps.cpp
(geometry.suite.sweeps.1, test_helices_on_cylinders), meets the race in
some runs only. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0034

[0034-osd-signal-handler-recursive-mutex.patch](../opencascade/0034-osd-signal-handler-recursive-mutex.patch)

Test: none that the before/after proof can run, because every line the
patch changes lies in OSD_signal.cxx's Windows branch (SIGWntHandler,
CallHandler and the handlers' setup under `#ifdef _WIN32`), which OCCT
compiles on Windows only; the proof tool builds and runs on Linux
(mitcad#2310). On Linux and macOS OCCT's signal handling is the POSIX
branch, which has no THE_SIGNAL_MUTEX, so no test there can fail without
the patch: geometry/tests/port/p0034-raised-signal-reaches-occts-exception.cpp
(ctest geometry.port.0034) passes with and without it there, at the tip and
at its place in the chain (shown 2026-11-20 by tools/occt-port-proof.py and
its --in-chain mode). The ctest is the patch's test on Windows, where the
port's build runs it (the Windows ctests of the release build):
test_raised_segv_is_the_operations_error and
test_raised_sigill_is_the_operations_error, a signal SIGWntHandler hands
to CallHandler (the nested lock) raised inside an operation is the
operation's error, not std::system_error;
test_raised_sigfpe_is_the_operations_error, the boundary case (raise
passes _FPE_EXPLICITGEN, which SIGWntHandler handles under its one lock),
holds with and without the patch;
test_raised_segv_on_another_thread_after_one_was_handled, the mutex is
free again after a handled signal: a new thread installs the handlers and
its signal is its operation's error within 30 s.

## 0050

[0050-chfi3d-kpart-end-runs-into-end-face.patch](../opencascade/0050-chfi3d-kpart-end-runs-into-end-face.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_runs_into_the_faces_at_its_ends: It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py).

## 0051

[0051-chfi3d-rolling-on-across-surfdatas.patch](../opencascade/0051-chfi3d-rolling-on-across-surfdatas.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_rolls_on_a_bevel_edge_round_the_corners: It fails without
the patch at its place in the chain: OCCT with the patches before it,
without and with this one (shown 2026-11-20 by tools/occt-port-proof.py
--in-chain, with the declarations of 0170, 0180, 0260, 0320, 0330, 0531,
1662, 1701 carried, which add API the MitCad code uses).

## 0052

[0052-chfi3d-rolling-start-beside-tangent-face.patch](../opencascade/0052-chfi3d-rolling-start-beside-tangent-face.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_rolls_beside_a_round_corner: It fails without the patch at
its place in the chain: OCCT with the patches before it, without and with
this one (shown 2026-11-20 by tools/occt-port-proof.py --in-chain, with
the declarations of 0170, 0180, 0260, 0320, 0330, 0531, 1662, 1701
carried, which add API the MitCad code uses).

## 0053

[0053-chfi3d-corner-after-overflow-onto-face-beyond.patch](../opencascade/0053-chfi3d-corner-after-overflow-onto-face-beyond.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_rolls_beside_a_round_corner,
test_rounding_beside_a_round_corner_ends_inside_its_faces: It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-20 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).

## 0054

[0054-chfi3d-contact-along-a-part-of-a-face-edge.patch](../opencascade/0054-chfi3d-contact-along-a-part-of-a-face-edge.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_contact_runs_along_a_part_of_the_walls_top_edge: It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-20 by tools/occt-port-proof.py
--in-chain, with the declarations of 0170, 0180, 0260, 0320, 0330, 0531,
1662, 1701 carried, which add API the MitCad code uses).

## 0055

[0055-chfi3d-kpart-blend-cut-by-face-beyond.patch](../opencascade/0055-chfi3d-kpart-blend-cut-by-face-beyond.patch)

Test: geometry/tests/port/p0055-kpart-blend-cut-by-face-beyond.cpp (ctest
geometry.port.0055): a block's top edge rounded with 3 past a round hole of
radius 2 through its side face whose top lies 1.5 under the edge (the
contact leaves the side across the hole's rim and comes back), and the same
hole through the top: built, valid, the blend cut by the hole's wall; the
hole 4 under the edge (no gap) as stock. The case is complete with 0057
(the gaps next to a closed spine's start, filled in order): the test fails
without 0055 and still with 0055 alone, and passes from 0057 on.

## 0056

[0056-chfi3d-corner-cutting-the-last-two-walked-pieces.patch](../opencascade/0056-chfi3d-corner-cutting-the-last-two-walked-pieces.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_between_rod_and_pipe_ends_at_an_inclined_top,
test_rounding_between_rod_and_pipe_ends_at_an_inclined_bottom: It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted
together with 0550, which changes the same lines).

## 0057

[0057-chfi3d-kpart-gaps-in-order-across-closed-lines.patch](../opencascade/0057-chfi3d-kpart-gaps-in-order-across-closed-lines.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rim_rounding_over_blind_holes_is_cut_by_them,
test_rim_rounding_over_notches_is_cut_by_them: It fails without the patch
at its place in the chain: OCCT with the patches before it, without and
with this one (shown 2026-11-20 by tools/occt-port-proof.py --in-chain,
with the declarations of 0170, 0180, 0260, 0320, 0330, 0531, 1662, 1701
carried, which add API the MitCad code uses).

## 0058

[0058-chfi3d-rounding-rolls-on-a-convex-far-edge.patch](../opencascade/0058-chfi3d-rounding-rolls-on-a-convex-far-edge.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow), test_rounding_rolls_on_a_bevels_far_edge,
test_concave_rounding_rolls_on_a_strips_far_edge: Its failure without the
patch is still to be shown: the patch does not revert at the port's tip
without the more than 40 later patches that change the same lines (tried
2026-11-20 by tools/occt-port-proof.py).

## 0059

[0059-chfi3d-kpart-blend-cut-by-the-face-at-the-end.patch](../opencascade/0059-chfi3d-kpart-blend-cut-by-the-face-at-the-end.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_at_a_groove_end_is_cut_by_the_top: It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-20 by tools/occt-port-proof.py
--in-chain, with the declarations of 0170, 0180, 0260, 0320, 0330, 0531,
1662, 1701 carried, which add API the MitCad code uses).

## 0060

[0060-shapefix-intersecting-edges-once-per-face.patch](../opencascade/0060-shapefix-intersecting-edges-once-per-face.patch)

Test: geometry/tests/test_shapefix.cpp (ctest geometry.suite.shapefix),
test_holes_repaired_as_before (the results stock OCCT 8.0.1 gave, on faces
of many holes) and test_apply_again_as_apply
(ShapeBuild_ReShape::ApplyAgain, which the patch adds: it does not build
without it). The repair's time has no bound yet. It fails without the
patch: the test does not build without it (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 1050, which changes the
same lines).

## 0070

[0070-chfi3d-kpart-cut-to-end-where-the-narrow-face-vanishes.patch](../opencascade/0070-chfi3d-kpart-cut-to-end-where-the-narrow-face-vanishes.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_at_a_groove_end_consumes_the_wall: It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-20 by tools/occt-port-proof.py
--in-chain, with the declarations of 0170, 0180, 0260, 0320, 0330, 0531,
1662, 1701 carried, which add API the MitCad code uses).

## 0071

[0071-chfi3d-kpart-cut-by-several-faces-beyond.patch](../opencascade/0071-chfi3d-kpart-cut-by-several-faces-beyond.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_over_a_slot_is_cut_by_its_faces: Its failure without the
patch is still to be shown: the patch does not revert at the port's tip
without the more than 40 later patches that change the same lines (tried
2026-11-20 by tools/occt-port-proof.py).

## 0072

[0072-chfi3d-kpart-cuts-on-closed-spines.patch](../opencascade/0072-chfi3d-kpart-cuts-on-closed-spines.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_chamfer_round_a_ring_past_the_side_is_cut_by_it;
geometry/tests/test_dressup.cpp (ctest geometry.suite.dressup),
test_dressups_of_circles_built_as_rings. Its failure without the patch is
still to be shown: the patch does not revert at the port's tip without the
more than 40 later patches that change the same lines (tried 2026-11-20 by
tools/occt-port-proof.py).

## 0073

[0073-chfi3d-kpart-pieces-over-a-hole-are-one-face.patch](../opencascade/0073-chfi3d-kpart-pieces-over-a-hole-are-one-face.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow), test_rounding_over_a_hole_is_cut_by_it,
test_rim_rounding_past_a_side_is_cut_by_it,
test_rounding_over_a_slot_is_cut_by_its_faces: the blend's pieces over the
cut one face (the count of faces). It fails without the patch at its place
in the chain: OCCT with the patches before it, without and with this one
(shown 2026-11-20 by tools/occt-port-proof.py --in-chain, with the
declarations of 0170, 0180, 0260, 0320, 0330, 0531, 1662, 1701 carried,
which add API the MitCad code uses).

## 0074

[0074-chfi3d-walked-blend-cut-by-the-face-at-the-end.patch](../opencascade/0074-chfi3d-walked-blend-cut-by-the-face-at-the-end.patch)

Test: geometry/tests/port/p0074-walked-blend-cut-at-the-end.cpp (ctest
geometry.port.0074): an inside corner between a plate's side N and a
block's side whose top edge is rounded with 2 (made a surface of
revolution, so that the corner's blend is walked), the corner's arc rounded
with 3: N's contact leaves N across the top before the arc ends; built,
valid, the blend ending in a point on the top's edge, the fill exact
(without the patch: invalid). Nearby radii (3 and 2, 2 and 1.5) built so
too; the boundary: the plate rising above the top, no cut, as stock.

## 0075

[0075-blendfunc-constrad-rolling-ball-section.patch](../opencascade/0075-blendfunc-constrad-rolling-ball-section.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp (ctest
geometry.suite.fillet_rolling_ball),
test_rounding_rolling_on_a_rim_past_its_face_is_a_rolling_ball and
test_rounding_rolling_on_a_skew_edge_is_a_rolling_ball: every point of the
blend at the radius from the curve of the ball's centre (OCCT's sections
missed it by up to 0.014). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 1390, 1520, 1523, 1760,
which change the same lines). (The tilted rod and cone tests named before
succeed without the patch.)

## 0076

[0076-chfi3d-extension-rolls-on-the-faces-own-surface.patch](../opencascade/0076-chfi3d-extension-rolls-on-the-faces-own-surface.patch)

Test: none that fails without it, because no body was found whose result
the patch changes beyond the checks' tolerances: the synthetic test below,
written for it, passes without the patch at the port's tip (reverted
together with 0240) and at its place in the chain (shown 2026-11-20 by
tools/occt-port-proof.py and its --in-chain mode; mitcad#2121,
mitcad#2310). The patch changes where ChFi3d_BuildPlane continues the walk
past the spine's end on a cylinder, cone, sphere or torus: on the face's
own surface instead of the plane tangent to it at the crossing. The two
continuations differ by the gap between the sections where the pieces
meet, 0.03 in the header's case (a rounding of 3 round a chamfered bar,
with 0075's rolling ball), which made the face builder drop part of a
planar face. The sphere cap below was built so that the blend's cut on
the flat face is made of the extension's sections, whose centres on the
tangent plane would leave the circle the ball's centre runs on by up to
0.057; yet without the patch its result is valid and its cuts lie within
2e-4 of the ball's radius from that circle, as with it. Why the
extension's sections there come out on the circle without the patch was
not traced (it needs a run that logs which continuation the walk takes
there). The header's chamfered
bar was not rebuilt as a synthetic body (its dimensions are not recorded
beyond the radius), so the patch stays without a proof. The test stays as
the patch's case:
geometry/tests/port/p0076-extension-rolls-on-the-faces-own-surface.cpp
(ctest geometry.port.0076),
test_cut_of_the_extension_past_the_sphere_is_a_rolling_ball: a cap of a
sphere of radius 5 on a plate, cut by a flat face leaning over the plate,
its foot rounded with 2 (walked): at both ends the sphere contact leaves
the sphere across the cap's edge past the spine's end (at 5.8 degrees)
before the plate contact reaches the flat face (13.2), so the blend's cut
on the flat face is made of the extension's sections; valid with its
tolerances raised to the tests' 1e-6 (the cap's edge on the trimmed
sphere keeps 1e-7 for a curve 1.17e-7 off), and the
cuts and the blend lie within 2e-4 of the ball's radius from the circle
its centre runs on (on the tangent plane the extension's centre leaves
that circle by up to 0.057).
test_cut_before_the_sphere_edge_is_a_rolling_ball, the boundary: the flat
face leaning further, the plate contact reaching it first, the cut made
of sections on the sphere face. (The tests of
0075 to 0077 in geometry/tests/test_fillet_rolling_ball.cpp round closed
edges, whose blends have no ends and never come here.)

## 0077

[0077-chfi3d-walk-past-the-spine-ends-before-a-singular-section.patch](../opencascade/0077-chfi3d-walk-past-the-spine-ends-before-a-singular-section.patch)

Test: none that fails without it, because no test was found that the patch
decides (mitcad#2121, mitcad#2310). The tests written with 0075 to 0077,
geometry/tests/test_fillet_rolling_ball.cpp (ctest
geometry.suite.fillet_rolling_ball),
test_rounding_between_a_plate_and_a_tilted_rod_is_a_rolling_ball,
test_rounding_of_a_tilted_rods_top_is_a_rolling_ball and
test_rounding_between_a_plate_and_a_tilted_cone_is_a_rolling_ball, round
closed edges, whose blends have no spine ends, so the cut past an end
never applies to them; they pass without the patch at its place in the
chain (shown 2026-11-20 by tools/occt-port-proof.py --in-chain), and at
the port's tip, reverted together with the ten later patches that change
its lines, OCCT does not build. The header's case, a rounding round a cone and
a cylinder on one axis whose walk ran on 15 past the end on a circle of
radius 17 and failed with "Failed approximation", gives no dimensions
beyond those, and the chamfer of a reported design it names is a corpus
case, which a test here may not hold; neither was rebuilt as a synthetic
body. OCCT's DRAW case blend/complex B4, which the patch must leave
valid, is its boundary in the port's DRAW runs.

## 0090

[0090-chfi3d-double-interference-at-vertex-kind.patch](../opencascade/0090-chfi3d-double-interference-at-vertex-kind.patch)

Test: geometry/tests/test_chamfer_kernel.cpp (ctest
geometry.suite.chamfer_kernel),
test_chamfer_pieces_meeting_at_a_vertex_of_the_edge_they_reach: It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-20 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).

## 0091

[0091-topopebrepbuild-point-uses-within-array.patch](../opencascade/0091-topopebrepbuild-point-uses-within-array.patch)

Test: geometry/tests/port/p0091-point-uses-counted-within-the-points.cpp
(ctest geometry.port.0091). With 0090 no body makes an index outside the
points, so the test builds the data structure itself: two points (one
used by an edge interference, one by none), an edge, a curve without
geometry and one interference of kind POINT with the index INT_MAX, then
calls BuildEdges. test_curve_support_outside_the_points,
test_edge_geometry_outside_the_points and
test_edge_support_outside_the_points, one per patched increment: it
returns without an error, keeps the used point and removes the other
(without the patch the increment reaches 8 GB past the array, a
segmentation fault that run_operation turns into an error);
test_curve_geometry_outside_the_points (the increment OCCT checked) and
test_all_indices_within_the_points are the boundary cases. The old
witness, geometry/tests/test_chamfer_kernel.cpp
(geometry.suite.chamfer_kernel,
test_chamfer_pieces_meeting_at_a_vertex_of_the_edge_they_reach), passes
without this patch as 0090 removed the bad index. It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0100

[0100-brepoffset-faces-vanishing-within-the-offset.patch](../opencascade/0100-brepoffset-faces-vanishing-within-the-offset.patch)

Test: geometry/tests/test_shell_vanishing.cpp (geometry.suite part of
mitcad#172); geometry/tests/port/p0100-offset-vanishing-face.cpp (ctest
geometry.port.0100): a block with its bottom front edge chamfered 0.5
shelled inwards from its open top by 1 (the strip vanishes: valid, the
cavity that of the plain block, its neighbours' offsets meeting) and by 0.2
(the strip stays, as stock), with the intersection and arc joins; the same
with a vertical edge chamfered, and the closed offset inwards; a slab
whose top face is a C0 B-spline ridge (not flat: the offset replaces a flat
one with its plane), refused before the vanishing faces are
sought (BRepOffset_C0Geometry), as stock.

## 0110

[0110-shapeupgrade-unifysamedomain-chain-not-concatenated.patch](../opencascade/0110-shapeupgrade-unifysamedomain-chain-not-concatenated.patch)

Test: geometry/tests/port/p0110-unify-chain-not-concatenated.cpp (ctest
geometry.port.0110): a shell of two coplanar squares sharing an edge, the
first one's bottom two B-spline pieces in line 2e-6 apart at a vertex of
tolerance 1.5e-6 (the curves do not join): the unification merges the
faces and the collinear sides and keeps the pieces as their edges, valid
(stock: it throws); with the vertex's tolerance 1e-5, the pieces merged
into one edge.

## 0120

[0120-brepcheck-face-wire-boxes-of-conics-past-their-range.patch](../opencascade/0120-brepcheck-face-wire-boxes-of-conics-past-their-range.patch)

Test: geometry/tests/port/p0120-wire-boxes-of-arcs-past-their-range.cpp
(ctest geometry.port.0120),
test_arc_holes_past_their_range_classified_as_fast_as_exact_ones: the
bound, 900 holes of arcs ending 4e-15 past 2*PI classified within six times
the time of the same holes ending at 2*PI (a ratio of about 1; about 25
without the patch, 5.1 s against 0.21 s). The statuses:
geometry/tests/test_large_bodies.cpp (ctest geometry.suite.large_bodies),
test_wires_of_arcs_past_their_range_are_classified (the same without the
patch).

## 0121

[0121-brepcheck-shell-orientation-edges-listed-once-per-face.patch](../opencascade/0121-brepcheck-shell-orientation-edges-listed-once-per-face.patch)

Test: geometry/tests/port/p0121-shell-orientation-many-edges.cpp (ctest
geometry.port.0121): the orientation of a plate with 81 holes is right and
a reversed face is still found, as below the threshold; a plate of 19 881
holes is checked within 2 s (the search of each face, quadratic, about 7 s).

## 0130

[0130-chfi3d-rounding-vanishing-at-a-tangent-end.patch](../opencascade/0130-chfi3d-rounding-vanishing-at-a-tangent-end.patch)

Test: geometry/tests/port/p0130-vanishing-end-tolerance.cpp (ctest
geometry.port.0130): a block cut to a vault of radius 10 springing
tangentially from its walls (rounded with 2 at the corners), the vault's
end rounded with 1: the rounding vanishes at the springing, removing 4.86
(without the patch: 5.12, the walk's end left short of the vertex); the
vault 10.00001 and 10.0001 (0.0014 and 0.0045 rad off tangency): the same,
within 1e-4, every tolerance within the hundredth of the radius (without
the patch 10.0001 is invalid). The boundary: 10.001 (0.014 rad), a corner
of its own, as before.

## 0140

[0140-chfi3d-contact-along-a-closed-face-edge.patch](../opencascade/0140-chfi3d-contact-along-a-closed-face-edge.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_chamfer_as_wide_as_a_step_ring and
test_chamfer_as_wide_as_a_lug_in_a_bore: the step ring's inner rim
chamfered by the ring's width (a whole circle with the counterbore's seam
at its vertex, two arcs with the seam there or turned away) and a lug's
top arc in a bore chamfered by the top's width, its ends away from the
bore's seam: built, valid, the exact volume, the ring (the lug's top)
gone; the cases as the patch came with them (mitcad#2310: the cases added
later, the turned whole circle, the fillets, the lug's ends on the seam
and the p-curves' fit, need 0144 and 0147 and are their tests). It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).

## 0141

[0141-chfi3d-onsame-end-second-contact-beyond-the-edge.patch](../opencascade/0141-chfi3d-onsame-end-second-contact-beyond-the-edge.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_chamfer_of_a_rib_cut_by_its_sides:
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py,
reverted together with 0282, which changes the same lines).

## 0142

[0142-chfi3d-kpart-cut-by-faces-beyond-that-grow.patch](../opencascade/0142-chfi3d-kpart-cut-by-faces-beyond-that-grow.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow),
test_chamfer_of_a_boss_beyond_a_plate_corner: a boss's foot under a
plate's rounded corner chamfered past the plate's outline, the boss as
long as the chamfer or longer with its seam away from the gap, and as
long as the chamfer with its seam in the gap: built, valid, the exact
volume; the cases as the patch came with them (mitcad#2310: the longer
boss with its seam in the gap, the p-curves' fit and the cone as one face
need 0145 and 0146 and are their test; the post beyond a square plate,
named here before, came with 0480 and 0481 and is theirs). It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).

## 0143

[0143-chfi3d-chamfer-corner-beyond-the-common-face.patch](../opencascade/0143-chfi3d-chamfer-corner-beyond-the-common-face.patch)

Test: geometry/tests/port/p0143-chamfer-corner-beyond-the-common-face.cpp
(ctest geometry.port.0143): a second geometry to test_chamfer_overflow.cpp's
hexagon, a square prism under a disc, the edges where its walls meet the
disc's underside chamfered 2, wider than the underside near the corners:
built, valid, exact volume, each corner cut by the disc's cylinder where
the chamfers' intersection meets it, with the cylinder's seam away from the
corners and on a corner's diagonal; chamfers of 0.8, whose contacts cross
on the underside, as stock.

## 0144

[0144-chfi3d-closed-contact-split-at-the-edges-vertex.patch](../opencascade/0144-chfi3d-closed-contact-split-at-the-edges-vertex.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_step_ring_rim_blends_past_the_seam
(the step ring's cases in every form, as since 0144 to 0147; renamed by
mitcad#2310, which gave 0140's own cases a function of their own, its
cases as in the run below): It fails without the patch at its place in the chain: OCCT
with the patches before it, without and with this one (shown 2026-11-20 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).


## 0145

[0145-chfi3d-kpart-pieces-in-their-faces-periods.patch](../opencascade/0145-chfi3d-kpart-pieces-in-their-faces-periods.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow),
test_boss_foot_chamfer_across_its_seam (the boss's cases as they are since
0145 and 0146; renamed by mitcad#2310, which gave 0142's own cases a
function of their own, its cases as in the run below): It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py, reverted together with
0481, 0581, 0650, which change the same lines).

## 0146

[0146-chfi3d-kpart-pieces-one-face-round-a-closed-spine.patch](../opencascade/0146-chfi3d-kpart-pieces-one-face-round-a-closed-spine.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow),
test_boss_foot_chamfer_across_its_seam (the boss's cases as they are since
0145 and 0146; renamed by mitcad#2310, which gave 0142's own cases a
function of their own, its cases as in the run below): It fails without the patch
at its place in the chain: OCCT with the patches before it, without and
with this one (shown 2026-11-20 by tools/occt-port-proof.py --in-chain,
with the declarations of 0170, 0180, 0260, 0320, 0330, 0531, 1662, 1701
carried, which add API the MitCad code uses).


## 0147

[0147-chfi3d-onsame-end-prolonged-along-a-seam.patch](../opencascade/0147-chfi3d-onsame-end-prolonged-along-a-seam.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_lug_in_a_bore_blends_on_its_seam
(the lug's cases in every form, as since this patch; renamed by
mitcad#2310, which gave 0140's own case a function of its own, its cases
as in the run below):
It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-20 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).


## 0148

[0148-chfi3d-chamfer-as-wide-as-a-ledge-along-a-line-and-an-arc.patch](../opencascade/0148-chfi3d-chamfer-as-wide-as-a-ledge-along-a-line-and-an-arc.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow),
test_chamfer_as_wide_as_a_ledge_along_a_line_and_an_arc: It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 0321, 0322, 0440, 1560, which change the same lines).

## 0149

[0149-chfi3d-end-snapped-onto-a-closed-edges-vertex-on-a-periodic-face.patch](../opencascade/0149-chfi3d-end-snapped-onto-a-closed-edges-vertex-on-a-periodic-face.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_chamfer_as_long_as_a_boss: It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted
together with 1110, 1560, 1730, which change the same lines).

## 0150

[0150-chfi3d-onsame-corner-beside-a-split-face.patch](../opencascade/0150-chfi3d-onsame-corner-beside-a-split-face.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results), test_slot_end_beside_a_split_side: It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).

## 0151

[0151-chfi3d-corner-on-an-end-face-split-in-one-plane.patch](../opencascade/0151-chfi3d-corner-on-an-end-face-split-in-one-plane.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results), test_rounding_ends_on_a_pads_split_foot:
It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with the declarations of 0170, 0180,
0260, 0320, 0330, 0531, 1662, 1701 carried, which add API the MitCad code
uses).

## 0152

[0152-chfi3d-closed-rolling-start-on-a-seam-vertex.patch](../opencascade/0152-chfi3d-closed-rolling-start-on-a-seam-vertex.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results),
test_closed_rounding_rolling_from_a_seam_vertex: It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 0173, 1230, 1370, 1430, 1580, which change the same lines).

## 0153

[0153-chfi3d-roundings-meeting-on-one-contact-line.patch](../opencascade/0153-chfi3d-roundings-meeting-on-one-contact-line.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results), test_plate_edges_rounded_to_a_full_round;
geometry/tests/test_fillet_occt_cases.cpp (ctest
geometry.fillet_occt_cases),
test_box_top_full_round_with_rounded_vertical_edges. It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-23 by tools/occt-port-proof.py
--in-chain, with the declarations of 0170, 0180, 0260, 0320, 0330, 0531,
1662, 1701 carried, which add API the MitCad code uses).

Benchmark cases (mitcad#2273): geometry/tests/test_fillet_bench_cases.cpp,
fillet/slab-half-round, fillet/hole-waist, fillet/boss-s-curve and
chamfer/block-ridge: two roundings or chamfers whose contact lines on the
face between their edges are one line, the face vanishing (a block's top
rounded to a half cylinder or chamfered to a ridge, a hole's two rims
rounded to its waist, a boss's rim and root rounded into an S); checked
against the exact volume and area with OCCT's dressup alone and with
MitCad's (any known failure named there with why). Not a proof of
this patch: their failure without it is not shown.

## 0160

[0160-chfi3d-walked-blend-over-a-rounded-rim.patch](../opencascade/0160-chfi3d-walked-blend-over-a-rounded-rim.patch)

Test: geometry/tests/port/p0160-walked-blend-over-a-rounded-rim.cpp (ctest
geometry.port.0160): a block's edge rounded with 3 past a hole whose rim is
rounded with 0.5, and past a wider hole whose rim is rounded with 1, the
contact on the top crossing the hole's mouth: built, valid, the rim cutting
the blend (the body less the ball's corner region); a hole whose mouth
stays behind the contact, as stock.

## 0170

[0170-brepblend-surfrst-rolling-ball-and-pivot.patch](../opencascade/0170-brepblend-surfrst-rolling-ball-and-pivot.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp (ctest
geometry.suite.fillet_rolling_ball),
test_rounding_rolling_on_a_skew_edge_is_a_rolling_ball,
test_rounding_rolling_on_a_rim_past_its_face_is_a_rolling_ball: Its
failure without the patch is still to be shown: the patch does not revert
at the port's tip without the more than 40 later patches that change the
same lines (tried 2026-11-20 by tools/occt-port-proof.py).

## 0171

[0171-chfi3d-rolling-ball-turns-about-a-vertex.patch](../opencascade/0171-chfi3d-rolling-ball-turns-about-a-vertex.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp (ctest
geometry.suite.fillet_rolling_ball),
test_rounding_rolling_on_a_skew_edge_is_a_rolling_ball,
test_rounding_rolling_on_an_edge_into_a_concave_turn: the ball's turn
about the edge's end. Its failure without the patch is still to be shown:
the patch does not revert at the port's tip without the more than 40 later
patches that change the same lines (tried 2026-11-20 by
tools/occt-port-proof.py).

## 0172

[0172-chfi3d-rolled-blend-cut-by-the-face-at-the-end.patch](../opencascade/0172-chfi3d-rolled-blend-cut-by-the-face-at-the-end.patch)

Test: geometry/tests/port/p0172-rolled-blend-cut-at-the-end.cpp (ctest
geometry.port.0172): a round pocket cut into a post off its axis, its floor
a lens between the pocket's wall and the post's rim narrower than the
radius, the floor's edge at the wall rounded with 2.5 and 2 (the ball
touching the wall and rolling on the rim): built, valid, the lens consumed,
the blend cut by the post's side at each tip; a pocket within the post,
its floor wide enough for the ball all round, as stock.

## 0173

[0173-chfi3d-stripe-computed-again-with-occt-inverse-where-rolling-walk-fails.patch](../opencascade/0173-chfi3d-stripe-computed-again-with-occt-inverse-where-rolling-walk-fails.patch)

Test: geometry/tests/port/p0173-occt-inverse-second-solver.cpp (ctest
geometry.port.0173): a sheet of a plate 10 x 20 with a round hole of radius
3 whose rim comes within 3e-5, 2.5e-5 (size 1) or 1.5e-5 (size 0.5) of the
plate's edge, a conical wall of polynomial B-splines on the rim and a side
wall hanging from that edge; the rim's near half rounded with 0.5: the
ball rolls on the plate's edge with the side wall and back; built, valid,
the area as at wider gaps (without the patch: not built). The boundary:
gaps of 5e-5 and 1e-4, built with the rolling equations alone.

## 0180

[0180-chfi3d-chains-stop-where-the-edges-turn.patch](../opencascade/0180-chfi3d-chains-stop-where-the-edges-turn.patch)

Test: geometry/tests/test_fillet_chains.cpp (ctest
geometry.suite.fillet_chains), test_contour_stops_at_a_drafted_crease,
test_selected_edges_past_a_turn_stay_one_contour,
test_mitcad_fillet_stops_at_a_drafted_crease;
geometry/tests/test_fillet_chain_loops.cpp (ctest
geometry.fillet_chain_loops),
test_contour_round_a_closed_chain_stops_where_it_turns. They use the
option the patch adds and do not build without it. Its failure without the
patch is still to be shown: the patch does not revert at the port's tip
without the more than 40 later patches that change the same lines (tried
2026-11-20 by tools/occt-port-proof.py).

## 0190

[0190-chfi3d-consumed-face-round-a-closed-stripe.patch](../opencascade/0190-chfi3d-consumed-face-round-a-closed-stripe.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_closed_rounding_consumes_an_earlier_rounding_over_a_rounded_rim: It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 0260, 0320,
0330, 0531, 1662, 1701 carried, which add API the MitCad code uses).

## 0200

[0200-chfi3d-walked-blend-about-a-standing-contact.patch](../opencascade/0200-chfi3d-walked-blend-about-a-standing-contact.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results),
test_top_rounded_past_a_corner_of_the_balls_radius,
test_closed_top_rounded_past_four_corners,
test_pocket_floor_rounded_past_its_corners: It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py).

## 0210

[0210-chfi3d-mitred-ends-at-a-short-sharp-edge.patch](../opencascade/0210-chfi3d-mitred-ends-at-a-short-sharp-edge.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners), test_two_roundings_mitred_at_a_short_edge:
It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 0260, 0320,
0330, 0531, 1662, 1701 carried, which add API the MitCad code uses).


## 0211

[0211-chfi3d-end-cut-by-a-periodic-face-across-its-seam.patch](../opencascade/0211-chfi3d-end-cut-by-a-periodic-face-across-its-seam.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners),
test_rounding_cut_by_a_cylinder_across_its_seam;
geometry/tests/test_fillet_occt_cases.cpp (ctest
geometry.fillet_occt_cases),
test_axis_edge_rounded_up_to_the_pole_of_a_partial_revolve. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0220

[0220-shapeupgrade-unifysamedomain-seams-half-a-period-apart.patch](../opencascade/0220-shapeupgrade-unifysamedomain-seams-half-a-period-apart.patch)

Test: geometry/tests/port/p0220-coaxial-cylinders-seams-half-a-turn-apart.cpp
(ctest geometry.port.0220): test_seams_half_a_turn_apart_merge fuses a
cylinder of radius 10 with its seam at +x and a cylinder or revolved ring
of the same radius on it with its seam at -x (made upwards or downwards,
revolved about +z or -z, either argument first: sixteen cases) and merges
the result with ShapeUpgrade_UnifySameDomain: each one valid solid of the
bodies' volume with one face on the cylinder of radius 10;
test_seams_together_merge does the same with both seams at +x (no half
circle between them; merged with and without the patch). It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py). The join in
geometry/tests/test_boolean.cpp
(test_join_merges_coaxial_cylinders_whatever_their_seams) passes without
it.

## 0230

[0230-chfi3d-contact-in-the-adjacent-face-period.patch](../opencascade/0230-chfi3d-contact-in-the-adjacent-face-period.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_chamfer_of_a_flange_under_teeth:
a flange's top circle chamfered past the strip between the circle and the
teeth, its contact running on into the teeth's faces and round the root
roundings' cylinders; without the patch the result is invalid. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py). (The lug
in a bore with its seam turned, named before, succeeds without the patch.)

## 0240

[0240-chfi3d-extension-on-the-faces-own-surface-away-from-its-seam.patch](../opencascade/0240-chfi3d-extension-on-the-faces-own-surface-away-from-its-seam.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results), test_extension_rolling_on_past_a_seam: It
fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0241

[0241-chfi3d-roundings-meeting-across-a-notch.patch](../opencascade/0241-chfi3d-roundings-meeting-across-a-notch.patch)

Test: geometry/tests/port/p0241-roundings-meeting-across-a-notch.cpp (ctest
geometry.port.0241): the rim of a half-round bay of radius 10 in a plate (a
concave wall) broken at its deepest point by a notch 0.1 deep and 0.34
wide, both arcs rounded with 0.3: built, valid, one torus face across the
notch, the volume of the whole rim rounded and the notch cut after; a
notch 1 deep as stock (two torus faces, the same volume). The disc's
convex rim: test_fillet_results.cpp.

## 0250

[0250-chfi3d-walk-on-along-a-face-of-the-spine.patch](../opencascade/0250-chfi3d-walk-on-along-a-face-of-the-spine.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_rounding_round_a_chamfered_box_walks_on_along_its_rounded_corners,
test_top_rounding_larger_than_the_rounded_corners_walks_round_them: It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with 0260, 0270, 0271, 0320 and the
declarations of 0330, 0531, 1662, 1701 carried, which add API the MitCad
code uses).

## 0260

[0260-chfi3d-fillet-carried-across-a-notch-in-its-rim.patch](../opencascade/0260-chfi3d-fillet-carried-across-a-notch-in-its-rim.patch)

Test: geometry/tests/port/p0260-contour-carried-across-a-notch.cpp (ctest
geometry.port.0260): a D of radius 20 whose rim is broken by a tapered
notch 0.25 deep and 0.57 wide, one arc given and rounded with 0.5: one
contour carried, built, valid, the volume of both arcs given and of the
whole rim rounded with the notch cut after; a notch 2 wide (wider than the
ball): nothing carried, one contour.

## 0270

[0270-chfi3d-roundings-meeting-on-one-contact-line-of-a-face-going-on.patch](../opencascade/0270-chfi3d-roundings-meeting-on-one-contact-line-of-a-face-going-on.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results),
test_rib_rounded_to_a_full_round_out_of_a_plate,
test_rib_rounded_to_a_full_round_out_of_a_ring,
test_slot_rounded_to_a_full_round_into_a_floor;
geometry/tests/test_fillet_occt_cases.cpp (ctest
geometry.fillet_occt_cases),
test_box_top_full_round_with_rounded_vertical_edges. The suite part of
test_fillet_results.cpp runs each test alone with MITCAD_TEST_ONLY since
mitcad#2310 (its tests were not guarded before, so a run alone ran them
all, and at the patch's place 0271's test failed). It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-23 by tools/occt-port-proof.py
--in-chain, with the declarations of 0320, 0330, 0531, 1662, 1701 carried,
which add API the MitCad code uses).

## 0271

[0271-chfi3d-roundings-cut-by-each-other-where-their-contacts-cross.patch](../opencascade/0271-chfi3d-roundings-cut-by-each-other-where-their-contacts-cross.patch)

Test: geometry/tests/port/p0271-roundings-cut-where-contacts-cross.cpp
(ctest geometry.port.0271): a block 20 x 12 x 10 whose long sides are
concave arcs of radius 20, the top 5.8 wide in the middle, both top edges
rounded with 4 (and 5 wide, rounded with 4 and 3): built, valid, the top in
two pieces, the body that of each edge rounded alone, the two in common
(without the patch: not built). The boundary: rounded with 2.8, the
contacts apart, the top one face, as stock.

## 0280

[0280-chfi3d-onsame-end-where-the-face-at-the-end-turns-tangentially.patch](../opencascade/0280-chfi3d-onsame-end-where-the-face-at-the-end-turns-tangentially.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners),
test_rounding_ending_where_the_top_turns_into_a_rim: It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-21 by tools/occt-port-proof.py
--in-chain, with the declarations of 0320, 0330, 0531, 1662, 1701 carried,
which add API the MitCad code uses).

## 0281

[0281-chfi3d-walk-round-the-vertex-past-a-short-edge.patch](../opencascade/0281-chfi3d-walk-round-the-vertex-past-a-short-edge.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners),
test_walk_round_the_corner_past_a_short_edge: It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py, reverted together with
0290, 0720, 1580, 1581, which change the same lines).

## 0282

[0282-chfi3d-end-cut-past-the-bounds-of-the-faces-at-the-end.patch](../opencascade/0282-chfi3d-end-cut-past-the-bounds-of-the-faces-at-the-end.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners),
test_rounding_cut_by_faces_past_their_bounds: It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py).

## 0283

[0283-chfi3d-standing-end-piece-one-face-with-the-one-before.patch](../opencascade/0283-chfi3d-standing-end-piece-one-face-with-the-one-before.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners), test_rounding_one_face_up_to_the_hole: It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 0320, 0330,
0531, 1662, 1701 carried, which add API the MitCad code uses).

## 0290

[0290-chfi3d-walk-round-the-vertex-through-a-face-met-at-a-point.patch](../opencascade/0290-chfi3d-walk-round-the-vertex-through-a-face-met-at-a-point.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners),
test_walk_round_the_corner_through_a_face_met_at_a_point: It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 1580, 1581, which change the same lines).

## 0291

[0291-chfi3d-end-past-the-spine-where-a-contact-ran-off-its-face.patch](../opencascade/0291-chfi3d-end-past-the-spine-where-a-contact-ran-off-its-face.patch)

Test: geometry/tests/test_fillet_corners.cpp (the tapering strip on a wall
turning into its tangent plane); geometry/tests/port/p0291-end-past-the-spine.cpp
(ctest geometry.port.0291): the same body mirrored, its top's edge with the
wall rounded with 1: built, valid, the same volume removed as from the
body itself; the cut plane lifted 0.75 at the wall so that the top stays
at least 1.1 wide (the contact of 1 stays on it) as stock.

## 0300

[0300-chfi3d-prolonged-end-edge-across-the-opposite-faces-seam.patch](../opencascade/0300-chfi3d-prolonged-end-edge-across-the-opposite-faces-seam.patch)

Test: geometry/tests/test_fillet_results.cpp (ctest
geometry.suite.fillet_results),
test_rib_foot_rounded_across_the_rods_seam. The suite part runs each test
alone with MITCAD_TEST_ONLY since mitcad#2310 (its tests were not guarded
before, so a run alone ran them all, and at the patch's place 0670's test
failed). It fails without the patch at its place in the chain: OCCT with
the patches before it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with the declarations of 0320, 0330,
0531, 1662, 1701 carried, which add API the MitCad code uses).

## 0320

[0320-chfi3d-where-a-fillet-stops.patch](../opencascade/0320-chfi3d-where-a-fillet-stops.patch)

Test: geometry/tests/test_fillet_kernel.cpp (ctest
geometry.suite.fillet_kernel), test_where_a_fillet_stops: it reads the
stage the patch adds and does not build without it. It fails without the
patch at its place in the chain: with the patches before it, the test
calls a function the patch adds (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with the declarations of 0531, 1662,
1701 and the declarations and new functions of 0330 carried, which add API
the MitCad code uses, the patch's own declarations in the state before it,
which those use).

## 0321

[0321-chfi3d-closed-rounding-consuming-its-face-ends-at-the-vertex.patch](../opencascade/0321-chfi3d-closed-rounding-consuming-its-face-ends-at-the-vertex.patch)

Test: geometry/tests/test_fillet_kernel.cpp (ctest
geometry.suite.fillet_kernel), test_fin_sides_rounded_one_after_the_other:
a fin's sides rounded one after the other, the second rounding's contacts
running along the whole edges the first one left, up to the block, its
stripe ending at those edges' vertices; without the patch the second
rounding fails.
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py,
reverted together with 0322, 0440, 1560, which change the same lines).
(The rim rounded as wide as its ledge, named before, succeeds without the
patch.)

## 0322

[0322-chfi3d-open-stripe-end-at-the-vertex-of-the-edge-its-contact-runs-along.patch](../opencascade/0322-chfi3d-open-stripe-end-at-the-vertex-of-the-edge-its-contact-runs-along.patch)

Test: geometry/tests/test_fillet_kernel.cpp (ctest
geometry.suite.fillet_kernel), test_fin_sides_rounded_one_after_the_other:
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py,
reverted together with 0440, 1560, which change the same lines).

## 0330

[0330-chfi3d-which-corner-a-fillet-fails-at.patch](../opencascade/0330-chfi3d-which-corner-a-fillet-fails-at.patch)

Test: geometry/tests/test_fillet_kernel.cpp (ctest
geometry.suite.fillet_kernel), test_which_corner_a_fillet_fails_at: it
reads the corner the patch adds and does not build without it. It fails
without the patch at its place in the chain: with the patches before it,
the test does not build without the API the patch adds (shown 2026-11-21
by tools/occt-port-proof.py --in-chain, with the declarations of 0531,
1662, 1701 carried, which add API the MitCad code uses).

## 0331

[0331-chfi3d-rounding-rolled-on-a-straight-edge-exactly.patch](../opencascade/0331-chfi3d-rounding-rolled-on-a-straight-edge-exactly.patch)

Test: geometry/tests/test_fillet_kernel.cpp (ctest
geometry.suite.fillet_kernel), test_rounding_rolled_on_a_straight_edge:
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0340

[0340-chfi3d-onsame-end-cut-by-the-face-of-the-edge-the-ball-rolls-on.patch](../opencascade/0340-chfi3d-onsame-end-cut-by-the-face-of-the-edge-the-ball-rolls-on.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners),
test_rounding_cut_by_the_face_of_the_edge_it_rolls_on: It fails without
the patch at its place in the chain: OCCT with the patches before it,
without and with this one (shown 2026-11-21 by tools/occt-port-proof.py
--in-chain, with the declarations of 0531, 1662, 1701 carried, which add
API the MitCad code uses).

## 0341

[0341-chfi3d-rolling-blend-walked-again-across-a-seam-past-its-ends.patch](../opencascade/0341-chfi3d-rolling-blend-walked-again-across-a-seam-past-its-ends.patch)

Test: geometry/tests/port/p0341-rolled-walk-across-a-seam.cpp (ctest
geometry.port.0341): 0340's rod with a ridge, the ridge's end rounded with
5, the rod's seam at 45 degrees on either side of the top, between the
lens's tip and the point where the ball comes tangent to the ridge's side:
built, valid, the material added that of the rod with its seam underneath
(without the patch: not built). The boundary: the seam at 90 degrees,
beyond the tangency, as 0340 builds it.

## 0342

[0342-chfi3d-rounding-rolled-on-past-its-edge-cut-by-the-faces-at-the-ends.patch](../opencascade/0342-chfi3d-rounding-rolled-on-past-its-edge-cut-by-the-faces-at-the-ends.patch)

Test: geometry/tests/test_fillet_corners.cpp (the ear on a boss);
geometry/tests/port/p0342-rolled-on-past-its-edge.cpp (ctest
geometry.port.0342): a second geometry, a rectangular ear on a flat wall
(not a boss), its top a strip narrower than the radius between the wall
and a chamfer on its outer edge, the top's edge with the wall rounded:
built, valid, the blend cut by the ear's sides; a radius the strip holds
as stock. The port test's case is complete with 0431 (the OnSame end of a
blend rolling on an edge that ends with it): it fails without 0342 and
still with 0342 alone, and passes from 0431 on.

## 0350

[0350-chfi3d-regularity-only-where-the-faces-are-tangent.patch](../opencascade/0350-chfi3d-regularity-only-where-the-faces-are-tangent.patch)

Test: geometry/tests/test_fillet_overflow.cpp (ctest
geometry.suite.fillet_overflow),
test_box_roundings_keep_their_tangent_edges_and_take_a_draft,
test_boss_top_rounded_after_its_foot_rounded_as_high_as_it: It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0351

[0351-chfi3d-closed-chain-consuming-a-face-all-round.patch](../opencascade/0351-chfi3d-closed-chain-consuming-a-face-all-round.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_wall_foot_round_a_stepped_pocket:
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0352

[0352-chfi3d-contact-on-a-face-beyond-at-an-acute-angle.patch](../opencascade/0352-chfi3d-contact-on-a-face-beyond-at-an-acute-angle.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow),
test_chamfer_as_wide_as_a_flange_with_a_rounded_rim,
test_wall_foot_round_a_stepped_pocket: It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 0360

[0360-bopalgo-wiresplitter-tangent-edges-measured-at-one-distance.patch](../opencascade/0360-bopalgo-wiresplitter-tangent-edges-measured-at-one-distance.patch)

Test: geometry/tests/test_boolean.cpp (ctest geometry.suite.boolean),
test_split_by_curves_tangent_at_a_vertex: It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py).

## 0380

[0380-chfi3d-chamfer-end-where-the-face-at-the-end-is-tangent-to-both-faces.patch](../opencascade/0380-chfi3d-chamfer-end-where-the-face-at-the-end-is-tangent-to-both-faces.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_chamfer_of_a_rounded_crease. The
test needs the later patch 0381 too: the two came in one change, and the
test's face count and wall-section check are 0381's (at 0380's place the
test fails on the face count with 0380 too, and at 0381's place it passes
with 0381 and fails without it), so in the chain the patch is shown with
0381 carried (mitcad#2310). It fails without the patch at its place in the
chain: OCCT with the patches before it, without and with this one (shown
2026-11-23 by tools/occt-port-proof.py --in-chain, with 0381 carried in
both states, which the test needs, and the declarations of 0531, 1662,
1701 carried, which add API the MitCad code uses).

## 0381

[0381-chfi3d-chamfer-guide-deformed-by-a-hundredth-of-its-distance-at-most.patch](../opencascade/0381-chfi3d-chamfer-guide-deformed-by-a-hundredth-of-its-distance-at-most.patch)

Test: geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_chamfer_of_a_rounded_crease: It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 0531, 1662,
1701 carried, which add API the MitCad code uses).


## 0390

[0390-chfi3d-rolling-ball-touches-the-next-edge-before-the-vertex.patch](../opencascade/0390-chfi3d-rolling-ball-touches-the-next-edge-before-the-vertex.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp (ctest
geometry.suite.fillet_rolling_ball),
test_rounding_rolling_on_an_edge_into_a_concave_turn: It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-21 by tools/occt-port-proof.py
--in-chain, with the declarations of 0531, 1662, 1701 carried, which add
API the MitCad code uses).

## 0391

[0391-brepblend-rstrst-rolling-ball-on-two-edges.patch](../opencascade/0391-brepblend-rstrst-rolling-ball-on-two-edges.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp,
test_rounding_resting_on_the_edges_of_both_faces_is_a_rolling_ball (ctest
geometry.suite.fillet_rolling_ball): a ring whose section is a house, its
inner wall off the axis, its ridge rounded with 3 so that both roof faces
are narrower than the ball's contacts: the ball rests on both eaves, each
tangent to it (before, that start was refused and the rounding missed the
eaves by 1.2). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0400

[0400-chfi3d-rounding-vanishing-at-its-end-stops-at-the-vertex.patch](../opencascade/0400-chfi3d-rounding-vanishing-at-its-end-stops-at-the-vertex.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_vanishing_where_its_faces_come_tangent_short_of_the_end (ctest
geometry.suite.fillet_corners): a valley's floor rounded with 1 up to the
vertex where its sides come tangent, the sides' curves exact and shifted by
2e-4 (the end then within the vertex's tolerance, not at it): both built
with a point at the vertex, the shifted one's volume within 2e-3 of the
exact one's. It fails without the patch at its place in the chain: OCCT
with the patches before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 0531, 1662,
1701 carried, which add API the MitCad code uses).


## 0401

[0401-chfi3d-end-cut-across-the-seam-line-of-a-periodic-face.patch](../opencascade/0401-chfi3d-end-cut-across-the-seam-line-of-a-periodic-face.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_cut_by_a_cylinder_whose_seam_starts_at_the_vertex (ctest
geometry.suite.fillet_corners): a gable's ridge rounded where it ends on a
cylinder whose seam starts at the corner's vertex: the volume removed that
of the same body with the seam a quarter turn away, within 1e-6 (before, an
invalid body without its floor). It fails without the patch at its place
in the chain: OCCT with the patches before it, without and with this one
(shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with the
declarations of 0531, 1662, 1701 carried, which add API the MitCad code
uses).


## 0410

[0410-chfi3d-rounding-ending-in-the-face-it-runs-into.patch](../opencascade/0410-chfi3d-rounding-ending-in-the-face-it-runs-into.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_ending_in_the_face_it_runs_into (ctest
geometry.suite.fillet_corners): a vault's end rounded with the radius of the
block's corner roundings: one contour, built, the volume removed the region
the ball sweeps away, the corner roundings keeping their faces below the
springing (before, the corner failed). It fails without the patch at its
place in the chain: OCCT with the patches before it, without and with this
one (shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with the
declarations of 0531, 1662, 1701 carried, which add API the MitCad code
uses).


## 0420

[0420-chfi3d-kpart-round-a-closed-edge-on-a-face-with-a-stepped-seam.patch](../opencascade/0420-chfi3d-kpart-round-a-closed-edge-on-a-face-with-a-stepped-seam.patch)

Test: geometry/tests/test_fillet_kernel.cpp,
test_boss_rim_rounded_across_a_stepped_seam (ctest
geometry.suite.fillet_kernel): the end circle of a cylinder built as two
bosses whose seams lie at an angle, merged into one face whose seam runs in
steps: one contour, valid, the volume removed as on a reference body whose
seam does not run in steps, within 1e-6. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 0430

[0430-chfi3d-rounding-rolls-before-another-roundings-face.patch](../opencascade/0430-chfi3d-rounding-rolls-before-another-roundings-face.patch)

Test: geometry/tests/port/p0430-rolls-before-another-roundings-face.cpp
(ctest geometry.port.0430): a window through a wall 3.54 thick, its end a
strip 0.5 wide and a face at 0.35, -0.35 or -10 degrees to it, its two
vertical edges there rounded with 2: built, valid, the rounding beside the
strip rolling on its far edge, the material exact; strips on both sides
(one past a sliver of 0.003): both roll (without the patch: not built).
The boundary: rounded with 1.5, the contacts apart, built as before.

## 0431

[0431-chfi3d-rolled-onsame-end-at-the-end-of-its-contact-curve.patch](../opencascade/0431-chfi3d-rolled-onsame-end-at-the-end-of-its-contact-curve.patch)

Test: none, because what the patch changes is a rounding error, not a
geometric condition. The blend rolling on an edge ends where that edge
ends, on the face at the end, and its contact curve on its other face is
bounded there, so the face at the end crosses that curve exactly at its
last parameter; IntCurveSurface finds the crossing within a few 1e-15 to
either side of it, and only a crossing past the end was dropped. Which
side it lands on comes from the arithmetic: the window of
geometry/tests/test_fillet_corners.cpp,
test_roundings_roll_before_each_others_faces (with 0430: its end's two
vertical edges rounded with 2, the end 3.54 wide, the rounding beside the
strip rolling on the strip's far edge), reaches this crossing and lands
inside the range: it passes with this patch and 1332, which changes the
same lines, reverted together, as the port's before/after run showed
(mitcad#1986). A crossing set past the end on purpose cannot be made
either: the curve's end comes from the walk and its approximation, within
1e-7 to 1e-4 of the exact section, so a face tilted to cross it a
nanometre past its end crosses it anywhere within those tolerances. A
body that failed without the patch would fail by chance, and lose it with
any change to the walk.

## 0432

[0432-chfi3d-roundings-crease-over-a-narrow-common-face.patch](../opencascade/0432-chfi3d-roundings-crease-over-a-narrow-common-face.patch)

Test: geometry/tests/port/p0432-roundings-crease-over-a-narrow-face.cpp
(ctest geometry.port.0432): a second geometry to geometry.fillet_corners'
lipped rim, a square rim (lines meeting at 90 degrees) whose lip's inner
bottom edges are rounded with 4 over a ring 1 wide and a 45-degree
chamfer: built, valid, the ends of each crease vertices, the volume exact;
a ring 5 wide as stock.

## 0440

[0440-chfi3d-contact-along-a-chain-of-edges-of-a-consumed-strip.patch](../opencascade/0440-chfi3d-contact-along-a-chain-of-edges-of-a-consumed-strip.patch)

Test: geometry/tests/test_chamfer_overflow.cpp,
test_chamfer_consuming_a_strip_along_a_chain_of_edges (ctest
geometry.suite.chamfer_overflow): the foot of a boss on a slab chamfered (or
rounded) by 2, as wide as the slab's top round the boss, with the top's far
edge whole and split, and the split on either side. It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 1560, which changes the same lines).

## 0450

[0450-chfi3d-rounding-face-turning-tangentially-into-the-end-face.patch](../opencascade/0450-chfi3d-rounding-face-turning-tangentially-into-the-end-face.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_ending_where_its_face_turns_tangentially_into_the_end_face
(ctest geometry.suite.fillet_corners): roundings whose face turns
tangentially into the face at the end, at several angles and spine lengths,
where the walk to that edge runs past the spine's end. It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 0802, 0932, 1020, 1430, 1580, which change the same lines).

## 0460

[0460-chfi3d-onsame-end-on-an-edge-of-a-consumed-corner-face.patch](../opencascade/0460-chfi3d-onsame-end-on-an-edge-of-a-consumed-corner-face.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_foot_rounding_ending_on_a_rounding_of_its_radius (ctest
geometry.suite.fillet_corners): the end feet of a boss rounded with 2, the
radius of the foot roundings beside them, the end section that rounding's
end arc. It fails without the patch at its place in the chain: OCCT with
the patches before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 0531, 1662,
1701 carried, which add API the MitCad code uses).


## 0480

[0480-geomint-open-line-parts-beyond-its-vertices.patch](../opencascade/0480-geomint-open-line-parts-beyond-its-vertices.patch)

Test: geometry/tests/test_chamfer_overflow.cpp,
test_chamfer_of_a_post_beyond_a_square_plate (ctest
geometry.suite.chamfer_overflow): a post's foot chamfered past a square
plate's outline, the post's seam at several angles and the plate turned by
1e-8 (its sides then nearly parallel to the seam): valid, the volume of the
chamfer's ring less its parts beyond the sides (the turned plate fails
without this patch by the test's documentation). Another caller:
geometry/tests/test_fillet_corner_cases.cpp,
test_rounding_end_prolonged_across_a_seam (ctest
geometry.fillet_corner_cases), whose volumes move by 1e-6 mm3 when the
intersection is split. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0481

[0481-chfi3d-kpart-gap-across-a-closed-spines-start-split-there.patch](../opencascade/0481-chfi3d-kpart-gap-across-a-closed-spines-start-split-there.patch)

Test: geometry/tests/test_chamfer_overflow.cpp,
test_chamfer_of_a_post_beyond_a_square_plate (ctest
geometry.suite.chamfer_overflow): the post's foot chamfered past the plate's
outline with the post's seam in a gap between the plate's sides (the spine's
start there): valid, the volume of the chamfer's ring less its parts beyond
the sides (every seam in a gap fails without this patch by the test's
documentation). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 0581, 0650, which change
the same lines).

## 0490

[0490-chfi3d-onsame-end-on-a-trimmed-face-past-its-bounds.patch](../opencascade/0490-chfi3d-onsame-end-on-a-trimmed-face-past-its-bounds.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_ending_at_a_trimmed_cone_past_its_rim (ctest
geometry.suite.fillet_corners): a plate on a conical foot, its bottom edge
rounded with 3 up to the foot, the side's contact meeting the trimmed cone
only above its rim. It fails without the patch at its place in the chain:
OCCT with the patches before it, without and with this one (shown
2026-11-21 by tools/occt-port-proof.py --in-chain, with the declarations
of 0531, 1662, 1701 carried, which add API the MitCad code uses).


## 0500

[0500-chfikpart-fillet-between-coaxial-surfaces-of-revolution.patch](../opencascade/0500-chfikpart-fillet-between-coaxial-surfaces-of-revolution.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp,
test_rounding_between_coaxial_cylinder_and_cone_is_a_torus (ctest
geometry.suite.fillet_rolling_ball): roundings between a cylinder and a
cone, and between two cones, about one axis: each one torus face (the ball's
sweep about the axis; OCCT walked it as a B-spline before); another caller:
geometry/tests/test_fillet_cutouts.cpp,
test_rounding_over_flats_is_the_poles_with_the_flats_cut_again (ctest
geometry.suite.fillet_cutouts), MitCad's fillet of the pole with flats built
by the kernel itself, with no note. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 0501

[0501-chfi3d-kpart-cut-round-a-cut-out-under-an-overhanging-rim.patch](../opencascade/0501-chfi3d-kpart-cut-round-a-cut-out-under-an-overhanging-rim.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp,
test_rim_rounding_below_a_ledge_over_flats_is_cut_round_them (ctest
geometry.suite.fillet_rolling_ball): a post with flats under an overhanging
rim, its rounding below the ledge cut round each flat on a circle of one
edge or two, the torus one face, against the post rounded without flats and
the flats cut after; another caller: geometry/tests/test_fillet_cutouts.cpp,
test_rounding_over_flats_is_the_poles_with_the_flats_cut_again (ctest
geometry.suite.fillet_cutouts), MitCad's fillet of the pole with flats built
by the kernel itself, with no note. It fails without the patch at its
place in the chain: OCCT with the patches before it, without and with this
one (shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with the
declarations of 0531, 1662, 1701 carried, which add API the MitCad code
uses).


## 0502

[0502-chfi3d-kpart-cut-by-the-end-face-over-a-gap.patch](../opencascade/0502-chfi3d-kpart-cut-by-the-end-face-over-a-gap.patch)

Test: geometry/tests/test_fillet_rolling_ball.cpp,
test_thin_rim_rounding_wider_than_the_rim_is_cut_by_the_pocket_and_the_caps
(ctest geometry.suite.fillet_rolling_ball): a thin rim's outer edge rounded
wider than the rim, cut where its round cap meets it before the contact on
the top comes back over the pocket, against the outer wall rounded alone
with the ring it loses taken off the rim outside the caps. It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 0531, 1662,
1701 carried, which add API the MitCad code uses).


## 0503

[0503-chfi3d-two-roundings-meeting-on-an-edge-within-its-tolerance.patch](../opencascade/0503-chfi3d-two-roundings-meeting-on-an-edge-within-its-tolerance.patch)

Test: geometry/tests/port/p0503-corner-ends-within-curves-spread.cpp
(ctest geometry.port.0503): a slot through a chamfered boss whose chamfer's
lower edges are rounded, the creases' curve on the cone turned 2e-7 off the
plane's within a tolerance of 1e-4: the corners are the two roundings'
intersections, valid, with the exact body's volume change (stock: plates
of no width); with the curves exact and the boss's roundings 1e-6 larger
than the walls' (ends about 4e-7 apart on both creases, the curves' spread
nought): a plate as before, or no result.

## 0510

[0510-boptools-pcurve-into-the-period-inside-a-face-spanning-many.patch](../opencascade/0510-boptools-pcurve-into-the-period-inside-a-face-spanning-many.patch)

Test: geometry/tests/test_multi_turn_faces.cpp (ctest
geometry.multi_turn_faces),
test_cross_hole_through_a_thread_whose_edges_run_many_turns,
test_cut_across_a_thread_whose_major_cylinder_is_stored_as_a_cone,
test_merge_after_a_cut_across_a_multi_turn_thread and
test_second_cut_across_a_thread_whose_faces_run_many_turns: cuts across
threads whose faces and edges run many turns: valid results that remove
the tools' material (without the patch the section curves lay a period or
more outside the faces, and the cuts removed nothing). It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py). (The boxes of 1
mm cut across a 32-turn thread, named before, succeed without the patch.)

## 0511

[0511-inttools-fclass2d-every-copy-on-a-face-spanning-many-periods.patch](../opencascade/0511-inttools-fclass2d-every-copy-on-a-face-spanning-many-periods.patch)

Test: geometry/tests/test_multi_turn_faces.cpp (ctest
geometry.multi_turn_faces),
test_cross_hole_through_a_thread_whose_edges_run_many_turns,
test_cut_across_a_thread_whose_major_cylinder_is_stored_as_a_cone and
test_second_cut_across_a_thread_whose_faces_run_many_turns: cuts across
threads whose faces and edges run many turns: the parts of the section
curves inside the faces are kept (without the patch they were classified
outside and the cuts removed nothing). It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py). (The box cuts across the 32-turn
thread, named before, succeed without the patch.)

## 0512

[0512-intcurvesurface-quadric-roots-sampled-by-knot-spans.patch](../opencascade/0512-intcurvesurface-quadric-roots-sampled-by-knot-spans.patch)

Test: geometry/tests/test_multi_turn_faces.cpp,
test_cross_hole_through_a_thread_whose_edges_run_many_turns (ctest
geometry.multi_turn_faces): a cross hole of radius 1 through the threaded
hole at mid-height: each helix edge crosses it twice where it passes the
hole, several times within a few turns: the cut checks the helices'
crossings with the hole (the curve was sampled every two thirds of a turn
before, and found one crossing of each such pair or none). It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0530

[0530-chfi3d-chamfer-guide-keeps-to-its-edges-at-the-blends-beside.patch](../opencascade/0530-chfi3d-chamfer-guide-keeps-to-its-edges-at-the-blends-beside.patch)

Test: none, because no test here reaches the patch's code. The patch
changes the guide of a walked chamfer between two chamfers made as cones
(ChFiKPart): a chain of edges joined at slight angles (1.4 degrees), arcs
between curved stretches, where ChFi3d_PerformElSpine made the stretch's
guide take the cones' end tangents and bent it up to 0.11 mm off its edge
over 16.6 mm. The fin test written with it,
geometry/tests/test_chamfer_overflow.cpp (ctest
geometry.suite.chamfer_overflow), test_chamfer_wider_than_a_fin_side_strip,
is 0531's case (a fin whose side strip is narrower than its tip's
chamfer): at 0530's place it fails with 0530 too, and with 0531 carried
in both states it passes without 0530 (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, mitcad#2310), so it does not show
the guide's bend. The case the patch was made for is a
reported design's chamfer, which a test here may not hold. A synthetic
body for it would be a plate whose top outline runs arc, curved stretch
(a B-spline), arc, the joints kinked by about 1.4 degrees so that the
three edges still form one chain, chamfered along the chain by a quarter
of a millimetre: the stretch's chamfer width in its middle against the
distance would show the bent guide. Such a body has not been built and
run yet, so the patch stays without a proof.

## 0531

[0531-chfi3d-chamfer-wider-than-a-narrow-face-cut-by-the-faces-beyond.patch](../opencascade/0531-chfi3d-chamfer-wider-than-a-narrow-face-cut-by-the-faces-beyond.patch)

Test: geometry/tests/test_chamfer_overflow.cpp,
test_chamfer_wider_than_a_fin_side_strip (ctest
geometry.suite.chamfer_overflow): the fin's tip edge chamfered wider than
its side strip, the guide kept to its edges where the foot's chamfer is
beside it: the volume removed that of the chamfer's sections less the part
inside the foot's chamfer extended. It fails without the patch at its
place in the chain: with the patches before it, the test does not build
without the API the patch adds (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses). NF on B's location:
test_chamfer_of_a_sloped_boss_top_moved in the same file, a sloped boss
top's chamfer on the body moved by a location the same as on the body as
made (before, the moved body's chamfer failed by the signal). The guide of
no length: none yet beside OCCT's DRAW cases above.


## 0540

[0540-chfi3d-rounding-running-on-into-an-earlier-roundings-tube.patch](../opencascade/0540-chfi3d-rounding-running-on-into-an-earlier-roundings-tube.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_running_on_into_an_earlier_roundings_tube (ctest
geometry.suite.fillet_corners): a cut-back side's foot rounded with the
radius of the rounding at its end, the floor edges as circles and as
B-splines: the end grows up to the section the two tubes share. Another
caller: geometry/tests/test_fillet_end_overflow.cpp,
test_wall_foot_ending_at_a_sliver (ctest geometry.fillet_end_overflow), the
same end across a sliver. It fails without the patch at its place in the
chain: OCCT with the patches before it, without and with this one (shown
2026-11-21 by tools/occt-port-proof.py --in-chain, with the declarations
of 1662, 1701 carried, which add API the MitCad code uses).


## 0550

[0550-chfi3d-end-over-pieces-where-the-end-faces-edge-is-in-two.patch](../opencascade/0550-chfi3d-end-over-pieces-where-the-end-faces-edge-is-in-two.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_ending_over_pieces_where_the_cap_edge_is_in_two (ctest
geometry.suite.fillet_corners): a bar on a cylinder, its concave edge
rounded with 3 on over the side's roundings and chamfers to the cap, the end
over two pieces of the cap's edge. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 0560

[0560-boptools-pcurve-checked-at-its-own-copy-in-the-face.patch](../opencascade/0560-boptools-pcurve-checked-at-its-own-copy-in-the-face.patch)

Test: geometry/tests/test_multi_turn_faces.cpp,
test_second_cut_across_a_thread_whose_faces_run_many_turns (ctest
geometry.multi_turn_faces): box cuts on the result of a cross hole through
the threaded hole, whose pieces of the thread's cylinders end well short of
their surfaces' first period: the section curves are put into the piece's
range and the cuts remove the boxes' material (before, the copy was checked
at twice the shift, the box cut removed nothing and its result was invalid).
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0580

[0580-chfi3d-kpart-contact-along-a-concave-edge-off-its-face.patch](../opencascade/0580-chfi3d-kpart-contact-along-a-concave-edge-off-its-face.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_ending_at_a_block_flush_with_its_contact (ctest
geometry.suite.fillet_corners): a pocket rounded over ribs up to blocks
whose sides lie where the rounding touches the ceiling, the contact past the
end along the block's concave edge: the volume added against the rounding's
section. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0581

[0581-chfi3d-roundings-joined-across-a-rib-with-square-sides.patch](../opencascade/0581-chfi3d-roundings-joined-across-a-rib-with-square-sides.patch)

Test: geometry/tests/port/p0581-joined-across-square-ribs.cpp (ctest
geometry.port.0581): a pocket's ceiling rounded with 8 along two walls
broken by ribs 1 thick standing out 6 (one rib on each wall, the other, or
both): built, valid, one cylinder along each wall, the material that of the
rounding's section along the walls and beyond the ribs' ends (without the
patch: invalid). The boundary: no ribs, as before.

## 0590

[0590-shapeupgrade-unifysamedomain-wire-repeating-itself-given-up.patch](../opencascade/0590-shapeupgrade-unifysamedomain-wire-repeating-itself-given-up.patch)

Test: geometry/tests/test_sweeps.cpp,
test_helix_slivers_within_the_bodies_faces (ctest geometry.suite.sweeps.2):
a nut threaded past its bore's bottom, intersected with the cylinder inside
the bore: the merge after it ends (before, it went back and forth over the
same seams without end). Its ctest timed out without the patch on
2026-11-20, but the run of the named test alone selected no test (the
runner's MITCAD_TEST_ONLY matches the guarded() label, not the function's
name; tools/occt-port-proof.py now maps one to the other). It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0591

[0591-adaptor3d-bspline-samples-within-the-adaptors-range.patch](../opencascade/0591-adaptor3d-bspline-samples-within-the-adaptors-range.patch)

Test: geometry/tests/test_multi_turn_faces.cpp,
test_face_on_a_part_of_a_multi_turn_surface_sampled_within_it (ctest
geometry.multi_turn_faces): the bound it checks: the samples of a face on a
third of a turn of a 32-turn flank are the knot spans within the face's
range, those of the whole face its knots times its degree as before (without
the patch the part had the whole surface's samples). It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0592

[0592-shapeupgrade-unifysamedomain-planarity-once-per-surface.patch](../opencascade/0592-shapeupgrade-unifysamedomain-planarity-once-per-surface.patch)

Test: none, because the patch's only effect is speed: the face merge tests
each surface for planarity once instead of once per neighbouring face, with
the test's own results, and no count of that work is observable from outside
the merge. geometry/tests/test_multi_turn_faces.cpp,
test_merge_after_a_cut_across_a_multi_turn_thread (ctest
geometry.multi_turn_faces) checks that the merge after a box cut across a
32-turn thread keeps the cut's result: valid, the same volume and faces.

## 0593

[0593-extrema-genextps-grid-of-very-many-knot-spans-sampled-evenly.patch](../opencascade/0593-extrema-genextps-grid-of-very-many-knot-spans-sampled-evenly.patch)

Test: geometry/tests/test_multi_turn_faces.cpp,
test_projection_onto_a_multi_turn_surface_works_on_a_bounded_grid (ctest
geometry.multi_turn_faces): the bound it checks: the first projection onto a
32-turn flank evaluates fewer than 100000 grid nodes (44 x 6000 without the
patch), and each point 0.05 off the surface projects back onto its foot
within 1e-7. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0600

[0600-chfi3d-onsame-end-cut-along-its-side-by-the-face-at-the-end.patch](../opencascade/0600-chfi3d-onsame-end-cut-along-its-side-by-the-face-at-the-end.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_cut_along_its_side_by_the_face_at_an_onsame_end (ctest
geometry.suite.fillet_corners): a plate with an overhanging tongue, the
tongue's edge rounded with 1 so that its contact runs off it: the OnSame end
cut along its side by the face beyond, against a tongue wide enough for the
contact; and
test_rounding_walked_again_past_an_onsame_end_generated_by_its_edge, the
same rounding with 0.3. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 0900, 0990, 1311, which
change the same lines).

## 0620

[0620-chfi3d-contact-off-its-face-before-the-end-rolls-on-the-edge.patch](../opencascade/0620-chfi3d-contact-off-its-face-before-the-end-rolls-on-the-edge.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_rolls_on_the_far_edge_of_a_floor_narrowing_before_the_end
(ctest geometry.suite.fillet_corners): a wall on a floor that narrows before
the end, its foot rounded with 1: the volume added against the full
rounding's section and the rolling ball's. Another caller:
geometry/tests/test_fillet_occt_cases.cpp,
test_cone_edge_rounded_from_its_apex_on_a_partial_revolve (ctest
geometry.fillet_occt_cases), a contact on a cone's rim not taken for an edge
the ball rolls on. It fails without the patch at its place in the chain:
OCCT with the patches before it, without and with this one (shown
2026-11-21 by tools/occt-port-proof.py --in-chain, with the declarations
of 1662, 1701 carried, which add API the MitCad code uses).


## 0650

[0650-chfi3d-kpart-cut-by-a-long-chain-of-faces-beyond.patch](../opencascade/0650-chfi3d-kpart-cut-by-a-long-chain-of-faces-beyond.patch)

Test: geometry/tests/test_chamfer_overflow.cpp,
test_chamfer_of_a_flange_under_teeth (ctest
geometry.suite.chamfer_overflow): a flange under a ring of 12 teeth, its rim
chamfered a little past the teeth's roots and exactly up to them, by
distance and angle and by two distances, its top circle in one edge or two:
the volume against the revolved section less the teeth extended down. It
fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0651

[0651-chfi3d-walk-from-a-seam-and-extension-ending-where-it-fails.patch](../opencascade/0651-chfi3d-walk-from-a-seam-and-extension-ending-where-it-fails.patch)

Test: geometry/tests/test_fillet_kernel.cpp,
test_rounding_walked_past_its_end_across_a_seam (ctest
geometry.suite.fillet_kernel): a cylinder under two tilted planes, one arc
of its top rounded with 1, its end where the cylinder's seam lies, so that
the walk past that end starts on the seam: the stripe is made. It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses).


## 0670

[0670-chfi3d-standing-ball-sphere-in-a-gap-between-pieces.patch](../opencascade/0670-chfi3d-standing-ball-sphere-in-a-gap-between-pieces.patch)

Test: geometry/tests/test_fillet_results.cpp,
test_bowl_topped_boss_rounded_past_corners_of_the_balls_radius (ctest
geometry.suite.fillet_results): a boss topped by a steep bowl, its top's
outline rounded with 0.2 past corners of the ball's radius: each corner one
face on the ball's sphere, its area the spherical triangle's. The suite
part runs each test alone with MITCAD_TEST_ONLY since mitcad#2310 (its
tests were not guarded before, so a run alone ran them all, and at the
patch's place 0690's test failed). It fails without the patch at its place
in the chain: OCCT with the patches before it, without and with this one
(shown 2026-11-23 by tools/occt-port-proof.py --in-chain, with the
declarations of 1662, 1701 carried, which add API the MitCad code uses).

## 0671

[0671-chfi3d-kpart-chamfer-cut-across-its-width-by-faces-beyond-at-both-ends.patch](../opencascade/0671-chfi3d-kpart-chamfer-cut-across-its-width-by-faces-beyond-at-both-ends.patch)

Test: geometry/tests/test_chamfer_overflow.cpp,
test_chamfer_of_a_groove_end_wall (ctest geometry.suite.chamfer_overflow): a
groove's end wall chamfered past its flanks at three reaches: the volume
removed that of the chamfer's wedge within the groove carried on past the
wall, and one edge between the chamfer and the top through MitCad's chamfer.
It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses).

## 0680

[0680-shapeupgrade-unifysamedomain-faces-round-a-period-without-winding.patch](../opencascade/0680-shapeupgrade-unifysamedomain-faces-round-a-period-without-winding.patch)

Test: geometry/tests/test_helical_joins.cpp,
test_merge_of_a_ring_cut_through_from_end_to_end (ctest
geometry.helical_joins): two slabs of four quarters with treads on their
rim: the rim merged into one face without a seam, valid;
test_merge_of_a_whole_ring, the boundary case: a whole ring merged into one
face with a seam as before; test_stair_joined_with_its_stringers: the rim's
560 pieces merged. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0690

[0690-chfi3d-end-on-a-face-along-the-line-of-its-seam.patch](../opencascade/0690-chfi3d-end-on-a-face-along-the-line-of-its-seam.patch)

Test: geometry/tests/test_fillet_results.cpp,
test_bar_top_rounded_into_a_bore_along_its_seam (ctest
geometry.suite.fillet_results): a bar's top edge rounded where the bar
meets a tube's bore at its seam, as fused and copied. It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 1850, which changes the same lines).

## 0691

[0691-chfi3d-onsame-end-on-a-tangent-face-the-crossing-nearest-the-corner.patch](../opencascade/0691-chfi3d-onsame-end-on-a-tangent-face-the-crossing-nearest-the-corner.patch)

Test: geometry/tests/test_fillet_results.cpp,
test_recess_rim_rounded_up_to_a_rib_across_it (ctest
geometry.suite.fillet_results): a disc's recess crossed by a rib, its rim
rounded. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 0910, which changes the
same lines).

## 0692

[0692-chfi3d-kpart-contact-end-within-a-vertex-tolerance-at-the-vertex.patch](../opencascade/0692-chfi3d-kpart-contact-end-within-a-vertex-tolerance-at-the-vertex.patch)

Test: the rib's edges with the cone carried 2e-5 past their vertices
(geometry test_fillet_results); the result is the exact body's.

## 0700

[0700-chfi3d-blends-cut-by-a-narrow-face-they-roll-over.patch](../opencascade/0700-chfi3d-blends-cut-by-a-narrow-face-they-roll-over.patch)

Test: geometry/tests/test_fillet_pierced.cpp,
test_rounding_over_a_cap_notched_by_it,
test_rounding_over_a_cap_notched_by_it_with_the_caps_edges and
test_rounding_pierced_by_a_cap (ctest geometry.suite.fillet_pierced): a
corner rounded over a cap of a ball standing out of it, the cap notching the
rounding (with and without the cap's edges with the wall rounded too) and
poking through it: the volume and the cap's area left against the plain
corner's rounding with the ball fused to it. It fails without the patch at
its place in the chain: OCCT with the patches before it, without and with
this one (shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with
the declarations of 1662, 1701 carried, which add API the MitCad code
uses).


## 0720

[0720-chfi3d-face-at-the-end-passed-wholly-beyond-is-consumed.patch](../opencascade/0720-chfi3d-face-at-the-end-passed-wholly-beyond-is-consumed.patch)

Test: geometry/tests/test_fillet_corner_cases.cpp,
test_rounding_past_an_end_piece_it_consumes (ctest
geometry.fillet_corner_cases): a bar with a tilted rod standing out of its
end, its top front edge rounded with 14 past the end's corner piece: valid,
the volume removed against the bar's and the rod's within the rounding's
corner. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0721

[0721-chfi3d-onsame-prolonged-edge-own-curve-on-both-surfaces.patch](../opencascade/0721-chfi3d-onsame-prolonged-edge-own-curve-on-both-surfaces.patch)

Test: geometry/tests/test_fillet_corner_cases.cpp,
test_rounding_end_prolonged_across_a_seam (ctest
geometry.fillet_corner_cases): a post with a rib under a chamfer cone, the
rib's side rounded with 0.5 along the cone just short of the cone's seam and
turned away from it, the post a cylinder or a spline: the same cone face,
and on the cylinder the same volume removed. Other callers:
geometry/tests/test_fillet_occt_cases.cpp,
test_meridian_edge_rounded_to_the_pole_of_a_partial_revolve and
test_throat_chamfer_ends_along_a_circle_keep_their_tolerance (ctest
geometry.fillet_occt_cases). It fails without the patch at its place in
the chain: OCCT with the patches before it, without and with this one
(shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with the
declarations of 1662, 1701 carried, which add API the MitCad code uses).


## 0740

[0740-chfi3d-end-face-split-round-the-vertex-pieces-of-one-surface.patch](../opencascade/0740-chfi3d-end-face-split-round-the-vertex-pieces-of-one-surface.patch)

Test: geometry/tests/test_fillet_results.cpp,
test_bar_top_rounded_into_a_bore_split_round_the_corner (ctest
geometry.suite.fillet_results): a bar across a ring of two layers, the
upper notched over the bar and the notch filled again (the bore's wall in
three pieces round each end of the bar's top edges), as copied (each piece
on its own surface): the bar's top edges rounded. It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0741

[0741-chfi3d-kpart-contact-end-at-a-vertex-on-the-edge-to-the-spines-end.patch](../opencascade/0741-chfi3d-kpart-contact-end-at-a-vertex-on-the-edge-to-the-spines-end.patch)

Test: geometry/tests/test_fillet_results.cpp,
test_recess_rim_rounded_up_to_a_rib_across_it (ctest
geometry.suite.fillet_results): the rib's edges with the cone stopped 2e-5
short of their vertices with the slopes; the result is the exact body's.
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0742

[0742-chfi3d-guide-past-an-open-end-along-the-end-edges-own-curve.patch](../opencascade/0742-chfi3d-guide-past-an-open-end-along-the-end-edges-own-curve.patch)

Test: geometry/tests/test_sweeps.cpp,
test_thread_roots_rounded_up_to_the_nuts_faces (ctest
geometry.suite.sweeps.3): a ring's bore threaded from below the ring to
above it, the thread's root rounded up to the ring's faces: valid, the
vertices within 1e-3, the volume added that of the section the ball fills
turned over the ring's height, and each face's gain that section turned
through the angle the root rises across it. It fails without the patch at
its place in the chain: OCCT with the patches before it, without and with
this one (shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with
the declarations of 1662, 1701 carried, which add API the MitCad code
uses).

## 0750

[0750-chfi3d-thread-crest-rounded-on-both-edges-cut-by-the-ends.patch](../opencascade/0750-chfi3d-thread-crest-rounded-on-both-edges-cut-by-the-ends.patch)

Test: geometry/tests/port/p0750-thread-crest-rounded-on-both-edges.cpp
(ctest geometry.port.0750): a thread of radius 28.5 to 30.2, pitch 3, its
crest 0.81 wide, its B-spline faces cut down to the faces, between the
planes z = 3 and 6 or 9; both crest edges rounded with 0.8: built, valid,
the crest gone, one turn of the blend taking off its section swept round
the axis (the port before this patch: not built). The boundary: rounded
with 0.6, the crest kept between two blends.

## 0770

[0770-chfi3d-rolling-on-an-edge-into-the-end-vertex-where-it-vanishes.patch](../opencascade/0770-chfi3d-rolling-on-an-edge-into-the-end-vertex-where-it-vanishes.patch)

Test: geometry/tests/test_fillet_lens_overflow.cpp,
test_rounding_rolling_into_a_vertex_where_it_vanishes (ctest
geometry.fillet_lens_overflow): a block on a post as wide as the block, the
post's foot rounded with 10: valid, its quarters alike, the material added
between the torus within 30 degrees of the block's axis and the whole torus.
It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses).


## 0771

[0771-chfi3d-contour-stops-where-the-rounding-vanishes.patch](../opencascade/0771-chfi3d-contour-stops-where-the-rounding-vanishes.patch)

Test: geometry/tests/test_fillet_lens_overflow.cpp,
test_chain_through_points_where_the_rounding_vanishes (ctest
geometry.fillet_lens_overflow): after the post's foot, the block's bottom
outline rounded with 1 through the points where the foot's rounding
vanishes: valid, the two points kept, its quarters alike. Another caller:
geometry/tests/test_fillet_occt_cases.cpp,
test_crossing_cylinders_rounded_through_their_near_tangent_points (ctest
geometry.fillet_occt_cases). It fails without the patch at its place in
the chain: OCCT with the patches before it, without and with this one
(shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with the
declarations of 1662, 1701 carried, which add API the MitCad code uses).


## 0772

[0772-chfi3d-stripes-joined-across-a-corner-of-the-solid.patch](../opencascade/0772-chfi3d-stripes-joined-across-a-corner-of-the-solid.patch)

Test: geometry/tests/test_fillet_lens_overflow.cpp,
test_roundings_joined_across_a_corner (ctest geometry.fillet_lens_overflow):
a bulge on a housing's rounded corner, its foot rounded with 20, the edges
given in both orders: valid, the junctions at the height of the blend within
1e-4, nothing beyond the first side above the top, the material added alike
on both sides of the corner's bisector. It fails without the patch at its
place in the chain: OCCT with the patches before it, without and with this
one (shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with the
declarations of 1662, 1701 carried, which add API the MitCad code uses).


## 0790

[0790-chfi3d-tangent-end-face-cuts-the-blend-back-along-the-stripe.patch](../opencascade/0790-chfi3d-tangent-end-face-cuts-the-blend-back-along-the-stripe.patch)

Test: geometry/tests/test_sweeps.cpp,
test_thread_crest_rounded_down_to_a_run_out (ctest geometry.suite.sweeps.3):
a post whose thread runs out along a straight cut, its lower crest rounded
down to the run-out: valid, one small face on the run-out's flank plane
where it cuts the rounding a turn back, the crest left sharp from there down
to the run-out against the helix's length. It fails without the patch at
its place in the chain: OCCT with the patches before it, without and with
this one (shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with
1662 and the declarations of 1701 carried, which add API the MitCad code
uses).


## 0800

[0800-chfi3d-onsame-cap-points-in-one-period-of-a-stored-cylinder.patch](../opencascade/0800-chfi3d-onsame-cap-points-in-one-period-of-a-stored-cylinder.patch)

Test: geometry/tests/port/p0800-onsame-cap-points-turns-apart.cpp (ctest
geometry.port.0800): test_cap_points_two_turns_apart, the post with a rib
under a chamfer cone, its cylinder trimmed two turns below its basis's
parameters, the rib's side whose cap runs toward smaller u rounded with
0.5 up to it: valid, the cone's face and the volume removed as on the plain
post (without the patch the points stay a period apart and the box of the
cap's intersection holds a tenth of the cap at either end); and
test_cap_points_one_turn_apart, the boundary, the cylinder one turn down,
which the one-period shift already handled. It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py). The earlier test,
geometry/tests/test_fillet_crest_corners.cpp,
test_end_on_a_cylinder_stored_turns_down (ctest
geometry.fillet_crest_corners), rounds the rib's other side, whose cap
runs toward larger u and lies in that box: it passes without the patch.

## 0801

[0801-chfi3d-tangent-faces-within-ten-times-the-edge-tolerance.patch](../opencascade/0801-chfi3d-tangent-faces-within-ten-times-the-edge-tolerance.patch)

Test: geometry/tests/port/p0801-tangent-faces-edge-tolerance.cpp (ctest
geometry.port.0801): two coplanar faces whose curves of one edge lie 6.5
tolerances apart are tangent (stock: no, 1.5 tolerances), 7.5 tolerances
apart they are not; within 1.4 tolerances and the 0.001 floor as before.

## 0802

[0802-chfi3d-contact-on-its-own-edge-near-a-vertex-passes-it.patch](../opencascade/0802-chfi3d-contact-on-its-own-edge-near-a-vertex-passes-it.patch)

Test: none, because no synthetic body reaches the case (mitcad#1986): its
contact meets the spine's own edge only through the stored bodies'
micrometre deviations at the walk's steps there. A probe in
ChFi3d_OwnEdgeNearVertex found no case in the geometry tests, in OCCT's
blend and chamfer DRAW groups, or in a synthetic analogue of the thread: a
plate whose top runs tangentially into a cylinder of radius 2 (the crest's
rounding and the flank), cut by a plane 1.7 to 8 degrees from the top along
a line that crosses their tangent edge at 10 to 60 degrees, the cut's two
edges rounded with 0.5 as one contour (the blend 0.015 wide at the vertex
for 1.7 degrees). With the edge's p-curve on the cylinder moved by 10 or 20
micrometres (its tolerance raised to match), the rounding fails without
reaching the case. Verified on the stored bodies only.

## 0803

[0803-chfi3d-onsame-cap-leaving-the-prolongation-tangentially.patch](../opencascade/0803-chfi3d-onsame-cap-leaving-the-prolongation-tangentially.patch)

Test: geometry/tests/test_fillet_crest_corners.cpp,
test_groove_crest_rounded_up_to_its_floors (ctest
geometry.fillet_crest_corners): a shaft with a thread cut as a groove whose
floor's and ceiling's rims are circles 2 micrometres inside the shaft, a
crest edge rounded with 0.3 up to them: valid, the volume of the section
along the edge. It fails without the patch at its place in the chain: OCCT
with the patches before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses).


## 0820

[0820-chfi3d-stripes-joined-across-a-corner-one-face-each.patch](../opencascade/0820-chfi3d-stripes-joined-across-a-corner-one-face-each.patch)

Test: geometry/tests/test_fillet_lens_overflow.cpp,
test_joined_roundings_one_face_each (ctest geometry.fillet_lens_overflow):
the housing of 0772's test, the joined blend one face per edge (the first's
with the cut by the first side, the second's with the cuts by the corner's
rounding and the second side, sharing an edge from the first side's
junction), in both orders of the edges. It fails without the patch at its
place in the chain: OCCT with the patches before it, without and with this
one (shown 2026-11-21 by tools/occt-port-proof.py --in-chain, with the
declarations of 1662, 1701 carried, which add API the MitCad code uses).


## 0821

[0821-chfi3d-lens-rounded-on-both-edges-s-shaped-section.patch](../opencascade/0821-chfi3d-lens-rounded-on-both-edges-s-shaped-section.patch)

Test: geometry/tests/port/p0821-lens-rounded-on-both-edges.cpp (ctest
geometry.port.0821): a key of radius 5.19 on a shaft of radius 25, the
key's end a lens 0.54 high, both lens edges rounded with 2: built, valid,
no face on the lens's plane, the convex blend's contact ending on the key's
sides, the material added alike on both sides (without the patch: not
built). The boundary: the lens's convex edge alone rounded with 0.2, the
lens kept.

## 0822

[0822-chfi3d-rounding-rolling-on-both-edges-into-a-lens-end.patch](../opencascade/0822-chfi3d-rounding-rolling-on-both-edges-into-a-lens-end.patch)

Test: geometry/tests/test_fillet_lens_overflow.cpp,
test_lens_end_rolling_on_both_edges (ctest geometry.fillet_lens_overflow): a
key on a shaft whose side is split near the key's other end, that end's
convex arc rounded with 2, the split at two heights: valid, the contacts'
ends within 1e-3, no face left on the lens's plane, a face past each split
ending in a point at the lens's end, the material removed alike on both
sides within 1e-3 mm3. It fails without the patch at its place in the
chain: OCCT with the patches before it, without and with this one (shown
2026-11-21 by tools/occt-port-proof.py --in-chain, with the declarations
of 1662, 1701 carried, which add API the MitCad code uses).


## 0840

[0840-chfi3d-contour-never-takes-its-own-edge-again.patch](../opencascade/0840-chfi3d-contour-never-takes-its-own-edge-again.patch)

Test: geometry/tests/test_fillet_chain_loops.cpp,
test_contour_round_a_closed_chain_stops_where_it_turns (ctest
geometry.fillet_chain_loops): a plate whose top outline turns twice: the
contour from a selected edge takes the outline's four edges once each
(without the patch the contour does not end within the test's 30 s);
test_mitcad_fillet_round_a_closed_chain_ends, MitCad's fillet of three of
them. It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses).


## 0860

[0860-chfi3d-contour-runs-on-into-a-near-tangent-edge-asked-for.patch](../opencascade/0860-chfi3d-contour-runs-on-into-a-near-tangent-edge-asked-for.patch)

Test: geometry/tests/test_fillet_near_tangent_contour.cpp,
test_contour_runs_on_into_a_near_tangent_edge (ctest
geometry.fillet_near_tangent_contour): a block whose top meets a wall along
three edges, the first between faces 0.05 radian apart: the contour started
from the second edge is one contour of the three, valid, with the volume of
the one started from the first. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 0861

[0861-chfi3d-contact-at-the-vertex-where-the-blend-vanishes-cuts-the-own-edge.patch](../opencascade/0861-chfi3d-contact-at-the-vertex-where-the-blend-vanishes-cuts-the-own-edge.patch)

Test: geometry/tests/test_fillet_near_tangent_contour.cpp,
test_blend_thinner_than_the_vertex_tolerance (ctest
geometry.fillet_near_tangent_contour): the same block with the vertex
between the first two edges at a tolerance of 0.02, above the blend's half
width there: valid, no part of the second edge left. It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-21 by tools/occt-port-proof.py
--in-chain, with the declarations of 1662, 1701 carried, which add API the
MitCad code uses).


## 0870

[0870-chfi3d-contact-off-a-narrow-face-across-its-tangent-edge-keeps-its-surface.patch](../opencascade/0870-chfi3d-contact-off-a-narrow-face-across-its-tangent-edge-keeps-its-surface.patch)

Test: geometry/tests/test_fillet_tangent_overflow.cpp (ctest
geometry.fillet_tangent_overflow): a block whose top's edge with an arc
wall is rounded with 6, the front edge rounded with 3 (its contact on the
top's flat part, 2 wide in the middle, runs off across the rounding's
tangent edge and comes back): the rounding's faces within 1e-3 of the
ball's path along the whole edge. Fails without the patch (0.171 off: the
ball rolled on over the rim), passes with it (mitcad#1986).

## 0871

[0871-chfi3d-stripe-end-curve-follows-a-run-walked-again.patch](../opencascade/0871-chfi3d-stripe-end-curve-follows-a-run-walked-again.patch)

Test: geometry/tests/port/p0871-closing-curve-after-a-kept-face.cpp (ctest
geometry.port.0871): the block above, the chain started from the front
edge and from the right arc: one valid solid, every tolerance within the
walk's 1e-4, the same volume; the boundary, a hole of radius 9 leaving the
strip 5 wide (no run), as before. Before the patch the first two fail
(4.33 and 174, the second not valid).

## 0880

[0880-chfi3d-start-across-a-strip-the-ball-cannot-touch.patch](../opencascade/0880-chfi3d-start-across-a-strip-the-ball-cannot-touch.patch)

Test: geometry/tests/test_fillet_strip_start.cpp,
test_rounding_along_a_strip_the_ball_cannot_touch (ctest
geometry.fillet_strip_start): a ring's notch with a strip 5 micrometres wide
at one corner, the strip's edge rounded with the notch: valid, the strip
gone, the rounding's face as large as the sharp corner's within 1e-6 and the
material removed as there within 1e-4, both corners at once as each alone.
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0890

[0890-chfi3d-onsame-end-across-a-sliver-at-the-vertex.patch](../opencascade/0890-chfi3d-onsame-end-across-a-sliver-at-the-vertex.patch)

Test: geometry/tests/test_fillet_end_overflow.cpp,
test_wall_foot_ending_at_a_sliver (ctest geometry.fillet_end_overflow): a
wall cut from a half-cylinder's rounded end, its foot rounded up to a sliver
the cut's end face leaves: valid, the sliver consumed, the volume the result
without the sliver has, with the material beyond the cut, within 1e-7. It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses).


## 0891

[0891-chfi3d-bridge-over-a-gap-round-a-closed-spine-and-across-its-start.patch](../opencascade/0891-chfi3d-bridge-over-a-gap-round-a-closed-spine-and-across-its-start.patch)

Test: geometry/tests/test_fillet_end_overflow.cpp (ctest
geometry.fillet_end_overflow), test_disc_rim_over_a_rounded_hole_mouth: a
disc's bottom rim rounded over a hole whose mouth is rounded wider than
the ball reaches: built. Fails without the patch (no result), passes with
it (mitcad#1986).

## 0900

[0900-chfi3d-walked-onsame-piece-generated-by-the-spine-edges.patch](../opencascade/0900-chfi3d-walked-onsame-piece-generated-by-the-spine-edges.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_walked_again_past_an_onsame_end_generated_by_its_edge (ctest
geometry.suite.fillet_corners): a plate with an overhanging tongue whose
left edge, rounded with 0.3, is rounded with 1 so that its last piece is
walked again past its OnSame end: every face of the result on a surface no
face of the body has is generated by the edge or by one of its vertices.
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0901

[0901-chfi3d-cap-intersection-ends-where-the-walk-first-passes-the-end.patch](../opencascade/0901-chfi3d-cap-intersection-ends-where-the-walk-first-passes-the-end.patch)

Test: geometry.port.0901
(geometry/tests/port/p0901-cap-line-cut-where-it-first-passes-the-end.cpp):
the roof slab of test_fillet_eave.cpp, 20 to 30 long every 0.1, the tip's
edge, the eave's end edge and the slope's top edge on the end face rounded
with 0.9 and with 1.0 (the three blends cut each other at the tip, their
caps walked): every result one valid solid, on Linux and on Windows.
Without the patch (reverted at the port's tip) 9 and 5 of the 101 slabs
came out invalid on Linux, which of them hanging on the walk's rounding
noise; with the point rule alone, the slab 28.0 long rounded with 1.0 on
Windows.

## 0902

[0902-chfi3d-rounding-vanishing-where-its-contact-crosses-onto-a-tangent-face.patch](../opencascade/0902-chfi3d-rounding-vanishing-where-its-contact-crosses-onto-a-tangent-face.patch)

Test: none, because no synthetic body reached its failure (mitcad#1986).
Tried with the patch's code traced and with the patch reverted: the tower
on a slab of test_fillet_vanishing_own_edge.cpp (the tower's corner 0.2 to
0.8, its foot 1.5 to 5, the rounding 0.1 to 0.8, the foot's torus 0 to
0.001 off the side; 231 bodies) and the vanishing cases of
test_fillet_corners.cpp reach ChFi3d_ContactOntoTangentFaceAtEnd, but each
builds alike without the patch (the material taken within 1e-5 of it): the
next piece's walk between the tangent faces failed only on the design's
faces a few micrometres off. Verified on the stored design's bodies
(core/import/docs/f3d-rounds-3.md, a bar's rim and notch walls rounded
with 0.86).

## 0903

[0903-chfi3d-rolling-contact-crossing-onto-the-next-spine-face-goes-on.patch](../opencascade/0903-chfi3d-rolling-contact-crossing-onto-the-next-spine-face-goes-on.patch)

Test: none, because no synthetic body reached its case (mitcad#1986).
Tried with the patch's code traced: none of the OCCT-only geometry tests'
bodies (43 programs) nor 198 variants of their dimensions has a contact
leave its face onto a face along the spine before the spine's end; a step
over a floor split into a strip beside it (the step's foot rounded, the
ball rolling on the step's top edge) does, but its stripe fails with and
without the patch. Verified on the stored design's bodies
(core/import/docs/f3d-rounds-3.md, a bar's rim and notch walls rounded
with 0.86).

## 0904

[0904-chfi3d-rounding-rolled-on-past-one-end-along-a-line-or-circle.patch](../opencascade/0904-chfi3d-rounding-rolled-on-past-one-end-along-a-line-or-circle.patch)

Test: none, because no synthetic body reached its failure (mitcad#1986).
Tried with the patch's code traced and with it switched off: about 35
bodies of the OCCT-only geometry tests (test_fillet_corners, _kernel,
_eave, _lens_overflow, _rolling_ball and the port tests of 0011, 0013,
0342 and 0430) and variants of their dimensions roll to a face at the end
(ChFi3d_RolledToFaceAtEnd), but each builds alike without the patch.
Earlier tries (an ear on a tube whose top narrows to a point under a
chamfer, a chamfer edge with a kink) failed in the blend's approximation
or turned concavely. Verified on the stored design's bodies
(core/import/docs/f3d-rounds-3.md, a bar's rim and notch walls rounded
with 0.86).

## 0905

[0905-chfi3d-ball-turning-about-a-vertex-of-the-edges-it-rolls-on.patch](../opencascade/0905-chfi3d-ball-turning-about-a-vertex-of-the-edges-it-rolls-on.patch)

Test: none, because no synthetic body reached its failure (mitcad#1986).
Tried with the patch's code traced and with it switched off: about 100
bodies of the OCCT-only geometry tests and variants of their dimensions
(lens overflows, rolling balls, concave creases, the eave, the rolled end
cut) take one of its three changes (the walk back not completed forward,
only the reached end extended, a turn's transition from its neighbour),
and each builds alike without it. Verified on the stored design's bodies
(core/import/docs/f3d-rounds-3.md, a bar's rim and notch walls rounded
with 0.86).

## 0906

[0906-chfi3d-next-overflow-start-where-the-first-walk-fails.patch](../opencascade/0906-chfi3d-next-overflow-start-where-the-first-walk-fails.patch)

Test: none, because no synthetic body reached its failure (mitcad#1986).
Tried with the patch's code traced and with the retry switched off: the
plates and slots whose rims ride on each other (test_fillet_ride_corners,
_ride_against, the eave's end strip and the port tests of 0016) take the
next overflow start many times, and variants of their dimensions too, but
each builds alike without the retry (the stripes are built by the ride
when it is not made). Verified on the stored design's bodies
(core/import/docs/f3d-rounds-3.md, a bar's rim and notch walls rounded
with 0.86).

## 0907

[0907-brepblend-pivot-released-where-a-step-lands-on-the-tangency.patch](../opencascade/0907-brepblend-pivot-released-where-a-step-lands-on-the-tangency.patch)

Test: none, because no synthetic body reached its failure (mitcad#1986).
Tried with the patch's code traced and with the patch reverted: the
release within the tolerance is taken only on test_fillet_rolled_end_cut's
body with its slot 1.06 to 1.15 times as wide (31 of 103 variants), whose
rim rounding fails with and without the patch; no other of the OCCT-only
geometry tests' bodies has a step land on the tangency. Verified on the
design's replayed body only, a few micrometres off the stored one
(core/import/docs/f3d-rounds-3.md).

## 0908

[0908-chfi3d-overflow-start-sampled-finer-where-none-is-found.patch](../opencascade/0908-chfi3d-overflow-start-sampled-finer-where-none-is-found.patch)

Test: none, because no synthetic body reached its case (mitcad#1986).
Tried with the patch's code traced: about 100 bodies of the OCCT-only
geometry tests and variants of their dimensions sample the spine finely
where the coarse samples find no start, but no fine sample found one; a
step over a floor split into a strip wide enough for the ball's contact
along 0.4 of a spine 20 long, between two coarse samples, fails with and
without the patch. Verified on the design's replayed body only
(core/import/docs/f3d-rounds-3.md).

## 0909

[0909-chfi3d-rounding-vanishing-a-hair-short-of-the-vertex-it-rolls-into.patch](../opencascade/0909-chfi3d-rounding-vanishing-a-hair-short-of-the-vertex-it-rolls-into.patch)

Test: none, because no synthetic body reached its failure (mitcad#1986).
Tried with the patch's code traced and with it switched off: a lens end
rolling on both edges (test_fillet_lens_overflow), the port test of 0341
and 44 variants of test_fillet_rolled_end_cut's slot reach
ChFi3d_RollsIntoVertex, and each builds alike without the patch: their
walks reach the vertex. Verified on the design's replayed body only, where
the contacts met 0.005 before the vertex (core/import/docs/f3d-rounds-3.md).

## 0910

[0910-chfi3d-onsame-cap-from-the-contacts-point-on-the-prolonged-edge.patch](../opencascade/0910-chfi3d-onsame-cap-from-the-contacts-point-on-the-prolonged-edge.patch)

Test: geometry.port.0910
(geometry/tests/port/p0910-onsame-cap-at-the-contacts-crossing.cpp):
a pin of radius 3 on a wedge-shaped top whose sides are tangent to it,
1.1 rad from +x, its foot rounded with 0.3 and 0.5 up to the sides (the
blend nearly tangent to the face at the end, the cap rising slowly from
its edge): valid, one torus, the material added within 1.5e-5 of the
exact ring's (6.1e-6 and 3.1e-6); without the patch (reverted at the
port's tip) 6.6e-5 and 3.5e-5 off. The sides 0.7 rad from +x, rounded with
0.5, are the control (exact within 1e-7 either way). The original case's
cap crossing the edge (a wire intersecting itself) hangs on a contact
curve 1.3e-6 off its face; no synthetic body reproduced that crossing.

## 0911

[0911-chfi3d-onsame-end-where-the-face-at-the-end-goes-on-tangentially.patch](../opencascade/0911-chfi3d-onsame-end-where-the-face-at-the-end-goes-on-tangentially.patch)

Test: geometry/tests/test_fillet_tangent_end_face.cpp,
test_rounding_cut_by_a_face_going_on_tangentially_from_the_pin (ctest
geometry.fillet_tangent_end_face): a pin standing out of a wedge whose sides
are tangent to it, its foot rounded along its arc on the wedge's top: valid,
one torus, the material added the ring at the foot within the sides' planes
within 1e-6 (4.514 against 4.764 and invalid without the patch by its
documentation). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 1560, which changes the
same lines).

## 0920

[0920-chfi3d-radius-law-of-the-callers-along-a-contour.patch](../opencascade/0920-chfi3d-radius-law-of-the-callers-along-a-contour.patch)

Test: geometry/tests/test_dressup.cpp, test_fillet_spline_radius (ctest
geometry.suite.dressup): a block's edge rounded with a B-spline radius: the
material removed is 1 - pi/4 times the integral of the radius squared along
the edge, and each cross-section misses 1 - pi/4 times the radius there
squared, within 2e-3 (without the patch the contour has no radius). It
fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0930

[0930-chfi3d-rounding-riding-anothers-contact-line-against-it.patch](../opencascade/0930-chfi3d-rounding-riding-anothers-contact-line-against-it.patch)

Test: geometry/tests/test_fillet_ride_against.cpp,
test_rims_turning_opposite_ways_lower_first and
test_rims_turning_opposite_ways_upper_first (ctest
geometry.fillet_ride_against): a disc 2.5 thick whose rims' circles turn
opposite ways, rounded with 1.5 on both rims in either order: valid, the
volume the section's revolution has within 1e-6 (invalid without the patch
by its documentation). It fails without the patch at its place in the
chain: OCCT with the patches before it, without and with this one (shown
2026-11-21 by tools/occt-port-proof.py --in-chain, with the declarations
of 1662, 1701 carried, which add API the MitCad code uses).


## 0931

[0931-chfi3d-seam-run-on-to-a-corners-cut-past-its-end.patch](../opencascade/0931-chfi3d-seam-run-on-to-a-corners-cut-past-its-end.patch)

Test: geometry/tests/port/p0931-seam-run-on-to-a-corners-cut.cpp (ctest
geometry.port.0931): a roof whose ridge runs into a rod standing on it, the
rod's seam a B-spline curve on the line through the ridge's end at +y or at
-y, the ridge rounded with 1: valid, the material removed the blend's
section along the ridge less the rod within 1e-6; without the patch the
result is invalid and its volume about 213 mm3 off. The boundary cases
build the same with and without the patch: the seam away from the ridge,
and the seam left a line (which OCCT's own way prolongs).
geometry/tests/test_fillet_seam_run_on.cpp (ctest
geometry.fillet_seam_run_on), the rod sewn from a cylinder face whose seam
lies away from its surface's u = 0, fails without the patch as well.

## 0932

[0932-chfi3d-walk-cut-before-a-singular-section-ends-there.patch](../opencascade/0932-chfi3d-walk-cut-before-a-singular-section-ends-there.patch)

Test: none, because no synthetic body reaches a case where the patch
changes the result (mitcad#1986). Its condition (an end of the walk cut by
0077, on no arc) is reached at the walk's start by
test_fillet_lens_overflow's key rounded on both edges of its lens, and by a
plate whose top runs tangentially into a cylinder, cut by a plane 1.7 to 8
degrees from the top and the cut's edges rounded with 0.5; there the walk
is not taken up again, and the results are the same without the patch (56
sizes compared). A probe found no walk cut at its last end with that end
open (the design's case) in the geometry tests, in OCCT's blend and chamfer
DRAW groups, or in 81 sizes of a boss on a plate with a rod at its foot
whose own foot is rounded (the design's small torus). Verified on the
design's replayed body only.

## 0933

[0933-chfi3d-ends-joined-over-a-narrow-face-that-pokes-through-the-blend.patch](../opencascade/0933-chfi3d-ends-joined-over-a-narrow-face-that-pokes-through-the-blend.patch)

Test: geometry/tests/test_fillet_pierced.cpp,
test_post_foot_rounded_over_two_caps and
test_thin_post_foot_rounded_over_two_caps (ctest
geometry.suite.fillet_pierced): a post on a plate, the edge round its foot
broken by two caps that poke through its rounding of 0.5, posts of radius 1
and 0.8: valid, the rounded post with the caps fused within 2e-5 mm3, both
caps' lenses (invalid without the patch by its documentation). It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0940

[0940-chfi3d-rounding-over-a-narrow-face-cut-by-the-face-beyond-its-concave-edge.patch](../opencascade/0940-chfi3d-rounding-over-a-narrow-face-cut-by-the-face-beyond-its-concave-edge.patch)

Test: geometry/tests/test_fillet_narrow_cut.cpp,
test_key_rounded_up_to_end_walls (ctest geometry.fillet_narrow_cut): a key
in a spherical hollow, cut off by end walls before its side runs out, its
top edge rounded with 2: valid, the material taken off the corner outside
the rounding's cylinder inside the hollow within 1e-6 mm3 (invalid without
the patch by its documentation); test_key_rounded_over_its_side_in_a_hollow
and test_key_rounded_over_a_lower_side are the boundary cases, which build
without the patch as well. It fails without the patch at its place in the
chain: OCCT with the patches before it, without and with this one (shown
2026-11-21 by tools/occt-port-proof.py --in-chain, with the declarations
of 1662, 1701 carried, which add API the MitCad code uses).


## 0941

[0941-chfi3d-kpart-chain-crossings-in-the-period-after-the-gaps-start.patch](../opencascade/0941-chfi3d-kpart-chain-crossings-in-the-period-after-the-gaps-start.patch)

Test: geometry/tests/test_chamfer_overflow.cpp,
test_chamfer_of_a_boss_beyond_a_slanted_plate_corner (ctest
geometry.suite.chamfer_overflow): a boss under a plate corner with a slanted
side, its foot chamfered past the corner, the boss's seam at 120, 150, 180
and 300 degrees, 13 and 15 long: valid, the boss with the chamfer's cone cut
by the plate's outline within 1e-8 relative (invalid without the patch for
the seams from 120 to 180 degrees by its documentation; 300 degrees the
boundary case). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0942

[0942-shapefix-intersecting-wires-cut-reported-only-where-cut.patch](../opencascade/0942-shapefix-intersecting-wires-cut-reported-only-where-cut.patch)

Test: geometry/tests/test_shapefix.cpp, test_arcs_meeting_in_two_wires
(ctest geometry.suite.shapefix): a planar face of two wires whose arcs of
one circle meet end to end at a vertex the wires share, at three placements:
the repair of intersecting wires returns with nothing to repair (it never
returns without the patch by its documentation). Its ctest timed out
without the patch on 2026-11-20, but the run of the named test alone
selected no test (the runner's MITCAD_TEST_ONLY matches the guarded()
label, not the function's name; tools/occt-port-proof.py now maps one to
the other). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0950

[0950-chfi3d-roundings-consuming-the-face-between-them-meet-on-one-contact-edge.patch](../opencascade/0950-chfi3d-roundings-consuming-the-face-between-them-meet-on-one-contact-edge.patch)

Test: geometry/tests/test_fillet_consumed_strip.cpp (ctest
geometry.fillet_consumed_strip): windows in a ring wall 3 thick, each
window's floor and ceiling rounded with 1.5 on both edges: built, the
material taken within 2e-3 of the section's. Fails without the patch
(not done), done with it (mitcad#1986).

## 0951

[0951-chfi3d-rounding-ending-at-a-step-back-of-its-wall-closes-with-a-torus.patch](../opencascade/0951-chfi3d-rounding-ending-at-a-step-back-of-its-wall-closes-with-a-torus.patch)

Test: geometry/tests/test_fillet_step_end.cpp (ctest
geometry.fillet_step_end): a block's edge rounded up to a vertex where its
wall steps back by micrometres, the step below or above the edge, one rim
or both, as built or mirrored: valid, one quarter torus per rounding's
end, the material taken that of the rim's section. These fail without the
patch (invalid, or 2100 taken) and pass with it; the cases with the
body's faces in the reverse order and the second rounding run on across
the step pass only with 0962 (mitcad#1986).

## 0960

[0960-projlib-projection-boundary-bisection-stops-at-adjacent-parameters.patch](../opencascade/0960-projlib-projection-boundary-bisection-stops-at-adjacent-parameters.patch)

Test: geometry/tests/test_projection_boundary.cpp,
projection_crossing_plane_bounds (ctest geometry.projection_boundary): a
segment's normal projection onto a bounded plane, its parameters offset by
1 and by 1e6 so that no representable parameter lies at either crossing of
the plane's edges: the bracket ends at adjacent parameters and the
projected points lie on the plane. Without the patch the projection's
constructor does not return (DichExactBound). It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py).

## 0961

[0961-chfi3d-rounding-vanishing-where-its-walk-stopped-on-its-own-edge.patch](../opencascade/0961-chfi3d-rounding-vanishing-where-its-walk-stopped-on-its-own-edge.patch)

Test: geometry/tests/test_fillet_vanishing_own_edge.cpp,
test_rounding_vanishing_where_its_faces_come_tangent (ctest
geometry.fillet_vanishing_own_edge): the tower on a slab flush with its
side, the foot's torus moved 0.0002 to 0.001 off the side within its
edges' tolerance, the side's edge rounded with 0.3 up to the vertex where
the three faces are tangent: valid, the blend ending in a point there, the
vertices' tolerances below 0.01, the material taken within 0.5 % of the
exact body's; the port's notes record the fillet failing without the
patch. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 0962

[0962-chfi3d-step-end-torus-cut-on-its-direct-frame-and-only-at-a-recess.patch](../opencascade/0962-chfi3d-step-end-torus-cut-on-its-direct-frame-and-only-at-a-recess.patch)

Test: geometry/tests/test_fillet_step_end.cpp (ctest
geometry.fillet_step_end): test_rounding_ends_at_a_step_back_of_its_wall
with the body's faces in the reverse order (the shape invalid or the
fillet failing before), and test_second_rounding_runs_on_across_the_step
(the cylinder run on to the first rounding's torus, no torus of its own).
It fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 0970

[0970-chfi3d-chain-past-a-wall-corner-tighter-than-the-ball-vertex-blend-and-torus.patch](../opencascade/0970-chfi3d-chain-past-a-wall-corner-tighter-than-the-ball-vertex-blend-and-torus.patch)

Test: geometry/tests/test_fillet_tight_corner.cpp (ctest
geometry.fillet_tight_corner): a block's bottom outline rounded with a
radius larger than its vertical corners' rounding, the front wall square
to the bottom or leaning: valid, the vertex blend and torus at each
corner. Fails without the patch, passes with it (mitcad#1986).

## 0980

[0980-chfi3d-rounding-across-slight-creases-of-its-faces-at-one-section.patch](../opencascade/0980-chfi3d-rounding-across-slight-creases-of-its-faces-at-one-section.patch)

Test: geometry/tests/test_fillet_concave_crease.cpp,
test_rounding_over_a_concave_crease_of_its_side (ctest
geometry.fillet_concave_crease): a box's top front edges rounded with 1
and 0.5 where the front turns outward by 3 and 5 degrees about a slanted
crease, the edges given either way: valid, no edge tolerance above 1e-4,
the material taken within 0.5 % of the two edges' sections; the port's
notes record the fillet failing at a vertex without the patch. It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-21 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses). It covers StartSol's turn
about the crease only: the other four changes (StopChainsAtTurns, the
guide's stretch, KinkIntoEndFace, PerformSetOfSurfOnElSpine) have no
synthetic test, as no synthetic body reproduced the creases' micrometre
jumps at one section; they are verified on the design's body only (left,
mitcad#1986).


## 0990

[0990-chfi3d-rolled-end-cut-by-the-faces-at-the-end-past-the-balls-tangency.patch](../opencascade/0990-chfi3d-rolled-end-cut-by-the-faces-at-the-end-past-the-balls-tangency.patch)

Test: geometry/tests/test_fillet_rolled_end_cut.cpp (ctest
geometry.fillet_rolled_end_cut): a slot's end wall over a groove, the
groove's rim on the wall's strip rounded with 10 (rolling on the strip's
top edges), cut by the notch walls at both ends: built and valid. Fails
without the patch, passes with it (mitcad#1986).

## 1000

[1000-chfi3d-chamfer-across-a-slight-concave-crease-over-a-narrow-face-grown-faces-continued.patch](../opencascade/1000-chfi3d-chamfer-across-a-slight-concave-crease-over-a-narrow-face-grown-faces-continued.patch)

Test: geometry/tests/port/p1000-chamfer-across-a-concave-crease.cpp
(ctest geometry.port.1000): a prism's top of a plane and the extrusion of
a straight B-spline meeting at a concave crease of 2 or 4 degrees, the
chain of its edges with the front chamfered by 1: built, valid, the
chamfer's triangle along the chain (without the patch's crease change: not
built, the chamfer taken for a blend rolling on the crease). The boundary:
no crease, as stock. The narrow-face and grown-face changes have no port
test of their own (the design's body only).

## 1001

[1001-chfi3d-guide-stretch-only-where-the-spine-turns-at-the-vertex.patch](../opencascade/1001-chfi3d-guide-stretch-only-where-the-spine-turns-at-the-vertex.patch)

Test: none, because no synthetic body reached its failure (mitcad#1986).
The stretch this patch drops fails the walk where the next edge's analytic
piece starts a hair past a vertex of tangent edges, as the design's faces
split it. Tried with the patch's code traced and with the patch reverted:
the tower on a slab of test_fillet_vanishing_own_edge.cpp (0961's
synthetic case; the tower's corner 0.2 to 0.8, its foot 1.5 to 5, the
rounding 0.1 to 0.8, the foot's torus 0 to 0.001 off the side; 231 bodies)
and a bore with a round pocket at its rim drop the stretch, but each
builds alike with it (the material taken within 1e-5). Verified on the
design's replayed and stored bodies only, where the rounding fails with
0980 and without 1001.

## 1010

[1010-chfi3d-kpart-closed-rim-gap-round-to-its-own-first-edge.patch](../opencascade/1010-chfi3d-kpart-closed-rim-gap-round-to-its-own-first-edge.patch)

Test: geometry/tests/test_chamfer_rim_cutout.cpp,
test_rim_chamfered_over_a_pocket (ctest geometry.chamfer_rim_cutout): a
disc's rim of radius 85 in two arcs split where a pocket from below breaks
through its wall 1 under the top, chamfered 1 on the top and 2 down the
wall, either arc first and the seam at either corner: valid, the material
taken within 1e-6 of the body's part outside the chamfer's revolved
section, the pocket's ceiling cut back to 84.5 from the axis; the two-arc
chamfers fail without the patch (the port's notes). The rim as one closed
edge, which builds without the patch as well, is the boundary case beside
it. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1011

[1011-chfi3d-intersection-at-end-across-the-end-faces-seam-in-one-period.patch](../opencascade/1011-chfi3d-intersection-at-end-across-the-end-faces-seam-in-one-period.patch)

Test: none, because no synthetic body reached its case (mitcad#1986).
Tried with the patch's code traced: 1080 plates with a bore of radius 20
whose top rim is rounded with 0.5 or 1 where a round pocket (radius 3 or
6, 0.3 to 5 deep) cuts into it, before or after the pocket, the bore's
seam put at a pocket's side and 0.003 and 0.03 rad either side of it, and
the pocket's top rim rounded with 1 or 0.5. None shifts an end into the
other end's period: OCCT's contour runs on from the pocket's top edge
tangentially along its edges with the torus and the bore, so the torus is
never the face at the end. Earlier tries (a bore's rim cut by a pocket or
a slot, the pocket's or slot's rim rounded up to the torus) built or
failed alike with and without the patch. Verified on the design's replayed
and stored bodies only, where the seam end's rounding gives a body of
1e12 mm3 without it.

## 1020

[1020-chfi3d-kink-into-a-face-going-on-smoothly-into-the-end-face-also-at-a-pieces-start.patch](../opencascade/1020-chfi3d-kink-into-a-face-going-on-smoothly-into-the-end-face-also-at-a-pieces-start.patch)

Test: geometry/tests/test_fillet_crease_into_end_walls.cpp,
test_drafted_peg_rounded_into_a_block (ctest
geometry.fillet_crease_into_end_walls): a square peg drafted by 3, 4 and 5
degrees hanging from a plate into a block whose walls go on from its sides
at the creases and whose vertical corners are rounded with 0.4, the peg's
corner edges given either way and rounded with 0.5: valid, the block's
corner cylinders from its foot up past the creases, the material taken
within 1e-3 mm3 of the peg rounded alone with the block carried on up into
it; the port's notes record the fillet failing with 0980 and without this
patch. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1030

[1030-geomfill-guide-trihedron-ac-second-derivative-of-the-binormal.patch](../opencascade/1030-geomfill-guide-trihedron-ac-second-derivative-of-the-binormal.patch)

Test: geometry/tests/test_sweeps.cpp, test_sweep_frames (ctest
geometry.suite.sweeps.3): an 8 x 2 bar swept 50 along a line and turned a
quarter turn by an auxiliary spine, with curvilinear equivalence
(GuideTrihedronAC, this patch) and without it (the plane rule,
GuideTrihedronPlan, the other caller, unchanged): the volume 800 within
1e-7 and the end cap's extents the turned bar's. Without the patch the
curvilinear sweep held 784.9. It fails without the patch (shown 2026-11-20
by tools/occt-port-proof.py).

## 1040

[1040-shapeupgrade-unifysamedomain-signed-angle-to-a-reversed-axis.patch](../opencascade/1040-shapeupgrade-unifysamedomain-signed-angle-to-a-reversed-axis.patch)

Test: geometry/tests/test_merge_reversed_axis.cpp,
test_sector_behind_with_its_axis_reversed and
test_sector_ahead_with_its_axis_reversed (ctest
geometry.merge_reversed_axis): a quarter of a cylinder fused with a sector
before or after it mirrored top to bottom (its axis down, its frame
indirect), either first: the merge valid, one cylindrical face, the volume
kept. Without the patch the two faces stay apart wherever the other face's
X direction lies behind the reference face's (the sector behind with the
quarter first, the sector ahead with the sector first); the other two
orders, whose angle lies between 0 and pi, are the boundary cases the
patch leaves as they were. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1050

[1050-shapefix-self-intersecting-wire-edges-read-once.patch](../opencascade/1050-shapefix-self-intersecting-wire-edges-read-once.patch)

Test: geometry/tests/port/p1050-self-intersecting-wire-many-edges.cpp
(ctest geometry.port.1050): one crossing of a closed wire is split into 4
new edges at one vertex, two crossings far apart are both split (the edges
are read again after the first fix), a polygon without crossings is left as
it is, and a wire of 6 000 edges with one crossing is fixed within 6 s
(reading each pair's edges by number from the sequence grew with the cube
of the edges).

## 1060

[1060-chfi3d-under-anothers-ball-not-past-its-other-face.patch](../opencascade/1060-chfi3d-under-anothers-ball-not-past-its-other-face.patch)

Test: geometry/tests/test_fillet_corner_past_wall.cpp,
test_roundings_run_past_the_corner_with_occt_alone (and, through MitCad's
fillet, test_roundings_run_past_the_corner_by_name; ctest
geometry.fillet_corner_past_wall): a wall with a slot whose floor is flush
with a ledge's top, the slot floor's edge and the ledge's edge along the
wall rounded with 1 at the convex corner left sharp: valid, the material
added and each rounding's face against the two quarter cylinders running
on past the corner into each other. Without the patch 0700 drops the
ledge's stripe and its rounding is missing. It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py, reverted together with
1726, which changes the same lines).

## 1070

[1070-brepmesh-delabella-exact-signs.patch](../opencascade/1070-brepmesh-delabella-exact-signs.patch)

Test: geometry/io/tests/test_io.cpp (test_nearly_cocircular_planes,
mitcad#846: the face with the concave arc meshed at every radius of the
sweep; it crashed without the patch).

## 1080

[1080-brepmesh-delabella-exact-signs-in-place.patch](../opencascade/1080-brepmesh-delabella-exact-signs-in-place.patch)

Test: none, because its only effect is speed: the signs, and so the
triangulations, are 1070's, which test_nearly_cocircular_planes
(geometry/io/tests/test_io.cpp, ctest geometry.io) checks, and no test
bounds Delabella's time (cli.perf_holes_mesh's limit catches only a plane
gone to BRepMesh's default insertion, not the factor of two this patch
saves).

## 1081

[1081-brepmesh-face-checker-wire-boxes.patch](../opencascade/1081-brepmesh-face-checker-wire-boxes.patch)

Test: none for its time, because its only effect is speed and no test
bounds the checker's time (cli.perf_holes_mesh's limit catches only a
plane gone to BRepMesh's default insertion). The crossings it must keep
finding are checked by test_face_checker_wire_boxes
(geometry/io/tests/test_io.cpp, ctest geometry.io): a square with 64 holes
apart has no crossing segments, and with two holes that overlap each other
exactly their two edges cross.

## 1082

[1082-brepmesh-delabella-cocircular-start-beyond-any-contour-edge.patch](../opencascade/1082-brepmesh-delabella-cocircular-start-beyond-any-contour-edge.patch)

Test: geometry/io/tests/test_io.cpp, test_rectangular_planes (ctest
geometry.io; and cli.perf_holes_mesh, no plane of the design redone): a 10
mm cube, a 5 mm cube placed as the perf_holes design's notch walls and
four rectangles of the sweep that failed, meshed by both of the view's
Delabella tiers: every plane triangulated, its triangles covering its
boundary, none made again. Without the patch the test fails where long
double has 53 bits (MSVC, ARM) and passes with 64 (GCC and Clang on
x86-64), so its before/after run needs such a platform or a build whose
long double has 53 bits (mitcad#1986).

## 1090

[1090-chfi3d-sharp-edge-limited-only-by-a-common-point-on-it.patch](../opencascade/1090-chfi3d-sharp-edge-limited-only-by-a-common-point-on-it.patch)

Test: geometry/tests/test_chamfer_sharp_corner.cpp,
test_convex_and_concave_bevel_at_a_sharp_edge (ctest
geometry.chamfer_sharp_corner): both ends of a bracket, either edge first,
and the floor's whole top face: the corner within the bevels' reach, a few
thousand triangles at most, the ends' volumes equal; one end fails without
the patch (the port's notes). It fails without the patch (shown 2026-11-20
by tools/occt-port-proof.py, reverted together with 1801, 1803, which
change the same lines).

## 1100

[1100-chfi3d-two-corner-common-points-at-a-vertex-of-the-pivot.patch](../opencascade/1100-chfi3d-two-corner-common-points-at-a-vertex-of-the-pivot.patch)

Test: geometry/tests/test_dressup_takes_face.cpp,
test_tray_corner_with_occt_alone (ctest
geometry.suite.dressup_takes_face): a tray's corner chamfered and rounded
with OCCT's dressups alone, which failed in
TopOpeBRepDS_DataStructure::Point without the patch; and test_tray and
test_tray_rounded_all_round through MitCad's dressups: the tray's four
edges at their size, the volumes exact. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 1110

[1110-chfi3d-first-end-snapped-onto-a-closed-edges-vertex-on-a-periodic-face.patch](../opencascade/1110-chfi3d-first-end-snapped-onto-a-closed-edges-vertex-on-a-periodic-face.patch)

Test: geometry/tests/test_dressup_takes_face.cpp,
test_hole_rim_with_occt_alone (ctest geometry.suite.dressup_takes_face): a
blind hole's rim chamfered and rounded with 2 by OCCT alone, valid and
with the exact volume, failing without the patch (the port's notes); and
test_hole_rim_at_its_size through MitCad's dressups: at their size, no
info, the volumes within 1e-6. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py, reverted together with 1560, 1730,
which change the same lines).

## 1120

[1120-chfi3d-guide-wholly-past-an-end-of-the-spine-is-its-straight-extension.patch](../opencascade/1120-chfi3d-guide-wholly-past-an-end-of-the-spine-is-its-straight-extension.patch)

Test: geometry/tests/test_chamfer_end_against_chamfer.cpp,
test_chamfer_of_a_chain_ending_against_a_chamfer (ctest
geometry.chamfer_end_against_chamfer): a plate's chain chamfered from one
diagonal with OCCT alone: one contour of five edges, valid and closed,
symmetric about the plate's plane, the volume removed pinned; it fails
without the patch in TopoDS_Builder::Add (the port's notes). The other
caller: test_facade_chamfers_ending_against_a_chamfer, the same through
MitCad's chamfer and a corner of two chamfers. It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py, reverted together with
1440, 1601, 1790, which change the same lines).

## 1130

[1130-brepoffset-offsets-tangent-past-the-end-keep-the-branch-along-the-crease.patch](../opencascade/1130-brepoffset-offsets-tangent-past-the-end-keep-the-branch-along-the-crease.patch)

Test: geometry/tests/test_shell_loft_to_circle.cpp,
test_occt_shells_of_octagon_lofts and
test_occt_shells_of_a_square_loft_to_a_circle_of_radius_20 (ctests
geometry.shell_loft_to_circle.octagon_24_5_seam_0, octagon_24_5_seam_22_5,
octagon_21_7_seam_0, octagon_21_7_seam_22_5 and square_20):
octagon-to-circle lofts of radii 24.5 and 21.7, the seam on a side and on
a corner, shelled 0.5 to 3 mm with both ends removed, and a square to a
circle of radius 20 shelled 0.5 to 2 mm, with OCCT alone: valid single
solids, both ends open, the walls the thickness within 1e-6 mm; the port's
notes record every OCCT-only case failing without the patch. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted
together with 1190, 1480, which change the same lines).

## 1140

[1140-chfi3d-cut-to-end-past-the-vertex-along-a-tangent-edge.patch](../opencascade/1140-chfi3d-cut-to-end-past-the-vertex-along-a-tangent-edge.patch)

Test: geometry/tests/test_fillet_corners.cpp (ctest
geometry.suite.fillet_corners):
test_step_foot_rounded_where_a_bar_rounding_tangent_to_the_floor_ends_it,
the step beside the rounded bar (failing without the patch), and
test_step_foot_on_a_rounding_ended_by_a_rounding_tangent_to_it, beside a
notch rounded tangent to the block's rounding, both against the block
alone rounded less what lies in the bar's profile; and
test_tab_top_rounded_up_to_its_side_roundings_tangent_to_a_curved_wall, a
tab's top rounded concave up to its side roundings tangent to a curved
wall, against the material under the rounding's torus within the tab's
outline.

## 1160

[1160-brepoffset-images-meeting-along-a-tangential-edge-meet-where-they-cross-its-offset.patch](../opencascade/1160-brepoffset-images-meeting-along-a-tangential-edge-meet-where-they-cross-its-offset.patch)

Test: geometry/tests/test_offset_loft_circle_seam.cpp,
test_square_to_a_circle_seam_on_a_side and
test_octagon_to_a_circle_seam_on_a_side (ctest
geometry.offset_loft_circle_seam): a square to a circle of radius 15
offset 0.5 to 3 mm and of radius 14.142 offset 2 mm, the octagon to a
circle of radius 21.7 offset 0.5 to 3 mm, the seam on a side, closed
inward offsets with OCCT alone: valid single solids, the walls the
thickness within 1e-6 mm, the volume within bounds from the areas. The
port's notes record every case but radius 15 at 0.5 mm failing without the
patch (that one is the boundary case). It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 1170

[1170-chfi3d-cut-to-end-past-the-vertex-along-faces-meeting-on.patch](../opencascade/1170-chfi3d-cut-to-end-past-the-vertex-along-faces-meeting-on.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_round_a_corner_cut_by_a_wall_standing_across_it (ctest
geometry.suite.fillet_corners): a block lowered on one side of a wall
across its rounded corner, against the lower block alone rounded on the
far side of the wall's plane (failing without the patch).

## 1180

[1180-brepoffset-extended-edge-keeps-a-pcurve-on-its-curve-where-reprojection-fails.patch](../opencascade/1180-brepoffset-extended-edge-keeps-a-pcurve-on-its-curve-where-reprojection-fails.patch)

Test: geometry/tests/port/p1180-extended-edge-keeps-its-curve-on-a-face.cpp
(ctest geometry.port.1180): an edge from (0, 0, 0) to (10, 0, 0) made of
the line v = 0 on the plane z = 0 trimmed to u from -2 to 12 and on a
B-spline surface over u from -4 to 14, extended by ExtentEdge.
test_curve_kept_where_the_surfaces_part_beyond_the_edge: the B-spline is
that plane from u = 0 on and a ramp before it, so the rebuilt curve's
first point lies 0.485 mm off it and the projection fails; the extended
edge stays same-parameter with its tolerance 1e-7, its curve on the
B-spline on the 3D curve within 1e-6 mm over the edge's range.
The boundaries: test_curve_projected_where_the_surfaces_agree (the
B-spline flat, the projection succeeds) and
test_curve_not_kept_where_the_surfaces_part_within_the_edge (the ramp
ending at u = 5: not kept, the edge not left exact). It fails without the
patch (shown 2026-11-20 by tools/occt-port-proof.py). The loft offsets in
geometry/tests/test_offset_loft_circle_seam.cpp
(test_square_to_a_circle_of_radius_20_seam_on_a_side) pass without it.

## 1190

[1190-brepoffset-offsets-tangent-inside-keep-the-branch-and-its-continuation.patch](../opencascade/1190-brepoffset-offsets-tangent-inside-keep-the-branch-and-its-continuation.patch)

Test: geometry/tests/test_shell_loft_to_circle.cpp, test_occt_lean_in
(ctests geometry.shell_loft_to_circle.lean_in_8, lean_in_10 and
lean_in_14_1): a square to circles of radii 8, 10 and 14.1 shelled 0.5 to
3 mm with both ends removed, the volume against the one-end shells and the
closed offset; and geometry/tests/test_offset_loft_circle_seam.cpp,
test_lean_in_seam_on_a_side (ctest
geometry.offset_loft_circle_seam_lean_in): walls leaning in towards the
circle, closed inward offsets with OCCT alone, invalid with 1130 and 1160
alone, valid single solids of the seam-on-a-corner loft's volume with this
patch. The vertex tolerance:
test_octagon_to_a_circle_of_radius_24_5_seam_on_a_side. It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 1480, which changes the same lines).

## 1200

[1200-brepoffset-c2-extension-across-a-tangential-side.patch](../opencascade/1200-brepoffset-c2-extension-across-a-tangential-side.patch)

Test: geometry/tests/test_offset_loft_circle_seam.cpp,
test_square_to_a_circle_of_radius_20_seam_on_a_side at 3 mm and
test_octagon_to_a_circle_of_radius_24_5_seam_on_a_side at 1 and 2 mm
(ctest geometry.offset_loft_circle_seam): edges within 0.001 mm, the
volume that of the seam-on-a-corner loft's offset within 1e-5 (3e-5 for
the octagon); the port's notes record the square at 3 mm failing without
the patch on the edges. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1210

[1210-brepfilletapi-rounding-over-slot-mouths-touching-the-top-along-a-crest.patch](../opencascade/1210-brepfilletapi-rounding-over-slot-mouths-touching-the-top-along-a-crest.patch)

Test: geometry/tests/port/p1210-slot-mouths-on-a-rounded-lip.cpp (ctest
geometry.port.1210): a lip of radius 93.5, its lower outer edge rounded
with 5, eight fins 1.5 thick standing out 1.5 round it, each one's top a
cylinder of radius 0.75 whose crest touches the rim at one point; the rim
rounded with 2.5: built, valid, one solid, taking off the rounding's
section round the rim and at most 5 % more (without the patch: not
built). The boundary: rounded with 0.5 and 0.7, smaller than the fins'
tops: not a mouth, OCCT's corners as before.

## 1220

[1220-chfi3d-ball-pivots-about-a-crease-at-the-end.patch](../opencascade/1220-chfi3d-ball-pivots-about-a-crease-at-the-end.patch)

Test: geometry/tests/test_fillet_corners.cpp,
test_rounding_pivots_about_a_crease_over_a_sliver (ctest
geometry.suite.fillet_corners): a bar with a concave arc meeting it with a
small overlap and its rim rounded, the end face's outline rounded (no
result without the patch).

## 1230

[1230-chfi3d-rolls-before-a-blend-curving-away-as-fast-as-the-ball.patch](../opencascade/1230-chfi3d-rolls-before-a-blend-curving-away-as-fast-as-the-ball.patch)

Test: geometry/tests/test_fillet_roll_over_blend.cpp,
test_rounding_rolls_on_the_far_edges and
test_rounding_rolls_on_the_far_edges_other_version (ctest
geometry.fillet_roll_over_blend): the cases fillet-roll-over-blend and
fillet-roll-over-blend-2, the inner rim rounded with OCCT alone: valid, of
the expected volume and faces, within 2 um of the stored body round the
rim; the port's notes record it failing without the patch. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted
together with 0173, 1370, 1430, 1580, which change the same lines). The
test reads a stored case (MITCAD_GEOMETRY_CASES), not a synthetic body: a
synthetic test is still wanted (mitcad#1986).

## 1240

[1240-chfi3d-short-end-walked-on-until-both-contacts-leave.patch](../opencascade/1240-chfi3d-short-end-walked-on-until-both-contacts-leave.patch)

Test: geometry/tests/test_fillet_step_corner.cpp,
test_synthetic_corner_occt (ctest geometry.fillet_step_corner): a
synthetic step corner rounded with OCCT alone at 0.5, 0.75 and 1 against
the body of planes (without the patch 0.75 folds at the top vertex and 1
is invalid); and test_synthetic_corner, the same through MitCad's fillet,
also after its floor is rounded. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py, reverted together with 1430, 1580,
which change the same lines).

## 1270

[1270-shapefix-shell-closed-shell-found-as-faces-are-added.patch](../opencascade/1270-shapefix-shell-closed-shell-found-as-faces-are-added.patch)

Test: none, because its only effect is speed: the shell is found closed at
the same face as before (the same test of the edges used an odd number of
times, kept as the faces are added), and no test bounds the time. Its
results are those of the face merge's tests and of
geometry.join_many_bodies (the union of a reported design's 257 bodies,
MITCAD_MITCAD_REPORTED), whose results are OCCT's without the patch.

## 1280

[1280-bopalgo-stick-vertices-of-face-pairs-by-an-index.patch](../opencascade/1280-bopalgo-stick-vertices-of-face-pairs-by-an-index.patch)

Test: geometry/tests/port/p1280-stick-vertices-many-bodies.cpp (ctest
geometry.port.1280): the union of 48 stars of 40 square pyramids whose
apexes meet (37 440 VV interferences, about 600 000 pairs of faces) and two
overlapping boxes is valid, 1921 solids with the volume of its parts, its
intersections within 25 s (the scan of every interference for each pair of
faces grew with their square: 49 s without the patch, 7.5 s with it); a row of 12 touching boxes, each pierced by
a cylinder, is one valid solid with the volume of its parts;
geometry/tests/test_join_many_bodies.cpp (results).

## 1281

[1281-brepclass3d-edge-faces-mapped-only-where-the-line-meets-an-edge.patch](../opencascade/1281-brepclass3d-edge-faces-mapped-only-where-the-line-meets-an-edge.patch)

Test: none, because its only effect is speed: the edge-to-faces map is
made the first time it is read instead of for every point, the
classification the same, and no test bounds its time. Its results are
those of geometry.classifier_far_plane and geometry.join_many_bodies,
OCCT's without the patch.

## 1282

[1282-brepclass3d-face-intersectors-made-beside-each-other.patch](../opencascade/1282-brepclass3d-face-intersectors-made-beside-each-other.patch)

Test: geometry/tests/test_classifier_pool_joins.cpp (joins with the
classifier's intersectors made beside each other, on several threads at
once, mitcad#1553). The speed itself: none, because the gain is the
machine's cores (a parallel loop over the faces); no bound holds on a
machine with one.

## 1283

[1283-intcurvesface-only-points-in-range-classified.patch](../opencascade/1283-intcurvesface-only-points-in-range-classified.patch)

Test: none, because its only effect is speed: the points kept are the same
(those in the segment's range, now the only ones classified), and no test
bounds the time. Its results are those of geometry.classifier_far_plane
and geometry.join_many_bodies, OCCT's without the patch.

## 1284

[1284-bopalgo-buildersolid-solid-edges-mapped-once-for-its-holes.patch](../opencascade/1284-bopalgo-buildersolid-solid-edges-mapped-once-for-its-holes.patch)

Test: none, because its only effect is speed: the solid's edges are mapped
once for all the holes tried in it instead of once per hole, the holes
placed the same, and no test bounds the time. Its results are those of
geometry.join_many_bodies (76 shells, the cavities placed), OCCT's without
the patch.

## 1285

[1285-brepclass3d-point-classified-along-fixed-lines-once-the-segments-run-out.patch](../opencascade/1285-brepclass3d-point-classified-along-fixed-lines-once-the-segments-run-out.patch)

Test: geometry/tests/test_classifier_far_plane.cpp,
test_point_in_the_plane_of_a_far_face and test_block_with_the_two_cavities
(ctest geometry.classifier_far_plane): a thin pointed cavity whose bottom
face, last in its shell, lies in the plane of a box-shaped cavity's bottom
beyond its point: points of that bottom classify IN the pointed cavity's
solid (OUT without the patch), points inside it OUT, and the checker
accepts the block (BRepCheck_SubshapeNotInShape without the patch); and
test_point_in_the_plane_of_a_flat_shell and test_cut_by_a_flat_shell, the
boundary of the fixed-line rule: points in a flat shell's plane outside it
classify OUT, and a pocketed block cut by it keeps its volume. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1290

[1290-chfi3d-cut-to-end-past-the-vertex-not-on-a-face-closed-round-its-axis.patch](../opencascade/1290-chfi3d-cut-to-end-past-the-vertex-not-on-a-face-closed-round-its-axis.patch)

Test: geometry/tests/test_fillet_tangent_end_face.cpp,
test_rounding_of_a_pin_on_a_plate_end_round_most_of_it (ctest
geometry.fillet_tangent_end_face): the pin on a plate's end whose seam
runs along a side at V2: valid, one torus,
the material added within 1e-6 of the ring at the foot within the sides'
planes (without the patch: not valid, two tori).

## 1300

[1300-chfi3d-concave-crease-beyond-judged-at-the-edge.patch](../opencascade/1300-chfi3d-concave-crease-beyond-judged-at-the-edge.patch)

Test: geometry/tests/test_fillet_fin_top_creases.cpp (ctest
geometry.fillet_fin_top_creases; with patch 1301; without this patch the
rounding fails on both contours).

## 1301

[1301-chfi3d-walk-ends-where-a-contact-reaches-the-next-analytic-piece.patch](../opencascade/1301-chfi3d-walk-ends-where-a-contact-reaches-the-next-analytic-piece.patch)

Test: geometry/tests/test_fillet_fin_top_creases.cpp, the rounding of a
reported fin's top across creases of 1.4 and 2.2 degrees: built and closed,
the stored body after it on the result within 0.01 mm (with patch 1300
alone the result has an edge on one face only).

## 1302

[1302-chfi3d-narrow-parts-pieces-generated-by-their-spine-edges.patch](../opencascade/1302-chfi3d-narrow-parts-pieces-generated-by-their-spine-edges.patch)

Test: geometry/tests/test_fillet_fin_top_creases.cpp, a reported fin's top
chamfered over side strips narrower than the chamfer: every chamfer face is
generated by a contour edge (five of ten without the patch).

## 1310

[1310-chfi3d-kpart-blend-runs-past-the-end-where-the-end-face-goes-on-tangentially.patch](../opencascade/1310-chfi3d-kpart-blend-runs-past-the-end-where-the-end-face-goes-on-tangentially.patch)

Test: geometry/tests/test_fillet_fin_end.cpp, the reported fin's top
rounded up to its end face: about the fin's end the result lies on the
stored body within 0.002 mm both ways (0.011 off without the patch).

## 1311

[1311-chfi3d-onsame-end-walked-on-past-the-end-of-an-extruded-stretch.patch](../opencascade/1311-chfi3d-onsame-end-walked-on-past-the-end-of-an-extruded-stretch.patch)

Test: geometry/tests/test_fillet_fin_end.cpp, the reported fin's end
rounded along its sides on the body MitCad builds: the rounding builds,
closed, and the stored body after it lies on the result within 0.01 mm
round the end (the rounding fails without the patch).

## 1320

[1320-blend-walking-vanishing-end-keeps-the-walk-parameter.patch](../opencascade/1320-blend-walking-vanishing-end-keeps-the-walk-parameter.patch)

Test: geometry/tests/test_fillet_vanishing_side.cpp, the reported plate's
sides rounded on the body MitCad builds: the result is valid and its faces
lie on the stored body after the rounding within 0.01 mm (invalid without
the patch, the faces' wires crossing at both vanishing ends).

## 1330

[1330-chfi3d-onsame-prolongation-along-a-seam-not-split-at-its-start.patch](../opencascade/1330-chfi3d-onsame-prolongation-along-a-seam-not-split-at-its-start.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the boss's foot rounded
with 1, 3 and 6 with the cylinder's seam at either vertex and on the body
MitCad's sketch and extrusions build (MITCAD_GEOMETRY_CASES): valid, the
material added as the exact ring between the sides within 1e-6 of it.

## 1331

[1331-chfi3d-onsame-opposite-contact-ending-on-the-prolonged-edge-cuts-it.patch](../opencascade/1331-chfi3d-onsame-opposite-contact-ending-on-the-prolonged-edge-cuts-it.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the plate's top edge
rounded with 1, 3 and 6 with the cylinder's seam at either vertex and on
the body MitCad's sketch and extrusions build (MITCAD_GEOMETRY_CASES):
valid, the material removed as the exact one within 1e-6 of it.

## 1332

[1332-chfi3d-periodic-contact-end-kept-at-its-end-of-a-whole-period.patch](../opencascade/1332-chfi3d-periodic-contact-end-kept-at-its-end-of-a-whole-period.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the boss's foot rounded
with 1, 3 and 6 on the body MitCad's sketch and extrusions build
(MITCAD_GEOMETRY_CASES): valid, the material added as the exact one within
1e-6 of it.

## 1340

[1340-chfi3d-rounding-rides-on-a-part-of-another-contact.patch](../opencascade/1340-chfi3d-rounding-rides-on-a-part-of-another-contact.patch)

Test: geometry/tests/test_fillet_boss_foot_rim.cpp, test_seam_on_the_top,
test_seam_at_the_foot_end and test_seam_below_the_top (ctest
geometry.fillet_boss_foot_rim): bars with a coaxial boss (height 2 with
radius 1, 2.5 with 1.25), the cylinder's seam on the top, at either end of
the foot and below the top, the boss's foot and rim rounded: valid and
closed, the volume continuing the smaller radii's within 2e-5 mm3, the
cylinder's area that of the bar's top from the rim's contact on; the
port's notes record the fillet failing without the patch, and the smaller
radii, which build without it, are the boundary cases. It fails without
the patch at its place in the chain: OCCT with the patches before it,
without and with this one (shown 2026-11-21 by tools/occt-port-proof.py
--in-chain, with the declarations of 1662, 1701 carried, which add API the
MitCad code uses).


## 1350

[1350-chfi3d-rounding-over-a-slight-concave-crease-cut-by-the-face-beyond.patch](../opencascade/1350-chfi3d-rounding-over-a-slight-concave-crease-cut-by-the-face-beyond.patch)

Test: geometry/tests/test_fillet_fin_top_creases.cpp,
test_transitions_cut_by_their_pieces, on the case `fin-top-chamfer-rounding`
of MITCAD_GEOMETRY_CASES (with 1351 and 1352).

## 1351

[1351-chfi3d-splitkpart-last-end-keeps-its-pieces-as-the-first.patch](../opencascade/1351-chfi3d-splitkpart-last-end-keeps-its-pieces-as-the-first.patch)

Test: geometry/tests/test_fillet_fin_top_creases.cpp,
test_transitions_cut_by_their_pieces (with 1350 and 1352).

## 1352

[1352-chfi3d-kpart-past-end-piece-one-face-with-the-end-piece.patch](../opencascade/1352-chfi3d-kpart-past-end-piece-one-face-with-the-end-piece.patch)

Test: geometry/tests/test_fillet_fin_top_creases.cpp,
test_transitions_cut_by_their_pieces (with 1350 and 1351): the rounding has
the stored body's faces.

## 1370

[1370-chfi3d-roll-over-a-sliver-past-the-seam-of-the-other-contacts-face.patch](../opencascade/1370-chfi3d-roll-over-a-sliver-past-the-seam-of-the-other-contacts-face.patch)

Test: geometry/tests/test_fillet_roll_over_blend.cpp,
test_rounding_turns_past_the_seam and
test_rounding_turns_past_the_seam_other_version (ctest
geometry.fillet_roll_over_blend): the cases fillet-roll-over-seam and
fillet-roll-over-seam-2, the rim rounded with OCCT alone against the
stored body after it; the port's notes record it failing without the
patch. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py). The test reads a stored case
(MITCAD_GEOMETRY_CASES), not a synthetic body: a synthetic test is still
wanted (mitcad#1986).

## 1380

[1380-chfi3d-concave-chain-through-a-corner-tighter-than-the-ball-closed-by-the-corner-s-own-ball.patch](../opencascade/1380-chfi3d-concave-chain-through-a-corner-tighter-than-the-ball-closed-by-the-corner-s-own-ball.patch)

Test: geometry/tests/test_fillet_three_face_corner.cpp,
test_radius_of_the_reported_design and test_other_radii (ctest
geometry.fillet_three_face_corner): a synthetic rod and wall as the
case's, rounded with 3, 2 and 4: at 3 the corner's points, the strip's and
the triangle's areas and the material added are the stored body's; the
port's notes record each failing without the patch. It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-21 by tools/occt-port-proof.py
--in-chain, with the declarations of 1662, 1701 carried, which add API the
MitCad code uses).

## 1381

[1381-chfi3d-wide-surface-of-a-trimmed-surface-is-its-basis.patch](../opencascade/1381-chfi3d-wide-surface-of-a-trimmed-surface-is-its-basis.patch)

Test: geometry/tests/test_fillet_three_face_corner.cpp,
test_rounding_over_a_trimmed_torus (ctest
geometry.fillet_three_face_corner): the case fillet-trimmed-torus, the
replay's body before its key's top is rounded: valid and closed; the
port's notes record its ends failing without the patch. It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py). The test reads a
stored case (MITCAD_GEOMETRY_CASES), not a synthetic body: a synthetic
test is still wanted (mitcad#1986).

## 1390

[1390-blendfunc-constrad-centre-picks-the-ball-where-the-mid-point-lies-on-the-guides-axis.patch](../opencascade/1390-blendfunc-constrad-centre-picks-the-ball-where-the-mid-point-lies-on-the-guides-axis.patch)

Test: geometry/tests/test_fillet_ball_twice_arc.cpp, a pocket whose floor
meets concave arcs of 0.4, rounded with 0.8 (and 0.79999, 0.80001) by
OCCT's fillet alone and by MitCad's, and the reported body (a case of
MITCAD_GEOMETRY_CASES): the rounding builds at exactly 0.8, valid, without
MitCad's 0.1 % smaller retry, and on the pocket it lies within 1e-4 mm of
the roundings at 0.79999 and 0.80001 (6e-6 mm measured). Without the
patch OCCT's fillet is not done at 0.8 (one faulty contour).

## 1400

[1400-chfi3d-convex-and-concave-roundings-crossing-before-a-smooth-common-end-overlap.patch](../opencascade/1400-chfi3d-convex-and-concave-roundings-crossing-before-a-smooth-common-end-overlap.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the plate's top edge
and the boss's foot rounded together with 1, 3 and 6 (and with the boss's
top edge) on the synthetic bodies with the cylinder's seam at either
vertex and on the body MitCad's sketch and extrusions build
(MITCAD_GEOMETRY_CASES): OCCT's fillet reports the overlap with the two
faulty contours (an invalid result without the patch);
geometry/tests/test_chamfer_sharp_corner.cpp, the floor and wall case
with a sharp edge, still built together.

## 1401

[1401-chfi3d-vanishing-end-on-a-periodic-faces-seam-kept-in-the-contacts-period.patch](../opencascade/1401-chfi3d-vanishing-end-on-a-periodic-faces-seam-kept-in-the-contacts-period.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the plate's top edge
and the boss's foot rounded together with 1 (the convex one first, the
concave one on its result, as MitCad's fillet and OCCT's fillet one after
the other build them) on the synthetic bodies and on the case of
MITCAD_GEOMETRY_CASES: valid, the volume the one after the other gives;
36 checks fail without the patch.

## 1402

[1402-chfi3d-end-walked-into-a-vanishing-end-taken-for-it-at-any-radius.patch](../opencascade/1402-chfi3d-end-walked-into-a-vanishing-end-taken-for-it-at-any-radius.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the plate's top edge
and the boss's foot rounded together with 3 and 6 on the synthetic bodies
and on the case of MITCAD_GEOMETRY_CASES: no face of the result smaller
than 1e-3 mm2; 24 checks fail without the patch.

## 1410

[1410-geomfill-constrainedfilling-field-not-judged-where-it-vanishes.patch](../opencascade/1410-geomfill-constrainedfilling-field-not-judged-where-it-vanishes.patch)

Test: geometry/tests/test_fillet_ball_twice_arc.cpp, test_case_body (ctest
geometry.fillet_ball_twice_arc): the case ball-twice-arc-rounding, the
body MitCad builds and the stored state, rounded at 0.79999, 0.8 and
0.80001: the area of the B-spline faces round each 0.4 arc the same within
1e-3 mm2 (the two endings differ by 0.06), the material added at 0.8
within 1e-3 mm3 of that at both radii beside it; without the patch the
area differs at one arc or the other (the port's notes). It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py). The test reads a
stored case (MITCAD_GEOMETRY_CASES), not a synthetic body: a synthetic
test is still wanted (mitcad#1986). The synthetic pocket of
test_pocket_floor_rounded checks the material added only.

## 1420

[1420-chfi3d-strip-edge-rolled-on-past-its-end-cut-by-a-transition-piece.patch](../opencascade/1420-chfi3d-strip-edge-rolled-on-past-its-end-cut-by-a-transition-piece.patch)

Test: geometry/tests/test_fillet_fin_end_strips.cpp, on the case
`fin-end-strips` of MITCAD_GEOMETRY_CASES.

## 1430

[1430-chfi3d-ball-rolled-on-a-convex-edge-into-the-end-face-cut-from-its-turn.patch](../opencascade/1430-chfi3d-ball-rolled-on-a-convex-edge-into-the-end-face-cut-from-its-turn.patch)

Test: geometry/tests/test_fillet_end_edge_turn.cpp, on the case
`notch-lip-corners` of MITCAD_GEOMETRY_CASES.

## 1440

[1440-chfi3d-walk-guided-along-the-balls-centres-where-the-spine-folds.patch](../opencascade/1440-chfi3d-walk-guided-along-the-balls-centres-where-the-spine-folds.patch)

Test: geometry/tests/test_fillet_acute_turn.cpp,
test_rounding_round_the_foot (ctest geometry.fillet_acute_turn): a
synthetic plate, rail and end wall rounded at radii 1, 1.5 and 1.9: valid,
closed, the walked face on the ball; the port's notes record one faulty
contour at each radius without the patch. test_reported_body does the same
on the case acute-concave-turn. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py, reverted together with 1601, 1790,
which change the same lines).

## 1460

[1460-chfi3d-end-crossing-of-a-periodic-edge-taken-in-the-edges-period.patch](../opencascade/1460-chfi3d-end-crossing-of-a-periodic-edge-taken-in-the-edges-period.patch)

Test: geometry/tests/test_fillet_end_reversed_faces.cpp, test_pieces_alike
(ctest geometry.fillet_end_reversed_faces): the case
fillet-end-on-reversed-faces, the piece of each body about the end (9
faces) rounded: both closed and valid, with as many faces, the same volume
and on each other within 1e-5 mm; without the patch MitCad's piece is
invalid and 0.51 mm3 short (the port's notes). It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py). The test reads a stored
case (MITCAD_GEOMETRY_CASES): a synthetic body needs an edge curve whose
range runs past its period's end with a non-periodic p-curve beside it, as
the import's bodies come (left, mitcad#1986).

## 1470

[1470-chfi3d-periodic-contact-endpoints-snapped-before-splitting.patch](../opencascade/1470-chfi3d-periodic-contact-endpoints-snapped-before-splitting.patch)

Test: geometry/tests/test_fillet_boss_foot_rim.cpp, test_seam_on_the_top,
test_seam_at_the_foot_end and test_seam_below_the_top (ctest
geometry.fillet_boss_foot_rim), their seven configurations: valid, closed,
the volume within 2e-5 mm3 and the areas as for 1340. Without the patch
two of them come out invalid on macOS (an unorientable torus face, a wrong
volume), so its before/after run needs that platform (mitcad#1986). The
neighbours: geometry.fillet_ride_against and the periodic-seam and
crossing-contact cases of geometry.suite.fillet_results.

## 1480

[1480-chfi3d-and-brepoffset-use-current-occt-apis.patch](../opencascade/1480-chfi3d-and-brepoffset-use-current-occt-apis.patch)

Test: none, because it is a build-only port change: it removes an unused
binding, names GC_MakeSegment2d instead of its deprecated alias and copies
a returned knot array before reparametrising it, with the same results;
its check is the warning-free build of the three translation units
(third_party/vcpkg-ports/docs/compiler-compatibility-1480.md), not a
geometric test.

## 1490

[1490-chfi3d-walk-back-to-the-spines-first-end-stepped-by-its-own-stretch.patch](../opencascade/1490-chfi3d-walk-back-to-the-spines-first-end-stepped-by-its-own-stretch.patch)

Test: geometry/tests/test_fillet_mitre_same_radius.cpp,
test_vertical_edge_through_the_mitre (ctest
geometry.fillet_mitre_same_radius): a box's top edges rounded with 1 at
each corner, then the vertical edge and the mitre with 1, each edge given
first, each way round, on the box and on the box mirrored about its middle
height so that its vertical edges run down: valid, the material taken the
vertical edge's rounding and the mitre's corner less the ball's eighth
within 1e-4; the port's notes record three corners of the mirrored box
failing without the patch, the unmirrored box is the boundary case. It
fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1500

[1500-chfi3d-convex-chain-into-a-mitre-of-tighter-roundings-closed-by-their-balls-torus.patch](../opencascade/1500-chfi3d-convex-chain-into-a-mitre-of-tighter-roundings-closed-by-their-balls-torus.patch)

Test: geometry/tests/test_fillet_convex_mitre.cpp,
test_vertical_edge_into_the_mitre, test_top_mitre_alone and
test_corner_face_named (ctest geometry.fillet_convex_mitre): a block on a
plate, its foot rounded with 1.5 and its top with 2.5, each vertical edge
rounded with 3, 4 and 6, each way round: valid, one torus of area rk phi
(rho pi / 2 + rk), the material taken at the top corner the formula's
within 1e-4; the block with only its top rounded, rounded with 3; MitCad's
fillet names every face and edge. The port's notes record each part
failing without the patch. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1520

[1520-blendfunc-constrad-ball-staying-put-between-tubes-walked-in-the-guides-normal-plane.patch](../opencascade/1520-blendfunc-constrad-ball-staying-put-between-tubes-walked-in-the-guides-normal-plane.patch)

Test: geometry/tests/test_fillet_stationary_ball.cpp,
test_top_rim_through_the_mitres and test_both_rims_on_the_half_lid (ctest
geometry.fillet_stationary_ball; with 1521): a lid of 23 cut by a plane,
the flat side's edges rounded with 1 and its top rim rounded with 1
through both corners' mitres, and the lid's half with its cone and post,
both rims rounded: valid, each corner one new face on the ball's sphere
within 1e-4, the top corner's area pi / 2 + asin(1 / 22) within 2e-3; the
port's notes record each rounding failing without 1520 and 1521. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted
together with 1523, 1760, which change the same lines).

## 1521

[1521-chfi3d-ball-staying-put-at-a-plane-walked-its-point-contact-along-no-edge.patch](../opencascade/1521-chfi3d-ball-staying-put-at-a-plane-walked-its-point-contact-along-no-edge.patch)

Test: geometry/tests/test_fillet_stationary_ball.cpp,
test_top_rim_through_the_mitres and test_both_rims_on_the_half_lid (ctest
geometry.fillet_stationary_ball; with 1520): the lid's top rim rounded
through both corners' mitres, the ball touching the top at one point:
valid, each corner one new face on the ball's sphere within 1e-4; the
port's notes record each rounding failing without 1520 and 1521. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1522

[1522-chfi3d-walked-blend-split-only-where-its-contacts-meet.patch](../opencascade/1522-chfi3d-walked-blend-split-only-where-its-contacts-meet.patch)

Test: geometry/tests/test_fillet_stationary_ball.cpp (each corner one face,
the case's result with the stored body's faces).

## 1523

[1523-blendfunc-constrad-ball-staying-put-between-tubes-within-a-tolerance.patch](../opencascade/1523-blendfunc-constrad-ball-staying-put-between-tubes-within-a-tolerance.patch)

Test: geometry/tests/port/p1523-still-ball-tolerance.cpp (ctest
geometry.port.1523): half a lid of radius 23 whose flat side's edges are
rounded with 1, the rounding along its top replaced by a fitted B-spline
tube whose section's radius ripples by 6e-6 or 1e-5; the top rim rounded
with 1 through the mitres, where its ball stays put: built, valid, taking
off what it takes off on the unrippled tube (without the patch: not
built). The boundary: the tube fitted without a ripple, as before.

## 1530

[1530-chfi3d-walk-onto-a-tight-tangent-face-at-the-end-rolls-on-cut-to-a-point.patch](../opencascade/1530-chfi3d-walk-onto-a-tight-tangent-face-at-the-end-rolls-on-cut-to-a-point.patch)

Test: geometry/tests/test_fillet_tight_tangent_end.cpp,
test_ends_cut_by_the_rims (ctest geometry.fillet_tight_tangent_end): the
case hub-foot-tight-rim-ends, its nine edges rounded with 1: valid and
closed, the material added the stored body's within 0.5 %, on the stored
body and back within 0.01 mm about both holes, its four ends at vertices
of the result within 0.01 mm; the port's notes record the contour failing
without the patch. The checks of each end's face, which 1540 added to the
same function, are its own test function since mitcad#2310 (at 1530's
place the test failed with 1530 too, on the end face's area). It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with the declarations of 1662, 1701
carried, which add API the MitCad code uses). The test reads a stored case
(MITCAD_GEOMETRY_CASES): OCCT's fillet cannot make the earlier roundings
of a hole's rim that touches the hub, so no synthetic body has been built
(left, mitcad#1986).

## 1540

[1540-chfi3d-tight-tangent-end-walked-and-cut-pieces-one-face.patch](../opencascade/1540-chfi3d-tight-tangent-end-walked-and-cut-pieces-one-face.patch)

Test: geometry/tests/test_fillet_tight_tangent_end.cpp,
test_end_faces_as_stored (ctest geometry.fillet_tight_tangent_end): on the
case hub-foot-tight-rim-ends rounded as for 1530, each end's face has the
stored body's area within 1 %, and the result the stored body's faces
(a function of its own since mitcad#2310; before, these checks were part
of 1530's test function). It fails without the patch (shown 2026-11-23 by
tools/occt-port-proof.py, reverted together with 1700, which changes the
same lines).

## 1560

[1560-chfi3d-rim-contact-split-where-it-runs-along-a-foot-roundings-edge.patch](../opencascade/1560-chfi3d-rim-contact-split-where-it-runs-along-a-foot-roundings-edge.patch)

Test: geometry/tests/test_fillet_rim_down_to_foot.cpp
(geometry.fillet_rim_down_to_foot).

## 1570

[1570-chfi3d-point-end-on-an-edge-at-the-vertex-runs-the-seam-on.patch](../opencascade/1570-chfi3d-point-end-on-an-edge-at-the-vertex-runs-the-seam-on.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the plate's top edge
picked at its other side (y = -25) rounded with 1, 3 and 6, alone and with
the boss's top edge, on the synthetic bodies and on the body MitCad's UI
builds from a slot and a circle (MITCAD_GEOMETRY_CASES): valid, the
material removed as the exact one within 1e-6 of it.

## 1571

[1571-chfi3d-kpart-cut-end-on-a-periodic-face-in-the-period-of-its-other-end.patch](../opencascade/1571-chfi3d-kpart-cut-end-on-a-periodic-face-in-the-period-of-its-other-end.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the plate's top edge
picked at its other side rounded with 1, 3 and 6 on the synthetic body
whose seam runs up at the side picked first (y = 25).

## 1572

[1572-chfi3d-kpart-cut-piece-on-the-end-edge-one-face-with-its-rest.patch](../opencascade/1572-chfi3d-kpart-cut-piece-on-the-end-edge-one-face-with-its-rest.patch)

Test: geometry/tests/test_fillet_boss_tangent.cpp, the plate's top edge
picked at its other side: the rounding of that edge one face named after
it.

## 1580

[1580-chfi3d-blend-run-on-past-the-end-over-both-faces-cut-by-the-faces-round-the-end.patch](../opencascade/1580-chfi3d-blend-run-on-past-the-end-over-both-faces-cut-by-the-faces-round-the-end.patch)

Test: geometry/tests/test_fillet_torus_side_ends.cpp, test_reported_body
(ctest geometry.fillet_torus_side_ends): on the stored case
`fillet-torus-side-ends` of MITCAD_GEOMETRY_CASES, the six edges rounded
with 5, valid and closed, on the stored body within 0.01 mm; documented to
fail without this patch (the corners fail). No synthetic body: the corner
patch is the end of an earlier rounding fading tangentially into the wall,
which OCCT's fillet does not make. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 1581

[1581-chfi3d-end-cut-on-a-face-the-way-round-inside-it.patch](../opencascade/1581-chfi3d-end-cut-on-a-face-the-way-round-inside-it.patch)

Test: geometry/tests/test_fillet_torus_side_ends.cpp, test_reported_body
(ctest geometry.fillet_torus_side_ends): on the stored case
`fillet-torus-side-ends` of MITCAD_GEOMETRY_CASES; documented to fail
without this patch (the tip's rounding 0.27 mm off the stored body). No
synthetic body, as for 1580. It fails without the patch (shown 2026-11-20
by tools/occt-port-proof.py).

## 1590

[1590-chfi3d-tight-corners-at-a-cylinders-rim-and-at-an-obtuse-wall-closed-by-the-rims-ball.patch](../opencascade/1590-chfi3d-tight-corners-at-a-cylinders-rim-and-at-an-obtuse-wall-closed-by-the-rims-ball.patch)

Test: geometry/tests/test_fillet_tight_rim_corner.cpp (ctest
geometry.fillet_tight_rim_corner):
test_tube_cut_face_rounded_past_its_rims (the four tori, the strips and
triangles on the rim's ball),
test_block_bottom_past_corners_with_an_obtuse_wall (the obtuse wall at 15
and 30 degrees, its corner faces on the corner's ball) and
test_reported_body (the stored case `tube-rim-tight-corners`); each
documented to fail without this patch. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 1600

[1600-chfi3d-walk-ends-at-the-next-analytic-piece-only-where-both-contacts-reach-it.patch](../opencascade/1600-chfi3d-walk-ends-at-the-next-analytic-piece-only-where-both-contacts-reach-it.patch)

Test: geometry/tests/test_fillet_still_ball_corners.cpp (ctest
geometry.fillet_still_ball_corners): test_d_block, a D-shaped block's top
rim rounded first and the wall's edges after it with 1 on a cylinder of 5,
0.2 on 5 and 2 on 8, each way round (without this patch the rounding
throws, or with 2 on 8 is invalid); and test_reported_body on the stored
case `fillet-opening-still-corners`. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 1601

[1601-chfi3d-two-stripes-meet-at-the-section-of-an-earlier-roundings-ball-that-stays-put.patch](../opencascade/1601-chfi3d-two-stripes-meet-at-the-section-of-an-earlier-roundings-ball-that-stays-put.patch)

Test: geometry/tests/test_fillet_still_ball_corners.cpp (ctest
geometry.fillet_still_ball_corners), test_reported_body: the case
`fillet-opening-still-corners` of MITCAD_GEOMETRY_CASES (a stored body,
skipped without it), the opening's wall rounded as one loop: built,
valid, the ball's face at each corner. Fails without the patch (no
result), passes with it; a synthetic case of its own is still to be
written (mitcad#1986).

## 1610

[1610-chfi3d-standing-ball-section-where-the-normal-planes-pass-forwards.patch](../opencascade/1610-chfi3d-standing-ball-section-where-the-normal-planes-pass-forwards.patch)

Test: geometry/tests/test_fillet_groove_rim_corners.cpp (ctest
geometry.fillet_groove_rim_corners): test_synthetic_leg, a leg's groove
rim rounded with 0.5, every new face on the ball's envelope within 1e-5
mm, and test_reported_body on the stored case `groove-rim-corners`;
without this patch both roundings fail (twisted sections). It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1611

[1611-chfi3d-two-corner-cut-on-a-periodic-pcurve-not-cut-to-its-period.patch](../opencascade/1611-chfi3d-two-corner-cut-on-a-periodic-pcurve-not-cut-to-its-period.patch)

Test: geometry/tests/test_fillet_groove_rim_corners.cpp (ctest
geometry.fillet_groove_rim_corners): test_synthetic_leg, each corner of
0.5 a piece of the ball's sphere (without this patch the leg's upper
corner is filled with a plate 0.37 mm off the envelope), and
test_reported_body on the stored case `groove-rim-corners` (156 faces; 157
without it). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1620

[1620-chfi3d-ball-turned-about-a-crease-at-its-end-vertex-into-the-end-face.patch](../opencascade/1620-chfi3d-ball-turned-about-a-crease-at-its-end-vertex-into-the-end-face.patch)

Test: geometry/tests/test_fillet_crease_end_corner.cpp (ctest
geometry.fillet_crease_end_corner): test_cut_cylinder_rims_at_30_degrees
and test_cut_cylinder_rims_at_45_degrees, a cylinder cut at a crease with
a block below the cut face, its rims rounded with 1.5: valid, the horn
tori of area R^2 (pi / 2 - psi) (pi / 2 - 1) (invalid without the patch);
the boundary test_cut_cylinder_rims_without_the_block, the tori cut by the
cut face as before; and test_reported_body on the stored case
`tube-end-crease-corners`. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1640

[1640-chfi3d-ride-on-a-contact-line-the-other-way-its-side-on-the-face-and-its-corners.patch](../opencascade/1640-chfi3d-ride-on-a-contact-line-the-other-way-its-side-on-the-face-and-its-corners.patch)

Test: geometry/tests/test_fillet_ride_corners.cpp (ctest
geometry.fillet_ride_corners): test_plate_end_face_rims (the boundary:
builds without the patch too), test_plate_end_face_rims_and_corners,
test_plate_all_edges and test_slab_end_face_pair, the plate 1.3 thick and
the roof slab's end face rounded with 1, each valid with the removed
volume of the roundings' sections; documented to fail without the patches
1640 to 1642 but for the first. The test needs the later patches 1641 and
1642 too: the three came in one change and the test with them, the slab's
end face pair is 1641's test, and the plate's end face consumed between
rides that end in corners is the case 1642 was written for (at its place
the test fails with 1640 too, mitcad#2310), so in the chain the patch is shown
with 1641 and 1642 carried, or together with them where they build on its
lines. It fails without the patch and 1641 at their place in the chain:
OCCT with the patches before them, without and with these (shown
2026-11-23 by tools/occt-port-proof.py --in-chain, with 1642 carried in
both states, which the test needs, and the declarations of 1662, 1701
carried, which add API the MitCad code uses; 1641 builds on the patch's
lines, so the patch is shown together with it).

## 1641

[1641-chfi3d-rider-ending-where-the-line-ends-on-the-arc-from-its-vertex.patch](../opencascade/1641-chfi3d-rider-ending-where-the-line-ends-on-the-arc-from-its-vertex.patch)

Test: geometry/tests/test_fillet_ride_corners.cpp, test_slab_end_face_pair
(ctest geometry.fillet_ride_corners): the roof slab's end face's two edges
along a slope rounded with 1 (and 0.9), the underside's rounding riding on
the slope's contact line and ending on the line's end on the eave's edge;
the material taken 14.029 against 14.019 of the roundings' union. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1642

[1642-chfi3d-face-consumed-between-rides-with-corners-at-both-ends.patch](../opencascade/1642-chfi3d-face-consumed-between-rides-with-corners-at-both-ends.patch)

Test: none, because the case the branch is for builds right at the port's
tip without it, and the branch's condition has no other shape than that
case. geometry/tests/test_fillet_ride_corners.cpp,
test_plate_end_face_rims_and_corners and test_plate_all_edges, round the
plate's end face rims and its vertical edges with OCCT's fillet alone: the
end face goes and the volume is right without the patch too, as the
port's before/after run showed (mitcad#1986). The branch needs a strip F
between two rims whose roundings meet on one line (a ride that is no part
ride) with a line's end at a corner's point inside F, that is the edge of
F at that end rounded too. That edge's rounding has its contact on F
crossing the line ridden on, and the corners at both its ends (with the
rim below and with the rider above) cut it at that same crossing: a
trapezoid end face, slanted end edges, other radii or only one end edge
rounded (the other end then on an arc, which the code before the branch
cuts) all give that same configuration. Since 1642, later patches treat
this contact cut to one point and the corners' points there (1652, which
makes the two points one, 1655 and the corner patches up to 1781), and
one of them takes F away; which one was not traced. A body that shows the
branch would need a strip whose end edge's contact on F is not cut to one
point by the corners at its ends, which this configuration does not
allow.

## 1650

[1650-chfi3d-rounding-of-an-edge-another-rolls-on-all-along-dropped.patch](../opencascade/1650-chfi3d-rounding-of-an-edge-another-rolls-on-all-along-dropped.patch)

Test: geometry/tests/test_fillet_eave.cpp, test_eave_long_edges (ctest
geometry.fillet_eave): the eave's two long edges rounded with 0.9 and 1,
no face left on the eave, the material taken 25.2 times the tip's
rounding's section within 1e-5 (fails without the patches 1650 to 1653);
and geometry.fillet_roof_slab (test_slab) at 0.9 and 1. It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1651

[1651-chfi3d-corner-of-a-rolling-blend-and-a-blend-on-the-face-it-consumes.patch](../opencascade/1651-chfi3d-corner-of-a-rolling-blend-and-a-blend-on-the-face-it-consumes.patch)

Test: geometry/tests/test_fillet_eave.cpp, test_eave_all_edges (ctest
geometry.fillet_eave): the eave's four edges rounded with 0.9 and 1, the
end edges' blends meeting the tip's where their contacts reach the obtuse
edge, the material taken within 1e-4 of the removal sets' union (fails
without the patch). It fails without the patch at its place in the chain:
OCCT with the patches before it, without and with this one (shown
2026-11-21 by tools/occt-port-proof.py --in-chain, with the declarations
of 1662, 1701 carried, which add API the MitCad code uses).


## 1652

[1652-chfi3d-contact-cut-to-one-point-by-the-corners-at-both-ends.patch](../opencascade/1652-chfi3d-contact-cut-to-one-point-by-the-corners-at-both-ends.patch)

Test: geometry/tests/test_fillet_eave.cpp,
test_end_face_strip_and_eave_end (ctest geometry.fillet_eave): an end
face's two edges along the slope and the eave's end edge at 0.7, 0.9 and
1, the end edge's contact cut to one point by both corners; valid, the
material taken between the largest single rounding's and the sum (invalid
without the patch). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1653

[1653-chfi3d-three-blends-cut-by-each-other-where-one-rolls-on-the-consumed-faces-edge.patch](../opencascade/1653-chfi3d-three-blends-cut-by-each-other-where-one-rolls-on-the-consumed-faces-edge.patch)

Test: geometry/tests/test_fillet_eave.cpp, test_tip_corner_of_three (ctest
geometry.fillet_eave): the tip's edge, the eave's end edge and the slope's
top edge on the end face, the three blends cut by each other meeting at
one point; valid, the material taken within 2e-3 of the removal sets'
union (an invalid sliver without the patch). It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py, reverted together with
1657, 1659, which change the same lines).

## 1654

[1654-chfi3d-plate-of-a-corner-with-a-rider-oriented-from-a-stripe-riding-on-no-line.patch](../opencascade/1654-chfi3d-plate-of-a-corner-with-a-rider-oriented-from-a-stripe-riding-on-no-line.patch)

Test: geometry/tests/port/p1654-plate-of-a-ride-corner-oriented.cpp
(ctest geometry.port.1654): test_plate_of_a_corner_with_a_rider, the roof
slab's 21 edges rounded with 0.8 by OCCT's fillet alone in the body's edge
order: valid, every face of a positive area (the plate's face the wrong
way round without the patch); and test_corners_without_riders, the
boundary, the same at 0.5, where nothing rides. It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py). The earlier test,
geometry/tests/test_fillet_roof_slab.cpp, test_slab (ctest
geometry.fillet_roof_slab), rounds the slab through the MitCad facade,
whose shape healing of a result BRepCheck rejects can turn the plate's
wire round: it passes without the patch.

## 1655

[1655-chfi3d-roundings-consuming-a-face-whose-edges-are-all-rounded-meet-on-one-contact-edge.patch](../opencascade/1655-chfi3d-roundings-consuming-a-face-whose-edges-are-all-rounded-meet-on-one-contact-edge.patch)

Test: geometry/tests/test_fillet_rib_full_round.cpp (ctest
geometry.fillet_rib_full_round): test_full_round and test_full_bevel, a
rib 2, 1.5 and 4 wide whose four top edges are rounded (and bevelled) by
half its width: one valid closed solid, no face left on top, the exact
volume within 1e-6 mm3. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1656

[1656-chfi3d-curve-left-by-a-stripe-cut-to-nothing-in-the-period-of-the-corner-surface.patch](../opencascade/1656-chfi3d-curve-left-by-a-stripe-cut-to-nothing-in-the-period-of-the-corner-surface.patch)

Test: geometry/tests/test_fillet_roof_slab.cpp, test_slab (ctest
geometry.fillet_roof_slab): the roof slab rounded on all its edges with
0.7, 0.8 and 1.1, valid and closed (the torus's face built over the other
part of the torus, area -2.68 at 0.8, without the patch). It fails without
the patch (shown 2026-11-20 by tools/occt-port-proof.py, reverted together
with 1659, which changes the same lines).

## 1657

[1657-chfi3d-corner-of-three-curves-marched-the-way-keeping-inside-the-contacts.patch](../opencascade/1657-chfi3d-corner-of-three-curves-marched-the-way-keeping-inside-the-contacts.patch)

Test: geometry/tests/test_fillet_roof_slab.cpp, test_slab (ctest
geometry.fillet_roof_slab): the roof slab rounded on all its edges with
1.1, valid and closed (both blends' wires crossing themselves without the
patch). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1658

[1658-chfi3d-the-rounding-whose-contact-lies-nearer-its-edge-rides-a-ride-of-no-width-drops-the-rider.patch](../opencascade/1658-chfi3d-the-rounding-whose-contact-lies-nearer-its-edge-rides-a-ride-of-no-width-drops-the-rider.patch)

Test: geometry/tests/test_fillet_roof_slab.cpp, test_slab (ctest
geometry.fillet_roof_slab): the roof slab rounded on all its edges with
0.862 and 0.865, valid and closed, its volume against the roundings' union
(a jump of 2 without the patch). The test needs the later patch 1659 too:
the two came in one change with the radii 0.862 and 0.865, whose end
edge's face crossed itself without 1659 (at 1658's place the rounding
fails with 1658 too, mitcad#2310), so in the chain the patch is shown with
1659 carried, or together with it where it builds on its lines. It fails
without the patch at its place in the chain: OCCT with the patches before
it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain, with 1659 carried in both states,
which the test needs, and the declarations of 1662, 1701 carried, which
add API the MitCad code uses).

## 1659

[1659-chfi3d-corner-of-three-cut-by-each-other-where-a-stripe-is-cut-at-a-ride-line-at-both-ends.patch](../opencascade/1659-chfi3d-corner-of-three-cut-by-each-other-where-a-stripe-is-cut-at-a-ride-line-at-both-ends.patch)

Test: geometry/tests/test_fillet_roof_slab.cpp, test_slab (ctest
geometry.fillet_roof_slab): the roof slab rounded on all its edges with
0.862 and 0.865 (the obtuse edge's rounding dropped by 1658), valid and
closed (the end edge's face a sliver whose wire crossed itself without the
patch). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1661

[1661-brepfill-trim-shell-corner-each-seam-ends-where-it-crosses-the-other-side.patch](../opencascade/1661-brepfill-trim-shell-corner-each-seam-ends-where-it-crosses-the-other-side.patch)

Test: geometry/tests/test_pipe_square_section.cpp (ctest
geometry.pipe_square_section): test_stirrup, the closed stirrup of lines
and arcs with slight kinks between them: a valid solid within 0.2 % of the
circle's area times the path's length. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py). Also test_four_lines and
test_kink, an 8 mm circle swept along the sine wave as 4 lines crossed
with the bent arc and along two splines meeting at a kink of 1 degree: the
section square to the path within 0.01 degrees at 20 points and the volume
within 0.1 % (58.8 degrees and -23.9 % before the patches 1661 and 1662);
they succeed without this patch alone.


## 1662

[1662-brepoffsetapi-pipe-shell-mitred-corners-from-a-ten-thousandth-radian.patch](../opencascade/1662-brepoffsetapi-pipe-shell-mitred-corners-from-a-ten-thousandth-radian.patch)

Test: geometry/tests/test_pipe_square_section.cpp (ctest
geometry.pipe_square_section): test_four_lines and test_kink (with 1661),
and test_stirrup, whose kink of 0.12 degrees is a mitred corner since this
patch: a valid solid within 0.2 % of the circle's area times the path's
length. It fails without the patch: the test does not build without it
(shown 2026-11-20 by tools/occt-port-proof.py).

## 1663

[1663-brepfill-trim-shell-corner-mitre-along-the-bisector-plane-where-the-sides-intersection-breaks-up.patch](../opencascade/1663-brepfill-trim-shell-corner-mitre-along-the-bisector-plane-where-the-sides-intersection-breaks-up.patch)

Test: geometry/tests/test_pipe_square_section.cpp
(geometry.pipe_square_section: the pipes mitred square to their paths).

## 1664

[1664-brepfill-sweep-sides-extended-past-a-corner-as-the-prisms-of-their-end-sections.patch](../opencascade/1664-brepfill-sweep-sides-extended-past-a-corner-as-the-prisms-of-their-end-sections.patch)

Test: none, because no body reaches the patch's case at the port's tip
and passes with it (mitcad#2121). A port test written for this patch
(not kept: an 8 mm circle swept with OCCT alone along its own
interpolated crossing of the sine wave kinked 3 degrees and the arc
bending 0.3 mm) does not reach its case.
With every patch applied (2026-11-20) the corrected frame's sweep has a
null side face at the corner and crashes in
BRepFill_TrimShellCorner::PerformByIntersection (with the section halved
as well), and the constant binormal's boundary is no valid solid (two
edges of invalid range); both fail the same way at 1663 without the patch.
MitCad's own path of that case (geometry/tests/test_pipe_square_section.cpp,
test_small_kinks, "kink of 3 degrees, nearly planar") does reach it: its
mitre with the circle in one edge is no valid shape, and with the section
halved the trim along the intersection fails and the mitre along the
bisector plane makes a valid pipe. That test passes without the patch
too: MitCad's pipe tries rounded corners where the mitre is not sound,
and test_ten_lines' corners are between lines, where the patch leaves
OCCT's extension.


## 1665

[1665-bopds-paves-of-an-edge-whose-blocks-do-not-chain.patch](../opencascade/1665-bopds-paves-of-an-edge-whose-blocks-do-not-chain.patch)

Test: geometry/tests/test_pipe_square_section.cpp, test_stirrup (ctest
geometry.pipe_square_section): the closed stirrup of lines and arcs whose
turns at slight kinks overran the intersection's array: a valid solid
within 0.2 % of the circle's area times the path's length. Its ctest
failed without the patch on 2026-11-20, but the run of the named test
alone selected no test (the runner's MITCAD_TEST_ONLY matches the
guarded() label, not the function's name; tools/occt-port-proof.py now
maps one to the other). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1666

[1666-brepfill-trim-shell-corner-round-along-the-bisector-plane-where-the-sides-intersection-breaks-up.patch](../opencascade/1666-brepfill-trim-shell-corner-round-along-the-bisector-plane-where-the-sides-intersection-breaks-up.patch)

Test: geometry/tests/test_pipe_square_section.cpp
(geometry.pipe_square_section: the pipes with rounded corners).

## 1670

[1670-chfi3d-end-crossing-on-the-next-edge-of-the-faces-common-boundary.patch](../opencascade/1670-chfi3d-end-crossing-on-the-next-edge-of-the-faces-common-boundary.patch)

Test: geometry/tests/test_fillet_end_past_split_edge.cpp,
test_end_past_split_edge (ctest geometry.fillet_end_past_split_edge): a
flange's foot rounded with 6.5, 7 and 8 past an end cap 2 short of its
tip: one valid closed solid, every edge on two faces, the exact volume; 7
and 8 left a free edge without the patch, 6.5 (the boundary) built before
it too. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1680

[1680-chfi3d-corner-join-face-classifier-set-up-once-per-face.patch](../opencascade/1680-chfi3d-corner-join-face-classifier-set-up-once-per-face.patch)

Test: none, because the patch changes only the time of the join across a
corner (each face's bounds, projection and explorer set up once, the same
answers for every point); the fillet and chamfer tests of geometry
(geometry.suite.fillet_*, geometry.fillet_*) check that the results are
unchanged, and the gain (on a threaded design's join) is below what a
bound in a test of seconds can tell on a loaded machine.

## 1690

[1690-brepblend-line-points-read-by-index-in-constant-time.patch](../opencascade/1690-brepblend-line-points-read-by-index-in-constant-time.patch)

Test: geometry/tests/test_kernel_repeated_work.cpp,
test_blend_line_keeps_order_and_addresses (ctest
geometry.kernel_repeated_work): the line's points in order and at their
addresses through appends, prepends, inserts and removals. The speed
itself: none, because the patch changes no result
(geometry.fillet_consumed_strip and
geometry.offset_loft_circle_seam_lean_in give stock OCCT's results), and
its gain is below what a bound in a test of seconds can tell on a loaded
machine.

## 1691

[1691-intcurvesurface-surface-polyhedra-made-once-per-face.patch](../opencascade/1691-intcurvesurface-surface-polyhedra-made-once-per-face.patch)

Test: geometry/tests/test_kernel_repeated_work.cpp
(geometry.kernel_repeated_work: the intersections with the kept polyhedra
are those made without them). The speed itself: none, because the gain (a
third of a closed offset's time) is below what a bound in a test of
seconds can tell on a loaded machine.

## 1695

[1695-inttools-wline-decomposition-reads-no-point-past-the-line-ends.patch](../opencascade/1695-inttools-wline-decomposition-reads-no-point-past-the-line-ends.patch)

Test: geometry/tests/test_dressup_consumes_face.cpp, the chains part
(ctest geometry.dressup_consumes_face.chains): its third row, a concave
two-distance chamfer of 0.99 of the strip's width along a tangent chain
over a cylinder strip, whose reference boolean read past the walking
line's ends (a crash in a release build, Standard_OutOfRange in a debug
build). Without the patch the chains part crashed (exit -11). It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1696

[1696-chfi3d-narrow-face-try-refused-by-its-analytic-pieces-walks-no-guide-past-the-spine.patch](../opencascade/1696-chfi3d-narrow-face-try-refused-by-its-analytic-pieces-walks-no-guide-past-the-spine.patch)

Test: geometry.fillet_consumed_strip (the fillet within 15 s, the same
volume and checks as before); tools/ui-fillet-chain-remark-test.sh.

## 1700

[1700-chfi3d-walked-end-runs-on-over-the-flat-stretch-beyond-a-curved-wall.patch](../opencascade/1700-chfi3d-walked-end-runs-on-over-the-flat-stretch-beyond-a-curved-wall.patch)

Test: geometry/tests/test_fillet_wall_run_on.cpp, test_reported_body
(ctest geometry.fillet_wall_run_on): on the stored case
`fillet-wall-run-on` of MITCAD_GEOMETRY_CASES, the outer foot rounded with
3 by OCCT's fillet alone: valid, the flat stretches gone, the foot on the
plate that of a ball on the plate and the curved side continued by its
polynomial within 0.01 mm, the volume a little under the run-on over the
flat stretch's plane (by at most a twentieth of the material added);
documented to fail without this patch.
No synthetic body: a U of cubic B-spline sides went another way at its
ends (invalid with and without the patch). It fails without the patch
(shown 2026-11-20 by tools/occt-port-proof.py).

## 1701

[1701-chfi3d-contact-of-no-length-joined-with-none-and-blends-in-no-solid-no-result.patch](../opencascade/1701-chfi3d-contact-of-no-length-joined-with-none-and-blends-in-no-solid-no-result.patch)

Test: geometry/tests/test_fillet_rib_full_round.cpp,
test_cube_rounded_within_a_hair_of_half_its_side (ctest
geometry.fillet_rib_full_round): the cube of 10 rounded on its twelve
edges with 4.9998 returns (before the patch it ran into the test's time
limit), either not done or one valid solid with the exact shape's volume,
never the cube as done. It fails without the patch: the test does not
build without it (shown 2026-11-20 by tools/occt-port-proof.py).

## 1703

[1703-chfi3d-regular-limit-normal-at-singular-end.patch](../opencascade/1703-chfi3d-regular-limit-normal-at-singular-end.patch)

Test: geometry/tests/test_fillet_lens_overflow.cpp (ctest
geometry.fillet_lens_overflow):
test_rounding_rolling_into_a_vertex_where_it_vanishes and
test_chain_through_points_where_the_rounding_vanishes, the block-on-post's
vanishing vertex: valid, its end points, the four quarters' volumes and
the symmetry. The rank-deficient end normal showed only on Apple Silicon;
the failure without the patch is to be shown on a macOS arm64 build (on
x86-64 the end normal is regular and the test passes either way).

## 1710

[1710-chfi3d-overlapping-chains-judged-by-the-spine-edges-their-blends-are-on.patch](../opencascade/1710-chfi3d-overlapping-chains-judged-by-the-spine-edges-their-blends-are-on.patch)

Test: geometry/tests/test_fillet_top_chains.cpp, test_reported_body (ctest
geometry.fillet_top_chains): on the stored case `fillet-wall-run-on` of
MITCAD_GEOMETRY_CASES, the fin top's outer and inner chains rounded with
0.5 by OCCT's fillet alone: valid, taking what the two chains take alone
within a thousandth; documented to fail without the patch
(Failure_Overlap). The same function also rounded the foot (1711 and
1728) until mitcad#2310 gave that part a function of its own, and the
test's executable runs each function alone with MITCAD_TEST_ONLY since
then (before, a run alone ran them all, and at the patch's place the
later patches' cases crashed it). It fails without the patch at its place
in the chain: OCCT with the patches before it, without and with this one
(shown 2026-11-23 by tools/occt-port-proof.py --in-chain).

## 1711

[1711-chfi3d-corner-parse-meeting-an-edge-twice-fails.patch](../opencascade/1711-chfi3d-corner-parse-meeting-an-edge-twice-fails.patch)

Test: none, because no small body reaches the parse's repeat. The parse
starts from the face that the first stripe's surfdata at the vertex
holds on its second side (SurfIndex FACE2) and goes on round the vertex
with ChFi3d_cherche_edge, which leaves Enext as it was where the face has
no edge at the vertex not met yet. An edge is met twice only where that
starting face is a face at the vertex that is not a face of the stripe's
own edge: the stripe's end leans on a face beyond one its blend consumed,
which the run-on of patch 1700 makes (a rounding of 3 over a curved wall's
flat stretch lower than the radius). Every face round the vertex then
takes the parse once at most, and it stops short. Tried by reading the
code (mitcad#2121), not by runs: a box corner of three roundings, whose
stripes' ends lean on the faces of their own edges, so the parse closes;
a corner with a face narrower than the radius, whose stripe would lean
on the face beyond only through the overflow and ride routes (0013, the
tangent rides of 1720-1729, the corners of three stripes of 1781), which
reach the corner by other code first, so a small body there cannot be
said to reach the repeat. Patch 1700 itself has no synthetic body (a U
of cubic B-spline sides went another way at its ends), and at the port's
tip the reported body's corner no longer reaches the repeat either:
geometry.fillet_top_chains (the stored case `fillet-wall-run-on` of
MITCAD_GEOMETRY_CASES, the top's chains with the foot's rounding of 3: the
fillet returns, valid; then part of test_reported_body, since mitcad#2310
test_outer_arm_ends_with_the_foot) passes without the patch
(shown 2026-11-20 by tools/occt-port-proof.py). The patch stays as a guard:
without it such a parse reads past a stripe's surfdata and does not
return.

## 1720

[1720-chfi3d-rounding-rides-tangent-on-an-earlier-roundings-blend.patch](../opencascade/1720-chfi3d-rounding-rides-tangent-on-an-earlier-roundings-blend.patch)

Test: geometry/tests/test_fillet_tangent_ride.cpp (ctest
geometry.fillet_tangent_ride): test_wall (2.9, 2.5 and 2.127 wide),
test_tower_foot (2.9, 2.5 and 2.2) and test_step (1.45 and 1.3 high), each
in both orders: valid, the volume of the section within a millionth;
without the patch the wall's result has more material and the other two
fail. The test's executable runs each function alone with
MITCAD_TEST_ONLY since mitcad#2310 (before, a run alone ran them all, and
at the patches' places in the chain the later patches' cases failed). It
fails without the patch at its place in the chain: OCCT with the patches
before it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain).

## 1721

[1721-chfi3d-rounding-rides-tangent-on-an-earlier-roundings-blend-part-of-the-way.patch](../opencascade/1721-chfi3d-rounding-rides-tangent-on-an-earlier-roundings-blend-part-of-the-way.patch)

Test: geometry/tests/test_fillet_tangent_ride.cpp (ctest
geometry.fillet_tangent_ride): test_narrowing_wall (either edge slanting,
either end narrow) and test_falling_step, each in both orders: valid, the
volume of the two roundings made one after the other within a millionth
(the wall's result 0.45 mm3 more without the patch). It fails without the
patch at its place in the chain: OCCT with the patches before it, without
and with this one (shown 2026-11-23 by tools/occt-port-proof.py
--in-chain).

## 1722

[1722-chfi3d-tangent-ride-ending-at-a-corner-with-one-more-rounding.patch](../opencascade/1722-chfi3d-tangent-ride-ending-at-a-corner-with-one-more-rounding.patch)

Test: geometry/tests/test_fillet_tangent_ride.cpp, test_wall_end_corners
(ctest geometry.fillet_tangent_ride): the wall 2.5 wide with an end's edge
rounded too (0.5 or 1, either side, one or both ends), a prism and a box,
both orders: valid, the volume within a millionth; and
geometry.fillet_top_chains, test_tower_top on the stored case
`fillet-wall-run-on` (without the legs' edges). Both functions held
1723's cases too until mitcad#2310 gave those functions of their own, and
both executables run each function alone with MITCAD_TEST_ONLY since then.
It fails without the patch at its place in the chain: OCCT with the
patches before it, without and with this one (shown 2026-11-23 by
tools/occt-port-proof.py --in-chain).

## 1723

[1723-chfi3d-tangent-ride-corner-of-three-blends.patch](../opencascade/1723-chfi3d-tangent-ride-corner-of-three-blends.patch)

Test: geometry/tests/test_fillet_tangent_ride.cpp,
test_end_corners_of_three_blends (ctest geometry.fillet_tangent_ride): the
end edges rounded past the ride's point (1.2 and 1.5 beside the rounding
of 2, 1.7 beside the rounding of 1), prism and box, both orders: valid, the
volume within a millionth; and geometry.fillet_top_chains,
test_tower_top_with_the_legs and test_thirteen_sets on the stored case
`fillet-wall-run-on` (functions of their own since mitcad#2310; in the run
below the first two were cases of 1722's functions, run with them). It
fails without the patch (shown 2026-11-20 by tools/occt-port-proof.py,
reverted together with 1781, which changes the same lines).

## 1724

[1724-chfi3d-tangent-ride-between-two-crossings.patch](../opencascade/1724-chfi3d-tangent-ride-between-two-crossings.patch)

Test: geometry/tests/test_fillet_tangent_ride.cpp,
test_tower_behind_the_back_edge (ctest geometry.fillet_tangent_ride): a
half-cylinder tower 2.3 and 2.5 from the plate's back edge, its foot
rounded with 2 and the edge with 1, both orders: valid, the volume of the
two made one after the other within a millionth (the fillet failed without
the patch). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 1725, 1727, 1728, 1729,
which change the same lines).

## 1725

[1725-chfi3d-tangent-ride-between-two-crossings-of-a-closed-rider.patch](../opencascade/1725-chfi3d-tangent-ride-between-two-crossings-of-a-closed-rider.patch)

Test: geometry/tests/test_fillet_tangent_ride.cpp,
test_tower_behind_a_rounded_rim (ctest geometry.fillet_tangent_ride): the
plate's corners rounded in plan so that the back edge's rounding is a
closed contour, the tower 2.3 and 2.5 from it at the middle and 3 off it,
both orders: valid, the volume within a millionth (the fillet failed
without the patch). It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1726

[1726-chfi3d-under-anothers-ball-only-where-its-own-ball-is-found.patch](../opencascade/1726-chfi3d-under-anothers-ball-only-where-its-own-ball-is-found.patch)

Test: geometry/tests/test_fillet_top_chains.cpp, test_pit_beside_the_foot
(ctest geometry.fillet_top_chains): on the stored case
`fillet-wall-run-on`, the fin's foot rounded with 3 and the pit's edge
with 5 in one fillet: valid, the pit's cylinder of 5 in the result, the
volume of the two made alone within 0.01 mm3 (without the patch the pit's
rounding was left out); and test_sets and test_thirteen_sets with and
without the pit's edges. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1727

[1727-chfi3d-ride-onto-the-corner-at-the-other-stripes-other-end.patch](../opencascade/1727-chfi3d-ride-onto-the-corner-at-the-other-stripes-other-end.patch)

Test: geometry/tests/test_fillet_top_chains.cpp, test_inner_arm_ends
(ctest geometry.fillet_top_chains): on the stored case
`fillet-wall-run-on`, both inner arm ends built together, valid, the
ride's ends on the plate at the foot's end and on the top's prolonged
edge. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 1728, 1729, which change
the same lines).

## 1728

[1728-chfi3d-ride-on-the-other-stripe-at-an-end-where-the-face-between-narrows-to-nothing.patch](../opencascade/1728-chfi3d-ride-on-the-other-stripe-at-an-end-where-the-face-between-narrows-to-nothing.patch)

Test: geometry/tests/test_fillet_top_chains.cpp (ctest
geometry.fillet_top_chains): test_outer_arm_ends_with_the_foot, both
outer arm ends (the foot's rounding and the top's edges) valid with the
ride's ends where the walk puts them (the 0.5's ends at the exact end on
the foot's line; a part of 1710's test function in the run below, a
function of its own since mitcad#2310), and
test_whole_fillet1, the design's whole rounding valid within 4 mm3 of the
stored body's with every vertex within 0.05 (the ride approximated by the
guide's parameter left 0.4 to 1.1 mm, mitcad#2075); both on the stored case
`fillet-wall-run-on`. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py, reverted together with 1729, which changes the
same lines).

## 1729

[1729-chfi3d-end-inside-the-stretch-another-blend-takes-away-fails.patch](../opencascade/1729-chfi3d-end-inside-the-stretch-another-blend-takes-away-fails.patch)

Test: geometry/tests/test_fillet_top_chains.cpp, test_one_inner_arm (ctest
geometry.fillet_top_chains): on the stored case `fillet-wall-run-on`, one
inner arm's foot rounded and the other's not, with the top's edges: never
done and not valid (done but invalid, the plate's wire open, without the
patch); and geometry.suite.fillet_corners, the two roundings mitred at a
short edge (0210) still build. It fails without the patch (shown
2026-11-20 by tools/occt-port-proof.py).

## 1730

[1730-chfi3d-kpart-contact-end-at-a-vertex-only-within-its-own-tolerance.patch](../opencascade/1730-chfi3d-kpart-contact-end-at-a-vertex-only-within-its-own-tolerance.patch)

Test: geometry.port.1730
(geometry/tests/port/p1730-contact-end-at-a-vertex-within-its-own-tolerance.cpp):
test_chamfer_crossing_an_arc_beside_a_wide_vertex_stays_on_it and
test_rounding_crossing_an_arc_beside_a_wide_vertex_stays_on_it: a 10
cube made of two halves fused at x = 5, the far end W of the arc S
between the two tops of tolerance 0.7, the front's top edge chain
chamfered or rounded with 0.5: one analytic piece per edge, the contact
on the top crossing S 0.5 from the spine's vertex V there, inside S
(without the patch both pieces' ends were drawn to V, 0.5 <= 0.7): valid,
a vertex at the crossing, none at V, the exact volume.
test_crossing_an_arc_beside_a_vertex_under_the_distance: the boundary, W
of tolerance 0.45, the same result.
test_chamfer_and_rounding_ending_beside_a_wide_vertex: the same at the
spine's end of a plain cube, where the corner builds the end anew (a
witness that such ends stay as they were). Also cli.f3d_edit_reported
(the reported design's Chamfer3 on the kernel's Fillet1 result). It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1760

[1760-blendfunc-constrad-ball-beyond-its-support-walked-in-the-guides-normal-plane.patch](../opencascade/1760-blendfunc-constrad-ball-beyond-its-support-walked-in-the-guides-normal-plane.patch)

Test: geometry/tests/test_fillet_ball_beyond_support.cpp, a block drafted
by 2.5 degrees, its corners rounded with 3, its top's rim rounded with 4,
6 and 8 (without the patch 6 failed and 4 was invalid) and on both sides
of the corners' radius (2.985 to 3.015: 3.005 and 3.015 were invalid), by
OCCT's fillet alone.

## 1770

[1770-chfi3d-start-of-a-rounding-along-a-knife-edge.patch](../opencascade/1770-chfi3d-start-of-a-rounding-along-a-knife-edge.patch)

Test: geometry/tests/test_fillet_knife_edge.cpp (ctest
geometry.fillet_knife_edge): test_two_cylinders, two cylinders tangent to
each other rounded at their knife edge with 1 and 5 (no start without the
patch), seams on the edge and arcs through three points, each mirrored,
the exact volume within 1e-6 relative; test_reported_knife_edge and
test_with_ordinary_edges; and the boundary test_radius_too_large (25
fails). Also geometry.suite.dressup, test_fillets_of_knife_edges. It fails
without the patch (shown 2026-11-20 by tools/occt-port-proof.py).

## 1780

[1780-chfi3d-convex-and-concave-roundings-crossing-before-ends-joined-by-a-sharp-edge.patch](../opencascade/1780-chfi3d-convex-and-concave-roundings-crossing-before-ends-joined-by-a-sharp-edge.patch)

Test: geometry/tests/test_fillet_linked_ends.cpp: the block's foot and top
rounded together with 2 and 0.5 at front faces 1.5 and 2 high: OCCT's
fillet reports the overlap with the four contours (an invalid result
without the patch); the rounding of the foot on the result of the top's
rounding is valid (its front face's wire not closed without the patch);
MitCad's fillet of both is valid and equals that result. At 3 high the
contacts do not cross and the roundings are built together as before.

## 1781

[1781-chfi3d-corners-of-three-stripes-before-corners-of-two.patch](../opencascade/1781-chfi3d-corners-of-three-stripes-before-corners-of-two.patch)

Test: geometry/tests/test_fillet_top_chains.cpp, test_whole_fillet1: the
whole of Fillet1 on the case `fillet-wall-run-on` of
MITCAD_GEOMETRY_CASES in the listed order and with the top's 0.5 right
after the foot's 3: every vertex within 0.05 and the same volume (without
the patch 0.66 and 0.14 mm3 apart).

## 1790

[1790-chfi3d-corner-where-a-knife-edge-runs-on-along-its-line-refused.patch](../opencascade/1790-chfi3d-corner-where-a-knife-edge-runs-on-along-its-line-refused.patch)

Test: geometry/tests/test_fillet_knife_chain.cpp: the block with the bite,
the knife edge and the square edge on its line in one fillet with 1 and 3:
not done, the corner failed at the vertex on the bite's end plane with the
message above (before the patch: done, a valid body 6611 mm3 larger than
the expected one at 1); the knife edge alone valid with the exact volume;
MitCad's fillet of the chain valid with the exact volume.

## 1800

[1800-chfi3d-analytic-line-round-the-period-outside-a-blends-domain-passed-over.patch](../opencascade/1800-chfi3d-analytic-line-round-the-period-outside-a-blends-domain-passed-over.patch)

Test: cli.fcstd_bend_reported (tools/cli/fcstd-bend-reported-test.cmake),
the reported bent guide imported with its bend radius set to 32: every
feature rebuilds and the body stays within its two pads' ends (before: the
rounding at a pad's end reached 3.8 beyond it). No body built from OCCT's
primitives alone reproduced the line round the period: it depends on the
input's last bits.

## 1801

[1801-chfi3d-corner-set-back-past-a-short-sharp-edge-walks-over-its-vertex.patch](../opencascade/1801-chfi3d-corner-set-back-past-a-short-sharp-edge-walks-over-its-vertex.patch)

Test: geometry.fillet_corner_walk_over_vertex
(geometry/tests/test_fillet_corner_walk_over_vertex.cpp) with the case
fillet-corner-walk-over-vertex of MITCAD_GEOMETRY_CASES, and
cli.fcstd_bend_reported (tools/cli/fcstd-bend-reported-test.cmake), the
reported bent guide with its bend radius set to 20, 24, 25, 30 and 32.

## 1802

[1802-brepblend-walk-whose-first-point-lies-on-a-restriction-ends-there.patch](../opencascade/1802-brepblend-walk-whose-first-point-lies-on-a-restriction-ends-there.patch)

Test: geometry.fillet_corner_walk_over_vertex (the case's second body) and
cli.fcstd_bend_reported (the bend radius set to 24).

## 1803

[1803-chfi3d-border-from-a-point-walked-onto-leaves-it-at-no-set-angle.patch](../opencascade/1803-chfi3d-border-from-a-point-walked-onto-leaves-it-at-no-set-angle.patch)

Test: cli.fcstd_bend_reported, the reported bent guide with its bend
radius set to 24.5 and 25 exported as STL at deviations 0.0005, 0.002, 0.01
and 0.05 mm with an angle of 10 degrees.

## 1804

[1804-brepmesh-polygons-crossing-at-a-cusp-of-a-wire-untangled.patch](../opencascade/1804-brepmesh-polygons-crossing-at-a-cusp-of-a-wire-untangled.patch)

Test: cli.fcstd_bend_reported, the reported bent guide with its bend
radius set to 24.5, 25 and 32 exported as STL at deviations from 0.0005 to
0.05 mm with angles of 10 degrees, and at 0.002 mm with 5 degrees and at
0.0005 and 0.002 mm with 2 degrees.

## 1810

[1810-hlr-polygonal-hiding-pass-in-parallel-through-a-grid-of-the-hiding-faces.patch](../opencascade/1810-hlr-polygonal-hiding-pass-in-parallel-through-a-grid-of-the-hiding-faces.patch)

Test: geometry.port.1810 (geometry/tests/port/p1810-polygonal-hiding-in-
parallel.cpp): a frame of 970 bodies seen almost along its rows, every
kind of visible and hidden line, of all the bodies and of each, equal to
the last bit and in the same order to the walk over every shell written
out with the algorithm's public parts (stock OCCT's Hide), and the pass
in less than half the walk's time (0.04 s against 0.45 s; stock OCCT's
pass takes the walk's time and fails that bound); one body, and a view of
none.

## 1820

[1820-stepcafcontrol-sub-shape-names-skip-what-the-reader-left-unresolved.patch](../opencascade/1820-stepcafcontrol-sub-shape-names-skip-what-the-reader-left-unresolved.patch)

Test: geometry.port.1820 (geometry/tests/port/p1820-step-sub-shape-names-
unresolved-bound.cpp): a box as a MANIFOLD_SOLID_BREP of a named product,
one face with a second FACE_BOUND whose bound is an entity not in the
file, read through STEPCAFControl_Reader with sub-shape names: stock OCCT
crashes; with the patch it is one valid solid of 1000 mm3 with six faces
under the product's name. geometry.io reads the same text with
read_step_structured.

## 1830

[1830-chfi3d-end-cut-across-the-faces-beside-a-tangent-face.patch](../opencascade/1830-chfi3d-end-cut-across-the-faces-beside-a-tangent-face.patch)

Test: geometry.port.1830 (geometry/tests/port/p1830-end-cut-across-a-tangent-face.cpp),
the half-disc block against the half-cylinder wall rounded and chamfered
with 0.5, 1 and 3: one valid solid each, the added volume that of the
section cut by both cylinders continued within 1e-5; the wall's top rounded
with 3 only at its edges, the side plane crossed: as before.

## 1840

[1840-tkopengl-lines-wider-than-a-pixel-in-a-core-profile-drawn-as-quads.patch](../opencascade/1840-tkopengl-lines-wider-than-a-pixel-in-a-core-profile-drawn-as-quads.patch)

Test: none in geometry/tests, because the defect exists only in a
core-profile context, which the OCCT-only tests do not create. The test
is app.ui-macos-selection-look (tools/ui-scripts/selection-look.mitcad-ui,
its view-line-width steps): it reads the macOS view's pixels back, a
selected edge at least 3 logical pixels wide at Normal and wider at
Thick, a sketch line at least 2 (less the driver's half pixel). Checked
with OCCT alone in a core profile (Mesa llvmpipe, OpenGL 4.5 core):
widths 4 and 6 drawn 4 and 6 pixels wide (1 without the patch), width 1
unchanged, a dashed line of width 4 dashed, a line on a shaded face at
its depth drawn over it and one under the face hidden.

## 1850

[1850-chfi3d-rounding-wider-than-a-walls-top-rolls-on-its-inner-edge-cut-by-the-slope-past-its-end.patch](../opencascade/1850-chfi3d-rounding-wider-than-a-walls-top-rolls-on-its-inner-edge-cut-by-the-slope-past-its-end.patch)

Test: geometry.port.1850
(geometry/tests/port/p1850-rounding-wider-than-a-walls-top-rolls-on-its-inner-edge.cpp):
test_rounding_wider_than_the_top_rolls_on_its_inner_edge: a plate whose
floor lies 4 below the top of a wall 2 wide along its outer side, the
top's end edges chamfered with 3 (slopes of 45 degrees), the wall's
outer top edge rounded with 2.5: the side over the top crosses the inner
wall (no ball touches it) and reaches the floor within 2r = 5 (without
the patch the ball on the outer wall and the floor took the wall away);
valid, in the middle the wall kept below the ball's arc rolling on the
inner top edge and gone above it, and the volume of a slab there exact;
the ends rolled on past the spine's ends and cut by the slopes.
test_rounding_wider_than_the_top_beside_a_high_inner_wall: the boundary,
the inner wall 6 high (past 2r, the floor beyond the ball's reach, where
the start rolled on the inner top edge before the patch too): the same
rounding. It fails without the patch (shown 2026-11-20 by
tools/occt-port-proof.py).

## 1860

[1860-chfi3d-torus-corner-with-unequal-sides-runs-on-to-where-the-smaller-blend-crosses-it.patch](../opencascade/1860-chfi3d-torus-corner-with-unequal-sides-runs-on-to-where-the-smaller-blend-crosses-it.patch)

Test: geometry.fillet_unequal_torus_corner.

## 1870

[1870-brepblend-approximation-solves-sections-on-the-walked-ball.patch](../opencascade/1870-brepblend-approximation-solves-sections-on-the-walked-ball.patch)

Test: geometry.port.1870
(geometry/tests/port/p1870-approximation-keeps-the-walked-ball.cpp),
test_foot_of_a_wide_low_step_rolled_on_its_rim and
test_foot_of_a_wider_low_step_rolled_on_its_rim: a plate with a round step
0.2 high, its foot rounded with 0.4 at the step's radii 100 and 200 and
with 1 at 200: one valid solid, its tolerances within 1e-4, the rolling
ball's volume within 1e-5, the step's side gone; the radius 50 as before.
On the port's tip before the patch the three wide cases failed and the
radius 50 built (mitcad#2161). Its failure without the patch is still to
be shown in the port's before/after run (mitcad#1986).

## 1805

[1805-chfi3d-corner-of-one-rounding-sets-back-along-its-concave-edges-only.patch](../opencascade/1805-chfi3d-corner-of-one-rounding-sets-back-along-its-concave-edges-only.patch)

Test: geometry.fillet_rib_foot_corner (test_rounding: a rib bridging an
open channel, its end's edge with its underside rounded at its foot on the
rim; without the patch its plate reaches 0.015 out of the body and the flat
past the rib loses a piece), cli.fcstd_bend_reported (every feature's
status ok at every radius).

## 2031

[2031-tkopengl-window-less-egl-context-draws-into-its-default-fbo-not-the-back-buffer.patch](../opencascade/2031-tkopengl-window-less-egl-context-draws-into-its-default-fbo-not-the-back-buffer.patch)

Test: none in geometry/tests, because the defect needs a window-less EGL
context of a driver that refuses GL_BACK there, which the OCCT-only tests
do not create. Checked with the reproducer above on NVIDIA 580.178.04
(Wayland EGL display, OpenGL 4.6 compatibility): without the patch the
error after SetWindow and both messages at the first frame and after a
resize; with it no error, the 4-sample scene FBO made at both sizes. In
MitCad on a headless GNOME Shell Wayland compositor on the same GPU, the
messages are gone and the view draws.

## 2140

[2140-brepoffset-crease-fading-into-a-smooth-face-keeps-its-branch-alone.patch](../opencascade/2140-brepoffset-crease-fading-into-a-smooth-face-keeps-its-branch-alone.patch)

Test: geometry/tests/port/p2140-crease-fading-into-a-smooth-face.cpp (ctest
geometry.port.2140), test_y_tube_shelled_inwards_with_its_ends_open: the
Y tube made of OCCT primitives (a cylinder and two quarter tori, fused),
shelled 2 mm inwards with its three ends open, Intersection join: one valid
solid, the wall 2 mm thick by points in the trunk, in both arms and under
the crotch, the volume that of the tube less the same tube of radius 8;
and geometry/tests/test_y_tube_crotch.cpp (ctest geometry.y_tube_crotch),
the same through MitCad's sweep, mirror and shell. It fails without the
patch (shown 2026-11-23 by tools/occt-port-proof.py).

## 2141

[2141-chfi3d-trace-of-the-walk-of-a-rounding.patch](../opencascade/2141-chfi3d-trace-of-the-walk-of-a-rounding.patch)

Test: none, because the patch only adds a trace and changes no result;
geometry/tests/test_y_tube_crotch.cpp --probe sets the variable round the
fillets it tries.

## 2142

[2142-chfi3d-no-start-on-a-section-narrower-than-a-vanishing-end.patch](../opencascade/2142-chfi3d-no-start-on-a-section-narrower-than-a-vanishing-end.patch)

Test: geometry/tests/test_y_tube_crotch.cpp (ctest geometry.y_tube_crotch),
test_crotch_rounded_6 and test_crotch_rounded_50: the Y tube's crotch
rounded with 6 and 50, one valid solid without a note or a smaller
radius, ending at both vanishing ends, passing the crotch's middle at the
ball's lowest point. It fails without the patch (shown 2026-11-23 by
tools/occt-port-proof.py).

## 2143

[2143-brepoffset-apex-cut-only-where-its-construction-has-a-solution.patch](../opencascade/2143-brepoffset-apex-cut-only-where-its-construction-has-a-solution.patch)

Test: geometry/tests/test_y_tube_crotch.cpp (ctest geometry.y_tube_crotch),
test_shell_after_rounding_6 and test_shell_after_rounding_50, and
cli.y_tube_crotch_reported. It fails without the patch (shown 2026-11-23
by tools/occt-port-proof.py).

## 2170

[2170-bopalgo-wiresplitter-vertex-met-a-period-apart.patch](../opencascade/2170-bopalgo-wiresplitter-vertex-met-a-period-apart.patch)

Test: geometry/tests/port/p2170-vertex-met-a-period-apart.cpp (ctest
geometry.port.2170), test_face_touching_itself_a_period_apart_rebuilt and
test_face_touching_itself_a_period_apart_cut: a cylinder's face without a
seam, built from scratch, whose outer wire passes one vertex at u = 0 and
at u = 2 pi and whose triangular hole touches it there, split by the
boolean's face builder from its own edges (one face of two wires) and
with a section line across it (two faces), in every order of the edges:
valid faces holding the face's area. The reported join of 31 bodies
(geometry.join_holds_inputs, from MITCAD_MITCAD_REPORTED) is one valid
body holding every body. Its failure without the patch is still to be
shown in the port's before/after run (mitcad#1986).

## 2050

[2050-brepoffset-intersection-round-a-periodic-curve-trimmed-over-its-edges-across-the-origin.patch](../opencascade/2050-brepoffset-intersection-round-a-periodic-curve-trimmed-over-its-edges-across-the-origin.patch)

Test: geometry/tests/test_shell_boss_on_corner.cpp,
test_boss_on_a_rounded_corner (ctest geometry.shell_boss_on_corner): the
plate 75 x 50 x 10 rounded 15 with the cylinder of radius 15 and 10 high on
a corner's axis, also with the radius and centre the reporter's sketch
solved to (14.999999999999996, 4e-15 off) and with the merged face's seam
where the cylinder meets the plate's side (as the reporter's design has
it), shelled 2 mm inside and outside with the cylinder's top and the
plate's bottom open, and offset 2 mm inward closed, with OCCT alone: valid
shapes of the volumes from the areas of the rounded rectangles and circles
within 1e-6. The reporter's design, with and without its roundings, is
shelled by tools/cli/shell-reported-test.cmake (never committed, read from
MITCAD_MITCAD_REPORTED). It fails without the patch (shown 2026-11-21 by
tools/occt-port-proof.py).

## 2228

[2228-chfi3d-rounding-whose-contact-line-folds-walked-in-pieces-the-twisted-one-filled-between-its-neighbours.patch](../opencascade/2228-chfi3d-rounding-whose-contact-line-folds-walked-in-pieces-the-twisted-one-filled-between-its-neighbours.patch)

Test: geometry.port.2228
(geometry/tests/port/p2228-contact-line-folding-at-crests.cpp),
test_wave_top_edge_rounded_past_its_crests: the band 24 wide and 10 high
between two sine waves (amplitude 8, wavelength 30), made with OCCT alone,
its walls bending at the crests with 2.85 mm, its upper top edge rounded
with OCCT's fillet at 2.5, 3, 5 and 8.5: done, one valid solid, the volume
(OCCT's Gauss-Kronrod integration; its default rule with an accuracy gave
the band 15382.3, not 14400) falling as the radius grows, by more than
half the corner's section (1 - pi / 4) r^2 along the edge and less than
r^2 along it; at 2.5 that section along the edge within 0.2 %. Without the
patch 3, 5 and 8.5 fail (the walk stops at the first cusp). Also geometry.fillet_wave_crests (the
same band through MitCad's fillet, one edge and both top edges; its
`--probe` traces OCCT's fillet at each radius) and cli.wavy_fillet_reported
(the reported STEP model and the other program's `.f3d` result, from
MITCAD_MITCAD_REPORTED). Its failure without the patch is still to be shown
in the port's before/after run (mitcad#1986).

## 2291

[2291-chfi3d-tight-rim-corner-obtuse-by-the-walls-normal-at-the-ball.patch](../opencascade/2291-chfi3d-tight-rim-corner-obtuse-by-the-walls-normal-at-the-ball.patch)

Test: geometry/tests/port/p2291-tight-rim-corner-obtuse-at-the-ball.cpp
(ctest geometry.port.2291),
test_top_rounded_past_a_cut_out_rim_whose_axis_lies_in_the_top: a block
whose top meets a concave quarter cylinder of radius 10, its axis in the
top's plane, the end face's edge with the cylinder rounded with 2 and then
its edge with the top with 3, with OCCT alone: valid, free of
self-interference, no face under 0.01 mm2, the corner's faces on the
surface the first rounding's own ball sweeps within 5e-4 mm and on the
ball by the centre along their normals within 2e-3 mm (the faces fitted
within 1e-4, the normals' error times rk = 2) (before, a torus running back over the
first rounding and a face of 0.0063 mm2);
test_top_rounded_with_the_rims_radius_stays_clean, the boundary (2 for
both): valid and clean with and without the patch. The reporter's design
is opened by tools/cli/fillet-tight-rim-corner-reported-test.cmake (never
committed, read from MITCAD_MITCAD_REPORTED). In the OCCT lab
(2026-11-23, its case 2291-cut-out-rim-corner) the same rounding by the
chain without the patch was invalid and self-interfering (4383.637326
mm3), with it valid and free of self-interference (4355.522879 mm3); the
lab's other 33 cases did not change. Not yet shown failing without the
patch by tools/occt-port-proof.py.

## 2284

[2284-brepgprop-gauss-joints-of-a-faces-pcurves-integrated.patch](../opencascade/2284-brepgprop-gauss-joints-of-a-faces-pcurves-integrated.patch)

Test: geometry/tests/port/p2284-joints-of-a-faces-pcurves-integrated.cpp
(ctest geometry.port.2284), test_joint_apart_along_v_integrated,
test_pcurve_off_its_edge_closed_in_space and test_closed_loops_unchanged:
half a cylinder of radius 10 and height 10, built from scratch, one of
whose edges' pcurves on its side face is changed by 1e-6 mm (the edge's
and its vertices' tolerances 1e-5 mm): valid bodies whose volume about
their middle and about points 100 mm away along X, Y and Z agrees within
1e-9 relative and equals pi R^2 H / 2 within 1e-9. The side line's pcurve
at u = pi ending 1e-6 short of the top: without the patch the volume
moves by 4.2e-7 relative along Y (the strip the joint leaves out). The
top arc's pcurve moved 1e-6 above its 3D curve: closing the joints alone
moved it by 4.2e-7 (the patch's first form, in the bundle check); the
ribbon to the 3D curve takes the strip away again. The half cylinder as
built keeps its volume. geometry.suite.fillet_corners,
test_two_roundings_mitred_at_a_short_edge: the patch's first form (the
joints alone) measured 16.0707129 mm3 removed against 16.0693324 from
booleans, past the test's 5e-5. The reported ring's
common with the union (geometry.join_holds_inputs, from
MITCAD_MITCAD_REPORTED) is within 1e-6 of the ring both ways and its
volume about each of three points within 1e-9 of the ring's.
geometry.fillet_tight_rim_corner, test_reported_body: each of the stored
bodies before and after the rounding and OCCT's rounding moves by at most
1e-3 mm3 with the point it is integrated about (without the patch 0.087,
0.132 and 0.144 mm3, shown in the OCCT lab's case 2284-tube-rim-volumes).
Its failure without the patch is still to be shown in the port's
before/after run (mitcad#1986).

## 2229

[2229-chfi3d-cap-leaving-the-end-face-across-a-tangent-edge-cut-by-the-face-beyond.patch](../opencascade/2229-chfi3d-cap-leaving-the-end-face-across-a-tangent-edge-cut-by-the-face-beyond.patch)

Test: geometry.port.2229
(geometry/tests/port/p2229-cap-leaving-the-end-face-across-a-tangent-edge.cpp),
test_cap_leaving_the_end_face_across_a_tangent_edge: the stepped block
rounded with 5 on eleven edges (by MitCad's rounding; OCCT's own fillet
does not build that body), its back's bottom edge chamfered with 4.5 and
4.9 by OCCT alone: one valid solid of the body's volume less its common
part with the bevel's wedge, the flat end faces of the exact shape's
areas; test_cap_wholly_across_a_tangent_edge, the chamfer of 5 (its cap
the chord of the band's edge, the end face gone), the same checks;
test_bevel_end_cut_by_the_half_balls, the chamfer of 5.5 and 7 (the end
cut by the half balls and the band, the patch's ChFi3d_ComputeCurves
part: each end's arc on a ball, a piece of a circle whose ends were found
in reverse order, trimmed the long way round the circle before, the
bevel's and the balls' wires crossing themselves), the same checks;
test_rounding_end_across_the_end_faces_lens, the rounding of 6 (its end
across the end face's lens between two crossings of one edge, the
patch's PerformIntersectionAtEnd crossing part: the face dropped before,
four edges in one face, volume 0) and of 7, the same checks;
test_cap_within_the_end_face, the chamfer of 4, as before;
test_rounding_as_wide_as_both_faces, the rounding and the chamfer of
exactly 10, the width of the bottom and the back (mitcad#2329: the
contacts touch each face at one point, the patch's SplitKPart and
ChFi3d_FilDS part), the same checks with the volume within 1e-6 and no
face under 0.01 mm2. Also geometry.suite.dressup_takes_face (the stepped
block's dressups of 10 through MitCad, and a ball swept over a square,
built from OCCT's primitives, its bottom and back ending in half circles,
rounded with 7 and 10). The
reported designs: cli.fillet_exact_reported (github-566, never
committed). In the OCCT lab (case 2329-ball-as-wide-as-both-faces: the
ball-swept square and a stand-in of the stepped block's end, a ball swept
over its end face's region) the rounding of 10 failed without the
patch's touch part ("StartSol echec": no domain on either face, no start
for the walk) and with it is one valid solid of the exact volume
(561.3195 of 561.3205 and 297.73099 of 297.73097 by vprops), the
shared cases and OCCT's blend and chamfer groups unchanged. Without the patch the probe of mitcad#2229 showed the invalid
shape 0.0036 mm3 short and end faces of area -0.69 (4.5) and -2.57 (4.9),
and at 5 a valid shape 0.058 mm3 short with end faces of area -3.06; with
the patch's first part 4.5 and 4.9 came out valid and exact (the end faces'
corner pieces 0.124 and 0.0043 mm2). Its failure without the patch is
still to be shown in the port's before/after run (mitcad#1986).
## 2306

[2306-chfi3d-corner-of-four-chamfers-meeting-in-a-plane-closed-by-a-planar-face.patch](../opencascade/2306-chfi3d-corner-of-four-chamfers-meeting-in-a-plane-closed-by-a-planar-face.patch)

Test: geometry/tests/test_fillet_bench_cases.cpp (ctest
geometry.fillet_bench_cases), test_chamfer_octahedron_all_edges: the
octahedron |x| + |y| + |z| <= 10 with all twelve edges chamfered sqrt(6),
four chamfers meeting at each vertex, by OCCT's chamfer alone and by
MitCad's Chamfer: one valid closed solid each, the volume 2752/3 and the
area 64 sqrt(3) + 192 sqrt(2) + 96 within 1e-6. On the port's tip without
the patch both paths failed, every corner's plate not done (the case's
known failures named mitcad#2306 until this patch). And
geometry/tests/port/p2306-corner-of-planar-chamfers-and-reversed-solid.cpp
(ctest geometry.port.2306): test_octahedron_chamfered_all_round_forward_solid
and test_octahedron_chamfered_all_round_oriented_solid, the octahedron
chamfered as a forward solid and as the reversed solid
BRepLib::OrientClosedSolid makes of its sewn shell, one valid solid of 26
planar faces, the volume and area exact within 1e-9;
test_reversed_box_blended_as_the_box, a box as a reversed solid around
its shell facing in, one edge chamfered 1 (995), a corner's three edges
chamfered and rounded 1, valid, with the faces, volumes and areas of the
box given forward. In the OCCT lab (DRAW's `chamf` and `blend`, the lab's
case 2306-octahedron-chamfer) the chain without the patch fails the
octahedron; with the corner part alone the reversed octahedron came out
invalid with the volume 490.67 and the reversed box's results invalid
with negative volumes; with both parts all are valid and exact. Its
failure without the patch is still to be shown in the port's before/after
run (mitcad#1986).

## 2272

[2272-brepblend-walk-ending-on-both-restrictions-takes-a-closed-guides-end-only-near-it.patch](../opencascade/2272-brepblend-walk-ending-on-both-restrictions-takes-a-closed-guides-end-only-near-it.patch)

Test: geometry/tests/test_fillet_nozzle_dome_junction.cpp (ctest
geometry.fillet_nozzle_dome_junction), test_foot_across_the_dome_edge: a
nozzle of radius 15 on a dome that runs on into a tube, its foot across the
circle where they meet, at four angles about the axis, rounded with 1.5 by
the kernel's fillet alone and by MitCad's, picked by either edge of the
foot's chain: one valid solid at the exact size without a note, the
material added the same at every angle; and cli.fillet_nozzle_dome_reported,
the reporter's design. Before the patch the kernel's fillet failed by the
tube edge at every angle and by the dome edge at one (the probe run of
mitcad#2272); with its first change alone, by the tube edge across the
seams (bundle-dev-132). The OCCT lab case 2272-nozzle-foot-on-dome-edge
fails without the patch (n0m, n0u by both edges, n270 by one half of its
chain) and builds every pick with it; the lab case 2272-nozzle (the
reporter's body before its copies' roundings, as .brep) fails without the
patch on the original nozzle by both edges and builds every edge of all
four feet with it. Its failure without the patch is still to be shown in
the port's before/after run (mitcad#1986).

## 2300

[2300-brepblend-no-twist-at-a-section-of-no-width.patch](../opencascade/2300-brepblend-no-twist-at-a-section-of-no-width.patch)

Test: geometry/tests/test_fillet_loft_rim.cpp (ctests
geometry.fillet_loft_rim.kernel_merge and kernel_keep): the rim between a
loft's side and a cylinder pulled from its end face, joined and split where
its rounding vanishes as MitCad's fillet gives it to the kernel, every
piece rounded with 2, 5 and 10 by the kernel's fillet alone, each build on
a copy of its own: done and valid (with patch 2301). In the OCCT lab (DRAW's
`blend` on the bodies `test_geometry_fillet_f_n fillet_loft_rim --probe` wrote) the chain
without the patch fails the top convex piece of the Merge loft with 5
("adjustment by reprocessing the non-written points"); with it the piece
builds. Its failure without the patch is still to be shown in the port's
before/after run (mitcad#1986).

## 2301

[2301-chfi3d-contact-end-on-an-arc-not-taken-at-a-crossing-farther-away.patch](../opencascade/2301-chfi3d-contact-end-on-an-arc-not-taken-at-a-crossing-farther-away.patch)

Test: geometry/tests/test_fillet_loft_rim.cpp (ctests
geometry.fillet_loft_rim.kernel_merge and kernel_keep, with patch 2300; and
the parts of MitCad's fillet, owner_merge, owner_keep, radii_merge,
pulls_merge, pulls_keep): the rim split where its rounding vanishes, every
piece rounded by the kernel's fillet alone, done and valid. In the OCCT lab
(the case 2300-loft-rim: the split bodies as .brep, each piece alone and
all at once with 2, 5 and 10, each on a freshly restored body) the chain
with 2300 but without this patch gives the Merge loft's top piece with 5
and its bottom piece with 10 invalid (BRepCheck_NotClosed, the second
contact's interference 0 to 0); with it every Merge build and every Keep
build with 5 and 10 is valid. The Keep loft's top and bottom pieces with 2
need patches 2303 and 2302 as well (mitcad#2390). Its failure
without the patch is still to be shown in the port's before/after run
(mitcad#1986).

## 2302

[2302-brepblend-walk-end-on-a-restriction-in-the-period-of-the-walk.patch](../opencascade/2302-brepblend-walk-end-on-a-restriction-in-the-period-of-the-walk.patch)

Test: geometry/tests/test_fillet_loft_rim.cpp (ctests
geometry.fillet_loft_rim.kernel_keep and radii_keep, with 2 mm): the Keep
loft's rim split where its rounding vanishes, its bottom convex piece
rounded with 2 by the kernel's fillet alone and by MitCad's fillet, done
and valid. In the OCCT lab (the case 2300-loft-rim, keep_r2_e7 and
keep_r2_all) the chain without the patch fails that piece ("PerformSurf :
Failed approximation!": the walk's end on the loft's periodic face a period
off its points before); with it the piece builds valid. Its failure without
the patch is still to be shown in the port's before/after run
(mitcad#1986).

## 2303

[2303-chfi3d-no-split-of-a-blend-at-a-minimum-of-noise-in-its-vanishing-end.patch](../opencascade/2303-chfi3d-no-split-of-a-blend-at-a-minimum-of-noise-in-its-vanishing-end.patch)

Test: geometry/tests/test_fillet_loft_rim.cpp (ctests
geometry.fillet_loft_rim.kernel_keep and radii_keep, with 2 mm): the Keep
loft's rim split where its rounding vanishes, all its pieces rounded at
once with 2 by the kernel's fillet alone and by MitCad's fillet, done and
valid. In the OCCT lab (the case 2300-loft-rim) the chain without the
patch crashes on the top convex piece with 2 (keep_r2_e11: a segmentation
fault in ChFi3d_CutConsumedFaces, after ChFi3d_FilDS read the spine past
its end at a sliver SplitSurf cut off), refuses all the pieces at once
with 2 (keep_r2_all, Failure_Overlap at the slivers' crossing), and gives
the bottom piece with 5 a sliver face (keep_r5_e7: 7 faces); with it all
of them build valid, keep_r5_e7 with 6 faces. Its failure without the
patch is still to be shown in the port's before/after run (mitcad#1986).

## 2365

[2365-chfi3d-kpart-cut-across-the-seam-of-a-holes-wall-under-the-blend.patch](../opencascade/2365-chfi3d-kpart-cut-across-the-seam-of-a-holes-wall-under-the-blend.patch)

Test: geometry/tests/port/p2365-counterbore-seam-under-rounding.cpp (ctest
geometry.port.2365): a block chamfered along its bottom with two
counterbores through the chamfer, each 1 mm from an end face, both walls'
seams towards +y, the chamfer's edges with the end faces rounded with 2 by
OCCT's fillet alone: test_each_end_edge_alone_as_its_mirror_image and
test_both_end_edges_as_mirror_images, valid solids whose two ends are each
other's mirror images (each end takes 28.0273 mm3, a grid of points round
the rim the same at both ends). In the OCCT lab (the case
2365-counterbore-seam-under-rounding) the chain without the patch takes
26.8059 mm3 at y = 20, with 17 faces against 14 at y = -20 (both edges:
22144.9018 mm3 and 18 faces); with it 22171.7075 mm3 and 14 faces at each
end alone, 22143.6803 and 15 together; the lab's other 45 cases unchanged.
The reported design: cli.fillet_counterbore_seam_reported (GitHub #635).
Its failure without the patch is still to be shown in the port's
before/after run (mitcad#1986).

## 2383

[2383-brepoffset-crease-split-in-several-edges-trimmed-once-and-oriented-in-each-face.patch](../opencascade/2383-brepoffset-crease-split-in-several-edges-trimmed-once-and-oriented-in-each-face.patch)

Test: geometry/tests/port/p2383-crease-split-in-several-edges.cpp (ctest
geometry.port.2383): a cone standing on a ring with notches cut up from the
ring's top through its wall, shelled 2 mm inside with Sharp corners by
OCCT's thick solid alone, the boss top and the foot open:
test_box_notch_floor_in_the_ring_top (a box notch, the body's axes up) and
test_round_notches_one_and_five (one and five notches of the reported
shape, the axes down as a revolve's): one valid solid each time, the wall
2 mm behind each notch, five notches adding five times the wall one adds.
MitCad's shell on the same bodies: geometry.shell_notched_cone; the
reported design: cli.shell_sharp_reported (GitHub #642). In the OCCT lab
(the case 2383-cone-notch-on-ring-top) the chain without the patch gives
both an invalid solid (the box notch with the axes up: 315757 mm3, the
offset shell in two; five notches: 344949 mm3), with it valid solids of
50882.6 and 53054.3 mm3 (that without notches, 49807.6, plus five times
what one notch adds, 649.3). Its failure without the patch is still to be
shown in the port's before/after run (mitcad#1986).

## 2384

[2384-intwalk-walk-end-put-on-a-boundary-within-a-room-scaled-to-the-parameter-range.patch](../opencascade/2384-intwalk-walk-end-put-on-a-boundary-within-a-room-scaled-to-the-parameter-range.patch)

Test: geometry/tests/port/p2384-walk-end-on-a-boundary-at-any-parameter-scale.cpp
(ctest geometry.port.2384),
test_arc_length_loft_section_reaches_the_tangent_points,
test_arc_length_loft_joined_to_its_mirror_image and
test_unit_parameter_loft_unchanged: MitCad's loft from a circle of radius
170 at z = 315 to one of radius 85 at z = 0 centred at x = 225 (its side
parametrized by arc length) and its mirror image about x = 0: the section
of the two sides ends on both points where they are tangent, (0, 170, 315)
and (0, -170, 315), within 1e-6 mm, and the kernel's union is one valid
solid of the union's volume (twice the loft's half at x >= 0) reaching
x = -310 and 310; the kernel's own loft of the same circles gives both as
before. Also geometry.mirror_loft_join (MitCad's join, alone and with the
design's end extrusions) and the reporter's design in cli.shell_reported
(GitHub mitcad/Mitcad#641). In the OCCT lab (the case
2384-loft-mirror-join, MitCad's loft and its mirror image as B-rep files)
the chain without the patch gives a section of 4 edges ending 0.004288 mm
from (0, -170, 315) and a fuse of 3 faces, the loft's volume and x from
-171.9; with it 5 edges ending on both points and a fuse of 5 faces,
27 047 300 mm3, x -310.9 .. 310.9, valid. The 44 other shared cases are
unchanged. OCCT's boolean group (4093 cases) against the chain without
the patch: none fixed or broken, 3304 the same, 786 skipped, 2 failing
with and without it (bsection/B6, volumemaker/E5), and periodicity/A2 OK
with a changed log: a box made periodic, planar faces only (which never
reach PutToBoundary, called by the parametric surfaces' intersection),
its twin faces listed in another order (a centroid at x 12.0455 for
12.9545, the mirror about 12.5).

## 2415

[2415-brepoffset-seam-only-where-the-face-has-it-twice-and-an-intersection-chain-kept-whole.patch](../opencascade/2415-brepoffset-seam-only-where-the-face-has-it-twice-and-an-intersection-chain-kept-whole.patch)

Test: geometry/tests/port/p2415-seam-only-where-the-face-has-it-twice.cpp
(ctest geometry.port.2415): a disk with an arm whose flat sides are tangent
to it, the disk's seam on one tangent line, shelled 2 and 1 mm inside with
Sharp corners by OCCT's thick solid alone: test_seam_on_tangent_line_plain
(the disk's top open) and test_seam_on_tangent_line_boss_and_rims (a boss
on the arm's end, its top open, the disk's rims rounded 3 mm, as GitHub
#642's second design has it): one valid solid each time, of the volume the
same body gives with its seam at +x, the arm's side on the seam with its
wall. MitCad's shell on the same body: geometry.shell_seam_on_tangent_edge;
the reported designs: cli.shell_sharp_reported (GitHub #642's second
design, Sharp 2 and 1 mm with the boss top open; GitHub #645's loft cut to
a slot, closed 2 mm, the patch's second part, which no synthetic body
reproduces yet). In the OCCT lab (the case
2415-seam-on-tangent-edge-and-slot-loft) the chain without the patch gives
for the plain body a valid solid of 147950 mm3 at 2 mm (36384.7 with the
seam at +x) and no result for the boss body (43304.5 with the seam at +x);
with it the volumes of the seam at +x; #645's split body's closed offset
1 and 2 mm inside, Sharp and Rounded, gives no shape without the patch and
valid solids of 79844.0 and 68937.2 mm3 with it. The lab's shared set (45
cases) shows no other change (the five-notch body of
2383-cone-notch-on-ring-top keeps 53054.3 mm3); OCCT's blend and chamfer
groups are the same (670 same, none fixed or broken), the offset and
thrusection groups break and fix none, their 19 "changed results" the same
values in another order or differences at round-off (the same cases change
between two runs of the chain without the patch). Its failure without the
patch is still to be shown in the port's before/after run (mitcad#1986).

## 2418

[2418-brepoffset-crease-turning-inside-its-edge-joined-across-the-tangency-and-a-vanishing-tip-cut.patch](../opencascade/2418-brepoffset-crease-turning-inside-its-edge-joined-across-the-tangency-and-a-vanishing-tip-cut.patch)

Test: geometry/tests/port/p2418-crease-turning-from-concave-to-convex.cpp
(ctest geometry.port.2418): GitHub #641's three-way pipe built from MitCad's
loft (its side parametrized by arc length) and the kernel's cylinders,
union, mirror and fillet, shelled 5 and 2 mm inside with its three ends open,
Sharp, by OCCT's thick solid alone (tolerance 1e-6, as MitCad's Shell):
test_pipe_crotch_sharp_5 and _2 (the crease between the lofts tangent at
both ends) and test_pipe_crotch_rounded_5 (the crotch rounded 20 by the
kernel's fillet, the rounding vanishing at the top circle): one valid solid
of the volume measured with patches 2418 and 2433 (within 1e-5: 2522285.2,
1019549.1 and 2512531.0 mm3; the lab's bodies, built in DRAW from the
stored loft and cylinders, give volumes 0.04 to 0.1 % apart, and the
reported design 2511954.5 and 2520204.5 in MitCad against the lab's
2512228.5 and 2521317.4: different bodies, not a change of the result, as
the lab's case with patch 2433 after 2418 gives its values unchanged),
points half the wall inside
the loft's side and the top cylinder in the solid, a millimetre past the
wall in the cavity. The reported design: cli.shell_reported
(shell_reported641.json, Sharp 5 mm with and without Fillet5). In the OCCT
lab (the case 2418-pipe-shell) the chain without the patch gives no thick
solid Sharp for any of the three bodies at 5 mm, nor for the reported and
the unrounded ones at 2 mm; with it valid solids of 2.51223e6, 1.01575e6
(reported), 2.52132e6, 1.0185e6 (unrounded) and 2.5131e6 mm3 (the kernel's
fillet), Rounded unchanged (2.5118e6). The lab's shared set (48 cases) shows
no other change; OCCT's blend and chamfer groups are the same (none fixed,
broken or changed); the offset group fixes none, lists compshape A3 as
broken (a known problem, BAD in its log: its TODO line) and 18 "changed
results" that are not the patch's: the case 2418-offset-group-samples runs
those 19 cases alone, with the same results with and without the patch.
## 2433

[2433-brepoffset-face-vanishing-beside-a-removed-face-closed-by-it.patch](../opencascade/2433-brepoffset-face-vanishing-beside-a-removed-face-closed-by-it.patch)

Test: geometry/tests/port/p2433-face-vanishing-beside-a-removed-face.cpp
(ctest geometry.port.2433): a box 40 x 30 x 20, its top edges rounded 2 mm,
shelled inside with the top open and Sharp corners by OCCT's thick solid
alone, 3 and 4 mm (test_rounded_top_open_wall_thicker_than_rounding): one
valid solid of the body's volume less the cavity (40 - 2w) x (30 - 2w) x
(20 - w), the corner the rounding fills in the wall. In the OCCT lab (the
case 2433-rounded-top-open) the chain without the patch gives no result for
the rounded box at 3 and 4 mm; with it 10010.9 and 12618.9 mm3, valid, and
every other line of the case the same (the sharp box at 1 to 4 mm; the
rounded box at 1 and 2 mm Sharp and at any wall Rounded, and GitHub #645's
split body with its top open at 1 to 3 mm: no result or invalid, before and
after). The lab's shared set (49 cases with this one) shows no other change;
OCCT's offset group breaks and fixes none (662 same, 130 failing in both),
its 21 "changed results" the same values in another order (16) or
differences at round-off (5; the masses of offset/shape_type_a/A1, 2e-4
apart, come out so in a run of another chain without this patch too). The port's before/after run (mitcad#1986, 26.11.2026) shows its test failing without the patch (with 2462 and 2463 reverted too): the thick solid is not done.

## 2463

[2463-brepoffset-arc-join-face-vanishing-beside-a-removed-face.patch](../opencascade/2463-brepoffset-arc-join-face-vanishing-beside-a-removed-face.patch)

Test: geometry/tests/port/p2463-arc-join-face-vanishing-beside-a-removed-face.cpp
(ctest geometry.port.2463): a box 40 x 30 x 20, its top edges rounded 2 mm,
shelled inside with the top open and Rounded corners (the arc join) by
OCCT's thick solid alone, 3 and 4 mm (test_rounded_top_open_arc_join): one
valid solid of the body's volume less the cavity (40 - 2w) x (30 - 2w) x
(20 - w), the corner the rounding fills in the wall. In the OCCT lab (the
case 2433-rounded-top-open) the chain without the patch gives no result for
the rounded box Rounded at 3 and 4 mm; with it 10010.9 and 12618.9 mm3,
valid, and every other line the same. The lab's shared set (49 cases)
shows no other change. OCCT's blend, chamfer and offset groups break
nothing (1334 same, 167 failing in both), their 18 "changed results" the
same values in another order; offset/compshape/A3 (an arc-join thick solid
with two faces removed, marked OCC23068) came out right in that run, but it
gives the right volume (3.0764e+06) or a wrong one from run to run on one
build (3 runs in 8 wrong with 2462 too), and neither patch's code runs on
it (no face vanishes there): noise, not a fix. The port's before/after run (mitcad#1986, 26.11.2026) shows its test failing without the patch (with 2462 reverted too): the thick solid is not done.

## 2462

[2462-brepoffset-wall-strip-at-a-removed-face-met-tangentially.patch](../opencascade/2462-brepoffset-wall-strip-at-a-removed-face-met-tangentially.patch)

Test: geometry/tests/port/p2462-wall-strip-at-a-removed-face-met-tangentially.cpp
(ctest geometry.port.2462): a box 40 x 30 x 20, its top edges rounded 2 mm,
shelled inside with the top open by OCCT's thick solid alone, 1 and 1.5 mm,
Sharp and Rounded (test_top_open_wall_no_thicker_than_rounding): one valid
solid of the body's volume less the cavity computed from the inner
roundings (3822.835 and 5502.710 mm3 in the OCCT lab, within 1e-6 of it),
a point in the wall under the top's edge in it, one past the strip and one
in the inner rounding's quarter not. In the OCCT lab (the case
2462-top-open-met-tangentially, a fresh copy of the body per run) the chain
without the patch gives no result Sharp and an invalid solid Rounded for
the box at 1 mm, and no result for GitHub #645's body with its top open at
1 and 2 mm; with it one valid solid each (3822.8; 9595.7 and 18284.9 mm3,
Rounded 9595.6 and 18284.9); the box at 2 mm and the body at 3 mm give no
result before and after. The case 2433-rounded-top-open differs in the
same lines (its loft's Rounded runs, after its failed 3 mm runs on the same
body, come out invalid there). The lab's shared set (50 cases) shows no
other change. OCCT's blend, chamfer and offset groups: 1334 same, 167
failing in both, 18 "changed results" the same values in another order,
and offset/compshape/A3 wrong in that run (see 2463: it flips from run to
run). The port's before/after run (mitcad#1986, 26.11.2026) shows its test failing without the patch (alone): the thick solid is not done.

offset/shape_type_a/A1 (a shell offset 0.01, complete intersection, arc
join; no face removed) showed another set of face areas and volume in the
group's end script in a bundle's check (0.598031 and 0.0709646 for 0.597674
and 0.0709518). Those are OCCT's coarse default measures; on one build with
the patch they come out as either set from run to run (4 runs: two of
each), with the faces in another order, while the precise measures are the
same in every run (both large faces 0.5978470 mm2, the volume 0.0709586,
the test's 0.0709583): run-to-run noise of the coarse integration, the
same shape, neither a correction nor a regression.

## 2457

[2457-brepoffset-spindle-torus-enlarged-up-to-its-apexes-and-no-shape-reported.patch](../opencascade/2457-brepoffset-spindle-torus-enlarged-up-to-its-apexes-and-no-shape-reported.patch)

Test: tools/cli/offset-face-shell-roundings-reported-test.cmake (the cli
test cli.offset_face_shell_roundings_reported, the reported design of
GitHub #678 by its content id under MITCAD_MITCAD_REPORTED, skipped
without it): Offset Face 2 mm on one straight inner rounding (15698.12
mm3), the inner torus alone (17158.01 mm3) and three faces without the
torus (21170.71 mm3) build, and so do all four faces and the torus with
Tangent Chain (23801.198 mm3, valid, with patch 2418 before this one; on
the chain without 2418 one edge of that result is out of tolerance, see the
patch's notes). No synthetic body reproduces
the spindle torus' enlargement yet: a body revolved from a quarter of a
spindle torus' section (the lab case 2457-spindle-torus-quarter) offsets
the same with and without the patch. In the OCCT lab (the case
2457-shell-roundings-offset, the reported body exported from the design)
the chain without the patch gives no result for the four faces, the torus
or the top plane moved (no offset of that body gives one: the outer top
plane moved 0.5 mm neither); with it the body closes, 23808.40 mm3, one
edge invalid (a tolerance of 2.9 mm). The lab's shared set (52 cases with
these two) shows no other change. OCCT's groups blend, chamfer and offset
(26.11.2026, on the branch's chain): none broken, blend and chamfer the
same. Where the patch acts was traced (a line printed wherever it stops a
spindle torus, makes a face at an apex or reports no shape): 14 offset
cases, every one the same with and without it (offset/faces_type_i D6, J9,
K2, K4, K8 and shape_type_i F7, F9 OK; faces_type_i C3, C7, D2, D3, D4 and
shape_type_i B8, F6 failing either way, the last two now FAILED, not done,
where they were BAD, an empty result). Its 17 to 20 "changed results"
(compshape A2, A5 to A7, faces_type_a A5, A8, A9, shape_type_a A1 to A4, A6
to A9, B1, B2, B4 to B6) are none of these: the same result volume and the
same face areas in another face order (offset/shape_type_a/A8: centres at
1e-16 and two mirror faces swapped), and two runs of the chain without the
patch differ from each other in the same way. compshape/A3 (OCCT's own
TODO ?OCC23068, a failure it expects at times) comes out 1.43293e+07 mm3
and invalid without the patch too (in one of two runs; with it in one of
three), the patch not acting in it. Its failure without the patch is still
to be shown in the port's before/after run (mitcad#1986).

## 2488

[2488-brepoffset-arc-join-tubes-and-corners-between-faces-of-different-offsets.patch](../opencascade/2488-brepoffset-arc-join-tubes-and-corners-between-faces-of-different-offsets.patch)

Test: geometry/tests/port/p2488-arc-join-between-faces-of-different-offsets.cpp
(ctest geometry.port.2488): a box 60 x 40 x 20 offset 2 mm outward with the
arc join and its bottom at 5 mm (SetOffsetOnFace) by OCCT's offset alone,
its top open (a thick solid) and whole (test_box_bottom_thicker_rounded):
one valid solid of 20000 + 1780 pi / 3 and of 72800 + 2396 pi / 3 mm3 (the
tubes along the bottom's edges from 5 mm down to 2 mm out, the bottom's
corners eighths of an ellipsoid); a wedge whose leaning side meets the
bottom at corners that are not square (test_wedge_corner_not_square): one
valid solid, the faces round those corners at the bottom's 5 mm, the left
side at its 2 mm. In the OCCT lab (the case 2488-own-offsets-rounded) the
chain without the patch gives every face the bottom's 5 mm (the box open
37759.6 and whole 101948.0 mm3, the cylinder with its side at 4 mm 23327.3
and whole 67766.2, the wedge's left side at 5 mm); with it 21864.0,
75309.1, 19957.5 and 61026.4 mm3 (the expected volumes), all valid, the
wedge's left side at 2 mm, and the lines without a rounding between
different offsets (the box inside, the box without own offsets) the same.
The lab's shared set (53 cases with this one) shows no other change; OCCT's
offset group breaks and fixes none (681 same, 131 failing in both), its one
"changed result" the mass of offset/shape_type_a/A1 at round-off, as noted
under 2433. Its failure without the patch is still to be shown in the
port's before/after run (mitcad#1986).

## 2494

[2494-brepoffset-rounding-of-chained-sides-vanishing-beside-a-removed-face.patch](../opencascade/2494-brepoffset-rounding-of-chained-sides-vanishing-beside-a-removed-face.patch)

Test: geometry/tests/port/p2494-rounding-of-chained-sides-vanishing-beside-a-removed-face.cpp
(ctest geometry.port.2494), OCCT's thick solid alone, the top open, 3 mm
inside, Sharp and Rounded: a box 40 x 30 x 20 with its top edges rounded
2 mm, one rounding's edge on the top split by a vertex (a rounding of five
edges; test_box_rounding_of_five_edges), and a slot 50 x 20, 15 high, its
top edges rounded 2 mm (walls meeting tangentially;
test_slot_walls_met_tangentially): one valid solid of the body's volume
less the body offset inside with sharp edges up to the top (10010.9 and
9327.8 mm3 in the OCCT lab). In the OCCT lab (case
2494-chained-rounding-and-seam-run, a fresh copy per run) the chain without
the patch gives no result for the box (Sharp and Rounded) and for the slot
Sharp (Rounded 9327.8 mm3 before and after), no result Rounded for GitHub
#645's loft without its rounding at 1, 2 and 3 mm (now 9576.7, 18526.8 and
26868.3 mm3, as Sharp before and after) and no result for the loft with its
rounding at 3 mm Sharp (now 26653.3 mm3); the loft with its rounding at
3 mm Rounded gives no result before and after. Of the lab's shared set (54
cases) 2433-rounded-top-open and 2462-top-open-met-tangentially differ in
the same lines (the loft at 3 mm Sharp now built; 2433's loft Rounded at 1
and 2 mm, after its failed 3 mm run on the same body, now valid), nothing
else. OCCT's blend, chamfer and offset groups against the chain without
the patch: 1352 same, 167 failing in both, none fixed, broken or changed;
offset/compshape/A3 right in that run (it flips from run to run, see 2463).
Rebased onto 2457 and 2488 (line offsets only): the same shared cases differ
in the same lines (54 run), and the groups give 1353 same, 167 failing in
both, nothing fixed, broken, changed or random.
The port's before/after run (mitcad#1986, 26.11.2026) shows both its tests
failing without the patch (alone): the thick solid is not done.

## 2535

[2535-brepoffset-arc-join-wall-across-vanishing-faces-to-a-chain-of-cap-edges.patch](../opencascade/2535-brepoffset-arc-join-wall-across-vanishing-faces-to-a-chain-of-cap-edges.patch)

Test: geometry/tests/port/p2535-arc-join-wall-chain-across-a-vanishing-rounding.cpp
(ctest geometry.port.2535), OCCT's thick solid alone, the top open, 3 mm
inside, Rounded: a box 40 x 30 x 20 with its top edges rounded 2 mm, the
edge where one long rounding meets its wall split by a vertex
(test_wall_of_two_edges_to_the_rounding), and the same with that edge and
the rounding's edge on the top split at different places
(test_chains_on_both_sides_of_the_rounding): one valid solid of the body's
volume less the box offset inside with sharp edges up to the top (10010.9
mm3 in the OCCT lab). In the OCCT lab (case
2535-wall-chain-across-vanishing-rounding, a fresh copy per run) the chain
without the patch gives for the first box Rounded a "valid" solid of absurd
volume, for the second no result, and for GitHub #645's loft with its
rounding at 3 mm Rounded no result; with the patch 10010.9, 10010.9 and
26653.3 mm3, as Sharp before and after. Of the lab's shared set (55 cases)
2433-rounded-top-open, 2462-top-open-met-tangentially and
2494-chained-rounding-and-seam-run differ only in that loft's Rounded run at
3 mm (now built), nothing else. OCCT's blend, chamfer and offset groups
against the chain without the patch: 1353 same, 167 failing in both,
nothing fixed, broken, changed or random. Not yet shown failing without the
patch by `tools/occt-port-proof.py 2535` (the lab case above shows the
reason for both bodies).

## 2542

[2542-brepoffset-crease-split-at-a-sphere-pole-cut-there.patch](../opencascade/2542-brepoffset-crease-split-at-a-sphere-pole-cut-there.patch)

Test: geometry/tests/port/p2542-crease-split-at-a-sphere-pole-cut-there.cpp
(ctest geometry.port.2542), OCCT's thick solid alone, the ends open, 2 mm
inside, Sharp: a box 120 x 96 x 60 with a groove of radius 5 on each long
side ending at z = 32 in a flat end (test_box_flat_ended_groove) and in a
quarter sphere whose pole lies on the side's edge
(test_box_ball_ended_groove), checked against the body less the cavity of
the points 2 mm or more inside every kept face (by hand: 73605.4 and
73565.0 mm3), and GitHub #720's body made of primitives (test_report_body)
against the closed offset 2 mm inside with the ends kept (74600.0 mm3). In
the OCCT lab (case 2542-channel-meets-groove) the chain without the patch
gives a valid but 3158.7 mm3 short wall for the flat end (the grooves' walls
missing) and invalid shapes for the ball end and the report's body (the
side's offset a disc); with it 73605.4, 73565.1 and 74599.8 mm3, valid; the
report's body at 3 mm 110813.0 (closed offset 110814.0). Of the lab's shared
set (56 cases) only 2542-channel-meets-groove differs. OCCT's blend,
chamfer and offset groups against the chain without the patch: 1353 same,
167 failing in both, nothing fixed, broken, changed or random. (An earlier
run of the patch showed offset/shape_type_a/A1 changed within its
tolerance and offset/compshape/A3 random: A1 gives 0.0709518 or 0.0709646
from run to run in one session with the patch's new paths not taken at
all.)
Reported design: tools/cli/shell-sharp-reported-test.cmake
(cli.shell_sharp_reported), GitHub #720's design at its marker, both round
ends removed, 2 and 3 mm inside, Sharp (shell_sharp_reported591.json).
