// SPDX-License-Identifier: MIT
#include "mitcad/geometry/plastic.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

#include "face_select.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Edges of a loop and the keys of the band's faces along them.
using Keys = std::vector<std::pair<TopoDS_Edge, std::string>>;

struct Loop {
  TopoDS_Wire wire;
  Keys keys;
};

// The region a loop bounds in the plane, its boundary counter-clockwise
// about the plane's normal (material inside).
TopoDS_Face loop_face(const gp_Pln& plane, const TopoDS_Wire& wire) {
  for (int attempt = 0; attempt < 2; ++attempt) {
    const TopoDS_Wire w = attempt == 0 ? wire : TopoDS::Wire(wire.Reversed());
    BRepBuilderAPI_MakeFace maker(plane, w, true);
    if (!maker.IsDone()) {
      continue;
    }
    const TopoDS_Face face = maker.Face();
    BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
    if (classifier.PerformInfinitePoint() != TopAbs_IN) {
      return face;
    }
  }
  throw std::runtime_error("a loop of the rim face bounds no region");
}

double area(const TopoDS_Face& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return std::abs(props.Mass());
}

// Whether the region of an inner loop of the rim face is an opening: just
// inside the loop and over the face there is no material (the foot of a
// boss standing on the face has it), at the middle of each of its first
// edges.
bool is_opening(const Shape& body, const TopoDS_Face& region, const gp_Dir& normal, double probe) {
  int tested = 0;
  for (TopExp_Explorer it(region, TopAbs_EDGE); it.More() && tested < 4; it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    gp_Pnt middle;
    gp_Vec tangent;
    curve.D1((curve.FirstParameter() + curve.LastParameter()) / 2.0, middle, tangent);
    if (tangent.Magnitude() <= Precision::Confusion()) {
      continue;
    }
    if (edge.Orientation() == TopAbs_REVERSED) {
      tangent.Reverse();
    }
    // The region is on the left of its boundary.
    const gp_Vec left = gp_Vec(normal).Crossed(tangent).Normalized();
    const gp_Pnt over = middle.Translated(left * probe + gp_Vec(normal) * probe);
    BRepClass3d_SolidClassifier classifier(body.occt(), over, Precision::Confusion());
    if (classifier.State() != TopAbs_OUT) {
      return false;
    }
    ++tested;
  }
  return tested > 0;
}


// The loop of `face` moved `d` out of its region (into it for d < 0),
// the moved edges keyed as the edges they come from.
Loop offset_loop(const gp_Pln& plane, const TopoDS_Face& face, const Keys& keys, double d) {
  if (std::abs(d) <= Precision::Confusion()) {
    return {BRepTools::OuterWire(face), keys};
  }
  const double before = area(face);
  std::string problem = "the loop could not be offset";
  // OCCT's sign of an offset depends on the loop's direction: the one
  // that makes the region grow for d > 0 is taken.
  for (const double sign : {1.0, -1.0}) {
    BRepOffsetAPI_MakeOffset maker(face, GeomAbs_Intersection);
    maker.Perform(sign * d);
    if (!maker.IsDone()) {
      continue;
    }
    std::vector<TopoDS_Wire> wires;
    for (TopExp_Explorer it(maker.Shape(), TopAbs_WIRE); it.More(); it.Next()) {
      wires.push_back(TopoDS::Wire(it.Current()));
    }
    if (wires.size() != 1) {
      problem = "the offset loop falls apart; make the band narrower";
      continue;
    }
    const TopoDS_Face moved = loop_face(plane, wires.front());
    if ((area(moved) > before) != (d > 0.0)) {
      continue;
    }
    Keys out;
    for (const auto& [edge, key] : keys) {
      for (const TopoDS_Shape& made : maker.Generated(edge)) {
        if (made.ShapeType() == TopAbs_EDGE) {
          out.emplace_back(TopoDS::Edge(made), key);
        }
      }
    }
    return {BRepTools::OuterWire(moved), out};
  }
  throw std::invalid_argument(problem);
}

const std::string* key_of(const Keys& keys, const TopoDS_Shape& edge) {
  for (const auto& [e, key] : keys) {
    if (e.IsSame(edge)) {
      return &key;
    }
  }
  return nullptr;
}

