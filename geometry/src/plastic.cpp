// SPDX-License-Identifier: MIT
#include "mitcad/geometry/plastic.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Circle.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Line.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>

#include "mitcad/geometry/user_error.hpp"

#include "face_select.hpp"
#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Edges of a loop and the keys of the band's faces along them.
using Keys = std::vector<std::pair<TopoDS_Edge, std::string>>;

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

struct Loop {
  TopoDS_Wire wire;
  Keys keys;
};

// The region a loop bounds in the plane, its boundary counter-clockwise
// about the plane's normal (material inside).
TopoDS_Face loop_face(const gp_Pln& plane, const TopoDS_Wire& wire) {
  for (int attempt = 0; attempt < 2; ++attempt) {
    const TopoDS_Wire w = attempt == 0 ? wire : TopoDS::Wire(wire.Reversed());
    BRepBuilderAPI_MakeFace maker(plane, w, true);
    if (!maker.IsDone()) {
      continue;
    }
    const TopoDS_Face face = maker.Face();
    BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
    if (classifier.PerformInfinitePoint() != TopAbs_IN) {
      return face;
    }
  }
  throw std::runtime_error("a loop of the rim face bounds no region");
}

double area(const TopoDS_Face& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return std::abs(props.Mass());
}

// Whether the region of an inner loop of the rim face is an opening: just
// inside the loop and over the face there is no material (the foot of a
// boss standing on the face has it), at the middle of each of its first
// edges.
bool is_opening(const Shape& body, const TopoDS_Face& region, const gp_Dir& normal, double probe) {
  int tested = 0;
  for (TopExp_Explorer it(region, TopAbs_EDGE); it.More() && tested < 4; it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    gp_Pnt middle;
    gp_Vec tangent;
    curve.D1((curve.FirstParameter() + curve.LastParameter()) / 2.0, middle, tangent);
    if (tangent.Magnitude() <= Precision::Confusion()) {
      continue;
    }
    if (edge.Orientation() == TopAbs_REVERSED) {
      tangent.Reverse();
    }
    // The region is on the left of its boundary.
    const gp_Vec left = gp_Vec(normal).Crossed(tangent).Normalized();
    const gp_Pnt over = middle.Translated(left * probe + gp_Vec(normal) * probe);
    BRepClass3d_SolidClassifier classifier(body.occt(), over, Precision::Confusion());
    if (classifier.State() != TopAbs_OUT) {
      return false;
    }
    ++tested;
  }
  return tested > 0;
}


// The loop of `face` moved `d` out of its region (into it for d < 0),
// the moved edges keyed as the edges they come from.
Loop offset_loop(const gp_Pln& plane, const TopoDS_Face& face, const Keys& keys, double d) {
  if (std::abs(d) <= Precision::Confusion()) {
    return {BRepTools::OuterWire(face), keys};
  }
  const double before = area(face);
  std::string problem = "the loop could not be offset";
  // OCCT's sign of an offset depends on the loop's direction: the one
  // that makes the region grow for d > 0 is taken.
  for (const double sign : {1.0, -1.0}) {
    BRepOffsetAPI_MakeOffset maker(face, GeomAbs_Intersection);
    maker.Perform(sign * d);
    if (!maker.IsDone()) {
      continue;
    }
    std::vector<TopoDS_Wire> wires;
    for (TopExp_Explorer it(maker.Shape(), TopAbs_WIRE); it.More(); it.Next()) {
      wires.push_back(TopoDS::Wire(it.Current()));
    }
    if (wires.size() != 1) {
      problem = "the offset loop falls apart; make the band narrower";
      continue;
    }
    const TopoDS_Face moved = loop_face(plane, wires.front());
    if ((area(moved) > before) != (d > 0.0)) {
      continue;
    }
    Keys out;
    for (const auto& [edge, key] : keys) {
      for (const TopoDS_Shape& made : maker.Generated(edge)) {
        if (made.ShapeType() == TopAbs_EDGE) {
          out.emplace_back(TopoDS::Edge(made), key);
        }
      }
    }
    return {BRepTools::OuterWire(moved), out};
  }
  throw std::invalid_argument(problem);
}

const std::string* key_of(const Keys& keys, const TopoDS_Shape& edge) {
  for (const auto& [e, key] : keys) {
    if (e.IsSame(edge)) {
      return &key;
    }
  }
  return nullptr;
}

