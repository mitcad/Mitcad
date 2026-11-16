// SPDX-License-Identifier: MIT
#include "mitcad/geometry/rib.hpp"

#include <algorithm>
#include <cmath>
#include <memory>
#include <optional>
#include <stdexcept>

#include <Adaptor3d_TopolTool.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Contap_Contour.hxx>
#include <Contap_Line.hxx>
#include <ElSLib.hxx>
#include <GeomAPI_PointsToBSpline.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomAbs_CurveType.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Surface.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Pln.hxx>
#include <gp_Sphere.hxx>

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

constexpr double kPi = 3.14159265358979323846;

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The face a curve sweeps back against `along` by `length`, or a null
// shape when the sweep fails.
TopoDS_Shape swept_back(const TopoDS_Shape& curve, const gp_Dir& along, double length) {
  BRepPrimAPI_MakePrism prism(curve, gp_Vec(along) * -length);
  prism.Build();
  return prism.IsDone() ? prism.Shape() : TopoDS_Shape();
}

// The parameter range [first, last] of a face widened by a quarter of its
// span at each end: at most one period of a periodic surface, else within
// the surface's own bounds [low, high].
void widen(double& first, double& last, bool periodic, double period, double low, double high) {
  const double margin = (last - first) / 4.0;
  first -= margin;
  last += margin;
  if (periodic) {
    if (last - first > period) {
      const double middle = (first + last) / 2.0;
      first = middle - period / 2.0;
      last = middle + period / 2.0;
    }
    return;
  }
  first = std::max(first, low);
  last = std::min(last, high);
}

// The edge along a contour line traced by points on the surface: a
// B-spline through them, its tolerance raised to how far it strays from
// the surface so that the cuts take it as lying on the face; null when the
// points do not make a curve.
TopoDS_Edge traced_edge(const std::vector<gp_Pnt>& points, const occ::handle<Geom_Surface>& surface) {
  NCollection_Array1<gp_Pnt> array(1, static_cast<int>(points.size()));
  for (std::size_t i = 0; i < points.size(); ++i) {
    array.SetValue(static_cast<int>(i) + 1, points[i]);
  }
  GeomAPI_PointsToBSpline fit(array, 3, 8, GeomAbs_C2, 1.0e-6);
  if (!fit.IsDone() || fit.Curve().IsNull()) {
    return TopoDS_Edge();
  }
  const occ::handle<Geom_BSplineCurve> curve = fit.Curve();
  BRepBuilderAPI_MakeEdge maker(curve);
  if (!maker.IsDone()) {
    return TopoDS_Edge();
  }
  const TopoDS_Edge edge = maker.Edge();
  double off = 0.0;
  constexpr int samples = 100;
  const double t0 = curve->FirstParameter();
  const double t1 = curve->LastParameter();
  for (int i = 0; i <= samples; ++i) {
    GeomAPI_ProjectPointOnSurf projection(curve->Value(t0 + (t1 - t0) * i / samples), surface);
    if (projection.NbPoints() > 0) {
      off = std::max(off, projection.LowerDistance());
    }
  }
  const double tolerance = 1.5 * off;
  if (tolerance > BRep_Tool::Tolerance(edge)) {
    BRep_Builder builder;
    builder.UpdateEdge(edge, tolerance);
    for (TopExp_Explorer v(edge, TopAbs_VERTEX); v.More(); v.Next()) {
      if (BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())) < tolerance) {
        builder.UpdateVertex(TopoDS::Vertex(v.Current()), tolerance);
      }
    }
  }
  return edge;
}

