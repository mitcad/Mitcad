# mitcad-dwg

Reader for `.dwg` drawings, R13 to R2018, for the import of their 3D
content (mitcad#453), and a writer of 2D drawings in R2000 for the drawing
export ([Writer](#writer), mitcad#728). No geometry: the bodies are built
by the import (`core/ffi/src/dwg_import.rs`, the `import_dwg` command,
`mitcad-cli import-dwg`).

The format knowledge comes from the *Open Design Specification for .dwg
files* (Open Design Alliance, version 5.4.1), a published description of
the format. No other reader's code is used. Where the specification is
unclear or its chapters disagree, the reader's choice is noted below.

## What real drawings showed

The trial corpus (`MITCAD_DWG_CORPUS`: eight drawings of R2000, R2004,
R2013 and R2018 with 3D solids, blocks and inserts) confirmed the R2004
compression and copy offsets, the page and section maps, the object map,
the R2010+ handle streams and string streams, the header variables, the
classes and the B-rep record's masking, and corrected these points, each
with a test that writes the layout found field by field:

- **No links bit from R2004:** the common entity data has the bit for the
  previous and next links only up to R2000 (the specification lists it as
  "always 1" from R2004; drawings have no such bit).
- **No xref index from R2007:** a table entry (layer, block header) has
  its 64-flag and xref dependency bits, but no xref index between them
  (seen in R2013 and R2018; R2007 and R2010 assumed the same).
- **Data storage records:** a data segment's record headers lie at their
  local offsets after the segment's header; the records start at the
  header's object data alignment offset (in 16 bytes).
- **The text form of R2000 drawings is version 400:** no end marker, and
  the topology lacks fields the later layout has (edge parameters, a
  vertex's end integer, a coedge's integer before its pcurve); an edge's
  parameters are then where its vertices lie on its curve
  (`mitcad_asm::convert::project_on_curve`).
- **The binary form of R2004 drawings is ASM 208** (the older magic): its
  vertices have no end integer either.
- **Extents never computed** are stored empty (minimum 1e20, maximum
  -1e20).
- **Surface curves without their approximation** (mitcad#589, an R2007
  drawing of ASM 21200): `par_int_cur` and `blend_int_cur` records in
  the summary form keep only the knots and fit tolerance of their
  approximation, then their support surfaces and parameter curves. The
  ASM reader (`mitcad_asm::asm::geom`, `IntGeom::OnSurface`) takes such a
  curve as its support along its parameter curve. Rolling-ball blend
  surfaces (`rb_blend_spl_sur`) in the summary form, or stored with no
  approximation at all, are made from their definition (mitcad#621): the
  ball of the offset's radius whose centre runs along the slice curve
  touching both supports (surfaces, or an edge's curve), walked by OCCT's
  blend machinery and approximated to a B-spline
  (`geometry/src/blend_surfaces.cpp`); the curves on such a blend are the
  blend along their parameter curves. So are blends stored with an
  approximation farther off than the import's tolerance (mitcad#622: fit
  tolerances of 0.0043 in, a tenth of a millimetre, left the boundary of a
  blend face round a whole rim open), with their contact curves
  (`blend_int_cur`) as the blend along their parameter curves. Where such
  a blend cannot be made (OCCT's walk fails, or its radius varies), the
  file's approximation stays its surface, as before, and the import report
  says so with a warning per face: the 3DSOLID and body, the face record,
  the approximation's fit tolerance in mm and why. Since mitcad#639 (a
  ball rolling on a circular rim level with its centre, slices starting a
  few micrometres off the ball's centre, a ball rolled once round a blend
  of its own) no blend of the sample drawings keeps its approximation.

The R2007 container and compression were met later, in drawings of user
reports (mitcad#563, mitcad#589). Not met in the trial drawings, so still
as the specification reads: the BLL width, entity colours with RGB
or colour book flags, `MESH` entities, polyface and polygon meshes,
surfaces, and the text form of R2004-R2010 drawings.

## Versions and containers

| Version | Id | Container |
|---|---|---|
| R13, R14, R2000 | `AC1012`, `AC1014`, `AC1015` | Section locators after the version; header variables, classes and the object map at absolute addresses; objects at the absolute addresses the map gives (`container::read_r13`) |
| R2004, R2010, R2013, R2018 | `AC1018`, `AC1024`, `AC1027`, `AC1032` | The masked file header data at 0x80 (XOR with the generated sequence), the page map and the section map (system pages), data sections in pages with masked 32-byte headers, each compressed (`lz`) |
| R2007 | `AC1021` | The file header at 0x80 and the system pages Reed-Solomon coded (255, 239) and interleaved, data pages (255, 251), the R2007 compression (`r2007`) |

Releases before R13 and drawings saved with a password (encrypted data)
are refused with a message.

- **Reed-Solomon (R2007):** the code is systematic, so the reader takes
  the data bytes and ignores the parity (it corrects no errors).
- **R2004 compression:** a copy's offset counts back from the last byte
  written, plus one (offset 0 is the byte just written).
- **R2007 compression:** literal runs are stored in the byte order of the
  specification's table (5.10.1), which the reader applies per run of up
  to 32 bytes; an opcode with high nibble 0 after a copy starts a literal
  run.
- The R2004 masked page header's mask is `0x4164536b` XOR the page's file
  address.

## Bit codes and objects

`bits`: B, BB, 3B, BS, BL, BLL, BD, DD, BT, BE, RC, RS, RL, RD, MC, MS,
H (handle references, offsets applied to the referring object's handle),
T/TU/TV, OT (a bit short before R2010, a bit pair and one or two bytes
after), and from R2007 the object's string stream (the flag bit before
the end of the data, the size before it, and the 0x8000 extension).

- **BLL:** the byte count is read as three bits (0 to 7); the
  specification's "3B" cannot give every count.
- **Objects:** MS size; from R2010 the handle stream's size (MC); OT;
  from R2000 to R2007 the data's size in bits; the handle; extended data;
  for entities the graphics flag (and data); R13-R14 the size in bits.
  The data then starts with the common entity or object data
  (20.4.1, 20.1); the handle stream starts where the data ends.
- **Entity colours (R2004 on):** the flags are the high byte of the bit
  short, an RGB value (0x8000) is read as a bit long, a transparency
  (0x2000) as a bit long, a colour book handle (0x4000) is in the handle
  stream.
- **The common entity handles:** owner (entity mode 0), reactors, the
  extension dictionary, R13-R14 layer and line type, R13-R2000 previous
  and next (when not implied), R2004 the colour book handle, the layer,
  line type, material (R2007), plot style, visual styles (R2010); the
  entity's own handles follow.
- **INSERT and MINSERT:** from R2004 the owned object count only when
  attributes follow.

## Header variables

`header` reads the variables of chapter 9 up to `$INSUNITS`, taking the
model space's `$EXTMIN` and `$EXTMAX` on the way. Texts and handles of
R2007 and later are in their own streams, which the reader skips; the
start of the data (an R2007 bit size, the R2013 `$REQUIREDVERSIONS`) is
found by the first value, which is 412148564080.0 followed by three 1.0
in every drawing.

## Classes

`classes`: the class number (from 500), the names and whether it makes
entities. The section's start differs between the versions and the
chapters (10.2 and 5.8) do not agree on every width; the reader tries the
layouts they describe and takes the one whose records number from 500.
Only the class entities the import uses need them: `MESH` and the
surfaces.

## The data storage (R2013 on)

`datastore`: the segment index, the data index and the data segments'
records with their objects' handles; a record larger than a segment is a
blob of pages. The B-rep record of a solid or region is the record whose
bytes are the binary ASM form.

## 3D content

| Entity | Comes in as |
|---|---|
| `3DSOLID`, `BODY`, `REGION`, surfaces with B-rep data (`PLANESURFACE`, `EXTRUDEDSURFACE`, `LOFTEDSURFACE`, `REVOLVEDSURFACE`, `SWEPTSURFACE`, `NURBSURFACE`) | Bodies: the B-rep record (text form in blocks, masked c → 0x9F - c, up to R2010; binary form in the entity or in the data storage) parsed by `mitcad_asm::asm` (`AsmFile`, the text through `asm::text`) |
| `3DFACE` | Mesh bodies: one per block, layer and colour |
| `POLYLINE` polyface mesh (with its vertices and face records) | A mesh body |
| `POLYLINE` polygon mesh (M × N vertices, closed in M or N) | A mesh body |
| `MESH` (subdivision mesh) | A mesh body of its control mesh |
| `INSERT`, `MINSERT` | Occurrences of the block's component (`scene::Plan`), or copies |
| 2D entities, 2D and 3D polylines, paper space | Left out, counted by type with why |

- **The text form:** its records as in the binary form; `asm::text`
  writes them as the binary form for `AsmFile`. Logical words become
  booleans (`reversed`, `double`, `in`, ... true; interval ends `I`
  infinite, `F` finite), positions and enumerations are read by the
  geometry cursor in their text spelling. Versions before 700 get the
  history integer and pointer after the attribute pointer; files without
  an `asmheader` record get one.
- **Subdivision meshes:** read as version, blend crease, level, the
  vertices, the face list (each face its vertex count and indices), the
  edges and the creases (the order DXF writes them in); a mesh that does
  not read so is left out with why.
- **Polylines' vertices:** from R2004 the polyline's list; before, the
  chain from its first to its last vertex; else the vertices it owns, by
  handle.

## Plan of components

`scene::Scene` collects each block's bodies, meshes and inserts (model
space too), the layers and their colours, and accounts for every entity.
`Scene::plan` makes the components: a named block with 3D content is a
component placed by each insert whose transform (OCS of the extrusion,
insertion point, rotation, scale, base point) is rigid; anonymous blocks
and inserts that scale or mirror are copied into the inserting component
with the transform; an array insert places one occurrence per cell.
Colours: the entity's own, `BYLAYER` the layer's, `BYBLOCK` the
inserting entity's.

The plan places at most `scene::PLAN_BUDGET` (10^6) bodies and
occurrences in the world, an occurrence's contents counted once per
occurrence at every level (an array may have 32767 × 32767 cells, and
nested arrays multiply); what does not fit is left out with a warning
(mitcad#931). The cells of an array are made one at a time.

## Damaged and hostile files

The reader runs on files from anywhere (in the import worker process):
a damaged field gives `DwgError::Corrupt`, never a panic or an
allocation the file's data does not back (mitcad#931). Offsets and sizes
read from the file are added with checks (`container::span`); a section,
page or system page that claims more than 64 times the file (at least
1 MiB, `container::size_limit`) is refused; decompression is not
pre-sized from the size a page claims, and an R2007 data page keeps only
the bytes its data gives (the section's bytes are zeros past them); an
R2004 page decompresses into at most 0x7400 bytes or what its section
still takes; lists counted by a field of the file grow with the data
read rather than being allocated for the count.

## Writer

`write::drawing` writes a 2D drawing (`mitcad_dxf::Drawing`: the drawing
export's sheet with a layer per pen, mitcad#728) as an R2000 (`AC1015`)
file, the version other drawing programs read most widely and the one
with the plain container: the sections one after another at the
addresses the file header's five locators give, nothing compressed.
MitCad's own code from the same specification as the reader; no
third-party writer.

The 3D content needs the `.f3d` reader's ASM code: the feature `solids`
(on by default; `core/ffi` asks for it) brings `scene`, `testfile`,
`solid::parse`, `mitcad-dwg-inspect` and the dependency on `mitcad-asm`.
`mitcad-model` uses the crate without it (`default-features = false`),
for the writer, the reader of the sections and objects and `flat`; so
the model does not depend on the `.f3d` reader, whose tests use the
model.

- **The file:** the file header (version, the preview's address, code
  page 30 (ANSI 1252), the locators of the header variables, the classes,
  the object map, the object free space record and the template record,
  their CRC XOR 0x3CC4 and the sentinel), the header variables, the
  classes, 0x200 bytes of padding, the objects in handle order, the object
  map, the object free space record (the object count, the update date,
  where the objects start), the template record (`MEASUREMENT`) and an
  empty preview. The second file header of R13 to R2000 is left out:
  nothing points to it, and readers find the sections by the locators.
- **The header variables:** every variable of chapter 9 for R2000, with
  the defaults of a metric drawing (ISO dimension values, `LUNITS` 2,
  `LUPREC` 4), `INSUNITS` and `MEASUREMENT` from the drawing's units, the
  extents of what it holds, the paper as the limits, the creation and
  update dates, the handles of the tables, dictionaries, blocks and line
  types below, and identifiers made from the drawing's content and date.
- **The objects:** the ten table controls (blocks, layers, text styles,
  line types, views, UCSs, viewports, applications, dimension styles,
  viewport entity headers); layer `0` and the drawing's layers (a layer
  that is off has its colour negated and the 2 bit of its flags set, which
  the specification calls "on" but drawings leave clear on layers that
  are on); the text style `Standard` (font `txt`); the line types
  `ByBlock`, `ByLayer`, `Continuous`; the application `ACAD`; the
  dimension style `Standard`; the viewport `*Active` showing the whole
  drawing; the named objects dictionary with `ACAD_GROUP`, `ACAD_LAYOUT`
  (`Model`, `Layout1`), `ACAD_MLINESTYLE` (`Standard`),
  `ACAD_PLOTSETTINGS` and `ACAD_PLOTSTYLENAME` (a dictionary with the
  default `Normal`, a placeholder); `*Model_Space` and `*Paper_Space`
  with their BLOCK and ENDBLK entities and layouts. The classes are those
  three objects without a fixed type: `ACDBDICTIONARYWDFLT`,
  `ACDBPLACEHOLDER`, `LAYOUT`.
- **The entities,** in model space, colour, line type and line weight by
  layer, each linked to the one before and after it: `LINE`, `ARC`,
  `CIRCLE` (an arc of a full turn), `POINT`, `ELLIPSE`, `SPLINE` (by its
  control points and knots, or its fit points) and `TEXT` (left aligned
  on its baseline, a text of several lines as a text per line, each 5/3
  of the height below the one before).
- **Texts** are in code page 1252; other characters are written as
  `\U+XXXX`, and `%%` (which starts a control code) as `%%%%%%`
  (`write::encode`, `write::decode`).
- Where the specification lists a field only some readers expect (the
  count of further handles of the dimension style control, four shorts at
  the end of the header variables), it is written: a reader that does
  not expect it stops at the object's or section's size.

The reader decodes `LINE`, `ARC`, `CIRCLE`, `POINT`, `ELLIPSE` and `TEXT`
(`objects::EntityData`; one that does not read stays undecoded, as other
2D entities), and `flat::drawing` gives a drawing's model space back as a
`mitcad_dxf::Drawing`: the tests read what the writer writes. The import
leaves 2D entities out as before.

## Tests

- `write` in `core.dwg`: a drawing of every entity kind written and read
  back: every object read, the header variables, the classes, the layers
  (one off), the entities' links, the geometry exactly and the texts with
  their characters; the file header's CRC, each object's CRC, the free
  space record, the template record and the preview; an empty drawing;
  invalid geometry refused.
- `core.dwg`: the bit codes (the specification's examples), the CRCs, both
  compressions, the object map and the data storage round trips, and a
  test drawing (`testfile`: a small truss of blocks, solids in both forms,
  3D faces, a polyface and a subdivision mesh, a line) written and read
  back in every version: header variables, classes, objects, the B-rep
  records parsed and converted, the scene and the plan.
- `asm::text` in `core.f3d`: MitCad's ASM test bodies written in the text
  form convert exactly as their binary form.
- `cli.import_dwg_<version>`: the test drawing imported through OCCT.
- `tools/ui-dwg-import-test.sh`: the test drawing opened in the
  application: Cancel while the import process reads it and while the
  window reads the imported design, the window answering throughout
  (mitcad#589).
- `dwg.corpus`: the trial models under `MITCAD_DWG_CORPUS`
  ([development.md](../../docs/development.md#dwg-corpus)).

`mitcad-dwg-inspect <file.dwg>` prints what the reader finds (sections,
header, classes, object counts, objects not read, the account and the
plan, each B-rep record's parse).
