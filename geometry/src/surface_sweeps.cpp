// SPDX-License-Identifier: MIT
#include "mitcad/geometry/surface_sweeps.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom2d_Line.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Line.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <Precision.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeFix_Wire.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "history.hpp"
#include "loft_merge.hpp"
#include "spine.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kFullTurn = 6.28318530717958647692;

// A chain's wire and the curve each of its edges comes from.
struct ChainWire {
  TopoDS_Wire wire;
  std::vector<std::pair<TopoDS_Edge, std::string>> edges;
};

gp_Pnt middle_of(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
}

// The chain's curves joined into a wire. The wire maker may copy an edge
// to join it: each edge of the wire is the curve whose middle is nearest
// to its own.
ChainWire chain_wire(const Path& chain) {
  if (chain.empty()) {
    throw std::invalid_argument("a chain has no curves");
  }
  BRepBuilderAPI_MakeWire make;
  std::vector<std::pair<gp_Pnt, std::string>> middles;
  for (const PathCurve& piece : chain) {
    const TopoDS_Edge edge = detail::model_edge(piece.curve);
    make.Add(edge);
    if (!make.IsDone()) {
      throw std::invalid_argument(placed_error("a curve does not join the curve before it",
                                               "curve " + piece.name + " does not join the curve before it"));
    }
    middles.emplace_back(middle_of(edge), piece.name);
  }
  ChainWire result;
  result.wire = make.Wire();
  for (TopExp_Explorer it(result.wire, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const gp_Pnt middle = middle_of(edge);
    double best = std::numeric_limits<double>::infinity();
    std::string name;
    for (const auto& [point, curve] : middles) {
      const double d = point.Distance(middle);
      if (d < best) {
        best = d;
        name = curve;
      }
    }
    result.edges.emplace_back(edge, name);
  }
  return result;
}

// An end of an open chain: whether it is its edge's last parameter, and
// its point.
struct ChainEnd {
  bool at_last = false;
  gp_Pnt point;
  bool exact = false; // the end of a line: a sketch's own point
};

// The two ends of an open chain's wire, its start first.
std::pair<ChainEnd, ChainEnd> chain_ends(const TopoDS_Wire& wire) {
  std::vector<TopoDS_Edge> ordered;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    ordered.push_back(it.Current());
  }
  if (ordered.empty()) {
    throw std::invalid_argument("a chain has no curves");
  }
  const auto end = [](const TopoDS_Edge& edge, bool start) {
    ChainEnd result;
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    // Along the wire a reversed edge starts at its last parameter.
    result.at_last = (edge.Orientation() == TopAbs_REVERSED) == start;
    result.point = curve->Value(result.at_last ? last : first);
    occ::handle<Geom_Curve> basis = curve;
    if (const occ::handle<Geom_TrimmedCurve> trimmed = occ::down_cast<Geom_TrimmedCurve>(basis)) {
      basis = trimmed->BasisCurve();
    }
    result.exact = !occ::down_cast<Geom_Line>(basis).IsNull();
    return result;
  };
  return {end(ordered.front(), true), end(ordered.back(), false)};
}

// The edge with the end at its last parameter (else its first) moved
// exactly onto `target`, the rest of it as it was: a B-spline whose pole
// there is the point (a line's own straight edge to it).
TopoDS_Edge edge_ending_at(const TopoDS_Edge& edge, bool at_last, const gp_Pnt& target) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  occ::handle<Geom_BSplineCurve> spline =
      GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
  spline->SetPole(at_last ? spline->NbPoles() : 1, target);
  BRepBuilderAPI_MakeEdge maker(spline);
  if (!maker.IsDone()) {
    throw std::runtime_error("the sections could not be lofted");
  }
  TopoDS_Edge result = maker.Edge();
  result.Orientation(edge.Orientation());
  return result;
}

