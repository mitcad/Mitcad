# The design dump schema (version 2)

The design dump is MitCad's JSON form of an `.f3d` design: parameters,
components, occurrences and timeline, with each item's definition. It is
the intermediate representation the `.f3d` import replays
(`mitcad_dump::Dump`, [README.md](README.md)). `schema` is
`"mitcad-f3d-dump"`, `schema_version` is `2`; an incompatible change
increments `schema_version` (version 1 differences: §7).

Notation: `T?` — key may be absent; `T | null` — key present, value may be
null. `vec3` is `[x, y, z]`, `vec2` is `[x, y]`, `mat4` a 4x4 nested array.

**[assumed]** marks a meaning not yet confirmed: the reading MitCad uses,
with the reference-model experiment that will settle it (`K<n>`).

## 1. General rules

### Units

All numbers are the file's internal values, never display units:

| Quantity | Unit |
|---|---|
| length, coordinates | cm |
| angle | rad |
| area | cm² |
| volume | cm³ |
| mass | kg |
| density | kg/cm³ |
| moment of inertia | kg·cm² |

The dump repeats this table in `units`. Parameters also carry the user's
`expression` (as typed, e.g. `"( 13 / 3 ) * 1 mm"`) and `unit` (as stored,
e.g. `"mm"`, `"deg"`, `""` for unitless); `value` is still internal (a
`"30 deg"` parameter has `value` 0.5235987…).

Numbers are IEEE doubles in shortest round-trip form (they parse back
bit-exactly). Non-finite values are written as `null`.

### Matrices and coordinate systems

- `mat4` is row-major: `[r][c]` is row `r`, column `c`. Points are column
  vectors (`p' = M · p`); translation is `[0][3]`, `[1][3]`, `[2][3]`; the
  last row is `[0, 0, 0, 1]` for rigid transforms.
- Sketch points and curves are in **sketch space**. The placement is given
  three ways (§5.3, *Sketch detail*):
  - `transform`: the stored transform. Its direction (sketch → model or
    the inverse) is **not** settled, so the import does not rely on it
    (`model_frame.transform_matches` answers it per sketch, K1).
  - `origin`, `xDirection`, `yDirection`: origin and axes in model space.
  - `model_frame`: the sketch-space points (0,0,0), (1,0,0), (0,1,0),
    (0,0,1) mapped to model space, and `sketch_to_model` built from them
    (model = M · sketch). This is the unambiguous mapping; use it. For
    root-component sketches model space is world space; for another
    component's sketch it is that component's space [assumed].
- B-rep fingerprints are in the space of the entity as reached: component
  space for a component's entity, root (world) space for an entity reached
  through an occurrence (a proxy). A proxy has a non-null `occurrence`
  (its `fullPathName`).
- Body snapshots (`steps`, `final`) list the root component's bodies, then
  each occurrence's bodies as proxies, so all their geometry is in world
  coordinates. A component with several occurrences appears once per
  occurrence.
- `occurrences[].transform2` places the full-path occurrence in the root
  (world) system [assumed, K71]; `transform` is an older form of the same
  placement; `initialTransform` is the occurrence's initial position.

### Identifiers

| Identifier | Meaning | Stability |
|---|---|---|
| timeline index | 0-based position in the flattened timeline | within a document version |
| marker position | number of items before the timeline marker (0 = start) | within a dump |
| sketch-local ids | `p<i>` points, `c<i>` curves, `t<i>` texts, `k<i>` constraints, `d<i>` dimensions: positions in the sketch's lists | within one dump |
| profile index | position in the sketch's profiles | within one dump |
| `entityToken` | opaque token of the entity | a hint only; can change over time |
| `tempId` | a face's or edge's temporary id | within a body, for one writer's run |
| component `id` | the component's id | stable |
| data file `id` | the stored design's id (a lineage URN) | stable |

`name` is the display name of timeline items, bodies, sketches and
components; usually but not necessarily unique.

### Absent keys and errors

- A value the writer does not know is omitted.
- A value whose reading failed is reported in its place, either as
  `{"_error": "Type: message"}` (feature fields, reflected properties) or
  in a sibling map `"_errors": {"<field>": "Type: message"}` (fixed-shape
  records: fingerprints, sketch entries, body snapshots).
- A failed whole section is listed in top-level `errors` and becomes
  `{"_error": "..."}`.

Readers must tolerate absent keys and `_error`/`_errors` everywhere.

### Value conversion

Feature fields (`detail`), reflected properties (`props`) and references
convert values as follows:

| Value | JSON |
|---|---|
| null, bool, int, string | as is |
| float | number (`null` if non-finite) |
| enumeration | member name (e.g. `"NewBodyFeatureOperation"`) when the writer knows the enumeration, else the int |
| 3D point or vector | `vec3` |
| 2D point or vector | `vec2` |
| 3D / 2D matrix | `mat4` / 3x3 |
| bounding box | `{"min": vec3, "max": vec3}` |
| model or user parameter | parameter reference (§3) |
| 3D or 2D curve geometry | curve geometry (§4) |
| surface geometry | surface geometry (§4) |
| entity (B-rep, sketch entity, profile, construction geometry, feature, component, occurrence ...) | reference (§3) |
| collection or list | array of converted items; at most `max_items` (dedicated feature fields: `max_detail_items`); a truncated array ends with `{"_truncated": <total count>}` |
| any other object | `{"_type": "<type name>", ...its properties...}` while depth allows, else the stub `{"_type": ..., "name"?: ...}` |

`"<type name>"` is the short type name without namespace
(`ExtrudeFeature`).

## 2. Writers

