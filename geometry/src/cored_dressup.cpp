// SPDX-License-Identifier: MIT
#include "cored_dressup.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools_History.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec.hxx>

#include "history.hpp"
#include "mitcad/geometry/naming.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kPi = 3.14159265358979323846;

// A point or direction in the prism's section.
struct V2 {
  double x = 0.0;
  double y = 0.0;
};
V2 operator+(V2 a, V2 b) { return {a.x + b.x, a.y + b.y}; }
V2 operator-(V2 a, V2 b) { return {a.x - b.x, a.y - b.y}; }
V2 operator*(V2 a, double k) { return {a.x * k, a.y * k}; }
double cross(V2 a, V2 b) { return a.x * b.y - a.y * b.x; }
double dot(V2 a, V2 b) { return a.x * b.x + a.y * b.y; }
double length(V2 a) { return std::hypot(a.x, a.y); }

double segment_distance(V2 p, V2 a, V2 b) {
  const V2 ab = b - a;
  const double l2 = dot(ab, ab);
  const double t = l2 > 0.0 ? std::clamp(dot(p - a, ab) / l2, 0.0, 1.0) : 0.0;
  return length(p - (a + ab * t));
}

double polygon_area(const std::vector<V2>& p) {
  double twice = 0.0;
  for (std::size_t k = 0; k < p.size(); ++k) {
    twice += cross(p[k], p[(k + 1) % p.size()]);
  }
  return 0.5 * twice;
}

