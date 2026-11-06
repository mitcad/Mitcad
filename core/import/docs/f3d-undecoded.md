# The .f3d import: what the file does not settle, and the published format descriptions

The inputs the file's streams do not settle yet, and the published format
descriptions compared with the decoder. Part of the [.f3d import](f3d.md)
notes; the [import documents](../README.md#documents) list the other topics.

### What the file does not settle yet (mitcad#104)

Counted over both private corpora with the learning dump and per-file
traces of the import (the backup corpus repeats some designs of
`MITCAD_F3D_CORPUS`, so these are instances, not distinct items):

- **Flips no byte decides** ([*What the history settles*](f3d-history.md#what-the-history-settles), *Direction*):
  one-sided cuts through all with the flag byte 0, a few distance and
  two-sided extrusions. The history settles them with the next
  definition; files without a history get the first.
- **Patterns and mirrors of features with fillets or chamfers among
  their objects** (the case of a pattern's join whose volume differed
  from its state). A rectangular pattern of a boss, a cut and a chamfer
  between the boss and the plate gave 3979 mm³ less than its state: the
  file's six copies carry the chamfer (755 mm³ each, the five copies
  added come to about that), MitCad's copies of the features do not (a
  chamfer leaves no tool body to copy; the decoded objects leave
  dressups out). The order of the copies does not matter there (applying
  each element's copies in the features' order gave the same volume). 36
  unsettled patterns and mirrors have such objects (35 settled ones too,
  where the dressups made no difference to the state). Done in
  mitcad#105: patterns and mirrors repeat fillets and chamfers among their
  features on the copies' edges (`commands.md`, *Fillets and chamfers
  among the features*), and the import tries them (*What the history
  settles*). Over both private corpora the patterns and mirrors with such
  objects came in parametric 30 → 39 of 72 (the rest: the copies'
  roundings OCCT does not build, mostly where copies overlap or meet the
  original's rounding, mitcad#107; copies that reach no body after an
  earlier fallback; the time limit), the final bodies as before, the
  import's time unchanged.
