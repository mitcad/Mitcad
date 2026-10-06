# The design dump schema (version 2)

The design dump is Mitcad's JSON form of an `.f3d` design: parameters,
components, occurrences and timeline, with each item's definition. It is
the intermediate representation the `.f3d` import replays
(`mitcad_f3d::design::ir::Dump`, [README.md](README.md)). `schema` is
`"mitcad-f3d-dump"`, `schema_version` is `2`; an incompatible change
increments `schema_version` (version 1 differences: §7).

Notation: `T?` — key may be absent; `T | null` — key present, value may be
null. `vec3` is `[x, y, z]`, `vec2` is `[x, y]`, `mat4` a 4x4 nested array.

**[assumed]** marks a meaning not yet confirmed: the reading Mitcad uses,
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
  version of the application that wrote the file.
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

Readers keep unknown keys in each record (a dump survives read and write
unchanged) and do not check `schema`.

## 3. References

Every reference is an object with a `kind`.

| kind | Keys |
|---|---|
| `parameter` | `name`, `expression`, `value`, `unit` |
| `profile` | `sketch` (name), `sketch_timeline_index`, `profile_index`, `loops` (`[{isOuter, curves: [sketch-local curve id \| null]}]`: the profile's loops in the writer's order with the sketch curve each profile curve lies on; identifies the region even when `profile_index` cannot be resolved), `area`, `centroid` (of the region, for cross-checking) |
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
`{"_truncated": true}`.

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
| `name` | timeline item name |
| `objectType` | short type of the item's entity (`Sketch`, `ExtrudeFeature`, `ConstructionPlane`, `Occurrence`, `Joint`, ...), null if unknown |
| `entityToken`, `component` | of the entity; `component` is the owning component's name |
| `isRolledBack` | at the document's original marker position |
| `isSuppressed`, `healthState`, `errorOrWarningMessage` | read with the marker at the end; `healthState` is the health state's name |
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
  (reference, for projected/included curves with `isReference`).
- `Constraint`: `id`, `type` (`CoincidentConstraint`,
  `HorizontalConstraint`, `TangentConstraint`, ...), `refs`, `props`.
- `Dimension`: `id`, `type` (`SketchLinearDimension`,
  `SketchDiameterDimension`, `SketchAngularDimension`, ...), `parameter`
  (parameter reference; null for some driven dimensions, K6), `isDriving`,
  `value` (internal units, cm or rad; for driven dimensions the only
  value), `textPosition` (vec3, sketch space; selects the quadrant of
  angular dimensions, K3), `refs`, `props`.
  - `refs`: every property of the constraint or dimension whose value is
    an entity, keyed by property name (`point`, `entity`, `lineOne`,
    `curveTwo`, `entityOne`, ...). The value is a sketch-local id string
    for entities of this sketch, a list of those, or a reference object
    otherwise.
  - `props`: the remaining plain properties (enum names, numbers, bools).
