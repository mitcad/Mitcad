// SPDX-License-Identifier: MIT
// A convex edge rounded with R up into the mitre where two earlier convex
// roundings of a radius rk below R meet (mitcad#1414), with OCCT's fillet
// alone.
//
// A block [10, 50]^2 x [3, 43] stands on a plate [0, 60]^2 x [0, 3]; the
// block's foot edges are rounded with 1.5, its top edges with rk = 2.5.
// At each corner the two top roundings meet in a mitre from the top of the
// block's vertical edge (to which it is tangent) up to the top face T.
// The vertical edge, one tangent chain with the mitres at its foot and at
// its top, is then rounded with R > rk. No ball of R rolls over the top
// mitre: it would touch both cylinders of rk from inside. OCCT's walk
// failed there.
//
// The construction (the port's patch 1500 on, third_party/vcpkg-ports): the
// rounding ends at the section through the vertical edge's top V; the ball
// of rk rolls between the rounding's end and T, a torus about the
// rounding's axis (its tube circle of radius rho = R - rk, rk below T)
// from the section of one top rounding to the other's. The top roundings
// are cut along those sections, T along the torus's contact (a circle of
// radius rho), and the mitre goes. Each corner, each way round, at radii 3
// (the reported design's), 4 and 6: valid, the torus's area
// rk phi (rho pi / 2 + rk), and the material taken at the top corner (the
// difference from the same rounding on the block with its foot rounded
// only, whose vertical edge runs up to T) the corner's
// (1 - pi/4) (rho^2 rk + rho pi rk^2 / 2 + 2 rk^3 / 3) less the straight
// rounding's rk R^2 (1 - pi/4) above V. The block with only its top
// rounded, rounded with 3: the straight piece and the corner. MitCad's
// fillet of the same: every face and edge named, the torus after the top
// mitre's edge.
//
// With MITCAD_GEOMETRY_CASES set, the case `convex-mitre-tighter-roundings`:
// `body.brep`, the reported design's stored body before the rounding,
// rounded with 3 along the chain through its vertical edge, and
// `after.brep`, the stored body after it: about the chain the result lies
// on the stored body and the stored body on the result within 0.01 mm, and
// the material taken is the stored body's.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"

namespace test {
namespace {

constexpr double kFoot = 1.5;
constexpr double kTop  = 2.5;

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

// The edge of `shape` nearest to `at`, within `within`; null if none.
TopoDS_Edge edge_at(const TopoDS_Shape& shape, const gp_Pnt& at, double within = 1e-6) {
  const TopoDS_Vertex point = BRepBuilderAPI_MakeVertex(at).Vertex();
  TopoDS_Edge         best;
  double              nearest = within;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    BRepExtrema_DistShapeShape d(e.Current(), point);
    if (d.IsDone() && d.Value() <= nearest) {
      nearest = d.Value();
      best    = TopoDS::Edge(e.Current());
    }
  }
  return best;
}

// The edges of `shape` with both vertices at the height z, within the box
// [lo, hi]^2 in x and y.
template <class F>
void each_edge_at_height(const TopoDS_Shape& shape, double z, double lo, double hi, F&& f) {
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    TopoDS_Vertex      v1, v2;
    TopExp::Vertices(edge, v1, v2);
    bool in = !v1.IsNull() && !v2.IsNull();
    for (const TopoDS_Vertex& v : {v1, v2}) {
      if (!in) {
        break;
      }
      const gp_Pnt p = BRep_Tool::Pnt(v);
      in = std::abs(p.Z() - z) < 1e-7 && p.X() > lo && p.X() < hi && p.Y() > lo && p.Y() < hi;
    }
    if (in) {
      f(edge);
    }
  }
}

// The block on the plate, its foot edges rounded with kFoot and, with
// `top`, its top edges with kTop.
TopoDS_Shape block(bool top) {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0., 0., 0.), gp_Pnt(60., 60., 3.)).Shape();
  const TopoDS_Shape box   = BRepPrimAPI_MakeBox(gp_Pnt(10., 10., 3.), gp_Pnt(50., 50., 43.)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, box).Shape());
  unify.Build();
  TopoDS_Shape body = unify.Shape();
  {
    BRepFilletAPI_MakeFillet fillet(body);
    each_edge_at_height(body, 3., 5., 55., [&](const TopoDS_Edge& e) { fillet.Add(kFoot, e); });
    fillet.Build();
    if (!fillet.IsDone()) {
      return TopoDS_Shape();
    }
    body = fillet.Shape();
  }
  if (top) {
    BRepFilletAPI_MakeFillet fillet(body);
    each_edge_at_height(body, 43., 5., 55., [&](const TopoDS_Edge& e) { fillet.Add(kTop, e); });
    fillet.Build();
    if (!fillet.IsDone()) {
      return TopoDS_Shape();
    }
    body = fillet.Shape();
  }
  return body;
}

