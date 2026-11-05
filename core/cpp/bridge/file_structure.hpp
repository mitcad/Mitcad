// SPDX-License-Identifier: MIT
#pragma once

// C++ half of core/ffi/src/kernel/file_structure.rs (mitcad#1274).

#include "bridge/shape.hpp"
#include "rust/cxx.h"

namespace mitcad::bridge {

// Shared with Rust; defined in the generated
// mitcad_bridge/kernel/file_structure.h.
struct FileStructure;

// A STEP file's assembly structure (io::read_step_structured), or its
// bodies; the shapes go into `shapes`.
FileStructure read_step_structure(rust::Str path, bool keep_assembly, ShapeList& shapes);

} // namespace mitcad::bridge