// Open sections whose ends meet (within the loft's tolerance) made to meet
// exactly (mitcad#2584): a fan's sections from one point. A sketch arc's
// end is its circle's point at an angle, a rounding off the sketch's point
// (7e-15 mm on Windows), and the loft then joins the ends by a side edge
// that long on a surface that does not quite collapse there. Each meeting
// is moved onto one point (a line's end where one ends there, those being
// the sketch's own points), so the loft's side there has no length at all
// and its surface collapses to the point (made a degenerated edge by
// with_meeting_points). Whether any ends meet.
bool join_meeting_ends(std::vector<ChainWire>& wires, double tolerance) {
  std::vector<std::pair<ChainEnd, ChainEnd>> ends;
  for (const ChainWire& wire : wires) {
    ends.push_back(chain_ends(wire.wire));
  }
  const auto end_of = [&](std::size_t k) -> ChainEnd& { return k % 2 == 0 ? ends[k / 2].first : ends[k / 2].second; };
  // The new edge for each edge moved, with its ends to move.
  std::vector<std::vector<std::pair<bool, gp_Pnt>>> moves(wires.size());
  std::vector<bool> taken(2 * wires.size(), false);
  bool any = false;
  bool met = false;
  for (std::size_t k = 0; k < 2 * wires.size(); ++k) {
    if (taken[k]) {
      continue;
    }
    std::vector<std::size_t> meeting{k};
    for (std::size_t m = k + 1; m < 2 * wires.size(); ++m) {
      if (!taken[m] && m / 2 != k / 2 && end_of(k).point.Distance(end_of(m).point) <= tolerance) {
        meeting.push_back(m);
      }
    }
    if (meeting.size() < 2) {
      continue;
    }
    met = true;
    gp_Pnt target = end_of(meeting.front()).point;
    for (const std::size_t m : meeting) {
      taken[m] = true;
    }
    for (const std::size_t m : meeting) {
      if (end_of(m).exact) {
        target = end_of(m).point;
        break;
      }
    }
    for (const std::size_t m : meeting) {
      ChainEnd& end = end_of(m);
      if (end.point.X() == target.X() && end.point.Y() == target.Y() && end.point.Z() == target.Z()) {
        continue;
      }
      end.point = target;
      moves[m / 2].emplace_back(m % 2 == 1, target);
      any = true;
    }
  }
  if (!any) {
    return met;
  }
  for (std::size_t i = 0; i < wires.size(); ++i) {
    if (moves[i].empty()) {
      continue;
    }
    // The edges in the wire's order, those at a moved end replaced.
    std::vector<TopoDS_Edge> ordered;
    for (BRepTools_WireExplorer it(wires[i].wire); it.More(); it.Next()) {
      ordered.push_back(it.Current());
    }
    for (const auto& [at_wire_end, target] : moves[i]) {
      TopoDS_Edge& edge = at_wire_end ? ordered.back() : ordered.front();
      const bool reversed = edge.Orientation() == TopAbs_REVERSED;
      // The wire's start is the edge's first parameter unless reversed.
      edge = edge_ending_at(edge, at_wire_end != reversed, target);
    }
    std::vector<std::pair<gp_Pnt, std::string>> middles;
    for (const auto& [edge, name] : wires[i].edges) {
      middles.emplace_back(middle_of(edge), name);
    }
    BRepBuilderAPI_MakeWire make;
    for (const TopoDS_Edge& edge : ordered) {
      make.Add(edge);
      if (!make.IsDone()) {
        throw std::runtime_error("the sections could not be lofted");
      }
    }
    ChainWire joined;
    joined.wire = make.Wire();
    for (TopExp_Explorer it(joined.wire, TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      const gp_Pnt middle = middle_of(edge);
      double best = std::numeric_limits<double>::infinity();
      std::string name;
      for (const auto& [point, curve] : middles) {
        const double d = point.Distance(middle);
        if (d < best) {
          best = d;
          name = curve;
        }
      }
      joined.edges.emplace_back(edge, name);
    }
    wires[i] = std::move(joined);
  }
  return met;
}

// The loft's side where open sections meet, an edge of no length on a
// surface that collapses to the point there, made the degenerated edge of
// one vertex that the point is (mitcad#2584): the offset of a thicken
// leaves out a degenerated edge's point where it checks the surface's
// normals, which the collapsed side has none of, and builds the offset
// face's apex from it. The faces are those of the loft, repaired.
TopoDS_Shape with_meeting_points(const TopoDS_Shape& loft, std::vector<std::pair<TopoDS_Shape, std::string>>& named) {
  ShapeFix_Shape fix(loft);
  fix.SetPrecision(Precision::Confusion());
  fix.SetMaxTolerance(1.0e-5);
  fix.FixWireTool()->ModifyTopologyMode() = true;
  fix.Perform();
  const TopoDS_Shape result = fix.Shape();
  if (result.IsNull()) {
    return loft;
  }
  std::vector<std::pair<TopoDS_Shape, std::string>> renamed;
  for (const auto& [face, name] : named) {
    for (TopExp_Explorer it(fix.Context()->Apply(face), TopAbs_FACE); it.More(); it.Next()) {
      renamed.emplace_back(it.Current(), name);
    }
  }
  named = std::move(renamed);
  return result;
}

// The wire moved by `trsf`, with the curves of its edges.
ChainWire moved(const ChainWire& chain, const gp_Trsf& trsf) {
  BRepBuilderAPI_Transform transform(chain.wire, trsf, true);
  ChainWire result;
  result.wire = TopoDS::Wire(transform.Shape());
  for (const auto& [edge, name] : chain.edges) {
    result.edges.emplace_back(TopoDS::Edge(transform.ModifiedShape(edge)), name);
  }
  return result;
}

// Faces of swept parts and their names, gathered into one surface body.
class SheetParts {
public:
  void add(const TopoDS_Shape& part) { m_parts.push_back(part); }
  // The faces `operation` generated from the chain's edges are named after
  // their curves.
  void name(BRepBuilderAPI_MakeShape& operation, const std::string& feature, const ChainWire& chain) {
    for (const auto& [edge, curve] : chain.edges) {
      for (const TopoDS_Shape& face : operation.Generated(edge)) {
        if (face.ShapeType() == TopAbs_FACE) {
          m_names.emplace_back(face, face_name(feature, "side", curve));
        }
      }
    }
  }
  void name(const TopoDS_Shape& face, const std::string& name) { m_names.emplace_back(face, name); }

  ShapePtr shape(const char* what) const {
    if (m_parts.empty()) {
      throw std::invalid_argument(std::string(what) + ": nothing to sweep");
    }
    TopoDS_Shape result = m_parts.front();
    if (m_parts.size() > 1) {
      BRep_Builder builder;
      TopoDS_Compound all;
      builder.MakeCompound(all);
      for (const TopoDS_Shape& part : m_parts) {
        builder.Add(all, part);
      }
      result = all;
    }
    detail::FaceNamer namer(result);
    for (const auto& [face, name] : m_names) {
      if (namer.contains(face)) {
        namer.add(face, name);
      }
    }
    namer.finish();
    ShapePtr sheet = namer.shape();
    detail::require_valid(sheet->occt(), what);
    return sheet;
  }

private:
  std::vector<TopoDS_Shape> m_parts;
  std::vector<std::pair<TopoDS_Shape, std::string>> m_names;
};

// Whether a face name is "<feature>:<role>" with a key or pieces.
bool has_role(const std::string& name, const std::string& feature, const std::vector<std::string>& roles) {
  for (const std::string& role : roles) {
    const std::string prefix = feature + ":" + role;
    if (name.compare(0, prefix.size(), prefix) == 0 &&
        (name.size() == prefix.size() || name[prefix.size()] == '(' || name[prefix.size()] == '#')) {
      return true;
    }
  }
  return false;
}

// The pipe shell of a chain along a spine starting at its section, in the
// orientation's mode.
TopoDS_Shape pipe_of(const ChainWire& chain, const TopoDS_Wire& spine, const Path& path,
                     SweepOrientation orientation, SheetParts& parts, const std::string& feature) {
  BRepOffsetAPI_MakePipeShell pipe(spine);
  pipe.SetTolerance(1.0e-4, 1.0e-4, 1.0e-2);
  if (orientation == SweepOrientation::Parallel) {
    pipe.SetMode(gp_Ax2(gp::Origin(), gp::DZ(), gp::DX()));
  } else {
    const detail::Spine measured(spine);
    const detail::PipeMode mode = detail::perpendicular_mode(measured, path);
    if (mode.kind == detail::PipeMode::Kind::Binormal) {
      pipe.SetMode(mode.binormal);
    } else {
      pipe.SetMode(false);
    }
  }
  // Mitred from a turn of 1e-4 rad on, as the pipes' mitres (port patch 1662).
  pipe.SetTransitionMode(BRepBuilderAPI_RightCorner, 1.0e-4);
  pipe.Add(chain.wire, false, false);
  detail::build(pipe);
  if (!pipe.IsDone()) {
    throw std::runtime_error("the curves could not be swept along the path");
  }
  parts.name(pipe, feature, chain);
  return pipe.Shape();
}

} // namespace

