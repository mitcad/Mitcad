# mitcad-3dm

Reader for `.3dm` models, the archive format of versions 5 to 8 (archive
versions 50 to 80), for the import of their bodies and blocks
(mitcad#1715). No geometry: the bodies are built by the import
(`core/ffi/src/import_3dm.rs`, the `import_3dm` command, `mitcad-cli
import-3dm`) with the B-rep builder of the other imports.

## Provenance

The format knowledge comes from the format's published reference source:
the C++ library its authors publish so that anyone can read and write
`.3dm` files, read as a description of the format. No code of it, and no
other reader's code, is used: what this reader takes from it are the
format's facts (typecodes, class ids, the order and meaning of the
fields), written anew in Rust under MitCad's licence (MIT). The names of
the typecodes (`TCODE_*`) are the format's own names for its values. The
format's name is used only to say which files MitCad reads.

The reader was contributed by GitHub user fjamesprice (mitcad/Mitcad#284)
and taken over and reviewed for MitCad in mitcad#1715: it was checked for
code copied from the reference source, for network or file access beyond
reading the given file, for a length, count or offset of the file used
unchecked, and for new dependencies (it has none: `miniz_oxide` is the
zlib inflater the other readers use).

## The archive

A 32-byte header (`3D Geometry File Format ` and the version, right
aligned in 8 characters), then chunks: a typecode of 4 bytes and a value
of 8 bytes (4 before version 5). A short chunk (`TCODE_SHORT`) keeps its
content in the value; any other chunk's value is the length of its data.
Numbers are little-endian. The tables (properties, settings, layers,
block definitions, objects, …) are chunks of records ending in
`TCODE_ENDOFTABLE`. An object is a class chunk: the class's UUID and the
object's data, which starts with its own version, so that fields added in
later versions are skipped with the rest of the chunk.

## Untrusted files

The reader gets files from anywhere, so a damaged or hostile file is
refused, object by object where it can be, and never makes it panic,
recurse without end or allocate far more than the file holds:

- Every length, count and offset read from the file is checked against
  the bytes left in its chunk before it is used (`Reader::count` with the
  least size of an item, `Reader::bytes`, the chunk lengths). An array is
  allocated only for as many items as its chunk can hold; the items of a
  B-rep's curve and surface arrays are boxed, so that an array of empty
  items costs a pointer each.
- Chunks nest at most `archive::MAX_DEPTH` (64) deep, which also bounds
  the recursion of curves that hold curves (polycurves, the profiles of
  surfaces of revolution and extrusions, the curves of sum surfaces).
- What the B-spline code relies on is checked as it is read: knot vectors
  finite, non-decreasing, no knot repeated more than the degree and a
  domain that is not empty (`geom::valid_knots`); orders of 26 at most
  (OCCT's degree 25); control points finite and weights not 0; arcs of at
  most a full turn over an increasing domain; lines, polylines and
  polycurves with increasing parameters.
- A mesh's compressed vertex buffer is inflated to the size its vertex
  count gives and no more (12 bytes a vertex, the count itself checked
  against the chunk), and its CRC is checked.
- Layer parents and nested blocks are followed 32 deep at most (a file's
  parents may form a loop); layers are looked up by index and id, not by
  walking the table for every object.
- The import (`core/ffi/src/import_3dm.rs`) leaves out an instance of a
  block inside that block (directly or through others), stops copies of
  blocks at 10,000 instances and 100,000 bodies, and takes a panic in the
  reader or in an object's conversion as that file or object not read.

An `Error` says what is wrong in words (`.0`, its `Display`) and where
apart (`.1`: offsets, byte counts, typecodes, class ids; `detail()` gives
both). The import shows only the words, naming an object by its kind,
name, block and layer, and puts the place and the object's index in the
report's `warning_details` and the log (docs/ui-conventions.md, "What a
message says"). An object whose data is damaged still has its
attributes read (`model::Failed`), so that it can be named so.

`core.3dm` cuts the test writer's models short at every length and
overwrites their bytes with values that make lengths and counts large and
floats not numbers (`cut_short_and_damaged_models_never_panic`), and the
fuzz target `model_3dm` (`core/fuzz`) runs the reader and the conversion
on arbitrary bytes.

## What real models showed

Two models of format version 7 (a solid of 509 faces in millimetres, and
an open polysurface of 271 faces with 31 curves in inches on nested
layers) corrected these points:

- **CRCs of the chunk's own bytes:** a chunk with `TCODE_CRC` ends in the
  CRC-32 of the bytes written while it was the innermost chunk, without
  the chunks nested in it (their headers, data and CRCs): a layer record,
  which holds only its layer object, has the CRC 0. The reader does not
  check these CRCs (it would have to read every field to know where
  nested chunks are); it checks every length and count against its chunk
  instead. The compressed buffers of meshes carry the CRC of their whole
  uncompressed data, which it checks. The test writer computes chunk CRCs
  as the format does (`the_writer_computes_crcs_as_the_format_does`).
- **Older class ids:** version 7 files still save B-reps, NURBS curves
  and surfaces and surfaces of revolution under the ids of an older
  library (the `*_TL` ids of `geom::class`), which name the same classes.
- **`is_solid` 3:** a B-rep may store 3, which the format reads as not
  known; so does the reader.
- **Block definitions of version 8** (the openly licensed sample models
  of `MITCAD_3DM_CORPUS`, mitcad#1715): the definition's id and name are
  in a chunk of another typecode (`TCODE_ANONYMOUS_CHUNK` + 2) as five
  fields in order, each after a byte that says whether it is there (a
  field not read, the id, a second id, an index, the name), where
  versions 6 and 7 have a set of bits saying which fields follow.
- **Spheres:** a sphere is a surface of revolution of a half circle whose
  one face's loop is its seam twice and two singular trims; once the
  singular trims are left out, the seam out and back bounds nothing (see
  the conversion).
- **Polycurve edges:** an open polysurface's edges include polycurves
  (30 more edges once split per segment).

## What is read

- Properties: the version of the library that wrote the file and the
  application; settings: the unit system (the format's numbers, custom
  units by metres per unit) and the absolute tolerance.
- Layers: index, id, parent, name, colour, visibility; the full path
  (`Parent::Child`) and visibility through the parents.
- Block definitions (format version 6 and later): id, name, the ids of
  their objects. The older form is reported and its instances left out.
- Object attributes: id, layer, name, colour and its source, visibility,
  mode (3: the geometry of a block definition).
- Geometry: B-reps (3D curves, surfaces, vertices, edges, trims, loops,
  faces, `is_solid`, the faces' render meshes; the 2D trim curves are
  skipped), extrusions (profiles, path, up direction, caps, mitered
  ends), NURBS curves and surfaces, lines, arcs, polylines, polycurves,
  plane surfaces, surfaces of revolution, sum surfaces, meshes (versions
  1 and 3, compressed vertex buffers), block instances.

## The conversion

`convert` turns B-reps, extrusions and untrimmed surfaces into the
neutral model of `core/asm` in millimetres:

- NURBS are clamped (the format lets periodic curves keep unclamped knot
  vectors) by knot insertion at the ends of their domain; rational
  control points are divided by their weights.
- An edge's curve keeps the file's parameters: lines by length, arcs by
  angle, NURBS as they are. An edge on a polycurve or polyline becomes
  one edge per segment, with vertices at the joints, so that no curve of
  mixed degrees is needed.
- A trim's direction is made relative to its face's normal (the neutral
  model's convention; the format's is relative to the surface), and a
  face's `reversed` takes in a surface whose parameters run against its
  geometry (a plane surface with reversed extents, a transposed surface
  of revolution) and a solid whose normals point in (`is_solid` 2).
  Singular trims (at a surface's poles) are left out of their loops. A
  face whose loops hold only its surface's seams and poles (a sphere: its
  seam from pole to pole and back, a singular trim at each pole) is all
  of its surface: it has no loops, on a bounded surface (a surface of
  revolution as its exact B-spline, the profile times a rational circle),
  which the builder takes whole with its seam and poles.
- A face faces as the file means: its surface's normal (a plane
  surface's `z`, against it where one extent runs against its domain),
  turned round by the face's own `reversed`; its loops run
  counterclockwise about that normal. An untrimmed plane surface faces
  along its surface's normal. (mitcad#1738: the open plane surfaces of a
  sample model, drawn in a top view, come in facing up; they rendered
  black from above only because the render's ground lay in their plane.)
- The profile of a surface of revolution and the curves of a sum surface
  are joined into one B-spline where they are polycurves (segments
  raised to the highest degree); a sum surface is the exact tensor
  product.
- An extrusion is built as its B-rep would be: a side face per segment of
  each profile, the caps planes, each face facing out (by the profiles'
  winding). Mitered extrusions are not converted.

## Tests

`core.3dm` (cargo): the sample model of `testfile` (a box whose bottom,
front and left planes face in, a cylinder on a surface of revolution with
its seam, an extruded plate with a hole, a round extrusion, a compressed
mesh, a curve, a block with two instances), a block that holds an
instance of itself and three plane surfaces drawn facing up, each kept
in another of the file's forms (`plane_surfaces`: the plane's frame up;
its frame down and the face reversed; an extent against its domain and
the face reversed), untrimmed plane surfaces with extents against their
domains, the clamping and joining of B-splines, models cut
short and damaged, curves nested too deep or with invalid knots, angles
or parameters, a count larger than its chunk, and the models of
`MITCAD_3DM_CORPUS` (each face on a plane whose outer loop is straight
faces as the file means: its loop's sense by its vertices, turned by the
face's `reversed`). Through OCCT (`tools/cli`): `cli.import_3dm` (the
sample), `cli.import_3dm_block_in_itself` (the block placed once, the
instance in itself left out), `cli.render_3dm_plane_surfaces` (the three
plane surfaces imported facing up and rendered lit from above) and
`3dm.corpus` (the models an `index.tsv`
under `MITCAD_3DM_CORPUS` lists; [development.md](../../docs/development.md#3dm-corpus)).

`mitcad-3dm-inspect [--convert] [--faces] <file.3dm>` prints a model's
summary; with `--convert` each B-rep's and extrusion's conversion and its
check; with `--faces` each B-rep's faces as the file keeps them (a plane
surface's frame, domain and extents, the face's `reversed`, its loops'
trims and each loop's normal by its vertices).
