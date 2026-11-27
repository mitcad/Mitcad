// SPDX-License-Identifier: MIT
// Roundings on synthetic shapes with the traits of the bodies stored in
// files that kept OCCT's fillet from the roundings round a thread's chamfer
// and up to a groove's floor (a cylinder trimmed turns below its basis's
// parameters, edges whose curves lie apart by more than their tolerance, a
// rim approximated inside its circle), built here with OCCT's fillet alone:
// the cases behind the patches 0800, 0801 and 0803 of the OCCT port
// (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>
#include <functional>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomProjLib.hxx>
#include <Geom_Circle.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Lin2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec2d.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The edges of a shape whose middle point passes the test.
std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<TopoDS_Edge>                                      edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge&            edge  = TopoDS::Edge(all(i));
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (!curve.IsNull() && keep(curve->Value(0.5 * (first + last)))) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// The edges rounded with radius r by OCCT's fillet alone; a null shape when
// it fails.
TopoDS_Shape rounded(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : edges) {
    fillet.Add(r, edge);
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet crest corner test: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

// Every cylindrical face's surface replaced by its cylinder trimmed to one
// period from the face's start `periods` turns lower, the face's curves on
// it moved down as far: so the bodies stored in files hold their cylinders,
// whose parameters start turns below those of the basis surface.
int cylinders_turns_down(const TopoDS_Shape& body, int periods) {
  const double shift = -2.0 * kPi * periods;
  BRep_Builder builder;
  int          changed = 0;
  for (TopExp_Explorer fx(body, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face&        face = TopoDS::Face(fx.Current());
    TopLoc_Location           loc;
    occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, loc);
    if (const occ::handle<Geom_RectangularTrimmedSurface> trimmed =
          occ::down_cast<Geom_RectangularTrimmedSurface>(surface)) {
      surface = trimmed->BasisSurface();
    }
    const occ::handle<Geom_CylindricalSurface> cylinder = occ::down_cast<Geom_CylindricalSurface>(surface);
    if (cylinder.IsNull()) {
      continue;
    }
    ++changed;
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const occ::handle<Geom_RectangularTrimmedSurface> stored =
      new Geom_RectangularTrimmedSurface(cylinder, u0 + shift, u0 + shift + 2.0 * kPi, v0 - 10.0, v1 + 10.0);
    std::vector<std::pair<TopoDS_Edge, std::pair<occ::handle<Geom2d_Curve>, occ::handle<Geom2d_Curve>>>> pcurves;
    for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge         edge = TopoDS::Edge(ex.Current());
      double                    f = 0, l = 0;
      occ::handle<Geom2d_Curve> c1 = occ::down_cast<Geom2d_Curve>(BRep_Tool::CurveOnSurface(edge, face, f, l)->Copy());
      occ::handle<Geom2d_Curve> c2;
      c1->Translate(gp_Vec2d(shift, 0.0));
      if (BRep_Tool::IsClosed(edge, face)) {
        c2 = occ::down_cast<Geom2d_Curve>(
          BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Reversed()), face, f, l)->Copy());
        c2->Translate(gp_Vec2d(shift, 0.0));
      }
      pcurves.push_back({edge, {c1, c2}});
    }
    for (const auto& [edge, c] : pcurves) {
      if (c.second.IsNull()) {
        builder.UpdateEdge(edge, c.first, stored, loc, BRep_Tool::Tolerance(edge));
      } else {
        builder.UpdateEdge(edge, c.first, c.second, stored, loc, BRep_Tool::Tolerance(edge));
      }
    }
    builder.UpdateFace(face, stored, loc, BRep_Tool::Tolerance(face));
  }
  return changed;
}

