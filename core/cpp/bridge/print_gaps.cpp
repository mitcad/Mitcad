// SPDX-License-Identifier: MIT
#include "bridge/print_gaps.hpp"

#include <algorithm>
#include <array>
#include <cstddef>
#include <numeric>
#include <vector>

#include <BRepGProp_Face.hxx>
#include <BRep_Tool.hxx>
#include <Geom_Surface.hxx>
#include <Precision.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <TopoDS_Face.hxx>
#include <gp.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "mitcad/analysis/measure.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad_bridge/kernel/print_gaps.h"

namespace mitcad::bridge {
namespace {

// {xmin, ymin, zmin, xmax, ymax, zmax}, as geometry::face_boxes gives them.
using FaceBox = std::array<double, 6>;

// Whether two boxes lie within `within` of each other on every axis.
bool boxes_meet(const FaceBox& a, const FaceBox& b, double within) {
  for (std::size_t k = 0; k < 3; ++k) {
    if (a[k] > b[k + 3] + within || b[k] > a[k + 3] + within) {
      return false;
    }
  }
  return true;
}

// The face's outward unit normal at a point on it (the face as it is
// oriented in its body), zero where it has none.
std::array<double, 3> outward_normal(const TopoDS_Face& face, const analysis::Vec3& point) {
  const std::array<double, 3> none{0.0, 0.0, 0.0};
  const Handle(Geom_Surface) surface = BRep_Tool::Surface(face);
  if (surface.IsNull()) {
    return none;
  }
  ShapeAnalysis_Surface parameters(surface);
  const gp_Pnt2d uv = parameters.ValueOfUV(gp_Pnt(point.x, point.y, point.z), Precision::Confusion());
  // BRepGProp_Face reverses the normal of a reversed face.
  BRepGProp_Face props(face);
  gp_Pnt at;
  gp_Vec normal;
  props.Normal(uv.X(), uv.Y(), at, normal);
  const double length = normal.Magnitude();
  if (!(length > gp::Resolution())) {
    return none;
  }
  normal.Divide(length);
  return {normal.X(), normal.Y(), normal.Z()};
}

} // namespace

rust::Vec<PrintFaceGap> face_separations(const geometry::Shape& a, const geometry::Shape& b, double within) {
  const std::vector<FaceBox> boxes_a = geometry::face_boxes(a);
  const std::vector<FaceBox> boxes_b = geometry::face_boxes(b);
  // The faces of b by the low x of their boxes: a face of a meets only those
  // that start before its box ends.
  std::vector<std::size_t> order(boxes_b.size());
  std::iota(order.begin(), order.end(), std::size_t{0});
  std::sort(order.begin(), order.end(),
            [&boxes_b](std::size_t x, std::size_t y) { return boxes_b[x][0] < boxes_b[y][0]; });
  rust::Vec<PrintFaceGap> found;
  for (std::size_t i = 0; i < boxes_a.size(); ++i) {
    const FaceBox& box_a = boxes_a[i];
    for (const std::size_t j : order) {
      const FaceBox& box_b = boxes_b[j];
      if (box_b[0] > box_a[3] + within) {
        break;
      }
      if (!boxes_meet(box_a, box_b, within)) {
        continue;
      }
      const TopoDS_Face& face_a = a.face(static_cast<int>(i));
      const TopoDS_Face& face_b = b.face(static_cast<int>(j));
      const analysis::Distance distance = analysis::min_distance(face_a, face_b);
      if (!(distance.value < within)) {
        continue;
      }
      PrintFaceGap gap{};
      gap.face_a = i;
      gap.face_b = j;
      gap.distance = distance.value;
      gap.on_a = {distance.on_a.x, distance.on_a.y, distance.on_a.z};
      gap.on_b = {distance.on_b.x, distance.on_b.y, distance.on_b.z};
      // Faces that touch need no normals: their pair is a contact.
      if (distance.value > Precision::Confusion()) {
        gap.normal_a = outward_normal(face_a, distance.on_a);
        gap.normal_b = outward_normal(face_b, distance.on_b);
      }
      found.push_back(gap);
    }
  }
  return found;
}

} // namespace mitcad::bridge
