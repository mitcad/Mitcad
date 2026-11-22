// SPDX-License-Identifier: MIT
// The end of a groove rounded as a ball mill ends it (mitcad#2154; see
// groove_end.hpp).

#include "groove_end.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <memory>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLProp_SLProps.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Torus.hxx>
#include <gp_Vec.hxx>

#include "face_join.hpp"
#include "face_select.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

// How far the groove's surfaces may lie from where they should (mm, per mm
// of the circle's radius at least 1).
double geometric_tolerance(double radius) { return 1.0e-6 * std::max(1.0, radius); }

constexpr double kPi = 3.14159265358979323846;

// The outward normal of `face` at the point of its surface nearest `at`.
std::optional<gp_Dir> outward_normal(const TopoDS_Face& face, const gp_Pnt& at) {
  GeomAPI_ProjectPointOnSurf projection(at, BRep_Tool::Surface(face));
  if (projection.NbPoints() == 0) {
    return std::nullopt;
  }
  double u = 0.0;
  double v = 0.0;
  projection.LowerDistanceParameters(u, v);
  const BRepAdaptor_Surface surface(face, false);
  BRepLProp_SLProps props(surface, u, v, 1, 1.0e-9);
  if (!props.IsNormalDefined()) {
    return std::nullopt;
  }
  gp_Dir normal = props.Normal();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal;
}

// The centre of a face's area.
gp_Pnt area_centre(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.CentreOfMass();
}

// One valid solid with closed shells.
bool one_closed_solid(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1 && BRepCheck_Analyzer(shape).IsValid();
}

// The common part or the difference of two solids; null where the boolean
// fails.
std::optional<TopoDS_Shape> common(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Common op(a, b);
  if (!op.IsDone() || op.HasErrors()) {
    return std::nullopt;
  }
  return op.Shape();
}

std::optional<TopoDS_Shape> cut(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Cut op(a, b);
  if (!op.IsDone() || op.HasErrors()) {
    return std::nullopt;
  }
  return op.Shape();
}

// The frame of the tube's piece at the end: for a groove along an arc the
// torus's frame (its axis turned so that the angle grows into the groove,
// its X direction to the circle's centre), else the circle's centre with
// the axis into the groove and the X direction `up` (the seam's side).
gp_Ax2 tube_frame(const GrooveEnd& end, const gp_Dir& up) {
  const gp_Pnt centre = end.circle.Location();
  if (end.bent) {
    const gp_Pnt on_axis = end.axis.Location();
    const gp_Dir d = end.axis.Direction();
    const gp_Vec along(d);
    const gp_Pnt foot = on_axis.Translated(along * gp_Vec(on_axis, centre).Dot(along));
    const gp_Dir x(gp_Vec(foot, centre));
    const gp_Dir axis = (gp_Vec(d) ^ gp_Vec(x)).Dot(gp_Vec(end.into)) > 0.0 ? d : d.Reversed();
    return gp_Ax2(foot, axis, x);
  }
  return gp_Ax2(centre, end.into, up);
}

// The tube's piece from the end cap into the groove: `depth` long along a
// straight groove, `depth` radians round the spine of a bent one.
TopoDS_Shape tube_piece(const GrooveEnd& end, const gp_Ax2& frame, double depth) {
  const double radius = end.circle.Radius();
  if (end.bent) {
    return BRepPrimAPI_MakeTorus(frame, end.spine, radius, depth).Shape();
  }
  const TopoDS_Edge rim = BRepBuilderAPI_MakeEdge(gp_Circ(frame, radius)).Edge();
  const TopoDS_Face disc = BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(rim).Wire(), true).Face();
  return BRepPrimAPI_MakePrism(disc, gp_Vec(end.into) * depth).Shape();
}

// The tube's angle (bent) or length (straight) that holds `length` along
// its spine.
double tube_depth(const GrooveEnd& end, double length) {
  if (end.bent) {
    return std::min(0.75 * kPi, length / end.spine);
  }
  return length;
}

