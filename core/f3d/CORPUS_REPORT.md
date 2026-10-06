# Corpus checks of .f3d bodies

The corpus is a set of real `.f3d`/`.f3z` files (ASM 224.4 to 232.3,
Windows BinaryFile4 and macOS BinaryFile8 writers). It lives outside the
repository (`MITCAD_F3D_CORPUS`, default `~/f3d-corpus`). Reports name
files by their position in sorted path order (f01, f02, ...), not by file
name; `-v` adds the names.

## How to reproduce

```bash
# Container, parsing and conversion (Rust only), totals over all files:
cargo run --release -p mitcad-f3d --bin mitcad-f3d-inspect -- coverage <files...>
# Per file: segments, blobs, ASM headers; or per body:
mitcad-f3d-inspect info <file>
mitcad-f3d-inspect bodies <file> -v
# Display meshes saved with the document, per body:
mitcad-f3d-inspect meshes <file>
# ASM history of the .smbh blobs, bodies rolled back state by state:
mitcad-f3d-inspect history <file>
# OCCT build of every body (use an optimised build: about 70 s for the
# corpus, 8-9 min in the debug preset); --history adds the rolled-back
# .smbh bodies (about 30 min), --save DIR writes invalid bodies as .brep:
test_brep_import --corpus [dir] [-v] [--history]
# Display meshes against the built bodies (volume tolerance in %):
test_brep_import --corpus --meshes 1
# Per-file table; comparison of .smbh bodies (and their earlier states)
# with the .smb bodies:
python3 core/f3d/tools/corpus_table.py <mitcad-f3d-inspect> <test_brep_import output> <corpus dir>
python3 core/f3d/tools/smbh_compare.py <test_brep_import --history output>
```

`ctest` runs, all skipped when the corpus is missing:

- `core.f3d` (Rust, `core/f3d/tests/corpus.rs`): every entry
  decompresses, every blob parses completely, at least 99.5 % of the
  bodies convert without issues, every display scene parses without
  issues and every vertex lies in its face's box, every `.smbh` history
  parses with copies of the right types.
- `f3d.corpus_occt`: every 10th body built with OCCT, at least 99 % of
  those solids valid.
- `f3d.corpus_meshes`: every display mesh matches a built solid of the
  same document with as many faces and the same bounding box (within 1 %
  of the diagonal), volumes within 1 %.

## Known invalid-body classes

Fixes are in the OCCT builder, `geometry/src/brep_import.cpp`.

| Symptom (BRepCheck) | Cause | Fix |
|---|---|---|
| planar face `IntersectingWires`; or `UnorientableShape` with edges `InvalidCurveOnSurface`; or sheet `InvalidSameParameterFlag` | circle or ellipse edges at negative parameters (e.g. -3.07 .. -1.9). BRepCheck clips pcurves to the curve's period [0, 2 pi] when it looks for intersecting wires, so a clipped pcurve seems to cross another loop | edges on circles and ellipses are shifted by whole periods into [0, 2 pi]; an edge that then crosses the seam turns a circle's frame to start at the edge, an ellipse's by half a turn |
| cone face `UnorientableShape`, wire not closed | a face reaching a cone apex is bounded there by a loop of one curve-less edge; leaving it out loses part of the face, and healing adds a seam only between two such loops (it handles sphere poles, not cone apexes) | the apex loop is kept as a point loop of the face; the builder closes the face with a degenerated edge (pcurve along the u period at the apex) and healing adds the seam |
| sheet `InvalidImbricationOfWires` (not fixed) | a sketch profile whose circular hole touches the outline: hole and outline share two vertices 1e-14 mm apart joined by edges of 1e-4 mm | none; a generic fix would merge such vertices and drop the edges |

A few valid solids have all faces pointing inwards as stored (negative raw
volume); orienting the solid after the build fixes them.

## Thread surfaces

Thread flanks are `helix_spl_line` faces (format in
[ASM_FORMAT.md](ASM_FORMAT.md)), built as ruled B-spline surfaces between
two sampled helices. Threaded bodies build as valid solids, and their
volumes agree with the display meshes within 0.1 %.

## Display meshes and history

- Only the newest documents (ASM 232.3) carry a display scene
  ([OGS_FORMAT.md](OGS_FORMAT.md)). Every meshed display body matches a
  top-level `.smbh` body; mesh volumes agree within about 0.3 % and areas
  are slightly smaller (chords cut curved faces). Hidden bodies have no
  mesh.
- Bodies rolled back through the `.smbh` history build like the current
  ones (all valid so far). The `.smb` keeps bodies at earlier timeline
  steps; most `.smbh` bodies equal an `.smb` body as they are or after
  rolling back some states. Matching states to timeline features needs
  the design stream decoder (`src/design`); the rolled-back bodies can
  then serve as the expected result of each step.

## Not verified by the corpus

- Body transforms: every body has an identity transform (or none).
- Elliptic cones with a non-zero angle (an elliptic cylinder occurs and is
  built as an extrusion).
