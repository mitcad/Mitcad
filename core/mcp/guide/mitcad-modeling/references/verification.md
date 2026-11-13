# Verification

<!-- SPDX-License-Identifier: MIT -->

Read this after every modeling change and before reporting completion.
Check both model data and visible geometry. A valid volume or a good
picture alone does not prove that the requested design is complete.

## Health

Run `design_review` through `model_query` after each stage. Inspect both
findings and errors for checks that could not run:

```json
{"query": "design_review", "scope": "all", "print": "auto"}
```

Scope to a component or an explicit feature-uid list when checking a
bounded change; select rule ids with the checks list. Automatic print
checks require a bed face, plate or printer profile. Set print to false
for inexpensive model checks, or true to request print analysis explicitly.
Each finding links a guide topic and anchor. An unavailable check does not
mean the design passed; repair the cause and query again. Review the
reported print context and the exported plate layout before claiming fit.

Read the command result, then `timeline`. Check changed features and
their downstream consumers for errors, warnings, suppression and
rollback. A suppressed or rolled-back feature is not a completed visible
detail. Read `feature` for failing definitions and notices instead of
treating successful tool transport as a successful model change.

An added or edited feature can fail after being applied and remain in
the timeline. A refused command array instead rolls back its earlier
entries. Read the error, stopped entry and failed features to distinguish
the cases. Inspect the actual state before retrying.

For a "found as" warning, inspect the recovered reference and use
`auto_fix` when its association is intended; check remaining problems.
Other warnings may require a definition edit. Report any warning
intentionally left and its practical effect. See
[references](references.md#warnings-and-repair).

## Sketch constraints

Query `sketch` for each sketch created or changed: it must solve, have
the intended connected curves and profiles, and normally have `dof` 0.
Check redundant constraints, collapsed curves and projections that no
longer follow. Fixing every position to remove degrees of freedom is
not equivalent to a dimensioned sketch.

Read `profiles` for the region a feature consumes. After a sketch edit,
check that profile references still select the intended area; region
count can change while most curves look unchanged. See
[sketches](sketches.md#constraints).

## Measure the result

Read `document` and `bodies` for expected body counts. Request body
properties for volume, area, kind and bounding box, or volumes/boxes
alone when enough. A solid item needs positive volume and the intended
solid kind; a surface is appropriate only when requested. A join can
leave disconnected solids in one body, so body count alone does not
prove physical connection. A cut that splits a body leaves its extra
pieces as new bodies with default names: name the wanted pieces and join
or remove the others (the review's stray-bodies rule).

Compare volume change with the operation: cuts should remove the
intended material and additions should join the intended target.
Plausible overall dimensions can hide missing holes, unwanted pockets
or floating material. Check important details with `measure`, `faces`
and `edges`; compare bounding-box extents with required dimensions.

For assemblies, read `components` and `instances`.
Placed instances give the context for positions and clearances; body
properties alone are component-local. Use `interference` for relevant
bodies where overlap should be absent and inspect any overlap. An empty
result does not prove the intended clearance or contact: measure those.

Read `parameters` to verify that named sizes drive geometry. For a
parameter with a min and a max, run `parameter_range`: it evaluates the
design at both limits (or at given `values` or `steps`) on a copy and
reports per value the bodies' volumes and boxes, the body count, failed
features and new warnings against the current value, without changing
the document or its undo history:

```json
{"query": "parameter_range", "parameter": "Width", "steps": 3}
```

A limit at which a profile splits, a feature fails or a body appears is
a limit to fix: lower the max or raise the min until both ends build
the intended shape. For other adjustable sizes, try a reasonable
`set_parameter` change, inspect recomputation, then restore the original
expression. Preserve the existing design's values and undo history
deliberately; do not leave a trial dimension as the final result.

## Look from several sides

Inspect two to four complementary views: an isometric overview and the
directions revealing holes, grooves, wall openings and assembly
relations. In the live application use `ui_camera` and `ui_screenshot`.
Use `model_render` for a headless design or reproducible model picture.
Look up camera and body-selection fields first. Include the bodies under
review; another body must not hide a missing or misplaced detail.

Pictures reveal shape and placement errors aggregate numbers miss.
Numbers establish sizes that perspective and pixels cannot. Never
estimate a critical thickness, angle, gap or diameter from an image when
queries can measure it. Use a section or another view for internal
features.

## Recover from the cause

Stop adding dependents to a failed stage. Read its error, definition,
inputs and last good geometry. Prefer editing the offending dimension or
reference, or `undo` to a good stage and a corrected construction. Do not
stack patches, duplicate bodies or tiny offsets to conceal failure.

In a live shared design, respect user edits and undo boundaries; never
undo unrelated work. A batch's one undo step helps with a bounded stage
only when its inputs and ids were already known.

A dialog open in the live application waits for the user: edits are
refused until it is answered, and the refusal and the dialog in
`ui_state` name it. Ask the user to answer it; do not delete files to get
past it. When you start MitCad yourself to check your work, start it with
`--no-recovery`, so that the question about an earlier session's unsaved
work does not stop your edits at start-up.

## Finish with evidence

Recheck the final timeline, sizes, body/component structure and every
requested detail. Name features and group adjacent stages as described
in [principles](principles.md#organisation). Save through the permitted
document tool when requested, using the requested path and approval
flow. Report what was built, verified and genuinely left. Never claim a
missing detail, unresolved failure or unperformed physical check passed.
