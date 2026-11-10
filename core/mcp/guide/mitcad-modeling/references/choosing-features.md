# Choosing features

<!-- SPDX-License-Identifier: MIT -->

Read this when several features could make the same shape. Choose the
construction whose editable dimensions and references explain its
purpose. Look up the chosen feature with `model_reference` for its
actual fields, operations and restrictions; this table is a decision
guide, not a replacement for the reference.

## Common choices

| Need | Prefer | Reason and boundary |
|---|---|---|
| Simple rectangular or round stock | `box`, `cylinder`, `sphere` or `torus` | A primitive keeps a few dimensions direct. Place it through references and parameterized offsets. |
| A section with related edges or openings | `sketch` followed by `extrude` | Constraints describe the section; the extrusion describes its depth. |
| A turned shape with steps or a groove | A constrained profile and `revolve` | The radial section expresses diameters and axial distances together. A plain cylinder is enough for simple stock. |
| A drilled, counterbored, countersunk or tapped hole | `hole` | It records hole intent, placement, size and extent. Use a sketch cut for a nonstandard opening it cannot express. |
| Thread specification for a manufactured item | Cosmetic `thread` | Retains the designation without expensive helical geometry. Choose its face and length deliberately. |
| Thread geometry required in an exported mesh | Modeled `thread` | Set the modeled option after checking the reference and intended process. A cosmetic thread does not cut a mesh thread. |
| A constant arbitrary section following a route | `sweep` | Section and path express the result; guides and orientation are explicit choices. |
| A round tube following a route | `pipe` | Directly describes the path and circular section, including wall size when hollow. |
| A regular helix or spring | `coil` | Radius, pitch and turns express the helix. Use a sweep when the section or path needs its extra control. |
| A changing section | `loft` | Several profiles describe the change; choose correspondence and rails deliberately. |
| A screw post on a housing | `boss` | Combines a round post, hole and optional ribs/draft. A boss cannot itself be patterned or mirrored: use separate bosses or a construction from supported features for repetition. |
| A plain post or custom reinforcement | Sketch plus `extrude`, or `rib` / `web` | Use an explicit construction when the boss options do not describe the shape. |
| Symmetry of solid geometry | `mirror` | A feature, body or component mirror keeps 3D symmetry explicit. |
| Symmetry inside a profile | `sketch.mirror` | The section's copied geometry follows its symmetry axis. |
| A feature adding or removing material immediately | The feature's operation, such as the operation of `extrude` | Join or cut the intended participants directly instead of creating unnecessary temporary bodies. |
| Boolean work between existing bodies | `combine` | Select target, tools, operation and whether tools remain. Verify the remaining bodies. |
| Move a body or create a positioned body copy | `move` | Transforms bodies; a copy is distinct geometry, not an occurrence of a shared component. |
| Edit selected faces locally | `move_face` | A deliberate shape edit, not an assembly placement or a substitute for changing the source dimension. |
| Place an assembly item | `move_occurrence` or a joint | Move an occurrence for placement; use a joint for a lasting assembly relation or motion. |
| Regular copies along directions, an axis or a path | `rectangular_pattern`, `circular_pattern` or `path_pattern` | Quantity, spacing or angle express the repetition. Choose feature, body or component objects consciously. |
| Copies at individual reference points | `pattern_at_points` | Each target adds a translated copy; originals remain. Its automatic or explicit anchor determines placement. |
| Rounding that defines the profile | `sketch.fillet` | The section carries tangent arcs and radius dimensions. |
| Finishing solid edges | `fillet` or `chamfer` | Keep finishing late and avoid making later features depend on its faces. |

## Bodies, components and occurrences

Keep one manufactured item in a component unless its purpose calls for
several bodies. Several bodies in one component are not automatically
independent assembly items. Occurrences of one component share geometry;
a body copy has no such meaning.

Create or activate the intended component before adding its sketches and
features. Check `components` and `document` for the active component and
the occurrence tree and placements. Use `instances` for placed bodies:
`bodies` reports component-local geometry while an occurrence places it
in the design.

Honor `policy` and `sketch_supports` results. Do not relax restrictions
just because they refuse a shortcut. Choose an allowed reference or
explain why the design structure needs an explicit policy decision.

## Typical plans

- **Turned part:** named diameters and axial lengths, an origin-based
  constrained half-section, `revolve`, functional holes or grooves, then
  edge finishing and threads. Check the axis and radial sizes.
- **Plate or bracket:** a section tied to the origin, `extrude`, holes
  located from functional references, then finishing. Use sheet-metal
  features for a folded sheet that needs a flat pattern.
- **Housing:** outer shape, `shell` with the intended opening, posts or
  `boss`, mating lip/groove and clearance, then finishing. Place details
  from stable base geometry rather than shell edge indices.
- **Tube or handle:** a dimensioned connected route, `pipe` for a round
  section or `sweep` for a custom one, referenced attachments, then
  finishing. A changing section calls for `loft`, not many cylinders.
- **Spring:** choose `coil` dimensions and section; check pitch, turns,
  axis and end requirements. Do not construct each turn independently.

Read [sketches](sketches.md) before making a profile or route and
[verification](verification.md) after each stage. If a feature cannot
preserve a requested detail, revise the construction and keep the detail
in scope.
