// SPDX-License-Identifier: MIT
#include "mitcad/geometry/input_edges.hpp"

#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_TangentialDeflection.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Points along an edge's curve within the deflection and the angle, in the
// curve's own direction (BRepAdaptor_Curve places it by the edge's
// location); its two ends when the sampler gives fewer.
std::vector<gp_Pnt> polyline(const TopoDS_Edge& edge, double deflection, double angle) {
  const BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  std::vector<gp_Pnt> points;
  const GCPnts_TangentialDeflection sampler(curve, first, last, angle, deflection, 2);
  for (int i = 1; i <= sampler.NbPoints(); ++i) {
    points.push_back(sampler.Value(i));
  }
  if (points.size() < 2) {
    points = {curve.Value(first), curve.Value(last)};
  }
  return points;
}

} // namespace

std::vector<std::vector<gp_Pnt>> edge_polylines(const Shape& shape, const std::vector<std::string>& edges,
                                                const std::vector<std::string>& faces, double deflection,
                                                double angle) {
  return detail::run("edge polylines", [&] {
    if (!(deflection > 0.0) || !(angle > 0.0)) {
      throw std::runtime_error("edge polylines need a positive deflection and angle");
    }
    // Each edge once, the named edges first, then the faces' boundaries.
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> picked;
    for (const std::string& name : edges) {
      const std::vector<int> found = shape.find_edges(name);
      if (found.empty()) {
        throw std::runtime_error("the body has no edge " + name);
      }
      for (const int e : found) {
        picked.Add(shape.edge(e));
      }
    }
    for (const std::string& name : faces) {
      const std::vector<int> found = shape.find_faces(name);
      if (found.empty()) {
        throw std::runtime_error("the body has no face " + name);
      }
      for (const int f : found) {
        for (TopExp_Explorer it(shape.face(f), TopAbs_EDGE); it.More(); it.Next()) {
          // The same edge as the body's, whatever its orientation in the
          // face's wire: the hasher ignores the orientation.
          picked.Add(it.Current());
        }
      }
    }
    std::vector<std::vector<gp_Pnt>> lines;
    lines.reserve(static_cast<std::size_t>(picked.Extent()));
    for (int i = 1; i <= picked.Extent(); ++i) {
      const TopoDS_Edge& edge = TopoDS::Edge(picked(i));
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      lines.push_back(polyline(edge, deflection, angle));
    }
    return lines;
  });
}

} // namespace mitcad::geometry
