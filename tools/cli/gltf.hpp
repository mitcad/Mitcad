// SPDX-License-Identifier: MIT
#pragma once

// mitcad-cli export to .glb and .gltf (mitcad#2210, gltf.cpp): File >
// Export's glTF writer (app/web/GltfExport.hpp), not the model's export
// command.

#include <string>

#include "import.hpp"

namespace mitcad::cli {

// Whether `path` names a glTF file: it ends in .glb or .gltf (any case).
bool is_gltf_file(const std::string& path);

// `mitcad-cli export <design> <out.glb|out.gltf> ...`, its arguments parsed
// as the export's (positional: the design and the file); the exit status.
int export_gltf(const Arguments& arguments);

} // namespace mitcad::cli
