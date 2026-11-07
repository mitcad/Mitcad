// SPDX-License-Identifier: MIT
// A wall's edges rounded through corners where the ball stays put in the
// tube of an earlier rounding of its own radius, with OCCT's fillet alone
// (mitcad#1427, the port's patches 1600 and 1601).
//
// test_d_block (1600): a D-shaped block, a vertical cylinder of radius Rc
// cut by the wall W (y = 0), its top rim rounded with R first (a torus).
// Then W's edges with the top, the torus and the cylinder are rounded with
// R as one chain. Over the torus's edge the ball first rolls on the top and
// W, then, once its centre reaches the torus's centre circle, stays put
// while its contact turns round the torus's section down to the cylinder:
// each top corner is a piece of the ball's sphere. Walked from the top's
// piece towards the cylinder's, the walk stopped where the contact on W
// reached the cylinder's piece, without that part of the ball: the
// rounding threw, or made an invalid body.
//
// test_reported_body (1600 and 1601): the case `fillet-opening-still-corners`
// of MITCAD_GEOMETRY_CASES, a reported design's body before the rounding
// (`body.brep`) and the stored body after it (`after.brep`). The wall of an
// opening through a plate meets the top and bottom, a cylinder of 5 whose
// rims an earlier rounding of 0.2 rounds (tori), and a side at a crease of
// 3.5 degrees whose rims are rounded with 0.2 too (cylinders going on into
// tori, cut at the wall by another feature's faces). The wall's whole edge
// loop is rounded with 0.2. Its boundary turns where those faces meet the
// tori, which splits the loop into two contours; at those vertices one
// contour's ball stays put in the earlier rounding's tube, and the other's
// runs straight on to that ball, as in the stored body.
//
// Checks: valid and closed; at each corner a face on the ball's sphere and
// the faces about the corner on the surfaces the construction expects; the
// same body for both orders of the edges; on the reported body the result
// on the stored body and the stored body on it within 0.01 mm about the
// opening.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_Map.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

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

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

bool closed(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> uses;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      const TopAbs_Orientation o = e.Current().Orientation();
      if (BRep_Tool::Degenerated(TopoDS::Edge(e.Current())) || o == TopAbs_INTERNAL || o == TopAbs_EXTERNAL) {
        continue;
      }
      const int i = uses.FindIndex(e.Current());
      if (i == 0) {
        uses.Add(e.Current(), 1);
      } else {
        ++uses(i);
      }
    }
  }
  for (int i = 1; i <= uses.Extent(); ++i) {
    if (uses(i) != 2) {
      return false;
    }
  }
  return true;
}

// The edges of the shape (each once) whose middle passes the test.
template <class Test> std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, Test test) {
  std::vector<TopoDS_Edge>                               out;
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    if (!seen.Add(e.Current())) {
      continue;
    }
    const BRepAdaptor_Curve c(TopoDS::Edge(e.Current()));
    if (test(c.Value(0.5 * (c.FirstParameter() + c.LastParameter())))) {
      out.push_back(TopoDS::Edge(e.Current()));
    }
  }
  return out;
}

TopoDS_Shape round_edges(const TopoDS_Shape& shape, const std::vector<TopoDS_Edge>& edges, double radius) {
  BRepFilletAPI_MakeFillet                               fillet(shape);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Edge& e : edges) {
    selected.Add(e);
  }
  fillet.StopChainsAtTurns(selected);
  for (const TopoDS_Edge& e : edges) {
    if (fillet.Contour(e) == 0) {
      fillet.Add(radius, e);
    }
  }
  fillet.CarryChainsAcrossNotches();
  try {
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("the rounding threw: %s\n", failure.what());
    return TopoDS_Shape();
  }
  if (!fillet.IsDone()) {
    std::printf("the rounding failed: %d faulty contours, %d faulty vertices\n",
                fillet.NbFaultyContours(),
                fillet.NbFaultyVertices());
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// Points inside the faces of a shape whose box meets the region, n by n
// on each face's parameters.
std::vector<gp_Pnt> points_in(const TopoDS_Shape& shape, const Bnd_Box& region, int n) {
  std::vector<gp_Pnt> out;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    Bnd_Box           box;
    BRepBndLib::Add(face, box);
    if (box.IsOut(region)) {
      continue;
    }
    double u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= n; ++i) {
      for (int j = 0; j <= n; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / n);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / n);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = s.Value(u, v);
        if (!region.IsOut(p)) {
          out.push_back(p);
        }
      }
    }
  }
  return out;
}

