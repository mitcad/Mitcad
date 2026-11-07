// SPDX-License-Identifier: MIT
// A face's outline rounded past a cylinder's rims rounded with less than
// the rounding's radius (mitcad#1426), with OCCT's fillet alone.
//
// A tube (bore 25, outside 30, 30 long) is cut by the plane y = -10; its
// rims are rounded with rk = 1 (tori). The outline of each cut face (a
// plane U) is then rounded with R = 1.5, one tangent chain: a line on the
// end plane, the rim torus, the line on the outside cylinder, the torus,
// the line on the other end plane, the bore's rim torus, the line on the
// bore, its torus. No ball of R rolls round a rim of rk: OCCT's walk failed
// (its approximation's weights). The rounding stops at each rim (the port's
// patch 0970, extended by 1590, third_party/vcpkg-ports):
//
// - at the bore's rims the bore's edge with U is acute: the ball touching
//   U, the end plane and the bore spans a vertex blend to the rim, and the
//   rim's torus turned about the bore's rounding until it is square to U
//   (a torus of major radius R - rk, minor rk) closes the corner, as on the
//   block of test_fillet_tight_corner, here with the bore a cylinder;
// - at the outside's rims the edge with U is obtuse: the ball touching U,
//   the end plane and the outside lies clear of the rim, and the rim's own
//   ball (rk) rolls on between the outside and the end plane's rounding (a
//   strip) and then between the two roundings (a triangle) down to where
//   their contacts on U meet, as patch 1380 builds a concave corner.
//
// Checks: valid; the four tori, their area rk phi ((R - rk) pi / 2 + rk);
// every point of the strips and triangles the rim's ball's (rk from a
// centre R - rk from the rounding along the end plane's axis and R - rk
// from the other rounding's axis or rk inside the outside); the corner
// faces' areas and the material taken are the stored body's of the design
// behind the patch (a tube of these sizes): before, the fillet failed. The
// same with planar walls: a block's bottom outline past vertical corners
// rounded with less than the radius, its front wall leaning away from the
// bottom (an obtuse edge), which patch 0970 left failing.
//
// With MITCAD_GEOMETRY_CASES set, the case `tube-rim-tight-corners`:
// `body.brep`, the reported design's stored body before the rounding, and
// `after.brep`, the stored body after it: the outlines of its two faces on
// y = -10 rounded with 1.5; about them the result lies on the stored body
// and the stored body on the result within 0.01 mm, and the material taken
// is the stored body's.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Torus.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kBore = 25.0;   // the tube's bore
constexpr double kOut = 30.0;    // its outside
constexpr double kLength = 30.0; // its length (z from 0)
constexpr double kCut = -10.0;   // the cut plane y = kCut
constexpr double kRk = 1.0;      // the rims' rounding
constexpr double kR = 1.5;       // the outline's rounding

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The tube cut by y = kCut (the part y > kCut kept), its four rims rounded
// with kRk.
TopoDS_Shape tube() {
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), kOut, kLength).Shape();
  const TopoDS_Shape inner =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), kBore, kLength + 2).Shape();
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(outer, inner).Shape();
  const TopoDS_Shape below = BRepPrimAPI_MakeBox(gp_Pnt(-40, -40, -5), gp_Pnt(40, kCut, kLength + 5)).Shape();
  const TopoDS_Shape cut = BRepAlgoAPI_Cut(ring, below).Shape();
  if (cut.IsNull()) {
    return cut;
  }
  BRepFilletAPI_MakeFillet rims(cut);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(cut, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
    if (e.GetType() == GeomAbs_Circle) {
      rims.Add(kRk, TopoDS::Edge(edges(i)));
    }
  }
  try {
    OCC_CATCH_SIGNALS
    rims.Build();
  } catch (const Standard_Failure&) {
    return TopoDS_Shape();
  }
  return rims.IsDone() ? rims.Shape() : TopoDS_Shape();
}

// Whether face F lies on the plane y = y0.
bool on_plane_y(const TopoDS_Face& f, double y0) {
  const BRepAdaptor_Surface s(f);
  if (s.GetType() != GeomAbs_Plane) {
    return false;
  }
  const gp_Pln pl = s.Plane();
  return std::abs(std::abs(pl.Axis().Direction().Y()) - 1) < 1e-9 && std::abs(pl.Distance(gp_Pnt(0, y0, 0))) < 1e-6;
}

