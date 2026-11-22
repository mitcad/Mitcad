// SPDX-License-Identifier: MIT
#include "mitcad/geometry/smooth_seams.hpp"

#include <cctype>
#include <cmath>
#include <cstddef>
#include <numeric>
#include <unordered_map>

#include <BRepAdaptor_Surface.hxx>
#include <BRepLib.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>

namespace mitcad::geometry {

std::string unsplit_face_name(const std::string& name) {
  std::string whole;
  whole.reserve(name.size());
  std::size_t i = 0;
  while (i < name.size()) {
    if (name[i] == '#') {
      std::size_t digits = i + 1;
      while (digits < name.size() && std::isdigit(static_cast<unsigned char>(name[digits])) != 0) {
        ++digits;
      }
      // A piece number: digits after the '#' up to the end of the name or
      // a character that is no part of a word (`)`, `]`, `|`, another '#').
      const bool ends = digits == name.size() ||
                        (std::isalnum(static_cast<unsigned char>(name[digits])) == 0 && name[digits] != '_');
      if (digits > i + 1 && ends) {
        i = digits;
        continue;
      }
    }
    whole += name[i];
    ++i;
  }
  return whole;
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

constexpr double kPi = 3.14159265358979323846;

bool analytic(GeomAbs_SurfaceType type) {
  return type == GeomAbs_Plane || type == GeomAbs_Cylinder || type == GeomAbs_Cone || type == GeomAbs_Sphere ||
         type == GeomAbs_Torus;
}

bool same_length(double a, double b) { return std::abs(a - b) <= kOneSurfaceLength; }

// Two directions along one line, either way.
bool parallel(const gp_Dir& a, const gp_Dir& b) {
  const double angle = a.Angle(b);
  return angle <= kOneSurfaceAngle || kPi - angle <= kOneSurfaceAngle;
}

// Two axes as lines: parallel, and each one's origin on the other's line
// (wherever on the line the origins are).
bool same_line(const gp_Ax1& a, const gp_Ax1& b) {
  return parallel(a.Direction(), b.Direction()) && gp_Lin(a).Distance(b.Location()) <= kOneSurfaceLength &&
         gp_Lin(b).Distance(a.Location()) <= kOneSurfaceLength;
}

bool same_point(const gp_Pnt& a, const gp_Pnt& b) { return a.Distance(b) <= kOneSurfaceLength; }

// Two analytic surfaces of one type, placed: whether they are one surface.
bool same_surface(const BRepAdaptor_Surface& a, const BRepAdaptor_Surface& b) {
  switch (a.GetType()) {
  case GeomAbs_Plane: {
    const gp_Pln p = a.Plane();
    const gp_Pln q = b.Plane();
    return parallel(p.Axis().Direction(), q.Axis().Direction()) && p.Distance(q.Location()) <= kOneSurfaceLength &&
           q.Distance(p.Location()) <= kOneSurfaceLength;
  }
  case GeomAbs_Cylinder: {
    const gp_Cylinder p = a.Cylinder();
    const gp_Cylinder q = b.Cylinder();
    return same_line(p.Axis(), q.Axis()) && same_length(p.Radius(), q.Radius());
  }
  case GeomAbs_Cone: {
    // The whole cone through the apex: the half-angle's size, the axis
    // either way.
    const gp_Cone p = a.Cone();
    const gp_Cone q = b.Cone();
    return same_point(p.Apex(), q.Apex()) && parallel(p.Axis().Direction(), q.Axis().Direction()) &&
           std::abs(std::abs(p.SemiAngle()) - std::abs(q.SemiAngle())) <= kOneSurfaceAngle;
  }
  case GeomAbs_Sphere: {
    const gp_Sphere p = a.Sphere();
    const gp_Sphere q = b.Sphere();
    return same_point(p.Location(), q.Location()) && same_length(p.Radius(), q.Radius());
  }
  case GeomAbs_Torus: {
    const gp_Torus p = a.Torus();
    const gp_Torus q = b.Torus();
    return same_line(p.Axis(), q.Axis()) && same_point(p.Location(), q.Location()) &&
           same_length(p.MajorRadius(), q.MajorRadius()) && same_length(p.MinorRadius(), q.MinorRadius());
  }
  default:
    return true;
  }
}

// Whether two different faces lie on one surface: OCCT's same surface at
// the same place, or two pieces of one named face that are not provably on
// different analytic surfaces.
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
  bool pieces = false;
  for (const std::string& a : named->face_names(i)) {
    const std::string whole = unsplit_face_name(a);
    if (whole.empty()) {
      continue;
    }
    for (const std::string& b : named->face_names(j)) {
      if (unsplit_face_name(b) == whole) {
        pieces = true;
        break;
      }
    }
    if (pieces) {
      break;
    }
  }
  return pieces && !different_analytic_surfaces(first, second);
}

// Whether the edge between two different faces is left out: one surface,
// smooth across.
bool hidden_between(const TopoDS_Edge& edge, const TopoDS_Face& first, const TopoDS_Face& second,
                    const Shape* named) {
  return one_surface(first, second, named) && smooth_across(edge, first, second);
}

} // namespace

