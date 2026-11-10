# Principles

<!-- SPDX-License-Identifier: MIT -->

Read this before building or changing a design. A good model describes
what the user intends to keep related when dimensions change. A picture
that looks right once is only part of that result.

## Intent before geometry

1. Read the request, existing design and open comments. Use `document`,
   `timeline`, `parameters` and `components` through `model_query`; do
   not assume an empty root or that the active component is the target.
   Address pinned requests with their modeling changes as the connected
   server instructs.
2. Identify important dimensions, mating or mounting references, symmetry,
   separate items and expected final bodies. Distinguish requirements
   from assumptions. Clarify a missing fact when it changes the intended
   result; make ordinary implementation choices yourself.
3. Describe a short plan: references, main shape, functional details,
   finishing features, then verification. Look up unfamiliar features
   and their fields with `model_reference` before using them.

Keep every requested hole, groove, clip, relation and component in that
plan. A modeling failure is a reason to repair the construction or report
the remaining problem. Do not omit a detail, shrink its intended size,
or silently change a requirement to obtain a successful tool result.

## Parameters

Create a user parameter with `add_parameter` for a dimension the user is
likely to change, a shared size or a relation that explains the design.
Use names such as PlateThickness, HoleSpacing and Clearance. Express
dependent sizes in terms of them: an outside diameter can follow a bore
and twice its wall thickness. Do not copy the same literal into each
related feature.

Read `parameters` to check stored expressions, units, values and
dependencies. JSON numbers mean millimeters for lengths and radians for
angles even when the design displays another unit. Text such as
"0.25 in" or "90 deg" avoids ambiguity. Do not give both a value and an
expression in one parameter command.

Use parameters in actual driving fields, including sketch dimensions and
placements; an unused parameter does not make a model parametric. Remove
unused experimental user parameters with `delete_parameter` when safe,
or explain their deliberate purpose. Before changing a shared parameter,
inspect its dependencies. Test reasonable intended changes and restore
the original expressions after checking them.

## Feature order

Build references before their consumers. Prefer origin datums,
construction geometry and important base faces for parents. Make the
core volume, then functional cuts, holes, posts and ribs. Apply patterns
and mirrors at the level that expresses their purpose. Leave edge
finishing with `fillet`, `chamfer` and detailed `thread` features late
when it is not essential to the main shape.

A rounded edge is a poor parent for the next sketch: changing its radius
can replace that face. A datum tied to the base shape usually explains
the relation better. Read [references](references.md) before choosing
support geometry.

Use `model_command` arrays for a small, coherent stage whose inputs are
already known. An array is one undo step. Query between stages to obtain
new ids and verify the result before building on it. A refused array
rolls back its earlier entries; a feature that fails after being applied
remains in the timeline. Inspect and repair a failed stage before
continuing.

## Organisation

Use `rename_feature`, `rename_body` and `rename_component` to name the
intent: Mounting holes, Main housing, Cover. Do not leave every feature
with a default name such as Extrude3. Keep returned ids for references:
display names are separate and feature names need not be unique.

Use `group_features` to group adjacent timeline features into meaningful
stages such as Base shape, Mounting and Edge finishing. It takes feature
uids from a contiguous run. Do not reorder features merely to form a
group if that would break dependencies, or hide an unfinished failure
inside a group.

A body is geometry; a component expresses an item in the design; an
occurrence places a component in an assembly. Give independent items
components and reuse a component with occurrences for the same item.
Keep the active component and its policy explicit. Read
[choosing-features](choosing-features.md) for these choices.

## Verify each stage

After a change, read `timeline`, check sketch degrees of freedom and
inspect the expected body count, volume and bounding boxes. Verify
important dimensions with queries and pictures from several directions.
One successful command or plausible silhouette is not a complete check.
Follow [verification](verification.md) after each stage and report any
requirement that remains unmet.
