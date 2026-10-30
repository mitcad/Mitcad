<!-- SPDX-License-Identifier: MIT -->
# Import performance of large designs (mitcad#119)

This page records where the `.f3d` import of a large design spends its time
and memory, which ideas were measured, which were implemented, and which are
left for tickets of their own. The reference is one design of the private
backup corpus, content id `0b758b01` (256 timeline items, a 25 MB file,
stored bodies of 14 662 and 23 430 faces, 149 history states). All numbers
are from the release build (`RelWithDebInfo`) with `--time-limit 300` on a
32-core Linux workstation shared with other jobs (load average 5–19 during
the runs, so single timings vary by about ±15 %).

## Result on the reference design

| | before | after |
|---|---|---|
| 8 threads, wall (paired runs) | 149 s | 82 s |
| 6 threads, wall (traced runs) | 167 s | 89 s |
| 1 thread, wall (paired runs) | 301 s | 169 s |
| 1 thread, wall (quieter machine) | 267 s | 159 s |
| peak resident memory, 8 / 6 / 1 threads | 3.6 / 3.6 / 2.7 GB | 3.6 / 3.7 / 2.6 GB |
| items parametric / fallback / skipped | 218 / 16 / 22 | 218 / 16 / 22 |

The reports with one thread are byte for byte the same before and after;
with six and eight threads the outcomes, inputs read and final bodies are
the same. (One thread and several differ, before as after, in the notes of
two sketches and a body's volume in the 11th digit: see *Not the same with
threads* below.)

Time per item type, six threads (seconds of the replay; reading the file
comes before it):

| | before | after |
|---|---|---|
| reading: inputs found / bodies converted (in parallel) | 20.9 / 15.0 | 13.4 / 13.4 |
| sketches (75) | 35.2 | 9.3 |
| fillets (45) | 33.7 | 21.9 |
| extrusions (71) | 30.7 | 16.3 |
| base features (4; the first builds the two large bodies) | 30.2 | 13.8 |
| rectangular patterns (3) | 5.5 | 6.7 |
| replay in all | 142.7 | 71.7 |

## Where the time went (profile before the changes)

Stack samples of every thread (gdb, every one or two seconds) and flat
`perf` samples, with the import's own trace (`MITCAD_IMPORT_TRACE`):

- **The first item, 30 s:** building and healing the two large stored
  bodies (ShapeFix, 6 and 7 s on two threads), then OCCT's checker on the
  23 430-face body for 21 s on one thread: its shell orientation check
  searched each face for each of its edges (9.6 s for a shell whose two
  plates hold 23 428 edges each) and the wire classification of the two
  plates compared 8.6 million pairs of their 2 929 holes (3.5 s each).
- **Lookups of a face through a point, 18 % of the replay:** an extrusion up
  to a face, a sketch plane, a face reference measured the point's distance
  from every face of every body (`BRepExtrema_DistShapeShape` per face).
- **Sketches with linked curves, 35 s:** every linked curve was fitted to
  the projection of every edge near the sketch (115 000 edges, 74 000
  projected per sketch), and every edge was projected again for each sketch.
- **Fillet edge lookups:** the edges of every body were measured again for
  each rounding, because the import kept the edges of 16 bodies and each
  lookup read more bodies than that.
- **Counting a body's faces** mapped all faces of the shape again (10 % of
  the replay thread on the large bodies).
- **Reading:** a third of the time of finding the inputs and converting the
  bodies was the standard library's keyed hash of entity indices; the seams
  of sketch circles named history states one extrusion at a time at the end.
- **Fallbacks:** a base feature writes its bodies as B-rep data and reads
  them again (brep data, import_brep and naming 40 000 faces, about 2 s for
  the two large bodies), also when a retry on the exact state replaces bodies
  the replay already has (`fallback_to_as` with `all`): about 10 s in all.
  (Since mitcad#244 the base feature adopts the stored shape instead of
  reading its data back.)
- Booleans on the large bodies: about 7 s; OCCT's optimal box (particle
  swarm on torus faces) a fifth of the CPU, but on idle cores.

## Changes (implemented)

