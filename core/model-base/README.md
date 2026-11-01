# mitcad-model-base

The base of the MitCad model (mitcad#777): the geometry `Kernel` trait
(`src/kernel.rs`) with the data it takes and gives, and the plain data
and helpers of the model that need nothing of its core.

| Module | What it holds |
|---|---|
| `kernel` | The `Kernel` trait and its input and output types |
| `exchange`, `drawing_view`, `sheet_charts`, `segments`, `curves` | Kernel data moved down from the model's `exchange`, `drawing`, `sheet` and `features`: bodies and file options, projected views and round holes, sheet metal material charts, named profile segments, points and ends of 3D curves |
| `ids`, `topo`, `profile`, `datum`, `transform` | Ids, topological names, profiles, datums and transforms |
| `sweeps`, `surfaces`, `surface_modelling`, `boundary`, `analysis` | Specifications of sweeps, surfaces and boundary bodies, analyses |
| `parameters`, `properties`, `configurations` | Parameters, typed properties, configuration tables |
| `appearance`, `render_settings` | Appearances and render settings |
| `material_class` | Material classes of materials and appearances (mitcad#1102) |
| `monitor`, `fingerprint`, `sha256`, `base64`, `iso286` | Recompute progress, result fingerprints, hashes, Base64, ISO 286 fits |
| `values`, `occurrence` | Command values (`ValueInput`, `resolve_value`) and the model's command error (`ModelError`); occurrence paths and links to other components' geometry (`OccurrencePath`, `OccurrenceLink`), moved down for the sketch crate (mitcad#780) |
| `image_header` | The format, size and pixel size of PNG, JPEG, BMP and WebP files from their headers, and of HEIF files' HEVC still images as shown (`irot`, `clap`; mitcad#850) (image references, mitcad#782) |

`mitcad-model` re-exports every module at its old path
(`mitcad_model::kernel`, `mitcad_model::exchange::LengthUnit`, …), so the
model and the crates above it name them as before. A module goes here only
when it needs nothing of the model's core; methods of these types that
need the core live in the model as extension traits (the sheet adapter's
`BoundedCurve2Ext`, for example).

An edit in the model's core does not recompile this crate. The mock
kernel (`mitcad-model-testkit`) depends on this crate alone.

The modules' own tests (`cargo test -p mitcad-model-base`) run as the
ctest `core.model_base`.
