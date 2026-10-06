// SPDX-License-Identifier: MIT
#pragma once

// Wavefront OBJ export and import of meshes.

#include <string>
#include <vector>

#include "mitcad/io/body.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad::io {

struct ObjWriteOptions {
  MeshOptions mesh = MeshOptions::from(Refinement::Medium);
};

// Writes the bodies, triangulated with `options.mesh`, as named OBJ
// objects, a moved copy per placement (see Body); colours go to a
// material file next to it (`<name>.mtl`).
void write_obj(const std::string& path, const std::vector<Body>& bodies,
               const ObjWriteOptions& options = {});

struct ObjReadOptions {
  // OBJ has no units: the length of one file unit in millimetres.
  double unit_mm = 1.0;
};

// Reads an OBJ file as mesh bodies, one per object or group, with names and
// material colours.
std::vector<Body> read_obj(const std::string& path, const ObjReadOptions& options = {});

} // namespace mitcad::io
