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
      throw std::invalid_argument(which + ": face " + section.face +
                                  (faces.empty() ? " does not exist" : " is in several pieces"));
    }
    const TopoDS_Face face = body->face(faces.front());
    if (wire_count(face) > 1) {
      throw std::invalid_argument("unsupported: a loft " + which + " with holes");
    }
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
ShapePtr along_centerline(const std::vector<Prepared>& sections, const Path& centerline,
                          const detail::NamedWire& named, const NameList& first, const NameList& last) {
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
  const detail::PipeMode mode = detail::perpendicular_mode(along);
  if (mode.kind == detail::PipeMode::Kind::Binormal) {
    pipe.SetMode(mode.binormal);
  } else {
    pipe.SetMode(false);
  }
  for (const Prepared& section : sections) {
    pipe.Add(section.shape, false, false);
  }
  detail::build(pipe);
  if (!pipe.IsDone()) {
    throw std::runtime_error("the sections could not be lofted along the centre line");
  }
  if (!pipe.MakeSolid()) {
    throw std::runtime_error("the loft along the centre line does not close into a solid");
  }
  detail::FaceNamer namer(detail::oriented_solid(pipe.Shape(), "the loft"));
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

// The B-spline curves of a wire's edges in wire order, each running along
// the wire, and the edges.
std::vector<occ::handle<Geom_BSplineCurve>> wire_curves(const TopoDS_Wire& wire,
                                                        std::vector<TopoDS_Edge>& edges) {
  std::vector<occ::handle<Geom_BSplineCurve>> curves;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(it.Current(), first, last);
    if (curve.IsNull()) {
      throw std::invalid_argument("a loft section has an edge without a curve");
    }
    occ::handle<Geom_BSplineCurve> spline =
        GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
    if (spline->IsPeriodic()) {
      spline->SetNotPeriodic();
    }
    if (it.Current().Orientation() == TopAbs_REVERSED) {
      spline->Reverse();
    }
    curves.push_back(spline);
    edges.push_back(it.Current());
  }
  return curves;
}

