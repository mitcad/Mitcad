// SPDX-License-Identifier: MIT
#include "bridge/point_clouds.hpp"

#include <stdexcept>
#include <vector>

#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/input_check.hpp"
#include "mitcad_bridge/kernel/point_clouds.h"

namespace mitcad::bridge {

rust::Vec<double> boundary_nearest(const geometry::Shape& shape, rust::Slice<const rust::String> faces,
                                   rust::Slice<const double> points) {
  const geometry::CheckedOperation check("closest points on faces");
  if (points.size() % 3 != 0) {
    throw std::invalid_argument("points come as x, y, z triples");
  }
  // The faces asked about as one compound: the distance is to their
  // surfaces, edges and vertices, never to a solid's inside (a point inside
  // the material is as far from the faces as it is).
  std::vector<int> indices;
  if (faces.empty()) {
    for (int f = 0; f < shape.face_count(); ++f) {
      indices.push_back(f);
    }
  } else {
    for (const rust::String& name : faces) {
      const std::vector<int> found = shape.find_faces(std::string(name));
      if (found.empty()) {
        throw std::invalid_argument("the body has no face " + std::string(name));
      }
      indices.insert(indices.end(), found.begin(), found.end());
    }
  }
  if (indices.empty()) {
    throw std::invalid_argument("the body has no faces to measure from");
  }
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const int f : indices) {
    builder.Add(compound, shape.face(f));
  }
  // The faces' boxes are set up once (LoadS2) and kept for every point.
  BRepExtrema_DistShapeShape distance;
  distance.LoadS2(compound);
  rust::Vec<double> result;
  result.reserve(points.size() / 3 * 4);
  for (std::size_t i = 0; i + 2 < points.size(); i += 3) {
    const gp_Pnt p(points[i], points[i + 1], points[i + 2]);
    distance.LoadS1(BRepBuilderAPI_MakeVertex(p).Vertex());
    if (!distance.Perform() || !distance.IsDone() || distance.NbSolution() < 1) {
      throw std::runtime_error("the distance of a point from the faces could not be measured");
    }
    const gp_Pnt nearest = distance.PointOnShape2(1);
    result.push_back(distance.Value());
    result.push_back(nearest.X());
    result.push_back(nearest.Y());
    result.push_back(nearest.Z());
  }
  return result;
}

} // namespace mitcad::bridge