// `edge` (a chain's edge) rounded with `radius`; null where it fails.
TopoDS_Shape round_edge(const TopoDS_Shape& shape, const TopoDS_Edge& edge, double radius) {
  try {
    BRepFilletAPI_MakeFillet fillet(shape);
    fillet.Add(radius, edge);
    fillet.Build();
    return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    return TopoDS_Shape();
  }
}

// The torus faces of `shape` and their total area.
int tori(const TopoDS_Shape& shape, double& area) {
  int n = 0;
  area  = 0.;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Torus) {
      ++n;
      area += area_of(f.Current());
    }
  }
  return n;
}

void test_vertical_edge_into_the_mitre() {
  const TopoDS_Shape mitred = block(true);
  const TopoDS_Shape foot   = block(false);
  CHECK(!mitred.IsNull() && !foot.IsNull());
  if (mitred.IsNull() || foot.IsNull()) {
    return;
  }
  const double c  = 1. - kPi / 4.;
  const double rk = kTop;
  for (const double radius : {3., 4., 6.}) {
    const double rho = radius - rk;
    // The corner's material: in each horizontal section at h above V the
    // square of side rho + s less the quarter disc of that radius
    // (s = sqrt(rk^2 - h^2)), against the straight rounding's square of
    // side R less its quarter disc.
    const double corner   = c * (rho * rho * rk + rho * kPi * rk * rk / 2. + 2. * rk * rk * rk / 3.);
    const double expected = corner - c * radius * radius * rk;
    const double torus    = rk * (kPi / 2.) * (rho * kPi / 2. + rk);
    for (const double x : {10., 50.}) {
      for (const double y : {10., 50.}) {
        const TopoDS_Edge edge  = edge_at(mitred, gp_Pnt(x, y, 23.));
        const TopoDS_Edge plain = edge_at(foot, gp_Pnt(x, y, 23.));
        CHECK(!edge.IsNull() && !plain.IsNull());
        if (edge.IsNull() || plain.IsNull()) {
          continue;
        }
        const TopoDS_Shape reference = round_edge(foot, plain, radius);
        CHECK(!reference.IsNull());
        if (reference.IsNull()) {
          continue;
        }
        const double taken_plain = volume_of(foot) - volume_of(reference);
        for (const TopoDS_Edge& first : {edge, TopoDS::Edge(edge.Reversed())}) {
          const TopoDS_Shape result = round_edge(mitred, first, radius);
          CHECK(!result.IsNull());
          if (result.IsNull()) {
            std::fprintf(stderr, "radius %g, corner %g %g: the rounding failed\n", radius, x, y);
            continue;
          }
          CHECK(BRepCheck_Analyzer(result).IsValid());
          const double taken = volume_of(mitred) - volume_of(result);
          CHECK_NEAR_TOL(taken - taken_plain, expected, 1e-4);
          double    area = 0.;
          const int n    = tori(result, area);
          CHECK(n == 1);
          CHECK_NEAR_TOL(area, torus, 1e-6);
        }
      }
    }
  }
}

// The block with only its top edges rounded (its foot sharp): the vertical
// edge and the top mitre, rounded with 3.
void test_top_mitre_alone() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(0., 0., 0.), gp_Pnt(60., 60., 3.)).Shape();
  const TopoDS_Shape box   = BRepPrimAPI_MakeBox(gp_Pnt(10., 10., 3.), gp_Pnt(50., 50., 43.)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, box).Shape());
  unify.Build();
  TopoDS_Shape             body = unify.Shape();
  BRepFilletAPI_MakeFillet fillet(body);
  each_edge_at_height(body, 43., 5., 55., [&](const TopoDS_Edge& e) { fillet.Add(kTop, e); });
  fillet.Build();
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  body                   = fillet.Shape();
  const TopoDS_Edge edge = edge_at(body, gp_Pnt(10., 10., 23.));
  CHECK(!edge.IsNull());
  if (edge.IsNull()) {
    return;
  }
  const TopoDS_Shape result = round_edge(body, edge, 3.);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The straight rounding from the foot up to V, and the corner.
  const double c     = 1. - kPi / 4.;
  const double rk    = kTop;
  const double rho   = 3. - rk;
  const double taken = 9. * c * (43. - 3. - rk) + c * (rho * rho * rk + rho * kPi * rk * rk / 2. + 2. * rk * rk * rk / 3.);
  CHECK_NEAR_TOL(volume_of(body) - volume_of(result), taken, 1e-4);
}

