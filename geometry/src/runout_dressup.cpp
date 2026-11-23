// SPDX-License-Identifier: MIT
#include "runout_dressup.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <initializer_list>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_LocalOperation.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLProp_CLProps.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ChFi3d.hxx>
#include <ElCLib.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Curve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gce_MakeCirc.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/naming.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {

void runout_trace(const std::string& what) {
  static const bool on = [] {
#ifdef _MSC_VER
    char* value = nullptr;
    std::size_t length = 0;
    if (_dupenv_s(&value, &length, "MITCAD_DEBUG_RUNOUT") != 0 || value == nullptr) {
      return false;
    }
    std::free(value);
    return true;
#else
    return std::getenv("MITCAD_DEBUG_RUNOUT") != nullptr;
#endif
  }();
  if (on) {
    std::fprintf(stderr, "runout: %s\n", what.c_str());
  }
}

namespace {

// Null, said on the trace.
ShapePtr failed(const std::string& why) {
  runout_trace(why);
  return nullptr;
}

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
// A sector round an arc too tight for a tube reaches this much more than the
// dressup's reach past the arc and on both sides of its plane: round such an
// arc (as tight as the dressup is deep) the dressup's own contacts may lie
// on curved faces past the plane faces' reach, and the sector cannot fold,
// so it is made wide.
constexpr double kSectorMargin = 2.0;

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
  int prev_edge = -1;     // the chain's edge before it (the body's), -1 for none
  int next_edge = -1;     // ... after it
  bool circle = false;    // the edge is an arc of a circle
  gp_Circ arc;            // that circle
  double u0 = 0.0;        // the edge's parameters on it (radians)
  double u1 = 0.0;
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

// A curve that lies on a circle, and its parameters on that circle
// (radians, u0 < u1, the circle running the curve's way).
struct CircleFit {
  gp_Circ circle;
  double u0 = 0.0;
  double u1 = 0.0;
};

// The circle a curve the kernel gives in another form lies on (a B-spline
// where a rounding ends on a plane, as the arc of a corner rounded before
// along the face it ends on; mitcad#2193): the circle through its ends and
// its middle, where seven more of its points lie on it within 1e-4 of its
// radius (1e-6 mm at least). None for other curves.
std::optional<CircleFit> circle_of(const BRepAdaptor_Curve& curve) {
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  const gp_Pnt a = curve.Value(first);
  const gp_Pnt m = curve.Value(0.5 * (first + last));
  const gp_Pnt b = curve.Value(last);
  const gce_MakeCirc made(a, m, b);
  if (!made.IsDone()) {
    return std::nullopt;
  }
  gp_Circ circle = made.Value();
  const double tolerance = std::max(1.0e-6, 1.0e-4 * circle.Radius());
  const gp_Vec axis(circle.Axis().Direction());
  for (int i = 1; i < 8; ++i) {
    const gp_Pnt p = curve.Value(first + (last - first) * i / 8.0);
    const gp_Vec from_centre(circle.Location(), p);
    if (std::abs(from_centre.Magnitude() - circle.Radius()) > tolerance ||
        std::abs(from_centre.Dot(axis)) > tolerance) {
      return std::nullopt;
    }
  }
  // The parameters of the ends, the middle between them; else the circle
  // runs the other way.
  for (int turn = 0; turn < 2; ++turn) {
    const double u0 = ElCLib::Parameter(circle, a);
    double um = ElCLib::Parameter(circle, m);
    double u1 = ElCLib::Parameter(circle, b);
    while (um < u0) {
      um += 2.0 * kPi;
    }
    while (u1 < u0) {
      u1 += 2.0 * kPi;
    }
    if (um < u1) {
      return CircleFit{circle, u0, u1};
    }
    circle = gp_Circ(gp_Ax2(circle.Location(), circle.Axis().Direction().Reversed(), circle.XAxis().Direction()),
                     circle.Radius());
  }
  return std::nullopt;
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
    const std::string& named = body.edge_name(chain.edges[i]);
    const auto faces = two_faces(body, chain.edges[i]);
    if (!faces) {
      runout_trace("unselected " + named + ": not between two faces");
      return std::nullopt;
    }
    const TopoDS_Face& a = body.face(faces->first);
    const TopoDS_Face& b = body.face(faces->second);
    const std::optional<bool> is_convex = convex(edge, a, b);
    if (!is_convex) {
      runout_trace("unselected " + named + ": neither convex nor concave");
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
    stretch.prev_edge = has_prev && n > 1 ? chain.edges[prev] : -1;
    stretch.next_edge = has_next && n > 1 ? chain.edges[next] : -1;
    const BRepAdaptor_Curve curve(edge);
    const double u0 = curve.FirstParameter();
    const double u1 = curve.LastParameter();
    if (curve.GetType() == GeomAbs_Circle) {
      stretch.circle = true;
      stretch.arc = curve.Circle();
      stretch.u0 = u0;
      stretch.u1 = u1;
    } else if (curve.GetType() != GeomAbs_Line) {
      if (const std::optional<CircleFit> fit = circle_of(curve)) {
        stretch.circle = true;
        stretch.arc = fit->circle;
        stretch.u0 = fit->u0;
        stretch.u1 = fit->u1;
      }
    }
    runout_trace("unselected " + named + ": curve type " + std::to_string(static_cast<int>(curve.GetType())) +
                 (stretch.circle ? ", an arc of radius " + std::to_string(stretch.arc.Radius()) : std::string()));
    stretch.length = GCPnts_AbscissaPoint::Length(curve, u0, u1);
    if (!(stretch.length > 1.0e-7)) {
      runout_trace("unselected " + named + ": too short");
      return std::nullopt;
    }
    std::vector<double> rel = section_parameters(edge);
    if (!forward) {
      std::reverse(rel.begin(), rel.end());
    }
    for (const double r : rel) {
      const std::optional<Section> s = section_at(edge, a, b, *is_convex, chain.size, measured[i], r, forward);
      if (!s) {
        runout_trace("unselected " + named + ": no section at " + std::to_string(r));
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

// The point of a circle at parameter u (radians).
gp_Pnt on_circle(const gp_Circ& circle, double u) {
  return circle.Location().Translated(gp_Vec(circle.XAxis().Direction()) * (circle.Radius() * std::cos(u)) +
                                      gp_Vec(circle.YAxis().Direction()) * (circle.Radius() * std::sin(u)));
}

// The region round an arc too tight for a tube (mitcad#2193): the sector of
// a cylinder about the arc's axis between the planes normal to the arc at
// its two vertices (the planes through the axis), out to `margin` past the
// arc and as far on both sides of its plane. It reaches the axis, so that
// it cannot fold over itself on the inside of the bend as a tube would.
// Its faces in those planes are named `start` and `end` (at the stretch's
// start and end vertex), the others `other`. Null when it cannot be made.
ShapePtr sector(const Stretch& stretch, double margin, const std::string& start, const std::string& end,
                const std::string& other) {
  const gp_Circ& arc = stretch.arc;
  const double angle = stretch.u1 - stretch.u0;
  if (!stretch.circle || !(angle > 1.0e-6 && angle < 2.0 * kPi - 1.0e-6) || !(margin > 0.0)) {
    return failed("sector: an arc of " + std::to_string(angle) + " rad");
  }
  const gp_Vec axis(arc.Axis().Direction());
  const gp_Pnt at_u0 = on_circle(arc, stretch.u0);
  const gp_Pnt at_u1 = on_circle(arc, stretch.u1);
  const gp_Vec from(arc.Location(), at_u0);
  const gp_Vec to(arc.Location(), at_u1);
  BRepPrimAPI_MakeCylinder maker(gp_Ax2(arc.Location().Translated(-axis * margin), gp_Dir(axis), gp_Dir(from)),
                                 arc.Radius() + margin, 2.0 * margin, angle);
  build(maker);
  if (!maker.IsDone()) {
    return failed("sector: the cylinder's sector fails");
  }
  // The stretch's start vertex lies at u0 or at u1.
  const gp_Pnt first = BRep_Tool::Pnt(stretch.start);
  const bool start_at_u0 = first.Distance(at_u0) <= first.Distance(at_u1);
  FaceNamer namer(maker.Shape());
  for (TopExp_Explorer it(maker.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    std::string name = other;
    const BRepAdaptor_Surface surface(face, false);
    if (surface.GetType() == GeomAbs_Plane &&
        std::abs(gp_Vec(surface.Plane().Axis().Direction()).Dot(axis)) < 1.0e-6) {
      // A face through the axis: the one on the side of u0 or of u1.
      GProp_GProps props;
      BRepGProp::SurfaceProperties(face, props);
      gp_Vec radial(arc.Location(), props.CentreOfMass());
      radial -= axis * radial.Dot(axis);
      const bool at_first = radial.Dot(from) >= radial.Dot(to);
      name = at_first == start_at_u0 ? start : end;
    }
    if (namer.contains(face)) {
      namer.add(face, name);
    }
  }
  namer.finish();
  return namer.shape();
}

// A vertex where a selected edge meets an unselected one, for the face end:
// the name of the kept region's face there, in the plane normal to the edge.
struct Transition {
  int selected = -1; // the body's selected edge
  TopoDS_Vertex vertex;
  std::string cap;
  std::size_t run = 0; // the kept region it bounds (kept_region's list)
};

// No regions, said on the trace.
std::vector<ShapePtr> none(const std::string& why) {
  runout_trace(why);
  return {};
}

// The space on a selected edge's side of the plane normal to the chain at
// the vertex between it and an unselected edge (point `p`, `toward` the
// selected edge), near that vertex: a box `size` deep and twice `size`
// across, its face in that plane named `name`, the others `other`. Null
// when it cannot be made.
ShapePtr selected_side(const gp_Pnt& p, const gp_Dir& toward, double size, const std::string& name,
                       const std::string& other) {
  const gp_Ax2 frame(p, toward);
  const gp_Pnt corner =
      p.Translated(gp_Vec(frame.XDirection()) * -size + gp_Vec(frame.YDirection()) * -size);
  BRepPrimAPI_MakeBox maker(gp_Ax2(corner, toward, frame.XDirection()), 2.0 * size, 2.0 * size, size);
  build(maker);
  if (!maker.IsDone()) {
    return nullptr;
  }
  const TopoDS_Face& in_plane = maker.BottomFace();
  FaceNamer namer(maker.Shape());
  for (TopExp_Explorer it(maker.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    if (namer.contains(it.Current())) {
      namer.add(it.Current(), it.Current().IsSame(in_plane) ? name : other);
    }
  }
  namer.finish();
  return namer.shape();
}

// The regions the result keeps the body's material in, for one
// convexity's chains: the tubes round their unselected edges (sectors
// round arcs too tight for one), less the run-outs' tapers; one region for
// each run of unselected edges that follow each other along a chain (an
// open chain whose selected edge runs on at both its ends into rounded
// corners has a run beyond each end, apart from each other; mitcad#2193).
// With the face end, their faces at the selected edges are named after
// `transitions`, which it lists. No region reaches back across the plane
// normal to the chain at a selected edge's vertex near that vertex (the
// tube round the edge after a corner as tight as the dressup is deep would,
// and put back the body the selected edge's own dressup takes away beside
// it). Empty when they cannot be built.
std::vector<ShapePtr> kept_region(const std::string& feature, const Shape& body,
                                  const std::vector<const PartialChain*>& chains, ChainEnd ending,
                                  const std::string& tool, std::vector<Transition>& transitions) {
  const bool run_out = ending == ChainEnd::RunOut;
  std::vector<ShapePtr> regions; // the runs done
  ShapePtr region;               // the run being built
  // Each selected edge's side of a vertex where it meets a run, cleared of
  // that run's region afterwards.
  struct Side {
    gp_Pnt p;
    gp_Dir toward;
    double size = 0.0;
    std::string name;
    std::size_t run = 0;
  };
  std::vector<Side> sides;
  for (const PartialChain* chain : chains) {
    if (region) {
      regions.push_back(region);
      region = nullptr;
    }
    const std::optional<std::vector<Stretch>> stretches = stretches_of(body, *chain);
    if (!stretches || stretches->empty()) {
      return none("kept region: no sections of the unselected edges");
    }
    // One tube radius for the chain: past every section's reach, inside
    // every edge's bend but those of the arcs too tight for a tube.
    double reach = 0.0;
    double depth = 0.0;
    for (const Stretch& stretch : *stretches) {
      for (const Section& s : stretch.sections) {
        reach = std::max(reach, s.reach);
        depth = std::max(depth, s.depth);
      }
    }
    if (!(reach > 0.0)) {
      return none("kept region: no reach");
    }
    // An arc whose radius leaves no tube radius between kTubeLeast times
    // the reach and the middle of the reach and the arc's radius (the arc
    // as tight as the dressup is deep, a rounding running into a corner
    // rounded with its own radius, mitcad#2193) takes a sector instead;
    // not under a run-out, whose taper folds over itself there the same
    // way.
    const auto tight = [&](const Stretch& stretch) {
      return stretch.circle && 0.5 * (reach + stretch.arc.Radius()) < kTubeLeast * reach;
    };
    double curvature = 0.0;
    for (const Stretch& stretch : *stretches) {
      if (!tight(stretch)) {
        curvature = std::max(curvature, stretch.curvature);
      } else if (run_out) {
        return none("kept region: no run-out over the tight arc " + body.edge_name(stretch.edge));
      }
    }
    double radius = kTubeMargin * reach;
    if (curvature > 0.0) {
      radius = std::min(radius, 0.5 * (reach + 1.0 / curvature));
    }
    if (radius < kTubeLeast * reach) {
      return none("kept region: no tube radius (reach " + std::to_string(reach) + ", curvature " +
                    std::to_string(curvature) + ")");
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
    // A tube or sector joined to the run being built.
    const auto add = [&](const ShapePtr& part) {
      if (!part) {
        return false;
      }
      if (!region) {
        region = part;
        return true;
      }
      region = single(boolean(BooleanOp::Join, {region.get()}, *part));
      return region != nullptr;
    };
    // Each stretch's run: a new one after a selected edge.
    std::vector<std::size_t> run_of;
    for (const Stretch& stretch : *stretches) {
      if (region && stretch.from_selected) {
        regions.push_back(region);
        region = nullptr;
      }
      run_of.push_back(regions.size());
      // The ends at selected edges: the flat end's face, or the face end's
      // cap (one name each); under a run-out the tool's.
      const auto end_name = [&](bool at_selected, const TopoDS_Vertex& vertex, int selected) {
        if (!at_selected || run_out) {
          return tool;
        }
        if (ending == ChainEnd::Flat) {
          return face_name(feature, "runout_end", vertex_or_edge(body, vertex, stretch.edge));
        }
        Transition transition;
        transition.selected = selected;
        transition.vertex = vertex;
        transition.cap = face_name(feature, "runout_cap", std::to_string(transitions.size()));
        transition.run = regions.size();
        transitions.push_back(transition);
        return transition.cap;
      };
      const std::string at_start = end_name(stretch.from_selected, stretch.start, stretch.prev_edge);
      const std::string at_end = end_name(stretch.to_selected, stretch.end, stretch.next_edge);
      // (Past the farthest any tube or sector of the run reaches.)
      const double side_size = (kSectorMargin + 1.0) * reach;
      if (stretch.from_selected) {
        const Section& s = stretch.sections.front();
        sides.push_back({s.p, gp_Dir(-gp_Vec(s.t)), side_size, at_start, regions.size()});
      }
      if (stretch.to_selected) {
        const Section& s = stretch.sections.back();
        sides.push_back({s.p, s.t, side_size, at_end, regions.size()});
      }
      if (tight(stretch)) {
        if (!add(sector(stretch, kSectorMargin * reach, at_start, at_end, tool))) {
          return none("kept region: the sector round " + body.edge_name(stretch.edge) + " does not join");
        }
        // At an open chain's end, a straight tube on past it.
        if (stretch.open_start && !add(lofted({tube_wire(shifted(stretch.sections.front(), -open_beyond), radius),
                                               tube_wire(stretch.sections.front(), radius)},
                                              tool, tool, tool, tool))) {
          return none("kept region: the open start past " + body.edge_name(stretch.edge) + " does not join");
        }
        if (stretch.open_end && !add(lofted({tube_wire(stretch.sections.back(), radius),
                                             tube_wire(shifted(stretch.sections.back(), open_beyond), radius)},
                                            tool, tool, tool, tool))) {
          return none("kept region: the open end past " + body.edge_name(stretch.edge) + " does not join");
        }
        continue;
      }
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
      const ShapePtr tube = lofted(wires, tool, tool, at_start, at_end);
      if (!add(tube)) {
        return none(std::string("kept region: the tube round ") + body.edge_name(stretch.edge) +
                      (tube ? " does not join" : " does not loft"));
      }
    }
    if (region) {
      regions.push_back(region);
      region = nullptr;
    }
    // The selected edges' sides of their vertices cleared of the runs.
    for (const Side& side : sides) {
      if (side.run >= regions.size()) {
        continue;
      }
      const ShapePtr box = selected_side(side.p, side.toward, side.size, side.name, tool);
      if (!box) {
        return none("kept region: no box on the selected edge's side of a vertex");
      }
      const BooleanResult cleared = boolean(BooleanOp::Cut, {regions[side.run].get()}, *box);
      if (!cleared.touched.empty() && !cleared.touched.front()) {
        continue;
      }
      const ShapePtr kept = single(cleared);
      if (!kept) {
        return none("kept region: clearing the selected edge's side of a vertex leaves more than one solid");
      }
      regions[side.run] = kept;
    }
    sides.clear();
    if (!run_out) {
      continue;
    }
    for (std::size_t k = 0; k < stretches->size(); ++k) {
      const Stretch& stretch = (*stretches)[k];
      ShapePtr& run = regions[run_of[k]];
      for (const bool from_start : {true, false}) {
        if (from_start ? !stretch.from_selected : !stretch.to_selected) {
          continue;
        }
        // A run-out over an edge shorter than the dressup is wide: the flat
        // end instead.
        const Section& at_vertex_section = from_start ? stretch.sections.front() : stretch.sections.back();
        if (stretch.length < at_vertex_section.c1.Distance(at_vertex_section.c2)) {
          return none("run-out: " + body.edge_name(stretch.edge) + " is shorter than the dressup is wide");
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
        for (std::size_t s = 0; made && s < sections.size(); ++s) {
          made = push(sections[s], lengths[s]);
        }
        made = made && push(shifted(sections.back(), beyond), stretch.length + beyond);
        if (!made) {
          return none("run-out: no taper sections over " + body.edge_name(stretch.edge));
        }
        const ShapePtr taper = lofted(wires, face_name(feature, "runout", body.edge_name(stretch.edge)), tool, tool, tool);
        if (!taper) {
          return none("run-out: the taper over " + body.edge_name(stretch.edge) + " does not loft");
        }
        const BooleanResult cut = boolean(BooleanOp::Cut, {run.get()}, *taper);
        if (!cut.touched.empty() && !cut.touched.front()) {
          continue; // (the taper misses the tube: nothing to take out)
        }
        run = single(cut);
        if (!run) {
          return none("run-out: the taper over " + body.edge_name(stretch.edge) + " does not cut");
        }
      }
    }
  }
  return regions;
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

using EdgeFaces = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// True when one of the names is a face of the feature's own dressup: the
// feature's, and none of the run-out's tools or ends.
bool dressup_face(const NameList& names, const std::string& feature) {
  const std::string prefix = feature + ":";
  return std::any_of(names.begin(), names.end(), [&](const std::string& name) {
    return name.compare(0, prefix.size(), prefix) == 0 && name.compare(prefix.size(), 6, "runout") != 0;
  });
}

// The middle of an edge's curve.
gp_Pnt middle_of_edge(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
}

// The face end at a transition's cap (its pieces, faces `caps` of `flat`,
// the result with flat ends; mitcad#2193): the dressup's cross-section in
// the plane normal to the edge at the vertex, exactly as the kernel built
// it there (the cap's edges on the dressup's faces), closed round the edge
// outside the body, and swept straight on past the vertex along the edge's
// direction by `length`. The cross-section's ends go on out of the
// material at right angles to the faces they lie on, so that the prism
// crosses those faces there instead of touching them. Its face swept from
// the cross-section is <feature>:face_end(<selected edge>), the others
// `tool`. Null when it cannot be made (a concave edge, a cross-section that
// is not one open curve between the edge's two faces).
ShapePtr face_end_prism(const std::string& feature, const Shape& body, const Shape& flat,
                        const std::vector<int>& caps, const Transition& transition, const EdgeFaces& faces_of,
                        double length, const std::string& tool) {
  if (caps.empty()) {
    return failed("face end: no cap");
  }
  const TopoDS_Face& face = flat.face(caps.front());
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Plane) {
    return failed("face end: the cap " + transition.cap + " is not planar");
  }
  const gp_Pln plane = surface.Plane();
  const gp_Vec normal(plane.Axis().Direction());
  // The selected edge's faces at the vertex: their directions away from
  // the edge in the plane normal to it (a section of any size gives them).
  const TopoDS_Edge& edge = body.edge(transition.selected);
  const auto faces = two_faces(body, transition.selected);
  if (!faces) {
    return failed("face end: the selected edge is not between two faces");
  }
  const TopoDS_Face& a = body.face(faces->first);
  const TopoDS_Face& b = body.face(faces->second);
  const std::optional<bool> is_convex = convex(edge, a, b);
  if (!is_convex || !*is_convex) {
    return failed("face end: the selected edge is not convex");
  }
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(edge, first, last);
  if (first.IsSame(last)) {
    return failed("face end: the selected edge is closed");
  }
  RunOutSize one;
  one.fillet = true;
  one.radius = 1.0;
  const std::optional<Section> s = section_at(edge, a, b, true, one, -1, at_vertex(edge, transition.vertex),
                                              transition.vertex.IsSame(last));
  if (!s) {
    return failed("face end: no section of the selected edge at the vertex");
  }
  // Along the edge on past the vertex: the cap's normal, that way.
  gp_Vec along = normal;
  if (along.Dot(gp_Vec(s->t)) < 0.0) {
    along.Reverse();
  }
  if (along.Dot(gp_Vec(s->t)) < 0.99) {
    return failed("face end: the cap does not lie in the plane normal to the edge");
  }
  const gp_Vec ea = unit(gp_Vec(s->p, s->c1));
  const gp_Vec eb = unit(gp_Vec(s->p, s->c2));
  // The cross-section: the edges of the cap's pieces on the dressup's
  // faces (the pieces may meet along the axis of a corner as tight as the
  // dressup is deep, the region kept round it in two parts there), copied
  // together so that they keep their common vertices.
  TopoDS_Compound bundle;
  BRep_Builder builder;
  builder.MakeCompound(bundle);
  int crossing = 0;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> taken;
  for (const int cap : caps) {
    for (TopExp_Explorer it(flat.face(cap), TopAbs_EDGE); it.More(); it.Next()) {
      const int index = faces_of.FindIndex(it.Current());
      if (index == 0 || taken.Contains(it.Current())) {
        continue;
      }
      bool on_dressup = false;
      for (const TopoDS_Shape& next : faces_of(index)) {
        const int f = flat.face_index(next);
        on_dressup = on_dressup || (f >= 0 && dressup_face(flat.face_names(f), feature));
      }
      if (on_dressup) {
        taken.Add(it.Current());
        builder.Add(bundle, it.Current());
        ++crossing;
      }
    }
  }
  if (crossing == 0) {
    std::string around;
    for (const int cap : caps) {
      for (TopExp_Explorer it(flat.face(cap), TopAbs_EDGE); it.More(); it.Next()) {
        const int index = faces_of.FindIndex(it.Current());
        for (const TopoDS_Shape& next : index == 0 ? NCollection_List<TopoDS_Shape>() : faces_of(index)) {
          const int f = flat.face_index(next);
          if (f >= 0 && !flat.face_names(f).empty()) {
            around += " " + flat.face_names(f).front();
          }
        }
      }
    }
    return failed("face end: no edge of the cap on the dressup's faces; the cap's neighbours:" + around);
  }
  BRepBuilderAPI_Copy copy(bundle);
  const TopoDS_Shape cross = copy.Shape();
  // Its two ends: the vertices of one edge each.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ends;
  TopExp::MapShapesAndAncestors(cross, TopAbs_VERTEX, TopAbs_EDGE, ends);
  std::vector<TopoDS_Vertex> loose;
  for (int i = 1; i <= ends.Extent(); ++i) {
    if (ends(i).Extent() == 1) {
      loose.push_back(TopoDS::Vertex(ends.FindKey(i)));
    }
  }
  if (loose.size() != 2) {
    return failed("face end: the cross-section has " + std::to_string(loose.size()) + " ends (" +
                  std::to_string(crossing) + " edges)");
  }
  const gp_Pnt head = BRep_Tool::Pnt(loose[0]);
  const gp_Pnt tail = BRep_Tool::Pnt(loose[1]);
  // Which end lies on which face: the one nearer to that face's line.
  const auto off_line = [&](const gp_Pnt& q, const gp_Vec& on) {
    const gp_Vec v(s->p, q);
    return (v - on * v.Dot(on)).Magnitude();
  };
  const bool head_on_a = off_line(head, ea) < off_line(head, eb);
  const bool tail_on_a = off_line(tail, ea) < off_line(tail, eb);
  if (head_on_a == tail_on_a) {
    return failed("face end: both ends of the cross-section lie on one face's side");
  }
  // Out of the material at right angles to each face's line, and round
  // the edge outside the body.
  const gp_Vec out_a = unit(ea * ea.Dot(eb) - eb);
  const gp_Vec out_b = unit(eb * ea.Dot(eb) - ea);
  const double reach = std::max(head.Distance(s->p), tail.Distance(s->p));
  if (!(reach > 1.0e-6)) {
    return failed("face end: the cross-section has no size");
  }
  const auto in_plane = [&](const gp_Pnt& q) {
    return q.Translated(-normal * gp_Vec(plane.Location(), q).Dot(normal));
  };
  const gp_Pnt head_out = in_plane(head.Translated((head_on_a ? out_a : out_b) * reach));
  const gp_Pnt tail_out = in_plane(tail.Translated((tail_on_a ? out_a : out_b) * reach));
  const gp_Pnt far = in_plane(s->p.Translated(-unit(ea + eb) * (3.0 * reach)));
  NCollection_List<TopoDS_Shape> curves;
  for (TopExp_Explorer it(cross, TopAbs_EDGE); it.More(); it.Next()) {
    curves.Append(it.Current());
  }
  const TopoDS_Vertex at_head_out = BRepBuilderAPI_MakeVertex(head_out).Vertex();
  const TopoDS_Vertex at_tail_out = BRepBuilderAPI_MakeVertex(tail_out).Vertex();
  const TopoDS_Vertex at_far = BRepBuilderAPI_MakeVertex(far).Vertex();
  BRepBuilderAPI_MakeWire wire;
  wire.Add(curves);
  for (const auto& [from, to] : {std::make_pair(loose[1], at_tail_out), std::make_pair(at_tail_out, at_far),
                                 std::make_pair(at_far, at_head_out), std::make_pair(at_head_out, loose[0])}) {
    BRepBuilderAPI_MakeEdge side(from, to);
    if (!side.IsDone()) {
      return failed("face end: no side of the profile");
    }
    wire.Add(side.Edge());
  }
  if (!wire.IsDone()) {
    return failed("face end: the profile's wire does not close");
  }
  BRepBuilderAPI_MakeFace profile(plane, wire.Wire(), true);
  if (!profile.IsDone()) {
    return failed("face end: no profile face");
  }
  BRepPrimAPI_MakePrism prism(profile.Face(), along * length);
  build(prism);
  if (!prism.IsDone()) {
    return failed("face end: no prism");
  }
  // The cross-section's edges in the profile, by their middles.
  std::vector<gp_Pnt> middles;
  for (TopExp_Explorer it(cross, TopAbs_EDGE); it.More(); it.Next()) {
    middles.push_back(middle_of_edge(TopoDS::Edge(it.Current())));
  }
  const std::string continued = face_name(feature, "face_end", body.edge_name(transition.selected));
  FaceNamer namer(prism.Shape());
  for (TopExp_Explorer it(profile.Face(), TopAbs_EDGE); it.More(); it.Next()) {
    const gp_Pnt middle = middle_of_edge(TopoDS::Edge(it.Current()));
    const bool on_cross = std::any_of(middles.begin(), middles.end(),
                                      [&](const gp_Pnt& m) { return m.Distance(middle) < 1.0e-6 * (1.0 + reach); });
    namer.generated(prism, it.Current(), on_cross ? continued : tool);
  }
  for (TopExp_Explorer it(prism.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    if (!namer.named(it.Current())) {
      namer.add(it.Current(), tool);
    }
  }
  namer.finish();
  return namer.shape();
}

// The result with flat ends (`flat`) with each transition's flat end
// carried on as its face end: the face ends' prisms, each clipped to the
// region kept round the unselected edges it runs on into (`regions`, its
// transition's run: the prism takes away the body's material only there),
// cut away. Null when one cannot be made.
ShapePtr face_ends(const std::string& feature, const Shape& body, const ShapePtr& flat,
                   const std::vector<ShapePtr>& regions, const std::vector<Transition>& transitions,
                   const std::string& tool) {
  // The prisms run on past the regions.
  Bnd_Box box;
  for (const ShapePtr& region : regions) {
    BRepBndLib::Add(region->occt(), box);
  }
  if (box.IsVoid()) {
    return failed("face end: an empty region");
  }
  const double length = std::sqrt(box.SquareExtent()) + 1.0;
  EdgeFaces faces_of;
  TopExp::MapShapesAndAncestors(flat->occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  std::vector<std::pair<ShapePtr, std::size_t>> prisms; // with their runs
  for (const Transition& transition : transitions) {
    if (transition.run >= regions.size()) {
      return failed("face end: a vertex without its region");
    }
    std::vector<int> caps; // the cap's pieces
    for (int f = 0; f < flat->face_count(); ++f) {
      if (face_matches(flat->face_names(f), transition.cap)) {
        caps.push_back(f);
      }
    }
    if (caps.empty()) {
      return failed("face end: the cap " + transition.cap + " is not in the result with flat ends");
    }
    const ShapePtr prism = face_end_prism(feature, body, *flat, caps, transition, faces_of, length, tool);
    if (!prism) {
      return nullptr;
    }
    prisms.emplace_back(prism, transition.run);
  }
  ShapePtr result = flat;
  for (const auto& [prism, run] : prisms) {
    const BooleanResult clipped = boolean(BooleanOp::Intersect, {prism.get()}, *regions[run]);
    if (clipped.pieces.empty()) {
      return failed("face end: the prism misses the region");
    }
    for (const BooleanPiece& piece : clipped.pieces) {
      const BooleanResult made = boolean(BooleanOp::Cut, {result.get()}, *piece.shape);
      if (!made.touched.empty() && !made.touched.front()) {
        continue;
      }
      result = single(made);
      if (!result) {
        return failed("face end: cutting the clipped prism away gives " + std::to_string(made.pieces.size()) +
                      " pieces");
      }
    }
  }
  return result;
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
                             const std::vector<PartialChain>& chains, ChainEnd end) {
  const std::string tool = face_name(feature, "runout_tool");
  std::vector<const PartialChain*> convex_chains;
  std::vector<const PartialChain*> concave_chains;
  for (const PartialChain& chain : chains) {
    const std::optional<bool> is_convex = chain_convex(body, chain);
    if (!is_convex) {
      return failed("a chain's selected edge is neither convex nor concave");
    }
    (*is_convex ? convex_chains : concave_chains).push_back(&chain);
  }
  // The face end takes material away: convex chains only.
  if (end == ChainEnd::Face && !concave_chains.empty()) {
    return failed("face end: a concave chain");
  }
  const bool run_out = end == ChainEnd::RunOut;
  std::vector<Transition> transitions;
  ShapePtr result = std::make_shared<Shape>(full);
  std::vector<ShapePtr> convex_regions;
  // Convex: the body's material inside the regions put back into the
  // dressup's result.
  if (!convex_chains.empty()) {
    convex_regions = kept_region(feature, body, convex_chains, end, tool, transitions);
    if (convex_regions.empty()) {
      return nullptr;
    }
    for (const ShapePtr& region : convex_regions) {
      const BooleanResult kept = boolean(BooleanOp::Intersect, {&body}, *region);
      if (kept.pieces.empty()) {
        return failed("the body has nothing inside a region kept");
      }
      for (const BooleanPiece& piece : kept.pieces) {
        result = single(boolean(BooleanOp::Join, {result.get()}, *piece.shape));
        if (!result) {
          return failed("putting the body back inside a region kept gives more than one solid");
        }
      }
    }
  }
  // Concave: the dressup's material inside the regions outside the body
  // taken away again.
  if (!concave_chains.empty()) {
    const std::vector<ShapePtr> regions = kept_region(feature, body, concave_chains, end, tool, transitions);
    if (regions.empty()) {
      return nullptr;
    }
    for (const ShapePtr& region : regions) {
      const BooleanResult outside = boolean(BooleanOp::Cut, {region.get()}, body);
      for (const BooleanPiece& piece : outside.pieces) {
        const BooleanResult made = boolean(BooleanOp::Cut, {result.get()}, *piece.shape);
        if (!made.touched.empty() && !made.touched.front()) {
          continue;
        }
        result = single(made);
        if (!result) {
          return failed("taking the dressup away outside the body gives more than one solid");
        }
      }
    }
  }
  // The flat ends carried on past their vertices as the face ends.
  if (end == ChainEnd::Face) {
    if (convex_regions.empty() || transitions.empty()) {
      return failed("face end: no vertex to end at");
    }
    result = face_ends(feature, body, result, convex_regions, transitions, tool);
    if (!result) {
      return nullptr;
    }
  }
  // The tools' own faces lie past the dressup: none is left. With the
  // run-out the flat end's face is inside the taper too, and with the face
  // end the caps it carried on are gone (a face end that would cut on into
  // the material past the region, where the faces beyond the vertex turn
  // towards the dressup, leaves the region's or the prism's faces).
  for (const char* role : {"runout_tool", "runout_end", "runout_cap"}) {
    if (std::string(role) == "runout_end" && !run_out) {
      continue;
    }
    if (holds_role(*result, feature, role)) {
      // (For the trace: where each such piece lies, the middle of its box.)
      const std::string prefix = feature + ":" + role;
      for (int f = 0; f < result->face_count(); ++f) {
        const NameList& names = result->face_names(f);
        if (std::none_of(names.begin(), names.end(),
                         [&](const std::string& name) { return name.compare(0, prefix.size(), prefix) == 0; })) {
          continue;
        }
        Bnd_Box box;
        BRepBndLib::Add(result->face(f), box);
        if (box.IsVoid()) {
          continue;
        }
        double x0 = 0.0;
        double y0 = 0.0;
        double z0 = 0.0;
        double x1 = 0.0;
        double y1 = 0.0;
        double z1 = 0.0;
        box.Get(x0, y0, z0, x1, y1, z1);
        runout_trace(std::string("left over, ") + role + " from x " + std::to_string(x0) + " y " +
                     std::to_string(y0) + " z " + std::to_string(z0) + " to x " + std::to_string(x1) + " y " +
                     std::to_string(y1) + " z " + std::to_string(z1));
      }
      return failed(std::string("the result still shows the role ") + role);
    }
  }
  if (!is_valid(result->occt())) {
    return failed("the result is not valid");
  }
  result->copy_notes(full);
  return result;
}

} // namespace mitcad::geometry::detail
