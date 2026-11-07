# mitcad-f3d-file: the document of an `.f3d` file

`core/f3d-file` reads the document inside an `.f3d` file's zip container
(`mitcad-zip`):

| Module | Contents |
|---|---|
| `container` | `F3dFile`: what each entry is, the design segments, the body blobs, previews and display meshes, the documents of an `.f3z` package. |
| `ogs` | The display meshes saved with a document, an independent check of the bodies ([OGS_FORMAT.md](../f3d/OGS_FORMAT.md)). |
| `protein` | The appearance and material asset packages (mitcad#228). |
| `paramesh`, `lzma` | The mesh containers of mesh bodies, `ParaMeshGeometry.<uuid>.paramesh` (mitcad#1366): registry, streams, vertices, triangles and feature edges, the streams inflated by a raw LZMA decoder of our own; `F3dFile::mesh_entries` lists them. The design records that place them are `mitcad_f3d_decode::mesh`. |

`F3dFile::read` takes the zip reader's default limit (256 MiB per entry);
`F3dFile::read_large` is the opt-in for body blobs and the documents of an
`.f3z` package (mitcad#923), which `mitcad-f3d-design`'s `package` opens one
at a time.

Until mitcad#776 this was part of `mitcad-f3d`, which re-exports it
under the same paths (`mitcad_f3d::container`, `mitcad_f3d::F3dFile`, ...).