// MitCad's fillet of the vertical edge at (10, 10) with 3: every face and
// edge named, the torus after the top mitre's edge.
void test_corner_face_named() {
  const TopoDS_Shape shape = block(true);
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  const mitcad::geometry::ShapePtr named = mitcad::geometry::import_body(shape, "F1", 0);
  const TopoDS_Edge                edge  = edge_at(named->occt(), gp_Pnt(10., 10., 23.));
  // The top mitre's middle: on both cylinders of kTop, 45 degrees up.
  const double      c     = kTop * std::sqrt(0.5);
  const TopoDS_Edge mitre = edge_at(named->occt(), gp_Pnt(12.5 - c, 12.5 - c, 40.5 + c), 1e-4);
  CHECK(!edge.IsNull() && !mitre.IsNull());
  std::string edge_name, mitre_name;
  for (int i = 0; i < named->edge_count(); ++i) {
    if (named->edge(i).IsSame(edge)) {
      edge_name = named->edge_name(i);
    }
    if (named->edge(i).IsSame(mitre)) {
      mitre_name = named->edge_name(i);
    }
  }
  CHECK(!edge_name.empty() && !mitre_name.empty());
  if (edge_name.empty() || mitre_name.empty()) {
    return;
  }
  const mitcad::geometry::ShapePtr rounded = mitcad::geometry::fillet("F2", *named, std::vector<std::string>{edge_name}, 3.);
  int unnamed = 0, unnamed_edges = 0, after_mitre = 0;
  for (int i = 0; i < rounded->face_count(); ++i) {
    unnamed += rounded->face_names(i).empty() ? 1 : 0;
    for (const std::string& n : rounded->face_names(i)) {
      after_mitre += n.rfind("F2:fillet(" + mitre_name + ")", 0) == 0 ? 1 : 0;
    }
  }
  for (int i = 0; i < rounded->edge_count(); ++i) {
    unnamed_edges += rounded->edge_name(i).empty() && !BRep_Tool::Degenerated(rounded->edge(i)) ? 1 : 0;
  }
  CHECK(unnamed == 0);
  CHECK(unnamed_edges == 0);
  CHECK(after_mitre == 1);
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

// The faces of a shape whose box meets the region.
TopoDS_Compound faces_in(const TopoDS_Shape& shape, const Bnd_Box& region) {
  TopoDS_Compound out;
  BRep_Builder    builder;
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
  double                worst  = 0.;
  for (TopExp_Explorer f(faces_in(from, region), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
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

// The reported design's stored body: its vertical edge through
// (0, 117.27, 23) between two planes at 90 degrees, a chain with the
// mitres of earlier roundings of 1.5 at its foot and 2.5 at its top.
void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string dir = cases + "/convex-mitre-tighter-roundings/";
  if (!std::ifstream(dir + "body.brep").good()) {
    std::printf("test_reported_body: skipped (no case convex-mitre-tighter-roundings in MITCAD_GEOMETRY_CASES)\n");
    return;
  }
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  // (The stored edge's x is 0 within 0.002.)
  const TopoDS_Edge edge = edge_at(shape, gp_Pnt(0., 117.27, 23.), 0.01);
  CHECK(!edge.IsNull());
  if (edge.IsNull()) {
    return;
  }
  const TopoDS_Shape result = round_edge(shape, edge, 3.);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  Bnd_Box region;
  region.Update(-4., 114., 2.5, 4., 122., 43.5);
  const double out  = farthest(result, after, region);
  const double back = farthest(after, result, region);
  std::printf("about the chain: the result on the stored body within %.6f mm, the stored body on it within %.6f mm\n",
              out,
              back);
  CHECK(out <= 0.01);
  CHECK(back <= 0.01);
  const double taken = volume_of(shape) - volume_of(result);
  const double file  = volume_of(shape) - volume_of(after);
  std::printf("the material taken %.6f mm3, the stored body's %.6f\n", taken, file);
  CHECK(std::abs(taken - file) <= 0.01);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_vertical_edge_into_the_mitre", test::test_vertical_edge_into_the_mitre);
  test::guarded("test_top_mitre_alone", test::test_top_mitre_alone);
  test::guarded("test_corner_face_named", test::test_corner_face_named);
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_convex_mitre: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
