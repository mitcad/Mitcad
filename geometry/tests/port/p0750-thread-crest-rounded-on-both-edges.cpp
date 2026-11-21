// SPDX-License-Identifier: MIT
// Port patch 0750: both edges of a thread's crest narrower than the two
// roundings rounded, one blend between the two flanks (0013's overlapping
// stripes computed again over the crest) cut by the planes at the thread's
// ends, which meet the helix at about a degree. Without the patch the walk
// past the spine's end lost the ball (the guide's extension and the
// flanks' surfaces stop about where their faces do), the stripes' blends
// were kept twice and their corners failed: the fillet failed.
//
// The thread: a core of radius 28.5, a ridge 1.7 high swept along a helix
// of pitch 3, its flanks 30 degrees off the plane square to the axis, cut
// by the planes z = 3 and z = 6 or 9 (one or two turns); its B-spline
// faces' surfaces cut down to the faces, as stored bodies have them; its
// crest 0.81 wide. Both crest edges rounded with 0.8 (narrower than the
// two roundings): built, valid, the crest gone, one blend in its place (as
// many faces as before), one turn of it taking off its section swept round
// the axis. The boundary: rounded with 0.6, the two roundings apart on the
// crest: built, valid, the crest kept between two blends.

#include <cmath>
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

constexpr double kRoot   = 28.5; // the core's radius
constexpr double kHeight = 1.7;  // the ridge's height
constexpr double kPitch  = 3.0;  // the helix's pitch
constexpr double kRadius = 0.8;  // the roundings' radius
constexpr double kCrest  = 0.81; // the crest's width

// Every B-spline face's surface cut down to the face (a hundredth beyond
// it), its edges' p-curves kept, as the bodies stored in files have them:
// a walk past such a face runs past its surface's bounds.
void trim_surfaces_to_faces(const TopoDS_Shape& body) {
  BRep_Builder builder;
  for (TopExp_Explorer fx(body, TopAbs_FACE); fx.More(); fx.Next()) {
    const TopoDS_Face& face = TopoDS::Face(fx.Current());
    TopLoc_Location loc;
    const occ::handle<Geom_BSplineSurface> surface = occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(face, loc));
    if (surface.IsNull()) {
      continue;
    }
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0, su0 = 0, su1 = 0, sv0 = 0, sv1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    surface->Bounds(su0, su1, sv0, sv1);
    const double mu = 0.01 * (u1 - u0), mv = 0.01 * (v1 - v0);
    occ::handle<Geom_BSplineSurface> trimmed = occ::down_cast<Geom_BSplineSurface>(surface->Copy());
    trimmed->Segment(std::max(su0, u0 - mu), std::min(su1, u1 + mu), std::max(sv0, v0 - mv),
                     std::min(sv1, v1 + mv));
    std::vector<std::pair<TopoDS_Edge, std::pair<occ::handle<Geom2d_Curve>, occ::handle<Geom2d_Curve>>>> pcurves;
    for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
      const TopoDS_Edge edge = TopoDS::Edge(ex.Current());
      double f = 0, l = 0;
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

// The thread, its crest `crest` wide, between the planes z = 3 and z = top.
TopoDS_Shape thread_body(const double crest, const double top) {
  const double t30   = std::tan(test::kPi / 6.0);
  const double foot  = 0.3; // the ridge's foot inside the core
  const double half0 = 0.5 * crest + (kHeight + foot) * t30;
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(kRoot - foot, 0, -half0));
  profile.Add(gp_Pnt(kRoot + kHeight, 0, -0.5 * crest));
  profile.Add(gp_Pnt(kRoot + kHeight, 0, 0.5 * crest));
  profile.Add(gp_Pnt(kRoot - foot, 0, half0));
  profile.Close();
  const int    turns = 8;
  const occ::handle<Geom_CylindricalSurface> cylinder =
    new Geom_CylindricalSurface(gp_Ax3(gp::XOY()), kRoot - foot);
  const double length = turns * std::sqrt(4.0 * test::kPi * test::kPi + kPitch * kPitch);
  const occ::handle<Geom2d_TrimmedCurve> line =
    new Geom2d_TrimmedCurve(new Geom2d_Line(gp_Lin2d(gp_Pnt2d(0, 0), gp_Dir2d(2.0 * test::kPi, kPitch))), 0, length);
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
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, top), 100, 100, 100).Shape()).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-50, -50, 3.0 - 100), 100, 100, 100).Shape()).Shape();
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
    BRepAdaptor_Curve curve(edge);
    bool on = true;
    double zlo = 1e100, zhi = -1e100;
    for (int k = 0; k <= 8 && on; ++k) {
      const gp_Pnt p = curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 8.0);
      on = std::abs(std::hypot(p.X(), p.Y()) - (kRoot + kHeight)) < 1e-4;
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
    BRepAdaptor_Surface surface(TopoDS::Face(fx.Current()));
    bool on = true;
    for (int i = 0; i <= 4 && on; ++i) {
      for (int j = 0; j <= 4 && on; ++j) {
        const double u = surface.FirstUParameter() + (surface.LastUParameter() - surface.FirstUParameter()) * i / 4.0;
        const double v = surface.FirstVParameter() + (surface.LastVParameter() - surface.FirstVParameter()) * j / 4.0;
        const gp_Pnt p = surface.Value(u, v);
        on = std::abs(std::hypot(p.X(), p.Y()) - (kRoot + kHeight)) < 1e-3;
      }
    }
    if (on) {
      return true;
    }
  }
  return false;
}