// A post of radius 20 (z 0 to 15) with a rib standing out of its side (x
// 19 to 22, y -0.2 to 2.8, turned by 1 radian about the post's axis), the
// post's top edge and the rib's top cut by a cone at 45 degrees: the rib's
// side y = -0.2 meets the cone along an edge from the post's side outward.
// (As in test_fillet_corner_cases.)
TopoDS_Shape post_with_rib() {
  const double turn = 1.0;
  const TopoDS_Shape post =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(-1, 0, 0)), 20.0, 15.0).Shape();
  gp_Trsf rotation;
  rotation.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), turn);
  const TopoDS_Shape rib =
    BRepBuilderAPI_Transform(BRepPrimAPI_MakeBox(gp_Pnt(19, -0.2, 0), gp_Pnt(22, 2.8, 15)).Shape(), rotation, true)
      .Shape();
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(19.5, 0, 15));
  profile.Add(gp_Pnt(30, 0, 4.5));
  profile.Add(gp_Pnt(30, 0, 20));
  profile.Add(gp_Pnt(19.5, 0, 20));
  profile.Close();
  const TopoDS_Shape above = BRepPrimAPI_MakeRevol(BRepBuilderAPI_MakeFace(profile.Wire(), true).Face(),
                                                   gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)))
                               .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(BRepAlgoAPI_Fuse(post, rib).Shape(), above).Shape(), true,
                                     true, true);
  unify.Build();
  return unify.Shape();
}

// The rib's side rounded with 0.5 along its edge with the cone: the
// material removed and the cone's area after it; negative where OCCT fails
// or the result is invalid.
struct RibRounding {
  double removed   = -1.0;
  double cone_area = -1.0;
};

RibRounding rib_rounding(const TopoDS_Shape& body) {
  const double                   turn = 1.0;
  const std::vector<TopoDS_Edge> edge = edges_where(body, [turn](const gp_Pnt& m) {
    const gp_Pnt p = m.Rotated(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), -turn);
    const double r = std::hypot(p.X(), p.Y());
    return std::abs(p.Y() + 0.2) < 1e-7 && p.X() > 19.9 && std::abs(p.Z() - (34.5 - r)) < 1e-6;
  });
  CHECK(edge.size() == 1);
  RibRounding out;
  if (edge.size() != 1) {
    return out;
  }
  const TopoDS_Shape result = rounded(body, edge, 0.5);
  if (result.IsNull() || !BRepCheck_Analyzer(result).IsValid()) {
    return out;
  }
  out.removed = volume_of(body) - volume_of(result);
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cone) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(f.Current(), props);
      out.cone_area = props.Mass();
    }
  }
  return out;
}

// The rounding of the rib starts at the post's side, an end where the
// cone's edge with the post is prolonged past the vertex up to the
// rounding's contact; the post's side is the face at that end, which cuts
// the rounding. Its cylinder's parameters starting two turns below the
// basis's (port patch 0800), the arcs' points on it lay in its own range
// and the opposite contact's point, projected on the surface, in the
// basis's: the cut was sought between points two periods apart, brought
// one period together only, and was not found. The cone's face is the one
// on the plain post, and the material removed is the same (within what
// the volumes integrated over the post's trimmed cylinder give: the bodies
// alone differ by 2.6e-7 of their volume).
void test_end_on_a_cylinder_stored_turns_down() {
  const TopoDS_Shape plain  = post_with_rib();
  const TopoDS_Shape stored = post_with_rib();
  CHECK(cylinders_turns_down(stored, 2) == 1);
  CHECK(BRepCheck_Analyzer(stored).IsValid());
  CHECK_NEAR_TOL(volume_of(stored), volume_of(plain), 1e-6);
  const RibRounding on_plain  = rib_rounding(plain);
  const RibRounding on_stored = rib_rounding(stored);
  CHECK(on_plain.removed > 0.0);
  CHECK(on_stored.removed > 0.0);
  CHECK(on_plain.cone_area > 0.0);
  CHECK_NEAR_TOL(on_stored.cone_area, on_plain.cone_area, 1e-6);
  CHECK_NEAR_TOL(on_stored.removed, on_plain.removed, 2e-3);
}

