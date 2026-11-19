// SPDX-License-Identifier: MIT
#include "runout_dressup.hpp"

#include <algorithm>
#include <cmath>
#include <initializer_list>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_LocalOperation.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepLProp_CLProps.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <ChFi3d.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Map.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/naming.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kPi = 3.14159265358979323846;
// How far below the full section the run-out's taper starts at the shared
// vertex, as a share of the dressup's depth: the taper's face then crosses
// the dressup's own face a little way along the next edge, at an angle,
// instead of meeting it along the section through the vertex (where the
// tube's end lies too).
constexpr double kBelow = 0.05;
// The run-out reaches the edge this share of the next edge's length before
// its far vertex: its face then crosses the edge there instead of ending
// on the body's vertex within the loft's approximation (a sliver edge).
constexpr double kEarly = 0.02;
// Sections of a tube or taper along a curved edge (a straight edge takes
// its two ends).
constexpr int kSections = 9;
// The tube's radius: this much more than the dressup's reach from the
// edge, and below the edge's smallest radius of curvature (a tube wider
// than that folds over itself on the inside of the bend).
constexpr double kTubeMargin = 1.3;
constexpr double kTubeLeast = 1.05;

// The dressup's cross-section at a point of an edge, in the plane normal
// to the edge there.
struct Section {
  gp_Pnt p;      // the point of the edge
  gp_Dir t;      // the edge's direction, along the chain
  gp_Dir bis;    // between the faces, into the wedge the dressup fills
  gp_Pnt c1, c2; // the contact points on the edge's two faces
  bool arc = false;
  gp_Pnt centre; // a rounding's centre
  double radius = 0.0;
  gp_Dir toward;      // from the profile towards the edge
  double depth = 0.0; // from the profile to the edge
  double reach = 0.0; // the contact points' largest distance from the edge
};

gp_Vec unit(const gp_Vec& v) { return v.Normalized(); }

// Whether the material's angle across the edge is less than a half turn.
std::optional<bool> convex(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b) {
  const ChFiDS_TypeOfConcavity type = ChFi3d::DefineConnectType(edge, a, b, 1.0e-4, false);
  if (type != ChFiDS_Convex && type != ChFiDS_Concave) {
    return std::nullopt;
  }
  return type == ChFiDS_Convex;
}

// The edge's two faces in the body, the first first as the edge's name has
// them (Shape::edge_faces), else as the body maps them.
std::optional<std::pair<int, int>> two_faces(const Shape& body, int edge) {
  const std::vector<int> faces = faces_at_edge(body, edge);
  if (faces.size() != 2 || faces[0] == faces[1]) {
    return std::nullopt;
  }
  return std::make_pair(faces[0], faces[1]);
}

// The relative parameter (0 or 1) of a vertex on an edge.
double at_vertex(const TopoDS_Edge& edge, const TopoDS_Vertex& vertex) {
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(edge, first, last);
  return vertex.IsSame(first) ? 0.0 : 1.0;
}

