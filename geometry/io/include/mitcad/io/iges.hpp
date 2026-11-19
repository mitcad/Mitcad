// SPDX-License-Identifier: MIT
#pragma once

// IGES export and import.

#include <string>
#include <vector>

#include <Message_ProgressRange.hxx>

#include "mitcad/io/body.hpp"

namespace mitcad::io {

struct IgesWriteOptions {
  LengthUnit unit = LengthUnit::Millimeter;
  // Solids as manifold solid B-rep objects (IGES type 186); otherwise
  // trimmed surfaces only, which some older programs need.
  bool solids = true;
};

// Writes the bodies with names (ASCII only: other characters become '_')
// and colours, a moved copy per placement (see Body).
void write_iges(const std::string& path, const std::vector<Body>& bodies,
                const IgesWriteOptions& options = {});

// Reads an IGES file as bodies in millimetres. Loose faces are sewn and
// closed shells become solids, so a surface-only export of a solid comes
// back as a solid. As read_step, the parsing cannot be stopped and the
// transfer stops within `range`, throwing Cancelled (mitcad#2032).
std::vector<Body> read_iges(const std::string& path,
                            const Message_ProgressRange& range = Message_ProgressRange());

} // namespace mitcad::io
