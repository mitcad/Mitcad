// SPDX-License-Identifier: MIT
#include "face_select.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRep_Tool.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax3.hxx>

namespace mitcad::geometry::detail {
namespace {

// Faces meeting at less than this angle (radians) are tangent.
constexpr double kSmoothAngle = 1.0e-3;

void add_once(std::vector<int>& list, int value) {
  if (std::find(list.begin(), list.end(), value) == list.end()) {
    list.push_back(value);
  }
}

} // namespace

std::vector<int> resolve_faces(const Shape& body, const std::vector<std::string>& references,
                               const char* what) {
  if (references.empty()) {
    throw std::invalid_argument(std::string("no ") + what + " selected");
  }
  std::vector<int> faces;
  for (const std::string& reference : references) {
    const std::vector<int> found = body.find_faces(reference);
    if (found.empty()) {
      throw std::invalid_argument("the body has no face " + reference);
    }
    for (int i : found) {
      add_once(faces, i);
    }
  }
  return faces;
}

std::vector<int> resolve_edges(const Shape& body, const std::vector<std::string>& edges,
                               const std::vector<std::string>& faces) {
  if (edges.empty() && faces.empty()) {
    throw std::invalid_argument("no edges selected");
  }
  std::vector<int> result;
  for (const std::string& reference : edges) {
    const std::vector<int> found = body.find_edges(reference);
    if (found.empty()) {
      throw std::invalid_argument("the body has no edge " + reference);
    }
    for (int i : found) {
      add_once(result, i);
    }
  }
  if (!faces.empty()) {
    for (int f : resolve_faces(body, faces)) {
      for (TopExp_Explorer it(body.face(f), TopAbs_EDGE); it.More(); it.Next()) {
        const int i = body.edge_index(it.Current());
        if (i >= 0 && !BRep_Tool::Degenerated(body.edge(i))) {
          add_once(result, i);
        }
      }
    }
  }
  return result;
}

gp_Dir normal_on_edge(const TopoDS_Face& face, const TopoDS_Edge& edge, double t) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    throw std::runtime_error("an edge has no curve on its face");
  }
  const gp_Pnt2d uv = pcurve->Value(first + (last - first) * t);
  const BRepAdaptor_Surface surface(face, false);
  BRepLProp_SLProps props(surface, uv.X(), uv.Y(), 1, 1.0e-9);
  if (!props.IsNormalDefined()) {
    throw std::runtime_error("a face has no normal at an edge");
  }
  gp_Dir normal = props.Normal();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal;
}

double normal_angle(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b,
                    double t) {
  return normal_on_edge(a, edge, t).Angle(normal_on_edge(b, edge, t));
}

bool is_smooth(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b) {
  if (a.IsSame(b) || BRep_Tool::Degenerated(edge)) {
    return false;
  }
  for (const double t : {0.1, 0.5, 0.9}) {
    if (normal_angle(edge, a, b, t) > kSmoothAngle) {
      return false;
    }
  }
  return true;
}

std::vector<int> faces_at_edge(const Shape& body, int edge) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  std::vector<int> faces;
  const TopoDS_Edge& e = body.edge(edge);
  if (ancestors.Contains(e)) {
    for (const TopoDS_Shape& face : ancestors.FindFromKey(e)) {
      const int f = body.face_index(face);
      if (f >= 0) {
        add_once(faces, f);
      }
    }
  }
  return faces;
}

std::vector<int> tangent_closure(const Shape& body, std::vector<int> faces) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  for (std::size_t next = 0; next < faces.size(); ++next) {
    const TopoDS_Face& face = body.face(faces[next]);
    for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (!ancestors.Contains(edge)) {
        continue;
      }
      for (const TopoDS_Shape& other : ancestors.FindFromKey(edge)) {
        const int f = body.face_index(other);
        if (f >= 0 && std::find(faces.begin(), faces.end(), f) == faces.end() &&
            is_smooth(edge, face, TopoDS::Face(other))) {
          faces.push_back(f);
        }
      }
    }
  }
  return faces;
}

std::optional<gp_Pln> face_plane(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  gp_Pln plane = surface.Plane();
  if (face.Orientation() == TopAbs_REVERSED) {
    gp_Ax3 axes = plane.Position();
    axes.ZReverse();
    plane.SetPosition(axes);
  }
  return plane;
}

Bnd_Box box_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box, false);
  return box;
}

TopoDS_Face plane_face(const gp_Pln& plane, const Bnd_Box& box) {
  double size = 100.0;
  gp_Pnt centre = plane.Location();
  if (!box.IsVoid()) {
    double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
    box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    const gp_Pnt low(xmin, ymin, zmin);
    const gp_Pnt high(xmax, ymax, zmax);
    size = std::max(1.0, low.Distance(high)) * 2.0;
    // The box's centre projected onto the plane.
    const gp_Pnt middle((xmin + xmax) / 2, (ymin + ymax) / 2, (zmin + zmax) / 2);
    const gp_Vec offset(plane.Location(), middle);
    const gp_Vec normal(plane.Axis().Direction());
    centre = middle.Translated(-normal * offset.Dot(normal));
  }
  const gp_Pln local(gp_Ax3(centre, plane.Axis().Direction(), plane.XAxis().Direction()));
  BRepBuilderAPI_MakeFace maker(local, -size, size, -size, size);
  if (!maker.IsDone()) {
    throw std::runtime_error("the plane could not be built");
  }
  return maker.Face();
}

std::string primary_name(const Shape& body, int face) {
  const NameList& names = body.face_names(face);
  return names.empty() ? std::string() : names.front();
}

} // namespace mitcad::geometry::detail
