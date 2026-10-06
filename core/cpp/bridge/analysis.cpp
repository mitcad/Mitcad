// SPDX-License-Identifier: MIT
#include "bridge/analysis.hpp"

#include <algorithm>
#include <array>
#include <limits>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/analysis/compare.hpp"
#include "mitcad/analysis/measure.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/io/step.hpp"
#include "mitcad_bridge/kernel/analysis.h"

namespace mitcad::bridge {
namespace {

// A point this near a face is on the boundary (boundary_distances).
constexpr double kOnBoundary = 1.0e-6;

std::array<double, 3> xyz(const analysis::Vec3& v) { return {v.x, v.y, v.z}; }

analysis::Vec3 vec(const std::array<double, 3>& v) { return {v[0], v[1], v[2]}; }

TopoDS_Shape compound(const std::vector<TopoDS_Shape>& shapes) {
  if (shapes.size() == 1) {
    return shapes.front();
  }
  BRep_Builder builder;
  TopoDS_Compound result;
  builder.MakeCompound(result);
  for (const TopoDS_Shape& shape : shapes) {
    builder.Add(result, shape);
  }
  return result;
}

// What a selection resolves to: one sub-shape, or a compound of all the
// pieces a name means.
struct Selected {
  TopoDS_Shape shape;
  std::string kind;
  std::size_t count = 1;
};

const char* kind_of(const TopoDS_Shape& shape) {
  switch (shape.ShapeType()) {
  case TopAbs_FACE:
    return "face";
  case TopAbs_EDGE:
    return "edge";
  case TopAbs_VERTEX:
    return "vertex";
  default:
    return "body";
  }
}

Selected select(const geometry::Shape& shape, const std::string& name) {
  if (shape.occt().IsNull()) {
    throw std::runtime_error("the body has no shape");
  }
  if (name.empty()) {
    return {shape.occt(), kind_of(shape.occt()), 1};
  }
  Selected result;
  std::vector<TopoDS_Shape> found;
  if (name.rfind("E{", 0) == 0) {
    result.kind = "edge";
    for (const int i : shape.find_edges(name)) {
      found.push_back(shape.edge(i));
    }
  } else if (name.rfind("V{", 0) == 0) {
    result.kind = "vertex";
    for (const int i : shape.find_vertices(name)) {
      found.push_back(shape.vertex(i));
    }
  } else {
    result.kind = "face";
    for (const int i : shape.find_faces(name)) {
      found.push_back(shape.face(i));
    }
  }
  if (found.empty()) {
    throw std::runtime_error("the body has no " + result.kind + " " + name);
  }
  result.count = found.size();
  result.shape = compound(found);
  return result;
}

analysis::Plane plane_of(const AnalysisPlane& plane) {
  analysis::Plane result;
  result.origin = vec(plane.origin);
  result.normal = vec(plane.normal);
  return result;
}

std::vector<TopoDS_Shape> occt(const ShapeList& list) {
  std::vector<TopoDS_Shape> shapes;
  for (const auto& shape : list.items()) {
    if (!shape || shape->occt().IsNull()) {
      throw std::runtime_error("a body has no shape");
    }
    shapes.push_back(shape->occt());
  }
  return shapes;
}

bool is_index(const std::string& text) {
  return !text.empty() && text.size() < 10 &&
         std::all_of(text.begin(), text.end(), [](char c) { return c >= '0' && c <= '9'; });
}

AnalysisDeviation deviation(const analysis::Deviation& d) {
  AnalysisDeviation result;
  result.max = d.max;
  result.rms = d.rms;
  result.at = xyz(d.at);
  result.samples = d.samples;
  return result;
}

} // namespace

std::shared_ptr<geometry::Shape> AnalysisInterferences::shape(std::size_t index) const {
  return std::make_shared<geometry::Shape>(m_found.at(index).common);
}

std::shared_ptr<geometry::Shape> AnalysisSection::curves() const {
  return std::make_shared<geometry::Shape>(m_section.curves);
}

std::shared_ptr<geometry::Shape> AnalysisSection::faces() const {
  return std::make_shared<geometry::Shape>(m_section.faces);
}

AnalysisProperties analysis_properties(const geometry::Shape& shape, double density) {
  const geometry::CheckedOperation check("physical properties");
  if (!(density > 0.0)) {
    throw std::invalid_argument("the density must be greater than zero");
  }
  const analysis::PhysicalProperties p = analysis::physical_properties(shape.occt(), density);
  AnalysisProperties result{};
  result.volume = p.volume;
  result.area = p.area;
  result.mass = p.mass;
  result.center = xyz(p.center_of_mass);
  for (std::size_t i = 0; i < 3; ++i) {
    for (std::size_t j = 0; j < 3; ++j) {
      result.inertia[3 * i + j] = p.inertia[i][j];
    }
    const std::array<double, 3> axis = xyz(p.principal_axes[i]);
    for (std::size_t k = 0; k < 3; ++k) {
      result.principal_axes[3 * i + k] = axis[k];
    }
  }
  result.principal_moments = {p.principal_moments[0], p.principal_moments[1],
                              p.principal_moments[2]};
  return result;
}

AnalysisMeasure analysis_measure(const geometry::Shape& shape, rust::Str name) {
  const geometry::CheckedOperation check("measure");
  const Selected selected = select(shape, std::string(name));
  AnalysisMeasure result{};
  result.kind = rust::String(selected.kind);
  if (selected.kind == "body") {
    const analysis::PhysicalProperties p = analysis::physical_properties(selected.shape, 1.0);
    result.has_volume = true;
    result.volume = p.volume;
    result.has_area = true;
    result.area = p.area;
  } else if (selected.kind == "face") {
    result.has_area = true;
    result.area = analysis::area(selected.shape);
  } else if (selected.kind == "edge") {
    result.has_length = true;
    result.length = analysis::length(selected.shape);
  } else if (selected.count == 1) {
    result.has_point = true;
    result.point = xyz(analysis::point(selected.shape));
  }
  if (selected.count == 1 && (selected.kind == "face" || selected.kind == "edge")) {
    if (const std::optional<analysis::Circle> circle = analysis::circle(selected.shape)) {
      result.has_circle = true;
      result.center = xyz(circle->center);
      result.axis = xyz(circle->axis);
      result.radius = circle->radius;
      result.sweep = circle->sweep;
    }
  }
  return result;
}

AnalysisSeparation analysis_between(const geometry::Shape& a, rust::Str a_name,
                                    const geometry::Shape& b, rust::Str b_name) {
  const geometry::CheckedOperation check("distance between");
  const Selected first = select(a, std::string(a_name));
  const Selected second = select(b, std::string(b_name));
  const analysis::Distance distance = analysis::min_distance(first.shape, second.shape);
  AnalysisSeparation result{};
  result.distance = distance.value;
  result.on_a = xyz(distance.on_a);
  result.on_b = xyz(distance.on_b);
  result.inside = distance.inside;
  try {
    result.angle = analysis::angle(first.shape, second.shape);
    result.has_angle = true;
  } catch (const analysis::Error&) {
    // Only planar faces and straight edges have an angle.
  }
  return result;
}

std::unique_ptr<AnalysisInterferences> analysis_interferences(const ShapeList& bodies,
                                                              double min_volume) {
  const geometry::CheckedOperation check("interference");
  analysis::InterferenceOptions options;
  options.min_volume = min_volume;
  return std::make_unique<AnalysisInterferences>(
      analysis::interferences(occt(bodies), options));
}

std::unique_ptr<AnalysisSection> analysis_section(const geometry::Shape& shape,
                                                  const AnalysisPlane& plane) {
  const geometry::CheckedOperation check("section");
  return std::make_unique<AnalysisSection>(analysis::section(shape.occt(), plane_of(plane)));
}

std::shared_ptr<geometry::Shape> analysis_clip(const geometry::Shape& shape,
                                               const AnalysisPlane& plane) {
  const geometry::CheckedOperation check("clip");
  return std::make_shared<geometry::Shape>(analysis::clip(shape.occt(), plane_of(plane)));
}

AnalysisComparison analysis_compare_step(const ShapeList& bodies, rust::Str path,
                                         rust::Str step_body, std::size_t samples, double fuzzy) {
  const geometry::CheckedOperation check("comparison with a STEP file");
  const std::vector<TopoDS_Shape> mine = occt(bodies);
  if (mine.empty()) {
    throw std::invalid_argument("there are no bodies to compare");
  }
  const std::vector<io::Body> file_bodies = io::read_step(std::string(path));
  if (file_bodies.empty()) {
    throw std::runtime_error("the STEP file has no bodies");
  }
  const std::string wanted(step_body);
  std::vector<const io::Body*> chosen;
  for (const io::Body& body : file_bodies) {
    if (wanted.empty() || body.name == wanted) {
      chosen.push_back(&body);
    }
  }
  if (chosen.empty() && is_index(wanted) && std::stoul(wanted) < file_bodies.size()) {
    chosen.push_back(&file_bodies[std::stoul(wanted)]);
  }
  if (chosen.empty()) {
    std::string names;
    for (const io::Body& body : file_bodies) {
      names += (names.empty() ? "" : ", ") + body.name;
    }
    throw std::runtime_error("the STEP file has no body '" + wanted + "' (it has " + names + ")");
  }
  std::vector<TopoDS_Shape> theirs;
  AnalysisComparison result{};
  for (const io::Body* body : chosen) {
    theirs.push_back(body->shape);
    result.step_bodies.push_back(rust::String(body->name));
  }
  analysis::CompareOptions options;
  options.samples = samples;
  options.fuzzy = fuzzy;
  const analysis::Comparison c = analysis::compare(compound(mine), compound(theirs), options);
  result.volume_a = c.volume_a;
  result.volume_b = c.volume_b;
  result.has_differences = c.a_minus_b.has_value() && c.b_minus_a.has_value();
  result.a_minus_b = c.a_minus_b.value_or(0.0);
  result.b_minus_a = c.b_minus_a.value_or(0.0);
  result.relative_difference = c.relative_difference.value_or(0.0);
  result.a_to_b = deviation(c.a_to_b);
  result.b_to_a = deviation(c.b_to_a);
  result.max_deviation = c.max_deviation;
  result.bounds_difference = c.bounds_difference;
  return result;
}

// .f3d import (T1).

AnalysisComparison analysis_compare_shapes(const ShapeList& a, const ShapeList& b,
                                           std::size_t samples, double fuzzy) {
  const geometry::CheckedOperation check("comparison");
  const std::vector<TopoDS_Shape> mine = occt(a);
  const std::vector<TopoDS_Shape> theirs = occt(b);
  if (mine.empty() || theirs.empty()) {
    throw std::invalid_argument("there are no bodies to compare");
  }
  analysis::CompareOptions options;
  options.samples = samples;
  options.fuzzy = fuzzy;
  const analysis::Comparison c = analysis::compare(compound(mine), compound(theirs), options);
  AnalysisComparison result{};
  result.volume_a = c.volume_a;
  result.volume_b = c.volume_b;
  result.has_differences = c.a_minus_b.has_value() && c.b_minus_a.has_value();
  result.a_minus_b = c.a_minus_b.value_or(0.0);
  result.b_minus_a = c.b_minus_a.value_or(0.0);
  result.relative_difference = c.relative_difference.value_or(0.0);
  result.a_to_b = deviation(c.a_to_b);
  result.b_to_a = deviation(c.b_to_a);
  result.max_deviation = c.max_deviation;
  result.bounds_difference = c.bounds_difference;
  return result;
}

rust::Vec<double> analysis_boundary_distances(const geometry::Shape& shape,
                                              rust::Slice<const double> points) {
  const geometry::CheckedOperation check("boundary distances");
  if (points.size() % 3 != 0) {
    throw std::invalid_argument("points come as x, y, z triples");
  }
  // The faces only, so that a point inside a solid has its distance to
  // the boundary, not zero.
  BRep_Builder builder;
  TopoDS_Compound faces;
  builder.MakeCompound(faces);
  // A point on the boundary (most points asked about are) is found on a
  // face whose box holds it: projected onto its surface and classified in
  // its parameter space, without measuring the other faces.
  struct Probe {
    TopoDS_Face face;
    Bnd_Box box;
    Handle(ShapeAnalysis_Surface) surface;
    std::unique_ptr<BRepTopAdaptor_FClass2d> inside;
  };
  std::vector<Probe> probes;
  for (TopExp_Explorer it(shape.occt(), TopAbs_FACE); it.More(); it.Next()) {
    builder.Add(faces, it.Current());
    Probe probe;
    probe.face = TopoDS::Face(it.Current());
    BRepBndLib::Add(probe.face, probe.box);
    probe.box.Enlarge(kOnBoundary);
    probes.push_back(std::move(probe));
  }
  const auto on_boundary = [](Probe& probe, const gp_Pnt& p) {
    if (probe.surface.IsNull()) {
      probe.surface = new ShapeAnalysis_Surface(BRep_Tool::Surface(probe.face));
      probe.inside = std::make_unique<BRepTopAdaptor_FClass2d>(probe.face, kOnBoundary);
    }
    const gp_Pnt2d uv = probe.surface->ValueOfUV(p, kOnBoundary);
    return probe.surface->Gap() <= kOnBoundary && probe.inside->Perform(uv) != TopAbs_OUT;
  };
  rust::Vec<double> out;
  BRepExtrema_DistShapeShape distance;
  distance.LoadS2(faces);
  for (std::size_t i = 0; i < points.size(); i += 3) {
    const gp_Pnt p(points[i], points[i + 1], points[i + 2]);
    const bool on = std::any_of(probes.begin(), probes.end(), [&](Probe& probe) {
      return !probe.box.IsOut(p) && on_boundary(probe, p);
    });
    if (on) {
      out.push_back(0.0);
      continue;
    }
    const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(p).Vertex();
    distance.LoadS1(vertex);
    distance.Perform();
    out.push_back(distance.IsDone() && distance.NbSolution() > 0
                      ? distance.Value()
                      : std::numeric_limits<double>::infinity());
  }
  return out;
}

rust::Vec<bool> analysis_points_inside(const geometry::Shape& shape, rust::Slice<const double> points) {
  const geometry::CheckedOperation check("points inside");
  if (points.size() % 3 != 0) {
    throw std::invalid_argument("points come as x, y, z triples");
  }
  const std::size_t count = points.size() / 3;
  std::vector<bool> inside(count, false);
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    // One classifier per solid (its set-up is the expensive part), and
    // points outside the solid's box are outside.
    Bnd_Box box;
    BRepBndLib::Add(it.Current(), box);
    std::vector<std::size_t> candidates;
    for (std::size_t i = 0; i < count; ++i) {
      if (!inside[i] && !box.IsOut(gp_Pnt(points[3 * i], points[3 * i + 1], points[3 * i + 2]))) {
        candidates.push_back(i);
      }
    }
    if (candidates.empty()) {
      continue;
    }
    BRepClass3d_SolidClassifier classifier(it.Current());
    for (const std::size_t i : candidates) {
      classifier.Perform(gp_Pnt(points[3 * i], points[3 * i + 1], points[3 * i + 2]), 1e-7);
      inside[i] = classifier.State() == TopAbs_IN;
    }
  }
  rust::Vec<bool> out;
  for (const bool in : inside) {
    out.push_back(in);
  }
  return out;
}

