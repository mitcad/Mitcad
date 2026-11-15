// SPDX-License-Identifier: MIT
#pragma once

// File › Export's glTF types (mitcad#1826, app/COMMANDS.md "Export"): the
// design's bodies as a glTF 2.0 file, binary (.glb, self-contained) or
// JSON (.gltf, with its buffer and texture images as files beside it),
// written by the web page's writer (web/WebGlb.hpp, gltfFile): the
// component tree as nested nodes named by the components and the bodies,
// placements as matrices, metres, Y up, the appearances as materials and
// textures.

#include <QString>
#include <QStringList>
#include <QtGlobal>

namespace mitcad {

class CommandContext;

namespace web {

struct GltfOptions {
  double linearDeflection = 0.1;  // mm, as Export's Refinement
  double angularDeflection = 0.5; // radians
  // .glb; else .gltf with the files beside it.
  bool binary = true;
  // The root node's name; empty: the file's name without its extension.
  QString title;
  // The folder that relative texture paths are relative to (the design
  // file's); empty: such textures are left out.
  QString designFolder;
  // Only these bodies (uids), every placement of each; empty: every
  // visible body.
  QStringList bodies;
};

struct GltfResult {
  bool ok = false;
  QString error;
  // The files written: the file itself first, then those beside it.
  QStringList files;
  qint64 bytes = 0; // of all of them
  int bodies = 0;   // body nodes (a body placed twice counts twice)
  qint64 triangles = 0;
  // What is not as in the design: bodies that cannot be meshed, texture
  // images that could not be read.
  QStringList warnings;
};

// The design open in `context` as a glTF file at `path` (its extension
// does not choose: `options.binary` does). Reads the model through
// `context` only; the design's shapes are meshed on copies.
GltfResult writeGltf(const CommandContext& context, const QString& path, const GltfOptions& options);

} // namespace web
} // namespace mitcad
