// SPDX-License-Identifier: MIT
#include "taper_events.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <limits>
#include <memory>
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
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepFill.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Curve.hxx>
#include <Geom_OffsetCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

// The message a person reads (KernelError::TAPER_CLOSES_PROFILE).
constexpr const char* kVanishes =
    "the taper closes the whole profile before the end of the extrusion; make the angle or the distance smaller";

// (KernelError::TAPER_CLOSES_PART, mitcad#1854.)
constexpr const char* kClosesPart =
    "the taper closes part of the profile on its way, and the geometry kernel could not build that; make the angle "
    "smaller";

// A length or an angle in words: up to three decimals, no trailing zeros.
std::string number_text(const double value) {
  char text[64];
  std::snprintf(text, sizeof text, "%.3f", value);
  std::string out = text;
  while (!out.empty() && out.back() == '0') {
    out.pop_back();
  }
  if (!out.empty() && out.back() == '.') {
    out.pop_back();
  }
  return out == "-0" ? "0" : out;
}

double diagonal_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  return box.IsVoid() ? 0.0 : std::sqrt(box.SquareExtent());
}

// How far the shape reaches along the pull from the neutral plane.
double reach_of(const TopoDS_Shape& shape, const gp_Pln& neutral, const gp_Dir& pull) {
  double reach = -std::numeric_limits<double>::infinity();
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
    reach = std::max(reach, gp_Vec(neutral.Location(), p).Dot(gp_Vec(pull)));
  }
  return reach;
}

// An edge of the near cap and the wall it bounds. Its curve runs from
// `first` to `last`; the cap's loop runs along it the same way or (a
// reversed edge) the other. The wall moves out of the material, along
// `side` (C'(u) x N) normalised, N the cap's normal; `out_first` and
// `out_last` are that direction at the curve's ends, `at_first` and
// `at_last` the points of its vertices there (shared with the edges next
// to it, so that the pieces of neighbouring walls meet in the same points).
struct Wall {
  TopoDS_Edge edge;
  double first = 0.0;
  double last = 0.0;
  double side = 1.0;
  gp_Pnt at_first;
  gp_Pnt at_last;
  gp_Vec out_first;
  gp_Vec out_last;
  NameList names;
};

// Which way out of the material is, at the curve's parameter u.
std::optional<gp_Vec> out_at(const Wall& wall, const gp_Dir& normal, double u) {
  const BRepAdaptor_Curve curve(wall.edge);
  gp_Pnt p;
  gp_Vec tangent;
  curve.D1(u, p, tangent);
  gp_Vec across = tangent.Crossed(gp_Vec(normal));
  if (across.Magnitude() <= gp::Resolution()) {
    return std::nullopt;
  }
  return across.Normalized() * wall.side;
}

// The loop's direction along the edge at the curve's parameter u.
std::optional<gp_Vec> run_at(const Wall& wall, double u) {
  const BRepAdaptor_Curve curve(wall.edge);
  gp_Pnt p;
  gp_Vec tangent;
  curve.D1(u, p, tangent);
  if (tangent.Magnitude() <= gp::Resolution()) {
    return std::nullopt;
  }
  tangent.Normalize();
  return wall.edge.Orientation() == TopAbs_REVERSED ? tangent.Reversed() : tangent;
}

// A face of a piece and its names.
struct Facet {
  std::vector<gp_Pnt> outline;
  NameList names;
};

// The faces sewn into one closed shell and made a solid with its material
// inside, each face named; null when they do not close up.
ShapePtr sewn_solid(BRepBuilderAPI_Sewing& sewing, const std::vector<TopoDS_Shape>& faces,
                    const std::vector<NameList>& names) {
  sewing.Perform();
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL); it.More(); it.Next(), ++shells) {
    shell = TopoDS::Shell(it.Current());
  }
  if (shells != 1 || !BRep_Tool::IsClosed(shell)) {
    return nullptr;
  }
  BRepBuilderAPI_MakeSolid make(shell);
  if (!make.IsDone()) {
    return nullptr;
  }
  TopoDS_Solid made = make.Solid();
  if (!BRepLib::OrientClosedSolid(made)) {
    return nullptr;
  }
  FaceNamer namer(made);
  for (std::size_t i = 0; i < faces.size(); ++i) {
    const TopoDS_Shape image = sewing.IsModified(faces[i]) ? sewing.Modified(faces[i]) : faces[i];
    for (const std::string& name : names[i]) {
      namer.add(image, name);
    }
  }
  return namer.shape();
}