ShapePtr open_faces(const Shape& shape, const std::string& feature, const std::vector<std::string>& roles) {
  return detail::run("surface walls", [&] {
    const auto kept = [&](const TopoDS_Shape& face) {
      const NameList& names = shape.names_of_face(face);
      return std::none_of(names.begin(), names.end(),
                          [&](const std::string& name) { return has_role(name, feature, roles); });
    };
    BRep_Builder builder;
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> named;
    ShapeMap placed;
    for (TopExp_Explorer s(shape.occt(), TopAbs_SHELL); s.More(); s.Next()) {
      TopoDS_Shell shell;
      builder.MakeShell(shell);
      int count = 0;
      for (TopExp_Explorer f(s.Current(), TopAbs_FACE); f.More(); f.Next()) {
        placed.Add(f.Current());
        if (!kept(f.Current())) {
          continue;
        }
        builder.Add(shell, f.Current());
        named.push_back({f.Current(), shape.names_of_face(f.Current())});
        ++count;
      }
      if (count > 0) {
        shell.Closed(BRep_Tool::IsClosed(shell));
        parts.push_back(shell);
      }
    }
    // Faces outside shells stay as they are.
    for (TopExp_Explorer f(shape.occt(), TopAbs_FACE); f.More(); f.Next()) {
      if (!placed.Contains(f.Current()) && kept(f.Current())) {
        parts.push_back(f.Current());
        named.push_back({f.Current(), shape.names_of_face(f.Current())});
      }
    }
    if (parts.empty()) {
      throw std::invalid_argument("surface walls: no faces are left");
    }
    TopoDS_Shape result = parts.front();
    if (parts.size() > 1) {
      TopoDS_Compound all;
      builder.MakeCompound(all);
      for (const TopoDS_Shape& part : parts) {
        builder.Add(all, part);
      }
      result = all;
    }
    return std::make_shared<Shape>(result, named);
  });
}

