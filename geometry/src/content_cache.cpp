// SPDX-License-Identifier: MIT
// What is computed of a shape again and again, kept by the shape's content
// (content_cache.hpp, mitcad#1801).
#include "content_cache.hpp"

#include <ios>
#include <sstream>

#include <BinTools.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>

#include "history.hpp"

namespace mitcad::geometry::detail {

std::string shape_bytes(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    return {};
  }
  try {
    OCC_CATCH_SIGNALS
    std::ostringstream stream(std::ios::out | std::ios::binary);
    // (Written as a copy: the same geometry and topology with new shapes,
    // whose flags (checked, modified, free) are the same however the
    // shape was used.)
    BinTools::Write(InputCopy(shape).shape(), stream, true, false, BinTools_FormatVersion_VERSION_4);
    return stream ? stream.str() : std::string();
  } catch (const Standard_Failure&) {
    return {};
  }
}

} // namespace mitcad::geometry::detail
