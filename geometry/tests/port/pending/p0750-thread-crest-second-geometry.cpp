// SPDX-License-Identifier: MIT
// Port patch 0750: both edges of a thread's crest narrower than the two
// roundings rounded, one blend between the two flanks cut by the faces at
// the thread's ends; a second geometry beside the case of
// geometry/tests/test_fillet_thread_crests.cpp (a crest 0.81 wide on a
// radius of 30.2, a pitch of 3, rounded with 0.8, ended by two planes).
//
// Here: a crest 0.5 wide on a radius of 8 (0.7 high, flanks 30 degrees off
// the plane square to the axis), a pitch of 1.5, rounded with 0.6 (the
// contacts on the crest lie 0.35 from each edge, the ball touching both
// flanks lies 0.17 under the crest). The thread is ended below by the
// plane z = 1.5, square to the axis, which meets the helix at 1.7 degrees,
// and above by a chamfer cone at 45 degrees (radius 7.1 at z = 4.5, 8.4 at
// z = 3.2) that cuts the whole ridge. The flanks are B-spline faces whose
// surfaces end where the faces do, as the bodies stored in files have them.
// Without the patch the walks past the spines' ends lose the ball (the
// guides and the flanks end about where their faces do) or roll it on the
// cone, and the two stripes' corners fail: the fillet fails. With it: one
// blend, the crest gone, valid, the material taken away that of the
// rounding's section swept between the planes where its centroid meets the
// end faces.
//
// The boundary: the same thread rounded with 0.25 (contacts 0.14 from each
// edge, 0.21 apart): two roundings, the crest stays.

#include <cmath>
#include <cstdio>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Lin2d.hxx>
#include <gp_Trsf.hxx>

#include "port_test.hpp"

namespace {

constexpr double kCrestRadius = 8.0; // the crest's radius
constexpr double kHeight      = 0.7; // the ridge's height over the root
constexpr double kRoot        = kCrestRadius - kHeight;
constexpr double kCrest       = 0.5; // the crest's width
constexpr double kPitch       = 1.5;
constexpr double kBottom      = 1.5; // the plane at the lower end
constexpr double kTop         = 4.5; // the top plane, inside the chamfer
constexpr double kConeTop     = 7.1; // the chamfer cone's radius at kTop (45 degrees)
constexpr double kConeHeight  = 1.3;

// Every B-spline face's surface cut down to the face (a hundredth beyond
// it), its edges' p-curves kept: a walk past such a face runs past its
// surface's bounds.
void trim_surfaces_to_faces(const TopoDS_Shape& body) {
  BRep_Builder builder;
  for (TopExp_Explorer fx(body, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face&                     face = TopoDS::Face(fx.Current());
    TopLoc_Location                        loc;
    const occ::handle<Geom_BSplineSurface> surface =
      occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(face, loc));
    if (surface.IsNull()) {
      continue;
    }
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0, su0 = 0, su1 = 0, sv0 = 0, sv1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    surface->Bounds(su0, su1, sv0, sv1);
    const double                     mu = 0.01 * (u1 - u0), mv = 0.01 * (v1 - v0);
    occ::handle<Geom_BSplineSurface> trimmed = occ::down_cast<Geom_BSplineSurface>(surface->Copy());
    trimmed->Segment(std::max(su0, u0 - mu), std::min(su1, u1 + mu), std::max(sv0, v0 - mv),
                     std::min(sv1, v1 + mv));
    std::vector<std::pair<TopoDS_Edge, std::pair<occ::handle<Geom2d_Curve>, occ::handle<Geom2d_Curve>>>> pcurves;
    for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge         edge = TopoDS::Edge(ex.Current());
      double                    f = 0, l = 0;
      occ::handle<Geom2d_Curve> c1 = BRep_Tool::CurveOnSurface(edge, face, f, l), c2;
      if (BRep_Tool::IsClosed(edge, face)) {
        c2 = BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Reversed()), face, f, l);
      }
      pcurves.push_back({edge, {c1, c2}});
    }
    for (const auto& [edge, c] : pcurves) {
      if (c.second.IsNull()) {
        builder.UpdateEdge(edge, c.first, trimmed, loc, BRep_Tool::Tolerance(edge));
      } else {
        builder.UpdateEdge(edge, c.first, c.second, trimmed, loc, BRep_Tool::Tolerance(edge));
      }
    }
    builder.UpdateFace(face, trimmed, loc, BRep_Tool::Tolerance(face));
  }
}