// The outlines of the faces on y = y0 rounded with r: null where the
// fillet fails.
TopoDS_Shape round_outlines(const TopoDS_Shape& body, double y0, double r) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    if (!on_plane_y(TopoDS::Face(f.Current()), y0)) {
      continue;
    }
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      if (fillet.Contour(TopoDS::Edge(e.Current())) == 0) {
        fillet.Add(r, TopoDS::Edge(e.Current()));
      }
    }
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
    return TopoDS_Shape();
  }
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

// How far point c (a ball's centre) is from satisfying the obtuse corner's
// conditions: R - rk from the axis of the end plane's rounding (along x,
// at y = kCut + kR and kR inside the end plane) and either R - rk from the
// axis of the outside's rounding (along z) or rk inside the outside.
double off_rim_ball(const gp_Pnt& c) {
  const double zp = c.Z() < 0.5 * kLength ? kR : kLength - kR;
  const double yc = kCut + kR;
  const double xo = std::sqrt((kOut - kR) * (kOut - kR) - yc * yc) * (c.X() < 0 ? -1 : 1);
  const double dp = std::hypot(c.Y() - yc, c.Z() - zp) - (kR - kRk);
  const double dO = std::hypot(c.X() - xo, c.Y() - yc) - (kR - kRk);
  const double dK = std::hypot(c.X(), c.Y()) - (kOut - kRk);
  return std::abs(dp) + std::min(std::abs(dO), std::abs(dK));
}

struct Faces {
  int tori = 0;          // tori of major radius R - rk
  double torus_area = 0; // each one's
  int balls = 0;         // B-spline faces at the outside's rims
  double ball_area = 0;  // all of them
  double worst = 0;      // their points' largest distance off the rim's ball
};

Faces corner_faces(const TopoDS_Shape& shape) {
  Faces out;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& f = TopoDS::Face(ex.Current());
    const BRepAdaptor_Surface s(f);
    if (s.GetType() == GeomAbs_Torus && std::abs(s.Torus().MajorRadius() - (kR - kRk)) < 1e-9) {
      CHECK(std::abs(s.Torus().MinorRadius() - kRk) < 1e-9);
      ++out.tori;
      out.torus_area = area_of(f);
      continue;
    }
    if (s.GetType() != GeomAbs_BSplineSurface) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::Add(f, box);
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    box.Get(x0, y0, z0, x1, y1, z1);
    // (The outside's rims: beyond the outside's rounding's axis.)
    if (std::min(std::abs(x0), std::abs(x1)) < kOut - kR - 2.0) {
      continue;
    }
    ++out.balls;
    out.ball_area += area_of(f);
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(f, u0, u1, v0, v1);
    for (int iu = 0; iu <= 8; ++iu) {
      for (int iv = 0; iv <= 8; ++iv) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * iu / 8);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * iv / 8);
        if (BRepClass_FaceClassifier(f, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        gp_Pnt p;
        gp_Vec du, dv;
        s.D1(u, v, p, du, dv);
        gp_Vec n = du.Crossed(dv);
        if (n.Magnitude() < 1e-12) {
          continue;
        }
        n.Normalize();
        if (f.Orientation() == TopAbs_REVERSED) {
          n.Reverse();
        }
        // (The ball's centre: rk into the material.)
        out.worst = std::max(out.worst, off_rim_ball(p.Translated(-kRk * n)));
      }
    }
  }
  return out;
}

void test_tube_cut_face_rounded_past_its_rims() {
  const TopoDS_Shape body = tube();
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape result = round_outlines(body, kCut, kR);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    std::printf("the cut faces' outlines: not done\n");
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const Faces f = corner_faces(result);
  // The bore's rims: phi between the bore's normal at the ball's contact
  // (the ball's centre kR inside the cut plane and kR from the bore) and
  // the end plane's rounding's direction (along x).
  const double phi = std::asin(std::abs(kCut + kR) / (kBore + kR));
  const double want_torus = kRk * phi * ((kR - kRk) * kPi / 2 + kRk);
  const double taken = volume_of(body) - volume_of(result);
  std::printf("the cut faces' outlines: %d tori of %.6f mm2 (%.6f wanted), %d faces of the rims' balls "
              "of %.6f mm2 off by %.2g, the material taken %.6f mm3\n",
              f.tori, f.torus_area, want_torus, f.balls, f.ball_area, f.worst, taken);
  CHECK(f.tori == 4);
  CHECK(std::abs(f.torus_area - want_torus) < 1e-6);
  // A strip and a triangle at each of the outside's four rims; the stored
  // body's: 0.523751 and 1.619144 mm2.
  CHECK(f.balls == 8);
  CHECK(std::abs(f.ball_area - 4 * (0.523751 + 1.619144)) < 4 * 2e-4);
  // (The faces fitted through the ball's sections, their normals the
  // fit's.)
  CHECK(f.worst < 1e-3);
  CHECK(taken > 0.);
}