// The number of faces of a shape lying on the sphere (centre, radius)
// within tol at every sample.
int faces_on_ball(const TopoDS_Shape& shape, const gp_Pnt& centre, double radius, double tol) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    double                    worst = 0.;
    for (int i = 0; i <= 6; ++i) {
      for (int j = 0; j <= 6; ++j) {
        const gp_Pnt p = s.Value(u0 + (u1 - u0) * i / 6., v0 + (v1 - v0) * j / 6.);
        worst          = std::max(worst, std::abs(p.Distance(centre) - radius));
      }
    }
    if (worst <= tol) {
      ++n;
    }
  }
  return n;
}

// The block: a vertical cylinder of radius rc about (0, -yc), z from -10 to
// 0, cut by the wall y = 0; its top rim rounded with r.
TopoDS_Shape d_block(double r, double rc, double yc) {
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0., -yc, -10.), gp::DZ()), rc, 10.).Shape();
  const TopoDS_Shape half     = BRepPrimAPI_MakeBox(gp_Pnt(-20., -20., -20.), gp_Pnt(20., 0., 20.)).Shape();
  const TopoDS_Shape block    = BRepAlgoAPI_Common(cylinder, half).Shape();
  const auto rim = [](const gp_Pnt& m) { return std::abs(m.Z()) < 1e-6 && m.Y() < -1e-6; };
  return round_edges(block, edges_where(block, rim), r);
}

void test_d_block() {
  struct Size {
    double r, rc, yc;
  };
  for (const Size size : {Size{1., 5., 3.}, Size{0.2, 5., 4.5}, Size{2., 8., 4.}}) {
    const TopoDS_Shape block = d_block(size.r, size.rc, size.yc);
    CHECK(!block.IsNull());
    if (block.IsNull()) {
      return;
    }
    // W's edges but its bottom one: with the top, the torus's two and the
    // cylinder's two.
    std::vector<TopoDS_Edge> edges =
      edges_where(block, [](const gp_Pnt& m) { return std::abs(m.Y()) < 1e-6 && m.Z() > -10. + 1e-6; });
    CHECK(edges.size() == 5);
    double first_change = 0.;
    for (const bool reversed : {false, true}) {
      if (reversed) {
        std::reverse(edges.begin(), edges.end());
      }
      std::printf("radius %g, cylinder %g about y = -%g, %s order\n",
                  size.r,
                  size.rc,
                  size.yc,
                  reversed ? "reversed" : "given");
      const TopoDS_Shape result = round_edges(block, edges, size.r);
      CHECK(!result.IsNull());
      if (result.IsNull()) {
        continue;
      }
      CHECK(BRepCheck_Analyzer(result).IsValid());
      CHECK(closed(result));
      const double change = volume_of(result) - volume_of(block);
      if (reversed) {
        CHECK_NEAR_TOL(change, first_change, 1e-6);
      } else {
        first_change = change;
      }
      // Each top corner: the ball R from W, the top and the cylinder (its
      // centre on the torus's centre circle).
      const double r = size.r, rc = size.rc, yc = size.yc;
      const double x = std::sqrt((rc - r) * (rc - r) - (yc - r) * (yc - r));
      for (const double side : {-1., 1.}) {
        const gp_Pnt c(side * x, -r, -r);
        const int    on_ball = faces_on_ball(result, c, r, 1e-4);
        std::printf("  corner at x = %.4f: %d face(s) on the ball\n", c.X(), on_ball);
        CHECK(on_ball >= 1);
        // About the corner every face lies on one of the surfaces of the
        // construction: W, the top, the cylinder, the torus, the rounding
        // along the top (axis y = z = -r), the rounding along the cylinder
        // (vertical, through c) or the ball.
        Bnd_Box region;
        region.Update(c.X() - 1.5 * r, -2.5 * r, -2.5 * r, c.X() + 1.5 * r, 0.5 * r, 0.5 * r);
        double worst = 0.;
        for (const gp_Pnt& p : points_in(result, region, 8)) {
          const double rho = std::hypot(p.X(), p.Y() + yc);
          const double d[] = {
            std::abs(p.Y()),
            std::abs(p.Z()),
            std::abs(rho - rc),
            std::abs(std::hypot(rho - (rc - r), p.Z() + r) - r),
            std::abs(std::hypot(p.Y() + r, p.Z() + r) - r),
            std::abs(std::hypot(p.X() - c.X(), p.Y() - c.Y()) - r),
            std::abs(p.Distance(c) - r),
          };
          worst = std::max(worst, *std::min_element(std::begin(d), std::end(d)));
        }
        std::printf("  about it every face on the construction's surfaces within %.2e\n", worst);
        CHECK(worst <= 1e-4);
      }
    }
  }
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
  for (const gp_Pnt& p : points_in(from, region, 10)) {
    BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), target);
    if (d.IsDone()) {
      worst = std::max(worst, d.Value());
    }
  }
  return worst;
}