bool inside_polygon(V2 p, const std::vector<V2>& polygon) {
  bool inside = false;
  for (std::size_t k = 0, j = polygon.size() - 1; k < polygon.size(); j = k++) {
    const V2 a = polygon[k];
    const V2 b = polygon[j];
    if ((a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) {
      inside = !inside;
    }
  }
  return inside;
}

// The disc of radius `R` that fills the notch of an inner corner `p` of the
// core polygon, between its edges from `p0` to `p` (direction t1) and from
// `p` to `p1` (direction t2): of the discs that touch both edges, each on
// the edge or at its far end, and lie outside the core (no closer than R to
// any of its edges), the one nearest `p`. Its contacts `a` on the first edge
// and `b` on the second bound the arc that replaces the corner: the core
// closed by the disc, as the ball rolled between the roundings of the two
// sides rounds the inner corner's edge. None where no disc is such.
struct Notch {
  V2 centre;
  V2 a;
  V2 b;
};

std::optional<Notch> notch(V2 p0, V2 p, V2 p1, double R, const std::vector<V2>& polygon, double tol) {
  const V2 e1 = p - p0;
  const V2 e2 = p1 - p;
  const double l1 = length(e1);
  const double l2 = length(e2);
  if (!(l1 > tol) || !(l2 > tol)) {
    return std::nullopt;
  }
  const V2 t1 = e1 * (1.0 / l1);
  const V2 t2 = e2 * (1.0 / l2);
  const V2 n1{t1.y, -t1.x};
  const V2 n2{t2.y, -t2.x};
  std::vector<Notch> candidates;
  // Touching both edges' lines.
  if (1.0 + dot(n1, n2) > 1.0e-9) {
    const V2 c = p + (n1 + n2) * (R / (1.0 + dot(n1, n2)));
    candidates.push_back({c, c - n1 * R, c - n2 * R});
  }
  // The first edge's line and the second edge's far end: c = p + t1 s + n1 R
  // with |c - p1| = R.
  {
    const V2 w = p + n1 * R - p1;
    const double half_b = dot(w, t1);
    const double disc = half_b * half_b - (dot(w, w) - R * R);
    if (disc >= 0.0) {
      for (const double s : {-half_b - std::sqrt(disc), -half_b + std::sqrt(disc)}) {
        const V2 c = p + t1 * s + n1 * R;
        candidates.push_back({c, c - n1 * R, p1});
      }
    }
  }
  // The first edge's far end and the second edge's line: c = p + t2 s + n2 R
  // with |c - p0| = R.
  {
    const V2 w = p + n2 * R - p0;
    const double half_b = dot(w, t2);
    const double disc = half_b * half_b - (dot(w, w) - R * R);
    if (disc >= 0.0) {
      for (const double s : {-half_b - std::sqrt(disc), -half_b + std::sqrt(disc)}) {
        const V2 c = p + t2 * s + n2 * R;
        candidates.push_back({c, p0, c - n2 * R});
      }
    }
  }
  // Both far ends.
  {
    const V2 chord = p1 - p0;
    const double half = 0.5 * length(chord);
    if (half > tol && half <= R) {
      const V2 m = p0 + chord * 0.5;
      const V2 across{-chord.y / (2.0 * half), chord.x / (2.0 * half)};
      const double h = std::sqrt(std::max(0.0, R * R - half * half));
      candidates.push_back({m + across * h, p0, p1});
      candidates.push_back({m - across * h, p0, p1});
    }
  }
  std::optional<Notch> best;
  for (Notch& c : candidates) {
    const double sa = dot(c.a - p0, t1);
    const double sb = dot(c.b - p, t2);
    if (sa < -tol || sa > l1 + tol || sb < -tol || sb > l2 + tol) {
      continue;
    }
    c.a = p0 + t1 * std::clamp(sa, 0.0, l1);
    c.b = p + t2 * std::clamp(sb, 0.0, l2);
    if (inside_polygon(c.centre, polygon) || cross(c.a - c.centre, c.b - c.centre) >= 0.0) {
      continue;
    }
    bool clear = true;
    for (std::size_t k = 0; k < polygon.size() && clear; ++k) {
      clear = segment_distance(c.centre, polygon[k], polygon[(k + 1) % polygon.size()]) >= R - tol;
    }
    if (clear && (!best || length(c.centre - p) < length(best->centre - p))) {
      best = c;
    }
  }
  return best;
}

using AncestorMap = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The prism's frame: u and v across it, d along it (u x v = d); a point p
// is (p.u, p.v) in the section and p.d along.
struct Frame {
  gp_Dir u;
  gp_Dir v;
  gp_Dir d;
  V2 at(const gp_Pnt& p) const {
    const gp_Vec w(p.XYZ());
    return {w.Dot(gp_Vec(u)), w.Dot(gp_Vec(v))};
  }
  double along(const gp_Pnt& p) const { return gp_Vec(p.XYZ()).Dot(gp_Vec(d)); }
  gp_Vec across(V2 m) const { return gp_Vec(u) * m.x + gp_Vec(v) * m.y; }
  gp_Pnt point(V2 q, double s) const { return gp_Pnt((across(q) + gp_Vec(d) * s).XYZ()); }
};

// A side of the prism: its face, its edges on the two ends, whether the
// fillet leaves it alone (no selected edge on it), and its section.
struct Side {
  int face = -1;
  std::array<int, 2> end_edge{{-1, -1}};
  bool alone = false;
  V2 from;
  V2 t;
  V2 inward;
};

// A corner of the section, where side k - 1 meets side k: the edge along
// the prism there and its vertices on the two ends.
struct Corner {
  int edge = -1;
  std::array<TopoDS_Shape, 2> vertex;
  bool convex = false;
};

// The ball's section about a core edge or corner, on its outer side
// (`outward` across, the core reaching from c0 to c1 along d): half a
// stadium, or half a disc where the core is flat, its straight side on the
// core.
struct Profile {
  TopoDS_Face face;
  TopoDS_Edge low;   // the arc about c0 (the whole arc where flat)
  TopoDS_Edge high;  // the arc about c1 (null where flat)
  TopoDS_Edge outer; // the line at r from c1 to c0 (null where flat)
};

TopoDS_Edge arc(const gp_Pnt& a, const gp_Pnt& m, const gp_Pnt& b) {
  const occ::handle<Geom_TrimmedCurve> curve = GC_MakeArcOfCircle(a, m, b).Value();
  return BRepBuilderAPI_MakeEdge(curve).Edge();
}

std::optional<Profile> profile(const Frame& frame, V2 c, V2 outward, double c0, double c1, double r) {
  const double h = std::sqrt(0.5);
  const auto at = [&](double a, double b) { return frame.point(c + outward * a, b); };
  Profile made;
  BRepBuilderAPI_MakeWire wire;
  if (c1 > c0) {
    const TopoDS_Edge axis = BRepBuilderAPI_MakeEdge(at(0.0, c0 - r), at(0.0, c1 + r)).Edge();
    made.high = arc(at(0.0, c1 + r), at(h * r, c1 + h * r), at(r, c1));
    made.outer = BRepBuilderAPI_MakeEdge(at(r, c1), at(r, c0)).Edge();
    made.low = arc(at(r, c0), at(h * r, c0 - h * r), at(0.0, c0 - r));
    wire.Add(axis);
    wire.Add(made.high);
    wire.Add(made.outer);
    wire.Add(made.low);
  } else {
    const TopoDS_Edge axis = BRepBuilderAPI_MakeEdge(at(0.0, c0 - r), at(0.0, c0 + r)).Edge();
    made.low = arc(at(0.0, c0 + r), at(r, c0), at(0.0, c0 - r));
    wire.Add(axis);
    wire.Add(made.low);
  }
  if (!wire.IsDone()) {
    return std::nullopt;
  }
  BRepBuilderAPI_MakeFace face(wire.Wire(), true);
  if (!face.IsDone()) {
    return std::nullopt;
  }
  made.face = face.Face();
  return made;
}

// The faces of a sweep of the ball's section named by the part of the
// section they come from (the wire the section's face was made of holds
// copies of its edges, so they are told apart by their surfaces): a sphere,
// a torus or a cylinder across the prism about c0 `low`, about c1 `high`, a
// cylinder along the prism `outer`. Planes stay unnamed.
std::vector<Shape::NamedFace> named_sweep(const TopoDS_Shape& sweep, const Frame& frame, double c0, double c1,
                                          double tol, const NameList& low, const NameList& high,
                                          const NameList& outer) {
  std::vector<Shape::NamedFace> faces;
  const auto about = [&](const gp_Pnt& centre) -> const NameList& {
    return c1 > c0 && std::abs(frame.along(centre) - c1) <= tol ? high : low;
  };
  for (TopExp_Explorer it(sweep, TopAbs_FACE); it.More(); it.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()), false);
    const NameList* names = nullptr;
    if (surface.GetType() == GeomAbs_Sphere) {
      names = &about(surface.Sphere().Location());
    } else if (surface.GetType() == GeomAbs_Torus) {
      names = &about(surface.Torus().Location());
    } else if (surface.GetType() == GeomAbs_Cylinder) {
      const gp_Ax1 axis = surface.Cylinder().Axis();
      names = std::abs(axis.Direction().Dot(frame.d)) > 1.0 - 1.0e-9 ? &outer : &about(axis.Location());
    }
    if (names && !names->empty()) {
      faces.push_back({it.Current(), *names});
    }
  }
  return faces;
}