// A block 40 x 20 x 10 made of two halves fused (x 0 to 20, 20 to 40), its
// faces B-spline surfaces: its top, front and bottom are each two faces of
// one plane, split at x = 20. The top's split edge (along y) has a
// tolerance of 1e-3 and its curves on the two halves of the top lie 0.9e-3
// to either side of its own curve (so the bodies stored in files have
// their approximated edges: each curve within the tolerance, the two
// nearly twice as far apart).
TopoDS_Shape block_with_split_top() {
  const TopoDS_Shape left  = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(20, 20, 10)).Shape();
  const TopoDS_Shape right = BRepPrimAPI_MakeBox(gp_Pnt(20, 0, 0), gp_Pnt(40, 20, 10)).Shape();
  const TopoDS_Shape body  = BRepBuilderAPI_NurbsConvert(BRepAlgoAPI_Fuse(left, right).Shape(), true).Shape();
  const std::vector<TopoDS_Edge> split = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.X() - 20.0) < 1e-7 && std::abs(m.Z() - 10.0) < 1e-7 && m.Y() > 1.0;
  });
  CHECK(split.size() == 1);
  if (split.size() != 1) {
    return body;
  }
  const double tol = 1e-3;
  BRep_Builder builder;
  const TopoDS_Edge& edge = split.front();
  std::vector<std::pair<TopoDS_Face, occ::handle<Geom2d_Curve>>> moved;
  for (TopExp_Explorer fx(body, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face& face = TopoDS::Face(fx.Current());
    for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
      if (!ex.Current().IsSame(edge)) {
        continue;
      }
      double                    f = 0, l = 0;
      occ::handle<Geom2d_Curve> c = occ::down_cast<Geom2d_Curve>(BRep_Tool::CurveOnSurface(edge, face, f, l)->Copy());
      // 0.9e-3 into the face (its surface's parameters are proportional to
      // the coordinates).
      GProp_GProps props;
      BRepGProp::SurfaceProperties(face, props);
      const double                    side    = props.CentreOfMass().X() < 20.0 ? -1.0 : 1.0;
      const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
      const gp_Pnt                    p(20.0, 10.0, 10.0);
      GeomAPI_ProjectPointOnSurf      at(p, surface);
      GeomAPI_ProjectPointOnSurf      off(p.Translated(gp_Vec(side * 0.9 * tol, 0, 0)), surface);
      double                          u0 = 0, v0 = 0, u1 = 0, v1 = 0;
      at.LowerDistanceParameters(u0, v0);
      off.LowerDistanceParameters(u1, v1);
      c->Translate(gp_Vec2d(u1 - u0, v1 - v0));
      moved.push_back({face, c});
    }
  }
  CHECK(moved.size() == 2);
  for (const auto& [face, c] : moved) {
    TopLoc_Location loc;
    builder.UpdateEdge(edge, c, BRep_Tool::Surface(face, loc), loc, tol);
  }
  for (TopExp_Explorer vx(edge, TopAbs_VERTEX); vx.More(); vx.Next()) {
    builder.UpdateVertex(TopoDS::Vertex(vx.Current()), tol);
  }
  return body;
}

