# mitcad-ipt: the `.ipt` part file reader

`core/ipt` reads `.ipt` part files: the bodies stored in the file, the
document properties, and the part's design: its parameters with their
expressions, sketches, work planes and features, as the import's dump IR
(`mitcad_f3d::design::ir::Dump`, [SCHEMA.md](../import/SCHEMA.md)). The
`.ipt` import (mitcad#60) replays that design with `mitcad-import` against
the ASM history of the B-rep record, as the `.f3d` import does, or builds
the stored bodies as base features (`core/ffi/src/ipt_import.rs`,
`core/ffi/src/ipt_history.rs`, the `import_ipt` command in
[commands.md](../model/src/api/commands.md#ipt-import), `mitcad-cli
import-ipt`, File › Open).

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
| `dc` | The definitions segment (`PmDCSegment`): records by type, the common header, references, lists, texts, labels. |
| `params` | Parameters: units, expression trees, their translation into Mitcad's expression language and their evaluation. |
| `sketch` | Planar sketches: transform, points, lines, circles and arcs, constraints, dimensions. |
| `features` | Extrusions, revolutions, holes, fillets, chamfers, rectangular and circular patterns, mirrors, work planes. |
| `profile` | The profiles a feature selected, measured from the definitions segment's own ASM record. |
| `design` | The timeline (the browser's features in order, with their history states) and the dump IR. |
| `testdata`, `testdesign` | Small parts made by these writers: two bodies of Mitcad's own ASM writer (a cube and a cylinder), part number, material, inches; the cube with the records of a parameter, a sketch and an extrusion that make it. |
| `bin/mitcad-ipt-inspect` | `list`, `properties`, `segments`, `records`, `record`, `asm`, `bodies`, `history`, `parameters`, `design [--json]`, `profile`, `write-test`. |

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
(u32), and one byte: 0, or 1 when an extended trailer follows (typed
property lists, not read; in the appearance segment and in one
definitions segment of the test files). After the last record: `FF FF FF
FF` and a few bytes. The reader checks every size against its echo and
stops at the first mismatch; an extended trailer ends where the next
record starts: a selector of the same form whose bytes are followed by
their size, or the end marker after the last record. Every segment of the
test files splits.

The meta stream's eight u16 release stamps hold the segment's major
version in the low byte of the sixth: 25 for the files a 2021 release
saved (26 for one), 28 for a 2024 release's *(verified)*.

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
data for the ASM magic instead and says so (`PmBRepSegment@scan`).

The ASM files of the test corpus are save versions 226 to 229 with
centimetres as the model unit (the header's 10 mm per unit), which the
`.f3d` reader's converter reads unchanged: every body converts without
issues and its checks pass. Their header flag says that they carry an ASM
history; it has 2 to 198 states (`history_states`), which the import
checks the features against (*The timeline and the history states*
below), as the `.f3d` import uses the `.smbh` history.

Besides the part's solid, three files store two one-face sheet bodies;
the import brings them in as sheet bodies.

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

- **Parameter** (`264d8790…`): the header, eight bytes, the name, u32,
  the unit, the expression (references), the nominal and model values (f64,
  cm or rad) and four bytes. **Integer parameter** (`dfd51dbb…`, a
  pattern's count): the header, eight bytes, the name, u32, the nominal
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
  two operands), negation (`0c7aa7f8…`). The test files use numbers,
  parameters, products and negation. The expression is written in Mitcad's
  language with each number in its display unit (at most 12 significant
  digits: `6 mm`, not `6.000000000000001 mm`), evaluated here against the
  stored value, and given to the import, which evaluates it again; an
  expression that does not translate or evaluate to its value becomes the
  stored value.

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

### Sketches

`sketch`. A sketch (`114d8790…`): the header, i32, u32, the list (kind
8) of its entities, constraints and dimensions, its transform, its normal
(`40df52ce…`: three f64 at its end), two u32 and a list.

- The **transform** (`184d8790…`): eight bytes, an optional u32 `0x203`,
  a u16 mask of the elements that are ±1 and a u16 mask of those that are 0
  or −1; the others follow as f64, row by row. It maps the sketch to the
  model (cm): x and y axes, normal and origin in its columns *(verified)*.
- **Entities**: eight bytes, u32 flags (0x40 construction, 0x80000
  centre line, 0x40000 projected from the model), the sketch; a point's x
  and y (f64, cm); a line's list of its two points, then points that lie
  on it; a circle's list of its end points (none for a full circle), and
  at its end the centre point, the radius (f64) and a byte (1: full). An
  arc runs counter-clockwise from its first end to its second *(verified:
  every arc of the test files that meets a line tangentially)*. A centre
  line bounds no profile (Mitcad's do): it comes in as construction
  geometry.
- **Constraints and dimensions**: the header, i32, a reference, two maps,
  the parameter, the entities. Coincident `944d…`, parallel `954d…`,
  perpendicular `964d…`, tangent `974d…`, horizontal `984d…`, vertical
  `994d…` (`…8790d011f8d10008cabc0663dc09`), collinear `e07b9a5a…`, equal
  length `e0c281c6…`, equal radius `d07d2c44…`, symmetric `107cdd0d…`,
  midpoint `bf70e821…` (by their geometry *(verified)*); a circle's centre
  `008c10e1…` (implicit in Mitcad). Dimensions: distance `58850511…`
  (aligned *(verified)*: points, a point and a line, parallel lines),
  horizontal and vertical distance `00c0ac00…`, `40ff8336…`, diameter
  `e096df74…`, radius `00b71b67…`, angle `100a0d59…`; a dimension whose
  parameter is in the part's table drives it.
- Not decoded (the sketch comes in partial, these left out): sketch block
  instances (`4c12f14c…`, their geometry `7e71a918…` and constraints
  `cf8d56f2…`), polygon constraints (`1f89dbfb…`, `38485352…`),
  `bc144b4e…`, splines (`013cbb1f…`) and ellipses (`60d40745…`).

### Features

`features`. A feature (`914d8790…`): the header, i32, u32, a reference
list (kind 2) of its properties by slot, u32. Properties are records:
parameters, enumerations (`28be9a72…` operation: 1 new body, 2 cut, 3
join, 4 intersection; `297d6392…` extent; eight bytes, u16 kind, u16
value), booleans (`284d8790…`: the value is the last byte), directions
(`40df52ce…`), lists.

| Kind | Slots and records | Decoded |
|---|---|---|
| Extrusion | 0 operation, 1 profiles (`91739422…`), 2 direction, 3 reversed, 4 distance, 5 taper, 6 extent (1 distance, 2 symmetric, 5 through all, 7 up to a face or plane), 7 symmetric | all but the face or plane it extends to (the import looks for it) |
| Revolution | 0 operation, 1 profiles, 2 axis (a work axis `896cf08e…`: a point and a direction before its last byte), 3 extent (3 an angle), 4 angle | the axis as a sketch line on it or an origin axis |
| Hole | 0 form (`117ccd43…`: 0 drilled, 1 countersink, 2 and 3 counterbore), 1 diameter, 2 depth, 3 counterbore or countersink diameter, 4 counterbore depth, 5 countersink angle, 6 drill point angle (0: flat), 7 its sketch points (`b1226593…`), 8 placement (a transform), 9 extent (1, 5, 7), 16 direction | position: the sketch points in the sketch (label child) the feature is on, else the placement's origin; up to a face as through all |
| Fillet | 0 edge sets (`dae9481b…` of `1641d6aa…`: edges, radius, selection, tangent chain), 11 form (`2788f278…`, 0 edge fillet) | radii; the edges are named by the file's topological naming, not decoded: the import finds them from the history state |
| Chamfer | 0 edges, 2 distance, 3 second distance, 4 form (`3200aa7d…`: 0 equal, 1 distance and angle, 2 two distances), 10 angle | sizes; edges as for fillets |
| Rectangular pattern, circular pattern, mirror | after the feature's list: u32, the list of the copied features, then properties as references at no fixed offsets: counts (integer parameters), spacings and directions (`7b4544a2…`: a flip, a start point, a vector and an end point), the angle and the axis, the mirror plane (a work plane) | all; the copied features in the timeline's order |
| Work plane (`42df52ce…`) | its last nine f64: origin, x axis, y axis; an offset plane's definition `c0d8280d…`: the plane, the plane it is offset from and the offset parameter | offset planes; others (mid planes `560a6ccb…`, at an angle `8af4e674…`, parallel through a point `eea9eab3…`, through two axes `5abeff8a…`) fixed where they are |

**Profiles** (`profile`): a profile selection (`3b2477a4…`) refers to an
entity link (`21750fcc…`: an ASM record of the definitions segment of the
B-rep record's type, a body id and a lump). That ASM file keeps every
selected profile as a wire body (as many as there are selections in the
test files *(verified)*); its loops are sampled, measured in the sketch
(area and centroid) and given to the import, which picks the sketch's
regions of the same area and centroid. Selections inside one another are
not measured (how they make up the regions is not settled).

### Results on the test files

`ipt.corpus` (dev build, 2026-10-08): 16 parts, 2,785 parameters (2,755
model, 30 user) imported with their expressions, every one agreeing with
its stored value, in the file and in Mitcad (1,943 parameter records of
annotations and features' own tables, and the internal variables, are not
the part's). 196 sketches, 344 features with their history states. Every
part's final bodies are the stored ones (22 bodies, all valid); the
features:

| Kind | Parametric | Partial | Fallback | Skipped |
|---|---|---|---|---|
| Sketch | 169 | 27 | | |
| Work plane | 7 | 37 | | |
| Extrusion | 83 | | 16 | |
| Hole | 90 | | 2 | 5 |
| Fillet | 45 | | 4 | |
| Chamfer | 10 | | | |
| Rectangular pattern | 11 | | 3 | 2 |
| Circular pattern | 2 | | | |
| Mirror | 19 | | 6 | 2 |
| Revolution | 2 | | 3 | |
| Face draft | | | 17 | |
| Shell, split, loft, rib, sweep, boundary patch | | | 16 | 2 |
| Sheet metal (face, flange, corner round and chamfer) | | | 25 | |
| Base solid | | | 1 | 1 |

*Partial*: a sketch with entities left out (above), a work plane fixed
where it is. *Skipped*: the history shows no change by it. Fallbacks are
the bodies of the feature's history state.

### What the files do not tell (feasibility)

Without files saved before and after one change by the authoring
application, these are not settled; such features come in as the bodies
of their history state:

- **Edges and faces** that features name (fillets, chamfers, drafts,
  shells, splits): the topological naming (the edge items `82695c37…`
  refer to the B-rep segment's naming records) is not decoded. Fillets
  and chamfers find their edges from the history state; drafts, shells
  and splits need their faces, so they fall back.
- **Mirrors of fillets and of patterns**: the copies Mitcad makes do not
  give the history state in 6 of the test files' 25 mirrors that are not
  skipped (Mitcad's mirror of features, not the decoding: the inputs and
  planes are decoded).
- **Sheet metal, lofts, ribs, sweeps, boundary patches**: record layouts
  not studied (few in the test files).
- **Extrusions from sketch blocks** (logos): blocks are not decoded.
- **Work planes** other than offset ones, and **work axes and points**
  (not imported as items).
- **Assemblies** (`.iam`, stage 4) and **model states**.

## Tests

- `cargo test -p mitcad-ipt` (ctest `core.ipt`): the container, property
  sets and segments on files made by the writers here, damaged files, the
  test part's bodies, a segment that does not split, extended trailers;
  the definitions records written by `testdesign` (parameters, units and
  expressions, a sketch, an extrusion, the history state table, both
  header forms) read into the dump IR, and replayed by the import on the
  model's mock kernel (`tests/design_import.rs`).
- `tests/corpus.rs`: every file under `MITCAD_IPT_CORPUS` opens, its B-rep
  records split out of their segments and every body converts cleanly;
  every segment splits; every parameter reads and its expression
  evaluates to its stored value; features name their history states;
  skipped without the corpus.
- With OCCT: `core.exchange` (the bridge: units, material, one undo step,
  a STEP reference, the part with a design replayed), `cli.import_ipt*`,
  `cli.ipt_design_file`, `ipt.corpus`
  ([tools/cli/ipt-corpus.cmake](../../tools/cli/ipt-corpus.cmake)) and
  `ui-import-test.sh`; see [core/import/README.md](../import/README.md#ipt-import).