// The rim face, its plane (the normal out of the body) and the loop the
// band follows, its edges keyed, with the region it bounds in the plane.
struct Rim {
  TopoDS_Face face;
  gp_Dir normal;
  gp_Pln plane;
  TopoDS_Wire loop;
  Keys keys;
  TopoDS_Face region;
};

Rim rim_of(const RimBandSpec& spec) {
  const Shape& body = *spec.body;
  const std::vector<int> found = body.find_faces(spec.face);
  if (found.size() != 1) {
    throw std::invalid_argument(found.empty() ? no_face_error(spec.face)
                                              : face_error(spec.face, "the rim face is in several pieces",
                                                           "the rim face " + spec.face + " is in several pieces"));
  }
  Rim rim;
  rim.face = body.face(found.front());
  const BRepAdaptor_Surface surface(rim.face);
  if (surface.GetType() != GeomAbs_Plane) {
    throw std::invalid_argument("the rim face must be planar");
  }
  rim.normal = detail::planar_normal(rim.face, surface.Plane());
  rim.plane = gp_Pln(surface.Plane().Location(), rim.normal);
  Bnd_Box box;
  BRepBndLib::Add(body.occt(), box);
  const double size = box.IsVoid() ? 1.0 : std::sqrt(box.SquareExtent());
  const double probe = std::max(1e-3, size * 1e-4);

  const TopoDS_Wire outer = BRepTools::OuterWire(rim.face);
  rim.loop = outer;
  if (!spec.outside) {
    // The openings: inner loops with no material over the face inside them.
    std::vector<std::pair<double, TopoDS_Wire>> openings;
    int inner = 0;
    for (TopExp_Explorer it(rim.face, TopAbs_WIRE); it.More(); it.Next()) {
      const TopoDS_Wire wire = TopoDS::Wire(it.Current());
      if (wire.IsSame(outer)) {
        continue;
      }
      ++inner;
      const TopoDS_Face region = loop_face(rim.plane, wire);
      if (is_opening(body, region, rim.normal, probe)) {
        openings.emplace_back(area(region), wire);
      }
    }
    if (inner == 0) {
      throw std::invalid_argument(
          "pick the rim face of a shelled body (a face with an inner and an outer edge)");
    }
    if (openings.empty()) {
      throw std::invalid_argument("pick the rim face of a shelled body: this face's inner edges are "
                                  "where other features stand on it, not an opening");
    }
    std::sort(openings.begin(), openings.end(),
              [](const auto& a, const auto& b) { return a.first > b.first; });
    if (openings.size() > 1 && openings[1].first > 0.5 * openings[0].first) {
      throw std::invalid_argument("pick the rim face of a shelled body (round one opening): this face has " +
                                  std::to_string(openings.size()) + " openings alike in size");
    }
    rim.loop = openings.front().second;
  }

  int k = 0;
  for (TopExp_Explorer it(rim.loop, TopAbs_EDGE); it.More(); it.Next(), ++k) {
    const std::optional<std::string> name = body.name_of_edge(it.Current());
    rim.keys.emplace_back(TopoDS::Edge(it.Current()), name ? *name : std::to_string(k));
  }
  rim.region = loop_face(rim.plane, rim.loop);
  return rim;
}

// The band with walls along the normal: the band face between the loop's
// offsets, swept from `start` to `end`.
ShapePtr straight_band(const RimBandSpec& spec, const Rim& rim) {
  // Into the face is out of the opening's region and into the outer
  // loop's.
  const double out = spec.outside ? spec.free : spec.width;
  const double in = spec.outside ? spec.width : spec.free;
  const Loop big = offset_loop(rim.plane, rim.region, rim.keys, out);
  const Loop small = offset_loop(rim.plane, rim.region, rim.keys, -in);
  const char* big_role = spec.outside ? "band_edge" : "band_side";
  const char* small_role = spec.outside ? "band_side" : "band_edge";

  BRepBuilderAPI_MakeFace maker(loop_face(rim.plane, big.wire));
  maker.Add(TopoDS::Wire(BRepTools::OuterWire(loop_face(rim.plane, small.wire)).Reversed()));
  if (!maker.IsDone() || !BRepCheck_Analyzer(maker.Face()).IsValid()) {
    throw std::invalid_argument("the band does not fit on the rim face; make it narrower");
  }
  const TopoDS_Face band_face = maker.Face();
  std::vector<std::pair<TopoDS_Edge, std::string>> names;
  int unnamed = 0;
  for (TopExp_Explorer it(band_face, TopAbs_EDGE); it.More(); it.Next()) {
    std::string name;
    if (const std::string* key = key_of(big.keys, it.Current())) {
      name = face_name(spec.feature, big_role, *key);
    } else if (const std::string* other = key_of(small.keys, it.Current())) {
      name = face_name(spec.feature, small_role, *other);
    } else {
      name = face_name(spec.feature, "band_wall", std::to_string(unnamed++));
    }
    names.emplace_back(TopoDS::Edge(it.Current()), name);
  }

  gp_Trsf move;
  move.SetTranslation(gp_Vec(rim.normal) * spec.start);
  BRepBuilderAPI_Transform moved(band_face, move, true);
  std::vector<std::pair<TopoDS_Edge, std::string>> moved_names;
  for (const auto& [edge, name] : names) {
    moved_names.emplace_back(TopoDS::Edge(moved.ModifiedShape(edge)), name);
  }
  const detail::SweepFace sweep{TopoDS::Face(moved.Shape()), moved_names, {}};
  const ShapePtr tool = detail::named_prism(sweep, gp_Vec(rim.normal) * (spec.end - spec.start),
                                            {face_name(spec.feature, "band_bottom")},
                                            {face_name(spec.feature, "band_top")});
  return detail::finished(*tool, "the band");
}

