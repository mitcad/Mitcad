# vcpkg overlay ports

`vcpkg.json` (`vcpkg-configuration.overlay-ports`) makes vcpkg build these
ports instead of its own, on every platform and for every use of the
manifest (the CMake presets, `tools/dev-env/build-cycles.sh`). vcpkg builds
an overlay port from source once per machine and triplet and keeps the
result in its binary cache; a change to the port rebuilds it.

## opencascade

vcpkg's `opencascade` port for OCCT 8.0.1 as of the baseline in
`vcpkg.json`, with patches of MitCad's.

| Files | Origin | Licence |
|---|---|---|
| `portfile.cmake`, `vcpkg.json`, patches 0001 to 0005 | microsoft/vcpkg, `ports/opencascade` at the baseline; unchanged but for `port-version`, the list of patches and the SPDX line | MIT |
| patches 0006 and later | MitCad | LGPL-2.1-only WITH OCCT-exception-1.0, as OCCT |

The patches change OCCT, so they are under OCCT's licence (the GNU LGPL
version 2.1 with the Open CASCADE exception), published here with MitCad's
source, and OCCT stays a set of shared libraries (vcpkg's dynamic
triplets), as the [licence policy](../../docs/development.md#licence-policy)
requires; the third-party notices of the packages name them. Each patch
says at its top what it changes and why.

The port folder (`opencascade/`) holds only what vcpkg reads: vcpkg hashes
every file in it, so any change there, a comment or a new file included,
rebuilds OCCT on every machine. The description of the patches lives in
[`docs/`](docs/) beside it instead.

## The patches by topic

Each file describes its patches under a heading named after the patch file
(`### 0011-chfi3d-fillet-edge-overflow`), with an overview first and the
reports and tests last. The [index](#index) below lists every patch.

| File | Patches | Tickets | What they change |
|---|---|---|---|
| [repeated-work.md](docs/repeated-work.md) | 0006–0009, 0060, 0120, 0121, 1050 | mitcad#39, mitcad#60, mitcad#72, mitcad#77, mitcad#79, mitcad#119, mitcad#642 | no result: repeated work that made booleans, the checker and ShapeFix quadratic or worse |
| [memory-and-threads.md](docs/memory-and-threads.md) | 0030–0034, 0090, 0091 | mitcad#132, mitcad#160, mitcad#203 | failed allocations, the checker's parallel mode, the Windows signal handler, a heap overflow |
| [fillet-edge-overflow.md](docs/fillet-edge-overflow.md) | 0010–0019, 0050–0077 | mitcad#121 | the rolling ball fillet where a contact leaves its face |
| [fillet-crashes.md](docs/fillet-crashes.md) | 0020–0026 | mitcad#122, mitcad#133 | crashes, exceptions and wrong shapes of the fillet |
| [fillet-corners.md](docs/fillet-corners.md) | 0130, 0210, 0211, 0280–0283, 0290, 0291, 0340–0342, 0400, 0401, 0410, 0430–0432, 0450, 0460, 0490, 0540 | mitcad#236, mitcad#330, mitcad#339, mitcad#341, mitcad#342 | the corners where a rounding ends |
| [chamfers.md](docs/chamfers.md) | 0140–0149, 0380, 0381, 0480, 0481, 0530, 0531, 1090 | mitcad#240, mitcad#263, mitcad#1005 | chamfers wider than a face beside them; a corner's sharp edge limited by a common point on it |
| [fillet-results.md](docs/fillet-results.md) | 0150–0153, 0200, 0230, 0240, 0241, 0270, 0271, 0300 | mitcad#239 | roundings whose results were invalid or failed |
| [fillet-walks-and-contours.md](docs/fillet-walks-and-contours.md) | 0160, 0180, 0190, 0250, 0260, 0840, 0860, 0861, 0920 | mitcad#238, mitcad#241, mitcad#261, mitcad#307, mitcad#308, mitcad#434, mitcad#441, mitcad#464 | walks over a rounded rim; which edges a contour takes, with what radius |
| [fillet-rolling-ball.md](docs/fillet-rolling-ball.md) | 0170–0172, 0390, 0391, 0500–0503 | mitcad#237, mitcad#378 | the blend rolling on an edge as a true rolling ball |
| [fillet-census.md](docs/fillet-census.md) | 0320–0331, 0350–0352, 0420, 0440 | mitcad#300, mitcad#322, mitcad#323, mitcad#331, mitcad#334, mitcad#337 | fillets the `.f3d` census found failing; consumed faces |
| [booleans-and-face-merge.md](docs/booleans-and-face-merge.md) | 0110, 0220, 0360, 0590, 0680, 1040 | mitcad#235, mitcad#284, mitcad#400, mitcad#642 | the face merge after booleans and the wire splitter |
| [multi-turn-faces.md](docs/multi-turn-faces.md) | 0510–0512, 0560, 0591–0593 | mitcad#235, mitcad#349, mitcad#380 | booleans on faces and edges that run many turns |
| [offset-projection-and-pipes.md](docs/offset-projection-and-pipes.md) | 0100, 0960, 1030 | mitcad#172, mitcad#504, mitcad#539 | the offset, the projection's boundary search, the pipe's guide trihedron |
| [fillet-replay-0550-0692.md](docs/fillet-replay-0550-0692.md) | 0550, 0580, 0581, 0600, 0620, 0650, 0651, 0670, 0671, 0690–0692 | mitcad#235, mitcad#343 | fillets and chamfers of stored designs: pockets, ribs, teeth, threads, recesses |
| [fillet-replay-0700-0772.md](docs/fillet-replay-0700-0772.md) | 0700, 0720, 0721, 0740–0742, 0750, 0770–0772, 1060 | mitcad#343, mitcad#415, mitcad#840 | notched blends, a thread's crest, a block on a post, a housing's corner, a slot beside a ledge |
| [fillet-replay-0790-0822.md](docs/fillet-replay-0790-0822.md) | 0790, 0800–0803, 0820–0822 | mitcad#343, mitcad#422 | a thread's run-out, traits of stored bodies, a key's lenses |
| [fillet-replay-0870-0911.md](docs/fillet-replay-0870-0911.md) | 0870, 0880, 0890, 0891, 0900–0911 | mitcad#343 | narrow bands, strips, slivers, a bar's rim and notch walls, OnSame ends |
| [fillet-replay-0930-0942.md](docs/fillet-replay-0930-0942.md) | 0930–0933, 0940–0942 | mitcad#343, mitcad#495 | a ring's rims, roundings ending on a rod, a key, a chamfer that hung |
| [fillet-replay-0950-0980.md](docs/fillet-replay-0950-0980.md) | 0950, 0951, 0961, 0962, 0970, 0980 | mitcad#343 | sills, a micrometre step, tangent vertices, a tight corner, slight creases |
| [fillet-replay-0990-1020.md](docs/fillet-replay-0990-1020.md) | 0990, 1000, 1001, 1010, 1011, 1020 | mitcad#343 | a rolled end, chamfers over creases, a bore's rim, a drafted peg |
| [meshing.md](docs/meshing.md) | 1070, 1080, 1081 | mitcad#846, mitcad#884 | Delabella's plane triangulation on nearly cocircular boundaries; the time of planes with many holes |

## Adding a patch

A patch to OCCT is the last resort: only when nothing else works
([code conventions](../../docs/development.md#code-conventions)).

1. Write the change in OCCT's source tree and take it as a patch file in
   `opencascade/`, named `NNNN-<toolkit or package>-<what it does>.patch`.
   Patches of one family share a block of numbers (0130 to 0139 for
   mitcad#236's corners, and so on); a new family takes the next free
   block.
2. Start it, as every patch of MitCad's does, with a header that names the
   origin and the licence (`Mitcad's patch to OCCT 8.0.1, under OCCT's
   licence:` and the line `SPDX-License-Identifier: LGPL-2.1-only WITH
   OCCT-exception-1.0`), then what it changes and why, with the ticket.
3. Append it to the `PATCHES` list of `opencascade/portfile.cmake`; vcpkg
   applies the patches in that order.
4. Give it a test with OCCT alone in `geometry/tests/` that fails without
   it, where a synthetic body can reproduce the case, and a report with the
   reproduction and the measurements in its ticket.
5. Describe it here: a section headed by the patch's name in the file of
   its topic under [`docs/`](docs/) (or a new file for a new topic, listed
   in the table above), its test in that file's *Reports and tests*, and a
   row in the [index](#index).

A change to the port folder rebuilds OCCT on every machine; a change to
`docs/` or to this README does not.

## Reports, upstream and newer OCCT

Each patch has a report for OCCT, with a reproduction that uses OCCT alone
and measurements; each topic file lists them with the tests under
*Reports and tests*. Sending them upstream is the maintainers' step. When
the baseline moves to a newer OCCT, check which of the changes OCCT has
made; update the port from vcpkg's and apply the remaining patches again
(`git apply` in OCCT's source tree, in order), and compare the corpus
results as for any change to the geometry. Building OCCT from the port is
described in [development.md](../../docs/development.md#faster-builds).

## Index

Every patch by number: where it is described, its ticket and one line of
what it does. The patches 0006 to 0009 come from mitcad#39, mitcad#72,
mitcad#77 and mitcad#79; their column names each one's report.

| Patch | Ticket | What it does |
|---|---|---|
| [0001](opencascade/0001-cmake-keep-build-use-vcpkg-explicit.patch) | vcpkg | CMake: `BUILD_USE_VCPKG` only when set explicitly |
| [0002](opencascade/0002-cmake-load-exported-package-dependencies.patch) | vcpkg | CMake: third-party libraries found with `find_package`, and found again by the exported package |
| [0003](opencascade/0003-image-remove-freeimage-msvc-autolink.patch) | vcpkg | FreeImage's MSVC auto-link removed |
| [0004](opencascade/0004-cmake-add-additional-path-extraction-for-OpenCASCADE.patch) | vcpkg | CMake: one more path level in `OpenCASCADEConfig.cmake` |
| [0005](opencascade/0005-drop-bin-letter.patch) | vcpkg | CMake: `BIN_LETTER` (`d`, `i`) of Debug and RelWithDebInfo builds dropped |
| [0006](docs/repeated-work.md#0006-shapeupgrade-unifysamedomain-seam-search-by-vertex) | mitcad#84 | face merge: removed edges looked up by vertex at a seam |
| [0007](docs/repeated-work.md#0007-brepcheck-analyzer-no-search-in-face) | mitcad#90 | checker: no search of a face for its own vertices and edges |
| [0008](docs/repeated-work.md#0008-brepcheck-wire-seam-edges-once-per-face) | mitcad#91 | checker: a face's seam edges found once, not once per wire |
| [0009](docs/repeated-work.md#0009-bopalgo-face-face-adaptors-once-per-face) | mitcad#92 | booleans: one surface adaptor per face, not per pair of faces |
| [0010](docs/fillet-edge-overflow.md#0010-brepblend-surfrst-line-on-both-restrictions) | mitcad#121 | a blend rolling on an edge ends on both restrictions |
| [0011](docs/fillet-edge-overflow.md#0011-chfi3d-fillet-edge-overflow) | mitcad#121 | edge overflow: the ball touches the face beyond or rolls on the far edge |
| [0012](docs/fillet-edge-overflow.md#0012-chfi3d-closed-stripe-end-on-vertex) | mitcad#121 | a closed stripe whose start lies on a vertex |
| [0013](docs/fillet-edge-overflow.md#0013-chfi3d-overflow-across-faces-and-overlapping-roundings) | mitcad#121 | overflow across several narrow faces; overlapping roundings |
| [0014](docs/fillet-edge-overflow.md#0014-chfi3d-contact-along-face-edge) | mitcad#121 | a contact along a face's far edge |
| [0015](docs/fillet-edge-overflow.md#0015-brepblend-surfrst-smaller-step-on-failed-reframing) | mitcad#121 | a smaller step where reframing on an arc fails |
| [0016](docs/fillet-edge-overflow.md#0016-chfi3d-rounding-rolls-on-anothers-contact-line) | mitcad#121 | a rounding rolls on another's contact line |
| [0017](docs/fillet-edge-overflow.md#0017-chfi3d-contact-along-face-edge-ends-at-vertices) | mitcad#121 | 0014's ends at the vertices where the other contact ends first |
| [0018](docs/fillet-edge-overflow.md#0018-chfi3d-corner-where-the-end-face-is-split-in-one-plane) | mitcad#121 | a corner where the end face is two faces of one plane |
| [0019](docs/fillet-edge-overflow.md#0019-chfi3d-undone-moresurfdata-corner-fails) | mitcad#121 | an undone `PerformMoreSurfdata` corner fails instead of corrupting the heap |
| [0020](docs/fillet-crashes.md#0020-chfi3d-crash-fixes) | mitcad#122 | three causes of segmentation faults |
| [0021](docs/fillet-crashes.md#0021-chfi3d-corner-of-contour-closed-at-sharp-vertex) | mitcad#122 | the corner of a contour closed at a sharp vertex |
| [0022](docs/fillet-crashes.md#0022-chfi3d-round-edges-asked-for-near-tangent) | mitcad#122 | edges between faces up to 0.1 rad apart can be rounded |
| [0023](docs/fillet-crashes.md#0023-chfi3d-meeting-point-of-periodic-contact-lines) | mitcad#122 | the meeting point of an arc's and a line's contact lines |
| [0024](docs/fillet-crashes.md#0024-chfi3d-order-of-periodic-kpart-surfaces) | mitcad#133 | the order of a circle's analytic pieces |
| [0025](docs/fillet-crashes.md#0025-chfi3d-end-cut-by-faces-around-a-cylinder-corner) | mitcad#133 | an end cut by several faces round a cylinder's corner patch |
| [0026](docs/fillet-crashes.md#0026-chfi3d-onsame-end-cut-by-several-faces) | mitcad#133 | an OnSame end cut by several faces |
| [0030](docs/memory-and-threads.md#0030-standard-failed-allocation-throws) | mitcad#132 | a failed allocation throws `Standard_OutOfMemory` and is counted |
| [0031](docs/memory-and-threads.md#0031-osd-threadpool-failed-allocations-to-launching-thread) | mitcad#132 | the thread pool hands failed allocations to the launching thread |
| [0032](docs/memory-and-threads.md#0032-ncollection-containers-intact-when-allocation-fails) | mitcad#132 | the collections stay valid when an allocation throws |
| [0033](docs/memory-and-threads.md#0033-brepcheck-edge-curve-adaptor-per-context) | mitcad#203 | the checker's parallel mode: one curve adaptor per context |
| [0034](docs/memory-and-threads.md#0034-osd-signal-handler-recursive-mutex) | — | the Windows signal handler's mutex is recursive |
| [0050](docs/fillet-edge-overflow.md#0050-chfi3d-kpart-end-runs-into-end-face) | mitcad#121 | an analytic blend whose contact runs into the end face |
| [0051](docs/fillet-edge-overflow.md#0051-chfi3d-rolling-on-across-surfdatas) | mitcad#121 | a blend rolling on across its pieces keeps the edge whole |
| [0052](docs/fillet-edge-overflow.md#0052-chfi3d-rolling-start-beside-tangent-face) | mitcad#121 | a rolling start beside a tangent face |
| [0053](docs/fillet-edge-overflow.md#0053-chfi3d-corner-after-overflow-onto-face-beyond) | mitcad#121 | the corner after an overflow onto the face beyond |
| [0054](docs/fillet-edge-overflow.md#0054-chfi3d-contact-along-a-part-of-a-face-edge) | mitcad#121 | a contact along a part of a face's edge |
| [0055](docs/fillet-edge-overflow.md#0055-chfi3d-kpart-blend-cut-by-face-beyond) | mitcad#121 | an analytic blend over a hole, cut by the face beyond |
| [0056](docs/fillet-edge-overflow.md#0056-chfi3d-corner-cutting-the-last-two-walked-pieces) | mitcad#121 | a corner cutting the last two walked pieces |
| [0057](docs/fillet-edge-overflow.md#0057-chfi3d-kpart-gaps-in-order-across-closed-lines) | mitcad#121 | more gaps of 0055 filled, in order across closed lines |
| [0058](docs/fillet-edge-overflow.md#0058-chfi3d-rounding-rolls-on-a-convex-far-edge) | mitcad#121 | a rounding rolls on a sharp convex far edge |
| [0059](docs/fillet-edge-overflow.md#0059-chfi3d-kpart-blend-cut-by-the-face-at-the-end) | mitcad#121 | an analytic blend cut by the face at the end all along |
| [0060](docs/repeated-work.md#0060-shapefix-intersecting-edges-once-per-face) | mitcad#60 | ShapeFix: intersecting edges repaired once per face, not per wire |
| [0070](docs/fillet-edge-overflow.md#0070-chfi3d-kpart-cut-to-end-where-the-narrow-face-vanishes) | mitcad#121 | 0059 where the narrow face vanishes |
| [0071](docs/fillet-edge-overflow.md#0071-chfi3d-kpart-cut-by-several-faces-beyond) | mitcad#121 | an analytic blend cut by several faces beyond |
| [0072](docs/fillet-edge-overflow.md#0072-chfi3d-kpart-cuts-on-closed-spines) | mitcad#121 | the cuts of 0055 and 0071 on closed spines and conic cuts |
| [0073](docs/fillet-edge-overflow.md#0073-chfi3d-kpart-pieces-over-a-hole-are-one-face) | mitcad#121 | the pieces over a hole are one face |
| [0074](docs/fillet-edge-overflow.md#0074-chfi3d-walked-blend-cut-by-the-face-at-the-end) | mitcad#121 | 0059 for a walked blend |
| [0075](docs/fillet-edge-overflow.md#0075-blendfunc-constrad-rolling-ball-section) | mitcad#121 | the walked constant-radius blend is a true rolling ball |
| [0076](docs/fillet-edge-overflow.md#0076-chfi3d-extension-rolls-on-the-faces-own-surface) | mitcad#121 | a spine's extension rolls on the face's own surface |
| [0077](docs/fillet-edge-overflow.md#0077-chfi3d-walk-past-the-spine-ends-before-a-singular-section) | mitcad#121 | the walk past the spine's ends stops before a singular section |
| [0090](docs/memory-and-threads.md#0090-chfi3d-double-interference-at-vertex-kind) | mitcad#160 | a second interference at a vertex takes the vertex's kind (heap overflow) |
| [0091](docs/memory-and-threads.md#0091-topopebrepbuild-point-uses-within-array) | mitcad#160 | `BuildEdges` counts point uses only within its array |
| [0100](docs/offset-projection-and-pipes.md#0100-brepoffset-faces-vanishing-within-the-offset) | mitcad#172 | offset: faces vanishing within the offset distance are dropped |
| [0110](docs/booleans-and-face-merge.md#0110-shapeupgrade-unifysamedomain-chain-not-concatenated) | — | face merge: a B-spline chain that cannot be concatenated stays as its edges |
| [0120](docs/repeated-work.md#0120-brepcheck-face-wire-boxes-of-conics-past-their-range) | mitcad#119 | checker: wire boxes of conics whose range passes their parameters |
| [0121](docs/repeated-work.md#0121-brepcheck-shell-orientation-edges-listed-once-per-face) | mitcad#119 | checker: a shell's edge orientations listed once per face |
| [0130](docs/fillet-corners.md#0130-chfi3d-rounding-vanishing-at-a-tangent-end) | mitcad#236 | a rounding vanishing at a tangent end ends in a point |
| [0140](docs/chamfers.md#0140-chfi3d-contact-along-a-closed-face-edge) | mitcad#240 | a fillet or chamfer as wide as its face along a closed edge |
| [0141](docs/chamfers.md#0141-chfi3d-onsame-end-second-contact-beyond-the-edge) | mitcad#240 | a chamfer's OnSame end whose second contact runs beyond the edge |
| [0142](docs/chamfers.md#0142-chfi3d-kpart-cut-by-faces-beyond-that-grow) | mitcad#240 | analytic blends that add material, cut by faces beyond that grow |
| [0143](docs/chamfers.md#0143-chfi3d-chamfer-corner-beyond-the-common-face) | mitcad#240 | the corner of two chamfers beyond their common face |
| [0144](docs/chamfers.md#0144-chfi3d-closed-contact-split-at-the-edges-vertex) | mitcad#263 | a closed contact split at the edge's vertex |
| [0145](docs/chamfers.md#0145-chfi3d-kpart-pieces-in-their-faces-periods) | mitcad#263 | pieces over a gap held in their faces' periods |
| [0146](docs/chamfers.md#0146-chfi3d-kpart-pieces-one-face-round-a-closed-spine) | mitcad#263 | 0073's one face on a spine closed by one edge |
| [0147](docs/chamfers.md#0147-chfi3d-onsame-end-prolonged-along-a-seam) | mitcad#263 | an OnSame end prolonged along the opposite face's seam |
| [0148](docs/chamfers.md#0148-chfi3d-chamfer-as-wide-as-a-ledge-along-a-line-and-an-arc) | mitcad#263 | a chamfer as wide as a ledge along a line and an arc |
| [0149](docs/chamfers.md#0149-chfi3d-end-snapped-onto-a-closed-edges-vertex-on-a-periodic-face) | mitcad#263 | an end snapped onto a closed edge's vertex on a periodic face |
| [0150](docs/fillet-results.md#0150-chfi3d-onsame-corner-beside-a-split-face) | mitcad#239 | an OnSame corner beside a split face |
| [0151](docs/fillet-results.md#0151-chfi3d-corner-on-an-end-face-split-in-one-plane) | mitcad#239 | an OnDiff corner on an end face split in one plane |
| [0152](docs/fillet-results.md#0152-chfi3d-closed-rolling-start-on-a-seam-vertex) | mitcad#239 | a closed rolling start on a seam vertex |
| [0153](docs/fillet-results.md#0153-chfi3d-roundings-meeting-on-one-contact-line) | mitcad#239 | two roundings meeting on one contact line |
| [0160](docs/fillet-walks-and-contours.md#0160-chfi3d-walked-blend-over-a-rounded-rim) | mitcad#238 | walks over a rounded rim the ball cannot touch |
| [0170](docs/fillet-rolling-ball.md#0170-brepblend-surfrst-rolling-ball-and-pivot) | mitcad#237 | the surface/edge blend is a true rolling ball |
| [0171](docs/fillet-rolling-ball.md#0171-chfi3d-rolling-ball-turns-about-a-vertex) | mitcad#237 | the rolling ball turns about a vertex |
| [0172](docs/fillet-rolling-ball.md#0172-chfi3d-rolled-blend-cut-by-the-face-at-the-end) | mitcad#237 | 0074 for a blend rolling on an edge |
| [0180](docs/fillet-walks-and-contours.md#0180-chfi3d-chains-stop-where-the-edges-turn) | mitcad#241 | a contour stops where its edges turn |
| [0190](docs/fillet-walks-and-contours.md#0190-chfi3d-consumed-face-round-a-closed-stripe) | mitcad#261 | 0160's walk round a closed stripe that consumes a face |
| [0200](docs/fillet-results.md#0200-chfi3d-walked-blend-about-a-standing-contact) | mitcad#239 | a walked blend turns about a standing contact |
| [0210](docs/fillet-corners.md#0210-chfi3d-mitred-ends-at-a-short-sharp-edge) | mitcad#236 | mitred ends at a short sharp edge |
| [0211](docs/fillet-corners.md#0211-chfi3d-end-cut-by-a-periodic-face-across-its-seam) | mitcad#236 | an end cut by a periodic face across its seam |
| [0220](docs/booleans-and-face-merge.md#0220-shapeupgrade-unifysamedomain-seams-half-a-period-apart) | mitcad#284 | face merge: cylinder faces whose seams lie half a turn apart |
| [0230](docs/fillet-results.md#0230-chfi3d-contact-in-the-adjacent-face-period) | — | a blend's open contact kept in the adjacent face's period |
| [0240](docs/fillet-results.md#0240-chfi3d-extension-on-the-faces-own-surface-away-from-its-seam) | mitcad#239 | an extension on the face's own surface away from its seam |
| [0241](docs/fillet-results.md#0241-chfi3d-roundings-meeting-across-a-notch) | mitcad#239 | roundings meeting across a notch |
| [0250](docs/fillet-walks-and-contours.md#0250-chfi3d-walk-on-along-a-face-of-the-spine) | mitcad#308 | a walk on along a face of the spine that 0160 took for a rim |
| [0260](docs/fillet-walks-and-contours.md#0260-chfi3d-fillet-carried-across-a-notch-in-its-rim) | mitcad#307 | a fillet carried across a notch in its rim |
| [0270](docs/fillet-results.md#0270-chfi3d-roundings-meeting-on-one-contact-line-of-a-face-going-on) | mitcad#239 | roundings meeting on one contact line of a face going on |
| [0271](docs/fillet-results.md#0271-chfi3d-roundings-cut-by-each-other-where-their-contacts-cross) | mitcad#239 | roundings cut by each other where their contacts cross |
| [0280](docs/fillet-corners.md#0280-chfi3d-onsame-end-where-the-face-at-the-end-turns-tangentially) | mitcad#236 | an OnSame end where the face at the end turns tangentially |
| [0281](docs/fillet-corners.md#0281-chfi3d-walk-round-the-vertex-past-a-short-edge) | mitcad#236 | the walk round the vertex past a short edge |
| [0282](docs/fillet-corners.md#0282-chfi3d-end-cut-past-the-bounds-of-the-faces-at-the-end) | mitcad#236 | an end cut past the bounds of the faces at the end |
| [0283](docs/fillet-corners.md#0283-chfi3d-standing-end-piece-one-face-with-the-one-before) | mitcad#236 | a standing end piece one face with the one before |
| [0290](docs/fillet-corners.md#0290-chfi3d-walk-round-the-vertex-through-a-face-met-at-a-point) | mitcad#236 | the walk round the vertex through a face met at a point |
| [0291](docs/fillet-corners.md#0291-chfi3d-end-past-the-spine-where-a-contact-ran-off-its-face) | mitcad#236 | an end past the spine where a contact ran off its face |
| [0300](docs/fillet-results.md#0300-chfi3d-prolonged-end-edge-across-the-opposite-faces-seam) | mitcad#239 | a prolonged end edge across the opposite face's seam |
| [0320](docs/fillet-census.md#0320-chfi3d-where-a-fillet-stops) | mitcad#322 | where a fillet that is not done stopped (API) |
| [0321](docs/fillet-census.md#0321-chfi3d-closed-rounding-consuming-its-face-ends-at-the-vertex) | mitcad#322 | a closed rounding consuming its face all round |
| [0322](docs/fillet-census.md#0322-chfi3d-open-stripe-end-at-the-vertex-of-the-edge-its-contact-runs-along) | mitcad#331 | an open stripe's end at the vertex of the edge its contact runs along |
| [0330](docs/fillet-census.md#0330-chfi3d-which-corner-a-fillet-fails-at) | mitcad#323 | which corner a fillet failed at and why (API) |
| [0331](docs/fillet-census.md#0331-chfi3d-rounding-rolled-on-a-straight-edge-exactly) | mitcad#323 | a rounding rolled on a straight edge as its exact cylinder |
| [0340](docs/fillet-corners.md#0340-chfi3d-onsame-end-cut-by-the-face-of-the-edge-the-ball-rolls-on) | mitcad#236 | an OnSame end cut by the face of the edge the ball rolls on |
| [0341](docs/fillet-corners.md#0341-chfi3d-rolling-blend-walked-again-across-a-seam-past-its-ends) | mitcad#236 | a rolling blend walked again across a seam past its ends |
| [0342](docs/fillet-corners.md#0342-chfi3d-rounding-rolled-on-past-its-edge-cut-by-the-faces-at-the-ends) | mitcad#236 | a rounding rolled on past its edge, cut by the faces at the ends |
| [0350](docs/fillet-census.md#0350-chfi3d-regularity-only-where-the-faces-are-tangent) | mitcad#300 | regularity only where the faces are tangent |
| [0351](docs/fillet-census.md#0351-chfi3d-closed-chain-consuming-a-face-all-round) | mitcad#300 | a closed chain consuming a face all round |
| [0352](docs/fillet-census.md#0352-chfi3d-contact-on-a-face-beyond-at-an-acute-angle) | mitcad#300, mitcad#327 | a contact on a face beyond at an acute angle |
| [0360](docs/booleans-and-face-merge.md#0360-bopalgo-wiresplitter-tangent-edges-measured-at-one-distance) | mitcad#235 | wire splitter: tangent edges ordered at one distance |
| [0380](docs/chamfers.md#0380-chfi3d-chamfer-end-where-the-face-at-the-end-is-tangent-to-both-faces) | mitcad#263 | a chamfer's end where the face at the end is tangent to both faces |
| [0381](docs/chamfers.md#0381-chfi3d-chamfer-guide-deformed-by-a-hundredth-of-its-distance-at-most) | mitcad#263 | a chamfer's guide deformed by a hundredth of its distance at most |
| [0390](docs/fillet-rolling-ball.md#0390-chfi3d-rolling-ball-touches-the-next-edge-before-the-vertex) | mitcad#237 | the ball touches the next edge before the vertex |
| [0391](docs/fillet-rolling-ball.md#0391-brepblend-rstrst-rolling-ball-on-two-edges) | mitcad#237 | the ball rests on two edges |
| [0400](docs/fillet-corners.md#0400-chfi3d-rounding-vanishing-at-its-end-stops-at-the-vertex) | mitcad#330 | a rounding vanishing at its end stops at the vertex |
| [0401](docs/fillet-corners.md#0401-chfi3d-end-cut-across-the-seam-line-of-a-periodic-face) | mitcad#330 | an end cut across the seam line of a periodic face |
| [0410](docs/fillet-corners.md#0410-chfi3d-rounding-ending-in-the-face-it-runs-into) | mitcad#236 | a rounding ending in the face it runs into tangentially |
| [0420](docs/fillet-census.md#0420-chfi3d-kpart-round-a-closed-edge-on-a-face-with-a-stepped-seam) | mitcad#337 | a closed edge on a cylinder whose seam runs in steps |
| [0430](docs/fillet-corners.md#0430-chfi3d-rounding-rolls-before-another-roundings-face) | mitcad#339 | a rounding rolls before another rounding's face |
| [0431](docs/fillet-corners.md#0431-chfi3d-rolled-onsame-end-at-the-end-of-its-contact-curve) | mitcad#339 | a rolled OnSame end at the end of its contact curve |
| [0432](docs/fillet-corners.md#0432-chfi3d-roundings-crease-over-a-narrow-common-face) | mitcad#339 | the crease of two roundings over a narrow common face |
| [0440](docs/fillet-census.md#0440-chfi3d-contact-along-a-chain-of-edges-of-a-consumed-strip) | mitcad#334 | a contact along a chain of edges of a consumed strip |
| [0450](docs/fillet-corners.md#0450-chfi3d-rounding-face-turning-tangentially-into-the-end-face) | mitcad#341 | a rounding's face turning tangentially into the end face |
| [0460](docs/fillet-corners.md#0460-chfi3d-onsame-end-on-an-edge-of-a-consumed-corner-face) | mitcad#342 | an OnSame end on an edge of a consumed corner face |
| [0480](docs/chamfers.md#0480-geomint-open-line-parts-beyond-its-vertices) | mitcad#263 | `GeomInt`: an open line's parts beyond its vertices kept |
| [0481](docs/chamfers.md#0481-chfi3d-kpart-gap-across-a-closed-spines-start-split-there) | mitcad#263 | a gap across a closed spine's start split there |
| [0490](docs/fillet-corners.md#0490-chfi3d-onsame-end-on-a-trimmed-face-past-its-bounds) | mitcad#236 | an OnSame end on a trimmed face past its bounds |
| [0500](docs/fillet-rolling-ball.md#0500-chfikpart-fillet-between-coaxial-surfaces-of-revolution) | mitcad#237 | the fillet between coaxial surfaces of revolution is a torus |
| [0501](docs/fillet-rolling-ball.md#0501-chfi3d-kpart-cut-round-a-cut-out-under-an-overhanging-rim) | mitcad#237 | an analytic blend cut round a cut-out under an overhanging rim |
| [0502](docs/fillet-rolling-ball.md#0502-chfi3d-kpart-cut-by-the-end-face-over-a-gap) | mitcad#237 | an analytic blend cut by the end face over a gap |
| [0503](docs/fillet-rolling-ball.md#0503-chfi3d-two-roundings-meeting-on-an-edge-within-its-tolerance) | mitcad#378 | two roundings meeting on an edge within its tolerance |
| [0510](docs/multi-turn-faces.md#0510-boptools-pcurve-into-the-period-inside-a-face-spanning-many) | mitcad#349 | booleans: a new p-curve put into the period inside a face of many turns |
| [0511](docs/multi-turn-faces.md#0511-inttools-fclass2d-every-copy-on-a-face-spanning-many-periods) | mitcad#349 | booleans: a point classified at every copy on such a face |
| [0512](docs/multi-turn-faces.md#0512-intcurvesurface-quadric-roots-sampled-by-knot-spans) | mitcad#349 | a B-spline's roots on a quadric sampled by knot spans |
| [0530](docs/chamfers.md#0530-chfi3d-chamfer-guide-keeps-to-its-edges-at-the-blends-beside) | mitcad#263 | a chamfer's guide keeps to its edges beside analytic blends |
| [0531](docs/chamfers.md#0531-chfi3d-chamfer-wider-than-a-narrow-face-cut-by-the-faces-beyond) | mitcad#263 | a chamfer wider than a narrow face, cut by the faces beyond |
| [0540](docs/fillet-corners.md#0540-chfi3d-rounding-running-on-into-an-earlier-roundings-tube) | mitcad#236 | a rounding running on into an earlier rounding's tube |
| [0550](docs/fillet-replay-0550-0692.md#0550-chfi3d-end-over-pieces-where-the-end-faces-edge-is-in-two) | mitcad#343 | an end over pieces where the end face's edge is in two |
| [0560](docs/multi-turn-faces.md#0560-boptools-pcurve-checked-at-its-own-copy-in-the-face) | mitcad#380 | booleans: a p-curve checked at its own copy in the face |
| [0580](docs/fillet-replay-0550-0692.md#0580-chfi3d-kpart-contact-along-a-concave-edge-off-its-face) | mitcad#343 | a contact along a concave edge is off its face |
| [0581](docs/fillet-replay-0550-0692.md#0581-chfi3d-roundings-joined-across-a-rib-with-square-sides) | mitcad#343 | roundings joined across a rib with square sides |
| [0590](docs/booleans-and-face-merge.md#0590-shapeupgrade-unifysamedomain-wire-repeating-itself-given-up) | mitcad#235 | face merge: a wire repeating itself is given up |
| [0591](docs/multi-turn-faces.md#0591-adaptor3d-bspline-samples-within-the-adaptors-range) | mitcad#235 | a B-spline's samples counted within the adaptor's range |
| [0592](docs/multi-turn-faces.md#0592-shapeupgrade-unifysamedomain-planarity-once-per-surface) | mitcad#235 | face merge: planarity tested once per surface |
| [0593](docs/multi-turn-faces.md#0593-extrema-genextps-grid-of-very-many-knot-spans-sampled-evenly) | mitcad#235 | a projection's grid on very many knot spans sampled evenly |
| [0600](docs/fillet-replay-0550-0692.md#0600-chfi3d-onsame-end-cut-along-its-side-by-the-face-at-the-end) | mitcad#343 | an OnSame end cut along its side by the face at the end |
| [0620](docs/fillet-replay-0550-0692.md#0620-chfi3d-contact-off-its-face-before-the-end-rolls-on-the-edge) | mitcad#343 | a contact off its face before the end rolls on the edge |
| [0650](docs/fillet-replay-0550-0692.md#0650-chfi3d-kpart-cut-by-a-long-chain-of-faces-beyond) | mitcad#235 | an analytic chamfer cut by a long chain of faces beyond |
| [0651](docs/fillet-replay-0550-0692.md#0651-chfi3d-walk-from-a-seam-and-extension-ending-where-it-fails) | mitcad#235 | a walk from a seam; an extension ends where it fails |
| [0670](docs/fillet-replay-0550-0692.md#0670-chfi3d-standing-ball-sphere-in-a-gap-between-pieces) | mitcad#235 | the standing ball's sphere in a gap between pieces |
| [0671](docs/fillet-replay-0550-0692.md#0671-chfi3d-kpart-chamfer-cut-across-its-width-by-faces-beyond-at-both-ends) | mitcad#235 | an analytic chamfer cut across its width at both ends |
| [0680](docs/booleans-and-face-merge.md#0680-shapeupgrade-unifysamedomain-faces-round-a-period-without-winding) | mitcad#400 | face merge: faces round a period without winding |
| [0690](docs/fillet-replay-0550-0692.md#0690-chfi3d-end-on-a-face-along-the-line-of-its-seam) | mitcad#235 | an end on a face along the line of its seam |
| [0691](docs/fillet-replay-0550-0692.md#0691-chfi3d-onsame-end-on-a-tangent-face-the-crossing-nearest-the-corner) | mitcad#235 | an OnSame end on a tangent face: the crossing nearest the corner |
| [0692](docs/fillet-replay-0550-0692.md#0692-chfi3d-kpart-contact-end-within-a-vertex-tolerance-at-the-vertex) | mitcad#235 | an analytic contact's end within a vertex's tolerance put at the vertex |
| [0700](docs/fillet-replay-0700-0772.md#0700-chfi3d-blends-cut-by-a-narrow-face-they-roll-over) | mitcad#343 | blends cut by a narrow face they roll over |
| [0720](docs/fillet-replay-0700-0772.md#0720-chfi3d-face-at-the-end-passed-wholly-beyond-is-consumed) | mitcad#343 | a face at the end passed wholly beyond is consumed |
| [0721](docs/fillet-replay-0700-0772.md#0721-chfi3d-onsame-prolonged-edge-own-curve-on-both-surfaces) | mitcad#343 | an OnSame prolonged edge as its own curve on both surfaces |
| [0740](docs/fillet-replay-0700-0772.md#0740-chfi3d-end-face-split-round-the-vertex-pieces-of-one-surface) | mitcad#415 | an end face split round the vertex in pieces of one surface |
| [0741](docs/fillet-replay-0700-0772.md#0741-chfi3d-kpart-contact-end-at-a-vertex-on-the-edge-to-the-spines-end) | mitcad#415 | an analytic contact's end at a vertex on the edge to the spine's end |
| [0742](docs/fillet-replay-0700-0772.md#0742-chfi3d-guide-past-an-open-end-along-the-end-edges-own-curve) | mitcad#415 | a guide past an open end along the end edge's own curve |
| [0750](docs/fillet-replay-0700-0772.md#0750-chfi3d-thread-crest-rounded-on-both-edges-cut-by-the-ends) | mitcad#343 | a thread's crest rounded on both edges, cut by the ends |
| [0770](docs/fillet-replay-0700-0772.md#0770-chfi3d-rolling-on-an-edge-into-the-end-vertex-where-it-vanishes) | mitcad#343 | rolling on an edge into the end vertex where it vanishes |
| [0771](docs/fillet-replay-0700-0772.md#0771-chfi3d-contour-stops-where-the-rounding-vanishes) | mitcad#343 | a contour stops where the rounding vanishes |
| [0772](docs/fillet-replay-0700-0772.md#0772-chfi3d-stripes-joined-across-a-corner-of-the-solid) | mitcad#343 | stripes joined across a corner of the solid |
| [0790](docs/fillet-replay-0790-0822.md#0790-chfi3d-tangent-end-face-cuts-the-blend-back-along-the-stripe) | mitcad#422 | a tangent end face cuts the blend back along the stripe |
| [0800](docs/fillet-replay-0790-0822.md#0800-chfi3d-onsame-cap-points-in-one-period-of-a-stored-cylinder) | mitcad#343 | OnSame cap points in one period of a stored cylinder |
| [0801](docs/fillet-replay-0790-0822.md#0801-chfi3d-tangent-faces-within-ten-times-the-edge-tolerance) | mitcad#343 | tangent faces within ten times the edge tolerance |
| [0802](docs/fillet-replay-0790-0822.md#0802-chfi3d-contact-on-its-own-edge-near-a-vertex-passes-it) | mitcad#343 | a contact on its own edge near a vertex passes it |
| [0803](docs/fillet-replay-0790-0822.md#0803-chfi3d-onsame-cap-leaving-the-prolongation-tangentially) | mitcad#343 | an OnSame cap leaving the prolongation tangentially |
| [0820](docs/fillet-replay-0790-0822.md#0820-chfi3d-stripes-joined-across-a-corner-one-face-each) | mitcad#343 | stripes joined across a corner, one face each |
| [0821](docs/fillet-replay-0790-0822.md#0821-chfi3d-lens-rounded-on-both-edges-s-shaped-section) | mitcad#343 | a lens rounded on both edges with an S-shaped section |
| [0822](docs/fillet-replay-0790-0822.md#0822-chfi3d-rounding-rolling-on-both-edges-into-a-lens-end) | mitcad#343 | a rounding rolling on both edges into a lens's end |
| [0840](docs/fillet-walks-and-contours.md#0840-chfi3d-contour-never-takes-its-own-edge-again) | mitcad#434 | a contour never takes its own edge again |
| [0860](docs/fillet-walks-and-contours.md#0860-chfi3d-contour-runs-on-into-a-near-tangent-edge-asked-for) | mitcad#441 | a contour runs on into a near-tangent edge asked for |
| [0861](docs/fillet-walks-and-contours.md#0861-chfi3d-contact-at-the-vertex-where-the-blend-vanishes-cuts-the-own-edge) | mitcad#441 | a contact at the vertex where the blend vanishes cuts the own edge |
| [0870](docs/fillet-replay-0870-0911.md#0870-chfi3d-contact-off-a-narrow-face-across-its-tangent-edge-keeps-its-surface) | mitcad#343 | a contact off a narrow face across its tangent edge keeps its surface |
| [0880](docs/fillet-replay-0870-0911.md#0880-chfi3d-start-across-a-strip-the-ball-cannot-touch) | mitcad#343 | a start across a strip the ball cannot touch |
| [0890](docs/fillet-replay-0870-0911.md#0890-chfi3d-onsame-end-across-a-sliver-at-the-vertex) | mitcad#343 | an OnSame end across a sliver at the vertex |
| [0891](docs/fillet-replay-0870-0911.md#0891-chfi3d-bridge-over-a-gap-round-a-closed-spine-and-across-its-start) | mitcad#343 | a bridge over a gap round a closed spine and across its start |
| [0900](docs/fillet-replay-0870-0911.md#0900-chfi3d-walked-onsame-piece-generated-by-the-spine-edges) | mitcad#343 | a walked OnSame piece generated by the spine's edges |
| [0901](docs/fillet-replay-0870-0911.md#0901-chfi3d-cap-intersection-ends-where-the-walk-first-passes-the-end) | mitcad#343 | a cap's intersection ends where the walk first passes the end |
| [0902](docs/fillet-replay-0870-0911.md#0902-chfi3d-rounding-vanishing-where-its-contact-crosses-onto-a-tangent-face) | mitcad#343 | a rounding vanishing where its contact crosses onto a tangent face |
| [0903](docs/fillet-replay-0870-0911.md#0903-chfi3d-rolling-contact-crossing-onto-the-next-spine-face-goes-on) | mitcad#343 | a rolling contact crossing onto the next spine face goes on |
| [0904](docs/fillet-replay-0870-0911.md#0904-chfi3d-rounding-rolled-on-past-one-end-along-a-line-or-circle) | mitcad#343 | a rounding rolled on past one end along a line or circle |
| [0905](docs/fillet-replay-0870-0911.md#0905-chfi3d-ball-turning-about-a-vertex-of-the-edges-it-rolls-on) | mitcad#343 | a ball turning about a vertex of the edges it rolls on |
| [0906](docs/fillet-replay-0870-0911.md#0906-chfi3d-next-overflow-start-where-the-first-walk-fails) | mitcad#343 | the next overflow start where the first walk fails |
| [0907](docs/fillet-replay-0870-0911.md#0907-brepblend-pivot-released-where-a-step-lands-on-the-tangency) | mitcad#343 | the pivot released where a step lands on the tangency |
| [0908](docs/fillet-replay-0870-0911.md#0908-chfi3d-overflow-start-sampled-finer-where-none-is-found) | mitcad#343 | the overflow start sampled finer where none is found |
| [0909](docs/fillet-replay-0870-0911.md#0909-chfi3d-rounding-vanishing-a-hair-short-of-the-vertex-it-rolls-into) | mitcad#343 | a rounding vanishing a hair short of the vertex it rolls into |
| [0910](docs/fillet-replay-0870-0911.md#0910-chfi3d-onsame-cap-from-the-contacts-point-on-the-prolonged-edge) | mitcad#343 | an OnSame cap from the contact's point on the prolonged edge |
| [0911](docs/fillet-replay-0870-0911.md#0911-chfi3d-onsame-end-where-the-face-at-the-end-goes-on-tangentially) | mitcad#343 | an OnSame end where the face at the end goes on tangentially |
| [0920](docs/fillet-walks-and-contours.md#0920-chfi3d-radius-law-of-the-callers-along-a-contour) | mitcad#464 | the caller's radius law along a contour is kept |
| [0930](docs/fillet-replay-0930-0942.md#0930-chfi3d-rounding-riding-anothers-contact-line-against-it) | mitcad#343 | a rounding riding another's contact line against it |
| [0931](docs/fillet-replay-0930-0942.md#0931-chfi3d-seam-run-on-to-a-corners-cut-past-its-end) | mitcad#343 | a seam run on to a corner's cut past its end |
| [0932](docs/fillet-replay-0930-0942.md#0932-chfi3d-walk-cut-before-a-singular-section-ends-there) | mitcad#343 | a walk cut before a singular section ends there |
| [0933](docs/fillet-replay-0930-0942.md#0933-chfi3d-ends-joined-over-a-narrow-face-that-pokes-through-the-blend) | mitcad#343 | ends joined over a narrow face that pokes through the blend |
| [0940](docs/fillet-replay-0930-0942.md#0940-chfi3d-rounding-over-a-narrow-face-cut-by-the-face-beyond-its-concave-edge) | mitcad#343 | a rounding over a narrow face cut by the face beyond its concave edge |
| [0941](docs/fillet-replay-0930-0942.md#0941-chfi3d-kpart-chain-crossings-in-the-period-after-the-gaps-start) | mitcad#495 | gap crossings on a closed line in the period after the gap's start |
| [0942](docs/fillet-replay-0930-0942.md#0942-shapefix-intersecting-wires-cut-reported-only-where-cut) | mitcad#495 | ShapeFix: intersecting wires cut only where they are cut |
| [0950](docs/fillet-replay-0950-0980.md#0950-chfi3d-roundings-consuming-the-face-between-them-meet-on-one-contact-edge) | mitcad#343 | roundings consuming the face between them meet on one contact edge |
| [0951](docs/fillet-replay-0950-0980.md#0951-chfi3d-rounding-ending-at-a-step-back-of-its-wall-closes-with-a-torus) | mitcad#343 | a rounding ending at a step back of its wall closes with a torus |
| [0960](docs/offset-projection-and-pipes.md#0960-projlib-projection-boundary-bisection-stops-at-adjacent-parameters) | mitcad#504 | projection: the boundary bisection stops at adjacent parameters |
| [0961](docs/fillet-replay-0950-0980.md#0961-chfi3d-rounding-vanishing-where-its-walk-stopped-on-its-own-edge) | mitcad#343 | a rounding vanishing where its walk stopped on its own edge |
| [0962](docs/fillet-replay-0950-0980.md#0962-chfi3d-step-end-torus-cut-on-its-direct-frame-and-only-at-a-recess) | mitcad#343 | 0951's torus on either side of the blend, only at a recess |
| [0970](docs/fillet-replay-0950-0980.md#0970-chfi3d-chain-past-a-wall-corner-tighter-than-the-ball-vertex-blend-and-torus) | mitcad#343 | a chain past a wall corner tighter than the ball: vertex blend and torus |
| [0980](docs/fillet-replay-0950-0980.md#0980-chfi3d-rounding-across-slight-creases-of-its-faces-at-one-section) | mitcad#343 | a rounding across slight creases of its faces at one section |
| [0990](docs/fillet-replay-0990-1020.md#0990-chfi3d-rolled-end-cut-by-the-faces-at-the-end-past-the-balls-tangency) | mitcad#343 | a rolled end cut by the faces at the end past the ball's tangency |
| [1000](docs/fillet-replay-0990-1020.md#1000-chfi3d-chamfer-across-a-slight-concave-crease-over-a-narrow-face-grown-faces-continued) | mitcad#343 | a chamfer across a slight concave crease; grown faces continued |
| [1001](docs/fillet-replay-0990-1020.md#1001-chfi3d-guide-stretch-only-where-the-spine-turns-at-the-vertex) | mitcad#343 | 0980's guide stretch only where the spine turns at the vertex |
| [1010](docs/fillet-replay-0990-1020.md#1010-chfi3d-kpart-closed-rim-gap-round-to-its-own-first-edge) | mitcad#343 | a closed rim's gap round to its own first edge |
| [1011](docs/fillet-replay-0990-1020.md#1011-chfi3d-intersection-at-end-across-the-end-faces-seam-in-one-period) | mitcad#343 | an intersection at the end across the end face's seam, in one period |
| [1020](docs/fillet-replay-0990-1020.md#1020-chfi3d-kink-into-a-face-going-on-smoothly-into-the-end-face-also-at-a-pieces-start) | mitcad#343 | a kink into a face going on smoothly into the end face |
| [1030](docs/offset-projection-and-pipes.md#1030-geomfill-guide-trihedron-ac-second-derivative-of-the-binormal) | mitcad#539 | pipes: the guide trihedron's second derivative of the binormal |
| [1040](docs/booleans-and-face-merge.md#1040-shapeupgrade-unifysamedomain-signed-angle-to-a-reversed-axis) | mitcad#642 | face merge: the signed angle to a reversed axis |
| [1050](docs/repeated-work.md#1050-shapefix-self-intersecting-wire-edges-read-once) | mitcad#642 | ShapeFix: a wire's edges read once |
| [1060](docs/fillet-replay-0700-0772.md#1060-chfi3d-under-anothers-ball-not-past-its-other-face) | mitcad#840 | 0700's stripes under another's ball: not past the blend's other face |
| [1070](docs/meshing.md#1070-brepmesh-delabella-exact-signs) | mitcad#846 | Delabella: exact in-circle and orientation signs; an inconsistent hull fails instead of crashing or looping |
| [1080](docs/meshing.md#1080-brepmesh-delabella-exact-signs-in-place) | mitcad#884 | Delabella: 1070's exact signs without the heap, and from exact differences directly |
| [1081](docs/meshing.md#1081-brepmesh-face-checker-wire-boxes) | mitcad#884 | the healer's wire check: wires whose boxes are apart not searched against each other |
| [1090](docs/chamfers.md#1090-chfi3d-sharp-edge-limited-only-by-a-common-point-on-it) | mitcad#1005 | a corner's sharp edge limited only by a common point that lies on it |
