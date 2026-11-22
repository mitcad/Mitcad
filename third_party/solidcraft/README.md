# SolidCraft: format upgrade steps, cam geometry and plastic rules

The `.solidcraft` import (`core/solidcraft`, mitcad#450) reads designs of
every SolidCraft file format by first upgrading an older file to the one
format it maps (63, the format of release 0.45.0; mitcad#1331,
mitcad#1994, mitcad#2153), and reads a design a newer build saved as far
as it can,
with a warning. The upgrade
steps are SolidCraft's own, adapted here. The cam geometry of motion links
(`cam.rs`, mitcad#466: where a follower touches a cam's profile over a
turn) is adapted from SolidCraft too and built into `core/model`
(`motion_links.rs`). The plastic rule library (`plastic.rs`, mitcad#459)
is SolidCraft's too, as data the import maps onto MitCad's plastic
rules.

- Source: https://github.com/tangency-labs/solidcraft.
  - `format.rs`: release `v0.45.0`, commit
    `e8ce12ef78fe76281a04b7242a9ce059ff7d5067` (19.11.2026, the newest
    release tag on 21.11.2026; it has no pending format changes:
    `crates/doc/src/format/pending/` holds only its README). Before
    mitcad#2153 it was release `v0.43.0`, commit `8ba46e9e` (format 57),
    and before mitcad#1994 release `v0.40.0-rc.2`, commit `777f9609`
    (format 50).
  - `cam.rs`, `mesh_convert.rs` and `plastic.rs`: commit
    `0b787f8eae4244cd40f07dddccd2e1b27e4cf9f8` (19.10.2026, after release
    0.16.2).
- Licence: MIT OR Apache-2.0, Copyright (c) 2026 SolidCraft contributors;
  used under MIT. Both licence texts are here (`LICENSE-MIT`,
  `LICENSE-APACHE`, copied from the repository). `format.rs`, `cam.rs`
  and `plastic.rs` keep the licence and copyright in their headers.
- Nothing of SolidCraft is a dependency: no crate of it is built or linked.

## Files

| Here | From | SHA-256 of the source file |
|---|---|---|
| `format.rs` | `crates/doc/src/format.rs` at `e8ce12ef` (`v2_to_v3`, `v3_to_v4`, `v4_to_v5`, `v5_to_v6`, `v13_to_v14` and their helpers, unchanged since `777f9609`) | `fa288085de4af1548a136a3bd2181b604261d7012e4292ab01faf8fa28da9e60` |
| | `crates/format/src/lib.rs` at `e8ce12ef` (`Formats::upgrade`, `Formats::parts`, `Family::newer`, `Family::canonical_applied_name`, `Newer`; unchanged since `8ba46e9e`) | `145b385301523c97a90a3d0d1752c70218aa0f7dcf87edf0a6867c7ff82975bd` |
| | `crates/doc/src/newer_file.rs` at `e8ce12ef` (`salvage`: what of a newer design is read and what is left out; unchanged since `8ba46e9e`) | `0faec28e7fad487e064ca94df878c7236abc5df6b11c2852ae88ce9ac89bb075` |
| | `crates/doc/src/assembly.rs` at `e8ce12ef` (`mat_mul`, `mat_inverse`, `transform_through`, `is_placement`) | `6c5ace5f5081bd4240f21547f9b1475396a92d88ba7bc61c81260a64d0cf267b` |
| | `crates/doc/src/face_anchor.rs` at `e8ce12ef` (`plane_face_anchor`, `FaceAnchor::author_frame`) | `933a78e34c43f35fde9f9f243dbc857edf25724e0e6bc76a03a8442bae6e7f37` |
| | `crates/doc/src/document.rs` at `e8ce12ef` (`EmbossFace::component_frame`, `Document::placement_frame`) | `62a917cf492ff64c0105fc56dd2b5fe6e8a229a27d381c61989c22d7ef887e1c` |
| | `crates/doc/src/project.rs` at `e8ce12ef` (`source_sketches`, `MAX_SKETCH_PLACEMENTS`) | `3b3d72a4d8f4c075b56f56071ceda4274ae500d5b27962e52abd0d9c2432fc41` |
| | `crates/doc/src/format/v28/two-sided-extrude-direction.rs` (`step` and its test; this and the step files below up to `v57` unchanged from `8ba46e9e` to `e8ce12ef`) | `5202815ffe17a6a7a1fc270300a3beed28d91a5231c8565635d0c0c91d987cb6` |
| | `crates/doc/src/format/v29/face-pieces-by-place.rs` | `d6a2bbc7fccaa26e3daad4db49563cda1a1b2d2096caec874668f715013138a2` |
| | `crates/doc/src/format/v35/inserted-file-features.rs` | `045cddbd953032ae87b50e51f03a574112ac6606d8353de61d920de36bcc8b70` |
| | `crates/doc/src/format/v40/feature-id-refs.rs` | `957bb125b91ca56943049c719850eb62c7e9d6f0ddffeeee39a58662ceaffcac` |
| | `crates/doc/src/format/v49/join-to-body-apart.rs` | `9778417d709ac7ca09b373efba36fa3cf680033bd9018d8c1a67f26b65f1a4cb` |
| | `crates/doc/src/format/v49/join-touching-bodies.rs` | `73f8faf7b9ed13e3c9ba472b019e7633d41b98d0392da15569eb422ef00d44e0` |
| | `crates/doc/src/format/v49/thread-ends.rs` | `263e0717478e049e647a418030c2a93cf699b536de7fc4b31a351ebff1077917` |
| | `crates/doc/src/format/v49/thread-hand.rs` | `323df849486034b1b226456b6bef9fea9aa314288a2d283eacf65de9a7de61e0` |
| | `crates/doc/src/format/v50/thread-nominal-size.rs` | `8bdea464676da93df601702d393174ea4ca4a143d0dee23ce1b7132e015109ae` |
| | `crates/doc/src/format/v51/two-sided-extrude-fixed-sides.rs` (at `8ba46e9e`, as the five below) | `7537a5c8b547a368a254ccce920841cf186513451e6603ae120604e0704e6e02` |
| | `crates/doc/src/format/v53/configuration-row-ids.rs` | `e9950d26e8a3b8a21bbaad2e4601570e4021225e93e83dd957c731a544cc89cd` |
| | `crates/doc/src/format/v53/hole-sketch-plane-start.rs` | `b846ce716ae2e5ab02a8db0def11259099b133909504c77208df212ae5b512b9` |
| | `crates/doc/src/format/v56/pattern-names-by-source.rs` | `c4046d16511ca7f16d93d9964b80e4af08243f902c2f2eb9d740c3fad0277c25` |
| | `crates/doc/src/format/v57/extrude-to-object-keeps-picked-placements.rs` | `6e440360c9ef7d808c2e63311b43aa149bd048b404223cc56ab742bb7416b4eb` |
| | `crates/doc/src/format/v57/sweep-walls-named-along-the-path.rs` | `f8da9eff6dd0416c170eca1fbb61c79ee69cde1e4278df40256224a5baf1e3d6` |
| | `crates/doc/src/format/v58/extrude-from-slanted-face.rs` (at `e8ce12ef`, as the six below) | `69ca547dace3f7b2ee3bdc85f63155c9deff179683f9330b4336e1c2fb6abf21` |
| | `crates/doc/src/format/v58/sketch-reference-placements.rs` | `470b7729f1df45c98fc4849098f97a18734ad32d2cbfead1f632f00a4cb19dfc` |
| | `crates/doc/src/format/v58/surface-sweep-exact-spline-path.rs` | `31c895d6be4a0a64bb3b64cb705361c025f31639e36558c02148eae657931664` |
| | `crates/doc/src/format/v62/sheet-edge-names.rs` | `a52bb729a53580429010b6b35fe6ec956556a9b80f64d65f587441d28d319cdd` |
| | `crates/doc/src/format/v62/sheet-face-names.rs` | `1d142aeb777f2ba54b4039573ed7ec9ce614cb42f95b01fa01b850dc90ea979b` |
| | `crates/doc/src/format/v62/sheet-hem-gap.rs` | `dbadb3063e475be6d6051c26a49b340b9f9c2cd53edf3b85b8e2c9a226f392f2` |
| | `crates/doc/src/format/v62/sheet-surface-convert.rs` | `bcbf4e5cd4153417eb76ac2b058a170c86af414bab4ad12390e6643830a91123` |
| | the names of every change file of `crates/doc/src/format/v23` to `v63` (`FOLDED`) | (the folders' listings) |
| | `crates/doc/src/params.rs` at `e8ce12ef` (`Document::name_all_feature_inputs`, the same naming as at `777f9609`, `0b787f8e` and `8ba46e9e`) | `52894294c222979ef6df460bd49162fdb00c480c74b0ceaffe25467f72019f9d` |
| `cam.rs` | `crates/doc/src/joints_cam.rs` at `0b787f8e` (the follower's contact with a cam's profile: `first_contact_along`, `touches_turning`, `overlaps_turning`, `first_contact_turning`, `next_contact_turning`, `dist_to_segment`, `inside`, `area`, `round`, the sampling loop of `table`) | `b31e49db7a82e8a72a472d2f2b128b5dce3c71ad6926093eb38436bc59a877cc` |
| `mesh_convert.rs` | `crates/kernel/src/mesh_convert.rs` at `0b787f8e` (Convert Mesh, mitcad#462, built into `core/model/src/features/mesh_convert.rs`: `convert_prismatic_mesh`, `weld`, `extrusion`, `certify_nesting`, `fit_loop`) | `982246e109a52b8270d07073fad4ebcf144bdbde16c05096a165a7747665905e` |
| `plastic.rs` | `crates/doc/src/plastic.rs` at `0b787f8e` (`PlasticRule::new`, `PlasticRule::library`, the order of `PlasticRule::fields`; mitcad#459) | `2a6dbad07597fe331bb5cf2a1f11f12e1bfbc38a2a2f8029fc0a7152a72aa7d4` |

The feature walk that names a feature's inputs (`core/solidcraft/src/inputs.rs`)
is MitCad's own port of `crates/doc/src/walk.rs` and the `#[walk]`
attributes of `crates/doc/src/document.rs`, checked against the golden
record `crates/doc/src/feature_walk_tests/golden_exprs.txt` at `e8ce12ef`
(formats 58 to 63 add a fillet's `Edge radius` inputs and a bounding
solid's six side margins; a plane at an angle's axis origin, an
expression since format 59, is a placement, not an input).

## Formats

Formats 23 to 63 are the changes of their `v<N>/` folders (`FOLDED` in
`format.rs`; formats 51 to 57 hold 65, formats 58 to 63 hold 42). The
steps that rewrite an older design as it opens:

| Format | Change | What the step does |
|---|---|---|
| 28 | `two-sided-extrude-direction` | a two-sided `negative` extrude becomes `positive` (its shape kept) |
| 29 | `face-pieces-by-place` | every feature keeps the kernel's numbering of split face pieces (`kernel_piece_order`) |
| 35 | `inserted-file-features` | an import or mesh import inside a component is marked `inserted` |
| 40 | `feature-id-refs` | pattern and mirror seeds and construction plane references become feature ids |
| 49 | `join-to-body-apart` | a Join extrude To Object of a design before format 40 keeps the body it runs to apart |
| 49 | `join-touching-bodies` | every joining feature keeps its touching pieces in one body (`touching: one_body`) |
| 49 | `thread-ends` | a thread's `tip: chamfer` becomes `both`, `ease_in` is dropped |
| 49 | `thread-hand` | format 39's `handedness: left` becomes `hand: left` |
| 50 | `thread-nominal-size` | a thread without a designation keeps the tap-drill size rule (`old_auto_size`) |
| 51 | `two-sided-extrude-fixed-sides` | a two-sided extrude of entered distances keeps the second side following the first across the sketch plane (`second_follows_first`) |
| 53 | `configuration-row-ids` | configuration rows get ids from the design's `next_id`, which moves past them |
| 53 | `hole-sketch-plane-start` | a hole at sketch points (not a Single hole) keeps starting at the body's surface (`old_surface_start`) |
| 56 | `pattern-names-by-source` | a Pattern of several bodies with stored names keeps reading them in the order the copies were made (`pattern_names_in_order`) |
| 57 | `extrude-to-object-keeps-picked-placements` | an extrude to or from an object gets the other components' placements the file holds (`picked_placements`) |
| 57 | `sweep-walls-named-along-the-path` | a sketch link's saved name on a wall of a sweep along several curves is dropped (`name_dropped`) |
| 58 | `extrude-from-slanted-face` | an extrude From Object keeps starting on a plane parallel to the sketch at the level where the face meets the middle of the profiles (`from_level`) |
| 58 | `sketch-reference-placements` | a sketch in a component captures where the sketches its links read (`sketch_placements`) and the construction plane of another component it lies on (`placement`) stood relative to its own component |
| 58 | `surface-sweep-exact-spline-path` | a surface sweep along a 3D sketch curve keeps its walls along the lines between the spline's samples (`spline_as_samples`) |
| 62 | `sheet-edge-names` | a flange's or hem's edges saved by the sheet's own names (`base:<line>:<side>`, `F<f>.<k>:<edge>:<side>`) take the model's names where the design says what made them |
| 62 | `sheet-face-names` | a sheet's faces saved by the sheet's own names (`sheet:<body>:base:<side>`) take the model's names where the design says what made them |
| 62 | `sheet-hem-gap` | a hem's `gap` and a rule's `hem_gap` are doubled (`2 * (…)`), the space between leg and face where they were the inside radius; a rule without `hem_gap` gets `0.02 mm` |
| 62 | `sheet-surface-convert` | a conversion of a plate (no bend edges) becomes Recognize of that plate from its face (`sheet_recognize`) |

Every other change of formats 28 to 63 needs no rewrite: it adds a field or
a kind an older design does not have, and is listed by name only (so a
design written while it was pending is read). The import reads the ones it
maps (`core/import-solidcraft/docs/solidcraft.md`); the display-only ones
(the `render-*` changes, `opus-quality`, `cpu-output-presentation`,
`glass-material-models`, `canvas-lock`, browser state, print faces,
identification colours, component origin visibility, history visibility,
timeline folders' joints, point clouds, embedded surface textures, model
caches' `blend-definition-faces` and `saved-face-points`, plugin data,
`names-count-characters`, `named-view-roll-and-up`,
`plugin-data-documents`, `canvas-anchor`, `sheet-rule-material`) are read
and ignored.

The pinned release has no pending changes, so `PENDING` is empty. A design
saved by a newer build of SolidCraft (a format past 63, or changes this
import does not know in its `format_pending`) is read as SolidCraft 0.45.0
reads it (`crates/doc/src/newer_file.rs`, mitcad#1994): `newer` says so,
the steps of the changes it knows run, a format past 63 has none to run,
and the reader (`core/solidcraft`) leaves out the sections it does not know
or cannot read and the parameters it cannot read, naming them, and keeps a
feature it cannot read in its place for the import's placeholder. Only a
file that is not a design (format 0, another format key) is refused.
SolidCraft's drawings (`solidcraft-drawing/N`, files of their own) are
refused as drawings.

## Changes

- The steps work on `serde_json::Value` and return `Result<(), String>`
  instead of SolidCraft's document error; `join-to-body-apart` reads the
  file's format with `core/solidcraft`'s `version_of`.
- `extrude-to-object-keeps-picked-placements` reads the occurrences it
  needs from the JSON (`Placing`) instead of SolidCraft's `Occurrence`,
  with `mat_mul`, `mat_inverse` and `transform_through` on `[[f64; 4]; 4]`.
- `sketch-reference-placements` reads the occurrences likewise, with one
  at the identity for each component the file never placed (as
  `add_occurrence` gives it); a sketch plane's face anchor is read from
  the JSON (`anchor` with its `path` and `author_path`, beneath an offset,
  a plane through a point, an angle or a flip) instead of SolidCraft's
  typed `PlaneRef` and `FaceAnchor`, a link's source sketches likewise
  (`sketch_curve`, `sketch_point`, `on_surface` and both sides of an
  `intersection`; a source of another kind reads none).
- The steps of formats 58 and 62 that have helpers of their own
  (`sketch-reference-placements`, `sheet-edge-names`, `sheet-face-names`)
  are modules of `format.rs`; `sheet-face-names`' `parts` is
  `name_parts`.
- A newer design is not refused: `newer` (SolidCraft's `Family::newer`)
  says what makes it newer, and `check` refuses only format 0.
- The upgrade path (`upgrade`, `check`, `parts`, `canonical`: SolidCraft's
  `Formats::upgrade`, `Family::take_version`, `Formats::parts`,
  `Family::canonical_applied_name`) is written out for the pinned format:
  the hand-written steps up to format 22, then each later format as the
  list of its changes (`FOLDED`, SolidCraft's `v<N>/` folders, listed there
  by a build script), each with its step when it has one, then the pending
  changes (`PENDING`). A design of a released format has every change of
  that format; one that lists `format_pending` gets the steps of its
  format's changes it does not list (mitcad#1331: a released format 28 is
  not upgraded to 28 again). The names of the steps that ran are given for
  the import's report.
- The steps' tests are SolidCraft's, adapted (`format.rs`, `mod tests`);
  `sketch-reference-placements` has none upstream at `e8ce12ef` (its tests
  there go through SolidCraft's document), so its tests here are MitCad's
  own on minimal designs.
- `name_all_feature_inputs` works on the design's JSON; the number of
  inputs of a feature comes from MitCad's port of SolidCraft's feature
  walk (`core/solidcraft/src/inputs.rs`).
- `cam.rs`: points are `[f64; 2]` with a small vector type of its own,
  errors are `String`s, and the setup (the profile polygon, the
  follower's line or pivot and arm, the signs) comes from MitCad's frames
  and joints (`CamSetup`); the table's interpolation is the joint
  solver's (`mitcad_solver::Relation::Table`), not SolidCraft's `lift_at`.
- `mesh_convert.rs`: the mesh is MitCad's `TriangleMesh`, errors are
  `String`s, and the result is the section (a frame, the height, regions of
  polygons and circles), which the Convert Mesh feature extrudes with
  MitCad's kernel and names; the rebuilt volume compared with the mesh's is
  the section's area times the height, and the fitted loops' nesting is
  certified geometrically (raw loops may not meet, a fitted circle may not
  reach across another loop) instead of by SolidCraft's planar booleans.
- `plastic.rs` gives the rule library as the design file's JSON form of a
  rule (the form of a design's own rules), which the import maps onto
  MitCad's plastic rules.