void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/fillet-opening-still-corners/";
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  // The opening's wall: the plane through (18.6386, 19.5645) whose normal is
  // (-0.827830, -0.560979, 0); all its edges.
  const gp_Pln wall(gp_Pnt(18.6386, 19.5645, -5.), gp_Dir(-0.827830, -0.560979, 0.));
  std::vector<TopoDS_Edge> edges;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More() && edges.empty(); f.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(f.Current()));
    if (s.GetType() != GeomAbs_Plane || !s.Plane().Axis().Direction().IsParallel(wall.Axis().Direction(), 1e-4)
        || wall.Distance(s.Plane().Location()) > 1e-3) {
      continue;
    }
    edges = edges_where(f.Current(), [](const gp_Pnt&) { return true; });
  }
  std::printf("the wall's edges: %zu\n", edges.size());
  CHECK(edges.size() == 12);
  if (edges.size() != 12) {
    return;
  }
  for (const bool reversed : {false, true}) {
    if (reversed) {
      std::reverse(edges.begin(), edges.end());
    }
    const TopoDS_Shape result = round_edges(shape, edges, 0.2);
    CHECK(!result.IsNull());
    if (result.IsNull()) {
      return;
    }
    CHECK(BRepCheck_Analyzer(result).IsValid());
    CHECK(closed(result));
    // About the opening.
    Bnd_Box opening;
    opening.Update(18.3, 16.5, -12.6, 20.7, 19.8, -2.3);
    const double out  = farthest(result, after, opening);
    const double back = farthest(after, result, opening);
    std::printf("%s order: about the opening the result on the stored body within %.6f mm, the stored body on it "
                "within %.6f mm\n",
                reversed ? "reversed" : "given",
                out,
                back);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
    // At the four corners the balls of 0.2: on the side's roundings' axes
    // (top and bottom) and on the tori's centre circles about the cylinder.
    for (const gp_Pnt& c : {gp_Pnt(20.387915, 16.626495, -2.65),
                            gp_Pnt(20.387915, 16.626495, -12.25),
                            gp_Pnt(18.473016, 19.452294, -2.65),
                            gp_Pnt(18.473016, 19.452294, -12.25)}) {
      const int on_ball = faces_on_ball(result, c, 0.2, 1e-4);
      std::printf("  corner (%.4f %.4f %.4f): %d face(s) on the ball\n", c.X(), c.Y(), c.Z(), on_ball);
      CHECK(on_ball >= 1);
    }
    // The change of volume about the opening (elsewhere the body before, the
    // replay's, has faces of other features up to 2.7e-4 mm off the stored
    // body's).
    const TopoDS_Shape box  = BRepPrimAPI_MakeBox(gp_Pnt(18.3, 16.55, -12.6), gp_Pnt(20.7, 19.8, -2.3)).Shape();
    const auto         part = [&box](const TopoDS_Shape& s) { return volume_of(BRepAlgoAPI_Common(s, box).Shape()); };
    const double       before = part(shape);
    const double       change = part(result) - before;
    const double       file   = part(after) - before;
    std::printf("  the change of volume about the opening %.6f mm3, the stored body's %.6f\n", change, file);
    CHECK(std::abs(change - file) <= 1e-3 * std::abs(file));
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_d_block", test::test_d_block);
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_still_ball_corners: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
