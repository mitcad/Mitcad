// SPDX-License-Identifier: MIT
#include "mitcad/geometry/rib.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>

#include "history.hpp"
#include "spine.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

using Names = std::vector<std::pair<TopoDS_Edge, std::string>>;

// The offsets of a wall's two sides from its curves along the direction of
// side 1.
std::pair<double, double> sides(ThicknessLocation location, double thickness) {
  switch (location) {
  case ThicknessLocation::Side1:
    return {0.0, thickness};
  case ThicknessLocation::Side2:
    return {-thickness, 0.0};
  case ThicknessLocation::Symmetric:
    break;
  }
  return {-thickness / 2.0, thickness / 2.0};
}

// The planar face the wire bounds; throws when it is not a valid region.
TopoDS_Face band_face(const gp_Pln& plane, const TopoDS_Wire& wire, const char* what) {
  BRepBuilderAPI_MakeFace maker(plane, wire, true);
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Face()).IsValid()) {
    throw std::invalid_argument(std::string("the ") + what +
                                " cannot be built from its curves; they may fold back on themselves");
  }
  return maker.Face();
}

// The band swept along `vector` from `offset` into a solid: faces named
// after the band's edges, the cap at the band `near` and the other `far`.
ShapePtr band_prism(const TopoDS_Face& band, const Names& names, const gp_Vec& offset,
                    const gp_Vec& vector, const NameList& near, const NameList& far) {
  gp_Trsf move;
  move.SetTranslation(offset);
  BRepBuilderAPI_Transform moved(band, move, true);
  Names moved_names;
  for (const auto& [edge, name] : names) {
    moved_names.emplace_back(TopoDS::Edge(moved.ModifiedShape(edge)), name);
  }
  detail::SweepFace face{TopoDS::Face(moved.Shape()), moved_names, {}};
  return detail::named_prism(face, vector, near, far);
}

// The tool cut by the bodies, keeping the pieces that hold a face named
// `keep`: it must end at the bodies, so no piece may hold `far`.
ShapePtr to_bodies(const ShapePtr& tool, const std::vector<ShapePtr>& bodies, const std::string& keep,
                   const std::string& far, const char* what) {
  if (bodies.empty()) {
    throw std::invalid_argument(std::string("there are no bodies for the ") + what + " to reach");
  }
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(tool->occt());
  NCollection_List<TopoDS_Shape> tools;
  for (const ShapePtr& body : bodies) {
    tools.Append(body->occt());
  }
  cut.SetArguments(arguments);
  cut.SetTools(tools);
  cut.SetNonDestructive(true);
  detail::build(cut);
  if (!cut.IsDone() || cut.HasErrors()) {
    throw std::runtime_error(std::string("the ") + what + " could not be trimmed to the bodies");
  }
  detail::FaceNamer namer(cut.Shape());
  namer.carry(cut, *tool);
  std::vector<ShapePtr> kept;
  for (const detail::FaceNamer::Piece& piece : namer.pieces()) {
    bool holds = false;
    for (int i = 0; i < piece.shape->face_count() && !holds; ++i) {
      for (const std::string& name : piece.shape->face_names(i)) {
        holds = holds || name_matches(name, keep);
      }
    }
    if (holds) {
      if (detail::has_face_named(*piece.shape, far)) {
        throw std::invalid_argument(std::string("the ") + what +
                                    " does not end at faces of the bodies; flip it or give it a depth");
      }
      kept.push_back(piece.shape);
    }
  }
  if (kept.empty()) {
    throw std::invalid_argument(std::string("the ") + what + " lies inside the bodies");
  }
  return detail::fused(kept, what);
}

// Long enough to pass every body and the curves.
double reach(const RibSpec& spec, const std::vector<TopoDS_Wire>& wires) {
  Bnd_Box box;
  for (const ShapePtr& body : spec.bodies) {
    box.Add(detail::bounds_of(body->occt()));
  }
  for (const TopoDS_Wire& wire : wires) {
    box.Add(detail::bounds_of(wire));
  }
  return 2.0 * std::sqrt(box.SquareExtent()) + 10.0;
}