// A solid of planar polygons whose corners neighbouring faces share.
ShapePtr polyhedron(const std::vector<Facet>& facets, double tolerance) {
  BRepBuilderAPI_Sewing sewing(tolerance);
  std::vector<TopoDS_Shape> faces;
  std::vector<NameList> names;
  for (const Facet& facet : facets) {
    BRepBuilderAPI_MakePolygon polygon;
    for (const gp_Pnt& p : facet.outline) {
      polygon.Add(p);
    }
    polygon.Close();
    if (!polygon.IsDone()) {
      return nullptr;
    }
    BRepBuilderAPI_MakeFace face(polygon.Wire(), true);
    if (!face.IsDone()) {
      return nullptr;
    }
    faces.push_back(face.Face());
    names.push_back(facet.names);
    sewing.Add(face.Face());
  }
  return sewn_solid(sewing, faces, names);
}

// The wedge a straight wall sweeps: between the wall (from the edge up the
// prism) and the wall leaned (from the edge to its offset by d at the top),
// across the top, and at its ends the planes square to the edge.
ShapePtr line_band(const Wall& wall, const gp_Vec& up, double d, const NameList& far, double tolerance) {
  const gp_Pnt& a = wall.at_first;
  const gp_Pnt& b = wall.at_last;
  const gp_Pnt a_top = a.Translated(up);
  const gp_Pnt b_top = b.Translated(up);
  const gp_Pnt a_out = a_top.Translated(wall.out_first * d);
  const gp_Pnt b_out = b_top.Translated(wall.out_last * d);
  return polyhedron({{{a, b, b_top, a_top}, wall.names},
                     {{a, a_out, b_out, b}, wall.names},
                     {{a_top, b_top, b_out, a_out}, far},
                     {{a, a_top, a_out}, {}},
                     {{b, b_out, b_top}, {}}},
                    tolerance);
}

// The band an arc's wall sweeps: its section at the arc's start (the edge
// up the prism, the leaned wall out to the offset at the top) turned about
// the arc's axis along the arc. Leaning toward the centre past it, the
// offset arc vanishes where it reaches the centre: the section ends on the
// axis there.
ShapePtr arc_band(const Wall& wall, const gp_Dir& pull, double height, double d, const NameList& far,
                  double tolerance) {
  const BRepAdaptor_Curve curve(wall.edge);
  const gp_Circ circle = curve.Circle();
  const gp_Pnt centre = circle.Location();
  const gp_Vec up = gp_Vec(pull) * height;
  const gp_Pnt& start = wall.at_first;
  const gp_Vec radial(centre, start);
  const double r = radial.Magnitude();
  if (r <= tolerance) {
    return nullptr;
  }
  const double grows = wall.out_first.Dot(radial / r); // +1 away from the centre, -1 toward it
  const double far_radius = r + grows * d;
  std::vector<gp_Pnt> section{start, start.Translated(up)};
  if (far_radius > tolerance) {
    section.push_back(start.Translated(up).Translated(wall.out_first * d));
  } else if (far_radius > -tolerance) {
    section.push_back(centre.Translated(up));
  } else {
    // The offset reaches the centre at r / (r - far_radius) of the height.
    section.push_back(centre.Translated(up));
    section.push_back(centre.Translated(up * (r / (r - far_radius))));
  }
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt& p : section) {
    polygon.Add(p);
  }
  polygon.Close();
  if (!polygon.IsDone()) {
    return nullptr;
  }
  BRepBuilderAPI_MakeFace face(polygon.Wire(), true);
  if (!face.IsDone()) {
    return nullptr;
  }
  // (The circle's parameter runs about its axis: turning by the arc's span
  // about it takes the start to the end.)
  BRepPrimAPI_MakeRevol revolve(face.Face(), gp_Ax1(centre, circle.Axis().Direction()), wall.last - wall.first);
  revolve.Build();
  if (!revolve.IsDone() || revolve.Shape().IsNull()) {
    return nullptr;
  }
  TopoDS_Shape solid;
  for (TopExp_Explorer s(revolve.Shape(), TopAbs_SOLID); s.More(); s.Next()) {
    solid = s.Current();
  }
  if (solid.IsNull()) {
    return nullptr;
  }
  // The wall (the cylinder) and the leaned wall (the cone) carry the wall's
  // names, the top the far cap's; the ends and the axis none.
  FaceNamer namer(solid);
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (surface.GetType() == GeomAbs_Cylinder || surface.GetType() == GeomAbs_Cone) {
      for (const std::string& name : wall.names) {
        namer.add(f.Current(), name);
      }
    } else if (surface.GetType() == GeomAbs_Plane && surface.Plane().Axis().Direction().IsParallel(pull, 1.0e-9)) {
      for (const std::string& name : far) {
        namer.add(f.Current(), name);
      }
    }
  }
  return namer.shape();
}

