# mitcad-f3d-decode: the streams, records and sketches of `.f3d` designs

`core/f3d-decode` decodes the design segment of an `.f3d` document as far
as the stream data goes; the feature building above it is
[`mitcad-f3d-design`](../f3d-design/README.md):

| Module | Contents |
|---|---|
| `stream` | `MetaStream.dat` and the object framing of `BulkStream.dat`. |
| `classes`, `decode`, `sketch`, `unicode` | Decoders for the parts the format study marks certain or probable. |
| `recipe`, `curve` | The recipes and stored curves that feature inputs name their entities with. |
| `learn` | Raw item records for the import's learning dump (mitcad#96). |
| `writer` | Synthetic design streams for tests (mitcad#196). |
| `mesh` | The design's mesh bodies (mitcad#1366): which container holds each, its item, component, placement and name. |

It uses nothing of the feature building: an edit in `mitcad-f3d-design`
does not rebuild this crate. Until mitcad#779 these modules were part of
`mitcad-f3d-design`, which re-exports them under their old paths
(`mitcad_f3d_design::stream`, `mitcad_f3d::design::stream`, ...).
`unicode.rs` is written by `core/f3d/tools/gen_unicode_tables.py`.

The feature `scan-count`, which only the tests turn on (as a
dev-dependency of `mitcad-f3d-design`), counts the references
`Segment::ref_at` reads on each thread (`stream::ref_reads`): the tests
check with it that the sketch scans stay linear in the size of the data
instead of timing them (mitcad#1348). Other builds carry no counter.