ShapePtr rib_tool(const RibSpec& spec) {
  const Path& chain = spec.chains.front();
  const TopoDS_Wire wire = detail::path_wire(chain);
  const detail::Spine spine(wire);
  const gp_Pnt a = spine.point(0.0);
  const gp_Pnt b = spine.point(spine.length());
  const gp_Vec chord(a, b);
  if (chord.Magnitude() <= Precision::Confusion()) {
    throw std::invalid_argument("a rib needs an open chain of curves");
  }
  const gp_Dir normal = spec.frame.normal();
  gp_Dir across = normal.Crossed(gp_Dir(chord));
  if (spec.flip) {
    across.Reverse();
  }
  const double length = spec.depth ? *spec.depth : reach(spec, {wire});
  const gp_Vec depth = gp_Vec(across) * length;
  // The band in the sketch plane: the curves in path order, the end at
  // their start, the far side back, and the end at their start.
  gp_Trsf shift;
  shift.SetTranslation(depth);
  BRepBuilderAPI_MakeWire outline;
  Names names;
  const auto add = [&](const TopoDS_Edge& edge, const std::string& name) {
    outline.Add(edge);
    if (!outline.IsDone()) {
      throw std::invalid_argument("the rib's curves do not join");
    }
    names.emplace_back(outline.Edge(), name);
  };
  std::vector<TopoDS_Edge> edges;
  for (const PathCurve& piece : chain) {
    edges.push_back(detail::model_edge(piece.curve));
    add(edges.back(), face_name(spec.feature, "side", piece.name));
  }
  add(BRepBuilderAPI_MakeEdge(b, b.Translated(depth)).Edge(), face_name(spec.feature, "tip1"));
  for (auto it = edges.rbegin(); it != edges.rend(); ++it) {
    add(TopoDS::Edge(BRepBuilderAPI_Transform(*it, shift, true).Shape()), face_name(spec.feature, "end"));
  }
  add(BRepBuilderAPI_MakeEdge(a.Translated(depth), a).Edge(), face_name(spec.feature, "tip0"));
  const TopoDS_Face band = band_face(gp_Pln(a, normal), outline.Wire(), "rib");
  const auto [low, high] = sides(spec.location, spec.thickness);
  const ShapePtr tool = band_prism(band, names, gp_Vec(normal) * low, gp_Vec(normal) * (high - low),
                                   {face_name(spec.feature, "wall2")}, {face_name(spec.feature, "wall1")});
  if (spec.depth) {
    return tool;
  }
  return to_bodies(tool, spec.bodies, face_name(spec.feature, "side", chain.front().name),
                   face_name(spec.feature, "end"), "rib");
}

