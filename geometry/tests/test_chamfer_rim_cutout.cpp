// SPDX-License-Identifier: MIT
// A closed rim chamfered past a cut-out under it, built with OCCT's chamfer
// alone. A disc's top rim is chamfered 1 on the top and 2 down its wall; a
// pocket from below, a cylinder whose side breaks through the disc's wall,
// ends 1 below the top, so along the pocket's arc the wall is a strip only
// 1 high over the pocket's ceiling. The chamfer's contact 2 down the wall
// lies there in the pocket: the ceiling cuts the chamfer's cone lengthwise
// (it keeps the ledge's part outside the cone) and the pocket's side cuts it
// near the arc's ends, both growing up to the cut. The rim is one closed
// edge or two arcs split where the pocket's side meets the wall. OCCT left
// the pieces of the closed rim's analytic blend with no piece to come back
// to after the gap (the gap ran from the only piece's end round to its
// start) and the chamfer failed. A stored design whose bore rim is
// chamfered over a pocket the same way is the case behind the patch 1010 of
// the OCCT port (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <LocOpe_SplitShape.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kR      = 85.0; // the disc's wall
constexpr double kBottom = -5.2;
constexpr double kTop    = 7.8;
constexpr double kCeil   = 6.8;  // the pocket's ceiling, 1 below the top
constexpr double kPx     = 70.0; // the pocket's axis, at (kPx, 0)
constexpr double kPr     = 22.5; // its radius, past the wall
constexpr double kOnTop  = 1.0;  // the chamfer on the top
constexpr double kDown   = 2.0;  // and down the wall

using EdgeFaces =
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The angle where the pocket's side meets the wall (y > 0).
double corner_angle() {
  const double x = (kR * kR - kPr * kPr + kPx * kPx) / (2.0 * kPx);
  return std::atan2(std::sqrt(kR * kR - x * x), x);
}

// The disc with its pocket; the wall's seam at angle seam (the rim's vertex
// there).
TopoDS_Shape pocketed_disc(const double seam) {
  const gp_Ax2 axis(gp_Pnt(0, 0, kBottom), gp_Dir(0, 0, 1), gp_Dir(std::cos(seam), std::sin(seam), 0));
  const TopoDS_Shape disc   = BRepPrimAPI_MakeCylinder(axis, kR, kTop - kBottom).Shape();
  const TopoDS_Shape pocket = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kPx, 0, kBottom - 1.0), gp_Dir(0, 0, 1)),
                                                       kPr, kCeil - kBottom + 1.0)
                                .Shape();
  return BRepAlgoAPI_Cut(disc, pocket).Shape();
}

// The top's rim: its edges with the wall, and the top face.
void rim_of(const TopoDS_Shape& body, NCollection_List<TopoDS_Shape>& rim, TopoDS_Face& top) {
  EdgeFaces ef;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, ef);
  for (int i = 1; i <= ef.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(ef.FindKey(i));
    if (BRep_Tool::Degenerated(e)) {
      continue;
    }
    double f = 0., l = 0.;
    const auto c = BRep_Tool::Curve(e, f, l);
    const gp_Pnt a = c->Value(f), b = c->Value(0.5 * (f + l));
    const bool onRim = std::abs(a.Z() - kTop) < 1e-7 && std::abs(b.Z() - kTop) < 1e-7
                       && std::abs(std::hypot(b.X(), b.Y()) - kR) < 1e-7;
    if (!onRim) {
      continue;
    }
    rim.Append(e);
    for (const TopoDS_Shape& fc : ef(i)) {
      const TopoDS_Face& face = TopoDS::Face(fc);
      if (BRep_Tool::Surface(face)->DynamicType()->Name() == std::string("Geom_Plane")) {
        top = face;
      }
    }
  }
}

// The body with its rim split at the other corner of the pocket (the seam at
// the first): two arcs, as the stored design's rim.
TopoDS_Shape split_rim(const TopoDS_Shape& body, const double at) {
  NCollection_List<TopoDS_Shape> rim;
  TopoDS_Face                    top;
  rim_of(body, rim, top);
  if (rim.Extent() != 1) {
    return TopoDS_Shape();
  }
  const TopoDS_Edge rimEdge = TopoDS::Edge(rim.First());
  double            f = 0., l = 0.;
  const auto        c = BRep_Tool::Curve(rimEdge, f, l);
  const gp_Pnt      p(kR * std::cos(at), kR * std::sin(at), kTop);
  GeomAPI_ProjectPointOnCurve projection(p, c, f, l);
  if (projection.NbPoints() == 0) {
    return TopoDS_Shape();
  }
  const double  w = projection.LowerDistanceParameter();
  TopoDS_Vertex v;
  BRep_Builder().MakeVertex(v, c->Value(w), 1e-7);
  LocOpe_SplitShape split(body);
  split.Add(v, w, rimEdge);
  const NCollection_List<TopoDS_Shape>& out = split.DescendantShapes(body);
  return out.IsEmpty() ? TopoDS_Shape() : out.First();
}