- OCCT port patches **0120** (wire boxes of circles and ellipses whose edge
  ranges pass their parameters by a rounding error) and **0121** (the
  shell's orientation check lists each large face's edge orientations once):
  the checker on the 23 430-face body 14.4 s → 2.1 s, statuses unchanged
  (`geometry/tests/test_large_bodies.cpp`, and the
  [port's notes on 0120 and 0121](../third_party/vcpkg-ports/docs/repeated-work.md#0120-and-0121-the-checker-on-bodies-with-thousands-of-holes)).
- A shape keeps its edges' middles and curves and a box per face
  (`Kernel::face_boxes`); lookups through a point pass over faces whose box
  lies farther than the tolerance.
- The import keeps body edges within a budget of edges (2 million) instead
  of 16 bodies.
- Sketch snaps and the projection rule's sources project only edges whose
  projected reach comes near a linked curve, each once
  (`core/import-replay/src/snap.rs`, `Projector`).
- A body's face count comes from the shape's own face map.
- The `.f3d` reader keys views, seen sets and entity maps with a quick
  integer hash (`core/asm/src/ids.rs`), finds a record's base type once, and
  works out circle seams for several extrusions at a time with threads.

## The ticket's ideas

| Idea | Status |
|---|---|
| Per-phase and per-item profile, OCCT share | Done (above). `perf` call graphs do not unwind on this machine's perf; flat `perf` plus gdb stack samples. |
| Memory: copies of bodies | Measured: peak 3.6 GB at 8 threads (the ticket's 9.4 GB predates #121's changes); history states keep distinct bodies only. Base features copied stored bodies through B-rep data; since mitcad#244 they share the stored body's B-rep (peak 3.5 → 2.9 GB at 8 threads, 2.5 → 1.9 GB at 1). |
| Faces grouped by surface | Planes per body version existed (#103); sketch planes are no longer a hotspot. Not needed. |
| Spatial index of face boxes | Implemented as per-shape face boxes for point lookups (extrusions 30.7 → 16.3 s). Edge matching by middle is not a hotspot now. |
| Per-group mass properties, divergence theorem | Measuring a body is on all cores and once per shape; under 3 % of wall. Not applicable now. |
| Incremental properties and comparison | The checker already checks only changed faces after booleans (#121); comparisons are cheap. Not applicable. |
| Lazy history bodies, cheap invariants first | Exist (bodies built on demand and ahead; Sig distances; extrusions that cannot change the volume are skipped). |
| Skip candidates by invariants | Exists; not a hotspot. |
| Local operations on a region | Exists for modelled threads. Booleans: examined in mitcad#247, not applicable (*Booleans* below). |
| Reuse between items by geometric hash | Results are cached by definition; nothing measured worth more. |
| Quadratic OCCT algorithms | Found and patched: wire classification boxes (0120) and shell orientation (0121); avoided per-face `BRepExtrema_DistShapeShape` by boxes. Upstream reports: follow-up. |
| Boolean options (OBB, fuzzy, glue, parallel) | Examined in mitcad#247: nothing to gain without changing results (*Booleans* below). |
| Validity and healing cached per shape | Exist (`checked_valid`, healed faces reused between states). |
| Adopt the file's bodies | Fallbacks do. Since mitcad#244 without a B-rep round trip: a base feature adopts the stored shape with its measures (`Kernel::adopt_shape`), the same reports, 170 → 156 s with 1 thread and 86 → 76 s with 8. The retry that replaces bodies the replay already has is left (it would change the report's names). |
| Stream the decoder and the replay | Reading (13 s at 6 threads) and the first bodies' healing (13 s) were sequential; since mitcad#245 the first state's large bodies are healed while the rest is read (*Healing while reading* below). |
| Process-level parallelism | Not applicable: one component, one dependent chain (#103's analysis). |
| GPU or SIMD | Not applicable: numeric integrals are not where the time goes. |
| Exact formulas for analytic faces | Not applicable now (see mass properties); a plane's centroid of a face with thousands of holes costs 3.5 s once per body version (`face_plane`): minor follow-up. |
| Matching faces by invariant signatures | Healed faces are matched by content hashes (#121); replay and history by signatures. Nothing to add. |

## Booleans (mitcad#247)

`MITCAD_BOOLEAN_TRACE=1` prints a line per boolean on standard error: the
operands' face counts and the time of each phase of `run_operation`
(`geometry/src/boolean.cpp`: copy, far features, OCCT's boolean, face
merge, naming, checker, holes put back) and of the whole call. Since
mitcad#855 each line also says what the checker saw after the face
merge: `checked=<n>-faces` (and `+<m>-solids`, solids of several shells
checked as solids) where only the changed faces were checked, or
`checked=whole-<reason>` with the reason the whole result was
(`unchecked-target`: a target the checker had not found valid;
`not-solids`, `solid-not-of-shells`, `no-shells`, `face-not-oriented`,
`face-twice`, `edge-not-oriented`, `free-edge`, `non-manifold-edge`,
`edge-uses-one-way`, `shell-not-connected`, `most-faces-changed`). Patterns
of features add `boolean-trace: pattern <uid> …` lines about their runs
of features in one boolean: `no run at <feature>/<place>: <why>`, `run
from … ends before …: <why>` (another body or operation, copies near
another feature's, or `face <i> of <body>, near its copies and those of
<feature>/<place>, may split`: foreseen from the face boxes), `run kept:
…` and `run not kept: …: <why>; the pattern goes on one place by one` (no
body reached, a new body, a feature's copies missing from the result, or
a face split near several features' copies, with its name). On the
reference design with one thread (release build, after
#243):

- 216 boolean calls (230 operations with the reruns without the face
  merge) take 18.7 s of a 161 s import; OCCT's boolean itself 11.1 s,
  the checker on the changed faces 1.7 s, the face merge 1.2 s, copies,
  far features and naming under 0.6 s.
- **No boolean takes a large body.** The largest target has 1 638 faces;
  the stored bodies of 14 662 and 23 430 faces are built, healed and
  compared, but no cut or join of the replay works on them any more
  (#119's "booleans on the large bodies, ~7 s" came before its own and
  #243's changes). Most cuts are of tools of 6–27 faces on bodies of
  about 1 300 faces with 170–320 holes taken out (far features already
  in place), 0.1–0.2 s each.
- The four slowest are a rectangular pattern's joins: 765 to 5 346 tool
  faces (up to 1 071 copies) into a body of 29 faces, 5.7 s together
  (2.3 s for the largest in OCCT's boolean alone).

What was tried, on the operands of the slowest operations written out
and run again alone (results compared as BRep text):

- **Oriented boxes** (`SetUseOBB`): the same result here, but no
  quicker (2.51 s against 2.35 s on the largest join; the copies and
  faces are axis-aligned, so the oriented boxes reject nothing the plain
  ones do not). Already known to change a corpus result (a hole cut kept
  an extra edge): not used.
- **Parallel mode** (`SetRunParallel`): already on; the same result as
  serial (3.9 s serial against 2.3 s on the largest join).
- **Glue options**: they assume the operands only share coinciding faces
  and never cross; the cuts and joins here cross, so a glue option would
  not be exact. Not used.
- **Local booleans** (only the faces near the tool, as for modelled
  threads, `local_boolean.hpp`): its shell is the far faces in the
  body's order followed by the kept pieces, not the order OCCT's boolean
  assembles. A body's volume, centre and reference point are sums over
  its faces and vertices in their order, so another order changes the
  volumes in the last digits, and the reports print the differences to
  the history in full. Making the order OCCT's would mean rebuilding its
  shell assembly. On the design's cuts the local path does not even
  apply (tools of several solids), and the ordinary boolean it falls
  back to, without the face merge, already gives other faces (1 390
  faces with 2 477 edges against 2 475, 6 at the same index). With no
  boolean on a large body on this design there is nothing for it to win
  either: kept global.
- The reruns without the face merge (after the merge broke a result)
  cost 0.6 s; reusing the first run's unmerged result is not safe (the
  merge writes into the sub-shapes it shares).

## Healing while reading (mitcad#245)

The first item of the reference design is a base feature with the two
large stored bodies; its history state is the large blob's oldest state
with bodies. Reading took 15.5 s with eight threads (four finding the
inputs, four converting the bodies), and the first item then waited
21 s for the two bodies' healing. Now the conversion of each history blob
converts its oldest state with bodies first, and heals its bodies of
1 000 faces or more on threads taken from the converting ones (at most
half of them; `mitcad_import::Share`, `in_order_sharing`) while the rest
of the file is read; the bodies go to the replay as bodies built ahead
(`core/import/docs/f3d-performance.md`, *Work beside the replay*). The two bodies were
healed in 12.5 and 5.4 s beside the conversion (14.8 s), and the first
item took 1.1 s instead of 22.6 s.

| `0b758b01` | before | after |
|---|---|---|
| 8 threads, wall | 88.5 s | 61.6 s |
| 8 threads, peak resident memory | 3.02 GB | 3.12 GB |
| 1 thread, wall | 178 s | 136 s |
| 1 thread, peak resident memory | 1.97 GB | 2.01 GB |
| reports | | byte for byte the same, before and after, with 1 and 8 threads |

One paired run each (the load average 12–15 during the 8-thread runs; 14
and 4 during the 1-thread ones, whose code is the same before and after:
with one converting thread nothing is healed while reading, so that
difference is the machine's load).

## History states named once (mitcad#246)

Finding the inputs names history states (`NamedState`: the bodies, faces
and edges of a blob at a state, with their names). On the reference design
it named 135 states 234 times: an item looks in its own state for its
bodies, its fillets' faces and its edges separately, an input not found
there is looked for up to 24 states back (the states of the items before
it), and the circle seams named each extrusion's state again after the
items. A state is a function of the blob and the position alone, so a
naming can be shared without changing what is found: `NamedState` owns
its view, and the inputs keep the states last used within a size budget
(`KEPT_STATES` in `core/f3d-design/src/inputs.rs`), the circle seams take
the whole circles when the items name the state, and a fillet's face
selections name the earlier states one at a time instead of all 24 at
once.

| Budget | Namings | Peak memory, finding the inputs alone |
|---|---|---|
| before | 234 | 0.61 GB |
| none kept (only shared while in use) | 188 | 0.63 GB |
| two or three states (chosen) | 184 | 0.72 GB |
| about twelve states | 155 | 1.09 GB |
| all | 135 | 5.7 GB |

Each naming takes 0.13–0.15 s on that design (about 160 000 faces, edges
and changed entities). The 24-state look-backs make the remaining
repeats: naming each state once needs about twelve states kept, and with
threads finding the inputs runs beside the conversion of the bodies,
where the import's memory peaks: twelve states raised that peak from 3.6
to 4.1 GB, so two or three are kept. The import on the reference design
(same reports, byte for byte, with one and with eight threads):

| | before | after |
|---|---|---|
| 1 thread, wall / peak memory | 154 s / 2.59 GB | 152 s / 2.52 GB |
| 8 threads, run back to back (load average 21–27) | 110 s / 3.69 GB | 92 s / 3.48 GB |
| 8 threads, first pair (load average 6–15 before, 11–17 after) | 68 s / 3.56 GB | 88 s / 3.47 GB |

The wall times follow the machine's load (other jobs ran beside them);
the namings saved are about 6 s of one thread's work. Memory is a little
lower, as each state's lists no longer keep spare room and the fillets'
look-backs hold one state at a time.

## Not the same with threads

With threads the history's bodies are built ahead, and a body built after
another state's build of the same blob took the faces it shares with it as
healed there (#121). Which builds existed first depended on timing, so a
body could be healed as a whole in one run and from shared faces in
another, and differ by an edge (113 568 against 113 569 replay edges on the
reference design): two sketches then kept one or two more linked curves as
fixed geometry, and a final body's volume differed in the 11th digit.
Since #243 each build's base is fixed by the file (the same body in the
blob's state before), waited for or built when missing, so the reports with
one thread and with several are the same.

## A dragged Extrude's preview meshes (mitcad#635)

The ticket's synthetic fixture: a 120 x 100 x 20 mm block with 83 through
holes of 4 mm and a 20 x 10 mm boss joined on top (94 faces), built by
`test_preview_meshing` through the model's commands; each sample previews a
new distance and meshes it as the preview job does. Linux workstation
(16-core desktop CPU, 32 threads), the `dev` build (OCCT's release
libraries, MitCad's own code unoptimised), not the ticket's macOS baseline.

`MITCAD_MESH_TRACE=1` before the change: all 99 faces of the result and
the tool meshed every time (the boolean's result is a copy of its operands
without triangulations: OCCT's empty copies of faces and edges drop them),
and almost all of the time in BRepMesh's face triangulation (507.8 of
527.5 ms; building its model 2.0, edges 1.5, healing 11.9, writing 3.8):
the two planes with 84 and 85 wires and about 12 000 boundary points took
504 and 487 ms each, side by side; the 83 cylinders 3 to 7 ms each. The
default (Watson) insertion of a plane's boundary points grows much faster
than linearly with them.

`test_preview_meshing --compare` (n = 10, from no triangulation):

| Options | Median / p95 | Triangles |
|---|---|---|
| final: 0.01 mm, 5 degrees | 528.3 / 541.9 ms | 48 180 |
| planes through Delabella, 0.01 mm, 5 degrees | 51.6 / 57.8 ms | 48 180 (same nodes) |
| 0.02 mm, 10 degrees | 154.4 / 171.8 ms | 24 276 |
| 0.05 mm, 20 degrees | 54.1 / 59.8 ms | 12 324 |

The interactive tier is the second: no coarser display, a tenth of the
time. With the untouched faces reused (88 of 94) the preview meshes 11
faces. `--paired` (n = 20 each, interleaved, load average 10 to 17 from
other jobs): before 794 / 896 ms, the interactive preview 81 / 106 ms,
the final tier with reuse (a commit or a refinement) 713 / 863 ms; the
block without holes 1.0 / 1.2, 1.8 / 2.5 and 1.1 / 1.3 ms. An earlier
run of the old path with a load average of about 7: 531 / 588 ms.

## A dragged Extrude's provisional tool (mitcad#636)

`tools/ui-drag-proxy-test.sh`: the same block, with and without the 83
holes, a 5 mm Join of the 20 x 10 mm rectangle, then a held drag of the
distance's arrow through the X server of a hidden Xvfb display (xdotool:
180 moves 16 ms apart, a six-tick pause half-way, about 3 s), once as the
exact previews take and once with each preview held back 500 ms
(`MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=500`, about 1 s a preview).
Linux workstation (16-core desktop CPU, 32 threads), the `dev` build
(MitCad's own code unoptimised, OCCT's release libraries), OpenGL by
Mesa's software renderer (Xvfb), 4x MSAA, scale 1; a load average of 12
to 35 from other jobs, so the counts below vary from run to run. Each
drag's line counts apart what the app got and did while the button was
held: snapped inputs, proxy updates drawn, exact previews shown, frames
submitted (`paintGL` calls, not fenced: no GPU timing).

| Drag (two runs) | Snapped inputs | Proxy updates, interval median / p95 | Exact previews, interval median / p95 | Frames |
|---|---|---|---|---|
| box | 40, 38 | 41 (67 / 82 ms), 50 (55 / 155 ms) | 14 (207 / 249 ms), 12 (208 / 333 ms) | 68, 41 |
| 83 holes | 34, 24 | 42 (63 / 145 ms), 29 (120 / 214 ms) | 8 (258 / 520 ms), 5 (548 / 861 ms) | 31, 21 |
| box, previews held 500 ms | 41, 41 | 43 (65 / 81 ms), 43 (68 / 88 ms) | 3 (1056 / 1193 ms), 3 (1131 / 1289 ms) | 48, 48 |
| 83 holes, previews held 500 ms | 23, 38 | 28 (81 / 220 ms), 40 (83 / 132 ms) | 5 (382 / 404 ms), 2 (1396 ms) | 24, 36 |

A proxy update (the stretched copy and its presentation) took 0.10 to
0.16 ms median, 0.5 ms at most; it follows every snapped input (a few more
updates than inputs: an exact preview arriving re-bases the proxy), also
while the exact preview computes. Without the delay the box's exact
previews came every 0.2 s, the perforated block's every 0.26 to 0.55 s
(the job about 0.1 s, its interactive meshing about 80 ms). On the
perforated block the frames themselves are the limit here: Mesa's
software renderer took about 67 to 85 ms a frame for its 48 000
triangles, Qt merged the mouse moves meanwhile (32 to 34 of the 180
reached the view) and so fewer inputs were snapped; the box drew a frame
in 10 to 20 ms and got 175 to 180 moves. After the release the exact
preview of the last value replaced the proxy 8 ms (box) and 351 ms (holes)
later, 1.4 to 1.5 s with the 500 ms hold. These are one machine's
software-rendered timings, not a frame rate of a GPU or of the model.

## Display meshing: planes through Delabella (mitcad#644)

Whether the final display tier (commit, refinement, opening a design) could
triangulate planes through Delabella as the interactive preview tier does
(mitcad#635). `mitcad-cli mesh-compare` meshes every body of a design twice,
all bodies together as the final tier does: (a) `viewMeshOptions` as it was
then (planes by the default insertion; now `defaultPlaneMeshOptions`, the
variant `default`), (b) the same with planes through Delabella
(`previewMeshOptions`, the variant `delabella`). Per face it
records the triangles, the largest distance of triangle centroids and edge
midpoints from the surface, the triangulated area against the face's own
(BRepGProp), triangles folded over others in the parameter plane, whether
the face has a triangulation, and its triangulation time on the mesher's
thread. Each meshing's copies are measured and let go before the other, so
a large design does not hold both sets of triangulations. `--summary`
totals reports by folder; `--dump` writes the faces the two disagree on as
BREP files.

The trial of 25.10.2026 (Linux workstation, release build, three designs
at a time beside other jobs, each design imported as the application
imports it, 6 GB and 20 minutes each): 1 334 of 1 392 distinct designs of
the local corpora compared (47 without bodies, 6 over the time, 4 over the
memory, 1 import that hung), 455 990 faces, 352 694 of them planes.

| group | achieved / target |
|---|---|
| faces triangulated (a) | 455 956 / 455 990 |
| faces triangulated (b) | 455 957 / 455 990 |
| planes with as many triangles in (b) | 352 672 / 352 676 |
| faces with a triangulation in (a), none in (b) | 4 / 455 990 |
| faces with a triangulation in (b), none in (a) | 5 / 455 990 |
| faces whose area is off in (b), not in (a) | 4 / 455 990 |
| faces folded or over the deviation in (b), not in (a) | 0 / 455 990 |

The planes' triangulation took 194.5 s in (a) and 35.6 s in (b) (summed
over faces); the meshing of all designs 431 s and 400 s, as the curved
faces, meshed the same way both times, take most of it. The designs with
large perforated planes gained most (9.7 s to 0.6 s, 9.5 s to 1.9 s); the
same work on curved faces varied by up to about 25 % per design between the
two runs with the load of the machine. The faces (b) does worse are few but
real: a quarter disc (Watson: 36 triangles) and a sliver left without a
triangulation, and a 0.018 mm² face that (a) leaves undrawn drawn by (b)
with 181 triangles over 11.8 mm², which nothing but the area comparison
detects. The ticket has the tables and the faces by design content id.

From 0.29.0 the final tier triangulated planes through Delabella and
checked each one (`io::MeshOptions::check_planes`): a plane left without a
triangulation, or whose triangles' area differs from the area its
boundary's discretization encloses (the outer wire's polygon less the
holes', from the edge points the mesher made, so next to no cost) by more
than 0.1 % or 0.001 mm², is triangulated again by the default insertion,
whose result stands. On the trial's faces that gives the default's
triangulation where Delabella left the quarter disc undrawn, leaves the
sliver undrawn as the default does instead of the 11.8 mm² patch, and
keeps Delabella's where it is right, also where the default had none.
`MeshReport::planes_redone` counts the planes made again
(`MITCAD_MESH_TRACE=1`: "planes redone N"); `mitcad-cli mesh-compare --b
checked` repeats the trial against the switched tier.

mitcad#846 put the final tier's planes back to the default insertion
(`viewMeshOptions` is `defaultPlaneMeshOptions`; the checked variant is
`checkedPlaneMeshOptions`): a large design of 7 471 faces, 6 455 of them
planes, meshed in 4.7 s with the default insertion did not end in 40
minutes with the checked Delabella, so the design could not be opened.
The checked variant stays available to `mitcad-cli mesh-compare --b
checked` until that hang is found.

## Drawing views (mitcad#962)

Adding a projected view to an assembly's drawing took about 33 s. A
drawing keeps definitions only: each view is projected by the kernel
(`Kernel::project_view`, `geometry/src/drawing.cpp`: OCCT's exact hidden
line removal, `HLRBRep_Algo`, or the polygonal one on a triangulation) and
kept in the document's projection cache by the bodies' versions and
placements and the view's direction, style and section
(`model-api/src/drawings.rs`, `view_key`), so a view whose bodies and
direction did not change is taken from the cache when the sheet is drawn
again, moved or rescaled. The views of a sheet are worked out one after
another; `add_drawing_view` runs as a job off the UI thread, and the
`drawing` query that redraws the sheet after it runs on the UI thread and
finds the new view in the cache.

`MITCAD_DRAWING_TRACE=1` prints `drawing-trace:` lines on standard error:

- `view eye=(…) exact=1 hidden=0 smooth=0 section=0 total=… ms prepare=…
  parts=27 faces=… (planes=… analytic=… freeform=…) edges=… count=… load=…
  update=… hide=… extract=… convert=… lines=… result=exact`: one per
  projection, from the kernel. `prepare` clips the parts of a section;
  `count` is the trace's own counting; `load`, `update` and `hide` are the
  exact algorithm's `Add`, `Update` and `Hide`; `extract` makes its
  compounds of visible, outline, smooth and hidden lines
  (`HLRBRep_HLRToShape`); `convert` turns their edges into drawn curves.
  The polygonal algorithm's phases are `mesh` (copies of the parts
  triangulated), `update`, `extract` and `convert`. An exact projection
  that failed adds `failed=… exact-failed` before the polygonal phases.
- `project 27 bodies exact: place=… kernel=… total=… ms; bodies 0 Body1, …`:
  one per projection, from the model: the bodies placed where their
  occurrences put them, and the kernel's call.
- `views of sheet 0 of Drawing1: 4 views, 1 projected (… ms), 3 reused, 0
  not drawn, … ms`: each time a sheet's views are worked out.
- `draw sheet 0 of Drawing1 total=… ms views=… holes=… annotations=…
  title=… compose=… rest=…`: the `drawing` query's sheet.
- `add view to Drawing1 total=… ms define=… project=… place=… store=…`:
  `add_drawing_view`.

Two values do more work, only to measure: `MITCAD_DRAWING_TRACE=compare`
also times the algorithm the view did not use on the same parts (`compare
polygonal …` or `compare exact …`, its lines thrown away), and
`MITCAD_DRAWING_TRACE=parts` projects each part alone with the exact
algorithm (`part <i> parts=1 faces=… … total=…`, `i` the body's index in
the `project` line), to show which bodies the time goes to. They combine
(`compare,parts`).

**Measured** (mitcad#962, the Windows build, a 26-body assembly with
modelled threads: 1 154 faces, 260 of them free-form, 2 816 edges): the
front, right, top and corner views took 34.9, 46.4, 20.3 and 5.3 s, of
which the exact algorithm's `Hide` 34.1, 45.8, 19.4 and 4.8 s; `Update`
0.5-0.8 s, the rest under 20 ms. Redrawing the sheet after each took
3-70 ms: every other view came from the cache. The polygonal algorithm
took 1.1-1.3 s for the same views (meshing 1.0 s of it). Alone, the
bodies with threads took 2-12 s each in the views along an axis, and
the others under 0.2 s: the hiding of the threads' helical edges and
faces is the cost.

**The quick path** (`ViewSpec::quick`, `DrawingViewSpec::quick`): a
drawing's views of bodies with free-form faces (anything but planes,
cylinders, cones, spheres and tori) are projected by the polygonal
algorithm on a triangulation within a 2000th of the view's size (the
diagonal of the bodies' box; at least the view's deflection, 0.01 mm),
its faces meshed in parallel. The algorithm's segments are joined end
to end where exactly two meet, and fitted within 1.5 times that
deviation (`geometry::fit_segments`): lines where they run straight,
arcs where at least three follow a circle (least squares, each
segment's sagitta within the tolerance too, so a hexagon stays six
lines), polylines elsewhere, broken at corners sharper than 0.6 rad.
A circle's chords end on the edge itself, so its arc has the edge's
centre and radius to the numerics. The view is `polygonal`. Cut sheets
keep the exact edges (they write the curves as they are, mitcad#853),
and bodies without free-form faces too. `test_drawing`'s "the quick
path" checks the quick view of curved bodies with B-spline faces
against the exact view of the same shapes within 0.1 mm, both ways, its
circles as arcs. The trace says `result=quick`; with `compare` it times
the exact algorithm too.
