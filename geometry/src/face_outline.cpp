// SPDX-License-Identifier: MIT
#include "mitcad/geometry/face_outline.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <Geom2dAPI_InterCurveCurve.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_Surface.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_Algo.hxx>
#include <HLRBRep_HLRToShape.hxx>
#include <HLRBRep_TypeOfResultingEdge.hxx>
#include <TopAbs_State.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
// How far apart (mm) two points may be and still be one: the ends of a
// silhouette and of an edge it runs along, a line seen end on.
constexpr double kSamePoint = 1e-7;
// How far from right angles to the view a face's normal may be (the cosine
// between them) along a seam on the outline.
constexpr double kOnOutline = 1e-7;
// A tolerance in a face's parameters: where a parameter line meets the
// face's boundary.
constexpr double kParameter = 1e-9;
// The tolerance of the classification of a parameter line's pieces.
constexpr double kClassify = 1e-7;

// How far apart two points are seen along d (their distance across d).
double apart_seen_along(const gp_Pnt& p, const gp_Pnt& q, const gp_Dir& d) {
  const gp_Vec off(q, p);
  const gp_Vec view(d);
  return (off - view * off.Dot(view)).Magnitude();
}

// Whether the points of an edge's curve (sampled) are all one point seen
// along d: an edge seen end on.
bool end_on(const TopoDS_Edge& edge, const gp_Dir& d) {
  const BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  const gp_Pnt start = curve.Value(first);
  constexpr int kSamples = 8;
  for (int i = 1; i <= kSamples; ++i) {
    const double t = first + (last - first) * static_cast<double>(i) / static_cast<double>(kSamples);
    if (apart_seen_along(curve.Value(t), start, d) > kSamePoint) {
      return false;
    }
  }
  return true;
}

// Whether two lines are the same, either way round.
bool same_line(const ModelCurve& a, const ModelCurve& b) {
  if (a.kind != ModelCurveKind::Line || b.kind != ModelCurveKind::Line) {
    return false;
  }
  return (a.start.Distance(b.start) <= kSamePoint && a.end.Distance(b.end) <= kSamePoint) ||
         (a.start.Distance(b.end) <= kSamePoint && a.end.Distance(b.start) <= kSamePoint);
}

// Whether two edges run along the same curve: their ends and middles meet,
// either way round.
bool same_edge(const TopoDS_Edge& a, const TopoDS_Edge& b) {
  const BRepAdaptor_Curve one(a);
  const BRepAdaptor_Curve other(b);
  const std::array<gp_Pnt, 3> p{one.Value(one.FirstParameter()),
                                one.Value(0.5 * (one.FirstParameter() + one.LastParameter())),
                                one.Value(one.LastParameter())};
  const std::array<gp_Pnt, 3> q{other.Value(other.FirstParameter()),
                                other.Value(0.5 * (other.FirstParameter() + other.LastParameter())),
                                other.Value(other.LastParameter())};
  if (p[1].Distance(q[1]) > kSamePoint) {
    return false;
  }
  return (p[0].Distance(q[0]) <= kSamePoint && p[2].Distance(q[2]) <= kSamePoint) ||
         (p[0].Distance(q[2]) <= kSamePoint && p[2].Distance(q[0]) <= kSamePoint);
}

// The non-degenerate boundary edges of face f by their indices in the
// shape, each once, as curves_of lists them.
std::vector<int> boundary_edges(const Shape& shape, int f) {
  std::vector<int> edges;
  for (TopExp_Explorer it(shape.face(f), TopAbs_EDGE); it.More(); it.Next()) {
    const int e = shape.edge_index(it.Current());
    if (e >= 0 && !BRep_Tool::Degenerated(TopoDS::Edge(it.Current())) &&
        std::find(edges.begin(), edges.end(), e) == edges.end()) {
      edges.push_back(e);
    }
  }
  return edges;
}

// Whether a seam edge of the face is on the face's outline seen along d:
// the face's normal at right angles to d all along it (sampled; where it
// has none, at a cone's apex, not asked), and the seam not seen end on.
// A seam without a curve on the face is kept, as curves_of lists it.
bool seam_on_outline(const TopoDS_Face& face, const TopoDS_Edge& edge, const gp_Dir& d) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull() || !(last > first)) {
    return true;
  }
  const BRepAdaptor_Surface surface(face, false);
  const gp_Vec view(d);
  constexpr int kSamples = 8;
  bool normal_seen = false;
  bool long_seen = false;
  gp_Pnt start;
  for (int i = 0; i <= kSamples; ++i) {
    const double t = first + (last - first) * static_cast<double>(i) / static_cast<double>(kSamples);
    const gp_Pnt2d uv = pcurve->Value(t);
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(uv.X(), uv.Y(), p, du, dv);
    if (i == 0) {
      start = p;
    } else if (apart_seen_along(p, start, d) > kSamePoint) {
      long_seen = true;
    }
    const gp_Vec normal = du.Crossed(dv);
    const double size = normal.Magnitude();
    if (size <= 1e-12) {
      continue;
    }
    normal_seen = true;
    if (std::abs(normal.Dot(view)) > kOnOutline * size) {
      return false;
    }
  }
  return normal_seen && long_seen;
}