// The outline of a curved face of any other kind (a torus, a freeform
// face) seen along `along` (mitcad#1865): the contour lines on its surface
// where the normal is at right angles to `along`, as OCCT's hidden-line
// removal traces them, over the face's parameter range widened a quarter
// each way. A line that does not close on itself is continued at both ends
// by lines along its tangents, long enough to cross everything, as the
// lines of cylinders and cones are. The curves are added as they are; the
// caller sweeps them.
std::vector<TopoDS_Edge> contours(const TopoDS_Face& face, const gp_Dir& along, double length) {
  std::vector<TopoDS_Edge> edges;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  if (surface.IsNull()) {
    return edges;
  }
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  double low_u = 0.0;
  double high_u = 0.0;
  double low_v = 0.0;
  double high_v = 0.0;
  surface->Bounds(low_u, high_u, low_v, high_v);
  widen(u0, u1, surface->IsUPeriodic(), surface->IsUPeriodic() ? surface->UPeriod() : 0.0, low_u, high_u);
  widen(v0, v1, surface->IsVPeriodic(), surface->IsVPeriodic() ? surface->VPeriod() : 0.0, low_v, high_v);
  if (u1 - u0 <= Precision::PConfusion() || v1 - v0 <= Precision::PConfusion()) {
    return edges;
  }
  // A line along `direction` from `from`, long enough to cross everything.
  const auto run_on = [&](const gp_Pnt& from, const gp_Vec& direction) {
    if (direction.Magnitude() > Precision::Confusion()) {
      edges.push_back(BRepBuilderAPI_MakeEdge(from, from.Translated(direction.Normalized() * length)).Edge());
    }
  };
  try {
    const occ::handle<Adaptor3d_Surface> adaptor = new GeomAdaptor_Surface(surface, u0, u1, v0, v1);
    const occ::handle<Adaptor3d_TopolTool> domain = new Adaptor3d_TopolTool(adaptor);
    Contap_Contour contour(adaptor, domain, gp_Vec(along));
    if (!contour.IsDone()) {
      return edges;
    }
    for (int i = 1; i <= contour.NbLines(); ++i) {
      const Contap_Line& line = contour.Line(i);
      switch (line.TypeContour()) {
      case Contap_Lin: {
        const gp_Lin straight = line.Line();
        const gp_Vec reach_along = gp_Vec(straight.Direction()) * length;
        edges.push_back(BRepBuilderAPI_MakeEdge(straight.Location().Translated(-reach_along),
                                                straight.Location().Translated(reach_along))
                            .Edge());
        break;
      }
      case Contap_Circle:
        edges.push_back(BRepBuilderAPI_MakeEdge(line.Circle()).Edge());
        break;
      case Contap_Walking: {
        std::vector<gp_Pnt> points;
        for (int k = 1; k <= line.NbPnts(); ++k) {
          const gp_Pnt& point = line.Point(k).Value();
          if (points.empty() || point.Distance(points.back()) > 1.0e-6) {
            points.push_back(point);
          }
        }
        if (points.size() < 2) {
          break;
        }
        const bool closed = points.size() > 2 && points.front().Distance(points.back()) <= 1.0e-6;
        if (points.size() == 2) {
          edges.push_back(BRepBuilderAPI_MakeEdge(points.front(), points.back()).Edge());
          run_on(points.front(), gp_Vec(points.back(), points.front()));
          run_on(points.back(), gp_Vec(points.front(), points.back()));
          break;
        }
        const TopoDS_Edge traced = traced_edge(points, surface);
        if (!traced.IsNull()) {
          edges.push_back(traced);
          if (!closed) {
            const BRepAdaptor_Curve curve(traced);
            gp_Pnt point;
            gp_Vec start;
            gp_Vec end;
            curve.D1(curve.FirstParameter(), point, start);
            run_on(point, -start);
            curve.D1(curve.LastParameter(), point, end);
            run_on(point, end);
          }
          break;
        }
        // No curve through them: the segments between the points.
        for (std::size_t k = 1; k < points.size(); ++k) {
          edges.push_back(BRepBuilderAPI_MakeEdge(points[k - 1], points[k]).Edge());
        }
        if (!closed) {
          run_on(points.front(), gp_Vec(points[1], points.front()));
          run_on(points.back(), gp_Vec(points[points.size() - 2], points.back()));
        }
        break;
      }
      case Contap_Restriction:
        // Along the face's boundary: its edges divide there.
        break;
      }
    }
  } catch (const Standard_Failure&) {
    // Not traced: the face adds no outline, as a face that does not turn.
  }
  return edges;
}