- **The stream decoder** (`mitcad_f3d::design`; `mitcad-f3d-inspect design
  --json` prints its dump) decodes an `.f3d` file's design streams.
  `source.mode` is `"f3d_stream"`. It writes parameters, components and
  occurrences, and the timeline with sketches and the main feature fields.
  Keys starting with `_f3d` hold its own data (stream format, class GUIDs,
  object ids, raw flags, body blobs, how much it decoded); other readers
  may ignore them. `generator.decoder` names it; `generator.writer` is the
  version of the application that wrote the file. Feature inputs that the
  streams name as the bodies name their faces (fillet and chamfer edges;
  moved, split, patterned and mirrored bodies; threads' faces; edges of
  sweeps' and pipes' paths and of lofts' sections and rails) are B-rep
  references with `_f3d: {object_id, recipe, entities, found,
  edge_points?}`: the names, and what the history state before the item
  gave for them. A body reference also has `producer` and `body_index`
  where its record names them: the timeline index of the item that made
  the body and the body's position among that item's bodies (mitcad#96);
  a body whose recipe does not decode has only those. A fillet's or
  chamfer's edge (or face) has them too, from the record of the body it is
  on, where it refers to one. An edge found there gets `mid_point`, `length`,
  `start_point` and `end_point` (and, from the tail of its recipe, `_f3d.end_faces`:
  per end vertex, the start first, the entities, indices into `_f3d.entities`, of the faces
  round it counter-clockwise seen from outside; core/f3d/EDGE_ORDER.md); a body `_f3d.edge_points` (middle points
  of some of its edges); a cylindrical face `geometry` (`Cylinder`:
  origin, axis as the file's surface has it, radius), `point_on_face` and
  `normal_at_point` (a whole cylinder also `_f3d.seam`: the unit direction
  of its surface's parameter origin, where a modelled thread's helix starts); a face of another surface type its `geometry`
  (`Plane`, `Cone`, `Sphere`, `Torus` with their data, `NurbsSurface` for
  any free-form surface, type only), `point_on_face` and
  `normal_at_point`, and a free-form one `_f3d.face_points` (more points
  inside it, spread over it, cm; mitcad#161); `found` says where, or why
  not ([docs/f3d.md](docs/f3d.md), *Limits*). Sweeps' guide surfaces are
  found too. An input found in another component's history than the
  item's has `_f3d.component` (that component's `_f3d.object_id`): its
  geometry is in that component's coordinates (mitcad#161). A reference
  to an entity of a component of another document has `_f3d.document`,
  the GUID of that document (lower case; the `document_guid` of one of
  `_f3d.external_documents`, the first of the two GUIDs of the reference
  object): the names are those of the other document's operations, so
  the entity is looked for in the last state of that document's history
  (the file given for it, `found` says "another document's last state"),
  its geometry is that document's own, and the components that stand for
  it have it (mitcad#196); without the file the design's own history is
  asked as for any input, and what it gives is a coincidence of names. An
  entity is looked for in the last state first, then in earlier ones
  (the other document's later operations may have renamed or changed it; a
  face or body found earlier is taken as the last state has it where it is
  still there). A document without history (its bodies stored only, such
  as an imported board or part) is asked in its stored bodies (`.smb`):
  each body names its entities from the operation that imported it, so the
  same names fit faces of several bodies; a face is the one with the most
  of the faces its recipe lists around it, and where several bodies tie it
  is not found (`found` says how many; mitcad#332). Documents that share a GUID (copies of one document keep
  it) are told apart by `_f3d.context`: the reference's slot carries the
  placement of the occurrence the entity was selected in, a row-major
  4 x 4 transform (cm; the key object of the slot, after the slot's inputs,
  holds 16 doubles), which is where one of the design's occurrences of
  the document is, at that item. The import takes the occurrence whose
  placement it is as the one the entity is in (`focus.rs`). A body found there also has `_f3d.stored`,
  `BREP.<guid>.smbh#<record>`: the blob and the record of the body in the
  last state, as that document's import names the stored bodies it matches
  its bodies to (`bodies[].file` of its report), which tells the body of
  the copy. The faces an extrusion's profile selects are such face
  references among its `profile`s (mitcad#169). A feature that carries
  out a timeline item in another component's feature list (a cut through
  the bodies of two components) comes as an item of its own right after
  its item, with an `index` after the timeline's items and
  `_f3d.part_of`, the item's index (mitcad#169). A construction plane's or
  point's stored definition code is `_f3d.datum_code` (planes: 0 offset
  or parallel through a point, 1 angle, 2 three points, 3 mid plane, 5 and
  10 tangent, 6 along a path, 7 two edges, 9 tangent at a point; points: 0
  at a vertex or point, 2 three planes, 3 centre of a circle; mitcad#186).
- **External dumps** come from other tools that read a design and write
  more than the decoder: feature inputs as B-rep fingerprints, bodies
  after each timeline item (`steps`), final bodies with faces (`final`),
  and exported files (`exports`). `generator` names the tool (`addin`,
  `addin_version`) and the application version it read the design with
  (`app_version`). The import replays such a dump instead of the file's
  streams with `mitcad-cli import-f3d --dump` or the `dump` option of
  `import_f3d` (the file still gives the bodies). The reference models'
  dumps are external dumps kept outside the repository
  (`MITCAD_F3D_MODELS`, [docs/development.md](../../docs/development.md)).

The stream decoder's `_f3d.appearance` (mitcad#228;
`mitcad_dump::AppearanceData`, `docs/f3d-appearances.md`, *Appearances and
materials*) holds the appearances and physical materials: `assets` (the
assets used, their textures and their materials' aspects: `id`, `schema`,
`name`, `library`, `ui_name`, `category`, `description`, `properties`
by id — colours as RGBA arrays, quantities as `{"unit", "value"}`,
texture paths as `{"kind", "paths"}` — and `connections`), `bodies`
(`entity`, `name`, `blob`, `selector`, `record`: the body's ASM record in
the blob; `appearance` and `material` as `{"token", "library", "preset",
"display", "asset"}`; `faces`: the faces with their own, each `guid`,
`appearance`, `color`, `faces` (fingerprints in the stored design, cm) or
`why`; `light_bulb`: false for a body the file hides, where its record
gives it, mitcad#1175), `faces` (face references whose face the stored
design does not have), `occurrences` (overrides: `object_id`, `component`, `appearance`)
and `warnings`. External dumps have none; the import then sets no
appearances.

Readers keep unknown keys in each record (a dump survives read and write
unchanged) and do not check `schema`.

## 3. References

Every reference is an object with a `kind`.

| kind | Keys |
|---|---|
| `parameter` | `name`, `expression`, `value`, `unit` |
| `profile` | `sketch` (name), `sketch_timeline_index`, `profile_index`, `loops` (`[{isOuter, curves: [sketch-local curve id \| null]}]`: the profile's loops in the writer's order with the sketch curve each profile curve lies on; identifies the region even when `profile_index` cannot be resolved), `area`, `centroid` (of the region, for cross-checking; given without `loops` they choose it: the region of that area and centroid, each of several alike tried, else a union of regions that has them) |
| `sketch_entity` | `objectType` (`SketchLine`, `SketchPoint`, ...), `sketch`, `sketch_timeline_index`, `id` (sketch-local id or null), `geometry?` (only if `id` is null) |
| `sketch_dimension`, `sketch_constraint` | as `sketch_entity`, `id` is `d<i>` / `k<i>` |
| `sketch` | `name`, `timeline_index`, `component` |
| `face`, `edge`, `vertex`, `body`, `loop`, ... | B-rep fingerprint (§4) |
| `construction_plane`, `construction_axis`, `construction_point` | `name`, `origin` (`"XY"`/`"XZ"`/`"YZ"`, `"X"`/`"Y"`/`"Z"`, `"Origin"` for the component's origin geometry, else null), `timeline_index` (null for origin geometry), `component`, `geometry` (plane: surface geometry; axis: `InfiniteLine3D` curve geometry; point: `vec3`) |
| `component` | `name`, `id` |
| `occurrence` | `name`, `fullPathName`, `component` |
| `feature` | `objectType`, `name`, `timeline_index` (any other timeline entity) |
| `document`, `appearance`, `material` | `name` (`id` for appearance/material) |
| `error` | `objectType`, `_error` (the reference itself failed) |

Inside a sketch dump, references to entities of the same sketch are plain
id strings (§5.3).

**Selections.** B-rep references in a feature's `detail` (its inputs:
fillet edges, shell faces, extent faces, planar entities ...) and a
sketch's `referencePlane` carry one level of topology (§4, *Topology*)
when the option `selection_topology` is true (default): faces get `loops`,
edges get `coedges`. References in `props` never do.

## 4. Geometry and fingerprints

### Curve geometry

```
{ "type": "Line3D" | "InfiniteLine3D" | "Arc3D" | "Circle3D" | "Ellipse3D" |
          "EllipticalArc3D" | "NurbsCurve3D" | "Line2D" | ... ,
  <the following keys when the type has them>
  "startPoint", "endPoint", "center", "origin": vec3,
  "direction", "normal", "referenceVector", "majorAxis": vec3,
  "radius", "majorRadius", "minorRadius": number,
  "startAngle", "endAngle": number (rad),
  "nurbs"?: { degree, isRational, isPeriodic, controlPointCount,
              knots: [number], controlPoints: [vec3], weights: [number],
              "_truncated"? } }
```

An arc runs counter-clockwise about `normal` from `startAngle` to
`endAngle`, measured from `referenceVector`. `weights` is empty for
non-rational curves.

### Surface geometry

```
{ "type": "Plane" | "Cylinder" | "Cone" | "Sphere" | "Torus" |
          "EllipticalCylinder" | "EllipticalCone" | "NurbsSurface",
  "origin": vec3, "normal", "uDirection", "vDirection", "axis",
  "majorAxis", "majorAxisDirection": vec3,
  "radius", "majorRadius", "minorRadius", "halfAngle": number,
  NurbsSurface only: degreeU, degreeV, controlPointCountU, controlPointCountV,
                     knotCountU, knotCountV, propertiesU, propertiesV }
```

Only the type's keys are present (Plane: origin, normal, uDirection,
vDirection; Cylinder: origin, axis, radius; Cone: origin, axis, radius,
halfAngle; Sphere: origin, radius; Torus: origin, axis, majorRadius,
minorRadius).

### B-rep fingerprint

Common keys: `kind` (`face`, `edge`, `vertex`, `body`, `loop`, `coedge`,
`lump`, `shell`), `objectType`, `entityToken?`, `tempId?`, `body?` (body
name; absent for bodies), `component`, `occurrence` (null for a
component's entities), `_errors?`.

| kind | Additional keys |
|---|---|
| face | `geometry` (surface geometry), `area`, `isParamReversed`, `point_on_face` (a point inside the face), `normal_at_point` (the normal there, with the face's orientation), `centroid?`, `bbox`, `edge_count`, `loop_count` |
| edge | `geometry` (curve geometry, NURBS limited to 200 control points), `length`, `start_point`, `end_point` (vertex positions; null for closed edges without vertices), `mid_point` (at the middle parameter), `isDegenerate`, `bbox`, `face_count` |
| vertex | `point`, `edge_count` |
| body | `name`, `isSolid`, `volume`, `area`, `bbox`, `face_count`, `edge_count`, `vertex_count` |
| loop | `isOuter`, `edge_count`, `face` (face fingerprint) |

`bbox` is not guaranteed to be tight.

### Topology

Where stated (selections §3, cap faces in `outputs` §5.3, option
`face_loops`), fingerprints carry one more level of topology. Nested
fingerprints are plain, so this never recurses.

| kind | Additional key |
|---|---|
| face | `loops`: `[{isOuter, coedges: [{isOpposedToEdge, edge: edge fingerprint}]}]`, each loop's coedges in loop order |
| edge | `coedges`: `[{isOpposedToEdge, face: face fingerprint}]`, with the face of each coedge's loop |
| loop | `coedges` as for a face's loop |

`isOpposedToEdge` is true if the coedge runs against the edge's direction
(`start_point` → `end_point`). With the face normals this tells which face
lies left and which right of the edge (chamfer sides; the left/right rule
is [assumed], K34). Without coedges the lists are the loop's edges and the
edge's faces, without `isOpposedToEdge`. At most 200 coedges per loop and
8 per edge; a cut list ends with `{"_truncated": <count>}`.

### Body snapshot

Used in `steps[].bodies` and `final.bodies`:

```
{ "occurrence": "" | "<fullPathName>",   ("" = root component body)
  "component", "name", "isSolid", "isVisible",
  "entityToken"?            (final only),
  "volume", "area", "mass", "density",   (physical properties at the accuracy option)
  "center_of_mass": vec3,
  "moments_xyz": { xx, yy, zz, xy, yz, xz },   (moments of inertia about the world axes, kg·cm²)
  "bbox":       { min, max },   (may be loose)
  "bbox_tight"?: { min, max },  (a tight box along X and Y)
  "face_count", "edge_count", "vertex_count", "lump_count", "shell_count",
  "_errors"? }
```

The accuracy is the option `steps.accuracy`; `mass` and `density` depend
on the body's material. The order of the moment values is unconfirmed.

## 5. Design dump

```
{ "schema": "mitcad-f3d-dump", "schema_version": 2,
  "generator": { addin, addin_version, app_version, python, os, created_utc,
                 decoder, writer },   (§2)
  "units": {...},              (§1)
  "source": {...},             (§5.1)
  "options": {...},            (the writer's effective options)
  "warnings": [str],           (ignored or corrected options)
  "document": {...},
  "parameters": { "user": [Parameter], "model": [Parameter] },
  "components": [Component],
  "occurrences": [OccurrenceNode],
  "timeline": {...},
  "steps": {...},
  "final": {...},
  "errors": [{ "section": str, "error": str }],
  "timing": { "total_s": number },
  "exports"?: {...},           (§6)
  "_f3d"?: {...} }             (the stream decoder's data, §2)
```

### 5.1 source and document

`source`: `mode` and what it needs.

- Stream decoder: `"f3d_stream"` with `file` (`name.f3d`, or
  `package.f3z!<document>`) and `segment` (the design segment's folder).
- External dumps: `"local_path"`, `"data_file_id"` or `"active"` with the
  source read and `document_owned_by_addin` (true if the tool opened and
  closed the design itself); or `{mode: "script", job_id, export_name}`.

`document`: `name`, `isModified`, `app_version`, `design_type`
(`"ParametricDesignType"` or `"DirectDesignType"`), `default_length_units`
(e.g. `"mm"`), `distance_display_units`, `root_component`,
`component_count`, `data_file` (`{id, name, versionNumber,
fileExtension}` or null).

### 5.2 parameters, components, occurrences

`Parameter`: `name`, `expression`, `value`, `unit`, `comment`,
`isFavorite`, `dependents` (names of parameters whose expressions use it).
Model parameters also have `role` (e.g. `"AlongDistance"`), `createdBy`
(reference, usually `feature` or `sketch_dimension`) and `component`. User
parameters are the design's; model parameters come from every component,
in component order.

`Component`: `name`, `id`, `partNumber`, `description`, `is_root`,
`bodies` (`[{name, isSolid, isVisible, entityToken}]`, its own bodies),
`sketches` (names), `occurrence_count`, `feature_count`.

`OccurrenceNode`: `name`, `fullPathName`, `component`, `transform`,
`transform2`, `initialTransform` (mat4, §1), `isGrounded`,
`isGroundToParent`, `isVisible`, `isLightBulbOn`, `isReferencedComponent`
(the component lives in another document), `entityToken`, `children`
(OccurrenceNode list). At most 5000 nodes; a list cut there ends with
`{"_truncated": true}`. The stream decoder gives `isLightBulbOn: false` on
the nodes of the paths the file hides (one hidden-occurrence object per
path, `core/import/docs/f3d-appearances.md` *Light bulbs*) and leaves it out elsewhere.

The stream decoder (mitcad#81) gives `_f3d.placements`: where the
timeline's items put the occurrence, in timeline order, each `{index,
object_id, transform}` (the item's timeline index, absent for objects
that are not timeline items; the occurrence path's placement in the root
component after it, mat4, cm). The first is the item that made the
occurrence, at the transform the file stores for it; joints, captured
positions and rigid groups after it move it without changing that stored
transform, so the last one is where the file shows the occurrence. A
nested occurrence whose path the file does not name gets them only where
the tree has one node of it.

Components of other documents (mitcad#196): an occurrence with
`isReferencedComponent` has `_f3d.external_key` and
`_f3d.external_document` (the document's key in this design and its URN,
`…?version=N`) and `_f3d.component_object`, the component's object id in
that document (its root component's, in the designs read). Every
occurrence has `_f3d.guids`: the GUIDs (`str16`) in its object, lower
case; the occurrence paths of captured positions and relationships name
their levels by one of them (`path_guids`), also the levels inside a
component of another document, which are occurrences of that document.
`_f3d.external_documents` lists the documents: `key`, `urn`, `version`
(the URN's), `guids` (the two GUIDs before the URN in the MetaStream's
tail; occurrences of the document's components carry the first) and
`document_guid` (the GUID after it: one of the GUIDs the referenced
file's `Manifest.dat` lists, which tells the file).

### 5.3 timeline

```
{ "available": bool,                    (false for direct-modelling designs)
  "count": int | null,                  (top-level items)
  "original_marker_position": int | null,
  "restored_marker_position": int | null,
  "groups": [{ name, index, isCollapsed, isSuppressed, count }],
  "items": [Item] }                     (sorted by index)
```

Groups are flattened: every non-group item appears once in `items`, with
its group in `group`.

`Item`:

| Key | Meaning |
|---|---|
| `index` | timeline index |
| `group` | `{name, index}` of the containing group, or null |
| `name` | timeline item name. The stream decoder gives the custom name, else the default name the application shows: the file keeps construction planes, axes and joint origins under the internal bases `WorkPlane`, `WorkAxis` and `JointOrigin`, shown as `Plane1`, `Axis1` and `Joint Origin1` |
| `objectType` | short type of the item's entity (`Sketch`, `ExtrudeFeature`, `ConstructionPlane`, `Occurrence`, `Joint`, ...), null if unknown. The stream decoder also names items it knows only by their class (`ComponentInsert`, `Fastener`, `GeometricRelationship`, `Group`, `FlangeFeature`, ...: [docs/f3d-components.md](docs/f3d-components.md#items-without-a-translation)) |
| `entityToken`, `component` | of the entity; `component` is the owning component's name (the stream decoder: the component whose feature list holds the item, also for occurrence items, whose external dumps name the occurrence's component; its object id is `_f3d.component`) |
| `isRolledBack` | at the document's original marker position. The stream decoder writes `isRolledBack: true` (and nothing otherwise) for an item after the timeline marker: the second of the two state bytes before its last reference (`_f3d.after`), and the marker positions of the timeline ([docs/f3d-fallbacks.md](docs/f3d-fallbacks.md#items-suppressed-in-the-file)) |
| `isSuppressed`, `healthState`, `errorOrWarningMessage` | read with the marker at the end; `healthState` is the health state's name. The stream decoder writes `isSuppressed: true` (and nothing otherwise) for an item its tail marks suppressed: result number -1 with the first or second of the three state bytes (`_f3d.flags`) set (`000100` an item the user suppressed, `010000` one suppressed through an item it depends on: `decode::item_suppression`); a third byte set alone (`000001`) marks an item whose change is all in its parts in other components (`_f3d.part_of`) (mitcad#180, [docs/f3d-fallbacks.md](docs/f3d-fallbacks.md#items-suppressed-in-the-file)) |
| `detail_marker` | marker position when `detail` and `props` were read |
| `detail` | dedicated fields (below) |
| `props` | reflection fallback: the entity's other properties, two levels deep, without output collections |
| `outputs?` | features only, read with the marker right after the item (below) |

`outputs` keys, each present only if the feature type has the property:

| Key | Content |
|---|---|
| `bodies` | body fingerprints |
| `faces`, `startFaces`, `endFaces`, `sideFaces` | face fingerprints (at most `max_output_faces`) and `<name>_count`. **`null`** (no count) when the type has the face set but the feature has none, e.g. no caps on a full revolve (K23) or a cut without caps; a failed reading is in `_errors` |
| `startFace`, `endFace` | a single face fingerprint or `null` (loft caps) |
| `resultFeatures` | references to the features a pattern or mirror created |
| `patternElements` | `[{id, name, isSuppressed, transform, faces_count, occurrences?, faces?}]`, at most `max_pattern_elements` (then `{"_truncated": <count>}`). `transform` is the element's position relative to the original; the first is the identity. `occurrences` (references) only for occurrence patterns; `faces` (fingerprints, at most `max_output_faces`) only with option `pattern_element_faces` |

`face_loops` option: `"caps"` (default) gives `loops` (§4, *Topology*) to
the fingerprints of `startFaces`, `endFaces`, `startFace` and `endFace`
(e.g. to measure the hole in a tapered extrude's top face, K10); `"all"`
also to every face set and to `final` faces; `"none"` to none.

When the timeline is walked (`per_step`), feature inputs are read with the
marker immediately **before** the item (some inputs exist only then);
sketches, construction geometry, occurrences, canvases, joint origins and
base features **after** it. Items the walk does not reach (time budget,
`max_steps`, failures) are read at the end marker. `detail_marker` tells
which.

#### Sketch detail

```
{ name, component, isVisible?, isParametric?, is3D?, isFullyConstrained?,
  areProfilesShown?, areDimensionsShown?, areConstraintsShown?, arePointsShown?,
  isComputeDeferred?,
  referencePlane: reference (construction_plane with origin "XY"... or a face
                  fingerprint with loops, §3 Selections),
  transform: mat4 (the stored transform; direction unsettled, §1),
  origin, xDirection, yDirection: vec3 (model space),
  model_frame: { origin, x_axis, y_axis, z_axis: vec3,
                 sketch_to_model: mat4,
                 transform_matches?: ["sketch_to_model"?, "model_to_sketch"?] },
  originPoint?: point id | null,
  counts: { points, curves, texts, constraints, dimensions, profiles },
  points: [Point], curves: [Curve], constraints: [Constraint],
  dimensions: [Dimension], texts: [Text], profiles: [Profile] }
```

- `model_frame`: `origin` is sketch-space (0,0,0) in model space;
  `x_axis`/`y_axis`/`z_axis` are (1,0,0), (0,1,0), (0,0,1) in model space
  minus `origin` (the sketch's unit vectors; `z_axis` is the sketch normal,
  the positive extrude direction). `sketch_to_model` has these as columns
  (axes, then origin): model = `sketch_to_model` · sketch.
  `transform_matches` (present when `transform` was read) lists which
  reading of `transform` is consistent: `"sketch_to_model"` if it equals
  `sketch_to_model`, `"model_to_sketch"` if it equals the inverse
  (relative tolerance 1e-7); both for an identity frame (XY sketch), none
  if neither. Use `sketch_to_model`.
- `originPoint`: id of the sketch point projected from the component
  origin; with projected face edges it lets the stream decoder recover the
  frame.
- `Point`: `id`, `xyz` (sketch space), `isFixed`, `isReference`,
  `isFullyConstrained?`, `connected` (ids of connected curves; null for an
  entity that could not be resolved).
- `Curve`: `id`, `type` (`SketchLine`, `SketchCircle`, `SketchArc`,
  `SketchEllipse`, `SketchEllipticalArc`, `SketchFittedSpline`,
  `SketchFixedSpline`, `SketchControlPointSpline`, `SketchConicCurve`),
  flags `isConstruction`, `isFixed`, `isReference`, `isCenterLine?`,
  `isLinked?`, `isFullyConstrained?`, `isVisible?`; point ids
  `startSketchPoint?`, `endSketchPoint?`, `centerSketchPoint?`,
  `apexSketchPoint?`; id lists `fitPoints?`, `controlPoints?`; scalars
  `radius?`, `majorAxisRadius?`, `minorAxisRadius?`, `rhoValue?`, `degree?`,
  `isClosed?`, `length?`, `majorAxis?` (vec3); `geometry` (curve geometry
  in sketch space, including NURBS data for splines); `referencedEntity?`
  (reference, for projected/included curves with `isReference`). The
  stream decoder gives the true flags `isConstruction`, `isCenterLine`,
  `isFixed` and `isLinked` with `isReference` (always equal) and the three
  spline types (mitcad#186); for a fitted spline `fitPoints` lists the end
  points and those between; it gives no `referencedEntity`, and a fitted
  spline's tangent handle lines are no curves (a constraint on one holds
  the `sketch_entity` reference with its geometry). A whole circle the
  stored bodies hold also carries `_f3d_seam`: where the file's circle
  starts, in radians counter-clockwise about the sketch normal from the
  sketch x axis (the circle record's own reference direction is always the
  x axis), read from the start vertex of the closed edge the first
  extrusion using it made.
- `Constraint`: `id`, `type` (`CoincidentConstraint`,
  `HorizontalConstraint`, `TangentConstraint`, ...), `refs`, `props`.
  The stream decoder also gives `SmoothConstraint` (type bit `0x200`) and
  three types the API does not list, which constrain no geometry
  (mitcad#186): `ControlPolygonConstraint` (bit 31: the lines of a control
  point spline's polygon and the spline) and `TextBoxConstraint` (bit 40:
  the four lines of a sketch text's box, and its corner point in some
  files) and `TextPathConstraint` (bit 41: a text and its path curve,
  mitcad#1177).
- `Dimension`: `id`, `type` (`SketchLinearDimension`,
  `SketchDiameterDimension`, `SketchAngularDimension`, ...), `parameter`
  (parameter reference; null for some driven dimensions, K6), `isDriving`,
  `value` (internal units, cm or rad; for driven dimensions the only
  value), `textPosition` (vec3, sketch space; selects the quadrant of
  angular dimensions, K3), `refs`, `props`. The stream decoder gives a
  driven dimension (no parameter: a null holder reference) `isDriving`
  false and the value it shows as stored in `_f3d.expression` (except a
  distance between two lines or a point and a line, `SketchOffsetDimension`,
  with no holder: those are driving, with no parameter); the class
  of a dimension it cannot read is in `_f3d.class` (mitcad#178). It also
  gives (mitcad#186) `SketchEllipseMajorRadiusDimension` and
  `SketchEllipseMinorRadiusDimension` (one ellipse; `_f3d.axisLength` when its value is the whole axis, twice the radius), 
  `SketchTangentDistanceDimension` (a line and a circle or arc: the
  distance between them) and, for a point or line and a face or plane
  outside the sketch, `SketchDistanceBetweenPointAndSurfaceDimension` and
  `SketchDistanceBetweenLineAndPlanarSurfaceDimension` (the surface is an
  `error` reference: not decoded).
  - `refs`: every property of the constraint or dimension whose value is
    an entity, keyed by property name (`point`, `entity`, `lineOne`,
    `curveTwo`, `entityOne`, ...). The value is a sketch-local id string
    for entities of this sketch, a list of those, or a reference object
    otherwise.
  - `props`: the remaining plain properties (enum names, numbers, bools).
  - The stream decoder gives a constraint's entities as `refs.entities`
    (the stored order). For offsets and sketch patterns it adds `props`
    under the API's names, with sketch-local ids: `OffsetConstraint`
    `distance` (cm, signed: the side), `dimension` (its
    `SketchOffsetCurvesDimension`), `parentCurves`, `childCurves`;
    `CircularPatternConstraint` `centerPoint` (the last entity),
    `createdEntities` (the copies: the entities after the first
    n / quantity), `quantity`, `totalAngle` (parameter references);
    `RectangularPatternConstraint` `createdEntities`, `quantityOne`,
    `quantityTwo`, `distanceOne`, `distanceTwo` (signed, along their
    direction), `directionOne`, `directionTwo` (unit vec3),
    `directionOneEntity?` and `flags` (three stored bytes, not decoded).
    A `PolygonConstraint`'s entities are its corners in order and its
    centre last.
- `Text`: `id`, `text`, `height`, `position`, `fontName`, `angle`,
  `isHorizontalFlip`, `isVerticalFlip`, `textStyle` (raw value),
  `boundingBox`, `definition?`. `height` is the capital letters' height (cm).
  The API gives no `position` and `angle` for a text in a box or on a path;
  the stream decoder writes them when the file stores the text's placement
  matrix (the box's corner and the baseline's direction), and `definition`
  as `MultiLineTextDefinition` (`rectangleLines`: the box's four curves
  `{kind: "sketch_entity", id}` in order, the first running along the
  baseline from the text's corner; `horizontalAlignment`,
  `verticalAlignment` as the API's names) or `AlongPathTextDefinition`
  (`path`: the curve; `isAbovePath` from the order the file stores its
  characters' placements in, mitcad#1177; for a whole circle as the path,
  `_f3d.pathStart`, where the circle starts as a path: radians about the
  sketch normal from the sketch x axis, opposite its reference direction). `textStyle` bit 1 (bold) comes from the font weight
  (600 and up); italic and underline are not decoded. `_f3d` holds the raw
  flags, weight, width factor and kind. The `.ipt` decoder (mitcad#414)
  adds `lineSpacing` (cm, baseline to baseline, measured from the glyph
  contours the file keeps), `penAligned` (lines align by their pens) and
  `inkTop` (a frame's top meets the first line's highest outline point),
  and `_ipt`: the text record, its alignment code and `glyphs` (each
  stored glyph contour's pen and pieces in sketch coordinates). From those
  contours it also gives `glyphs` (mitcad#416): one per character of
  `text` (newlines included), `{advance, contours}` in cm in the text's
  own axes from the character's pen, each contour a list of Bézier pieces
  (`[[x, y], …]`: two points a line, three a quadratic; the file's
  B-splines split at their knots); the advance runs to the next pen of its
  line (white space between included), a line's last to where its
  alignment about the anchor ends the line (left: its outline). Left out,
  with the reason in `_ipt.glyphsLeftOut`, when the contours do not lie as
  the text's layout puts them (one pen per character that is not white
  space, pens on baselines `lineSpacing` apart, a line starting at or
  centred on the anchor, the frame's top at the first line's highest
  outline point, to 1e-5 cm). The import gives them to the model text as
  its `glyphs` (in units of the height), its shapes while the font is not
  installed.
- `Profile`: `index`, `loops` (`[{isOuter, curves: [{curve, geometry}]}]`:
  `curve` is the id of the sketch curve the profile curve lies on,
  `geometry` the profile curve's own geometry, possibly part of that
  curve), `area`, `centroid`, `perimeter` (at the accuracy option), `bbox`,
  `isOnSketchPlane` and `plane` (surface geometry of the profile's plane,
  which can differ from the sketch plane when `isOnSketchPlane` is false;
  its space is unknown). `centroid` is relative to the sketch origin, read
  as sketch space [assumed]; `bbox` likewise [assumed]; both to be
  confirmed with a sketch on an offset plane (K1).

#### Feature detail

`detail` keys are the feature type's property names, present when the
writer could read them, converted per §1. Nested objects (extent
definitions, edge sets, joint motions, thread info ...) are reflected two
property levels deep (`LoftFeature.loftSections`: three, so each section's
`endCondition` keeps its angle and weight), e.g.

```json
"extentOne": {"_type": "DistanceExtentDefinition",
              "distance": {"kind": "parameter", "name": "d2", "expression": "10 mm", "value": 1.0, "unit": "mm"}}
```

A key `a.b` is property `b` of property `a`, used where `a` is a
collection whose own flag would otherwise be lost
(`centerLineOrRails.isCenterLine`); null when `a` is null.

*Retired* names are older names of the same values, read after the current
ones for designs that have them; *preview* names are values not every
design has. Nested fields that come with a listed field's reflection are in
parentheses.

| objectType | detail keys |
|---|---|
| ExtrudeFeature | operation, profile, extentType, extentOne, extentTwo (`ToEntityExtentDefinition`: entity, isChained, offset, isMinimumSolution, directionHint; `ThroughAllExtentDefinition`: isPositiveDirection; `SymmetricExtentDefinition`: distance, isFullLength, taperAngle), hasTwoExtents, symmetricExtent, startExtent, taperAngleOne, taperAngleTwo, participantBodies, isSolid, isThinExtrude, thinExtrudeWallLocationOne/Two, thinExtrudeWallThicknessOne/Two. The stream decoder gives the extents from the extrude record: `extentType` and `hasTwoExtents`; a side through all with `isPositiveDirection` (the flag byte: 0 along the sketch normal, 1 against it); `SymmetricExtentDefinition` (also as `symmetricExtent`) with its length byte as `isFullLength`; `ToEntityExtentDefinition` with `isChained` (the slot role 17 or 18), `isMinimumSolution` (the byte after the object reference) and `directionHint` (the stored direction vector, turned round by the flag byte); `isSymmetric` false on distances; `startExtent` `ProfilePlaneStartDefinition` (its `profilePlane` the sketch plane in sketch coordinates); `_f3d_missing_texts`, the object ids of the sketch texts its text profiles (`92F9A5F4`) name that the design no longer has (mitcad#332); `_f3d_text_profiles`, those the sketch has (`texts`: their sketch-local ids), with the number of profiles the item selects (`profiles`) and whether all are the texts' (`only`; mitcad#1177) |
| RevolveFeature | operation, profile, axis, isProjectAxis, extentDefinition (angle, isSymmetric / angleOne, angleTwo / to-entity), isSolid, participantBodies. The stream decoder gives `operation`, `isSolid`, `isProjectAxis` (class version 2), `extentDefinition` (`isSymmetric`; `TwoSidesAngleExtentDefinition` with `angleOne`, `angleTwo`), `axis` (an origin or construction axis, a sketch line, a cylindrical or conical face, an edge) and `participantBodies` of a join or cut |
| HoleFeature | holeType, holeTapType, holeDiameter, tipAngle, counterboreDiameter, counterboreDepth, countersinkDiameter, countersinkAngle, isDefaultDirection, extentDefinition, position, direction, holePositionDefinition, participantBodies, tappedHoleInfo (a ThreadInfo), clearanceHoleInfo, thread (the hole's ThreadFeature: isModeled, isFullLength, threadLength, threadOffset, threadLocation, isRightHanded). The stream decoder gives a tapped hole's `holeTapType` (`TappedHoleTapType`), `tappedHoleInfo` and `thread` (`isModeled`, `isFullLength`, `threadLocation`, `threadLength` the thread depth, `threadOffset`) from its thread sub-item. It also gives (mitcad#67) `position`, the first of the hole's points (`_f3d_positions`, every point, in the component, cm), `holeType` (from the counterbore or countersink sizes among its parameters, else `SimpleHoleType`), `counterboreDiameter`, `counterboreDepth` and an `extentDefinition` from the hole's slot of role 5 (`AllExtentDefinition` with the bodies as its input, `OneSideToExtentDefinition`, its type only, with a plane or face; a hole without such a slot goes to its depth), and `holePositionDefinition` (`PlaneAndOffsetsHolePositionDefinition`: `planarEntity`, `edgeOne`, `edgeTwo`, `offsetOne`, `offsetTwo`; its `planarEntity` is also a construction or origin plane the hole starts on), and since mitcad#25 `isDefaultDirection` (class version 7 records); not `direction` |
| ThreadFeature | threadInfo (isInternal, isRightHanded, isTapered, threadType, threadSize, threadDesignation, threadClass, majorDiameter, minorDiameter, pitchDiameter, threadPitch, threadAngle, ...), isModeled, isFullLength, threadLength, threadOffset, threadLocation, isRightHanded, inputCylindricalFaces, inputCylindricalFace, hole. The stream decoder gives `threadInfo` (type, size, designation, class, `isInternal`, `isRightHanded` (versions 4 and 5 of the thread object), the angle in degrees, the diameters and pitch in cm), `isModeled`, `isFullLength`, `threadLocation`, `threadLength`, `threadOffset` and `inputCylindricalFaces` (named faces, §2). The `.ipt` import gives a face as `{kind: face, geometry: {type: Cylinder, axis}, start_point, end_point}`: the thread's two ends (cm), on its cylinder's surface or axis, the import taking the cylinder through both |
| FilletFeature | filletFeatureType, edgeSets (`ConstantRadiusFilletEdgeSet`: edges, radius, isTangentChain, continuity, tangencyWeight; variable radius: startRadius, endRadius, midRadii, midPositions; chord length; asymmetric), isRollingBallCorner, ruleFilletSettings, fullRoundFilletFaceSets; retired isG2, isTangentChain |
| ChamferFeature | edgeSets (distance / distanceOne, distanceTwo / distance, angle; isFlipped, isTangentChain), cornerType; retired chamferType, chamferTypeDefinition, edges, isTangentChain |
| ShellFeature | inputEntities, insideThickness, outsideThickness, isTangentChain, shellType. The stream decoder gives `isTangentChain` and `shellType` (`RoundedOffsetShellType` or `SharpOffsetShellType`) from the two bytes after the root part. The `.ipt` decoder gives the body as `inputEntities` and the number of faces removed as `_ipt_removed_faces`, not the faces: the import finds them with the history (the faces of the body the next state no longer has) |
| DraftFeature | inputFaces, plane, isTangentChain, isDirectionFlipped, draftDefinition (angles, symmetry); preview draftType, partingLineType, partingLineCurves, movingPartingLineDirection, movingPartingLineFixedEdges |
| SweepFeature | profile, path, guideRail, guideSurfaces, operation, orientation, distanceOne, distanceTwo, taperAngle, twistAngle, isSolid, profileScaling, extent, isDirectionFlipped, isChainSelection, participantBodies, solidBody, solidOrientation, solidAlignedAxis, solidTwistAxis. The stream decoder gives `operation`, `profile` (one profile reference per selected profile, with its sketch only), `path` and `guideRail` (`PathEntity` items of sketch curves or named edges, §2), `guideSurfaces` (their named faces, when there are some), the distances, `taperAngle`, `twistAngle` and since mitcad#25 `orientation` |
| LoftFeature | loftSections (entity, index, endCondition), centerLineOrRails, centerLineOrRails.isCenterLine, operation, isSolid, isClosed, isTangentEdgesMerged, startLoftEdgeAlignment, endLoftEdgeAlignment, participantBodies. The stream decoder gives `operation`, `loftSections` (`entity` a profile with its sketch only, a sketch point, or a list of `PathEntity` items of named edges; the first and last sections' `endCondition` with its `weight` and `angle`), and `centerLineOrRails` as a list of paths (`PathEntity` lists) with `centerLineOrRails.isCenterLine`, and since mitcad#25 `isClosed` and `isSolid` |
| PipeFeature | path, sectionType, sectionSize, sectionThickness, isHollow, operation, distanceOne, distanceTwo, participantBodies. The stream decoder gives `operation`, `path` (as a sweep's), `sectionSize`, `sectionThickness` and the distances, and since mitcad#25 `sectionType` and `isHollow` |
| CoilFeature, RibFeature, WebFeature | external dumps: no inputs are recorded; their model parameters (`parameters.model`: `role`, `createdBy`) give the sizes. The stream decoder gives a coil's `diameter`, `pitch`, `revolutions`, `height`, `angle` and `sectionSize` (the same parameters), and since mitcad#25 `coilType`, `sectionType`, `sectionPosition`, `isClockwise` (true for the default right-handed helix), `operation`, `participantBodies`, the `plane` it stands on and its `frame` and `placement` (4x4 matrices in cm: the world placement is `frame * placement`; absent: the identity). The `.ipt` import gives `coilType`, `pitch`, `height`, `revolutions`, `angle`, `operation`, `profile` (profile references) and `axis` (a sketch line, an origin axis, or a work axis by its geometry, fixed where it is) with `flip` (the coil runs against the axis reference), without the hand |
| BoxFeature, CylinderFeature, SphereFeature, TorusFeature | external dumps: no inputs are recorded. The stream decoder gives (mitcad#25) `operation`, `participantBodies`, the `plane` (a plane or face reference), `frame` and `placement` (4x4 matrices, cm: a point `p` of the primitive's own frame is at `frame * placement * p` in the component's coordinates; absent: the identity; a box's width `DistanceX` is along its x, its length `DistanceY` along y, its height `DistanceZ` along z; a cylinder's base centre, a sphere's centre and a torus' centre are the origin, the axis z), the sizes as parameter references (`distanceX`, `distanceY`, `distanceZ`, `offsetDistanceX`, `offsetDistanceY`, `height`, `diameter`, `majorDiameter`, `minorDiameter`) and a torus' `torusPosition` (`Center`, `Inside`, `OutsideTorusPosition`) |
| RectangularPatternFeature | inputEntities, patternEntityType, directionOneEntity, directionTwoEntity, directionOne, directionTwo (vectors), quantityOne, quantityTwo, distanceOne, distanceTwo, patternDistanceType, isSymmetricInDirectionOne/Two, patternComputeOption, suppressedElementsIds. The stream decoder gives `inputEntities` (bodies, features as `feature` references, or faces) with `patternEntityType`, and each direction's entity (an origin or construction axis, a sketch line, an edge or a face) with its vector where one is stored (mitcad#67), and the data object's settings (mitcad#186): `patternDistanceType`, `isSymmetricInDirectionOne` (and `Two` for a second direction), `patternComputeOption` and, for a pattern with no suppressed element, `suppressedElementsIds` `[]`. A second direction with no entity has its vector from the data object (24 bytes after a `-1` marker before the two distances' holders; all zero when the pattern has one direction, then none is given). A flipped direction is a negative `distanceOne`; no flag and no changed vector |
| CircularPatternFeature | inputEntities, patternEntityType, axis, quantity, totalAngle, isSymmetric, patternComputeOption, suppressedElementsIds. The stream decoder gives `inputEntities` and `patternEntityType` as for rectangular patterns, and the `axis` (an origin or construction axis, a sketch line, an edge or a face), and the data object's settings (mitcad#186): `isSymmetric`, `patternComputeOption` and `suppressedElementsIds` (the API's numbering, the original 0) |
| PathPatternFeature | inputEntities, patternEntityType, path, quantity, distance, startPoint, patternDistanceType, isFlipDirection, isOrientationAlongPath, isSymmetric, patternComputeOption, suppressedElementsIds |
| MirrorFeature | inputEntities, mirrorPlane, patternComputeOption, isCombine, stitchTolerance. The stream decoder gives `inputEntities` and `patternEntityType` as for patterns, and `mirrorPlane` (an origin or construction plane, or a planar face by its names), `isCombine` and `patternComputeOption` (mitcad#186) |
| CombineFeature | targetBody, toolBodies, operation, isKeepToolBodies, isNewComponent. The stream decoder gives `operation`, `isKeepToolBodies`, `targetBody` and `toolBodies` (bodies by their names, mitcad#67); where the tools are bodies of other documents the lists hold the body record the combine made where the target is, and the tools are the selections of the first list, the target the only other selection of the item (mitcad#196). The `.ipt` decoder reads them from the record and marks them `_ipt_inputs`: the history then guesses bodies, not other operations |
| SplitBodyFeature | splitBodies, splittingTool, isSplittingToolExtended. The stream decoder gives `splitBodies`, `isSplittingToolExtended` and `splittingTool` when the tool is one plane, face or body (mitcad#67) |
| SplitFaceFeature | facesToSplit, splittingTool, splitType, directionEntity, isSplittingToolExtended. The `.ipt` decoder gives `splittingTool` (profiles of a sketch, or a construction plane) and the `body`, not the faces: the import finds them with the history (the faces of the body the next state changed that it no longer has whole); profiles are split along the curves around their regions, then along the whole sketch |
| MoveFeature | inputEntities, definition (its subtypes: transform / xDistance, yDistance, zDistance, isDesignSpace / axisEntity, angle / ...), retired transform (the rigid matrix) |
| CopyPasteBody | sourceBody |
| OffsetFacesFeature | inputFaces, distance |
| ThickenFeature | inputFaces, thickness, isSymmetric, operation, isChainSelection |
| ScaleFeature | inputEntities, point, scaleFactor, isUniform, xScale, yScale, zScale |
| ReplaceFaceFeature | targetFaces, isTangentChain; sourceFaces and inputFaces when a writer records them (otherwise the import finds the replaced faces from the file's history) |
| DeleteFaceFeature | deletedFaces |
| MoveFaceFeature | faces (named faces, §2, with their geometry where the history at the item's place has them), transform (the rigid 4x4 matrix, row-major, cm, as a move's); the stream decoder's, mitcad#343 |
| EmbossFeature | profile (one reference to the sketch per selected profile, the loops in `_f3d_profile_loops` as an extrusion's), inputFaces (named faces, §2), depth (negative: sunk), horizontalDistance, verticalDistance, rotationAngle (the profiles' placement on the face); the stream decoder's, mitcad#458 |
| RemoveFeature | itemToRemove |
| OffsetFeature, PatchFeature, StitchFeature, ExtendFeature, TrimFeature, RuledSurfaceFeature, SilhouetteSplitFeature, BoundaryFillFeature | their input entities and parameters (unverified) |
| BaseFeature | sourceBodies |
| ConstructionPlane | definition (e.g. `ConstructionPlaneOffsetDefinition`: planarEntity, offset; the `.ipt` import also gives `ConstructionPlaneMidplaneDefinition`: planarEntityOne, planarEntityTwo, `ConstructionPlaneAtAngleDefinition`: linearEntity, planarEntity, angle, `ConstructionPlaneThreePointsDefinition`: pointEntityOne, pointEntityTwo, pointEntityThree, `ConstructionPlaneTwoEdgesDefinition`: linearEntityOne, linearEntityTwo, `ConstructionPlaneLineAndPointDefinition` (through both) and `ConstructionPlaneNormalToLineDefinition` (through the point, normal to the line): linearEntity, pointEntity; a plane, axis or point (`Point3D` with its `origin`) off the timeline by its `geometry`), geometry, transform, isParametric. The stream decoder (mitcad#178) gives the offset's planarEntity, the angle's linearEntity (an axis, an edge, a sketch line, or a cylindrical or conical face: a tangent plane) and planarEntity where the item names one (a face, a plane or a sketch profile, `kind: profile`), `ConstructionPlaneDistanceOnPathDefinition`'s distance and `pathEntities` (edges or sketch curves), and, for items without a parameter, the midplane, two-edge, line-and-point and three-point definitions from what their inputs are; faces and edges with their geometry where the history at the item's place in the timeline has them |
| ConstructionAxis, ConstructionPoint | definition, geometry, isParametric. The stream decoder (mitcad#186) gives, from the entities an item names (`_f3d.datum_code`: its stored definition code), a plane's definitions through three points (vertices, construction points or sketch points: `pointEntityOne`, `pointEntityTwo`, `pointEntityThree`), tangent to a face at a point (`ConstructionPlaneTangentAtPointDefinition`: `tangentFace`, `pointEntity`), tangent to a face with a reference plane and an angle (`ConstructionPlaneTangentDefinition`: `tangentFace`, `planarEntity`, `angle`) and parallel to a plane through a point (`ConstructionPlaneOffsetThroughPointDefinition`: `planarEntity`, `point`, the offset parameter being the distance); a point's `ConstructionPointPointDefinition` (`pointEntity`), `ConstructionPointCenterDefinition` (`circularEntity`) and `ConstructionPointThreePlanesDefinition` (`planeOne`, `planeTwo`, `planeThree`); an axis' `ConstructionAxisEdgeDefinition` (`edgeEntity`: an edge or sketch line), `ConstructionAxisCircularFaceDefinition` (`circularFace`), `ConstructionAxisTwoPointDefinition` (`pointEntityOne`, `pointEntityTwo`) `ConstructionAxisTwoPlaneDefinition` (`planarEntityOne`, `planarEntityTwo`) and `ConstructionAxisNormalToFaceAtPointDefinition` (`face`, which may be a sketch profile, `pointEntity`). A vertex is `{"kind": "vertex", "objectType": "BRepVertex", "point": [x, y, z], "_f3d": ...}`, its position (cm) found in the history at the item's place; a point's `geometry` is `[x, y, z]` (cm), an axis' the line the item stores |
| Occurrence | component, fullPathName, transform2, transform (retired), initialTransform, isGrounded, isGroundToParent, isReferencedComponent, isLightBulbOn, isVisible. The stream decoder (mitcad#75) gives the items that place occurrences (`Occurrence`, `ComponentInsert`, `Fastener`, `CopyPasteOccurrence`, `DerivedInstance`) `occurrence`: the occurrence the item made, an `occurrence` reference whose `_f3d.path` is its object id |
| Joint | occurrenceOne, occurrenceTwo, geometryOrOriginOne, geometryOrOriginTwo, jointMotion, offset, angle, isFlipped, isLocked. The stream decoder (mitcad#66) gives `occurrenceOne`/`Two` (`occurrence` references with `component` and `_f3d`: `path`, the occurrence object ids from the joint's component down, `null` for a level in another document (one level of the file can name several occurrences, a path inside a component of another document: each is a level here, mitcad#75); `path_guids`; `context_component`), `geometryOrOriginOne`/`Two` (a `JointGeometry`: `origin`, `primaryAxisVector`, `secondaryAxisVector`, `thirdAxisVector` of the side's frame in its component, `entityOne`/`entityTwo` the faces or edges its key point and direction name, by their names; `_f3d`: `frame`, `key_points`, `directions` with their codes; or a `feature` reference to a joint origin on the timeline; or `{_type: JointOrigin, _f3d: {entity_input, target}}` for an origin in another document), `jointMotion` (`<Kind>JointMotion` with `jointType`, `rotationLimits`/`slideLimits` (`JointLimits` of parameter references) and `_f3d`: `type_code` (rigid 0, revolute 1, slider 2, cylindrical 4, pin-slot 6, planar 7, ball 8; mitcad#81) and `motions`, the free motions in the order turns about x, y, z, slides along x, y, z, each `{motion, axis, limits}` (`rx` … `tz`, an axis code, the stored maximum, minimum and rest values; the current values are not stored with the joint: the occurrences' `placements` give them)), `angle`, `offset` (z) and `offsetX`, `offsetY` (parameter references), `isFlipped` (the frames' z axes opposed), and `_f3d`: `opposed` (the same as a byte), `frames` (each side's stored frame, the identity where none is stored), `aligned_frame` (side one's frame with the alignment applied). With the occurrences' placements after the joint (`_f3d.placements`), `world(one) · frame one · Rx(π if isFlipped) · T(−offset) · Rz(−angle) = world(two) · frame two` (mitcad#81: the joint test model's joints at 30°, flipped and not); a ball joint's values are `Rz(pitch) · Rx(yaw) · Rz(roll)` about its pitch and yaw axes |
| AsBuiltJoint | occurrenceOne, occurrenceTwo, geometry, jointMotion, offset. The stream decoder gives `occurrenceOne`/`Two`, `geometry` (a `JointGeometry` or null), `jointMotion` as for joints, and `_f3d.placements`: each occurrence with the frame recorded in its component (`{occurrence, frame}`), the recorded placement |
| JointOrigin | geometry, offsetX, offsetY, offsetZ, angle, isFlipped, xAxisEntity, zAxisEntity. The stream decoder gives `geometry` as a joint's `JointGeometry` and the offsets and angle |
| RigidGroup | occurrences, includeChildren. The stream decoder (mitcad#81): the file stores a rigid group as an as-built joint of motion type 11; it gives `occurrences` (`occurrence` references as a joint's; with "include children" the members' children are listed too) and `_f3d.placements` (each member with the group's frame in its component) |
| Snapshot | a captured position. The stream decoder (mitcad#75) gives `positions`: per occurrence it places `occurrence` (an `occurrence` reference as a joint's, the path from the item's component) and `transform`, the occurrence's placement in that component (mat4, cm; the identity where none is stored). The stored occurrence transforms are older than the captured positions: the file's joints and as-built joints hold at the last captured placements *(the designs read)*. Some paths name top-level occurrences by ids no occurrence has (`null` with their `path_guids`). A captured position of joints' values stores them as parameters; its `positions` are not decoded (the occurrences' `placements` name it) |
| GroundOccurrence | the stream decoder: `occurrence`, the grounded occurrence (as a joint's `occurrenceOne`); the occurrence tree's node gets `isGrounded` |
| GeometricRelationship | an item of assembly constraints. The stream decoder (mitcad#155) gives `rows`, one per constraint in order: `type` (`align`, `center` or `angle`; absent when the constraint is not decoded), `geometry` (per frame `point`, `axis`, `plane` or `circle`: side one's first, a centring's two faces per side), `sense` (`aligned`: the frames' z axes the same way, `opposed`, `either`), `occurrenceOne`/`Two` (`occurrence` references as a joint's; an empty path the item's component's own geometry), `entitiesOne`/`Two` (each side's faces or edges by their names), `offset` or `angle` (parameter references), and `_f3d`: `record` and `constraint` (object ids), `type_code`, `frames` (mat4, cm, in the component the side's path ends in: a plane's origin on it and z its normal, an axis's z along it, a circle's centre and normal), `points` (the points picked on the faces and edges, cm), `selections`, `values` (the stored maximum, minimum and value; the maximum and minimum are not used in the files read), `flags`, `inputs`, `key_points`. Where the file places the occurrences after the item, two planes hold `(o2 − o1)·z1 = offset`, a point or an axis and a plane `(o1 − o2)·z2 = offset`, two axes are collinear, two circles share their axis with their planes `offset` apart, a centring's two pairs of faces share their middle plane, an angle's z axes are at `angle` *(the designs read)* |
| TangentRelationship | a contact between two bodies. The stream decoder (mitcad#155) gives `occurrenceOne`/`Two` (as a joint's), `bodyOne`/`Two` (body references by their names and the items that made them), `pointOne`/`Two` (the point picked on each body's face, cm, in its component) and `faceOne`/`Two` (those faces by their names) |
| Canvas | imageFilename, opacity, isDisplayedThrough, planarEntity, transform |
| DerivedContext | a derive: entities (bodies) of other documents brought into the item's component (mitcad#324). The stream decoder gives `source` (`kind` `design`, `name`, `_f3d`: the source design's document `key`, `urn`, `document_guid` and its root `component_object`), `placements` (the deriving design's placement in the source design, then one without a path) and `entities`: per entity `kind` (`body`), `placement` and `_f3d` (the entity's document `key`, `urn`, `document_guid`, its `object_id` and `class` there, `feature`: the entity's own base feature in this design, `bodies`: the body objects of this design it holds, `links`: the properties that stay linked, e.g. `Geometry`, `Transform`). A placement is `occurrence` (an `occurrence` reference as a joint's, its `path_guids` an occurrence path of the source design from its root) and `transform` (mat4, cm) where the file keeps a matrix; else, where the source design's file is at hand, the path's placement in the source design's occurrence tree (`_f3d.from_source`), or `_f3d.why` |
| any other type | `{}`; everything is in `props` |

Detail keys are left out of `props`, so nothing is repeated. The stream
decoder writes the main fields of the types it decodes; what the import
reads is listed in `core/model-api/src/commands.md` (*From the dump IR*).

Input selection details (mitcad#186):

- An extrusion's `participantBodies` is read from its role-8 input slot.
  Body references in termination slots are not participants. A decoded
  empty slot remains an empty list; a missing or unresolved slot retains
  the older body-input traversal. This field names the input selection;
  resolving it to a replayed body is a separate step.
- For constant-radius fillets and chamfers, a size parameter's holder
  identifies its set record and that record's counted selection input.
  A reordered flat input list therefore does not reorder the edge sets.
  The direct reading requires one set and one selection group, selector
  8 or 16, and a complete nonempty list of decoded edge/face references.
  Unsupported or incomplete layouts keep the older group reading.
  Fillet options are associated by the size parameter's holder too.
- A mirror keeps a decoded `inputEntities` selection even when its
  `mirrorPlane` is missing, and keeps its plane when selection decoding
  fails. Absence of one field does not erase the other.

The import report and learning dump have a separate `input_provenance`
field (`read`, `inferred`, `unknown`); it is not part of this design IR.
It describes an accepted candidate's inputs independently of geometric
verification or the candidate's history-only `guess` policy. See
`docs/f3d-learning.md`, *Input decoding audit*.

Sheet metal (mitcad#211): the `.ipt` decoder's `SheetMetalFaceFeature`,
`SheetMetalFlangeFeature` and `CutFeature` items and the stream decoder's
`FlangeFeature` items carry `detail.sheet`, an object of one `kind`
(lengths in cm, angles in radians, parameters as parameter references):

| `kind` | Keys |
|---|---|
| `face` | `operation` (`NewBodyFeatureOperation`, `JoinFeatureOperation`), `profile` (profile references as an extrusion's; `_f3d_profile_loops` where the stream decoder reads them), `direction` (the unit vector the material lies along from the sketch's plane, unless `side` is 1) or `along_sketch_normal: true` (along the file's sketch normal), `side` (`.ipt`: 0, or 1 for the material against `direction`), `thickness`, `rules` |
| `flange` | `operation`, `edges` (how many edges it is on), `angle`, `length`, `radius` (a parameter, or `{value}` when the rule's radius is stored as a number), `participantBodies`; either (`.ipt`) `path` (points across the middle of the edge, from the stationary tangent line through the bend to the flange's end), `length_datum` and `length_end` (the points its length is measured between), `edge_list` (the edges read by their names, mitcad#242: edge references with `start_point`, `end_point`, `mid_point` and `length` from the history state the flange starts from, each with a `width_extent` `{type: "offsets", from, to, offset_from, offset_to}` where the file has one: the ends of the edge, cm, and the offsets measured from them) and `edge_errors` (why an edge was not read), or (`.f3d`) `edge` (an edge reference with its geometry), `position` (`inside`, `outside`, `adjacent`, `tangent`), `length_mode` (`inner`, `outer`, `tangent`), `flip`, `widths` (the width owners: `{role, parameter}`) |
| `cut` | `operation`, `profile`, `extentType`, `extentOne` and `direction` as an extrusion's, `participantBodies`, `options` (`.ipt`: the two booleans of the sheet-metal cut, slots 31 and 33) |
| `corner_round`, `corner_chamfer` (`.ipt`) | `dressup` (a fillet's or chamfer's detail; each edge set's `edges` the edges read by their names, as `edge_list`'s, mitcad#242), `edge_errors`, `participantBodies` |
| `bend` (`.ipt`, mitcad#242) | `edges`, `edge_list`, `edge_errors` (its two edges), `radius`, `thickness`, `participantBodies` |
| `fold` (`.ipt`, mitcad#242) | `angle`, `radius`, `line` (`{point, direction}`, cm), `kind_enumeration` and `flags` (not decoded), `participantBodies` |

`rules` is the rule the body follows: `name`, `thickness`, `bend_radius`,
`bend_relief` and `corner_relief` (`type`, `width`, `depth`), `k_factor`
(`{value}`), each a parameter reference or `{expression, value}` (an
expression of the file's parameters); `inferred` lists the inputs that are
not read from the file, with why. A sheet item the decoder could not read
has `detail._sheet_error` instead.

### 5.4 steps

```
{ "enabled": bool, "accuracy": "low" | "medium" | "high" | "very_high",
  "complete": bool,
  "stop_reason": null | "max_steps" | "time_budget" | "disabled" | "no_timeline",
  "items": [Step] }
Step = { timeline_index, name, objectType,
         marker_position,      (marker right after the item, i.e. the state the snapshot shows)
         error,                (null or a message; other steps continue)
         bodies: [Body snapshot],
         elapsed_s }           (seconds since the dump started)
```

One step per non-group timeline item in index order, suppressed items
included. A step's bodies are all bodies that exist with the marker after
that item.

### 5.5 final

```
{ "marker_position": int | null,       (the end of the timeline)
  "bodies": [Body snapshot + { "faces": [face fingerprint], "faces_truncated": bool }] }
```

The final state is always the fully computed timeline (marker at the end),
even if the document was saved with the marker further back (that position
is `timeline.original_marker_position`). `faces` holds at most `max_faces`
fingerprints per body; with `face_loops` = `"all"` they carry `loops` (§4,
*Topology*).

## 6. exports

```
{ "dir": "<the folder of the files>",
  "files": [ { "kind": "f3d" | "step" | "smt" | "body_step" | "body_stp" | "body_smt" | "body_sat",
               "path": str, "ok": bool, "size"?: int, "error"?: str, "traceback"?: str,
               "body_index"?: int, "body"?: str, "occurrence"?: str } ] }
```

Files an external writer exported with the dump: the design
(`design.f3d`), the final bodies (`final.step`, `final.smt`) and each body
(`body-<n>.<ext>`, `<n>` its index in `final.bodies`). A dump named
`<name>` has `<name>.f3d`, `<name>.step`, `<name>.smt` and
`<name>-body-<n>.<ext>`. A per-body file of an occurrence's body is in the
coordinates the writer gave it.

## 7. Version 1 dumps

Readers do not check `schema_version` or `schema` and keep unknown keys,
so version 1 dumps still read, but these version 1 keys are not the ones
above and are ignored:

| Version 1 | Version 2 |
|---|---|
| `DraftFeature.inputEntities` | `inputFaces` |
| `MoveFeature.moveFeatureDefinition` | `definition` (`defineAsFreeMove` removed) |
| `OffsetFacesFeature.faces` (an output) | `inputFaces` |
| `HoleFeature.isTapped`, `threadInfo` | `holeTapType`, `tappedHoleInfo` |
| `RevolveFeature.extentType`/`extentOne`/`extentTwo` | `extentDefinition` |
| `FilletFeature.cornerType`, `ChamferFeature.isFlipped`, `MirrorFeature.patternEntityType` | removed (the type has no such value; `isFlipped` is per edge set) |
| empty `outputs` face sets absent | `null` |
| other `schema` name; application version under another key in `generator` and `document` | `"mitcad-f3d-dump"`; `app_version`; `_f3d.format.writer_build` |

Version 1 dumps also lack many fields above (sketch `model_frame`,
`originPoint`, dimension `value`, profile `plane`, topology in
fingerprints, several feature fields and enum names), so the import
falls back where it needs them.
