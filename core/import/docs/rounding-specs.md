# The rounding specs of the `.f3d` corpora

Every fillet or chamfer of the `.f3d` corpora that the geometry kernel
fails on during an import, kept as a spec: the body it rounds, its
definition (edges by name, sizes) and the history's stored bodies before
and after it, so that OCCT port patches can be run against the real
roundings without importing the designs again. mitcad#107 made the first
set (249 specs) and mitcad#121, mitcad#238 and the port rounds after them
measured against it; that set lived in a removed worktree and was
regenerated, with its generator in the repository, in mitcad#1398. Part of
the [.f3d import](f3d.md) notes; the [import documents](../README.md#documents)
list the other topics.

## Where the specs are

The specs are derived from the owner's designs, so they are kept outside
the repository, in the private test files repository (`mitcad_testfiles`)
under `analysis/fillet-specs/`, as written by the generator:

| Path | What it holds |
|---|---|
| `files.tsv` | each file run, once per content: content id (the first 8 hex digits of the file's SHA-256), the corpus variable, its path in that corpus |
| `specs/<content id>/<design>.jsonl` | the import's dressup dump: one line per failed definition (item, feature name, definition, error, the history's states, the files) |
| `specs/<content id>/<design>/T<item>-<n>.shape` | the input body (`-before`, `-after`: the stored bodies of the history's states before and after the item nearest to it), in the kernel's stored-result bytes with the face names the definition refers to |
| `reports/<content id>*.json` | the import's report of each design (the items' outcomes) |
| `index.tsv` | one row per spec: id, corpus, content id, item, feature name, type, sizes, edges and faces selected, the error's class, the item's outcome in the import, the files, the signature distances, the states, the error |
| `summary.txt` | the specs counted by class and outcome |
| `results.tsv` | the last run of the specs again (below) |

Each spec's id is `<content id>/<design>/T<item>-<n>`. An item's spec whose
item still ends parametric failed only in an earlier candidate of the
import (another reading of its edges, the exact state's retry); the
item's outcome column tells them apart.

## Making them

```bash
cmake --preset <release preset> && cmake --build build/rel --target test_f3d_import
bash tools/f3d-rounding-specs.sh build/rel OUT --jobs 3
```

`tools/f3d-rounding-specs.sh` imports each file under `MITCAD_F3D_CORPUS`,
`MITCAD_F3D_MODELS` and `MITCAD_F3D_BACKUP` once per content (reference
copies of the same file once), each in a child process of
`test_f3d_import --corpus-file`, with the import's dressup dump
(`MITCAD_IMPORT_DRESSUPS`, `core/import-replay/src/dressups.rs`) in
`OUT/specs/<content id>/` and the report in `OUT/reports/`; a stopped run
goes on where it was (`OUT/done/`). Then
`tools/f3d-rounding-specs-index.py OUT` writes the index and the summary.
The run is a coverage run's import: 889 distinct files, three at a time,
about 40 minutes on the Linux workstation (a release build). Copy `OUT`
without `logs/`, `done/` and `files.tsv.new` to `analysis/fillet-specs/`
of the test files.

## Running them again

```bash
cmake --build build/rel --target mitcad-rounding-specs
python3 tools/f3d-rounding-specs-index.py OUT --run build/rel/tools/rounding-specs/mitcad-rounding-specs \
    [--class "walk between two faces"] [--spec <id>]... --jobs 3
```

`mitcad-rounding-specs` (`tools/rounding-specs`) runs one spec through the
dressup as the import runs it (`geometry/src/dressup.cpp`, with its own
retries), on the input body, and compares the result with the stored body
after it: MATCH (valid, the file's faces, the change of volume within
1e-6 of the file's), NEAR (valid, within 1e-3), OFF (valid, further),
INVALID, FAIL (the dressup's error). Where the history has one state for
before and after (the `states` column), the file stores no body between,
so the comparison is not the rounding's alone. Constant and chord-length
fillets and the three chamfer sizes run; variable, asymmetric and
curvature continuous sets are skipped.

`--describe` prints the selected edges, their faces and the other faces at
their vertices first (each surface's kind and radii, `concave` where it
bends away from the material), which tells a spec's geometry (a concave
arc tighter than the ball, a rim rounded tighter than the ball) without
opening the design. `--chains` prints the edges the dressup's own
tangent chain adds to the selection and, at each end of the chain and each
turn in it, the edges there with their angle to the chain's edge and the
angle between their faces at their start, middle and end (mitcad#1435).

## The set of mitcad#1398

Made on dev `895641dcd` (port patches up to 1560, 1380 among them): 889
distinct files (44 under `MITCAD_F3D_CORPUS`, 509 under
`MITCAD_F3D_MODELS`, 336 under `MITCAD_F3D_BACKUP`), every import finished.
708 specs of 149 items in 79 files (an item gets up to 12, one per failed
candidate with a new error). By the item's outcome: 92 items fall back, 32
end parametric (a candidate failed before the one taken), 7 are skipped
(the file's own operation failed) and 18 are items of documents the files
refer to (`linked`). By the class of the first spec of each item:

| Class | Items | Fall back |
|---|---|---|
| the corner at a rounding's end | 38 | 24 |
| the result would leave the body open | 32 | 22 |
| an invalid result | 20 | 12 |
| no start | 14 | 10 |
| an exception in OCCT | 8 | 6 |
| a twisted surface | 6 | 2 |
| a chamfer wider than a face | 6 | 2 |
| roundings overlapping across a face | 6 | 2 |
| the walk between two faces | 4 | 0 |
| two contours' roundings intersect | 4 | 3 |
| two or more roundings joined at a vertex | 3 | 3 |
| the computation of a rounding | 3 | 2 |
| others | 3 | 2 |
| rolling on an edge | 2 | 2 |

The set of mitcad#107 (249 specs, 9 October 2026) was made before most
of the port's fillet patches, on inputs that were then mostly the files'
stored bodies; this one has the replay's bodies as inputs, so the two are
not comparable spec by spec.

### mitcad#238's three-face specs

mitcad#238 left 9 specs (8 inputs) of the old set open where the ball
would touch three faces at once: a corner rounded with 0.5 or 1 under a
rounding of 1 or 1.2, a corner pocket of 1.25 under a rounding of 3,
roundings of 0.5 at both ends of the edge under a rounding of 1. The old
set's ids are lost with it; the items were found again by that geometry
among the items whose roundings fail in the walk between two faces now
(any candidate; `--describe`) and the items mitcad#1371 built:

| Item (content id) | Geometry | Now |
|---|---|---|
| `5ef2dbd5` 30, `99ef321f` 14 | a rod's foot rounded with 3 past a wall's concave arcs of 1.25 | parametric, verified (1380, mitcad#1371) |
| `852ad5eb` 10 | edges rounded with 1 between earlier roundings of 0.5 and a bore of 17.75 | parametric, verified (a first candidate's walk fails; the item's corner faces within 0.21 mm) |
| `4365a039` 14 | a plate's edge along a cylinder of 11.25, rounded with 1 past earlier concave roundings of 0.5 at both ends | skipped: the file's own rounding failed (its operation left the body as it was); every candidate fails (in the walk or at its end corners), as in the file |
| `64052a50` 41, `d7d06e51` 41 | four straight edges rounded with 1 (a chain of planes) past a corner rounded with 0.1 | skipped: the file's own rounding failed (it names an earlier item's operation, no state of its own) |

Each is the file's result: built and verified, or, where the file's
modeller failed the rounding too, skipped as the file has it.

The other items whose roundings fail in the walk between two faces in a
candidate fall back at another cause first (their first spec's class):

| Item (content id) | Geometry | Where OCCT stops |
|---|---|---|
| `5d6ee474` 88 (also `36f79fb9`, `81beaf7b`, `9cef13ca`: copies of one design) | radius 5 along the side edges of an earlier concave rounding of 3 (a torus) and past B-spline faces; the input is the stored body of an earlier fallback | the corner at the ends: the walk round the vertex does not come back to the rounding; now parametric, verified (1580 and 1581, mitcad#1425, [f3d-rounds-4.md](f3d-rounds-4.md#a-walls-edges-rounded-past-a-concave-corner-into-a-tip-mitcad1425)) |
| `6eccefc2` 11, `c23a90e7` 12 | radius 1.5 round a rib's foot on a tube whose rims are rounded with 1 (convex tori tighter than the ball) | the computation of the rounding (the rational section's weights, f3d-rounds-2.md); on the stored body (`6eccefc2`) and on the replay's (`c23a90e7`); now parametric, verified (1590, mitcad#1426, [f3d-rounds-4.md](f3d-rounds-4.md#a-cut-faces-outline-rounded-past-a-tubes-rims-tighter-than-the-ball-mitcad1426)) |
| `c4e070f9` 8 | radius 0.2 round an opening on a plane past earlier roundings of 0.2 (tori) | the corner at the ends: no result; now parametric, verified (1600 and 1601, mitcad#1427, [f3d-rounds-4.md](f3d-rounds-4.md#an-openings-wall-rounded-through-the-corners-of-earlier-roundings-mitcad1427)), its seven named edges read and run on through the turns (mitcad#1435) |
| `09c282ac` 22 | radius 0.5 along a concave cylinder of 12.5 past earlier roundings of 2 and 0.5 | the rounding's surface twists: the walk on from the sphere over the corner of 0.5 started from a second section of the ball, where the guide turns back round the rounding of 2; now parametric, verified (1610 and 1611, mitcad#1428, [f3d-rounds-4.md](f3d-rounds-4.md#a-grooves-rim-rounded-past-a-corner-of-the-balls-radius-mitcad1428)), its edges read (the file's pieces of an edge split at a seam, mitcad#1437) |
