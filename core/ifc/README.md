# mitcad-ifc: the IFC file reader

`core/ifc` reads IFC files (Industry Foundation Classes, ISO 16739-1): the
ISO 10303-21 text of the schemas IFC2X3 TC1, IFC4 ADD2 TC1 and IFC4X3
ADD2, also zipped (`.ifczip`, a zip archive of one `.ifc`). It builds no
geometry: the IFC import (mitcad#273, `core/import-ifc`,
`core/ffi/src/ifc_import.rs`, the `import_ifc` command in
[commands.md](../model-api/src/commands.md#ifc-import), `mitcad-cli
import-ifc`, File › Open) builds the bodies through the model's kernel from
what this crate decodes.

The reader follows the published standards: the clear-text encoding of
ISO 10303-21 and buildingSMART's specification of the three schemas. The
subset of entities and attributes it reads was written by hand from the
specification's text; no EXPRESS schema file and nothing generated from
one is used or shipped (the specification is CC BY-ND 4.0). Other readers
of the format were neither copied, translated nor linked.

## Modules

| Module | Contents |
|---|---|
| `step` | ISO 10303-21 records: a byte stream split into records a buffer at a time (comments left out, strings kept whole, `''` in strings), parsed into header entities, section keywords and entity instances, complex instances too. Values: `$`, `*`, integers, reals, strings with their `\X2\…\X0\`, `\X4\…\X0\`, `\X\hh`, `\S\c` and `\\` escapes decoded, enumerations, binaries, references, lists and typed values. |
| `lib.rs` | `IfcFile`: the header (descriptions and the view definition, file name, originating system, schemas), the instances by number with their types, counts per type, warnings about records that cannot be read and numbers given twice. `ReadOptions::import` leaves out the attributes of the property values the import does not read (bounded, list, table and complex values, templates) and the space boundaries' connection geometry (their types stay known); property sets with their single and enumerated values are kept (mitcad#496); `.ifczip` by its content. |
| `schema` | The subset: which products are spatial elements, spaces, openings, grids, annotations, ports, structural items or virtual elements (every other product is an element), and which representations are bodies. |
| `units` | The project's units (IfcUnitAssignment): SI units with prefixes and conversion-based units (feet, inches, degrees) as millimetres, radians, mm² and mm³. |
| `math` | Points, directions and affine transforms (3 × 4 rows). |
| `model` | The products (recognised by their attributes: GlobalId, ObjectPlacement, Representation), their absolute placements (local placements relative to theirs, grid placements at their grid's axes' intersections), the spatial structure (IfcRelAggregates, IfcRelContainedInSpatialStructure), openings and their fillings, types and their representation maps, materials (layer sets, constituents, profiles), element quantities (volumes), the colours of styles (IFC2X3's presentation style assignments and IFC4's styles directly), shape representations with the bodies first, grids' axes. |
| `properties` | Property sets (single and enumerated values) and element quantities (lengths, areas, volumes, counts, weights, times) as named values in MitCad's units, with measure units where a single value states one; an element's own sets (IfcRelDefinesByProperties, also an IFC4 set of definitions) and its type object's (HasPropertySets) are listed by the model (mitcad#496). |
| `geometry` | Representation items decoded in millimetres: curves (polylines, lines, circles and ellipses whole or trimmed by points or parameters, composite curves, indexed poly curves with arcs, B-splines rational or not, the 3D curves of surface and intersection curves), profiles (rectangle, rounded and hollow rectangle, circle, hollow circle, ellipse, I, asymmetric I, L, U, T, Z, C and trapezium with their fillet and edge radii, arbitrary closed profiles with voids, derived and composite profiles), extrusions (tapered too), revolutions, sweeps along a curve on a plane or with a fixed reference, swept disks, booleans and clipping results, half spaces (plain, boxed and polygonally bounded), CSG primitives, faceted breps and surface models, triangulated and polygonal face sets (polygons with holes triangulated), advanced breps (planes, cylinders, cones, spheres, tori, B-spline surfaces, swept surfaces; edges with their curves and vertices), mapped items, bounding boxes. What is not read is an `Unsupported` item that says why. |
| `testdata` | A small IFC4 building written for the tests: storeys, a wall with an opening and a window of a type, a slab with a hole, an I-beam, a clipped column, a faceted cube, a tessellated tetrahedron, a space, a grid and an element without a shape. |
| `bin/mitcad-ifc-inspect` | `header`, `entities`, `tree`, `products [--json]`, `entity <id> [depth]`, `units`, `items <id> [--json]`. |

## Reading

- A record ends at `;` outside strings and comments; the reader keeps
  one record in memory at a time, so a file's text is never held whole.
  The instances are kept by number; a large file's memory is mostly its
  geometry. A record that cannot be parsed is a warning (the first 100 are
  listed), and the reader goes on.
- The schema is the first FILE_SCHEMA name: `IFC2X3`, `IFC4`, `IFC4X3…`;
  another name reads as IFC4.
- Attributes are read by their positions, which the three schemas keep
  for what is read here (later schemas add attributes at the end or make
  earlier ones optional): IFC4 shape profiles' extra radii and slopes
  are read where present, `IfcQuantityVolume` with or without its formula.
- Products are recognised by IfcProduct's attributes, so element types of
  any of the three schemas (IFC4X3's courses, track elements, earthworks
  and others) are read without a list of them.
- A placement relative to another composes with it; a grid placement lies
  where its two grid axes (offset by the given distances, to their left)
  cross, in the grid's placement. A placement that cannot be read is
  noted on its product, which then stands at the origin.
- A mapped item's transform maps the representation map's origin onto the
  target: `target × origin⁻¹`. Cartesian transformation operators may
  scale non-uniformly and mirror.
- An arbitrary closed profile whose curve does not end where it starts is
  closed with a line (exports leave out the closing segment).
- Trimming parameters of circles and ellipses are plane angles in the
  project's angle unit; points are preferred where the file says so.
- Limits against hostile files (mitcad#930): values nest at most 64
  levels in a record (deeper is a record not read, not a stack overflow);
  items, curves and profiles nest at most 64 levels. Within one decoded
  item an entity used several times is decoded at most twice, then
  copied; an item that would copy more than 100 000 segments, points and
  nodes that way is unsupported as a whole ("too many nested items"). A
  B-spline's knot multiplicities must sum (without wrapping) to poles +
  degree + 1, each at most degree + 1.

## Tests

`cargo test -p mitcad-ifc` (the ctest `core.ifc`): the records and values
with every escape, buffers of one byte, files that are not ISO 10303-21,
the attributes left out, `.ifczip`; units (millimetres and degrees, feet
and cubic centimetres); the test building's model (structure, placements,
openings, types, materials, styles, quantities, grid axes) and its items
(profile areas, the I-beam's, the clipped column, the cube, the
tetrahedron, the mapped window); rounded profiles' areas; three-point
arcs; conics as rational B-splines; triangulated polygons with holes;
the limits of mitcad#930 (a record of 100 000 nested parentheses,
40-level self-doubling booleans and composite curves, B-spline
multiplicities that would wrap).

The corpus test (`tests/corpus.rs`) reads every `.ifc` that the index of
`MITCAD_IFC_CORPUS` lists (`index.tsv`; files named by the first 8 hex
digits of their SHA-256): each opens without records it cannot read, its
schema is the one the index gives, it has a project, units and readable
placements, and every body item of every product (and of the
representation maps they use) decodes. Skipped without the corpus. The
import's corpus test is the ctest `ifc.corpus`
([tools/cli/ifc-corpus.cmake](../../tools/cli/ifc-corpus.cmake)).