// ---------------------------------------------------------------------------
// The band with shaped walls (mitcad#2511): the wall along the loop
// following the faces beside it, the other one leaning.

// A point of the loop with what moves it: where it is at the face, the
// unit direction into the rim face across the loop in its plane, and the
// line that continues the face beside the loop there, as a vector that
// rises 1 along the normal (zero when the wall rises straight).
struct LoopPoint {
  double t = 0.0;
  gp_Pnt at;
  gp_Vec into;
  gp_Vec rise;
};

// An edge of the loop as its region's boundary runs (its orientation
// there), with its points from its first parameter to its last.
struct LoopEdge {
  TopoDS_Edge edge;
  std::string key;
  std::string beside;
  bool line = false;
  bool circle = false;
  gp_Lin lin;
  gp_Circ circ;
  double first = 0.0;
  double last = 0.0;
  std::vector<LoopPoint> points;
};

constexpr double kDegree = 3.14159265358979323846 / 180.0;

// The name of a face of the body for an error's place; empty without one.
std::string name_of(const Shape& body, const TopoDS_Shape& face) {
  const NameList& names = body.names_of_face(face);
  return names.empty() ? std::string() : names.front();
}

// The places of an error about the rim face and the face beside it.
std::vector<ErrorPlace> rim_places(const RimBandSpec& spec, const std::string& beside) {
  std::vector<ErrorPlace> places{face_place(spec.face)};
  if (!beside.empty()) {
    places.push_back(face_place(beside));
  }
  return places;
}

// The line that continues `face` (beside the loop's edge) at the edge's
// point of parameter t along its tangent, at right angles to the edge, as
// a vector rising 1 along `normal`. Throws when the face runs along the
// rim face there.
gp_Vec rise_at(const RimBandSpec& spec, const TopoDS_Edge& edge, const TopoDS_Face& face, const std::string& name,
               double t, const gp_Pnt& at, const gp_Vec& tangent, const gp_Dir& normal) {
  double f = 0.0;
  double l = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, f, l);
  gp_Pnt2d uv;
  if (!pcurve.IsNull()) {
    uv = pcurve->Value(t);
  } else {
    GeomAPI_ProjectPointOnSurf project(at, BRep_Tool::Surface(face));
    if (project.NbPoints() == 0) {
      throw std::invalid_argument(placed_error(
          "the face beside the rim could not be followed", "the face " + name + " beside the rim face " + spec.face +
                                                                " has no point at its edge to follow",
          rim_places(spec, name)));
    }
    double u = 0.0;
    double v = 0.0;
    project.LowerDistanceParameters(u, v);
    uv = gp_Pnt2d(u, v);
  }
  const BRepAdaptor_Surface surface(face, false);
  gp_Pnt point;
  gp_Vec du;
  gp_Vec dv;
  surface.D1(uv.X(), uv.Y(), point, du, dv);
  const gp_Vec across = du.Crossed(dv).Crossed(tangent);
  const double up = across.Dot(gp_Vec(normal));
  if (across.Magnitude() <= Precision::Confusion() || std::abs(up) <= std::sin(1.0 * kDegree) * across.Magnitude()) {
    throw std::invalid_argument(placed_error(
        "the face beside the rim runs along the rim face; the band cannot follow it",
        "the face " + name + " beside the rim face " + spec.face +
            " meets it tangentially or has no direction across the edge; the band cannot follow it",
        rim_places(spec, name)));
  }
  return across / up;
}