- `Text`: `id`, `text`, `height`, `position`, `fontName`, `angle`,
  `isHorizontalFlip`, `isVerticalFlip`, `textStyle` (raw value),
  `boundingBox`, `definition?`.
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
| ExtrudeFeature | operation, profile, extentType, extentOne, extentTwo (`ToEntityExtentDefinition`: entity, isChained, offset, isMinimumSolution, directionHint; `ThroughAllExtentDefinition`: isPositiveDirection; `SymmetricExtentDefinition`: distance, isFullLength, taperAngle), hasTwoExtents, symmetricExtent, startExtent, taperAngleOne, taperAngleTwo, participantBodies, isSolid, isThinExtrude, thinExtrudeWallLocationOne/Two, thinExtrudeWallThicknessOne/Two |
| RevolveFeature | operation, profile, axis, isProjectAxis, extentDefinition (angle, isSymmetric / angleOne, angleTwo / to-entity), isSolid, participantBodies |
| HoleFeature | holeType, holeTapType, holeDiameter, tipAngle, counterboreDiameter, counterboreDepth, countersinkDiameter, countersinkAngle, isDefaultDirection, extentDefinition, position, direction, holePositionDefinition, participantBodies, tappedHoleInfo (a ThreadInfo), clearanceHoleInfo, thread (the hole's ThreadFeature: isModeled, isFullLength, threadLength, threadOffset, threadLocation, isRightHanded) |
| ThreadFeature | threadInfo (isInternal, isRightHanded, isTapered, threadType, threadSize, threadDesignation, threadClass, majorDiameter, minorDiameter, pitchDiameter, threadPitch, threadAngle, ...), isModeled, isFullLength, threadLength, threadOffset, threadLocation, isRightHanded, inputCylindricalFaces, inputCylindricalFace, hole |
| FilletFeature | filletFeatureType, edgeSets (`ConstantRadiusFilletEdgeSet`: edges, radius, isTangentChain, continuity, tangencyWeight; variable radius: startRadius, endRadius, midRadii, midPositions; chord length; asymmetric), isRollingBallCorner, ruleFilletSettings, fullRoundFilletFaceSets; retired isG2, isTangentChain |
| ChamferFeature | edgeSets (distance / distanceOne, distanceTwo / distance, angle; isFlipped, isTangentChain), cornerType; retired chamferType, chamferTypeDefinition, edges, isTangentChain |
| ShellFeature | inputEntities, insideThickness, outsideThickness, isTangentChain, shellType |
| DraftFeature | inputFaces, plane, isTangentChain, isDirectionFlipped, draftDefinition (angles, symmetry); preview draftType, partingLineType, partingLineCurves, movingPartingLineDirection, movingPartingLineFixedEdges |
| SweepFeature | profile, path, guideRail, guideSurfaces, operation, orientation, distanceOne, distanceTwo, taperAngle, twistAngle, isSolid, profileScaling, extent, isDirectionFlipped, isChainSelection, participantBodies, solidBody, solidOrientation, solidAlignedAxis, solidTwistAxis |
| LoftFeature | loftSections (entity, index, endCondition), centerLineOrRails, centerLineOrRails.isCenterLine, operation, isSolid, isClosed, isTangentEdgesMerged, startLoftEdgeAlignment, endLoftEdgeAlignment, participantBodies |
| PipeFeature | path, sectionType, sectionSize, sectionThickness, isHollow, operation, distanceOne, distanceTwo, participantBodies |
| CoilFeature, RibFeature, WebFeature | no inputs are recorded; their model parameters (`parameters.model`: `role`, `createdBy`) give the sizes |
| RectangularPatternFeature | inputEntities, patternEntityType, directionOneEntity, directionTwoEntity, directionOne, directionTwo (vectors), quantityOne, quantityTwo, distanceOne, distanceTwo, patternDistanceType, isSymmetricInDirectionOne/Two, patternComputeOption, suppressedElementsIds |
| CircularPatternFeature | inputEntities, patternEntityType, axis, quantity, totalAngle, isSymmetric, patternComputeOption, suppressedElementsIds |
| PathPatternFeature | inputEntities, patternEntityType, path, quantity, distance, startPoint, patternDistanceType, isFlipDirection, isOrientationAlongPath, isSymmetric, patternComputeOption, suppressedElementsIds |
| MirrorFeature | inputEntities, mirrorPlane, patternComputeOption, isCombine, stitchTolerance |
| CombineFeature | targetBody, toolBodies, operation, isKeepToolBodies, isNewComponent |
| SplitBodyFeature | splitBodies, splittingTool, isSplittingToolExtended |
| SplitFaceFeature | facesToSplit, splittingTool, splitType, directionEntity, isSplittingToolExtended |
| MoveFeature | inputEntities, definition (its subtypes: transform / xDistance, yDistance, zDistance, isDesignSpace / axisEntity, angle / ...), retired transform (the rigid matrix) |
| CopyPasteBody | sourceBody |
| OffsetFacesFeature | inputFaces, distance |
| ThickenFeature | inputFaces, thickness, isSymmetric, operation, isChainSelection |
| ScaleFeature | inputEntities, point, scaleFactor, isUniform, xScale, yScale, zScale |
| ReplaceFaceFeature | targetFaces, isTangentChain; sourceFaces and inputFaces when a writer records them (otherwise the import finds the replaced faces from the file's history) |
| DeleteFaceFeature | deletedFaces |
| RemoveFeature | itemToRemove |
| OffsetFeature, PatchFeature, StitchFeature, ExtendFeature, TrimFeature, RuledSurfaceFeature, SilhouetteSplitFeature, BoundaryFillFeature | their input entities and parameters (unverified) |
| BaseFeature | sourceBodies |
| ConstructionPlane | definition (e.g. `ConstructionPlaneOffsetDefinition`: planarEntity, offset), geometry, transform, isParametric |
| ConstructionAxis, ConstructionPoint | definition, geometry, isParametric |
| Occurrence | component, fullPathName, transform2, transform (retired), initialTransform, isGrounded, isGroundToParent, isReferencedComponent, isLightBulbOn, isVisible |
| Joint | occurrenceOne, occurrenceTwo, geometryOrOriginOne, geometryOrOriginTwo, jointMotion, offset, angle, isFlipped, isLocked |
| AsBuiltJoint | occurrenceOne, occurrenceTwo, geometry, jointMotion, offset |
| JointOrigin | geometry, offsetX, offsetY, offsetZ, angle, isFlipped, xAxisEntity, zAxisEntity |
| RigidGroup | occurrences, includeChildren |
| Canvas | imageFilename, opacity, isDisplayedThrough, planarEntity, transform |
| any other type | `{}`; everything is in `props` |

Detail keys are left out of `props`, so nothing is repeated. The stream
decoder writes the main fields of the types it decodes; what the import
reads is listed in `core/model/src/api/commands.md` (*From the dump IR*).

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