ShapePtr band(const RimBandSpec& spec) {
  const Shape& body = *spec.body;
  const std::vector<int> found = body.find_faces(spec.face);
  if (found.size() != 1) {
    throw std::invalid_argument(found.empty() ? no_face_error(spec.face)
                                              : face_error(spec.face, "the rim face is in several pieces",
                                                           "the rim face " + spec.face + " is in several pieces"));
  }
  const TopoDS_Face& face = body.face(found.front());
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Plane) {
    throw std::invalid_argument("the rim face must be planar");
  }
  const gp_Dir normal = detail::planar_normal(face, surface.Plane());
  const gp_Pln plane(surface.Plane().Location(), normal);
  Bnd_Box box;
  BRepBndLib::Add(body.occt(), box);
  const double size = box.IsVoid() ? 1.0 : std::sqrt(box.SquareExtent());
  const double probe = std::max(1e-3, size * 1e-4);

  const TopoDS_Wire outer = BRepTools::OuterWire(face);
  TopoDS_Wire loop = outer;
  if (!spec.outside) {
    // The openings: inner loops with no material over the face inside them.
    std::vector<std::pair<double, TopoDS_Wire>> openings;
    int inner = 0;
    for (TopExp_Explorer it(face, TopAbs_WIRE); it.More(); it.Next()) {
      const TopoDS_Wire wire = TopoDS::Wire(it.Current());
      if (wire.IsSame(outer)) {
        continue;
      }
      ++inner;
      const TopoDS_Face region = loop_face(plane, wire);
      if (is_opening(body, region, normal, probe)) {
        openings.emplace_back(area(region), wire);
      }
    }
    if (inner == 0) {
      throw std::invalid_argument(
          "pick the rim face of a shelled body (a face with an inner and an outer edge)");
    }
    if (openings.empty()) {
      throw std::invalid_argument("pick the rim face of a shelled body: this face's inner edges are "
                                  "where other features stand on it, not an opening");
    }
    std::sort(openings.begin(), openings.end(),
              [](const auto& a, const auto& b) { return a.first > b.first; });
    if (openings.size() > 1 && openings[1].first > 0.5 * openings[0].first) {
      throw std::invalid_argument("pick the rim face of a shelled body (round one opening): this face has " +
                                  std::to_string(openings.size()) + " openings alike in size");
    }
    loop = openings.front().second;
  }

  Keys keys;
  int k = 0;
  for (TopExp_Explorer it(loop, TopAbs_EDGE); it.More(); it.Next(), ++k) {
    const std::optional<std::string> name = body.name_of_edge(it.Current());
    keys.emplace_back(TopoDS::Edge(it.Current()), name ? *name : std::to_string(k));
  }
  const TopoDS_Face region = loop_face(plane, loop);
  // Into the face is out of the opening's region and into the outer
  // loop's.
  const double out = spec.outside ? spec.free : spec.width;
  const double in = spec.outside ? spec.width : spec.free;
  const Loop big = offset_loop(plane, region, keys, out);
  const Loop small = offset_loop(plane, region, keys, -in);
  const char* big_role = spec.outside ? "band_edge" : "band_side";
  const char* small_role = spec.outside ? "band_side" : "band_edge";

  BRepBuilderAPI_MakeFace maker(loop_face(plane, big.wire));
  maker.Add(TopoDS::Wire(BRepTools::OuterWire(loop_face(plane, small.wire)).Reversed()));
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Face()).IsValid()) {
    throw std::invalid_argument("the band does not fit on the rim face; make it narrower");
  }
  const TopoDS_Face band_face = maker.Face();
  std::vector<std::pair<TopoDS_Edge, std::string>> names;
  int unnamed = 0;
  for (TopExp_Explorer it(band_face, TopAbs_EDGE); it.More(); it.Next()) {
    std::string name;
    if (const std::string* key = key_of(big.keys, it.Current())) {
      name = face_name(spec.feature, big_role, *key);
    } else if (const std::string* other = key_of(small.keys, it.Current())) {
      name = face_name(spec.feature, small_role, *other);
    } else {
      name = face_name(spec.feature, "band_wall", std::to_string(unnamed++));
    }
    names.emplace_back(TopoDS::Edge(it.Current()), name);
  }

  gp_Trsf move;
  move.SetTranslation(gp_Vec(normal) * spec.start);
  BRepBuilderAPI_Transform moved(band_face, move, true);
  std::vector<std::pair<TopoDS_Edge, std::string>> moved_names;
  for (const auto& [edge, name] : names) {
    moved_names.emplace_back(TopoDS::Edge(moved.ModifiedShape(edge)), name);
  }
  const detail::SweepFace sweep{TopoDS::Face(moved.Shape()), moved_names, {}};
  const ShapePtr tool = detail::named_prism(sweep, gp_Vec(normal) * (spec.end - spec.start),
                                            {face_name(spec.feature, "band_bottom")},
                                            {face_name(spec.feature, "band_top")});
  return detail::finished(*tool, "the band");
}

} // namespace

ShapePtr rim_band(const RimBandSpec& spec) {
  if (!spec.body) {
    throw std::invalid_argument("the band needs a body");
  }
  detail::require_positive("band width", spec.width);
  detail::require_finite("band's free side", spec.free);
  if (spec.free < 0.0) {
    throw std::invalid_argument("the band's free side must not be negative");
  }
  detail::require_finite("band start", spec.start);
  detail::require_finite("band end", spec.end);
  if (!(spec.start < spec.end)) {
    throw std::invalid_argument("the band must end above its start");
  }
  return detail::run("band", [&] { return band(spec); });
}

} // namespace mitcad::geometry
