// SPDX-License-Identifier: MIT
#include "mitcad/geometry/fill_cavity.hpp"

#include <array>
#include <cmath>
#include <cstddef>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>

#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopAbs.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/naming.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Axes whose cosine is below this are at right angles.
constexpr double kSquare = 1.0e-9;

gp_Vec vector(const std::array<double, 3>& v) { return gp_Vec(v[0], v[1], v[2]); }

// The box with its faces named: the top `spec.top`, the sides
// <feature>:fill_side(k) (k 0: u = min, 1: u = max, 2: v = min, 3: v = max)
// and the bottom <feature>:fill_bottom.
ShapePtr named_box(const CavitySpec& spec) {
  const gp_Vec x_vector = vector(spec.x_axis);
  const gp_Vec n_vector = vector(spec.normal);
  if (x_vector.Magnitude() < Precision::Confusion() || n_vector.Magnitude() < Precision::Confusion()) {
    throw std::invalid_argument("the cap's frame has no direction");
  }
  const gp_Dir x(x_vector);
  const gp_Dir n(n_vector);
  if (std::abs(x.Dot(n)) > kSquare) {
    throw std::invalid_argument("the cap's frame is not square");
  }
  const gp_Dir y = n.Crossed(x);
  std::array<double, 3> size{};
  for (std::size_t k = 0; k < 3; ++k) {
    size[k] = spec.max[k] - spec.min[k];
    if (!std::isfinite(size[k]) || size[k] <= Precision::Confusion()) {
      throw std::invalid_argument("the box below the cap has no volume");
    }
  }
  const gp_Pnt origin(spec.origin[0], spec.origin[1], spec.origin[2]);
  const gp_Pnt corner = origin.Translated(gp_Vec(x) * spec.min[0] + gp_Vec(y) * spec.min[1] + gp_Vec(n) * spec.min[2]);
  BRepPrimAPI_MakeBox maker(gp_Ax2(corner, n, x), size[0], size[1], size[2]);
  maker.Build();
  if (!maker.IsDone()) {
    throw std::runtime_error("the box below the cap could not be built");
  }
  const TopoDS_Shape solid = maker.Shape();
  // Each face by the plane of the box it lies on: its centre's distance
  // from the six planes in the frame.
  std::vector<Shape::NamedFace> names;
  for (TopExp_Explorer explorer(solid, TopAbs_FACE); explorer.More(); explorer.Next()) {
    const TopoDS_Face& face = TopoDS::Face(explorer.Current());
    GProp_GProps props;
    BRepGProp::SurfaceProperties(face, props);
    const gp_Vec at(origin, props.CentreOfMass());
    const std::array<double, 3> c{at.Dot(gp_Vec(x)), at.Dot(gp_Vec(y)), at.Dot(gp_Vec(n))};
    const std::array<double, 6> distance{std::abs(c[0] - spec.min[0]), std::abs(c[0] - spec.max[0]),
                                         std::abs(c[1] - spec.min[1]), std::abs(c[1] - spec.max[1]),
                                         std::abs(c[2] - spec.min[2]), std::abs(c[2] - spec.max[2])};
    std::size_t nearest = 0;
    for (std::size_t k = 1; k < distance.size(); ++k) {
      if (distance[k] < distance[nearest]) {
        nearest = k;
      }
    }
    std::string name;
    if (nearest == 5) {
      name = spec.top;
    } else if (nearest == 4) {
      name = face_name(spec.feature, "fill_bottom");
    } else {
      name = face_name(spec.feature, "fill_side", std::to_string(nearest));
    }
    names.push_back({face, {name}});
  }
  return std::make_shared<Shape>(solid, names);
}

} // namespace

std::vector<ShapePtr> cavity_pieces(const Shape& body, const CavitySpec& spec) {
  const ShapePtr box = detail::run("fill cavity", [&] {
    detail::require_surfaces(body.occt(), "fill cavity");
    return named_box(spec);
  });
  const BooleanResult result = boolean(BooleanOp::Cut, {box.get()}, body);
  std::vector<ShapePtr> pieces;
  if (result.touched.empty() || !result.touched[0]) {
    pieces.push_back(box);
    return pieces;
  }
  for (const BooleanPiece& piece : result.pieces) {
    pieces.push_back(piece.shape);
  }
  return pieces;
}

} // namespace mitcad::geometry