// The section at relative parameter `rel` of the edge between faces a and
// b; `forward` when the chain runs along the edge's parameter, `measured`
// the face (0: a, 1: b, -1: none) a chamfer's first distance lies on.
std::optional<Section> section_at(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b,
                                  bool is_convex, const RunOutSize& size, int measured, double rel,
                                  bool forward) {
  const BRepAdaptor_Curve curve(edge);
  const double u = curve.FirstParameter() + rel * (curve.LastParameter() - curve.FirstParameter());
  gp_Pnt p;
  gp_Vec d1;
  curve.D1(u, p, d1);
  if (d1.Magnitude() < 1.0e-12) {
    return std::nullopt;
  }
  gp_Vec t = unit(d1);
  if (!forward) {
    t.Reverse();
  }
  const gp_Vec na(normal_on_edge(a, edge, rel));
  const gp_Vec nb(normal_on_edge(b, edge, rel));
  // Along each face away from the edge: below the other face's tangent
  // plane at a convex edge, above it at a concave one.
  const auto along = [&](const gp_Vec& own, const gp_Vec& other) -> std::optional<gp_Vec> {
    const gp_Vec cross = own.Crossed(t);
    if (cross.Magnitude() < 1.0e-9) {
      return std::nullopt;
    }
    gp_Vec e = unit(cross);
    const double side = e.Dot(other);
    if (std::abs(side) < 1.0e-6) {
      return std::nullopt; // the faces meet tangentially
    }
    if ((side < 0.0) != is_convex) {
      e.Reverse();
    }
    return e;
  };
  const std::optional<gp_Vec> ea = along(na, nb);
  const std::optional<gp_Vec> eb = along(nb, na);
  if (!ea || !eb) {
    return std::nullopt;
  }
  const double wedge = ea->Angle(*eb);
  if (!(wedge > 1.0e-3 && wedge < kPi - 1.0e-3)) {
    return std::nullopt;
  }
  Section s;
  s.p = p;
  s.t = gp_Dir(t);
  s.bis = gp_Dir(*ea + *eb);
  if (size.fillet) {
    const double r = size.radius > 0.0 ? size.radius : size.chord / (2.0 * std::cos(wedge / 2.0));
    if (!(r > 0.0)) {
      return std::nullopt;
    }
    const double setback = r / std::tan(wedge / 2.0);
    s.c1 = p.Translated(*ea * setback);
    s.c2 = p.Translated(*eb * setback);
    s.arc = true;
    s.radius = r;
    s.centre = p.Translated(gp_Vec(s.bis) * (r / std::sin(wedge / 2.0)));
    s.toward = gp_Dir(-gp_Vec(s.bis));
    s.depth = r / std::sin(wedge / 2.0) - r;
    s.reach = setback;
  } else {
    double d1a = size.distance;
    double d2b = size.distance2 > 0.0 ? size.distance2 : size.distance;
    if (size.angle > 0.0) {
      const double third = kPi - wedge - size.angle;
      if (!(third > 1.0e-6)) {
        return std::nullopt;
      }
      d2b = d1a * std::sin(size.angle) / std::sin(third);
    }
    if (measured == 1) {
      std::swap(d1a, d2b);
    }
    if (!(d1a > 0.0 && d2b > 0.0)) {
      return std::nullopt;
    }
    s.c1 = p.Translated(*ea * d1a);
    s.c2 = p.Translated(*eb * d2b);
    const gp_Vec line = unit(gp_Vec(s.c1, s.c2));
    const gp_Vec to_p(s.c1, p);
    const gp_Vec normal = to_p - line * to_p.Dot(line);
    if (normal.Magnitude() < 1.0e-9) {
      return std::nullopt;
    }
    s.toward = gp_Dir(normal);
    s.depth = normal.Magnitude();
    s.reach = std::max(d1a, d2b);
  }
  return s;
}

// The section moved along the edge's direction by `along` (beyond an end
// of the edge, straight on).
Section shifted(Section s, double along) {
  const gp_Vec by = gp_Vec(s.t) * along;
  s.p.Translate(by);
  s.c1.Translate(by);
  s.c2.Translate(by);
  s.centre.Translate(by);
  return s;
}

TopoDS_Edge segment(const gp_Pnt& a, const gp_Pnt& b) { return BRepBuilderAPI_MakeEdge(a, b).Edge(); }