ShapePtr extrude_sheet(const std::string& feature, const std::vector<Path>& chains, const gp_Vec& shift,
                       const gp_Dir& direction, double one, double two) {
  detail::require_finite("the distance", one);
  detail::require_finite("the second distance", two);
  if (!(one + two > 0.0) || one < 0.0 || two < 0.0) {
    throw std::invalid_argument("surface extrusion: the distances must not be negative and add up to more than zero");
  }
  return detail::run("surface extrusion", [&] {
    SheetParts parts;
    gp_Trsf start;
    start.SetTranslation(shift - gp_Vec(direction) * two);
    for (const Path& chain : chains) {
      const ChainWire wire = moved(chain_wire(chain), start);
      BRepPrimAPI_MakePrism prism(wire.wire, gp_Vec(direction) * (one + two));
      detail::build(prism);
      if (!prism.IsDone()) {
        throw std::runtime_error("the curves could not be extruded");
      }
      parts.add(prism.Shape());
      parts.name(prism, feature, wire);
    }
    return parts.shape("surface extrusion");
  });
}

ShapePtr revolve_sheet(const std::string& feature, const std::vector<Path>& chains, const gp_Ax1& axis,
                       double angle1, double angle2) {
  detail::require_finite("the angle", angle1);
  detail::require_finite("the second angle", angle2);
  const double total = angle1 + angle2;
  if (!(total > 0.0) || angle1 < 0.0 || angle2 < 0.0 || total > kFullTurn + 1.0e-9) {
    throw std::invalid_argument("surface revolution: the angles must not be negative and add up to "
                                "more than zero and at most a full turn");
  }
  return detail::run("surface revolution", [&] {
    SheetParts parts;
    gp_Trsf back;
    back.SetRotation(axis, -angle2);
    for (const Path& chain : chains) {
      const ChainWire wire = moved(chain_wire(chain), back);
      const bool full = total >= kFullTurn - 1.0e-9;
      const auto revol = full ? std::make_unique<BRepPrimAPI_MakeRevol>(wire.wire, axis)
                              : std::make_unique<BRepPrimAPI_MakeRevol>(wire.wire, axis, total);
      detail::build(*revol);
      if (!revol->IsDone()) {
        throw std::runtime_error("the curves could not be revolved");
      }
      parts.add(revol->Shape());
      parts.name(*revol, feature, wire);
    }
    return parts.shape("surface revolution");
  });
}