// The band any other curve's wall sweeps (a spline, an ellipse): the wall
// up the prism and the leaned wall ruled from the edge to its offset at
// the top (OCCT's offset curve), the top between the edge and its offset
// there, and the triangles at its ends; sewn.
ShapePtr curve_band(const Wall& wall, const gp_Dir& normal, const gp_Vec& up, double d, const NameList& far,
                    double tolerance) {
  double first = 0.0;
  double last = 0.0;
  const TopoDS_Edge forward = TopoDS::Edge(wall.edge.Oriented(TopAbs_FORWARD));
  const occ::handle<Geom_Curve> base = BRep_Tool::Curve(forward, first, last);
  if (base.IsNull()) {
    return nullptr;
  }
  // Geom_OffsetCurve moves a point by d along (C'(u) x V) normalised: V is
  // the cap's normal turned so that this is out of the material.
  const gp_Dir lean = wall.side > 0.0 ? normal : normal.Reversed();
  const occ::handle<Geom_Curve> trimmed = new Geom_TrimmedCurve(base, first, last);
  const occ::handle<Geom_Curve> offset = new Geom_OffsetCurve(trimmed, d, lean);
  const occ::handle<Geom_Curve> top = occ::down_cast<Geom_Curve>(base->Translated(up));
  const occ::handle<Geom_Curve> top_offset = occ::down_cast<Geom_Curve>(offset->Translated(up));
  if (top.IsNull() || top_offset.IsNull()) {
    return nullptr;
  }
  const gp_Pnt a = wall.at_first;
  const gp_Pnt b = wall.at_last;
  const gp_Pnt a_top = a.Translated(up);
  const gp_Pnt b_top = b.Translated(up);
  const gp_Pnt a_out = top_offset->Value(first);
  const gp_Pnt b_out = top_offset->Value(last);
  BRepBuilderAPI_MakeEdge bottom_edge(base, first, last);
  BRepBuilderAPI_MakeEdge top_edge(top, first, last);
  BRepBuilderAPI_MakeEdge out_edge(top_offset, first, last);
  if (!bottom_edge.IsDone() || !top_edge.IsDone() || !out_edge.IsDone()) {
    return nullptr;
  }
  const TopoDS_Face wall_face = BRepFill::Face(bottom_edge.Edge(), top_edge.Edge());
  const TopoDS_Face leaned_face = BRepFill::Face(bottom_edge.Edge(), out_edge.Edge());
  BRepBuilderAPI_MakeWire outline;
  outline.Add(top_edge.Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(top_edge.Vertex2(), out_edge.Vertex2()).Edge());
  outline.Add(out_edge.Edge());
  outline.Add(BRepBuilderAPI_MakeEdge(out_edge.Vertex1(), top_edge.Vertex1()).Edge());
  if (!outline.IsDone() || wall_face.IsNull() || leaned_face.IsNull()) {
    return nullptr;
  }
  BRepBuilderAPI_MakeFace top_face(outline.Wire(), true);
  if (!top_face.IsDone()) {
    return nullptr;
  }
  BRepBuilderAPI_Sewing sewing(tolerance);
  std::vector<TopoDS_Shape> faces{wall_face, leaned_face, top_face.Face()};
  std::vector<NameList> names{wall.names, wall.names, far};
  for (const std::vector<gp_Pnt>& end : {std::vector<gp_Pnt>{a, a_top, a_out}, std::vector<gp_Pnt>{b, b_out, b_top}}) {
    BRepBuilderAPI_MakePolygon polygon(end[0], end[1], end[2], true);
    if (!polygon.IsDone()) {
      return nullptr;
    }
    BRepBuilderAPI_MakeFace face(polygon.Wire(), true);
    if (!face.IsDone()) {
      return nullptr;
    }
    faces.push_back(face.Face());
    names.emplace_back();
  }
  for (const TopoDS_Shape& face : faces) {
    sewing.Add(face);
  }
  return sewn_solid(sewing, faces, names);
}

