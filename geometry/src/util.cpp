// SPDX-License-Identifier: MIT
#include "util.hpp"

#include <cctype>
#include <cmath>
#include <sstream>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <TopExp_Explorer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopoDS.hxx>

namespace mitcad::geometry::detail {
namespace {

long long rounded(double value) { return std::llround(value * 1.0e6); }

} // namespace

void require_positive(const char* what, double value) {
  if (!(value > 0.0) || !std::isfinite(value)) {
    throw std::invalid_argument(std::string(what) + " must be greater than zero");
  }
}

void require_finite(const char* what, double value) {
  if (!std::isfinite(value)) {
    throw std::invalid_argument(std::string(what) + " must be a finite number");
  }
}

ShapeKey shape_key(const TopoDS_Shape& shape) {
  GProp_GProps props;
  switch (shape.ShapeType()) {
  case TopAbs_SOLID:
  case TopAbs_COMPSOLID:
  case TopAbs_COMPOUND:
    BRepGProp::VolumeProperties(shape, props);
    break;
  case TopAbs_FACE:
  case TopAbs_SHELL:
    BRepGProp::SurfaceProperties(shape, props);
    break;
  case TopAbs_EDGE:
  case TopAbs_WIRE:
    BRepGProp::LinearProperties(shape, props);
    break;
  default:
    break;
  }
  gp_Pnt centre;
  if (shape.ShapeType() == TopAbs_VERTEX) {
    centre = BRep_Tool::Pnt(TopoDS::Vertex(shape));
  } else {
    centre = props.CentreOfMass();
  }
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  if (!box.IsVoid()) {
    box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  }
  return {rounded(centre.X()), rounded(centre.Y()), rounded(centre.Z()),
          rounded(xmin),       rounded(ymin),       rounded(zmin)};
}

ShapeKey edge_key(const TopoDS_Shape& edge) {
  const TopoDS_Edge& e = TopoDS::Edge(edge);
  if (BRep_Tool::Degenerated(e)) {
    return shape_key(edge);
  }
  const BRepAdaptor_Curve curve(e);
  const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
  const ShapeKey whole = shape_key(edge);
  return {rounded(middle.X()), rounded(middle.Y()), rounded(middle.Z()),
          whole[0],           whole[1],           whole[2]};
}

namespace {

std::string status_name(BRepCheck_Status status) {
  std::ostringstream text;
  BRepCheck::Print(status, text);
  std::string name = text.str();
  while (!name.empty() && std::isspace(static_cast<unsigned char>(name.back()))) {
    name.pop_back();
  }
  return name;
}

// The first problem the analyzer found, as " (<kind>: BRepCheck_...)" for
// messages: of a sub-shape itself, or of one in the context of another.
std::string first_problem(const BRepCheck_Analyzer& analyzer, const TopoDS_Shape& shape) {
  static const std::pair<TopAbs_ShapeEnum, const char*> kinds[] = {
      {TopAbs_VERTEX, "vertex"}, {TopAbs_EDGE, "edge"},   {TopAbs_WIRE, "wire"},
      {TopAbs_FACE, "face"},     {TopAbs_SHELL, "shell"}, {TopAbs_SOLID, "solid"}};
  for (const auto& [type, kind] : kinds) {
    for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
      const occ::handle<BRepCheck_Result>& result = analyzer.Result(it.Current());
      if (result.IsNull()) {
        continue;
      }
      for (const BRepCheck_Status status : result->Status()) {
        if (status != BRepCheck_NoError) {
          return std::string(" (") + kind + ": " + status_name(status) + ")";
        }
      }
      // Sub-shapes checked in this one's context (wires of a face).
      for (const auto& [sub_type, sub_kind] : kinds) {
        for (TopExp_Explorer sub(it.Current(), sub_type); sub.More(); sub.Next()) {
          if (!result->IsStatusOnShape(sub.Current())) {
            continue;
          }
          for (const BRepCheck_Status status : result->StatusOnShape(sub.Current())) {
            if (status != BRepCheck_NoError) {
              return std::string(" (") + sub_kind + " of a " + kind + ": " + status_name(status) + ")";
            }
          }
        }
      }
    }
  }
  return "";
}

} // namespace

// (The checker catches an allocation that fails and calls the sub-shape
// invalid, mitcad#132: that is no answer.)
bool is_valid(const TopoDS_Shape& shape) {
  throw_if_cancelled();
  const std::size_t failed = failed_allocations_in_thread();
  const bool valid = BRepCheck_Analyzer(shape).IsValid();
  throw_if_allocation_failed(failed);
  return valid;
}

void require_valid(const TopoDS_Shape& shape, const char* operation) {
  throw_if_cancelled();
  const std::size_t failed = failed_allocations_in_thread();
  const BRepCheck_Analyzer analyzer(shape);
  throw_if_allocation_failed(failed);
  if (!analyzer.IsValid()) {
    throw std::runtime_error(std::string(operation) + " produced an invalid shape" + first_problem(analyzer, shape));
  }
}

} // namespace mitcad::geometry::detail