// The angles u of the generators of a cylinder or a cone (`position`, and
// the sine and cosine of its half angle: 0 and 1 for a cylinder) along
// which its normal, cos a (cos u X + sin u Y) - sin a Z, is at right angles
// to d: none seen along the axis or from inside the cone's angle.
std::vector<double> generator_angles(const gp_Ax3& position, double sine, double cosine, const gp_Dir& d) {
  const double dx = d.Dot(position.XDirection());
  const double dy = d.Dot(position.YDirection());
  const double dz = d.Dot(position.Direction());
  const double rho = std::hypot(dx, dy);
  std::vector<double> angles;
  if (rho <= 1e-12) {
    return angles;
  }
  const double k = sine * dz / (cosine * rho);
  if (std::abs(k) > 1.0) {
    return angles;
  }
  const double phi = std::atan2(dy, dx);
  const double turn = std::acos(k);
  angles.push_back(phi - turn);
  angles.push_back(phi + turn);
  return angles;
}

// The pieces of the face's parameter line at `u` between vmin and vmax
// that lie on the face (inside it or on its boundary), as (v0, v1): the
// line is cut where it meets the curves of the face's edges on its
// surface, and each piece between cuts is classified by its middle.
std::vector<std::pair<double, double>> pieces_on_face(const TopoDS_Face& face, double u, double vmin, double vmax) {
  std::vector<double> cuts{vmin, vmax};
  const double reach = 1.0 + (vmax - vmin);
  const occ::handle<Geom2d_Line> line = new Geom2d_Line(gp_Pnt2d(u, 0.0), gp_Dir2d(0.0, 1.0));
  const occ::handle<Geom2d_Curve> along = new Geom2d_TrimmedCurve(line, vmin - reach, vmax + reach);
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull() || !(last > first)) {
      continue;
    }
    // An edge's curve along the line (a seam, a part's straight side)
    // meets it all along: its ends cut it.
    for (const double t : {first, last}) {
      const gp_Pnt2d end = pcurve->Value(t);
      if (std::abs(end.X() - u) <= kParameter) {
        cuts.push_back(end.Y());
      }
    }
    const occ::handle<Geom2d_Curve> piece = new Geom2d_TrimmedCurve(pcurve, first, last);
    const Geom2dAPI_InterCurveCurve crossing(piece, along, kParameter);
    for (int i = 1; i <= crossing.NbPoints(); ++i) {
      cuts.push_back(crossing.Point(i).Y());
    }
    for (int i = 1; i <= crossing.NbSegments(); ++i) {
      occ::handle<Geom2d_Curve> on_edge;
      occ::handle<Geom2d_Curve> on_line;
      crossing.Segment(i, on_edge, on_line);
      if (!on_line.IsNull()) {
        cuts.push_back(on_line->Value(on_line->FirstParameter()).Y());
        cuts.push_back(on_line->Value(on_line->LastParameter()).Y());
      }
    }
  }
  for (double& v : cuts) {
    v = std::clamp(v, vmin, vmax);
  }
  std::sort(cuts.begin(), cuts.end());
  std::vector<double> stops;
  for (const double v : cuts) {
    if (stops.empty() || v - stops.back() > kParameter) {
      stops.push_back(v);
    }
  }
  std::vector<std::pair<double, double>> pieces;
  for (std::size_t i = 0; i + 1 < stops.size(); ++i) {
    BRepClass_FaceClassifier classifier(face, gp_Pnt2d(u, 0.5 * (stops[i] + stops[i + 1])), kClassify);
    const TopAbs_State state = classifier.State();
    if (state != TopAbs_IN && state != TopAbs_ON) {
      continue;
    }
    if (!pieces.empty() && pieces.back().second >= stops[i] - kParameter) {
      pieces.back().second = stops[i + 1];
    } else {
      pieces.emplace_back(stops[i], stops[i + 1]);
    }
  }
  return pieces;
}

