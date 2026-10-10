# mitcad-ipt: the `.ipt` part and `.iam` assembly file reader

`core/ipt` reads `.ipt` part files: the bodies stored in the file, the
document properties, and the part's design: its parameters with their
expressions, sketches, work planes and features, as the import's dump IR
(`mitcad_f3d::design::ir::Dump`, [SCHEMA.md](../import/SCHEMA.md)). The
`.ipt` import (mitcad#60) replays that design with `mitcad-import` against
the ASM history of the B-rep record, as the `.f3d` import does, or builds
the stored bodies as base features (`core/ffi/src/ipt_import.rs`,
`core/ffi/src/ipt_history.rs`, the `import_ipt` command in
[commands.md](../model/src/api/commands.md#ipt-import), `mitcad-cli
import-ipt`, File › Open). It also reads `.iam` assemblies: the files they
refer to and the occurrences that place them ([Assemblies](#assemblies)),
which the `.iam` import makes Mitcad's components and occurrences.

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
| `cfb` | Compound files (MS-CFB): versions 3 and 4, FAT, DIFAT, mini stream, the directory's red-black trees. Chains are checked against the file size and loops. A writer for tests. |
| `props` | Property set streams (MS-OLEPS): one or two sets per stream, the dictionary, the code page, typed values (integers, reals, booleans, strings, file times, class ids). A writer for tests. |
| `rse` | The segment database: meta stream headers, record tables, record splitting, zlib and zstd. A writer for tests. |
| `lib.rs` | `IptFile`: segments, property sets, document properties, B-rep records, the definitions segment; `read_bodies` converts the bodies with the `.f3d` reader's ASM code. |
| `mesh` | The meshes of mesh features in the graphics segment. A writer for tests. |
| `dc` | The definitions segment (`PmDCSegment`): records by type, the common header, references, lists, texts, labels. |
| `params` | Parameters: units, expression trees, their translation into Mitcad's expression language and their evaluation. |
| `sketch` | Planar sketches: transform, points, lines, circles and arcs, constraints, dimensions. |
| `groups` | Polygons and circular and rectangular patterns of sketch entities. |
| `features` | Extrusions, revolutions, holes, fillets, chamfers, rectangular and circular patterns, mirrors, threads, coils, work planes, combines, splits, shells, sweeps; the bodies features refer to and the items that made them. |
| `profile` | The profiles a feature selected, measured from the definitions segment's own ASM record. |
| `design` | The timeline (the browser's features in order, with their history states) and the dump IR. |
| `result` | The result segment's body list: which bodies the part shows, solids or surfaces, their range boxes (*The result segment* below). A writer for tests. |
| `states` | The bodies of the B-rep records at the states of their ASM history, read once for all tries of the import: the distinct ones by the step they were converted at, converted again when asked for. |
| `assembly` | Assemblies (`.iam`): the referenced files, the occurrences with their keys, flags, labels, documents, placements, range boxes and display transforms (*Assemblies* below). |
| `testdata`, `testdesign` | Small parts made by these writers: two bodies of Mitcad's own ASM writer (a cube and a cylinder), part number, material, inches; the cube with the records of a parameter, a sketch and an extrusion that make it; a part of a mesh feature's tetrahedron; an empty part; the cube and the cylinder with a result list that keeps the cylinder hidden. |
| `testassembly` | Small assemblies made by these writers and a test project of them (a top assembly as saved on another machine, a sub-assembly, parts found relative to it and by name, a missing one; hidden, grounded and suppressed occurrences). |
| `bin/mitcad-ipt-inspect` | `list`, `properties`, `segments`, `records`, `record [--raw]`, `data`, `dump`, `labels`, `stream [--raw]`, `asm`, `bodies [-v|-f]`, `result`, `history`, `parameters`, `typed`, `show`, `constraints`, `prefix`, `flags`, `design [--json]`, `profile`, `assembly`, `write-test`. |

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
  parameters, products and negation. The expression is written in Mitcad's
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
  line bounds no profile (Mitcad's do): it comes in as construction
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
  profiles are a union of Mitcad's regions, which the import matches by
  area and centroid); the others as construction geometry *(seen)*. A curve drawn twice (the same ends, centre, radius or
  angles) bounds profiles once: the later copy comes in as construction
  geometry *(seen)*.
- **Constraints and dimensions**: the header, i32, a reference, two maps,
  the parameter, the entities. Coincident `944d…`, parallel `954d…`,
  perpendicular `964d…`, tangent `974d…`, horizontal `984d…`, vertical
  `994d…` (`…8790d011f8d10008cabc0663dc09`), collinear `e07b9a5a…`, equal
  length `e0c281c6…`, equal radius `d07d2c44…`, symmetric `107cdd0d…`,
  midpoint `bf70e821…`, two points on a horizontal line `c0a3558f…` and
  on a vertical line `5052da64…` (by their geometry *(verified)*); a
  circle's centre `008c10e1…` (implicit in Mitcad); a fixed point or
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
  141 of the 152 distances to circles of the older parts)*, Mitcad's
  concentric circle dimension; another distance to a circle or arc (to
  its edge *(seen)*) has no counterpart in Mitcad and is left out. A
  distance from a point or line to a centre line whose value is twice the
  geometry's is a diameter about it (a revolution's profile), Mitcad's
  linear diameter *(seen)*.
- What holds nothing *(seen)*: a constraint between geometry projected
  from the model (flag 0x40000) and the end and centre points of
  projected curves (Mitcad keeps them fixed, and would take the
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
- Not decoded (the sketch comes in partial, these left out): sketch block
  instances (`4c12f14c…`, their geometry `7e71a918…` and constraints
  `cf8d56f2…`), texts (`013cbb1f…`: a text with its font codes, its
  frame `7f341025…`; taken for splines before) and splines through points
  (`47d9553e…`: the list of the points they pass through) with what
  constrains them.
- **Splines** (`d42f37f9…`) *(seen: projected from the model, in 8 parts
  of the older corpus)*: the entity's list of its two end points (one, where it starts and ends, for a closed spline, whose first and last poles are that point), other
  lists, then its NURBS data twice: u32 degree, f64 tolerance, the knots
  as an array (u32 count, u32 capacity, u32 8, the f64), eight bytes, the
  control points as an array (u32 8, u32 count, u32 capacity, u32 8, two
  f64 each, cm). The reader finds the data by that form and checks it (knots
  that do not decrease, as many as the control points and the degree and
  one, the first and last control points on the end points); the spline
  comes in as a fixed spline (Mitcad's splines of stored geometry). A
  spline's **control polygon** (`9d858c5d…`, in the sketch's list as a
  constraint: after the parameter the spline, its control points and the
  lines between them) *(seen: 22 records in 6 parts, each point on one of
  the spline's poles)*: a spline drawn so comes in as Mitcad's spline of
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
| Extrusion | 0 operation, 1 profiles (`91739422…`), 2 direction, 3 reversed, 4 distance, 5 taper, 6 extent (1 distance, 2 symmetric, 5 through all, 7 up to a face or plane, 4 up to the next face), 7 symmetric (set in every symmetric extent, and in 9 of 279 through all: through all both ways); up to the next face, 24 the body it reaches (a list `83aadad5…`: all 48 such extrusions of the older parts); up to a face, 18 the face's surface (`f0801215…`: after the prefix 16 zero bytes, a byte 1 and a u32 kind; `0x19` a plane: a u32, its origin (cm) and x and y axes, both across the extrusion in all 130 such records; `0x49` a cone: two u32, the cosine and sine of its half angle, a point on its axis (cm), its two radii, its axis and a reference direction: 8 cylinders) and 13 a boolean (`fd24893d…`, not decoded), or 11 a work plane (4), or 25 a work point (2, not decoded) *(seen)* | all; the next face as the first face of that body reached, the face as its plane or a cylindrical face on that cylinder, which the import looks for |
| Revolution | 0 operation, 1 profiles, 2 axis (a work axis `896cf08e…`: a point and a direction before its last byte), 3 extent (1 an angle, 3 a full turn *(verified)*: its angle parameter keeps a value it does not use, 0 or the angle it was made with), 4 angle, 5 direction (an enumeration `c26aa0c7…`, not decoded) | the axis as a sketch line on it or an origin axis, else the work axis fixed where it is |
| Thread | 0 its faces (named by the file's topological naming, not decoded), 1 full length (a boolean), 3 length and 4 offset of one that is not, 6 modelled (a boolean), 10 its ends (`9984474c…`: after the prefix two points, cm, on its cylinder's surface or axis, where it starts and ends); its size in a record of its own (`1f7e08a4…`, its header's context the thread: after the header a reference, then texts: the nominal size, the designation `M3x0.5`, the thread type, an empty text, a u32, four empty texts, the class `6g`, then its diameters' limits and pitch as texts in the saving machine's number format) *(seen)* | all; the face as the cylinder through both ends, which the import looks for |
| Coil | 0 operation, 1 profiles, 2 axis (a work axis), 3 against the axis (a boolean: the coil runs against the work axis' stored direction), 5 its type (`b80cb14f…`: the prefix, u16 kind, u16 value: 0 pitch and turns, 1 turns and height, 2 pitch and height; 3, a spiral, not seen), 6 pitch, 7 height, 8 turns, 9 taper *(seen)*; the parameters its type does not use keep values | all but its hand (the import tries both, the direction slot 3 gives first), as Mitcad's helix; the axis a line of its sketch on the work axis, an origin axis, else the work axis fixed where it is (a cylinder's axis: 46 of the 85 coils of the older parts, all of which fell back before); tapered coils are not replayed |
| Hole | 0 form (`117ccd43…`: 0 drilled, 1 countersink, 2 and 3 counterbore), 1 diameter, 2 depth, 3 counterbore or countersink diameter, 4 counterbore depth, 5 countersink angle, 6 drill point angle (0: flat), 7 its sketch points (`b1226593…`), 8 placement (a transform), 9 extent (1, 5, 7), 16 direction | position: the sketch points in the sketch (label child) the feature is on, else the placement's origin; up to a face as through all |
| Fillet | 0 edge sets (`dae9481b…` of `1641d6aa…`: edges, radius, selection, a boolean), 11 form (`2788f278…`, 0 edge fillet) | radii; the edges are named by the file's topological naming, not decoded: the import finds them from the history state. The boolean (not a boolean record in the parts of `ipt.corpus`) is false in every fillet of the older parts below, while their history states show whole tangent chains rounded from an edge inside them, so it is not the tangent chain option: the sets follow tangent chains |
| Chamfer | 0 edges, 2 distance, 3 second distance, 4 form (`3200aa7d…`: 0 equal, 1 distance and angle, 2 two distances), 10 angle | sizes; edges as for fillets |
| Rectangular pattern, circular pattern, mirror | after the feature's list: u32, the list of the copied features, then properties as references at no fixed offsets: counts (integer parameters), spacings and directions (`7b4544a2…`: a flip, a start point, a vector and an end point; or a direction `40df52ce…` whose flip is not decoded, or a path `aa7b97a5…` *(seen)*), the angle and the axis, the mirror plane (a work plane) | all; the copied features in the timeline's order; a direction of another record is found with the history |
| Loft (`e0ef542f…` label class) | 0 its sections (a list `509e2f31…` of sections `e850314b…`: after the prefix an entity link as a profile selection's, its wire body the section's boundary), 1 operation *(seen: every loft of the test files)*; the label shows the sections' sketches, or records `7ca882b3…` of the sections | the sections as the profiles of the sketches whose planes hold them (the label's, else the latest before it); rails and conditions not decoded |
| Work plane (`42df52ce…`) | its last nine f64: origin, x axis, y axis. Definitions (each the header, an i32, the plane, then references): offset `c0d8280d…` (the plane it is offset from, the offset parameter), mid plane `560a6ccb…` (the two planes), at an angle `8af4e674…` (the work axis it turns about, the plane it is at an angle to, the angle parameter, then data not decoded), through three work points `8a924209…` (the points), through two work axes `5abeff8a…` (the axes), of a work point and a work axis `62b73858…` (through both, or through the point normal to the axis: the plane's geometry tells), parallel to a plane through a work point `63b73858…` (the point, then the plane; the plane's origin is the point) and through a point of the model's geometry `eea9eab3…` (the plane, then an edge `4a068a52…` not decoded, and two f64) *(seen)*; a work point `3edf52ce…`: after the prefix 12 bytes (up to major 18; 16 from 24), its position (three f64, cm) and two empty lists *(seen)*; not decoded: `315417a7…` (a work point and an edge: normal to the edge at the point, by the plane's geometry). Planes the file keeps fixed: those no definition names (their flags after the prefix have `0x100000`; 33 of the 49 fixed planes of the older parts, among them planes of parts made from other formats), those of a definition `118e11a6…` with no references after the plane (7); `61b8480c…` defines a plane off the timeline as another one (its first reference, then the plane it copies) | offset, mid and angle planes (a plane off the timeline by its geometry: the import looks for the face in it); planes through three points, two lines, a line and a point, or normal to a line through a point, the work points and axes as the origin's or fixed where they are (work points and axes are not imported); parallel ones as offsets from their plane to where the point puts them (the point fixed where it is); the others fixed where they are |
| Combine (`336723c0…`) | 0 the target (a list of bodies `83aadad5…`, one body), 1 the tools (a list of bodies `ae70680e…`), 2 operation (2 cut, 3 join, 4 intersection), 3 keep the tools *(verified: every combine of the test files)* | all |
| Split (`baa87183…`) | 0 its kind (`1990e1b8…`: 2 split faces, 3 split bodies), 2 the faces to split (a list of face names `f1b2bc24…`), 4 the tool's sketch profiles (as an extrusion's), 5 a work plane as the tool, 7 the bodies (`83aadad5…`) *(seen: 77 face splits by sketch profiles, one body split by a work plane)* | kind, tool and bodies; the faces are found with the history |
| Shell (`12963cb8…`) | 0 direction (`e469a1b3…`: 0 inside), 1 the faces removed (`f1b2bc24…`), 3 thickness, 9 its body (`ae70680e…`) *(seen: every shell of the test files, inside, removing no face or one)* | all but the faces: their number is given, the import finds them with the history |
| Sweep (`05daba90…`) | 0 profiles, 1 the path (a profile selection, or a path selection `473f20fc…` of the same layout), 2 operation, 3 taper; the label shows the profile's sketch and the path's (they may be one) *(seen: every sweep of the test files)* | profiles, operation, taper; the path as the curves of the label's sketches on the selected wire |

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
feature's profile splits this one in Mitcad's sketch). Selections inside
one another make up their regions by the even-odd rule
(`profile::even_odd`): a region is taken when an odd number of the
selections cover it. Each selection is mostly a single loop, and a ring is
the selection of its outside and that of its hole *(seen: a plate's
outline and a circle in it, both selected, make the plate with the hole,
as its history state has it; four loops, the middle one twice, make the
ring between the outer and the inner one)*; a selection that keeps a hole
and the hole's own selection make the whole. The distinct loops nest in a
tree, each loop less its children one of the sketch's regions as far as
the selections tell; the regions taken are given with their area and
centroid. Without selections inside one another, a boundary selected more
than once (the regions of coincident curves, as a degenerate pattern's
copies on their original *(seen)*) is given once.

### Results on the test files

`ipt.corpus` (dev build, 2026-10-08; the outcomes below with the sketch
groups and points on lines, release build, 2026-10-09): 16 parts, 2,785 parameters (2,755
model, 30 user) imported with their expressions, every one agreeing with
its stored value, in the file and in Mitcad (1,943 parameter records of
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
up to the next face left fall back where their profile starts inside
the body they reach (Mitcad's extent up to a body does not take that
case).

### What the files do not tell (feasibility)

Without files saved before and after one change by the authoring
application, these are not settled; such features come in as the bodies
of their history state:

- **Edges and faces** that features name (fillets, chamfers, drafts,
  shells, splits): the topological naming (the edge items `82695c37…`
  refer to the B-rep segment's naming records) is not decoded. Fillets
  and chamfers find their edges from the history state, and so do shells
  (the faces of their body the next state no longer has, most of their
  points off its faces) and face splits (the faces of the body the next
  state changed that it no longer has whole, no face there of the same
  surface and area); drafts need their faces, so they fall back.
- **Face splits along open sketch curves**: Mitcad's split takes the
  curves around a sketch's regions; a split whose curves bound no region
  of Mitcad's sketch falls back.
- **Sheet metal** (faces, flanges, corner rounds and chamfers, cuts,
  bends, folds): Mitcad's model has no sheet-metal features; they come in
  as the bodies of their history states.
- **Mirrors of fillets and of patterns**: the copies Mitcad makes do not
  give the history state in 6 of the test files' 25 mirrors that are not
  skipped (Mitcad's mirror of features, not the decoding: the inputs and
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
[commands.md](../model/src/api/commands.md#iam-import), `mitcad-cli
import-iam`, File › Open) makes them Mitcad's components and occurrences
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
occurrence's placement in Mitcad against it.

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
test: [core/import/README.md](../import/README.md#iam-corpus-test)), the
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
  the extrusions' extents) read into the dump IR, and replayed by the
  import on the model's mock kernel (`tests/design_import.rs`); the
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
  `ui-import-test.sh`; see [core/import/README.md](../import/README.md#ipt-import).
  Assemblies: `cli.import_iam`, `cli.info_iam`, `cli.import_ipt_iam` (the
  `testassembly` project) and `iam.corpus`
  ([tools/cli/iam-corpus.cmake](../../tools/cli/iam-corpus.cmake): every
  `.iam` under `MITCAD_IPT_CORPUS`).