// The block's top front edge, two edges meeting at the split, rounded with
// 3 (port patch 0801): at their common vertex the faces of the third edge,
// the top's split, are one plane, so the two edges are one contour. Its
// curves on the faces lying 1.8e-3 apart, beyond one and a half times its
// tolerance, the faces were taken for faces meeting at an angle and the
// contour stopped there: two roundings meeting end to end, and the result
// was invalid. Now one, the material removed that of the block's edge
// (within the walked blend's approximation on the B-spline faces).
void test_contour_across_a_stored_smooth_edge() {
  const TopoDS_Shape body = block_with_split_top();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> front = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(m.Y()) < 1e-7 && std::abs(m.Z() - 10.0) < 1e-7;
  });
  CHECK(front.size() == 2);
  if (front.size() != 2) {
    return;
  }
  const double             r = 3.0;
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : front) {
    if (fillet.Contour(edge) == 0) {
      fillet.Add(r, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet crest corner test: %s\n", failure.what());
  }
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  CHECK_NEAR_TOL(volume_of(body) - volume_of(fillet.Shape()), (1.0 - kPi / 4.0) * r * r * 40.0, 1e-3);
}

// A shaft of radius 30.2 (z 0 to 12) with a thread cut into it as a groove
// between z = 3 and 9, down to a core of radius 28.5: the thread's ridge
// (as in test_fillet_thread_crests, swept along a helix of pitch 4, its
// flanks 30 degrees off the plane square to the axis) is 1.16 wide on the
// shaft's cylinder, its crest. The groove ends in a floor at z = 3 and a
// ceiling at z = 9, planes square to the axis that meet the helix at about
// a degree; below and above them the shaft goes on.
TopoDS_Shape grooved_shaft() {
  const double root = 28.5, height = 2.0, crest = 0.81, pitch = 4.0, shaft = 30.2;
  const double t30   = std::tan(kPi / 6.0);
  const double foot  = 0.3; // the ridge's foot inside the core
  const double half0 = 0.5 * crest + (height + foot) * t30;
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(root - foot, 0, -half0));
  profile.Add(gp_Pnt(root + height, 0, -0.5 * crest));
  profile.Add(gp_Pnt(root + height, 0, 0.5 * crest));
  profile.Add(gp_Pnt(root - foot, 0, half0));
  profile.Close();
  const int                                  turns = 5;
  const occ::handle<Geom_CylindricalSurface> cylinder =
    new Geom_CylindricalSurface(gp_Ax3(gp::XOY()), root - foot);
  const double length = turns * std::sqrt(4.0 * kPi * kPi + pitch * pitch);
  const occ::handle<Geom2d_TrimmedCurve> line =
    new Geom2d_TrimmedCurve(new Geom2d_Line(gp_Lin2d(gp_Pnt2d(0, 0), gp_Dir2d(2.0 * kPi, pitch))), 0, length);
  TopoDS_Edge helix = BRepBuilderAPI_MakeEdge(line, cylinder).Edge();
  BRepLib::BuildCurves3d(helix);
  BRepOffsetAPI_MakePipeShell pipe(BRepBuilderAPI_MakeWire(helix).Wire());
  pipe.SetMode(gp_Dir(0, 0, 1));
  pipe.Add(profile.Wire());
  pipe.Build();
  if (!pipe.IsDone()) {
    return TopoDS_Shape();
  }
  pipe.MakeSolid();
  gp_Trsf down;
  down.SetTranslation(gp_Vec(0, 0, -pitch));
  const TopoDS_Shape ridge = BRepBuilderAPI_Transform(pipe.Shape(), down, true).Shape();
  // The thread between z = 3 and 9 (its crest standing out of the shaft),
  // the shaft below and above it, all within the shaft's cylinder: that
  // cylinder is the thread's crest.
  const TopoDS_Shape core =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -2 * pitch), gp_Dir(0, 0, 1)), root, (turns + 4) * pitch).Shape();
  TopoDS_Shape body = BRepAlgoAPI_Fuse(core, ridge).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, 9.0), 100, 100, 100).Shape()).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, 3.0 - 100), 100, 100, 100).Shape()).Shape();
  const TopoDS_Shape below =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), shaft, 3.0).Shape();
  const TopoDS_Shape above =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 9.0), gp_Dir(0, 0, 1)), shaft, 3.0).Shape();
  body = BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(body, below).Shape(), above).Shape();
  body = BRepAlgoAPI_Common(
           body, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), shaft, 12.0).Shape())
           .Shape();
  ShapeUpgrade_UnifySameDomain unify(body, true, true, true);
  unify.Build();
  const TopoDS_Shape result = unify.Shape();
  // The rims of the floor and the ceiling (their edges with the shaft's
  // cylinder) as the bodies stored in files have them: their curves (in
  // space and on the plane) a circle 2 micrometres inside the shaft's, the
  // edges' tolerances 5 micrometres.
  const double depth = 2e-6, tol = 5e-6;
  BRep_Builder builder;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(result, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    BRepAdaptor_Curve  curve(edge);
    if (curve.GetType() != GeomAbs_Circle || std::abs(curve.Circle().Radius() - shaft) > 1e-7) {
      continue;
    }
    const double z = curve.Circle().Location().Z();
    if (std::abs(z - 3.0) > 1e-7 && std::abs(z - 9.0) > 1e-7) {
      continue;
    }
    double f = 0, l = 0;
    BRep_Tool::Range(edge, f, l);
    gp_Circ inner = curve.Circle();
    inner.SetRadius(shaft - depth);
    builder.UpdateEdge(edge, new Geom_Circle(inner), TopLoc_Location(), tol);
    builder.Range(edge, f, l);
    for (TopExp_Explorer fx(result, TopAbs_FACE); fx.More(); fx.Next()) {
      const TopoDS_Face& face = TopoDS::Face(fx.Current());
      if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane) {
        continue;
      }
      bool has = false;
      for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More() && !has; ex.Next()) {
        has = ex.Current().IsSame(edge);
      }
      if (!has) {
        continue;
      }
      // On the plane: the edge's curve projected, the same parameters.
      TopLoc_Location                 floc;
      const occ::handle<Geom_Surface> plane = BRep_Tool::Surface(face, floc);
      builder.UpdateEdge(edge, GeomProjLib::Curve2d(new Geom_Circle(inner), f, l, plane), plane, floc, tol);
    }
    for (TopExp_Explorer vx(edge, TopAbs_VERTEX); vx.More(); vx.Next()) {
      builder.UpdateVertex(TopoDS::Vertex(vx.Current()), tol);
    }
  }
  return result;
}

