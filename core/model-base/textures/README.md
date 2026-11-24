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
  that every image is there and that no program of the package carries a
  copy built in (mitcad#2251, mitcad#2323: a run of 64 bytes from the
  middle of each image is looked for in each program). Built
  into the programs with `include_bytes!`, as before mitcad#2218, each
  program linking the model carried its own copy: `mitcad` and
  `mitcad-cli` (`mitcad-render` and `mitcad-thumbnailer` do not link the
  model), so the packages had the images twice.

- **Source:** MitCad's own images, made from noise by
  [`tools/wood-textures/generate.rs`](../../../tools/wood-textures/generate.rs)
  (`tools/wood-textures/generate.sh` builds it with `rustc`, runs it and
  turns its PPM and PGM images into these JPEG files with ImageMagick).
  No photographs and no third-party images; the generator's fixed seeds
  make the same images again.
- **Licence:** MIT, as MitCad (`LICENSE`); the appearances' texture
  `source` says `{"id": "mitcad", "asset": "<id>", "resolution": "1k",
  "license": "MIT"}`.
- **Size:** 1024 × 1024 pixels each, tileable; 42 files, 4,612,244
  bytes (4.6 MB) in all. The packages grow by about as much, once: JPEG
  data does not compress further (mitcad#2251: the 42 images alone in a
  squashfs with the AppImage's zstd and 128 KiB blocks take 4,567,040
  bytes; built into `mitcad` and `mitcad-cli`, as before mitcad#2218,
  twice that, about 9.1 MB, a figure derived from the two copies: no
  release package of that state was made).
- **Measured in the release AppImages (mitcad#2323):** 0.69.0, without
  the textures, 288,287,224 bytes; 0.70.0, the first release with them
  (as files, mitcad#2218 came before the release), 296,970,744 bytes;
  0.71.0 300,452,344 bytes. 0.70.0 grew by 8,683,520 bytes: about
  4,567,040 for the images, the rest from the programs, whose files
  (without the images, uncompressed) grew by 12.4 MB from 0.69.0 to
  0.70.0, as they grew by 10.6 MB (3.5 MB in the AppImage, no image
  changed) from 0.70.0 to 0.71.0. The 0.71.0 AppImage has the 42 images,
  identical to this folder's, in `usr/share/mitcad/textures` and none in
  a program of `usr/bin`. The state with the images built in (between
  mitcad#2186 and mitcad#2218) was on `dev` for six hours and in no
  release, so no package of it was made.

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