// The loop's edges as its region's boundary runs, each with its points:
// where they move to follow the faces beside them (with `tangent`), and the
// direction into the rim face.
std::vector<LoopEdge> loop_edges(const RimBandSpec& spec, const Rim& rim) {
  const Shape& body = *spec.body;
  Ancestors ancestors;
  TopExp::MapShapesAndAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  // Into the rim face: out of the opening's region, into the outer loop's
  // (the region lies to the left of its boundary about the normal).
  const double into_sign = spec.outside ? 1.0 : -1.0;
  std::vector<LoopEdge> edges;
  const TopoDS_Wire boundary = BRepTools::OuterWire(rim.region);
  for (BRepTools_WireExplorer it(boundary, rim.region); it.More(); it.Next()) {
    LoopEdge loop_edge;
    loop_edge.edge = it.Current();
    if (BRep_Tool::Degenerated(loop_edge.edge)) {
      continue;
    }
    const std::string* key = key_of(rim.keys, loop_edge.edge);
    loop_edge.key = key ? *key : std::to_string(edges.size());
    TopoDS_Face beside;
    if (spec.tangent) {
      const int index = ancestors.FindIndex(loop_edge.edge);
      if (index > 0) {
        for (const TopoDS_Shape& face : ancestors.FindFromIndex(index)) {
          if (!face.IsSame(rim.face)) {
            beside = TopoDS::Face(face);
            break;
          }
        }
      }
      if (beside.IsNull()) {
        throw std::invalid_argument(placed_error("the rim's edge has no face beside it to follow",
                                                 "an edge of the rim face " + spec.face +
                                                     " has no other face of the body to follow",
                                                 rim_places(spec, std::string())));
      }
      loop_edge.beside = name_of(body, beside);
    }
    const BRepAdaptor_Curve curve(loop_edge.edge);
    loop_edge.first = curve.FirstParameter();
    loop_edge.last = curve.LastParameter();
    loop_edge.line = curve.GetType() == GeomAbs_Line;
    loop_edge.circle = curve.GetType() == GeomAbs_Circle;
    if (loop_edge.line) {
      loop_edge.lin = curve.Line();
    }
    if (loop_edge.circle) {
      loop_edge.circ = curve.Circle();
    }
    const bool reversed = loop_edge.edge.Orientation() == TopAbs_REVERSED;
    const int n = loop_edge.line || loop_edge.circle ? 16 : 48;
    for (int i = 0; i <= n; ++i) {
      LoopPoint point;
      point.t = loop_edge.first + (loop_edge.last - loop_edge.first) * static_cast<double>(i) / n;
      gp_Vec tangent;
      curve.D1(point.t, point.at, tangent);
      if (tangent.Magnitude() <= Precision::Confusion()) {
        throw std::invalid_argument(
            placed_error("an edge of the rim has no direction somewhere; the band cannot follow it",
                         "an edge of the rim face " + spec.face + " has a zero tangent", rim_places(spec, std::string())));
      }
      const gp_Vec along = reversed ? tangent.Reversed() : tangent;
      point.into = gp_Vec(rim.normal).Crossed(along).Normalized() * into_sign;
      point.rise = spec.tangent ? rise_at(spec, loop_edge.edge, beside, loop_edge.beside, point.t, point.at, tangent,
                                          rim.normal)
                                : gp_Vec(0.0, 0.0, 0.0);
      loop_edge.points.push_back(point);
    }
    edges.push_back(std::move(loop_edge));
  }
  if (edges.empty()) {
    throw std::invalid_argument("the rim face's loop has no edges");
  }
  return edges;
}

// The band must stay open: at its ends (heights `start` and `end`) the
// leaning wall lies further into the face than the wall along the loop.
void check_open(const RimBandSpec& spec, const std::vector<LoopEdge>& edges) {
  for (const double z : {spec.start, spec.end}) {
    const double side = spec.width - z * std::tan(spec.side_taper);
    for (const LoopEdge& edge : edges) {
      for (const LoopPoint& point : edge.points) {
        if (side - z * point.rise.Dot(point.into) <= 1.0e-6) {
          throw std::invalid_argument(placed_error(
              "the band's walls meet: make it wider, lower or its draft smaller",
              "the band along the rim face " + spec.face + " closes at a height of " + std::to_string(z) +
                  " mm: its leaning wall reaches the wall that follows the face beside the rim",
              rim_places(spec, edge.beside)));
        }
      }
    }
  }
}