rust::Vec<AnalysisEdgeMiddle> analysis_edge_middles(const geometry::Shape& shape) {
  const geometry::CheckedOperation check("edge middles");
  rust::Vec<AnalysisEdgeMiddle> out;
  for (const geometry::EdgeMiddle& middle : geometry::edge_middles(shape)) {
    AnalysisEdgeMiddle m;
    m.name = rust::String(middle.name);
    m.point = {middle.point.X(), middle.point.Y(), middle.point.Z()};
    m.tangent = {middle.tangent.X(), middle.tangent.Y(), middle.tangent.Z()};
    m.length = middle.length;
    out.push_back(std::move(m));
  }
  return out;
}

rust::Vec<AnalysisFacePoints> analysis_face_points(const geometry::Shape& shape) {
  const geometry::CheckedOperation check("face points");
  rust::Vec<AnalysisFacePoints> out;
  for (const geometry::FacePoints& face : geometry::face_points(shape)) {
    AnalysisFacePoints f;
    f.name = rust::String(face.name);
    for (const gp_Pnt& p : face.points) {
      f.points.push_back(p.X());
      f.points.push_back(p.Y());
      f.points.push_back(p.Z());
    }
    out.push_back(std::move(f));
  }
  return out;
}

std::size_t analysis_face_count(const geometry::Shape& shape) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(shape.occt(), TopAbs_FACE, faces);
  return static_cast<std::size_t>(faces.Extent());
}

} // namespace mitcad::bridge