// The silhouettes of a cylindrical or conical face: its generators at the
// angles of generator_angles, at each place of the angle in the face's
// parameter range, clipped to the face.
std::vector<ModelCurve> generator_silhouettes(const TopoDS_Face& face, const BRepAdaptor_Surface& surface,
                                              const gp_Dir& d) {
  gp_Ax3 position;
  double sine = 0.0;
  double cosine = 1.0;
  if (surface.GetType() == GeomAbs_Cylinder) {
    position = surface.Cylinder().Position();
  } else {
    const gp_Cone cone = surface.Cone();
    position = cone.Position();
    sine = std::sin(cone.SemiAngle());
    cosine = std::cos(cone.SemiAngle());
  }
  double umin = 0.0;
  double umax = 0.0;
  double vmin = 0.0;
  double vmax = 0.0;
  BRepTools::UVBounds(face, umin, umax, vmin, vmax);
  std::vector<ModelCurve> lines;
  for (const double angle : generator_angles(position, sine, cosine, d)) {
    for (int k = -3; k <= 3; ++k) {
      const double shifted = angle + kTwoPi * static_cast<double>(k);
      if (shifted < umin - kParameter || shifted > umax + kParameter) {
        continue;
      }
      const double u = std::clamp(shifted, umin, umax);
      for (const auto& [v0, v1] : pieces_on_face(face, u, vmin, vmax)) {
        ModelCurve line;
        line.kind = ModelCurveKind::Line;
        line.start = surface.Value(u, v0);
        line.end = surface.Value(u, v1);
        // (Seen end on: a generator along the view, a point.)
        if (apart_seen_along(line.end, line.start, d) <= kSamePoint) {
          continue;
        }
        const bool known = std::any_of(lines.begin(), lines.end(),
                                       [&line](const ModelCurve& other) { return same_line(other, line); });
        if (!known) {
          lines.push_back(line);
        }
      }
    }
  }
  return lines;
}

// The outlines of another curved face: OCCT's hidden line removal of the
// face alone (a copy of it: the face is left as it is), seen along d, its
// outline edges in model space, visible and hidden ones, each once.
std::vector<ModelCurve> traced_outlines(const TopoDS_Face& face, const gp_Dir& d) {
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(face, true, false).Shape();
  const occ::handle<HLRBRep_Algo> algo = new HLRBRep_Algo();
  algo->Add(copy);
  algo->Projector(HLRAlgo_Projector(gp_Ax2(gp::Origin(), d)));
  algo->Update();
  algo->Hide();
  HLRBRep_HLRToShape out(algo);
  std::vector<TopoDS_Edge> edges;
  for (const bool visible : {true, false}) {
    const TopoDS_Shape found = out.CompoundOfEdges(HLRBRep_OutLine, visible, true);
    if (found.IsNull()) {
      continue;
    }
    for (TopExp_Explorer it(found, TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge) || end_on(edge, d)) {
        continue;
      }
      const bool known = std::any_of(edges.begin(), edges.end(), [&edge](const TopoDS_Edge& other) {
        return other.IsSame(edge) || same_edge(other, edge);
      });
      if (!known) {
        edges.push_back(edge);
      }
    }
  }
  std::vector<ModelCurve> curves;
  curves.reserve(edges.size());
  for (const TopoDS_Edge& edge : edges) {
    curves.push_back(edge_curve(edge));
  }
  return curves;
}

std::vector<ModelCurve> silhouettes_of(const TopoDS_Face& face, const gp_Dir& d) {
  TopLoc_Location location;
  if (BRep_Tool::Surface(face, location).IsNull()) {
    return {};
  }
  const BRepAdaptor_Surface surface(face);
  switch (surface.GetType()) {
  case GeomAbs_Plane:
    return {};
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
    return generator_silhouettes(face, surface, d);
  default:
    return traced_outlines(face, d);
  }
}

} // namespace

std::vector<ModelCurve> face_outline(const Shape& shape, const std::string& name, const gp_Dir& direction) {
  return detail::run("face outline", [&] {
    std::vector<ModelCurve> curves;
    const std::vector<int> faces = shape.find_faces(name);
    std::vector<int> listed;
    for (const int f : faces) {
      const TopoDS_Face& face = shape.face(f);
      for (const int e : boundary_edges(shape, f)) {
        if (std::find(listed.begin(), listed.end(), e) != listed.end()) {
          continue;
        }
        listed.push_back(e);
        const TopoDS_Edge& edge = shape.edge(e);
        // A seam is no edge to see: only where it is on the outline.
        if (BRep_Tool::IsClosed(edge, face) && !seam_on_outline(face, edge, direction)) {
          continue;
        }
        curves.push_back(edge_curve(edge));
      }
    }
    const std::size_t edges = curves.size();
    for (const int f : faces) {
      for (ModelCurve& silhouette : silhouettes_of(shape.face(f), direction)) {
        // A silhouette along an edge (a seam on the outline, a part's
        // straight side) is that edge.
        const bool known = std::any_of(curves.begin(), curves.begin() + static_cast<std::ptrdiff_t>(edges),
                                       [&silhouette](const ModelCurve& c) { return same_line(c, silhouette); });
        if (!known) {
          curves.push_back(std::move(silhouette));
        }
      }
    }
    return curves;
  });
}

std::vector<ModelCurve> face_silhouettes(const TopoDS_Face& face, const gp_Dir& direction) {
  return detail::run("face silhouettes", [&] { return silhouettes_of(face, direction); });
}

} // namespace mitcad::geometry
