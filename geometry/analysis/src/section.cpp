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
#include <gp_Ax3.hxx>
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

// A strip's direction in the plane (its part along the normal left out).
gp_Dir strip_direction(const Plane& plane, const Strip& strip) {
  const gp_Vec normal(direction(plane));
  gp_Vec along(strip.along.x, strip.along.y, strip.along.z);
  along -= normal * along.Dot(normal);
  if (along.Magnitude() <= gp::Resolution()) {
    throw Error("the section strip needs a direction in the plane");
  }
  if (!(std::isfinite(strip.from) && std::isfinite(strip.to) && strip.from < strip.to)) {
    throw Error("the section strip needs a width");
  }
  return gp_Dir(along);
}

// A square face in the plane, centred where the plane meets the line from
// the box centre along the normal, large enough to cover the box; with a
// strip, the part of it within the strip (a null face when the strip
// misses it, mitcad#2479). `distance` is the signed distance of the box
// centre from the plane.
TopoDS_Face cover(const Bounds& box, const Plane& plane, double& size, double& distance,
                  const Strip* strip = nullptr) {
  const Vec3 extent = box.size();
  size = std::sqrt(extent.x * extent.x + extent.y * extent.y + extent.z * extent.z) + 1.0;
  const gp_Dir normal = direction(plane);
  const gp_Pnt center(0.5 * (box.min.x + box.max.x), 0.5 * (box.min.y + box.max.y),
                      0.5 * (box.min.z + box.max.z));
  distance = gp_Vec(detail::pnt(plane.origin), center).Dot(gp_Vec(normal));
  const gp_Pnt on_plane = center.Translated(gp_Vec(normal) * -distance);
  if (strip == nullptr) {
    return BRepBuilderAPI_MakeFace(gp_Pln(on_plane, normal), -size, size, -size, size).Face();
  }
  // The face's u runs along the strip from where the box centre meets the
  // plane; the strip's ends measure from the plane's origin.
  const gp_Dir along = strip_direction(plane, *strip);
  const double shift = gp_Vec(detail::pnt(plane.origin), on_plane).Dot(gp_Vec(along));
  const double low = std::max(strip->from - shift, -size);
  const double high = std::min(strip->to - shift, size);
  if (!(low < high)) {
    return TopoDS_Face();
  }
  return BRepBuilderAPI_MakeFace(gp_Pln(gp_Ax3(on_plane, normal, along)), low, high, -size, size).Face();
}