// The lines along which a curved face turns from facing `along` to facing
// away from it (its outline seen along it), swept back against `along`
// (mitcad#1857): for cylinders and cones the lines on them where the
// normal is at right angles to `along`, for spheres the circle, for other
// curved faces (tori, freeform faces) the contour lines traced on them
// (mitcad#1865). Each line is made long enough to cross everything; a cut
// where the face does not turn divides nothing that matters. Planes add
// nothing.
void add_outlines(const TopoDS_Face& face, const gp_Dir& along, double length, std::vector<TopoDS_Shape>& tools) {
  const BRepAdaptor_Surface surface(face);
  const auto add = [&](const TopoDS_Shape& curve) {
    const TopoDS_Shape swept = swept_back(curve, along, length);
    if (!swept.IsNull()) {
      tools.push_back(swept);
    }
  };
  if (surface.GetType() == GeomAbs_Plane) {
    return;
  }
  if (surface.GetType() == GeomAbs_Sphere) {
    const gp_Sphere sphere = surface.Sphere();
    add(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(sphere.Location(), along), sphere.Radius())).Edge());
    return;
  }
  if (surface.GetType() != GeomAbs_Cylinder && surface.GetType() != GeomAbs_Cone) {
    for (const TopoDS_Edge& edge : contours(face, along, length)) {
      // A line along `along` sweeps no face.
      const BRepAdaptor_Curve curve(edge);
      if (curve.GetType() != GeomAbs_Line || !curve.Line().Direction().IsParallel(along, 1.0e-9)) {
        add(edge);
      }
    }
    return;
  }
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const double v = (v0 + v1) / 2.0;
  // Along a line of a cylinder or a cone the normal stays the same, and
  // round it the normal's part along `along` is a cos u + b sin u + c.
  const auto facing = [&](double u) {
    gp_Pnt point;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(u, v, point, du, dv);
    const gp_Vec normal = du.Crossed(dv);
    return normal.Magnitude() <= Precision::Confusion() ? 0.0 : normal.Normalized().Dot(gp_Vec(along));
  };
  const double at0 = facing(0.0);
  const double at90 = facing(kPi / 2.0);
  const double at180 = facing(kPi);
  const double c = (at0 + at180) / 2.0;
  const double a = (at0 - at180) / 2.0;
  const double b = at90 - c;
  const double amplitude = std::hypot(a, b);
  if (amplitude <= 1.0e-9 || std::abs(c) >= amplitude) {
    return;
  }
  const double phase = std::atan2(b, a);
  const double turn = std::acos(std::clamp(-c / amplitude, -1.0, 1.0));
  for (const double u : {phase - turn, phase + turn}) {
    // Only where the face is.
    for (int k = -2; k <= 2; ++k) {
      const double w = u + 2.0 * kPi * k;
      if (w < u0 - 1.0e-9 || w > u1 + 1.0e-9) {
        continue;
      }
      gp_Pnt point;
      gp_Vec du;
      gp_Vec dv;
      surface.D1(w, v, point, du, dv);
      if (dv.Magnitude() > Precision::Confusion()) {
        const gp_Vec line = dv.Normalized() * length;
        add(BRepBuilderAPI_MakeEdge(point.Translated(-line), point.Translated(line)).Edge());
      }
      break;
    }
  }
}

// A plane face of the bodies that a continuation leaves at a rim
// (mitcad#1888): at an edge of the bodies between a face the tangent runs
// into (a boss's wall) and a plane face it runs away from (its top face),
// the points just past the edge pass over the plane face, yet below its
// level they are closed off by the first. The plane, extended past the
// edge, caps what the end fills there: the end fills up to the wall below
// the top face's level, so that the fill's top lies in the top face and
// meets it along the rim, with no step between the line along the tangent
// through the rim and the top face. A curved face the tangent runs away
// from keeps that line as its bound.
struct Cap {
  TopoDS_Face face; // the plane extended past the rim, to divide by
  gp_Pln plane;     // its normal out of the bodies
};

// The normal of the face where the edge is at the middle of its range,
// pointing out of the solid the face bounds, and that point; null when it
// has none there.
gp_Vec normal_on_edge(const TopoDS_Edge& edge, const TopoDS_Face& face, gp_Pnt& point) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    return gp_Vec();
  }
  const gp_Pnt2d uv = pcurve->Value((first + last) / 2.0);
  const BRepAdaptor_Surface surface(face);
  gp_Vec du;
  gp_Vec dv;
  surface.D1(uv.X(), uv.Y(), point, du, dv);
  gp_Vec normal = du.Crossed(dv);
  if (normal.Magnitude() <= Precision::Confusion()) {
    return gp_Vec();
  }
  normal.Normalize();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal;
}

// The cap of a plane face of the bodies the tangent runs away from at
// `at`, with `outward` its normal out of the bodies: the plane `length`
// each way from `at`. None for a face of any other kind.
std::optional<Cap> cap_of(const TopoDS_Face& face, const gp_Pnt& at, const gp_Vec& outward, double length) {
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  gp_Pln plane = surface.Plane();
  gp_Ax3 axes = plane.Position();
  if (gp_Vec(axes.Direction()).Dot(outward) < 0.0) {
    axes.ZReverse();
  }
  plane.SetPosition(axes);
  double u = 0.0;
  double v = 0.0;
  ElSLib::Parameters(plane, at, u, v);
  BRepBuilderAPI_MakeFace maker(plane, u - length, u + length, v - length, v + length);
  if (!maker.IsDone()) {
    return std::nullopt;
  }
  return Cap{maker.Face(), plane};
}

