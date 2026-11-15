# mitcad-gltf

Reader of glTF 2.0 files for the import of mesh bodies and components
(mitcad#1826): the binary form (`.glb`) and the JSON form (`.gltf`) with
its buffers and images embedded as data URIs or as files beside it. No
geometry kernel: the model makes the mesh bodies
(`core/model/src/exchange/gltf_import.rs`, `import_file`), and the
`gltf_file` query answers the file as read. MitCad's own glTF files are
written by the application (`app/web/WebGlb.cpp`, Export's glTF types).

Written from the published glTF 2.0 specification; no other reader's code.

## What is read

- The scene's node tree: names, meshes, children, placements (a matrix,
  or translation, rotation and scale).
- Triangles (lists, strips and fans) of `POSITION` accessors of floats or
  of normalized integers (`KHR_mesh_quantization`), with indices of 8, 16
  or 32 bits, interleaved (a byte stride) or not.
- Metallic-roughness materials: base colour and alpha, metalness,
  roughness, emission (`KHR_materials_emissive_strength`), transmission
  (`KHR_materials_transmission`), the index of refraction
  (`KHR_materials_ior`), blending; the PNG or JPEG images of the base
  colour, the normal map, the ambient occlusion and the metalness and
  roughness (mitcad#1862).
- The texture coordinates of a primitive whose material has a base
  colour image (mitcad#1862, `Primitive::uvs`): the set the image's
  `texCoord` names (`TEXCOORD_0` by default), floats or normalized
  integers, moved by the image's `KHR_texture_transform` (offset,
  rotation, scale, its own `texCoord`), in MitCad's convention: v runs up
  the image from its bottom (glTF's runs down from its top, so v is
  1 - v).

## What is not read, said in words

`Gltf::notes` lists, once each, what the import does not take: animations,
skins, morph targets, points and lines, compressed meshes
(`KHR_draco_mesh_compression`, `EXT_meshopt_compression`), sparse
accessors, images that are not PNG or JPEG, other extensions (also those
the file requires), scenes other than the default one, and a textured
mesh without texture coordinates or with ones that cannot be read (its
texture is then projected as MitCad projects textures). Each note has a
detail with indexes for agents and the log; the words have none.

## Untrusted files

Every index, count, offset and stride is checked against what it points
into before it is used; a primitive whose data cannot be read is left out
with a note, the rest of the file read. Files beside the glTF file are
read only from its folder or below it: an absolute path, a path into a
parent folder or a URL is not read. A file of more than 50 million
triangles is refused. The node tree is walked once per node, cycles and
nesting deeper than 64 levels cut off.

## Tests

`core.gltf` (`cargo test -p mitcad-gltf`): files of the crate's test
writer (`testfile`) in every form, cut short and damaged, and the
public-domain sample `samples/two-blocks.gltf` (CC0, `samples/LICENSE`).
The import's tests are in `core/model-tests/files/src/gltf_tests.rs`.
Real, openly licensed sample files (never committed) are imported
through OCCT by the ctest `gltf.corpus` from `MITCAD_GLTF_CORPUS`
([development.md](../../docs/development.md#gltf-corpus)).