// The taper's section: the dressup's profile moved `tau` towards the edge
// (negative: away from it), and the region on the edge's side of it, `wide`
// across beyond it. Its first edge is the profile.
std::optional<TopoDS_Wire> taper_wire(const Section& s, double tau, double wide) {
  const gp_Vec move = gp_Vec(s.toward) * tau;
  gp_Pnt e1;
  gp_Pnt e2;
  TopoDS_Edge profile;
  if (s.arc) {
    // The rounding's circle, its half towards the edge: past the contact
    // points to where it runs along `toward`, so that the region's sides
    // go on from it tangentially.
    const gp_Vec chord(s.c2, s.c1);
    const gp_Vec across = chord - gp_Vec(s.toward) * chord.Dot(gp_Vec(s.toward));
    if (across.Magnitude() < 1.0e-9) {
      return std::nullopt;
    }
    const gp_Vec q = unit(across) * s.radius;
    const gp_Pnt centre = s.centre.Translated(move);
    e1 = centre.Translated(q);
    e2 = centre.Translated(-q);
    const gp_Pnt middle = centre.Translated(gp_Vec(s.toward) * s.radius);
    GC_MakeArcOfCircle arc(e1, middle, e2);
    if (!arc.IsDone()) {
      return std::nullopt;
    }
    profile = BRepBuilderAPI_MakeEdge(arc.Value()).Edge();
  } else {
    // The bevel's line, a quarter of its width longer at each end.
    const gp_Vec line(s.c1, s.c2);
    const gp_Vec more = line * 0.25;
    e1 = s.c1.Translated(move - more);
    e2 = s.c2.Translated(move + more);
    profile = segment(e1, e2);
  }
  const gp_Vec out = gp_Vec(s.toward) * wide;
  const gp_Pnt f2 = e2.Translated(out);
  const gp_Pnt f1 = e1.Translated(out);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(profile);
  wire.Add(segment(e2, f2));
  wire.Add(segment(f2, f1));
  wire.Add(segment(f1, e1));
  if (!wire.IsDone()) {
    return std::nullopt;
  }
  return wire.Wire();
}

// The tube's section: a circle of `radius` about the edge, starting on the
// side away from the dressup (the same side along the edge).
TopoDS_Wire tube_wire(const Section& s, double radius) {
  const gp_Circ circle(gp_Ax2(s.p, s.t, gp_Dir(-gp_Vec(s.bis))), radius);
  return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circle).Edge()).Wire();
}

