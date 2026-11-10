// SPDX-License-Identifier: MIT
// The taper of an emboss (mitcad#1547; emboss_taper.hpp): a region's loops
// moved in their plane, each edge's curve offset by OCCT and the corners
// where the offsets meet.

#include "emboss_taper.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <limits>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ElSLib.hxx>
#include <GeomAPI.hxx>
#include <GeomLib.hxx>
#include <Geom2dAPI_InterCurveCurve.hxx>
#include <Geom2dAPI_ProjectPointOnCurve.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2dConvert_ApproxCurve.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Circle.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_OffsetCurve.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BoundedCurve.hxx>
#include <Geom_Plane.hxx>
#include <IntRes2d_IntersectionPoint.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_DataMap.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>

#include "mitcad/geometry/user_error.hpp"

namespace mitcad::geometry::detail {

namespace {

constexpr double kPi = 3.14159265358979323846;

using Collapse = std::function<std::invalid_argument(const std::string&)>;

// An edge's curve moved: the offset curve, the parameters where the edge's
// own ends lie on it, how far it may be followed past them, and the range
// its image takes between the corners.
struct Moving {
  occ::handle<Geom2d_Curve> curve;
  double first = 0.0; // where the edge starts, along the curve
  double last = 0.0;  // where it ends
  double low = 0.0;   // how far the curve reaches before the start
  double high = 0.0;  // and after the end
  bool closed = false;
  double from = 0.0; // the image's range
  double to = 0.0;
};

// The curve of an edge in the plane's coordinates, without trims.
occ::handle<Geom2d_Curve> flat_curve(const TopoDS_Edge& edge, const gp_Pln& plane, double& first, double& last) {
  TopLoc_Location location;
  occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
  if (curve.IsNull()) {
    throw std::runtime_error("an edge of the profile has no curve");
  }
  if (!location.IsIdentity()) {
    curve = occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
  }
  occ::handle<Geom2d_Curve> flat = GeomAPI::To2d(curve, plane);
  for (occ::handle<Geom2d_TrimmedCurve> trimmed = occ::down_cast<Geom2d_TrimmedCurve>(flat); !trimmed.IsNull();
       trimmed = occ::down_cast<Geom2d_TrimmedCurve>(flat)) {
    flat = trimmed->BasisCurve();
  }
  return flat;
}

// A 2D B-spline continued past both its ends by `length` along its
// tangents, smoothly (OCCT's curve extension, on its copy in the plane z =
// 0).
occ::handle<Geom2d_BSplineCurve> extended(const occ::handle<Geom2d_BSplineCurve>& spline, double length) {
  NCollection_Array1<gp_Pnt> poles(1, spline->NbPoles());
  for (int i = 1; i <= spline->NbPoles(); ++i) {
    poles(i) = gp_Pnt(spline->Pole(i).X(), spline->Pole(i).Y(), 0.0);
  }
  const occ::handle<Geom_BSplineCurve> space =
      spline->IsRational()
          ? new Geom_BSplineCurve(poles, *spline->Weights(), spline->Knots(), spline->Multiplicities(),
                                  spline->Degree())
          : new Geom_BSplineCurve(poles, spline->Knots(), spline->Multiplicities(), spline->Degree());
  occ::handle<Geom_BoundedCurve> bounded(space);
  for (const bool after : {true, false}) {
    gp_Pnt2d end;
    gp_Vec2d tangent;
    spline->D1(after ? spline->LastParameter() : spline->FirstParameter(), end, tangent);
    if (tangent.Magnitude() < 1.0e-12) {
      continue;
    }
    tangent.Normalize();
    if (!after) {
      tangent.Reverse();
    }
    const gp_Pnt2d to = end.Translated(tangent * length);
    GeomLib::ExtendCurveToPoint(bounded, gp_Pnt(to.X(), to.Y(), 0.0), 1, after);
  }
  const occ::handle<Geom_BSplineCurve> grown = occ::down_cast<Geom_BSplineCurve>(bounded);
  if (grown.IsNull()) {
    throw std::runtime_error("an edge of the profile could not be continued");
  }
  NCollection_Array1<gp_Pnt2d> flat(1, grown->NbPoles());
  for (int i = 1; i <= grown->NbPoles(); ++i) {
    flat(i) = gp_Pnt2d(grown->Pole(i).X(), grown->Pole(i).Y());
  }
  if (grown->IsRational()) {
    return new Geom2d_BSplineCurve(flat, *grown->Weights(), grown->Knots(), grown->Multiplicities(),
                                   grown->Degree());
  }
  return new Geom2d_BSplineCurve(flat, grown->Knots(), grown->Multiplicities(), grown->Degree());
}

// The parameter of a point on a curve (it lies on it).
double parameter_of(const occ::handle<Geom2d_Curve>& curve, const gp_Pnt2d& point) {
  Geom2dAPI_ProjectPointOnCurve projection(point, curve);
  if (projection.NbPoints() == 0) {
    throw std::runtime_error("an edge of the profile could not be continued");
  }
  return projection.LowerDistanceParameter();
}

// The edge's curve offset by `distance` to the side away from the
// material, continued `reach` past its ends where it can be. `edge` is
// oriented as the face holds it (its orientation and the face's together):
// the material lies to its left about the face's normal.
Moving moving(const TopoDS_Edge& edge, const TopoDS_Face& face, const gp_Pln& plane, double distance, double reach,
              const Collapse& collapsed) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> curve = flat_curve(edge, plane, first, last);
  Moving result;
  result.closed = BRep_Tool::IsClosed(edge);
  // The side away from the material: right of the edge as the face runs
  // it, in the plane's coordinates when they turn about the face's normal
  // (a plane of a left-handed frame, or a face against its plane's
  // normal, mirrors that).
  gp_Pnt2d middle;
  gp_Vec2d tangent;
  curve->D1((first + last) / 2.0, middle, tangent);
  if (tangent.Magnitude() < 1.0e-12) {
    throw std::runtime_error("an edge of the profile has no direction");
  }
  const gp_Vec2d right = gp_Vec2d(tangent.Y(), -tangent.X()).Normalized();
  const bool runs = edge.Orientation() != TopAbs_REVERSED;
  const bool turns = plane.Position().Direct() == (face.Orientation() != TopAbs_REVERSED);
  const gp_Vec2d outwards = runs == turns ? right : -right;
  // Geom2d_OffsetCurve's side for a positive offset.
  const occ::handle<Geom2d_OffsetCurve> unit = new Geom2d_OffsetCurve(curve, 1.0);
  const double side = gp_Vec2d(middle, unit->Value((first + last) / 2.0)).Dot(outwards) > 0.0 ? 1.0 : -1.0;
  const double offset = side * distance;

