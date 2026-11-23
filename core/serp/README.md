# mitcad-serp

Reader of `.serp` files for the import of bodies (mitcad#1870): the
native files of an open-source NURBS modeller built on OCCT, format
versions 1 to 9. No geometry kernel: the model makes the bodies
(`core/model/src/exchange/serp_import.rs`, `import_file`), the B-reps read
by the kernel's `import_brep`, the meshes by its `triangle_body`.

Written from the format as its writer saves it; there is no published
specification, and the format still changes quickly (versions 1 to 9
within a few months), so the reader refuses a version newer than it knows
in words ("made by a newer version of its program") rather than read it
half.

## The format

- Version 1: a bare JSON scene. Versions 2 and later: a zip with
  `meta.json` (format, version, saved time, counts), `document.json` (the
  scene) and `thumbnail.png`; from version 3 point clouds as binary
  members under `blobs/`, from version 4 pictures under `images/`.
- The scene: `format`, `version`, `units` (`mm`, `cm`, `m`, `in`, `ft`;
  millimetres when missing or unknown), `layers` (`id`, `name`, `color`,
  `visible`, `parent`, …), `objects` (`name`, `layer`, `visible`,
  `color`, `block`, and `brep` or `mesh`), `block_defs` (by id: `name` and
  `shapes`), and named views, layouts, annotation styles, hatch patterns,
  history records, point clouds, pictures and scan paths.
- An object's `brep` is base64 of the shape as OCCT's binary B-rep format
  writes it (`BinTools`). The same member holds other kinds in the
  writer's own tagged bytes: a text (`STXT`), a hatch (`SHCH`), a
  dimension (`SDIM`), a picture (`SERPPICTURE`) and a point cloud
  (`SPCL`), which are left out, and in a block's shapes a mesh (`SMSH`,
  form 1, or 2 with a float64 origin: the counts, float32 vertices and
  uint32 triangles).
- An object's `mesh`: base64 float32 vertices `v`, uint32 triangles `t`
  and an optional origin `o` they are measured from.
- Colours are three numbers from 0 to 1 (read from 0 to 255 when one is
  above 1). An object without a colour has its layer's.
- A placed block is an object whose shape is the block's shapes placed:
  the file keeps no placement. The import finds it by comparing the
  shapes (`Kernel::rigid_copies`).
- A point cloud (mitcad#2240): an entry of the scene's `pointclouds`
  (`id`, `name`, `layer`, `visible`, `count`, `bbox`, an optional float64
  `origin` the points are measured from when the cloud is far out, and
  `blobs`): `blobs/<id>/xyz.f32`, little-endian float32 x, y, z per
  point, and optionally `rgb.u8`, a byte per channel per point, both in
  the container, stored. A bare JSON scene keeps no points.

## What is read

`read` gives `Serp`: the version, the units, the layers (with
`layer_path`, `Parts::Bolts`, and `layer_shown`, which follows the
parents), the objects (each `Shape::Brep`, `Shape::Mesh`, `Shape::Other`
or `Shape::Unreadable`), the blocks, the point clouds whose points the
container holds (`Cloud`, with `cloud_shown`; mitcad#2240), and
`Skipped`: how many point clouds (those whose points cannot be read, and
the tagged `SPCL` shapes), scan paths, pictures, hatches, texts,
dimensions, layouts and history records the import does not take, in
words (`Skipped::parts`).

## Untrusted files

Every count and index is checked before it is used; an object whose data
cannot be read is `Shape::Unreadable` with why, and the rest of the file
is read. The scene's JSON may take 2 GiB once unpacked, a mesh 50 million
triangles. A file that is not a `.serp` file, a damaged container and a
newer version are errors in words, their detail for agents and the log.

## Tests

`core.serp`: files of the test writer (`testfile`), both forms, every
version up to the newest, a newer one, meshes far from the origin, layer
paths and colours, damaged and cut-short files.