// The cut of the tools from the prism, faces in one plane or surface
// merged, names carried; null where the boolean fails.
ShapePtr cut_named(const ShapePtr& prism, const std::vector<ShapePtr>& tools, double fuzzy) {
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(prism->occt());
  NCollection_List<TopoDS_Shape> list;
  for (const ShapePtr& tool : tools) {
    list.Append(tool->occt());
  }
  cut.SetArguments(arguments);
  cut.SetTools(list);
  cut.SetNonDestructive(true);
  if (fuzzy > 0.0) {
    cut.SetFuzzyValue(fuzzy);
  }
  build(cut);
  if (!cut.IsDone() || cut.HasErrors()) {
    return nullptr;
  }
  cut.SimplifyResult();
  FaceNamer namer(cut.Shape());
  namer.carry(cut, *prism);
  for (const ShapePtr& tool : tools) {
    namer.carry(cut, *tool);
  }
  return namer.shape();
}

} // namespace

std::string taper_vanishes_error(const Shape& solid, int near, double angle, double height, double reached) {
  const std::string detail = std::string(kVanishes) + " (leaning by " +
                             number_text(std::abs(angle) * 180.0 / 3.14159265358979323846) +
                             " degrees, the profile closes " + number_text(std::max(0.0, reached)) +
                             " mm from the sketch plane, before the extrusion's end " + number_text(height) +
                             " mm from it)";
  // The profile painted: the middle of each edge of its cap.
  std::vector<ErrorPlace> places;
  try {
    OCC_CATCH_SIGNALS
    if (near >= 0 && near < solid.face_count()) {
      for (TopExp_Explorer e(solid.face(near), TopAbs_EDGE); e.More() && places.size() < 32; e.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
        if (BRep_Tool::Degenerated(edge)) {
          continue;
        }
        const BRepAdaptor_Curve curve(edge);
        places.push_back(point_place(curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()))));
      }
    }
  } catch (const Standard_Failure&) {
    // (The places found so far.)
  }
  return placed_error(kVanishes, detail, places);
}

std::string taper_closes_part(const std::string& detail, const std::vector<ErrorPlace>& places) {
  return placed_error(kClosesPart, std::string(kClosesPart) + " (" + detail + ")", places);
}

