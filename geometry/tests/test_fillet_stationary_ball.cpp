// SPDX-License-Identifier: MIT
// A lid's rims rounded with r through the mitres of two earlier roundings of
// r (mitcad#1412), with OCCT's fillet alone.
//
// A lid (a cylinder of 23 up to its top at z = 30, under it a cone of 45
// degrees down to a post of 20) is cut by the plane y = 0. The edges of its
// flat side (the vertical ones, the one along the top and the ones with the
// cone and the post) are rounded with r = 1: at each corner of the top the
// vertical rounding and the one along the top meet in a mitre, at each
// corner of the cone rim the vertical rounding and the walked one between
// the flat side and the cone. A rim's tangent chain goes on past a short
// edge (between the top and the vertical rounding, 0.045 mm; between the
// cone and it, 0.014 mm) through the mitre. Rounding a rim with r, the ball
// reaches the vertical rounding's axis where the rim ends and stays put
// there over the short edge and the mitre (its centre on both roundings'
// axes): the corner is a piece of its sphere, bounded by the rim's torus
// end, the vertical rounding's contact circle and the other rounding's.
// Over the mitre the rounding's conditions leave its section a freedom
// (port patch 1520 walks it in the guide's normal plane); the edge between
// the top and the vertical rounding was a torus of no tube circle, the ball
// touching the top at one point (port patch 1521 walks it with the mitre).
// Without the patches each rim's rounding failed. Each corner is one face
// (port patch 1522, mitcad#1420): the walk over the short arc and the
// mitre, one piece, was split where its contacts' distance was least, the
// ball's width apart.
//
// The lid with its top only, the top rim rounded, and the lid's half with
// its cone and post, both rims rounded: valid, and at each corner the new
// faces lie on the ball's sphere and cover its piece there (the top's:
// pi / 2 + asin(1 / 22) r^2).
//
// With MITCAD_GEOMETRY_CASES set, the case `lid-rim-stationary-ball`:
// `body.brep`, the reported design's body before the rounding as the replay
// makes it, and, where the case has it, `stored-before.brep`, the stored
// body before it, their rims rounded with 1, against `after.brep`, the
// stored body after it: the result has the stored body's faces, lies on it
// and it on the result within 0.01 mm, and the volumes agree. On the stored
// body the earlier rounding of the cone rim's corner is a B-spline that is
// a tube of the ball's radius within 1.6e-3 mm, its sections' curvature
// rippling by up to 2.5 % (port patch 1523 takes such a ball as staying
// put, mitcad#1420).

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
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kRadius = 1.0;
constexpr double kRim    = 23.0;
constexpr double kTop    = 30.0;
constexpr double kRimLow = 28.0;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

// The lid: a cylinder of kRim from kRimLow up to kTop; with `cone`, under it
// a cone down to a post of 20 standing on z = 0. Cut by y = 0 (y >= 0 kept),
// the edges of its flat side but the lowest one rounded with kRadius.
TopoDS_Shape lid(bool cone) {
  TopoDS_Shape body =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kRimLow), gp_Dir(0, 0, 1)), kRim, kTop - kRimLow).Shape();
  double bottom = kRimLow;
  if (cone) {
    const TopoDS_Shape flange =
        BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 25), gp_Dir(0, 0, 1)), 20., kRim, kRimLow - 25.).Shape();
    const TopoDS_Shape post = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 20., 25.).Shape();
    body = BRepAlgoAPI_Fuse(BRepAlgoAPI_Fuse(body, flange).Shape(), post).Shape();
    ShapeUpgrade_UnifySameDomain unify(body, true, true, true);
    unify.Build();
    body   = unify.Shape();
    bottom = 0.;
  }
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, -10), gp_Pnt(30, 0, 40)).Shape()).Shape();
  BRepFilletAPI_MakeFillet flat(body);
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(ex.Current()));
    const gp_Pnt            a = c.Value(c.FirstParameter());
    const gp_Pnt            b = c.Value(c.LastParameter());
    const gp_Pnt            m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(a.Y()) < 1e-7 && std::abs(b.Y()) < 1e-7 && std::abs(m.Y()) < 1e-7
        && !(std::abs(a.Z() - bottom) < 1e-7 && std::abs(b.Z() - bottom) < 1e-7)) {
      flat.Add(kRadius, TopoDS::Edge(ex.Current()));
    }
  }
  flat.Build();
  CHECK(flat.IsDone());
  return flat.IsDone() ? flat.Shape() : TopoDS_Shape();
}