- **Fillets and chamfers whose by-name definition fails in the kernel**:
  273 fall back with every edge found by its name. 218 of them round
  edges of stored faces (bodies a base feature brought in, the faces
  named `import`: an earlier fallback, or the file's own base
  features), whose tolerances (up to 0.05 mm) and approximations OCCT's
  dressups often do not build on; 25 round replayed faces after an
  earlier fallback; 29 bodies replayed throughout (radius or distance too
  large 10, rounding cannot end at a vertex 6, invalid shape 3, others
  not the state or names gone). In the samples of the last class the
  rounding is wider than a face next to it (a 90 mm edge rounded wider
  than the 4 mm face beside it; a rounding along a chamfer wider than
  the chamfer), so it runs over or removes that face, which OCCT's
  rolling-ball fillet does not do. These are geometry kernel limits, not
  decoding: fewer fallbacks before them is the cure for the first class.

  Classified in mitcad#107 on the 249 such dressups of both private
  corpora (192 distinct inputs), each built again on its stored input
  body by OCCT's rounding alone (the 1e-3 smaller retry and the rings
  off) and compared with the file's next state (volume within 1e-5 and
  the same face count):

  | Class | Dressups (distinct) | Built by the real rounding |
  |---|---|---|
  | OCCT's result rejected only for faults of the input away from the rounding | 12 (12) | yes: taken as it is |
  | a whole circle stored in two arcs | 3 (3) | yes: the pieces merged first |
  | knife edges (the faces' normals opposite) | 3 (2) | yes: tighter tolerances of OCCT's fillet |
  | built, but not the file's body (faces split otherwise; an input with 11 mm tolerances) | 3 (2) | built, not matching |
  | rounding wider than a face next to it: the face gone in the file's body | 54 (43) | no |
  | the same, the face partly gone | 59 (40) | no |
  | the same, the face kept or not known (state unreadable) | 44 (37) | no |
  | two roundings on the two sides of a face narrower than both | 16 (11) | no |
  | rounding cannot end at a vertex (7: the face it ends on runs on tangentially into a face of the other convexity) | 18 (13) | no |
  | OCCT's result invalid and not the file's body | 11 (10) | no |
  | no start for the rolling ball, or the walk fails, on wide faces | 8 (4) | no |
  | a bevel exactly as wide as its face, contours whose corners fail | 7 (5) | no |
  | OCCT finds no edge to round (faces 1° from tangent), OCCT exceptions | 4 (4) | no |
  | OCCT crashes | 7 (6) | no |

  Where the rounding is wider than a neighbouring face, the file's
  rounding runs over that face or rides its far edge, while OCCT's
  rolling ball stays on the two faces of the edge: it finds no start, the
  walk fails, or it cannot end. Deleting the narrow face first
  (`BRepAlgoAPI_Defeaturing`, its neighbours extended; one attempt, the
  faces next to the edges that the rounding is wider than all along) and
  rounding the edge that takes its place gives the file's body in none
  of them: it applies to 91 of the 249, the deletion fails in 49, the
  rounding in 25, 10 leave no edge where the face was, and the 7 it
  builds all take more material than the
  file's rounding (up to twice: the ball touches the extended neighbour
  instead of riding the narrow face's edge); it costs 6 s a case on
  average, up to 127 s on large bodies. It is not done. OCCT's other
  options (polynomial or quasi-angular sections, its continuity and
  tolerance parameters other than the knife edges', one contour after
  another, healing or merging the input's faces first) build none of
  the rest.

  Coverage with the 1e-3 smaller retry and the rings off in both builds
  (the dressups OCCT's rounding builds), before and after, both corpora:
  fillets parametric 237 → 238 and 967 → 991, chamfers 39 → 39 and
  162 → 164, all items 1049 → 1050 and 4677 → 4702; final bodies as
  before except one body of a design stored twice, exact before and now
  4e-6 from the file's (three of its fillets on a stored body with faulty
  faces come in parametric, matching their states within 1e-5).

  The OCCT port's edge overflow (mitcad#121, patches 0010 to 0012 of
  [the port](../../../third_party/vcpkg-ports/docs/fillet-edge-overflow.md#0010-to-0019-edge-overflow)) lets the rolling
  ball run onto the face beyond a narrow face or roll on its far edge
  where the narrow face's contact leaves it along the whole edge (OCCT
  found no start there), and drops the narrow face it consumes. Of the
  249, measured the same way: the face gone in the file's body 6 now the
  file's body (MATCH) and 11 within 1e-3 (NEAR: corner patches or
  approximated blend surfaces other than the file's), 1 off; the face
  kept 2 NEAR; the face partly gone none (OCCT's walks there reach the
  far edge and roll on it as before, then fail at the corners or build
  invalid shapes; where the contact leaves a face near the end of the
  spine OCCT cuts the rounding with the face beyond, as before, which the
  file's body agreed with in the samples). The rest fail in OCCT's
  corners at vertices, in walks that switch to a tangent neighbour on the
  wrong branch of its surface, on narrow faces on both sides of the edge,
  or are chamfers (OCCT has no chamfer rolling on an edge). The other
  classes build as before: where two roundings overlap on a face the
  file's one surface replaces both and the band between them, which
  would merge two stripes (OCCT computes both and fails at the corner
  between them); a chamfer exactly as wide as a face gets no start; and
  where roundings of one fillet consume each other's faces, OCCT stops
  because the two stripes' surfaces intersect.
  Coverage with OCCT's and the patched fillet, both corpora: fillets
  parametric 251 → 253 and 1088 → 1101, all items 1070 → 1072 and
  4874 → 4886, circular patterns 89 → 88 in the backup corpus; final
  bodies exact 525 → 523 and 3169 → 3162, the others within 0.1 %, none
  off. The fillets the patch builds mostly failed before by building
  a wrong body, so the replay took the file's state before them and
  went on exactly; now they come in parametric on the chain's earlier
  approximation (an approximate chamfer before them: up to 2e-4), so
  the final bodies are as far from the file's as that chain, and one
  pattern after such a chain no longer comes within the tolerance. One
  rounding next to a 0.0015 mm sliver now builds onto the face beyond
  it, removing 6 % more or less than the file's (its body 7e-6 off),
  which the import takes; and one fillet after three that now come in
  fails on their result.

  Second round (mitcad#121, patch 0013): the overflow runs over several
  narrow faces in a row (a strip and a sliver beyond it) and on both sides
  of the edge, and two roundings whose contacts cross on a face between
  their edges are one rounding. Of the 249, measured the same way: the
  face gone 6 MATCH / 11 NEAR → 9 / 8 (one family of three now the file's
  body: the ball reaches the face beyond a 0.0005 mm sliver instead of
  rolling on the sliver's edge), the face kept 2 → 3 NEAR, two roundings
  overlapping on a face 0 → 3 NEAR (the volume within 1e-7; the file
  merges two coplanar faces the rounding leaves next to each other). Of
  the NEARs, 9 change the volume as the file's within 1 %; 9 do not (2 to
  56 % off) and are not taken by the import's change comparison. With
  the patches 0020 to 0023 after them, the face partly gone builds 3 NEAR
  too. The rest still fail: corners where a rounding that runs onto faces
  beyond ends (`PerformIntersectionAtEnd`, `PerformOneCorner`), walks that
  fail rolling on an edge, roundings partly wider than the narrow face
  near the spine's ends, chamfers exactly as wide as their face (the
  contact lies on the face's boundary, which the hatching of the analytic
  chamfer leaves out), the second rounding riding on the first one's torus
  and earlier roundings the file's rounding replaces.
  Coverage before (main, with OCCT's overflow of 0010 to 0012 and without
  the change comparison) and after, both corpora at the same time: items
  parametric 1061 → 1034 and 4912 → 4796, fillets 252 → 231 and
  1101 → 1035; final bodies exact 523 → 527 and 3162 → 3180, within 0.1 %
  12 → 8 and 72 → 54, off as before (0 and 1). Nearly all the items that
  no longer come in parametric are results the change comparison rejects
  (above; the smallest 1.0 to 1.5 % off with other faces than the file's)
  or items after them that the replay no longer reaches the same way; the
  rest: the time limit in three large designs (as in other runs), the
  fillet after the sliver rounding above, now the file's, whose edges
  then resolve to other edges (the design is in both corpora), and one
  rounding next to earlier roundings stored as splines that the file
  rounds again (built within 1.7e-4 before, now off). The patch alone
  (the same import before and after): 1 fillet lost and 1 gained in the
  main corpus, 3 lost and 4 gained in the backup corpus.

  Third round (mitcad#121, patches 0014 to 0016): an analytic rounding or
  chamfer exactly as wide as its face (its contact line on the face's far
  edge) is split on that face and stored as a blend on the edge, so that
  the face goes (0014); a rounding rolling on an edge halves its step
  where a section cannot be reframed (0015); and of two roundings of the
  same kind overlapping on a face narrower than both, which 0013 cannot
  make one, the later rolls on the earlier one's contact line and the two
  meet there (0016). Of the 249, measured the same way: two roundings
  overlapping on a face 0 MATCH / 4 NEAR → 6 / 4; the other classes as
  before. The chamfers exactly as wide as their face now get their exact
  surface but fail in the corners at their ends; walked (non-analytic)
  blends along a face's boundary, corners after an overflow onto
  different faces beyond, rolling that pivots on a point or turns at a
  vertex and the ball leaving the edge it rolls on at the start still
  fail. Coverage before (main) and after, both corpora at the same time:
  items parametric 1059 → 1060 and 4905 → 4907, fillets 244 → 245 and
  1092 → 1094, chamfers 56 and 236 as before; final bodies exact
  527 → 529 and 3176 → 3182, within 0.1 % 8 → 6 and 58 → 52, off as
  before (0 and 1). Lost: a two-distance chamfer exactly as wide as its
  face whose ends meet a cylinder (the exact result has a face of
  inconsistent orientation; before, the 0.1 % smaller retry built it),
  and a fillet whose exactly consumed face takes an edge the file keeps.
  The results rejected at 2 to 4 % with the file's faces were compared
  with the file's bodies: one is a different rounding (0.15 mm from the
  file's along its whole length, 2.1 % off), others differ only at the
  corner patches at the ends (2.4 %) or by spline approximations spread
  over the body (2.4 %): no face count or percentage separates them, so
  the tolerance stays 1 %.

  Fourth round (mitcad#121, patches 0017 to 0019): the ends of a contact
  along a face's far edge are put at the edge's vertices also where the
  other face's contact ends first (0017: the two-distance chamfer above,
  whose exact result now is valid and the file's); a fillet or chamfer
  ending where the face at the end is split in two faces of one plane
  builds its corner (0018); and a corner that OCCT's
  `PerformMoreSurfdata` leaves undone fails cleanly instead of leaving
  point 0 in the data structure (0019, before an exception from the
  topological build or, without range checks, a write out of bounds).
  The dressups' naming gives a face that a rounding or chamfer consumed,
  where the blend runs along the face's far edge, to the blend's face, so
  that the edges the consumed face had keep their names: the fillet above
  whose later tangent chain named such an edge resolves again. Of the
  249, measured the same way: chamfers failing in their end corners
  0 → 3 NEAR (the file's volume, 1e-12; the file merges the split faces);
  three crashing specs now fail cleanly; the other classes as before.
  Still failing, with their causes: a rounding rolling on an edge stops
  where the ball pivots on a point of it (its contact on the edge stops
  moving) or at the edge's end, where it would turn onto the next edge,
  and the walk between the faces from there fails; corners after an
  overflow onto a face beyond that reaches past the face at the end (the
  rounding's end lies inside that face, not on an edge both share);
  starts where the ball leaves the edge it rolls on; and contacts leaving
  their face near the spine's end, where a walked piece with one side
  reduced to a point leaves the corner without an arc. Coverage before
  (main) and after, both corpora at the same time (three files at a time
  each): items parametric 1060 → 1061 and 4908 → 4914, fillets
  245 → 246 and 1095 → 1100, chamfers 56 → 56 and 236 → 237; final
  bodies the same (529 exact and 6 within 0.1 %; 3182 exact, 52 within
  0.1 %, 1 off). No item was lost.

  The other classes were traced inside OCCT's fillet (mitcad#122, patches
  0020 to 0023 of [the port](../../../third_party/vcpkg-ports/docs/fillet-crashes.md#0020-to-0026)).
  Measured the same way, on OCCT with the edge overflow, before and after:

  | Class (dressups) | Cause found | File's body before → after |
  |---|---|---|
  | OCCT crashes (7) | an empty restriction curve after a stripe met an obstacle face that does not hold its edge; a seam intersected with a parallel curve; the corner code reading another stripe's surface data | 0 → 2; none crashes now, 3 fail cleanly (no start past the obstacle), 2 build invalid shapes |
  | OCCT exceptions (2) | the corner of a contour closed by its sharp corner, at a vertex of more than three sharp edges, found only one of the contour's ends | 0 → 2 (3 more of the narrow-face classes now come within 1e-3) |
  | faces 1° to 6° from tangent (2) | an edge OCCT's loose tangency test (0.1 rad) calls tangent was refused although asked for | 0 → 1; the other fails in the walk at the ends of a circle between faces 1° apart |
  | OCCT's result invalid (11) | at a shallow corner of an arc and a line, the meeting point of the contact lines taken on the arc's extension | 0 → 1; the others: corner caps at vertices where a stripe ends on its face's boundary, contact lines reset to the far end of a periodic hatch at the spine's start (`SplitKPart`) |
  | rounding cannot end at a vertex (18) | the contact leaves the face near the end of the spine (a face narrower than the rounding there), and the corner (`PerformMoreThreeCorner`) takes an edge that does not touch the vertex for the continuation of another; correcting that gives invalid corners | 0 → 0 |
  | two roundings on the two sides of a narrow face (16), a bevel exactly as wide as its face (7), consumed earlier roundings (3) | the file's rounding consumes the face or the earlier roundings: edge overflow across two stripes, which OCCT does not merge, and chamfers, which the overflow does not cover | 0 → 0 |
  | no start or the walk fails on wide faces (8) | the walk stops where the contact meets a torus' seam or switches to a tangent neighbour | 0 → 0 |
  | built, not the file's body (3) | OCCT's blend split into slivers at a torus seam (the same volume, more faces) | 0 → 0 |

  None of the 249 crashes now (7 before). Coverage before and after the
  patches (both with the edge overflow), both corpora: fillets parametric
  253 → 256 and 1101 → 1112, chamfers 55 → 55 and 231 → 232, all items
  1072 → 1075 and 4881 → 4898; final bodies as before (523 and 3162
  exact). The backup numbers leave out one large design that reaches the
  coverage runner's 4 GB memory limit in both runs (before: the import's
  low-memory stop; after: OCCT's shape checker failing to allocate, a
  segmentation fault in `CSLib_Class2d`); without the limit it imports
  the same before and after.

  The classes left after mitcad#122 were traced further (mitcad#133, patches
  0024 to 0026 of [the port](../../../third_party/vcpkg-ports/docs/fillet-crashes.md#0020-to-0026)), measured
  the same way on OCCT with patches 0010 to 0023, before and after:

  | Class (dressups) | Cause found | File's body (within 1e-3) before → after |
  |---|---|---|
  | no start or the walk fails on wide faces (8) | a rounding along a whole circle whose contact line on a plane crosses six slots: SplitKPart's pieces were sorted with a stale piece, repeated and lost, and the walks between them threw (3, one design, fixed by 0024); a contact line crossing a notch in its plane (2) or a near-tangent spline edge whose approximation fails (2); a rounding wider than the earlier roundings on both sides of its edge (1, the edge overflow's class) | 0 (0) → 0 (3); the file keeps one torus across the slots and trims it with the slots' faces, OCCT runs the rounding along the slot walls (0.7 % of the rounding's volume) |
  | rounding cannot end at a vertex, the face continuing tangentially (7) | the rounding's face turns tangentially into a rounded wall at the end and the contact on it ends off any edge (the OnSame state), the end cut by the rounded wall and the faces beyond it: OCCT's intersection at end gave up on a contact off an edge (5, two designs, the face on either side of the rounding, fixed by 0026: the face's edge at the vertex is prolonged to the contact, the spine's edge cut there, the ends on the cylinder taken in its period); the rounded wall at the end is an earlier rounding that the file extends along the new one in a way OCCT's corners do not build (2) | 0 → 5 (the file's body) |
  | rounding cannot end at a vertex, other (11) | an end cut by a sliver, a cylinder's corner patch and the next sliver (2, one design, fixed by 0025: the intersection at end took the other sliver's crossing of their shared circle, did not prolong the slivers' edges to crossings beyond their ends nor cut the corner patch's edges, counted the cylinder's seam as an edge at one corner, and did not cut the end's intersection at the seam); two roundings meeting on a face narrower than both (5, the edge overflow's class; mitcad#121's patch 0013 does not build them yet); the file extending a cone at the end (2); end corners of a radius of 100 crossing on the face at the end (1); an edge of 0.0006 mm at the vertex (1) | 0 → 2 (the file's body) |
  | OCCT's result invalid (10) | the edge chain ends on both sides of a notch the file's rounding runs across (6, two designs: the file continues the torus across the notch and trims it with the notch's faces; SplitKPart also resets the contact point to the far end of the periodic hatch there; the chains end at faces 0.17 to 0.57 mm wide under roundings of 0.3 to 0.5 mm, so the rounding overflows them as in the narrow face class); side faces narrower than the rounding (2, the edge overflow's class); a face of 2e-5 mm² in the input (1); a chamfer, not traced (1) | 0 → 0 |
  | built, not the file's body (3) | a contact circle tangent to two edges of the plate (the hatch sees two crossings 0.008 mm long, which OCCT walks as slivers; merging them gives the file's faces but a plane face touching itself, which the file splits); an input with 11 mm tolerances (2) | 0 → 0 |
  | faces 1° from tangent (1), crashes that now fail cleanly (3) | the rolling ball's radius exceeds the convex curvature radius of a face (10 on a circle of 8, 1 past a corner of 0.5): the walk reports a twist and stops | 0 → 0 |
  | crashes that now build invalid shapes (2) | the end of a concave rounding where the chain turns convex tangentially (1); not traced (1) | 0 → 0 |

  Over both corpora (fallbacks on, with patches 0024 to 0026 against
  without) fillets came in parametric 4 and 15 times more and as
  fallbacks 4 and 21 times less; no final body is off. A few final bodies
  move from exact to within 0.1 %: a fillet that now builds runs on a body
  where an earlier fillet failed and was accepted within the history's
  tolerance, so the earlier fillet's missing rounding reaches the final
  body instead of being replaced by the stored state.

  Where the edge overflow stands at the close of mitcad#121 (the port's
  patches up to 0077, the later rounds' details in the [port's
  notes](../../../third_party/vcpkg-ports/docs/fillet-edge-overflow.md)), measured the same way:
  of the 249, 63 are the file's body (MATCH), 41 within 1e-3 (NEAR), 1 is
  off and 144 (117 distinct inputs) fail. By class (MATCH / NEAR of all):
  the narrow face gone 15 / 12 of 54, partly gone 6 / 13 of 59, kept
  2 / 6 of 39, not known 0 / 0 of 5; two roundings overlapping on a face
  6 / 4 of 16; the others (built before, crashes, wide faces) 34 / 6 of
  76. Where OCCT stops for the 144, traced in the fillet builder
  (distinct inputs in brackets):

  | Where it stops | Fail (distinct) | Cause |
  |---|---|---|
  | the corner at a vertex where a rounding ends | 48 (40) | the end face's intersection with the blend fails (`PerformOneCorner`, 8), no face at the end for the cap (5), the vertex's edge not found (8), the cap crosses another rounding's contact line on the face at the end (6), the intersection at the end through several faces (7), an end on a tangent continuation with no point on the arc (3), others (11) |
  | an invalid result | 27 (22) | the topological build gives self-intersecting wires (15) or unorientable faces (12): pieces that end on face boundaries, corner patches |
  | rolling on an edge after an overflow start | 15 (13) | the blend rolling on the far edge stops where the ball pivots on the edge or turns at its end; its sections are OCCT's plane sections, not the rolling ball of 0075 |
  | the walk between two faces | 16 (12) | after a contact reaches its face's edge, or from a start on it, the first step leaves the next face (11 distinct; traced: a wall's concave rounding of a smaller radius than the ball, which the ball cannot touch and should bridge; where that rounding is a cylinder between two walls on the chain's face, built since mitcad#1371, port patch 1380, as the corner's own ball rolled on between the roundings); one walk whose steps stay too large |
  | the computation of a rounding | 14 (11) | exceptions in the stripe's computation, mostly without a message; two of a fillet's roundings whose surfaces intersect (they consume each other's faces) |
  | two roundings whose contact lines cross on a face | 8 (5) | the contacts of two roundings of one fillet cross on a face they share, not all along it as 0013 and 0016 handle (`ChFi3d_StripeEdgeInter`) |
  | no start | 8 (8) | among them a wall's top rounded on both sides with one radius, the top 0.02 mm narrower than two radii: no ball touches both the bore and the other side's rounding (two equal radii) |
  | chamfers wider than a face | 7 (5) | the overflow is for fillets only: no chamfer runs over a face or along an edge |
  | no edge to round | 1 (1) | faces 1 degree from tangent |

  No single cause covers more than a few designs: each class above is a
  new construction in OCCT's fillet (corner patches after an overflow,
  rolling-ball sections on an edge, a blend bridging a face the ball
  cannot touch, chamfer overflow), which the follow-up tickets of
  mitcad#121 take one by one.

  The 48 that stop at the corner where a rounding ends were traced one by
  one in mitcad#236 (patches 0130, 0210, 0211, 0280 to 0283, 0290, 0291,
  0340 to 0342 and 0410 of [the port](../../../third_party/vcpkg-ports/docs/fillet-corners.md)). Grouped by where the
  corner code stops, with the file's body reached (within 1e-5, the same
  faces) / specs:

  | Where the corner stops | File's body / specs | Cause |
  |---|---|---|
  | the end face's intersection with the blend (`PerformOneCorner`) | 1 / 5 | a rounding whose faces come tangent at its end vertex stopped a little short of it and could not be capped (1, fixed by 0130: the end is moved to the vertex and the rounding ends in a point there, as in the file); the wall's top rounded where the wall turns tangentially into a concave corner of the rounding's radius, the floor's contact running into that corner's foot before the end (3, one part in three files); the blend's face turning tangentially into an earlier rounding at the end (1) |
  | the vertex's edge not found in the walk round the vertex | 2 / 7 (fourth round) | the foot of a rib against a bore wall rounded up to the ceiling, where a groove runs along the ceiling's junction with the wall and the rib's top under it is a sliver of 0.05 mm² beside the ceiling in its plane, the two meeting only at the point where the groove's side crosses the rib's side (1): the walk round the vertex now passes the faces between the two there, the groove's side crossed at a point (0290); the file's body (1.1e-7, the same 12 faces). A top face tapering to the vertex between a wall and a sloped face, the wall running on past the vertex into a face tangent to it (1): the top's contact runs off the top before the end, and the piece walked from there past the spine's end on the top's plane had that side given up and was dropped at the corner; it now continues the torus before it, cut by the sloped face and the face beyond the wall (0291); the file's body (the boolean differences with its state empty both ways, 39 faces). Left: two roundings of 5 on both edges of a top 3.5 wide, each rolling on the other's edge (one part in two files, 2): the corner's walk follows the face the blend rolls on, which does not hold the corner's vertex, and loops up to its face limit; the file's two tori meet each other and are both cut by the slot's wall, a corner of two blends rolling on each other's edges that OCCT does not build. A rounding of 20 whose narrow end face it consumes, beside a boss whose foot passes through the end of the contact's edge (1): its end section lies under the boss for 16 mm; with 0290 OCCT cuts the blend with the boss's wall, which gives an invalid body removing 453 mm³ more than the file's volume change (the spec stores no next state); a blend ending in the end face's plane under the boss would need a face of the boss's underside that the body does not have. A concave rounding at a five-edge vertex (one part, 2): the piece before the end ran 0.43 mm past the point where its contact left the wall (its recorded end point lies on the wall's bottom edge, its last section's contact 0.3 below it), and the file cuts both that piece's last part and the extension with the bottom face, grown along the ridge circle to where the cut meets the other contact; a corner over two surfdata with that prolonged edge, not built |
  | the cap crosses another rounding's cap on the face at the end | 5 / 6 (second round) | two roundings end at the two ends of a short edge that is not rounded, and their caps cross on the face at the end (4, one part in two files): the file intersects the two blends with each other and trims both with that face, an end of two curves; now built so (0210, the mitre an exact ellipse arc, all 4 the file's body); a radius of 100 on a body of 20 (1): the face at the end is a cylinder whose seam starts at the corner's vertex, and the cap was sought round the whole cylinder between the two sides of the seam, crossing the next rounding's cap (0211, now the file's body); edges of 0.6 micrometres at the vertex (1): two vertices half a micrometre apart where the file has one, so two roundings end at two vertices that are one within the import's geometric tolerance; with the edges shorter than 0.01 mm merged first the rounding builds within 1e-3 (the same faces) but removes 5 % less than the file, whose corner patches at those vertices differ; the merge belongs to the healing of stored bodies with the import's tolerance (mitcad#224), not done here |
  | no cap written (`OnSame` end) | 0 / 5 (fifth round: traced again, unchanged) | a plate's top edge rounded with 3 up to where a conical boss rises from the top (one part in two files, 2): the contact on the plate's side stops at the side's edge with the face beyond it, short of the cone's surface, so the side's contact never meets the cone and the cap by the cone cannot start; the file's cap needs the blend walked on over that face beyond and cut by the cone and by it, a corner over several faces not built; roundings of 10 at the feet of ribs 3 thick whose wall contact runs into an arched window of the wall (1); a top outline rounded with 1 whose last edge, an arc of 0.5, ends where a wall rises from the top, the end piece purged as an extension (1); one now fails before its corners, in the walk of a stripe (1) |
  | the intersection at the end through several faces | 5 / 7 (third round) | a tab's rounded corners whose top edges are rounded up to where the top turns tangentially into a rim's torus (3, one part in three files): the walk round one corner's vertex passed an edge of 13 micrometres and took the far one of the two edges a torus and a cylinder share (0281: the edge at the walk's vertex, prolonged from there), and at the other corners the top's contact crosses the tangent edge past the vertex, the blend cut by the torus up to that point (0280); all three the file's body (5.2e-6, the same 64 faces). A wall's foot rounded up to a hole in the floor beside the wall (2, one part in two files): the blend is cut by the hole's side and corner chamfer continued past their faces (0282) and is one face with its piece along the next edge (0283); both the file's body (3.8e-12, the same 96 faces). Left: a rounding of 0.5 whose contact on a boss reaches the boss's concave edge with a bore just before the bore's rim rounding, where the file pivots the ball about that edge in a corner patch of its own (1, not built); a rounding of 0.1 over a plane face of 0.075 mm² narrower than the radius, whose overflow onto the face beyond leaves no face of the stripe at the vertex (the walk round that face cycles), where the file's rounding vanishes 1.3 short of the vertex and leaves the rest of the edge sharp (1; its edge of 6.7 micrometres lies between the same two faces, so merging it changes nothing, and the strips of mitcad#299 do not apply) |
  | an `OnSame` end with no point on an edge | 6 / 6 (fifth round) | a concave rounding at the foot of a small cylinder's end face (a lens 2.5 high) wider than the face (one part in three files, 3): the ball rolls on the lens's edge with the cylinder's side up to both ends and on until it comes tangent to that side, where the file cuts the blend with the side, its edges with the rod prolonged; OCCT took the lens for the face at the end (0340), and at one end the walk stopped on the rod's seam short of the tangency (0341); the file's body (the boolean differences with its state empty both ways, the same 6 faces). The concave rounding of a lug's top (narrower than the radius) against a boss, the ball rolling on the top's edge with the lug's chamfer (one part in three files, 3, found in the fourth round): past that edge's ends OCCT turned the ball about them and rolled it along the lug's sides; the file rolls it on along the edge's line and cuts it with the lug's sides, one end at a vertex of four edges where the side meets the boss at that vertex only (0342); the blend is the file's (the boolean differences with the file's state are only the two pieces where the spec's input already differs from it, 0.082 mm³ each at the lugs' outer rounded edges, an earlier rounding built as another surface; the same 26 faces) |
  | others | 0 / 15 (1 now within 1e-3; sixth round below: 2 more build, with the file's rounding) | corners of two roundings that both overflow a 1 mm rim (3); corner patches of several roundings after an overflow, whose face beyond holds neither the vertex nor the edge (3, and 2 where a consumed face leaves a constraint of zero length); ends degenerate to 1e-10 at a vertex whose 4-edge corners then fail (3); a notch of 0.1 mm (1); exceptions inside OCCT's intersections and projections (2, one of them now built by 0130 but 15 % off the file's change); the edge beside the end not crossing the face at the end (1) |

  Every group left needs a new construction in OCCT's corners, mostly
  after an edge overflow; none covers more than one part.

  The others (sixth round, patch 0410 of [the
  port](../../../third_party/vcpkg-ports/docs/fillet-corners.md#0410-an-end-section-in-the-face-the-rounding-runs-into)): the specs that still fail
  at a corner on main outside the groups of the second to fifth rounds, 17,
  grouped by their first corner failure, with the
  file's body reached / specs:

  | Where the corner stops | File's body / specs | Cause |
  |---|---|---|
  | the end section lies in the face the blend runs into (0410) | 0 / 2, both build | a vault's end edge rounded with the radius of the rounded vertical corners under it (one part in two files): the ball overflows the corners onto the end face, and at the vault's springing its section is the corner's own quarter circle, which OCCT took for no face at the end (one end point at the vertex, the other inside the end face, on its edge with the corner). Now the cap is that section on the corner's face (0410). The rounding is the file's (the same torus R 18, r 2 and contacts; within the rounding's region the volume is the file's to 0.004 mm³), but the body is 1.18 mm³ off, all of it at the vault's other end, where the input already differs from the file's next stored state (a cone and a torus of radius 3 there instead of the replay's chamfer and rounding: an earlier item) |
  | two roundings overflowing a 1 mm rim meet at the rim's re-entrant corners | 0 / 3 (one part in three files) | the file's balls (radius 4) ride the rim's outer edge, 8 µm into the chamfer beyond it; OCCT's touch the chamfer tangentially, 0.013 mm from the file's. At each corner the two blends' extensions past the walls' common edge give up their wall side, so the two-stripe corner finds them on no common face (their other sides lie on the chamfer's corner piece, a fan of 4.7 µm at the rim, which does not hold the vertex). Giving the extensions the walls' faces and the vertex's parameters on the fan builds 6 of the 8 corners (the mitre of OCCT's two-stripe corner); at the other two one stripe's extension was dropped (its chamfer-side end lies inside the fan). Not kept: no spec builds |
  | the n-sided patch of a convex and a concave rounding as wide as a slot's ceiling | 0 / 2 (one part in two files) | the slot's lead-in rounding (radius 1) turns tangentially into the outer wall; the ceiling's concave rounding (1.5) runs along the opening's edge, so its faces match neither face of the walk round the vertex: its side index stays unset and the patch gets a side of zero length (the plate's curve throws). The file builds three-sided patches with degenerate edges between the convex rounding, cut halfway, and the concave ones |
  | the n-sided corner after an overflow, its edge not found | 0 / 2 | a radius of 1.5 over a 0.5 mm side face onto a concave rounding of the same radius, ending at a fin's slanted foot (four edges); a radius of 2 over a plane onto a torus beyond it, ending at a vertex whose other edges are roundings of 0.5 |
  | p-curves missing in the n-sided corner | 0 / 2 | a wall-foot rounding of 0.5 whose ends are cut by a 2 mm notch, its contacts 27 mm apart at the ends; a notch of 0.1 mm (edges of 0.102 and 0.159 mm at the vertex) |
  | OnSame ends | 0 / 2 | a radius of 10 over an 8.8 mm face at a cylinder's end (no cap); a radius of 0.4 with no point on an arc |
  | other single corners | 0 / 4 | a channel wall's end rounded where it turns tangentially into a lip (0053's crossing check); n-sided plates at 13 vertices of one rounding; a concave rounding of 0.5 whose analytic pieces are cut 5 mm apart by spline faces (the one-stripe corner leaves without a cap); an edge of 4.3 µm at the vertex (micro-geometry: two vertices 4.3 µm apart where the file has one, to be merged within the import's geometric tolerance of 0.01 mm when stored bodies are healed, mitcad#224) |

  Of the 249 specs, only the two above reach 0410's code; the 151 that
  build on main build as before.

  Seventh round (port patch 0490, and MitCad's face integrals). The specs
  still failing at a corner were run again on main with the failing
  corner's kind and message (`FailedCorner`, 0330). The two-roundings
  corners of the 1 mm rim (3 specs) are stale: their inputs are bodies
  after an earlier fallback, and mitcad#339's 0432 builds that item in the
  designs. The groups left are of one part each (two specs or fewer), so
  this round took the missing-cap group's largest part, a plate's bottom
  outline rounded with 3 together with the edges of the conical feet below
  it (one part in two files, `98a29231` and `24523ade` item 32):

  | Where the corner stops | File's body / specs | Cause |
  |---|---|---|
  | no cap written (`OnSame` end) at a conical foot | 2 / 2 (the file's change; one face more) | the cone is the face at the end; the side's contact, 3 above the bottom, meets the cone continued above its rim before the edge's end, but the stored cone lies on a surface trimmed at the rim, which the corner's extension of the face at the end (B-splines only) left bounded: no intersection. Now the face is taken as its basis surface continues where the bounded one is not crossed (0490); the boolean differences with the file's state are empty both ways. The one face more is a blend split at the input cone's seam at another corner, where the file's cone has its seam elsewhere |

  The faces past their trims then measured wrong: the corner extends the
  cone above its rim on the same trimmed surface (as OCCT's corner of two
  roundings already did at the three other feet), `BRepTools::UVBounds`
  clips the face's bounds at the trim, and the face integrals clamped the
  boundary there (`geometry/analysis/src/face_integral.cpp`), so the
  rounding removed 12.6 % more than the file's by MitCad's measure (OCCT's
  agreed with the file). The bounds now widen to the boundary curves'
  extent within the basis surface's bounds. Test: a cylinder cut by an
  inclined plane, its side on a surface trimmed below the cut, in
  `geometry/analysis/tests/test_analysis.cpp` (fails without the change).

  Imported again by content id (before → after):

  | Design | Features parametric | Inputs read | Items matching the history |
  |---|---|---|---|
  | `98a29231` | 76 → 77 / 88 | 56 → 57 (1 inferred) | 54 → 55 |
  | `24523ade` | 76 → 77 / 88 | 56 → 57 (1 inferred) | 54 → 55 |

  Item 33 after it (a rounding of the roundings' edges) still falls back,
  now on item 32's result: its walk stops where it rolls on an edge of a
  neighbouring face (mitcad#237's class, not traced here).

  Left, each one part: the slot whose lead-in and ceiling roundings meet
  (2, n-sided patch with a side of zero length); a rounding of 5 on both
  edges of a 3.5 mm top (2, `PerformIntersectionAtEnd` reaches its face
  limit); a concave rounding at a five-edge vertex (2); the boss whose
  rounding pivots about a concave edge (2); a rounding of 0.1 over a face
  narrower than the radius (2); the n-sided corners after an overflow (2);
  missing p-curves in n-sided corners (2); single OnSame ends (rib feet
  into an arched window, the last edge of an outline at a wall, a rounding
  of 10 over an 8.8 mm face: 3; their faces at the end are planes and a
  cylinder on untrimmed surfaces, so not 0490's case).

  Eighth round (port patch 0540). The specs still failing at a corner were
  run again with the port of main (0500 to 0512 in): unchanged. Their
  designs were then imported again with the failure dump, and every failed
  definition run through OCCT's fillet alone. Several specs are stale: their
  designs no longer reach that corner (`6ec2d3e0`, the rounding of 10 over an
  8.8 mm face, is parametric; `236b88c6`'s item now fails in its stripe; the
  slot of `81b4c9eb` now fails where two roundings overlap). The corners the
  designs fail at today, by item, with the cause:

  | Item | Where the corner stops | Cause |
  |---|---|---|
  | `61526c66` 13 | OnSame end, an exception inside the intersection | a wall's foot rounded with 0.5 where an earlier rounding of 0.5 leaves the wall tangentially (its spine curving away, its floor contact crossing the wall's plane at the spine's end vertex, 1.5 past the section where the two spines are tangent): the new blend's end section lies in the earlier tube. Fixed at one end (0540: the cap is that section, the earlier rounding grows up to it, its floor edge prolonged along the tube's iso; the faces there are the file's). At the other end the earlier rounding ends 0.02 above the floor and 0.163 from the wall, a planar sliver (0.163 × 0.0199) closing the gap at the vertex; the file's earlier rounding grows over the sliver, which the corner would have to consume. The item still falls back |
  | `f9d6d78a` 24 | no cap (OnSame) | a top outline rounded with 1 whose last edge, an arc of 0.5, ends where a wall rises from the top: the top's contact reaches the wall's foot on the edge before the arc, the walk gives that side up for the rest (an extension, dropped at an OnSame end), and no piece runs along the arc. The file cuts the cylinder piece past that point and the torus piece along the arc with the wall's plane, the wall growing down to them: the pieces past the crossing on the top's plane and a cap over two pieces, not built |
  | `f5e08b18` 23 | the edge beside the end does not cross the face at the end (0460's cap across the face beyond) | a rounding of 0.5 along a 0.073 mm floor edge at a wall's step: the floor contact runs from a rounding of 8 onto a floor strip 0.073 wide, and the face at the end is a concave rounding of 3, tangent to the floor along a line 0.073 from the rounding of 8's; the strip's edge with the rounding of 8 passes 0.9 µm below the rounding of 3 (exact geometry, two cylinders tangent to the floor along parallel lines), so it never crosses it. The file cuts the extension piece over the rounding of 8 and a piece over the strip with the rounding of 3, whose floor edge it prolongs by 0.5: the piece over the strip and a cap of two curves, not built |
  | `db73863f` 27 | no cap (OnSame) | a rounding of 0.6 along a thread's root helix ending at the thread's start plane: the flank's contact reaches the plane 0.2 (in height) before the root's contact, 14° earlier along the helix, and the root contact's curve ends there; it meets the plane only on the blend walked on past its first section (the flank continued below the plane), not built |
  | `7b638e0c` 69 | no cap (OnSame) | a rounding of 10 between a floor and a wall in a 3 mm gap between ribs 5 high, the wall's contact running into an arched window: the piece's contacts end 15 mm apart (the floor's between the ribs, the wall's at the window), the ribs' sides holding only the floor's end; not traced further |
  | `a6123cc1` 38, `db73863f` 74 and 75, `49ccc315` 35 | the n-sided corner (`PerformMoreThreeCorner`): an edge at the vertex has no curve on a face it takes it on | after an overflow the faces round the vertex are not those the stripes run on, and the plate's side between two stripes is taken on a face that does not hold the edge. For `49ccc315` 35 (and `e41d24ac`, the same part) the stripes themselves differ from the file's: two roundings of 5 on both edges of a 3.5 mm overhang, which the file makes one S-shaped profile (the convex rounding tangent to the concave one's torus, their axes 10 apart), while OCCT's roll each on the other's edge (0430) |
  | `3fe35ae9` 21, `61526c66` 12 | two faces met round the vertex share no edge | the n-sided corners after an overflow of the sixth round, unchanged (`61526c66` 12: the rounding of 0.1 over a face narrower than the radius) |
  | `99ef321f` 21 | the n-sided plate's projection of a curve throws (`GeomPlate_BuildPlateSurface::ProjectCurve`) | not traced further |
  | `99ef321f` 22, `5ef2dbd5` 95 | `PerformIntersectionAtEnd` through several faces | the boss whose rounding pivots about a concave edge (third round), unchanged |
  | `db73863f` 143 | intersection at the end | a thread's pieces on a cone and B-splines; not traced further |

  Traced and not kept (no design reaches them today): at the stale
  rounding of 10 over an 8.8 mm face (`6ec2d3e0`'s spec), the stored bore is
  one face spanning more than a period (the plate's part round the whole
  circle with its seam, the post's part from 5° on in the next period, a
  closed edge between their copies along the plate's underside). At one end
  the end point lies at a vertex of three edges and was put on the edge
  towards the face the blend consumes; at the other the prolonged edge
  crosses the seam's foot and then runs along the closed edge. Taking the
  edge towards the face holding the vertex, ending the pieces at the seam's
  vertex and cutting the closed edge there give the file's change (3.1e-7),
  but a synthetic bore with the closed edge's direction reversed then gives
  an invalid body, so the closed edges' orientation is not handled yet. At
  `236b88c6`'s lens (stale spec) the walked blend's last span turns sharply
  at the end section (its derivative across the sections 11.3 against 0.90
  one step inside), so 0340's tangency test fails at the second end; with
  the normal taken from just inside, the corner builds, but the lens's other
  end, where two more contours end at 4-edge vertices, gives an invalid
  body. The extra face of `98a29231` and `24523ade` item 32 (a blend split at
  the input cone's seam) is not a corner fix: OCCT splits the blend where it
  crosses the seam of the cone it runs on, and the file's cone has its seam
  elsewhere after the item (the file moved it), which would need the face
  re-seamed.

  0540's code is reached by `61526c66` 13 only of the target designs'
  failed definitions (378 runs, the others identical) and by the same part's
  spec among the corner specs.

  The invalid results (27), the contact lines crossing on a face (8) and
  the exceptions in a rounding's computation (14) were traced in
  mitcad#239 (patches 0150 to 0153 of [the
  port](../../../third_party/vcpkg-ports/docs/fillet-results.md#0150-to-0153-invalid-or-failed-results)); the first failing step
  is found with a dump of the fillet's data structure before the
  topological build. With the file's body reached (within 1e-5, the same
  faces) / within 1e-3 / specs:

  | Group | Reached / near / specs | Cause |
  |---|---|---|
  | invalid results | 1 / 2 / 27 | fixed: a closed rounding rolling on an edge from a vertex where a seam starts, whose seam was cut (0152, the file's body); a corner on an end face split in two faces of one plane (0151, the volume exact, the file splits the two faces along another line: one face more); a corner beside a side face split along an edge in the face at the end (0150, the file's four roundings exactly, its stored state is a different body). Left: the tip of an outline where the chain ends at both vertices of a 0.57 mm edge and runs over a corner arc of the rounding's radius (3, one part); walked blends on stored spline faces whose approximation degenerates (edges of 8.8 mm tolerance, 3, one part); two roundings meeting at the vertices at both ends (two-stripe corners) that leave blend faces of 0.04 mm² (2, not traced further); walked blends of short edges at 4-edge vertices extended far past their spine (2); corners where one contact point is not on an edge (2, and a rib outline with 0.12 mm edges at the corners, 3, one part); concave and convex roundings on both sides of a rib narrower than both, the file's concave one riding on the convex one's torus (1); a concave arc turning tangentially into a convex one at the rounding's end (2); a face of zero width (two coincident circles) at the chain's vertices (1); a radius of 100 between two cylinders ending at 4-edge vertices (1); a corner over several surface data leaving a blend face of zero area (1); chamfers, not traced (3) |
  | contact lines crossing on a face | 0 / 0 / 8 | two roundings 5.8 apart with radius 4, whose blend cylinders the file trims against each other (2); a corner's projection exception, then one contact ending on another's (3, one part); contacts coinciding where a rib is exactly two radii wide: 0153 makes the blends meet where the face between is a strip (a plate's two edges rounded to a full round), but here the rib's top is one face with the ring's top, which would need the ring's edge across the rib's root (2, the file's states unreadable in the dump); coinciding over part of a full circle and an arc (1) |
  | exceptions in a rounding's computation | 0 / 0 / 14 | the contact on one face stands still while the ball pivots about it, where the rounding meets a convex corner rounded with the same radius (the file puts a sphere patch there): the walk sets no transition on that side (11, 9 distinct); a walked piece of zero width along the chain's last edge, whose faces are tangent there, inside the faces (not an extension, so the stripe fails; it would have to end where the faces become tangent, 2); a rational section's weights (1) |

  The pivoting ball is the largest group: OCCT has no blend for a contact
  standing still, and the file's sphere patch replaces part of the
  earlier rounding.

  Chamfers wider than a face (mitcad#240, patches 0140 to 0143 of [the
  port](../../../third_party/vcpkg-ports/docs/chamfers.md#0140-to-0149-chamfers-wider-than-a-face-beside-them)): of the 7 specs (5
  distinct inputs), all 7 build now, 4 the file's body (MATCH: a step
  ring's rim chamfered as wide as the ring, the rim in two arcs; a
  hexagonal prism's foot under a disc whose chamfers cross beyond the
  disc's rim, twice; a lug in a bore as wide as its chamfer) and 3 within
  1e-3 (NEAR) where the stored next state differs from the stored input
  elsewhere too (a rib's top; other features' chamfers), the chamfer faces
  themselves the file's (the cone areas of four boss feet within 0.5 mm²
  of the file's). Causes, each fixed in the port: the hatching of a contact
  lying on a circular or closed face edge (moved across the boundary,
  domains across the start of a closed line), a closed contact along a
  closed edge not stored on that edge; a chamfer's OnSame end whose second
  contact meets the face at the end beyond its edge, looked for on the
  bounded face; a chamfer that adds material running a little past its
  face's outline, where the faces beyond must grow down to it (their edges
  prolonged) instead of being cut back; and two chamfers whose contacts
  cross beyond their common face, where the face beyond cuts both.
  Imported again with the port before and after (the ten files of the
  specs' designs and of those whose chamfers fell back with "chamfer
  failed" in a recent run): 5 of the 8 such chamfers and 2 fillets now
  parametric, nothing lost, the final bodies as before; 3 chamfers still
  fail (not among the specs, not traced).

  Rolling on an edge (mitcad#237, patches 0170 to 0172 of [the
  port](../../../third_party/vcpkg-ports/docs/fillet-rolling-ball.md#0170-to-0172-the-rolling-ball-and-its-pivot)): the blend rolling on
  an edge is now the rolling ball (as 0075 for two faces), turns about
  the edge's ends, and where its contact leaves its face before the end
  is walked on and cut by the face at the end. Of the 15 specs of the
  class (13 distinct inputs) none reaches the file's body; traced, most
  are not a ball rolling on an edge in the file: a rounding of a narrow
  step that the file builds as two tangent tori of opposite curvature
  eating the floor (3); a thin rim whose other rounding fails in the walk
  between two faces (the bridge of mitcad#238) and whose rolling part
  reaches a vertex where the next edge turns towards the ball, so that
  the ball touches it before the vertex (3); a notched rim, the file's
  torus kept over the notches and cut by their faces (2); a ledge
  narrower than the radius at a corner of smaller radius, the file's
  straight blend extended and cut by the corner's wall (2); a floor with
  a hole under the contact, the file's blend cut by the hole (1); a start
  where the ball does not rest on the edge but on the face beside it, with
  no start between the two faces either (3, one of radius 129); a fine
  rim whose rolling piece fails in the approximation (1). Of the 38 specs
  that built through the blend on an edge, one more reaches the file's
  body (a corner whose rounding is coaxial with the circle it rolls on),
  one is closer (7.9e-5 to 3.7e-7, the file's face in three pieces) and
  the others are as before.

  The two constructions round 1 lacked (mitcad#237, second round, patches
  0390 and 0391 of the [port](../../../third_party/vcpkg-ports/docs/fillet-rolling-ball.md#0390-and-0391-the-next-edge-and-two-edges)): a
  ball rolling on an edge that comes to an edge turning towards it touches
  that edge before the vertex and rolls on from there, a piece of the ball
  between; a ball whose faces are both too narrow rests on both far edges
  (closed contours only: the corners at an open contour's ends are not
  made).
  Reached the file's body / specs of the two groups that needed them: 0 / 5.
  The thin rim (3 specs, one part, no stored state: only the volume change
  is known) now passes both of its turns, near the rim's ends where its
  inner edge turns into a spoke, but the ball's piece at the turn cannot
  be made: its contacts on the edges lie more than a right angle round the
  ball from its contact on the outer wall (96 degrees), so that the
  sections rise 0.0067 mm over the top face beside the spoke, and between
  them the sphere crosses neither the spoke's wall nor the inner wall, nor
  the edge between them (at 20 degrees round the ball, the sections at 0 to
  7). The notched rim (2 specs, one part) passes its turn at the tooth the
  ball touches 0.076 mm below the tooth's corner, but there too the ball's
  piece crosses neither face's common edge; the file there is no ball rolling
  on an edge (its torus touches the cylinder below the rim's band and is cut
  by the notch faces, 0.003 mm from the rolling ball), so this group stops
  here. Of the 38 specs that build through the blend on an edge, all give
  the same results with 0390 and 0391; none of them reaches the new code.

  Third round (mitcad#237, patches 0500 to 0502 of the
  [port](../../../third_party/vcpkg-ports/docs/fillet-rolling-ball.md#0500-to-0503-coaxial-surfaces-flats-and-gaps)): neither group is a
  ball rolling on an edge in the files, and both are now built the files'
  way. Reached the file's body / specs: 5 / 5. The notched rim (2 specs):
  the rounding between the coaxial cylinder and flare is a torus (0500),
  cut round each flat under the overhanging rim (0501); the result has
  the stored state's faces (46), and the boolean differences with it are
  empty (its volume change 3.838 against the spec's 4.567, which the
  stored body's own measure gives; the ring the rounding takes is 3.835).
  The thin rim (3 specs): its outer edge, rounded wider than the rim, is
  cut by the rim's round end caps where the contact comes back from over
  the pocket (0502); the body has the stored state's 56 faces, the boolean
  differences with it are empty and the precise volume change agrees
  within 1.1 · 10⁻⁴ mm³ (the spec's change, -26.2216, is the default
  measure's). The coaxial torus also replaces the walked B-spline of every
  rounding between a cylinder and a cone about one axis, and the dressup's
  fallback of mitcad#311 (flats cleared and cut again) is no longer
  needed for the pole flats.

  The walks between two faces were traced further (mitcad#238, patch 0160
  of the [port](../../../third_party/vcpkg-ports/docs/fillet-walks-and-contours.md#0160-walks-over-a-rounded-rim): a walk over a
  hole whose rim is rounded more sharply than the ball goes on over it on
  the face's own surface and the rim cuts it). Of the 16 (12): in 9 (8)
  the ball would have to pass a face it cannot touch onto the face beyond
  it (a corner rounded with 0.5 or 1 under a rounding of 1 or 1.2 along
  the top edges, a corner pocket of radius 1.25 under a rounding of 3,
  roundings of 0.5 at both ends of the edge under a rounding of 1): at one
  section the ball touches three faces, the pieces before and after it end
  on different arcs of that ball, and the rolling ball leaves the gap
  between them over the face it cannot touch open (the file's body closes
  it with faces of its own), so there is no rolling-ball construction to
  make; they still fail, the last one now at its end corner. In 2 (1) the
  contact crosses such a hole, but the stripe's start overflowed an earlier
  rounding all round a closed edge, a face that nothing cuts loose (the
  result kept it), so the walk over the hole is not taken there. In 3 (1)
  both contacts leave their faces at one section; 2 (2) are chamfers whose
  contact crosses a cylinder's seam where the other contact lies on its
  face's edge. The 249 measured the same way give the same classes with and
  without 0160 (on top of 0130–0153 and 0170–0172: 70 MATCH, 45 NEAR, 1 OFF,
  133 failing); nothing is lost.

  The 2 (1) whose start overflowed an earlier rounding all round a closed
  edge (mitcad#261, patch 0190 of the
  [port](../../../third_party/vcpkg-ports/docs/fillet-walks-and-contours.md#0190-round-a-closed-stripe)) now reach the file's body
  within 1e-3 (NEAR: 7.3889 mm³ removed where the file removes 7.3902, the
  walked blend in four pieces where the file has one torus). The earlier
  rounding vanishes once its neighbours are cut; what kept it was the
  walk over the hole's rim, whose piece ended 0.011 mm off the walked
  section where the contact on the cylinder crosses the cylinder's seam,
  so that the seam kept its piece under the blend, and with it the
  cylinder's lower part and the earlier rounding. The other 6 specs of
  mitcad#107's class "no start or the walk fails on wide faces" give the
  same results with and without 0190 (3 the file's body, 3 failing at
  their end corners).

  The ball turning about a standing contact (mitcad#239, patch 0200 of the
  [port](../../../third_party/vcpkg-ports/docs/fillet-results.md#0200-a-walked-blend-about-a-standing-contact)): where a rounding runs
  past a corner rounded with the ball's own radius, its contact on the
  face across the corner stands still and the blend is a piece of the
  ball's sphere, as the file's sphere patches. On stored (spline) faces
  OCCT's walk did not build it: the line got no transition on the
  standing side (`Standard_DomainError`), that side's twist flag was
  numerical noise, and the blend's orientation was taken at its point
  side. With 0200 the sphere pieces build in all 11 specs (9 distinct
  inputs; in one, three of its four), where a result can be compared the
  file's spheres (four of radius 0.1, 0.011 mm² each).
  Reached the file's body / within 1e-3 / specs: 0 / 1 / 11 (0 / 1 / 9
  distinct). The others stop after the sphere pieces, at causes of other
  classes:

  | Specs (distinct) | Where they stop now |
  |---|---|
  | 1 (1) | within 1e-3: the file's four spheres; a corner of three roundings at another vertex is OCCT's corner patch, not the file's (8.6e-5 mm³, 3 % of the change) |
  | 3 (3) | one part: a later stripe's extension, rolling past its face on a cylinder's own unbounded surface (0076), stops on that surface's seam, an edge without vertices (`Standard_NullObject` in `ChFi3d_FilCommonPoint`); handled in a trial, the result has self-intersecting wires: the contact on the top of the analytic piece before the sphere starts 24 mm off, where the torus' contact circle, which the mirror-image half of the outline shares, first enters the top face (SplitKPart's hatch domain, not cut at the piece's start) |
  | 3 (2) | one part with two pointed tips: the check for crossing contact lines (`ChFi3d_StripeEdgeInter`) finds two stripes' contacts on the top overlapping; one stripe's extension for the corner where the two meet runs 15.9 along the whole other side and is not cut back on the top |
  | 2 (1) | a pocket whose inner corner, drafted, is rounded with 0.42 at the floor under a ball of 0.5: the ball cannot touch that rounding and should bridge it (the class of mitcad#238); the walks stop on both sides ("Chaining is impossible") |
  | 1 (1) | the corners where two stripes meet next to their sphere pieces (`PerformTwoCorner`, `PerformMoreThreeCorner`): a contact p-curve on the common face has no tangent there (`LProp_NotDefined` in `CalculBatten`) |
  | 1 (1) | three of four corners build; at the fourth the chain ends just before the corner and the extension past its end starts inside the turn with both contacts on their faces' boundaries; the walk makes no step |

  The 84 specs that built before give the same results with 0200.

  Two of those groups were traced further (mitcad#239, third round,
  patches 0240 and 0241 of the
  [port](../../../third_party/vcpkg-ports/docs/fillet-results.md#0240-and-0241-the-third-round)). Both are one part, an
  outline of arcs of one cylinder whose rim is broken at its tips by small
  V notches (0.4 wide, 0.1 deep) whose edges are not rounded, the rounding
  (0.3 and 0.5) wider than the notch is deep: its contact on the top passes
  above the notch, the file's torus runs on over it, cut by the notch's
  faces, as one face. The edge chain stops on both sides of each notch;
  OCCT ran each end's contact on the top on to the end of its domain on
  that face, round the other half of the outline, and the corner did not
  cut it back. 0241 joins the two ends over the notch: one blend runs on,
  cut by the notch's faces, up to the other's end section, and the two are
  one face. Reached the file's body (within 1e-5, the same faces) / within
  1e-3 / specs:

  | Group | Reached / near / specs | Cause |
  |---|---|---|
  | contacts of two stripes overlapping on the top | 3 / 3 / 3 (2 distinct) | fixed by 0241 (6.1e-6 of the body's volume; the boolean differences of the result and the file's state both ways are empty, so the remainder is the volume integration on the input's spline faces) |
  | an extension stopping on its own surface's seam | 0 / 0 / 3 (one part) | 0240 keeps the seam of the face the extension rolls on (0076) out of the ball's way; 0241 joins the ends over two notches. The rounding still fails as recorded: the file rounds the lower outline's both arcs, but the decoded edge list has one of them only (two of the fillet's 23 edge references in the file resolve in the design dump to two short arcs the list already holds; not traced further), so the recorded rounding ends at a notch whose other side is sharp, with no rolling-ball end. With that arc added the three specs give the file's next state (2.7e-6, the same 53 faces, the boolean differences empty) |

  Of the specs that built before, the six that reach the changed code (an
  extension on a face's own surface, a cut piece at a vertex of the
  contact's face) give the same results with 0240 and 0241.

  The second group's missing arc (mitcad#305, mitcad#307, patch 0260 of
  the [port](../../../third_party/vcpkg-ports/docs/fillet-walks-and-contours.md#0260-a-fillet-carried-across-a-notch-in-its-rim)): the decoded edge
  list is the file's own (both references the arc seemed to need name, in
  the file, edges the list already holds). The file's rounding carries on
  from the lower outline's left arc across the three top edges of a small
  tapered hole (0.28 to 0.29 long, 0.25 deep, 0.57 wide at the rim; the
  radius 0.5) onto the right arc between the same top face and the same
  cylinder: one torus face covers both arcs and the notch's edges, which no
  input names. MitCad's fillets now carry a contour of constant radius on
  across such a notch (its edges within the rounding's contact on the top
  and within the ball's diameter of the contour's end) onto the edge beyond
  it on the same line or circle between faces of the same surfaces; 0241
  then joins the two over the notch. Reached the file's body: 3 / 3 (the
  volume within 2.7e-6, the same 53 faces, the boolean differences with
  the file's next state empty both ways). Of the other specs, one more
  reached the carry before the width bound was added (a groove 1 long and
  0.0043 deep beside a rounding of 0.3, which the file's rounding does not
  cross: the edge beyond it stays sharp in the file's state); with the
  bound none does, so their results are unchanged.

  The group of contact lines crossing on a face was taken up again
  (mitcad#239, fourth round, patches 0270 and 0271 of the
  [port](../../../third_party/vcpkg-ports/docs/fillet-results.md#0270-and-0271-roundings-meeting-on-one-contact-line)), on synthetic bodies
  built after the classes above; the specs themselves were not rerun in
  this round.
  - Contacts coinciding where a rib is exactly two radii wide and its top
    is one face with the top it stands out of (2): 0270 makes the blends
    meet on the line where the face goes on past the line's ends; the
    corners at the rib's root cut the top off along the prolonged edge of
    the face at the root (the ring's edge across the rib's root), and the
    two corners' points there are made one. Ribs out of a plate, between
    two plates and out of a ring, and the concave slot, build with the
    exact volumes.
  - Contacts crossing part of the way (the two roundings 5.8 apart with
    radius 4, 2): where the contacts cross twice on a face that narrows
    to less than the two roundings' width, 0271 cuts the two blends by
    each other between the crossings and leaves the face in two pieces,
    as the files' bodies do; the result has the volume and the faces of
    the body rounded at each edge alone, the two in common (fused, for
    concave roundings), also with different radii and on spline faces.
    Contacts that overlap all along (a plate thinner than the roundings'
    diameter) stay with 0016's ride, whose blend is not the files' cut;
    where such a pair also ends against a face that goes on (a rib), the
    corner at the rib's tip still fails.
  - Not changed: contacts coinciding over part of a full circle and an
    arc (1), which needs the closed stripe's line split where the arc's
    ends cut the face; and the corner's projection exception before one
    contact ends on another's (3, one part), not reproduced.

  Fifth round (mitcad#239, patch 0300 of the
  [port](../../../third_party/vcpkg-ports/docs/fillet-results.md#0300-a-prolonged-end-edge-across-a-seam)), measured the same way on
  the 249 with the port as of this round, before and after: MATCH 88, NEAR
  51 → 54, OFF 1, failing 109 → 106; every other spec gives the same result
  line. The three are one part (a rib of radius 2.5 along a rod of 4.5,
  its foot rounded with 3, the rib's end a spline face): the rib's end face
  grows down to the rounding's contact on the rod, and the rod's seam runs
  between the foot and the contact, so the prolonged edge is cut at the
  seam; its pieces' curves on the end face were the ones computed on the
  rod (the edges of 8.8 mm tolerance that the first round saw). Now valid
  and within 4e-5 of the body (the same 8 faces); the change, 4.885 mm³
  against the file's 4.839, is off by the walked blend's extension to the
  end face (the contact ends 0.07 mm from the file's on one side, 0.008 on
  the other): the file's blends are cylinders, OCCT walks a blend between
  two cylinders and extends its spline. Of this ticket's classes, what is
  left (specs, distinct inputs) is one part each:

  | Class | Specs | Cause |
  |---|---|---|
  | invalid results | 3 (1) | a crescent top whose middle, at a notched tip, is narrower than two radii, rounded all round in one contour ending on both sides of the notch: the contacts from both sides overlap within one stripe and the top keeps its original edge (self-intersecting wire) |
  | | 2 (1) | knife edges (the faces' normals opposite) 0.5 long rounded with 0.2: walked blends wrapping 166 degrees at 4-edge vertices, approximated with 0.05 mm tolerances |
  | | 2 (1) | a fin 1.5 thick rounded on both sides with 0.75 along a chain of faces 1.5 wide (its edge, two earlier roundings, end faces): a full round over several faces that ends on faces that go on (0153 and 0270 handle one strip face) |
  | | 2 (1) each | two-stripe corners leaving blend faces of 0.04 mm² and corners where one contact point is not on an edge (as the first round traced them); an input whose edges have 11 mm tolerances |
  | | 3 (2) | chamfers, not traced |
  | | 7 (7) | not traced further (single specs) |
  | | 1 (1) | a radius of 100 between two cylinders ending at 4-edge vertices |
  | contacts crossing on a face (`ChFi3d_StripeEdgeInter`) | 3 (1) | a boss on a lug whose foot circle touches the lug's rounded end at one point, the lug's face pinched into two faces there: the boss's foot, in two arcs, rounded with 1.5 runs out over the lug's edge at that point (the corner's filling surface fails in a projection, the retry's two stripes then cross) |
  | | 1 (1) | coinciding over part of a full circle and an arc |
  | exceptions in a rounding's computation | 2 (1) | a walked piece along the chain's last edge whose faces become tangent: the section's opening angle reaches zero (`BlendFunc::GetMinimalWeights`) |
  | | 1 (1) | the walked blend's approximation gives knots OCCT's spline surface refuses (`StoreData`) |
  | | 1 (1) | no start for one stripe, the others fail in the walk |

  Strips narrower than the import's tolerance (mitcad#299): in one spec
  the stored body has a band of 0.5 micrometres between two chamfers of a
  flange (and a ring of 0.5 micrometres under the lower one) where the
  design has one edge, and the roundings of the chamfers' edges end at the
  band's two vertices; their caps crossed and the corner failed. The file's
  body has no band: its two roundings meet in one mitre on the crease
  between the chamfers (two faces of 0.256 and 0.281 mm² at each of the 8
  corners, no corner patch). The dressup now, after its other tries, takes
  such strips at the selected edges' vertices (four edges, two of them at
  most 0.01 long, the other two within 0.01 of each other) as the crease
  they stand for: the faces beyond meet in the curve where their surfaces
  meet, each end's two vertices become one, and OCCT's own corner of two
  roundings at a vertex builds the file's mitre (faces of 0.256 and 0.281
  mm², 73 faces as the file, removed volume 1.8915 against the file's
  1.8962 mm³; the earlier probe that merged short edges only, keeping the
  band, built 0.295 mm² faces running past the crease and lacked the lower
  roundings). A step between parallel faces is no crease and is left as
  it is. Of the 249 measured the same way: MATCH 70 → 72 (this spec, and
  one whose chain ends at a face of zero width, two coincident half
  circles, which mitcad#239 listed among the invalid results), NEAR 46, OFF 1,
  failing 132 → 130; every other spec gives the same result line.

- **Edge names several edges fit** ("n edges fit the names"): 118 edges
  in 25 items. The recipe's tail after its type string, `i32 -1 | i32 -1
  | u32 n` then per vertex of the edge a record of entity indices
  separated by `-1`, appears to list the faces around each end of the
  edge in cyclic order (the edge's two faces and the end face, in an
  orientation that differs between otherwise equal recipes), followed by
  two words (`0 0`) or longer index lists; it would tell mirror-image
  edges apart, but the decoder's B-rep walk has no cyclic face order
  around vertices yet. Not decoded in this round.
- **Named edges the replay lacks**: of the settled fillets whose every
  edge the decoder found but whose answer is a guess, 85 have named edges
  that are not in the replay's bodies (no by-name definition); 21 more
  had the by-name definition rejected, the answer differing from the
  names both ways (the replay's bodies differ from the file's there).
- **One tangent chain at a time** (`by_chains`): building the
  definition takes no measurable time (0.4 s over 1547 items); its
  evaluation does. Over the 125 files with fillets that fell back or took
  a guess, it was evaluated 300 times for 72 s (5 % of those fillets'
  1488 s): after an approximate match of the edges at once 68 times
  (23 gave the state or came closer, 12 s), after a kernel failure 218
  times (6 gave the state, 57 s), after another state 14 times (1). No
  failure kind, chain count or whether the chains meet at a vertex
  separates the 6 from the 212, so it stays after every failure.

## The published format descriptions (mitcad#154)

Two public descriptions of the `.f3d` streams were compared with what
`core/f3d-decode/src/` and `core/f3d-design/src/` read: the cadmpeg
project's format description
and its offset tables (CC BY 4.0, <https://github.com/cadmpeg/cadmpeg>,
`docs/formats/f3d.md`, `docs/layouts/f3d.md`, `docs/formats/asm.md`) and
the ezf3d project's format notes (MIT licence, copyright its authors,
<https://github.com/AlexSabaka/ezf3d>, `docs/format/`). They were used as
information only: the layouts below are written in our own words, no
text or code was taken over, and nothing depends on either project. Each
decode taken over was checked on the reference models' dumps
(`MITCAD_F3D_MODELS`, `core/f3d/tests/design_models.rs`) and, where the
history settles the input, on the corpora (the learning dump, *Learning
the undecoded inputs*). Where a layout below was found by comparing the
reference models' records with their dumps and neither description gives
it, the table says so.

Status: **read** (and verified as the last column says), **read
differently** (we read the same bytes with another meaning or another
layout), **not read**. "Ref. models" means checked on every reference
model that has the input; "history" means the corpora's history settled
it ([*What the history settles*](f3d-history.md#what-the-history-settles)).

### From the descriptions to the decoder

| What the descriptions give (source) | Decoder | Notes |
|---|---|---|
| Container: ZIP with stored, deflated and zstd entries, `.f3z` packages (both) | read | `core/f3d-file/src/container.rs`; `.f3z` members as separate designs |
| Segment registry `Manifest.dat`, external references `RedirectionsStream.dat`, component reference slot (cadmpeg) | not read | the import names inserted documents' occurrences from the design stream only |
| `MetaStream.dat`: header, type table (GUID, base GUID, version, module, ids), primary and secondary record index, named ids, version contexts, property block (both) | read | the whole stream (`MetaStream::parse`); the secondary index is our sub-chunk table, the version contexts our external references, the property block our feature versions; the two `u32` after the module name, which we called a segment version, are a segment type code by cadmpeg (1 design) |
| Reference encoding: present flag, id, same/other segment, cross-document form with link name and version; the inline type GUID of one writer generation (cadmpeg) | read | `stream::ref_at`; the cross-document form as an assembly context (`Ref::context`) |
| Members written one class level at a time in name order; leading block and property block flags (cadmpeg) | read differently | we read the root part (references, attributes) and then fixed layouts per class and version found on the files; the name ordering explains why a member added in a later version shifts a layout, which matches the version-keyed layouts we keep |
| Property block keys (`EntityGenesis`, `pt_tag`, `crv_primary_id`, `crv_secondary_id`, operation id flags) (both) | read | the root part's attributes |
| Timeline record: context reference and ordered items; items without a scope; order = authored order (both) | read | the only non-empty timeline is the design's (mitcad#37 reads each component's list) |
| Parameter records: expression, role, unit, name, value in cm/rad, owner; the three header forms; owner frames and companions with time stamps (both) | read differently | we read every class version found (including the 2026 writers' version 8) by locating the strings; the owner frames and companions are not read, the holder object gives the owner (section 7 of the format study) |
| Sketch points (versions 0, 8, 10, 11), their companion with incident curves (cadmpeg) | read | coordinates and the curves that use each point |
| Sketch lines: start, displacement, direction, auxiliary normal (both) | read | **subclasses now** (mitcad#178): the subclass `16DEFC4D` keeps the line in its sub-chunk, `AE42BAB6` after a reference of its own; read at the base class's place before, 158 lines of 20 sketches came out as denormal numbers |
| Sketch arcs and circles (ezf3d: by their point count) | read | |
| Sketch NURBS curves: nullable carrier, nested header, degree, fit tolerance, counted knots, weights, control points (cadmpeg) | **read now** (mitcad#154) | `sketch::splines`; ref. models' fitted and control-point splines exact; fitted and control-point splines are not told apart (both come in as fixed splines) |
| Sketch ellipses and elliptical arcs (neither) | **read now** (mitcad#154) | `sketch::ellipses`, read on the reference model and the corpus' arcs; centre, unit major and minor directions, radii, angles |
| Sketch texts (`textex_tag`, `txt_tag`): font, height, width factor, colour, string, frame transform or path (cadmpeg) | not read | 21 texts in 7 corpus files; texts come in from external dumps only |
| Sketch relations: member runs, relation ordinals, constraint bits (cadmpeg) | read | constraint types by bit (format study); the ordinal meaning is new to us and not needed |
| Dimension frames (paired, counted, null-locus, annotated) and the rules that bind a parameter to geometry by measurement (cadmpeg) | read differently | we read each dimension object's entities and holder directly; the measurement rules are the description's way to recover what we read; a driven dimension has a null holder reference, read since mitcad#178 |
| Sketch container, member runs, placement matrix, visibility flag (cadmpeg) | read | the transform object; the light bulb (mitcad#6) |
| Profile regions: loops by endpoint incidence, containment, arrangement (both) | read differently | MitCad computes the regions itself; the decoder reads the selected loops (`_f3d_profile_loops`, mitcad#96) |
| Construction recipes (body, face, bounded face, edge, vertex), generic tag attributes on the B-rep, recipe resolution against the history (cadmpeg) | read | `recipe.rs`, `names.rs`, `inputs.rs`; vertex recipes not read |
| Parameter scopes: state identities, ordered reference table, family name and ordinal (cadmpeg) | read differently | our item tail (result number, base name, index, sub-items, state bytes) |
| Extrude: operation, direction (one side, two sides, symmetric), face-extend option, direction reversal, geometry kind, start support, profile normal, per-side extent kinds (both; ezf3d the first three) | read; **solid, per-side kinds, thin-wall block and the intersect code now** | the reversal is our flag (mitcad#96), the normal our stored vector; `_f3d.extrude.solid`, `start`, `sides` (mitcad#154): every corpus extrusion where read is a solid, sides agree with the parameters and slots; corpora have no "up to next"; `isSolid` checked on 243 ref. model extrusions (4 surfaces); `thin`, `thin_location`, `thin_holder` (mitcad#186: see [*Extrusion options from one-option variants*](f3d-learning.md#extrusion-options-from-one-option-variants-mitcad186)) |
| Extrude: groups by role (profile 0x41, existing bodies 0x04/0x08, termination 0x11/0x12, start face, shape target 0x05) (cadmpeg) | read differently | our input slots and their role numbers (65, 8, 5, 17, 18) are these roles' high words |
| Extrude profile selection by points and history transitions (cadmpeg) | read differently | we read the loops directly (mitcad#96) |
| Revolve: operation, extent kind, direction kind, profile and axis groups, angle scalars (cadmpeg) | read (operation, profile, sketch-line axis) | axes that are faces or edges stay history guesses |
| Fillet and chamfer sizes, fixed scalar lanes, full round, variable law, chamfer forms (cadmpeg) | **read now** except fixed lanes and full round | see the next table |
| Hole: point and direction carrier, support face selection, parameters (cadmpeg) | read (points, type, sizes) | the direction and the support face are not read; two corpus holes, both parametric |
| Thread: designation, size, profile, pitch, modelled flag (cadmpeg) | read | mitcad#35 |
| Loft: section groups, rails, centre line, operation (cadmpeg) | read | mitcad#34 |
| Sweep: profile, path, guide rail, guide surface group, operation (cadmpeg) | read; guide surfaces recognised | orientation, direction and scaling are not given by the descriptions either |
| Pipe: path group and four scalars, settings (cadmpeg) | read (path, operation, sizes) | section type and hollow are in the settings record, not read |
| Combine: operation, keep tools, target and tool pairs (cadmpeg) | read | mitcad#67 |
| Move, Scale: transform record, body group; scale factor and centre (cadmpeg) | read (move) | no corpus scale |
| Shell: thickness, outward flag, removed faces or target bodies (cadmpeg) | **faces, bodies, outside thickness now** | inside or outside is the parameter's role; faces checked on the ref. models; tangent chain not read |
| Offset faces: signed distance, face group (cadmpeg) | **faces now** | |
| Thicken, surface patch, extend, offset, stitch, trim, ruled, boundary fill, delete face (cadmpeg) | not read | not translated by the import |
| Draft: angle, drafted faces, neutral plane or parting tool (cadmpeg) | not read | one corpus draft |
| Split body, split face: tool and target groups (cadmpeg) | read (split body) | |
| Cylinder, sphere, box, torus primitives: operation, placement matrix, sizes (cadmpeg) | **read** (mitcad#25) | the matrix is in the frame of the plane or face the primitive stands on (that frame carries the plane's origin): `frame * placement`; two cylinders and both spheres of the corpus' one design come in, four cylinders on faces of an inserted component do not |
| Coil: operation, clockwise, extent kind, section, position, placement (cadmpeg) | **read** (mitcad#25) | type, section, position, rotation and placement; the corpus' coil (four copies) comes in parametric |
| Base feature: result bodies by entity suffix, state pair (cadmpeg) | read differently | the history's states stand in for base features |
| Component insert, local occurrences, copy-paste, derived instances, occurrence patterns (cadmpeg) | read | occurrence placements (mitcad#75, mitcad#81) |
| Work planes, axes, points: placement frames, solved axis, point data (cadmpeg) | read (planes: frame, definition and, since mitcad#178, the entities they are measured from; axes partly) | construction axes without decoded geometry are skipped; axes are fixed (their inputs are not read) |
| Joint origins, joints, as-built joints, assembly operand frames, alignment owners (cadmpeg) | read | mitcad#66; the description's alignment lanes agree with our owner order |
| Mirror and patterns: seed groups, counts, angle, axis record (origin and displacement), plane group (cadmpeg) | read | the axis record is our `_f3d_axis` (mitcad#96) |
| Appearance and material assets (`.protein` packages: page framing, instance records by schema, descriptions), body presentation records, per-face assignments, body map (cadmpeg) | **read** (mitcad#228) | [*Appearances and materials*](f3d-appearances.md#appearances-and-materials); every record of the corpora decodes to its end |
| Canvas, decal, mesh bodies, body visibility, unit systems, bounding box caches, display meshes (both) | not read (display meshes: read) | not needed by the timeline import |
| Sheet metal base flange, edge flange, hem (cadmpeg) | base and edge flanges read (mitcad#211); hems not | [*Sheet metal*](f3d-sheet-metal.md#sheet-metal): the edge flange's flip byte and height datum 3 are ours |
| ACT segment, browser segment (both) | not read | |

### Fillets and chamfers against the descriptions

The descriptions name a fillet's and a chamfer's sizes by the roles of
the parameters a set's holders hold (`Radius`, `StartRadius`,
`EndRadius`, `MidRadius` with `MidParams`, `ChordLen`, `EdgeOffset1`
with `EdgeOffset2`, `TangencyWeight`; `Distance`, `Distance 1`,
`Distance 2`, `Rotate Angle` and older spellings), sizes stored without
parameters as fixed values in the item, a full round as a face group, and
how edge recipes resolve against the history. They do not give the
options a fillet or chamfer stores beside its sizes; those were read on
the reference models (one model per option, against their dumps):

| Input | Where the file keeps it | Decoder before | Now |
|---|---|---|---|
| constant radius per set | `Radius` parameters | read | read |
| variable radius (start, end, mid radii and positions) | parameter roles | not read (the item fell back: no sets) | read, verified on the reference models |
| chord length | `ChordLen` | not read | read (no reference model, no corpus item) |
| asymmetric offsets | `EdgeOffset1`, `EdgeOffset2` | not read (fell back) | read, verified |
| which face takes which asymmetric offset | the set's options object (`4A557CE2`): flipped byte | not read (both tried) | read, verified (flipped and not) |
| G2 continuity | the set's options object: curvature byte | not read (built G1) | read, verified |
| tangency weight | `TangencyWeight` parameter, one per set | not given to the import | read, verified |
| tangent chain | the set's options object: tangent chain byte | not read (the import chained) | read, verified (on and off) |
| rolling-ball or setback corners | the fillet object: the byte after its set list | not read (built rolling) | read, verified |
| what a set selects (edges, faces, a face's edges) | the set input (mitcad#96) | read | read |
| full round, rule fillet | face groups (description) | not read | not read (no corpus item) |
| chamfer corners (chamfer, miter, blend) | the chamfer object: the `u32` after its root part | not read (built with chamfer corners) | read, verified |
| chamfer tangent chain and flip | the chamfer set object (`0D383DB2`): `u8 tangent chain`, `u8 flipped` after its root part | not read | read from the API reference dumps' 312 sets (the flipped byte agrees on all; the chain byte 324 of 324 with dumps and models); older class versions not read |
| options of the older set records (class version 9 and earlier) | another layout | not read | not read (no reference model) |

Over both private corpora every fillet set is a constant radius with G1
continuity and every chamfer has chamfer corners; 12 fillet sets have
their tangent chain off (the import chained them before), and the
options of 92 sets in older records are not read. Of the 249 dressups
classified in mitcad#107 and rebuilt in mitcad#121 (at its close: 63
MATCH, 41 NEAR, 1 OFF, 144 failing), every one is a constant-radius G1
fillet with rolling-ball corners and its tangent chain on, or an equal or
two-distance chamfer with chamfer corners, as the import built them (15
of them in records whose options are read only in part, none of the
corpus' options differing from the defaults there): their inputs are
complete and honoured, and what remains of them is the geometry
kernel's.

### Our other problems against the descriptions

| Problem (where it is described here) | Answered by the descriptions? | Done |
|---|---|---|
| Sketches left partial by curves not decoded (96 + 25 partial sketches; [*Corpus test*](../README.md#corpus-test)) | NURBS curves: yes (cadmpeg); ellipses: no | both read now: 125 sketches of the backup corpus and 25 of the main corpus had splines (`D82E012F`) or ellipses (`FF23079A`) |
| Extrusions whose regions are wrong ([*What the history settles*](f3d-history.md#what-the-history-settles)) | the descriptions infer the selection from points and the history as we did before mitcad#96; our stored loops are more direct | — |
| Left-out sketch constraints and dimensions ([*Corpus test*](../README.md#corpus-test)) | partly: relation bits and dimension frames | not done |
| One-sided cuts through all whose flag byte is 0 go either way (*What the file does not settle yet*) | no: the description's direction reversal is our flag and its profile normal our vector; nothing else | — |
| Two-sided extrusions up to objects (mitcad#104) | the termination groups in reference order, as we read them | — |
| Surface extrusions tried as solids | yes: the geometry kind | read now |
| Extents up to the next face | yes: per-side kind 3 | read now (none in the corpora) |
| Rectangular patterns: spacing or whole extent | the description takes the stored distance for the extent; our reference models (spacing) store the spacing: see *Where the descriptions and our reading disagree* | not done |
| Patterns and revolves about edges or faces | yes: the analytic carrier's axis in the history | not done (the history settles them) |
| Revolves about construction axes not imported (5 in the backup corpus) | yes: the solved axis carrier | read now: construction axes come in as fixed axes |
| A hole's direction and extent up to an object | direction yes (point-and-direction carrier), extent by parameters | not done (two corpus holes, both parametric) |
| Sweeps' orientation, direction and scaling; a pipe's section and hollow | no | — |
| Closed lofts, participant bodies of lofts and sweeps | no | — |
| Sketch texts | yes (`textex_tag`) | not done (21 texts) |
| Shells whose body or faces were not found (5 + 1) | yes (face group, outward flag) | read now |
| Offset faces whose faces were not decoded (2 + 2) | yes | read now |
| Cylinder and sphere primitives (6 + 2) | operation and sizes yes; the placement matrix does not place the corpus' primitives | placement read in the frame of the plane (mitcad#25): 4 of 8 come in |
| Coils (3) | yes (operation, handedness, extent kind, section) | read (mitcad#25) |
| Drafts (1), moved faces, embosses, sheet metal | drafts yes; sheet metal yes; embosses yes; the others no | embosses read (mitcad#458: both corpus items); drafts not done (MitCad's draft exists; one corpus item) |
| Unknown item classes (`?…`) | — | none in the corpora |

### Coverage before and after

Both private corpora with the release build before and after the change
(the same base, three files at a time, [*Coverage runs in parallel*](../README.md#coverage-runs-in-parallel);
items parametric / partial / fallback / skipped):

| Type | `MITCAD_F3D_CORPUS` before → after | backup corpus before → after (one design left out, below) |
|---|---|---|
| all items | 1066 / 61 / 237 / 270 → 1095 / 34 / 236 / 269 | 4798 / 269 / 1031 / 1149 → 4948 / 168 / 992 / 1139 |
| sketches | 335 / 46 → 363 / 18 | 1632 / 161 → 1740 / 53 |
| extrusions | 295 / 76 fallback → 295 / 76 (one gained, one lost) | 1354 / 325 → 1373 / 307 |
| fillets | 248 / 77 → 248 / 77 | 1084 / 312 → 1088 / 308 |
| revolutions | 20 / 7 → 21 / 6 | 81 / 20 → 87 / 14 |
| circular patterns | 14 / 13 → 14 / 13 | 80 / 50 → 84 / 46 |
| mirrors | 6 / 2 → 6 / 2 | 18 / 9 → 22 / 5 |
| moves | 6 / 0 → 6 / 0 | 11 / 8 → 15 / 6 |
| shells | 0 / 1 → 0 / 1 | 0 / 5 → 1 / 4 |
| construction axes | 1 skipped → 1 partial (fixed) | 7 skipped → 7 partial (fixed) |

Final bodies: `MITCAD_F3D_CORPUS` 529 exact and 6 within 0.1 % before and
after; backup corpus without the design below 3182 exact and 52 within
0.1 % after (3187 and 52 with it before). The sketches with splines and
ellipses account for most of the gain (and the items after them that now
replay instead of following a fallback, among them items that reached
the import's time limit before); two revolves of the backup corpus come
in about construction axes. Extrusions: 23 gained and 5 lost in the
backup corpus, 1 and 1 in the other. The lost ones come after items
that now replay differently: a boolean giving `BadOrientationOfSubshape`
on a profile with an elliptical arc (the same design in both corpora),
a cut of one design (stored twice) whose first reading leaves out a body
of its state and whose other reading OCCT fails ("Courbes non
jointives"), and a cut whose target no longer lies across the whole
profile. One revolution of the backup corpus no longer gives its state.

One design of the backup corpus (140 items) now ends the import process:
OCCT corrupts its heap (`free(): invalid next size`) in a chamfer's
second definition on bodies that a fallback before it brought in; the
items before it come in differently now that their sketches are
complete. It is left out of the backup numbers above; it is a kernel
crash, not decoding (open).

### Shells after the descriptions (mitcad#158)

With the removed faces read (above), the shells whose body was not found
came in as follows:

- the labelled example (a box hollowed from its top face, sharp, inside,
  wall 200 mm) is parametric and matches its history state;
- a removed face with a hole round its middle (a planar ring) was not
  found: the decoder gave the mean of the face's edge points as its
  `point_on_face`, which lies in the hole. A planar face's point is now
  the mean only where that lies in the face, else the point of the face
  farthest from its edges along lines across it
  (`mitcad_f3d::names::point_inside`);
- stored bodies (after a fallback) with only C0 continuous B-spline
  faces: the kernel's offset refused them; the shell now splits such
  faces at their creases first (`geometry/src/faceops.cpp`,
  `split_at_creases`);
- the rest fail in the kernel's offset, which cannot drop a face that
  vanishes at the wall's thickness: a 0.17 mm chamfer strip left between
  a 3 mm rounding and the bottom under a 0.8 mm wall (two designs), and a
  convex rounding of about 0.45 mm under a 2 mm wall (one design, stored
  in both corpora). The same shells come out at thinner walls (0.3 mm and
  0.4 mm). On two designs whose bodies before the shell are stored ones
  (after fallbacks), the offset raises an exception extending an edge of
  the stored B-spline roundings, or gives a shape whose edges fail the
  check (that design's state before the shell cannot be rebuilt, so it
  could not be confirmed either way). The shell's error now says which
  of these it met.

With the OCCT port's patch 0100 (mitcad#172) the kernel's offset drops
such faces and joins the faces beside them: the shells with vanishing
faces (two designs of one family, and the design stored in both corpora)
are parametric. Three of the four match their history states; the
other's state cannot be rebuilt (its input comes after a fallback).

### Where the descriptions and our reading disagree

- **Symmetric extrusions' side kinds.** The cadmpeg description gives
  `(3, 1, 0)` for a symmetric distance and `(3, 1, 1)` only for two
  class-keyed forms; every symmetric extrusion of the corpora whose sides
  are read stores `(3, 1, 1)`.
- **Two-sided extrusions with a side through all.** The description lists
  two-sided distances and two-sided faces only; the corpora have `(2, 1,
  4)`, `(2, 4, 1)` and `(2, 4, 4)`, which our input slots already read as
  a side through all.
- **Rectangular pattern distances.** The description takes the stored
  direction distances for the seed-to-last extent (spacing = extent / (n
  - 1)); the reference models' patterns, of the spacing type, store the
  spacing. The file's choice between the two is not read.
- **Primitive placement.** The description reads the cylinder's and
  sphere's matrix as their placement; it is placed in the frame of the
  plane or face the primitive stands on, whose own matrix carries the
  origin (mitcad#25).
- **The bytes after a spline's degree.** One paragraph of the cadmpeg
  description calls them a constant token, another a fit tolerance: they
  are an `f64` (1e-9 in the records looked at).
- **The two `u32` after the meta stream's module name**: a segment type
  code by the description; we had called them a segment version (they are
  read, not used).
- **ezf3d**: the sketch's placement is in the design stream (the sketch's
  transform object, and the cadmpeg description agrees), and an extrusion
  names its profile's sketch (its profile source input, mitcad#96); ezf3d
  finds neither. ezf3d gives a parameter to the feature before it by
  position; we follow the parameter's holder, which names it.
- **Scopes no timeline item lists.** The cadmpeg description calls them
  internal operation operands or history carriers whose state chain
  bridges listed scopes. The off-timeline extrusions of the corpora are
  real features: the parts of timeline items in other components, which
  the design's table of item parts names (mitcad#169, *Parts of items,
  face profiles and sketch heights*). Their records also hold the number
  of the operation before theirs in their component (seen on the parts
  looked at; not decoded), which may be the chain the description means.

### Removals, deleted faces, copies, splits and cut profiles (model by model)

Found by importing real designs one at a time; each rule is general.

- **Removals** (`RemoveFeature`): the item's body input is decoded
  (`itemToRemove`) and imported as `remove_body`. The history keeps the
  removed body as it was, so it cannot check the item against a state: the
  body is the stored body of the state the replay stands for that has its
  measures, and that stored body leaves the history from the replay's state
  on (`removal.rs`, as the tools a combine consumed do). A split's new piece
  is found by its index among the split's own new bodies, not among the
  bodies the split divides as well (`producers.rs`).
- **Deleted faces** (`DeleteFaceFeature`): the face inputs are decoded
  (`deletedFaces`) and imported as `delete_face`, one feature per body.
  Face recipes with a second entity list (faces and bounded faces; the first
  list names the face as it does for edges) now decode. A face whose point
  lies on its surface but off the replay's trimmed face is not found.
- **Copies and cut and paste of bodies** (`CopyPasteBodies`): the source
  bodies are decoded (`sourceBody`). A copy stays in its component: a
  `move` with `copy` and an identity transform. A cut and paste into
  another component is the Move to Component command with an existing
  destination (`pasting.rs`): the bodies and the features that made them go
  into the component the item is in, an existing one placed once like the
  source, and the bodies keep their identity and their place in the design.
  The item has no state of its own, so the check is that the replay's
  bodies are unchanged and the destination's afterwards. The item's other
  translation is a `body_transfer` in the destination (mitcad#465): the
  bodies alone move at the item's point of the timeline and the features
  that made them stay in their component, which is exactly what the item
  does in the file (both are parametric; neither takes the file's stored
  bodies). Move to Component comes first because it also gives the
  destination the bodies' history; where it is refused (a definition that
  cannot yet change coordinates between differently placed components,
  such as a hole; a feature of the source that works on other bodies too)
  the body transfer is the item's mapping. Its check is the same: the
  bodies are where they were in the design (their centres placed by their
  occurrences) and the destination's afterwards, and the note names why
  the features stay. A last command that also takes the bodies a feature
  ties to the first ones (a guess, more bodies than the item moves) is
  tried when both are refused. Bodies that went with an earlier cut (the features
  that made them also made the bodies of that cut) are in the destination
  already: the item is satisfied. A cut whose item has no body input (the
  file keeps the cached result of a reference that failed there) says so.
  A copy, a cut and a removal use no sketch of another component (the
  copies of the last sketch made for items that use none are gone).
- **Split tools**: a plane the file no longer has (the construction plane
  was deleted after the split used it) is stored with the entity input as an
  origin and two unit directions (`PLANE_FRAME`); it is decoded as the
  "reconstructed" plane of the API dumps (the two designs checked agree
  exactly). A tool selection may hold several tools (`splittingTools`): the
  split's `more_tools` divide the pieces in turn. A tool body (or the body of
  a tool face) of another component than the split bodies' is read there by
  the split's `tool_links`, as a combine's tools are (the paths to the two
  components, each placed once), and the split goes where its bodies are.
- **Cut profiles**: the regions the faces a profile selects cover leave out
  slivers (below a millionth of the face's area, left where the file's lines
  nearly meet); with the sketch region selected too, a sliver made the XOR
  of the regions an empty tool. The sketch's region tolerance is at least
  1e-6 mm: lines of a file that lie 4e-8 mm apart on each other no longer
  join the regions either side of them. A cut's pieces below a millionth of
  the largest piece's volume stay in its body as lumps, as the file keeps
  them: an extrusion's, a revolution's and a combine's cut alike.
- **Sketch regions at tangent contacts**: the bounds that decide which
  curves of a sketch can meet were those of the sampled points, so an arc's
  top fell short of the line that ends tangent on it and the contact was
  never tried (the lens between the line and the arc was no region, and the
  extrusion that selected it fell back). Round curves now bound their
  extremes.
- **Tangent distance** (`SketchTangentDistanceDimension`): the gap between
  a circle or arc and a line (the centre's distance from the line less the
  radius) is the model's `tangent_distance` dimension, solved with the
  sketch's other dimensions.
- **Reference curves that do not meet**: curves the file keeps linked to
  the model are locked in the sketch; where two of them share ends that lie
  off one of them (projected ellipses differ by micrometres) no solution
  keeps all of them, and the sketch did not solve. Each level of the
  sketch's import is now tried with those curves locked, then free (their
  ends settle where the curves meet), before it gives up constraints.
- **Tangent handles**: a fitted spline's fit points are held at the middle
  of their tangent handles by midpoint constraints whose curve is a line the
  sketch does not list. The spline's curve is kept as the file has it, so
  these constraints bind nothing and no longer count as left out.
- **Offsets with round corners**: an offset of a chain whose offset curves
  include arcs of the offset distance about a vertex of the chain (the
  corners the file rounds where the chain turns away from the offset side)
  has no copy of each curve alone: MitCad's exact offset of lines and arcs
  meets its curves at the corners. It is imported as the derived offset
  (computed from the source chain, as an offset of ellipses and splines is):
  the chain in the file's order, each curve's offset, and the corner arcs
  by the join they close (the offsets of the chain's curves `join` and
  `join + 1`). Nothing else can constrain the computed curves. MitCad
  rounds a corner where the chain turns away from the offset side and trims
  it where the chain turns toward it; where the file does otherwise (in the
  two designs seen, one corner is extended to meet the next offset curve
  while another is rounded), the model refuses the offset and it is left
  out as before, the curves staying as the file has them, after the
  sketch's levels with it (the sketch is partial).
- **Midpoint of two lines**: a midpoint constraint that names two curves
  (the API gives its `midPointCurve` only: the other is no point) puts the
  middle of the first line on the second; a point at that middle holds the
  two constraints.
- **Curves drawn twice**: two curves that lie on each other along a stretch
  (the same edge projected twice) were cut at every sampled piece of it, so
  that the pieces of the two curves did not pair and left slivers that kept
  the regions either side of the curves from being separate. Their
  intersection now stops at the ends of the stretch, where the arrangement
  keeps one of the two. A revolved cut of a sketch with several such edges
  selected too few profiles before.
- **Splines the decoder calls periodic**: a spline with an unclamped knot
  vector is flagged periodic; one whose control polygon does not wrap is an
  open B-spline and imports as one (the projected arcs of a cylinder came
  with such knot vectors).
- **Not translated** (causes found): extrusion profiles that are sketch
  text (the sketch's texts are not decoded); a tapered cut whose tool lies
  away from the material (the replay's tool does not reach the body); cuts
  whose boolean the geometry kernel reports as invalid; a cut across
  components whose parts differ in two of seven bodies; primitives' placement
  (the record's entity inputs are not decoded).