ShapePtr resolved_taper(const Shape& solid, const std::vector<bool>& leaned, int near, const gp_Pln& neutral,
                        const gp_Dir& pull, double angle) {
  if (near < 0 || near >= solid.face_count() || std::abs(angle) < 1.0e-12) {
    return nullptr;
  }
  const TopoDS_Face cap = TopoDS::Face(solid.face(near).Oriented(TopAbs_FORWARD));
  const BRepAdaptor_Surface plane(cap);
  if (plane.GetType() != GeomAbs_Plane) {
    return nullptr;
  }
  // The cap's own normal (its plane's X x Y; an indirect frame's axis
  // points the other way).
  const gp_Ax3 frame = plane.Plane().Position();
  const gp_Dir normal = frame.XDirection().Crossed(frame.YDirection());
  const double height = reach_of(solid.occt(), neutral, pull);
  const double scale = std::max(1.0, diagonal_of(solid.occt()));
  const double tolerance = std::max(Precision::Confusion(), 1.0e-9 * scale);
  if (!(height > tolerance)) {
    return nullptr;
  }
  // Each wall's offset at the top: out of the material for a taper that
  // widens.
  const double d = height * std::tan(angle);
  if (!(std::abs(d) > tolerance)) {
    return nullptr;
  }
  const gp_Vec up = gp_Vec(pull) * height;

  // The names of the wall each edge of the cap bounds, and of the caps.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(solid.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  NameList far;
  for (int i = 0; i < solid.face_count(); ++i) {
    if (i != near && !leaned[static_cast<std::size_t>(i)]) {
      const NameList& names = solid.face_names(i);
      far.insert(far.end(), names.begin(), names.end());
    }
  }

  // The cap's loops, each edge as the loop runs with its wall.
  std::vector<std::vector<Wall>> loops;
  const double step = std::max(1.0e-6, 1.0e-6 * scale);
  for (TopExp_Explorer w(cap, TopAbs_WIRE); w.More(); w.Next()) {
    std::vector<Wall> loop;
    for (BRepTools_WireExplorer e(TopoDS::Wire(w.Current()), cap); e.More(); e.Next()) {
      const TopoDS_Edge& edge = e.Current();
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      Wall wall;
      wall.edge = edge;
      const BRepAdaptor_Curve curve(edge);
      wall.first = curve.FirstParameter();
      wall.last = curve.LastParameter();
      // The material lies on the left of the loop as it runs about the
      // cap's normal: out of it is the run crossed with the normal (checked
      // with a point just across the edge's middle).
      wall.side = edge.Orientation() == TopAbs_REVERSED ? -1.0 : 1.0;
      const double middle = 0.5 * (wall.first + wall.last);
      const std::optional<gp_Vec> out = out_at(wall, normal, middle);
      if (!out) {
        return nullptr;
      }
      const gp_Pnt probe = curve.Value(middle).Translated(*out * step);
      if (BRepClass_FaceClassifier(cap, probe, Precision::Confusion()).State() == TopAbs_IN) {
        wall.side = -wall.side;
      }
      const std::optional<gp_Vec> out_first = out_at(wall, normal, wall.first);
      const std::optional<gp_Vec> out_last = out_at(wall, normal, wall.last);
      if (!out_first || !out_last) {
        return nullptr;
      }
      wall.out_first = *out_first;
      wall.out_last = *out_last;
      TopoDS_Vertex first_vertex;
      TopoDS_Vertex last_vertex;
      TopExp::Vertices(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD)), first_vertex, last_vertex);
      if (first_vertex.IsNull() || last_vertex.IsNull()) {
        return nullptr;
      }
      wall.at_first = BRep_Tool::Pnt(first_vertex);
      wall.at_last = BRep_Tool::Pnt(last_vertex);
      const int index = faces_of.FindIndex(edge);
      if (index > 0) {
        for (const TopoDS_Shape& face : faces_of.FindFromIndex(index)) {
          const int f = solid.face_index(face);
          if (f >= 0 && f != near && leaned[static_cast<std::size_t>(f)]) {
            const NameList& names = solid.face_names(f);
            wall.names.insert(wall.names.end(), names.begin(), names.end());
          }
        }
      }
      loop.push_back(wall);
    }
    if (!loop.empty()) {
      loops.push_back(std::move(loop));
    }
  }
  if (loops.empty()) {
    return nullptr;
  }

  std::vector<ShapePtr> pieces;
  pieces.push_back(std::make_shared<Shape>(solid));
  try {
    OCC_CATCH_SIGNALS
    for (const std::vector<Wall>& loop : loops) {
      // The band each wall sweeps.
      for (const Wall& wall : loop) {
        const GeomAbs_CurveType type = BRepAdaptor_Curve(wall.edge).GetType();
        const ShapePtr band = type == GeomAbs_Line     ? line_band(wall, up, d, far, tolerance)
                              : type == GeomAbs_Circle ? arc_band(wall, pull, height, d, far, tolerance)
                                                       : curve_band(wall, normal, up, d, far, step);
        if (!band) {
          return nullptr;
        }
        pieces.push_back(band);
      }
      // The mitre at each corner where the offsets of the walls part: a
      // pyramid from the corner at the near cap to the mitre at the top,
      // its sides in the two leaned walls (p1 + s t1 = p2 - u t2, the two
      // offsets run on along their ends' tangents to where they meet).
      const std::size_t count = loop.size();
      for (std::size_t i = 0; i < count; ++i) {
        const Wall& before = loop[i];
        const Wall& after = loop[(i + 1) % count];
        const bool before_forward = before.edge.Orientation() != TopAbs_REVERSED;
        const bool after_forward = after.edge.Orientation() != TopAbs_REVERSED;
        const double u1 = before_forward ? before.last : before.first;
        const double u2 = after_forward ? after.first : after.last;
        const std::optional<gp_Vec> t1 = run_at(before, u1);
        const std::optional<gp_Vec> t2 = run_at(after, u2);
        if (!t1 || !t2) {
          return nullptr;
        }
        const gp_Vec& n1 = before_forward ? before.out_last : before.out_first;
        const gp_Vec& n2 = after_forward ? after.out_first : after.out_last;
        const gp_Pnt corner = after_forward ? after.at_first : after.at_last;
        const double turn = t1->Crossed(*t2).Dot(gp_Vec(normal));
        if (std::abs(turn) < 1.0e-9) {
          continue; // tangent: the offsets meet
        }
        const gp_Pnt p1 = corner.Translated(n1 * d);
        const gp_Pnt p2 = corner.Translated(n2 * d);
        const gp_Vec w(p1, p2);
        const double s = w.Crossed(*t2).Dot(gp_Vec(normal)) / turn;
        const double u = t1->Crossed(w).Dot(gp_Vec(normal)) / turn;
        if (!(s > tolerance) || !(u > tolerance)) {
          continue; // the offsets cross: the bands overlap there
        }
        // (The top's points as the bands make theirs, so that the faces
        // they share are the same.)
        const gp_Pnt top = corner.Translated(up);
        const gp_Pnt top1 = top.Translated(n1 * d);
        const gp_Pnt top2 = top.Translated(n2 * d);
        const gp_Pnt top_mitre = top1.Translated(*t1 * s);
        const ShapePtr pyramid = polyhedron({{{corner, top1, top}, {}},
                                             {{corner, top_mitre, top1}, before.names},
                                             {{corner, top2, top_mitre}, after.names},
                                             {{corner, top, top2}, {}},
                                             {{top, top1, top_mitre, top2}, far}},
                                            tolerance);
        if (!pyramid) {
          return nullptr;
        }
        pieces.push_back(pyramid);
      }
    }
  } catch (const Standard_Failure&) {
    return nullptr;
  }

  // Every piece a valid solid before the booleans meet them (a curve's
  // offset that loops, where the curve bends tighter than the offset,
  // makes a band whose top crosses itself): else as before.
  for (std::size_t i = 1; i < pieces.size(); ++i) {
    if (!pieces[i] || pieces[i]->occt().IsNull() || !is_valid(pieces[i]->occt())) {
      return nullptr;
    }
  }

  // Widening: the prism with the bands; narrowing: without them. Exactly,
  // then with the pieces' corners a little apart counted as one.
  const ShapePtr prism = pieces.front();
  const std::vector<ShapePtr> tools(pieces.begin() + 1, pieces.end());
  ShapePtr result;
  for (const double fuzzy : {0.0, std::max(1.0e-7, 1.0e-7 * scale)}) {
    ShapePtr built;
    try {
      OCC_CATCH_SIGNALS
      built = d > 0.0 ? fused(pieces, "the taper", true, fuzzy) : cut_named(prism, tools, fuzzy);
    } catch (const Cancelled&) {
      throw;
    } catch (const std::runtime_error& error) {
      if (std::string(error.what()).find("out of memory") != std::string::npos) {
        throw;
      }
      built = nullptr;
    } catch (const Standard_Failure&) {
      built = nullptr;
    }
    if (!built || built->occt().IsNull()) {
      continue;
    }
    int solids = 0;
    for (TopExp_Explorer it(built->occt(), TopAbs_SOLID); it.More(); it.Next()) {
      ++solids;
    }
    if (solids == 0) {
      // Narrowing, nothing left at all.
      throw std::invalid_argument(taper_vanishes_error(solid, near, angle, height, 0.0));
    }
    if (!is_valid(built->occt())) {
      continue;
    }
    result = built;
    break;
  }
  if (!result) {
    return nullptr;
  }
  GProp_GProps volume;
  BRepGProp::VolumeProperties(result->occt(), volume);
  const double reached = reach_of(result->occt(), neutral, pull);
  if (!(volume.Mass() > tolerance * scale * scale) ||
      reached < height - std::max(1.0e-6, 1.0e-7 * scale)) {
    throw std::invalid_argument(taper_vanishes_error(solid, near, angle, height, reached));
  }
  return result;
}

} // namespace mitcad::geometry::detail