  const occ::handle<Geom2d_Line> line = occ::down_cast<Geom2d_Line>(curve);
  const occ::handle<Geom2d_Circle> circle = occ::down_cast<Geom2d_Circle>(curve);
  if (!line.IsNull()) {
    // A line moved across itself: its parameters stay.
    const gp_Vec2d across(curve->Value(first), unit->Value(first));
    result.curve = occ::down_cast<Geom2d_Curve>(line->Translated(across * offset));
    result.first = first;
    result.last = last;
    result.low = first - reach;
    result.high = last + reach;
  } else if (!circle.IsNull()) {
    // A circle about the same centre: its angles stay.
    const occ::handle<Geom2d_OffsetCurve> moved = new Geom2d_OffsetCurve(curve, offset);
    const gp_Pnt2d centre = circle->Circ2d().Location();
    const gp_Pnt2d on = circle->Value(first);
    const gp_Pnt2d there = moved->Value(first);
    // Through the centre and out the other side: the arc collapsed.
    const double radius = centre.Distance(there);
    if (gp_Vec2d(centre, on).Dot(gp_Vec2d(centre, there)) <= 0.0 ||
        radius <= 1.0e-7 * std::max(1.0, circle->Radius())) {
      throw collapsed("an arc of its top shrinks to nothing");
    }
    gp_Circ2d moved_circle = circle->Circ2d();
    moved_circle.SetRadius(radius);
    result.curve = new Geom2d_Circle(moved_circle);
    result.first = first;
    result.last = last;
    const double turn = result.closed ? 0.0 : std::min(reach / radius, (2.0 * kPi - (last - first)) / 2.0);
    result.low = first - turn;
    result.high = last + turn;
  } else {
    // Any other curve as a B-spline, continued, and its offset curve.
    const occ::handle<Geom2d_BSplineCurve> spline =
        Geom2dConvert::CurveToBSplineCurve(new Geom2d_TrimmedCurve(curve, first, last));
    occ::handle<Geom2d_Curve> basis(spline);
    double from = spline->FirstParameter();
    double to = spline->LastParameter();
    if (!result.closed) {
      const occ::handle<Geom2d_BSplineCurve> longer = extended(spline, reach);
      from = parameter_of(longer, spline->Value(spline->FirstParameter()));
      to = parameter_of(longer, spline->Value(spline->LastParameter()));
      basis = longer;
    }
    result.curve = new Geom2d_OffsetCurve(basis, offset);
    result.first = from;
    result.last = to;
    result.low = basis->FirstParameter();
    result.high = basis->LastParameter();
  }
  result.from = result.first;
  result.to = result.last;
  return result;
}

} // namespace