// The rims of `body`: its circles of kRim about the z axis at the heights
// given, longer than a millimetre.
std::vector<TopoDS_Edge> rims(const TopoDS_Shape& body, const std::vector<double>& heights) {
  std::vector<TopoDS_Edge> out;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge       edge = TopoDS::Edge(ex.Current());
    const BRepAdaptor_Curve c(edge);
    if (c.GetType() != GeomAbs_Circle || std::abs(c.Circle().Radius() - kRim) > 1e-6
        || std::abs(c.Circle().Location().X()) > 1e-6 || std::abs(c.Circle().Location().Y()) > 1e-6
        || std::abs(c.LastParameter() - c.FirstParameter()) * kRim < 1.) {
      continue;
    }
    for (const double z : heights) {
      if (std::abs(c.Circle().Location().Z() - z) < 1e-6
          && std::none_of(out.begin(), out.end(), [&edge](const TopoDS_Edge& e) { return e.IsSame(edge); })) {
        out.push_back(edge);
      }
    }
  }
  return out;
}

TopoDS_Shape rounded(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges) {
  try {
    BRepFilletAPI_MakeFillet fillet(body);
    for (const TopoDS_Edge& edge : edges) {
      fillet.Add(kRadius, edge);
    }
    fillet.Build();
    return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    return TopoDS_Shape();
  }
}

// The faces of `shape` whose boxes lie within `region`.
std::vector<TopoDS_Face> faces_within(const TopoDS_Shape& shape, const Bnd_Box& region) {
  std::vector<TopoDS_Face> out;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
    box.Get(x0, y0, z0, x1, y1, z1);
    if (!region.IsOut(gp_Pnt(x0, y0, z0)) && !region.IsOut(gp_Pnt(x1, y1, z1))) {
      out.push_back(TopoDS::Face(f.Current()));
    }
  }
  return out;
}

// The farthest distance from `centre`, less kRadius, of points inside `face`.
double off_the_sphere(const TopoDS_Face& face, const gp_Pnt& centre) {
  double u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface s(face);
  double                    worst = 0.;
  for (int i = 0; i <= 10; ++i) {
    for (int j = 0; j <= 10; ++j) {
      const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 10.);
      const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 10.);
      if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
        continue;
      }
      worst = std::max(worst, std::abs(s.Value(u, v).Distance(centre) - kRadius));
    }
  }
  return worst;
}

// The corner of `result` about the ball's centre `centre` on the vertical
// rounding's axis: the face there that is not an earlier face (planes,
// cylinders, the rims' tori) lies on the ball's sphere within 1e-4, one face
// as the stored body's; its area.
double corner_on_the_ball(const TopoDS_Shape& result, const gp_Pnt& centre, const char* what) {
  Bnd_Box region;
  region.Update(centre.X() - 1.01, -0.01, centre.Z() - 1.01, centre.X() + 1.01, 1.06, centre.Z() + 1.01);
  double area  = 0.;
  double worst = 0.;
  int    count = 0;
  for (const TopoDS_Face& face : faces_within(result, region)) {
    const GeomAbs_SurfaceType type = BRepAdaptor_Surface(face).GetType();
    if (type == GeomAbs_Plane || type == GeomAbs_Cylinder || type == GeomAbs_Cone || type == GeomAbs_Torus) {
      continue;
    }
    worst = std::max(worst, off_the_sphere(face, centre));
    area += area_of(face);
    ++count;
  }
  std::printf("%s: %d faces on the ball within %.2g mm, area %.6f\n", what, count, worst, area);
  CHECK(count == 1);
  CHECK(worst <= 1e-4);
  return area;
}

// The ball's centre at a corner: on the vertical rounding's axis (1 inside
// the flat side, kRim - kRadius from the lid's axis) at height z.
gp_Pnt centre_at(double side, double z) {
  return gp_Pnt(side * std::sqrt((kRim - kRadius) * (kRim - kRadius) - kRadius * kRadius), kRadius, z);
}