// Where the point lies under the cap (on the bodies' side of its plane),
// the tangent `along` turned into the plane: the way the point reaches the
// faces below the cap's level. None above it, or where the tangent runs
// straight through the plane.
std::optional<gp_Dir> under(const Cap& cap, const gp_Pnt& point, const gp_Dir& along) {
  const gp_Vec normal(cap.plane.Axis().Direction());
  if (gp_Vec(cap.plane.Location(), point).Dot(normal) >= -Precision::Confusion()) {
    return std::nullopt;
  }
  const gp_Vec flat = gp_Vec(along) - normal * gp_Vec(along).Dot(normal);
  if (flat.Magnitude() <= 1.0e-6) {
    return std::nullopt;
  }
  return gp_Dir(flat);
}

// The cap moved by `offset` (a copy).
Cap moved_cap(const Cap& cap, const gp_Vec& offset) {
  gp_Trsf move;
  move.SetTranslation(offset);
  return Cap{TopoDS::Face(cap.face.Moved(TopLoc_Location(move))), cap.plane.Translated(offset)};
}

// True when the cap's plane is one of the caps' already.
bool has_plane(const std::vector<Cap>& caps, const Cap& cap) {
  for (const Cap& other : caps) {
    if (other.plane.Axis().IsParallel(cap.plane.Axis(), 1.0e-9) &&
        other.plane.Distance(cap.plane.Location()) <= Precision::Confusion()) {
      return true;
    }
  }
  return false;
}

// How a face of the bodies faces the tangent at an edge: -1 when the
// tangent runs into it, 1 when away from it, 0 along it. `normal` points
// out of the piece, into the bodies.
int facing(const gp_Vec& normal, const gp_Dir& along) {
  constexpr double kSlant = 1.0e-4;
  const double dot = -normal.Dot(gp_Vec(along));
  return dot < -kSlant ? -1 : (dot > kSlant ? 1 : 0);
}

