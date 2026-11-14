// SPDX-License-Identifier: MIT
#include "mitcad/geometry/smooth_seams.hpp"

#include <cctype>

#include <BRepLib.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_Shape.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>

namespace mitcad::geometry {

std::string unsplit_face_name(const std::string& name) {
  std::size_t end = name.size();
  for (;;) {
    std::size_t digits = end;
    while (digits > 0 && std::isdigit(static_cast<unsigned char>(name[digits - 1])) != 0) {
      --digits;
    }
    if (digits == end || digits == 0 || name[digits - 1] != '#') {
      break;
    }
    end = digits - 1;
  }
  return name.substr(0, end);
}

bool smooth_across(const TopoDS_Edge& edge, const TopoDS_Face& first, const TopoDS_Face& second) {
  if (edge.IsNull() || first.IsNull() || second.IsNull()) {
    return false;
  }
  try {
    if (BRep_Tool::Degenerated(edge) || !BRep_Tool::IsGeometric(edge)) {
      return false;
    }
    // A continuity the builder coded (a primitive's seam, a fillet's
    // boundary) is taken as it is.
    if (BRep_Tool::Continuity(edge, first, second) > GeomAbs_C0) {
      return true;
    }
    // Sampled along the edge: the surfaces' normals on both sides, and
    // their derivatives across it (a seam on a periodic elementary surface
    // is CN without sampling).
    return BRepLib::ContinuityOfFaces(edge, first, second, kSmoothSeamAngle) >= GeomAbs_G1;
  } catch (const Standard_Failure&) {
    return false;
  }
}

namespace {

// Whether two different faces lie on one surface: OCCT's same surface at
// the same place, or two pieces of one named face.
bool one_surface(const TopoDS_Face& first, const TopoDS_Face& second, const Shape* named) {
  TopLoc_Location at_first;
  TopLoc_Location at_second;
  const occ::handle<Geom_Surface>& surface_first = BRep_Tool::Surface(first, at_first);
  const occ::handle<Geom_Surface>& surface_second = BRep_Tool::Surface(second, at_second);
  if (!surface_first.IsNull() && surface_first.get() == surface_second.get() && at_first.IsEqual(at_second)) {
    return true;
  }
  if (named == nullptr) {
    return false;
  }
  const int i = named->face_index(first);
  const int j = named->face_index(second);
  if (i < 0 || j < 0) {
    return false;
  }
  for (const std::string& a : named->face_names(i)) {
    const std::string whole = unsplit_face_name(a);
    if (whole.empty()) {
      continue;
    }
    for (const std::string& b : named->face_names(j)) {
      if (unsplit_face_name(b) == whole) {
        return true;
      }
    }
  }
  return false;
}

} // namespace

ShapeMap smooth_seams(const TopoDS_Shape& shape, const Shape* named) {
  ShapeMap seams;
  if (shape.IsNull()) {
    return seams;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_at;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_at);
  for (int i = 1; i <= faces_at.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces_at.FindKey(i));
    const NCollection_List<TopoDS_Shape>& faces = faces_at(i);
    if (faces.Extent() == 1) {
      // A seam: the face on both sides.
      const TopoDS_Face& face = TopoDS::Face(faces.First());
      if (BRep_Tool::IsClosed(edge, face) && smooth_across(edge, face, face)) {
        seams.Add(edge);
      }
    } else if (faces.Extent() == 2) {
      const TopoDS_Face& first = TopoDS::Face(faces.First());
      const TopoDS_Face& second = TopoDS::Face(faces.Last());
      if (one_surface(first, second, named) && smooth_across(edge, first, second)) {
        seams.Add(edge);
      }
    }
  }
  return seams;
}

} // namespace mitcad::geometry