// A smooth closed loft: through-sections in OCCT is not tangent where it
// closes, so each side face is a surface interpolating the sections' curves
// periodically (C1 at the first section, C2 elsewhere), the curves made
// compatible first. The sections' wires must have as many edges each,
// starting at corresponding points.
ShapePtr smooth_closed(const std::vector<Prepared>& sections, const detail::NamedWire& named) {
  const std::size_t count = sections.size();
  std::vector<std::vector<occ::handle<Geom_BSplineCurve>>> curves;
  std::vector<TopoDS_Edge> first_edges;
  for (std::size_t i = 0; i < count; ++i) {
    std::vector<TopoDS_Edge> edges;
    curves.push_back(wire_curves(TopoDS::Wire(sections[i].shape), edges));
    if (i == 0) {
      first_edges = edges;
    } else if (curves[i].size() != curves[0].size()) {
      throw std::invalid_argument(
          "unsupported: a smooth closed loft through sections with different numbers of edges");
    }
  }
  // Chord-length parameters of the sections' centres, round to the first.
  std::vector<gp_Pnt> centres;
  for (const Prepared& section : sections) {
    centres.push_back(center_of(section.shape));
  }
  const occ::handle<NCollection_HArray1<double>> parameters =
      new NCollection_HArray1<double>(1, static_cast<int>(count) + 1);
  double at = 0.0;
  for (std::size_t i = 0; i <= count; ++i) {
    parameters->SetValue(static_cast<int>(i) + 1, at);
    const double step = centres[i % count].Distance(centres[(i + 1) % count]);
    at += std::max(step, 1.0e-3);
  }
  BRepBuilderAPI_Sewing sewing(1.0e-6);
  std::vector<std::pair<TopoDS_Face, std::string>> faces;
  for (std::size_t j = 0; j < curves[0].size(); ++j) {
    GeomFill_SectionGenerator generator;
    for (std::size_t i = 0; i < count; ++i) {
      generator.AddCurve(curves[i][j]);
    }
    generator.Perform(Precision::PConfusion());
    const int poles = generator.NbPoles();
    NCollection_Array1<double> u_knots(1, generator.NbKnots());
    NCollection_Array1<int> u_mults(1, generator.NbKnots());
    generator.KnotsAndMults(u_knots, u_mults);
    // Row k of the surface interpolates pole k of every section.
    std::vector<NCollection_Array1<gp_Pnt>> section_poles;
    std::vector<NCollection_Array1<double>> section_weights;
    for (std::size_t i = 0; i < count; ++i) {
      section_poles.emplace_back(1, poles);
      section_weights.emplace_back(1, poles);
      generator.Poles(static_cast<int>(i) + 1, section_poles.back());
      generator.Weights(static_cast<int>(i) + 1, section_weights.back());
    }
    std::vector<occ::handle<Geom_BSplineCurve>> rows;
    for (int k = 1; k <= poles; ++k) {
      for (std::size_t i = 1; i < count; ++i) {
        if (std::abs(section_weights[i](k) - section_weights[0](k)) > 1e-9) {
          throw std::invalid_argument(
              "unsupported: a smooth closed loft through sections of different kinds of curves");
        }
      }
      const occ::handle<NCollection_HArray1<gp_Pnt>> points =
          new NCollection_HArray1<gp_Pnt>(1, static_cast<int>(count));
      for (std::size_t i = 0; i < count; ++i) {
        points->SetValue(static_cast<int>(i) + 1, section_poles[i](k));
      }
      GeomAPI_Interpolate interpolate(points, parameters, true, Precision::Confusion());
      interpolate.Perform();
      if (!interpolate.IsDone()) {
        throw std::runtime_error("the closed loft could not be interpolated");
      }
      rows.push_back(interpolate.Curve());
    }
    const occ::handle<Geom_BSplineCurve>& model = rows.front();
    const int v_poles = model->NbPoles();
    NCollection_Array2<gp_Pnt> net(1, poles, 1, v_poles);
    NCollection_Array2<double> weights(1, poles, 1, v_poles);
    for (int k = 1; k <= poles; ++k) {
      for (int l = 1; l <= v_poles; ++l) {
        net(k, l) = rows[static_cast<std::size_t>(k - 1)]->Pole(l);
        weights(k, l) = section_weights[0](k);
      }
    }
    const occ::handle<Geom_BSplineSurface> surface =
        new Geom_BSplineSurface(net, weights, u_knots, model->Knots(), u_mults, model->Multiplicities(),
                                generator.Degree(), model->Degree(), false, true);
    const TopoDS_Face face = BRepBuilderAPI_MakeFace(surface, Precision::Confusion()).Face();
    std::string name;
    for (const auto& [edge, edge_name] : named.edges) {
      if (edge.IsSame(first_edges[j])) {
        name = edge_name;
      }
    }
    faces.emplace_back(face, name);
    sewing.Add(face);
  }
  detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
  const TopoDS_Shape sewn = sewing.SewedShape();
  detail::FaceNamer namer(detail::oriented_solid(sewn, "the closed loft"));
  for (const auto& [face, name] : faces) {
    if (!name.empty()) {
      namer.add(sewing.IsModified(face) ? sewing.Modified(face) : face, name);
    }
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
  const auto imposes = [](const LoftEnd& end) {
    return end.kind != LoftEnd::Kind::Free && end.kind != LoftEnd::Kind::PointSharp;
  };
  const bool skinned = imposes(spec.start) || imposes(spec.end) || !spec.rails.empty();
  if (skinned) {
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
      result = detail::skinned_loft(skin, spec.start, spec.end, spec.rails, named->named, first, last);
    } else if (!spec.centerline.empty()) {
      result = along_centerline(sections, spec.centerline, named->named, first, last);
    } else if (spec.closed && !spec.ruled) {
      result = smooth_closed(sections, named->named);
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
      result = detail::through_sections(shapes, named->named, first, last, spec.ruled, !spec.closed);
    }
    return detail::finished(*result, "the loft");
  });
}

} // namespace mitcad::geometry
