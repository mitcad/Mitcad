# mitcad-model-build

The BUILD tab's planning (mitcad#375; [commands.md](../model-api/src/commands.md),
"BUILD: profile assemblies"): placing and editing parts, connectors,
panels, timber members and their joinery, spacing sets, timber frames,
fasteners, the junctions where profiles meet, end caps (`end_cap.rs`,
mitcad#2243) and the cut list.

The crate sits above `mitcad-model` and below `mitcad-model-api`
(mitcad#777), so that an edit of BUILD planning recompiles only this crate
and the ones above it, not the model. What the design file keeps of BUILD
(the library sources, the records on components, the libraries' `build`
blocks, `SideFrom`) stays in `mitcad_model::build`, because the file
format and the features (`derived_part`, `bound_length`) need it; this
crate re-exports it, so `mitcad_model_build::…` names all of BUILD's
API. The vector helpers, `value_in` and what the parts list and the
properties read of a placed part (`PartInfo`, `Document::part_info`, its
designation and part number) are the model's too, under
`mitcad_model::internal::build` (mitcad#811); the plans BUILD works out
before it changes the design, which the api reads, are under this
crate's `internal`.

The document's BUILD methods are extension traits of
`mitcad_model::Document`: import `mitcad_model_build::prelude::*` to call
`build_place`, `part_info`, `build_junctions` and the others. Each impl
block's methods form traits by their visibility: `<Block>Build` (public)
and `<Block>CrateOps` (the crate's).

The crate has no tests of its own: BUILD is tested through the JSON api in
`core/model-tests/build-frames` and `build-timber`, with the test library
of synthetic parts in `core/model-tests/support`
([README](../model-tests/README.md)).
