// SPDX-License-Identifier: MIT
#include "mitcad/geometry/rib.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <iomanip>
#include <memory>
#include <optional>
#include <sstream>
#include <stdexcept>

#include <Adaptor3d_TopolTool.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFill.hxx>
#include <BRepLib.hxx>
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
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Pln.hxx>
#include <gp_Sphere.hxx>

#include "history.hpp"
#include "mitcad/geometry/user_error.hpp"
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

// A drafted wall's faces (mitcad#2102), each with its name.
using NamedFaces = std::vector<std::pair<TopoDS_Face, std::string>>;

std::runtime_error not_built(const char* what) {
  return std::runtime_error(std::string("the drafted ") + what + " could not be built");
}

// The solid the faces bound, each face named: they must close up along
// their edges, as a drafted wall's faces do.
ShapePtr sewn_solid(const NamedFaces& faces, const char* what) {
  BRepBuilderAPI_Sewing sewing(1.0e-6);
  for (const auto& named : faces) {
    sewing.Add(named.first);
  }
  sewing.Perform();
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer e(sewing.SewedShape(), TopAbs_SHELL); e.More(); e.Next()) {
    shell = TopoDS::Shell(e.Current());
    ++shells;
  }
  if (shells != 1 || sewing.NbFreeEdges() != 0) {
    throw not_built(what);
  }
  shell.Closed(true);
  BRep_Builder builder;
  TopoDS_Solid solid;
  builder.MakeSolid(solid);
  builder.Add(solid, shell);
  if (!BRepLib::OrientClosedSolid(solid)) {
    throw not_built(what);
  }
  detail::FaceNamer namer(solid);
  for (const auto& [face, name] : faces) {
    namer.add(sewing.IsModified(face) ? sewing.Modified(face) : face, name);
  }
  return namer.shape();
}

// The edge moved by `by` (a copy).
TopoDS_Edge moved_edge(const TopoDS_Edge& edge, const gp_Vec& by) {
  gp_Trsf move;
  move.SetTranslation(by);
  return TopoDS::Edge(BRepBuilderAPI_Transform(edge, move, true).Shape());
}

// The face the edge sweeps along `by`: a plane for a line.
TopoDS_Face swept_face(const TopoDS_Edge& edge, const gp_Vec& by, const char* what) {
  BRepPrimAPI_MakePrism prism(edge, by);
  prism.Build();
  if (prism.IsDone()) {
    for (TopExp_Explorer f(prism.Shape(), TopAbs_FACE); f.More(); f.Next()) {
      return TopoDS::Face(f.Current());
    }
  }
  throw not_built(what);
}

// The planar face inside the polygon through the points, in order.
TopoDS_Face polygon_face(const std::vector<gp_Pnt>& points, const char* what) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt& point : points) {
    polygon.Add(point);
  }
  polygon.Close();
  if (polygon.IsDone()) {
    BRepBuilderAPI_MakeFace maker(polygon.Wire(), true);
    if (maker.IsDone()) {
      return maker.Face();
    }
  }
  throw not_built(what);
}

// The face between two edges whose curves run the same way, each end joined
// to the other's end at the same end of its curve: a plane where they are
// parallel lines, else the ruled face between them (a cone between arcs
// about one axis). The edges' orientations do not count: an edge taken from
// a wire may be reversed in it while its moved copy is not, and pairing
// their ends by orientation then twists the face into a bow tie.
TopoDS_Face ruled_face(const TopoDS_Edge& from_edge, const TopoDS_Edge& to_edge, const char* what) {
  const TopoDS_Edge from = TopoDS::Edge(from_edge.Oriented(TopAbs_FORWARD));
  const TopoDS_Edge to = TopoDS::Edge(to_edge.Oriented(TopAbs_FORWARD));
  const BRepAdaptor_Curve a(from);
  const BRepAdaptor_Curve b(to);
  if (a.GetType() == GeomAbs_Line && b.GetType() == GeomAbs_Line &&
      a.Line().Direction().IsParallel(b.Line().Direction(), 1.0e-9)) {
    TopoDS_Vertex a0;
    TopoDS_Vertex a1;
    TopoDS_Vertex b0;
    TopoDS_Vertex b1;
    TopExp::Vertices(from, a0, a1, true);
    TopExp::Vertices(to, b0, b1, true);
    return polygon_face({BRep_Tool::Pnt(a0), BRep_Tool::Pnt(a1), BRep_Tool::Pnt(b1), BRep_Tool::Pnt(b0)}, what);
  }
  const TopoDS_Face face = BRepFill::Face(from, to);
  if (face.IsNull()) {
    throw not_built(what);
  }
  return face;
}

