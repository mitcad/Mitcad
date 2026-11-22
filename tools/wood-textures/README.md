# Wood and panel textures

`generate.sh` makes the texture sets of the library's woods and panels
(mitcad#2186) into `core/model-base/textures`: it builds `generate.rs`
(plain Rust, no dependencies) with `rustc`, runs it into a work folder
(default `build/wood-textures`) and turns its PPM and PGM images into
JPEG files with ImageMagick (`magick` or `convert`).

    tools/wood-textures/generate.sh [work folder] [look id ...]

The images are MitCad's own (MIT): growth rings, fibres and pores for the
woods, rotary-cut figure for the plywoods, strands for OSB, chips for
chipboard and faint mottling for MDF, all from tileable noise with fixed
seeds. See `core/model-base/textures/README.md` for the files and how the
library uses them.