// The volume one turn of the rounding takes away: the section between the
// flanks' lines, the crest and the ball's arc, swept round the axis at its
// centroid's radius (helical sweeps keep a meridian section's volume).
double removed_per_turn() {
  const double alpha  = test::kPi / 6.0; // a flank's angle to the plane of the ridge's middle
  const double half   = 0.5 * kCrest;
  const double apex   = half / std::tan(alpha); // the flanks' meeting line above the crest
  const double t      = kRadius / std::tan(alpha);
  const double centre = kRadius / std::sin(alpha);
  const double theta  = test::kPi - 2.0 * alpha;
  const double kite   = kRadius * t;
  const double sector = 0.5 * kRadius * kRadius * theta;
  const double off    = 2.0 * kRadius * std::sin(0.5 * theta) / (1.5 * theta);
  const double a1     = kite - sector;
  const double c1     = (kite * (t * std::cos(alpha) + centre) / 3.0 - sector * (centre - off)) / a1;
  const double a2     = half * apex;
  const double c2     = 2.0 * apex / 3.0;
  const double area   = a1 - a2;
  const double from   = (a1 * c1 - a2 * c2) / area;
  const double radius = kRoot + kHeight + apex - from;
  return 2.0 * test::kPi * radius * area;
}

// The crest edges of the thread rounded with r: the result, or a null
// shape where it fails or is invalid.
TopoDS_Shape rounded_thread(const TopoDS_Shape& body, double r) {
  const std::vector<TopoDS_Edge> crest = crest_edges(body);
  CHECK(crest.size() == 2);
  if (crest.size() != 2) {
    return TopoDS_Shape();
  }
  const TopoDS_Shape rounded = port::rounded(body, crest, r);
  CHECK(port::valid(rounded));
  return port::valid(rounded) ? rounded : TopoDS_Shape();
}

// The crest 0.81 wide rounded with 0.8 on both edges (each contact on the
// crest r tan(30 deg) = 0.46 from its edge, past the other's): one blend
// between the flanks, the crest gone, as many faces as before. One turn of
// it (the thread one turn longer, its ends alike) takes off the section
// between the flanks, the crest and the ball's arc swept round the axis
// (within 0.5 %: the walk's approximation).
void test_crest_narrower_than_both_roundings_is_consumed() {
  double removed[2] = {-1.0, -1.0};
  for (int i = 0; i < 2; ++i) {
    const TopoDS_Shape body = thread_body(kCrest, 6.0 + 3.0 * i);
    CHECK(port::valid(body));
    const TopoDS_Shape rounded = rounded_thread(body, kRadius);
    if (rounded.IsNull()) {
      return;
    }
    CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE));
    CHECK(!has_crest_face(rounded));
    removed[i] = port::volume_of(body) - port::volume_of(rounded);
  }
  CHECK_NEAR_TOL(removed[1] - removed[0], removed_per_turn(), 5e-3);
}

// The boundary: the same crest rounded with 0.6 (contacts 0.35 from each
// edge, apart): two blends, the crest kept between them.
void test_crest_wider_than_both_roundings_stays() {
  const TopoDS_Shape body    = thread_body(kCrest, 9.0);
  const TopoDS_Shape rounded = rounded_thread(body, 0.6);
  if (rounded.IsNull()) {
    return;
  }
  CHECK(port::count_of(rounded, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 2);
  CHECK(has_crest_face(rounded));
}

} // namespace

MITCAD_PORT_TEST("0750", test_crest_narrower_than_both_roundings_is_consumed);
MITCAD_PORT_TEST("0750", test_crest_wider_than_both_roundings_stays);