// The band of a web along one curve in the sketch plane, between the offsets
// `low` and `high` to its left; edges named wall1, wall2, tip0 and tip1.
TopoDS_Face web_band(const RibSpec& spec, const PathCurve& curve, double low, double high, Names& names) {
  const gp_Dir normal = spec.frame.normal();
  const std::string& key = curve.name;
  const ModelCurve& c = curve.curve;
  TopoDS_Edge left;  // at the larger offset: side 1
  TopoDS_Edge right; // at the smaller
  gp_Pnt start_low, start_high, end_low, end_high;
  if (c.kind == ModelCurveKind::Line) {
    const gp_Vec along(c.start, c.end);
    if (along.Magnitude() <= Precision::Confusion()) {
      throw std::invalid_argument("web curve " + key + " has no length");
    }
    const gp_Vec side = gp_Vec(normal.Crossed(gp_Dir(along)));
    start_low = c.start.Translated(side * low);
    start_high = c.start.Translated(side * high);
    end_low = c.end.Translated(side * low);
    end_high = c.end.Translated(side * high);
    left = BRepBuilderAPI_MakeEdge(start_high, end_high).Edge();
    right = BRepBuilderAPI_MakeEdge(start_low, end_low).Edge();
  } else if (c.kind == ModelCurveKind::Conic && !c.closed &&
             std::abs(c.major - c.minor) <= 1e-9 * std::max(1.0, c.major)) {
    // Left of an arc counter-clockwise about the sketch normal is towards
    // its centre.
    const double turn = gp_Vec(c.normal).Dot(gp_Vec(normal)) > 0.0 ? 1.0 : -1.0;
    const double r_low = c.major - turn * low;
    const double r_high = c.major - turn * high;
    if (r_low <= Precision::Confusion() || r_high <= Precision::Confusion()) {
      throw std::invalid_argument("the web is thicker than arc " + key + " allows");
    }
    const gp_Ax2 axes(c.center, c.normal, c.x_axis);
    left = BRepBuilderAPI_MakeEdge(gp_Circ(axes, r_high), c.first, c.last).Edge();
    right = BRepBuilderAPI_MakeEdge(gp_Circ(axes, r_low), c.first, c.last).Edge();
    const auto at = [&](double radius, double t) {
      return c.center.Translated(gp_Vec(c.x_axis) * (radius * std::cos(t)) +
                                 gp_Vec(axes.YDirection()) * (radius * std::sin(t)));
    };
    start_low = at(r_low, c.first);
    start_high = at(r_high, c.first);
    end_low = at(r_low, c.last);
    end_high = at(r_high, c.last);
  } else {
    throw std::invalid_argument("unsupported: web curves other than lines and arcs (" + key + ")");
  }
  BRepBuilderAPI_MakeWire outline;
  const std::pair<TopoDS_Edge, const char*> roles[] = {
      {right, "wall2"},
      {BRepBuilderAPI_MakeEdge(end_low, end_high).Edge(), "tip1"},
      {left, "wall1"},
      {BRepBuilderAPI_MakeEdge(start_low, start_high).Edge(), "tip0"}};
  for (const auto& [edge, role] : roles) {
    outline.Add(edge);
    if (!outline.IsDone()) {
      throw std::invalid_argument("the web along " + key + " does not close");
    }
    names.emplace_back(outline.Edge(), face_name(spec.feature, role, key));
  }
  // Any point of the band's plane: the curve's start.
  const gp_Pnt on_plane = c.kind == ModelCurveKind::Line ? c.start : c.center;
  return band_face(gp_Pln(on_plane, normal), outline.Wire(), "web");
}

ShapePtr web_tool(const RibSpec& spec) {
  const gp_Dir normal = spec.frame.normal();
  gp_Dir up = normal;
  if (spec.flip) {
    up.Reverse();
  }
  std::vector<TopoDS_Wire> wires;
  for (const Path& chain : spec.chains) {
    wires.push_back(detail::path_wire(chain));
  }
  const double length = spec.depth ? *spec.depth : reach(spec, wires);
  const auto [low, high] = sides(spec.location, spec.thickness);
  std::vector<ShapePtr> parts;
  for (const Path& chain : spec.chains) {
    for (const PathCurve& curve : chain) {
      Names names;
      const TopoDS_Face band = web_band(spec, curve, low, high, names);
      const std::string start = face_name(spec.feature, "start", curve.name);
      const std::string end = face_name(spec.feature, "end", curve.name);
      ShapePtr tool = band_prism(band, names, gp_Vec(0, 0, 0), gp_Vec(up) * length, {start}, {end});
      if (!spec.depth) {
        tool = to_bodies(tool, spec.bodies, start, end, "web");
      }
      parts.push_back(tool);
    }
  }
  return detail::fused(parts, "the web");
}

} // namespace

ShapePtr rib(const RibSpec& spec) {
  const char* what = spec.web ? "web" : "rib";
  detail::require_positive(spec.web ? "web thickness" : "rib thickness", spec.thickness);
  if (spec.depth) {
    detail::require_positive(spec.web ? "web depth" : "rib depth", *spec.depth);
  }
  if (spec.chains.empty() || spec.chains.front().empty()) {
    throw std::invalid_argument(std::string("the ") + what + " has no curves");
  }
  if (!spec.web && spec.chains.size() != 1) {
    throw std::invalid_argument("a rib grows from one chain of curves");
  }
  return detail::run(what, [&] {
    const ShapePtr tool = spec.web ? web_tool(spec) : rib_tool(spec);
    return detail::finished(*tool, spec.web ? "the web" : "the rib");
  });
}

} // namespace mitcad::geometry