ShapePtr sweep_sheet(const std::string& feature, const std::vector<Path>& chains, const Path& path,
                     const gp_Pln& plane, double extent1, double extent2, SweepOrientation orientation) {
  for (const double extent : {extent1, extent2}) {
    if (!(extent >= 0.0 && extent <= 1.0)) {
      throw std::invalid_argument("surface sweep: a fraction of the path must be between 0 and 1");
    }
  }
  if (!(extent1 + extent2 > 0.0)) {
    throw std::invalid_argument("surface sweep: the sweep covers none of the path");
  }
  return detail::run("surface sweep", [&] {
    const detail::Spine spine(detail::path_wire(path));
    const double length = spine.length();
    SheetParts parts;
    for (const Path& chain : chains) {
      const ChainWire wire = chain_wire(chain);
      // Where the path meets the curves' plane, nearest their start.
      const BRepAdaptor_Curve first(wire.edges.front().first);
      const double at = spine.locate(plane, first.Value(first.FirstParameter()));
      const double after = spine.closed() ? length : length - at;
      const double before = spine.closed() ? length : at;
      const double to = at + extent1 * after;
      const double from = at - extent2 * before;
      // The path from the curves on, and back from them reversed; each
      // pipe shell starts at the curves.
      if (to - at > 1.0e-9) {
        const TopoDS_Wire ahead = spine.part(at, to);
        parts.add(pipe_of(wire, ahead, path, orientation, parts, feature));
      }
      if (at - from > 1.0e-9) {
        const TopoDS_Wire behind = TopoDS::Wire(spine.part(from, at).Reversed());
        parts.add(pipe_of(wire, behind, path, orientation, parts, feature));
      }
    }
    return parts.shape("surface sweep");
  });
}

ShapePtr loft_sheet(const std::string& feature, const std::vector<Path>& sections, bool ruled, bool closed,
                    bool merge_tangent) {
  if (sections.size() < 2) {
    throw std::invalid_argument("surface loft: two or more sections are needed");
  }
  return detail::run("surface loft", [&] {
    std::vector<ChainWire> wires;
    for (const Path& section : sections) {
      wires.push_back(chain_wire(section));
    }
    const bool open = !BRep_Tool::IsClosed(wires.front().wire);
    for (const ChainWire& wire : wires) {
      if (BRep_Tool::IsClosed(wire.wire) == open) {
        throw std::invalid_argument("surface loft: the sections must be all open or all closed");
      }
    }
    // Tangent Edges: Merge (mitcad#2162): each chain's runs of tangent
    // curves joined; chains that each became one smooth closed edge start
    // at corresponding points and loft as they are.
    bool aligned = false;
    if (merge_tangent) {
      std::vector<detail::MergeSection> merging;
      for (const ChainWire& wire : wires) {
        detail::MergeSection one;
        one.named.wire = wire.wire;
        one.named.edges = wire.edges;
        merging.push_back(std::move(one));
      }
      aligned = detail::merge_tangent_edges(merging);
      for (std::size_t i = 0; i < wires.size(); ++i) {
        wires[i].wire = merging[i].named.wire;
        wires[i].edges = merging[i].named.edges;
      }
    }
    const bool met = open && join_meeting_ends(wires, Precision::Confusion());
    BRepOffsetAPI_ThruSections loft(false, ruled, 1.0e-6);
    loft.CheckCompatibility(!aligned);
    for (const ChainWire& wire : wires) {
      loft.AddWire(wire.wire);
    }
    if (closed) {
      loft.AddWire(wires.front().wire);
    }
    detail::build(loft);
    if (!loft.IsDone()) {
      throw std::runtime_error("the sections could not be lofted");
    }
    std::vector<std::pair<TopoDS_Shape, std::string>> named;
    for (const auto& [edge, curve] : wires.front().edges) {
      const TopoDS_Shape face = loft.GeneratedFace(edge);
      if (!face.IsNull()) {
        named.emplace_back(face, face_name(feature, "side", curve));
      }
    }
    SheetParts parts;
    parts.add(met ? with_meeting_points(loft.Shape(), named) : loft.Shape());
    for (const auto& [face, name] : named) {
      parts.name(face, name);
    }
    return parts.shape("surface loft");
  });
}

