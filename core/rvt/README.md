# mitcad-rvt: the `.rvt` project and `.rfa` family file reader

`core/rvt` reads `.rvt` project files and `.rfa` family files (and their
templates, `.rte` and `.rft`): the container, the metadata, the file's own
serialisation schema, the element table and the element records, walked
field by field by that schema, as typed views (category, class, level,
family and type, transform, bounding box, saved meshes, levels,
materials), the building elements' parameters (mitcad#275) and the
geometry the file keeps of its elements (B-reps, mitcad#333). It builds no
bodies: the `.rvt` import (mitcad#274, mitcad#275, mitcad#333,
`mitcad_import_rvt` in `core/import-rvt`, `core/ffi/src/rvt_import.rs`, the `import_rvt`
command in [commands.md](../model-api/src/commands.md#rvt-import),
`mitcad-cli import-rvt`, File › Open) builds the bodies through the
model's geometry kernel.

The container follows Microsoft's published specification (MS-CFB; the
`mitcad-cfb` crate, shared with the `.ipt` reader). Everything else comes
from public format notes and from inspecting files. One public reader of
the format (rvt-rs 0.4.0, Apache-2.0) was read for understanding and its
outputs on the test corpus serve as a cross-check in the tests; its code
was neither copied, translated nor linked. Where this reader and that one
disagree, this README says why. Claims are marked as in
[core/ipt](../ipt/README.md): *(verified)* means checked on every file of
the test corpus (below), *(seen)* means observed but not understood
further.

## Modules

| Module | Contents |
|---|---|
| `gzip` | Page trailers, gzip members (RFC 1952) with their CRC-32 and size checked, members inflated on demand; writers for tests. |
| `info` | `BasicFileInfo` (release, build; personal keys left out), `PartAtom`, `TransmissionData`, the preview PNG. |
| `schema` | The serialisation schema (`Formats/Latest`): classes, bases, fields, type encodings, the known unknowns. A writer for tests. |
| `walk` | Records walked by the schema: objects, pointers and the objects behind them, partial walks. |
| `partition` | The partition streams' record chains (element headers, element objects, their geometry) and the loaded families' documents after them. A writer for tests. |
| `elements` | The element table and the typed views: elements, levels, materials, type boxes, saved meshes, GlobalIds. |
| `brep` | The elements' geometry as the file keeps it: bodies (winged-edge B-reps in their faces' parameters) and placed types (mitcad#333). |
| `building` | The building elements' parameters: walls, floors, roofs, family instances and types, stair runs, grids, links (mitcad#275). |
| `categories` | Built-in categories' names and which are building elements; system families by type kind. |
| `params` | Parameters' kinds and names (mitcad#501): the kind a parameter definition's class and spec give, the specs of lengths, areas, volumes, angles and masses, the common built-in parameters' names and kinds. |
| `testfile` | A small project and family written by the writers (the tests', the CLI's and the UI test's), and a building project with a link (and its house variant: a gable roof over a wall attached to it, mitcad#348, with a door in it and a window whose type cannot cut, mitcad#345). |
| `bin/mitcad-rvt-inspect` | `list`, `info [--json]`, `stream <path> [--raw]`, `schema [<class>] [--json]`, `table`, `elements [--json\|--csv] [--all]`, `levels`, `materials`, `record <id> [--all]`, `records <class>[,…]`, `building`, `classes [<class>]`, `documents`, `geometry [<id>] [--points]`, `preview`, `write-test`. |

## The container

An MS-CFB compound file of **version 4 with 4096-byte sectors**, mini
sectors of 64 bytes and a mini-stream cutoff of 4096 *(verified)*. Entry
names repeat across storages (`Formats/Latest`, `Global/Latest`), so
streams are found by their full paths. At the root:

| Stream | Contents |
|---|---|
| `BasicFileInfo` | The saving release and build, the original path, a text block of `Key: value` lines *(verified)* |
| `Contents` | A wrapper (`62 19 22 05`) and one gzip member of UTF-16 text *(seen)* |
| `Formats/Latest` | The serialisation schema |
| `Global/ElemTable` | The element table |
| `Global/Latest`, `PartitionTable`, `History`, `DocumentIncrementTable`, `ContentDocuments` | The document's own objects, walked by the same schema *(verified for walking; not used)* |
| `Partitions/NN` | The elements' records; NN names a partition, not the release *(verified)* |
| `PartAtom` | A family's Atom XML (families only) *(verified)* |
| `TransmissionData` | The linked files (XML) *(verified)* |
| the preview stream (`…Preview4.0`) | The wrapper, then a PNG *(verified where present: 30 of 37 files)* |
| `ProjectInformation` | *(seen; not read)* |

## Pages and gzip members

The schema, every `Global/*` stream and every partition are stored in
pages of 65,249 bytes: 64,896 bytes of the stream and a 353-byte trailer,
the last page shorter and without one. With the trailers removed, the
schema is one gzip member at offset 0, a `Global/*` stream an 8-byte
prefix and one member, a partition a 44-byte header and members back to
back (3 to 935) *(verified)*. Every member is complete: its CRC-32 and
size are checked *(verified)*. The trailers' contents are not read
*(seen)*. The partition header's first word is 7, 8 or 9 by release
*(seen)*.

## Metadata

`BasicFileInfo` starts with a layout word (10 for the 2016–2018 releases,
13 for 2019 and 2020, 14 from 2021) *(verified)*; from byte 14 come
strings of an `i32` count of UTF-16 units: the release and the build
(layout 13 and later) or one text naming both (layout 10), then the
original path *(verified)*. The release is the text block's `Format`
line (2019 on), else the first string's year *(verified)*. The block's
user names and paths are left out of everything the reader shows.

The element table's class tag (its first word, see below) also names the
release, one value per release *(verified)*:

| Release | 2016 | 2017 | 2018 | 2019 | 2020 | 2021 | 2022 | 2023 | 2024 | 2025 | 2026 | 2027 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Tag | 1174 | 1191 | 1232 | 1233 | 1244 | 1271 | 1312 | 1370 | 1411 | 1451 | 1481 | 1510 |

`PartAtom` is UTF-8 Atom XML: title, id, update time, categories (term
and scheme) *(verified)*. `TransmissionData` is a `u32` count of UTF-16
units, then XML of `ExternalFileReference` elements (`ElementId`,
`ExternalFileReferenceType`, `DesiredPath`, `DesiredLoadState`, …); the
reader keeps the file names, not the folders *(verified)*. The preview is
the PNG from its signature to the end of its `IEND` chunk *(verified)*.

## The serialisation schema

`Formats/Latest` describes every class the file stores *(verified: every
corpus schema reads to its end, 3,792 classes in 2016 to 4,757 in 2027)*:

- records separated by a zero `u16`; a class is a `u16`-counted ASCII
  name, a `u16` base reference, a `u32` version, a `u32` field count, the
  fields (a `u32`-counted name and a type) and a `u32`-counted list of
  16-byte GUIDs;
- a class reference is a `u16`; with bit 15 set the class is defined
  inline right after a zero `u16`, and the low 15 bits repeat its tag
  (checked);
- a class's tag is its definition ordinal, from 12, inline definitions
  counted where they appear: tags are never stored, and they shift from
  release to release *(verified: the element records' class tags name the
  classes the reference reader names on every compared element)*;
- a type is a `u32` word: a kind byte, a modifier byte and 16 bits of
  flags; modifier 0x10 adds a `u32` array count, kind 0x0d adds
  `01 00 00 00 20` and an inner type (and a class reference when the inner
  type has one), kind 0x0e with a by-value modifier adds a class
  reference.

The schema is identical in every file of one release *(verified)*.

### Field encodings

What the walker reads for each type *(verified: every element record of
every corpus file walks to its end with these, but the extensible storage
data below)*:

| Kind | Stored as |
|---|---|
| 0x01 | a byte (boolean) |
| 0x02 | a byte |
| 0x03 | two bytes |
| 0x04, 0x05 | four bytes |
| 0x06 | a `f32` |
| 0x07 | a `f64` |
| 0x08 (modifier 0x60) | a `u32` count of UTF-16 units, then the units |
| 0x09 | 16 bytes (a GUID) |
| 0x0a | a `u16` class tag |
| 0x0b | eight bytes |
| 0x0d | its inner type |
| 0x0e, by value | the class's fields in place; an `ElementId` is its number: 4 bytes through 2023, 8 from 2024 (the schema's `Identifier` says which) |
| 0x0e, pointer 1, 2, 4 | a `u32` (0 null; all ones: an object of its own; else an object number, the first pointer with it bringing the object) and a `u16` class tag; the object follows the record's fields |
| 0x0e, pointer 3 | a `u32` (the document) |

Modifier 0x10 makes a fixed array of the type, 0x50 a container (a `u32`
count, then the elements). A field's flags 0x02 mark it as not stored
(only in the 2027 schema, on fields named as temporary, cached or unsaved
state) *(verified on the 2027 file)*.

No corpus schema has a type word outside this table (the tests fail on
one). The **known unknowns** (the storage is known, the meaning is not;
also printed by `mitcad-rvt-inspect schema`):

- flag 0x04 (`VarExpr.m_refCt` in every release): stored as its kind says;
- flags 0x10 and 0x20: seen only with the not-stored flag 0x02;
- pointer kinds 2 and 4: stored as kind 1; what sets them apart is not known;
- kinds 0x04 and 0x05: both four-byte integers; which is signed is not known;
- the classes' GUID lists.

## Records

An object is its class's fields in declaration order, the root base
class's first. The objects behind its pointers are not in place: each
pointer leaves its class tag, and the objects follow the record's own
fields in the order their pointers came, the pointers in those objects
queued behind *(verified)*. A record's walk must end exactly at its end,
so a misread is found, not passed on.

**Extensible storage.** An element whose cells hold extensible storage
entities (`ESEntityCell`) stores each entity's data (`ESEntity.m_blob`)
in a schema of its own, identified by a GUID, which `Formats/Latest` does
not describe. The walker stops there and keeps what it read: the
element's own fields always come before such data *(verified: 1,330
elements of the largest project, 1,171 of the MEP sample, 7 of each
structural sample, 1 to 5 elsewhere; one marshalled element of the MIT
project likewise)*. Reading those schemas is a later step.

