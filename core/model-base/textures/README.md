# The built-in woods' and panels' textures

The texture sets of the library's woods and panels (mitcad#2186,
`src/appearance/wood_textures.rs`, commands.md "Built-in wood and panel
textures"), shipped with MitCad as files and read at run time
(mitcad#2218).

- **Installed once:** the install rules copy this folder's `*.jpg` to
  `bin/textures` (Windows, `cmake/Packaging.cmake`),
  `share/mitcad/textures` (the AppImage, `cmake/AppImage.cmake`) and
  `Contents/Resources/textures` (macOS, `app/packaging/macos/Bundle.cmake`),
  where the model in both `mitcad` and `mitcad-cli` finds them beside the
  running program; programs in a build tree and the tests read this
  folder. The package checks (`tools/appimage-test.sh`,
  `tools/installer-windows-test.ps1`, `tools/check-bundle-macos.sh`) check
  that every image is there. Built into the programs with
  `include_bytes!`, as before mitcad#2218, each program linking the model
  carried its own copy.

- **Source:** MitCad's own images, made from noise by
  [`tools/wood-textures/generate.rs`](../../../tools/wood-textures/generate.rs)
  (`tools/wood-textures/generate.sh` builds it with `rustc`, runs it and
  turns its PPM and PGM images into these JPEG files with ImageMagick).
  No photographs and no third-party images; the generator's fixed seeds
  make the same images again.
- **Licence:** MIT, as MitCad (`LICENSE`); the appearances' texture
  `source` says `{"id": "mitcad", "asset": "<id>", "resolution": "1k",
  "license": "MIT"}`.
- **Size:** 1024 × 1024 pixels each, tileable; 42 files, about 4.7 MB
  in all (the installer grows by about as much, once: JPEG data does not
  compress further).

| Appearance id | Files | One repeat |
|---|---|---|
| `wood_birch` | `wood_birch-color.jpg`, `-normal.jpg`, `-roughness.jpg` | 800 mm |
| `wood_cedar` | `wood_cedar-*.jpg` | 800 mm |
| `wood_cherry` | `wood_cherry-*.jpg` | 800 mm |
| `wood_douglas_fir` | `wood_douglas_fir-*.jpg` | 800 mm |
| `wood_eucalyptus` | `wood_eucalyptus-*.jpg` | 800 mm |
| `wood_pine` | `wood_pine-*.jpg` | 800 mm |
| `wood_teak` | `wood_teak-*.jpg` | 800 mm |
| `panel_chipboard` | `panel_chipboard-*.jpg` | 400 mm |
| `panel_mdf` | `panel_mdf-*.jpg` | 600 mm |
| `panel_osb` | `panel_osb-*.jpg` | 600 mm |
| `plywood_birch` | `plywood_birch-*.jpg` | 1000 mm |
| `plywood_eucalyptus` | `plywood_eucalyptus-*.jpg` | 1000 mm |
| `plywood_pine` | `plywood_pine-*.jpg` | 1000 mm |
| `plywood_spruce` | `plywood_spruce-*.jpg` | 1000 mm |

Each set: `-color.jpg` the base colour (sRGB; its mean in linear light is
the appearance's `base_color`), `-normal.jpg` a tangent-space normal map
(OpenGL convention, green up), `-roughness.jpg` a grey roughness map (its
mean the appearance's `roughness`). The grain runs along the images'
width, which the `grain` projection lays along each face's longest side.

To change them, edit `generate.rs` (the looks' colours and roughness must
stay those of `src/appearance.rs`), run `tools/wood-textures/generate.sh`
and commit the new JPEG files.