// A solid lofted through the wires, its faces named: the face of the first
// wire's first edge `first_edge`, the end faces `start` and `end`, the
// others `other`. Null when the loft fails.
ShapePtr lofted(const std::vector<TopoDS_Wire>& wires, const std::string& first_edge, const std::string& other,
                const std::string& start, const std::string& end) {
  if (wires.size() < 2) {
    return nullptr;
  }
  BRepOffsetAPI_ThruSections loft(true, false, 1.0e-6);
  loft.CheckCompatibility(false);
  for (const TopoDS_Wire& wire : wires) {
    loft.AddWire(wire);
  }
  build(loft);
  if (!loft.IsDone()) {
    return nullptr;
  }
  FaceNamer namer(loft.Shape());
  bool first = true;
  for (BRepTools_WireExplorer it(wires.front()); it.More(); it.Next()) {
    const TopoDS_Shape face = loft.GeneratedFace(it.Current());
    if (!face.IsNull() && namer.contains(face)) {
      namer.add(face, first ? first_edge : other);
    }
    first = false;
  }
  for (const auto& [cap, name] : {std::make_pair(loft.FirstShape(), start), std::make_pair(loft.LastShape(), end)}) {
    for (TopExp_Explorer it(cap, TopAbs_FACE); it.More(); it.Next()) {
      if (namer.contains(it.Current())) {
        namer.add(it.Current(), name);
      }
    }
  }
  // (A face the loft's history misses is the tool's too.)
  for (TopExp_Explorer it(loft.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    if (!namer.named(it.Current())) {
      namer.add(it.Current(), other);
    }
  }
  namer.finish();
  return namer.shape();
}

// The one solid of a boolean, or null.
ShapePtr single(const BooleanResult& made) {
  if (made.pieces.size() != 1 || !made.pieces.front().shape) {
    return nullptr;
  }
  return made.pieces.front().shape;
}

// The relative parameters of an edge's sections.
std::vector<double> section_parameters(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  const int n = curve.GetType() == GeomAbs_Line ? 2 : kSections;
  std::vector<double> rel;
  for (int i = 0; i < n; ++i) {
    rel.push_back(static_cast<double>(i) / static_cast<double>(n - 1));
  }
  return rel;
}

// One unselected edge of a chain: where it runs, and its sections.
struct Stretch {
  int edge = -1;
  TopoDS_Vertex start; // along the chain
  TopoDS_Vertex end;
  bool from_selected = false; // a selected edge ends at its start
  bool to_selected = false;   // ... at its end
  bool open_start = false;    // the chain's open end is its start
  bool open_end = false;
  std::vector<Section> sections; // from start to end
  std::vector<double> lengths;   // of each section from the start
  double length = 0.0;
  double curvature = 0.0; // the largest
};

// The chain's measured face (0 or 1 of each edge's two faces) along the
// chain, carried from the set's measured face at its first selected edge
// across each vertex to the next edge's face that continues it; -1 for
// equal distances and fillets.
std::vector<int> measured_sides(const Shape& body, const PartialChain& chain) {
  const std::size_t n = chain.edges.size();
  std::vector<int> side(n, -1);
  if (chain.size.fillet || chain.size.measured < 0) {
    return side;
  }
  std::size_t seed = n;
  for (std::size_t i = 0; i < n; ++i) {
    if (chain.selected[i] != 0) {
      seed = i;
      break;
    }
  }
  if (seed == n) {
    return side;
  }
  const auto faces = two_faces(body, chain.edges[seed]);
  if (!faces) {
    return side;
  }
  side[seed] = chain.size.measured == faces->second ? 1 : 0;
  // From edge i to its neighbour j across their common vertex: the face of
  // j whose normal there is nearest the measured face's of i.
  const auto carry = [&](std::size_t i, std::size_t j) {
    const TopoDS_Edge& ei = body.edge(chain.edges[i]);
    const TopoDS_Edge& ej = body.edge(chain.edges[j]);
    TopoDS_Vertex common;
    const auto fi = two_faces(body, chain.edges[i]);
    const auto fj = two_faces(body, chain.edges[j]);
    if (!fi || !fj || side[i] < 0 || !TopExp::CommonVertex(ei, ej, common)) {
      return;
    }
    const int own = side[i] == 0 ? fi->first : fi->second;
    const gp_Dir n = normal_on_edge(body.face(own), ei, at_vertex(ei, common));
    const gp_Dir na = normal_on_edge(body.face(fj->first), ej, at_vertex(ej, common));
    const gp_Dir nb = normal_on_edge(body.face(fj->second), ej, at_vertex(ej, common));
    side[j] = n.Dot(na) >= n.Dot(nb) ? 0 : 1;
  };
  for (std::size_t k = 1; k < n; ++k) {
    const std::size_t i = (seed + k - 1) % n;
    const std::size_t j = (seed + k) % n;
    if (!chain.closed && j < i) {
      break;
    }
    carry(i, j);
  }
  if (!chain.closed) {
    for (std::size_t i = seed; i > 0; --i) {
      carry(i, i - 1);
    }
  }
  return side;
}

// The unselected edges of the chain with their sections; nullopt where a
// section cannot be made.
std::optional<std::vector<Stretch>> stretches_of(const Shape& body, const PartialChain& chain) {
  const std::size_t n = chain.edges.size();
  const std::vector<int> measured = measured_sides(body, chain);
  std::vector<Stretch> stretches;
  for (std::size_t i = 0; i < n; ++i) {
    if (chain.selected[i] != 0) {
      continue;
    }
    const TopoDS_Edge& edge = body.edge(chain.edges[i]);
    const auto faces = two_faces(body, chain.edges[i]);
    if (!faces) {
      return std::nullopt;
    }
    const TopoDS_Face& a = body.face(faces->first);
    const TopoDS_Face& b = body.face(faces->second);
    const std::optional<bool> is_convex = convex(edge, a, b);
    if (!is_convex) {
      return std::nullopt;
    }
    Stretch stretch;
    stretch.edge = chain.edges[i];
    const bool has_prev = chain.closed || i > 0;
    const bool has_next = chain.closed || i + 1 < n;
    const std::size_t prev = (i + n - 1) % n;
    const std::size_t next = (i + 1) % n;
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(edge, first, last);
    // The chain's direction along the edge: from the vertex it shares with
    // the edge before it (or away from the one it shares with the next).
    bool forward = true;
    TopoDS_Vertex common;
    if (has_prev && n > 1 && TopExp::CommonVertex(edge, body.edge(chain.edges[prev]), common)) {
      forward = common.IsSame(first);
    } else if (has_next && n > 1 && TopExp::CommonVertex(edge, body.edge(chain.edges[next]), common)) {
      forward = !common.IsSame(first);
    }
    stretch.start = forward ? first : last;
    stretch.end = forward ? last : first;
    stretch.from_selected = has_prev && n > 1 && chain.selected[prev] != 0;
    stretch.to_selected = has_next && n > 1 && chain.selected[next] != 0;
    stretch.open_start = !has_prev;
    stretch.open_end = !has_next;
    const BRepAdaptor_Curve curve(edge);
    const double u0 = curve.FirstParameter();
    const double u1 = curve.LastParameter();
    stretch.length = GCPnts_AbscissaPoint::Length(curve, u0, u1);
    if (!(stretch.length > 1.0e-7)) {
      return std::nullopt;
    }
    std::vector<double> rel = section_parameters(edge);
    if (!forward) {
      std::reverse(rel.begin(), rel.end());
    }
    for (const double r : rel) {
      const std::optional<Section> s = section_at(edge, a, b, *is_convex, chain.size, measured[i], r, forward);
      if (!s) {
        return std::nullopt;
      }
      stretch.sections.push_back(*s);
      const double u = u0 + r * (u1 - u0);
      const double along = GCPnts_AbscissaPoint::Length(curve, u0, u);
      stretch.lengths.push_back(forward ? along : stretch.length - along);
      BRepLProp_CLProps props(curve, u, 2, 1.0e-9);
      if (curve.GetType() != GeomAbs_Line) {
        stretch.curvature = std::max(stretch.curvature, std::abs(props.Curvature()));
      }
    }
    stretches.push_back(std::move(stretch));
  }
  return stretches;
}

// The convexity of the chain's dressup: of its first selected edge.
std::optional<bool> chain_convex(const Shape& body, const PartialChain& chain) {
  for (std::size_t i = 0; i < chain.edges.size(); ++i) {
    if (chain.selected[i] == 0) {
      continue;
    }
    const auto faces = two_faces(body, chain.edges[i]);
    if (!faces) {
      return std::nullopt;
    }
    return convex(body.edge(chain.edges[i]), body.face(faces->first), body.face(faces->second));
  }
  return std::nullopt;
}

std::string vertex_or_edge(const Shape& body, const TopoDS_Vertex& vertex, int edge) {
  if (const std::optional<std::string> name = body.name_of_vertex(vertex); name && !name->empty()) {
    return *name;
  }
  return body.edge_name(edge);
}

// The region the result keeps the body's material in, for one convexity's
// chains: the tubes round their unselected edges, less the run-outs'
// tapers. Null when it cannot be built.
ShapePtr kept_region(const std::string& feature, const Shape& body, const std::vector<const PartialChain*>& chains,
                     bool run_out, const std::string& tool) {
  ShapePtr region;
  for (const PartialChain* chain : chains) {
    const std::optional<std::vector<Stretch>> stretches = stretches_of(body, *chain);
    if (!stretches || stretches->empty()) {
      return nullptr;
    }
    // One tube radius for the chain: past every section's reach, inside
    // every edge's bend.
    double reach = 0.0;
    double curvature = 0.0;
    double depth = 0.0;
    for (const Stretch& stretch : *stretches) {
      curvature = std::max(curvature, stretch.curvature);
      for (const Section& s : stretch.sections) {
        reach = std::max(reach, s.reach);
        depth = std::max(depth, s.depth);
      }
    }
    double radius = kTubeMargin * reach;
    if (curvature > 0.0) {
      radius = std::min(radius, 0.5 * (reach + 1.0 / curvature));
    }
    if (!(reach > 0.0) || radius < kTubeLeast * reach) {
      return nullptr;
    }
    // Past the ends of a taper: straight on by a quarter of the tube's
    // radius.
    const double beyond = 0.25 * radius;
    // Past an open chain's end the tube runs on by twice its radius: the
    // chain ends at a sharp corner, and the face beyond it (the dressup's
    // end) may lie at a slant to the edge's normal plane, so that a shorter
    // tube's end would cut into the dressup's material inside the body
    // there and leave a face of its own (up to 63 degrees of slant).
    const double open_beyond = 2.0 * radius;
    const double wide = 2.0 * (radius + depth);
    const auto add = [&](const ShapePtr& part, BooleanOp op) {
      if (!part) {
        return false;
      }
      if (!region) {
        if (op == BooleanOp::Cut) {
          return false;
        }
        region = part;
        return true;
      }
      const BooleanResult made = boolean(op, {region.get()}, *part);
      if (op == BooleanOp::Cut && !made.touched.empty() && !made.touched.front()) {
        return true; // (the taper misses the tube: nothing to take out)
      }
      region = single(made);
      return region != nullptr;
    };
    for (const Stretch& stretch : *stretches) {
      std::vector<TopoDS_Wire> wires;
      if (stretch.open_start) {
        wires.push_back(tube_wire(shifted(stretch.sections.front(), -open_beyond), radius));
      }
      for (const Section& s : stretch.sections) {
        wires.push_back(tube_wire(s, radius));
      }
      if (stretch.open_end) {
        wires.push_back(tube_wire(shifted(stretch.sections.back(), open_beyond), radius));
      }
      // The ends at selected edges: the flat end without a run-out.
      const auto end_name = [&](bool at_selected, const TopoDS_Vertex& vertex) {
        return at_selected && !run_out ? face_name(feature, "runout_end", vertex_or_edge(body, vertex, stretch.edge))
                                       : tool;
      };
      const ShapePtr tube = lofted(wires, tool, tool, end_name(stretch.from_selected, stretch.start),
                                   end_name(stretch.to_selected, stretch.end));
      if (!add(tube, BooleanOp::Join)) {
        return nullptr;
      }
    }
    if (!run_out) {
      continue;
    }
    for (const Stretch& stretch : *stretches) {
      for (const bool from_start : {true, false}) {
        if (from_start ? !stretch.from_selected : !stretch.to_selected) {
          continue;
        }
        // A run-out over an edge shorter than the dressup is wide: the flat
        // end instead.
        const Section& at_vertex_section = from_start ? stretch.sections.front() : stretch.sections.back();
        if (stretch.length < at_vertex_section.c1.Distance(at_vertex_section.c2)) {
          return nullptr;
        }
        // Sections from the selected edge's vertex on: the profile moved
        // towards the edge linearly with the length, from kBelow of the
        // depth below the full section there to the edge just before the
        // far vertex (kEarly).
        std::vector<Section> sections = stretch.sections;
        std::vector<double> lengths = stretch.lengths;
        if (!from_start) {
          std::reverse(sections.begin(), sections.end());
          std::reverse(lengths.begin(), lengths.end());
          for (Section& s : sections) {
            s.t.Reverse();
          }
          for (double& l : lengths) {
            l = stretch.length - l;
          }
        }
        const auto tau = [&](const Section& s, double along) {
          return s.depth * (-kBelow + (1.0 + kBelow) * along / ((1.0 - kEarly) * stretch.length));
        };
        std::vector<TopoDS_Wire> wires;
        const auto push = [&](const Section& s, double along) {
          const std::optional<TopoDS_Wire> wire = taper_wire(s, tau(s, along), wide);
          if (wire) {
            wires.push_back(*wire);
          }
          return wire.has_value();
        };
        bool made = push(shifted(sections.front(), -beyond), -beyond);
        for (std::size_t k = 0; made && k < sections.size(); ++k) {
          made = push(sections[k], lengths[k]);
        }
        made = made && push(shifted(sections.back(), beyond), stretch.length + beyond);
        if (!made) {
          return nullptr;
        }
        const ShapePtr taper = lofted(wires, face_name(feature, "runout", body.edge_name(stretch.edge)), tool, tool, tool);
        if (!add(taper, BooleanOp::Cut)) {
          return nullptr;
        }
      }
    }
  }
  return region;
}

bool holds_role(const Shape& shape, const std::string& feature, const std::string& role) {
  const std::string prefix = feature + ":" + role;
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& name : shape.face_names(f)) {
      if (name.compare(0, prefix.size(), prefix) == 0 &&
          (name.size() == prefix.size() || name[prefix.size()] == '(' || name[prefix.size()] == '#')) {
        return true;
      }
    }
  }
  return false;
}

} // namespace

