# SolidCraft: format upgrade steps, cam geometry and plastic rules

The `.solidcraft` import (`core/solidcraft`, mitcad#450) reads designs of
every SolidCraft file format by first upgrading an older file to the one
format it maps (57, the format of release 0.43.0; mitcad#1331,
mitcad#1994), and reads a design a newer build saved as far as it can,
with a warning. The upgrade
steps are SolidCraft's own, adapted here. The cam geometry of motion links
(`cam.rs`, mitcad#466: where a follower touches a cam's profile over a
turn) is adapted from SolidCraft too and built into `core/model`
(`motion_links.rs`). The plastic rule library (`plastic.rs`, mitcad#459)
is SolidCraft's too, as data the import maps onto MitCad's plastic
rules.

- Source: https://github.com/tangency-labs/solidcraft.
  - `format.rs`: release `v0.43.0`, commit
    `8ba46e9e2181396a9b5903b98031399aef61cf26` (15.11.2026, the newest
    release tag on 17.11.2026; it has no pending format changes). Before
    mitcad#1994 it was release `v0.40.0-rc.2`, commit `777f9609` (format
    50).
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
| `format.rs` | `crates/doc/src/format.rs` at `8ba46e9e` (`v2_to_v3`, `v3_to_v4`, `v4_to_v5`, `v5_to_v6`, `v13_to_v14` and their helpers, unchanged since `777f9609`) | `6491bef00d8d9af206d593be25a8b6dc68bc285b06899dfd5efffd6d0456ca4f` |
| | `crates/format/src/lib.rs` at `8ba46e9e` (`Formats::upgrade`, `Formats::parts`, `Family::newer`, `Family::canonical_applied_name`, `Newer`) | `145b385301523c97a90a3d0d1752c70218aa0f7dcf87edf0a6867c7ff82975bd` |
| | `crates/doc/src/newer_file.rs` at `8ba46e9e` (`salvage`: what of a newer design is read and what is left out) | `0faec28e7fad487e064ca94df878c7236abc5df6b11c2852ae88ce9ac89bb075` |
| | `crates/doc/src/assembly.rs` at `8ba46e9e` (`mat_mul`, `mat_inverse`, `transform_through`) | `9efa5528e0cd8dc0b3bc411f553dcba7f9fd0f5516f226f6f383c6f63236412e` |
| | `crates/doc/src/format/v28/two-sided-extrude-direction.rs` (`step` and its test) | `5202815ffe17a6a7a1fc270300a3beed28d91a5231c8565635d0c0c91d987cb6` |
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
| | the names of every change file of `crates/doc/src/format/v23` to `v57` (`FOLDED`) | (the folders' listings) |
| | `crates/doc/src/params.rs` at `8ba46e9e` (`Document::name_all_feature_inputs`, the same naming as at `777f9609` and `0b787f8e`) | `3053d3856fb61e187bc58c423e234248c0e1b86f5444db9b1a729d9eb6a46c10` |
| `cam.rs` | `crates/doc/src/joints_cam.rs` at `0b787f8e` (the follower's contact with a cam's profile: `first_contact_along`, `touches_turning`, `overlaps_turning`, `first_contact_turning`, `next_contact_turning`, `dist_to_segment`, `inside`, `area`, `round`, the sampling loop of `table`) | `b31e49db7a82e8a72a472d2f2b128b5dce3c71ad6926093eb38436bc59a877cc` |
| `mesh_convert.rs` | `crates/kernel/src/mesh_convert.rs` at `0b787f8e` (Convert Mesh, mitcad#462, built into `core/model/src/features/mesh_convert.rs`: `convert_prismatic_mesh`, `weld`, `extrusion`, `certify_nesting`, `fit_loop`) | `982246e109a52b8270d07073fad4ebcf144bdbde16c05096a165a7747665905e` |
| `plastic.rs` | `crates/doc/src/plastic.rs` at `0b787f8e` (`PlasticRule::new`, `PlasticRule::library`, the order of `PlasticRule::fields`; mitcad#459) | `2a6dbad07597fe331bb5cf2a1f11f12e1bfbc38a2a2f8029fc0a7152a72aa7d4` |

The feature walk that names a feature's inputs (`core/solidcraft/src/inputs.rs`)
is MitCad's own port of `crates/doc/src/walk.rs` and the `#[walk]`
attributes of `crates/doc/src/document.rs`, checked against the golden
record `crates/doc/src/feature_walk_tests/golden_exprs.txt` at `8ba46e9e`.

## Formats

Formats 23 to 57 are the changes of their `v<N>/` folders (`FOLDED` in
`format.rs`; formats 51 to 57 hold 65). The steps that rewrite an older
design as it opens:

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

Every other change of formats 28 to 57 needs no rewrite: it adds a field or
a kind an older design does not have, and is listed by name only (so a
design written while it was pending is read). The import reads the ones it
maps (`core/import-solidcraft/docs/solidcraft.md`); the display-only ones
(the `render-*` changes, `opus-quality`, `cpu-output-presentation`,
`glass-material-models`, `canvas-lock`, browser state, print faces,
identification colours, component origin visibility, history visibility,
timeline folders' joints, point clouds, embedded surface textures, model
caches' `blend-definition-faces` and `saved-face-points`, plugin data) are
read and ignored.

The pinned release has no pending changes, so `PENDING` is empty. A design
saved by a newer build of SolidCraft (a format past 57, or changes this
import does not know in its `format_pending`) is read as SolidCraft 0.43.0
reads it (`crates/doc/src/newer_file.rs`, mitcad#1994): `newer` says so,
the steps of the changes it knows run, a format past 57 has none to run,
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
- The steps' tests are SolidCraft's, adapted (`format.rs`, `mod tests`).
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
