// SPDX-License-Identifier: MIT
#include "mitcad/geometry/loft.hpp"

#include <algorithm>
#include <optional>
#include <stdexcept>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomConvert.hxx>
#include <GeomFill_SectionGenerator.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_HArray1.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>

#include "history.hpp"
#include "loft_skin.hpp"
#include "spine.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// A section ready to loft: its wire (or vertex), the names of the faces its
// edges make, the key of its cap, and what end conditions measure from: a
// region's plane, a face and its body.
struct Prepared {
  TopoDS_Shape shape;
  detail::NamedWire named;
  std::string key;
  bool point = false;
  std::optional<gp_Dir> normal;
  ShapePtr body;
  TopoDS_Face face;
};

int wire_count(const TopoDS_Face& face) {
  int count = 0;
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    ++count;
  }
  return count;
}

Prepared prepare(const std::string& feature, const LoftSection& section, std::size_t index) {
  const std::string which = "section " + std::to_string(index + 1);
  Prepared prepared;
  switch (section.kind) {
  case LoftSection::Kind::Point:
    prepared.shape = BRepBuilderAPI_MakeVertex(section.point).Vertex();
    prepared.point = true;
    return prepared;
  case LoftSection::Kind::Region: {
    const ProfileFace profile = make_profile(section.frame, section.region);
    if (wire_count(profile.face) > 1) {
      throw std::invalid_argument("unsupported: a loft " + which + " with holes");
    }
    prepared.named.wire = BRepTools::OuterWire(profile.face);
    for (const auto& [edge, segment] : profile.edges) {
      prepared.named.edges.emplace_back(edge, face_name(feature, "side", segment));
    }
    prepared.key = section.region.name;
    prepared.normal = section.frame.normal();
    break;
  }
  case LoftSection::Kind::Face: {
    if (!section.body) {
      throw std::invalid_argument(which + " has no body");
    }
    // A copy of the body: OCCT's lofts widen the tolerances of the
    // sections' edges, which are the body's (input_check.hpp).
    const ShapePtr body = detail::working_copy(*section.body);
    const std::vector<int> faces = body->find_faces(section.face);
    if (faces.size() != 1) {
      const std::string why = faces.empty() ? " does not exist" : " is in several pieces";
      throw std::invalid_argument(face_error(section.face, which + ": the face" + why,
                                             which + ": face " + section.face + why));
    }
    const TopoDS_Face face = body->face(faces.front());
    // A face with holes lofts its outer boundary, as a face without them
    // (mitcad#1183); the face itself, holes and all, stays what tangent and
    // smooth ends measure from.
    prepared.named.wire = BRepTools::OuterWire(face);
    for (TopExp_Explorer e(prepared.named.wire, TopAbs_EDGE); e.More(); e.Next()) {
      if (const std::optional<std::string> name = body->name_of_edge(e.Current())) {
        prepared.named.edges.emplace_back(TopoDS::Edge(e.Current()), face_name(feature, "side", *name));
      }
    }
    prepared.key = section.face;
    prepared.body = body;
    prepared.face = face;
    break;
  }
  }
  prepared.shape = prepared.named.wire;
  return prepared;
}

gp_Pnt center_of(const TopoDS_Shape& shape) {
  if (shape.ShapeType() == TopAbs_VERTEX) {
    return BRep_Tool::Pnt(TopoDS::Vertex(shape));
  }
  GProp_GProps props;
  BRepGProp::LinearProperties(shape, props);
  return props.CentreOfMass();
}

// Along a centre line: the sections swept along the part of it between the
// first and the last section, at right angles to it.
ShapePtr along_centerline(const LoftSpec& spec, const std::vector<Prepared>& sections,
                          const detail::NamedWire& named, const NameList& first, const NameList& last) {
  const Path& centerline = spec.centerline;
  const bool solid = spec.solid;
  const detail::Spine spine(detail::path_wire(centerline));
  double a = spine.nearest(center_of(sections.front().shape));
  double b = spine.nearest(center_of(sections.back().shape));
  TopoDS_Wire part;
  if (a <= b) {
    part = spine.part(a, b);
  } else {
    part = TopoDS::Wire(spine.part(b, a).Reversed());
  }
  BRepOffsetAPI_MakePipeShell pipe(part);
  const detail::Spine along(part);
  // The sections at right angles to the centre line, or as its frame turns
  // them (mitcad#543), the frame's within OCCT's confusion as a framed
  // sweep's.
  const bool framed = spec.frenet || !spec.auxiliary.empty() || spec.constant_binormal.has_value();
  if (int(spec.frenet) + int(!spec.auxiliary.empty()) + int(spec.constant_binormal.has_value()) > 1) {
    throw std::invalid_argument(
        "a loft follows the Frenet frame, an auxiliary curve or a constant binormal, not both");
  }
  detail::PipeMode mode = detail::frame_mode(spec.frenet, spec.auxiliary, spec.curvilinear,
                                             spec.constant_binormal, detail::perpendicular_mode(along));
  // Parallel (mitcad#547): a constant frame, so the sections only move
  // along the centre line, as a parallel sweep's profile does.
  if (spec.parallel) {
    if (framed) {
      throw std::invalid_argument("a parallel loft along a centre line takes no frame");
    }
    mode.kind = detail::PipeMode::Kind::Fixed;
    mode.fixed = gp_Ax2(along.point(0.0), along.tangent(0.0));
  }
  if (framed || spec.parallel) {
    pipe.SetTolerance(1.0e-7, 1.0e-7, 1.0e-2);
  }
  detail::set_mode(pipe, mode);
  for (const Prepared& section : sections) {
    pipe.Add(section.shape, false, false);
  }
  detail::build(pipe);
  if (!pipe.IsDone()) {
    throw std::runtime_error("the sections could not be lofted along the centre line");
  }
  if (solid && !pipe.MakeSolid()) {
    throw std::runtime_error("the loft along the centre line does not close into a solid");
  }
  detail::FaceNamer namer(solid ? detail::oriented_solid(pipe.Shape(), "the loft") : pipe.Shape());
  for (const auto& [edge, name] : named.edges) {
    namer.generated(pipe, edge, name);
  }
  if (!sections.front().point) {
    detail::name_caps(namer, pipe.FirstShape(), first);
  }
  if (!sections.back().point) {
    detail::name_caps(namer, pipe.LastShape(), last);
  }
  return namer.shape();
}

} // namespace

