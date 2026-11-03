# mitcad-sketch

The general sketch of MitCad (mitcad#780): 2D geometry, entities,
constraints and dimensions, the solve (through `mitcad-solver`), profile
regions, projections, offsets, patterns, equal spacing, text (fonts,
shaping, QR codes), SVG and DXF drawings, and the edits the sketch
commands make (`SketchEdit`). It sits below the model's core, on
`mitcad-model-base`, `mitcad-expr`, `mitcad-solver`, `mitcad-dxf` and
`mitcad-vec3`.

| Module | What it holds |
|---|---|
| `lib.rs` | Entities, constraints, dimensions, projections, texts and their file forms |
| `def` | The sketch feature's definition (`SketchDef`, `SketchPlane`, `FrameDef`), its checks and solve, and its evaluated output (`SketchOutput`) |
| `geometry`, `intersect`, `regions` | 2D curves, their intersections, the profile regions |
| `solve` | The solve and its status |
| `edit`, `modify`, `tools`, `points`, `cleanup`, `stale`, `convert`, `axes` | Sketch edits (`SketchEdit`) |
| `fillet` | The tangent circle of a sketch fillet at a corner of a circular arc with a line or an arc (mitcad#1192) |
| `project` | Model geometry and other sketches' curves and points (`ProjectionSource`, `SketchEntityRef`, mitcad#904) projected into the sketch; linked projections that follow their sources |
| `offset`, `pattern`, `spacing` | Offsets, patterns, equal spacing |
| `text`, `fonts`, `code`, `svg`, `dxf` | Texts, font files, QR codes, SVG and DXF drawings |
| `legacy` | The shapes of early version 2 files |

The model re-exports the crate as `mitcad_model::sketch` and the
definition as `mitcad_model::features::sketch`, so code above the model
names it as before. What needs the document stays in the model
(`core/model/src/features/sketch.rs`): the sketch feature's references
and checks against the timeline (`FeatureInfo`), its evaluation
(`Evaluate`: the plane's frame, the sources of linked projections, which
`project::follow_links` takes as a closure), and the features it names
through links.

An edit in the model's core does not recompile this crate; an edit here
recompiles the model and what is above it.

The crate's own tests (`cargo test -p mitcad-sketch`) run as the ctest
`core.sketch`; tests through the JSON api are in
`core/model-tests/sketch` and `core/model-tests/sketch-refs`.