bool different_analytic_surfaces(const TopoDS_Face& first, const TopoDS_Face& second) {
  if (first.IsNull() || second.IsNull()) {
    return false;
  }
  try {
    // With the faces' locations; no bounds are needed.
    const BRepAdaptor_Surface a(first, false);
    const BRepAdaptor_Surface b(second, false);
    if (!analytic(a.GetType()) || !analytic(b.GetType())) {
      return false;
    }
    if (a.GetType() != b.GetType()) {
      return true;
    }
    return !same_surface(a, b);
  } catch (const Standard_Failure&) {
    return false;
  }
}

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
      if (hidden_between(edge, first, second, named)) {
        seams.Add(edge);
      }
    }
  }
  return seams;
}

std::vector<std::uint32_t> smooth_surface_groups(const Shape& shape) {
  const int count = shape.face_count();
  std::vector<int> root(static_cast<std::size_t>(count > 0 ? count : 0));
  std::iota(root.begin(), root.end(), 0);
  const auto find = [&root](int face) {
    while (root[static_cast<std::size_t>(face)] != face) {
      int& up = root[static_cast<std::size_t>(face)];
      up = root[static_cast<std::size_t>(up)];
      face = up;
    }
    return face;
  };
  if (count > 0 && !shape.occt().IsNull()) {
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_at;
    TopExp::MapShapesAndUniqueAncestors(shape.occt(), TopAbs_EDGE, TopAbs_FACE, faces_at);
    for (int i = 1; i <= faces_at.Extent(); ++i) {
      const NCollection_List<TopoDS_Shape>& faces = faces_at(i);
      if (faces.Extent() != 2) {
        continue;
      }
      const TopoDS_Face& first = TopoDS::Face(faces.First());
      const TopoDS_Face& second = TopoDS::Face(faces.Last());
      const int a = shape.face_index(first);
      const int b = shape.face_index(second);
      if (a < 0 || b < 0 || a == b) {
        continue;
      }
      if (hidden_between(TopoDS::Edge(faces_at.FindKey(i)), first, second, &shape)) {
        const int ra = find(a);
        const int rb = find(b);
        root[static_cast<std::size_t>(ra > rb ? ra : rb)] = ra < rb ? ra : rb;
      }
    }
  }
  std::vector<std::uint32_t> groups(root.size());
  std::unordered_map<int, std::uint32_t> number;
  for (int face = 0; face < count; ++face) {
    const int top = find(face);
    const auto [at, added] = number.emplace(top, static_cast<std::uint32_t>(number.size()));
    (void)added;
    groups[static_cast<std::size_t>(face)] = at->second;
  }
  return groups;
}

} // namespace mitcad::geometry