std::string region_list(const NameList& regions) {
  std::string text;
  for (const std::string& region : regions) {
    text += (text.empty() ? "" : ", ") + region;
  }
  return text;
}

MovedLoops moved_loops(const TopoDS_Face& face, double distance, const std::string& regions) {
  const Collapse collapsed = [&regions](const std::string& why) {
    return std::invalid_argument("the taper collapses the profile " + regions + ": " + why);
  };
  if (distance == 0.0) {
    throw std::invalid_argument("a taper moves the profile's edges");
  }
  const occ::handle<Geom_Plane> surface = occ::down_cast<Geom_Plane>(BRep_Tool::Surface(face));
  if (surface.IsNull()) {
    throw std::invalid_argument("a tapered profile must be planar");
  }
  const gp_Pln plane = surface->Pln();
  Bnd_Box box;
  BRepBndLib::Add(face, box);
  const double size = box.IsVoid() ? 1.0 : std::max(1.0, std::sqrt(box.SquareExtent()));
  // Where the offsets' ends count as one point, and how far they may be
  // followed to meet.
  const double joint = 1.0e-7 * std::max(size, std::abs(distance));
  const double reach = 10.0 * std::abs(distance) + 1.0e-3 * size;

  using Moves = NCollection_DataMap<TopoDS_Shape, Moving, TopTools_ShapeMapHasher>;
  Moves moves;
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    if (!moves.IsBound(e.Current())) {
      moves.Bind(e.Current(), moving(TopoDS::Edge(e.Current()), face, plane, distance, reach, collapsed));
    }
  }
  // The corners: where each edge's offset meets the next one's, along each
  // loop; each edge's corners in its curve's direction.
  BRep_Builder builder;
  using Ends = NCollection_DataMap<TopoDS_Shape, std::pair<TopoDS_Vertex, TopoDS_Vertex>, TopTools_ShapeMapHasher>;
  Ends ends;
  MovedLoops result;
  // An image's range has turned when its corners passed each other.
  const auto turned = [](const Moving& m) {
    return !(m.to - m.from > 1.0e-9 * std::max(1.0, std::abs(m.last - m.first)));
  };
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    std::vector<TopoDS_Edge> loop;
    for (BRepTools_WireExplorer e(TopoDS::Wire(w.Current()), face); e.More(); e.Next()) {
      loop.push_back(e.Current());
    }
    const std::size_t n = loop.size();
    if (n == 1) {
      // A closed curve alone: no corners.
      const Moving& only = moves.Find(loop.front());
      const gp_Pnt2d at = only.curve->Value(only.first);
      TopoDS_Vertex vertex;
      builder.MakeVertex(vertex, ElSLib::Value(at.X(), at.Y(), plane), joint);
      ends.Bind(loop.front(), {vertex, vertex});
      continue;
    }
    // The corner after each edge still there, where its offset meets the
    // next one's; an edge whose corners pass each other is gone, and its
    // neighbours meet anew, until every edge left keeps a stretch.
    std::vector<bool> gone(n, false);
    std::vector<gp_Pnt2d> corners(n);
    for (std::size_t round = 0;; ++round) {
      std::vector<std::size_t> live;
      for (std::size_t i = 0; i < n; ++i) {
        if (!gone[i]) {
          live.push_back(i);
          Moving& m = moves.ChangeFind(loop[i]);
          m.from = m.first;
          m.to = m.last;
        }
      }
      if (live.size() < 2 || round > n) {
        throw collapsed("nothing is left of a loop of its top");
      }
      for (std::size_t k = 0; k < live.size(); ++k) {
        const TopoDS_Edge& a = loop[live[k]];
        const TopoDS_Edge& b = loop[live[(k + 1) % live.size()]];
        Moving& ma = moves.ChangeFind(a);
        Moving& mb = moves.ChangeFind(b);
        const bool a_forward = a.Orientation() != TopAbs_REVERSED;
        const bool b_forward = b.Orientation() != TopAbs_REVERSED;
        const gp_Pnt2d p = ma.curve->Value(a_forward ? ma.last : ma.first);
        const gp_Pnt2d q = mb.curve->Value(b_forward ? mb.first : mb.last);
        const bool neighbours = (live[k] + 1) % n == live[(k + 1) % live.size()];
        if (neighbours && p.Distance(q) <= joint) {
          // A smooth join: the offsets' ends meet.
          corners[live[k]] = gp_Pnt2d((p.XY() + q.XY()) / 2.0);
          continue;
        }
        const occ::handle<Geom2d_Curve> ca = new Geom2d_TrimmedCurve(ma.curve, ma.low, ma.high);
        const occ::handle<Geom2d_Curve> cb = new Geom2d_TrimmedCurve(mb.curve, mb.low, mb.high);
        const Geom2dAPI_InterCurveCurve crossing(ca, cb, 1.0e-9);
        const gp_Pnt2d between((p.XY() + q.XY()) / 2.0);
        double nearest = std::numeric_limits<double>::infinity();
        int best = 0;
        for (int c = 1; c <= crossing.NbPoints(); ++c) {
          const double d = crossing.Point(c).Distance(between);
          if (d < nearest) {
            nearest = d;
            best = c;
          }
        }
        if (best == 0) {
          throw collapsed("neighbouring edges of its top do not meet");
        }
        const IntRes2d_IntersectionPoint& point = crossing.Intersector().Point(best);
        corners[live[k]] = point.Value();
        (a_forward ? ma.to : ma.from) = point.ParamOnFirst();
        (b_forward ? mb.from : mb.to) = point.ParamOnSecond();
      }
      bool more = false;
      for (const std::size_t i : live) {
        if (turned(moves.Find(loop[i]))) {
          gone[i] = true;
          more = true;
        }
      }
      if (!more) {
        break;
      }
    }
    // The corners as vertices, each edge's in its curve's direction; a
    // gone edge's top is the corner its neighbours meet at.
    std::vector<TopoDS_Vertex> vertices(n);
    for (std::size_t i = 0; i < n; ++i) {
      if (!gone[i]) {
        builder.MakeVertex(vertices[i], ElSLib::Value(corners[i].X(), corners[i].Y(), plane), joint);
      }
    }
    // The corner before edge i: the one after the edge left before it,
    // where the next edge left starts.
    const auto before = [&](std::size_t i) {
      std::size_t previous = (i + n - 1) % n;
      while (gone[previous]) {
        previous = (previous + n - 1) % n;
      }
      return vertices[previous];
    };
    for (std::size_t i = 0; i < n; ++i) {
      if (gone[i]) {
        result.vanished.Bind(loop[i], before(i));
        continue;
      }
      const TopoDS_Vertex& start = before(i);
      const TopoDS_Vertex& end = vertices[i];
      if (loop[i].Orientation() != TopAbs_REVERSED) {
        ends.Bind(loop[i], {start, end});
      } else {
        ends.Bind(loop[i], {end, start});
      }
    }
  }
  // The images: each offset between its corners.
  std::vector<std::pair<TopoDS_Shape, occ::handle<Geom2d_Curve>>> images;
  for (Moves::Iterator it(moves); it.More(); it.Next()) {
    if (result.vanished.IsBound(it.Key())) {
      continue;
    }
    const Moving& m = it.Value();
    if (turned(m)) {
      throw collapsed("an edge of its top shrinks to nothing or turns over");
    }
    const auto& [v0, v1] = ends.Find(it.Key());
    // An offset curve as OCCT's B-spline of it, within 1e-9 mm: lines
    // and circles stay as they are.
    occ::handle<Geom2d_Curve> curve = m.curve;
    double from = m.from;
    double to = m.to;
    if (curve->IsKind(STANDARD_TYPE(Geom2d_OffsetCurve))) {
      Geom2dConvert_ApproxCurve approximation(new Geom2d_TrimmedCurve(m.curve, m.from, m.to), 1.0e-9, GeomAbs_C2,
                                              1000, 14);
      if (!approximation.HasResult()) {
        throw std::runtime_error(placed_error("a tapered edge of the profile could not be built",
                                              "a tapered edge of the profile " + regions + " could not be built"));
      }
      curve = approximation.Curve();
      from = curve->FirstParameter();
      to = curve->LastParameter();
    }
    TopoDS_Edge image;
    builder.MakeEdge(image, GeomAPI::To3d(curve, plane), joint);
    builder.Add(image, v0.Oriented(TopAbs_FORWARD));
    builder.Add(image, v1.Oriented(TopAbs_REVERSED));
    builder.Range(image, from, to);
    result.moved.Bind(it.Key(), image);
    images.emplace_back(it.Key(), new Geom2d_TrimmedCurve(curve, from, to));
  }
  // No two moved edges cross, but where they share a corner.
  for (std::size_t i = 0; i < images.size(); ++i) {
    for (std::size_t j = i + 1; j < images.size(); ++j) {
      const Geom2dAPI_InterCurveCurve crossing(images[i].second, images[j].second, 1.0e-9);
      // Pairs, not structured bindings: a lambda cannot capture those in
      // C++17.
      const std::pair<TopoDS_Vertex, TopoDS_Vertex>& first_ends = ends.Find(images[i].first);
      const std::pair<TopoDS_Vertex, TopoDS_Vertex>& second_ends = ends.Find(images[j].first);
      const auto shared = [&](const gp_Pnt2d& p) {
        for (const TopoDS_Vertex& a : {first_ends.first, first_ends.second}) {
          for (const TopoDS_Vertex& b : {second_ends.first, second_ends.second}) {
            if (a.IsSame(b) && ElSLib::Value(p.X(), p.Y(), plane).Distance(BRep_Tool::Pnt(a)) <= 10.0 * joint) {
              return true;
            }
          }
        }
        return false;
      };
      for (int k = 1; k <= crossing.NbPoints(); ++k) {
        if (!shared(crossing.Point(k))) {
          throw collapsed("the edges of its top cross");
        }
      }
      if (crossing.NbSegments() > 0) {
        throw collapsed("the edges of its top overlap");
      }
    }
  }
  // The face of the images, its loops as the face's.
  TopoDS_Face moved;
  builder.MakeFace(moved, surface, joint);
  for (TopoDS_Iterator w(face, false, false); w.More(); w.Next()) {
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    for (TopoDS_Iterator e(w.Value(), false, false); e.More(); e.Next()) {
      if (!result.vanished.IsBound(e.Value())) {
        builder.Add(wire, result.moved.Find(e.Value()).Oriented(e.Value().Orientation()));
      }
    }
    wire.Closed(true);
    wire.Orientation(w.Value().Orientation());
    builder.Add(moved, wire);
  }
  moved.Orientation(face.Orientation());
  result.face = moved;
  return result;
}

} // namespace mitcad::geometry::detail