// One edge of the thread's crest rounded with 0.3 (port patch 0803): at
// the groove's floor and ceiling the rounding ends where its contact on the
// crest meets the plane, the plane's rim prolonged up to there, and the
// plane cuts the rounding along a cap 12 long that leaves the rim
// tangentially and runs on inside it, a few micrometres from it for a
// while. With the rim's curve 2 micrometres inside, the two crossed 0.052
// from the point they start from, beyond its tolerance, and the ceiling's
// wire was invalid. The planes' faces are valid now, and the rounding takes
// away the volume of its section swept along the edge (the section's area
// times the edge's length, within what its ends leave).
void test_groove_crest_rounded_up_to_its_floors() {
  const TopoDS_Shape body = grooved_shaft();
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // An edge of the crest (on the shaft's radius, neither a circle nor a
  // line): its tangent chain is one of the crest's two edges.
  std::vector<TopoDS_Edge>                                      crest;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(body, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(all(i));
    BRepAdaptor_Curve  curve(edge);
    bool               on = curve.GetType() != GeomAbs_Line && curve.GetType() != GeomAbs_Circle;
    for (int k = 0; k <= 8 && on; ++k) {
      const gp_Pnt p = curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 8.0);
      on             = std::abs(std::hypot(p.X(), p.Y()) - 30.2) < 1e-3;
    }
    if (on) {
      crest.push_back(edge);
    }
  }
  CHECK(!crest.empty());
  if (crest.empty()) {
    return;
  }
  const double             r = 0.3;
  BRepFilletAPI_MakeFillet fillet(body);
  fillet.Add(r, crest.front());
  double length = 0.0;
  for (int i = 1; i <= fillet.NbEdges(1); ++i) {
    GProp_GProps edge_props;
    BRepGProp::LinearProperties(fillet.Edge(1, i), edge_props);
    length += edge_props.Mass();
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet crest corner test: %s\n", failure.what());
  }
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  // The section: the crest and the flank meet at 120 degrees inside.
  const double area = r * r * (std::tan(kPi / 6.0) - kPi / 6.0);
  CHECK_NEAR_TOL(volume_of(body) - volume_of(fillet.Shape()), area * length, 2e-2);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_crest_corners) {
  test::guarded("test_end_on_a_cylinder_stored_turns_down", test::test_end_on_a_cylinder_stored_turns_down);
  test::guarded("test_contour_across_a_stored_smooth_edge", test::test_contour_across_a_stored_smooth_edge);
  test::guarded("test_groove_crest_rounded_up_to_its_floors",
                test::test_groove_crest_rounded_up_to_its_floors);
  if (test::failures == 0) {
    std::puts("test_fillet_crest_corners: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