void append(NameList& to, const NameList& from) { to.insert(to.end(), from.begin(), from.end()); }

CoredFillet cored_along(const std::string& feature, const Shape& body, const std::vector<double>& radius, double r,
                        const std::vector<gp_Dir>& normal, const AncestorMap& faces_of, const gp_Dir& d) {
  CoredFillet out;
  const double tol = 1.0e-7 * std::max(1.0, r);
  Frame frame;
  frame.d = d;
  frame.u = gp_Ax2(gp_Pnt(0.0, 0.0, 0.0), d).XDirection();
  frame.v = d.Crossed(frame.u);
  const auto face_index = [&](const TopoDS_Shape& f) { return body.face_index(f); };
  const auto faces_at = [&](const TopoDS_Shape& edge) {
    std::vector<int> found;
    if (faces_of.Contains(edge)) {
      for (const TopoDS_Shape& f : faces_of.FindFromKey(edge)) {
        found.push_back(face_index(f));
      }
    }
    return found;
  };
  const auto selected = [&](int e) { return e >= 0 && radius[static_cast<std::size_t>(e)] > 0.0; };
  // The ends and the sides.
  std::array<int, 2> ends{{-1, -1}};
  std::vector<char> side(static_cast<std::size_t>(body.face_count()), 0);
  for (int f = 0; f < body.face_count(); ++f) {
    const double c = normal[static_cast<std::size_t>(f)].Dot(d);
    if (std::abs(c) < 1.0e-9) {
      side[static_cast<std::size_t>(f)] = 1;
    } else if (c < -1.0 + 1.0e-9 && ends[0] < 0) {
      ends[0] = f;
    } else if (c > 1.0 - 1.0e-9 && ends[1] < 0) {
      ends[1] = f;
    } else {
      return out;
    }
  }
  if (ends[0] < 0 || ends[1] < 0) {
    return out;
  }
  std::array<double, 2> s{};
  for (std::size_t k = 0; k < 2; ++k) {
    TopExp_Explorer v(body.face(ends[k]), TopAbs_VERTEX);
    if (!v.More()) {
      return out;
    }
    s[k] = frame.along(BRep_Tool::Pnt(TopoDS::Vertex(v.Current())));
  }
  for (TopExp_Explorer v(body.occt(), TopAbs_VERTEX); v.More(); v.Next()) {
    const double at = frame.along(BRep_Tool::Pnt(TopoDS::Vertex(v.Current())));
    if (std::abs(at - s[0]) > tol && std::abs(at - s[1]) > tol) {
      return out;
    }
  }
  const double width = s[1] - s[0];
  if (width < 2.0 * r - tol) {
    return out;
  }
  // The section: the first end's loop, counterclockwise about d.
  const TopoDS_Face& start = body.face(ends[0]);
  int loops = 0;
  TopoDS_Wire loop;
  for (TopExp_Explorer w(start, TopAbs_WIRE); w.More(); w.Next()) {
    loop = TopoDS::Wire(w.Current());
    ++loops;
  }
  if (loops != 1) {
    return out;
  }
  std::vector<TopoDS_Shape> vertices;
  std::vector<TopoDS_Shape> edges;
  for (BRepTools_WireExplorer w(loop, start); w.More(); w.Next()) {
    if (BRepAdaptor_Curve(w.Current()).GetType() != GeomAbs_Line) {
      return out;
    }
    edges.push_back(w.Current());
    vertices.push_back(w.CurrentVertex());
  }
  const std::size_t n = vertices.size();
  if (n < 3) {
    return out;
  }
  std::vector<V2> q;
  for (const TopoDS_Shape& vertex : vertices) {
    q.push_back(frame.at(BRep_Tool::Pnt(TopoDS::Vertex(vertex))));
  }
  const double area = polygon_area(q);
  if (std::abs(area) < tol) {
    return out;
  }
  if (area < 0.0) {
    std::vector<TopoDS_Shape> v2(n);
    std::vector<TopoDS_Shape> e2(n);
    std::vector<V2> q2(n);
    for (std::size_t k = 0; k < n; ++k) {
      v2[k] = vertices[n - 1 - k];
      q2[k] = q[n - 1 - k];
      e2[k] = edges[(2 * n - 2 - k) % n];
    }
    vertices = std::move(v2);
    edges = std::move(e2);
    q = std::move(q2);
  }
  // Each side: its face, its edges on the two ends; each corner: its edge
  // along the prism.
  const auto shared_edge = [&](int a, int b) {
    int found = -1;
    int count = 0;
    for (TopExp_Explorer it(body.face(a), TopAbs_EDGE); it.More(); it.Next()) {
      const std::vector<int> at = faces_at(it.Current());
      if (at.size() == 2 && ((at[0] == a && at[1] == b) || (at[0] == b && at[1] == a))) {
        const int e = body.edge_index(it.Current());
        if (e != found) {
          found = e;
          ++count;
        }
      }
    }
    return count == 1 ? found : -1;
  };
  std::vector<Side> sides(n);
  for (std::size_t k = 0; k < n; ++k) {
    Side& one = sides[k];
    const std::vector<int> at = faces_at(edges[k]);
    if (at.size() != 2) {
      return out;
    }
    one.face = at[0] == ends[0] ? at[1] : at[0];
    if (one.face < 0 || side[static_cast<std::size_t>(one.face)] == 0) {
      return out;
    }
    one.end_edge[0] = body.edge_index(edges[k]);
    one.end_edge[1] = shared_edge(one.face, ends[1]);
    if (one.end_edge[1] < 0) {
      return out;
    }
    const V2 span = q[(k + 1) % n] - q[k];
    const double l = length(span);
    if (!(l > tol)) {
      return out;
    }
    one.from = q[k];
    one.t = span * (1.0 / l);
    one.inward = {-one.t.y, one.t.x};
    one.alone = true;
    for (TopExp_Explorer it(body.face(one.face), TopAbs_EDGE); it.More() && one.alone; it.Next()) {
      one.alone = !selected(body.edge_index(it.Current()));
    }
  }
  // Every side is a face of its own, and every face a side or an end.
  {
    std::vector<int> seen;
    for (const Side& one : sides) {
      seen.push_back(one.face);
    }
    std::sort(seen.begin(), seen.end());
    if (std::adjacent_find(seen.begin(), seen.end()) != seen.end() ||
        static_cast<int>(seen.size()) + 2 != body.face_count()) {
      return out;
    }
  }
  std::vector<Corner> corners(n);
  for (std::size_t k = 0; k < n; ++k) {
    Corner& corner = corners[k];
    const Side& before = sides[(k + n - 1) % n];
    const Side& after = sides[k];
    corner.edge = shared_edge(before.face, after.face);
    if (corner.edge < 0) {
      return out;
    }
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(TopoDS::Edge(body.edge(corner.edge)), a, b);
    const bool a_first = std::abs(frame.along(BRep_Tool::Pnt(a)) - s[0]) <= tol;
    corner.vertex[0] = vertices[k];
    corner.vertex[1] = a_first ? b : a;
    if (!(a_first ? a : b).IsSame(vertices[k])) {
      return out;
    }
    corner.convex = cross(before.t, after.t) > 0.0;
  }
  // The selection: the ends are rounded; every side the fillet does not
  // leave alone has its edges on the ends rounded, every outer corner
  // between two such sides too (an inner one may be, closed by its notch
  // below).
  for (const int end : ends) {
    bool any = false;
    for (TopExp_Explorer it(body.face(end), TopAbs_EDGE); it.More() && !any; it.Next()) {
      any = selected(body.edge_index(it.Current()));
    }
    if (!any) {
      return out;
    }
  }
  for (const Side& one : sides) {
    if (!one.alone && (!selected(one.end_edge[0]) || !selected(one.end_edge[1]))) {
      return out;
    }
  }
  std::vector<char> inner(n, 0);
  for (std::size_t k = 0; k < n; ++k) {
    const bool rounded = !sides[(k + n - 1) % n].alone && !sides[k].alone;
    if (!rounded) {
      continue;
    }
    if (corners[k].convex && !selected(corners[k].edge)) {
      return out;
    }
    if (!corners[k].convex && selected(corners[k].edge)) {
      inner[k] = 1;
      out.inner.push_back(corners[k].edge);
    }
  }
  // A side left alone cuts the result by its plane: the whole section lies
  // on its inner side.
  for (const Side& one : sides) {
    if (!one.alone) {
      continue;
    }
    for (const V2& p : q) {
      if (dot(p - one.from, one.inward) < -tol) {
        return out;
      }
    }
  }
  // The core: the sides moved inward by r (those left alone outward, past
  // where they cut), their lines met in turn.
  const double beyond = 4.0 * r + width + 1.0;
  std::vector<V2> moved(n);
  for (std::size_t k = 0; k < n; ++k) {
    moved[k] = sides[k].from + sides[k].inward * (sides[k].alone ? -beyond : r);
  }
  std::vector<V2> core(n);
  for (std::size_t k = 0; k < n; ++k) {
    const Side& before = sides[(k + n - 1) % n];
    const Side& after = sides[k];
    const double turn = cross(before.t, after.t);
    if (std::abs(turn) < 1.0e-9) {
      return out;
    }
    const double a = cross(moved[k] - moved[(k + n - 1) % n], after.t) / turn;
    core[k] = moved[(k + n - 1) % n] + before.t * a;
  }
  for (std::size_t k = 0; k < n; ++k) {
    if (dot(core[(k + 1) % n] - core[k], sides[k].t) < -tol) {
      return out;
    }
  }
  for (std::size_t k = 0; k < n; ++k) {
    if (inner[k]) {
      continue;
    }
    for (std::size_t j = 0; j < n; ++j) {
      if (!sides[j].alone && segment_distance(core[k], q[j], q[(j + 1) % n]) < r - tol) {
        return out;
      }
    }
  }
  if (polygon_area(core) < -tol * r) {
    return out;
  }
  // Along the prism: from c0 to c1, one place where the ends are 2r apart.
  double c0 = s[0] + r;
  double c1 = s[1] - r;
  const bool thick = c1 - c0 > tol;
  if (!thick) {
    c0 = c1 = 0.5 * (s[0] + s[1]);
  }
  // The core's corners that coincide: one group each.
  std::vector<std::size_t> group(n);
  std::vector<V2> centre;
  for (std::size_t k = 0; k < n; ++k) {
    std::size_t g = 0;
    while (g < centre.size() && length(centre[g] - core[k]) > tol) {
      ++g;
    }
    if (g == centre.size()) {
      centre.push_back(core[k]);
    }
    group[k] = g;
  }
  const std::size_t none = centre.size();
  const auto group_at = [&](V2 p) {
    for (std::size_t g = 0; g < centre.size(); ++g) {
      if (length(centre[g] - p) <= tol) {
        return g;
      }
    }
    return none;
  };
  // Where the fillet rounds an inner corner's edge too (between two rounded
  // sides), the ball rolls between the two sides' roundings: the core is
  // closed there by a disc of 2r (notch), its corner an arc of 2r.
  std::vector<Notch> notches(n);
  for (std::size_t k = 0; k < n; ++k) {
    if (!inner[k]) {
      continue;
    }
    const std::optional<Notch> made = notch(centre[group[(k + n - 1) % n]], centre[group[k]],
                                            centre[group[(k + 1) % n]], 2.0 * r, core, tol);
    if (!made) {
      return out;
    }
    notches[k] = *made;
  }
  // The core's outline, counterclockwise: each side's core edge (from where
  // a rounded inner corner's arc ends to where the next begins) and the
  // arcs. Tangents along it; its outward normal is the tangent turned
  // clockwise.
  struct Run {
    std::size_t side = 0;
    bool arc = false;
    V2 a;
    V2 b;
    V2 ta;
    V2 tb;
    std::size_t ga = 0;
    std::size_t gb = 0;
  };
  const auto outward_of = [](V2 t) { return V2{t.y, -t.x}; };
  const auto arc_tangent = [](V2 p, V2 c) {
    const V2 w = p - c;
    return V2{w.y, -w.x} * (1.0 / length(w));
  };
  std::vector<Run> runs;
  std::vector<std::size_t> zero_at(n, none);
  for (std::size_t k = 0; k < n; ++k) {
    const std::size_t next = (k + 1) % n;
    Run line;
    line.side = k;
    line.a = inner[k] ? notches[k].b : centre[group[k]];
    line.b = inner[next] ? notches[next].a : centre[group[next]];
    line.ta = line.tb = sides[k].t;
    line.ga = group_at(line.a);
    line.gb = group_at(line.b);
    const double l = dot(line.b - line.a, sides[k].t);
    if (l < -tol) {
      return out;
    }
    if (l > tol) {
      runs.push_back(line);
    } else if (!sides[k].alone) {
      zero_at[k] = line.ga;
    }
    if (inner[next]) {
      const Notch& corner = notches[next];
      Run arc;
      arc.side = next;
      arc.arc = true;
      arc.a = corner.a;
      arc.b = corner.b;
      arc.ta = arc_tangent(corner.a, corner.centre);
      arc.tb = arc_tangent(corner.b, corner.centre);
      arc.ga = group_at(corner.a);
      arc.gb = group_at(corner.b);
      runs.push_back(arc);
    }
  }
  // The names: a side's edges on the ends rounded, a corner's edge along the
  // prism rounded, the corners of the ends.
  const auto fillet_of = [&](int e) {
    const std::string& name = body.edge_name(e);
    return name.empty() ? NameList() : NameList{face_name(feature, "fillet", name)};
  };
  const auto corner_of = [&](const TopoDS_Shape& vertex) {
    const std::optional<std::string> name = body.name_of_vertex(vertex);
    return name ? NameList{face_name(feature, "corner", *name)} : NameList();
  };
  std::vector<ShapePtr> pieces;
  const auto add_piece = [&](const TopoDS_Shape& shape, std::vector<Shape::NamedFace> faces) {
    pieces.push_back(std::make_shared<Shape>(shape, faces));
  };
  // Each run of a rounded side: the ball's section swept along it; each arc:
  // the section turned about the arc's centre, its names the inner
  // corner's.
  for (const Run& one : runs) {
    if (one.arc) {
      const Notch& corner = notches[one.side];
      const V2 wa = one.a - corner.centre;
      const V2 wb = one.b - corner.centre;
      const double angle = std::atan2(cross(wb, wa), dot(wa, wb));
      const std::optional<Profile> made = profile(frame, one.b, outward_of(one.tb), c0, c1, r);
      if (!made || !(angle > 1.0e-9)) {
        return out;
      }
      BRepPrimAPI_MakeRevol turn(made->face, gp_Ax1(frame.point(corner.centre, 0.0), frame.d), angle);
      if (!turn.IsDone()) {
        return out;
      }
      NameList low = corner_of(corners[one.side].vertex[0]);
      NameList high = corner_of(corners[one.side].vertex[1]);
      NameList outer = fillet_of(corners[one.side].edge);
      if (!thick) {
        append(low, high);
        append(low, outer);
        high.clear();
        outer.clear();
      }
      add_piece(turn.Shape(), named_sweep(turn.Shape(), frame, c0, c1, tol, low, high, outer));
      continue;
    }
    const Side& side_k = sides[one.side];
    if (side_k.alone) {
      continue;
    }
    const double l = length(one.b - one.a);
    const std::optional<Profile> made = profile(frame, one.a, side_k.inward * -1.0, c0, c1, r);
    if (!made || !(l > tol)) {
      return out;
    }
    BRepPrimAPI_MakePrism sweep(made->face, frame.across(side_k.t) * l);
    if (!sweep.IsDone()) {
      return out;
    }
    NameList low = fillet_of(side_k.end_edge[0]);
    NameList high = fillet_of(side_k.end_edge[1]);
    if (!thick) {
      append(low, high);
      high.clear();
    }
    add_piece(sweep.Shape(), named_sweep(sweep.Shape(), frame, c0, c1, tol, low, high, body.face_names(side_k.face)));
  }
  // Each outer corner of the core: the section turned about it. Its names:
  // the corners of the ends there, the corner edges' roundings, and the
  // roundings of the sides whose core edges shrank to it.
  const auto names_at = [&](std::size_t g, NameList& low, NameList& high, NameList& outer) {
    for (std::size_t k = 0; k < n; ++k) {
      if (group[k] == g && !inner[k]) {
        append(low, corner_of(corners[k].vertex[0]));
        append(high, corner_of(corners[k].vertex[1]));
        if (selected(corners[k].edge)) {
          append(outer, fillet_of(corners[k].edge));
        }
      }
      if (zero_at[k] == g) {
        append(low, fillet_of(sides[k].end_edge[0]));
        append(high, fillet_of(sides[k].end_edge[1]));
      }
    }
    if (!thick) {
      append(low, high);
      append(low, outer);
      high.clear();
      outer.clear();
    }
  };
  const auto touches_alone = [&](std::size_t g) {
    for (std::size_t k = 0; k < n; ++k) {
      if (group[k] == g && (sides[k].alone || sides[(k + n - 1) % n].alone)) {
        return true;
      }
    }
    return false;
  };
  if (runs.empty() && centre.size() != 1) {
    return out;
  }
  if (runs.empty()) {
    // The core is one point (the ball) or a segment along the prism.
    NameList low;
    NameList high;
    NameList outer;
    names_at(0, low, high, outer);
    const std::optional<Profile> made = profile(frame, centre[0], {1.0, 0.0}, c0, c1, r);
    if (!made) {
      return out;
    }
    if (!thick) {
      BRepPrimAPI_MakeSphere ball(frame.point(centre[0], c0), r);
      std::vector<Shape::NamedFace> faces;
      for (TopExp_Explorer it(ball.Shape(), TopAbs_FACE); it.More(); it.Next()) {
        faces.push_back({it.Current(), low});
      }
      add_piece(ball.Shape(), faces);
    } else {
      BRepPrimAPI_MakeRevol turn(made->face, gp_Ax1(frame.point(centre[0], 0.0), frame.d), 2.0 * kPi);
      if (!turn.IsDone()) {
        return out;
      }
      add_piece(turn.Shape(), named_sweep(turn.Shape(), frame, c0, c1, tol, low, high, outer));
    }
  }
  for (std::size_t i = 0; i < runs.size(); ++i) {
    const Run& before = runs[i];
    const Run& after = runs[(i + 1) % runs.size()];
    const std::size_t g = after.ga;
    const double c = cross(before.tb, after.ta);
    const double o = dot(before.tb, after.ta);
    double angle = std::atan2(c, o);
    if (std::abs(c) < 1.0e-12 && o < 0.0) {
      angle = kPi;
    }
    if ((before.arc || after.arc) && angle < -1.0e-9) {
      // The arc of an inner corner meets the outline at an inner corner.
      return out;
    }
    if (angle <= 1.0e-9 || (g != none && touches_alone(g))) {
      continue;
    }
    const std::optional<Profile> made = profile(frame, after.a, outward_of(before.tb), c0, c1, r);
    if (!made) {
      return out;
    }
    BRepPrimAPI_MakeRevol turn(made->face, gp_Ax1(frame.point(after.a, 0.0), frame.d), angle);
    if (!turn.IsDone()) {
      return out;
    }
    NameList low;
    NameList high;
    NameList outer;
    if (g != none) {
      names_at(g, low, high, outer);
    }
    add_piece(turn.Shape(), named_sweep(turn.Shape(), frame, c0, c1, tol, low, high, outer));
  }
  // The core's outline thickened by r along the prism, where it has an area.
  {
    // Its points, each with the centre of the arc to the next (or none).
    std::vector<std::pair<V2, std::optional<V2>>> outline;
    for (const Run& one : runs) {
      if (outline.empty() || length(outline.back().first - one.a) > tol) {
        outline.push_back({one.a, std::nullopt});
      }
      if (one.arc) {
        outline.back().second = notches[one.side].centre;
      }
    }
    while (outline.size() > 1 && length(outline.back().first - outline.front().first) <= tol &&
           !outline.back().second) {
      outline.pop_back();
    }
    std::vector<V2> points;
    for (const auto& p : outline) {
      points.push_back(p.first);
    }
    if (outline.size() >= 2 && (outline.size() >= 3 || outline.front().second || outline.back().second) &&
        polygon_area(points) > tol * tol) {
      BRepBuilderAPI_MakeWire wire;
      for (std::size_t k = 0; k < outline.size(); ++k) {
        const V2 a = outline[k].first;
        const V2 b = outline[(k + 1) % outline.size()].first;
        if (outline[k].second) {
          const V2 c = *outline[k].second;
          const V2 w = (a - c) + (b - c);
          const V2 m = c + w * (2.0 * r / length(w));
          wire.Add(arc(frame.point(a, c0 - r), frame.point(m, c0 - r), frame.point(b, c0 - r)));
        } else {
          wire.Add(BRepBuilderAPI_MakeEdge(frame.point(a, c0 - r), frame.point(b, c0 - r)).Edge());
        }
      }
      if (!wire.IsDone()) {
        return out;
      }
      BRepBuilderAPI_MakeFace face(wire.Wire(), true);
      if (!face.IsDone()) {
        return out;
      }
      BRepPrimAPI_MakePrism slab(face.Face(), gp_Vec(frame.d) * (c1 - c0 + 2.0 * r));
      if (!slab.IsDone()) {
        return out;
      }
      add_piece(slab.Shape(), {});
    }
  }
  if (pieces.empty()) {
    return out;
  }
  // Joined, cut by the sides left alone, the pieces of one surface merged.
  occ::handle<BRepTools_History> history;
  const auto merge = [&](const occ::handle<BRepTools_History>& made) {
    if (history.IsNull()) {
      history = new BRepTools_History(*made);
    } else {
      history->Merge(made);
    }
  };
  TopoDS_Shape result = pieces.front()->occt();
  if (pieces.size() > 1) {
    NCollection_List<TopoDS_Shape> first;
    NCollection_List<TopoDS_Shape> rest;
    first.Append(result);
    for (std::size_t p = 1; p < pieces.size(); ++p) {
      rest.Append(pieces[p]->occt());
    }
    BRepAlgoAPI_Fuse fuse;
    fuse.SetArguments(first);
    fuse.SetTools(rest);
    fuse.SetRunParallel(false);
    fuse.Build();
    if (!fuse.IsDone() || fuse.HasErrors()) {
      return out;
    }
    result = fuse.Shape();
    merge(fuse.History());
  }
  NCollection_List<TopoDS_Shape> cutters;
  double big = 0.0;
  for (const V2& p : q) {
    big = std::max(big, length(p));
  }
  big = 10.0 * (big + std::abs(s[0]) + std::abs(s[1]) + r + 1.0);
  for (const Side& one : sides) {
    if (!one.alone) {
      continue;
    }
    const gp_Dir outward(frame.across(one.inward * -1.0));
    const gp_Dir x(frame.across(one.t));
    const gp_Vec y = gp_Vec(outward.Crossed(x));
    const gp_Pnt corner = frame.point(one.from, s[0]).Translated(gp_Vec(x) * -big - y * big);
    cutters.Append(BRepPrimAPI_MakeBox(gp_Ax2(corner, outward, x), 3.0 * big, 3.0 * big, big).Shape());
  }
  if (!cutters.IsEmpty()) {
    NCollection_List<TopoDS_Shape> argument;
    argument.Append(result);
    BRepAlgoAPI_Cut cut;
    cut.SetArguments(argument);
    cut.SetTools(cutters);
    cut.SetRunParallel(false);
    cut.Build();
    if (!cut.IsDone() || cut.HasErrors()) {
      return out;
    }
    result = cut.Shape();
    merge(cut.History());
  }
  ShapeUpgrade_UnifySameDomain unify(result, true, true, false);
  unify.Build();
  result = unify.Shape();
  merge(unify.History());
  int solids = 0;
  for (TopExp_Explorer it(result, TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  int shells = 0;
  for (TopExp_Explorer it(result, TopAbs_SHELL); it.More(); it.Next()) {
    ++shells;
  }
  if (solids != 1 || shells != 1 || !BRepCheck_Analyzer(result, true, true).IsValid()) {
    return out;
  }
  FaceNamer namer(result);
  for (const ShapePtr& piece : pieces) {
    namer.carry(*history, *piece);
  }
  namer.carry_by_surface(body, "fillet");
  namer.finish();
  out.shape = namer.shape();
  return out;
}

} // namespace

CoredFillet cored_fillet(const std::string& feature, const Shape& body, const std::vector<double>& radius) {
  CoredFillet out;
  try {
    OCC_CATCH_SIGNALS
    if (static_cast<int>(radius.size()) != body.edge_count()) {
      return out;
    }
    double r = 0.0;
    std::vector<gp_Dir> directions;
    for (int e = 0; e < body.edge_count(); ++e) {
      const double size = radius[static_cast<std::size_t>(e)];
      if (!(size > 0.0)) {
        continue;
      }
      if (r == 0.0) {
        r = size;
      } else if (std::abs(size - r) > 1.0e-9 * std::max(1.0, r)) {
        return out;
      }
      const BRepAdaptor_Curve curve(TopoDS::Edge(body.edge(e)));
      if (curve.GetType() != GeomAbs_Line) {
        continue;
      }
      const gp_Dir d = curve.Line().Direction();
      if (std::none_of(directions.begin(), directions.end(),
                       [&](const gp_Dir& other) { return std::abs(other.Dot(d)) > 1.0 - 1.0e-12; })) {
        directions.push_back(d);
      }
    }
    if (!(r > 0.0)) {
      return out;
    }
    int solids = 0;
    for (TopExp_Explorer it(body.occt(), TopAbs_SOLID); it.More(); it.Next()) {
      ++solids;
    }
    if (solids != 1) {
      return out;
    }
    std::vector<gp_Dir> normal;
    for (int f = 0; f < body.face_count(); ++f) {
      const BRepAdaptor_Surface surface(body.face(f));
      if (surface.GetType() != GeomAbs_Plane) {
        return out;
      }
      gp_Dir n = surface.Plane().Axis().Direction();
      if (body.face(f).Orientation() == TopAbs_REVERSED) {
        n.Reverse();
      }
      normal.push_back(n);
    }
    AncestorMap faces_of;
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
    for (const gp_Dir& d : directions) {
      CoredFillet made = cored_along(feature, body, radius, r, normal, faces_of, d);
      if (made.shape) {
        return made;
      }
      out.inner.insert(out.inner.end(), made.inner.begin(), made.inner.end());
    }
  } catch (const Standard_Failure&) {
    out.shape = nullptr;
  }
  std::sort(out.inner.begin(), out.inner.end());
  out.inner.erase(std::unique(out.inner.begin(), out.inner.end()), out.inner.end());
  return out;
}

} // namespace mitcad::geometry::detail
