# Display meshes in .f3d files (OGS scene)

Newer `.f3d` documents hold a display scene: a tessellation of the
design's bodies made when the file was saved, an independent check of the
bodies built from the ASM blobs ([ASM_FORMAT.md](ASM_FORMAT.md)). Read by
`src/ogs.rs` (`mitcad-f3d-inspect meshes <file>`).

**Method.** Black-box only: the 10 corpus documents with a scene (all
saved by ASM 232.3) were dumped and compared with each other and with the
bodies converted from their ASM blobs. No writer code was decompiled and
no code of other readers was used. *(verified)*: holds for every record of
the corpus; *(assumed)*: plausible, not confirmed. Files are named by
corpus position (f01, ...; see [CORPUS_REPORT.md](CORPUS_REPORT.md)).

## Entries

`FusionAssetName[Active]/OGS.BlobFolder/OGS/DefaultScene/`:

| Entry | Content |
|---|---|
| `world` | The scene graph (below). |
| `Fusion_mesh_000` | Float32 vertices and u32 indices of body faces, body edges and sketch profiles. Absent in assemblies of external components (no bodies in the scene). |
| `stream_mesh_000` | Small meshes of work geometry (origin point, axes, planes): 8-float vertices and u32 indices such as `0 1 2 2 3 0`. Not used. |

The `_000` suffix suggests large scenes are split into several buffers; a
face record names its buffer by number *(assumed: every corpus face uses
buffer 0)*.

## `world`

A serialized scene graph. Objects start with their type name: u32
character count and UTF-16LE characters (`GroupNode`, `GeometryNode`,
`ARenderList`, `SingleNodeWorld`, `Face`, `Edge`, `TransformAttribute`,
...). Scene nodes carry an id string of 16 upper-case hex digits
(`000002116E9F4EB8`, the object's address at save time); later references
to a node repeat the id. A node's client type (`Body`, `Component`,
`Instance`, `Sketch3D`, `OriginPlane`, ...) is written after its children.
The general grammar (attribute payloads, child counts, end marks) is not
decoded; the reader finds the records below by their type names and
cross-checks them.

**Member prefix** *(verified)*: the body record, its face lists and each
of its faces are immediately preceded by 25 bytes:

| Bytes | Content |
|---|---|
| 4 | u32 1 |
| 12 | owner key, the same for a body and all its faces (e.g. `000000000000000000000494`; in some files the first u32 is 1) |
| 8 | f64: 0 for the body and its face lists; for a face an integral number unique within the body (e.g. 0 ... 7, 110 ... 113; or 14614, 39659, ...), possibly a persistent face id *(assumed)* |
| 1 | u8 1 |

**Face list** (`Faces`) *(verified)*: after the type name 29 bytes (25
zero bytes and f32 1.0), u32 3, 8 bytes, u32 0x10, u32 face count; the
faces follow. The count agrees with the face records found for the body.
Sketch profiles use `Faces` lists with other values; `Edges` lists use 5
and 0x08.

**Face record** (`Face`), offsets after the type name *(verified)*:

| Offset | Content |
|---|---|
| 0 | 46 bytes, the same in every face (25 zero bytes, f32 1.0, 17 zero bytes) |
| 46 | u8 flags: 0x08 the face has a mesh, 0 it has none |
| 47 | u16 0 |
| 49 | with a mesh: u32 vertex layout, always 7 (interleaved position, normal and uv floats followed by triangle indices) |
| 53 | u32 buffer number (`Fusion_mesh_NNN`) |
| 57 | u32 byte offset of the face's data in the buffer |
| 61 | u32 counts: position floats, normal floats, uv floats, indices |
| 77 | u32 n, then n u32 edge indices into the body's `Edges` list |
| | 6 x f64 bounding box of the face (min xyz, max xyz) |

Without a mesh the bounding box follows the u16 directly. Faces without a
mesh were seen in one body only, which has a `VisibilityAttribute` and is
probably hidden.

**Body record** (`Body`) *(verified)*: the client type of the body's group
node, written after the body's edges, appearance
(`FPrioritizedProteinAttribute` followed, after 5 bytes, by a Protein
asset id string), face lists, a texture mapping attribute and the node id
(sometimes followed by another attribute). Its member prefix carries the
key of its faces; the reader assigns each face to the next body record
with the same key.

**Transforms**: no body group of the corpus has a `TransformAttribute`
*(verified)*. Component occurrences are `Instance` records preceded by a
`TransformAttribute` (5 bytes, then 16 f32 of a column-major 4x4 matrix
with the translation in centimetres in elements 12-14) and the id of the
component's group node *(assumed from one file, where an instance is a
180-degree rotation about y with a translation)*. Which body lies in which
component needs the scene grammar and is not decoded.

## Mesh buffer

Per face *(verified)*: `positions / 3` vertices of `(position + normal +
uv) / vertices` floats each (always 8: position xyz, normal xyz, uv),
interleaved, followed directly by the u32 triangle indices, local to the
face (a triangle list). Face data do not overlap; the rest of the buffer
holds edge polylines (`Edge`, layout 6: position and normal floats, two
counts, no indices) and sketch profiles (`ProfileFace`, `ProfileEdge`).

- **Units and frame** *(verified)*: centimetres, in the coordinates of the
  body's component, as in the ASM blob. Every vertex lies inside the
  bounding box stored with its face, and mesh bounding boxes equal those
  of the matching converted ASM bodies where the extremes are vertices.
- **Orientation** *(verified)*: triangles are counter-clockwise seen from
  outside; every closed mesh has a positive volume.
- **Watertightness**: faces are tessellated separately. The vertices of a
  shared edge coincide in most bodies after welding positions within 1e-6
  of the bounding-box diagonal (exact equality leaves a few gaps). Some
  bodies stay open because the faces on the two sides of a curved edge
  divide it into different numbers of segments (e.g. 28 and 29 along a
  180-degree arc); the gaps are slivers, so volumes stay close. A mesh may
  also have a non-manifold edge where a face folds a pair of sliver
  triangles back onto it.
- **Content**: the scene holds the bodies of the current design only
  (they match `.smbh` bodies), not the other bodies of the `.smb` blobs.