// The faces that divide a piece of a continuation (mitcad#1857) into parts
// whose points all reach the faces of the bodies along `along`, or all
// pass them: swept back against `along` from the edges where two faces of
// the bodies meet in the piece (a top face's rim) and from the outlines of
// their curved faces. Edges on the band's own faces and seams divide
// nothing. At a rim where the tangent runs into one face and away from the
// other, a plane, the plane's cap (mitcad#1888) is added to `caps` and
// divides too.
std::vector<TopoDS_Shape> reach_bounds(const Shape& piece, const gp_Dir& along, double length,
                                       std::vector<Cap>& caps) {
  ShapeMap band;
  std::vector<TopoDS_Face> stops;
  for (int i = 0; i < piece.face_count(); ++i) {
    if (piece.face_names(i).empty()) {
      stops.push_back(piece.face(i));
    } else {
      band.Add(piece.face(i));
    }
  }
  std::vector<TopoDS_Shape> tools;
  Ancestors faces_of_edge;
  TopExp::MapShapesAndAncestors(piece.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
  for (int i = 1; i <= faces_of_edge.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces_of_edge.FindKey(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    bool between_stops = true;
    for (const TopoDS_Shape& face : faces_of_edge.FindFromIndex(i)) {
      if (band.Contains(face) || BRep_Tool::IsClosed(edge, TopoDS::Face(face))) {
        between_stops = false;
        break;
      }
    }
    if (!between_stops) {
      continue;
    }
    if (faces_of_edge.FindFromIndex(i).Size() == 2) {
      const TopoDS_Face& one = TopoDS::Face(faces_of_edge.FindFromIndex(i).First());
      const TopoDS_Face& other = TopoDS::Face(faces_of_edge.FindFromIndex(i).Last());
      gp_Pnt at;
      const gp_Vec n1 = normal_on_edge(edge, one, at);
      const gp_Vec n2 = normal_on_edge(edge, other, at);
      if (n1.Magnitude() > 0.5 && n2.Magnitude() > 0.5 && facing(n1, along) * facing(n2, along) < 0) {
        const bool first_away = facing(n1, along) > 0;
        const std::optional<Cap> cap = cap_of(first_away ? one : other, at, first_away ? -n1 : -n2, length);
        if (cap && !has_plane(caps, *cap)) {
          tools.push_back(cap->face);
          caps.push_back(*cap);
        }
      }
    }
    // A line along the tangent sweeps no face.
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() == GeomAbs_Line && curve.Line().Direction().IsParallel(along, 1.0e-9)) {
      continue;
    }
    const TopoDS_Shape swept = swept_back(edge, along, length);
    if (!swept.IsNull()) {
      tools.push_back(swept);
    }
  }
  for (const TopoDS_Face& face : stops) {
    add_outlines(face, along, length, tools);
  }
  return tools;
}

// The piece divided by the faces (as reach_bounds gives them), one shape
// per part; faces from the dividing faces are named `name`. The piece
// itself when there is nothing to divide it by.
std::vector<ShapePtr> divided(const ShapePtr& piece, const std::vector<TopoDS_Shape>& tools, const std::string& name) {
  if (tools.empty()) {
    return {piece};
  }
  BRepAlgoAPI_Splitter splitter;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(piece->occt());
  NCollection_List<TopoDS_Shape> faces;
  for (const TopoDS_Shape& tool : tools) {
    faces.Append(tool);
  }
  splitter.SetArguments(arguments);
  splitter.SetTools(faces);
  splitter.SetNonDestructive(true);
  detail::build(splitter);
  if (!splitter.IsDone() || splitter.HasErrors()) {
    throw std::runtime_error("the rib's ends could not be continued to the bodies");
  }
  detail::FaceNamer namer(splitter.Shape());
  namer.carry(splitter, *piece);
  for (const TopoDS_Shape& tool : tools) {
    for (const TopoDS_Shape& image : splitter.Modified(tool)) {
      if (image.ShapeType() == TopAbs_FACE && !namer.named(image)) {
        namer.add(image, name);
      }
    }
    if (namer.contains(tool) && !namer.named(tool)) {
      namer.add(tool, name);
    }
  }
  std::vector<ShapePtr> parts;
  for (const detail::FaceNamer::Piece& part : namer.pieces()) {
    parts.push_back(part.shape);
  }
  return parts;
}

// Whether the line from `from` along `along` meets a face of the shapes
// within `length`.
class Reach {
public:
  Reach(const std::vector<TopoDS_Shape>& shapes, double length) : m_length(length) {
    for (const TopoDS_Shape& shape : shapes) {
      auto hits = std::make_unique<IntCurvesFace_ShapeIntersector>();
      hits->Load(shape, Precision::Confusion());
      m_shapes.push_back(std::move(hits));
    }
  }

  bool meets(const gp_Pnt& from, const gp_Dir& along) const {
    const gp_Lin line(from, along);
    for (const auto& hits : m_shapes) {
      hits->Perform(line, Precision::Confusion(), m_length);
      if (hits->IsDone() && hits->NbPnt() > 0) {
        return true;
      }
    }
    return false;
  }

private:
  std::vector<std::unique_ptr<IntCurvesFace_ShapeIntersector>> m_shapes;
  double m_length;
};

// Whether the point reaches a face of the bodies (as the rays find them)
// along the tangent, or lies under one of the caps and reaches one along
// the cap's surface (mitcad#1888).
bool reaches(const Reach& rays, const std::vector<Cap>& caps, const gp_Pnt& point, const gp_Dir& along) {
  if (rays.meets(point, along)) {
    return true;
  }
  for (const Cap& cap : caps) {
    if (const std::optional<gp_Dir> flat = under(cap, point, along); flat && rays.meets(point, *flat)) {
      return true;
    }
  }
  return false;
}

// The faces of the shapes whose bounds meet the piece's, added to `tools`.
void add_faces_near(const std::vector<TopoDS_Shape>& shapes, const Shape& piece, std::vector<TopoDS_Shape>& tools) {
  const Bnd_Box around = detail::bounds_of(piece.occt());
  for (const TopoDS_Shape& shape : shapes) {
    for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
      if (!detail::bounds_of(face.Current()).IsOut(around)) {
        tools.push_back(face.Current());
      }
    }
  }
}

bool touching(const Shape& a, const Shape& b) {
  BRepExtrema_DistShapeShape distance(a.occt(), b.occt());
  return distance.IsDone() && distance.Value() <= 1.0e-5;
}

