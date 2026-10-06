// SPDX-License-Identifier: MIT
#pragma once

// Bodies without history (base features): B-rep data in memory, face names
// by index, and what a body is made of. Files are read and written by the
// data exchange library (mitcad/io); this is what the model stores.

#include <cstddef>
#include <string>
#include <vector>

#include <TopoDS_Shape.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {

// The shape in OCCT's binary B-rep format (BinTools, format version 4).
// Triangulations of faces with surfaces are left out (they are made again
// for display); faces without a surface (mesh bodies) keep theirs.
std::string brep_data(const TopoDS_Shape& shape);

// Reads binary B-rep data, or OCCT's text format (BRepTools, also with a
// DRAW header). Throws std::runtime_error when the data is not a shape.
TopoDS_Shape read_brep_data(const char* data, std::size_t size);

// The shape with face i (in Shape's face order) named
// "<feature>:import(<first_face + i>)".
ShapePtr import_body(const TopoDS_Shape& shape, const std::string& feature, int first_face);

// A compound of the shapes with their face names.
ShapePtr compound(const std::vector<const Shape*>& shapes);

enum class BodyKind {
  Empty, // no faces
  Solid, // has solids
  Sheet, // faces with surfaces but no solid
  Mesh,  // only faces without surfaces (triangulations)
};

BodyKind body_kind(const TopoDS_Shape& shape);

} // namespace mitcad::geometry
