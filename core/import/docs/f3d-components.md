# The .f3d import: components, joints and assemblies

Components and occurrences, components of other documents, derived designs,
sketches on other components' faces, joints, assembly relationships and the
items without a translation. Part of the [.f3d import](f3d.md) notes; the
[import documents](../README.md#documents) list the other topics.

## Components

`components.rs` makes the dump's components and places them by the
occurrence tree before the replay:

- placements: the decoder's top-level `transform` (relative to the root)
  and nested `_f3d.local_transform` (relative to the parent), or an
  external dump's `transform2` (full path) made relative to the parent
  occurrence; cm become mm; grounded and hidden occurrences stay so (the
  decoder reads the light bulbs: [*Light bulbs*](f3d-appearances.md#light-bulbs));
- a component used several times comes in once and is placed by each
  occurrence;
- occurrences of components of other documents (`isReferencedComponent`:
  inserted parts, fasteners) are occurrences of empty components, one per
  component of each document, named after the item that inserted it
  (`Screw1`, `Component Insert2`: the decoder gives each occurrence item
  the occurrence it made), else `Inserted component` (mitcad#75). Their
  bodies are not in the file, but joints, captured positions and ground
  items name them. The report counts them apart (`external`). Once the
  replay is done, those documents' own files fill them where they are
  found (*Components of other documents* below).

The file has one timeline for the whole design; every item goes into a
component:

- an external dump names it;
- for the decoder, the component whose ASM blob the item's operation
  changed (`StoredGeometry::item_components`; each component keeps its
  bodies in its own blob, in its own coordinates), else the component that
  owns the item: every component has a list of its items (class
  `3A6D1E62`, `classes::FEATURE_LIST`, which refers to the component's
  feature manager), and the decoder writes the owner as the item's
  `component` and `_f3d.component`. A feature that changes another
  component's bodies than its owner's (the file's assembly context) goes
  where they are;
- sketches and construction geometry follow the first feature that uses
  them, unless their owner is placed elsewhere than that feature's
  component at that feature's point of the timeline (*Occurrence
  placements*): their geometry is in their owner's coordinates, so they
  stay in the owner and the feature gets a copy (*Across components*).
  One no feature uses goes into its owner;
- other items without an owner follow the next item with a component
  (else the one before);
- a definition goes where the bodies, sketches and construction geometry
  it names are when they agree, else into the item's component; a sketch
  whose owner is known goes on a face of its own component only.

**Components made from bodies** (`ComponentFromBodies`, mitcad#156). The
component is placed by the occurrence tree before the replay, like any
other. The item's state is the new component's first one (its
`_f3d.made_result_no`, [*The history as the reference*](f3d-history.md#the-history-as-the-reference)): the bodies that
state gives the new component, each the same solid as a body of the
replay's in the item's own component, move there by a
`component_from_bodies` feature in that component, whose component is
then the placed one (`Document::move_made_component`; the one the
feature made goes). The history of the item's own component keeps the
moved bodies as they were, though the design no longer has them there:
they leave its states from the item's on and the stored design
(`Oracle::retire`). Later features of the new component work on the
moved bodies. Before, the stored bodies stood in for the item, and the
moved bodies stayed in the root too (two copies in the design).

New-component operations become new bodies of that component. Fallback
base features are made per component, and the final comparison wants
every body in its component (a body left in another one is replaced
there). The report counts components, occurrences and items per component
(`components`) and names each item's component.

A component whose bodies are stored only in an `.smb` blob (no `.smbh`
history) has no features; its solids join the stored design unless one of
the last state's bodies is the same, and come in with the final
comparison. The `.smb` blobs of components with a history also hold
feature tools and sketch faces, which stay out. Occurrence items and
other assembly items change no body and are skipped (occurrence
placements come from the tree; *Items without a translation* below);
joints, as-built joints, joint origins and ground items come in as
MitCad's (*Joints* below).

**Across components.** MitCad's features work only on their own
component's bodies, so a feature of the file that changes another
component's bodies (the file's assembly context: a part cut by a sketch of
the assembly) goes into the component whose bodies it changes, and what it
uses of other components comes along:

- a sketch of another component is imported again into the feature's
  component, moved by the two occurrences' placements at the feature's
  point of the timeline onto a planar face of its component or a fixed
  construction plane, so it stays parametric (its dimensions keep the
  file's parameters); features at points where the placements differ get
  copies of their own;
- an extrusion up to a face of another component goes up to a fixed plane
  where that face's occurrence places it in the sketch's component at the
  extrusion's point of the timeline.

- a combine whose tools are bodies of another component than its target
  (a part cut by another part) goes into the target's component and
  takes those tools by their occurrence links (`tool_links`,
  `commands.md`, *combine*; mitcad#104): the model reads each tool in its
  component and moves it by the placements at the combine's point of the
  timeline. Such a combine that consumes its tools lists body removals
  in their components instead of body records (`selections.rs`); the
  consumed tools leave their components. Before, these combines fell
  back (5 items of `MITCAD_F3D_CORPUS` and 17 of the backup corpus, all
  but one cuts keeping their tools); all now give their states.

These need each component placed once (one occurrence on the way from the
root); else the feature falls back. A feature that changes the bodies of
two components at once is one feature per component in the file (*Parts
of items, face profiles and sketch heights* below), each replayed in its
own component. Other features that work on bodies of two components at
once fall back: the base feature takes the item's history state in each
component, and later items continue parametrically.

**Occurrence placements.** The transform the file stores for an
occurrence is where the item that made it put it; joints, captured
positions (`Snapshot` items) and rigid groups after it move it without
changing that transform. The decoder reads the occurrence's placement
after each item that placed it (mitcad#81), so the file places an
occurrence at the end of its timeline at its last one (else at its last
captured position, else at its stored transform), and that is the
occurrence's own placement in MitCad; the captured positions also come in
as `capture_position` features at their points of the timeline
(*Joints*). The history's states are in each component's own
coordinates, so the matching does not depend on placements; only the
copies above do. Those take the placements the file has at the item's
point of the timeline (`Captures::placement_at`, mitcad#86): where the
items before it put the occurrence (else, without its placements, its
last captured position before the item, else its stored transform), not
where the occurrence starts in MitCad. Where a joint or a move places an
occurrence elsewhere at an item's point in the timeline than the
placements say, the copy does not follow it and the item falls back.
Joints come in only where they hold where the file places the
occurrences (*Joints*); occurrence moves are not replayed ([*Limits*](f3d.md#limits)).

The placements are given per path, in the root component. An occurrence
inside a component placed several times is one occurrence in MitCad (in
that component) but has a path per placement of its parent: each of its
steps is made relative to the parent's placement along the step's own path
at the step's point of the timeline (`Captures::path_steps`), and those are
its placements in the parent. Before, the last path's root placement was
taken as its placement in the parent, so every copy of the parent moved it
there: in a design of 20 copies of a part (each a component holding one
occurrence of the part's body component), all 20 bodies stood one copy's
offset away from their places (41 of the design's 113 occurrence paths
misplaced; none after).

## Components of other documents (mitcad#196)

A design can place components of other documents (inserted designs,
fasteners). The `.f3d` file of the design does not hold those documents,
and a multi-design package of such a design cannot always be made, so
their files are kept beside it: next to `<design> vN.f3d` a folder
`<design> vN.refs/` holds one `.f3d` per referenced document at the
version the design uses (`<name> v<version>.f3d`), for the whole tree of
references, and `references.json`, whose tree maps each document's
version id (the URN the design names it by) to its file (format:
`core/model-api/src/commands.md`, *Components of other documents*).

**What the design stores about them** (the decoder's reading,
`core/import/SCHEMA.md` §5.2):

- The MetaStream's tail lists the external documents: per document a key
  (`u64`), two GUIDs (`str16`), the URN (`str16`,
  `urn:…:fs.file:vf.<id>?version=N`), a GUID (`str16`) and a `u32`.
  References into another document carry the key: `01 id [str8 guid] 01
  key 00 str8(guid) 00`, where `id` is the object's id in *that* document
  and the GUID names its class.
- An occurrence (`CE2913AA`) of a component of another document refers
  to the component that way: the root component of the other document
  (its `id` there). The reader took such a reference only when the id
  happened to be an object of this design as well (a root component has
  the same small id in many designs), so occurrences whose component id
  was none of the design's own were missing from the occurrence tree (one
  of five in the first design looked at). The reader now takes it by the
  key and the class GUID alone. The transform after it is the
  occurrence's placement, as for any occurrence.
- The occurrence object carries GUIDs (`str16`); the paths of captured
  positions and relationships name each level by one of them
  (`path_guids`). A level inside a component of another document is an
  occurrence of that document, named by a GUID its own occurrence object
  carries in its own file (checked on a bearing whose balls' positions an
  assembly captured).
- The external document's first GUID is in the objects of its
  occurrences too; the last one is one of the GUIDs the referenced file's
  `Manifest.dat` lists (the same for every version of that document in
  the files read), which tells the file without its name. The design
  stores no name of the document.
- Joints, as-built joints and relationships name occurrences of these
  components as of any other; their geometry inside such a component is
  named through the key as above.

**The import.** The caller (`mitcad-ffi`, `f3d_references.rs`) first finds
each document's file (`reference_folder.rs`: a file given for it; the
reference folder next to the design or one given, by `references.json`,
else another version of the same document there with a note; another
design of an `.f3z` package, see below; a `.f3d` of those folders whose
manifest lists the document's GUID) and imports it on its own with the
same options and its own references (the same folders; a document that
refers back to one being imported is not imported again; 16 levels at
most), before the design is replayed. Each document (file and design) is
imported once per import: the documents imported so far are shared by
the imports of the documents it refers to, so a document placed by
several occurrences, or referred to on several levels, is replayed once
and copied from that import for each of them (mitcad#229).

**Packages.** An `.f3z` package holds the design and the documents it
refers to, one `.f3d` per design. `Manifest.json` names the root design
(`{"root": "<file>.f3d"}`; imported when no design is chosen, else the
one with the longest timeline), and `DesignDescription.json` lists every
design with the URN designs refer to it by (`about`, with its version),
its `.f3d` in the package (`relativePath`) and its document's name
(`friendlyName`, `displayName`; `mitcad_f3d::design::package`). A
reference not given a file is resolved through that list before the
reference folders (the exact URN, then the same document at another
version, with a note), and the occurrences take the document's name
from it. Without the list the design of the package whose `Manifest.dat`
lists the document's GUID is taken (of several, the one listing the
fewest GUIDs), but never the root design or a design being imported:
an assembly's `Manifest.dat` lists the GUIDs of the documents it refers
to as well as its own, so the first design listing the GUID was often
the assembly itself (a root assembly with four fasteners replayed four
times, its bodies placed for three of the fasteners).

The replay places each occurrence of such a component as an occurrence
of an empty component (*Components* above), records them (`ComponentReport::inserted`, with the document's URN and
GUID), the GUIDs of all occurrences, and the captured positions whose
paths go inside them (`positions_inside`); then, before its first item
(`prefill.rs`), `references.rs` copies each imported document with its
history into the component standing for it
(`Document::fill_component_copy`, named after the document). Occurrences
of one document share a copy where the captured positions arrange their
insides alike (every inner occurrence either placed the same by the
positions of both, or placed by neither: each level's placement is
compared in the occurrence's coordinates, with the document's own
placement where only one has a position, `split_variants`); the others
get a component of their own, made for them first
(`Document::give_occurrence_own_component`, `place_other_component`), so
each arrangement has its own copy. Components placed more than once
inside a copy (a bearing in an assembly) have one set of inner
occurrences: where the positions put one of those differently by
different paths, the occurrence of the later path gets a copy of its
component (`Linker::separate`, `Document::make_unique`, which copies the
component with its features and the components inside it) and the
positions then name the occurrences of the copy (a GUID names one
occurrence per copy, `ComponentReport::occurrence_guids` lists them; the
one whose parent the path is at is meant). The occurrences inside start
where the design's captured positions put them: each path's levels
inside the document by their GUIDs, each placement in its parent worked
out from the placements the earlier positions gave the levels above it
(a position names the levels it moves and leaves those it does not), the
last one where the paths agree, and every placement kept for the items
between (`Captures::placement_at`). The captured positions come in as
`capture_position` features at the end. It is all one undo step with the
import.

The replay matches the design's own history, which holds none of the
copies' bodies, so while it runs the document's body view leaves the
filled components (and the components inside them) out
(`Document::set_import_scopes`, `refs::bodies_for`): matching, the final
comparison and the fallbacks see the design's own bodies only. What the
design names in another document is looked for among that document's
bodies (its `_f3d.document` GUID): the reference resolution in `refs.rs`
takes the bodies of the scope. So the items that name things inside the
copies find them as they find those of any other component:

- **Joints, as-built joints, relationships, grounded occurrences.** A
  path level inside another document is an occurrence of that document,
  named by a GUID (`Importer::occurrence_path`); a side whose frame lies
  on the copy's faces and edges goes on them (with the frame override
  where it does not), else on the copy's origin (a joint) or fixed
  geometry (a relationship) as before, counted as sides on a component of
  another document. A rigid group whose members are all inside one
  occurrence has nothing to join (it says so).
- **Bodies, faces and edges of another document named by an item**
  (decoder: *Entities of other documents* in SCHEMA.md). A body is the
  body of the copy that matches the stored body the decoder found in the
  other document's history (`_f3d.stored`, the other import's
  `bodies[].file`; `ComponentReport::external_bodies`, one per copy): a
  combine whose tools are bodies of another document
  (`Importer::external_body`) takes them by their occurrence links like
  any tool of another component. A face or edge is found by its geometry
  among the scope's bodies, so extrusions up to or from faces of other
  documents' bodies and planes on them find the copy's face.
- **Which occurrence.** A document placed more than once has an
  occurrence per placement, and the file names the entity by its names in
  the document only; what tells the occurrence is the placement of it that
  the reference's slot carries (`_f3d.context`, SCHEMA.md). Once the
  components are filled, `focus.rs` finds for each such reference the
  occurrence path of the design whose placement at the item is that
  (before the item or right after it, the design's own occurrences as the
  captured positions have them there, those inside the copies as the
  positions that placed them did): the reference is looked for among the
  bodies of that occurrence's component
  (`Document::set_import_focus`, `refs::bodies_for`), and the links of
  the features that use the body (a combine's tools, an extrusion's
  objects, a plane's faces) lead through that path, not through the
  component's only path. Where one item names two occurrences that share
  a component (a plane between the end faces of two copies of one bar),
  the later one gets a copy of its own (`Document::make_unique`), so that
  the body each reference is found on tells the occurrences apart.
  Documents that share a GUID are told apart by the decoder the same way
  (`resolve_external`). A reference that no occurrence is at (the
  occurrence is inside a document the design does not name, or the file
  has it moved since) is found as before.
- **A plane between faces of components.** A construction plane's
  definition takes faces, edges and vertices of other components' bodies
  through `links` (`commands.md`, *construction_plane*; one link per body,
  as an extrusion's `object_links`), so that a midplane between two
  placed faces is made in the plane's component's coordinates where the
  design places them, and follows when they move; before, such a plane
  stayed a fixed plane.

**Not found.** A document whose file is not found leaves its components
empty, as before; the report lists it (`missing`, or `skipped` where the
user declined to give it) and a warning names it. The application asks
for each one with a file dialog naming the document (its name where a
reference folder gives one, else the design's, and its version), whose
Skip leaves it out; a chosen file's folder is searched for the others
([app/COMMANDS.md](../../../app/COMMANDS.md), *Files*). `mitcad-cli
import-f3d` never asks: `--references DIR` adds folders,
`--no-references` leaves the components empty (and the things inside them
as before: joints on the origin, sides inside left out).

**The report** (`references`): per document its URN, name, version,
file and how it was found, `status` (`linked`, `missing`, `skipped`,
`failed`), a note, MitCad's components, the occurrences, its own import's
items by outcome and its own references; `components.linked` counts the
occurrences filled, `components.placed_inside` the captured positions
inside them that placed copied occurrences.

**Tests.** `mitcad_f3d::testreferences` writes a synthetic assembly that
places a part once and a sub-assembly (which places the part) twice, and
its reference folder; `core.f3d` reads its occurrences and documents,
`core.import` the folder's lookups, the filling on the mock kernel (a
two-document dump with a captured position inside the part), a joint
inside a copy translated in place, and a combine whose tool is a body of
the part, the occurrences of a part that the positions arrange
differently (a copy each), a component placed twice inside a copy whose
inner occurrences the positions place differently (a copy of it, the
pins where the file has them), a tool read through the occurrence its
context names, two occurrences named by one item (a copy each), and a
removal that names a cut's first record (the new piece, not the target),
`core.f3d` the slot's placement and the choice among documents that
share a GUID, `core.model` the import scopes, the operations that give an
occurrence a component, make it unique, or place another one, and a
construction plane between faces of components, and
`cli.import_f3d_references*` the import from the folder, without it (the
documents missing) and from a folder given (found by their GUIDs), and
`app.ui-windows-references` the application's dialogs (skipped, then a
file chosen whose folder holds the other).

**Corpus counts.** The 53 designs of the backup corpus that have a
reference folder next to them, imported one by one (release build; the
counts of a run without references of the same build first, then with the
folders; 128 referenced documents linked, 2 not in their folders):

| | without references | with the folders |
|---|---|---|
| items parametric / partial / fallback / skipped | 2089 / 49 / 196 / 633 | 2126 / 47 / 187 / 607 |
| captured positions inside other documents' components left out | 1794 (in 20 items) | 0 placed inside the copies: 385 |
| of them, left out because occurrences of the component put the occurrence in different places (one copy for all) | | 0 (112 in one design before a copy per arrangement) |
| components made for occurrences arranged differently, copies of components placed twice inside a copy, copies for occurrences one item names | | 9 (in 5 designs) |
| joints with a side on the origin of an empty component | 31 | 0 (on the copies' geometry) |
| relationship rows with a side inside a component of another document, left out | 61 | 0 |
| relationship sets / their rows translated | 54 / 76 | 67 / 91 |
| joints skipped | 40 | 29 |

All designs import without an error and with their final bodies as
before. What the folders gave: the 4 combines whose tools are bodies of
another document, 2 planes and 2 extrusions on faces of other
documents' bodies, joints, relationships and rigid groups inside the
copies (as joints where they hold where the file places the occurrences,
else as as-built joints or constraint rows left out with the reason).
What the copies per arrangement and the occurrence a reference is in gave
on top: the captured positions of designs that place one assembly six
times, or twice, each arranged differently inside, are placed (273
positions placed inside the copies of one design, 26 before), the
midplane between the end faces of two occurrences of a bar is a
parametric plane (it was a fixed one), and the removal after a cut that
splits its target takes the new piece: the fillets after it are
parametric (2 designs; the two bodies of the cut were counted target
first, the file counts the new piece alone). The rest, with the cause:

- Rows and joints that do not hold where the file places the occurrences
  (87 rows): the placements of occurrences inside a copy are known only
  where a captured position names them.
- An assembly of printed circuit boards names 201 of its inner
  occurrences' positions by GUIDs that no stream of the board's file
  holds (checked as text and as bytes in all of the file's streams and in
  the design's other referenced files; they stand in the assembly, its
  earlier versions and another design that embeds it): 29 of the 189
  GUIDs of the board's components are the second GUID of an occurrence of
  the board, the others name nothing the board has, and of those 157
  positions only 18 put an occurrence where one of the board's
  components is (140 do not; the board is dense, so a few millimetres of
  tolerance would pair many by chance). The assembly's positions are
  from a state of the board that the referenced version does not have, so
  they stay left out ("the other document does not have it").
- Planes and extents between faces of components of other documents
  where the file's slot carries no placement that matches an occurrence
  (an occurrence of a document the design does not name, or placed in the
  file by a step the import does not follow): found as before, among all
  the copies' bodies.
- Derived designs: their items are decoded and translated since
  mitcad#324 (*Derived designs* below); the derive items of the design
  that names a document no occurrence places bring no entities.
- Primitives placed on a face (cylinders, spheres): the placement is not
  decoded in any design.
- Two designs of an enclosure whose documents are placed once each keep
  two extrusion joins that change the bodies of two of the design's own
  components (a lid and a base): the replayed result has other faces in
  both components than the file's state, so they fall back. They name no
  entity of another document; the fix is in the extrusion, not in the
  references.

## Derived designs (mitcad#324)

A derive item (`DerivedContext`) brings bodies of other documents into
its component: a part drawn in the context of an assembly derives the
bodies of the assembly's other parts it is built around. The decoder
reads its record (`build/derive.rs`, SCHEMA.md §5.3 `DerivedContext`):

- **The source design**: a reference to the root component of the
  document the derive was made in (an assembly), through that document's
  key, and its name twice.
- **The entities**: per entity its own base feature in this design (the
  bodies it holds), the entity in the document that holds it (a part the
  source design places; a body object, by its id there and the document's
  key), the properties that stay linked (`Appearance`, `Geometry`,
  `PhysicalMaterial`, `Properties`, `Transform`) and a placement.
- **The placements**: the deriving design's in the source design, and the
  entity's occurrence's there, each an occurrence path of the source design
  (GUIDs from its root down) with a matrix where one was captured. Where
  there is none (the deriving design's placement in 8 of the 9 body
  derives of the corpora), the path is
  looked up in the source design's own occurrence tree: the source
  design's file is found like the documents the design refers to and its
  design streams are decoded for the tree, not imported
  (`derive_sources.rs`). Where a matrix is stored it is the full
  placement down to the entity's component *(the one design that has
  them: its derived body is where the matrices put it)*.

The derived body is where the derive puts it in the deriving design,
`placement(design)⁻¹ · placement(entity)` *(the stored derived bodies of
the corpora)*. The import (`derive.rs`) treats each entity's document as a
document the design refers to (*Components of other documents* above):
found, imported from its own file and copied into the design, here as a
hidden occurrence in the item's component at that placement (the levels
the body's component sits at inside the copy taken off). The item becomes
MitCad's `derive` feature (`commands.md`, *derive*): a linked copy of the
copy's body that its own import matched to the body object (its
`_f3d.appearance` body records, `DesignReport::body_entities`), checked
against the file's history like any other item. A derive whose record
lists no entities brings nothing: skipped as no geometry. The hidden
sources' bodies lie under the derived copies, so sketches' faces and linked
curves are not looked for on them.

**Corpora** (each design once by its content, imported alone with its
reference folders; 14 designs, the main corpus' package design among
them):

| | achieved / target |
|---|---|
| derive records decoded | 23 / 23 (9 bring bodies, 14 nothing) |
| derives that bring nothing: no geometry | 14 / 14 |
| derives of bodies parametric, checked against the history | 7 / 9 |
| their bodies matching the file's state after the item | 10 / 12 |
| final bodies of these designs exact (\|dV\| < 1e-6) / within 0.1 % / off | 21 / 4 / 0 of 25 (as without the translation) |
| sketches on a derived body's face (were on fixed planes) | 4 / 6 |
| linked curves of the sketches after them as projections (were kept fixed) | 144 / 229 |

The two derives that stay fallbacks: one design's derived body is its
source part's body of an earlier state (59 faces: the part's history state
five items before its end), not the referenced version's final body
(3.0 % more volume, its centre 4.9 mm away), so the file keeps a derive that
was not updated to the version it refers to; the other's source version
is not in the reference folders (another version of the document stands
in, a different body). Their sketches stay on fixed planes with their 77
linked curves fixed, as before. The other 8 linked curves still fixed are
in a design that derives the same body twice to the same place: each
follows two coincident edges.

**Tests.** `core.f3d`: a synthetic derive record (source, placements,
one body, the linked properties) and one that lists nothing; the
placement of an occurrence path in a source tree. `core.import`: a part
deriving the block of another document from an assembly (the hidden
filled component, its placement, the derive's link and copy), a derive
that brings nothing, and one whose document is not found.
`core.model` (`link_tests.rs`): the derive feature, its link, following
the source, the design file, and Paste New of a component holding its
hidden source.

## Sketches on other components' faces, linked curves as projections (mitcad#225)

A sketch may lie on a face of a body of another component (a sketch of
the root on a circuit board's face), and its linked curves (projected
edges and faces, *Sketch curve flags* below) follow edges of its own
component or of others. The model keeps both through occurrence links
(mitcad#100, `commands.md`, *Geometry of other components*); the import
uses them (`linked.rs`):

- **The plane.** A sketch whose component the decoder gives, and that no
  planar face of its own component carries, lies on the planar face of
  another component's body, seen through one of its occurrences (those of
  components of other documents, mitcad#196, included), that is in the
  sketch's plane: the only one looking along the sketch's normal, else
  the only one against it. It comes in on that face with a `plane_link`
  (the occurrence path from the root to the face's component, and the one
  the sketch's component is seen at: its first path) instead of a fixed
  plane; the note names the component and occurrence. Two such faces
  (two parts' faces at one height) leave the fixed plane.
- **Projections.** The file does not name what a linked curve follows,
  so the replay is searched: the edges of the sketch's own component and
  those of every other component's bodies through each of their
  occurrences (bodies whose box does not come near the sketch left out),
  moved into the sketch's component. A linked line, circle, arc or
  ellipse that exactly one edge projects onto (within the snap's 2e-3 mm;
  the edges in the sketch's plane first, as the file projects the face a
  sketch lies on) becomes a linked projection of that edge: with a `link`
  when the edge is another component's, without in its own. The edge's
  name must give one curve, and the curve must already be where the
  model's projection puts it (the snap put it there: the choice of edge is
  made once, and the snap puts the curve on the chosen edge's projection
  first, mitcad#227; a line the edge runs the other way round is turned),
  so following it moves nothing on import. The sketch is tried with the
  projections first, then, at each level, without them.
- **Kept fixed.** A linked curve no edge or several edges project onto
  (a wall's top and bottom edges seen along it), a spline, or one whose
  point the snap could not move (shared with a curve put elsewhere) stays
  fixed reference geometry, and the sketch's note says so: "n linked
  curves as projections (k of other components); m linked curves kept as
  fixed reference geometry (no single edge of the replay they follow)".
  A copy of a sketch in another component (*Components*) keeps its linked
  curves fixed.

A linked circle keeps where the file starts it (its `start_angle`, the
seam of what is made from it) when it follows its edge (`commands.md`).
A sketch with such links stays in its own component (the bodies its
links name are not its component's).

Six assembly designs imported one at a time (with their reference
folders): sketches that stood on a fixed plane because their face was not
found, now on a face of another component; linked curves now projections
(of other components / of their own component) and those kept fixed. The
item outcomes and modelling inputs are the same as before in all six.

| Design | Sketches on another component's face | Linked curves as projections | Other / own component | Kept fixed |
| --- | ---: | ---: | ---: | ---: |
| `e3c34193` (a circuit board with a part around it) | 1 / 1 | 525 / 537 | 499 / 26 | 12 |
| `c3e08097` | 5 / 10 | 192 / 266 | 58 / 134 | 74 |
| `48ff426e` | 9 / 9 | 428 / 470 | 126 / 302 | 42 |
| `52ba7a15` | 5 / 8 | 400 / 450 | 192 / 208 | 50 |
| `bd57969c` | 4 / 9 | 191 / 265 | 57 / 134 | 74 |
| `db73863f` | 2 / 4 | 92 / 193 | 32 / 60 | 101 |
| all | 26 / 41 | 1828 / 2181 | 964 / 864 | 353 |

The sketches left on fixed planes have no planar face of any replayed body
in their plane at their point of the timeline (in the two related
designs `c3e08097` and `bd57969c` the replay has no bodies of other
components there yet). The linked curves left fixed have
several edges projecting onto them (a wall's top and bottom edges seen
along it), none (splines, edges of bodies not replayed), or a point the
snap put elsewhere.

## A revolution's touching regions, and a snap that keeps the sketch's regions (mitcad#227)

The example design's Revolve3 (a 360° cut of two thin regions that share
an edge: the wedge between a wall of a cylinder and a cone and the line
across both) fell back with 1077 mm³ cut where the file cuts 1050 mm³. The
ticket named the snap of linked curves (a line put on an edge 2 µm away,
splitting the profile); on main after mitcad#224/#225 that no longer
held: the solved sketch is the file's numbers to 1e-15 mm (the snap moved
nothing), its two regions are the file's two profile regions exactly
(1.454 and 4.846 mm²), and the cut failed on the file's own stored body
too. The cause was the revolution: each region revolved on its own gives
the file's volume (245.655 + 804.642 = 1050.297 mm³), but the two solids
fused and simplified (`BRepAlgoAPI_Fuse::SimplifyResult`) merged the two
parts of the cone face (the line the shared edge cut in two) into one
face built wrong (the fused solid alone was 1077.658 mm³, its centre of
mass off the axis). Regions that touch are now united as one planar face
before the turn (`geometry/src/revolve.cpp`; the straddling profiles did
so already), so no shared face is left for the fuse to merge; geometry
test `test_touching_regions`.

The snap (`snap.rs`) now follows the ticket's rules:

- **One decision per curve.** The projection rule's choice of edge
  (`linked::choices`: the distinct edges whose projection the curve lies
  on, those in the sketch's plane first, the curve follows the only one)
  is made once; the snap puts the curve on that edge's projection first,
  then on any other fitting edge whose projection keeps the points
  already placed (two edges' projections may differ by nanometres), and
  the projection follows the same edge.
- **The regions stay the file's.** A curve is moved only when the curves
  it meets, and how (a crossing inside both, an end of one on the other),
  stay those of the file's numbers, for the curve and for every curve the
  move touches: points the file holds coincident move along, points held
  on the curve go with it (the solver takes them), corners two curves
  share are the solver's and not counted. A curve whose contacts would
  change stays where the file stored it; the note says "n left where the
  file stored them: a snap onto the model's edge would change the
  sketch's regions" and the trace names the contacts. (Checking every
  curve a point of which had moved in any earlier snap, instead of the
  curves this move touches, cost 0.5 s on a 166-curve sketch; scoped, it
  costs nothing measurable.)

The example design (150 items, 78 of them occurrences and one a component
insert that place components without geometry of their own):

| Group | Target | Achieved |
| --- | --- | --- |
| Occurrences | placed as in the file | 78 / 78 (the final bodies the file's, volumes within 0.002 %) |
| Modelling items (71) | parametric | 68 / 71 (before 67 / 71) |
| their inputs | read from the bytes | 54 / 54 (0 inferred, 0 unknown; before 53) |
| Sketches (14) | imported, on the right plane | 14 / 14 |
| linked curves as projections | | 525 / 537 (12 kept fixed: several edges project onto them) |
| Suppressed in the file (4) | definition imported as suppressed | 2 / 4 |

Left, each with its cause:

- **Fillet9** (fallback, also on the file's own stored state): a 0.9 mm
  rounding of the 135° edge between Chamfer3 (0.5 mm) and a cylindrical
  band beside it only 0.300 mm high (its seam; C-Pattern2's slots cut it).
  The rounding needs 0.373 mm of the band (r·tan 22.5°), so the ball must
  run over the band's far edge onto the blend face beyond; OCCT's rolling
  ball with the port's overflow patches fails ("the radius may be too
  large"); 0.7 mm builds. The overflow class of mitcad#121 (a narrow band
  bounded by a blend face and cut by pattern slots), not this ticket.
- **Extrude4, Extrude5** (suppressed in the file, placeholders): the
  decoder gives Extrude4's two-sided extent only as guesses (its extent
  records are not decoded), and a suppressed item has no history state to
  check guesses against; Extrude5 cuts the body Extrude4 would make.
  Decoding the two-sided extent records would bring both in.

The 37 designs whose earlier reports had an extrusion or revolution not
parametric (the designs the snap could touch), imported with main's
import and with this one, item outcomes and the sketches' notes compared:

| Group | Target | Achieved |
| --- | --- | --- |
| Sketches whose regions are the file's | all | no sketch's outcome or regions changed (502 sketches; 0 curves left where the file stored them) |
| Curves snapped onto the model's edges | as before or more | 2162 / 2159 before (one design 42 / 49: edges the projection rule prefers place a shared point elsewhere, its projections 150 / 149) |
| Linked curves as projections | as before or more | 3858 / 3857 |
| Items parametric | as before or more | 1575 / 1574 (Revolve3 of the example design's other copy; nothing lost) |

## Parts of items, face profiles and sketch heights

Three readings of mitcad#169 (a design whose base body is moved into a
component of its own after a lid's first body was extruded from a sketch
on it):

- **Parts of items in other components.** A feature that changes the
  bodies of several components (screw holes cut through a lid and the
  base under it) is one timeline item, owned by one component, and one
  feature more of the item's class in the feature list of each other
  component it changes, with its own inputs (the same profile, that
  component's bodies), its own item tail and its own operation in that
  component's history, which no timeline item names. The design's table
  of item parts (class `12D00AC0`, `classes::ITEM_PARTS`) lists each such
  item with its features. The decoder gives every part with a result as
  an item of its own right after its item, numbered after the timeline's
  items, named after the item, with `_f3d.part_of` (the item's index) and
  its own `component` and `result_no`; the import replays it in its
  component like any item, at its item's point of the timeline
  (`Captures::timeline_at`: placements, stops). The item itself, when its
  own component's bodies did not change (result number -1), is skipped
  with a note naming its parts. Before, the parts' operations were states
  no item names, which a base feature brought the replay through before
  the next item of that component. Corpora: every off-timeline extrusion
  with a result of its own is such a part (`mitcad-f3d-inspect
  off-timeline`): `MITCAD_F3D_CORPUS` 5 extrusions in 2 designs, the
  backup corpus 69 in 11 designs, of which 12 (7 designs) are parts of
  extrusions and given as items, and 57 (4 printed-circuit designs) parts
  of a PCB item, which has no translation; besides, a fillet, a chamfer
  and a revolution are given as parts too, and the threads of holes
  (parts of another class) are not.
- **Faces in an extrusion's profile.** The profile slot (role 65) can
  select planar faces of bodies besides or instead of sketch profiles; the
  decoder gives each as a face reference among the extrusion's
  `profile`s (it takes one of the selected profile entries), found in the
  history by its names like the face of an extent. Faces together with
  sketch profiles are projected into the extrusion's sketch (linked; a
  face of another component through the occurrence placing it once,
  which needs the face found in that component's history, mitcad#161),
  and the sketch's regions on them join the selected regions; only into a
  sketch no other feature uses yet, since the projections split its
  regions. Faces alone go into one sketch on the first face, before the
  faces guessed by their areas. Corpora: `MITCAD_F3D_CORPUS` 28
  extrusions of faces only and 3 with sketch profiles too, the backup
  corpus 87 and 10. A face is looked for on the bodies the extrusion joins
  or cuts (its decoded participants) first, then on all: where another
  body has a face through the same point (a body lying against the
  extrusion's own), the bodies' order chose either, and half of a cut's
  ten profile faces went to the other body (mitcad#325: two cuts of one
  design and its copies).
- **A part's profile faces that its item took away** (mitcad#1347). The
  file makes a feature's tool once, before any of its operations; the
  replay makes the item's own operation first, and a cut from a face of
  the item's own body (an extrusion of that face into the body) takes the
  face away before the part in another component is replayed. Where a
  part's profile faces are not on the replay's bodies, the part takes
  them where its item's operation stands
  (`Importer::part_face_sketch`): with the timeline marker before the
  item's last feature, the faces are found as the profile names them, a
  sketch on the first one with their boundaries projected (linked; the
  plane and the projections through the occurrence placing the face's
  component, once) is made there and stays as one of the part's
  features, and the part's extrusions of its regions come after the
  item's operation as before. The faces are read where each is the only
  face that fits there. In one design of the backup corpus a cut of
  another component goes on 0.0043 mm into a plate (its part): the part
  was skipped as within the geometric tolerance, and the plate lacked a
  step whose edge ends a later rounding's tangent chain; with the part
  parametric that rounding's edges are read and the rounding of the
  step's edges after it builds on the replay's body.
- **Sketches lying off their frame.** A sketch on a face can keep its
  points at the face's height in its own coordinates while its stored
  frame lies in a parallel plane through the origin (in the design
  above, 1.6 mm below the base's top face, so the lid's first body came
  out 1.6 mm too low). Where more than half of a sketch's points share
  one height off the frame, the frame moves there (`plane_height`). Four
  files of the corpora have such a sketch, all the same design's.

Coverage, the 56 files of both corpora these touch, before and after
(`--time-limit 300`; one large design left out): parametric 2734 → 2777,
fallbacks 647 → 626, history matches 1642 → 1685, base features that
bring the bodies to a state 89 → 81, no item that came in parametric
before falls back. One part falls back where its operation was a state
no item named before ("does not cut into any participant body"). The
design above: its two parts parametric and the base feature before the
base's second fillet gone; the lid's first body still falls back until
the face its profile selects, on the root's body, is found in another
component's history (mitcad#161); with that lookup it comes in
parametric (no fallback left, 66 items parametric, 38 history matches).

## Joins that merge bodies, tapers that close, symmetric sketch patterns

Fixes from re-importing designs the last census left non-parametric. Over
the 37 re-imports of the designs these touch: parametric items 1522 →
1713, fallbacks 304 → 121, partial items 13 → 5; 25 imports improved and
none got worse (the final bodies of all still agree with the file's).

- **Bodies a join merged stay in the history.** A join that merges bodies
  into another (an extrusion, revolution or sweep joining several
  participant bodies) leaves the merged bodies in the ASM history's state
  as they were, as a combine's consumed tools do (above): the same stored
  body id, the same solid, though the design no longer has them (the API
  reference dumps of such designs list one body less after the join). The
  candidate's result is matched against the state without such bodies too:
  those of the state before it that are unchanged and that no body of the
  candidate's result is (`history::without_merged`). A match retires them
  from that state on (`Oracle::retire`), so that the later items and the
  final comparison do not expect them; the merged volume must be in the
  result body, so a candidate that did not merge them does not match this
  way. Before, such an item fell back to the stored bodies, and the items
  after it that built on them often with it: one design 112 → 173
  parametric items (77 → 16 fallbacks), another 92 → 127, a third 104 → 106.
- **A face profile keeps its taper.** An extrusion of a body's faces
  (*Parts of items, face profiles and sketch heights* above) gave the
  sketch of the face's projected boundary an extent without the file's
  taper; the decoded faces' candidate now takes the taper too (the
  candidates found by the faces' areas are for untapered prisms, and stay
  so). Its provenance is `read` where the stream's extent kinds say the
  distance goes along the sketch's normal.
- **Tapers that close the profile** (geometry kernel, `tapered`): the
  stored designs end a tapered extrusion where its sides meet when that is
  before the end of its distance, instead of failing: a hexagonal socket
  becomes a pyramid, a round pocket a cone, and the volume agrees with the
  API reference dumps (the pyramid's `A h / 3`). The sides of a
  convex profile of lines are intersected as half-spaces (a pyramid, or a
  roof with a ridge for a rectangle), a circle closes into a cone; the far
  cap is gone. OCCT's draft fails on the first and loops on the second. 13
  imports of 3 models (one keeps a thread that falls back for another cause)
  went from a fallback to a parametric extrusion; a profile with holes or
  arcs among its lines that a taper closes stays an error.
- **Three points on a line.** A collinear constraint whose entities are
  three points (a centre line's ends and the origin it passes) is a
  coincidence of the third point with the sketch's line through the other
  two. The API reference dumps cannot read such a constraint (an
  assertion on the geometry count), six have it, all the same sketch
  shape; 4 imports' sketches came in whole.
- **Symmetric rectangular patterns.** The stream's rectangular pattern
  flags are (distance kind, symmetric in direction one, symmetric in
  direction two): all 41 patterns of the API reference dumps agree with
  `distanceType`, `isSymmetricInDirectionOne` and `…Two` (two are symmetric
  in direction one, with 5 and 11 instances; none is in direction two). A
  symmetric direction has the originals as its middle instance, so the
  copies lie on both sides: `patterns[].symmetric` in the sketch (model,
  [commands.md](../../model-api/src/commands.md), *sketch*), the copies'
  indices counting from the end against the direction. An even count
  stays an error. 3 imports' sketches came in whole.
- **An offset without curves** (the stored list of its offset curves is
  empty: the curves were deleted) binds nothing and no longer makes its
  sketch partial (1 import).

## Joints

`joints.rs` translates what the decoder reads of joints, as-built joints,
joint origins, rigid groups and ground items (mitcad#55, mitcad#81,
SCHEMA.md §5.3) into MitCad's joint features (`commands.md`, *Joints*),
and `captures.rs` the captured positions (mitcad#75). Their occurrence
paths (object ids from the joint's component down) become paths of the
occurrences the tree made, those of components of other documents
included (fasteners, inserted parts: empty components, *Components*); a
joint of a level inside a component of another document is left out with
that reason.

**Where the file places the occurrences.** The transform the file stores
for an occurrence is where the item that made it put it; joints, captured
positions and rigid groups after it move it without changing it. The
decoder reads each occurrence's placements after every item that placed
it (`_f3d.placements`, mitcad#81): the occurrences start at their last
ones (else at their last captured positions, else at their stored
transforms), and after each joint item every occurrence must be where it
was before the item, where the file places it after the item, or where it
ends (1e-6 of the size, 1e-7 in rotation), else the definition is taken
back and the next one tried.

- **Joints.** Side one is origin `a` (it moves onto side two, as `a` does
  onto `b`), side two `b`. The file's relation is MitCad's `frame_a =
  frame_b · motion(values) · Rz(angle) · Tz(offset) · flip` with the
  angle and z offset parameters as they are, `flip` the file's flip (its
  `opposed` byte: the frames' z axes opposed), and the x and y offsets
  moving `b`'s origin within its plane (numbers). The joint test model
  (the reference model `joint_kinds` under `MITCAD_F3D_MODELS`: every kind at known values, flipped and
  not, mitcad#81) settles the senses: MitCad's values are the file's
  (turns, slides along x, y or z by the motion's slot, a cylindrical
  joint's turn and slide, a pin-slot's turn and slide along x, a planar
  joint's x and y slides and turn); a ball joint's file values are
  `Rz(pitch) · Rx(yaw) · Rz(roll)`, MitCad's `Rz · Ry · Rx`, so its
  values differ while the placements agree.
- **Checked before it is added** (mitcad#87). The values the motions have
  where the file places the occurrences (at the marker, else right after
  the item) come from the relation (`mitcad_model::joints::motion_values`)
  with the sides' frames; where it does not hold there, no definition is
  added (the note says how far side one is from side two) and the joint is
  kept as an as-built joint at once. Where it holds, one definition is
  added: with the file's limits (rotation limits bound `rz`, slide limits
  the slide; planar and ball limits are left out, which motion they bound
  is not decoded) when they hold the values, with the values as the
  joint's `position` where the rest value is another; else without the
  limits (`its limits left out (outside them: rz is … there)`). A joint
  item takes one recompute instead of one per definition tried and its
  undo.
- **Offsets the file stores rounded** (the owner's decision in mitcad#81):
  where the relation holds but for the offset along the joint's z, by at
  most half a unit of the last digit of the offset parameter's expression
  (`-2.646 mm`: 0.0005 mm; expressions with decimals only), the parameter
  keeps its name and takes the offset the placements give (`its offset …
  from the placements`). The joints of the corpus that seemed rounded
  hold with the stored offset at the placements after them (the stored
  transforms are older): none needs it.
- **Sides.** The frames are the file's stored ones, in the components the
  paths end in. A side goes on geometry of that component at the joint's
  point of the timeline that gives the frame, with a `frame_override` for
  the parts it does not give (a planar face's origin is its middle, so a
  key point elsewhere on it is an override): a joint origin the side
  names, construction geometry its key point or direction names (the
  component's origin point, planes and axes, imported construction
  features), circular edges centred on the frame's origin along its z
  axis, planar faces through it across z, faces of revolution about z
  (edges first when the key point is on an edge; at most eight). The
  faces and edges come from one pass over each body
  (`Kernel::face_geometries`, `edge_geometries`, mitcad#87) instead of
  finding each by its name. Each is checked by resolving it as the joint
  will (`joint_frame`). Else the side is a fixed plane with the frame, and
  the joint is `partial`. A side on a component of another document (a
  fastener's joint origin) goes on the origin of the empty component
  standing for it, with a `frame_override` where the file's frame there is
  not the identity.
- **Kept as as-built joints.** A joint that does not hold becomes an
  as-built joint of its kind at the placements at its point of the
  timeline (`relative` from them; the motion frame side two's), else a
  rigid one, with a warning and the reason; the item is `partial`. A
  joint whose motion is not decoded is left out.
- **As-built joints.** `relative` from the placements at its point of the
  timeline. The file records the joint's frame in each occurrence's
  component; where those frames disagree with the placements a warning
  says so (the placements are kept). Kinds with motions take the recorded
  frame as their motion frame, on geometry of occurrence two's component
  as for a joint's sides, else of occurrence one's, else fixed on
  occurrence two; their limits where the recorded placement is the
  file's (MitCad's values are 0 there) and the limits hold 0, with 0 as
  the position where the rest value is another.
- **Rigid groups** (mitcad#81; the file stores them as as-built joints of
  motion type 11). A `rigid_group` of the occurrences of the group's
  component its members are or are in: members below the top level
  ("include children" lists the members' children too) and inside
  components of other documents move with the occurrence they are in. A
  group with the component's own geometry (an empty path) joins its
  occurrences to it by rigid as-built joints.
- **Joint origins.** A `joint_origin` of the owning component with the
  file's angle and z offset parameters on geometry that gives its frame
  before them (the component's origin point for the designs read, a face
  or edge as above, else a fixed plane), checked against the file's frame
  (`datums`); else the file's frame fixed without the offsets (`partial`).
  Later items can use it as a joint's side.
- **Ground items.** The occurrence's ground flag (the decoder also sets it
  in the occurrence tree); the item is `parametric` without a feature.
- **Captured positions.** A `capture_position` feature in each component
  whose occurrences the item places, each occurrence at its placement in
  its parent: the file gives the placement of each path in the item's
  component, made relative to the parent path's placement (the same
  item's, else the last captured or stored one). A captured position of
  joints' values stores parameters instead; its positions are those the
  occurrences' placements name at its index (so are those of positions
  with an empty path). A path level the decoder
  does not find (some designs name a sub-assembly's occurrence by an id no
  occurrence has) is the one occurrence placing the component the next
  level sits in; its id then stands for that occurrence in the item's
  other paths. Positions inside components of other documents and of
  grounded occurrences (MitCad does not move those) are left out with a
  note (`partial`). The features are not checked against the end
  placements: they are the file's placements at their points.

The report counts them (`joints`: as joints, their sides on fixed frames
and on components of other documents, kept as as-built joints, as-built
joints, joint origins, grounded, captured positions, rigid groups, left
out). With `MITCAD_IMPORT_TRACE` each joint item traces its time (`a
joint in 0.005 s (sides 0.004 s, 1 definitions added)`).

In the corpus (10 designs with joint items or captured positions; every
joint is rigid, nearly every one joins a fastener or another part
inserted from another document):

| | Joints | As-built joints | Rigid groups | Captured positions | Joint origins |
|---|---|---|---|---|---|
| mitcad#81 | 9: 8 in (6 sides on fasteners and inserted parts), 1 inside another document | 3: all in | 10: 7 in, 3 whose other members are inside another document | 15: 6 in, 8 in without positions inside other documents, 1 left out | 6: all in |
| Before | 9: 5 in, 3 kept as as-built joints, 1 inside another document | 13 (the rigid groups among them, as rigid as-built joints of their first two members): 10 in, 3 inside another document | not decoded | 15: 4 in, 8 in without positions inside other documents, 3 left out | 6: all in |

The joints kept before as as-built joints held where the file places the
occurrences after them: two seemed 4.5e-4 mm off (a rounded offset), one
fastener 12 mm off, against the transforms the file stores, which are
older than the joints. In the corpus a joint item now takes at most 0.02
s (0.05 to 0.18 s before); on a large design (199 timeline items, a body
of 1239 faces and 2981 edges) with twelve rigid joints onto its own
geometry added to it, the first that holds took 0.22 s (reading the
component's faces and edges), the others that hold 3 to 5 ms and those
that do not 1 to 3 ms (2.3 to 3.0 s each before: each definition tried
added, recomputed and taken back, and every face and edge found by its
name).

## Assembly relationships

`relationships.rs` (mitcad#155) brings the file's assembly relationships
in as MitCad's constraint sets (`commands.md`, *Assembly constraints*):
what the decoder reads of them (`core/f3d-design/src/build/relationships.rs`,
SCHEMA.md §5.3 `GeometricRelationship`, `TangentRelationship`).

**What the file stores** *(read from the corpora's relationship items: the
occurrence placements the file stores after each item satisfy the decoded
rows)*. A relationship item holds one or more constraints, each a row: the
two sides' occurrences (by their placements), the faces or edges picked
on each side, the points picked on them, an `Offset` or `Angle`
parameter, and a constraint whose data gives the kind of each picked
entity (a point, an axis, a plane or a circle) with its frame in the
component the side's path ends in, and a sense (the frames' z axes the
same way, opposed, or either). Older files keep the same data in the
constraint itself (its class versions 3 to 6) with type codes one
higher. Where the file places the occurrences, two planes hold `(o2 −
o1)·z1 = offset` (side two's plane that far along side one's normal), a
point or an axis on a plane `(o1 − o2)·z2 = offset`, two axes are
collinear, two circles share their axis with their planes `offset` apart,
a centring's two pairs of faces share their middle plane, and an angle's
axes are at its angle. Of the 282 rows of the corpora (75 in the 37 items
of `MITCAD_F3D_CORPUS`, 207 in 109 items of a second corpus, a backup of
designs), 183 have both sides in the occurrence tree: 125 hold where the
file places the occurrences right after their item, 24 after a later
item that moved them, 34 never (in four large assemblies whose parts
later items moved or replaced). The other 99 have a side inside a
component of another document or on an occurrence that is not in the
occurrence tree. A frame stored as none is the identity (a circle
centred on its component's origin holds so).

**Translation.** One `constraint_set` per item in its component, a row per
constraint (side two `a`, side one `b`; a point or an axis on a plane is
`a`, the plane `b`): two planes or a plane and a circle `align`, a point
or an axis on a plane `align` with the offset above it, two axes (or an
axis and a circle) `align` (concentric), two circles `insert`, a
centring `center` (`a`, `a2` side one's faces, `b`, `b2` side two's), an
angle `angle`. Each frame goes on geometry of the side's component at the
item's point of the timeline that gives its plane, axis, circle or point
(planar faces through its origin across z, cylindrical and conical faces
about z, circular edges centred on it, spheres centred on it; the one
nearest the point picked on it first), else on fixed geometry with the
frame (the item `partial`). A side on a component of another document
(an empty component here) is fixed geometry of it with the file's frame;
a circle there has no edge to go on and the row is left out, as is a
side inside a component of another document. `flip` and the sign of the
offset come from the references as MitCad resolves them where the file
places the occurrences (the `constraint_row` query's measures): planes,
circles and axes must be parallel there (within 1e-5 rad) and the offset
the parameter or its negative (within 1e-4 mm); the offset stays the
file's parameter.

**Checked when added.** The set is added; every occurrence must stay
where it was, where the file places it after the item, or where it ends
(within 1e-4 mm and 1e-5 in rotation: the file's placements and frames
agree to a few 1e-5 mm), else the rows are added one by one and those
that move an occurrence, or that the joint solver cannot hold together
with the earlier ones, are left out with the reason. The report counts
the sets, rows, rows translated, their sides on fixed geometry and on
components of other documents, the tangent relationships and the rows
left out by reason (`relationships`).

**Tangent relationships** (the decoder's `TangentRelationship`, a contact
between two bodies). A `tangent` row between the faces of the sides'
components that the file's points lie on, where they are planar,
cylindrical or spherical (MitCad's tangent rows take those), `flip`
tried for a contact inside; else left out with the reason. The three of
the corpora pick a plane and a cylinder: in one design they touch where
the file places the occurrences right after the item (gap 0), in the
other (two versions) they overlap by 0.36 mm there; later items move the
parts apart (31 mm and 6.6 mm at the end), so none holds where the file
places them at the end and all three are left out with those gaps in
the note.

In the corpora (every relationship file, `--time-limit 60`):

| | Items | Rows | As rows | Left out: inside another document | does not hold at the end placements | occurrence not in the tree | circle on another document | the solver cannot hold with the others |
|---|---|---|---|---|---|---|---|---|
| `MITCAD_F3D_CORPUS` | 37 relationships (15 sets: 11 parametric, 4 partial), 1 tangent | 75 | 24 (29 sides on components of other documents) | 30 | 21 | 0 | 0 | 0 |
| Backup corpus | 109 relationships (53 sets: 35 parametric, 18 partial), 2 tangents | 207 | 74 (100 sides on components of other documents, 2 on fixed geometry) | 52 | 51 | 17 | 5 | 8 |

Before mitcad#155 every relationship item was skipped. The rows the
solver cannot hold with the others fix one rotation twice where the
file's frames differ by a few 1e-7 rad (the solver's tolerance is 1e-10
of the size).

## Items without a translation

The decoder names timeline items whose own data it does not read by
their class (`core/f3d-decode/src/classes.rs`, `ITEM_TYPES` and
`ITEM_CLASSES`), and the report says what each does:

- **No geometry**, skipped at once (they never wait for a fallback):
  occurrences (`Occurrence`, the item of a new or placed occurrence,
  named after its component), components inserted from other designs
  (`ComponentInsert`, `Fastener`), pasted and derived occurrences
  (`CopyPasteOccurrence`, `DerivedInstance`), patterns of occurrences
  (`RectangularOccurrencePattern`, `CircularOccurrencePattern`: their
  copies are occurrences of the tree, each placed by its stored transform;
  MitCad has no pattern of occurrences, so they do not follow the
  pattern's quantities and distances), motion links and contact sets
  (assembly relationships and tangent relationships come in as
  constraint sets: *Assembly relationships*), assembly contexts
  (`Context`),
  canvases and timeline groups (`Group`, named by their own name; their
  items follow them).
- **Mesh bodies** (`MeshFeature`, mitcad#1366): the item comes in as a
  base feature of its mesh bodies, the triangles of their containers
  (`ParaMeshGeometry.<uuid>.paramesh`) placed by their records, in its
  component where it stands on the timeline (suppressed when the item is
  suppressed in the file); the meshes no item brings in come in at the
  end, a base feature per component, and one that cannot be read is a
  warning ([*The published format descriptions*](f3d-undecoded.md#the-published-format-descriptions-mitcad154)).
  A mesh body gets the physical material and appearance its record
  names (mitcad#1376, [*Mesh bodies*](f3d-appearances.md#mesh-bodies-mitcad1376));
  the import of the stored bodies alone (`import_f3d` without the
  timeline) brings the mesh bodies in too, a base feature each.
- **Changes bodies, not translated**: the history states stand in for
  them (fallback), with the reason in the report. Sheet metal flanges
  (`FlangeFeature`) the decoder does not read ([*Sheet metal*](f3d-sheet-metal.md#sheet-metal) below),
  electronics boards
  (`PCBFeature`), whose inputs are not decoded; components mirrored into new ones (`MirrorComponent`).
  Drafts (`DraftFeature`, mitcad#1363) come in as MitCad's `draft`, one
  per body of their drafted faces: the decoder reads the drafted faces
  (the input slots of role 16), the neutral plane (the slot of role 33:
  a face, or an origin or construction plane), the angle (the parameter
  of role `Angle1`), the second angle (`Angle2`) and the tangent-chain
  flag (SCHEMA.md). The file stores no flip: it pulls along the neutral
  face's outward normal and along a plane's normal, so a face draft is
  MitCad's with `flip` (the reference model of a face drafted 5 degrees
  about the bottom face grows by the wedge, 49 049.86 mm³, as MitCad's
  flipped draft does) and a plane draft is MitCad's without it; the other
  way is a guess the history checks after it. Not translated: a zero
  angle, a neutral group of several faces, and a parting-line draft whose
  parting tool is more than one face (MitCad's draft turns faces about
  one plane; parting-line drafts are not modelled). A parting-line draft
  about one parting face, and a draft with a second angle (whether it is
  two-sided or symmetric is not decoded), come as guesses only the
  history can confirm. The backup corpus' one draft (`fc1b7b7e` item 18)
  is read in full and agrees with the file's own dump of it (the API
  reference: the face, the angle, the tangent chain on, not flipped): a
  cylinder 6 mm across and 1.5 mm long drafted -45 degrees about its end
  face, whose stored state (24.74 mm³) is the frustum that MitCad's
  flipped draft of -45 degrees makes.
  Moved faces (`MoveFaceFeature`, mitcad#343) come in as MitCad's
  `move_face`: the decoder reads their faces (named faces, found in the
  history at the item's place) and their motion (the rigid matrix of their
  transform input, as a move's), one `move_face` per body with a free
  move's matrix.
  Embosses (`EmbossFeature`, mitcad#458) come in as MitCad's `emboss`:
  the decoder reads their profiles (their loops, as an extrusion's), the
  face and the parameters by role (`Depth`, negative for a sunk emboss;
  `Horizontal Distance`, `Vertical Distance` and `Rotation Angle`, the
  profiles moved on the face); the import maps the profiles as an
  extrusion's, the face by its point and names, and refuses profiles
  moved or turned on the face (no MitCad counterpart). Both embosses of
  the backup corpus (`547b68ad` item 29, `d73c1f69` item 15, planar,
  sunk) are parametric and verified against the history.
  Bodies derived from other designs (`DerivedContext`) come in as MitCad's
  derives where the documents they come from are found (*Derived designs*
  below), else the history states stand in. Bodies derived from another
  design by the derive's base class (`ExternalDerived`, mitcad#641: its
  record is not `DerivedContext`'s) are static in the source too, as the
  design they follow is not in the file: the history state of their base
  feature stands in, with that reason. Copies and cuts of bodies
  (`CopyPasteBodies`) come in as MitCad's except a cut whose body the file
  lost. Bodies aligned by geometry (`AlignFeature`, mitcad#641) come in as
  moves by the motion they made ([*Grid-unit designs*](f3d-history.md#grid-unit-designs-pastes-aligns-counts-and-start-faces-mitcad641)). Bodies moved into a new component
  (`ComponentFromBodies`) come in as MitCad's where the history settles
  them (*Components*), else so.
- Lofts of another class are `LoftFeature`s, translated (*Sweeps, pipes
  and lofts*); points of another class are `ConstructionPoint`s, skipped
  without decoded geometry.

Items of classes still unknown keep the type `?<default name>`
("unknown timeline item").


## A cut pasted into its own component (mitcad#1175)

A cut and paste of bodies (`CutPasteBodies`) whose bodies are the
destination component's already moves nothing: their features went with
an earlier cut. The file may also keep such an item as a copy: in a
reported drill index the blank was cut and pasted into the component it
came from, and the item's own history state holds the blank and a copy
of it, in which the later holes are cut (the blank hidden). The
translation (`ops::cut_paste_command`) asks the item's state
(`Importer::state_is_current`): where it is not the bodies as they are,
the item is a copy in place (a `move` that copies with the identity),
which the usual route checks against the state; otherwise it moves
nothing, as before. Before, the copy was missing, a base feature brought
the file's state before the first hole, and every hole cut that base
feature's body.

A linked sketch curve that then follows the coincident copies of one
edge in two bodies (`linked::prefer_own_body`) follows the copy in the
body whose face the sketch lies on: each projects the same curve, and
without the choice the curve stayed fixed geometry.
