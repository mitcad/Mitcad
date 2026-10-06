// SPDX-License-Identifier: MIT
#include "mitcad/geometry/import.hpp"

#include <algorithm>
#include <memory>
#include <sstream>
#include <stdexcept>

#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BinTools.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>

#include "util.hpp"

namespace mitcad::geometry {

std::string brep_data(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    throw std::invalid_argument("B-rep data: the shape is empty");
  }
  return detail::run("B-rep data", [&] {
    std::ostringstream out(std::ios::out | std::ios::binary);
    BinTools::Write(shape, out, false, false, BinTools_FormatVersion_VERSION_4);
    if (!out) {
      throw std::runtime_error("B-rep data: writing failed");
    }
    return out.str();
  });
}

TopoDS_Shape read_brep_data(const char* data, std::size_t size) {
  // BinTools starts with a line "Open CASCADE Topology V<n>, ..." after a
  // newline; the text format with "CASCADE Topology V<n>, ...", possibly
  // after a DRAW header.
  const std::string head(data, std::min<std::size_t>(size, 64));
  const bool binary = head.find("Open CASCADE Topology") != std::string::npos;
  TopoDS_Shape shape = detail::run("B-rep data", [&] {
    std::istringstream in(std::string(data, size), std::ios::in | std::ios::binary);
    TopoDS_Shape read;
    if (binary) {
      BinTools::Read(read, in);
    } else {
      BRepTools::Read(read, in, BRep_Builder());
    }
    return read;
  });
  if (shape.IsNull()) {
    throw std::runtime_error("B-rep data: no shape could be read");
  }
  return shape;
}

ShapePtr import_body(const TopoDS_Shape& shape, const std::string& feature, int first_face) {
  ShapeMap faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  std::vector<Shape::NamedFace> named;
  named.reserve(static_cast<std::size_t>(faces.Extent()));
  for (int i = 1; i <= faces.Extent(); ++i) {
    named.push_back({faces(i), {face_name(feature, "import", std::to_string(first_face + i - 1))}});
  }
  return std::make_shared<Shape>(shape, named);
}

ShapePtr compound(const std::vector<const Shape*>& shapes) {
  BRep_Builder builder;
  TopoDS_Compound result;
  builder.MakeCompound(result);
  std::vector<Shape::NamedFace> named;
  for (const Shape* shape : shapes) {
    builder.Add(result, shape->occt());
    for (int i = 0; i < shape->face_count(); ++i) {
      if (!shape->face_names(i).empty()) {
        named.push_back({shape->face(i), shape->face_names(i)});
      }
    }
  }
  return std::make_shared<Shape>(result, named);
}

BodyKind body_kind(const TopoDS_Shape& shape) {
  bool faces = false;
  bool surfaces = false;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    faces = true;
    TopLoc_Location location;
    if (!BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      surfaces = true;
      break;
    }
  }
  if (!faces) {
    return BodyKind::Empty;
  }
  if (!surfaces) {
    return BodyKind::Mesh;
  }
  return TopExp_Explorer(shape, TopAbs_SOLID).More() ? BodyKind::Solid : BodyKind::Sheet;
}

} // namespace mitcad::geometry
