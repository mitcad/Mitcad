// SPDX-License-Identifier: MIT
#include "mitcad/analysis/section.hpp"

#include <algorithm>
#include <cmath>
#include <string>
#include <utility>

#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <OSD_Exception.hxx>
#include <OSD_Signal.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pln.hxx>

#include "convert.hpp"
#include "mesh_section.hpp"

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

// The shape's box; an empty shape cannot be sectioned.
Bounds box_of(const TopoDS_Shape& shape) {
  const Bounds box = bounds(shape);
  if (box.empty) {
    throw Error("cannot section an empty shape");
  }
  return box;
}

// A square face in the plane, centred where the plane meets the line from
// the box centre along the normal, large enough to cover the box.
// `distance` is the signed distance of the box centre from the plane.
TopoDS_Face cover(const Bounds& box, const Plane& plane, double& size, double& distance) {
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

PlaneReach plane_reach(const Bounds& box, const Plane& plane) {
  const gp_Vec normal(direction(plane));
  PlaneReach reach;
  if (box.empty) {
    return reach;
  }
  // Along each axis the nearer and the farther end of the box.
  const gp_Pnt origin = detail::pnt(plane.origin);
  const double low[3] = {box.min.x, box.min.y, box.min.z};
  const double high[3] = {box.max.x, box.max.y, box.max.z};
  const double at[3] = {origin.X(), origin.Y(), origin.Z()};
  const double n[3] = {normal.X(), normal.Y(), normal.Z()};
  for (int i = 0; i < 3; ++i) {
    const double a = (low[i] - at[i]) * n[i];
    const double b = (high[i] - at[i]) * n[i];
    reach.min += std::min(a, b);
    reach.max += std::max(a, b);
  }
  return reach;
}

ClipSide clip_side(const Bounds& box, const Plane& plane) {
  const PlaneReach reach = plane_reach(box, plane);
  if (box.empty) {
    return ClipSide::Cut;
  }
  // Behind first: a face lying on the plane stays (a sheet in the plane).
  if (reach.max <= Precision::Confusion()) {
    return ClipSide::Kept;
  }
  if (reach.min >= -Precision::Confusion()) {
    return ClipSide::Removed;
  }
  return ClipSide::Cut;
}

Section section(const TopoDS_Shape& shape, const Plane& plane) {
  if (shape.IsNull()) {
    throw Error("cannot section a null shape");
  }
  if (detail::mesh_only(shape)) {
    return detail::mesh_section(shape, plane);
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
  const TopoDS_Face disk = cover(box_of(solids), plane, size, distance);
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

namespace {

// The fuzzy value of ClipAttempt::Fuzzy: this many times the shape's
// largest tolerance, at most kMostFuzzy (mm).
constexpr double kFuzzyFactor = 10.0;
constexpr double kMostFuzzy = 0.01;

// The largest tolerance of the shape's vertices, edges and faces, at least
// Precision::Confusion().
double largest_tolerance(const TopoDS_Shape& shape) {
  double most = Precision::Confusion();
  for (TopExp_Explorer it(shape, TopAbs_VERTEX); it.More(); it.Next()) {
    most = std::max(most, BRep_Tool::Tolerance(TopoDS::Vertex(it.Current())));
  }
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    most = std::max(most, BRep_Tool::Tolerance(TopoDS::Edge(it.Current())));
  }
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    most = std::max(most, BRep_Tool::Tolerance(TopoDS::Face(it.Current())));
  }
  return most;
}

double clip_fuzzy(const TopoDS_Shape& shape) {
  return std::min(kMostFuzzy, kFuzzyFactor * largest_tolerance(shape));
}

// The plane moved by `by` along its normal.
Plane moved(const Plane& plane, double by) {
  const gp_Dir normal = direction(plane);
  Plane result = plane;
  result.origin = {plane.origin.x + by * normal.X(), plane.origin.y + by * normal.Y(),
                   plane.origin.z + by * normal.Z()};
  return result;
}

// The shape cut by the slab from the plane over the whole shape (its box
// `box`) on the normal side, with the fuzzy value (0: none), and with
// `faces` what became of each face; Error when the boolean fails.
TopoDS_Shape cut_at(const TopoDS_Shape& shape, const Bounds& box, const Plane& plane, double fuzzy,
                    std::vector<ClippedFace>* faces) {
  double size = 0.0;
  double distance = 0.0;
  const TopoDS_Face disk = cover(box, plane, size, distance);
  // A slab from the plane over the whole shape on the normal side.
  const double depth = std::abs(distance) + size;
  const TopoDS_Shape removed =
      BRepPrimAPI_MakePrism(disk, gp_Vec(direction(plane)) * depth).Shape();
  BRepAlgoAPI_Cut cut;
  if (fuzzy > 0.0) {
    cut.SetFuzzyValue(fuzzy);
  }
  build_keeping_inputs(cut, shape, removed);
  if (!cut.IsDone() || cut.HasErrors()) {
    throw Error("the boolean failed");
  }
  const TopoDS_Shape& result = cut.Shape();
  if (faces) {
    // Each face's images; a face the cut leaves as it is is its own.
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> kept;
    TopExp::MapShapes(result, TopAbs_FACE, kept);
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> own;
    TopExp::MapShapes(shape, TopAbs_FACE, own);
    faces->clear();
    for (int i = 1; i <= own.Extent(); ++i) {
      ClippedFace face{own(i), {}};
      if (!cut.IsDeleted(face.face)) {
        for (const TopoDS_Shape& image : cut.Modified(face.face)) {
          if (image.ShapeType() == TopAbs_FACE && kept.Contains(image)) {
            face.pieces.push_back(image);
          }
        }
        if (face.pieces.empty() && kept.Contains(face.face)) {
          face.pieces.push_back(face.face);
        }
      }
      faces->push_back(std::move(face));
    }
  }
  return result;
}

// One attempt on a shape with surfaces whose box is `box`.
TopoDS_Shape attempt_clip(const TopoDS_Shape& shape, const Bounds& box, const Plane& plane, ClipAttempt attempt,
                          std::vector<ClippedFace>* faces, ClipReport* report) {
  double fuzzy = 0.0;
  double shift = 0.0;
  if (attempt == ClipAttempt::Fuzzy) {
    fuzzy = clip_fuzzy(shape);
  } else if (attempt == ClipAttempt::Moved) {
    shift = kClipShift;
  }
  try {
    TopoDS_Shape result = cut_at(shape, box, shift != 0.0 ? moved(plane, shift) : plane, fuzzy, faces);
    if (report) {
      report->attempt = attempt;
      report->fuzzy = fuzzy;
      report->shift = shift;
    }
    return result;
  } catch (const OSD_Exception&) {
    // A crash inside OCCT (its signal handlers' exceptions) is not tried
    // again, nor memory that ran out: the caller reports them (mitcad#392).
    throw;
  } catch (const OSD_Signal&) {
    throw;
  } catch (const Standard_OutOfMemory&) {
    throw;
  } catch (const Standard_Failure& failure) {
    const char* message = failure.what();
    throw Error(message != nullptr && *message != '\0' ? message : "the boolean failed");
  }
}

const char* attempt_name(ClipAttempt attempt) {
  switch (attempt) {
  case ClipAttempt::Plain:
    return "plain cut";
  case ClipAttempt::Fuzzy:
    return "fuzzy cut";
  case ClipAttempt::Moved:
    return "plane moved";
  }
  return "cut";
}

} // namespace

TopoDS_Shape clip_by(const TopoDS_Shape& shape, const Plane& plane, ClipAttempt attempt,
                     std::vector<ClippedFace>* faces) {
  if (shape.IsNull()) {
    throw Error("cannot clip a null shape");
  }
  try {
    return attempt_clip(shape, box_of(shape), plane, attempt, faces, nullptr);
  } catch (const Error& error) {
    throw Error(std::string("the shape could not be clipped (") + attempt_name(attempt) + ": " + error.what() + ")");
  }
}

TopoDS_Shape clip(const TopoDS_Shape& shape, const Plane& plane, std::vector<ClippedFace>* faces,
                  ClipReport* report) {
  if (shape.IsNull()) {
    throw Error("cannot clip a null shape");
  }
  if (report) {
    *report = ClipReport{};
  }
  if (detail::mesh_only(shape)) {
    return detail::mesh_clip(shape, plane, faces);
  }
  const Bounds box = box_of(shape);
  // A shape the plane does not reach is kept or removed as a whole without
  // a boolean (mitcad#438): Section Analysis clips every body shown, and
  // most lie on one side.
  const ClipSide side = clip_side(box, plane);
  if (side != ClipSide::Cut) {
    if (faces) {
      NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> own;
      TopExp::MapShapes(shape, TopAbs_FACE, own);
      faces->clear();
      for (int i = 1; i <= own.Extent(); ++i) {
        ClippedFace face{own(i), {}};
        if (side == ClipSide::Kept) {
          face.pieces.push_back(face.face);
        }
        faces->push_back(std::move(face));
      }
    }
    if (side == ClipSide::Kept) {
      return shape;
    }
    TopoDS_Compound nothing;
    BRep_Builder().MakeCompound(nothing);
    return nothing;
  }
  // The attempts in turn (mitcad#1230): a plane along the seams and edges
  // of a revolved body's profile made the plain boolean fail, and the body
  // was shown whole.
  std::string failed;
  for (const ClipAttempt attempt : {ClipAttempt::Plain, ClipAttempt::Fuzzy, ClipAttempt::Moved}) {
    try {
      TopoDS_Shape result = attempt_clip(shape, box, plane, attempt, faces, report);
      if (report) {
        report->failed = failed;
      }
      return result;
    } catch (const Error& error) {
      failed += std::string(failed.empty() ? "" : "; ") + attempt_name(attempt) + ": " + error.what();
    }
  }
  throw Error("the shape could not be clipped (" + failed + ")");
}

} // namespace mitcad::analysis