// What the rounding of radius `radius` adds at the groove's end on the
// extended body (the tube closed by the end cap's whole disc), within the
// tube: below the groove's radius the geometry kernel's rounding of the
// whole circle in a block holding the tube's piece; at it the tube's piece
// up to the ball's centre less the ball (`clip`, the faces beyond's
// material side, taken first, so that the ball touches the end cap's plane
// outside what is cut where the circle's centre lies outside the body).
std::optional<TopoDS_Shape> added_by_rounding(const GrooveEnd& end, const gp_Ax2& frame, double radius,
                                              const std::vector<TopoDS_Shape>& clip) {
  const double rc = end.circle.Radius();
  const gp_Pnt centre = end.circle.Location();
  const auto clipped = [&](TopoDS_Shape shape) -> std::optional<TopoDS_Shape> {
    for (const TopoDS_Shape& half : clip) {
      const std::optional<TopoDS_Shape> kept = common(shape, half);
      if (!kept) {
        return std::nullopt;
      }
      shape = *kept;
    }
    return shape;
  };
  if (std::abs(radius - rc) <= kGrooveRadiusTolerance) {
    // The ball: on the axis, `radius` from the end cap's plane; along an
    // arc on the spine where it lies `radius` from that plane.
    double depth = radius;
    gp_Pnt ball = centre.Translated(gp_Vec(end.into) * radius);
    if (end.bent) {
      if (radius >= end.spine) {
        return std::nullopt;
      }
      depth = std::asin(radius / end.spine);
      const gp_Vec x(frame.XDirection());
      const gp_Vec y(frame.YDirection());
      ball = frame.Location().Translated(x * (end.spine * std::cos(depth)) + y * (end.spine * std::sin(depth)));
    }
    const std::optional<TopoDS_Shape> within = clipped(tube_piece(end, frame, depth));
    if (!within) {
      return std::nullopt;
    }
    return cut(*within, BRepPrimAPI_MakeSphere(ball, radius).Shape());
  }
  // The extended body: a block round the tube's piece (a radius past it),
  // the piece cut out of it, the circle at the end cap rounded.
  const TopoDS_Shape tube = tube_piece(end, frame, tube_depth(end, 2.0 * radius + rc));
  Bnd_Box box;
  BRepBndLib::Add(tube, box);
  box.Enlarge(rc);
  double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  const std::optional<TopoDS_Shape> extended =
      cut(BRepPrimAPI_MakeBox(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1)).Shape(), tube);
  if (!extended) {
    return std::nullopt;
  }
  TopoDS_Edge circle;
  const double tolerance = geometric_tolerance(rc);
  for (TopExp_Explorer ex(*extended, TopAbs_EDGE); ex.More() && circle.IsNull(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() == GeomAbs_Circle && curve.Circle().Location().Distance(centre) <= tolerance &&
        std::abs(curve.Circle().Radius() - rc) <= tolerance) {
      circle = edge;
    }
  }
  if (circle.IsNull()) {
    return std::nullopt;
  }
  BRepFilletAPI_MakeFillet rounding(*extended);
  rounding.Add(radius, circle);
  rounding.Build();
  if (!rounding.IsDone()) {
    return std::nullopt;
  }
  const std::optional<TopoDS_Shape> added = common(rounding.Shape(), tube);
  if (!added) {
    return std::nullopt;
  }
  return clipped(*added);
}

// The faces beyond the end cap: those sharing an edge with it, but the
// end cap and the floor (by index in `body`).
std::vector<int> faces_beyond(const Shape& body, const GrooveEnd& end) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  std::vector<int> beyond;
  for (TopExp_Explorer ex(body.face(end.cap), TopAbs_EDGE); ex.More(); ex.Next()) {
    if (!faces_of.Contains(ex.Current())) {
      continue;
    }
    for (const TopoDS_Shape& face : faces_of.FindFromKey(ex.Current())) {
      const int f = body.face_index(face);
      if (f >= 0 && f != end.cap && f != end.floor && std::find(beyond.begin(), beyond.end(), f) == beyond.end()) {
        beyond.push_back(f);
      }
    }
  }
  return beyond;
}