// An edge of a moved loop: its curve along its own parameter from `first`
// to `last` and its ends and end tangents as the loop runs.
struct MovedEdge {
  occ::handle<Geom_Curve> curve;
  double first = 0.0;
  double last = 0.0;
  bool line = false;
  bool reversed = false;
  gp_Pnt start;
  gp_Pnt end;
  gp_Vec start_tangent;
  gp_Vec end_tangent;
};

gp_Pnt moved_point(const LoopPoint& point, double z, double offset) {
  return point.at.Translated(point.rise * z + point.into * offset);
}

bool same_vectors(const std::vector<LoopPoint>& points, gp_Vec LoopPoint::*member) {
  const gp_Vec& v0 = points.front().*member;
  for (const LoopPoint& point : points) {
    if ((point.*member - v0).Magnitude() > 1.0e-9 * (1.0 + v0.Magnitude())) {
      return false;
    }
  }
  return true;
}

// The loop's edge moved to height z, `offset` into the rim face: a line
// or a circle where it stays one, else the curve through its moved
// points at their parameters.
MovedEdge moved_edge(const LoopEdge& edge, double z, double offset) {
  MovedEdge out;
  out.first = edge.first;
  out.last = edge.last;
  out.reversed = edge.edge.Orientation() == TopAbs_REVERSED;
  const LoopPoint& p0 = edge.points.front();
  if (edge.line && same_vectors(edge.points, &LoopPoint::rise) && same_vectors(edge.points, &LoopPoint::into)) {
    const gp_Vec shift = p0.rise * z + p0.into * offset;
    out.curve = new Geom_Line(edge.lin.Translated(shift));
    out.line = true;
  }
  if (out.curve.IsNull() && edge.circle) {
    // A circle about the same axis where every point moves alike.
    const gp_Ax2 axes = edge.circ.Position();
    const gp_Vec axis(axes.Direction());
    const gp_Pnt q0 = moved_point(p0, z, offset);
    const gp_Pnt centre = edge.circ.Location().Translated(axis * gp_Vec(edge.circ.Location(), q0).Dot(axis));
    const double radius = centre.Distance(q0);
    if (radius > Precision::Confusion()) {
      const gp_Circ circle(gp_Ax2(centre, axes.Direction(), axes.XDirection()), radius);
      const occ::handle<Geom_Circle> geom = new Geom_Circle(circle);
      bool on = true;
      for (const LoopPoint& point : edge.points) {
        if (geom->Value(point.t).Distance(moved_point(point, z, offset)) > 1.0e-7) {
          on = false;
          break;
        }
      }
      if (on) {
        out.curve = geom;
      }
    }
  }
  if (out.curve.IsNull()) {
    const int n = static_cast<int>(edge.points.size());
    occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, n);
    occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n);
    for (int i = 0; i < n; ++i) {
      const LoopPoint& point = edge.points[static_cast<std::size_t>(i)];
      points->SetValue(i + 1, moved_point(point, z, offset));
      params->SetValue(i + 1, point.t);
    }
    GeomAPI_Interpolate interpolate(points, params, false, 1.0e-9);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error("the band's wall along a curved edge of the rim could not be built");
    }
    out.curve = interpolate.Curve();
  }
  gp_Pnt a;
  gp_Pnt b;
  gp_Vec ta;
  gp_Vec tb;
  out.curve->D1(out.first, a, ta);
  out.curve->D1(out.last, b, tb);
  if (out.reversed) {
    out.start = b;
    out.end = a;
    out.start_tangent = tb.Reversed();
    out.end_tangent = ta.Reversed();
  } else {
    out.start = a;
    out.end = b;
    out.start_tangent = ta;
    out.end_tangent = tb;
  }
  if (out.start_tangent.Magnitude() <= Precision::Confusion() || out.end_tangent.Magnitude() <= Precision::Confusion()) {
    throw std::runtime_error("the band's wall along an edge of the rim has no direction at its end");
  }
  out.start_tangent.Normalize();
  out.end_tangent.Normalize();
  return out;
}

TopoDS_Vertex vertex_at(const gp_Pnt& point, double tolerance) {
  TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
  BRep_Builder().UpdateVertex(vertex, std::max(tolerance, Precision::Confusion()));
  return vertex;
}

