# The polygonal hidden-line removal's hiding pass: 1810

mitcad#2021 (a follow-up of mitcad#1997): a drawing's base view of a steel
frame of 884 bodies (39 966 faces, 105 871 edges), on the quick path
(the polygonal hidden-line removal of a triangulation).
Back to the [port's README](../README.md).

## Overview

- **1810** hides the segments of OCCT's polygonal hidden-line removal in
  blocks beside each other on the threads of the default pool, and tests
  each segment only against the hiding faces near it on a grid over the
  view, instead of every shell and each face of every shell whose box
  meets it. The lines are the same, to the last bit, in the same order.
  It changes no result.

## The case

`HLRBRep_PolyHLRToShape::Update` hides every segment (a piece of an edge
or an outline, `HLRAlgo_BiPoint`) of every shell, one after another, on
one thread, also when a view asks only for its visible lines.
`HLRAlgo_PolyAlgo::Hide` tests the segment's packed box against the box of
every shell, and for each shell whose box meets it, every hiding face of
that shell (`HLRAlgo_PolyData::HideByPolyData`: the face's box, then each
of its triangles). The boxes are those of the 1020 × 1020 × 508
quantization of the whole view, so on a 6 m frame a cell is about 6 mm,
and the frame's long members meet many shells. The work grows with the
segments times the shells, plus the segments times the faces of the
shells they meet.

The reported view, with `MITCAD_DRAWING_TRACE` (which now times the pass
on its own, `hide`), spent 7.3 to 8.5 s in the pass of a projection of
about 13 s (placed at 1:50 and at 1:20), with the machine loaded by
another check.

## 1810: the hiding pass beside each other, through a grid

### 1810-hlr-polygonal-hiding-pass-in-parallel-through-a-grid-of-the-hiding-faces

- `HLRAlgo_PolyAlgo::Update` keeps the segments in the order of
  `InitHide` and `NextHide` (`NbSegments`) and a grid of 128 × 128 cells
  (8 × 8 steps of the quantization in x and y) of the hiding faces: each
  face is listed in the cells its box covers, from one step below its own
  in y (the packed box test passes a segment whose y reaches one step
  below the face's where the segment's depth reaches the face's: the
  depth's field carries into y's). A box outside the quantization (a face
  without hiding triangles) covers every cell. Each cell lists its faces
  in the order of the faces: shell by shell, a shell's hiding faces in
  order.
- `HLRAlgo_PolyAlgo::HideSegment(theSegment, theWork, …) const` is `Hide`
  for one segment, with the work data of the calling thread
  (`HidingWork`: the triangle scratch and the candidate faces). The
  candidates are the faces listed in the cells the segment's box covers,
  each taken in the first cell both boxes share, sorted in the order of
  the faces; each is tested as `Hide` tested it: its shell's box, then
  `HideByPolyData` with the same arguments. A face whose box test passes
  is always a candidate, and the faces hide the segment in the order in
  which the walk over the shells took them, so the edge status is
  `Hide`'s. It writes nothing of the algorithm. `Hide` is now
  `HideSegment` of the current segment.
- `HLRBRep_PolyAlgo::NbSegments` and `HideSegment`: the same, with the
  edge or face of the segment.
- `HLRBRep_PolyHLRToShape::Update` hides the segments in blocks of 64 on
  the default pool's threads (`OSD_ThreadPool::Launcher`, one
  `HidingWork` per thread), each block's visible and hidden lines in
  lists of its own, joined in the order of the blocks: the lists, and
  every compound made from them, are those of one segment after another.
- No catch is added; an exception in a block reaches the caller as the
  pool passes it on.

What is left of the projection is serial: `HLRBRep_PolyAlgo::Update`
(the triangles stored and boxed, about 1.3 s on the reported view) and
the compounds asked for each body (`InternalCompound`, about 0.45 s).

### The DRAW reproduction

A frame of 12 × 8 bays: posts, beams at two heights, tubes along the
beams, a brace through each bay and a ball on each post, 970 bodies,
meshed, its polygonal hidden lines (visible and hidden) computed along a
view almost along its rows:

```tcl
pload MODELING VISUALIZATION
set parts {}
set n 0
set bay 600.
set brace [expr hypot($bay, 1800.)]
set slope [expr atan2(1800., $bay) * 180. / acos(-1.)]
for {set i 0} {$i <= 12} {incr i} {
  for {set j 0} {$j <= 8} {incr j} {
    set x [expr $i * $bay]
    set y [expr $j * $bay]
    box b$n [expr $x - 40] [expr $y - 40] 0 80 80 2000; lappend parts b$n; incr n
    psphere b$n 60; ttranslate b$n $x $y 2060; lappend parts b$n; incr n
    foreach z {900 1900} {
      if {$i < 12} {
        box b$n [expr $x + 40] [expr $y - 30] $z [expr $bay - 80] 60 100; lappend parts b$n; incr n
        pcylinder b$n 18 [expr $bay - 80]; trotate b$n 0 0 0 0 1 0 90
        ttranslate b$n [expr $x + 40] [expr $y - 60] [expr $z + 50]; lappend parts b$n; incr n
      }
      if {$j < 8} {
        box b$n [expr $x - 30] [expr $y + 40] $z 60 [expr $bay - 80] 100; lappend parts b$n; incr n
      }
    }
    if {$i < 12 && $j < 8} {
      box b$n 0 -20 -20 $brace 40 40; trotate b$n 0 0 0 0 -1 0 $slope
      ttranslate b$n [expr $x + 40] [expr $y + 0.5 * $bay] 100; lappend parts b$n; incr n
    }
  }
}
eval compound $parts frame
incmesh frame 0.1 -a 11.46
dchrono t restart
vcomputehlr frame lines 0 0 0 1 -0.05 0.02 0 0 1 -hidden
dchrono t stop
puts [dchrono t show]
puts [nbshapes lines]
puts [lprops lines]
```

`vcomputehlr` (the update, the hiding pass and the compounds) took 0.67 s
with stock OCCT and 0.29 s with 1810 (32 threads), both giving 62 915
edges of a total length of 2.46765e+06.

## Reports and tests

- `geometry.port.1810` (`geometry/tests/port/p1810-polygonal-hiding-in-parallel.cpp`):
  the frame of the reproduction meshed body by body, every kind of
  visible and hidden line, of all the bodies and of each, equal to the
  last bit and in the same order to the lines of the walk over every
  shell, written out with the algorithm's public parts (stock OCCT's
  `Hide`); the pass in less than half the walk's time (0.04 s against
  0.45 s; with stock OCCT the pass is the walk and fails that bound); one
  body, and a view of none.
- `geometry.drawing`: MitCad's views, the quick path included, as before.
- OCCT's own HLR cases (DRAW: the `hlr` group, exact and polygonal, and
  the 27 cases of `bugs` that compute hidden lines; 197 cases, of which
  42 have their data files here): the same status and the same printed
  values with 1810 as without.
- The reported view (`mitcad-reported/github-425` in the test files, never
  committed), placed at 1:50 and its scale set to 1:20 through the
  headless MCP server with `MITCAD_DRAWING_TRACE` (one run each, the
  machine loaded by another check, a load of 7 to 14 before and 12 to 16
  after): the pass 7.35 s and 8.41 s before, 0.46 s and 0.55 s with 1810;
  the pass and the compounds together 7.79 s and 8.86 s before, 0.93 s
  and 1.07 s with it (8.3 times faster). The `drawing` query's sheets
  before and after are the same byte for byte.