// An empty compound: what a cut leaves of a shape that goes as a whole.
TopoDS_Shape nothing() {
  TopoDS_Compound none;
  BRep_Builder().MakeCompound(none);
  return none;
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

namespace {

// The curves of a section not yet built.
TopoDS_Shape section_curves(BRepAlgoAPI_Section& curves) {
  curves.Approximation(true);
  // The shape is a body of the model, which other results share: the
  // booleans here leave its sub-shapes as they are.
  curves.SetNonDestructive(true);
  curves.Build();
  if (!curves.IsDone() || curves.HasErrors()) {
    throw Error("the section could not be computed");
  }
  return curves.Shape();
}

// section and section_within: the whole plane, or the strip of it.
Section section_of(const TopoDS_Shape& shape, const Plane& plane, const Strip* strip) {
  if (shape.IsNull()) {
    throw Error("cannot section a null shape");
  }
  if (detail::mesh_only(shape)) {
    if (strip != nullptr) {
      throw Error("a section limited to a strip cuts bodies with surfaces only");
    }
    return detail::mesh_section(shape, plane);
  }
  Section result;
  TopoDS_Face band;
  if (strip != nullptr) {
    double size = 0.0;
    double distance = 0.0;
    band = cover(box_of(shape), plane, size, distance, strip);
    if (band.IsNull()) {
      // The strip misses the shape.
      result.curves = nothing();
      result.faces = nothing();
      return result;
    }
  }
  if (strip != nullptr) {
    BRepAlgoAPI_Section curves(shape, band, false);
    result.curves = section_curves(curves);
  } else {
    BRepAlgoAPI_Section curves(shape, gp_Pln(detail::pnt(plane.origin), direction(plane)), false);
    result.curves = section_curves(curves);
  }
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
  const TopoDS_Face disk = cover(box_of(solids), plane, size, distance, strip);
  if (disk.IsNull()) {
    result.faces = nothing();
    return result;
  }
  // The part of the plane (of the strip) inside the solids, holes included.
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

} // namespace

Section section(const TopoDS_Shape& shape, const Plane& plane) { return section_of(shape, plane, nullptr); }

Section section_within(const TopoDS_Shape& shape, const Plane& plane, const Strip& strip) {
  return section_of(shape, plane, &strip);
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

// What clip gives for a shape it keeps whole (`kept`) or removes whole,
// with `faces` what became of each face.
TopoDS_Shape whole(const TopoDS_Shape& shape, bool kept, std::vector<ClippedFace>* faces) {
  if (faces) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> own;
    TopExp::MapShapes(shape, TopAbs_FACE, own);
    faces->clear();
    for (int i = 1; i <= own.Extent(); ++i) {
      ClippedFace face{own(i), {}};
      if (kept) {
        face.pieces.push_back(face.face);
      }
      faces->push_back(std::move(face));
    }
  }
  return kept ? shape : nothing();
}

// The shape cut by the slab from the plane over the whole shape (its box
// `box`) on the normal side, with the fuzzy value (0: none), and with
// `faces` what became of each face; Error when the boolean fails. With a
// strip, the slab is the strip's prism (mitcad#2479).
TopoDS_Shape cut_at(const TopoDS_Shape& shape, const Bounds& box, const Plane& plane, double fuzzy,
                    std::vector<ClippedFace>* faces, const Strip* strip) {
  double size = 0.0;
  double distance = 0.0;
  const TopoDS_Face disk = cover(box, plane, size, distance, strip);
  if (disk.IsNull()) {
    // The strip misses the shape: nothing goes.
    return whole(shape, true, faces);
  }
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
                          std::vector<ClippedFace>* faces, ClipReport* report, const Strip* strip = nullptr) {
  double fuzzy = 0.0;
  double shift = 0.0;
  if (attempt == ClipAttempt::Fuzzy) {
    fuzzy = clip_fuzzy(shape);
  } else if (attempt == ClipAttempt::Moved) {
    shift = kClipShift;
  }
  try {
    // The plane moves along its normal: the strip's ends stay where they
    // are along it.
    TopoDS_Shape result = cut_at(shape, box, shift != 0.0 ? moved(plane, shift) : plane, fuzzy, faces, strip);
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

namespace {

// What clip_within does with a shape whose box is `box`: clip_side's
// answer for the plane, and a box wholly outside the strip is kept, one
// within it is removed only when it lies on the normal side.
ClipSide strip_side(const Bounds& box, const Plane& plane, const Strip& strip) {
  const ClipSide side = clip_side(box, plane);
  if (box.empty || side == ClipSide::Kept) {
    return side;
  }
  const gp_Dir along = strip_direction(plane, strip);
  Plane across;
  across.origin = plane.origin;
  across.normal = {along.X(), along.Y(), along.Z()};
  const PlaneReach reach = plane_reach(box, across);
  if (reach.max <= strip.from + Precision::Confusion() || reach.min >= strip.to - Precision::Confusion()) {
    return ClipSide::Kept;
  }
  const bool within =
      reach.min >= strip.from - Precision::Confusion() && reach.max <= strip.to + Precision::Confusion();
  return side == ClipSide::Removed && within ? ClipSide::Removed : ClipSide::Cut;
}

// clip and clip_within: the half-space beyond the whole plane, or the
// strip's prism of it.
TopoDS_Shape clip_of(const TopoDS_Shape& shape, const Plane& plane, const Strip* strip,
                     std::vector<ClippedFace>* faces, ClipReport* report) {
  if (shape.IsNull()) {
    throw Error("cannot clip a null shape");
  }
  if (report) {
    *report = ClipReport{};
  }
  if (detail::mesh_only(shape)) {
    if (strip != nullptr) {
      throw Error("a section limited to a strip cuts bodies with surfaces only");
    }
    return detail::mesh_clip(shape, plane, faces);
  }
  const Bounds box = box_of(shape);
  // A shape the plane does not reach is kept or removed as a whole without
  // a boolean (mitcad#438): Section Analysis clips every body shown, and
  // most lie on one side.
  const ClipSide side = strip != nullptr ? strip_side(box, plane, *strip) : clip_side(box, plane);
  if (side != ClipSide::Cut) {
    return whole(shape, side == ClipSide::Kept, faces);
  }
  // The attempts in turn (mitcad#1230): a plane along the seams and edges
  // of a revolved body's profile made the plain boolean fail, and the body
  // was shown whole.
  std::string failed;
  for (const ClipAttempt attempt : {ClipAttempt::Plain, ClipAttempt::Fuzzy, ClipAttempt::Moved}) {
    try {
      TopoDS_Shape result = attempt_clip(shape, box, plane, attempt, faces, report, strip);
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

} // namespace

TopoDS_Shape clip(const TopoDS_Shape& shape, const Plane& plane, std::vector<ClippedFace>* faces,
                  ClipReport* report) {
  return clip_of(shape, plane, nullptr, faces, report);
}

TopoDS_Shape clip_within(const TopoDS_Shape& shape, const Plane& plane, const Strip& strip, ClipReport* report) {
  return clip_of(shape, plane, &strip, nullptr, report);
}

} // namespace mitcad::analysis