// A block x in [-7, 7], y in [0, 10], z in [0, 5] whose front wall through
// y = 0 on the bottom leans by `lean` degrees (towards -y as it rises where
// lean < 0: the bottom's front edge obtuse), its front vertical corners
// rounded with 0.3; its bottom's front, side and back edges rounded with
// 0.5.
void test_block_bottom_past_corners_with_an_obtuse_wall() {
  const double rk = 0.3, r = 0.5, half = 7.0, depth = 10.0, height = 5.0;
  for (const double lean : {-15.0, -30.0}) {
    const double t = std::tan(lean * kPi / 180.0);
    BRepBuilderAPI_MakePolygon poly;
    poly.Add(gp_Pnt(-half, 0, 0));
    poly.Add(gp_Pnt(-half, depth, 0));
    poly.Add(gp_Pnt(-half, depth, height));
    poly.Add(gp_Pnt(-half, height * t, height));
    poly.Close();
    const TopoDS_Face profile = BRepBuilderAPI_MakeFace(poly.Wire(), true).Face();
    const TopoDS_Shape prism = BRepPrimAPI_MakePrism(profile, gp_Vec(2 * half, 0, 0)).Shape();
    BRepFilletAPI_MakeFillet corners(prism);
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
    TopExp::MapShapes(prism, TopAbs_EDGE, edges);
    for (int i = 1; i <= edges.Extent(); ++i) {
      const BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
      const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
      const bool rising = std::abs(a.Z() - b.Z()) > 1.0 && std::abs(std::abs(a.X()) - half) < 1e-9 &&
                          std::abs(a.X() - b.X()) < 1e-9;
      const bool front = std::abs((a.Z() < b.Z() ? a : b).Y()) < 1e-9;
      if (rising && front) {
        corners.Add(rk, TopoDS::Edge(edges(i)));
      }
    }
    corners.Build();
    CHECK(corners.IsDone());
    if (!corners.IsDone()) {
      continue;
    }
    const TopoDS_Shape body = corners.Shape();
    BRepFilletAPI_MakeFillet fillet(body);
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> bodyEdges;
    TopExp::MapShapes(body, TopAbs_EDGE, bodyEdges);
    for (int i = 1; i <= bodyEdges.Extent(); ++i) {
      const BRepAdaptor_Curve e(TopoDS::Edge(bodyEdges(i)));
      const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
      if (std::abs(a.Z()) < 1e-9 && std::abs(b.Z()) < 1e-9 && fillet.Contour(TopoDS::Edge(bodyEdges(i))) == 0) {
        fillet.Add(r, TopoDS::Edge(bodyEdges(i)));
      }
    }
    try {
      OCC_CATCH_SIGNALS
      fillet.Build();
    } catch (const Standard_Failure& e) {
      std::printf("fillet threw: %s\n", e.what());
    }
    std::printf("the bottom's outline past corners at a wall leaning by %g degrees:", lean);
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      std::printf(" not done\n");
      continue;
    }
    const TopoDS_Shape result = fillet.Shape();
    CHECK(BRepCheck_Analyzer(result).IsValid());
    // The corners' faces: the vertical corners' ball (rk) rolled on, rk
    // from a centre R - rk from the side wall's rounding's axis (along y at
    // x = +-(half - r), z = r) and from the front's (along the front wall
    // at r from it and from the bottom) or rk inside the front wall.
    const double a = lean * kPi / 180.0;
    const gp_Dir no(0, -std::cos(a), std::sin(a));
    int balls = 0;
    double worst = 0;
    for (TopExp_Explorer ex(result, TopAbs_FACE); ex.More(); ex.Next()) {
      const TopoDS_Face& f = TopoDS::Face(ex.Current());
      const BRepAdaptor_Surface s(f);
      if (s.GetType() != GeomAbs_BSplineSurface) {
        continue;
      }
      ++balls;
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(f, u0, u1, v0, v1);
      for (int iu = 0; iu <= 6; ++iu) {
        for (int iv = 0; iv <= 6; ++iv) {
          const double u = u0 + (u1 - u0) * (0.02 + 0.96 * iu / 6);
          const double v = v0 + (v1 - v0) * (0.02 + 0.96 * iv / 6);
          if (BRepClass_FaceClassifier(f, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
            continue;
          }
          gp_Pnt p;
          gp_Vec du, dv;
          s.D1(u, v, p, du, dv);
          gp_Vec n = du.Crossed(dv);
          if (n.Magnitude() < 1e-12) {
            continue;
          }
          n.Normalize();
          if (f.Orientation() == TopAbs_REVERSED) {
            n.Reverse();
          }
          const gp_Pnt c = p.Translated(-rk * n);
          const double sx = c.X() < 0 ? -1 : 1;
          const double dp = std::hypot(c.X() - sx * (half - r), c.Z() - r) - (r - rk);
          // The front's rounding's axis (along x): r above the bottom and r
          // inside the front wall, at y = r (1 + sin a) / cos a.
          const gp_Vec w(gp_Pnt(c.X(), 0, 0), c);
          const double inFront = -w.Dot(gp_Vec(no));
          const double yo = r * (1 + std::sin(a)) / std::cos(a);
          const double dO = std::hypot(c.Y() - yo, c.Z() - r) - (r - rk);
          const double dK = inFront - rk;
          worst = std::max(worst, std::abs(dp) + std::min(std::abs(dO), std::abs(dK)));
        }
      }
    }
    std::printf(" %d faces of the corners' balls off by %.2g\n", balls, worst);
    CHECK(balls == 4);
    CHECK(worst < 1e-3);
  }
}