ShapePtr loft(const LoftSpec& spec) {
  const std::size_t count = spec.sections.size();
  if (count < 2) {
    throw std::invalid_argument("a loft needs two or more sections");
  }
  for (std::size_t i = 1; i + 1 < count; ++i) {
    if (spec.sections[i].kind == LoftSection::Kind::Point) {
      throw std::invalid_argument("a point can only be the first or the last section of a loft");
    }
  }
  if (spec.closed && (spec.sections.front().kind == LoftSection::Kind::Point ||
                      spec.sections.back().kind == LoftSection::Kind::Point)) {
    throw std::invalid_argument("a closed loft has no point sections");
  }
  if (spec.closed && !spec.centerline.empty()) {
    throw std::invalid_argument("a closed loft has no centre line");
  }
  if ((spec.frenet || !spec.auxiliary.empty() || spec.constant_binormal) && spec.centerline.empty()) {
    throw std::invalid_argument("a loft's frame turns its sections along a centre line: it needs one");
  }
  if (spec.parallel && spec.centerline.empty()) {
    throw std::invalid_argument("a parallel loft moves its sections along a centre line: it needs one");
  }
  if (spec.closed && !spec.ruled && count < 3) {
    throw std::invalid_argument("a smooth closed loft needs three or more sections");
  }
  const auto imposes = [](const LoftEnd& end) {
    return end.kind != LoftEnd::Kind::Free && end.kind != LoftEnd::Kind::PointSharp;
  };
  const bool imposed = imposes(spec.start) || imposes(spec.end) || !spec.rails.empty();
  if (imposed) {
    if (spec.closed) {
      throw std::invalid_argument("a closed loft has no end conditions or rails");
    }
    if (spec.ruled) {
      throw std::invalid_argument("a ruled loft has no end conditions or rails");
    }
    if (!spec.centerline.empty()) {
      if (!spec.rails.empty()) {
        throw std::invalid_argument("a loft has a centre line or rails, not both");
      }
      throw std::invalid_argument("unsupported: end conditions of a loft along a centre line");
    }
  }
  // Smooth lofts through three or more sections, and closed ones, are the
  // stored ones' piecewise cubic Hermite surfaces (skin.hpp), which OCCT's
  // through-sections do not give; two sections it makes the same ruled
  // surface.
  const bool hermite = !imposed && spec.centerline.empty() && !spec.ruled && count >= 3;
  const bool skinned = imposed || hermite;
  return detail::run("loft", [&] {
    std::vector<Prepared> sections;
    for (std::size_t i = 0; i < count; ++i) {
      sections.push_back(prepare(spec.feature, spec.sections[i], i));
    }
    const auto named = std::find_if(sections.begin(), sections.end(), [](const Prepared& s) { return !s.point; });
    if (named == sections.end()) {
      throw std::invalid_argument("a loft needs a section that is not a point");
    }
    const NameList first = sections.front().point || spec.closed
                               ? NameList{}
                               : NameList{face_name(spec.feature, "start", sections.front().key)};
    const NameList last = sections.back().point || spec.closed
                              ? NameList{}
                              : NameList{face_name(spec.feature, "end", sections.back().key)};
    ShapePtr result;
    if (skinned) {
      // End conditions and rails: OCCT's through-sections cannot impose
      // them; free ends without rails keep its results.
      std::vector<detail::SkinSection> skin;
      for (const Prepared& section : sections) {
        skin.push_back({section.shape, section.point, section.normal, section.body, section.face});
      }
      result = detail::skinned_loft(skin, spec.start, spec.end, spec.rails, named->named, first, last, spec.closed,
                                    spec.solid);
    } else if (!spec.centerline.empty()) {
      result = along_centerline(spec, sections, named->named, first, last);
    } else {
      std::vector<TopoDS_Shape> shapes;
      for (const Prepared& section : sections) {
        shapes.push_back(section.shape);
      }
      if (spec.closed) {
        shapes.push_back(sections.front().shape);
      }
      // Matching the wires' starts would turn the repeated first section of
      // a closed loft on its own, twisting the last piece.
      result = detail::through_sections(shapes, named->named, first, last, spec.ruled, !spec.closed, spec.solid);
    }
    return detail::finished(*result, "the loft");
  });
}

} // namespace mitcad::geometry
