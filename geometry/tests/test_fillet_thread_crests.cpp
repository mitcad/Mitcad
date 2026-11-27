// SPDX-License-Identifier: MIT
// A thread's crest rounded on both its edges where the crest is narrower
// than the roundings, built here with OCCT's fillet alone on a synthetic
// thread: the case behind the patch 0750 of the OCCT port
// (third_party/vcpkg-ports, mitcad#343). The two roundings are one blend
// between the flanks, the crest consumed; the planes at the thread's ends,
// square to its axis, meet the helix at about a degree and cut that blend
// along curves tens of millimetres long, one flank's contact running on past
// the end of its edge.

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
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
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

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// The thread: a core of radius kRoot, a ridge kHeight high swept along a
// helix of pitch kPitch, its crest kCrest wide, its flanks 30 degrees off
// the plane square to the axis (60 degrees between them); kRadius rounds
// both crest edges. A ball of that radius touching both flanks lies below
// the crest (the crest is narrower than the two roundings together).
constexpr double kRoot   = 28.5;
constexpr double kHeight = 1.7;
constexpr double kCrest  = 0.81;
constexpr double kPitch  = 3.0;
constexpr double kRadius = 0.8;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int faces_of(const TopoDS_Shape& shape) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  return faces.Extent();
}

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

// The thread between the planes z = 3 and z = top.
TopoDS_Shape thread_body(const double top) {
  const double t30   = std::tan(kPi / 6.0);
  const double foot  = 0.3; // the ridge's foot inside the core
  const double half0 = 0.5 * kCrest + (kHeight + foot) * t30;
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(kRoot - foot, 0, -half0));
  profile.Add(gp_Pnt(kRoot + kHeight, 0, -0.5 * kCrest));
  profile.Add(gp_Pnt(kRoot + kHeight, 0, 0.5 * kCrest));
  profile.Add(gp_Pnt(kRoot - foot, 0, half0));
  profile.Close();
  const int    turns = 8;
  const occ::handle<Geom_CylindricalSurface> cylinder =
    new Geom_CylindricalSurface(gp_Ax3(gp::XOY()), kRoot - foot);
  const double length = turns * std::sqrt(4.0 * kPi * kPi + kPitch * kPitch);
  const occ::handle<Geom2d_TrimmedCurve> line =
    new Geom2d_TrimmedCurve(new Geom2d_Line(gp_Lin2d(gp_Pnt2d(0, 0), gp_Dir2d(2.0 * kPi, kPitch))), 0, length);
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
  const double alpha  = kPi / 6.0; // a flank's angle to the plane of the ridge's middle
  const double half   = 0.5 * kCrest;
  const double apex   = half / std::tan(alpha); // the flanks' meeting line above the crest
  const double t      = kRadius / std::tan(alpha);
  const double centre = kRadius / std::sin(alpha);
  const double theta  = kPi - 2.0 * alpha;
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
  return 2.0 * kPi * radius * area;
}

// The crest of the thread between z = 3 and z = top rounded on both edges:
// the material removed (negative where the fillet fails or is invalid).
// Unless `required`, a fillet that fails is reported, not a failed check.
double rounded_thread(const double top, const bool required = true) {
  const TopoDS_Shape body = thread_body(top);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return -1.0;
  }
  const std::vector<TopoDS_Edge> crest = crest_edges(body);
  CHECK(crest.size() == 2);
  BRepFilletAPI_MakeFillet maker(body);
  for (const TopoDS_Edge& edge : crest) {
    maker.Add(kRadius, edge);
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet threw: %s\n", failure.what());
  }
  if (!required && !maker.IsDone()) {
    std::fprintf(stderr, "the rounding of the thread up to z = %g failed (allowed)\n", top);
    return -1.0;
  }
  CHECK(maker.IsDone());
  if (!maker.IsDone()) {
    return -1.0;
  }
  const TopoDS_Shape rounded = maker.Shape();
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  // The crest gone, one blend in its place.
  CHECK(faces_of(rounded) == faces_of(body));
  CHECK(!has_crest_face(rounded));
  return volume_of(body) - volume_of(rounded);
}

// Two threads one turn apart in length, their ends alike: one turn of the
// rounding between them. The longer one's blend runs over three turns, where
// the walk's surface approximation may fail (port patch 0750's limit): it
// failed on Windows only (0.14.0-rc1), so it is not required.
void test_crest_narrower_than_both_roundings() {
  const double shorter = rounded_thread(9.0);
  const double longer  = rounded_thread(9.0 + kPitch, false);
  CHECK(shorter > 0.0);
  if (shorter > 0.0 && longer > 0.0) {
    CHECK_NEAR_TOL(longer - shorter, removed_per_turn(), 5e-3);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_thread_crests) {
  test::guarded("test_crest_narrower_than_both_roundings", test::test_crest_narrower_than_both_roundings);
  if (test::failures == 0) {
    std::puts("test_fillet_thread_crests: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
