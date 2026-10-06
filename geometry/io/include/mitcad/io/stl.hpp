// SPDX-License-Identifier: MIT
#pragma once

// STL export of bodies and STL import as a mesh body.

#include <string>
#include <vector>

#include "mitcad/io/body.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad::io {

enum class StlFormat { Binary, Ascii };

struct StlWriteOptions {
  StlFormat format = StlFormat::Binary;
  MeshOptions mesh = MeshOptions::from(Refinement::Medium);
};

// Writes the shapes, triangulated with `options.mesh`, into one STL file.
// Mesh bodies are written as they are. Coordinates are millimetres.
void write_stl(const std::string& path, const std::vector<TopoDS_Shape>& shapes,
               const StlWriteOptions& options = {});

struct StlReadOptions {
  // STL has no units: the length of one file unit in millimetres.
  double unit_mm = 1.0;
};

// Reads a binary or ASCII STL file as one mesh body named after the file.
// Coincident vertices are merged.
Body read_stl(const std::string& path, const StlReadOptions& options = {});

} // namespace mitcad::io