ShapePtr helix_sheet(const std::string& feature, const std::vector<Path>& chains, const gp_Ax1& axis, double pitch,
                     double revolutions, bool left_handed, const HelixSheetOptions& options) {
  detail::require_positive("helix pitch", pitch);
  detail::require_positive("helix revolutions", revolutions);
  detail::require_finite("helix growth", options.growth);
  bool ends = false;
  for (const double end : {options.start_flat, options.start_transition, options.end_transition, options.end_flat}) {
    detail::require_finite("helix end angle", end);
    if (end < 0.0) {
      throw std::invalid_argument("the helix's end angles must not be negative");
    }
    ends = ends || end > 0.0;
  }
  return detail::run("surface helix", [&] {
    SheetParts parts;
    const gp_Dir up = axis.Direction();
    for (const Path& chain : chains) {
      const ChainWire wire = chain_wire(chain);
      if (options.growth != 0.0 || ends) {
        // A growth or ends: the solid helix's motion (mitcad#1797).
        HelixSweepSpec spec;
        spec.feature = feature;
        spec.axis = axis;
        spec.pitch = pitch;
        spec.revolutions = revolutions;
        spec.left_handed = left_handed;
        spec.growth = options.growth;
        spec.start_flat = options.start_flat;
        spec.start_transition = options.start_transition;
        spec.end_transition = options.end_transition;
        spec.end_flat = options.end_flat;
        const detail::ScrewedWire swept = detail::screwed_wire(wire.wire, spec);
        parts.add(swept.shape);
        for (const auto& [edge, faces] : swept.faces) {
          for (const auto& [own, curve] : wire.edges) {
            if (own.IsSame(edge)) {
              for (const TopoDS_Shape& face : faces) {
                parts.name(face, face_name(feature, "side", curve));
              }
            }
          }
        }
        continue;
      }
      // A helix through the curves' middle: with the axis as the fixed
      // binormal every helix of the axis and pitch moves the curves alike,
      // by the screw motion (as the solid helix's profiles move).
      Bnd_Box box;
      BRepBndLib::Add(wire.wire, box);
      if (box.IsVoid()) {
        throw std::invalid_argument("surface helix: a chain has no extent");
      }
      const gp_Pnt middle((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0);
      const gp_Vec offset(axis.Location(), middle);
      const double along = offset.Dot(gp_Vec(up));
      const gp_Vec radial = offset - gp_Vec(up) * along;
      double radius = radial.Magnitude();
      gp_Dir x = detail::any_normal(up);
      if (radius > Precision::Confusion()) {
        x = gp_Dir(radial);
      } else {
        radius = 1.0;
      }
      gp_Ax3 axes(axis.Location().Translated(gp_Vec(up) * along), up, x);
      if (left_handed) {
        axes.YReverse();
      }
      // On the unrolled cylinder the helix is a line: a turn rises a pitch.
      const occ::handle<Geom_Surface> surface = new Geom_CylindricalSurface(axes, radius);
      const gp_Vec2d step(kFullTurn, pitch);
      const occ::handle<Geom2d_Line> line = new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(step));
      BRepBuilderAPI_MakeEdge helix(line, surface, 0.0, revolutions * step.Magnitude());
      if (!helix.IsDone()) {
        throw std::runtime_error("surface helix: the helix could not be built");
      }
      TopoDS_Edge edge = helix.Edge();
      const int samples = std::max(32, static_cast<int>(std::ceil(revolutions * 32.0)));
      BRepLib::BuildCurves3d(edge, 1.0e-7, GeomAbs_C1, 14, samples);
      const TopoDS_Wire spine = BRepBuilderAPI_MakeWire(edge).Wire();
      BRepOffsetAPI_MakePipeShell pipe(spine);
      pipe.SetTolerance(1.0e-6, 1.0e-6, 1.0e-2);
      pipe.SetMode(up);
      pipe.Add(wire.wire, false, false);
      detail::build(pipe);
      if (!pipe.IsDone()) {
        throw std::runtime_error("the curves could not be moved along the helix");
      }
      parts.add(pipe.Shape());
      parts.name(pipe, feature, wire);
    }
    return parts.shape("surface helix");
  });
}

} // namespace mitcad::geometry