## Partitions

A partition's inflated bytes hold two chains of records *(verified on
every partition of the corpus)*:

1. the element headers: `id`, `u32` size, the `ElementHeader` object (its
   `u16` class tag and fields), the size again; ended by an `id` of all
   ones and a `u64` 0;
2. the element objects, in the same order: `id`, a `u32` *(seen; not
   read)*, `u32` size, the element's object, the size again; ended by an
   `id` of all ones and a `u64` 1.

Then *(verified on every partition of the 2023 to 2027 projects)*:

3. a `u32` and a third chain like the second: per element a record of its
   geometry, a `GElement` (see [The geometry](#the-geometry-mitcad333)), or
   2 bytes (`SerializedDummy`) for an element without; ended by an `id` of
   all ones and a `u64` 1;
4. the documents of the loaded families, each a `u32`, its own two chains
   (its elements under ids of their own: a family's forms `ExtrusionElem`,
   `BlendElem`, `SweepElem`, `RevolutionElem` with their sketches, types,
   parameters), a `u32` and its third chain *(seen: 46 documents in a
   partition of the MIT project, every record walking to its end; not
   used by the import, mitcad#276)*.

The import reads a partition as far as its third chain (`Documents`);
`mitcad-rvt-inspect documents` lists the families' documents too. An
element can have records in several
partitions; the one in the partition with the highest number is the
element as last saved *(verified: on the MIT project the walls' types in
the higher partition are the IFC export's, those in the lower an earlier
state)*.

The `ElementHeader` gives `m_categroryId` (the category: a negative id of
the public built-in category enumeration), `m_familyId`, `m_ownerViewId`
(view-specific elements), `m_designOptionId`, `m_unplacedOwnerId`
(members of a group's definition), `m_viewRules`, `m_classDef` (the
element's class tag), `m_pBBox` (an `Outline`: the bounding box in model
feet) and `m_parents` (lists of elements it depends on).

## The element table

`Global/ElemTable` is an `ElemTable` object (its class tag, then its
fields): `m_elemArr`, an `ElemRec` per element (`m_history`, `m_id`,
`m_OwningElementId`, `m_partitionId`), then one 32-bit word *(verified:
the walk ends exactly 4 bytes before the stream's end)*. Its records are
28 bytes through 2023 and 40 from 2024 because the ElementId is 4 or 8
bytes. Every declared element has records in the partitions *(verified)*.

The reference reader reads this table from 24 bytes into it, in records
of 28 or 40 bytes: it leaves out the first element and (but in projects of
28-byte records) reads one record past the table; on one family it found
no record size at all. Its ids are otherwise exactly ours, in order
*(verified)*.

## The views

All values are fields the schema names, read by their names:

- **Element:** the header's category, class, box, view, design option and
  group definition; `Element.m_assocLevelId`; the family instance's
  transform (`m_pInstanceInfo` → `InstInfoBase.m_Trf`: a 3×3 matrix by
  rows, whose columns are the instance's axes, and an origin in feet) and
  placed symbol (`m_symbolId`); the host (`m_hostId`).
- **Level:** an element's own `m_assocLevelId`; else the first own
  ElementId field that names a level (`RoomElem.m_levelId`, …); else the
  first level its record names anywhere (a beam's
  `FamInstCurveDriver.m_attachmentData.m_levelReferenceId`, a column's
  `FamilyLevelRef`); else an ElementId parameter that names one; else its
  host's; else that of the first element it names that has one (a work
  plane, a stair).
- **Type:** the first own ElementId field naming a type (a class derived
  from `Symbol`) that has a name: `FamilyInstance.m_masterSymbolId`,
  `VWall.m_WallAttributesId`, `Floor.m_floorAttributesId`,
  `BuildingPad.m_buildingPadTypeId`, `StairsSupport.m_typeId`, …; a stair
  run's or landing's is its stair type's `m_runTypeId` or
  `m_landingTypeId`.
- **Names:** a type's `m_symbolInfo` → `SymbolInfo.m_name`; a family's
  `FamilyBase.m_name` (a family type's `FamilySymbol.m_familyId`); a
  level's `DatumPlane.m_text` and its elevation the origin of its
  `m_pSurface` plane; a material's `MaterialElem.m_pMaterial` →
  `Material.m_name`, `m_color` (red, green, blue in the low bytes) and
  `m_transparency`.
- **System families** are not stored: their names come from the type's
  kind, as the program shows them: `BasicWallType` Basic Wall,
  `NewCurtainWallType` Curtain Wall, `FloorAttributes` Floor,
  `BuildingPadType` Pad, `CompoundCeilingType` Compound Ceiling,
  `RoofAttributes` Basic Roof, `StairsRailingAttr` Railing,
  `CorniceAttr` Wall Sweep, `EdgeSlabAttr` Slab Edge, `RampAttributes`
  Ramp, `Text3dAttrSymbol` Model Text; stair runs and landings Monolithic
  or Non-Monolithic by `m_hasMonolithicSupport`, stairs Cast-In-Place
  (a monolithic run type) or Assembled, stair supports Stringer or
  Carriage by the stair type fields that name them *(verified: every
  compared element's family name)*.
- **Type boxes:** a family type's `FamilySymbol.m_outline`, in its own
  coordinates.
- **Saved meshes:** a `GPolyMesh` in an element's record →
  `m_pFacetedTopology`, a `FacetedTopology` of `f32` points in model feet
  and `u16` triangles *(verified on the corpus's topography surfaces,
  their points within their elements' boxes)*. B-rep snapshots of the
  elements' geometry steps (`SnapshotData`, `GElement`, `Face`, `Edge`)
  are in the records too *(seen)*; the import uses the partitions' third
  chains instead ([The geometry](#the-geometry-mitcad333)).

Built-in categories are not named in the file; `categories` lists the
public enumeration's names of the common ones.

## The building elements (mitcad#275)

The `building` module reads what the import rebuilds walls, floors,
roofs, columns, beams and stair runs from, by the schema's field names,
in feet:

- **Curves:** `GLine` (`m_origin`, `m_dirVec`, `m_endParams`: the
  points `origin + t·dir`) and `GArc` (`m_center`, `m_radius`, `m_xVec`,
  `m_yVec`, `m_endParams`: the angles from the x vector towards the y
  vector) *(verified: the walls' and floors' curves of the MIT project
  land on the IFC export's solids)*; `GEllipse` (`m_center`, `m_xVec`,
  `m_yVec`, the radii `m_xLen` and `m_yLen`, `m_endParams` likewise:
  mitcad#340) *(verified: the elliptical handrails' profiles, two halves
  of 1.5 by 1.125 in)*; `GCylindricalHelix` (`m_basePnt`, `m_xVec`,
  `m_yVec`, `m_zVec`, each an `XYZ` object's `m_xyz`, `m_radius`,
  `m_pitchOver2PI` the rise per radian, `m_endParams` the angles from the
  x vector towards the y vector: a spiral rail's path, mitcad#385)
  *(verified: the 4 spiral rails of `33271010`, each two helices one
  after the other about one axis; the stored tubes, Hermite surfaces, lie
  within 1.4 mm of the exact ones)*.
- **Walls** (`VWall`): the location curve (`m_pCurveDriver` → `m_pCrv`);
  the base and the height from the location reference face
  (`m_pRefFaces[0]`, a `Plane` whose envelope spans the curve's length
  and the height); the faces' offsets from the other vertical reference
  faces; the type's layers (`m_WallAttributesId` → `m_pCompoundStructure`
  → `m_layers`: `m_layerWidth`, `m_materialId`, `m_layerFunction`); per
  end the walls joined there in order (`m_controlJoinsSet`:
  `CDJoinInfo.m_info`, each `EndInfo` an element and its end, 0 and 1
  its ends, 2 and 3 its sides; `m_wallJoinType`); the elements cutting it
  (`CutPairGStep.m_otherId` of its geometry steps) and the inserts whose
  cut it takes (`WallCutoutGStep.m_instId`); an edited profile's loops
  (`VWallExtrusionHelper.m_pCurveLoops`, in the wall's vertical plane);
  its top and base attachments (`JoinToRoofGStep`, see
  [Wall attachments](#wall-attachments-mitcad348)).
  **Joins** *(verified on the MIT project's 360 walls)*: where walls meet
  end to end, the first listed runs on to the far face of the second
  (and stays at the joint when the second runs on in line), the others
  stop at its near face (one in line with it where it ends); an end on
  another wall's side stops at that side's near face; joins with what is
  no layered wall (room separation lines, curtain walls) shape nothing,
  nor do joins with a wall wholly above or below this one *(seen: a
  corner's join lists the foundation walls and the storey's walls on
  them, a storey's wall first; the foundation walls' ends in the geometry
  the file keeps are joined to each other)*.
- **Floors, pads and roofs:** the loops of their extrusion helper cell
  (`FloorExtrusionHelper`, `BuildingPadExtrusionHelper`,
  `RoofExtrusionHelper`: `m_pCurveLoops`) or a footprint roof's own
  (`ProfileRoof.m_curveLoops`); a floor's `m_top` and `m_bottom`, a pad's
  horizontal reference faces; a roof's layer reference faces
  (`m_pRefLayerFaces`: the boundaries of its type's layers, per slope)
  and an extrusion roof's extents (`m_extrusionDepth`, along its open
  profile's plane's normal: the plane is not read, the side the extents
  run to is the side its stored box lies on *(seen: the two halves of a
  gable drawn from their eaves to the ridge, their profiles running
  opposite ways with the same extents)*); a ceiling's horizontal
  reference faces, its top and its core's top: its type's layers run
  down from its top *(seen: its body in the geometry the file keeps)*; a shape edit in use
  (`SlabShapeEditGStep.m_dormant` false).
- **Family instances and types:** the level references (`m_refs`'
  `FamilyLevelRef`: `m_elemId`, `m_offset`: a column's base and top), a
  beam's location curve and setbacks (`FamInstCurveDriver`: `m_pCrv`,
  `m_setbacks`); a type's parameters (`m_pParams` → `NamedParam`:
  `m_paramId`, `m_value`, `m_str`, `m_elemId`, `m_int`; a family
  parameter's name and kind are its element's `m_pParamDef`'s, see
  [Parameters' kinds](#parameters-kinds-mitcad501); the structural sections' built-in ids
  -1005502 width, -1005503 height, -1005504 diameter, -1005505/-1005506
  wall thickness, -1005524 flange and -1005525 web thickness, -1005529
  and -1005530 the hollow sections' corner radii *(verified: the steel
  sections' published dimensions)*), an insert type's cut loops and
  direction (`m_pCutLoops`, `m_cutDirs`: a door's opening, in the type's
  coordinates) *(verified: the MIT project's door openings are the IFC
  export's)*, else its void geometry (`m_complexCutGeoms`, see
  [Wall openings](#wall-openings-mitcad345)).
- **Stair runs** (`StairsRun`): `m_bottomElevation`, `m_topElevation`,
  the risers (`m_topRiserIndex - m_baseRiserIndex`), the treads' depths
  (`m_stairsTreadInfoArr` → `m_actDepth`), the start and end riser lines,
  the line along the nosings (`m_oSupportPathCurveLoops`); the run type's
  `m_hasMonolithicSupport` and `m_structuralDepth` (the support's depth
  under that line).
- **Grids** (`Grid`): `m_text` and the line of `m_pSurface` (a `Plane`
  whose envelope's u range is the line).
- **Links:** `RvtLinkInstance` (`m_name`, `m_pInstanceInfo` → the
  transform and the type) and `RvtLinkSymbol` (the file's name,
  `m_symbolInfo`; the stored path,
  `m_otherDocModelLocation.m_lastKnownModelPath.m_path`).
- **GlobalIds:** the element table's history (`ElemRec.m_history`:
  `m_originalElementId`, `m_creationDate`, an `EpisodeId`) names the
  episode that created the element in `Global/History` (a count, then
  per episode its GUID and one byte; the list newest first, so an
  `EpisodeId` counts from its end); the GlobalId is that GUID with its
  last four bytes xor-ed with the element id, in IFC's base-64 form
  *(verified: every one of the 854 elements of the MIT project's IFC
  export)*.

### Wall attachments (mitcad#348)

A wall whose top or base is attached to other elements keeps a
`JoinToRoofGStep` among its geometry steps (in `m_bRepAdjustGList`)
*(seen: 45 walls of `c805df44`, 20 of `33271010`, 16 of `bb62c161`, 125 of
`ff791052`)*:

- `m_ids`: the elements it is attached to: footprint roofs (`Roof`,
  `ProfileRoof`), face roofs (`FaceRoof`), floors and reference planes
  (`RefPlane`) *(seen)*; several when the end runs under several
  elements (two floors of one height in `c805df44`);
- `m_baseOrTop`: 1 the top, 0 the base *(seen: every top-attached wall's
  stored box reaches the attached element's lower surface, every
  base-attached one's the upper surface; the base-attached walls of
  `33271010` stand on a sloped reference plane and their boxes' bottoms
  are the plane's lowest heights under them)*;
- `m_leadingIdx`: 0 or all ones *(seen; not read)*.

The wall's own base and height (its location face's envelope, the height
parameter -1001101) stay those of its unattached top *(seen: the gable
walls of `bb62c161` keep 9.84 ft while their stored boxes reach the ridge
19.4 ft above their base)*. The attached end follows the element's
surface wherever it lies, below or above the wall's own end: a gable wall
reaches up into its gable *(seen in `bb62c161`)*, a wall under a lower
roof stands lower. Where the surface is level at the wall's own top the
two agree *(verified: the 45 walls of `c805df44` attached to floors end at
the floors' `m_bottom`, and the project's IFC export clips none of its
walls)*.

A reference plane (`RefPlane`) is its `m_pSurface`, a `Plane` (`m_origin`,
`m_xVec`, `m_yVec`), and its name `m_text` *(seen)*.

The import (`mitcad_import_rvt::plan`) builds an attached wall from its
base to above the surface and cuts away what lies beyond the surface's
planes: a floor's or ceiling's bottom (top) or top (base), a footprint
roof's slopes' lower (top) or upper (base) planes (its layer faces), a
reference plane. A face roof's surface is not read.

A top under elements that each cover part of the wall (a ridge wall
under the two halves of a gable whose edges end within its thickness, a
wall reaching out from under its roof) follows each element only within
its outline and keeps its own top elsewhere: half spaces bounded to the
outline cut it where the surface lies below its own top, and its
footprint runs on up within the outline to under the surface where that
lies above *(seen in a 2026 project: the ridge walls' bodies in the
geometry the file keeps rise to their own tops between the halves'
edges)*.

### Wall openings (mitcad#345)

What opens a wall, by its records:

- **Its cut steps.** A wall takes an insert's cut by a step of its own
  (`WallCutoutGStepBase.m_instId`): `WallCutoutGStep` *(seen: 192 in
  `c805df44`, 291 in `33271010`, 30 in `bb62c161`)*, and in the schema
  `WallComplexCutoutGStep` and `FamilyHostCutoutGStep` *(none in the
  corpus)*.
- **The insert's type's cut loops** (`InsertableObj.m_pCutLoops`,
  `m_cutDirs`, in the type's coordinates) *(seen: every door and window
  of `c805df44`, 185 and 7; 227 of the 280 inserts with a cut step in
  `33271010`)*.
- **Else its type's void geometry**: the other 53 inserts of `33271010`
  with a cut step (the windows of an opening family, specialty equipment)
  have no cut loops, their own symbol and their type each a
  `FamilySymbol.m_complexCutGeoms`: pointers to `Geometry` B-reps (the
  family's void forms, in the type's coordinates; read as the
  [geometry](#the-geometry-mitcad333)'s bodies), with `m_complexCutRefIds`
  (0 or all ones; not read) *(seen)*.
- **Elements it hosts that cut nothing**: trims and muntins (generic
  models) and the doors of curtain walls have no cut step, no cut loops
  and no void geometry *(seen: 160 and 7 in `33271010`)*.
- **Rectangular wall openings** (`SWallRectOpening`): `m_hostId`, and
  `m_oRectOpeningData` → `RectOpeningData`: `m_aPoint`, two opposite
  corners in model coordinates (`m_lowerLeftCorner`, `m_upperRightCorner`
  the same in the wall's face's (u, v)); the wall keeps a
  `WallRectOpeningGStep` *(seen: 11 in `33271010`, 2 in `bb62c161`)*.
- **Walls in walls**: `WallEmbedWallGStep` (a `WallCutoutGStepBase`
  whose `m_instId` is a wall embedded in this one, a curtain wall in a
  basic wall) *(seen in a 2026 project: the host's body in the geometry
  the file keeps has the embedded walls' location lines by their base and
  height as its openings)*: the import cuts that rectangle through the
  host; `WallShadowCutoutGStep.m_otherWallId` *(seen: 117 in `33271010`;
  not read)*.
- **The geometry the file keeps** of a wall holds its openings *(seen: the
  faces of a wall's cut-out snapshot, `m_bRepCutOutSnapshot`, run round
  its insert's opening)*.

The IFC export voids the walls with `IfcOpeningElement`s
(`IfcRelVoidsElement`) that the doors and windows fill
(`IfcRelFillsElement`) *(seen: 201 openings and 138 fillings in
`bfdf36ff`)*. The import cuts each insert by its type's cut loops, else
its void geometry, else (when the export is the import's reference) the
IFC opening its GlobalId fills; a rectangular opening by its
rectangle through the wall; the report says why an opening is missing.

On the MIT project with its IFC export (`c805df44`, `bfdf36ff`) every
wall (360), slab (80, and 20 floors the export names shading devices) and
column (256) built as features agrees with the export's solid (its volume
within 0.1 %, its box within 0.001 mm). Elsewhere the features are
checked against the geometry the file keeps of the element where that
meshes closed (the volume within 2 %, the box within 5 mm: a wall's
stored box also holds the sheets of its layers' boundaries, which reach
its unconnected height; a rail's, mitcad#391, against that geometry's box
both ways, its thin profile meshing within a chord that takes up to a
tenth of its volume), else against its stored box (`rvt.corpus`
imports a structural sample with the architectural sample it links).

## The geometry (mitcad#333)

The `brep` module reads the `GElement` of an element's third-chain record
in the highest-numbered partition that has one (a partition need not
repeat an unchanged element's) into its bodies and the types it places
*(verified: every such record of the 2023 to 2027 projects walks to its
end and reads, 13,979 elements of the MIT project, 11,644 of `33271010`)*:

- **The tree.** `GElement`, `GGroup` and `GFilter` hold nodes
  (`m_subNodes`); a `GFilter`'s `m_oConditions` say in which views its
  nodes show, all of them to hold: the import takes what a 3D view shows
  at the fine detail level (`brep::condition_in_3d`): a view direction
  (`GConditionDir`, `m_dir`) or a cut (`GConditionCut`) never holds
  there, its negation (`m_negateDirCondition`) always; `GConditionInt`
  with `m_param` 3 is the detail level (`m_value` 1 coarse, 2 medium, 3
  fine) compared by `m_comp` (0 equal, 3 not equal) *(seen in a 2026
  project: under direction and cut conditions lie the plans', the
  elevations' and the cut's symbolic faces, single planes, and the 3D
  bodies under their negations; the detail conditions come in
  complementary pairs, a representation and its alternative)*. A type
  that shows nothing at the fine level is read at the medium, else the
  coarse one *(seen: a column family whose body shows at the coarse level
  only)*. `Geometry` is a body (`m_pFaces`, `m_pEdges`); `GInstance`
  places a type's geometry (`m_instanceInfo` → `InstanceInfo`: `m_symbolId`,
  `m_Trf`; or `m_oEmbeddedSymbolGRep`). Curves (`GLine`, `GArc`) are
  symbolic lines. A family instance's own `GElement` is one `GInstance` of
  its placed type (which may be the instance's own symbol, a mirrored copy
  of its type); doors, windows, furniture, curtain panels and mullions are
  such instances *(verified on the MIT project: 185 doors, 7 windows, 16
  furniture, 3,359 panels, 3,786 mullions)*.
- **The bodies** are winged-edge B-reps. A `Face` has a surface
  (`m_pSurf`), loops (`m_pFirstLoop` → `EdgeLoop`, its `m_nextLoop`) and a
  material (`m_renderStyleId`); a loop names its first edge (`m_next`) and
  its last edge names the loop as its next. An `Edge` has two sides: its
  faces (`m_pFace`, a null reference for the open side of a sheet's edge:
  walls keep one face per body), its next and previous edges in each
  face's loop (`m_next`, `m_prev`), and no curve of its own but its points
  in each face's (u, v): the first and last (`m_firstAndLastEdgePnts`)
  and, along a curved edge, those between (`m_interiorEdgePnts`). Faces
  and edges name each other by reference pointers, numbers of the
  record's objects (`Record::numbered`). A face can have no loops *(seen:
  1,916 faces of the MIT project; left out)*; a body of one plane is a
  sheet, not a solid (a wall's layer boundaries, a symbol's face; left
  out of the meshes); a plane can have several outer loops besides its
  holes *(seen on an in-place mass)*: a loop is a hole of the loop that
  holds it.
- **The surfaces**, (u, v) to a point *(verified: the model points of an
  edge's two sides lie within 0.0005 ft (0.15 mm) of each other over every
  corpus project, `mitcad-rvt-inspect geometry`)*: `Plane` `m_origin + u
  m_xVec + v m_yVec`; `CylSurf` `m_center + m_radius (cos u m_xVec + sin
  u m_yVec) + v m_zVec`; `ConeSurf` (the apex `m_center`) `m_center + v
  (sin a (cos u m_xVec + sin u m_yVec) + cos a m_zVec)`, `a` its
  `m_halfAngle`; `SurfRev` its `m_pProfileCurve` (in the frame `m_xVec`,
  `m_yVec`, `m_zVec` about `m_center`) at v, turned by u about the frame's
  z; `RuledSurf` the points at the fraction u of `m_pProfileCurve1` and
  `m_pProfileCurve2` (`m_Point1`, `m_Point2` for a point) joined along v.
  `HermiteSurf` bicubic Hermite patches between its nodes (`m_NodeArray`:
  `m_iPoint`, `m_iTangent` in u and v, `m_iMixedDer`; u running fastest) at
  the parameters `m_uParams` by `m_vParams` *(seen in 3 projects, 76 faces at
  most)*. A surface of another class would take its faces' points from the
  neighbouring faces.

### Curtain walls, railings, stairs, sweeps (mitcad#333, mitcad#340)

What the import rebuilds these from, read by the `building` module. Each
built element is checked against the IFC export by GlobalId or its stored
box, so a misread shows as a fallback with its reason; *(verified)* below
means the built solids lie within 5 mm of the elements' stored boxes on
the 2024 projects `c805df44` and `33271010` for the share given, and,
where so said, that the geometry the file keeps of the element has the
shape described (`mitcad-rvt-inspect geometry <id> --points`).

- **Curtain panels and mullions** are family instances whose own symbol
  (`m_pInstanceInfo` → `m_symbolId`, a copy of the type per instance)
  carries their shape in the symbol's coordinates, placed by the
  instance's transform:
  - a mullion's `SysMullionFamSym` holds a `MullionSymFamRef`: its path
    (`m_pCurve`, a `GLine`), the normal its thickness runs along
    (`m_pathNorm`) and `m_flags`; its profile is `m_sysProfile`'s class
    with the type's built-in parameters: `SysRectMullionProfile` width
    -1007300 towards `m_pathNorm × path` and -1007301 the other way,
    thickness -1007304 along the normal about the offset -1007351; with
    flag 4 (a border mullion) both widths lie on one side, from the path
    towards `m_pathNorm × path` *(verified with the stored geometry of
    every rectangular mullion symbol of `33271010`, 311, the off-centre
    ones among them)*;
    `SysQuadMullionProfile` (a corner
    mullion) a rectangle of its depths -1007357 and -1007358 about the
    path *(verified)*; `SysCustMullionProfile` the profile family type
    `m_profileId`, its profile's loops square to the path *(verified: every
    mullion of `c805df44`, 6,160; its orientation and side are taken from
    the stored box)*; `SysVMullionProfile` (a V corner mullion) two legs
    of the thickness -1007304 at right angles, along the directions of
    its corner angles `m_cornerAngles` (from `m_pathNorm × path` towards
    `m_pathNorm`), each leg's middle line the thickness away from the line
    through the path along it (away from the other leg), reaching its
    length -1007354 or -1007355 from where the middle lines meet and half
    the thickness back past it: an L *(verified: the 14 V mullions of
    `33271010`, all of right angles, legs of 5 in, 2.5 in thick, offset
    0; which length goes with which leg is not known, both being equal)*;
  - a mullion's ends are trimmed to its neighbours (`m_trimEnd0`,
    `m_trimEnd1`: `MullionTrimNeighbor`, its `m_trimPath` the neighbour's
    path through the end, `m_nbrSymbolId` the neighbour's type, `m_flags`,
    `m_trimPathNorm`, `m_direction`, `m_flip`, `m_cornerAngles`; a
    mullion type's sizes are read from its own record as an instance
    symbol's are). In the curtain's plane the mullion and its neighbour
    are strips across their paths, the trim's path having one end at the
    mullion's end (mitcad#387). The trim's flags: 2 a mitre, else a butt
    (the end stops along the neighbour's side facing it); 1 the neighbour
    is a border mullion, its full width (-1007300 plus -1007301) on the
    corner's inside (for a border mullion trimmed, the side its own width
    lies on turned towards the neighbour; else the shorter way round; at a
    neighbour running on past the joint, the side facing the mullion); 4
    *(seen; not read)*. A border mullion's strip runs from its path to its
    full width towards `m_pathNorm × path`; a corner neighbour's (quad) is
    half its depth -1007357, an L's its leg -1007354, either side; any
    other rectangular mullion's is its width -1007301 either side, whatever
    its -1007300 (the off-centre mullions of `33271010` mitre so). A mitre
    runs between the strips' overlap's corners farthest out and in along
    the two members' directions from the joint; a neighbour in line leaves
    the end square, and a cut reaching past the mullion's other end leaves
    both its ends square. A neighbour of two pieces meeting at the end
    (the trim's path of two curves) that bend there (mitcad#390): the end
    stops at each piece's near side, on that piece's side of where the two
    near sides cross, an end of two planes *(verified: 127 of the 130
    trimmed ends of `33271010`, `ff791052` and `bb62c161` where their
    symbols' stored bodies end, within 1e-9 ft along their sides, the end
    of two faces of `ff791052` among them; not so: two butts of `33271010`
    nearly square (88°) to a sloped border running on past them, whose
    stored ends are not cut, and the end of `1448325` at a corner mullion,
    not cut, the mirror image of `1448308`'s end 0, which is)*. The import cuts the
    mullion's prism along the rule's planes and checks them against the
    symbol's stored end faces (a planar face per end, faces of one plane
    one end; two planes split where their faces meet across the path,
    mitcad#390); where they differ the stored face is taken and the report
    says by how much (mitcad#385, mitcad#387) *(verified: the 14 trimmed
    mullions of `33271010` within 1e-11 mm of their stored boxes)*. For
    the three ends the rule does not give, nothing else the file keeps of
    those joints decides (mitcad#391): the curtain wall's join overrides
    (`WallCGDriver.m_mullionJoins`, `GridJoinInfo`: the grid segments'
    ends meeting at a node) name none of their nodes, the border
    segments there are trimmed against each other (border to border, in
    line) as at the cut ends of `ff791052` and `bb62c161`, their
    `MullionFamRef`s are alike, and neither the grid's unlocked
    mullions (`CurtainGrid.m_unlockedMullions`: only `1448325`'s) nor
    their geometry's regeneration episodes set them apart *(seen)*: their
    stored end faces stay;
  - a panel's `SysPanelFamSym` holds a `CurtainCellSymFamRef`: its cell's
    loops (`m_pCurveLoops`) on its plane (`m_pSurf`), `m_flipped`, and the
    symbol's `m_thickness` and `m_offset`: the panel is the cell's loops
    extruded by the thickness, its middle `m_offset` along `y × x` of the
    plane (along `x × y` when flipped) *(verified: every system panel of
    `c805df44`, 348 of `33271010`; the others are panel families, whose
    shape is their family's, and empty panels)*;
  - the curtain wall itself (its type `NewCurtainWallType`, no layers)
    keeps only sheets of its faces: its panels and mullions are its shape.
- **Railings** (`BaseRailing`): the railing's geometry places its
  balusters and posts (family instances of category -2000948, elements of
  their own) and its continuous rails' symbols; its type
  (`StairsRailingAttr`) holds the baluster pattern
  (`BalusterPlacement`, `BalusterPattern`, `BalusterInfo`: the family
  type, spaces and offsets of each baluster and post) and the
  non-continuous rails (`RailStructure.m_nonContinuousRails`: none in the
  corpus). The balusters and posts are placed family instances already
  (they come in with their family's geometry); a railing whose geometry
  holds nothing else has no body of its own.
- **Rails** (`TopRail`, `HandRail`: `ContinuousRail`): the path is the
  `Path3d` element `m_oPathProvider.m_path3dId` (its `m_curveLoops`, model
  coordinates, the rail's extensions included), the profile the profile
  family type `m_profileData.m_profileFamSymId` (its type's `m_profileId`)
  swept along it as one sweep keeps it, square to the path and turning
  with it in the path's plane (a post return's rail lies on the inside of
  its folds) *(verified: 213 of 213 rails of `33271010`; the spiral
  stair's 4 rails run along helices, mitcad#385: the profile swept along
  the one helix their pieces make, its ends cut by vertical planes
  through the helix's ends, square to it in plan, as the stored rails
  end)*. The rail's type (`ContinuousRailType`: `TopRailType`,
  `HandRailType`) gives its joins, `m_defaultContinuousRailJoinType` (1 a
  fillet) and `m_filletRadius` (mitcad#387): where its path's lines meet at
  an angle the stored rail rounds the corner, the profile swept round an
  arc of the fillet radius tangent to both lines (its surfaces of
  revolution about the arc's axis, the profile's centre the radius from
  it) *(verified: the elliptical rail `1501328`, 6 corners of a 2.5 in
  radius, its arcs' centres the stored surfaces' and its box within 1e-12
  mm of the stored body's; `660284`, two 2 in fillets)*. The import
  fillets the corners of a chain where the fillets fit on their lines.
  Along a chain that is not planar (a rail up a stair, mitcad#390) the
  stored rail is the profile (its x square to up) along each line and
  turned about each fillet's axis, where the corner turns about up (in
  plan) or about the profile's x (in a vertical plane); the import builds
  it so, piece by piece *(verified: the 82 such rails of `33271010`, 450
  arcs, each about a stored surface's axis within 3e-14 ft and by its
  angle within 3e-13 rad; the 43 whose stored geometry is the rail alone
  within 0.0005 mm of its box; the others' stored geometry also holds
  their supports or a copy one storey up)*. A corner turning about
  neither (mitcad#391) is rounded in plan: an arc of the fillet radius
  tangent to the two lines' plan directions, lifted to a helix about the
  vertical through its centre, from the first line's point above the
  arc's start to the second line's point above its end (rising at one
  slope), the profile swept along it with its x level towards the axis;
  the helix leaves and joins the lines at kinks, mitred (the planes
  square to the sum of the two directions there) *(verified: the one such
  corner of the corpus, `1270819`'s: the 100 nodes of its stored
  `HermiteSurf`s, a quarter of the profile per face, lie on that sweep's
  sections within 1e-14 ft; its mitre faces' normals are the sums')*.
  A chain that is not planar and has elliptical pieces (sloped arcs: 3
  road stripings of `33271010`) is not filleted; its pieces are swept
  along it one by one (mitcad#395). Along an elliptical piece the stored
  rail keeps the profile square to the piece and its x level (mitcad#397);
  where it meets another piece at an angle (a level arc), the two meet in
  the first one's end section, the second cut in it *(verified: the
  Hermite edges the file keeps along `2150352`'s elliptical piece, their
  corners level across the profile at its start, middle and end and its
  thickness square to the piece there; at every such joint of the 3 rails
  each straight edge the file keeps lies along the first piece's end
  section within 1e-5 rad; the `Path3d`'s joint of `2150352` lies 0.3 mm
  on along the path from the stored one)*.
  Where a rail's type does not
  fillet its joins (a join type other than 1, or a radius of 0), the
  stored rail up a stair is the profile along each line, the lines mitred
  at their corners (mitcad#395: the planes square to the sum of the two
  directions) *(verified: the plane faces and edges the file keeps of the
  5 such rails of `33271010`, `832432` among them, lie in those planes
  within 1e-4 mm)*. A rail's free end on a sloped
  line is cut plumb (mitcad#391): by the vertical plane through the
  path's end, square to the line in plan *(verified: the 12 such ends of
  `33271010` and `bb62c161`, spiral rails and a chain of elliptical
  pieces apart; their stored end faces
  are vertical)*. A rail's per-join settings
  (`ContinuousRailJoinInfoCell.m_joinInfoItems`) are empty on every rail
  of the corpus *(seen: the 243 rails of `33271010`, `bb62c161`,
  `41f1893a` and `bc547fe9`; not read)*.
- **Rails' supports** (mitcad#391): `ContinuousRail.m_supportIds` lists
  a rail's supports (wall mounts, pickets: family instances of category
  -2000948 whose `m_ownerElemId` is the rail); the rail's geometry places
  them where they are (and on every storey a multistory stair repeats it
  on). A family instance naming the rail as its owner that the rail does
  not list is an earlier layout's: nothing places it *(verified: 2,293
  listed supports of `33271010`, each placed; 315 not listed, none
  placed, each beside a listed one)*: the import
  leaves it out.
- **Multistory stairs** (`MultistoryStairs`, mitcad#391): a stair of one
  (`StairsElement` with a `MultistoryStairsDataCell` among its cells)
  is repeated on the levels of `m_levelIdIndexPairSet` (`m_data`:
  `ElementIdPlusIndex.m_elementId`, the copy's `m_index`; up to
  `m_instancesUpToLevelId`), each copy moved straight up by the level's
  elevation less the stair's base level's (`m_baseLevelId`). What belongs
  to the stair: its runs and landings (`m_stairsId`), their supports
  (`StairsSupport.m_hostCompId`), its railings (`BaseRailing.m_stairsId`),
  their rails (`ContinuousRail.m_railSystemId`), the rails' supports and
  the railings' balusters (`FamilyInstance.m_ownerElemId`). The stair's
  geometry places its runs, landings and supports (elements of their own)
  on each storey; a railing's and a rail's own geometry place their
  symbols on each storey, the railing's balusters and the rail's supports
  likewise *(verified: 425 copied elements of `33271010`'s 3 repeated
  stairs and 64 of `ff791052`'s 5 at exactly those heights, and no other
  element placed straight up by another's geometry)*. The import builds
  a rail's copies with the rail (its stored geometry and box hold them),
  and every other element's as an element of its own on the copy's level
  (`… (on L4)`, the same features or mesh moved up); the stair itself has
  no body of its own.
- **Profile families** (category -2003000): each type keeps its profile
  at its own dimensions in its record, `FamilySymbol.m_arrProfileLoops`
  (`CurveLoop`s in the family's xy plane, its origin where the sweep's
  path runs) *(verified: the copings' and rails' types of `33271010`,
  whose family documents draw one type's dimensions)*; their types'
  geometry is a box of reference lines *(seen)*. The family's own
  document in its partition (the loaded families' documents after the
  chains) holds its curves at the family's dimensions: the family's
  record's `FamilyDocument.m_big2SmallMap2` maps project element ids to
  the document's (`ForeignElemRef.m_id64`), and the document whose
  elements hold all those ids is the family's; its `CurveElem`s' curves
  (`m_pCurveDriver` → `m_pCrv`) are read where a type has no loops of its
  own *(verified: the circular handrail's two arcs of radius 1/16 ft)*.
- **Wall sweeps** (`WallSweep`): per wall a `WallSweepCurveGStep` of its
  geometry steps (`m_hostId` the wall) gives a path, `m_offsetCurves`
  (`m_curves` moved by `m_wallSweepInfo.m_wallOffset` from the wall) where
  the profile's origin runs: at the wall's face, at the height
  `m_distance` from the end `m_distanceMeasuredFrom` names, with the
  pull-backs and extensions (`m_pullBackParam0`, `m_pullBackParam1`)
  applied *(seen)*; the step's `m_side` puts the profile's x to the left
  of the path (0: along `up × path`) or the right (1: mirrored), and
  `m_wallSweepInfo.m_flipped` hangs it below the path (its y mirrored)
  *(verified: 390 of the 390 sweeps and slab edges of `33271010` built
  this way)*. Its ends (mitcad#385): the step's `m_pullBackParam0` (the
  path's starts) and `m_pullBackParam1` (its ends) hold `PullBack`s
  (`m_end` the wall's end pulled back from, 0xffffffff where an opening in
  the wall breaks the path; `m_offsets`, already applied to the curves;
  `m_returns`: `ReturnInfo`, `m_angle` and `m_length`). An opening's
  return is at the ends of the path's pieces beside the gap (the path's
  curves keep a curve of no length on the opening's far side), else at
  the path's own end; the wall end's at the path's end. A return turns by
  its angle away from the profile's side (round the wall's end) and runs
  its length, mitred to the sweep; a length of 0 or -1 closes the corner
  as a return of no length would (the mitre's wedge and the return's
  triangle). `m_joinedSweeps` (`TagEnd.m_end`, `EndInfo.m_elemId`) names
  the sweep (or its own other path) an end meets: the two are mitred
  along the bisector of their directions there, before any return. The
  sweeps a sweep is joined to cut it (`CutPairGStep.m_otherId`, as for
  walls): a coping meeting another coping at a corner is cut by that
  coping's solid as it is after its own cuts. Paths of two walls in line
  are one line *(verified: the 66 of 290 sweeps with returns or joins
  whose stored meshes measure within 0.2 % of their stored volumes,
  `1966901` and `1968773` equal to theirs; a sample of 16 others against
  their stored faces within 0.15 %)*. Not read: the extents
  `m_oProfileExtents` (`ProfileExtents`: `m_in`, `m_out`, `m_left`,
  `m_right`, the profile's reach about its path) *(seen)*.
- **Slab edges, fascias, gutters** (`SweepHost`): the paths are the
  `m_pathLoop`s of its `SweepHostLoopData` with `m_driveVec` (the
  profile's up), the profile its type's `m_profileId`, the offsets the
  parameters -1012825 (across) and -1012827 (up); which of the profile's
  mirror images is taken from the box of the element's stored body: the
  element's own box (its header's) can reach far past it (a slab edge's
  three feet out and up), where a mirror image on the slab's side of its
  edge fitted as well as the right one and gave volumes about 2 % low at
  convex corners (mitcad#387) *(verified: the 75 slab edges of `33271010`;
  69 of the 72 whose stored meshes measure within 0.5 % of them, the other
  three 0.76 % below meshes whose concave arcs' chords add to them: their
  stored ends are square and their profile the type's, `2089027`; their
  boxes against their stored bodies' boxes: 57 within 0.01 mm, 9 within
  0.015 mm, 9 cut by other elements within 1.5 mm)*.
- **Stair runs of treads and risers** (`StairsRun` of a
  non-monolithic `StairsRunType`): the type's `m_hasTread`,
  `m_treadThickness`, `m_hasRiser`, `m_riserThickness`, `m_nosingLength`;
  the treads' depths run from nosing to nosing, a riser stands the nosing
  length behind its tread's front edge (mitcad#397): riser k's front from
  its base up to the bottom of the tread above, the last riser's too (to
  where the tread above it would be); with slanted risers
  (`m_riserIsSlanted`) it leans from its base up to tread k's front, its
  thickness square to its face. With `m_riserTreadConnection` 1 a riser
  stands on the tread below, which reaches under it to its back; with 0
  it reaches down behind that tread to its bottom, the tread reaching to
  its front, and the first riser reaches down behind the landing the run
  leaves (a landing that is the run's tread). Tread k (its top at its
  step's height, its thickness below) runs from its front (along riser k's
  front where they slant) to the next riser, or where there are none to
  where it would stand (the nosing length on past the next tread's
  front); a top tread (as many treads as risers) to its depth. The run
  type's nosing profile (`m_nosingProfileTypeId`) shapes each tread's
  front as a landing's (its x into the tread from the front, its y up from
  the top, the strip it spans cut to it) and, with `m_nosingPlacement` 3,
  runs out along each tread's two sides (the corners mitred, the ends
  square) *(verified: all 36 straight runs of `33271010` of the four types
  there (30 of slanted risers on their treads, 4 and 1 of risers behind
  their treads with a front nosing profile, 3 of no risers and side
  nosings), with their copies up multistory stairs, within 7e-11 of the
  volume and 1e-11 mm of the box, both ways, of the geometry the file
  keeps; the stored treads and risers meet edge to edge, a mesh that is
  balanced but not a manifold. Not read: a nosing profile with slanted
  risers, `m_nosingPlacement` 1 and 2)*.
- **Monolithic runs**: the steps of the same depths, each riser leaning
  from its base the nosing length behind its tread's front up to that
  front, each tread reaching back to the next riser's base; the support
  line (`m_oSupportPathCurveLoops`) runs along the nosings, the underside
  the type's `m_structuralDepth` (square to it) below the line through the
  risers' bases, a riser and the nosing's rise below the support line
  *(verified: the geometry of two runs of `33271010`, 17 and 12 risers;
  the stored box of the third reaches the support line's end, above the
  run's top)*.
- **Spiral runs** (a `StairsSpiralRunCell` among the run's objects;
  `StairsRunInfo`: the centre `m_origin`, the radii `m_bndBox.m_corners`
  0 and 2, per piece of the run; the riser lines in plan
  `m_compatibleCrvInfo.m_orderedRisers` → `m_pRiserCurve`, the first at
  the start) turn about their centre (mitcad#385): tread k's front on
  riser line k, its back on riser line k + 1 moved on by the nosing
  length, between the circles of the two radii, its top at its step's
  height and its type's tread thickness below; the run type's nosing
  profile (`m_nosingProfileTypeId`, a profile family type) runs under its
  front from circle to circle *(verified: the 2 spiral runs of
  `33271010`, open risers, within 1e-8 mm of their stored boxes and
  0.17 % above their stored meshes' volumes, whose arcs are chords)*.
- **Landings** (`StairsLanding`): the boundary `m_oBoundary` at the top
  `m_baseElevation`, down by its type's (`StairsLandingType`) `m_thickness`
  when monolithic, else its `m_treadThickness`. A non-monolithic landing
  type with `m_useSameTriserAsRun` (mitcad#395) takes its treads and
  risers from its stair's run type: the landing is that run type's tread,
  as thick as its `m_treadThickness`. Its runs are its
  `m_logicJoinInfo.m_logiciJoins` (`m_peerElementId`, `m_peerJoinEndType`:
  1 the run's start meets it, 2 its end). Where a run leaves it, it
  reaches on past the run's start riser line by the nosing length and,
  where a riser stands on the tread below (`m_riserTreadConnection` 1;
  0: the riser reaches down behind it), under the first riser to its
  back. With slanted risers (`m_riserIsSlanted`: each leans from its base
  the nosing length behind its tread's front up to that front, the
  riser's height the run's height over its risers) the edge there runs
  along the riser's back face, and where a run arrives along its last
  riser's front face, which meets the landing's top at the end riser
  line. The run type's nosing profile (`m_nosingProfileTypeId`) lies at
  the front where a run arrives (the profile's x into the landing from
  the front, its y up from the top), the strip it spans cut to it, and
  with `m_nosingPlacement` 3 (front and sides) along every other edge
  too, outwards (its x's far end at the edge), the corners mitred and the
  ends square, its strip at the front cut across those ends *(verified:
  all 17 landings of `33271010` (3 with slanted risers on the treads, 1
  with no risers and side nosings) and 5 of `ff791052` (risers behind the
  treads, a front nosing), with their copies up multistory stairs, within
  1e-11 of the volume and of the box of the geometry the file keeps;
  `m_nosingPlacement` 1 and 2 (one side) are not read)*.
- **Groups**: a placed group (`ElementGroup`) places its members by
  their element ids (a `GInstance` each, at its transform); the members of
  every group type, placed or not, carry `m_unplacedOwnerId` *(verified on
  the MIT project's 11 groups)*.

The import (`mitcad_import_rvt::plan`) sweeps a profile along each chain
of its paths as one solid: a straight chain an extrusion of its length, a
planar chain of lines and arcs one sweep (the profile square to the
chain's start, its x across the path, `Item::Sweep` with the profile's
frame as the sweep starts: no other placement, so the solid lies on the
path; a closed chain starts in the middle of a line), a chain that is not
planar (a rail up a stair) one solid of its pieces, each line's piece run
on past a corner and cut at the mitre there (the plane square to the sum
of the two directions: the stored rails' faces, mitcad#395) and an
elliptical piece (a sloped arc) swept with the binormal up (the sweep's
fixed binormal, mitcad#397: the profile level and square to the piece all
along, as the file keeps it, where keeping its angle to the arc's plane
left it 0.3° apart at the end, 0.27 mm at the profile's edge), the piece
after it at an angle cut in its end section *(verified: the plane faces
and edges the file keeps of the 5 such rails of `33271010` lie in those
planes within 1e-4 mm)*, a chain of helices one
sweep along a fixed helix path (mitcad#385); a rail whose type fillets its
joins with arcs at its chains' corners (mitcad#387), along a chain that
is not planar the profile extruded along each line and turned about each
arc's axis, the pieces joined (mitcad#390); a corner rounded in plan along
a helix swept, mitred to its lines, and a free end on a sloped line cut
plumb (mitcad#391); a rail's copies up a multistory stair moved up with
it (mitcad#391). The design the import makes has the default geometric
tolerance (0.01 mm) as its join tolerance (mitcad#397, [commands.md](../model-api/src/commands.md#combines-joins-within-the-join-tolerance-the-rvt-imports-tolerance)):
its joins, cuts and unions fuse within it where they fail exactly (a
rail's sloped elliptical piece and the level arc after it meet in faces
1.2 µm apart).

`mitcad-rvt-inspect geometry` counts the faces by surface and the seam
gaps; with an element id it lists that element's bodies and placed types,
and with `--points` each face's loops as points (feet).

## Limits on crafted files (mitcad#924)

A file is read on opening (the link listing) and by the import, so no
count stored in it may make the reading run long or allocate without
bound. Each limit is far above what real files hold:

- **Streams** (`gzip::stream_limit`): a stream's members together inflate
  to 64 times its stored size and 16 MiB more at most, besides each
  member's 1 GiB; past it the stream is an error ("inflates to more than
  … bytes"). Element data inflates to a few times its stored size.
- **Records** (`walk`): every value that reads no bytes of its own (an
  object other than an ElementId, a fixed array) counts against 1,024
  and 16 per byte of the record, every other value reads at least a
  byte, so a walk takes at most 17 steps per byte and 1,024 more. An
  array or container whose elements cannot fit the bytes left (each
  element's fewest bytes by the schema) or the allowance left (elements
  that never read a byte, as objects of a class without stored fields)
  is refused before its elements are read.
- **History** (`info::episodes`): one pass; only the counts within the
  run of entries ending like the newest are candidates.
- **Node trees** (`brep`): a node whose sub-nodes name itself or an
  ancestor is left out there; one walk of a record reads a million nodes
  and faces at most (a real tree reads each of its objects once); each
  with a note once.
- **Bodies' loops** (`brep`, mitcad#942): a face's loop that names a loop
  already read on the face ends the face's loops there, with a note once
  (a real face's loops end with a null). A body takes at most two edge
  steps (an edge's side in a face's loop) per object of the record and
  does not read past them ("its loops take more than … edge steps"): in
  a real body each side of an edge lies in one loop, and every edge is an
  object of the record. One walk takes ten million edge steps at most
  over all its bodies (bodies shared by several nodes multiply as the
  nodes do); the bodies after it are left out with a note once.
- **Placed types** (the import's meshes): 100,000 placements per element,
  nested ones counted, and no more placements once the element has ten
  million triangles, with a note.
- **Links**: a link's file name is looked for only as a bare name, and
  the path the project stores only below the project's folder (see
  `core/ffi/src/rvt_import.rs`, `find_link`).

## The test corpus

37 files under `MITCAD_RVT_CORPUS` (never in the repository): a sample
family saved by every release 2016–2026, a structural sample project by
every release 2016–2027, further 2024–2026 projects, an MIT-licensed 2024
project (`c805df44`), a 2023 project and four 2020 families. Every file
opens with its release, kind, preview (the reference reader's image byte
for byte), schema (whole, no unknown encoding, every class the reference
reader finds) and element table; every element header record walks to its
end, and every element object record but those that stop at extensible
storage data *(verified)*.

On the 2024 and 2025 projects every element the reference reader lists
agrees on its category, class, box (where the reference keeps the stored
box: it trims walls at joins, cuts columns by walls and draws some by
their type's profile), transform, type, type name, family name and level,
and every material on its name, colour and transparency, with these
explained differences (`tests/corpus.rs`):

- **Levels the reference finds by its own rules** (the nearest level below
  the box, a host of the same class, a constraint read by position) where
  this reader reads a level from the element's records: 132 elements of
  `6dc833f9` (beams on their work planes' levels; a sloped beam on the
  first of its two levels), 66 of `33271010` (slab edges on their slabs'
  levels, light fixtures on their hosts'), 1 of `037165e5` (model text
  whose record names no level).
- **Materials the reference misreads** (a black colour and a denormal
  transparency): 4 to 24 per file, not compared.
- `33271010`: a material whose own name differs from the one the
  reference reads, and 7 stair supports whose own `m_typeId` names
  another type than the reference reads.

| Content id | Release | Declared | Compared elements | Disagreements explained |
|---|---|---|---|---|
| `c805df44` | 2024 | 26,425 | 970 | none |
| `33271010` | 2024 | 47,233 | 6,075 | 66 levels, 7 types, 1 material |
| `6dc833f9` | 2024 | 16,571 | 1,282 | 132 levels |
| `ff791052` | 2024 | 17,120 | 5,657 | none |
| `bb62c161` | 2024 | 8,288 | 464 | none |
| `0eeb6814` | 2024 | 13,738 | 905 | none |
| `aab838f8` | 2024 | 13,819 | 454 | none |
| `41f1893a` | 2025 | 8,327 | 464 | none |
| `037165e5` | 2025 | 28,070 | 6,729 | 1 level |
| `5089b2c6` | 2025 | 13,861 | 454 | none |

The other releases' element records walk with the same schema-driven
reader; their views are not compared (the reference reader reads only
metadata there).

## Parameters' kinds (mitcad#501)

A parameter's numbers are in the file's internal units (feet, square and
cubic feet, radians, kilograms); its definition tells what they are
(`params`):

- A project, shared or family parameter (a positive id) is an element of a
  `ParamElem` class (`ParamElemFamily`, `ParamElemExternal`,
  `ParamElemProject`, `ParamElemGlobal`, ...) whose `m_pParamDef` names it
  (`m_caption`). The definition's class gives the kind: `ParamDefValue`
  and its classes by their spec (`m_specTypeId` → `m_typeId`, an
  identifier `<namespace>.spec.<name>-<version>`: `aec:length`,
  `aec:area`, `aec:volume`, `aec:angle`, `aec:number`,
  `aec.piping:pipeSize`, `aec.hvac:ductSize`,
  `aec.electrical:conduitSize`, `aec.electrical:current`, ...; versions 1
  and 2 of a spec alike), `ParamDefYesNo` a yes or no, `ParamDefInt` (and
  `ParamDefNoOfPoles`) an integer, `ParamDefString` and its browsers
  (`ParamDefURL`) text, `ParamDefMaterialBrowse`, `ParamDefFamType` an
  element *(seen: the parameter definitions of the 2024 and 2025 projects
  `c805df44`, `bb62c161`, `33271010` and `037165e5`; verified: lengths,
  areas and angles against the types' names and the sections' published
  areas, in `core/import-rvt/tests/corpus/rvt_corpus.rs`)*. Before 2021 the
  definition has a `m_unitType` number instead of the spec *(seen)*; it
  is not read.
- The values: an element's own in its parameter sets
  (`m_pParamValueSetDouble`, `…Int`, `…AString`, `…ElementId`); a type's
  in its `NamedParam`s, a number in `m_value`, a text in `m_str`, an
  element in `m_elemId`, a yes or no and an integer in `m_int` *(seen:
  `m_value` is 0 for these)*.
- A built-in parameter (a negative id) is not defined in the file. The
  reader's table names the common ones as the corpus's schedules head
  their columns (`ScheduleField`: `m_paramElemId`, `m_columnHeading`) and
  gives their kinds: -1001300 height, -1001301 width and -1001302
  thickness of door and window types *(verified: types named `3 x 8`,
  `800 x 2100`)*, -1001101 a wall's height *(verified against its faces)*
  and -1001105 its unconnected height *(equal to it on the walls whose
  top is not attached to a level)*, the offsets, marks and comments
  *(seen)*. A wall type keeps no width: it is its layers' total.

## Tests

`cargo test -p mitcad-rvt` (`core.rvt` in ctest): the written project and
family (container, pages, members, metadata, schema, walks, chains,
element views, saved meshes) and `tests/corpus.rs` on the corpus (skipped
without `MITCAD_RVT_CORPUS`; its files several at once, each in a thread
of its own, the largest first: `MITCAD_CORPUS_JOBS`, else a quarter of
the cores, at most 4; mitcad#875). The import: `core/import-rvt/src` (mock
kernel), `core/import-rvt/tests/corpus/rvt_mullion_trims.rs` (the trimmed mullions' rule
against their stored ends on three projects),
`core/import-rvt/tests/corpus/rvt_corpus.rs` (mitcad#501: parameters in their
units against the types' names on `c805df44` and `bb62c161`),
`core/import-rvt/tests/corpus/rvt_rails.rs` (mitcad#391: the multistory stairs' copies
where the file places them, the helix corner on its stored Hermite
faces; mitcad#395: the landings with their run's treads against their
stored treads, the mitred rails against their stored faces; mitcad#397:
the runs of treads and risers against the volume and box of their stored
runs, the sloped elliptical rail pieces' joints against their stored
edges),
`core.exchange`,
`cli.import_rvt`, `cli.import_rfa`,
`rvt.corpus` (its project and its link check in child processes at
once, mitcad#875) and `tools/ui-rvt-import-test.sh`.
