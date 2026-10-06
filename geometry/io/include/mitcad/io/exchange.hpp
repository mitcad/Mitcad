// SPDX-License-Identifier: MIT
#pragma once

// Reading and writing by file extension: .step/.stp, .iges/.igs, .stl,
// .obj and .brep (OCCT's native format, for debugging).

#include <optional>
#include <string>
#include <vector>

#include "mitcad/io/body.hpp"
#include "mitcad/io/iges.hpp"
#include "mitcad/io/mesh.hpp"
#include "mitcad/io/obj.hpp"
#include "mitcad/io/step.hpp"
#include "mitcad/io/stl.hpp"

namespace mitcad::io {

enum class Format { Step, Iges, Stl, Obj, Brep };

// The format of a path's extension (case-insensitive), if known.
std::optional<Format> format_of(const std::string& path);

// Options of every format for write_file; each format uses its own.
struct WriteOptions {
  MeshOptions mesh = MeshOptions::from(Refinement::Medium);
  StlFormat stl_format = StlFormat::Binary;
  StepSchema step_schema = StepSchema::AP214;
  LengthUnit unit = LengthUnit::Millimeter;
};

// Reads a file by its extension. Mesh files take millimetres as their unit.
std::vector<Body> read_file(const std::string& path);

// Writes the bodies to a file by its extension, each where its placements
// say (see Body).
void write_file(const std::string& path, const std::vector<Body>& bodies,
                const WriteOptions& options = {});

// Writes the bodies in OCCT's BRep format: the shape of one body, or a
// compound of them, a moved copy per placement.
void write_brep(const std::string& path, const std::vector<Body>& bodies);

} // namespace mitcad::io
