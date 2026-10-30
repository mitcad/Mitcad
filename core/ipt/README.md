# mitcad-ipt: the `.ipt` part and `.iam` assembly file reader

`core/ipt` reads `.ipt` part files: the bodies stored in the file, the
document properties, and the part's design: its parameters with their
expressions, sketches, work planes and features, as the import's dump IR
(`mitcad_dump::Dump`, [SCHEMA.md](../import/SCHEMA.md)). The
`.ipt` import (mitcad#60) replays that design with `mitcad-import` against
the ASM history of the B-rep record, as the `.f3d` import does, or builds
the stored bodies as base features (`core/ffi/src/ipt_import.rs`,
`core/ffi/src/ipt_history.rs`, the `import_ipt` command in
[commands.md](../model-api/src/commands.md#ipt-import), `mitcad-cli
import-ipt`, File › Open). It also reads `.iam` assemblies: the files they
refer to and the occurrences that place them ([Assemblies](#assemblies)),
which the `.iam` import makes MitCad's components and occurrences.

The container and the property sets follow Microsoft's published
specifications (MS-CFB, MS-OLEPS). Everything else here comes from
inspecting files: no program code of the authoring application was used,
and other readers of the format were neither copied nor linked. Record
layouts of the definitions segment were cross-checked with the public
format notes of the cadmpeg project (CC BY 4.0,
<https://github.com/cadmpeg/cadmpeg>), which describe older segment
versions; the layouts here are those of the test files. Claims are
marked as in [ASM_FORMAT.md](../f3d/ASM_FORMAT.md): *(verified)* means
checked on every file of the test corpus (below), *(seen)* means observed
but not understood further.

## Modules

| Module | Contents |
|---|---|
| `cfb` | Compound files (MS-CFB): versions 3 and 4, FAT, DIFAT, mini stream, the directory's red-black trees. Chains are checked against the file size and loops; the directory walk refuses storages nested deeper than 64 levels and paths taking more than 32 MiB together (`MAX_DEPTH`, `MAX_PATH_BYTES`), so a hostile directory cannot make its paths quadratic in its size. A writer for tests. The `mitcad-cfb` crate (`core/cfb`), shared with the `.rvt` reader and re-exported here as `cfb`. |
| `props` | Property set streams (MS-OLEPS): one or two sets per stream, the dictionary, the code page, typed values (integers, reals, booleans, strings, file times, class ids). A writer for tests. |
| `rse` | The segment database: meta stream headers, record tables, record splitting, zlib and zstd. A writer for tests. |
| `lib.rs` | `IptFile`: segments, property sets, document properties, B-rep records, the definitions segment; `read_bodies` converts the bodies with the `.f3d` reader's ASM code. |
| `mesh` | The meshes of mesh features in the graphics segment. A writer for tests. |
| `dc` | The definitions segment (`PmDCSegment`): records by type, the common header, references, lists, texts, labels. |
| `params` | Parameters: units, expression trees, their translation into MitCad's expression language and their evaluation. |
| `sketch` | Planar sketches: transform, points, lines, circles and arcs, constraints, dimensions. |
| `groups` | Polygons and circular and rectangular patterns of sketch entities. |
| `text` | Sketch texts: the text and frame records, the glyph contours the file keeps, the application segment's font table (mitcad#414). |
| `features` | Extrusions, revolutions, holes, fillets, chamfers, rectangular and circular patterns, mirrors, threads, coils, work planes, combines, splits, shells, sweeps; the bodies features refer to and the items that made them. |
| `profile` | The profiles a feature selected, measured from the definitions segment's own ASM record. |
| `design` | The timeline (the browser's features in order, with their history states) and the dump IR. |
| `result` | The result segment's body list: which bodies the part shows, solids or surfaces, their range boxes (*The result segment* below). A writer for tests. |
| `states` | The bodies of the B-rep records at the states of their ASM history, read once for all tries of the import: the distinct ones by the step they were converted at, converted again when asked for. |
| `assembly` | Assemblies (`.iam`): the referenced files, the occurrences with their keys, flags, labels, documents, placements, range boxes and display transforms (*Assemblies* below). |
| `sheet` | Sheet metal (mitcad#211): the sheet-metal style's K-factor, faces, flanges, cuts, corners, bends and folds as the dump's sheet items (*Sheet metal* below). |
| `naming` | Topological names (mitcad#242): the items features name edges and faces by, the naming table's names, the naming attributes of the ASM entities at a history state, and the edge a name names there (*Topological names* below). |
| `testdata`, `testdesign` | Small parts made by these writers: two bodies of MitCad's own ASM writer (a cube and a cylinder), part number, material, inches; the cube with the records of a parameter, a sketch and an extrusion that make it; a part of a mesh feature's tetrahedron; an empty part; the cube and the cylinder with a result list that keeps the cylinder hidden. |
| `testassembly` | Small assemblies made by these writers and a test project of them (a top assembly as saved on another machine, a sub-assembly, parts found relative to it and by name, a missing one; hidden, grounded and suppressed occurrences). |
| `bin/mitcad-ipt-inspect` | `list`, `properties`, `segments`, `records`, `record [--raw]`, `data`, `dump`, `labels`, `stream [--raw]`, `asm`, `bodies [-v|-f]`, `result`, `history`, `parameters`, `typed`, `show`, `constraints`, `prefix`, `flags`, `design [--json]`, `profile [-v]`, `slots` (a sheet-metal group's parts too), `tree`, `find`, `assembly`, `write-test`; for the names (mitcad#242) `words` (a segment's records as u32 words), `nodes`, `names` (each feature's items with their names), `entry`, `brepnames <state>` (the faces and edges of a history state with their naming attributes), `study` (every item resolved in the state before its feature, checked against the file's own `matched` marks); `texts` (the sketch texts, their fonts and glyph contours, mitcad#414). |

## The container

A compound file (signature `D0 CF 11 E0 A1 B1 1A E1`) of version 3 with
512-byte sectors *(verified)*. The root storage's class id is
`{4d29b490-49b2-11d0-93c3-7e0706000000}` for a part *(verified)*; the
reader warns about another class but goes on. At the root:

| Entry | Contents |
|---|---|
| `\x05…` streams | Property sets (below) |
| `RSeStorage/` | The segment database (below) |
| `RSeStorage/M<id>`, `B<id>` | A segment's meta and data streams; `<id>` is a 26-character name |
| `RSeStorage/RSeSegInfo`, `RSeDbRevisionInfo`, `V<n>/RSeDb` | The segment registry, revisions, the database header *(seen; not read)* |
| `UFRxDoc`, `Protein`, `RSeStorage/RSeEmbeddings/` | References, appearances, embedded documents *(seen; not read)* |

## Property sets

Streams at the root whose names start with the character 0x05, each a
property set stream with one set *(verified)*; property 255 holds the set's
name, the dictionary names it. The import reads two sets by their format
ids:

| Format id | Property | Meaning |
|---|---|---|
| `{32853f0f-3444-11d1-9e93-0060b03c1ca6}` (tracking properties) | 5 | part number *(verified)* |
| | 20 | material name *(verified)* |
| | 67 | the release that saved the file, as text *(verified)* |
| `{bb586990-af3e-11d3-95a9-00a0c9b6e37a}` (model settings) | 8 | length unit: 11269 mm, 11272 inch *(verified)*; 11268 cm, 11270 m, 11271 µm, 11273 ft, 11274 yd, 11275 mile in the same enumeration (not seen) |
| | 9, 10, 11 | angle, time and mass units (11279 degree, 11290 second, 11283 kg, 11286 pound) *(seen)* |

No cached volume or area was found in the property sets of the test files.

The part's properties (`IptFile::part_properties`, mitcad#496) are read
from the summary (`{f29f85e0-4ff9-1068-ab91-08002b27b3d9}`: 2 title, 3
subject, 4 author, 5 keywords, 6 comments, 8 last saved by, the standard
ids of the format), the document summary
(`{d5cdd502-2e9c-101b-9397-08002b2cf9ae}`: 2 category, 14 manager, 15
company), the tracking properties above, and every property a set's
dictionary names (the user's own); the import gives them to the design's
root.

## The segment database

Every segment is a pair of streams with the same `<id>`:

**Meta stream** `M<id>` *(verified)*: a u32 length and the tag
`RSe Meta Stream Version 8`, a u16 (8), 16 bytes of release stamps, the
segment's name (u32 count of UTF-16 code units, then the name, e.g.
`PmBRepSegment`), its 16-byte id, three u32, the creation and modification
times as length-prefixed text, a byte 1, and then the tables, compressed:

| Field | |
|---|---|
| u32 | record slots (one more than the records) |
| u16 | a kind: 2 for the B-rep and graphics segments, 1, 3, 5 or 8 for others |
| 8 bytes | *(seen)* |
| u32 n, n × u32 | the block table: each record's size; the top bit set in every record seen |
| u32, u32 m, m × 10 bytes | a node table *(seen; not read)* |
| 12 bytes, u32 t, t × 28 bytes | the type table: a 16-byte type id, two u16 and two u32 *(seen)* |

**Data stream** `B<id>` *(verified)*: a 16-byte prefix (the same in every
segment), two bytes (4, 1), then the compressed records one after the other
in the order of the block table: a selector (u32; its low byte is the
record's index in the type table), the record's bytes, its size again
(u32), and from major version 20 on one byte: 0, or 1 when an extended
trailer follows (typed property lists, not read; in the appearance segment
and in one definitions segment of the test files). Segments of majors 16
and 18 have no such byte: the next record's selector follows the size
*(verified: every segment of the files of 2012 and 2014 releases; a byte
there was taken for a trailer's flag)*. After the last record: `FF FF FF
FF` and a few bytes. The reader checks every size against its echo and
stops at the first mismatch; an extended trailer ends where the next
record starts: a selector of the same form whose bytes are followed by
their size, or the end marker after the last record. A segment of a
version not seen is split with the layout of its version and, when that
fails, with the other. Every segment of the test files splits.

The meta stream's eight u16 release stamps hold the segment's major
version in the low byte of the sixth: 16 for the files a 2012 release
saved, 18 for 2014, 20 for 2016, 21 for 2017, 22 for 2018, 24 for 2020, 25
for 2021 (26 for one), 27 for 2023 and 28 for 2024 *(verified)*.

The tables and the records are zlib streams in every file seen; a zstd
frame is recognised by its magic number (`28 B5 2F FD`) and decompressed,
for releases that are said to have changed the compression (no such file
was available to check).

Segments of a part file seen: `PmBRepSegment` (the bodies), `PmDCSegment`
(definitions: parameters, sketches, features), `PmGraphicsSegment`,
`PmBrowserSegment`, `PmAppSegment`, `PmResultSegment`,
`DesignViewSegment`, `NBNotebookSegment`, `FBAttributeSegment`.

## The B-rep record

The B-rep segment holds one record of the type
`5c5945f6d5113313100060a6bba647b5` (as stored) *(verified)*. From its 14th
byte the record is an ASM binary file (`ASM BinaryFile4`, the format of the
`.f3d` body blobs, [ASM_FORMAT.md](../f3d/ASM_FORMAT.md)) ending with
`End-of-ASM-data`, followed by 18 bytes the import does not read. The
14-byte head is `00 00 00 00 02 00 1D 03 00 00` and a u32 (43 or 44)
*(seen)*. When a B-rep segment's records do not split, the reader scans its
data for the ASM magic instead and says so (`PmBRepSegment@scan`). Files
of a 2012 release store the same binary format under its older magic (a
four-letter prefix instead of `ASM` and no integer width digit; ASM 217),
whose end and history markers name it by that prefix: the ASM reader
reads it as the newer one *(verified on the one such file)*.

A B-rep record may hold no bodies at all: an empty part (only the origin's
work features), or a part whose body is a mesh feature's (*Mesh features*
below). The import opens an empty part as an empty document, with a
warning.

The ASM files of the test corpus are save versions 226 to 229 with
centimetres as the model unit (the header's 10 mm per unit), which the
`.f3d` reader's converter reads unchanged: every body converts without
issues and its checks pass. Their header flag says that they carry an ASM
history; it has 2 to 198 states (`history_states`), which the import
checks the features against (*The timeline and the history states*
below), as the `.f3d` import uses the `.smbh` history.

Besides the part's solid, three files store two one-face sheet bodies;
the import brings them in as sheet bodies.

A B-rep record may also hold bodies of wires only, without faces (seen
in two parts of the older corpus, next to their solid): the result list
below has no entry for them, and the import leaves them out with a
warning instead of failing.

### Bodies the builder repairs

Some stored bodies are valid in their own modeller but not as OCCT reads
them; the converter and the builder (`geometry/src/brep_import.cpp`)
repair them without changing their shape, and the import report says
what was done (`tidied`, `messages`):

- **Slits**: an edge whose two coedges follow one another in a loop,
  out and back (a cylinder touching a plane along a line keeps that line
  in the plane's loop), bounds nothing and is left out of the loop.
- **Pinched loops**: a loop of a planar face that passes a vertex twice
  (a hole touching the boundary at a point is one loop with it) is split
  there into loops sharing the vertex; such wires are healed again
  without ShapeFix's repair of intersecting wires, which put edges of no
  length at the shared point.
- **Faces across the seam of a closed surface**: a B-spline surface the
  file keeps closed but not periodic (closure 1, the ends meet) has
  faces whose parameter curves run past its seam; it is built periodic,
  as a periodic one is, so that the face wraps round instead of lying on
  the surface's extrapolation *(seen: a third-party part, two faces
  of 1e19 mm² whose meshing took more than 6 GB and minutes; now 1e4 mm²
  each, the part meshed in seconds, mitcad#499)*.
- **Edges of more than two faces** of a shell (solids that touch along an
  edge, stored as one shell): the body is healed again with ShapeFix
  orienting the shells, which splits them at those edges, when that keeps
  the volume *(verified: two parts of the older corpus, valid solids of
  the same volume)*.
- **Cusps**: two edges of a loop that leave their shared vertex in the
  same direction (a fillet arc tangent to a short arc that turns back
  along it, in a gear's planar face) stay within the tolerance of each
  other beyond the vertex, and OCCT takes the loop for self-intersecting.
  The edges' parameter ranges and their ends agree with the vertices
  (within 1e-4 mm); the vertex's tolerance is doubled until the edges no
  longer cross outside it, at most 0.05 mm *(verified: once, 0.2 µm)*.

## The result segment

`result`. `PmResultSegment` starts with a record of the type
`24af8a12…` (as stored) that lists the part's bodies as the browser's
body folders show them *(verified: every part of both corpora with
bodies, the entries in the order of the B-rep record's bodies)*: a u32, a
u16, a byte (majors 16 and 18) or a u32, then a list (kind 4: u16 4, u16
`0x3000`, u32 count and, when not empty, a u32 tag) with per body

| Field | |
|---|---|
| u32 | the body's node number (the header's node of its body record in the definitions segment) |
| list of u32 | the features that made its faces (as the faces' naming attributes in the B-rep record name them) |
| u32 | 1 shown, 0 hidden (the browser's visibility) |
| u32 | 2 a solid, 1 surfaces |
| list of u32 | the node numbers of the bodies of a group (a composite feature's surfaces: one entry for all of them); empty for one body |
| 6 × f64 | the range box, cm: min, max |

and two more lists the import does not read. The list keeps entries of
bodies a later feature consumed, with an empty range (min above max, or
all zero). The import hides the bodies whose entries are hidden, each
matched to the imported body whose bounding box is closest to its range
box (within 1% of the box's diagonal, at least 0.05 mm), in the
bodies-only and the history import, as the `.f3d` import treats hidden
bodies; a hidden range no body matches is a warning.

What the list told about the parts whose body count looked wrong:

- A part of one shown solid and one hidden surface (a surface a sculpt
  consumed): the import makes both and hides the surface.
- A part of 23 bodies: the list has 23 shown solids; the part is a
  multi-body part, and 23 is right.
- Parts with a group of surfaces (a composite feature of 2 and of 4
  sheets, shown): the history import left them out because the last
  history state "could not be rebuilt" — its sheets, with edges of three
  faces and edge ends farther than the bound from their vertices, counted
  as converted with lost faces. The bodies of the last history state are
  the stored design itself, as the bodies-only import builds them, and no
  longer count so: both imports now make the sheets.

## Mesh features

`mesh`. A mesh feature (a mesh read from another file into the part) has
a record of the type `664c2dec…` in the definitions segment *(seen: every
part with a mesh feature in the browser)*; its triangles are not in the
B-rep record (which then has no bodies) but in the graphics segment
(`PmGraphicsSegment`), in four records one after the other:

| Record type | Contents |
|---|---|
| `02adf9de…` | vertices: f32 x, y, z (cm, the part's coordinates) |
| `03adf9de…` | triangles: u32 vertex indices, three a triangle, counter-clockwise seen from outside |
| `ce171165…` | normals: f32 x, y, z (length 10 or 1) |
| `03adf9de…` | the normal of each triangle corner, by index |

Each is a six-byte head (u32, u16) and a list (kind 2) whose second u32 is
the count again and the third an item code (279 for points, 280 for
normals, 0 for indices; 290, 291 and 8 in a part saved by a 2023
release), followed by an empty map *(verified: in every mesh of the test
files the indices are within the vertices, the winding agrees with the
normals, and the meshes are closed apart from one)*. The
import builds each vertex record with the triangle record after it as a
mesh body (as an STL file gives); the normals are not read. Parts without
a mesh feature have no records of these types.

## The definitions segment

`PmDCSegment` holds the design: a network of typed records that refer to
each other (`dc`). Claims are marked as above; *(seen)* layouts are those
the test files have and the import reads, not checked against the bodies.

- **References** *(verified)*: a u32 whose bits 0–30 are a one-based
  record index (0: none); bit 31 is a flag that does not change the index.
- **Header** *(verified)*: most records start with u32, u16 id, u32 next
  reference, u32 flags, u32 context reference (the collection the record
  belongs to) and u32 node number (22 bytes); from major version 28 a u32
  more (0) comes before the context (26 bytes).
- **Prefix** *(verified)*: the records of parameters, sketch entities,
  transforms, profile selections and feature properties (enumerations,
  booleans, lists, directions, fillet edge sets) have a prefix after the
  header: from major 25 eight bytes (an i32, −1 or, in the parameter and
  booleans of a fillet edge set, their index; and an i32 release stamp,
  2706 in major 27), in major 24 the first i32 alone, and none in majors
  20 to 22 (major 23 not seen; read as 24). The rest of these records is
  the same in every major seen: every parameter of both corpora reads and
  evaluates to its stored value, every sketch transform is a rotation.
  Below, "the prefix" stands for it.
- **Lists** *(verified)*: u16 kind, u16 `0x3000`, u32 count; a list that
  is not empty then has two u32 (kind 2) or two u16 (kinds 3 and 8) and
  the u32 items (references). Maps (kind 6) have pairs.
- **Texts**: a u32 count of UTF-16 code units.
- **Document and browser** *(verified)*: the first record (`164d8790…`)
  names the part and refers to the part's root collection (`634d8790…`),
  which is the context of the part's parameters. Label records
  (`2ba4482b…`) name records for the browser: the labelled record, a list
  of the records shown under it and the name, then a 16-byte class id
  that tells the kind of feature. The document's label lists the
  top-level nodes in the browser's order: folders, work features,
  sketches, features and the end of the part (`24fd418f…`).

### Parameters

`params`; every parameter of the test files' parameter tables reads and
evaluates to its stored value *(verified)*.

- **Parameter** (`264d8790…`): the header, the prefix, the name, u32,
  the unit, the expression (references), the nominal and model values (f64,
  cm or rad) and four bytes. **Integer parameter** (`dfd51dbb…`, a
  pattern's count): the header, the prefix, the name, u32, the nominal
  and model values (u32). Names `d<n>` are model parameters, others user
  parameters; `RDxVar<n>` are internal variables of the features (not
  imported). Parameter records in other collections than the part's root
  (annotations, reference dimensions, a hole's own table) are not the
  part's and are not imported.
- **Unit** (`fd79a7f8…`): u32, u16, the numerator and denominator lists
  (kind 3) of base units, a byte and a reference. **Base units** (u32, u16,
  f64 magnitude, f64 factor): a length or angle in internal units
  (`bc204162…`, `f0cd305c…`: a parameter's unit) or a display unit:
  metre times the magnitude (`f579a7f8…`; 0.001: millimetre), inch
  (`f679a7f8…`), degree (`f6cd305c…`), unitless (`23009d5f…`,
  `22009d5f…`); foot (`f779a7f8…`) and radian (`f2cd305c…`) are taken from
  the public notes, not seen.
- **Expressions**, trees of records that start with u32, u16 and their
  display unit: a number (`047aa7f8…`: f64 in internal units, u16, u32),
  a parameter (`057aa7f8…`), `+ - * / % ^` (`067aa7f8…` to `0b7aa7f8…`:
  two operands), negation (`0c7aa7f8…`), parentheses (`0d7aa7f8…`: one
  operand; *seen*, the values agree), functions (`037aa7f8…`: a list
  (kind 2) of the operands, then a u32 code). Of the codes, 1 (cosine,
  `SW / cos(30 deg)`) and 26 (three operands: a value and the numbers 1 mm
  and 1 deg, whose value is the first operand's in all 16 such nodes of
  the older parts) are *(seen)*; the others are read in the order of the
  functions in the expression language's public documentation, which
  codes 1 and 26 fit (sine 2, tangent 3, the inverse and hyperbolic
  functions, square root 13, sign, exp, floor, ceil, round, abs, max, min,
  ln, log, pow 24; 25, a random number, is not translated): an expression
  whose code was guessed wrong does not give its stored value, and the
  stored value is used. The test files use numbers,
  parameters, products and negation. The expression is written in MitCad's
  language with each number in its display unit (at most 12 significant
  digits: `6 mm`, not `6.000000000000001 mm`), evaluated here against the
  stored value, and given to the import, which evaluates it again; an
  expression that does not translate or evaluate to its value becomes the
  stored value. A parameter named in an expression whose own expression
  is not read counts with its stored value, as the import gives it.
- **Computed parameters**: a parameter whose header flags have the bit
  `0x01000000` has a value the model computes (a thread's minor radius
  from its table, a distance an extent gives): its expression, a number,
  need not give it *(seen: 18 of the 67 such parameters of 902 parts have
  an expression of another value; no parameter without the bit has)*. The
  stored value is imported (reported as computed by the model), and the
  expressions that name it are evaluated with it.
- **Parameters nothing uses**: a parameter whose expression does not give
  its stored value, and which nothing imported uses (no sketch dimension,
  feature or work plane that comes in with it, nor another parameter's
  expression), keeps its stored value and the report says so (`unused`)
  *(seen: the outputs of a library feature, whose feature is not
  translated, `18` stored and `1` as their expressions; unitless parameters
  of a part made from a table, `0` as their expressions, that only
  annotation records not decoded (`3018a439…` of `921f10ae…`) refer to)*.

### The timeline and the history states

`design`. The timeline is the document label's top-level nodes in order;
a feature's sketch (a label child) comes right before it, a work plane's
base planes before it. A feature's record type is `914d8790…` (most),
`44326720…` (rectangular pattern), `31719706…` (circular pattern),
`b5a9d9fa…` (mirror), and others for sheet metal; its kind is its label's
class id. Nodes after the end of the part are left out.

The **history state table** (`b6212145…`) lists for each feature its node
number (bit 31 set) and, after a fixed 21-byte mark, the id (i32) of an
ASM history state: first the state before each feature, then the state
after it *(verified: every feature of the test files names a state of the
B-rep record's ASM history, apart from 11 of 355)*. The state after it is
the one its operation made: the import checks the feature against it
(`ipt_history.rs`), as the `.f3d` import checks against `result_no`.

- The mark's 13th to 20th bytes are not fixed: a u16 and a byte, each
  followed by zeros, are `08 01` and `02` in most parts, `07 01` and `01`
  or `09 01` / `0a 01` and `02` in others (the same in every entry of a
  record). Read with the fixed `08 01 … 02`, 128 parts of the older
  corpus below had no feature with a state, and every item there was
  checked only against the states between its neighbours' *(seen)*.
- A sheet-metal feature (a face, a flange) is a group record whose label
  children are features of their own (plates, bends, corners), and the
  table names those: the last child's state is the group's.
- A feature suppressed in the file keeps the ids it had while its states
  are gone from the history, which holds states with lower and higher ids
  (ids are not reused; its "before" is the state where it would go, often
  the last one). Its result is not kept: the import skips it (*the file
  keeps no result for it*) instead of trying its definitions *(seen: 351
  items of the older corpus, none of which gave a state, and the history
  showed no change by any)*.

A body an operation deleted (a combine's tools, joined into its target)
is no top-level body of the B-rep record any more: its last copy is the
`before` of the state's bulletin with no `after`
(`deleted_bodies`). Rolled back past that state, the body is
there again with the copy as its entity, so the states before it hold it
*(seen: every combine of the test files that consumes its tools; before,
the features that made the tools had no state with their bodies)*.

The import reads the states once for all its tries (`states`, the
watchdog's tries after a hang share them): every state's bodies are
converted to find the distinct ones, but only the step of the history
each distinct body was converted at is kept, and a body is converted
again and built when the replay asks for its state. Kept converted, the
933 states of a part of one body of 14,884 faces took more than the
corpus run's 6 GB before the replay began; now the import peaks at
0.9 GB. A design without timeline items (parameters only) reads the
stored bodies alone. The replay runs under the `.f3d` import's memory
guard (mitcad#80): when memory gets tight it drops the bodies it built
for the states behind it, and when it is low it cuts the definition
being evaluated short, before an allocation fails. When no import thread
can be started (its stack cannot be reserved beside a try the watchdog
gave up), the try runs on the calling thread instead of failing: the
first one without the watchdog, one after a hang with the stored bodies
and without the timeline. After a hang, the item that first ran the
process low on memory takes the file's bodies at once in the next try,
as the items whose definitions gave no state do.

### Sketches

`sketch`. A sketch (`114d8790…`): the header, i32, u32, the list (kind
8) of its entities, constraints and dimensions, its transform, its normal
(`40df52ce…`: three f64 at its end), two u32 and a list.

- The **transform** (`184d8790…`): the prefix, an optional u32 `0x203`,
  a u16 mask of the elements that are ±1 and a u16 mask of those that are 0
  or −1; the others follow as f64, row by row. It maps the sketch to the
  model (cm): x and y axes, normal and origin in its columns *(verified)*.
- **Entities**: the prefix, u32 flags (0x40 construction, 0x80000
  centre line, 0x40000 projected from the model), the sketch; a point's x
  and y (f64, cm); a line's list of its two points, then points that lie
  on it; a circle's list of its end points (none for a full circle; an
  arc's two ends, then points that lie on it, checked to be on it), and
  at its end the centre point, the radius (f64) and a byte (1: full). An
  arc runs counter-clockwise from its first end to its second *(verified:
  every arc of the test files that meets a line tangentially)*. An
  ellipse (`60d40745…`): its list of end points as an arc's, a list of
  points on it, then at its end the centre point, its major axis's
  direction (two f64), the major and minor radii and a byte (1: full)
  *(seen)*; an elliptical arc is taken to run as a circular one. A
  segment (`46737b31…`): the list of its two points and no geometry, a
  straight line between them (one from a point to itself is nothing)
  *(seen)*. A centre
  line bounds no profile (MitCad's do): it comes in as construction
  geometry. The flag 0x40 is not construction alone: of the lines with
  it, 190 lie off and 9 on the profiles the features of the test files
  selected, but in the older corpus 1,639 lie on them (whole sketches of
  0x40 lines bound profiles). A curve flagged 0x40 that lies on a loop of
  a profile measured for its sketch (`profile`) bounds a profile, and
  comes in as a curve: on the loop all along, or along a part of it with
  the rest outside the profiles (a pattern's original that its copy's
  edge trims, a long line that a profile's sides end on: one of the
  loop's edges lies on it), unless that part only separates two
  profiles that one feature selects together (a line through a circle
  whose halves one cut takes: kept, it would split the feature's faces)
  and bounds no other feature's profile (then the first feature's
  profiles are a union of MitCad's regions, which the import matches by
  area and centroid); the others as construction geometry *(seen)*. A curve drawn twice (the same ends, centre, radius or
  angles) bounds profiles once: the later copy comes in as construction
  geometry *(seen)*. A line flagged 0x40 along an edge of a profile's
  loop with the profiles on one side only is that profile's side even
  where it runs on through another profile (a circle across it whose
  parts one feature takes, a circle it runs into) *(seen)*. Whether
  other lines of the sketch already bound a flagged line's part of a
  loop is decided from the loop's edges along it, not from samples of
  the line: a corner of a profile 0.25 mm long on a flagged line of
  370 mm, beside a short sketch line along its other end, lay between
  two samples, and the region stayed open (mitcad#235: `8d946c8f`
  Extrusion6, and Extrusion9 after it).
- **Sketch blocks**: a placed block (`4c12f14c…`) is an entity of the
  sketch that refers to its definition (`c4aaa486…`, laid out as a sketch
  without a transform) and ends with the placement, two rows of three
  f64 (a rigid motion of the sketch plane, cm) and a u32; the block's
  points as the sketch sees them (`7e71a918…`) refer to the placement and
  the definition's point, and `cf8d56f2…` groups them. The definition's
  points and curves come in moved by the placement and fixed; the
  sketch's constraints on the block's points apply to them *(seen: one
  block in each of six parts)*.
- **Constraints and dimensions**: the header, i32, a reference, two maps,
  the parameter, the entities. Coincident `944d…`, parallel `954d…`,
  perpendicular `964d…`, tangent `974d…`, horizontal `984d…`, vertical
  `994d…` (`…8790d011f8d10008cabc0663dc09`), collinear `e07b9a5a…`, equal
  length `e0c281c6…`, equal radius `d07d2c44…`, symmetric `107cdd0d…`,
  midpoint `bf70e821…`, two points on a horizontal line `c0a3558f…` and
  on a vertical line `5052da64…` (by their geometry *(verified)*); a
  circle's centre `008c10e1…` (implicit in MitCad); a fixed point or
  curve `b07b170f…` (one entity, *(seen)*); a rectangle `0ead987a…`
  (after the parameter its four sides in order, then its corners: its
  sides at right angles, three perpendicular constraints; its frame
  `08db93dc…` and anchor `5f33ec8f…` add nothing the sides do not
  *(seen)*); a driven angle of three points `845c3bbf…` (the origin
  point and a point twice, its parameter computed: it holds nothing
  *(verified: the 37 of the corpus)*). Dimensions: distance `58850511…`
  (aligned *(verified)*: points, a point and a line, parallel lines),
  horizontal and vertical distance `00c0ac00…`, `40ff8336…`, diameter
  `e096df74…`, radius `00b71b67…`, angle `100a0d59…`; a dimension whose
  parameter is in the part's table drives it. A distance between
  concentric circles or arcs is the difference of their radii *(verified:
  141 of the 152 distances to circles of the older parts)*, MitCad's
  concentric circle dimension; another distance to a circle or arc (to
  its edge *(seen)*) has no counterpart in MitCad and is left out. A
  distance from a point or line to a centre line whose value is twice the
  geometry's is a diameter about it (a revolution's profile), MitCad's
  linear diameter *(seen)*.
- What holds nothing *(seen)*: a constraint between geometry projected
  from the model (flag 0x40000) and the end and centre points of
  projected curves (MitCad keeps them fixed, and would take the
  constraint for a contradiction), a coincidence of a curve and its own
  end point, and a line whose two ends are one point, with what
  constrains it: all left out; a dimension between projected geometry is
  driven. A line parallel or perpendicular to a circle or arc is stored
  either way round; perpendicular to it, it runs along its radius: the
  circle's centre on the line.
- **Groups** (`groups`, *(seen)* in majors 21 to 27): polygons and
  patterns of sketch entities. A group (`38485352…`): the prefix, a u32,
  the sketch, its entity (a polygon's or circular pattern's centre point,
  a rectangular pattern's direction line, one group per direction) and
  its pattern record (none for a polygon). Its members (`1f89dbfb…`, in
  the sketch's list as constraints) have after the parameter two
  entities, the group's entity, the group twice, a u32 and the pattern
  record: in a polygon two adjacent sides or corners, in a pattern a copy
  and its source (the original, or the copy before it). A pattern record
  (`e6ded4b5…` circular, `637dc972…` rectangular): the header, an i32 −1
  (in every major), a map (kind 6) of the entities to their instances
  (`9b68a675…`), the sketch, and among the references after it the
  count (an integer parameter) and the angle, or per direction the
  direction line, count and spacing in the directions' order. They come
  in as the dump's polygon and pattern constraints (the corners in order
  around the polygon; a pattern direction turned to the side where its
  copies lie). A circular pattern whose copies lie on their originals
  (about the original's own centre) holds each copy on its source
  instead: a point coincident with its source point, a circle or arc
  equal to its source (lines follow their end points) *(seen)*.
- **Offsets** (`groups`, *(seen)*): an offset record (`bc144b4e…`, in the
  sketch's list as a constraint) has after the parameter two pairs of a
  curve and its offset, neighbours along the chain. The pairs joined by
  the records make the chains, each the dump's offset constraint with its
  distance (lines' or concentric arcs' distance) and the dimension between
  a curve and its offset, which drives it.
- **Texts** (`text`, mitcad#414) *(seen: 9 texts in 4 parts, all in a
  frame)*. A text (`013cbb1f…`, in the sketch's list as an entity): the
  header, the prefix, its anchor (x, y, z, cm), its angle (radians), a
  u32, its alignment (u32: the low two bits 1 left, 2 centre, 3 right; the
  rest 2 for the frame's top: 9 and 10 seen, the anchor the frame's top
  left corner or the middle of its top side in the text's own axes), a
  u32, 21 bytes, an f64 (0.065 in all), the string, the font table entry
  it starts with (u32), 12 bytes, an empty list, the list of its glyph
  contours, its frame's box in its own axes (two points: the anchor's x
  to it plus the frame's width, the anchor's y less the frame's height to
  it), a u32, three bytes, the sketch, an empty list and the string again.
  The string's font changes are codes `\F<n>;` naming entries of the
  part's **font table** (`01691837…`, in the application segment: after
  a u32 and a u16 a list (kind 2) of entries: index (u32), a u16, weight
  (u16: 400, 700), italic (a byte), a byte, size (f32, cm), width factor
  (f32: 1, 0 in some entries, 0.77 to 0.9 in others), family (a text), 11
  bytes); lines break at `\n`. Its frame (`7f341025…`, in the list as a
  constraint): after the prefix the corner point of the frame's
  construction rectangle at the text's top left (in its own axes) and the
  text. Its **glyph contours** (`cb63f719…`): a 22-byte header whose last
  u32 counts its pieces, the pieces (a u32 kind: 0x2a a B-spline, two u32,
  the NURBS data of a sketch spline with x, y, z points, an f64, two u32
  and its parameter range; 0x17 a line, its start and the vector to its
  end), the sketch and the pen position of its character on the baseline
  (model coordinates, cm): the font's outlines exactly, as quadratic
  B-splines and lines, each contour closed.
  A text in a frame in one font of the font's own width comes in as
  MitCad's sketch text: the string, the family, bold from weight 600,
  italic, the size as the capitals' height (`cap_height`), the frame
  (`frame`, from the rectangle's four construction lines), the alignment,
  `pen_align` and `ink_top`, and the lines' spacing (`line_spacing`)
  measured from the contours' baselines. What the contours show *(seen:
  `c2ed6f98` and `b266c91f`, two lines in Tahoma bold of 1.5 mm in a
  17.2 × 8.1 mm frame, centred; two single-line texts of `11f1b478`,
  turned a half turn, aligned left)*: the size is the capitals' height
  (an X and an L 1.500000 mm high; the contours' control points are whole
  units of a grid of 1234 units to the capitals, to 0.0004 units); lines
  are centred or aligned by their pens (the first line's pens, from the
  first one to the end of the last advance, centred to 3e-6 mm), not by
  their outlines; the frame's top meets the first line's highest outline
  point (an 8's top 1.534 mm above the baseline, an S's 3.154 mm), not the
  capitals' top; the second line lies 2.46971 mm (1.6465 capital heights)
  below the first, which is not the font's ascender plus descender over
  its capitals' height as MitCad lays out the `.f3d` files' texts, hence
  the measured spacing. MitCad lays the text out from the installed font
  (the scale from the top of its A); the file's glyph contours stay in the
  dump (`_ipt.glyphs`, sketch coordinates). For a computer without the font
  (mitcad#416; Tahoma is not installed on the Linux test machine) the text
  also keeps them as its stored glyphs (the dump's `glyphs`, the model
  text's `glyphs`): each character's contours from its pen in the text's
  axes, the B-splines split into their quadratic pieces at their knots,
  and its advance to the next pen; a line's last advance ends where the
  centring about the anchor ends the line (`c2ed6f98`'s 8: 1.3140200 mm,
  its other digits 1.3140195), or at the outline of a line aligned left.
  The decoder checks that the contours lie as the text's layout puts them
  (one pen per character that is not white space, pens on baselines the
  line spacing apart, a line starting at or centred on the anchor, the
  frame's top at the first line's highest outline point, to 1e-5 cm) and
  leaves them out otherwise (`_ipt.glyphsLeftOut`). Without the font the
  model lays the text out from these outlines instead of Droid Sans, so
  the text keeps the file's shapes and stays an editable text: a new
  frame, height or alignment re-lays the stored outlines, a new string or
  font drops them (commands.md, sketch `texts`). Left out with their reason: a text
  in several fonts (`f5e9d2d7`: lines of 1.5 and 3 mm in one text, and no
  glyph contours), a font with a width factor (`11f1b478`: 0.77; MitCad's
  texts keep the font's widths), a text without a frame (not seen); the
  outlines below then come in for it.
- Not decoded (the sketch comes in partial, these left out): sketch block
  instances (`4c12f14c…`, their geometry `7e71a918…` and constraints
  `cf8d56f2…`), the texts left out above, and splines through points
  (`46737b31…`: the list of the
  points they pass through, not the curve; the curve through them is the
  authoring application's own fit) and their offsets (`47d9553e…`: the
  spline, its two end points and the distance) with what constrains them.
  What such a sketch draws comes in as the outlines of the profiles the
  features selected in it (mitcad#235): the selections' wire bodies keep
  each region's boundary exactly (a glyph's B-spline pieces, an offset
  spline), and their edges that lie on none of the sketch's lines, arcs
  and circles become fixed curves (lines, and B-splines trimmed to the
  edges' ranges; a circle's arc or a whole circle as an arc or circle, an
  ellipse's arc as rational quadratic pieces) through
  fixed points, each shared edge once. The features then find their
  profiles (*seen: a text cut 0.1 mm deep and a pocket bounded by an
  offset spline, both verified against their history states*); a text no
  feature selects stays out. A sketch that draws only construction
  geometry gets the same (`01fb13c9` Extrusion8: construction lines about
  a hole whose circle the selection holds, cut with a 45° taper; the
  circle comes in as a fixed circle, verified).
- **Splines** (`d42f37f9…`) *(seen: projected from the model, in 8 parts
  of the older corpus)*: the entity's list of its two end points (one, where it starts and ends, for a closed spline, whose first and last poles are that point), other
  lists, then its NURBS data twice: u32 degree, f64 tolerance, the knots
  as an array (u32 count, u32 capacity, u32 8, the f64), eight bytes, the
  control points as an array (u32 8, u32 count, u32 capacity, u32 8, two
  f64 each, cm). The reader finds the data by that form and checks it (knots
  that do not decrease, as many as the control points and the degree and
  one, the first and last control points on the end points); the spline
  comes in as a fixed spline (MitCad's splines of stored geometry). A
  spline's **control polygon** (`9d858c5d…`, in the sketch's list as a
  constraint: after the parameter the spline, its control points and the
  lines between them) *(seen: 22 records in 6 parts, each point on one of
  the spline's poles)*: a spline drawn so comes in as MitCad's spline of
  those points (its poles matched to them by where they lie), and the
  polygon holds nothing more; a projected spline's polygon, all of
  projected geometry, holds nothing.

### Features

`features`. A feature (`914d8790…`): the header, i32, u32, a reference
list (kind 2) of its properties by slot, u32. Properties are records:
parameters, enumerations (`28be9a72…` operation: 1 new body, 2 cut, 3
join, 4 intersection; `297d6392…` extent; the prefix, u16 kind, u16
value), booleans (`284d8790…`: the value is the last byte), directions
(`40df52ce…`), lists.

| Kind | Slots and records | Decoded |
|---|---|---|
| Extrusion | 0 operation, 1 profiles (`91739422…`), 2 direction, 3 reversed, 4 distance, 5 taper, 6 extent (1 distance, 2 symmetric, 5 through all, 7 up to a face or plane, 4 up to the next face), 7 symmetric (set in every symmetric extent, and in 9 of 279 through all: through all both ways); up to the next face, 24 the body it reaches (a list `83aadad5…`: all 48 such extrusions of the older parts); up to a face, 18 the face's surface (`f0801215…`: after the prefix 16 zero bytes, a byte 1 and a u32 kind; `0x19` a plane: a u32, its origin (cm) and x and y axes, both across the extrusion in all 130 such records; `0x49` a cone: two u32, the cosine and sine of its half angle, a point on its axis (cm), its two radii, its axis and a reference direction: 8 cylinders) and 13 a boolean (`fd24893d…`, not decoded), or 11 a work plane (4), or 25 a work point (2, not decoded: both suppressed in their files); two sides, 27 the second side's distance, 28 its taper and 29 its extent (1 a distance: both such extrusions) *(seen)* | all; two sides as MitCad's two-sided extent; the next face as the first face of that body reached, the face as its plane or a cylindrical face on that cylinder, which the import looks for |
| Revolution | 0 operation, 1 profiles, 2 axis (a work axis `896cf08e…`: a point and a direction before its last byte), 3 extent (1 an angle, 3 a full turn *(verified)*: its angle parameter keeps a value it does not use, 0 or the angle it was made with), 4 angle, 5 direction (an enumeration `c26aa0c7…`, not decoded) | the axis as a sketch line on it or an origin axis, else the work axis fixed where it is |
| Thread | 0 its faces (named by the file's topological naming, not decoded), 1 full length (a boolean), 3 length and 4 offset of one that is not, 6 modelled (a boolean), 10 its ends (`9984474c…`: after the prefix two points, cm, on its cylinder's surface or axis, where it starts and ends); its size in a record of its own (`1f7e08a4…`, its header's context the thread: after the header a reference, then texts: the nominal size, the designation `M3x0.5`, the thread type, an empty text, a u32, four empty texts, the class `6g`, then its diameters' limits and pitch as texts in the saving machine's number format) *(seen)* | all; the face as the cylinder through both ends, which the import looks for |
| Coil | 0 operation, 1 profiles, 2 axis (a work axis), 3 against the axis (a boolean: the coil runs against the work axis' stored direction), 5 its type (`b80cb14f…`: the prefix, u16 kind, u16 value: 0 pitch and turns, 1 turns and height, 2 pitch and height; 3, a spiral, not seen), 6 pitch, 7 height, 8 turns, 9 taper, 10 to 15 its ends (a boolean: flat, else natural; the transition angle; the flat angle; for the start, then the end: the one coil of the older parts with a flat end gives its history state so), 19 its bodies (`ae70680e…`, as an extrusion's) *(seen)*; the parameters its type does not use keep values, as do a natural end's angles | all but its hand (the import tries both, the direction slot 3 gives first), as MitCad's helix; the axis a line of its sketch on the work axis, an origin axis, else the work axis fixed where it is (a cylinder's axis: 46 of the 85 coils of the older parts, all of which fell back before); a taper as the growth `pitch * tan(taper)` outwards (the one tapered coil of the older parts gives its state so; inwards is tried next), flat ends as the helix's ends, beyond the turns at the full pitch (mitcad#235) |
| Hole | 0 form (`117ccd43…`: 0 drilled, 1 countersink, 2 and 3 counterbore), 1 diameter, 2 depth, 3 counterbore or countersink diameter, 4 counterbore depth, 5 countersink angle, 6 drill point angle (0: flat), 7 its sketch points (`b1226593…`), 8 placement (a transform), 9 extent (1, 5, 7), 16 direction | position: the sketch points in the sketch (label child) the feature is on, else the placement's origin; up to a face as through all |
| Fillet | 0 edge sets (`dae9481b…` of `1641d6aa…`: edges, radius, selection, a boolean), 11 form (`2788f278…`, 0 edge fillet) | radii; the edges are named by the file's topological naming, not decoded: the import finds them from the history state. The boolean (not a boolean record in the parts of `ipt.corpus`) is false in every fillet of the older parts below, while their history states show whole tangent chains rounded from an edge inside them, so it is not the tangent chain option: the sets follow tangent chains |
| Chamfer | 0 edges, 2 distance, 3 second distance, 4 form (`3200aa7d…`: 0 equal, 1 distance and angle, 2 two distances), 10 angle | sizes; edges as for fillets; the face a distance and angle (or the first of two distances) lies on is not read: each definition is tried with the other face next to it, and the history state chooses (mitcad#235) |
| Rectangular pattern, circular pattern, mirror | after the feature's list: u32, the list of the copied features, then properties as references at no fixed offsets: counts (integer parameters), spacings and directions (`7b4544a2…`: a flip, a start point, a vector and an end point; or a direction `40df52ce…`, or a path `aa7b97a5…` *(seen)*), the angle and the axis, the mirror plane (a work plane) | all; the copied features in the timeline's order; directions `40df52ce…` and paths below (mitcad#288); other directions are found with the history |
| Loft (`e0ef542f…` label class) | 0 its sections (a list `509e2f31…` of sections `e850314b…`: after the prefix an entity link as a profile selection's, its wire body the section's boundary), 1 operation *(seen: every loft of the test files)*; the label shows the sections' sketches, or records `7ca882b3…` of the sections | the sections as the profiles of the sketches whose planes hold them (the label's, else the latest before it); rails and conditions not decoded |
| Work plane (`42df52ce…`) | its last nine f64: origin, x axis, y axis. Definitions (each the header, an i32, the plane, then references): offset `c0d8280d…` (the plane it is offset from, the offset parameter), mid plane `560a6ccb…` (the two planes), at an angle `8af4e674…` (the work axis it turns about, the plane it is at an angle to, the angle parameter, then data not decoded), through three work points `8a924209…` (the points), through two work axes `5abeff8a…` (the axes), of a work point and a work axis `62b73858…` (through both, or through the point normal to the axis: the plane's geometry tells), parallel to a plane through a work point `63b73858…` (the point, then the plane; the plane's origin is the point) and through a point of the model's geometry `eea9eab3…` (the plane, then an edge `4a068a52…` not decoded, and two f64) *(seen)*; a work point `3edf52ce…`: after the prefix 12 bytes (up to major 18; 16 from 24), its position (three f64, cm) and two empty lists *(seen)*; not decoded: `315417a7…` (a work point and an edge: normal to the edge at the point, by the plane's geometry). Planes the file keeps fixed: those no definition names (their flags after the prefix have `0x100000`; 33 of the 49 fixed planes of the older parts, among them planes of parts made from other formats), those of a definition `118e11a6…` with no references after the plane (7); `61b8480c…` defines a plane off the timeline as another one (its first reference, then the plane it copies) | offset, mid and angle planes (a plane off the timeline by its geometry: the import looks for the face in it); planes through three points, two lines, a line and a point, or normal to a line through a point, the work points and axes as the origin's or fixed where they are (work points and axes are not imported); parallel ones as offsets from their plane to where the point puts them (the point fixed where it is); the others fixed where they are |
| Combine (`336723c0…`) | 0 the target (a list of bodies `83aadad5…`, one body), 1 the tools (a list of bodies `ae70680e…`), 2 operation (2 cut, 3 join, 4 intersection), 3 keep the tools *(verified: every combine of the test files)* | all |
| Split (`baa87183…`) | 0 its kind (`1990e1b8…`: 2 split faces, 3 split bodies), 2 the faces to split (a list of face names `f1b2bc24…`), 4 the tool's sketch profiles (as an extrusion's), 5 a work plane as the tool, 7 the bodies (`83aadad5…`) *(seen: 77 face splits by sketch profiles, one body split by a work plane)* | kind, tool and bodies; the faces are found with the history |
| Shell (`12963cb8…`) | 0 direction (`e469a1b3…`: 0 inside), 1 the faces removed (`f1b2bc24…`), 3 thickness, 9 its body (`ae70680e…`) *(seen: every shell of the test files, inside, removing no face or one)* | all but the faces: their number is given, the import finds them with the history |
| Sweep (`05daba90…`) | 0 profiles, 1 the path (a profile selection, or a path selection `473f20fc…` of the same layout), 2 operation, 3 taper; the label shows the profile's sketch and the path's (they may be one) *(seen: every sweep of the test files)* | profiles, operation, taper; the path as the curves of the label's sketches on the selected wire |

**Pattern directions without a pattern direction record** (mitcad#288):
a rectangular pattern whose properties hold directions `40df52ce…` (a unit
vector each, in the order of the counts and spacings) takes them as its
directions; one that follows a path `aa7b97a5…` (u32 -1, a boolean, a
work point `3edf52ce…` at the path's start, u32 n, n segments each of u16,
u32 23 (a line), its start point and its vector (cm), then the end point,
a byte and the item of the selected edge) moves its copies along the
path's first line from its start, reversed when the boolean is set, where
the copies stay on that line ((count - 1) × spacing at most the line's
length). Each of these patterns has four booleans per direction and two
more: none is set in the patterns seen, so which of them reverses a
direction is not known; a pattern with any of them set keeps finding its
direction with the history. The elements the pattern leaves out are a
record `bef06e8d…` among its properties: after the prefix a list of kind 4
(u32 `0x30000004`, the count, 8 bytes when not empty), then per element
two u32, its place along the second direction and along the first (both
from 1); the import makes them MitCad's `suppressed_elements` (`k1 + n1
k2` from 0) *(verified: the three such patterns of the sheet-metal parts,
two with two directions 60 degrees apart and 101 and 122 elements left
out (`fdcb8c30`, the copies that would reach the flanges' bends), one
along a closed path of four lines with none (`b1b53e24`), give their
states)*.

**Bodies**: a solid body (`474d8790…`: the prefix and a u32, the body's
number) is named by its label (`Solid1`). Every feature refers to a list
of bodies (`ae70680e…`, the prefix and a list of body records; a
combine's target `83aadad5…` has the same layout): the body a new body
made, the bodies a join, cut or intersection worked on, which come in as
its participants in parts of several bodies *(seen)*. The first feature
on the timeline to refer to a body made it: a body input names that item
and the body's index among its bodies (`_f3d.producer`,
`_f3d.body_index`, as the `.f3d` import's), by which the import finds it.
A combine whose bodies are not found so (their producer came in with a
later item's fallback) gets its target and tools from the history, as
the `.f3d` stream decoder's combines without inputs do.

A sweep's **path** is the wire body of its selection (as a profile's,
`profile`; an open wire keeps the end of its last edge, whose next
coedge is none or itself). The lines, arcs and circles (a path round a circle *(seen)*) of the label's sketches
that lie on it (their ends and middle within the wire's sampling), in
the order along it, make the path; the sketch with the most of them is
the path's.

**Profiles** (`profile`): a profile selection (`3b2477a4…`) refers to an
entity link (`21750fcc…`: an ASM record of the definitions segment of the
B-rep record's type, a body id and a lump). That ASM file keeps every
selected profile as a wire body (as many as there are selections in the
test files *(verified)*); in majors up to 24 as a body of one planar face
instead, whose loops are read the same way *(seen)*. The loops are
sampled, measured in the sketch
(area and centroid) and given to the import, which picks the sketch's
regions of the same area and centroid (each of several alike tried, and
else a union of regions that has them: a line that bounds another
feature's profile splits this one in MitCad's sketch). Selections inside
one another make up their regions by the even-odd rule
(`profile::even_odd`): a region is taken when an odd number of the
selections cover it. Each selection is mostly a single loop, and a ring is
the selection of its outside and that of its hole *(seen: a plate's
outline and a circle in it, both selected, make the plate with the hole,
as its history state has it; four loops, the middle one twice, make the
ring between the outer and the inner one)*; a selection that keeps a hole
and the hole's own selection make the whole. A selection inside another
one that runs along the other's boundary for part of its own (at least a
twentieth) is not a hole of it but adds nothing to it
(`profile::subsumed`) *(seen: a lid's outline and the ring between it and
an inner outline that touches it at two corners, both selected: the
history state adds the whole outline's slab, mitcad#235)*. The distinct loops nest in a
tree, each loop less its children one of the sketch's regions as far as
the selections tell; the regions taken are given with their area and
centroid. Without selections inside one another, a boundary selected more
than once (the regions of coincident curves, as a degenerate pattern's
copies on their original *(seen)*) is given once.

### Topological names

`naming` (mitcad#242). A feature that selects edges (a flange's edges, a
rounding's or chamfer's, a bend's) keeps them as **items** (`82695c37…`):
after the header and the prefix a list (kind 2) of one value, then -1 and
0. The value is the node number of a **reference** (`534d8790…`: the
body, a list (kind 8) of feature nodes, the item, and at its end the
naming table and an index) *(verified: every item of the sheet-metal
parts)*. The **naming table** (`4220e9cc…`, one in the definitions
segment: u32, u16, u32, the next free index, the count, then (index,
reference) pairs) gives the index's **name record**. Name records start
with u32, u16, u32 1 or 5, a list (kind 2, empty), five u32 0, u32 180 and
u32 0; a name made of other names refers to them as table entries (the
table's reference and an index, 8 bytes). The classes seen:

| Class | Name of | Layout after the head |
|---|---|---|
| tags: `205fa956…` (generated), `a5c24506…` (swept), `341b896f…` (flange), `98415d8e…` (blend), `7a260eae…` (hole), `74ed6921…` (cut across a bend), `0a82f490…`, `08a2042e…` (vertices) | an entity a feature made, by the feature's own numbers | the feature nodes (a list, kind 2: count, capacity, `0x10000000`, then the numbers up to a -1; a pattern's copy is named by the pattern's node, the copy's number and the copied feature's node), the count of names it is made in the context of and those entries, then the class's numbers (below) |
| `b619c605…` | a vertex | byte 1, u32 n, n entries: the faces that meet there, then more not read |
| `a6e032bf…` | an edge | its two end vertices, byte 1, a list of feature nodes (when not empty: -1, a count and entries), a byte, u32 2 and the two faces it lies between |
| `180d36f4…`, `1c28b49b…` (swept) | an edge a feature made | its two end vertices, byte 1, the feature nodes and -1, u32 n, n entries each with a byte (the faces it lies between), then its own tag: the feature nodes, -1, the face it was made from, and the numbers of a generated (swept) tag |

The B-rep record's ASM entities carry the same names as **naming
attributes**: `NMx_<class>-…-NamingMatching-attrib` (up to ASM 228), or
`ATTRIB_CUSTOM-attrib` with the class as a text (`INV_NMX_GEN_TAG`,
`INV_NMX_SWEEPGENERATED_TAG`, `INV_NMX_BLEND_TAG`, `INV_NMX_HOLE_TAG`,
`INV_NMX_MATCHED_ATTRIB`; ASM 229) and a 1 before the rest: 43 (44), the
count of feature nodes, the nodes up to a -1, then the class's numbers
*(seen)*. A tag's numbers as the name record stores them and as the
attribute holds them:

| Tag | Record (after the context) | Attribute |
|---|---|---|
| generated | u32 0, u32 a (the feature's number of the entity: a sketch curve's), u32 c (its kind: 20 a side face, 12 and 4 the end caps, 10 and 2 the side faces' top and bottom edges, 18 the edges between side faces), u32 0 | a, 0, c |
| swept | as generated, then seven u32 | a, 0, c and the seven |
| flange | bytes a, b, c, u32 d, u32 f, u32 e, u32 g | a, b, c, d, e, f, g |
| blend | byte a, u32 b, u32 c, zeros | a, b, c, four zeros |
| hole | u32 a, u16 b | a, b |
| cut across a bend | u32 0, u32 a, u32 c, a byte, u32 x, u16 | a, 0, c, 0, x, 0 |

The second number of a generated or swept attribute is 0 in every one
seen and is not compared; neither are the context names (no attribute of
a tag with one holds anything of them).

**Resolving a name** in the history state a feature starts from (the
state table's first state of its node; the ASM history is rolled back to
that state by the state's position in the history, not by its id: a
part's newest state may have the id 0 *(seen)*): the faces of the state's
bodies (the deleted bodies' copies too) whose attributes are the face
names' tags; the edge of an edge's name is the edge those two faces share
(an edge a feature made: the one edge with its own tag, where there is
one), where they share several, the one whose ends are the vertices its
name names (each the faces meeting there). The file marks the entity a
reference last resolved to with an attribute `matched` whose feature node
is the reference's node: a check of the decoding, not used by it.

On the 18 sheet-metal parts of the test files (the items of every
timeline feature and of the sheet-metal groups' plates): every edge name
a flange, corner round, corner chamfer, bend, rounding or chamfer
selects resolves to exactly one edge of the state before it, and each of
the 213 edges the file marks as matched by the reference is the edge
resolved (2 more have no mark; none is marked elsewhere). Not resolved:
the names of earlier flanges' edges that later flanges' bend records keep
(4), whose faces the later state no longer has; the import does not use
them.

### Sheet metal

`sheet` (mitcad#211). A sheet-metal feature of the browser is a group
record, a face (`904bc1c4…`) or a flange (`08dcddc3…`), whose label
shows the sketch it uses and the feature records it is made of
(`914d8790…`, labelled `Plate<n>`, `Bend<n>`, `Corner<n>`); the history
state table names those parts, and the group takes its last part's state
*(seen: every sheet-metal part of the corpora, majors 24 to 28)*. Sheet
cuts (`CutFeature`), corner rounds and chamfers, bends and folds are
feature records of their own. The import makes them the dump's sheet
items (`core/import/SCHEMA.md`, *Sheet metal*), which it replays as
MitCad's native sheet features.

- **The style** (the application segment `PmAppSegment`): the style
  record (`6bc0d610…`: its name, `1:Default`, then its values as
  expressions of `#Thickness#` and `#BendRadius#`, a few u32
  enumerations and the thickness as text) and the unfold rule
  (`3e0b6bda…`: its name, `1:<name>`, then the K-factor as a unitless
  value, `0.440 ul`, twice) *(seen: one unfold rule in every sheet-metal
  part)*. The part's sheet-metal parameters (`Thickness`, `BendRadius`,
  `BendReliefWidth`, `BendReliefDepth`, `CornerReliefSize`, `SeamGap`,
  `MinimumRemnant`, `TransitionRadius`, `JacobiRadiusSize`, `GapSize`) hold
  the style's values with its expressions, in the style's order: the rules
  take the bend radius and reliefs from them by their names, and the
  K-factor from the unfold rule. Not decoded: the style's relief shapes
  (the u32 values 2, 1, 4 after the transition radius and 4 after the
  corner relief size in every style of the corpora: no file varies them),
  and a style other than the K-factor's.
- **A plate** (the face's part, and a flange's wall): slots 0 a path
  selection (`473f20fc…`, the layout of a profile selection: an entity
  link to a wire body of the definitions segment's ASM record), 1 a
  direction, 2 a side enumeration (`69b9505e…`, 0 or 1; a boolean in the
  major 16 part), 3 the thickness, 4 the bend's record (`92596ba9…`), 8 a
  list of dimension records, 9 the corner settings (`83ce7786…`), 10 the
  bend relief (`64880596…`: a boolean, the width, the depth, a shape
  enumeration `720c3e2b…`, the remnant, `SeamGap`), 11 the transition
  (`e9345026…`), 12 the edges (`ce2d9b29…` → `799c7cfa…` → a list of edge
  items `82695c37…`, *Topological names*), 13 an enumeration `051772e5…`
  (0 in every flange), 14 the width extents of the edges (`19dfdc53…`, a
  list of `f922288f…`; below), 15 its bodies, 16 the operation (kind 5: 1
  a new body, 3 joined to the sheet) *(seen)*. The older layout (the
  major-16 part, 15 slots, mitcad#280) has no slots 15 and 16: slot 6
  names its body directly, and the plate makes that body when it is the
  first item to refer to it (as every feature does, the decoder's
  producers), else joins it *(verified: its face and flange give their
  states)*.
- **A face**: its plate's path is the face's profile in its sketch,
  measured as an extrusion's; the material lies along the plate's
  direction from the sketch's plane for side 0, against it for side 1
  (mitcad#242) *(verified: the 17 new-body faces of majors 24 to 28, side
  0, each giving its history state; the two joined faces reached with a
  valid sheet, side 1, lie in their panel's plane and give theirs)*. A path
  of several separate loops (a face of separate regions) gives one profile
  per loop. A face joined to the sheet (operation 3) has a bend part of its
  own whose slot 1 is the bend radius and slot 0 the list of what its
  bend joins (mitcad#280). Where its material lies in a panel's plane the
  import joins it there (`sheet_face` with the body). An empty list: the
  face is joined by no bend; where its material lies in no panel's plane it
  touches no material of the sheet (the body holds it as separate lumps)
  and the import makes its regions new sheet bodies *(verified: the joined
  face of two strips of the corpora gives its state)*. The faces bent onto
  the sheet list two items of another class (`3839a63b…`, the layout of an
  edge item, both naming the face's own plate node), not decoded: those
  faces are not reached with a valid sheet in the corpora.
- **A flange**: its plate's path is a wire across the middle of the edge
  from the stationary tangent line through the bend to the flange's end;
  it runs on the back surface (the side away from the face's sketch) in
  every flange of the corpora, inner or outer. Its dimension records
  (`b1377779…`: after the header the two points measured between, cm,
  then among the references the parameter) are the length (from the
  outer or inner virtual sharp of the bend, or the tangent line, to the
  flange's end) and the angle; the bend part's slot 1 is the bend radius.
  Its edges are read by their names (mitcad#242, *Topological names*),
  each found in the history state the plate starts from and given with
  its ends: every one of the 44 edges of the 34 flanges of majors 24 to
  28 *(verified: each the edge the file marks as matched)*; the 66 edges
  of the 22 corner rounds and chamfers too, but for the 4 of two rounds
  suppressed in their files, whose items have no reference record. The
  path belongs
  to one of them (the first): the import tells the position (the tangent
  line's distance from that boundary: the outer setback inside, the inner
  one outside) and the length's datum from the stored path and points
  (`inferred`: no field of the flange's records varies with them, the
  enumerations of slots 2 and 13 and the bend part's slot 8 included); a
  tangent line on the boundary itself is the position `edge` *(verified:
  the one such flange gives its state)*. A flange on several edges (7 of
  the 35) is one flange per edge with the path's inputs. A **width
  extent** (`f922288f…`: after the header and the prefix the edge's item,
  then the two points the flange spans, cm) has a definition
  (`fe193a84…`, its only other referrer the root collection): after the
  header u32 its type, u32 2, then the body, the item, an internal
  variable (the width), a direction, the thickness, 0, the extent, the
  bodies, 0, two work points and two offsets *(seen: the 5 extents of 3
  flanges, all of type 4)*. Type 4: each offset measured from its work
  point, an end of the edge (on either surface); the import makes them the
  flange's offsets. Two of the extents reach 0.5 mm beyond both ends
  (offsets -0.5 mm): the import gives them as MitCad's width from the
  boundary's middle (half the boundary less each offset), the overhangs
  starting at the bend's moving tangent line (`root` 0) as in the history
  state *(verified: the flange and the later flanges on its overhanging
  wall give their states)*.
- **Corner settings** (plate slot 9, `83ce7786…`): after the header and
  the prefix the references 0 a boolean (true: the style's settings, false:
  the flange's own), 1 `GapSize`, 2 `CornerReliefSize`, 3 the corner
  relief shape (`720c3e2b…`, kind 501: 102 or 103 with the style's
  settings), 4 an enumeration (`e0a8d8af…`, kind 8: 3), 5 an enumeration
  (`720c3e2b…`: 303), 6 `JacobiRadiusSize`, 7 none, 8 a boolean (true)
  *(seen: all 35 flanges)*. With the style's settings, the corners of
  flanges on adjacent edges are MitCad's trim across the earlier bend
  *(verified: the four-edge flange and the flanges on the sides of earlier
  ones give their states)*. One three-edge flange has its own settings,
  relief shape 5: its walls stop t + g/√2 from the outer corner (2.561 mm)
  and each bend ends on a line from that point at the wall's tangent to a
  point 1.5 mm further at the panel's tangent, the panel's corner cut
  between the two bends' ends. Its gap, thickness and bend radius are all
  1.5 mm, so which of them each dimension follows is not known; variant
  files are needed (the private reference-data ticket). The import refuses
  a flange on several edges with settings of its own, with that note.
  Needed (mitcad#288; the reference application of that ticket cannot open
  `.ipt` files, so they must be `.ipt` parts saved by the authoring
  application): a flange on two or three meeting edges with corner
  settings of its own, once each with only the gap, only the thickness
  and only the bend radius changed (all three different, such as gap 0.5,
  thickness 1 and radius 2 mm), and once for each corner relief shape of
  slot 9's enumeration, each part with its history state.
- **A cut**: a feature record of the extrusion's layout (slots 0
  operation 2, 1 profiles, 2 direction, 3 reversed, 4 distance, 6 extent)
  *(seen)*, and two booleans of its own in slots 31 and 33. Slot 31 true
  (one cut of the corpora, mitcad#280) cuts the profile from the developed
  sheet across the bends: the import makes it a developed `sheet_cut` with
  every bend opened, the panel whose surface holds the sketch staying
  *(verified: that cut gives its state, the side walls' ends removed with
  the base panel's corners)*. Slot 33 is false in every cut of the corpora
  (normal to the face by the defaults' order, not decoded). With neither
  set the import makes it a straight `sheet_cut` (along the sketch's
  normal, through a bend too) when it goes through the sheet; on a body that is no longer a valid sheet it falls
  back, its note naming the item that took the sheet meaning away.
- **Corner rounds and chamfers**: feature records of a fillet's and a
  chamfer's layout (slot 0 the edge sets or edges, the radius or
  distances, the chamfer's form `3200aa7d…`) *(seen: all 22 of the
  corpora)*, read as fillets and chamfers. Each edge set's edges (a
  round's sets are slot 0's `1641d6aa…` records, each's first reference
  its edge list; a chamfer's one set is slot 0) are read by their names
  (mitcad#242) in the state before the feature and given with the set;
  where each is the thickness edge at a corner of a panel's contour, the
  import makes them `sheet_corner_round` or `sheet_corner_chamfer` of
  those corners; on a body that is no longer a valid sheet they fall back,
  the note naming the item that took the sheet meaning away.
- **Bends** (`BendFeature`, mitcad#242): a feature record of the layout
  of a flange's bend part: slots 0 the list of its two edges (items), 1
  the bend radius, 2 the thickness, 3 the bend record, 4 the relief, 7 the
  transition, 9 its bodies *(seen: the 4 bends of the corpora, in one
  part)*. Each joins a side edge of a strip of a joined face to the side
  edge of a flange's wall beside it, parallel to it and 0.8 mm (the
  thickness and 0.1 mm) away: a double bend (two 45-degree bends and the
  straight between them, the wall cut back for it). The strips touch no
  material before it (their face's bend part lists nothing; the history
  state holds them as separate lumps), so the double bend closes no loop
  (mitcad#280; #242 took them for loops). The bend record (`92596ba9…`,
  mitcad#280): after the prefix the references 0 a boolean, 1 the
  thickness, 2 an enumeration (`7267cc3b…`, kind 9), 3 and 4 two
  directions, 5 the bend radius, 6 an enumeration (`7267cc3b…`), 7 and 8
  two lengths, 9 and 10 booleans. The enumerations are 1 and 1 in a
  flange's bend, 0 and 1 in a joined face's bend part, 3 and 5 in the
  double bends; the directions are zero but in those: the bends' axis
  `a` and the direction `d` from the fixed edge's panel towards the other
  panel's plane, `a x d` pointing from the fixed edge (whose panel stays
  whole) away from its panel's material *(verified: the four double bends
  give their states; the fixed edge is also the first of the two in each,
  so which of the two rules the file follows is not told apart)*. Kinds 3
  and 5 are the double bend of two 45-degree bends; other kinds are not
  decoded.
- **Folds** (`FoldFeature`, mitcad#242) *(seen: one, in the major-16
  part)*: slots 0 an enumeration (`c12c2606…`, 1), 1 and 2 booleans
  (false), 3 the angle, 4 the bend radius, 5 the line (a list
  `911b3d83…` of a work axis on the fold sketch's line), 6 the relief, 7
  the body, 8 and 12 booleans, 10 the thickness, 13 the transition, 14 an
  enumeration. Its line is a line of the sketch before it (the work axis
  runs from its start to its end). With the enumeration 1 and both
  booleans false the bend starts at the line and the material on the
  line's right, seen along the sketch's normal, stays; the other side turns
  against the sketch's normal (mitcad#280) *(seen: the one fold, its
  history state; its sketch lies on the outside of a flange's wall, where
  the sketch's normal and the side away from the material are one, so
  which of them the turn follows is not told apart)*. The import makes it
  `sheet_fold` (position `edge`); that fold's line crosses the windows of
  the cuts before it, which the model's fold takes since mitcad#288 (a seam
  per stretch of material) *(verified: the fold gives its state)*.

### Results on the test files

`ipt.corpus` (dev build, 2026-10-08; the outcomes below with the sketch
groups and points on lines, release build, 2026-10-09): 16 parts, 2,785 parameters (2,755
model, 30 user) imported with their expressions, every one agreeing with
its stored value, in the file and in MitCad (1,943 parameter records of
annotations and features' own tables, and the internal variables, are not
the part's). 196 sketches, 344 features with their history states. Every
part's final bodies are the stored ones (22 bodies, all valid); the
features:

| Kind | Parametric | Partial | Fallback | Skipped |
|---|---|---|---|---|
| Sketch | 180 | 16 | | |
| Work plane | 7 | 37 | | |
| Extrusion | 83 | | 16 | |
| Hole | 90 | | 2 | 5 |
| Fillet | 45 | | 4 | |
| Chamfer | 10 | | | |
| Rectangular pattern | 11 | | 3 | 2 |
| Circular pattern | 2 | | | |
| Mirror | 20 | | 5 | 2 |
| Revolution | 2 | | 3 | |
| Face draft | | | 17 | |
| Shell, split, loft, rib, sweep, boundary patch | | | 16 | 2 |
| Sheet metal (face, flange, corner round and chamfer) | | | 25 | |
| Base solid | | | 1 | 1 |

*Partial*: a sketch with entities left out (above), a work plane fixed
where it is. *Skipped*: the history shows no change by it. Fallbacks are
the bodies of the feature's history state.

### Results on older parts

A second corpus of 902 real parts (not public), saved by the releases of
2012 to 2023 (none by the 2019, 2021 or 2022 releases), checked as
`ipt.corpus` does (release build, 2026-10-09, with a hang limit of
300 s). Before the prefix and the layouts above were decoded, the parts
saved before 2021 came in as the bodies of their history states; the
timeline items (parametric / partial / fallback / skipped) by the release
that saved the part ("before": parts that did not open then count as
none):

| Release (major) | Parts | Before | After |
|---|---|---|---|
| 2016 (20) | 3 | 0 / 6 / 26 / 13 | 28 / 6 / 11 / 0 |
| 2017 (21) | 8 | 0 / 0 / 89 / 56 | 118 / 8 / 18 / 1 |
| 2018 (22) | 3 | 0 / 0 / 37 / 9 | 10 / 4 / 31 / 1 |
| 2020 (24) | 780 | 1 / 87 / 4,996 / 2,970 | 4,825 / 279 / 2,471 / 479 |
| 2023 (27) | 95 | 824 / 203 / 328 / 206 | 1,062 / 77 / 295 / 127 |
| others (most did not open before) | 13 | 0 / 0 / 1 / 0 | 102 / 20 / 29 / 0 |

894 of the parts pass every check. Of the others, three that passed
before fail it now only because their parameters are read: their
expressions do not give the stored values (numbers of zero in features
made from a table, a library feature's outputs); their bodies are the
file's.

The repairs of *Bodies the builder repairs* make every stored body of
the corpus a valid one where it was not (a gear's planar face with a
cusp, two parts of solids touching along edges, two with pinched loops
and slits), and the threads of 55 and 81 turns come in (2026-10-09, mitcad#60): four
parts that failed before pass. One part whose replay stopped at its
invalid body now replays further and runs out of the corpus run's 6 GB.

Since the fillet sets follow tangent chains (above) and roundings and
bevels of whole circles that run over a neighbouring face are built as
rings, 1,313 fillets of these parts are parametric (1,154 before), 410
fall back (569) and 152 are skipped; chamfers 283 (281), 41 (43) and 33.
Most fallbacks left are base solids (the parts' imported bodies), fillets
and chamfers the geometry kernel does not build (mostly where a rounding
has to end or meet another at a vertex), extrusions whose sketch has no profile that matches the
selected one, and features whose result is not in the history.

With selections inside one another by the even-odd rule, measured
profiles matched as unions of regions, coil axes by their geometry and
direction, parallel planes and control polygons (release build,
2026-10-10, mitcad#60), the timeline items of these parts are 7,181
parametric, 94 partial, 2,296 fallback and 392 skipped (6,885, 102,
2,594 and 382 before): extrusions 2,184 parametric and 147 fallbacks
(2,094 and 240; those whose result was not in the history 47, from
104), coils 29 and 44 (14 and 62), sweeps 15 and 6 (8 and 13), fixed
work planes 41 (49), and fillets after them 1,329 parametric (1,240).

With the extents up to the next face, up to work planes and the surfaces
of faces, through all both ways, and closed splines (release build,
2026-10-10, mitcad#60), the timeline items are 7,222 parametric, 92
partial, 2,257 fallback and 392 skipped (7,183, 94, 2,294 and 392
before): extrusions 2,220 parametric and 111 fallbacks (2,184 and 147;
those whose result was not in the history 20, from 47). The extrusions
up to the next face left fell back where their profile starts inside
the body they reach (MitCad's extent up to a body did not take that
case).

With MitCad's extent up to a body or a curved face taking a profile that
starts inside the body or on its faces (each part of the profile up to
the first face beyond it) (release build, 2026-10-10, mitcad#60), three
more extrusions (up to the next face and up to a cylinder) come in
parametric.

With MitCad's helix built of faces of at most a third of a turn where a
side of its profile lies on a cylinder of the bodies it is cut from or
joined to (OCCT's booleans failed on faces winding several turns round
the axis against such a cylinder), and a side as long as the pitch moved
0.1 µm off the cylinder so that the turns do not touch in an edge on it
(release build, 2026-10-10, mitcad#60), coils are 61 parametric and 12
fallbacks (30 and 43), in 35 files. Of the coils left, one is tapered,
two run out of memory, one out of time, two do not cut into the bodies
the import offers and six have no definition whose result is in the
history.

Extrusions on two sides (slots 27–29), flagged lines that are a
profile's side with nothing beyond it though they run on through another
profile, profiles touching only at a point extruded as parts apart, and
holes touching an edge outside them kept in a boolean's body instead of
sitting it out (release build, 2026-10-13, mitcad#60, each part run
alone): seven more extrusions come in parametric in seven parts (two
two-sided; two through all that needed both such a line and a circle
touching another profile at a point; two up to a face that needed such
lines; one cut beside countersinks touching the plate's edge), and the
flagged lines bring four more items (three extrusions and a revolution
whose sketches had no profiles, or none the history matched). The
extrusions up to a work point (both of them) are suppressed in their
files. With placed sketch blocks read, the six parts whose one sketch is
a block come in whole (sketch and extrusion parametric; before, the
sketch partial without curves and the extrusion a fallback).

With tapered profiles with holes built as their outline's taper less
their holes', holes touching the outline tangentially sharing its vertex,
coils naming their bodies, and coils' tapers and flat ends (release
build, 2026-10-14, mitcad#235, each part run alone): the three parts of a
ring cut with a 50° taper that closes it (`98292282`, `8c8d1f94`,
`775c9cad`) and the part of the tapered coil with a flat end (`2ac732ee`)
are fully parametric; two extrusions of profiles with a hole touching
the outline (`d43fb190`, `ab53146d`) come in parametric; the part whose
coil cut all bodies first and ran out of memory (`4e6624b5`) imports in
8 minutes with the coil parametric. Left (mitcad#235): cuts along a
hole's wall where OCCT finds no intersection at a point where three
surfaces meet tangentially, two coils whose booleans fail on the stored
bodies, sketch texts and splines through points.

Cuts along a hole's wall through threaded material (dev build,
2026-10-15, mitcad#235, each part run alone). Two causes, both in the
boolean that splits the body's and the tool's faces. Where the section of
a thread's crest leaves the hole's circle tangentially (on a cut's end
face, or on a tool lying on the bore), OCCT ordered the two edges about
their vertex by directions measured at different distances from it and
built a face with a negative area: the cut left the body empty or lost
faces (OCCT port patch 0360). Where the tool lies on the bore of a stored
body whose thread's faces carry 1e-4 to 4e-4 mm tolerances, the section
edges of neighbouring thread faces end a few tenths of a micrometre
apart and OCCT left the tool's pieces inside the body; an imported
design's cuts are now retried within the join tolerance where the exact
cut fails so (`Kernel::cut_within`, model `commands.md`, *Cuts within the
join tolerance of imported designs*). Items before → after: `9452d970`
Extrusion6 (a counterbore through threaded holes) 31 → 32 parametric of
40, its result the file's state (4258.506 mm³, 2800.945 mm²);
`547d59d1` Extrusion7 23 → 24 of 25, no fallback left; `01fb13c9`
Extrusion9 25 → 26 of 31; `c2ed6f98` Extrusion13 25 → 26 of 31.
`26fb87c7` has no such cut left (its two fallbacks are chamfers). Left:
`5e552aad` Extrusion10 (the same cut as `01fb13c9`'s on a 24-tooth part:
the exact cut and every fuzzy value up to 0.01 mm leave the tool's pieces
inside the body) and its Extrusion11 (an invalid shape exactly and
within the tolerance).

The next round (dev build, 2026-10-16, mitcad#235, each part run alone):
`3e0e22df` "top" (a lid's outline and the ring beside it, both selected:
the outline's slab, `profile::subsumed`) parametric and verified, 43 → 44
of 48. A parametric Fillet7 before Extrusion11 left a vertex with a
tolerance of 481 mm where its rounding's edge ends 0.12 mm from it, and
the join on that body built no solid; a dressup's vertex tolerances that
OCCT inflated far beyond what their edges need are now cut down, and a
result that is not valid without them (here the corner left open, faces
that cannot be oriented) fails, so Fillet7 falls back as before and
Extrusion11, Fillet9 and Fillet11 stay parametric and verified.
`0f25d160` Extrusion19 (up to a plane, decoded right: OCCT's exact join
against the stored body's rounded cylindrical wall built no solid at
all; an imported design's join is now retried within the join tolerance
where the solids are lost) 44 → 45 of 61. The texts and splines through
points above: `c2ed6f98` 26 → 27 of 31 (the text's cut verified),
`b266c91f` the text's cut and the pocket of an offset spline verified.
Coil `033448e3` 11 → 12 of 12 (17669.006 mm³ against the file's
17669.005): the pin's bore was one cylindrical face merged from two whose
seams lay a quarter turn apart, its seam stepping along an arc, and
OCCT's cut of the helix removed nothing from it; a cut that removes
nothing from such a target is retried on it rebuilt with one straight
seam (the boundary circle the seam crosses split in the neighbouring face
too), and the coil with its pieces' seams shifted then cuts it. `517c1204`
20 → 21 of 24 with it. Left: `8d946c8f` Extrusion9 (the tool's face lies
exactly in a body face's plane over the cut: OCCT keeps the body's face
whole and adds the tool's reversed, a zero-thickness double layer of 1060
mm², the volume right; the same with every fuzzy value up to 0.01 mm);
`5e552aad` Extrusion10 and Extrusion11 and `d9aec91e` Extrusion5
unchanged.

A thread running out past a blind bore's bottom (dev build, 2026-10-16,
mitcad#235, the part run alone): `d9aec91e` Coil1 cuts an internal
thread whose base is as long as the pitch into a 38.25 mm bore 10 mm
deep, from 2.1 mm below the part to 2.6 mm past the bore's bottom, and
Extrusion5 joins a ring over the thread's end. MitCad's helix moves such
a base 0.1 µm off the bore's cylinder into the bore's void so that its
turns do not touch (the helix section of the model's `commands.md`);
past the bottom that sliver lay in the material inside the cylinder and
the coil cut a slot 0.1 µm thick there (0.024 mm³, two faces of 186 mm²),
which the ring left as a gap (+372 mm² against the file's state). The
fuzzy joins of the round before closed it only in part: the slot was the
coil's, not the join's. The sliver is now cut back to the cylinder beyond
the bodies' faces on it; Extrusion5 is verified (39 → 40 of 46). The
root of the thread keeps the 1 µm ridge the turns are kept apart by
(the file's turns meet in a sharp edge): a helix whose turns overlap in
the void instead gives the file's edge but takes over 20 s a coil and
did not bring Fillet1 in (on the file's own state before it, MitCad's
0.2 mm rounding of the thread's crest removes 20.66 mm³, the rolling
ball's value for its 53° crest over 573 mm, against the file's 25.23).

What the file's rounding of that crest is (2026-10-16, mitcad#235): a
constant-radius rolling ball of 0.2 mm, as decoded. Its stored surface is
a rolling-ball blend whose two supports are offset by -0.2 mm (the convex
side), fitted within 7.6 µm; its sections are circles of 0.2 mm (the
contact lines at 19.4828 mm from the axis, its middle at 19.3722, both
the analytic values for the 53.13° crest at 19.125); and the area the
state changes, -203.97 mm², is the 0.2 mm ball's (-204; a 0.222 mm ball
would change it by -227). The 25.23 mm³ is not the rounding's: the
stored blend's edges lie up to 1.9 µm off its faces, and a boundary not
closed by that much has no exact volume (MitCad's rounding of the file's
own state before it, by face: flanks 1186.31 mm² against the state's
1187.12, the rounding 257.25 against 256.61, all other faces equal; the
replay of Fillet2 on the root shows the same, 51.19 against 56.16). The
import's change check now lets the faces judge a change that the changed
faces' edge slack brings within its limit instead of rejecting it by the
volumes alone (`d9aec91e` Fillet10, 0.14 mm³ against the file's 0.11, its
faces' slack 100 mm³: parametric and verified). Fillet1 itself still
fails earlier: on MitCad's thread the crest is not an edge but the strip
of the bore's cylinder 0.2 µm wide that keeps the turns apart (with the
0.03 mm faces the moved base's corners leave beside it), and no ball of
0.2 mm touches both sides of it. A sharp crest needs the turns to touch
on the cylinder: swept as separate pieces and fused with a tube in the
bore's void the cut gives it (4 s for 6 turns), but rounding it then
fails at the pieces' joints and the part's bottom face; a construction
whose turns meet in one edge is what is left to do. The same strips
stop `01fb13c9` Combine2: the join of the file's bodies before it is
valid, with MitCad's thread insert (487 faces, the file's 10) the union
is invalid (a self-intersecting wire), the insert's helical slot lying
on the hole's wall.

Turns that meet in one edge (dev and release builds, 2026-10-17,
mitcad#235, the parts run alone): a helix whose profile has a side on a
cylinder of its bodies as long as the pitch is now built as the stored
bodies keep threads, one face per profile edge running all the turns, its
turns meeting in one edge on that cylinder (a core beyond that side in the
void, cut away beyond the bodies' faces; the helix section of the model's
`commands.md`). `d9aec91e` Fillet1, the 0.2 mm rounding of Coil1's crest,
is parametric and verified (41 → 42 of 46): the crest is an edge between
the flanks. `d9aec91e` Fillet4 (on Coil2's thread, also built so now)
still falls back, on the item's time where the kernel's walk failed
before. Where such a thread runs out past a blind bore's bottom, its turns
meet on the bore's cylinder in the material there; the face merge after
intersecting that part with the cylinder went back and forth over the
same seams without end until the memory was gone (OCCT port patch 0590
gives such a wire up).

`01fb13c9` Combine2 did not come from the strip: Coil2's groove (pitch
0.5005 mm, 25.6 turns through the wall of a tube of 1.229 to 1.5 mm) has
no side as long as the pitch. Swept in pieces, its run-out past the
insert's end face left faces there of 1e-4 mm² and less, and the union
with the part failed on one of them (a self-intersecting wire; so with the
file's own main body too, while the file's insert unites with MitCad's
main body). Built as a B-rep the groove unites, but the booleans on its
25-turn faces took 10 to 35 s each, over the item's time share. Where the
time went (sampled stacks): the edge-face intersections built a polyhedron
of each piece's whole 25-turn surface (`Adaptor3d_HSurfaceTool` counted
all its 4000 knots), the face merge tested the whole surface for planarity
for every pair of tangent neighbouring faces, and every projection onto
such a face (the booleans' projector of each face, `ShapeFix` in the face
merge) computed distances over a grid of 44 x 12000 nodes, kept in every
thread (7.5 GB for the union); the union also ran on one core. OCCT port
patches 0591 to 0593 bound that work (samples within the face's range,
planarity once per surface, at most 1001 grid parameters a direction) and
the union runs on all cores (2026-10-17, release build, the part run
alone): Coil2's cut 6 s (gave way after its 25 s share before), Combine2
3.5 s (20 to 35 s), so every thread of one profile with a side on a
cylinder of its bodies is built as a B-rep again. `01fb13c9` 27 → 28 of 31,
Combine2 verified (the final body 15332.607 mm³, within 0.0002 % of the
file's).
The other named coil and thread parts are unchanged item by item
(`d9aec91e` 42/46, `033448e3` 12/12, `2ac732ee` 14/14, `517c1204` 22/24,
`9452d970` 32/40, `c2ed6f98` 27/31, `547d59d1` 24/25). `d9aec91e` Fillet4
does not come from the time: OCCT's rolling ball fails on its two
contours along Coil2's thread in under 3 s (a walking failure and an
error), and the item's time goes to the retries after that (shrunken
sizes, the cut-outs cleared, whose distances between 25-turn helix edges
are slow).

Chamfers past the roots of teeth and a thread's crests rounded
(mitcad#235, 2026-10-17, release build, the parts run one at a time):
`01fb13c9` Chamfer1 and Chamfer2 chamfer a pulley's flange rims 2.5 mm at
15°, wider than the flange's top up to the tooth roots, and `c2ed6f98`
Chamfer1 and Chamfer2 a pinion's exactly up to its root circle. The face a
distance and angle chamfer's distance lies on is now tried both ways (the
history state chooses), and OCCT port patch 0650 cuts the chamfer under
each tooth by its seven faces, fills gaps whose ends are the root arcs'
vertices, and keeps the pieces about a closed spine's start. All four
chamfers verify (within 0.05 mm³ of the file's states): `01fb13c9` 28 → 30
of 31 (Thread2: no result in the file), `c2ed6f98` 27 → 29 of 31 (Sketch12
partial on an undecoded record, Thread4 skipped). `d9aec91e` Fillet4
rounds Coil2's crests (0.15 mm, two contours about 700 mm long; it failed
on the file's own state too): patch 0651 starts the walk that begins on a
crest face's seam on the seam's other side and ends the corner's extension
where it fails over the thread's run-out; Fillet4 verifies, 42 → 43 of 46
(Fillet7 out of time, Fillet13 an unorientable face, Fillet5 skipped). Not
fixed: `517c1204` Chamfer2 (22/24), whose two contacts both leave into the
coil's flank before the edge's ends, so both corners fail (a cut across the
whole width at each end is needed, as 0059 does for one contact), and
`9452d970` Fillet7, Fillet12 and Fillet14 (32/40), roundings of 0.2 mm in
a pocket whose wall corners have that same radius on a steep cylindrical
floor: the walk into each corner moves the wall's contact back out of the
corner face at its first step (the true blend there is a sphere patch, as
the file's state has), so they fail on the file's own states too. The
other named parts are unchanged (`033448e3` 12/12, `2ac732ee` 14/14,
`547d59d1` 24/25).

Both left from that round are fixed (mitcad#235, 2026-10-18, release
build, the parts run one at a time). `9452d970` Fillet7, Fillet12 and
Fillet14: round each tab corner of the pocket the ball stands on the
corner's axis while its contact sweeps round the corner; OCCT port patch
0670 makes that piece the ball's sphere between the walls' analytic
roundings instead of walking it (the walk's sections lean with the steep
floor and swept the corner before the guide reached it). All three verify
(within 4e-5 mm³ of the file's states), 32 → 35 of 40 (the other five keep
no result in the file). `517c1204` Chamfer2: patch 0671 cuts the chamfer
across its whole width at both ends by the groove's tip rounding and
flanks grown on past the flat end face, the chamfer ending in a point
where they meet its contact on the bore (one end on the next turn's crest,
the other on the crest's prolongation past the edge's end); MitCad's
chamfer joins the contacts' pieces at the vertices they leave, as the file
keeps one edge there (Fillet7 rounds the crests beside it and failed at
those vertices). Chamfer2 and Fillet7 verify, 22 → 23 of 24 (Fillet3
still falls back: two roundings overlap on the face between them). The
other named parts are unchanged (`01fb13c9` 30/31, `d9aec91e` 43/46,
`033448e3` 12/12, `2ac732ee` 14/14, `c2ed6f98` 29/31, `547d59d1` 24/25).

The three fillets left are fixed (mitcad#235, 2026-10-18, release build,
the parts run one at a time). `517c1204` Fillet3 rounds both edges of a
step 0.5 mm high in a bore with 0.3 mm, one convex and one concave: their
contacts overlap on the step's face, and OCCT runs overlapping roundings
over the face as one only when both have the same kind. The file's state
has an S-shaped section whose convex arc is the plain rounding and whose
concave ball touches it; MitCad's fillet now builds the convex roundings
first and the concave ones on that result when OCCT fails so
(`concave_after_convex` in `geometry/src/dressup.cpp`), the file's state
to 1e-6 mm³. `d9aec91e` Fillet13 rounds a bar's top edges where the bar
meets a bore's wall at the wall's seam, the seam continuing the bar's side
edge up from the corner: OCCT port patch 0690 carries the seam down to the
rounding's contact. `d9aec91e` Fillet7 (the item's time went to the
dressup's retries after OCCT failed in 0.1 s) rounds a recess's rim up to
a rib whose bottom edges are rounded tangent to the part's bottom: patch
0691 takes the contact's crossing with the edge the rib's rounding leaves
the bottom along (the intersection gave the touch at the rim's other
end), and 0692 puts the contact's end, found 5e-6 inside an edge whose
curve ends 3.7e-5 past its vertex (within its tolerance), at the vertex.
All three verify: `517c1204` 23 → 24 of 24, `d9aec91e` 43 → 45 of 46
(Fillet5 keeps no result in the file). With Fillet7 built, two later
items are evaluated on the file's states (warnings, still verified):
Fillet10 (0.3 mm on a ring at the top of Revolution1's cut) fails its walk
on the replay, whose Coil2 thread meets that ring's wall 90° from where
the file's does (the replay is 0.016 mm off the file's state before
Fillet7), and Fillet13 finds the bore's wall in three faces of one
cylinder round the corner (MitCad's cuts keep faces of different names
apart), which OCCT's corner does not join (a face at the end split round
the vertex). The other named parts are unchanged (`01fb13c9` 30/31,
`033448e3` 12/12, `2ac732ee` 14/14, `9452d970` 35/40, `c2ed6f98` 29/31,
`547d59d1` 24/25).

The two items traced further (mitcad#415, 2026-10-18, release build, the
part run alone). `d9aec91e` Fillet13 is built on the replay again: OCCT
port patch 0740 gives the corner whose face at the end is the bore's wall
in three faces of one cylinder round the vertex, each with its own
surface, the curves its arcs lacked on the piece the cut goes on. Fillet10
rounds the foot of the threaded post (the circles of 22 and 23 mm at the
step 22 mm up) with 0.3 mm; Coil2's groove runs out 0.175 mm above that
step, inside the rounding. Its first deviation is not Coil2: the replay's
thread, its run-out and the post after Extrusion7 have the file's
sections at every height checked (the cylinder's seam lies elsewhere, 0°
against the file's 162°, which alone does not matter). It is Fillet2, the
0.3 mm rounding of Coil1's root up to the bore's bottom and the part's
bottom: OCCT's guide past the root's ends turned off the root's helix, and
the rounding's ends were joined by vertices with tolerances of 12 and
14 mm. Extrusion7's cut merged the vertex of the post's foot circle, 12 mm
above the first, into it; the circle's spine was no longer closed, and the
guide over the gap where Coil2's run-out takes the rounding's contact ran
backwards (the walk failed). Port patch 0742 extends the guide
along the root's own curve: the vertices' tolerances are now 2e-4, and
Fillet2 verifies by its faces (within 0.01 mm of the file's). Fillet10
still takes the file's state (45 of 46 as before; Fillet5 keeps no result
in the file): on the replay Fillet4 (0.15 mm, Coil2's crests) rounds the
lowest crest down to the run-out's end, as OCCT follows the tangent chain
past the edges the history state rounds, while the file's rounding stops
331° before it, at 23.554 mm, on a planar end face of 0.006 mm² where the
crest goes on tangentially with no vertex (Fillet4 still verifies, its
faces within 0.016 mm of the file's). The foot's contact then meets
Fillet4's rounding from 214° on instead of 192°, and OCCT's walk over the
gap from the run-out (153°)
fails; on the file's own state after Extrusion7, OCCT's rounding of
Fillet4's edges leaves the same and Fillet10 fails the same way, while on
the file's state after Fillet4 it builds. A rounding that stops where the
file's does needs that end's rule, which this one item does not tell.
With 0740 to 0742 the `.f3d` designs of mitcad#235's and mitcad#343's
rounds keep their counts, each run alone: `5ef2dbd5` 80 of 98 (item by
item as without the three patches), `a9e82a9a` and `de7b8b9d` 181 of 199,
`52ba7a15` and `0c99fb88` 97 of 110, `61526c66` 12 of 14, `db73863f` 128
of 145.

Where the crest rounding stops (mitcad#422, 2026-10-18, dev build, the
part run alone). `d9aec91e` Fillet4 rounds Coil2's two crests, each down to
the thread's run-out (Extrusion7's straight cut along the tangent), where
the crest runs on tangentially into the run-out's crest, which is not
rounded. In the file's state after Fillet4 the upper crest's rounding ends
at the run-out with a corner patch, as OCCT's corner there does. The lower
crest's rounding ends 331° before it, at a face of 0.006 mm² on the plane
of the run-out's lower flank (the stored face's plane, origin and normal):
that plane, tangent to the groove's lower flank along the start profile,
only touches the rounding near the run-out, the crest lies inside it over
the next 331°, and there the crest rises out of it again and the plane
cuts the rounding across. That is the rule: the face at the end, going on
tangentially from the rounding's face, ends the rounding where its surface,
followed back along it, cuts it across; where it never does (the upper
crest's run-out plane), the corner at the vertex stays. OCCT port patch
0790 ends such a rounding at that cut. The replay's face there is
0.006442 mm² with its centre at the file's (0.006438 mm²), the crest stays
sharp down to the run-out, and Fillet10 is built on the replay (verified,
its edges found against the history state), with no base feature before
it: 45 of 46 parametric as before (Fillet5 keeps no result in the file).
The final body is 14053.131 mm³ against the file's 14050.214 (0.02 %, at
most 0.026 mm off), where Base1 gave the file's own body before. The part
takes 324 s instead of 178 s: Fillet4's result lies 2.8e-4 from the
history state's measures (1.9e-4 before; the stored bodies' edge slack
makes their volumes loose, 138 mm³ here), more than the bodies before it
carried, so the import looks for a closer definition (three more
evaluations, 20 s, the first kept), and every item after Fillet10
compares the replay, 0.016 to 0.021 mm off the file's states as before
Fillet10, where it compared the file's own state (7 to 20 s more each).
The other named coil and thread parts keep their counts (`01fb13c9`
30/31, `033448e3` 12/12, `2ac732ee` 14/14, `517c1204` 24/24, `9452d970`
35/40, `c2ed6f98` 30/31, `547d59d1` 24/25), and so do the `.f3d` designs
of mitcad#415's list, each run alone (`5ef2dbd5` 81 of 98, `a9e82a9a` and
`de7b8b9d` 181 of 199, `52ba7a15` and `0c99fb88` 97 of 110, `61526c66`
12 of 14, `db73863f` 128 of 145).

Sketch texts (dev build, 2026-10-18, mitcad#414, each part run alone on
the Linux machine, which has no Tahoma): `c2ed6f98` Sketch12 (a text in
its frame, *Texts* above) comes in parametric as MitCad's text, where it
was partial with the outlines of the regions its cut selected. Its text
is laid out in Droid Sans here, so Extrusion8 (the text cut 0.1 mm deep)
removes 0.587 mm³ against the file's 0.835 and falls back: 29 of 31
parametric before and after (Sketch12 for Extrusion8). `b266c91f`, the
same text: 28 → 27 of 32; Sketch12 parametric, Extrusion8 a fallback, and
Extrusion10 (a cut 1.0 s on the replayed body) gives way after its 23 s
share on the chain that continues from Extrusion8's stored body. The
other named parts are unchanged (`01fb13c9` 30/31, `d9aec91e` 45/46,
`033448e3` 12/12, `2ac732ee` 14/14, `517c1204` 24/24, `9452d970` 35/40,
`547d59d1` 24/25).

The stored glyphs (dev build, 2026-10-18, mitcad#416, each part run alone
on the Linux machine without Tahoma): the text of `c2ed6f98` and
`b266c91f` is laid out from the file's outlines, and the sketch has no font
warning. `c2ed6f98` 29 → 30 of 31 parametric: Extrusion8 is parametric and
verified against its history state (Thread4, which the file keeps no
result for, is skipped). `b266c91f` 27 → 29 of 32: Extrusion8 and
Extrusion10 parametric and verified; left: Fillet4 (its result is not in
the file's history), Sketch6 partial (two undecoded records) and Thread3
(no result in the file). Reopening the saved design keeps the outlines; a
new character spacing keeps them, a new string gives Droid Sans and the
warning. Parts without texts are unchanged (`01fb13c9` 30/31, `033448e3`
12/12, `547d59d1` 24/25).

### What the files do not tell (feasibility)

Without files saved before and after one change by the authoring
application, these are not settled; such features come in as the bodies
of their history state:

- **Edges and faces** that features name (fillets, chamfers, drafts,
  shells, splits): the topological names are decoded for edges
  (*Topological names*, mitcad#242) and used by the sheet-metal features;
  ordinary fillets and chamfers still find their edges from the history
  state (their names resolve on the sheet-metal parts, but the import of
  ordinary features has not been switched to them), and so do shells
  (the faces of their body the next state no longer has, most of their
  points off its faces) and face splits (the faces of the body the next
  state changed that it no longer has whole, no face there of the same
  surface and area); drafts need their faces, so they fall back.
- **Face splits along open sketch curves**: MitCad's split takes the
  curves around a sketch's regions; a split whose curves bound no region
  of MitCad's sketch falls back.
- **Sheet metal** (mitcad#211, mitcad#242, *Sheet metal* above):
  new-body faces, flanges (their edges read by their names), cuts through
  the sheet and corner rounds and chamfers come in as MitCad's sheet
  features, and so do joined faces apart from the sheet, double bends,
  cuts across the bends and the one decoded case of a fold (mitcad#280);
  faces bent onto the sheet (their bend part's items not decoded), flanges
  with corner settings of their own whose dimensions the files do not tell
  apart, cuts with the slot-33 option, other bend kinds and fold options,
  and folds across holes come in as the bodies of their history states.
- **Mirrors of fillets and of patterns**: the copies MitCad makes do not
  give the history state in 6 of the test files' 25 mirrors that are not
  skipped (MitCad's mirror of features, not the decoding: the inputs and
  planes are decoded).
- **Ribs, boundary patches, thickens**: record layouts
  not studied (few in the test files); lofts' rails and conditions.
- **Extrusions from sketch blocks** (logos): blocks are not decoded.
- **Work planes** of the definitions not decoded above (normal to an edge
  at a point), and **work axes and points** (not imported as items: the
  planes, coils and revolutions that use them take them fixed where they
  are, and a plane parallel to another through a point of the model keeps
  its offset when the model changes).
- **Model states** of parts (the stored bodies are the active state's).

## Assemblies

`assembly` reads `.iam` assembly files: the documents an assembly refers
to and the occurrences that place them; the `.iam` import
(`core/ffi/src/iam_import.rs`, the `import_iam` command in
[commands.md](../model-api/src/commands.md#iam-import), `mitcad-cli
import-iam`, File › Open) makes them MitCad's components and occurrences
and imports each part with the `.ipt` import. Claims are marked as above;
the test corpus here is 166 assemblies of a private project tree (saved by
releases from 2016 to 2023, 1 to 459 occurrences each, sub-assemblies
nested up to seven deep, with the tree's 902 part files).

An assembly is a compound file with the segment database, as a part. The
root storage's class id is `{e60f81e1-49b3-11d0-93c3-7e0706000000}`
*(verified)*. Its segments: `AmDcSegment` (definitions: occurrences,
constraints, the browser's labels), `AmRxSegment` (the occurrences'
documents and placements), `AmGraphicsSegment`, `AmBrowserSegment`,
`AmAppSegment`, `AmBREPSegment`, `DesignViewSegment`, `NBNotebookSegment`
*(verified: the first two in every assembly)*. Three places make up an
occurrence, tied together by a key (a u32, unique in the file):

**Occurrences** (definitions segment, `614d8790…`) *(verified: one per
occurrence)*: the common header (22 or 26 bytes, as in a part) whose flags
tell grounded (`0x02000000`; one occurrence in most assemblies *(seen)*),
hidden (`0x01000000` *(seen, 9 occurrences)*) and suppressed
(`0x40000000` *(seen, once)*); then i32 −1 (not in older releases), u32
and, last, a reference to a record (`604d8790…`) that ends with the key,
the u32 3, the text `DCx` and two bytes. A suppressed occurrence refers to
a record of another type and has no key; it has no document descriptor or
placement either (its document is not loaded). The browser's label of an
occurrence is empty unless it was renamed: the shown name is the
document's file name and the instance number (`part:2`).

**Document descriptors** (reference segment, `fe32cdfd…`) *(verified:
one per key)*: u32, u16, u16, the key, the text `AmRx`, u16, a reference,
three 16-byte ids, the first the referenced document's version (a part
lists it in its own `UFRxDoc` stream: the import checks that a file found
elsewhere is the document referred to *(seen: so for 1306 of the 1347
part files found; the others were saved again after the assembly)*);
then a list of the document's bodies (`06 00 00 30`, the count, u32 0;
per body a head ending with `00 00 00 10`, the body's node number and u32
flags, the body's range box as six f64 in cm and its colour as a text
`R,G,B`), and the model state's name (`Default`, `[Primary]`) last
*(verified)*. The entries are the part's result list (*The result
segment*): flags `0x01` a solid, `0x22` a group of surfaces, `0x10` a
mesh, `0x08` hidden *(seen)*. The occurrence's range box is the union of
the shown entries' boxes; an earlier reader read the entries of solids
only, so that the box of a part whose surfaces reach
beyond its solids was too small (the parts that lay 0.7 and 0.89 mm
outside it: their bodies were right).

**Placements** (reference segment, `bc922723…`) *(verified: one per
key)*: u32, u16, a reference to the descriptor, the 4x4 transform stored
with masks as a sketch's (`sketch`: an optional u32 `0x203`, the masks,
the other elements as f64 row by row), which maps the document's
coordinates to the assembly's (cm), then nine f64: three not read, the
transform's x axis and the centre of the document's range box placed
*(seen; the centre is not always up to date: 7 occurrences of one assembly
keep a centre from an earlier placement, which every other record of the
file, the graphics segment's among them, has left)*. Every placement of
the corpus is a rotation and a translation *(verified)*.

**Display transforms** (graphics segment, `07d0d0b9…`) in assemblies of
older releases: a 15-byte head, the transform with masks and the key
tagged `GRx` (as `DCx` above). Where an occurrence has one, it is its
placement within rounding *(verified)*: the import checks every
occurrence's placement in MitCad against it.

**Referenced files** (the `UFRxDoc` stream, not a property set): the
document's own path as saved (the first path in the stream), its model
states and design views, then a list with an entry per file: the full path as saved (on the machine that
saved the assembly), u32, a library's name (`Content Center Files` for
library parts, else empty), u16, the library member's name, eight bytes,
two 16-byte ids, the file's id (u32), two u32 and u32 flags (with `0x20` a
text, a variant's name, and a u32 follow); an empty text ends the list
*(verified)*. Six bytes, the number of occurrences, u32 0 and the
occurrence table follow: per occurrence the file's id, the key, a
sequence number and the instance number (four u32), then properties of
variable length (not decoded; a part's hold its version id). The reader
finds the table's entries by their heads (a file of the list, a key not
yet seen, plausible numbers) *(verified: every key of the corpus found;
the version id within the entry for all but 2 of 6,329 part entries)*.

Sub-assemblies are occurrences of `.iam` files, whose own occurrences are
in those files: the import follows them (each file once). Placements of
an assembly are relative to it, so a nested occurrence is placed in its
sub-assembly's component.

Not decoded: constraints and joints (they do not move the stored
placements), patterns of components (their elements are occurrences of
their own and come in as such), design views and positional
representations (the stored placements are the active ones), the
occurrence table's properties and the model state an occurrence uses
(read: its name).

### Results on the assemblies

`iam.corpus` (dev build, 2026-10-09, parts as their stored bodies; the
test: [core/import/docs/iam.md](../import/docs/iam.md#iam-corpus-test)), the
166 assemblies of the private project tree (saved by releases from 2016
to 2023, sub-assemblies nested up to seven deep):

- Every assembly reads and imports; every occurrence that is not
  suppressed is placed (14,746 in all, nested ones counted in each
  assembly that places them; one suppressed, 9 hidden); every placement
  is a rotation and a translation.
- Referenced files (counted per assembly): 1,554 found relative to the
  assembly as saved, 6 by the saved path's tail, 2 by name; 47 not in the
  tree (library parts and one part of another project), their 581
  occurrences placed as empty components. Of the part files found, 1,316
  are the documents referred to by their version id, 41 were saved again
  after the assembly.
- Every placement the file displays (older releases: 4,857 occurrences)
  agrees with it.
- Every placed part's bodies lie within their stored range boxes (12,161;
  the box is mostly tight, up to 15 mm larger for parts with curved
  faces; before the descriptors' surface entries were read, two parts
  with groups of surfaces lay up to 0.89 mm outside theirs). Of
  the parts saved again after their assembly, 54 of 300 still lie within
  the range box the assembly stored.
- Every assembly passes (before the bodies of wires only were left out
  and the surface entries read, 5 failed for their parts: two part files
  whose bodies of wires failed the import, and the two parts above). The
  run takes 8 minutes with 3 files at once.

## Tests

- `cargo test -p mitcad-ipt` (ctest `core.ipt`): the container, property
  sets and segments on files made by the writers here, damaged files, the
  test part's bodies, the result list and its hidden bodies, a segment
  that does not split, extended trailers,
  segments of majors 16 and 18 without trailer flags, mesh records and a
  part of a mesh feature;
  the definitions records written by `testdesign` (parameters, units and
  expressions, a sketch, an extrusion, the history state table, both
  header forms, the record prefix of majors 20 to 28; polygons, circular
  and rectangular patterns, offsets, fixed entities, points on horizontal
  and vertical lines, arcs with points on them, a distance between
  circles left out; a profile kept as a face, flagged lines that bound it;
  selections inside one another by the even-odd rule; a spline with its
  control polygon; planes parallel to a plane through a point; a coil's
  axis by its geometry;
  the extrusions' extents; a sheet-metal face with its style's rules,
  front and back, mitcad#211; topological names, mitcad#242: an item's
  reference and table entry, an edge by its two faces and by its own tag
  in a state, the attribute classes in both spellings, a flange tag's
  numbers) read into the dump IR, and replayed by the
  import on the model's mock kernel (`core/model-tests/imports`,
  `ipt_design_tests.rs`, ctest `core.model.imports`); the
  assemblies written by `testassembly` (files, occurrences, keys, flags,
  labels, placements, range boxes, display transforms, the occurrence
  table, a part's own version id).
- `tests/corpus.rs`: every file under `MITCAD_IPT_CORPUS` opens, its B-rep
  records split out of their segments and every body converts cleanly;
  every segment splits; every parameter reads and its expression
  evaluates to its stored value; features name their history states;
  skipped without the corpus.
- With OCCT: `core.exchange` (the bridge: units, material, one undo step,
  a STEP reference, the part with a design replayed, a part of a mesh
  feature, an empty part, a body the result list keeps hidden), `cli.import_ipt*`,
  `cli.ipt_design_file`, `ipt.corpus`
  ([tools/cli/ipt-corpus.cmake](../../tools/cli/ipt-corpus.cmake)) and
  `ui-import-test.sh`; see [core/import/docs/ipt.md](../import/docs/ipt.md#ipt-import).
  Assemblies: `cli.import_iam`, `cli.info_iam`, `cli.import_ipt_iam` (the
  `testassembly` project) and `iam.corpus`
  ([tools/cli/iam-corpus.cmake](../../tools/cli/iam-corpus.cmake): every
  `.iam` under `MITCAD_IPT_CORPUS`).