std::string geometry_cases() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, "MITCAD_GEOMETRY_CASES") == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv("MITCAD_GEOMETRY_CASES");
  return value != nullptr ? std::string(value) : std::string();
#endif
}

TopoDS_Shape read(const std::string& path) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  BRepTools::Read(shape, path.c_str(), builder);
  return shape;
}

// The faces of a shape whose box meets the region.
TopoDS_Compound faces_in(const TopoDS_Shape& shape, const Bnd_Box& region) {
  TopoDS_Compound out;
  BRep_Builder builder;
  builder.MakeCompound(out);
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    if (!box.IsOut(region)) {
      builder.Add(out, f.Current());
    }
  }
  return out;
}

// The farthest of points inside the faces of `from` within the region from
// the faces of `to` about it.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to, const Bnd_Box& region) {
  Bnd_Box wider = region;
  wider.Enlarge(1.);
  const TopoDS_Compound target = faces_in(to, wider);
  double worst = 0.;
  for (TopExp_Explorer f(faces_in(from, region), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 10; ++i) {
      for (int j = 0; j <= 10; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 10.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 10.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = s.Value(u, v);
        if (region.IsOut(p)) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), target);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

// The reported design's stored body: a tube of the sizes above cut by
// y = -10, its rims rounded with 1, and another tube across it far from the
// cut faces.
void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string dir = cases + "/tube-rim-tight-corners/";
  if (!std::ifstream(dir + "body.brep").good()) {
    std::printf("test_reported_body: skipped (no case tube-rim-tight-corners in MITCAD_GEOMETRY_CASES)\n");
    return;
  }
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  const TopoDS_Shape result = round_outlines(shape, kCut, kR);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  Bnd_Box region;
  region.Update(-kOut - 1., kCut - 0.5, -0.5, kOut + 1., kCut + 2.5, kLength + 0.5);
  const double out = farthest(result, after, region);
  const double back = farthest(after, result, region);
  std::printf("about the outlines: the result on the stored body within %.6f mm, the stored body on it within %.6f mm\n",
              out, back);
  CHECK(out <= 0.01);
  CHECK(back <= 0.01);
  const double taken = volume_of(shape) - volume_of(result);
  const double file = volume_of(shape) - volume_of(after);
  std::printf("the material taken %.6f mm3, the stored body's %.6f\n", taken, file);
  CHECK(std::abs(taken - file) <= 0.01);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_tube_cut_face_rounded_past_its_rims", test::test_tube_cut_face_rounded_past_its_rims);
  test::guarded("test_block_bottom_past_corners_with_an_obtuse_wall",
                test::test_block_bottom_past_corners_with_an_obtuse_wall);
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_tight_rim_corner: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
