# A rounding whose contact line folds, walked in pieces: 2228

mitcad#2228: a reported wave-shaped band whose top edge rounds with 2.5 mm
but not with 3. Back to the [port's README](../README.md).

## Overview

- **2228** lets the walk of a rounding step across a cusp where a contact
  turns back (its line folding over itself on a face, the edge bending
  towards that face more tightly than the ball touches it from the edge),
  and splits the guides of a constant rounding around each such fold, so
  that the walk across it is a twisted piece of its own between untwisted
  ones, which OCCT's reprocessing of twisted pieces then fills between
  them. Before, the walk stopped at the first cusp and the stripe failed.

## The case

A band 24 wide and 10 high between two vertical walls along two waves
(amplitude 8, wavelength 30, two waves along x from -30 to 30): its walls
bend at the crests with a radius of 2.77 mm in the reported model (2.85
for an exact sine). A ball of radius r between the top face and a wall
touches the top r from the edge (r tan(a / 2) for faces at an angle a
between their normals). Where the edge bends towards the top face with a
radius under that (each crest where the material lies inside the bend),
the contact line on the top, the edge moved r into the face, runs
backwards between two cusps and crosses itself in a swallowtail. The top
edge along a wave rounds with 2.5 (below 2.77) and fails with 3, 5 and
8.5, which another program builds; its result splits the rounding into
several faces with seams at the crests.

The ball's sections exist all along the edge. But
`BRepBlend_Walking::CheckDeflection` takes a step whose chord on a face
points against the contact's tangent at the point before as a step back
of the walk (`Blend_Backward`, then too large): at a cusp the contact
turns back, so every step across it is refused and the walk stops there
(`Failure_Walk`, one piece computed: the first probe, below). Past the
cusp the walk would be twisted on that face
(`BlendFunc_ConstRad::TwistOnS1`: the contact's tangent against the
guide's direction). `ChFi3d_Builder::PerformSetOfKGen` reprocesses a
twisted piece that has untwisted neighbours on the same faces: it cuts
them where their contact lines cross (`ChFi3d_IntTraces`, the corner where
the swallowtail is cut away), closes each along a line on its own blend
from that corner to its other contact, and fills the piece between them
with a `GeomFill_ConstrainedFilling` (three sides: the two lines and the
contact line on the other face). A rounding along one edge is one walk,
one piece, which would have no neighbour: "adjustment by reprocessing the
non-written points: no neighbor", `ChFiDS_TwistedSurface`.

## 2228: across the cusps, and the guide split around each fold

### 2228-chfi3d-rounding-whose-contact-line-folds-walked-in-pieces-the-twisted-one-filled-between-its-neighbours

- `BRepBlend_Walking::CheckDeflection`: where a contact's tangent points
  along the guide at the point before and against it at the new point
  (or the other way) and the contact moved no farther than the other
  contact, it turned back at a cusp between them: the step is not judged
  by that contact (`Blend_OK` for it); the other contact's check stands.
  Only where the other contact keeps moving (its speed at both points at
  least a twentieth of the guide's): where the ball itself nearly stands
  still a contact turning back is the walk going over to another branch.
  Only a short step (at most 5 % of the walk's largest step, at least 2
  `tolgui`) is taken so; a longer one is too large and halved, as the
  sections between the walk's points are solved again from a cubic
  through the contacts and their tangents (1870), which follows a cusp
  only over a short step.
- `BRepBlend_AppFuncRoot::SearchPoint`: where the solver does not
  converge from the linear start between two of the walk's points, it
  starts again from each of those points (sections themselves), the
  nearer first; only where neither converges is the section not found.
  A section (four variables) lying farther from the linear start than
  twice the two points lie apart is solved again from each of them and
  the solution nearest the linear start kept: where the contact line
  folds over itself the ball's centre may also lie where a line crosses
  the contact line's other branches, millimetres apart for a large
  radius, and the solver landed there.
- `ChFi3d_Builder::CompleteData` (two faces), for a twisted piece: the
  approximation takes the sections' values alone (`GeomAbs_C0`), not
  their derivatives along the guide, which near a ball that nearly stands
  still come through a near-singular Jacobian and are no measure of the
  smooth sections; the filling takes only the piece's contact curves and
  ends, which the values give within the tolerance.
- `ChFi3d_Builder::CompleteData` (two faces), for a twisted piece: its
  contacts' curves on the faces are interpolated through the walk's own
  contacts (taken before the approximation adds its sections to the
  line), at the walk's parameters; the filling takes the untwisted
  side's as its side on that face.
- `ChFi3d_Builder::StoreData`, for a twisted piece (whose surface the
  filling replaces): the normals that orient it count as vanishing only
  below `Precision::Confusion()`, not below the tolerance its
  approximation reached (a cross product, an area, against a length).
- `ChFi3d_Builder::Compute`: with the walk traced, the exception that
  fails a stripe's computation (its type, message and the spine's error
  status) is said on the standard error, and so is each section the
  approximation does not find (`SearchPoint`) and each walked line whose
  approximation is not done (`CompleteData`).
- `BlendFunc_ConstRad::SetSectionOnContact` (and the inverse function's):
  where the contact line on one face folds and the one on the other face
  does not, the guide's normal plane holds that other contact instead of
  the contacts' mid-point. The mid-point's line folds too where the fold
  ratio passes 2, and there its condition holds two balls that meet: the
  walk went over to the wrong one. `ChFi3d_FoldCutsOf` notes the faces,
  `ChFi3d_FilBuilder` sets the condition for the walk across the fold
  alone.
- `BRepBlend_Walking::InternalPerform`: once the walk is twisted, its
  steps are at most 5 % of its largest step, so that it stays on its own
  branch of the equations where the contact line folds back near it.
- `BRepBlend_Walking::TestArret`: a step in which a contact moves more
  than fifty times the guide's own advance is too large and halved,
  whatever the step's length: the walk has gone over to another solution
  of the equations, a ball touching the face elsewhere.
- `ChFi3d_Builder::PerformSetOfKPart`, after its guides are made, for a
  fillet of constant radius along an open spine (not a chamfer with
  penetration): `ChFi3d_FoldsOfContact` finds, on each edge and each of
  its two faces, the stretches where the edge's curvature towards the
  face's inside times r tan(a / 2) exceeds 1 (sampled about every
  twentieth of the radius, at least 64 and at most 4000 points, the
  borders found by halving); `ChFi3d_FoldCuts` puts a cut a tenth of each
  stretch's length (at least 10 tolesp) before and after it, stretches
  whose margins meet being one and one reaching within its margin of the
  spine's ends left out (it would have no neighbour there);
  `ChFi3d_SplitGuidesAtFolds` splits the guides at the cuts (the first
  piece keeps the guide's previous particular piece, the last its next).
- `ChFi3d_Builder::PerformSetOfSurfOnElSpine`: a guide's end at such a
  cut is not extended by the 1 % that helps the solver: the walk ends
  there, where the next guide's piece starts.
- `ChFi3d_Builder::PerformSetOfKGen`: a guide that starts at such a cut
  where the guide before it ends starts from the last piece walked (its
  `Previous`), as a guide after a particular piece does, also when the
  stripe is walked again. Without one, `PerformSetOfSurfOnElSpine`'s
  `RemoveSD` of two null ends removed every piece walked before it.
- With `MITCAD_TRACE_FILLET_WALK` set (2141), on the standard error:
  each edge's largest fold ratio on each face, each folding stretch, each
  fold not split off (it reaches the spine's end), each split and each
  contact turning back at a cusp.

Nothing changes for a rounding whose contacts do not turn back.

## Reports and tests

- First probe (2026-11-22, the guide split alone): 2.5 built; 3, 5 and
  8.5 failed with `Failure_Walk` and one piece computed, as before: the
  walk stopped at the first cusp, so the split pieces were never walked.
  The step across the cusp was added then. The band's volume read 15382.3
  by OCCT's default rule with an accuracy (`VolumeProperties` with 1e-9),
  not 14400: the tests now measure with OCCT's Gauss-Kronrod integration
  (the port test) and MitCad's own (the geometry test).
- Second probe (2026-11-22, with the step across a cusp): the folds are
  found (at 3 mm on the top face from w 10.82 to 11.89 and 56.25 to
  57.32, the guide split at 10.71, 12.00, 56.14 and 57.43) and the walk
  crosses a cusp (`contact 2 turns back between w 10.37 and 10.90`). But
  the first guide's walk ran on 1 % of its length past its end (to 11.12,
  into the fold: `PerformSetOfSurfOnElSpine` extends every open guide by
  1 % at both ends), and the folded piece, starting from that piece's end
  instead of its own first parameter, found no path ("compute data
  fails: no path", `Failure_Walk`). The guide's ends at the cuts are no
  longer extended since.
- Third probe (2026-11-22, the guides' ends at the cuts not extended):
  3 and 5 mm build on the band and on the reported model, one valid
  solid of 11 faces (the rounding in pieces at the folds), the volume
  taken within the corner's section along the edge (3 mm: 163.968 against
  163.970; 5 mm: 491.50 against 487.43); both top edges at 3 and 5 build
  (16 faces). 8.5 failed: the folded piece walked (7.59 to 15.12, both
  cusps crossed, in steps of 0.38), then the stripe raised an exception
  before the piece was stored (`Failure_Stripe`, error status 1). Since,
  a step across a cusp is short (above) and the exception is traced.
- Fourth probe (2026-11-22, short steps across the cusps): 2.5, 3 and 5
  as before; at 8.5 the folded piece walked in steps of 0.0099 across its
  cusps (44 points), then the stripe raised "PerformSurf : Failed
  approximation!": `CompleteData`'s approximation of the walked line was
  not done (its only other refusal, `StoreData`'s orientation, needs a
  degenerate normal). The approximation fails where a section between the
  walk's points is not found; since, `SearchPoint` starts again from the
  walk's points, and both places are traced.
- Fifth probe (2026-11-22, sections retried from the walk's points): no
  section missing and no approximation not done, yet "Failed
  approximation!" at 8.5: `CompleteData` returned `StoreData`'s refusal,
  its only one being the orientation (both normals above the tolerance
  the approximation reached somewhere between the piece's start and its
  middle). The twisted piece's normals are now measured against the
  confusion, and the tolerance its approximation reached and any refused
  orientation are traced.
- Sixth probe (2026-11-22, the twisted piece's orientation by the
  confusion): 8.5 "done" but invalid, the volume -27926937. The folded
  pieces' approximations reached 679 and 685 mm (at 2.9, 3 and 5 within
  1e-5): sections between the walk's points solved on other branches of
  the equations (the ball's centre where a line in the guide's normal
  plane crosses the folded contact line's other branches). Since, such a
  section is solved again from the walk's points and traced.
- Seventh probe (2026-11-22, sections far from the walk solved again):
  unchanged, the re-solve never fired (no section lay far from its
  linear start), the approximations again 679 and 685 mm. The
  guide-plane walk (1760) does not apply (the wall is an extrusion), nor
  1412's still ball (no tubes). The twisted pieces' walked sections
  (arc, centre, axis, jumps) and their surfaces' distances from the
  walked contacts are traced since, to find where the approximation
  leaves the walk.
- Eighth probe (2026-11-22, diagnostics): the folded pieces' 44 walked
  sections are smooth (each arc pi / 2, no jump of the centre or axis),
  but their approximated surfaces leave the walk only from w 12.85 to
  13.40 (up to 571 mm; 3 x 65006 poles), where the wall contact keeps
  still (at (-21.017, 19.617, 1.5) from w 12.853 to 13.0) while the guide
  runs on: the ball nearly stands still. The values there are right; the
  derivatives the C1/C2 approximation also fits are not. Since, a twisted
  piece is approximated C0.
- Ninth probe (2026-11-22, twisted pieces approximated C0): at 8.5 the
  folded pieces' approximations reach 0.0175 (their surfaces within
  0.0039 of the walk), the stripe is done, the volume 12953.02 (1446.98
  taken against the corner's section along the edge, 1408.66), 11 faces,
  but OCCT's checker refuses the body. The re-solve of sections far from
  the walk fired some 2600 times at microscopic distances (the
  approximation's points 1e-5 apart round the ball that nearly stands
  still) without changing anything: its floor is 0.01 since. The probe
  prints the result's faces and the checker's report.
- Tenth probe (2026-11-22, the checker's report): at 8.5 the wave wall
  (the face the folded contact runs round) is refused: its wire crosses
  itself (`BRepCheck_SelfIntersectingWire`) and the face is not
  orientable; the wall and the two crest fillings carry edge tolerances
  up to 0.0498 (at 5 at most 1e-4). Its top boundary is the walked wall
  contact: the pieces' and, across each crest, the folded piece's, which
  nearly stands still there and was approximated within 0.0175. The probe
  names the crossing edges and their meeting point since, and the
  sections' trace gives the contacts.
- Eleventh probe (2026-11-22, the crossing edges): the wall's wire
  crosses itself in one edge, the first crest's filling's side on the
  wall (from (-25.72, 18.25, 1.5) to (-19.28, 18.25, 1.5), tolerance
  0.0498), 34.8 long where the walked wall contact runs straight in the
  wall's parameters (u 7.59 to 15.12 at v -1.5) and 7.5 in space. The
  approximation's sections between the walk's points had the wall contact
  kept still on another branch (w 12.85 to 13.0); the curve through them
  looped. The twisted piece's contact curves are the walk's since.
- Twelfth probe (2026-11-22, contact curves through the walk): the band
  builds at 8.5, valid, 11 faces, 1447.19 taken (`geometry.port.2228`
  and `geometry.fillet_wave_crests` pass). The reported model
  (`--probe-reported`) fails at 8.5: at w 12.80 the ball nearly stands
  still, the wall contact turned back under the cusp step and then the
  top contact, and the walk ran back along another branch to the edge's
  start (its wall contact at (-30, 12, 1.5) at w 13.09, on the start
  face's edge): "PerformSetOfSurfOnElSpine : Chaining is impossible." The
  cusp step is taken since only where the other contact keeps moving.
- Thirteenth probe (2026-11-22, the cusp step only where the other
  contact moves): the reported model's walk unchanged. Its sections: at
  w 12.8015 the wall contact at (-21.02, 19.62, 1.5) (its parameter u
  12.887, the walk's own solution: u about w), at w 12.8022, 0.0007 on,
  at (-27.64, 15.79, 1.5) (u 4.46, the crest's other flank), the centre
  4.4 mm away: the walk jumped to another ball and ran back along it.
  The step checks let that pass (the contacts nearly standing still, the
  deflection's test off for such short steps). A contact's jump against
  the guide's advance is refused since.
- Fourteenth probe (2026-11-22, the jump guard): the reported model
  builds with 2.5, 3 and 5 (7, 11 and 11 faces, valid); with 8.5 the
  jump no longer passes, the stripe fails in the walk ("CallPerformSurf
  : Path failed!"). The folded piece's walk followed the wall contact at
  u about w (within a few thousandths) to w 12.632, then a step of 0.168
  to w 12.800 landed on another branch (wall contact u 12.8865) that
  turns back at w 12.8015; there the only solutions nearby were the
  crest's other flank, refused by the jump guard, and the walk stopped.
  A twisted walk's steps are kept short since.
- Fifteenth probe (2026-11-22, short twisted steps): the reported model
  with 8.5 walked the folded piece to its end in 4582 short steps and
  approximated it, then the stripe failed ("Standard_DomainError"); the
  band with 8.5, which built before, failed too. The walk's wall contact
  followed the guide (u about w) to w 9.70, then fell behind and turned
  back at w 9.81, and by w 13.09 lay at the edge's start (u 0): the walk
  had gone over to another ball. The guide's plane holds the contacts'
  mid-point, 4.25 from the edge with 8.5; with a fold ratio of up to 3
  on the top face, the mid-point's line folds where the ratio passes 2
  (about w 9.8 and 12.9), and at those points the mid-point's condition
  holds two balls that meet (the equations singular). The long steps
  before had jumped over them by chance. The wall contact, the edge moved
  straight down, never folds: its condition picks one ball all along.
- Sixteenth probe (2026-11-22, the contact that does not fold holds the
  sections): the reported model builds at 2.5, 3, 5 and 8.5 (volumes
  14278.15, 14224.53, 13908.61, 12953.23; 7, 11, 11 and 11 faces; valid),
  the band too. The `.f3d` design's fillet is 8.7 mm (its parameter d2);
  the import's replay builds it, but takes 1516.50 mm³ where the file's
  result takes 1530.88 (14.39 mm³, 0.11 % of the body, less), its new
  faces up to 0.082 mm off the file's (half their area beyond the import's
  0.01 mm) and the file's up to 0.091 mm off it: the import keeps the
  stored body ("its result is not in the file's history"). The likely
  cause then: across each crest PerformSetOfKGen fills the twisted stretch
  smoothly between its neighbours (cut where their contact lines cross),
  where the rolling ball's own envelope folds over itself and the file's
  rounding has seams (its faces meeting along a crease, the swallowtail
  cut away). `cli.wavy_fillet_reported` names that difference (volume
  within 0.2 %, faces within 0.1 mm either way). The stored body, read
  since (mitcad#2316, below), has no crease: its crest faces are lofts
  between the neighbours' sections through the corner.
- Bundle check (2026-11-23): the rules of the walk across a fold, and
  the section on the contact that does not fold, had applied to every
  walk (the cusp step, the jump guard, the short twisted steps, the
  sections searched again, the twisted pieces' storing) or to every walk
  of a spine with a fold: roundings that built before failed (a three
  faces' corner, an acute turn, a pierced face, a ball standing still,
  a design's fillet). Since, only the walk whose guide runs across a
  stretch where its contact line folds takes them
  (`ChFi3d_FoldWalkOf`, `BRepBlend_SetFoldWalk`), and a contact line
  folds only where the ball's contact lies on its face.
- OCCT lab (2026-11-23): round 18 still broke two roundings that build
  without the patch: an acute concave turn (1.5) and a three faces'
  corner (3). Traced, the corner's short concave arcs (radii 0.4 and
  1.25) bend the chain's edges more tightly than the ball of 3 touches
  the rod (a fold ratio up to 1.32), but the ball leaves the arc's face
  before the contact on the rod would turn back: the split guides broke
  a walk OCCT completes. No estimate from the faces' tangent planes told
  that apart reliably on those small cylinders. Since, every stripe is
  computed first as before, and only a stripe that fails along an edge
  whose contact line folds is computed again with the folds split and
  the fold walk's rules (`ChFi3d_ContactLineFolds`,
  `ChFi3d_SetFoldStripes`). The lab's 30 base cases are unchanged
  against the chain without the patch; its case `2228-wave` (the
  reported model's upper top wave edge) builds valid with 2.5, 3, 5, 8.5
  and 8.7 (7, 11, 11, 11 and 11 faces), none above 2.5 without it.
- The `.f3d` design's stored body (mitcad#2316, 2026-11-23; the lower top
  edge, the one the design rounds, crests at x -7.5 and 22.5): its
  rounding is five faces, as the patch's: three rolling-ball faces and,
  across each crest, one lofted face. Each crest face is three-sided:
  its apex is the corner where the top contact lines cross (y -7.591 at
  the crest's x), its sides on the neighbours are their sections through
  that corner (both arcs of the ball of 8.7 centred 8.7 below the corner,
  which touches the wall twice, at the crest's x -7.471 and +7.471, z
  1.3), and its side on the wall is the wall contact between those two
  points, round the crest. The faces meet tangentially along those arcs:
  no crease. The surface is a loft between the two arcs, tangent to the
  neighbours. The patch's filling ends elsewhere: PerformSetOfKGen cuts
  the neighbours on the top at the corner but on the wall at their guides'
  ends (the fold cuts, the crest's x -3.263 and +3.263), joined by a
  straight line in the neighbour's parameters, so the filling meets the
  wall only between the fold cuts. A lab round cut the neighbours on the
  wall at the same sections and gave the filling the wall contact between
  them (the neighbours' contacts and the twisted piece's joined; only for
  a stripe computed with the folds): the result's vertices are then the
  stored body's (wall points at -14.971, -0.029, 15.029 and 29.968; the
  same 11 faces, valid). But `GeomFill_ConstrainedFilling` over those
  wider sides keeps less than the file's loft: up to 0.247 mm inside it
  (on the crest's flanks, where the neighbours' own surfaces are close to
  the loft), the rounding taking 1575.3 mm³ against the file's 1530.9.
  Without the round, the result lies up to 0.128 mm off the loft (at the
  crest's middle) and takes 1517.2 mm³ (the band of the reported STEP
  model at 8.7, volumes from a mesh of 0.002 mm deflection, which gives
  the band's 14400 exactly; the loft sampled from the file's stored
  approximation). Neither is within the import's 0.01 mm, so the round is
  not in the patch. OCCT's blend code has no loft between two sections
  tangent to surfaces; the twisted piece's filling is its only means
  there. Building the file's loft would be a new surface construction,
  left to the owner's decision. `cli.wavy_fillet_reported` keeps its
  named difference.
- Probe: `test_fillet_wave_crests --probe` prints, for 2.5, 2.9, 3, 5 and
  8.5, what OCCT's own fillet of the band's upper top edge gives (done,
  failure, each contour's status and computed surfaces, the volume) with
  the walk traced (the folds found, the guide's splits, each piece walked,
  each cusp crossed).
- Tests: `geometry.port.2228` (the band made with OCCT alone, rounded
  with OCCT's fillet at 2.5, 3, 5 and 8.5),
  `geometry.fillet_wave_crests` (through MitCad's fillet, one edge and
  both top edges) and `cli.wavy_fillet_reported` (the reported STEP model
  at 2.5, 3, 5, 8.5 and 8.7, and the other program's `.f3d` result, 8.7
  mm, against MitCad's replay of its fillet, within the difference named
  above).