TopoDS_Edge segment(const TopoDS_Vertex& from, const TopoDS_Vertex& to) {
  BRepBuilderAPI_MakeEdge maker(from, to);
  if (!maker.IsDone()) {
    throw std::invalid_argument("an edge of the band's wall along the rim vanishes at a corner");
  }
  return maker.Edge();
}

// The wire of the loop moved to height z, `offset` into the rim face,
// with the band_edge names of its edges. Where the moved edges meet at a
// corner with a gap or an overlap, lines meet where they cross and a
// curve goes on along its end tangent to where it crosses the next edge's;
// a curve that would have to be cut back is refused.
struct LevelWire {
  TopoDS_Wire wire;
  std::vector<std::pair<TopoDS_Edge, std::string>> names;
};

LevelWire level_wire(const RimBandSpec& spec, const Rim& rim, const std::vector<LoopEdge>& edges, double z,
                     double offset) {
  const std::size_t n = edges.size();
  std::vector<MovedEdge> moved;
  moved.reserve(n);
  for (const LoopEdge& edge : edges) {
    moved.push_back(moved_edge(edge, z, offset));
  }
  // Corner i joins edge i's end to edge i + 1's start.
  struct Corner {
    gp_Pnt at;
    double tolerance = 0.0;
    bool extend_end = false;   // edge i goes on along its tangent to `at`
    bool extend_start = false; // edge i + 1 comes from `at` along its tangent
  };
  std::vector<Corner> corners(n);
  const gp_Vec normal(rim.normal);
  for (std::size_t i = 0; i < n; ++i) {
    const MovedEdge& a = moved[i];
    const MovedEdge& b = moved[(i + 1) % n];
    Corner& corner = corners[i];
    const double gap = a.end.Distance(b.start);
    if (gap <= 1.0e-5) {
      corner.at = gp_Pnt((a.end.XYZ() + b.start.XYZ()) / 2.0);
      corner.tolerance = gap / 2.0 + Precision::Confusion();
      continue;
    }
    const auto refuse = [&]() {
      return std::invalid_argument(placed_error(
          "the band cannot follow the faces beside the rim round one of its corners",
          "at a sharp corner of the rim face " + spec.face +
              " the band's wall along the loop cannot be joined (a curved edge would have to be cut back)",
          rim_places(spec, edges[i].beside)));
    };
    const double cross = a.end_tangent.Crossed(b.start_tangent).Dot(normal);
    if (std::abs(cross) <= std::sin(1.0 * kDegree)) {
      throw refuse();
    }
    const gp_Vec between(a.end, b.start);
    const double la = between.Crossed(b.start_tangent).Dot(normal) / cross;
    const double lb = between.Crossed(a.end_tangent).Dot(normal) / cross;
    if (std::abs(la) > 10.0 * gap || std::abs(lb) > 10.0 * gap) {
      throw refuse();
    }
    corner.at = a.end.Translated(a.end_tangent * la);
    corner.tolerance = Precision::Confusion();
    if (!a.line) {
      if (la < -1.0e-7) {
        throw refuse();
      }
      corner.extend_end = la > 1.0e-7;
    }
    if (!b.line) {
      if (lb > 1.0e-7) {
        throw refuse();
      }
      corner.extend_start = lb < -1.0e-7;
    }
  }
  std::vector<TopoDS_Vertex> at(n);
  for (std::size_t i = 0; i < n; ++i) {
    at[i] = vertex_at(corners[i].at, corners[i].tolerance);
  }
  LevelWire out;
  BRepBuilderAPI_MakeWire wire;
  const auto add = [&](const TopoDS_Edge& edge, const std::string& key) {
    wire.Add(edge);
    if (!wire.IsDone()) {
      throw std::runtime_error("the band's wall along the rim could not be joined round the loop");
    }
    // The edge as the wire holds it (a copy where it merged a vertex).
    out.names.emplace_back(wire.Edge(), face_name(spec.feature, "band_edge", key));
  };
  for (std::size_t i = 0; i < n; ++i) {
    const MovedEdge& edge = moved[i];
    const Corner& before = corners[(i + n - 1) % n];
    const Corner& after = corners[i];
    const TopoDS_Vertex& from = at[(i + n - 1) % n];
    const TopoDS_Vertex& to = at[i];
    if (edge.line) {
      add(segment(from, to), edges[i].key);
      continue;
    }
    const TopoDS_Vertex start = before.extend_start ? vertex_at(edge.start, Precision::Confusion()) : from;
    const TopoDS_Vertex end = after.extend_end ? vertex_at(edge.end, Precision::Confusion()) : to;
    if (before.extend_start) {
      add(segment(from, start), edges[i].key);
    }
    // The curve between its vertices along its own parameter, then turned
    // the way the loop runs.
    BRepBuilderAPI_MakeEdge maker(edge.curve, edge.reversed ? end : start, edge.reversed ? start : end, edge.first,
                                  edge.last);
    if (!maker.IsDone()) {
      throw std::runtime_error("the band's wall along an edge of the rim could not be built");
    }
    add(edge.reversed ? TopoDS::Edge(maker.Edge().Reversed()) : maker.Edge(), edges[i].key);
    if (after.extend_end) {
      add(segment(end, to), edges[i].key);
    }
  }
  out.wire = wire.Wire();
  return out;
}