// The material the chamfer takes: the body's part outside the chamfer's cone
// (its section's line from (kR - kOnTop, kTop) to (kR, kTop - kDown), carried
// on 1 above the top and 4 below the wall's foot of it, revolved).
double exact_loss(const TopoDS_Shape& body) {
  const double slope = kOnTop / kDown; // the line's r per unit of z down
  const double z0 = kTop + 1.0, z1 = kTop - kDown - 4.0;
  BRepBuilderAPI_MakePolygon poly(gp_Pnt(kR - kOnTop - slope * (z0 - kTop), 0, z0), gp_Pnt(kR + 10.0, 0, z0),
                                  gp_Pnt(kR + 10.0, 0, z1), gp_Pnt(kR - kOnTop + slope * (kTop - z1), 0, z1),
                                  true);
  const TopoDS_Face  section = BRepBuilderAPI_MakeFace(poly.Wire(), true).Face();
  const TopoDS_Shape ring    = BRepPrimAPI_MakeRevol(section, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape();
  return volume_of(BRepAlgoAPI_Common(body, ring).Shape());
}

double length_of(const TopoDS_Shape& edge) {
  GProp_GProps props;
  BRepGProp::LinearProperties(edge, props);
  return props.Mass();
}

// longFirst: the rim's longer arc given first (the spine starts with it).
void chamfer_case(const char* name, const TopoDS_Shape& body, const bool longFirst) {
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  NCollection_List<TopoDS_Shape> rim;
  TopoDS_Face                    top;
  rim_of(body, rim, top);
  CHECK(!rim.IsEmpty() && !top.IsNull());
  if (rim.Extent() == 2 && (length_of(rim.First()) > length_of(rim.Last())) != longFirst) {
    rim.Reverse();
  }
  BRepFilletAPI_MakeChamfer chamfer(body);
  for (const TopoDS_Shape& e : rim) {
    chamfer.Add(kOnTop, kDown, TopoDS::Edge(e), top);
  }
  try {
    OCC_CATCH_SIGNALS
    chamfer.Build();
  } catch (const Standard_Failure& error) {
    std::printf("%s: %s\n", name, error.what());
  }
  CHECK(chamfer.IsDone());
  if (!chamfer.IsDone()) {
    std::printf("%s: not done\n", name);
    return;
  }
  const TopoDS_Shape result = chamfer.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const double loss  = volume_of(body) - volume_of(result);
  const double exact = exact_loss(body);
  std::printf("%s: %d rim edges, material taken %.7f, the section's %.7f\n", name, rim.Extent(), loss, exact);
  CHECK_NEAR_TOL(loss, exact, 1e-6);
  // The ceiling keeps its part outside the cone: up to 84.5 from the axis.
  double ceilingReach = 0.;
  for (TopExp_Explorer ex(result, TopAbs_VERTEX); ex.More(); ex.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
    if (std::abs(p.Z() - kCeil) < 1e-6) {
      ceilingReach = std::max(ceilingReach, std::hypot(p.X(), p.Y()));
    }
  }
  std::printf("%s: the ceiling reaches %.7f\n", name, ceilingReach);
  CHECK_NEAR_TOL(ceilingReach, kR - kOnTop + kOnTop * (kTop - kCeil) / kDown, 1e-7);
}

void test_rim_chamfered_over_a_pocket() {
  const double a = corner_angle();
  // Two arcs, split at both corners, the longer one first or second.
  chamfer_case("two arcs, the longer first", split_rim(pocketed_disc(-a), a), true);
  chamfer_case("two arcs, the shorter first", split_rim(pocketed_disc(-a), a), false);
  chamfer_case("two arcs from the other corner, the longer first", split_rim(pocketed_disc(a), -a), true);
  // The rim one closed edge, its vertex away from the pocket.
  chamfer_case("one edge", pocketed_disc(2.0), true);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(chamfer_rim_cutout) {
  test::guarded("test_rim_chamfered_over_a_pocket", test::test_rim_chamfered_over_a_pocket);
  if (test::failures == 0) {
    std::puts("test_chamfer_rim_cutout: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
