// SPDX-License-Identifier: MIT
#pragma once

// STEP (ISO 10303-21) export and import of solids with names and colours.

#include <string>
#include <vector>

#include "mitcad/io/body.hpp"

namespace mitcad::io {

enum class StepSchema {
  AP214, // AP214 IS (automotive design), the most widely read
  AP242, // AP242 DIS (managed model based 3D engineering)
};

struct StepWriteOptions {
  StepSchema schema = StepSchema::AP214;
  // Unit of the file; the bodies are in millimetres and are converted.
  LengthUnit unit = LengthUnit::Millimeter;
};

// Writes each body as a product (part) with its name and colour. When a
// body has placements that move it (or several), the parts go into one
// assembly named after the file, an instance of a part per placement
// (named after it), and bodies without placements once as they are.
void write_step(const std::string& path, const std::vector<Body>& bodies,
                const StepWriteOptions& options = {});

// Reads the parts of a STEP file, with assembly placements applied, as
// bodies in millimetres. A part made of several solids gives one body per
// solid; the bodies take names from the solids when the file names them,
// otherwise from the part (with " (2)", " (3)", ... for the further solids).
// Colours of instances, solids and parts are read in that priority; a body
// without one takes the colour most of its faces have.
std::vector<Body> read_step(const std::string& path);

} // namespace mitcad::io