// The solid of the wall along the loop with `tangent`: ruled between the
// moved loop at z0 and at z1, its walls band_edge(<key>), its caps named
// `near` and `far`.
ShapePtr ruled_side(const RimBandSpec& spec, const Rim& rim, const std::vector<LoopEdge>& edges, double z0,
                    double z1, const NameList& near, const NameList& far) {
  const LevelWire low = level_wire(spec, rim, edges, z0, -spec.free);
  const LevelWire high = level_wire(spec, rim, edges, z1, -spec.free);
  if (low.names.size() != high.names.size()) {
    throw std::invalid_argument(placed_error(
        "the band cannot follow the faces beside the rim round one of its corners",
        "the band's wall along the loop of the rim face " + spec.face +
            " changes its corners between its bottom and its top",
        rim_places(spec, std::string())));
  }
  BRepOffsetAPI_ThruSections loft(true, true, 1.0e-6);
  loft.AddWire(low.wire);
  loft.AddWire(high.wire);
  loft.CheckCompatibility(false);
  detail::build(loft);
  if (!loft.IsDone()) {
    throw std::runtime_error("the band's wall along the faces beside the rim could not be built");
  }
  detail::FaceNamer namer(loft.Shape());
  for (const auto& [edge, name] : low.names) {
    const TopoDS_Shape face = loft.GeneratedFace(edge);
    if (!face.IsNull()) {
      namer.add(face, name);
    }
  }
  for (const std::string& name : near) {
    namer.add(loft.FirstShape(), name);
  }
  for (const std::string& name : far) {
    namer.add(loft.LastShape(), name);
  }
  return namer.shape();
}

// The planar face of a loop moved to height z, its edges named `role`
// after their keys and band_wall(<k>) where they have none.
detail::SweepFace face_at(const std::string& feature, const gp_Pln& plane, const Loop& loop, const char* role,
                          double z, int& walls) {
  const TopoDS_Face face = loop_face(plane, loop.wire);
  std::vector<std::pair<TopoDS_Edge, std::string>> names;
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    std::string name;
    if (const std::string* key = key_of(loop.keys, it.Current())) {
      name = face_name(feature, role, *key);
    } else {
      name = face_name(feature, "band_wall", std::to_string(walls++));
    }
    names.emplace_back(TopoDS::Edge(it.Current()), name);
  }
  gp_Trsf move;
  move.SetTranslation(gp_Vec(plane.Axis().Direction()) * z);
  BRepBuilderAPI_Transform moved(face, move, true);
  std::vector<std::pair<TopoDS_Edge, std::string>> moved_names;
  for (const auto& [edge, name] : names) {
    moved_names.emplace_back(TopoDS::Edge(moved.ModifiedShape(edge)), name);
  }
  return {TopoDS::Face(moved.Shape()), moved_names, {}};
}

// The solid whose boundary is the band's leaning wall, from z0 to z1: the
// loop's offset into the face by the width there, leaning about the face's
// plane.
ShapePtr leaning_side(const RimBandSpec& spec, const Rim& rim, double z0, double z1, const NameList& near,
                      const NameList& far, int& walls) {
  const double width = spec.width - z0 * std::tan(spec.side_taper);
  const Loop loop = offset_loop(rim.plane, rim.region, rim.keys, spec.outside ? -width : width);
  const detail::SweepFace face = face_at(spec.feature, rim.plane, loop, "band_side", z0, walls);
  // The region grows as the wall goes out of it: an opening's offset
  // shrinks as a lip narrows, an outer loop's inner offset grows.
  const double taper = spec.outside ? spec.side_taper : -spec.side_taper;
  return detail::distance_side(face, z1 - z0, taper, rim.normal, near, far);
}