// The thread: a core of radius kRoot and a ridge swept along a helix of
// six turns (its crest's middle from z = -1.5 to 7.5), cut by the plane
// z = kBottom below and by the chamfer cone and the plane z = kTop above.
TopoDS_Shape thread_body() {
  const double               t30   = std::tan(test::kPi / 6.0);
  const double               foot  = 0.1; // the ridge's foot inside the core
  const double               half0 = 0.5 * kCrest + (kHeight + foot) * t30;
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(kRoot - foot, 0, -half0));
  profile.Add(gp_Pnt(kCrestRadius, 0, -0.5 * kCrest));
  profile.Add(gp_Pnt(kCrestRadius, 0, 0.5 * kCrest));
  profile.Add(gp_Pnt(kRoot - foot, 0, half0));
  profile.Close();
  const int                                  turns = 6;
  const occ::handle<Geom_CylindricalSurface> cylinder =
    new Geom_CylindricalSurface(gp_Ax3(gp::XOY()), kRoot - foot);
  const double                           length = turns * std::sqrt(4.0 * test::kPi * test::kPi + kPitch * kPitch);
  const occ::handle<Geom2d_TrimmedCurve> line   = new Geom2d_TrimmedCurve(
    new Geom2d_Line(gp_Lin2d(gp_Pnt2d(0, 0), gp_Dir2d(2.0 * test::kPi, kPitch))), 0, length);
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
  down.SetTranslation(gp_Vec(0, 0, -kPitch));
  const TopoDS_Shape ridge = BRepBuilderAPI_Transform(pipe.Shape(), down, true).Shape();
  const TopoDS_Shape core =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -2 * kPitch), gp_Dir(0, 0, 1)), kRoot, (turns + 4) * kPitch).Shape();
  TopoDS_Shape body = BRepAlgoAPI_Fuse(core, ridge).Shape();
  // Above: everything outside the cone from kTop - kConeHeight up, and
  // everything above kTop.
  const double       base = kTop - kConeHeight;
  const TopoDS_Shape cone =
    BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, base), gp_Dir(0, 0, 1)), kConeTop + kConeHeight, kConeTop, kConeHeight)
      .Shape();
  const TopoDS_Shape above = BRepAlgoAPI_Cut(
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, base), gp_Dir(0, 0, 1)), 20.0, 20.0).Shape(), cone).Shape();
  body = BRepAlgoAPI_Cut(body, above).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, kBottom - 20), 40, 40, 20).Shape()).Shape();
  ShapeUpgrade_UnifySameDomain unify(body, true, true, true);
  unify.Build();
  body = unify.Shape();
  trim_surfaces_to_faces(body);
  return body;
}

// The edges of the crest: on its radius all along, along the helix.
std::vector<TopoDS_Edge> crest_edges(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> crest;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    bool                    on  = true;
    double                  zlo = 1e100, zhi = -1e100;
    for (int k = 0; k <= 8 && on; ++k) {
      const gp_Pnt p =
        curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 8.0);
      on  = std::abs(std::hypot(p.X(), p.Y()) - kCrestRadius) < 1e-4;
      zlo = std::min(zlo, p.Z());
      zhi = std::max(zhi, p.Z());
    }
    if (on && zhi - zlo > 0.5 * kPitch) {
      crest.push_back(edge);
    }
  }
  return crest;
}

