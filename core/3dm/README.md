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
  most a full turn over an increasing domain, and surfaces of revolution
  of at most a full turn the same way; lines, polylines and polycurves
  with increasing parameters.
- What the conversion builds from the product of two curves is bounded,
  since each curve is bounded by its bytes but their product is not: an
  arc is made of at most `nurbs::MAX_ARC_SPANS` spans, and a sum surface
  or a whole surface of revolution with more than
  `nurbs::MAX_SURFACE_POLES` control points is refused in words, its
  face not built. A failed allocation aborts instead of panicking, so the
  import's catch of panics would not save it.
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
  1 and 3, compressed vertex buffers), block instances, SubD objects'
  control nets (below).

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

## SubD objects

A SubD object (class `F09BA4D9-455B-42C3-BA3B-E6CCACEF853B`, format
version 7 and later) is read as its control net, level 0
(`src/subd.rs`, mitcad#1757), from the format's facts in its published
reference source, as the rest of the reader:

- The object's data: a byte, 0 for an empty SubD; then a chunk with a
  version of two ints (major 1) holding the count of levels, the largest
  vertex, edge and face ids (three ints) and a bounding box (always
  empty), then one chunk per level (major 1, minor 1 or later), then
  texture settings, the symmetry and the face packing, which are not read
  (the chunk is left with them).
- A level: its index (u16; level 0 is read, the levels after it are the
  net subdivided and are not), three bytes, the net's bounding box, then
  four u32 archive ids that part the components: vertices from the first
  to the second, edges to the third, faces to the fourth. The components
  follow in that order, each with the next archive id (checked: they
  follow on without a gap), its own id (u32) and its level (u16); then,
  from archive version 70, additions: a size byte and that many bytes
  each, 0 a slot not there, 254 a chunk, 255 the end (before 70, two
  optional points behind a flag byte each).
- A list of components (a vertex's edges and faces, an edge's vertices
  and faces, a face's edges): a u16 count and per item its archive id
  (u32) and a flag byte, whose lowest bit says that the item is taken
  against its own direction (a face that runs along the edge from its
  second vertex to its first).
- A vertex: its tag (1 smooth, 2 crease, 3 corner, 4 dart), its point,
  the counts of its edges and faces (u16 each), a byte 0 (or a count of
  saved limit points and 101 bytes each), its edge list and its face
  list, then its additions (or a byte 0 before version 70).
- An edge: its tag (1 smooth, 2 crease, 4 smooth with both ends sharp
  vertices), the count of its faces, two sector coefficients and the
  sharpness at its start (f64s), its vertex list (two) and its face list,
  then its additions: from archive version 80 the first, of 8 bytes, is
  the sharpness at its end. A smooth edge with a sharpness is a
  semi-sharp crease; version 7 keeps no sharpness.
- A face: the id of the level-0 face it comes from and an obsolete int,
  its edge count (u16) and its edge list (whose count repeats it), then
  its additions (texture domain, material, colour, packing, texture
  points), all skipped.

Every count is checked against the bytes left in its chunk before it is
read (the least size of a vertex, an edge and a face against the level's
bytes), every archive id against its part of the level.

`subd::subd_cage` makes the net a Form's control cage
(`mitcad_subdiv::Cage`) in millimetres: each face's loop from its edges
in order (refused when they do not join), the faces turned to run one way
and out of each closed piece (`mitcad_subdiv::orient`; refused for a
one-sided surface), an edge of more than two faces refused, the creased
edges between two faces creases sharp for good, a smooth edge with a
sharpness a crease of that sharpness (the mean of its ends' where they
differ, said in a note), corner vertices corners, vertices on no face left
out (a note); the vertices and faces keep the file's ids where they are
all above 0 and distinct. The notes and the errors are words, without ids
or points. The import (`core/ffi/src/import_3dm.rs`) makes each cage an
editable Form feature; what is not carried (the symmetry, the subdivided
levels, per-face texture, material and colour, the format's own sector
rules) is listed in `core/model-api/src/commands.md`, *.3dm import*.

The test writer writes SubD objects as the reader reads them
(`testfile::write_subd`, `testfile::subd`): a cube of 2 cm with a creased
edge, a semi-sharp edge whose sharpness runs from 1 to 2, a corner and a
vertex on no face, of archive version 80 (or 70). The reading is also
checked on a SubD written by the format's own application (mitcad#1810):
the test files under `MITCAD_3DM_CORPUS` hold an openly licensed model of
archive version 80 (content id `98400d4d`, MIT) with one SubD, a box of
10 mm: 8 vertices, 6 quads, the bottom ring and one vertical edge creased
(a corner where three creases meet, a dart where one ends), three vertical
edges semi-sharp (1, 3, and 0.5 to 2.5 along the edge) and the top ring
smooth. Its `index.tsv` row checks the import's Form line and runs a
`mitcad-cli` script on the imported design
([development.md](../../docs/development.md#3dm-corpus)).

## Tests

`core.3dm` (cargo): the SubD cube of `testfile` (archive versions 80 and
70: the cage, its ids, creases, corner and notes; faces turned round and
refused nets), the sample model of `testfile` (a box whose bottom,
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
face's `reversed`; each SubD's net makes a Form's cage). Through OCCT (`tools/cli`): `cli.import_3dm` (the
sample), `cli.import_3dm_block_in_itself` (the block placed once, the
instance in itself left out), `cli.render_3dm_plane_surfaces` (the three
plane surfaces imported facing up and rendered lit from above) and
`3dm.corpus` (the models an `index.tsv`
under `MITCAD_3DM_CORPUS` lists; [development.md](../../docs/development.md#3dm-corpus)),
`cli.import_3dm_subd` (the SubD cube as a Form feature, built as a solid)
and `cli.import_3dm_subd_edit` (that Form's cage as the file's, a vertex
moved with `form.move`).

`mitcad-3dm-inspect [--convert] [--faces] <file.3dm>` prints a model's
summary; with `--convert` each B-rep's and extrusion's conversion and its
check; with `--faces` each B-rep's faces as the file keeps them (a plane
surface's frame, domain and extents, the face's `reversed`, its loops'
trims and each loop's normal by its vertices).