// The solid whose boundary is the band's straight wall along the loop,
// from z0 to z1.
ShapePtr straight_side(const RimBandSpec& spec, const Rim& rim, double z0, double z1, const NameList& near,
                       const NameList& far, int& walls) {
  const Loop loop = offset_loop(rim.plane, rim.region, rim.keys, spec.outside ? spec.free : -spec.free);
  const detail::SweepFace face = face_at(spec.feature, rim.plane, loop, "band_edge", z0, walls);
  return detail::named_prism(face, gp_Vec(rim.normal) * (z1 - z0), near, far);
}

// `from` less `less`, the faces of both named as they were.
ShapePtr cut_band(const ShapePtr& from, const ShapePtr& less) {
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(from->occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(less->occt());
  cut.SetArguments(arguments);
  cut.SetTools(tools);
  cut.SetNonDestructive(true);
  detail::build(cut);
  if (!cut.IsDone() || cut.HasErrors()) {
    throw std::runtime_error("the band's walls could not be built");
  }
  detail::FaceNamer namer(cut.Shape());
  namer.carry(cut, *from);
  namer.carry(cut, *less);
  return detail::finished(*namer.shape(), "the band");
}

// The band with a wall that follows the faces beside the loop or a wall
// that leans: the solid inside the outer wall less the one inside the
// inner wall, the inner one running past the band's ends so that no caps
// meet.
ShapePtr shaped_band(const RimBandSpec& spec, const Rim& rim) {
  const std::vector<LoopEdge> edges = loop_edges(spec, rim);
  check_open(spec, edges);
  const NameList bottom{face_name(spec.feature, "band_bottom")};
  const NameList top{face_name(spec.feature, "band_top")};
  const double past = 0.25 * (spec.end - spec.start) + 1.0e-3;
  int walls = 0;
  const auto along_loop = [&](double z0, double z1) {
    return spec.tangent ? ruled_side(spec, rim, edges, z0, z1, bottom, top)
                        : straight_side(spec, rim, z0, z1, bottom, top, walls);
  };
  // Along an opening the leaning wall is the outer one; along the outer
  // edge the wall along the loop is.
  if (spec.outside) {
    // Past the band's ends the leaning wall goes on only while it stays
    // into the face: at least half the narrower end's width.
    const double lean = std::abs(std::tan(spec.side_taper));
    const double narrow = std::min(spec.width - spec.start * std::tan(spec.side_taper),
                                   spec.width - spec.end * std::tan(spec.side_taper));
    const double lean_past = lean > 1.0e-12 ? std::min(past, 0.5 * narrow / lean) : past;
    const ShapePtr outer = along_loop(spec.start, spec.end);
    const ShapePtr inner =
        leaning_side(spec, rim, spec.start - lean_past, spec.end + lean_past, bottom, top, walls);
    return cut_band(outer, inner);
  }
  const ShapePtr outer = leaning_side(spec, rim, spec.start, spec.end, bottom, top, walls);
  const ShapePtr inner = along_loop(spec.start - past, spec.end + past);
  return cut_band(outer, inner);
}

ShapePtr band(const RimBandSpec& spec) {
  const Rim rim = rim_of(spec);
  if (!spec.tangent && spec.side_taper == 0.0) {
    return straight_band(spec, rim);
  }
  return shaped_band(spec, rim);
}

} // namespace

ShapePtr rim_band(const RimBandSpec& spec) {
  if (!spec.body) {
    throw std::invalid_argument("the band needs a body");
  }
  detail::require_positive("band width", spec.width);
  detail::require_finite("band's free side", spec.free);
  if (spec.free < 0.0) {
    throw std::invalid_argument("the band's free side must not be negative");
  }
  detail::require_finite("band start", spec.start);
  detail::require_finite("band end", spec.end);
  if (!(spec.start < spec.end)) {
    throw std::invalid_argument("the band must end above its start");
  }
  detail::require_finite("band's side lean", spec.side_taper);
  if (std::abs(spec.side_taper) >= 85.0 * kDegree) {
    throw std::invalid_argument("the band's side must lean less than 85 degrees");
  }
  return detail::run("band", [&] { return band(spec); });
}

} // namespace mitcad::geometry
