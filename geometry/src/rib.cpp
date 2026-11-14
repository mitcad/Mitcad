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
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
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

// True when a face of the shape has a name matching `name`.
bool holds(const Shape& shape, const std::string& name) {
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& face : shape.face_names(i)) {
      if (name_matches(face, name)) {
        return true;
      }
    }
  }
  return false;
}

// The tool cut by the shapes, one piece per solid, its faces named after
// the tool's.
std::vector<ShapePtr> cut_pieces(const ShapePtr& tool, const std::vector<TopoDS_Shape>& shapes, const char* what) {
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(tool->occt());
  NCollection_List<TopoDS_Shape> tools;
  for (const TopoDS_Shape& shape : shapes) {
    tools.Append(shape);
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
  std::vector<ShapePtr> pieces;
  for (const detail::FaceNamer::Piece& piece : namer.pieces()) {
    pieces.push_back(piece.shape);
  }
  return pieces;
}

std::vector<TopoDS_Shape> shapes_of(const std::vector<ShapePtr>& bodies) {
  std::vector<TopoDS_Shape> shapes;
  for (const ShapePtr& body : bodies) {
    shapes.push_back(body->occt());
  }
  return shapes;
}

// The tool cut by the bodies, keeping the pieces that hold a face named
// `keep`: it must end at the bodies, so no piece may hold `far`.
ShapePtr to_bodies(const ShapePtr& tool, const std::vector<ShapePtr>& bodies, const std::string& keep,
                   const std::string& far, const char* what) {
  if (bodies.empty()) {
    throw std::invalid_argument(std::string("there are no bodies for the ") + what + " to reach");
  }
  std::vector<ShapePtr> kept;
  for (const ShapePtr& piece : cut_pieces(tool, shapes_of(bodies), what)) {
    if (holds(*piece, keep)) {
      if (detail::has_face_named(*piece, far)) {
        throw std::invalid_argument(std::string("the ") + what +
                                    " does not end at faces of the bodies; flip it or give it a depth");
      }
      kept.push_back(piece);
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

// One open end of a rib's chain (mitcad#1746): where it is, its tangent out
// of the chain, and its names.
struct RibEnd {
  gp_Pnt point;
  gp_Dir outward;
  const char* which; // "start" or "end", for the notes
  const char* tip;
  const char* extend;
  const char* reach;
};

// What the notes of an end that stays as drawn say (no names or places in
// the text; the place goes with the note).
void stays(const RibEnd& end, const char* why, std::vector<std::pair<std::string, gp_Pnt>>& notes) {
  notes.emplace_back(std::string("the ") + end.which + " of the rib's curves " + why + "; it stays where it is drawn",
                     end.point);
}

// The continuation of one end of a rib (mitcad#1746): the band beyond the
// end's tip, along the end's tangent for `length` and as deep as the rib,
// cut by the bodies (moved along the tangent by the end offset), and of it
// the pieces that touch the rib: so it fills from the tip up to the faces
// the end meets, whatever their shape. Null when there is nothing between
// the tip and the faces, or with a note when the continuation meets no
// face that closes it off.
ShapePtr extension(const RibSpec& spec, const Shape& drawn, const RibEnd& end, const gp_Vec& depth, double low,
                   double high, double length, std::vector<std::pair<std::string, gp_Pnt>>& notes) {
  if (spec.bodies.empty()) {
    stays(end, "has no bodies to be continued to", notes);
    return nullptr;
  }
  // Along the way the rib grows, the continuation would have no width.
  if (gp_Vec(end.outward).Crossed(depth).Magnitude() <= 1.0e-6 * depth.Magnitude()) {
    stays(end, "runs along the rib's direction there and cannot be continued", notes);
    return nullptr;
  }
  const gp_Dir normal = spec.frame.normal();
  const gp_Pnt p = end.point;
  const gp_Pnt q = p.Translated(gp_Vec(end.outward) * length);
  const std::string reach_name = face_name(spec.feature, end.reach);
  const std::string far_side = face_name(spec.feature, "end");
  BRepBuilderAPI_MakeWire outline;
  Names names;
  const auto add = [&](const gp_Pnt& from, const gp_Pnt& to, const std::string& name) {
    outline.Add(BRepBuilderAPI_MakeEdge(from, to).Edge());
    if (!outline.IsDone()) {
      throw std::invalid_argument("the rib's ends cannot be continued");
    }
    names.emplace_back(outline.Edge(), name);
  };
  add(p, q, face_name(spec.feature, end.extend));
  add(q, q.Translated(depth), reach_name);
  add(q.Translated(depth), p.Translated(depth), far_side);
  add(p.Translated(depth), p, face_name(spec.feature, end.tip));
  const TopoDS_Face band = band_face(gp_Pln(p, normal), outline.Wire(), "rib");
  const ShapePtr slab = band_prism(band, names, gp_Vec(normal) * low, gp_Vec(normal) * (high - low),
                                   {face_name(spec.feature, "wall2")}, {face_name(spec.feature, "wall1")});
  // The faces it stops at, moved along the tangent by the offset (copies:
  // the bodies stay as they are).
  std::vector<TopoDS_Shape> stops = shapes_of(spec.bodies);
  if (spec.end_offset != 0.0) {
    gp_Trsf move;
    move.SetTranslation(gp_Vec(end.outward) * spec.end_offset);
    for (TopoDS_Shape& stop : stops) {
      stop = stop.Moved(TopLoc_Location(move));
    }
  }
  std::vector<ShapePtr> kept;
  for (const ShapePtr& piece : cut_pieces(slab, stops, "rib")) {
    BRepExtrema_DistShapeShape distance(piece->occt(), drawn.occt());
    if (!distance.IsDone() || distance.Value() > 1.0e-5) {
      continue;
    }
    // Not closed off by the faces: it would run on to the far side of the
    // bodies' bounds (or, up to the next faces, past their depth).
    if (holds(*piece, reach_name) || (!spec.depth && holds(*piece, far_side))) {
      stays(end, "meets no face of the bodies when continued along its curve", notes);
      return nullptr;
    }
    kept.push_back(piece);
  }
  if (kept.empty()) {
    return nullptr;
  }
  return detail::fused(kept, "the rib");
}

ShapePtr rib_tool(const RibSpec& spec, std::vector<std::pair<std::string, gp_Pnt>>& notes) {
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
  if (spec.direction) {
    // Its Direction (mitcad#1746), kept in the sketch plane.
    const gp_Vec given(*spec.direction);
    const gp_Vec flat = given - gp_Vec(normal) * given.Dot(gp_Vec(normal));
    if (flat.Magnitude() <= 1.0e-6) {
      throw std::invalid_argument("the rib's direction does not lie in its sketch plane");
    }
    across = gp_Dir(flat);
    if (gp_Vec(across).Crossed(gp_Vec(gp_Dir(chord))).Magnitude() <= 1.0e-6) {
      throw std::invalid_argument("the rib's direction runs along its curves' chord");
    }
  }
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
  const ShapePtr drawn = spec.depth ? tool
                                    : to_bodies(tool, spec.bodies, face_name(spec.feature, "side", chain.front().name),
                                                face_name(spec.feature, "end"), "rib");
  if (!spec.extend_ends) {
    return drawn;
  }
  // Extend Ends (mitcad#1746): each end continued along its tangent, far
  // enough to leave the bodies' bounds (and the offset's more).
  const RibEnd ends[] = {
      {a, spine.tangent(0.0).Reversed(), "start", "tip0", "extend0", "reach0"},
      {b, spine.tangent(spine.length()), "end", "tip1", "extend1", "reach1"},
  };
  const double run_out = reach(spec, {wire}) + std::abs(spec.end_offset);
  std::vector<ShapePtr> parts{drawn};
  for (const RibEnd& end : ends) {
    if (const ShapePtr more = extension(spec, *drawn, end, depth, low, high, run_out, notes)) {
      parts.push_back(more);
    }
  }
  return detail::fused(parts, "the rib");
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
      throw std::invalid_argument(placed_error("a web curve has no length", "web curve " + key + " has no length"));
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
  if (spec.web && spec.extend_ends) {
    throw std::invalid_argument("a web's curves end where they are drawn");
  }
  if (spec.web && spec.direction) {
    throw std::invalid_argument("a web grows along the sketch normal");
  }
  return detail::run(what, [&] {
    std::vector<std::pair<std::string, gp_Pnt>> notes;
    const ShapePtr tool = spec.web ? web_tool(spec) : rib_tool(spec, notes);
    const ShapePtr result = detail::finished(*tool, spec.web ? "the web" : "the rib");
    // The ends that stay as drawn (mitcad#1746), each at its end.
    for (const auto& [note, at] : notes) {
      result->add_note(note, {Shape::NotePlace::at(at.X(), at.Y(), at.Z())});
    }
    return result;
  });
}

} // namespace mitcad::geometry
