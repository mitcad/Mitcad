# References

<!-- SPDX-License-Identifier: MIT -->

Read this before placing a sketch, primitive, component or feature and
when upstream edits affect its dependents. A reference should say what
the placement follows, not just where it happened to be.

## Placement

Choose an origin datum, a construction datum derived from stable
geometry or a functional base face/axis/point when it describes the
intended relation. A mounting hole normally follows its mounting face
and offsets; a part on a shaft follows its axis and seat. A position
chosen only to match today's picture hides that relationship.

Sketches use their plane reference. Primitives and supported standard
features can use their `at` placement. Look up the selected definition:
axis, point and face placements have different offsets and alignment
rules. Do not supply an `at` placement with a conflicting legacy plane
or centre.

Reference forms include origin planes "xy", "xz", "yz", axes "x", "y",
"z", the point "origin", a construction feature uid, or an object naming
a body's face, edge or vertex. Sketch points and curves can also provide
applicable datums. Use `faces`, `edges` or `sketch` to obtain actual
references and geometry before selecting them.

A plane `{"origin": [...], "normal": [...]}` is fixed in its coordinate
system; it does not follow a model face when that face moves. Use it
deliberately for an independent section, not as a shortcut for a missing
face lookup. Do not replace a failed association with coordinates
without deciding that the fixed relation is what the user wants.

Check frames and signs. A face plane uses its outward normal; in-plane
axes need not match the picture. Origin datums have documented frames:
XZ's second coordinate is along negative Z. Use placement alignment and
flip controls when appropriate, then inspect and measure the result.

## Stable parents

Prefer parents that survive expected edits: an origin or deliberate
construction plane, a major base-extrusion face, a layout sketch or a
functional cylinder's axis. Keep reference direction readable in the
feature order and avoid unnecessary dependency chains.

Avoid fillet and chamfer faces/edges as parents for new functional
features. Their sizes, counts and boundaries change with finishing
radii and nearby geometry. Pattern-instance faces are fragile when the
quantity or ordering changes. Use the original feature, a layout or a
datum expressing the repetition. Deliberately modifying a finishing
face can still be appropriate when that is the requested purpose; do not
use it as an unintended foundation.

Mirror about the origin or construction plane expressing the symmetry,
not a temporary face that happens to be at the midpoint. Derive a moving
symmetry plane from the size or references it should follow.

## Topological names

Use returned names rather than screen positions or tessellation indices.
Feature uids are persistent and separate from display names. Faces carry
the creating feature and semantic role; edges are named by their meeting
faces. A split's suffix identifies a piece; omitting it can mean several
pieces. Check what the command accepts before using a partial name.

After a topology-changing edit, query faces and edges again. Do not
assume a list index still identifies the same surface. Preserve the
intended association, not just the first surface with a similar normal.

## Warnings and repair

A "found as" warning means MitCad recovered a reference under another
name after a change. The feature can succeed with the old name still in
its definition. Read its warnings/notices, verify the recovered geometry
and use `auto_fix` for selected feature uids when that association is
intended. Inspect changed and remaining problems: it cannot repair every
missing or ambiguous parent.

When candidates are equally plausible or the geometry is gone, the
feature should fail rather than choose arbitrarily. Inspect suggested
candidates with queries, then edit the reference deliberately. Undo an
incorrect upstream edit when that better preserves the model. Do not
hide an unresolved warning by adding replacement solids over it.

## Components and policies

References are interpreted in their owning component and assembly
context. Occurrences move geometry into the design and instances of one
component can have different placements. Use `instances` and occurrence
placements across items; do not compare component-local coordinates as
world positions.

Read `policy` and `sketch_supports` before cross-component support or
projection links. A refusal is a design boundary, not an invitation to
disable a rule. Choose permitted geometry or resolve the structure
explicitly. Keep dependencies acyclic: create sources before consumers
and avoid reference loops between components.

Follow [verification](verification.md) after placement and after changes
to an important upstream reference.
