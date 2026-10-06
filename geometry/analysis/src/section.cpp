// SPDX-License-Identifier: MIT
#include "mitcad/analysis/section.hpp"

#include <cmath>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pln.hxx>

#include "convert.hpp"

namespace mitcad::analysis {

namespace {

// Builds the boolean of `object` with `tool` without changing either: the
// shape is a body of the model, which other results share.
void build_keeping_inputs(BRepAlgoAPI_BooleanOperation& operation, const TopoDS_Shape& object,
                          const TopoDS_Shape& tool) {
  NCollection_List<TopoDS_Shape> objects;
  objects.Append(object);
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(tool);
  operation.SetArguments(objects);
  operation.SetTools(tools);
  operation.SetNonDestructive(true);
  operation.Build();
}

gp_Dir direction(const Plane& plane) {
  const gp_Vec normal(plane.normal.x, plane.normal.y, plane.normal.z);
  if (normal.Magnitude() <= gp::Resolution()) {
    throw Error("the section plane needs a non-zero normal");
  }
  return gp_Dir(normal);
}

// A square face in the plane, centred where the plane meets the line from
// the shape's box centre along the normal, large enough to cover the shape.
// `distance` is the signed distance of the box centre from the plane.
TopoDS_Face cover(const TopoDS_Shape& shape, const Plane& plane, double& size, double& distance) {
  const Bounds box = bounds(shape);
  if (box.empty) {
    throw Error("cannot section an empty shape");
  }
  const Vec3 extent = box.size();
  size = std::sqrt(extent.x * extent.x + extent.y * extent.y + extent.z * extent.z) + 1.0;
  const gp_Dir normal = direction(plane);
  const gp_Pnt center(0.5 * (box.min.x + box.max.x), 0.5 * (box.min.y + box.max.y),
                      0.5 * (box.min.z + box.max.z));
  distance = gp_Vec(detail::pnt(plane.origin), center).Dot(gp_Vec(normal));
  const gp_Pnt on_plane = center.Translated(gp_Vec(normal) * -distance);
  return BRepBuilderAPI_MakeFace(gp_Pln(on_plane, normal), -size, size, -size, size).Face();
}

} // namespace

Section section(const TopoDS_Shape& shape, const Plane& plane) {
  if (shape.IsNull()) {
    throw Error("cannot section a null shape");
  }
  Section result;
  BRepAlgoAPI_Section curves(shape, gp_Pln(detail::pnt(plane.origin), direction(plane)), false);
  curves.Approximation(true);
  // The shape is a body of the model, which other results share: the
  // booleans here leave its sub-shapes as they are.
  curves.SetNonDestructive(true);
  curves.Build();
  if (!curves.IsDone() || curves.HasErrors()) {
    throw Error("the section could not be computed");
  }
  result.curves = curves.Shape();
  for (TopExp_Explorer it(result.curves, TopAbs_EDGE); it.More(); it.Next()) {
    ++result.edge_count;
  }
  GProp_GProps linear;
  BRepGProp::LinearProperties(result.curves, linear);
  result.length = linear.Mass();

  const TopoDS_Shape solids = oriented_solids(shape);
  if (!TopExp_Explorer(solids, TopAbs_SOLID).More()) {
    return result;
  }
  double size = 0.0;
  double distance = 0.0;
  const TopoDS_Face disk = cover(solids, plane, size, distance);
  // The part of the plane inside the solids, holes included.
  BRepAlgoAPI_Common common;
  build_keeping_inputs(common, disk, solids);
  if (!common.IsDone() || common.HasErrors()) {
    throw Error("the section faces could not be computed");
  }
  result.faces = common.Shape();
  for (TopExp_Explorer it(result.faces, TopAbs_FACE); it.More(); it.Next()) {
    ++result.face_count;
  }
  result.area = surface_properties(result.faces).Mass();
  return result;
}

TopoDS_Shape clip(const TopoDS_Shape& shape, const Plane& plane) {
  if (shape.IsNull()) {
    throw Error("cannot clip a null shape");
  }
  double size = 0.0;
  double distance = 0.0;
  const TopoDS_Face disk = cover(shape, plane, size, distance);
  // A slab from the plane over the whole shape on the normal side.
  const double depth = std::abs(distance) + size;
  const TopoDS_Shape removed =
      BRepPrimAPI_MakePrism(disk, gp_Vec(direction(plane)) * depth).Shape();
  BRepAlgoAPI_Cut cut;
  build_keeping_inputs(cut, shape, removed);
  if (!cut.IsDone() || cut.HasErrors()) {
    throw Error("the shape could not be clipped");
  }
  return cut.Shape();
}

} // namespace mitcad::analysis