// The lid with its top only, the top rim rounded through both mitres.
void test_top_rim_through_the_mitres() {
  const TopoDS_Shape body = lid(false);
  if (body.IsNull()) {
    return;
  }
  const std::vector<TopoDS_Edge> rim = rims(body, {kTop});
  CHECK(rim.size() == 1);
  const TopoDS_Shape result = rounded(body, rim);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The ball's piece at the top: a spherical triangle with two right angles
  // and one of pi / 2 plus the turn of the rim's meridian there.
  const double piece = kPi / 2.0 + std::asin(kRadius / (kRim - kRadius));
  for (const double side : {1.0, -1.0}) {
    const double area = corner_on_the_ball(result, centre_at(side, kTop - kRadius), "top corner");
    CHECK(std::abs(area - piece) <= 2e-3);
  }
}

// The lid's half with its cone and post (x <= 0), both rims rounded: each
// up to the cut, through the mitres at its other end.
void test_both_rims_on_the_half_lid() {
  TopoDS_Shape body = lid(true);
  if (body.IsNull()) {
    return;
  }
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(0, -30, -10), gp_Pnt(30, 30, 40)).Shape()).Shape();
  const std::vector<TopoDS_Edge> both = rims(body, {kRimLow, kTop});
  CHECK(both.size() == 2);
  const TopoDS_Shape result = rounded(body, both);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const double top = corner_on_the_ball(result, centre_at(-1.0, kTop - kRadius), "top corner");
  CHECK(std::abs(top - (kPi / 2.0 + std::asin(kRadius / (kRim - kRadius)))) <= 2e-3);
  // The cone rim's ball: kRadius from the cylinder and from the cone (45
  // degrees), its centre (sqrt 2 - 1) r above the rim.
  corner_on_the_ball(result, centre_at(-1.0, kRimLow + (std::sqrt(2.0) - 1.0) * kRadius), "cone corner");
}

std::string geometry_cases() {
#ifdef _MSC_VER
  char*       value  = nullptr;
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

// The farthest of points inside the faces of `from` from `to`.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to) {
  double worst = 0.;
  for (TopExp_Explorer f(from, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 6; ++i) {
      for (int j = 0; j <= 6; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 6.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 6.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(s.Value(u, v)).Vertex(), to);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

int face_count(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    ++count;
  }
  return count;
}

// A body of the reported design before its rims are rounded with 1, against
// the stored body after.
void reported_body(const std::string& path, const TopoDS_Shape& after) {
  const TopoDS_Shape shape = read(path);
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  const std::vector<TopoDS_Edge> both = rims(shape, {kRimLow, kTop});
  CHECK(both.size() == 2);
  const TopoDS_Shape result = rounded(shape, both);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  std::printf("%d faces, the stored body %d\n", face_count(result), face_count(after));
  CHECK(face_count(result) == face_count(after));
  const double out  = farthest(result, after);
  const double back = farthest(after, result);
  std::printf("the result on the stored body within %.6f mm, the stored body on it within %.6f mm\n", out, back);
  CHECK(out <= 0.01);
  CHECK(back <= 0.01);
  // (The volumes within the faces' distance over the rims' area, 0.05 mm3.)
  const double difference = volume_of(result) - volume_of(after);
  std::printf("the volume %.6f mm3 from the stored body's\n", difference);
  CHECK(std::abs(difference) <= 0.05);
}

// The reported design's body before its rims are rounded with 1: the
// replay's, and the stored one where the case has it.
void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string dir = cases + "/lid-rim-stationary-ball/";
  if (!std::ifstream(dir + "body.brep").good()) {
    std::printf("test_reported_body: skipped (no case lid-rim-stationary-ball in MITCAD_GEOMETRY_CASES)\n");
    return;
  }
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!after.IsNull());
  if (after.IsNull()) {
    return;
  }
  std::printf("the replay's body before the rounding:\n");
  reported_body(dir + "body.brep", after);
  if (!std::ifstream(dir + "stored-before.brep").good()) {
    std::printf("the stored body before the rounding: skipped (not in the case)\n");
    return;
  }
  std::printf("the stored body before the rounding:\n");
  reported_body(dir + "stored-before.brep", after);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_top_rim_through_the_mitres", test::test_top_rim_through_the_mitres);
  test::guarded("test_both_rims_on_the_half_lid", test::test_both_rims_on_the_half_lid);
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_stationary_ball: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