// A number in an error's detail for agents.
std::string number_text(double value) {
  std::ostringstream out;
  out << std::setprecision(6) << value;
  return out.str();
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

// True when a face of the shape has a name matching one of `names`.
bool holds_any(const Shape& shape, const std::vector<std::string>& names) {
  return std::any_of(names.begin(), names.end(), [&](const std::string& name) { return holds(shape, name); });
}

// Of the pieces of a tool cut by the bodies, those that hold a face named
// one of `keep`, fused: it must end at the bodies, so no piece may hold
// `far` (`unreached`, when given, is the error then: a draft that closed
// the tool before its far end, mitcad#2102).
ShapePtr kept_to_bodies(const std::vector<ShapePtr>& pieces, const std::vector<std::string>& keep,
                        const std::string& far, const char* what, const std::string& unreached = {}) {
  std::vector<ShapePtr> kept;
  for (const ShapePtr& piece : pieces) {
    if (holds_any(*piece, keep)) {
      if (detail::has_face_named(*piece, far)) {
        if (!unreached.empty()) {
          throw std::invalid_argument(unreached);
        }
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

// The tool cut by the bodies, keeping the pieces that hold a face named
// `keep`: it must end at the bodies, so no piece may hold `far`.
ShapePtr to_bodies(const ShapePtr& tool, const std::vector<ShapePtr>& bodies, const std::string& keep,
                   const std::string& far, const char* what, const std::string& unreached = {}) {
  if (bodies.empty()) {
    throw std::invalid_argument(std::string("there are no bodies for the ") + what + " to reach");
  }
  return kept_to_bodies(cut_pieces(tool, shapes_of(bodies), what), {keep}, far, what, unreached);
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

// One open end of a rib's chain (mitcad#1746) or a free end of a web's wall
// (mitcad#1968): where it is, its tangent out of the curves, the axis its
// thickness lies along (a rib's sketch normal, a web wall's side), and the
// names of the faces its continuation makes.
struct RibEnd {
  gp_Pnt point;
  gp_Dir outward;
  gp_Dir across;
  std::string label;     // what the notes call it: "the start of the rib's curves"
  std::string along;     // the face along the continuation from the tip
  std::string tip;       // the end's tip
  std::string extend;    // the faces that divide the continuation
  std::string reach;     // the continuation's far end
  std::string far;       // the far side, at the depth
  std::string near_cap;  // its side at the smaller offset
  std::string far_cap;   // its side at the larger
  std::string other_tip; // the tip at the curves' other end
  gp_Pnt probe;          // just inside the drawn end, to look along the tangent from
};

// What the notes of an end that stays as drawn say (no names or places in
// the text; the place goes with the note).
void stays(const RibEnd& end, const char* why, std::vector<std::pair<std::string, gp_Pnt>>& notes) {
  notes.emplace_back(end.label + " " + why + "; it stays where it is drawn", end.point);
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
// when no part of the continuation meets a face. A web's free end
// (mitcad#1968) is continued the same way, its band across the wall's
// thickness instead of the sketch plane; `what` names the rib or the web.
// With a draft (mitcad#2102) its sides lean out by `lean` per unit of
// `depth`, as the wall's do.
ShapePtr extension(const RibSpec& spec, const Shape& drawn, const RibEnd& end, const gp_Vec& depth, double low,
                   double high, double lean, double length, std::vector<std::pair<std::string, gp_Pnt>>& notes,
                   const char* what) {
  if (spec.bodies.empty()) {
    stays(end, "has no bodies to be continued to", notes);
    return nullptr;
  }
  // Along the way the rib grows, the continuation would have no width.
  if (gp_Vec(end.outward).Crossed(depth).Magnitude() <= 1.0e-6 * depth.Magnitude()) {
    stays(end, (std::string("runs along the ") + what + "'s direction there and cannot be continued").c_str(),
          notes);
    return nullptr;
  }
  const gp_Dir across = end.across;
  const gp_Pnt p = end.point;
  const gp_Pnt q = p.Translated(gp_Vec(end.outward) * length);
  const std::string& reach_name = end.reach;
  const std::string& far_side = end.far;
  BRepBuilderAPI_MakeWire outline;
  Names names;
  const auto add = [&](const gp_Pnt& from, const gp_Pnt& to, const std::string& name) {
    outline.Add(BRepBuilderAPI_MakeEdge(from, to).Edge());
    if (!outline.IsDone()) {
      throw std::invalid_argument(std::string("the ") + what + "'s ends cannot be continued");
    }
    names.emplace_back(outline.Edge(), name);
  };
  ShapePtr slab;
  if (lean == 0.0) {
    add(p, q, end.along);
    add(q, q.Translated(depth), reach_name);
    add(q.Translated(depth), p.Translated(depth), far_side);
    add(p.Translated(depth), p, end.tip);
    const TopoDS_Face band = band_face(gp_Pln(p, across), outline.Wire(), what);
    slab = band_prism(band, names, gp_Vec(across) * low, gp_Vec(across) * (high - low), {end.near_cap}, {end.far_cap});
  } else {
    // The end's section, its sides leaning out as the wall's do, swept
    // along the tangent: the same faces as the band across the thickness.
    const double out = depth.Magnitude() * lean;
    const gp_Pnt c0 = p.Translated(gp_Vec(across) * low);
    const gp_Pnt c1 = p.Translated(gp_Vec(across) * high);
    const gp_Pnt c2 = p.Translated(depth + gp_Vec(across) * (high + out));
    const gp_Pnt c3 = p.Translated(depth + gp_Vec(across) * (low - out));
    add(c0, c1, end.along);
    add(c1, c2, end.far_cap);
    add(c2, c3, far_side);
    add(c3, c0, end.near_cap);
    const TopoDS_Face section = band_face(gp_Pln(p, gp_Dir(gp_Vec(across).Crossed(depth))), outline.Wire(), what);
    slab = band_prism(section, names, gp_Vec(0, 0, 0), gp_Vec(end.outward) * length, {end.tip}, {reach_name});
  }
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
  for (const ShapePtr& piece : cut_pieces(slab, stops, what)) {
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
      parts = divided(piece, bounds, end.extend);
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
  return detail::fused(kept, (std::string("the ") + what).c_str());
}

// A little inside a drawn end, to look along its tangent from: off the
// sketch plane into the way the rib or web grows, so that the line clears a
// face the curves lie on.
double probe_step(const RibSpec& spec) {
  return std::max(1.0e-4, 0.01 * spec.thickness);
}

// The drawn rib or wall (mitcad#1968): the tool as it is with a depth, or
// cut to the bodies up to the next faces (keeping the pieces that hold the
// first of `keep`, or once an end is cut back any of them; none may hold
// `far`). With Extend Ends, the curves' free
// ends that lie past the walls they cross are cut back to them: the tool
// cut by the bodies loses each piece that holds the tip of such an end, and
// not the tip at the curves' other end (it lies beyond a wall from the
// rest), where the end's tangent out of the curves meets no face of the
// bodies (an end inside the room meets the wall ahead of it). The piece
// that holds `middle` (a point just inside the curves' middle) is the rest
// and stays; with two such ends and no piece there nothing is cut back.
// `cut` tells which of `ends` were cut back so; they are not continued.
// `unreached`, when given, is the error for a tool that holds `far`.
ShapePtr drawn_part(const RibSpec& spec, const ShapePtr& tool, const std::vector<std::string>& keep,
                    const std::string& far, const std::vector<RibEnd>& ends, const gp_Pnt& middle, double length,
                    const char* what, std::vector<bool>& cut, const std::string& unreached) {
  cut.assign(ends.size(), false);
  const auto plain = [&]() -> ShapePtr {
    if (spec.depth) {
      return tool;
    }
    if (spec.bodies.empty()) {
      throw std::invalid_argument(std::string("there are no bodies for the ") + what + " to reach");
    }
    return kept_to_bodies(cut_pieces(tool, shapes_of(spec.bodies), what), {keep.front()}, far, what, unreached);
  };
  if (!spec.extend_ends || spec.bodies.empty() || ends.empty()) {
    return plain();
  }
  const std::vector<TopoDS_Shape> bodies = shapes_of(spec.bodies);
  const Reach rays(bodies, length);
  std::vector<bool> open_ends(ends.size(), false);
  bool any = false;
  for (std::size_t i = 0; i < ends.size(); ++i) {
    open_ends[i] = !rays.meets(ends[i].probe, ends[i].outward);
    any = any || open_ends[i];
  }
  if (!any) {
    return plain();
  }
  const std::vector<ShapePtr> pieces = cut_pieces(tool, bodies, what);
  const auto holds_middle = [&](const ShapePtr& piece) {
    BRepClass3d_SolidClassifier classifier(piece->occt(), middle, Precision::Confusion());
    return classifier.State() == TopAbs_IN;
  };
  if (ends.size() > 1 && std::none_of(pieces.begin(), pieces.end(), holds_middle)) {
    return plain();
  }
  std::vector<ShapePtr> kept;
  for (const ShapePtr& piece : pieces) {
    bool past = false;
    for (std::size_t i = 0; i < ends.size(); ++i) {
      if (open_ends[i] && holds(*piece, ends[i].tip) && !holds(*piece, ends[i].other_tip) && !holds_middle(piece)) {
        past = true;
        cut[i] = true;
      }
    }
    if (!past) {
      kept.push_back(piece);
    }
  }
  if (std::none_of(cut.begin(), cut.end(), [](bool c) { return c; })) {
    return plain();
  }
  if (!spec.depth) {
    return kept_to_bodies(kept, keep, far, what, unreached);
  }
  if (kept.empty()) {
    throw std::invalid_argument(std::string("the ") + what + " lies outside the bodies");
  }
  return detail::fused(kept, what);
}

// How far a drafted wall runs (mitcad#2102) from its curves the way it
// grows, `along` from `from` (a point between its sides at the curves):
// `run`, its depth, or with none far enough to pass the bodies. Where the
// draft narrows it (`lean` < 0), a depth that reaches the line where its
// sides meet fails; with none it runs no further than where they come a
// tenth of the thickness apart, and `unreached` is then the error for a
// wall that no face of the bodies closes off by there.
double drafted_run(const RibSpec& spec, double lean, double run, const gp_Pnt& from, const gp_Dir& along,
                   const char* what, std::string& unreached) {
  unreached.clear();
  if (lean >= 0.0) {
    return run;
  }
  const double closes = spec.thickness / (2.0 * -lean);
  const std::vector<ErrorPlace> place{point_place(from.Translated(gp_Vec(along) * closes))};
  const std::string said = std::string("the draft angle closes the ") + what;
  const std::string named = said + " " + number_text(closes) + " mm from its curves (a draft angle of " +
                            number_text(spec.draft_angle * 180.0 / kPi) + " degrees, a thickness of " +
                            number_text(spec.thickness) + " mm)";
  if (spec.depth) {
    if (run < closes * (1.0 - 1.0e-6)) {
      return run;
    }
    throw std::invalid_argument(
        placed_error(said + " before its depth", named + ", before its depth of " + number_text(run) + " mm", place));
  }
  const double most = 0.9 * closes;
  if (run <= most) {
    return run;
  }
  unreached = placed_error(said + " before it reaches the faces of the bodies",
                           named + ", before a face of the bodies closes it off", place);
  return most;
}

// A rib with its draft (mitcad#2102): the curves' edges swept the way the
// rib grows (`depth`), its sides at `low` and `high` along the sketch
// normal at the curves leaning out by `lean` per unit: side(<curve>)
// across the curves, wall2 and wall1 the curves swept along the leaned
// directions (planes for lines), end across the curves moved by the depth,
// tip0 and tip1 the trapezoids at the chain's start `a` and end `b`.
ShapePtr drafted_rib(const RibSpec& spec, const Path& chain, const std::vector<TopoDS_Edge>& edges, const gp_Pnt& a,
                     const gp_Pnt& b, const gp_Vec& depth, double low, double high, double lean) {
  const gp_Vec n(spec.frame.normal());
  const double out = depth.Magnitude() * lean;
  const double far_low = low - out;
  const double far_high = high + out;
  const std::string wall1 = face_name(spec.feature, "wall1");
  const std::string wall2 = face_name(spec.feature, "wall2");
  const std::string end = face_name(spec.feature, "end");
  NamedFaces faces;
  for (std::size_t i = 0; i < edges.size() && i < chain.size(); ++i) {
    const TopoDS_Edge at_low = moved_edge(edges[i], n * low);
    const TopoDS_Edge at_high = moved_edge(edges[i], n * high);
    faces.emplace_back(swept_face(at_low, n * (high - low), "rib"), face_name(spec.feature, "side", chain[i].name));
    faces.emplace_back(swept_face(at_low, depth - n * out, "rib"), wall2);
    faces.emplace_back(swept_face(at_high, depth + n * out, "rib"), wall1);
    faces.emplace_back(swept_face(moved_edge(edges[i], depth + n * far_low), n * (far_high - far_low), "rib"), end);
  }
  const auto tip = [&](const gp_Pnt& p) {
    return polygon_face({p.Translated(n * low), p.Translated(n * high), p.Translated(depth + n * far_high),
                         p.Translated(depth + n * far_low)},
                        "rib");
  };
  faces.emplace_back(tip(a), face_name(spec.feature, "tip0"));
  faces.emplace_back(tip(b), face_name(spec.feature, "tip1"));
  return sewn_solid(faces, "rib");
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
  const auto [low, high] = sides(spec.location, spec.thickness);
  const double middle = (low + high) / 2.0;
  // A draft (mitcad#2102): the sides lean out by `lean` per unit the rib
  // grows, so that it widens from its curves to the faces it reaches.
  const double lean = std::tan(spec.draft_angle);
  std::string unreached;
  const double length =
      drafted_run(spec, lean, spec.depth ? *spec.depth : reach(spec, {wire}),
                  spine.point(spine.length() / 2.0).Translated(gp_Vec(normal) * middle), across, "rib", unreached);
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
  const ShapePtr tool = lean == 0.0
                            ? band_prism(band, names, gp_Vec(normal) * low, gp_Vec(normal) * (high - low),
                                         {face_name(spec.feature, "wall2")}, {face_name(spec.feature, "wall1")})
                            : drafted_rib(spec, chain, edges, a, b, depth, low, high, lean);
  // Extend Ends (mitcad#1746): each end continued along its tangent, far
  // enough to leave the bodies' bounds (and the offset's more); an end past
  // the walls the curves cross is cut back to them instead (mitcad#1968).
  const auto end_of = [&](const gp_Pnt& point, const gp_Dir& outward, const char* which, const char* n,
                          const char* other) {
    RibEnd end;
    end.point = point;
    end.outward = outward;
    end.across = normal;
    end.label = std::string("the ") + which + " of the rib's curves";
    end.along = face_name(spec.feature, std::string("extend") + n);
    end.tip = face_name(spec.feature, std::string("tip") + n);
    end.extend = end.along;
    end.reach = face_name(spec.feature, std::string("reach") + n);
    end.far = face_name(spec.feature, "end");
    end.near_cap = face_name(spec.feature, "wall2");
    end.far_cap = face_name(spec.feature, "wall1");
    end.other_tip = face_name(spec.feature, std::string("tip") + other);
    end.probe = point.Translated(gp_Vec(across) * probe_step(spec) + gp_Vec(normal) * middle);
    return end;
  };
  std::vector<RibEnd> ends;
  if (spec.extend_ends) {
    ends.push_back(end_of(a, spine.tangent(0.0).Reversed(), "start", "0", "1"));
    ends.push_back(end_of(b, spine.tangent(spine.length()), "end", "1", "0"));
  }
  const double run_out = reach(spec, {wire}) + std::abs(spec.end_offset);
  std::vector<std::string> keep;
  for (const PathCurve& piece : chain) {
    keep.push_back(face_name(spec.feature, "side", piece.name));
  }
  // Just inside the middle of the curves.
  const gp_Pnt middle_point = spine.point(spine.length() / 2.0).Translated(gp_Vec(across) * probe_step(spec) +
                                                                          gp_Vec(normal) * middle);
  std::vector<bool> cut;
  const ShapePtr drawn =
      drawn_part(spec, tool, keep, face_name(spec.feature, "end"), ends, middle_point, run_out, "rib", cut, unreached);
  if (!spec.extend_ends) {
    return drawn;
  }
  std::vector<ShapePtr> parts{drawn};
  for (std::size_t i = 0; i < ends.size(); ++i) {
    if (cut[i]) {
      continue;
    }
    if (const ShapePtr more = extension(spec, *drawn, ends[i], depth, low, high, lean, run_out, notes, "rib")) {
      parts.push_back(more);
    }
  }
  return detail::fused(parts, "the rib");
}

// A web's curve as its wall sees it (mitcad#1968): a line or a circular
// arc, where it starts and ends, the way it runs there, and the offsets of
// its wall's sides to its left.
struct WebCurve {
  const PathCurve* curve = nullptr;
  bool arc = false;
  gp_Pnt ends[2];   // its start and end
  gp_Dir travel[2]; // the way it runs at its start and at its end
  gp_Ax2 axes;      // an arc's circle, counter-clockwise from `first` to `last`
  double radius = 0.0;
  double first = 0.0;
  double last = 0.0;
  double turn = 1.0; // +1 when an arc runs counter-clockwise about the sketch normal
  double low = 0.0;  // the wall's sides to the left of the curve
  double high = 0.0;
  int nodes[2] = {-1, -1}; // the nodes its ends are at
  bool loose[2] = {false, false}; // touches no other curve
};

WebCurve web_curve(const PathCurve& curve, const gp_Dir& normal) {
  WebCurve w;
  w.curve = &curve;
  const ModelCurve& c = curve.curve;
  if (c.kind == ModelCurveKind::Line) {
    const gp_Vec along(c.start, c.end);
    if (along.Magnitude() <= Precision::Confusion()) {
      throw std::invalid_argument(
          placed_error("a web curve has no length", "web curve " + curve.name + " has no length"));
    }
    w.ends[0] = c.start;
    w.ends[1] = c.end;
    w.travel[0] = gp_Dir(along);
    w.travel[1] = gp_Dir(along);
    return w;
  }
  if (c.kind != ModelCurveKind::Conic || c.closed || std::abs(c.major - c.minor) > 1e-9 * std::max(1.0, c.major)) {
    throw std::invalid_argument("unsupported: web curves other than lines and arcs (" + curve.name + ")");
  }
  w.arc = true;
  w.axes = gp_Ax2(c.center, c.normal, c.x_axis);
  w.radius = c.major;
  w.first = c.first;
  w.last = c.last;
  w.turn = gp_Vec(c.normal).Dot(gp_Vec(normal)) > 0.0 ? 1.0 : -1.0;
  const gp_Vec x(w.axes.XDirection());
  const gp_Vec y(w.axes.YDirection());
  const double at[2] = {c.first, c.last};
  for (int k = 0; k < 2; ++k) {
    w.ends[k] = c.center.Translated(x * (w.radius * std::cos(at[k])) + y * (w.radius * std::sin(at[k])));
    w.travel[k] = gp_Dir(x * -std::sin(at[k]) + y * std::cos(at[k]));
  }
  return w;
}

// The left of a curve where it runs `travel`, in the sketch plane.
gp_Vec left_of(const gp_Dir& normal, const gp_Dir& travel) {
  return gp_Vec(normal.Crossed(travel));
}

// One side of a wall: the curve's parallel at `offset` to its left, a line
// or a circle.
struct WallSide {
  bool arc = false;
  gp_Pnt origin; // a line's point at the curve's start
  gp_Dir along;  // a line's direction
  gp_Ax2 axes;   // an arc's circle
  double radius = 0.0;
};

WallSide side_of(const WebCurve& w, double offset, const gp_Dir& normal) {
  WallSide side;
  side.arc = w.arc;
  if (!w.arc) {
    side.along = w.travel[0];
    side.origin = w.ends[0].Translated(left_of(normal, w.travel[0]) * offset);
  } else {
    side.axes = w.axes;
    side.radius = w.radius - w.turn * offset;
  }
  return side;
}

// Where a side crosses the line through `at` along `dir`, in the sketch
// plane of normal `normal`.
std::vector<gp_Pnt> crossings(const WallSide& side, const gp_Pnt& at, const gp_Dir& dir, const gp_Dir& normal) {
  std::vector<gp_Pnt> points;
  if (!side.arc) {
    const gp_Vec m(normal.Crossed(dir));
    const double d = m.Dot(gp_Vec(side.along));
    if (std::abs(d) <= 1.0e-12) {
      return points;
    }
    points.push_back(side.origin.Translated(gp_Vec(side.along) * (m.Dot(gp_Vec(side.origin, at)) / d)));
    return points;
  }
  const gp_Vec e(dir);
  const gp_Vec w(side.axes.Location(), at);
  const double b = e.Dot(w);
  const double r2 = side.radius * side.radius;
  const double disc = b * b - (w.SquareMagnitude() - r2);
  if (disc < -1.0e-9 * r2) {
    return points;
  }
  const double root = std::sqrt(std::max(disc, 0.0));
  points.push_back(at.Translated(e * (-b - root)));
  points.push_back(at.Translated(e * (-b + root)));
  return points;
}

// Where two sides cross.
std::vector<gp_Pnt> crossings(const WallSide& a, const WallSide& b, const gp_Dir& normal) {
  if (!a.arc) {
    return crossings(b, a.origin, a.along, normal);
  }
  if (!b.arc) {
    return crossings(a, b.origin, b.along, normal);
  }
  const gp_Vec between(a.axes.Location(), b.axes.Location());
  const double d = between.Magnitude();
  if (d <= 1.0e-9) {
    return {};
  }
  const double r2 = a.radius * a.radius;
  const double along = (r2 - b.radius * b.radius + d * d) / (2.0 * d);
  const double h2 = r2 - along * along;
  if (h2 < -1.0e-9 * r2) {
    return {};
  }
  const double h = std::sqrt(std::max(h2, 0.0));
  const gp_Vec ex = between / d;
  const gp_Vec ey = gp_Vec(normal).Crossed(ex);
  const gp_Pnt base = a.axes.Location().Translated(ex * along);
  return {base.Translated(ey * h), base.Translated(ey * -h)};
}

std::optional<gp_Pnt> nearest(const std::vector<gp_Pnt>& points, const gp_Pnt& to) {
  std::optional<gp_Pnt> best;
  for (const gp_Pnt& point : points) {
    if (!best || point.Distance(to) < best->Distance(to)) {
      best = point;
    }
  }
  return best;
}

// The angle of a point about an arc's circle, the one nearest `around`.
double angle_of(const gp_Ax2& axes, const gp_Pnt& point, double around) {
  const gp_Vec v(axes.Location(), point);
  const double angle = std::atan2(v.Dot(gp_Vec(axes.YDirection())), v.Dot(gp_Vec(axes.XDirection())));
  return around + std::remainder(angle - around, 2.0 * kPi);
}

// How one end of a wall closes (mitcad#1968): where its two sides end, and
// the corner between them where a sharp mitre is cut off.
struct WallEnd {
  gp_Pnt low;
  gp_Pnt high;
  std::optional<gp_Pnt> corner;
};

// The end square across the curve, as a free end's.
WallEnd square_end(const WebCurve& w, int k, const gp_Dir& normal) {
  const gp_Vec left = left_of(normal, w.travel[k]);
  return WallEnd{w.ends[k].Translated(left * w.low), w.ends[k].Translated(left * w.high), std::nullopt};
}

// The ends of walls `a` and `b` whose curves meet at a corner, end `ka` of
// a at end `kb` of b (mitcad#1968): each side runs on to where it crosses
// the other wall's side on the same side of the corner, so that the walls
// meet in a mitre with no notch, whatever the angle and the Thickness Side.
// On the corner's outside a mitre that would reach more than `limit` from
// the corner is cut off square to the corner's bisector there, rather than
// run to a spike. Where the curves run on in one line, or an arc's sides do
// not cross near the corner, the ends stay square. `outer`, when given, is
// set to how far the mitre's outer point lies past the corner along the
// bisector, before any cut (0 where the ends stay square).
void mitre(const WebCurve& a, int ka, const WebCurve& b, int kb, const gp_Dir& normal, double limit, WallEnd& end_a,
           WallEnd& end_b, double* outer = nullptr) {
  if (outer) {
    *outer = 0.0;
  }
  const gp_Pnt corner = a.ends[ka];
  // From the corner into each wall.
  const gp_Vec into_a = gp_Vec(a.travel[ka]) * (ka == 0 ? 1.0 : -1.0);
  const gp_Vec into_b = gp_Vec(b.travel[kb]) * (kb == 0 ? 1.0 : -1.0);
  const gp_Vec bisector = into_a + into_b;
  if (bisector.Magnitude() <= 1.0e-6) {
    return;
  }
  if ((into_a - into_b).Magnitude() <= 1.0e-6) {
    throw std::invalid_argument("two curves of the web run back along each other from a corner");
  }
  const gp_Dir inside(bisector);
  // The side of b on the same side of the corner as a's side at offset o:
  // at o when one runs into the corner and the other out of it, else at -o.
  const bool same = ka != kb;
  const double offsets[2] = {a.low, a.high};
  gp_Pnt meet[2];
  for (int i = 0; i < 2; ++i) {
    const std::optional<gp_Pnt> x =
        nearest(crossings(side_of(a, offsets[i], normal), side_of(b, same ? offsets[i] : -offsets[i], normal), normal),
                corner);
    // An arc's sides that cross far away, or not at all, keep square ends.
    if (!x || ((a.arc || b.arc) && x->Distance(corner) > 10.0 * limit)) {
      return;
    }
    meet[i] = *x;
  }
  const auto depth = [&](const gp_Pnt& p) { return gp_Vec(corner, p).Dot(gp_Vec(inside)); };
  const int out = depth(meet[0]) < depth(meet[1]) ? 0 : 1;
  if (outer) {
    *outer = -depth(meet[out]);
  }
  gp_Pnt side_a[2] = {meet[0], meet[1]};
  gp_Pnt side_b[2] = {meet[0], meet[1]};
  std::optional<gp_Pnt> cut_corner;
  if (depth(meet[out]) < -limit) {
    // The cut across the bisector at `limit` outside the corner: its corner
    // on the mitre between the two crossings.
    const gp_Pnt& in = meet[1 - out];
    const double t = (-limit - depth(in)) / (depth(meet[out]) - depth(in));
    const gp_Pnt q = in.Translated(gp_Vec(in, meet[out]) * t);
    const gp_Dir across = normal.Crossed(inside);
    const double o = offsets[out];
    const std::optional<gp_Pnt> xa = nearest(crossings(side_of(a, o, normal), q, across, normal), q);
    const std::optional<gp_Pnt> xb = nearest(crossings(side_of(b, same ? o : -o, normal), q, across, normal), q);
    if (!xa || !xb) {
      return;
    }
    side_a[out] = *xa;
    side_b[out] = *xb;
    cut_corner = q;
  }
  end_a = WallEnd{side_a[0], side_a[1], cut_corner};
  // b's low side is a's low side when they run the same way, else its high.
  end_b = same ? WallEnd{side_b[0], side_b[1], cut_corner} : WallEnd{side_b[1], side_b[0], cut_corner};
}

// The band of a web's wall in the sketch plane between its sides, closed by
// its two ends; edges named wall2 (the low side), wall1 (the high), tip0
// and tip1 after the curve.
TopoDS_Face web_band(const RibSpec& spec, const WebCurve& w, const WallEnd& start, const WallEnd& end, Names& names) {
  const gp_Dir normal = spec.frame.normal();
  const std::string& key = w.curve->name;
  const char* short_curve = "a web curve is too short for the walls that meet at its ends";
  const gp_Pnt from[2] = {start.low, start.high};
  const gp_Pnt to[2] = {end.low, end.high};
  const double offsets[2] = {w.low, w.high};
  TopoDS_Edge walls[2];
  for (int i = 0; i < 2; ++i) {
    if (!w.arc) {
      if (gp_Vec(from[i], to[i]).Dot(gp_Vec(w.travel[0])) <= Precision::Confusion()) {
        throw std::invalid_argument(short_curve);
      }
      walls[i] = BRepBuilderAPI_MakeEdge(from[i], to[i]).Edge();
      continue;
    }
    const double radius = w.radius - w.turn * offsets[i];
    if (radius <= Precision::Confusion()) {
      throw std::invalid_argument("the web is thicker than arc " + key + " allows");
    }
    const double t0 = angle_of(w.axes, from[i], w.first);
    const double t1 = angle_of(w.axes, to[i], w.last);
    if (t1 - t0 <= 1.0e-9 || t1 - t0 >= 2.0 * kPi) {
      throw std::invalid_argument(short_curve);
    }
    walls[i] = BRepBuilderAPI_MakeEdge(gp_Circ(w.axes, radius), t0, t1).Edge();
  }
  BRepBuilderAPI_MakeWire outline;
  const auto add = [&](const TopoDS_Edge& edge, const char* role) {
    outline.Add(edge);
    if (!outline.IsDone()) {
      throw std::invalid_argument("the web along " + key + " does not close");
    }
    names.emplace_back(outline.Edge(), face_name(spec.feature, role, key));
  };
  // An end from its low side to its high, through a cut-off mitre's corner.
  const auto shut = [&](const WallEnd& at, const char* role) {
    if (at.corner && at.corner->Distance(at.low) > Precision::Confusion() &&
        at.corner->Distance(at.high) > Precision::Confusion()) {
      add(BRepBuilderAPI_MakeEdge(at.low, *at.corner).Edge(), role);
      add(BRepBuilderAPI_MakeEdge(*at.corner, at.high).Edge(), role);
    } else {
      add(BRepBuilderAPI_MakeEdge(at.low, at.high).Edge(), role);
    }
  };
  add(walls[0], "wall2");
  shut(end, "tip1");
  add(walls[1], "wall1");
  shut(start, "tip0");
  return band_face(gp_Pln(w.ends[0], normal), outline.Wire(), "web");
}

// The convex hull of points in the sketch plane, in order round it: the
// indices of its corners among the points.
std::vector<std::size_t> hull(const std::vector<gp_Pnt>& points, const Frame& frame) {
  struct Flat {
    double x;
    double y;
    std::size_t index;
  };
  std::vector<Flat> flat;
  for (std::size_t i = 0; i < points.size(); ++i) {
    const gp_Vec v(frame.origin, points[i]);
    flat.push_back({v.Dot(gp_Vec(frame.x_axis)), v.Dot(gp_Vec(frame.y_axis)), i});
  }
  std::sort(flat.begin(), flat.end(), [](const Flat& p, const Flat& q) { return p.x < q.x || (p.x == q.x && p.y < q.y); });
  const auto turn = [](const Flat& o, const Flat& p, const Flat& q) {
    return (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
  };
  constexpr double kTurn = 1.0e-12;
  std::vector<Flat> rim;
  for (int pass = 0; pass < 2; ++pass) {
    const std::size_t base = rim.size();
    for (std::size_t i = 0; i < flat.size(); ++i) {
      const Flat& p = pass == 0 ? flat[i] : flat[flat.size() - 1 - i];
      while (rim.size() >= base + 2 && turn(rim[rim.size() - 2], rim.back(), p) <= kTurn) {
        rim.pop_back();
      }
      rim.push_back(p);
    }
    rim.pop_back();
  }
  std::vector<std::size_t> corners;
  for (const Flat& p : rim) {
    if (corners.empty() || points[p.index].Distance(points[corners.back()]) > Precision::Confusion()) {
      corners.push_back(p.index);
    }
  }
  while (corners.size() > 1 && points[corners.front()].Distance(points[corners.back()]) <= Precision::Confusion()) {
    corners.pop_back();
  }
  return corners;
}

// What joins three or more walls whose curves end at one node (mitcad#1968):
// the convex hull of the node and the corners of the walls' square ends
// there, as tall as the walls, so that their union closes the gaps between
// them. Its faces are named after `curve`'s end there (`tip`), its caps
// `start` and `end` after it. Null when the hull has no area. With a draft
// (mitcad#2102), `top` holds the same points at the walls' far end (their
// sides moved out): the hull's corners there, moved by `height`, are joined
// to those in the sketch plane by ruled faces.
ShapePtr node_piece(const RibSpec& spec, const std::vector<gp_Pnt>& points, const std::string& curve,
                    const char* tip, const gp_Vec& height, const std::vector<gp_Pnt>& top) {
  const std::vector<std::size_t> order = hull(points, spec.frame);
  if (order.size() < 3) {
    return nullptr;
  }
  std::vector<gp_Pnt> corners;
  for (const std::size_t i : order) {
    corners.push_back(points[i]);
  }
  if (top.size() == points.size()) {
    std::vector<gp_Pnt> far;
    for (const std::size_t i : order) {
      far.push_back(top[i].Translated(height));
    }
    NamedFaces faces{{polygon_face(corners, "web"), face_name(spec.feature, "start", curve)},
                     {polygon_face(far, "web"), face_name(spec.feature, "end", curve)}};
    for (std::size_t i = 0; i < corners.size(); ++i) {
      const std::size_t next = (i + 1) % corners.size();
      faces.emplace_back(ruled_face(BRepBuilderAPI_MakeEdge(corners[i], corners[next]).Edge(),
                                    BRepBuilderAPI_MakeEdge(far[i], far[next]).Edge(), "web"),
                         face_name(spec.feature, tip, curve));
    }
    return sewn_solid(faces, "web");
  }
  BRepBuilderAPI_MakeWire outline;
  Names names;
  for (std::size_t i = 0; i < corners.size(); ++i) {
    outline.Add(BRepBuilderAPI_MakeEdge(corners[i], corners[(i + 1) % corners.size()]).Edge());
    if (!outline.IsDone()) {
      return nullptr;
    }
    names.emplace_back(outline.Edge(), face_name(spec.feature, tip, curve));
  }
  const TopoDS_Face band = band_face(gp_Pln(corners.front(), spec.frame.normal()), outline.Wire(), "web");
  return band_prism(band, names, gp_Vec(0, 0, 0), height, {face_name(spec.feature, "start", curve)},
                    {face_name(spec.feature, "end", curve)});
}

// Whether a web's curves lie on a face of the bodies (mitcad#2102): the
// material just behind the sketch plane, against the way the web grows
// (`up`), at the middle of one of them.
bool stands_on(const RibSpec& spec, const std::vector<TopoDS_Edge>& edges, const gp_Dir& up) {
  for (const TopoDS_Edge& edge : edges) {
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt behind = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0)
                              .Translated(gp_Vec(up) * -probe_step(spec));
    for (const ShapePtr& body : spec.bodies) {
      BRepClass3d_SolidClassifier classifier(body->occt(), behind, Precision::Confusion());
      if (classifier.State() == TopAbs_IN) {
        return true;
      }
    }
  }
  return false;
}

// A web's wall with its draft (mitcad#2102): its band in the sketch plane
// (`band`, its edges' names `names`) and the band of the wall at its far
// end (the curve `top` with its sides moved out by the draft, its ends
// `top_ends`) moved by `height`, joined edge to edge by ruled faces named
// after the band's edges: planes between lines, cones between arcs.
ShapePtr drafted_wall(const RibSpec& spec, const TopoDS_Face& band, const Names& names, const WebCurve& top,
                      const std::array<WallEnd, 2>& top_ends, const gp_Vec& height, const std::string& start,
                      const std::string& end) {
  Names top_names;
  const TopoDS_Face top_band = web_band(spec, top, top_ends[0], top_ends[1], top_names);
  if (top_names.size() != names.size()) {
    throw not_built("web");
  }
  gp_Trsf move;
  move.SetTranslation(height);
  BRepBuilderAPI_Transform moved(top_band, move, true);
  NamedFaces faces{{band, start}, {TopoDS::Face(moved.Shape()), end}};
  for (std::size_t k = 0; k < names.size(); ++k) {
    faces.emplace_back(ruled_face(names[k].first, TopoDS::Edge(moved.ModifiedShape(top_names[k].first)), "web"),
                       names[k].second);
  }
  return sewn_solid(faces, "web");
}

// A web (mitcad#1968): one wall per curve, the curves joined where their
// ends meet. At a node of two curves their walls meet in a mitre; a run of
// curves joined so end to end keeps its walls on one side, the side its
// first curve gives (a curve drawn the other way round has its Thickness
// Side turned). At a node of three or more the walls end square and a node
// piece joins them. An end that touches no other curve is free: with
// Extend Ends it is continued along its curve to the bodies, or cut back to
// the wall it crosses when it lies past it. With a draft (mitcad#2102) each
// wall, mitre and node piece is built between its outline in the sketch
// plane and its outline at the far end, its sides moved out there.
ShapePtr web_tool(const RibSpec& spec, std::vector<std::pair<std::string, gp_Pnt>>& notes) {
  const gp_Dir normal = spec.frame.normal();
  gp_Dir up = normal;
  if (spec.flip) {
    up.Reverse();
  }
  std::vector<TopoDS_Wire> wires;
  for (const Path& chain : spec.chains) {
    wires.push_back(detail::path_wire(chain));
  }
  const auto [low, high] = sides(spec.location, spec.thickness);
  std::vector<WebCurve> curves;
  std::vector<TopoDS_Edge> edges;
  Bnd_Box box;
  for (const Path& chain : spec.chains) {
    for (const PathCurve& curve : chain) {
      curves.push_back(web_curve(curve, normal));
      edges.push_back(detail::model_edge(curve.curve));
      box.Add(detail::bounds_of(edges.back()));
    }
  }
  const double tolerance = 1.0e-6 * std::max(1.0, std::sqrt(box.SquareExtent()));
  // A draft (mitcad#2102), thicker where the web meets the bodies: the
  // sides lean out by `lean` per unit the web grows, narrowing it the way
  // it grows when its curves lie on a face of the bodies, else widening it.
  double lean = std::tan(spec.draft_angle);
  if (lean != 0.0 && stands_on(spec, edges, up)) {
    lean = -lean;
  }
  std::string unreached;
  double length = spec.depth ? *spec.depth : reach(spec, wires);
  if (lean != 0.0) {
    // Between the sides at the middle of the first curve.
    const BRepAdaptor_Curve first(edges.front());
    gp_Pnt mid;
    gp_Vec tangent;
    first.D1((first.FirstParameter() + first.LastParameter()) / 2.0, mid, tangent);
    length = drafted_run(spec, lean, length, mid.Translated(left_of(normal, gp_Dir(tangent)) * ((low + high) / 2.0)),
                         up, "web", unreached);
  }
  const gp_Vec height = gp_Vec(up) * length;
  // The nodes where the curves' ends meet, and the ends at each.
  std::vector<gp_Pnt> nodes;
  std::vector<std::vector<std::pair<std::size_t, int>>> at_node;
  for (std::size_t i = 0; i < curves.size(); ++i) {
    for (int k = 0; k < 2; ++k) {
      std::size_t n = 0;
      while (n < nodes.size() && nodes[n].Distance(curves[i].ends[k]) > tolerance) {
        ++n;
      }
      if (n == nodes.size()) {
        nodes.push_back(curves[i].ends[k]);
        at_node.emplace_back();
      }
      curves[i].nodes[k] = static_cast<int>(n);
      at_node[n].emplace_back(i, k);
    }
  }
  // A free end touches no other curve: a T's stem on its bar is not free.
  for (std::size_t i = 0; i < curves.size(); ++i) {
    for (int k = 0; k < 2; ++k) {
      if (at_node[static_cast<std::size_t>(curves[i].nodes[k])].size() != 1) {
        continue;
      }
      const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(curves[i].ends[k]).Vertex();
      bool touches = false;
      for (std::size_t j = 0; j < curves.size() && !touches; ++j) {
        if (j != i) {
          BRepExtrema_DistShapeShape distance(vertex, edges[j]);
          touches = distance.IsDone() && distance.Value() <= tolerance;
        }
      }
      curves[i].loose[k] = !touches;
    }
  }
  // Each run of curves joined end to end at nodes of two keeps its walls on
  // the side its first curve gives: a curve that runs the other way round
  // has its sides turned.
  std::vector<int> turned(curves.size(), -1);
  for (std::size_t first = 0; first < curves.size(); ++first) {
    if (turned[first] >= 0) {
      continue;
    }
    turned[first] = 0;
    std::vector<std::size_t> todo{first};
    while (!todo.empty()) {
      const std::size_t i = todo.back();
      todo.pop_back();
      for (int k = 0; k < 2; ++k) {
        const auto& ends = at_node[static_cast<std::size_t>(curves[i].nodes[k])];
        if (ends.size() != 2) {
          continue;
        }
        const auto& other = ends[0].first == i && ends[0].second == k ? ends[1] : ends[0];
        if (other.first == i || turned[other.first] >= 0) {
          continue;
        }
        // One running into the node and the other out of it run the same way.
        turned[other.first] = other.second != k ? turned[i] : 1 - turned[i];
        todo.push_back(other.first);
      }
    }
  }
  for (std::size_t i = 0; i < curves.size(); ++i) {
    curves[i].low = turned[i] != 0 ? -high : low;
    curves[i].high = turned[i] != 0 ? -low : high;
  }
  std::vector<std::array<WallEnd, 2>> ends(curves.size());
  for (std::size_t i = 0; i < curves.size(); ++i) {
    for (int k = 0; k < 2; ++k) {
      ends[i][static_cast<std::size_t>(k)] = square_end(curves[i], k, normal);
    }
  }
  // With a draft, the walls at their far end: their sides moved out.
  const double out = length * lean;
  std::vector<WebCurve> tops = curves;
  std::vector<std::array<WallEnd, 2>> top_ends(curves.size());
  for (std::size_t i = 0; i < curves.size(); ++i) {
    tops[i].low -= out;
    tops[i].high += out;
    for (int k = 0; k < 2; ++k) {
      top_ends[i][static_cast<std::size_t>(k)] = square_end(tops[i], k, normal);
    }
  }
  // A mitre reaches at most a thickness past the wall's outer side.
  const double limit = std::max(std::abs(low), std::abs(high)) + spec.thickness;
  const double top_limit = std::max(std::abs(low - out), std::abs(high + out)) + spec.thickness + 2.0 * out;
  for (const auto& meeting : at_node) {
    if (meeting.size() == 2 && meeting[0].first != meeting[1].first) {
      const std::size_t i = meeting[0].first;
      const std::size_t j = meeting[1].first;
      const int ki = meeting[0].second;
      const int kj = meeting[1].second;
      WallEnd& end_i = ends[i][static_cast<std::size_t>(ki)];
      WallEnd& end_j = ends[j][static_cast<std::size_t>(kj)];
      double outer = 0.0;
      mitre(curves[i], ki, curves[j], kj, normal, limit, end_i, end_j, &outer);
      if (lean == 0.0) {
        continue;
      }
      WallEnd& top_i = top_ends[i][static_cast<std::size_t>(ki)];
      WallEnd& top_j = top_ends[j][static_cast<std::size_t>(kj)];
      double top_outer = 0.0;
      mitre(tops[i], ki, tops[j], kj, normal, top_limit, top_i, top_j, &top_outer);
      // A mitre cut off at one end of the wall and not at the other is cut
      // at both, at the same share of its length, so that its corner runs
      // from the sketch plane to the far end.
      if (end_i.corner.has_value() != top_i.corner.has_value() && outer > 0.0 && top_outer > 0.0) {
        if (end_i.corner) {
          top_i = square_end(tops[i], ki, normal);
          top_j = square_end(tops[j], kj, normal);
          mitre(tops[i], ki, tops[j], kj, normal, top_outer * limit / outer, top_i, top_j);
        } else {
          end_i = square_end(curves[i], ki, normal);
          end_j = square_end(curves[j], kj, normal);
          mitre(curves[i], ki, curves[j], kj, normal, outer * top_limit / top_outer, end_i, end_j);
        }
      }
    }
  }
  const double run_out = reach(spec, wires) + std::abs(spec.end_offset);
  std::vector<ShapePtr> parts;
  for (std::size_t i = 0; i < curves.size(); ++i) {
    const WebCurve& w = curves[i];
    const std::string& key = w.curve->name;
    Names names;
    const TopoDS_Face band = web_band(spec, w, ends[i][0], ends[i][1], names);
    const std::string start = face_name(spec.feature, "start", key);
    const std::string end = face_name(spec.feature, "end", key);
    const ShapePtr tool = lean == 0.0 ? band_prism(band, names, gp_Vec(0, 0, 0), height, {start}, {end})
                                      : drafted_wall(spec, band, names, tops[i], top_ends[i], height, start, end);
    // Its free ends (with Extend Ends).
    std::vector<RibEnd> loose;
    for (int k = 0; spec.extend_ends && k < 2; ++k) {
      if (!w.loose[k]) {
        continue;
      }
      const std::string n = k == 0 ? "0" : "1";
      RibEnd e;
      e.point = w.ends[k];
      e.outward = k == 0 ? w.travel[0].Reversed() : w.travel[1];
      e.across = normal.Crossed(w.travel[k]);
      e.label = "an end of the web's curves";
      e.along = start;
      e.tip = face_name(spec.feature, "tip" + n, key);
      e.extend = face_name(spec.feature, "extend" + n, key);
      e.reach = face_name(spec.feature, "reach" + n, key);
      e.far = end;
      e.near_cap = face_name(spec.feature, "wall2", key);
      e.far_cap = face_name(spec.feature, "wall1", key);
      e.other_tip = face_name(spec.feature, k == 0 ? "tip1" : "tip0", key);
      e.probe = e.point.Translated(gp_Vec(up) * probe_step(spec) + gp_Vec(e.across) * ((w.low + w.high) / 2.0));
      loose.push_back(e);
    }
    // Just inside the middle of the curve.
    const BRepAdaptor_Curve along(edges[i]);
    gp_Pnt mid;
    gp_Vec tangent;
    along.D1((along.FirstParameter() + along.LastParameter()) / 2.0, mid, tangent);
    const gp_Pnt middle = mid.Translated(gp_Vec(up) * probe_step(spec) +
                                         left_of(normal, gp_Dir(tangent)) * ((w.low + w.high) / 2.0));
    std::vector<bool> cut;
    const ShapePtr drawn = drawn_part(spec, tool, {start}, end, loose, middle, run_out, "web", cut, unreached);
    parts.push_back(drawn);
    for (std::size_t f = 0; f < loose.size(); ++f) {
      if (cut[f]) {
        continue;
      }
      if (const ShapePtr more =
              extension(spec, *drawn, loose[f], height, w.low, w.high, lean, run_out, notes, "web")) {
        parts.push_back(more);
      }
    }
  }
  // Nodes of three or more ends: a piece that joins the walls there.
  for (std::size_t n = 0; n < nodes.size(); ++n) {
    if (at_node[n].size() < 3) {
      continue;
    }
    std::vector<gp_Pnt> points{nodes[n]};
    std::vector<gp_Pnt> top;
    if (lean != 0.0) {
      top.push_back(nodes[n]);
    }
    for (const auto& [i, k] : at_node[n]) {
      points.push_back(ends[i][static_cast<std::size_t>(k)].low);
      points.push_back(ends[i][static_cast<std::size_t>(k)].high);
      if (lean != 0.0) {
        top.push_back(top_ends[i][static_cast<std::size_t>(k)].low);
        top.push_back(top_ends[i][static_cast<std::size_t>(k)].high);
      }
    }
    const auto [i, k] = at_node[n].front();
    ShapePtr piece = node_piece(spec, points, curves[i].curve->name, k == 0 ? "tip0" : "tip1", height, top);
    if (piece && !spec.depth) {
      piece = to_bodies(piece, spec.bodies, face_name(spec.feature, "start", curves[i].curve->name),
                        face_name(spec.feature, "end", curves[i].curve->name), "web", unreached);
    }
    if (piece) {
      parts.push_back(piece);
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
  if (spec.web && spec.direction) {
    throw std::invalid_argument("a web grows along the sketch normal");
  }
  if (!std::isfinite(spec.draft_angle) || std::abs(spec.draft_angle) >= kPi / 2.0) {
    throw std::invalid_argument(std::string("the ") + what + "'s draft angle must lie between -90 and 90 degrees");
  }
  return detail::run(what, [&] {
    std::vector<std::pair<std::string, gp_Pnt>> notes;
    const ShapePtr tool = spec.web ? web_tool(spec, notes) : rib_tool(spec, notes);
    const ShapePtr result = detail::finished(*tool, spec.web ? "the web" : "the rib");
    // The ends that stay as drawn (mitcad#1746), each at its end.
    for (const auto& [note, at] : notes) {
      result->add_note(note, {Shape::NotePlace::at(at.X(), at.Y(), at.Z())});
    }
    return result;
  });
}

} // namespace mitcad::geometry