// The continuation of one end of a rib (mitcad#1746): the band beyond the
// end's tip, along the end's tangent for `length` and as deep as the rib,
// cut by the bodies (moved along the tangent by the end offset), and of it
// the pieces that touch the rib: so it fills from the tip up to the faces
// the end meets, whatever their shape. A piece the faces do not close off
// (it runs on to the far end of the band, or up to the next faces past
// their depth: the continuation passes over or beside the bodies) keeps
// only the parts whose points reach a face along the tangent and that join
// the rib through such parts (mitcad#1857), or that lie under the cap of a
// rim the continuation passes over and reach the bodies along it, so that
// the fill ends in the top face's level with no step at the rim
// (mitcad#1888); with a positive offset, the points must also reach the
// bodies where they are (mitcad#1865), under their caps where they are.
// Null when there is nothing between the tip and the faces, or with a note
// when no part of the continuation meets a face.
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
  // Not closed off by the faces: it would run on to the far side of the
  // bodies' bounds (or, up to the next faces, past their depth).
  const auto unclosed = [&](const Shape& piece) {
    return holds(piece, reach_name) || (!spec.depth && holds(piece, far_side));
  };
  // A positive offset moves the faces past the bodies' own: where they are
  // moved over a top edge, the parts between the bodies and the moved
  // faces above the top face would pass for reaching them and leave a lip.
  // So the end's faces move, the cut line does not (mitcad#1865): the
  // pieces are also divided by the faces of the bodies where they are, and
  // a part must reach the bodies where they are too (from inside them it
  // does).
  const bool sunk = spec.end_offset > 0.0;
  const std::vector<TopoDS_Shape> bodies = shapes_of(spec.bodies);
  std::vector<ShapePtr> kept;
  // Of the pieces not closed off, the parts whose points reach a face.
  std::vector<ShapePtr> reaching;
  bool opened = false;
  std::unique_ptr<Reach> rays;
  std::unique_ptr<Reach> bare_rays;
  for (const ShapePtr& piece : cut_pieces(slab, stops, "rib")) {
    if (!touching(*piece, drawn)) {
      continue;
    }
    if (!unclosed(*piece)) {
      kept.push_back(piece);
      continue;
    }
    opened = true;
    if (!rays) {
      rays = std::make_unique<Reach>(stops, length);
    }
    if (sunk && !bare_rays) {
      bare_rays = std::make_unique<Reach>(bodies, length);
    }
    std::vector<Cap> caps;
    std::vector<TopoDS_Shape> bounds = reach_bounds(*piece, end.outward, length, caps);
    // The caps of the bodies where they are: the rims of the moved faces
    // moved back.
    std::vector<Cap> bare_caps;
    if (sunk) {
      add_faces_near(bodies, *piece, bounds);
      for (const Cap& cap : caps) {
        bare_caps.push_back(moved_cap(cap, gp_Vec(end.outward) * -spec.end_offset));
        bounds.push_back(bare_caps.back().face);
      }
    }
    std::vector<ShapePtr> parts;
    try {
      parts = divided(piece, bounds, face_name(spec.feature, end.extend));
    } catch (const std::runtime_error&) {
      // Not divided: the piece adds nothing, as one that meets no face.
      continue;
    }
    for (const ShapePtr& part : parts) {
      if (unclosed(*part)) {
        continue;
      }
      const gp_Pnt inside = detail::interior_point(part->occt());
      if (reaches(*rays, caps, inside, end.outward) &&
          (!bare_rays || reaches(*bare_rays, bare_caps, inside, end.outward))) {
        reaching.push_back(part);
      }
    }
  }
  // The reaching parts that join the rib, directly or through each other.
  std::vector<const Shape*> joined{&drawn};
  for (const ShapePtr& piece : kept) {
    joined.push_back(piece.get());
  }
  std::vector<bool> taken(reaching.size(), false);
  for (bool grew = true; grew;) {
    grew = false;
    for (std::size_t i = 0; i < reaching.size(); ++i) {
      if (taken[i] || std::none_of(joined.begin(), joined.end(),
                                   [&](const Shape* other) { return touching(*reaching[i], *other); })) {
        continue;
      }
      taken[i] = true;
      joined.push_back(reaching[i].get());
      kept.push_back(reaching[i]);
      grew = true;
    }
  }
  if (kept.empty()) {
    if (opened) {
      stays(end, "meets no face of the bodies when continued along its curve", notes);
    }
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