// Whether the face lies on the surface of `on` (its area's centre within
// `tolerance` of it).
bool lies_on(const TopoDS_Face& face, const TopoDS_Face& on, double tolerance) {
  GeomAPI_ProjectPointOnSurf projection(area_centre(face), BRep_Tool::Surface(on));
  return projection.NbPoints() > 0 && projection.LowerDistance() <= tolerance;
}

} // namespace

std::optional<GrooveEnd> groove_end(const Shape& body, int edge) {
  try {
    OCC_CATCH_SIGNALS
    if (edge < 0 || edge >= body.edge_count()) {
      return std::nullopt;
    }
    const TopoDS_Edge& shape = body.edge(edge);
    if (BRep_Tool::Degenerated(shape)) {
      return std::nullopt;
    }
    const BRepAdaptor_Curve curve(shape);
    if (curve.GetType() != GeomAbs_Circle) {
      return std::nullopt;
    }
    const gp_Circ circle = curve.Circle();
    const double rc = circle.Radius();
    const gp_Pnt centre = circle.Location();
    const double tolerance = geometric_tolerance(rc);
    const std::vector<int> faces = faces_at_edge(body, edge);
    if (faces.size() != 2) {
      return std::nullopt;
    }
    GrooveEnd end;
    end.circle = circle;
    for (const int f : faces) {
      const GeomAbs_SurfaceType type = BRepAdaptor_Surface(body.face(f), false).GetType();
      if (type == GeomAbs_Plane) {
        end.cap = f;
      } else if (type == GeomAbs_Cylinder || type == GeomAbs_Torus) {
        end.floor = f;
      }
    }
    if (end.cap < 0 || end.floor < 0) {
      return std::nullopt;
    }
    const TopoDS_Face& cap = body.face(end.cap);
    const TopoDS_Face& floor = body.face(end.floor);
    const gp_Pln plane = BRepAdaptor_Surface(cap, false).Plane();
    gp_Dir out = plane.Axis().Direction();
    if (cap.Orientation() == TopAbs_REVERSED) {
      out.Reverse();
    }
    if (!out.IsParallel(circle.Axis().Direction(), 1.0e-9)) {
      return std::nullopt;
    }
    end.into = out;
    const BRepAdaptor_Surface surface(floor, false);
    if (surface.GetType() == GeomAbs_Cylinder) {
      const gp_Cylinder cylinder = surface.Cylinder();
      if (!cylinder.Axis().Direction().IsParallel(out, 1.0e-9) ||
          gp_Lin(cylinder.Axis()).Distance(centre) > tolerance || std::abs(cylinder.Radius() - rc) > tolerance) {
        return std::nullopt;
      }
    } else {
      const gp_Torus torus = surface.Torus();
      const gp_Dir d = torus.Axis().Direction();
      const gp_Vec to_centre(torus.Location(), centre);
      if (!d.IsNormal(out, 1.0e-9) || plane.Distance(torus.Location()) > tolerance ||
          std::abs(to_centre.Dot(gp_Vec(d))) > tolerance ||
          std::abs(to_centre.Magnitude() - torus.MajorRadius()) > tolerance ||
          std::abs(torus.MinorRadius() - rc) > tolerance || torus.MajorRadius() <= rc + tolerance) {
        return std::nullopt;
      }
      end.bent = true;
      end.axis = torus.Axis();
      end.spine = torus.MajorRadius();
    }
    // A groove, not a rod's end: the floor's material outside the tube (its
    // outward normal at the edge towards the circle's centre) and the floor
    // on the end cap's outer side.
    const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    const std::optional<gp_Dir> normal = outward_normal(floor, middle);
    if (!normal || gp_Vec(*normal).Dot(gp_Vec(middle, centre)) <= 0.0) {
      return std::nullopt;
    }
    if (gp_Vec(centre, area_centre(floor)).Dot(gp_Vec(out)) <= tolerance) {
      return std::nullopt;
    }
    // An end cap across the tube: all of it inside the circle (a segment
    // of the disc, the faces beyond across it). Not a pocket's floor whose
    // concave corner is an arc of a cylinder (its outline going on
    // tangentially past the arc, the floor reaching far outside the
    // circle: the empty space there is not the tube's, and a ball wider
    // than the arc fits, mitcad#1368's pocket).
    for (TopExp_Explorer ex(cap, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge& bound = TopoDS::Edge(ex.Current());
      if (BRep_Tool::Degenerated(bound)) {
        continue;
      }
      const BRepAdaptor_Curve along(bound);
      const double first = along.FirstParameter();
      const double last = along.LastParameter();
      for (int i = 0; i <= 8; ++i) {
        if (along.Value(first + (last - first) * i / 8.0).Distance(centre) > rc + tolerance) {
          return std::nullopt;
        }
      }
    }
    // A shallow groove's end: the end cap cut across by planar faces beyond
    // it (not a whole disc: a blind hole's bottom stays the geometry
    // kernel's), the circle's centre outside the body there (on the outer
    // side of a face beyond's plane).
    const std::vector<int> beyond = faces_beyond(body, end);
    if (beyond.empty()) {
      return std::nullopt;
    }
    bool outside = false;
    for (const int f : beyond) {
      const TopoDS_Face& face = body.face(f);
      const BRepAdaptor_Surface across(face, false);
      if (across.GetType() != GeomAbs_Plane) {
        return std::nullopt;
      }
      const gp_Pln on = across.Plane();
      gp_Dir away = on.Axis().Direction();
      if (face.Orientation() == TopAbs_REVERSED) {
        away.Reverse();
      }
      outside = outside || gp_Vec(on.Location(), centre).Dot(gp_Vec(away)) > tolerance;
    }
    if (!outside) {
      return std::nullopt;
    }
    return end;
  } catch (const Standard_Failure&) {
    return std::nullopt;
  }
}

ShapePtr rounded_groove_end(const std::string& feature, const Shape& body, int edge, const GrooveEnd& end,
                            double radius) {
  try {
    OCC_CATCH_SIGNALS
    const double rc = end.circle.Radius();
    if (radius <= 0.0 || radius > rc + kGrooveRadiusTolerance || body.edge_name(edge).empty()) {
      return nullptr;
    }
    const double tolerance = geometric_tolerance(rc);
    const TopoDS_Face& cap = body.face(end.cap);
    // The faces beyond the end cap: planes, whose material sides cut what
    // the rounding adds.
    const std::vector<int> beyond = faces_beyond(body, end);
    if (beyond.empty()) {
      return nullptr;
    }
    std::vector<TopoDS_Shape> clip;
    std::vector<gp_Pln> planes;
    gp_Dir up = end.circle.XAxis().Direction();
    for (std::size_t i = 0; i < beyond.size(); ++i) {
      const TopoDS_Face& face = body.face(beyond[i]);
      const BRepAdaptor_Surface surface(face, false);
      if (surface.GetType() != GeomAbs_Plane) {
        return nullptr;
      }
      const gp_Pln plane = surface.Plane();
      gp_Dir out = plane.Axis().Direction();
      if (face.Orientation() == TopAbs_REVERSED) {
        out.Reverse();
      }
      planes.push_back(plane);
      const gp_Pnt inside = plane.Location().Translated(gp_Vec(out) * -1.0);
      clip.push_back(BRepPrimAPI_MakeHalfSpace(BRepBuilderAPI_MakeFace(plane).Face(), inside).Solid());
      // (The tube's seam away from the body: towards the first face
      // beyond's outer side, across the tube.)
      if (i == 0) {
        const gp_Vec across = gp_Vec(out) - gp_Vec(end.into) * gp_Vec(out).Dot(gp_Vec(end.into));
        if (across.Magnitude() > 1.0e-6) {
          up = gp_Dir(across);
        }
      }
    }
    throw_if_cancelled();
    const gp_Ax2 frame = tube_frame(end, up);
    const std::optional<TopoDS_Shape> added = added_by_rounding(end, frame, radius, clip);
    if (!added) {
      return nullptr;
    }
    const double volume = std::abs(volume_of(*added));
    const double before = std::abs(volume_of(body.occt()));
    if (volume <= 1.0e-9) {
      return nullptr;
    }
    // (It only fills the groove's end: none of it in the body's material.)
    const std::optional<TopoDS_Shape> overlap = common(*added, body.occt());
    if (!overlap || std::abs(volume_of(*overlap)) > 1.0e-6 * std::max(1.0, volume)) {
      return nullptr;
    }
    // Its faces named: on the end cap's plane or the floor's surface as
    // those (inside the result after the join), on a face beyond's plane
    // as that face (it grows out over them), else the rounding's.
    const std::string made = face_name(feature, "fillet", body.edge_name(edge));
    std::vector<Shape::NamedFace> named;
    for (TopExp_Explorer ex(*added, TopAbs_FACE); ex.More(); ex.Next()) {
      const TopoDS_Face& face = TopoDS::Face(ex.Current());
      NameList names{made};
      if (lies_on(face, cap, tolerance)) {
        names = body.face_names(end.cap);
      } else if (lies_on(face, body.face(end.floor), tolerance)) {
        names = body.face_names(end.floor);
      } else {
        for (std::size_t i = 0; i < beyond.size(); ++i) {
          if (BRepAdaptor_Surface(face, false).GetType() == GeomAbs_Plane &&
              planes[i].Distance(area_centre(face)) <= tolerance) {
            names = body.face_names(beyond[i]);
            break;
          }
        }
      }
      named.push_back({face, names});
    }
    const Shape tool(*added, named);
    throw_if_cancelled();
    const BooleanResult joined = boolean(BooleanOp::Join, {&body}, tool);
    if (joined.pieces.size() != 1) {
      return nullptr;
    }
    ShapePtr result = join_same_surfaces(joined.pieces.front().shape);
    if (!result || !one_closed_solid(result->occt())) {
      return nullptr;
    }
    const double after = std::abs(volume_of(result->occt()));
    if (std::abs(after - (before + volume)) > 1.0e-6 * std::max(1.0, before)) {
      return nullptr;
    }
    bool rounded = false;
    for (int f = 0; f < result->face_count() && !rounded; ++f) {
      const NameList& names = result->face_names(f);
      rounded = std::find(names.begin(), names.end(), made) != names.end();
    }
    return rounded ? result : nullptr;
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_Failure&) {
    return nullptr;
  } catch (const std::exception&) {
    return nullptr;
  }
}

std::string groove_too_narrow(const Shape& body, int edge, const GrooveEnd& end, double radius) {
  char numbers[160];
  std::snprintf(numbers, sizeof numbers, "radius %.6g mm, larger than the radius %.6g mm of", radius,
                end.circle.Radius());
  const std::string& name = body.edge_name(edge);
  const NameList& floor = body.face_names(end.floor);
  const std::string said = "the fillet is wider than the groove it ends: no ball of that radius fits in the groove";
  // (The names in the detail only.)
  return "fillet failed; " +
         placed_error(said,
                      "the fillet of edge " + name + " has " + numbers + " the groove's floor " +
                          (floor.empty() ? std::string("beside it") : "face " + floor.front()) +
                          ": no ball of that radius fits in the groove",
                      {edge_place(name)});
}

} // namespace mitcad::geometry::detail