std::vector<PartialChain> partial_chains(const Shape& body, const std::vector<int>& owner,
                                         const std::vector<char>& chain_off, bool chamfer) {
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] >= 0) {
      selected.Add(body.edge(edge));
    }
  }
  const auto collect = [&](BRepFilletAPI_LocalOperation& maker) {
    std::vector<PartialChain> chains;
    for (int ic = 1; ic <= maker.NbContours(); ++ic) {
      PartialChain chain;
      bool mixed = false;
      bool unselected = false;
      for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
        const int edge = body.edge_index(maker.Edge(ic, ie));
        if (edge < 0) {
          mixed = true;
          break;
        }
        const int own = owner[static_cast<std::size_t>(edge)];
        if (own < 0) {
          unselected = true;
        } else if (chain.set < 0) {
          chain.set = own;
        } else if (own != chain.set) {
          mixed = true;
        }
        chain.edges.push_back(edge);
        chain.selected.push_back(own >= 0 ? 1 : 0);
      }
      if (mixed || !unselected || chain.set < 0 || chain_off[static_cast<std::size_t>(chain.set)] == 0) {
        continue;
      }
      chain.closed = maker.Closed(ic);
      chains.push_back(std::move(chain));
    }
    return chains;
  };
  // As the dressups set their makers up (round_edges, bevel): the chains
  // stop where their edges turn unless the next edge is selected too.
  if (chamfer) {
    BRepFilletAPI_MakeChamfer maker(body.occt());
    maker.StopChainsAtTurns(selected);
    for (int edge = 0; edge < body.edge_count(); ++edge) {
      if (owner[static_cast<std::size_t>(edge)] >= 0 && maker.Contour(body.edge(edge)) == 0) {
        maker.Add(1.0, body.edge(edge));
      }
    }
    return collect(maker);
  }
  BRepFilletAPI_MakeFillet maker(body.occt());
  maker.StopChainsAtTurns(selected);
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] >= 0 && maker.Contour(body.edge(edge)) == 0) {
      maker.Add(1.0, body.edge(edge));
    }
  }
  return collect(maker);
}

