# ASM binary format of .f3d body blobs

The body blobs of `.f3d` files (`Breps.BlobParts/BREP.<guid>.smb` and
`.smbh`) hold B-rep entity records, their geometry and, in `.smbh`, their
history. The format is named by the magic at the start of every blob.

**Method.** Black-box only: 229 blobs from 41 files (ASM 224.4 to 232.3,
Windows and macOS writers) were tokenised, compared and checked against
geometry (vertices on their curves and surfaces, closed loops, valid OCCT
solids with positive volume). No writer code was decompiled and no code of
other readers was used. *(verified)*: holds for the whole corpus;
*(assumed)*: plausible, not confirmed by the data.

## File structure

| Offset | Content |
|---|---|
| 0 | Magic, 15 bytes: `ASM BinaryFile4` or `ASM BinaryFile8` |
| 15 | Four integers of 4 or 8 bytes (the magic's digit): save version, record count, top-level entity count, flags |
| | Three strings (tag 0x07): writing product's name, ASM version (`ASM 231.6.3.65535 NT`), save date |
| | Three doubles (tag 0x06): units, `resabs` (1e-6), `resnor` (1e-10) |
| | Records until `End-of-ASM-data` |

- **BinaryFile4 vs BinaryFile8** *(verified)*: the digit is the byte width
  of every integer-valued token (integer, pointer, enum) and of the four
  header integers. BinaryFile8 blobs come from macOS builds (version string
  ends in `OSX`), BinaryFile4 from Windows (`NT`): the writer's native
  `long`. Nothing else differs.
- **Save version**: ASM major version x 100 (22400 ... 23200). Some subtype
  objects carry their own version integer (22404, 22601, 22800, 23100).
- **Record count**: always 0.
- **Top-level entity count** *(verified)*: the first that many records
  (including `asmheader`, record 0) are the entities saved explicitly; the
  rest is what they reference. In `.smb` blobs these are bodies plus faces
  and edges the design refers to; in assembly designs only faces, whose
  bodies are reached through the owner pointers.
- **Flags**: 2 in `.smb`, 3 in `.smbh` (4 and 5 in ASM 226 and 227). Bit 0
  is set exactly when the blob carries history.
- **Units** *(verified for `.smb`)*: millimetres per model unit. Every
  `.smb` has 10 (model unit centimetre). `.smbh` blobs hold 0, 10, 20, ...
  70 although their geometry is also in centimetres; the meaning is open.
  Mitcad always scales by 10.

## Tokens

A one-byte tag followed by its data, little-endian.

| Tag | Meaning | Data |
|---|---|---|
| 0x02 | char *(assumed, not seen)* | 1 byte |
| 0x03 | short *(assumed, not seen)* | 2 bytes |
| 0x04 | integer | 4 or 8 bytes (see BinaryFile4/8) |
| 0x05 | float *(assumed, not seen)* | 4 bytes |
| 0x06 | double | 8 bytes |
| 0x07 | string | u8 length + bytes |
| 0x08 | string *(not seen)* | u16 length + bytes |
| 0x09 | string *(not seen)* | u32 length + bytes |
| 0x0a | true | - |
| 0x0b | false | - |
| 0x0c | pointer: record index, -1 for none | 4 or 8 bytes |
| 0x0d | identifier: record type, subtype or keyword name | u8 length + bytes |
| 0x0e | identifier prefix, joined to the next identifier with `-` | u8 length + bytes |
| 0x0f | subtype object start `{` | - |
| 0x10 | subtype object end `}` | - |
| 0x11 | record end | - |
| 0x12 | long string *(not seen)* | u32 length + bytes |
| 0x13 | position | 3 doubles |
| 0x14 | vector | 3 doubles |
| 0x15 | enumeration value | 4 or 8 bytes |

Booleans are tags 0x0a/0x0b; `T` below means true. Keywords such as
`nubs`, `nullbs`, `null_surface`, `plane` or `both` are identifiers (0x0d),
not strings.

## Records

- A record starts with its type name: zero or more prefix tokens (0x0e)
  and an identifier (0x0d), joined with `-` (`plane-surface`,
  `intcurve-curve`, `tcoedge-coedge`, `ATTRIB_CUSTOM-attrib`). Fields
  follow until the record end tag.
- Records are numbered from 0 in file order; pointers are these numbers.
- Markers: `End-of-ASM-data` ends the file (no fields, no end tag).
  `End-of-ASM-History-Section` has no fields and takes no record number.
  `Begin-of-ASM-History-Data` and the following `delta_state` records are
  numbered, but pointers skip them: the entity copies after
  `End-of-ASM-History-Section` continue the pointer numbering of the live
  entities *(verified: every bulletin pairs an entity with a copy of the
  same type)*.
- **Subtype objects** `{ name ... }` hold procedural geometry. They are
  numbered in file order of their opening brace, nested ones included
  (pre-order); `{ ref N }` reuses object N *(verified: every ref points
  back to an object of the expected kind)*.

### Entity header

Nearly every entity record starts with: attribute pointer, an integer
(history id or tag, usually -1), a pointer that is always -1. `transform`
lacks the third field. The layouts below start after the header.

### Topology

| Record | Fields after the header |
|---|---|
| `asmheader` | (no third field) string: ASM version |
| `body` | lump, wire, transform |
| `lump` | next lump, shell, body |
| `shell` | next shell, subshell, first face, wire, lump |
| `face` | next face, first loop, shell, subshell, surface, sense (`T` reversed), double-sided, [containment, only when double-sided] |
| `loop` | next loop, first coedge, face |
| `coedge` | next, previous, partner, edge, sense (`T` reversed against the edge), loop, integer (0), pcurve |
| `tcoedge-coedge` | coedge fields, then two doubles (parameter range), pointer, integer, the coedge's own 3D curve (`intcurve ...` or `null_curve`), integer |
| `edge` | start vertex, start parameter, end vertex, end parameter, a coedge, curve, sense (`T` reversed against the curve), convexity string (`unknown`, `convex`, `concave`, `tangent`, `tangent_not_g2`, ...) |
| `tedge-edge` | edge fields, tolerance, integer (version, e.g. 23100), integer |
| `vertex` | an edge, integer (0 or 1: which end of that edge), point |
| `tvertex-vertex` | vertex fields, integer (-1), two doubles (tolerances), integer |
| `point` | position |
| `transform` | (attribute, integer) three vectors (images of the x, y and z axes), translation vector, scale, three booleans (rotation, reflection, shear) |

Linked lists end with -1; the coedges of a loop form a ring (next of the
last is the first). No `subshell` or `wire` records occur in the corpus.

**Degenerate edges**: an edge without a curve (pointer -1) occurs at cone
apexes (presumably also at sphere poles) and starts and ends at the same
vertex. When it is the only coedge of its loop (the side face of a pointed
cone has the base circle loop and an apex loop), Mitcad keeps the loop as a
point loop of the face and the builder bounds the face there with a
degenerated edge. In a loop with other edges it is left out (OCCT's healing
adds the degenerated edge).

Conventions *(verified)*:
- A coedge with sense `T` runs against its edge; with this every loop
  closes.
- Edge parameters run along the edge. With edge sense `T` the curve
  parameter is the negated edge parameter: the start vertex lies at curve
  parameter `-start`.
- On periodic B-spline curves (closure 2) an edge may cross the seam: its
  parameters run past the end of the knot range and wrap around. On other
  approximations they may overshoot the knot range by rounding.
- Loops have the face on their left seen against the face's outward
  normal, which is the surface normal negated when the face sense is `T`.
- A shell of a sheet body may consist of face groups that share no edge.
- The body transform is identity (or missing) in every corpus file; its
  interpretation (rows = images of the axes, then translation) is
  *(assumed)*.

### Analytic surfaces and curves

| Record | Fields after the header |
|---|---|
| `straight-curve` | root, direction, parameter range (2 ends) |
| `ellipse-curve` | center, normal, major axis vector (length = major radius), ratio minor/major, range |
| `plane-surface` | root, normal, u direction, reverse-v flag, u range, v range |
| `cone-surface` | base ellipse center, axis, major axis vector, ratio, base ellipse range, sin and cos of the half angle, u scale (= radius), reverse-v flag, u range, v range |
| `sphere-surface` | center, radius, u direction, pole direction, reverse-v flag, ranges |
| `torus-surface` | center, axis, major radius, minor radius, u direction, reverse-v flag, ranges |
| `spline-surface` | reversed flag, subtype object, u range, v range |
| `intcurve-curve` | reversed flag, subtype object, range |
| `pcurve` | integer kind; 0: reversed flag, `{ exp_par_cur ... }`; otherwise (e.g. 2, -2) a pointer to an `intcurve-curve` whose parameter curve is used; then two doubles |

A range end is `F` (unbounded) or `T` followed by a double.

Conventions *(verified by vertices on the surfaces and valid OCCT solids
with positive volume)*:
- Straight curve: `root + t * direction`. Ellipse: `center + major cos t +
  ratio (normal x major) sin t`, the same parameter as OCCT.
- Cone: at height `h` along the axis from the base the radius is
  `r0 + h * sin/cos`; `sin = 0` is a cylinder. The normal points away from
  the axis when `cos > 0`, towards it when `cos < 0`.
- Sphere with negative radius and torus with negative minor radius have
  inward normals.
- Torus: `center + (R + r cos v)(cos u X + sin u Y) + r sin v Z` with
  `X` = u direction. A negative major radius `R` (|R| < r) is a lemon
  torus with the same formula; Mitcad builds it as the surface of
  revolution of that meridian circle.
- Cone with ratio not 1 and `sin = 0` is an elliptic cylinder, built as the
  extrusion of the base ellipse along the axis. Elliptic cones with an
  angle do not occur in the corpus.
- `spline-surface` with reversed flag `T`: normal = -(du x dv).
  `intcurve-curve` with flag `T`: the curve parameter is the negated
  parameter of its definition.

### Embedded geometry

Inside subtype objects, surfaces and curves are written without a record:
a keyword followed by the record's fields (without the header): `plane`,
`cone`, `sphere`, `torus`, `spline` (reversed flag + subtype + ranges),
`null_surface`; `straight`, `ellipse`, `intcurve` (reversed flag + subtype
+ range), `null_curve`.

### B-spline data

- **bs3 curve**: `nubs` or `nurbs` (rational), degree, closure enum (0
  open, 1 closed, 2 periodic), knot count, (knot, multiplicity) pairs,
  control points x y z (+ weight when rational). End knots have
  multiplicity = degree, so there are `sum(mult) - degree + 1` control
  points; OCCT needs `degree + 1` at the ends. `nullbs` = no curve.
  Periodic curves are also stored clamped (first and last pole coincide);
  Mitcad makes them periodic in OCCT (`SetPeriodic`) so that edges can
  cross the seam.
- **bs2 curve** (parameter space): the same with u v (+ w) points.
- **bs3 surface**: `nubs` or `nurbs` + rational kind (`both`, `u`, `v`),
  u degree, v degree, four enums (u closure, v closure, u singularity, v
  singularity), u knot count, v knot count, u knots, v knots, control
  points with u varying fastest *(verified with rational surfaces of
  revolution: the weights of a circular arc follow v)*.

### Subtype objects

| Subtype | Layout (start) | Use |
|---|---|---|
| `exact_int_cur` | version, enum, bs3 curve, fit tolerance, two surfaces, two bs2 curves, ... | exact B-spline |
| `int_int_cur` | version, enum, bs3 approximation, fit tolerance, the two intersected surfaces, their pcurves, ... | approximation |
| `blend_int_cur`, `off_int_cur`, `par_int_cur`, `offset_int_cur` | version, enum, bs3 approximation, ... | approximation |
| `helix_int_cur` | version, range, center, major axis vector, minor axis vector, pitch vector (per turn), taper, axis, ... | sampled and interpolated: `center + major cos t + minor sin t + pitch t / 2pi` |
| `exp_par_cur` | bs2 curve, fit tolerance, the surface | not used (OCCT computes pcurves) |
| `exact_spl_sur` | version, enum, bs3 surface | exact B-spline |
| `cyl_spl_sur` | version, profile (`intcurve ...` + range), direction vector, point, approximation level enum (0: bs3 surface follows; 2: none), ... | exact: linear extrusion `C(u) + v dir` |
| `rot_spl_sur` | version, profile, axis point, axis direction, enum, bs3 approximation | approximation |
| `rb_blend_spl_sur` | version, support surfaces/curves with their spines and offsets, radius data, then enum and bs3 approximation | approximation |
| `loft_spl_sur` | version, section curves, ..., bs3 approximation | approximation |
| `helix_spl_line` | version, three ranges, helix data as in `helix_int_cur`, two `null_surface`, two `nullbs`, line vector; no approximation | exact up to the sampling (see below) |

All procedural curves except helices, and all procedural surfaces except
`helix_spl_line` and `cyl_spl_sur` with approximation level 2 (built
exactly anyway), carry a B-spline approximation directly in their subtype
object (not in a nested one). The approximation is used where no exact
construction is implemented; its parameterisation is that of the
definition.

**`helix_spl_line`** (thread flanks) *(verified)*: a straight line swept
along a helix. Layout: version (22601), three intervals (the line's range,
then twice the helix's), center, major and minor axis vectors, pitch vector
(advance per turn), taper, unit axis, two `null_surface`, two `nullbs`,
line vector `L`. With `r(v) = major cos v + minor sin v`:
`S(u, v) = center + (1 + taper v / 2pi + L.x u) r(v) + pitch v / 2pi + L.z u axis`.
`u` is the arc length along the line, whose direction has radial component
`L.x |major|` and axial component `L.z` (60 degree thread:
`L.x |major| = 0.866`, `L.z = -0.5`; an M3 thread has `L.x = 6.8488` at
radius 0.12645 cm); `v` is the helix angle. `L.y` is always 0 (presumably
tangential). The surface record's u and v ranges bound the face. Since
motion along a helix is rigid, `S` is the ruled surface between the helices
of the line's two ends: Mitcad samples both at the same angles (48 points
per turn) and OCCT interpolates a ruled B-spline surface. The faces' edges
(`helix_int_cur` of the same helices) lie on the built surfaces within the
edge tolerance.

### Attributes and history

- Attributes are skipped: `ATTRIB_CUSTOM-attrib` (named definitions:
  `generic_tag_attrib_def` with the design's entity tags,
  `sketch_attrib_def`, `Timestamp_attrib_def`), `DXID-attrib`,
  `no_merge_attribute-st-attrib`, `vertedge-sys-attrib`.
- History (`.smbh` only; layout in `src/asm/history.rs`):
  `Begin-of-ASM-History-Data`, `delta_state` records,
  `End-of-ASM-History-Section`, copies of earlier entity states,
  `End-of-ASM-data` *(verified on the corpus; meanings partly assumed)*:
  - `history_stream` (the identifier after `Begin-of-ASM-History-Data`):
    integers (current state number twice, 0, a count), then pointers -1,
    current state, oldest state, -1. These pointers number the delta
    states from 0 in file order.
  - `delta_state`: state number (later states larger), 1, 0, pointers to
    the newer state, the older state and itself, -1, 0, a boolean, then
    bulletin boards: `1 <state> 2`, bulletins `1 <before> <after>` and a 0;
    the boards end with a 0, then one more integer. A bulletin pairs an
    entity (`after`) with a copy of its data before the operation
    (`before`). `before` -1: the operation created the entity; `after` -1:
    it deleted it, and the copy stands for it. A copy has the entity's
    record type.
  - Following the older-state pointers from the current state and
    replacing each changed entity by its copy (`History::view`,
    `convert_body_at`) gives the bodies before the newest operations; they
    convert and build like the current ones.
  - Delta states are ASM operations, not timeline features: a feature can
    take several states, and some states change only attributes.

## `.smb` and `.smbh`

- `.smbh`: the current bodies of the design (often exactly one) followed
  by their ASM history; flag bit 0 set. Every display mesh saved with a
  document ([OGS_FORMAT.md](OGS_FORMAT.md)) matches an `.smbh` body.
- `.smb`: B-rep the timeline keeps: sketch profile faces (sheets, often
  double-sided), construction sheets, the faces and edges the design
  refers to (saved at the top level; assembly files save only faces of the
  components, whose bodies are found through the owner pointers), and
  solids. No history.
- Most top-level `.smbh` bodies are identical to an `.smb` body, or equal
  one after rolling their history back: the `.smb` keeps bodies at earlier
  timeline steps (e.g. a nut before its threaded hole).
  `core/ffi/src/f3d_import.rs` relies on this to assign history blobs to
  components.
- Each design names its blobs in UTF-16 (`BREP.<guid>.smb`) in the design
  segment's `BulkStream.dat`. In multi-component designs every component
  has its own `.smb`/`.smbh` pair, named right after the component's name;
  `F3dFile::blob_references` reports the strings before each name as a
  hint.

## Open questions

- The `.smbh` header units double (0-70 instead of 10).
- The second integer of the entity header and the always -1 third pointer.
- `pcurve` records of kind other than 0, and the two trailing doubles.
- `helix_spl_line`: the third interval and the line vector's middle
  component (always 0).
- Body transforms other than identity (none in the corpus).
- How delta states map to timeline features, and which timeline step each
  `.smb` body belongs to.
- Unknown integers in `history_stream` and `delta_state` (the count in
  `history_stream`, the 1 and 0 after the state number, the 2 of a
  bulletin board, the trailing integer).
- Full layouts of the procedural subtypes after their approximations
  (support surfaces, discontinuity information, ranges); not needed so far.