// Whether a face of the shape lies on the crest's cylinder.
bool has_crest_face(const TopoDS_Shape& shape) {
  for (TopExp_Explorer fx(shape, TopAbs_FACE); fx.More(); fx.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(fx.Current()));
    bool                      on = true;
    for (int i = 0; i <= 4 && on; ++i) {
      for (int j = 0; j <= 4 && on; ++j) {
        const double u =
          surface.FirstUParameter() + (surface.LastUParameter() - surface.FirstUParameter()) * i / 4.0;
        const double v =
          surface.FirstVParameter() + (surface.LastVParameter() - surface.FirstVParameter()) * j / 4.0;
        const gp_Pnt p = surface.Value(u, v);
        on             = std::abs(std::hypot(p.X(), p.Y()) - kCrestRadius) < 1e-3;
      }
    }
    if (on) {
      return true;
    }
  }
  return false;
}

// The section the rounding of radius r over the whole crest takes away
// (between the flanks' lines, the crest and the ball's arc), and the radius
// of its centroid.
struct Section {
  double area;
  double radius;
};

Section crest_section(double r) {
  const double alpha  = test::kPi / 6.0; // a flank's angle to the ridge's middle plane
  const double half   = 0.5 * kCrest;
  const double apex   = half / std::tan(alpha); // the flanks' meeting line above the crest
  const double t      = r / std::tan(alpha);
  const double centre = r / std::sin(alpha);
  const double theta  = test::kPi - 2.0 * alpha;
  const double kite   = r * t;
  const double sector = 0.5 * r * r * theta;
  const double off    = 2.0 * r * std::sin(0.5 * theta) / (1.5 * theta);
  const double a1     = kite - sector;
  const double c1     = (kite * (t * std::cos(alpha) + centre) / 3.0 - sector * (centre - off)) / a1;
  const double a2     = half * apex;
  const double c2     = 2.0 * apex / 3.0;
  const double area   = a1 - a2;
  const double from   = (a1 * c1 - a2 * c2) / area;
  return {area, kCrestRadius + apex - from};
}

// The crest's two edges rounded with r; a null shape when it fails.
TopoDS_Shape rounded_crest(const TopoDS_Shape& body, double r) {
  const std::vector<TopoDS_Edge> crest = crest_edges(body);
  CHECK(crest.size() == 2);
  if (crest.size() != 2) {
    return TopoDS_Shape();
  }
  return port::rounded(body, crest, r);
}

// The crest rounded with 0.6 on both edges: one blend, the crest gone. The
// section swept along the helix takes 2 pi rc A a turn (rc its centroid's
// radius); cut by the plane z = kBottom and the cone r + z = kConeTop + kTop,
// it is taken from where its centroid lies on the plane to where it lies
// on the cone.
void test_crest_of_a_small_thread_ended_by_a_plane_and_a_cone() {
  const TopoDS_Shape body = thread_body();
  CHECK(port::valid(body));
  if (!port::valid(body)) {
    return;
  }
  const TopoDS_Shape rounded = rounded_crest(body, 0.6);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE));
  CHECK(!has_crest_face(rounded));
  const Section s     = crest_section(0.6);
  const double  turns = (kConeTop + kTop - s.radius - kBottom) / kPitch;
  CHECK_NEAR_TOL(port::volume_of(body) - port::volume_of(rounded), 2.0 * test::kPi * s.radius * s.area * turns,
                 2e-2);
}

// The boundary: the crest rounded with 0.25, the two roundings' contacts on
// the crest 0.21 apart. The overlapping stripes' handling stays out: where
// the fillet is built, two blends with the crest between them.
void test_crest_wider_than_both_roundings_stays() {
  const TopoDS_Shape body = thread_body();
  CHECK(port::valid(body));
  if (!port::valid(body)) {
    return;
  }
  const TopoDS_Shape rounded = rounded_crest(body, 0.25);
  if (rounded.IsNull()) {
    std::printf("p0750 boundary: the rounding with 0.25 is not built\n");
    return;
  }
  CHECK(port::valid(rounded));
  CHECK(has_crest_face(rounded));
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 2);
}

} // namespace

MITCAD_PORT_TEST("0750", test_crest_of_a_small_thread_ended_by_a_plane_and_a_cone);
MITCAD_PORT_TEST("0750", test_crest_wider_than_both_roundings_stays);
