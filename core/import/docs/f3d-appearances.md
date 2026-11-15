# The .f3d import: appearances, materials and light bulbs

The file's appearances and materials on bodies and faces, opening designs
with many face appearances, and the light bulbs. Part of the [.f3d
import](f3d.md) notes; the [import documents](../README.md#documents) list
the other topics.

## Opening a design with many face appearances (mitcad#311)

A design of a circuit board with a pole round it (content id `e3c34193`,
150 items, 78 occurrences) opened in 12 s before the appearances
(mitcad#228) and in 190–205 s after, with the same outcome. The commit
that made it slow is `9e165ea9` (points inside faces of extruded and
revolved curves, merged with mitcad#228 in `4dd9265d`): its 127 face
appearances name 5 074 stored faces, most of them faces of B-spline
curves extruded, and those then had points inside them. Two steps took
the time, neither of them the replay (8 s):

- **Reading** (25 s): the point inside a free-form face matched each edge
  point to the nearest point of a grid over the surface by going through
  the whole grid (up to 2000 × 2000 points), and built the grid evaluating
  the face's curve once per grid point. The nearest grid point is now
  found through cubic cells (`names/surface.rs`, `Nearest`: the same
  point as the full search, the first of equally near ones), the grid
  takes each curve point once (`Patch::grid`, the same points bit for
  bit), and the faces are worked out on the reading threads
  (`appearance::fill_faces`).
- **Applying them** (159 s): a face of a surface of linear extrusion had
  an unbounded box (OCCT's box of it is sampled), so every lookup through a
  point measured its distance from every such face of the body, with an
  extremum search that builds a grid each time. Such a face now gets a box
  of its own (`geometry/src/query.cpp`, `extrusion_box`: the hull of the
  curve's poles over the face's stretch, moved to both ends of its stretch
  along the direction, widened by its tolerance); a point beyond a face's
  box is not measured against it; and each body's faces, boxes and edge
  names are read once for all its faces (`refs::PieceIndex`).

The reports are byte for byte the same. Times of this design (release
build, default threads; seconds; reading, replay, applying the
appearances from the import trace):

| build | open | reading | replay | appearances |
|---|---|---|---|---|
| `b7e68d3c` (before mitcad#228) | 11.9–13.2 | 0.1 | 9.7–11.1 | — |
| `7aff9023` (main) | 189.6–204.8 | 24.7–25.7 | 8.0–8.2 | about 160 |
| this change | 11.1–11.9 | 1.2 | 7.7 | 0.9–1.0 |

The design's items: 69 parametric, no fallback, 81 skipped (the
occurrences, the component insert and two items suppressed in the
file); the file's own base features come in as its bodies. No parametric
item goes through a retry on the exact state, and all 54 modelling inputs
are read. What fell back or was partial on 8.10.2026 (main `7fe8bac`, 38
parametric, 2 partial, 27 fallback) and came in parametric since:

| item | 8.10.2026 | first deviation then | now |
|---|---|---|---|
| Revolve1 | fallback: "crosses the revolve axis" | the axis not decoded (guessed as the sketch's y axis), the profile guessed among regions that straddle it | parametric, the decoded axis and profile (revolve axes and flags decoded, mitcad#186; straddling profiles) |
| Fillet5 | fallback: not in the history | on Revolve1's fallback body, its edges taken from the stored body's faces | parametric, edges found by their names |
| Extrude8 | fallback: cuts no participant | its sketch on a fixed plane (the face was on Revolve1's fallback body) | parametric |
| Sketch12 | partial: a curve type not decoded | one curve left out | parametric (sketch curve kinds decoded, mitcad#186) |
| Revolve2 | fallback: its sketch has no profiles | Sketch12's missing curve left no region | parametric |
| WorkPlane1 (now Plane1) | partial: a fixed plane | the plane's inputs not decoded | parametric (construction planes from their inputs, mitcad#178) |
| Revolve3 | fallback: not in the history | the two touching regions fused into a wrong face | parametric (mitcad#227) |
| Extrude11 | fallback: not in the history | single regions tried where the file selects two | parametric, the decoded profiles |
| Fillet9 | fallback: the radius may be too large | the geometry kernel's walk stops at the lugs (below) | parametric (mitcad#311, the fallback below) |

**Fillet9** is a 0.9 mm rounding of the edge between Chamfer3's cone and
the pole's bore, a cylinder of radius 35.1 (the material outside it: the
edge is convex). Ten lugs stand into the bore up to z 0.8, their flat
faces chords of the bore, their tops 0.3 mm below the edge; the
rounding's contact on the bore lies r · tan 22.5° = 0.373 mm below the
edge, over each lug's top. The geometry kernel fails it on the replay's
body and on the file's own stored body (`--retry-bodies replace`): its
walk stops at the lugs (ChFiDS_WalkingFailure); up to 0.724 mm (a contact
of 0.300 mm) it builds. The file's result is one torus round the whole
edge, its contact at z 0.727 also over the lugs, whose tops grow out to
meet it (19.477 → 19.537 mm² each): the bore rounded as if the lugs were
not there, with the lugs kept.

A fillet the kernel fails on is therefore built once more with the
regions it runs over cleared from its faces (`geometry/src/cutouts.cpp`,
only where every other way failed, so a rounding that builds today is as
before). For each face of the selected edges, near the edges (as far as
the rounding touches the faces, with a margin), the regions that
interrupt the face's surface: a cut-out (outside the body on the face's
material side: a flat, slot or hole cut into it) or a protrusion (of the
body on its outer side: a lug or rib), each meeting the face along one of
its other edges, lying on the surface and coming within the contact
distance of the edges, bounded only by planes, cylinders, cones, spheres
and tori (its walls) and not by the edges' other face. Each region's tool
is the region on its side of every wall, near it (as far as the rounding
changes the body): a cut-out's must hold none of the body, a protrusion's
no other part of the body past the face's surface. The cut-outs are
filled and the protrusions taken away, the rounding is built on that body,
and the regions are put back: the cut-outs' tools cut out again, the
protrusions' tools' share of the body joined again. The walls' faces grow
to the rounding; the pieces of one face on one surface are merged, new
pieces named after the face they lie on. The result's note says so. Each
face is searched near its own selected edges only, and boxes, distances
and the region's corners on the sides of plane walls reject most
candidates before any boolean: where there is no such region, the fillet
fails a few seconds later on a body of a thousand faces. Two
geometry tests build the same from primitives (`test_fillet_cutouts.cpp`:
ten flats cut into a pole, lugs on a wall), each against the rounding of
the body without them with them put back afterwards (volume and faces).

`f3d.corpus_designs` (`tools/cli/f3d-designs.cmake`) checks the design
item by item where it is found by its content id under
`MITCAD_F3D_CORPUS` or `MITCAD_F3D_BACKUP`: the items above parametric,
none falling back, no parametric item after a retry on the exact
state, every input read, and reading and applying the appearances taking
less time than the replay. Without the design it is skipped.

## Light bulbs

`Importer::light_bulbs`, after the replay: each imported sketch and
construction plane, axis and point keeps the file's light bulb only where
it differs from MitCad's default (sketch hidden while a feature uses it,
construction geometry shown) — a used sketch the file shows, an unused one
it hides, a hidden plane. A sketch that follows the default keeps nothing,
so deleting its last consumer shows it again.

The light bulb is the item's `props.isLightBulbOn` (external dumps and the
decoder), else a sketch's `detail.isVisible` (external dumps); otherwise
the default decides. The decoder reads it for sketches of the current file
format (sketch class version 18, most of 17) and for construction planes;
older sketches, construction axes and points stay unknown. Copies of a
sketch imported into another component take its light bulb too
(mitcad#1802; before they followed the default). The report
counts sketches and construction features, those with unknown light bulb
and those kept (`light_bulbs`).

Occurrences: the file keeps one object (class `2BB3BB69`,
`classes::HIDDEN_OCCURRENCE`) per occurrence path whose light bulb is off
and none for those shown. Its last reference list is the path (`u32 n | n
× ref occurrence`, from the root component down); a top-level occurrence
of a component of another document has an empty list and the object refers
to that one occurrence of the file only; a path that goes on inside another
document names its levels there by context references and is that
document's own (its file holds it). The decoder turns `isLightBulbOn` off
on the occurrence node at the path; the others stay unknown (shown).
Against the API reference dumps: the hidden occurrences of all 35 designs
that have any agree by count per component (two designs name the
components differently), and in one design of 113 occurrences one by one.
An occurrence the file hides is hidden in MitCad
(`set_occurrence_visible`); one inside a component placed several times
(one occurrence in MitCad, a path per placement in the file) follows its
first path.

### Bodies' light bulbs (mitcad#1175)

A body record (class `D3937028`, the appearances' body record below)
of class version 19 keeps the body's light bulb after its browser node
GUID: `str16 GUID | f64 volume (cm³) | f64 | u32 | u32 1 | u8 0 | u32 |
u8 1 | u8 light bulb` (1 on, 0 off). The decoder gives it as the body's
`light_bulb` in `_f3d.appearance.bodies` (`appearance::light_bulb`); a
record of another version or laid out otherwise gives none, and its body
stays shown. The import hides a body whose light bulb is off
(`set_body_visible`), the replayed body the final comparison matched
with the stored one; the report's `appearances.hidden` and `hidden_set`
count them (`hidden bodies: x/y`).

Checked against the display scene the files keep (`mitcad-f3d-inspect
meshes`, the body's key its record's id in the key's last 8 digits): in
a sample of 50 designs of the corpora, 1 of the 54 version-19 bodies
whose light bulb is off has a mesh there (a body the file's preview does
not show either), against 119 of the 188 that are on (the others have
their faces listed without triangles, or are not in the scene: in hidden
occurrences, not in the design); and in a reported drill index the hidden body (a blank kept beside the copy the
holes were cut in) has no mesh and the file's preview shows the holes.
Shown, that blank covered the holes in MitCad.

## Appearances and materials

The file's appearances and physical materials (mitcad#228): the decoder
reads them into `_f3d.appearance` (`mitcad_f3d::design::appearance`,
`mitcad_f3d::protein`; SCHEMA.md §2), and `appearances.rs`
(`mitcad-import-appearances`, mitcad#791) gives them to the replayed
bodies and faces after the replay (commands.md, *Appearances and
materials of the file*). `mitcad-f3d-inspect assets <file> [-v]` lists
a document's assets, `appearances <file> [--json]` what the design gives
its bodies and faces, `blob-bodies <file> <blob>` a blob's bodies with
their keys and their faces' appearance GUIDs.

**The asset packages.** `ProteinAssets.BlobParts/*.protein` is a ZIP
archive (deflated) in the document's ZIP, one folder per asset set:
`Schemas/*.xml` (the property schemas) and `AssetData/`
(`InstanceProperties.bin`, `DefinitionIteratorProperties.bin`,
`AssetTableOfContents.bin`). Both `.bin` streams are paged, as the
published description has it:

```text
stream   u32 page size P (136) | 12 bytes | pages of P bytes
page     u32 | u16 P − 8 | u16 1    the first page of a record (its body whole)
         u32 | u16 P − 8 | u16 0    a further page
         ffffffff | u16 used | u16  the last page: the first `used` bytes
```

An instance record is `str schema | str asset GUID | str asset name | str
library id` (`str`: u32 length, ASCII) and then the values of the
schema's properties — its own and those of the schemas it is based on
(`<Base val=…>`), a derived schema's property in place of the base's of
the same id — in byte order of their ids, without the library id (in the
header), those marked `readonly` and those marked `definitionIteratorData`.
Values: Boolean u8; Integer and Choice u32; Float f64, with a `unit`
attribute u32 unit tag and f64; Distance u32 unit tag and f64; String,
Uuid and URL `str`; Color four f64 (RGBA in [0, 1]); Reference nothing;
TextureURI u8 kind, then kind 0 u32 n and n `str` (library resources),
kind 1 one `str` (a file name); a property with `allowmultiplevalues`
(not TextureURI) u32 n and n values. A property with
`allowconnectedassets` ends with u8 0, or u8 1, u8 1, u32 n and n `str`
GUIDs of the texture assets connected to it. A unit tag is `class << 12 |
unit`: lengths `0x200d` cm, `0x200e` mm, `0x2016` inch; density `0x11001`
kg/m³. A texture's schema (based on `TextureMap2dSchema`) writes one of
its two map channel integers less: the record decodes to its end without
`texture_MapChannel_ID_Advanced`. A description record is `str schema |
u8 | str name given | str asset name | u32 version` and, for version 2,
`str category | str | str description`, then u32 n keywords and u32 n
preview paths; the descriptions come in the instances' order (an edited
copy of a library appearance names another asset in its description, so
they are joined by position). Every record of every package of the
corpora and the reference models decodes to its exact end (5 558 assets
in 16 schemas: the opaque, metal and transparent shaders and their older
generic, metal, plastic, ceramic and glazing forms, physical materials,
structural and thermal aspects, bitmap and bump textures).

The name a person gave an appearance (`Custom Blue`) is the description's
name where it is no library preset; a library appearance's display name is
not in the file (the library resolves its preset), so the import names it
by its description (`Paint - enamel red glossy`, `Steel - satin`). A
texture's image is a library resource, not in the file either.

**The design stream.** A body's record (class `D3937028`, its object id
the body's entity `E`, its name `0_E`) names the body's physical material
and its appearance, and lists the faces with appearances of their own;
names are

```text
name     str16 token | str16 library | (u32 0 | str16 name given) | str16 preset
body     … u8 1 | u32 n | n × face | … | str16 browser node GUID | … |
         material name | ref (CD57BC48) | ref E + 1 | str16 body name |
         … | appearance name | f32 0 | f32 1 | …
face     str16 face GUID | 4 × f32 RGBA | u8 1 | u8 1 | 9 × u8 0 | u8 form |
         appearance name | f32 0 | f32 1
```

The token is the asset's GUID; the asset it names is the token's own or
the one of the token with `_Post2015` suffixes (an appearance kept for the
newer shaders beside the older one under the token), the one whose name is
the preset first. Libraries are upper-case GUIDs (`BA5EE55E-…`
appearances, `C1EEA57C-…` physical materials and the appearances they
bring, `314DE259-…` the generic one, older ones). A name whose asset is a
physical material (`PhysMatSchema`) is the body's material; the other one
not in a face entry its appearance. Face references (class `BFB592FE`,
a face input) that carry an appearance give it as `… | appearance name |
f32 0 | f32 1 | str16 face GUID`; those of faces the body's record does
not list are added to the body whose face carries the GUID. An override
(class `10506394`) names its component and occurrence (references) before
`4 × f32 | u8 1 | u8 1 | 10 bytes | appearance name`; one without an
occurrence is the component's own (the root's).

**Joins.** The body map (classes `74CA4562`, `40DCC3F4`: `u32 n | n ×
(u64 selector, u64 entity) | ref | 0–4 zero bytes | str16 blob name` to
the record's end) gives each entity its blob and selector. The selector is
the key a body's record carries (its header's second field) where a body
of the blob has it, else its position among the blob's last top-level
bodies, as many as the body map names: a snapshot blob (`.smb`) saves the
faces the design refers to as bodies before the design's (the faces' GUIDs
put the design's bodies there in all 38 snapshot blobs that showed it).
The body's ASM record in the blob is the stored body's (`StoredBody::source`,
`blob#record`), which the final comparison matched with a replayed body
(a record also names the lumps `<source>#k` of a stored body that comes
in as one body per solid, as the `.ipt` import's do, mitcad#1375). A
face's GUID is the string after `NEUTRON_Material_attrib_def` in an
`ATTRIB_CUSTOM` attribute of the face in the blob's stored state
(`NamedState::face_appearance_guid`); every face of the body carrying it
(copies and pieces carry it too) gives a fingerprint as a feature's face
input does (points inside faces of extruded and revolved curves since
mitcad#228, `names/surface.rs`; a planar face's point no longer falls into
a hole where the line through the mean of its edge points runs through
the seam of a closed edge, whose ends met only to rounding:
`names::point_inside` closes such gaps).

**Parameters.** The base colour is the shader's colour property
(`opaque_albedo`, `metal_f0`, `transparent_color`, `generic_diffuse`, …;
`common_Tint_color` where its tint is on), as stored (its colour space
sRGB; a linear one encoded); roughness `surface_roughness` (generic: 1 −
glossiness); metals metalness 1; an opaque surface's index of refraction
from its reflectance `f0 = ((n − 1)/(n + 1))²`; transparent: transmission
1 and its own index. Translucency, emission, bump, normal and cut-out
images and texture images are listed as notes.

**Materials.** A physical material's density is its structural aspect's
`structural_Density` (kg/m³); MitCad's material is set where its name is
the file material's description up to the first comma or the last part of
its classification (also with a grade after it, `Aluminum 6061`; ABS
written out), and its density the file's within 2 %.

### Against the reference models

The appearance variant models (`MITCAD_F3D_MODELS`, `diff_appearance_*`,
13 designs, 15 bodies) against their API dumps:

| | achieved / target |
|---|---|
| bodies with the file's appearance | 15 / 15 |
| faces with their own appearance | 1 / 1 |
| occurrence overrides applied | 1 / 1 |
| bodies with the file's material recorded | 15 / 15 |
| bodies with MitCad's material set (steel, aluminium) | 15 / 15 |
| colours within ΔE 1 (CIE76) of the dump's colour property | 15 / 16 |

The one colour off is the textured oak: its colour is the image (the
dump's colour property is null; the stored colour, white, comes in).
The largest difference elsewhere is ΔE 0.44 (the stored values against the
dump's 8-bit ones).

The body and face colours of all reference models' STEP exports (475
models) against the decoded appearances: 574 / 576 within ΔE 1, the two
others the textured oak and the occurrence override (its body's own
appearance is decoded; the import gives it the override); 10 entries
have no counterpart (models without bodies, and body records of bodies the
export leaves out, which the import leaves out too).

### Mesh bodies (mitcad#1376)

A mesh body record (class `EA90DA22`, `mitcad_f3d_decode::mesh`) names
its physical material and its appearance in two presentation envelopes
after its placements, each a name as above with an empty display name:

```text
envelope str16 token | str16 C1EEA57C-… | u32 0 | str16 preset
mesh     … placements | material envelope | str16 body name |
         appearance envelope | refs …
```

The first is the physical material (`PrismMaterial-018`), the second
the appearance it brings (`PrismMaterial-018_physmat_aspects:Prism-256`,
whose asset is the token's `_Post2015` form); the corpus' one mesh body
is laid out so, its two assets in the document's package. The decoder
gives them in `_f3d.appearance.meshes` (`entity` the mesh body record),
and the import gives them to the mesh body that record placed
(`meshes.rs` of `mitcad-import-appearances`), as a body's: after the
occurrences' overrides, which a mesh body keeps. The import of the
stored bodies without the timeline (`import_f3d`) gives them too.

### On the corpora

The corpus (`MITCAD_F3D_CORPUS`, 48 designs) and the owner's design
backup (499 designs), imported with `--time-limit 60` (items after it
take the file's bodies; the appearances join the stored bodies either
way):

| | achieved / target |
|---|---|
| the stored design's solids with the file's appearance | 5 726 / 5 741 |
| faces with their own appearance | 3 088 / 3 088 |
| solids with the file's material recorded (with its density) | 5 719 / 5 719 |
| solids with MitCad's material set | 4 482 / 5 719 |
| occurrence overrides applied | 40 / 58 |

- The 15 solids without: no body record names them. 7 are bodies of a
  history blob's stored state without a key, which the body map does not
  name (each beside a keyed body of the same faces, in 5 designs); 8 are
  bodies of a snapshot blob whose body map is empty (one design and its
  copy). Whether the stored design holds them at all is open: the import
  takes every top-level body of the stored state.
- The materials not set are ones MitCad's library does not have (a generic
  electronic component material of 6 020 kg/m³ on 1 120 bodies,
  polyoxymethylene, PC/ABS blends, …); they are listed with their density.
- 18 overrides are on components placed several times (MitCad keeps one
  appearance per body).
- 4 face references name faces the stored design no longer has; 4 566
  body records are of bodies the stored design does not hold (a snapshot
  blob's copies, bodies taken by later operations).
- Notes: translucency (18 appearances), bump and normal maps (20),
  texture images for the colour (8) and the roughness (5).


## Body names, body folders and bodies out of the design (mitcad#1802)

A large reported design (a house of ten components, 2246 body records, 24
body folders) came in with every body named `Body<n>` by MitCad, without
its folders, with flat surfaces and bodies the file does not show in the
view. What the file keeps, read from its bytes and checked against the
display scene it saves (`mitcad-f3d-inspect meshes`: meshes in their
component's coordinates; its scene graph keeps each occurrence's matrix):

- **Names.** A body record (class `D3937028`) keeps its name after the
  reference to its display record (class `702B9CD2`, module `Scene`):
  that record is entity `E + 1` only where the body was made with its
  record, so the decoder read 105 of the 2246 names before; a body a later
  item brought has it further on. A record that stores no name (410 of
  them) is shown by its default name `Body<n>`, whose n follows its
  browser node GUID (`f64 volume | f64 | u32 n`); the removals the file
  names after their bodies (`RemoveBody-Body361`) agree with every n.
  The decoder gives the name as the body's `name` in
  `_f3d.appearance.bodies`; the import names the replayed bodies so
  (`Document::name_bodies`, one step for all of them). Names are unique
  within a component in MitCad now, as in the file: bodies of different
  components may share one (176 of the design's do).
- **Folders.** A body folder (class `3A8E00DC`, module `Body`) keeps its
  name, its place among its component's folders and the body records it
  holds (`appearance::body_folders`); the decoder gives each body's
  `folder`, the import puts the body there (the model's body folders,
  commands.md, *Folders of the Bodies folder*). The folders of the design
  do not nest; a folder that stores no name (one of the corpora) is
  named `Folder<place>`.
- **Browser nodes.** The body record of class `D26351F0` (the one body
  inputs refer to) keeps `u8 out | u8 light bulb | ref body` after its
  node GUID (`appearance::body_nodes`). Of the bodies whose `out` is 0 the
  scene draws those whose light bulb is 1 (1064 of 1065); every body
  whose `out` is 1 (922) is missing from the scene: a removal took it out
  of the design, though its blob still holds it. The decoder takes the
  node's light bulb where the record has no light bulb byte (records
  laid out without it), and marks the bodies out of the design
  `removed`. Such a body still in the replay at its end (its removal did
  not come in) is hidden and counted in the report (`removed_left`, the
  text's `bodies the file took out of the design`).
- **Removals.** A removal's body input names its body object
  (`_f3d.body_entity`); where neither the item that made the body nor
  its names find it, the import takes the stored body of that object's
  record in the history state and removes the replay's body with the
  same faces and measures (`Importer::removed_by_record`). A removal
  after items the history has not settled yet (pending) was skipped
  before; now its body is found so in the state the replay last stood
  for, removed without a check against a state, and the stored body
  leaves the states from there on (`Oracle::retire`), so that the
  pending items' fallback does not bring it back
  (`Importer::remove_behind_pending`). Of the design's 676 removals not
  suppressed, 138 came in before, 664 now.
- **Suppressed removals.** A removal suppressed in the file because the
  pattern that made its body is (the copies a suppressed pattern made,
  removed one by one) came in as a placeholder, as no state of the import
  holds such a body. Two things changed (*Items suppressed in the file*,
  [f3d-fallbacks.md](f3d-fallbacks.md#items-suppressed-in-the-file)): a
  timeline group between suppressed items no longer ends their run (the
  design grouped eight removals right after their suppressed pattern, so
  the pattern was suppressed again before them); and a suppressed removal
  whose body was made by a feature suppressed before it comes in
  suppressed with its definition, not built (`Document::
  add_import_suppressed`): its body is that feature's body of the file's
  index (`_f3d.producer`, `_f3d.body_index`), unchecked as every
  suppressed item is. Of the design's 93 such placeholders, 85 remained
  after the first change, 2 after the second (one's producer is a
  placeholder itself, the other's is not suppressed and its body is gone
  by then).
- **The stored design** is the design's bodies: the stored bodies whose
  records say the file took them out of the design are left out of it
  (`Importer::out_of_design`), so the last fallback (`Stored bodies`)
  takes out those the replay still has. A stored solid no body record
  names, where the records name the bodies of their blobs, is no body of
  the design either: it stays, hidden (the report's `unnamed_hidden`).
- **Surfaces of sketches.** A component's history blob keeps a sketch's
  profiles and features' tools as sheets beside the design's bodies; a
  fallback's base feature took them in as surface bodies (eleven flat
  surfaces on the ground plane and a wall's plane, which looked like
  sketches whose light bulbs did nothing). Where the file's body records
  name the bodies of their blobs, a fallback now takes only the sheets a
  record names (`Importer::design_sheets`).
- **Copies of sketches.** A sketch imported again into another
  component for a feature there (*Across components*,
  [f3d-components.md](f3d-components.md#components)) takes the file's
  light bulb as the sketch itself does; before it followed the default,
  and where its feature fell back to the file's bodies it no longer used
  the copy, which then showed (48 of the design's 107 copies, drawn where
  the file hides every sketch but one).
- **Placements.** The occurrences are placed as the file's last captured
  positions place them, which is also where its display scene draws them
  (two components moved by captured positions, by 14.6 mm and 125 mm);
  the final comparison now gives each body's `center_distance` and
  `box_distance` against the file's body in their component's
  coordinates (`refs::compare_bodies`).

The ctest `cli.f3d_reported_house` (`tools/cli/f3d-reported-house-test.py`)
imports the design (MITCAD_MITCAD_REPORTED, with expectations read from
the file's bytes apart from the decoder) and checks every body.

## Bodies by their records for every reference (mitcad#1808)

What mitcad#1802 left of the same design: references other than removals
still found their body by its maker's index, 12 removals not suppressed
in the file were skipped, and 15 items suppressed in the file came in as
placeholders.

- **References by record.** Every body input, not only a removal's, goes
  through the body's record first (`records.rs`,
  `Importer::recorded_body`): the stored body of its record in the state
  the replay stands for, and the replay's body of the same faces and
  measures in that body's component; where none has the same faces, the
  one within the import's tolerance (a replayed split piece can have its
  faces split otherwise); where several do (a pasted copy where its
  original is), the one the item that made the body made. The maker's
  index is the fallback, and the record's body refuses it where they
  differ (29 references of the design: the index counts the bodies the
  maker still has in the replay, or a fallback's bodies with others).
  A record is where the body is at the end of the timeline, so it is not
  used for a body that a cut and paste at or after the item moves into
  another component (its record is in the destination's blob, where an
  earlier state holds another body under that number), nor where the
  maker made one body with its own features and the input names it by
  the first index. Without those two rules the record took the wrong body
  for 3 cut and paste items and one extrusion of the design, each of which
  the maker's body, checked against the history state after it, had right;
  a wider rule (the record's body must be one its maker made) lost 83
  removals, as the replay's list of an item's bodies does not follow the
  bodies that cut and paste and body transfers moved.
- **The 12 removals** came in as remove features: one by its record in
  the stored body's component (a copy and its original lay in one place,
  in two components), one by its maker among two copies in one place, 10
  by the tolerance (the pieces of a replayed split, whose faces the file
  splits otherwise). All 676 removals not suppressed
  in the file are remove features now.
- **Placeholders, 15 to 10.** A move whose matrix the file does not keep
  comes in by its decoded definition (a translation by its distances). A
  split by sketch curves: the selection's entity inputs name the curves by
  their tags without a target object, which the decoder now reads
  (`Builder::member`), and the import makes one tool of the curves of
  each sketch. A copy or move of bodies a suppressed feature made comes
  in suppressed and unbuilt, as such removals do (`Importer::unbuilt_body`,
  the note *it takes bodies items suppressed before it made*). The 10
  left: five moves of faces and two splits whose faces are in no state of
  any of the file's history blobs (every state of the ten blobs searched
  by the faces' names: the suppressed items that made them have no
  states), the removal of one of those splits' bodies, the removal of a
  body that a later cut and paste moves (its maker's bodies are gone from
  the replay by then, and its record names another body at that point),
  and one move of a face the kernel refuses on the file's own body (the
  replay matched the file's state before it within 3e-13). Their notes
  say why in words.
