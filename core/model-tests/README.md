# The model's tests through the JSON api

The tests that drive the MitCad model through its JSON api
(`command`, `preview`, `query`) live here, in small crates by area
(mitcad#777). They sit above `mitcad-model` and `mitcad-model-api`, so
that a test added to one area recompiles only that area's crate, not the
model, the api or the other tests. Each crate is a library whose only
content is its tests (`#![cfg(test)]`); its ctest is `core.model.<area>`.

| Crate folder | ctest | What it tests |
|---|---|---|
| `api` | `core.model.api` | Commands and queries in general: the document query, the browser, named views and DXF insert, small UI gaps and metric defaults, failed features, inserting at a timeline position, autoselected profiles, faces and edges, text parameters, parameter limits, the result caches and store, cancelling recomputes, preview placement, tables, the rolled-back features evaluated beside the document (mitcad#1267) |
| `appearance` | `core.model.appearance` | Appearances, texture maps, render settings, mesh export, typed properties, materials |
| `analysis` | `core.model.analysis` | Section analyses, Measure, plane geometry, measurement objects and annotations, image references |
| `sketch` | `core.model.sketch` | The general sketch: entities, constraints, dimensions, patterns, offsets, text, the sketch's axes, inactive constraints, one solve per edit, copy and paste |
| `sketch-refs` | `core.model.sketch-refs` | Regular polygons, equal spacing, driven references, dimension names, projections and their offsets, Convert to Sketch Geometry, assembly constraints, section curves, selected profile areas |
| `features-profile` | `core.model.features-profile` | Profile features: extrudes and their scopes, extrudes from faces, revolves and sweeps, holes (clearance holes by screw size) |
| `features-solid` | `core.model.features-solid` | Patterns and dressups, construction geometry, renamed references, automatic operations, surface modelling |
| `parts` | `core.model.parts` | Place At, plastic part features, printable bearings, pulleys, gears, seats, point patterns |
| `sheet` | `core.model.sheet` | Sheet metal |
| `assembly` | `core.model.assembly` | Components and occurrences, reparenting, Move Features into a Component, body transfer, the design policy |
| `joints` | `core.model.joints` | Joints, the joint solver, motion studies, motion links, mount frames |
| `links` | `core.model.links` | Geometry of other components in sketches, component libraries |
| `drawing` | `core.model.drawing` | Drawings: the sheet, stored drawings, templates and revisions, tables, the .dwg export |
| `drawing-views` | `core.model.drawing-views` | Drawing views: features, following the model, picks, annotations on views |
| `print` | `core.model.print` | Slicers' print settings files, purge volumes, printed fits, the print library and profiles, Print Analysis, the print bed face (and the ignored `3mf.corpus` test) |
| `files` | `core.model.files` | Import and export, the design file, design comparison, Reorient Design, import notes, tables in the file and the comparison |
| `build-frames` | `core.model.build-frames` | BUILD: frames, placement, closing, sides and ends, junctions |
| `build-timber` | `core.model.build-timber` | BUILD: timber, wall bearings and splices, fasteners, profiled boards, the cut list, bound lengths, library sources |
| `imports` | `core.model.imports` | The imports' tests that need the api (mitcad#831): the `.ipt` reader's test designs replayed by the import, the IFC import's property set levels, the `.solidcraft` import's contact sets, motion studies and motion links |
| `support` | — | Not tests: BUILD's test library of synthetic parts, shared by the BUILD crates |

## Where a new test goes

- Through the api: into the crate of its area; a file of its own is a
  `mod` line in the crate's `lib.rs`. Keep a crate at a few thousand
  lines: an area that grows past that splits into two crates.
- A new area: a new folder here with a `Cargo.toml` like its neighbours',
  a member in `core/Cargo.toml` and a
  `mitcad_cargo_test(core.model.<area> mitcad-model-tests-<area>)` line in
  `core/CMakeLists.txt`.
- Without the api (a module's own logic): beside the code in
  `core/model` (`core.model`); the drawing layout's beside its code in
  `core/model-drawing` (`core.model_drawing`); the sketch's beside its
  code in `core/sketch` (`core.sketch`).
- BUILD: through the api into `build-frames` or `build-timber`; BUILD's
  planning itself lives in `core/model-build`, which has no tests of its
  own (import `mitcad_model_build::prelude::*` for `part_info`,
  `build_junctions` and the other BUILD methods).
- An import's test: beside the import (its crate under `core/import-*`,
  `core/ipt`) when it checks what the import made through the model's
  own types (the report, `Document`'s features, parameters, bodies and
  properties), so that the import's tests need neither the api nor, for a
  reader that does not depend on the model, the model; into `imports`
  when it needs the api's commands or queries (mitcad#831). An import
  crate keeps `mitcad-model-api` out of its `[dev-dependencies]`: a
  change in the api then rebuilds `imports` alone, not each import's
  tests. (`core/import` and `core/vcs` keep it: the replay runs the api's
  commands its callers pass in, and the version history's tests make
  their designs with the api's commands.)
- A crate without tests of its own sets `test = false` in its `[lib]`
  and has no ctest, so that a change in the model does not build and
  link an empty test binary for it (mitcad#831); its first test removes
  that line and adds its `mitcad_cargo_test` line.

The tests name the model as `mitcad_model::…`, the drawing crate as
`mitcad_model_drawing::…` and the api as `mitcad_model_api::…`
(import `DocumentApi` or `mitcad_model_api::prelude::*` for `command`,
`preview` and `query`; `mitcad_model_drawing::DrawingCache` for the
document's projection cache). Shared fixtures: the mock kernel
(`mitcad_model::testing`, the crate `core/model-testkit`) and `mitcad_model::test_support` (the block
document, feature definitions, face and edge names, `allow_attachments`),
both under the model's `testing` feature; model items that only tests use
are public there and hidden from the docs.
