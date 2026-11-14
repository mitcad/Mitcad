# mitcad-model-drawing

The drawing layout and writers of the MitCad model, a crate of its own
above `mitcad-model` (mitcad#777): a drawing's views laid out on a sheet
with their dimensions, annotations, hole tables, parts lists, frame and
title block, and the sheet written as PDF, SVG, DXF or DWG. No geometry
library: the kernel projects the views (`Kernel::project_view`) and the
JSON api (`mitcad-model-api`, its `drawing*.rs`) asks this crate to lay
them out.

| Module | What it does |
|---|---|
| `compose` | The layout of a sheet: views in their places, their marks, the automatic annotations, the frame and the title block |
| `clip` | Detail views and breaks cut in the view's plane |
| `convention` | Drawing conventions on the projected lines (mitcad#1112): fillets and chamfers smaller than the pen as sharp corners, tangent and short edges that would blot left out, threads drawn by ISO 6410 |
| `annotate` | Annotations on views laid out from their picks |
| `tables` | Hole tables and parts lists drawn from their rows |
| `template` | Sheet templates drawn, with their title blocks' images (mitcad#1625); the built-in ones (`templates/*.json`) and the user's and projects' libraries |
| `sheet` | The sheet's groups, pens (the one table of ISO 128 line widths, also for the application's sheet) and texts that the writers write |
| `pdf`, `svg`, `dxf`, `dwg` | The writers |
| `pdf_image` | A title block's images (mitcad#1625) as PDF images: a PNG decoded with its transparency as a soft mask, a JPEG as it is; the exports' warnings for what a format leaves out of the images |
| `svg_image` | A title block's SVG images (mitcad#1627) read with usvg into paths of lines and cubic Béziers with solid colours, opacities, strokes and clip paths, their text as its glyphs' outlines (mitcad#1637: the system's fonts and the bundled Droid Sans, `svg_image::fonts`), fitted in their box as the view draws them: the PDF writer's vector paths, the DXF and DWG writers' lines (flattened within 0.01 mm) |
| `cut_sheet` | Cut sheets (mitcad#853): a part's outline as the body's silhouette from its projection, parts placed, turned and checked for overlaps, the DXF and SVG of their outlines |
| `bspline` | B-splines in a plane evaluated, cut, turned round and split into Bézier pieces, so cut sheets write the projection's curves as they are |
| `lib.rs` | The projection cache in the document's side cache (`DrawingCache`) |

What a design file keeps of a drawing stays in the model
(`core/model/src/drawing`): the definition (`def.rs`) with its stored
annotations (`annotate_def.rs`), table settings (`tables_def.rs`) and
templates and revisions (`template_def.rs`), a cut sheet's material and
parts (`cut_sheet_def.rs`), and the options of
`export_drawing`. This crate re-exports all of it under the same paths,
so `mitcad_model_drawing::…` names everything of drawings.

Where code goes: a new stored field or its check into the model's
`drawing/`; layout and writing here; a command into the api crate. Tests
of the layout without a document live beside the code here
(`core.model_drawing`); tests through the JSON api in
`core/model-tests/drawing` and `drawing-views`. One-off pictures of a
layout come from an example of this crate run by a `tools/look-*.sh`
script (`examples/radius_leader_look.rs`, a radius's arrow on its leader
round a small fillet, mitcad#1770: `tools/look-drawing-radius-leader.sh`).