ShapePtr over_selected_edges(const std::string& feature, const Shape& body, const Shape& full,
                             const std::vector<PartialChain>& chains, bool run_out) {
  const std::string tool = face_name(feature, "runout_tool");
  std::vector<const PartialChain*> convex_chains;
  std::vector<const PartialChain*> concave_chains;
  for (const PartialChain& chain : chains) {
    const std::optional<bool> is_convex = chain_convex(body, chain);
    if (!is_convex) {
      return nullptr;
    }
    (*is_convex ? convex_chains : concave_chains).push_back(&chain);
  }
  ShapePtr result = std::make_shared<Shape>(full);
  // Convex: the body's material inside the region put back into the
  // dressup's result.
  if (!convex_chains.empty()) {
    const ShapePtr region = kept_region(feature, body, convex_chains, run_out, tool);
    if (!region) {
      return nullptr;
    }
    const BooleanResult kept = boolean(BooleanOp::Intersect, {&body}, *region);
    if (kept.pieces.empty()) {
      return nullptr;
    }
    for (const BooleanPiece& piece : kept.pieces) {
      result = single(boolean(BooleanOp::Join, {result.get()}, *piece.shape));
      if (!result) {
        return nullptr;
      }
    }
  }
  // Concave: the dressup's material inside the region outside the body
  // taken away again.
  if (!concave_chains.empty()) {
    const ShapePtr region = kept_region(feature, body, concave_chains, run_out, tool);
    if (!region) {
      return nullptr;
    }
    const BooleanResult outside = boolean(BooleanOp::Cut, {region.get()}, body);
    for (const BooleanPiece& piece : outside.pieces) {
      const BooleanResult made = boolean(BooleanOp::Cut, {result.get()}, *piece.shape);
      if (!made.touched.empty() && !made.touched.front()) {
        continue;
      }
      result = single(made);
      if (!result) {
        return nullptr;
      }
    }
  }
  // The tools' own faces lie past the dressup: none is left. With the
  // run-out the flat end's face is inside the taper too.
  if (holds_role(*result, feature, "runout_tool") || (run_out && holds_role(*result, feature, "runout_end"))) {
    return nullptr;
  }
  if (!is_valid(result->occt())) {
    return nullptr;
  }
  result->copy_notes(full);
  return result;
}

} // namespace mitcad::geometry::detail
